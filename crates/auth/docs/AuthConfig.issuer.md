Expected `iss`. `None` disables the issuer check.

When set, a token whose `iss` differs is [`AuthError::Invalid`]; a token with
no `iss` claim still passes. Add `"iss"` to [`AuthConfig::required_claims`]
to make the claim mandatory.
