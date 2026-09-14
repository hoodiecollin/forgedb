A JWKS document fetched over HTTP, cached behind a lock, and refreshed by a
background thread (#81). Schema-agnostic — cryptographic material only, the
same class as [`KeySource`]. Cloneable handle (`Arc`) so the refresh thread
and the [`Authenticator`] share one cache.
