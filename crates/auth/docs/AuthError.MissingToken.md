The request carried no usable `Authorization: Bearer <token>` header.

Produced only by [`axum_mw::require_tenant`]: the header is absent, is not
valid UTF-8, does not start with `Bearer ` or `bearer `, or has nothing after
the prefix. [`Authenticator::authenticate`] never returns it.
