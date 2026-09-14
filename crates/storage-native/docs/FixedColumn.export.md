Export the rows at `indices` as a [`ColumnExport`]: a zero-copy `mmap` alias when `indices` is exactly the dense prefix `0, 1, ..., n - 1` with `0 < n <= len()`, otherwise an owned [`FixedColumn::gather`] copy.

The bytes are identical either way (`indices.len() * value_size`), so the consumer is transparent to which path was taken. The column inspects only the shape of `indices`, never a field name, type or schema. See [`ColumnExport`] for the aliasing invariant a mapped export relies on.

Returns `InvalidInput` if any index is `>= len()`, or the I/O error if mapping the prefix fails.
