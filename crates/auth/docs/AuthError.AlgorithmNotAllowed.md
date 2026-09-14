The token header's `alg` is not in [`AuthConfig::algorithms`]; carries the
rejected algorithm.

Checked before any key is selected or the signature verified. A header whose
`alg` is not a known algorithm at all (`none` included) fails earlier as
[`AuthError::Invalid`].
