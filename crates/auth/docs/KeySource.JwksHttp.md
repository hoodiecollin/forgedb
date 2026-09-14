A JWKS document fetched over HTTP and refreshed on a schedule (#81). The
cache holds the current key set behind a lock and a background thread
re-fetches it, so a key rotated in at the IdP is picked up at the next
refresh. Feature-gated to keep the default dep surface lean.
