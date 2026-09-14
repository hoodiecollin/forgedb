Copy the rows at `indices` into one contiguous owned buffer of `indices.len() * value_size` bytes, in the order given.

`indices` are opaque physical row positions the caller computed (typically the live set after tombstone filtering); the column reads no schema. Duplicates and any order are allowed, and an empty `indices` returns an empty buffer.

For a selection of at least 8 rows the spanned file region from `min(indices)` to `max(indices)` is mapped once and consecutive runs of indices are copied out with one `memcpy` each; if the mapping fails, or for fewer rows, each row is read with its own positional read. The mapping is held only for the call and the result is owned, so the only requirement is that the spanned rows are committed and no truncation runs concurrently, which the single-writer discipline guarantees. A sparse selection spanning a large file maps the whole span and may fault in more pages than the per-row path would read.

Returns `InvalidInput` if any index is `>= len()`.
