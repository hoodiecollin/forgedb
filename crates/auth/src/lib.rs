//! # forgedb-auth
//!
//! Schema-agnostic, **verify-only** JWT authentication for ForgeDB generated
//! servers, plus a single tenant cross-check.
//!
//! This is Class-1 substrate — the same class as `forgedb-changefeed`. It knows
//! *less* about a schema than `forgedb-storage`
//! does: it decodes no field, dispatches on no model name, reconstructs no
//! schema surface. Its entire vocabulary is "verify a token", "extract a
//! configured claim", "compare two opaque strings", "carry a principal". It
//! never reads a `.forge` schema and holds no notion of models, rows, columns,
//! or policies — deliberately. The instant this crate grows a per-model map or
//! a "role X may read model Y" decision, it has crossed into the runtime engine
//! the ForgeDB identity forbids; keep that seam bright.
//!
//! ## What it does
//!
//! Given a raw bearer token and a configured [`AuthConfig`] + a single
//! `process_tenant` string, [`Authenticator::authenticate`]:
//!
//! 1. rejects any signature algorithm outside the configured allowlist
//!    (defeats `alg: none` and the HS/RS confusion downgrade),
//! 2. verifies the signature against an asymmetric key selected by the token's
//!    `kid` (static PEM or a JWKS document),
//! 3. validates `exp`/`nbf` (with skew), `iss`, `aud`, and required claims,
//! 4. extracts the configured tenant claim and **cross-checks it against the
//!    process's tenant** — a plain string equality; a mismatch is a 403,
//! 5. returns an opaque [`Principal`] (subject, tenant, roles, raw claims).
//!
//! It never *issues* tokens or stores users — bring your own IdP.

use std::collections::HashMap;

use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde_json::Value;

// Re-export so consumers (generated servers, the CLI) name the algorithm type
// without depending on `jsonwebtoken` directly.
pub use jsonwebtoken::Algorithm;

/// Parse an algorithm name (e.g. `"RS256"`, `"ES256"`). Asymmetric families
/// only — `HS*` deliberately returns `None` (verify-only auth is asymmetric).
pub fn parse_algorithm(name: &str) -> Option<Algorithm> {
    Some(match name.trim().to_ascii_uppercase().as_str() {
        "RS256" => Algorithm::RS256,
        "RS384" => Algorithm::RS384,
        "RS512" => Algorithm::RS512,
        "PS256" => Algorithm::PS256,
        "PS384" => Algorithm::PS384,
        "PS512" => Algorithm::PS512,
        "ES256" => Algorithm::ES256,
        "ES384" => Algorithm::ES384,
        "EDDSA" => Algorithm::EdDSA,
        _ => return None,
    })
}

/// A verification failure. [`AuthError::status_code`] maps each to the HTTP
/// status the middleware returns: `401` for an authentication failure, `403`
/// for a valid token whose tenant is not authorized for this process.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// No `Authorization: Bearer <token>` header.
    #[error("missing or malformed Authorization header")]
    MissingToken,
    /// The token's algorithm is not in the configured allowlist.
    #[error("token algorithm {0:?} is not in the allowlist")]
    AlgorithmNotAllowed(Algorithm),
    /// Signature/claims verification failed.
    #[error("token verification failed: {0}")]
    Invalid(String),
    /// No key matched the token's `kid`.
    #[error("no verification key for kid {0:?}")]
    UnknownKey(Option<String>),
    /// The configured tenant claim is absent from the token.
    #[error("tenant claim '{claim}' missing from token")]
    MissingTenantClaim { claim: String },
    /// A configured required claim is absent.
    #[error("required claim '{0}' missing from token")]
    MissingRequiredClaim(String),
    /// A valid token, but its tenant is not the one this process serves.
    #[error(
        "tenant mismatch: token tenant '{token}' is not authorized for this process (serves '{process}')"
    )]
    TenantMismatch { token: String, process: String },
    /// The JWKS document or a static key could not be parsed.
    #[error("invalid verification key material: {0}")]
    Key(String),
    /// Fetching the JWKS document over HTTP failed (#81).
    #[cfg(feature = "jwks-http")]
    #[error("failed to fetch JWKS from {url}: {reason}")]
    Fetch { url: String, reason: String },
}

