//! Verify-only JWT + tenant cross-check tests. Tokens are signed in-test with a
//! throwaway RSA keypair (below), so the whole path — signature, claims, tenant
//! cross-check, principal extraction — is exercised fully offline.

use std::collections::HashMap;

use forgedb_auth::{AuthConfig, AuthError, Authenticator, KeySource};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::json;

// Throwaway 2048-bit RSA keypair — TEST ONLY, never a real key.
const TEST_PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC00dZHGD92WIm2
4EojxoYOUmYT6XOEhoUEI7EJ3kzfLwdE2stVPA2LuWuRUFmbjT/5rOFXR8Pujznm
xhiZZjYHYyfWUbaKzr1wHUVkfp4L6yLVTD5FHJlE7Ev9XQWN54lZ96wyikOt7ZDf
lQ5esn+4E+1bKGdEzevN0dcYVpSEVvZXYcWWopRMzqIhAd7i5IrwAoyD48RBNNas
9HqMQRIO7axuNzvGc7Rs2YeDhuABrYoyZTmzTXI3Mj8K5F9oUeYTZT8m2lfMKoGb
ISDSovx8enXMmV3+f9STnXO/5ZHgMyOT5HA7myiNt4KVk+ULL+yHlKTaICyWOJy4
8PZ+xuBdAgMBAAECggEAB5q5jxTdz7gqgSvj7Pw+NV+mwQNKOJJrCVZc7YQmqc4x
rpDLCK3Ks2EcEn5AE9LNaXkpTZaMOWisRTXN4V795Qf5AWzp/I10RGueuFAQ4oHE
Mxc5Q9vPiv6rCsqSNLpo5C/j0zOjDq9VLpSiCD6BMOMkUiHxXIs9IFNJCfzTfdCT
pPlxHKFL+/VyYIBt1/Z/aODAFyfZ3UvsLJxNsNSUtZp/Ghkx89+xRHlswHdUUfzX
7Qh65o+iw9HOlr0lotTVk0Kkt2MRUOfHIBqfjMtVjOv6ArN48EUtglxXTPWdmJEh
5jMZ/Hy0jxbmyKMC3IVDOf79m2LoOk8ox3TyTWjpqwKBgQDlaoM9+sPI2rgaBOXO
gslvCICyQISdjAK5bk40or0bNQcF+mY5pMxv7UF+APKoGcFM81rWSW9Fy0oZ1KbX
MC4TDgEl/HTv9H/pxJwo4b8GFNY1+9F1JBh5lW7nttbFrrkii3wQA7khQDK/B1hw
NeHOFKaIF2W3nNDHwZUKx0WlGwKBgQDJxb3/T/pqq09kH2DnhGA1xKlQrohfrXW4
Ux3gwCRad1VY4vgnUxWdT0bt+wJdte/OyhBK44J/wcEwig99Ssf42LG0MwntqGlP
KxTYSohw7Jm+jYkTth9Mi8uYt4e1zpzefwDXRSW4XyOW6A2mYhaVSKIENGJouX9S
sAUNKvv/5wKBgAcyeDuRimLaubvXO35nC/q3wZHWBFMM+Wjn1PxBvr0DxNyjJmHY
kbFROCTD0tkDNdU8LTVbyGngHssAqNtHX6qpXc/bQ6/jc7/Zsyx1KJEARlgbNk7+
euYVkg0i50n8WUKELbgy5bPtV6o2iMe8aQEWFMNgOIiyGrqpkAtuhPjRAoGANUTF
UmA1BnBPt2kpVjX2iHtxD+HkEw5iY9Vdr/ZKIrAakirpxMgEjtFdMnrwNvlPZFKo
Vn0V+NCYRk5MpJFXlfTvhVlsJ5gspUAEcs3Kk7WDKXGIXPHZ9YV6rMjXRUJU29C/
0hVpTfGgHbkJ0YFX4PWaAG4sBOXkHVpnGwDcIsUCgYEAp8bh4fGZzgbvWyEjSdgn
MuVZVGqC7WqzZQ4UPb5cCzXalkVRv4lXQ8beSbxEJO4ZFIqE19/EdUKTWSL3pY1A
0fym7VTXRnpHEdBTjCh+OXV0o3JRKLAhWTHd1D1pvSkyqyC4vJd+sqs1BWg31JaM
ec5TQRA+196oYabYm02iSUM=
-----END PRIVATE KEY-----";

