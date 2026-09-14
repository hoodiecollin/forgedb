An in-memory selection of a [`VariableColumn`]'s rows, produced by [`VariableColumn::gather_buffered`].

It holds the selected rows' bytes (a mapped or copied span of the data file, or for a sparse selection a packed copy of just the selected rows) plus each slot's `(offset, length)` into that buffer, so reads slice from memory without a syscall. Slots are addressed `0..len()` in selection order; slot `i` is row `indices[i]`.