impl AuthError {
    /// HTTP status for this failure: `403` only for a tenant mismatch (a valid,
    /// authenticated caller reaching the wrong process), `401` for every
    /// authentication failure.
    pub fn status_code(&self) -> u16 {
        match self {
            AuthError::TenantMismatch { .. } => 403,
            _ => 401,
        }
    }
}

/// A single asymmetric public key with an optional `kid`.
#[derive(Debug, Clone)]
pub struct StaticKey {
    /// Key id to match against the token header's `kid`. `None` = wildcard /
    /// sole key.
    pub kid: Option<String>,
    /// PEM-encoded public key (SPKI or PKCS#1 for RSA, SEC1/PKCS#8 for EC/Ed).
    pub pem: String,
    /// The algorithm this key verifies.
    pub algorithm: Algorithm,
}

/// Where verification keys come from. Both variants resolve to a public key
/// selected by the token's `kid` (or the sole key when the token carries none).
/// This type holds cryptographic material only — no schema, no models.
pub enum KeySource {
    /// One or more static public keys (PEM).
    StaticPem(Vec<StaticKey>),
    /// A parsed JWKS document, e.g. an IdP's `.well-known/jwks.json`.
    Jwks(jsonwebtoken::jwk::JwkSet),
    /// A JWKS document fetched over HTTP and refreshed on a schedule (#81). The
    /// cache holds the current key set behind a lock and a background thread
    /// re-fetches it, so a key rotated in at the IdP is picked up at the next
    /// refresh. Feature-gated to keep the default dep surface lean.
    #[cfg(feature = "jwks-http")]
    JwksHttp(std::sync::Arc<JwksHttpCache>),
}

/// A JWKS document fetched over HTTP, cached behind a lock, and refreshed by a
/// background thread (#81). Schema-agnostic — cryptographic material only, the
/// same class as [`KeySource`]. Cloneable handle (`Arc`) so the refresh thread
/// and the [`Authenticator`] share one cache.
#[cfg(feature = "jwks-http")]
pub struct JwksHttpCache {
    url: String,
    keys: std::sync::RwLock<jsonwebtoken::jwk::JwkSet>,
}

#[cfg(feature = "jwks-http")]
impl JwksHttpCache {
    /// Fetch + parse the JWKS document at `url` (blocking).
    fn fetch(url: &str) -> Result<jsonwebtoken::jwk::JwkSet, AuthError> {
        let body = ureq::get(url)
            .call()
            .map_err(|e| AuthError::Fetch {
                url: url.to_string(),
                reason: e.to_string(),
            })?
            .into_string()
            .map_err(|e| AuthError::Fetch {
                url: url.to_string(),
                reason: e.to_string(),
            })?;
        serde_json::from_str(&body).map_err(|e| AuthError::Key(e.to_string()))
    }

    /// Re-fetch the JWKS and swap in the new key set. On error the previous set
    /// is retained (an IdP blip must not lock everyone out mid-rotation).
    fn refresh(&self) -> Result<(), AuthError> {
        let set = Self::fetch(&self.url)?;
        *self.keys.write().unwrap() = set;
        Ok(())
    }
}

impl KeySource {
    /// A single static PEM public key.
    pub fn static_pem(kid: Option<String>, pem: impl Into<String>, algorithm: Algorithm) -> Self {
        KeySource::StaticPem(vec![StaticKey {
            kid,
            pem: pem.into(),
            algorithm,
        }])
    }

    /// Parse a JWKS document (e.g. the body of `.well-known/jwks.json`). Offline
    /// and pure (no HTTP) so it is fully testable; for the fetch-and-refresh
    /// variant see [`KeySource::jwks_url`] (#81).
    pub fn from_jwks_json(json: &str) -> Result<Self, AuthError> {
        let set: jsonwebtoken::jwk::JwkSet =
            serde_json::from_str(json).map_err(|e| AuthError::Key(e.to_string()))?;
        Ok(KeySource::Jwks(set))
    }