const TEST_PUB_PEM: &str = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAtNHWRxg/dliJtuBKI8aG
DlJmE+lzhIaFBCOxCd5M3y8HRNrLVTwNi7lrkVBZm40/+azhV0fD7o855sYYmWY2
B2Mn1lG2is69cB1FZH6eC+si1Uw+RRyZROxL/V0FjeeJWfesMopDre2Q35UOXrJ/
uBPtWyhnRM3rzdHXGFaUhFb2V2HFlqKUTM6iIQHe4uSK8AKMg+PEQTTWrPR6jEES
Du2sbjc7xnO0bNmHg4bgAa2KMmU5s01yNzI/CuRfaFHmE2U/JtpXzCqBmyEg0qL8
fHp1zJld/n/Uk51zv+WR4DMjk+RwO5sojbeClZPlCy/sh5Sk2iAsljicuPD2fsbg
XQIDAQAB
-----END PUBLIC KEY-----";

/// Far-future expiry so tokens don't age out of the test suite.
const FAR_FUTURE_EXP: u64 = 4_102_444_800; // 2100-01-01

fn sign(claims: serde_json::Value) -> String {
    let key = EncodingKey::from_rsa_pem(TEST_PRIV_PEM.as_bytes()).unwrap();
    encode(&Header::new(Algorithm::RS256), &claims, &key).unwrap()
}

fn authenticator_for(process_tenant: &str) -> Authenticator {
    let cfg = AuthConfig {
        algorithms: vec![Algorithm::RS256],
        issuer: Some("https://idp.example".into()),
        audience: Some("forgedb".into()),
        tenant_claim: "tenant".into(),
        leeway_secs: 60,
        required_claims: vec![],
    };
    let keys = KeySource::static_pem(None, TEST_PUB_PEM, Algorithm::RS256);
    Authenticator::new(cfg, keys, process_tenant)
}

fn valid_claims(tenant: &str) -> serde_json::Value {
    json!({
        "sub": "user-123",
        "iss": "https://idp.example",
        "aud": "forgedb",
        "exp": FAR_FUTURE_EXP,
        "tenant": tenant,
        "roles": ["admin", "editor"],
    })
}

#[test]
fn accepts_valid_token_for_this_tenant() {
    let auth = authenticator_for("acme");
    let token = sign(valid_claims("acme"));
    let p = auth.authenticate(&token).expect("should authenticate");
    assert_eq!(p.subject, "user-123");
    assert_eq!(p.tenant, "acme");
    assert_eq!(p.roles, vec!["admin".to_string(), "editor".to_string()]);
}

#[test]
fn rejects_token_for_a_different_tenant_with_403() {
    let auth = authenticator_for("acme");
    // A perfectly valid token — but its tenant is "globex", not this process.
    let token = sign(valid_claims("globex"));
    let err = auth.authenticate(&token).unwrap_err();
    assert!(matches!(err, AuthError::TenantMismatch { .. }), "got {err:?}");
    assert_eq!(err.status_code(), 403);
}

#[test]
fn rejects_bad_signature_with_401() {
    let auth = authenticator_for("acme");
    let mut token = sign(valid_claims("acme"));
    // Corrupt the signature segment.
    token.pop();
    token.push(if token.ends_with('A') { 'B' } else { 'A' });
    let err = auth.authenticate(&token).unwrap_err();
    assert_eq!(err.status_code(), 401, "got {err:?}");
}

#[test]
fn rejects_wrong_issuer() {
    let auth = authenticator_for("acme");
    let token = sign(json!({
        "sub": "u", "iss": "https://evil.example", "aud": "forgedb",
        "exp": FAR_FUTURE_EXP, "tenant": "acme",
    }));
    let err = auth.authenticate(&token).unwrap_err();
    assert_eq!(err.status_code(), 401, "got {err:?}");
}

