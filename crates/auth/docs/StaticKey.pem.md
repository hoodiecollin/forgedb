The PEM-encoded public key.

Accepted encodings follow [`StaticKey::algorithm`]: `RS*` and `PS*` accept a
PKCS#1 `RSA PUBLIC KEY` block or an SPKI `PUBLIC KEY` block; `ES*` and `EdDSA`
(Ed25519) accept an SPKI `PUBLIC KEY` block only. A PEM that does not decode
for the key's family is [`AuthError::Key`] at verification.
