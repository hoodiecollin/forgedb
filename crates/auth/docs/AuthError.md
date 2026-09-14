A verification failure from [`Authenticator::authenticate`] or the axum
middleware.

[`AuthError::status_code`] maps each variant to the HTTP status the middleware
answers with: `403` for [`AuthError::TenantMismatch`], `401` for everything
else. The `Display` text is the `error` string in the middleware's JSON
rejection body.
