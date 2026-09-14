#![doc = include_str!("../docs/crate.md")]
use std::collections::HashMap;

use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use serde_json::Value;

pub use jsonwebtoken::Algorithm;

#[doc = include_str!("../docs/parse_algorithm.md")]
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

#[doc = include_str!("../docs/AuthError.md")]
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[doc = include_str!("../docs/AuthError.MissingToken.md")]
    #[error("missing or malformed Authorization header")]
    MissingToken,
    #[doc = include_str!("../docs/AuthError.AlgorithmNotAllowed.md")]
    #[error("token algorithm {0:?} is not in the allowlist")]
    AlgorithmNotAllowed(Algorithm),
    #[doc = include_str!("../docs/AuthError.Invalid.md")]
    #[error("token verification failed: {0}")]
    Invalid(String),
    #[doc = include_str!("../docs/AuthError.UnknownKey.md")]
    #[error("no verification key for kid {0:?}")]
    UnknownKey(Option<String>),
    #[doc = include_str!("../docs/AuthError.MissingTenantClaim.md")]
    #[error("tenant claim '{claim}' missing from token")]
    MissingTenantClaim {
        #[doc = include_str!("../docs/AuthError.MissingTenantClaim.claim.md")]
        claim: String,
    },
    #[doc = include_str!("../docs/AuthError.MissingRequiredClaim.md")]
    #[error("required claim '{0}' missing from token")]
    MissingRequiredClaim(String),
    #[doc = include_str!("../docs/AuthError.TenantMismatch.md")]
    #[error(
        "tenant mismatch: token tenant '{token}' is not authorized for this process (serves '{process}')"
    )]
    TenantMismatch {
        #[doc = include_str!("../docs/AuthError.TenantMismatch.token.md")]
        token: String,
        #[doc = include_str!("../docs/AuthError.TenantMismatch.process.md")]
        process: String,
    },
    #[doc = include_str!("../docs/AuthError.Key.md")]
    #[error("invalid verification key material: {0}")]
    Key(String),
    #[doc = include_str!("../docs/AuthError.Fetch.md")]
    #[cfg(feature = "jwks-http")]
    #[error("failed to fetch JWKS from {url}: {reason}")]
    Fetch {
        #[doc = include_str!("../docs/AuthError.Fetch.url.md")]
        url: String,
        #[doc = include_str!("../docs/AuthError.Fetch.reason.md")]
        reason: String,
    },
}

impl AuthError {
    #[doc = include_str!("../docs/AuthError.status_code.md")]
    pub fn status_code(&self) -> u16 {
        match self {
            AuthError::TenantMismatch { .. } => 403,
            _ => 401,
        }
    }
}

#[doc = include_str!("../docs/StaticKey.md")]
#[derive(Debug, Clone)]
pub struct StaticKey {
    #[doc = include_str!("../docs/StaticKey.kid.md")]
    pub kid: Option<String>,
    #[doc = include_str!("../docs/StaticKey.pem.md")]
    pub pem: String,
    #[doc = include_str!("../docs/StaticKey.algorithm.md")]
    pub algorithm: Algorithm,
}

#[doc = include_str!("../docs/KeySource.md")]
pub enum KeySource {
    #[doc = include_str!("../docs/KeySource.StaticPem.md")]
    StaticPem(Vec<StaticKey>),
    #[doc = include_str!("../docs/KeySource.Jwks.md")]
    Jwks(jsonwebtoken::jwk::JwkSet),
    #[doc = include_str!("../docs/KeySource.JwksHttp.md")]
    #[cfg(feature = "jwks-http")]
    JwksHttp(std::sync::Arc<JwksHttpCache>),
}

#[doc = include_str!("../docs/JwksHttpCache.md")]
#[cfg(feature = "jwks-http")]
pub struct JwksHttpCache {
    url: String,
    keys: std::sync::RwLock<jsonwebtoken::jwk::JwkSet>,
}

#[cfg(feature = "jwks-http")]
impl JwksHttpCache {
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

    fn refresh(&self) -> Result<(), AuthError> {
        let set = Self::fetch(&self.url)?;
        *self.keys.write().unwrap() = set;
        Ok(())
    }
}

impl KeySource {
    #[doc = include_str!("../docs/KeySource.static_pem.md")]
    pub fn static_pem(kid: Option<String>, pem: impl Into<String>, algorithm: Algorithm) -> Self {
        KeySource::StaticPem(vec![StaticKey {
            kid,
            pem: pem.into(),
            algorithm,
        }])
    }

