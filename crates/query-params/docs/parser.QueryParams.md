The three parts of a parsed query string: filters, an optional sort and pagination.

Build one with [`Self::from_query_string`] or [`Self::from_map`]; both route `sort`/`order` to [`Sort`], `limit`/`offset` to [`Pagination`] and every remaining parameter to a [`Filter`]. `Default` is no filters, no sort and default pagination.

The derived `Deserialize` impl fills only [`Self::pagination`] (its `limit` and `offset` are flattened into this struct, unclamped); `filters` and `sort` are skipped and come out empty. Use the two constructors for a full parse.
