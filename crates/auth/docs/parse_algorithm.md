Parse an algorithm name such as `"RS256"` or `"ES256"` into an [`Algorithm`].

Matching trims whitespace and is case-insensitive. Accepted names: `RS256`,
`RS384`, `RS512`, `PS256`, `PS384`, `PS512`, `ES256`, `ES384`, `EdDSA`. Any
other name, including the symmetric `HS*` family, returns `None`.
