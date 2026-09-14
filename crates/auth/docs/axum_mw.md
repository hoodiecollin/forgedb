axum middleware: verify the bearer token, cross-check the tenant, and inject
the [`crate::Principal`] into request extensions.

Pure transport. It makes the 401/403 decision from
[`crate::AuthError::status_code`] and carries the principal into handlers; it
holds no schema knowledge. Only present with the `axum` feature (on by
default).