#[test]
fn rejects_wrong_audience() {
    let auth = authenticator_for("acme");
    let token = sign(json!({
        "sub": "u", "iss": "https://idp.example", "aud": "someone-else",
        "exp": FAR_FUTURE_EXP, "tenant": "acme",
    }));
    let err = auth.authenticate(&token).unwrap_err();
    assert_eq!(err.status_code(), 401, "got {err:?}");
}

#[test]
fn rejects_expired_token() {
    let auth = authenticator_for("acme");
    let token = sign(json!({
        "sub": "u", "iss": "https://idp.example", "aud": "forgedb",
        "exp": 1_000_000_000u64, // 2001 — long past, beyond leeway
        "tenant": "acme",
    }));
    let err = auth.authenticate(&token).unwrap_err();
    assert_eq!(err.status_code(), 401, "got {err:?}");
}

#[test]
fn rejects_algorithm_off_the_allowlist() {
    // Allowlist ES256 only, but the token is signed RS256.
    let cfg = AuthConfig {
        algorithms: vec![Algorithm::ES256],
        issuer: None,
        audience: None,
        tenant_claim: "tenant".into(),
        leeway_secs: 60,
        required_claims: vec![],
    };
    let keys = KeySource::static_pem(None, TEST_PUB_PEM, Algorithm::RS256);
    let auth = Authenticator::new(cfg, keys, "acme");
    let token = sign(valid_claims("acme"));
    let err = auth.authenticate(&token).unwrap_err();
    assert!(matches!(err, AuthError::AlgorithmNotAllowed(_)), "got {err:?}");
}

#[test]
fn rejects_missing_tenant_claim() {
    let auth = authenticator_for("acme");
    let token = sign(json!({
        "sub": "u", "iss": "https://idp.example", "aud": "forgedb",
        "exp": FAR_FUTURE_EXP, // no tenant claim
    }));
    let err = auth.authenticate(&token).unwrap_err();
    assert!(matches!(err, AuthError::MissingTenantClaim { .. }), "got {err:?}");
}

#[test]
fn enforces_required_custom_claims() {
    let mut cfg = AuthConfig {
        algorithms: vec![Algorithm::RS256],
        issuer: None,
        audience: None,
        tenant_claim: "tenant".into(),
        leeway_secs: 60,
        required_claims: vec!["org_id".into()],
    };
    cfg.required_claims = vec!["org_id".into()];
    let keys = KeySource::static_pem(None, TEST_PUB_PEM, Algorithm::RS256);
    let auth = Authenticator::new(cfg, keys, "acme");

    // Missing org_id -> rejected.
    let token = sign(json!({ "sub": "u", "exp": FAR_FUTURE_EXP, "tenant": "acme" }));
    let err = auth.authenticate(&token).unwrap_err();
    assert!(matches!(err, AuthError::MissingRequiredClaim(_)), "got {err:?}");

    // With org_id -> accepted.
    let token = sign(json!({ "sub": "u", "exp": FAR_FUTURE_EXP, "tenant": "acme", "org_id": "x" }));
    assert!(auth.authenticate(&token).is_ok());
}

#[test]
fn roles_from_space_delimited_scope() {
    let cfg = AuthConfig {
        algorithms: vec![Algorithm::RS256],
        issuer: None,
        audience: None,
        tenant_claim: "tenant".into(),
        leeway_secs: 60,
        required_claims: vec![],
    };
    let keys = KeySource::static_pem(None, TEST_PUB_PEM, Algorithm::RS256);
    let auth = Authenticator::new(cfg, keys, "acme");
    let token = sign(json!({
        "sub": "u", "exp": FAR_FUTURE_EXP, "tenant": "acme",
        "scope": "read write admin",
    }));
    let p = auth.authenticate(&token).unwrap();
    assert_eq!(p.roles, vec!["read", "write", "admin"]);
}

#[test]
fn full_claims_are_carried_on_the_principal() {
    let auth = authenticator_for("acme");
    let token = sign(valid_claims("acme"));
    let p = auth.authenticate(&token).unwrap();
    let claims: &HashMap<String, serde_json::Value> = &p.claims;
    assert_eq!(claims.get("iss").and_then(|v| v.as_str()), Some("https://idp.example"));
}
