No verification key could be selected for the token; carries the header's
`kid`, if any.

Returned when the token names a `kid` that no key matches (for
[`KeySource::StaticPem`], only after also failing to find a key with no
`kid`), or when the token has no `kid` and the source holds no keys at all.
