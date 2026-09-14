The token could not be decoded or failed signature or claim verification;
carries the underlying `jsonwebtoken` error text.

Covers a malformed token or header, a bad signature, an expired or missing
`exp`, an `iss` or `aud` that does not match the configured value, an
[`AuthConfig::algorithms`] list mixing key families, and a [`StaticKey`] whose
[`StaticKey::algorithm`] is a symmetric `HS*` algorithm.