    /// Fetch a JWKS document over HTTP and keep it fresh (#81). Fetches once
    /// **synchronously** (so a bad URL / unreachable IdP fails loud at startup,
    /// never a silently-unauthenticated server), then spawns a background thread
    /// that re-fetches every `refresh_interval` — picking up a rotated-in signing
    /// key within one interval. On a refresh error the previous key set is kept.
    #[cfg(feature = "jwks-http")]
    pub fn jwks_url(
        url: impl Into<String>,
        refresh_interval: std::time::Duration,
    ) -> Result<Self, AuthError> {
        let url = url.into();
        // Initial synchronous fetch — fail loud if the IdP is unreachable.
        let set = JwksHttpCache::fetch(&url)?;
        let cache = std::sync::Arc::new(JwksHttpCache {
            url,
            keys: std::sync::RwLock::new(set),
        });
        // Background refresh thread (detached; lives for the process). A zero
        // interval disables refresh (single startup fetch only).
        if !refresh_interval.is_zero() {
            let bg = std::sync::Arc::clone(&cache);
            std::thread::Builder::new()
                .name("forgedb-jwks-refresh".into())
                .spawn(move || loop {
                    std::thread::sleep(refresh_interval);
                    if let Err(e) = bg.refresh() {
                        eprintln!("[forgedb-auth] JWKS refresh failed (keeping current keys): {e}");
                    }
                })
                .ok();
        }
        Ok(KeySource::JwksHttp(cache))
    }

    fn select(&self, header: &jsonwebtoken::Header) -> Result<DecodingKey, AuthError> {
        match self {
            KeySource::StaticPem(keys) => {
                let chosen = match &header.kid {
                    Some(kid) => keys
                        .iter()
                        .find(|k| k.kid.as_deref() == Some(kid.as_str()))
                        .or_else(|| keys.iter().find(|k| k.kid.is_none())),
                    None => keys.first(),
                }
                .ok_or_else(|| AuthError::UnknownKey(header.kid.clone()))?;
                build_static_key(chosen)
            }
            KeySource::Jwks(set) => select_from_jwks(set, header),
            #[cfg(feature = "jwks-http")]
            KeySource::JwksHttp(cache) => {
                let set = cache.keys.read().unwrap();
                select_from_jwks(&set, header)
            }
        }
    }
}

/// Select a key from a parsed JWKS set by the token's `kid` (or the sole key
/// when the token carries none) and build a `DecodingKey`. Shared by the static
/// `Jwks` and the fetched `JwksHttp` sources (#81).
fn select_from_jwks(
    set: &jsonwebtoken::jwk::JwkSet,
    header: &jsonwebtoken::Header,
) -> Result<DecodingKey, AuthError> {
    let jwk = match &header.kid {
        Some(kid) => set.find(kid),
        None => set.keys.first(),
    }
    .ok_or_else(|| AuthError::UnknownKey(header.kid.clone()))?;
    DecodingKey::from_jwk(jwk).map_err(|e| AuthError::Key(e.to_string()))
}

fn build_static_key(k: &StaticKey) -> Result<DecodingKey, AuthError> {
    let key = match k.algorithm {
        Algorithm::RS256
        | Algorithm::RS384
        | Algorithm::RS512
        | Algorithm::PS256
        | Algorithm::PS384
        | Algorithm::PS512 => DecodingKey::from_rsa_pem(k.pem.as_bytes()),
        Algorithm::ES256 | Algorithm::ES384 => DecodingKey::from_ec_pem(k.pem.as_bytes()),
        Algorithm::EdDSA => DecodingKey::from_ed_pem(k.pem.as_bytes()),
        // Symmetric HMAC keys are rejected structurally: verify-only auth uses
        // asymmetric keys so the process never holds a signing secret. This also
        // closes the classic HS/RS `alg` confusion attack even if HS* slipped
        // into the allowlist.
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
            return Err(AuthError::Invalid(
                "symmetric (HS*) algorithms are not supported — verify-only auth is asymmetric"
                    .into(),
            ));
        }
    };
    key.map_err(|e| AuthError::Key(e.to_string()))
}

