Truncate the column to hold exactly `rows` rows, discarding everything
after that point.

This is the crash-recovery primitive: after replaying a WAL, generated
code calls `truncate_to_rows` on every column to realign them back to the
last consistent row watermark before resuming appends.  Any partial
trailing value left by a torn write is physically removed so the next
`append_*` lands at exactly row `rows`.

# Errors

Returns `Err(InvalidInput)` if `rows > self.len()` — truncation cannot
extend a column.  All other errors come from the underlying `set_len`
syscall.
