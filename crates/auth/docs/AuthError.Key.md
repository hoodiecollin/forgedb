Key material could not be parsed; carries the parser's error text.

Raised for a JWKS body that does not deserialize as a JWK Set (from
[`KeySource::from_jwks_json`], [`KeySource::jwks_url`], or a background
refresh), a [`StaticKey::pem`] that does not decode for its
[`StaticKey::algorithm`], or a selected JWK whose parameters cannot form a
decoding key.
