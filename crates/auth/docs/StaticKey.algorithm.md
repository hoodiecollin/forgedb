The algorithm this key verifies; it selects the PEM decoder (`RS*`/`PS*`:
RSA, `ES*`: EC, `EdDSA`: Ed25519).

A symmetric `HS*` value makes every verification that selects this key fail
with [`AuthError::Invalid`]. The token's own `alg` must additionally be in
[`AuthConfig::algorithms`].
