Expected `aud`. `None` disables the audience check entirely.

When set, a token whose `aud` (a string, or an array with no matching element)
differs is [`AuthError::Invalid`]; a token with no `aud` claim still passes.
Add `"aud"` to [`AuthConfig::required_claims`] to make the claim mandatory.
