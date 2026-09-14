Truncate the column to exactly `rows` rows, discarding everything after them.

This is the crash-recovery primitive: after replaying the WAL, generated code truncates every column to the last consistent row count, which also removes any partial trailing value a torn write left, so the next append lands at exactly row `rows`.

Returns `InvalidInput` if `rows > len()`; truncation cannot extend a column. Other errors come from `set_len`.
