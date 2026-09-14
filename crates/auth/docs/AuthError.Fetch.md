The HTTP request for a JWK Set failed (transport error or non-success status),
or its body could not be read as text.

Raised by [`KeySource::jwks_url`] on the initial fetch and by the background
refresh, which then keeps the previous key set. A body that arrives but does
not parse is [`AuthError::Key`] instead. Only present with the `jwks-http`
feature.