    #[doc = include_str!("../docs/KeySource.from_jwks_json.md")]
    pub fn from_jwks_json(json: &str) -> Result<Self, AuthError> {
        let set: jsonwebtoken::jwk::JwkSet =
            serde_json::from_str(json).map_err(|e| AuthError::Key(e.to_string()))?;
        Ok(KeySource::Jwks(set))
    }

    #[doc = include_str!("../docs/KeySource.jwks_url.md")]
    #[cfg(feature = "jwks-http")]
    pub fn jwks_url(
        url: impl Into<String>,
        refresh_interval: std::time::Duration,
    ) -> Result<Self, AuthError> {
        let url = url.into();
        let set = JwksHttpCache::fetch(&url)?;
        let cache = std::sync::Arc::new(JwksHttpCache {
            url,
            keys: std::sync::RwLock::new(set),
        });
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
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
            return Err(AuthError::Invalid(
                "symmetric (HS*) algorithms are not supported — verify-only auth is asymmetric"
                    .into(),
            ));
        }
    };
    key.map_err(|e| AuthError::Key(e.to_string()))
}

#[doc = include_str!("../docs/AuthConfig.md")]
#[derive(Debug, Clone)]
pub struct AuthConfig {
    #[doc = include_str!("../docs/AuthConfig.algorithms.md")]
    pub algorithms: Vec<Algorithm>,
    #[doc = include_str!("../docs/AuthConfig.issuer.md")]
    pub issuer: Option<String>,
    #[doc = include_str!("../docs/AuthConfig.audience.md")]
    pub audience: Option<String>,
    #[doc = include_str!("../docs/AuthConfig.tenant_claim.md")]
    pub tenant_claim: String,
    #[doc = include_str!("../docs/AuthConfig.leeway_secs.md")]
    pub leeway_secs: u64,
    #[doc = include_str!("../docs/AuthConfig.required_claims.md")]
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

#[doc = include_str!("../docs/Principal.md")]
#[derive(Debug, Clone)]
pub struct Principal {
    #[doc = include_str!("../docs/Principal.subject.md")]
    pub subject: String,
    #[doc = include_str!("../docs/Principal.tenant.md")]
    pub tenant: String,
    #[doc = include_str!("../docs/Principal.roles.md")]
    pub roles: Vec<String>,
    #[doc = include_str!("../docs/Principal.claims.md")]
    pub claims: HashMap<String, Value>,
}

#[doc = include_str!("../docs/Authenticator.md")]
pub struct Authenticator {
    config: AuthConfig,
    keys: KeySource,
    process_tenant: String,
}

impl Authenticator {
    #[doc = include_str!("../docs/Authenticator.new.md")]
    pub fn new(config: AuthConfig, keys: KeySource, process_tenant: impl Into<String>) -> Self {
        Authenticator {
            config,
            keys,
            process_tenant: process_tenant.into(),
        }
    }

    #[doc = include_str!("../docs/Authenticator.process_tenant.md")]
    pub fn process_tenant(&self) -> &str {
        &self.process_tenant
    }

    #[doc = include_str!("../docs/Authenticator.authenticate.md")]
    pub fn authenticate(&self, token: &str) -> Result<Principal, AuthError> {
        let header = decode_header(token).map_err(|e| AuthError::Invalid(e.to_string()))?;

        if !self.config.algorithms.contains(&header.alg) {
            return Err(AuthError::AlgorithmNotAllowed(header.alg));
        }

        let key = self.keys.select(&header)?;

        let mut validation = Validation::new(header.alg);
        validation.algorithms = self.config.algorithms.clone();
        validation.leeway = self.config.leeway_secs;
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

#[doc = include_str!("../docs/axum_mw.md")]
#[cfg(feature = "axum")]
pub mod axum_mw {

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

    #[doc = include_str!("../docs/axum_mw.require_tenant.md")]
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
        match KeySource::jwks_url("http://127.0.0.1:1/.well-known/jwks.json", Duration::ZERO) {
            Ok(_) => panic!("unreachable JWKS endpoint must fail loud, not succeed"),
            Err(e) => assert!(
                matches!(e, AuthError::Fetch { .. }),
                "expected a Fetch error, got {e:?}"
            ),
        }
    }
}
