Re-derive the row count from the file's current length.

The count is otherwise maintained in memory by this handle's own appends and truncations, so rows another process appended to the same file are invisible until this is called. It reads only the file metadata.
