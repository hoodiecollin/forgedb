Schema-agnostic parsing of a REST list endpoint's URL query string into filter, sort and pagination values.

[`QueryParams::from_query_string`] decodes a `key=value&…` string and [`QueryParams::from_map`] consumes an already-decoded map. Both split the parameters into three parts:

- `sort` and `order` become a [`Sort`]. `order` accepts `asc`, `ascending`, `desc` or `descending` in any letter case and defaults to ascending; without `sort` there is no sort at all.
- `limit` and `offset` become a [`Pagination`]. `limit` defaults to [`DEFAULT_LIMIT`] and is clamped to `1..=`[`MAX_LIMIT`]; `offset` defaults to `0`. A value that is not a non-negative integer is treated as absent.
- Every other parameter becomes a [`Filter`] on that field, with a [`FilterValue`] classified from the text alone: a number when the text round-trips through `f64` exactly, a bool for exactly `true` or `false`, otherwise a string.

```text
GET /users?status=active&age=30&sort=created_at&order=desc&limit=20&offset=40
```

Nothing here reads a `.forge` schema. Field names are carried as opaque strings, and whether `status` is a real column, what type it has and how it compares is decided by the caller. The generated ForgeDB REST `list` handler calls [`QueryParams::from_map`] on the request's query map, filters and sorts with its own per-model generated code, and pages the result with [`Pagination::apply`].

The public surface is what is re-exported at the crate root: [`QueryParams`], [`Filter`], [`FilterValue`], [`Sort`], [`SortOrder`], [`Pagination`], [`DEFAULT_LIMIT`] and [`MAX_LIMIT`].
