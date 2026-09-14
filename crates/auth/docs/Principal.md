An authenticated caller, as returned by [`Authenticator::authenticate`] and
inserted into request extensions by the axum middleware.

Everything here is opaque data for handlers; the type carries no enforcement
logic. By the time a `Principal` exists, [`Principal::tenant`] has been
cross-checked to equal the process tenant.
