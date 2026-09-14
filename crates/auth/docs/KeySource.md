Where verification keys come from.

Every variant resolves one public key per token from the header's `kid`: a
matching key when the token names one, otherwise the first key the source
holds. [`KeySource::StaticPem`] additionally falls back to a key with no
`kid`. A source that yields no key is [`AuthError::UnknownKey`]. This type
holds cryptographic material only.
