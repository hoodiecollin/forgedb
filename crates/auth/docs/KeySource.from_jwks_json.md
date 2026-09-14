Parse a JWKS document (e.g. the body of `.well-known/jwks.json`). Offline
and pure (no HTTP) so it is fully testable; for the fetch-and-refresh
variant see [`KeySource::jwks_url`] (#81).
