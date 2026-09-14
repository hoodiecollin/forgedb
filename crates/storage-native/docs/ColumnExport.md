The bytes of one column at one row selection, held either as an owned copy or as a read-only `mmap` alias of the column file.

Both forms expose the same `as_ptr` / `len` / `as_slice` surface, so a consumer such as generated FFI glue handing an Arrow buffer across the boundary is transparent to which path produced it. [`FixedColumn::export`] returns [`ColumnExport::Mapped`] when the selection is exactly the dense prefix `[0, n)` and [`ColumnExport::Owned`] (a [`FixedColumn::gather`] copy) otherwise; [`VariableColumn::gather_buffered`] uses it for the spanned data region. Numeric columns are stored little-endian, so a mapped prefix is a valid Arrow primitive buffer as-is.

# Aliasing invariant for `Mapped`

A mapping aliases live file pages, so it relies on the single-writer, append-only discipline the reader handles already assume: the mapped rows are committed and immutable (appends only extend the file past them), and no compaction or `truncate_to_rows` that would rewrite or shorten the mapped range runs while the export is outstanding. Under that discipline the alias observes a stable snapshot.
