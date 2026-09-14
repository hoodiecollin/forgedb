Open or create the column's data file at `data_path` and offsets file at `offsets_path`.

Creates any missing parent directories of `data_path` and opens both files read-write without truncating them. The row count is the offsets file's length divided by 16 and the append position is the data file's length. A partial trailing offsets entry is excluded from the count but not removed, so a caller recovering after a crash should call [`VariableColumn::truncate_to_rows`] with the consistent row count before appending again.
