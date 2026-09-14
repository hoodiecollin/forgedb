The state behind [`KeySource::JwksHttp`]: the JWKS URL and the most recently
fetched JWK Set behind an `RwLock`.

Created only by [`KeySource::jwks_url`], which shares it between the source
and the refresh thread through an `Arc`; it exposes no public methods. Each
key selection reads the current set under the lock, and a refresh replaces the
whole set at once. Only present with the `jwks-http` feature.
