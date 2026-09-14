Fetch a JWKS document over HTTP and keep it fresh (#81). Fetches once
**synchronously** (so a bad URL / unreachable IdP fails loud at startup,
never a silently-unauthenticated server), then spawns a background thread
that re-fetches every `refresh_interval` — picking up a rotated-in signing
key within one interval. On a refresh error the previous key set is kept.
