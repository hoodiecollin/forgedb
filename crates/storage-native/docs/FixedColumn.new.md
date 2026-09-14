Open or create the column file at `path` with `value_size` bytes per row.

Creates any missing parent directories and opens the file read-write without truncating it; the row count is the file length divided by `value_size`. A trailing partial value left by a torn write is excluded from the count but not removed, so a caller recovering after a crash should call [`FixedColumn::truncate_to_rows`] with the consistent row count before appending again.

Returns `InvalidInput` if `value_size` is `0`.