/// Verification policy. Every field comes from deployment config
/// (`forgedb.toml` / env) — **never** from a `.forge` schema.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Allowed signature algorithms. Pin to asymmetric families (RS*/PS*/ES*/
    /// EdDSA). `alg: none` and anything outside this list are rejected.
    pub algorithms: Vec<Algorithm>,
    /// Expected `iss`. `None` disables the issuer check (not recommended).
    pub issuer: Option<String>,
    /// Expected `aud`. `None` disables the audience check.
    pub audience: Option<String>,
    /// Name of the claim carrying the tenant identity (e.g. `"tenant"`, `"org"`).
    pub tenant_claim: String,
    /// Clock-skew leeway in seconds for `exp`/`nbf`/`iat`.
    pub leeway_secs: u64,
    /// Claim names that must be present (beyond `exp`, always required).
    pub required_claims: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig {
            algorithms: vec![Algorithm::RS256],
            issuer: None,
            audience: None,
            tenant_claim: "tenant".to_string(),
            leeway_secs: 60,
            required_claims: Vec::new(),
        }
    }
}

/// An authenticated caller. Everything here is opaque data for handlers — it
/// carries no enforcement logic. `tenant` has already been cross-checked to
/// equal the process's tenant by the time a `Principal` exists.
#[derive(Debug, Clone)]
pub struct Principal {
    /// The `sub` claim.
    pub subject: String,
    /// The verified tenant (== the process's tenant).
    pub tenant: String,
    /// Roles/scopes lifted from `roles`/`scp`/`scope`/`permissions`, if present.
    pub roles: Vec<String>,
    /// The full decoded claim set, for handlers that need more.
    pub claims: HashMap<String, Value>,
}

/// Verifies tokens and enforces the single tenant cross-check for one process.
///
/// Construct one per process at startup with the process's tenant identity; it
/// is cheap to wrap in an `Arc` and share across requests.
pub struct Authenticator {
    config: AuthConfig,
    keys: KeySource,
    /// The one tenant this process serves. A verified token whose tenant claim
    /// differs is rejected with 403. Opaque string, compared for equality — it
    /// carries no model/row/schema meaning.
    process_tenant: String,
}

impl Authenticator {
    /// Build an authenticator for a process serving `process_tenant`.
    pub fn new(config: AuthConfig, keys: KeySource, process_tenant: impl Into<String>) -> Self {
        Authenticator {
            config,
            keys,
            process_tenant: process_tenant.into(),
        }
    }

    /// The tenant this process serves (the cross-check target).
    pub fn process_tenant(&self) -> &str {
        &self.process_tenant
    }

    /// Verify a raw bearer token and cross-check its tenant claim against the
    /// process tenant. On success returns the authenticated [`Principal`].
    pub fn authenticate(&self, token: &str) -> Result<Principal, AuthError> {
        let header = decode_header(token).map_err(|e| AuthError::Invalid(e.to_string()))?;

        // Algorithm pinning up front: reject `none` and anything off the
        // allowlist before touching keys.
        if !self.config.algorithms.contains(&header.alg) {
            return Err(AuthError::AlgorithmNotAllowed(header.alg));
        }

        let key = self.keys.select(&header)?;

        let mut validation = Validation::new(header.alg);
        validation.algorithms = self.config.algorithms.clone();
        validation.leeway = self.config.leeway_secs;
        // `exp` is the only spec claim we force; custom required claims are
        // checked below (set_required_spec_claims accepts spec claims only).
        validation.set_required_spec_claims(&["exp"]);
        if let Some(iss) = &self.config.issuer {
            validation.set_issuer(&[iss]);
        }
        match &self.config.audience {
            Some(aud) => validation.set_audience(&[aud]),
            None => validation.validate_aud = false,
        }

        let data = decode::<HashMap<String, Value>>(token, &key, &validation)
            .map_err(|e| AuthError::Invalid(e.to_string()))?;
        let claims = data.claims;

        for rc in &self.config.required_claims {
            if !claims.contains_key(rc) {
                return Err(AuthError::MissingRequiredClaim(rc.clone()));
            }
        }

        let tenant = claims
            .get(&self.config.tenant_claim)
            .and_then(Value::as_str)
            .ok_or_else(|| AuthError::MissingTenantClaim {
                claim: self.config.tenant_claim.clone(),
            })?
            .to_string();

        // The tenant cross-check: opaque string equality, no model vocabulary.
        if tenant != self.process_tenant {
            return Err(AuthError::TenantMismatch {
                token: tenant,
                process: self.process_tenant.clone(),
            });
        }

        let subject = claims
            .get("sub")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let roles = extract_roles(&claims);

        Ok(Principal {
            subject,
            tenant,
            roles,
            claims,
        })
    }
}

