An in-memory selection of a [`FixedColumn`]'s rows, produced by [`FixedColumn::gather_buffered`].

It holds the selected rows as one contiguous buffer (an `mmap` alias when the selection was the dense prefix, a gathered copy otherwise) and exposes the same `read_*` accessors as [`FixedColumn`], addressed by **slot**, `0..len()` in selection order, rather than by physical row index. Decoding a column from it costs no syscalls, so a column scan can use the identical per-field logic it uses for single-row reads.

Every accessor returns `InvalidInput` for a slot at or beyond [`BufferedFixedColumn::len`]. The typed accessors decode the leading bytes of the slot and panic if the column's `value_size` is narrower than the type they decode; call the accessor that matches the column's width.
