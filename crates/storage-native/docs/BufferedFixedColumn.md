An in-memory bulk-loaded selection of a [`FixedColumn`]'s rows (#168).

Produced by [`FixedColumn::gather_buffered`], it holds one column's selected
rows as a single contiguous buffer (a zero-copy `mmap` alias of the dense
prefix, or a gathered copy) and exposes the **same** positional
`read_*` accessors as [`FixedColumn`], addressed by **slot** (`0..n` over the
selection order) rather than physical row index.  Decoding one column of a
scan from this buffer costs no syscalls; it lets the generated column scan
decode with the identical per-field logic it uses for per-row reads.

Errors mirror [`FixedColumn`]: an out-of-range slot returns `InvalidInput`.
