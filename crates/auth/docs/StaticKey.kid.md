Key id matched against the token header's `kid`.

When the token carries a `kid` and no key's id equals it, the first key whose
`kid` is `None` is used instead. When the token carries no `kid`, the first key
in the list is used regardless of this field.
