# forgedb-auth

Schema-agnostic, verify-only JWT authentication for ForgeDB generated servers,
plus a single tenant cross-check.

The crate verifies tokens; it never issues them, stores users, or reads a
`.forge` schema. It holds no notion of models, rows, columns or policies: its
whole vocabulary is "verify a token", "read a configured claim", "compare two
opaque strings" and "carry a principal". Any authorization beyond the tenant
cross-check is the caller's job.

## What it does

Given a raw bearer token, an [`AuthConfig`] and a single `process_tenant`
string, [`Authenticator::authenticate`]:

1. decodes the token header and rejects any signature algorithm outside
   [`AuthConfig::algorithms`];
2. selects a public key by the header's `kid` from a [`KeySource`] (static PEM
   keys, a parsed JWK Set, or a JWKS URL kept fresh in the background);
3. verifies the signature and validates `exp` (with
   [`AuthConfig::leeway_secs`] of skew), `iss` and `aud` where configured, and
   the presence of every [`AuthConfig::required_claims`] entry;
4. reads [`AuthConfig::tenant_claim`] and compares it with the process tenant
   by string equality; a mismatch is [`AuthError::TenantMismatch`], the only
   failure that maps to HTTP 403;
5. returns a [`Principal`] carrying the subject, tenant, roles and the raw
   claim map.

Verification is asymmetric: [`parse_algorithm`] refuses `HS*` names, and a
[`StaticKey`] configured with an `HS*` algorithm fails every verification.
`nbf` and `iat` are not checked.

## Features

- `axum` (default): the [`axum_mw`] module, an axum middleware that runs the
  steps above on the `Authorization: Bearer` header and injects the
  [`Principal`] into request extensions.
- `jwks-http`: [`KeySource::jwks_url`], [`KeySource::JwksHttp`] and
  [`JwksHttpCache`], which fetch a JWK Set over HTTP with `ureq` and refresh it
  on a background thread.
