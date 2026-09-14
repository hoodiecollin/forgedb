Verify a raw bearer token (no `Bearer ` prefix) and return the authenticated
[`Principal`].

In order: decode the header; reject an `alg` outside
[`AuthConfig::algorithms`]; select a key by `kid` from the [`KeySource`];
verify the signature and validate `exp` with leeway plus `iss` and `aud` where
configured; check that every [`AuthConfig::required_claims`] entry is present;
read [`AuthConfig::tenant_claim`] and compare it with
[`Self::process_tenant`], a mismatch being [`AuthError::TenantMismatch`]. The
first failing step's [`AuthError`] is returned.
