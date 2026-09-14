Variable-length column storage: a data file of concatenated value bytes plus an offsets file holding one 16-byte entry per row, a little-endian `(offset: u64, length: u64)` pair locating the row's bytes in the data file.

Values are appended to the data file in row order, so data offsets are monotonic in row index. Reads take `&self` and use positional I/O; appends take `&mut self`, seek both files to their ends and write. The row count and the data-file append position are maintained in memory and refreshed from the files only by [`VariableColumn::sync_from_disk`]. The column stores opaque bytes; [`VariableColumn::read_string`] additionally requires them to be valid UTF-8.

# Durability

Appends write to the OS page cache and do **not** fsync. A crash before [`VariableColumn::flush`] (or `sync_to_drive` followed by `barrier`) can lose the most recent appends from either file. Flush at commit or checkpoint boundaries, or before advancing the WAL checkpoint.
