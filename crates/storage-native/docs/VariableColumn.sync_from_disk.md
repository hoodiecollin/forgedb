Re-derive the row count from the offsets file's length (16 bytes per row) and the append position from the data file's length.

Both are otherwise maintained in memory by this handle's own appends and truncations, so rows another process appended are invisible until this is called. It reads only file metadata.
