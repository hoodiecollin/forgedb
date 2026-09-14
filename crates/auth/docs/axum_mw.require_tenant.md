Middleware for `axum::middleware::from_fn_with_state` with an
`Arc<Authenticator>` as state.

Reads the `Authorization` header, accepting a `Bearer ` or `bearer ` prefix,
and passes the trimmed token to [`Authenticator::authenticate`]. On success
the [`crate::Principal`] is inserted into the request's extensions and the
request continues. On failure the response carries the error's
[`AuthError::status_code`] (`401`, or `403` for a tenant mismatch) and a JSON
body of the form `{"error": "<message>"}`; a missing or malformed header is
[`AuthError::MissingToken`].
