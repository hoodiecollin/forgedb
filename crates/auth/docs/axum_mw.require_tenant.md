axum middleware (for `from_fn_with_state`): authenticate + tenant
cross-check, then inject the [`super::Principal`] into request extensions.
Rejects with 401 (auth failure) or 403 (tenant mismatch).