/// Lift roles/scopes from the common claim shapes: an array under
/// `roles`/`permissions`, or a space-delimited string under `scope`/`scp`.
fn extract_roles(claims: &HashMap<String, Value>) -> Vec<String> {
    for key in ["roles", "permissions", "scp", "scope"] {
        match claims.get(key) {
            Some(Value::Array(arr)) => {
                return arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
            Some(Value::String(s)) => {
                return s.split_whitespace().map(String::from).collect();
            }
            _ => {}
        }
    }
    Vec::new()
}

#[cfg(feature = "axum")]
pub mod axum_mw {
    //! axum middleware glue: verify the bearer token, cross-check the tenant,
    //! and inject the [`Principal`] into request extensions. Pure transport —
    //! it carries the authenticated principal into handlers and makes the
    //! 401/403 decision; it holds no schema knowledge.

    use std::sync::Arc;

    use axum::{
        body::Body,
        extract::State,
        http::{header::AUTHORIZATION, Request, StatusCode},
        middleware::Next,
        response::{IntoResponse, Response},
        Json,
    };

    use super::{AuthError, Authenticator};

    /// axum middleware (for `from_fn_with_state`): authenticate + tenant
    /// cross-check, then inject the [`super::Principal`] into request extensions.
    /// Rejects with 401 (auth failure) or 403 (tenant mismatch).
    pub async fn require_tenant(
        State(auth): State<Arc<Authenticator>>,
        mut req: Request<Body>,
        next: Next,
    ) -> Response {
        let token = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                s.strip_prefix("Bearer ")
                    .or_else(|| s.strip_prefix("bearer "))
            })
            .map(|s| s.trim().to_string());

        let token = match token {
            Some(t) if !t.is_empty() => t,
            _ => return reject(&AuthError::MissingToken),
        };

        match auth.authenticate(&token) {
            Ok(principal) => {
                req.extensions_mut().insert(principal);
                next.run(req).await
            }
            Err(e) => reject(&e),
        }
    }

    fn reject(e: &AuthError) -> Response {
        let status =
            StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::UNAUTHORIZED);
        (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
    }
}

#[cfg(all(test, feature = "jwks-http"))]
mod jwks_http_tests {
    use super::{AuthError, KeySource};
    use std::time::Duration;

    #[test]
    fn jwks_url_fails_loud_when_unreachable(){
        // #81: the initial fetch is synchronous, so an unreachable IdP is a hard
        // error at construction — never a silently-unauthenticated server. Port 1
        // on loopback refuses fast. refresh_interval 0 disables the background
        // thread (this test only exercises the initial fetch).
        match KeySource::jwks_url("http://127.0.0.1:1/.well-known/jwks.json", Duration::ZERO) {
            Ok(_) => panic!("unreachable JWKS endpoint must fail loud, not succeed"),
            Err(e) => assert!(
                matches!(e, AuthError::Fetch { .. }),
                "expected a Fetch error, got {e:?}"
            ),
        }
    }
}
