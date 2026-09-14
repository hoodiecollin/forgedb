Parse an algorithm name (e.g. `"RS256"`, `"ES256"`). Asymmetric families
only — `HS*` deliberately returns `None` (verify-only auth is asymmetric).
