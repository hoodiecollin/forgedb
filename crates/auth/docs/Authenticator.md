Verifies tokens and enforces the single tenant cross-check for one process.

Construct one per process at startup with the process's tenant identity; it
is cheap to wrap in an `Arc` and share across requests.
