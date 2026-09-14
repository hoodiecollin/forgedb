An in-memory bulk-loaded selection of a [`VariableColumn`]'s rows (#168).

Produced by [`VariableColumn::gather_buffered`]; holds the column's data
bytes plus each selected slot's `(absolute offset, length)` so `read_string`
slices from memory without a syscall.  The variable-column peer of
[`BufferedFixedColumn`]: same `read_string` name as [`VariableColumn`],
addressed by **slot** (`0..n` over the selection order).
