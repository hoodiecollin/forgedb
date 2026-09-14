Builds a sort from the optional `sort` and `order` parameters.

`None` when there is no `sort` field; the field is taken verbatim, even when empty. `order` goes through [`SortOrder::from_str`], and an absent or unrecognised value falls back to [`SortOrder::Asc`] rather than failing.
