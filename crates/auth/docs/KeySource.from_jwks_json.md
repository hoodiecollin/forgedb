Parse a JWK Set document into a [`KeySource::Jwks`].

Offline and pure: no HTTP is involved and the set is never refreshed. A
document that does not deserialize as a JWK Set is [`AuthError::Key`]. For the
fetch-and-refresh form see [`KeySource::jwks_url`].
