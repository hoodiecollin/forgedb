Fetch a JWK Set from `url` and keep it fresh, yielding a
[`KeySource::JwksHttp`].

The first fetch is synchronous, so an unreachable IdP or a non-success status
fails here as [`AuthError::Fetch`] and an unparsable body as
[`AuthError::Key`], rather than producing a server that verifies nothing.
When `refresh_interval` is non-zero, a thread named `forgedb-jwks-refresh`
then sleeps that long and re-fetches, for the life of the process, so a
rotated-in signing key is seen within one interval. A failed refresh keeps the
current key set and writes one line to stderr; a thread that cannot be spawned
is ignored. A zero interval fetches once and never refreshes.
