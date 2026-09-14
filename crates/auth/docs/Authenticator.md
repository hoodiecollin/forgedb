Verifies tokens and enforces the tenant cross-check for one process.

Construct one at startup with the process's tenant identity and share it
across requests; the axum middleware takes it as `Arc<Authenticator>` state.
