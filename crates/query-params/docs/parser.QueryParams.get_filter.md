Returns the filter whose [`Filter::field`] equals `field` exactly (case-sensitive), or `None`.

The parsers produce at most one filter per field, so this is the only one there is; with [`Self::new`] and duplicate fields, the first match wins.
