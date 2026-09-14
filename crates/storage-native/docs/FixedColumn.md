Fixed-width column storage: one file holding `value_size` bytes per row, addressed positionally as `offset = index * value_size`.

Reads take `&self` and use positional I/O (`read_exact_at`), never the file cursor, so they can run concurrently from many threads. Appends take `&mut self`, seek to the end of the file and write. The row count is maintained in memory and refreshed from the file only by [`FixedColumn::sync_from_disk`]. Values are little-endian; each typed `append_*` writes exactly its type's width and each typed `read_*` decodes the leading bytes of a row's slot, so the caller is responsible for using the accessors that match the column's `value_size`.

# Durability

Appends write to the OS page cache and do **not** fsync. Appended values are immediately readable through this or any other descriptor to the same file, but a crash before [`FixedColumn::flush`] (or `sync_to_drive` followed by `barrier`) can lose the most recent appends. Flush at commit or checkpoint boundaries, or before advancing the WAL checkpoint.
