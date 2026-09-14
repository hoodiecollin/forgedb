axum middleware glue: verify the bearer token, cross-check the tenant,
and inject the [`Principal`] into request extensions. Pure transport —
it carries the authenticated principal into handlers and makes the
401/403 decision; it holds no schema knowledge.
