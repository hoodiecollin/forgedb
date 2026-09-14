Return the subset of `rows` that are **not** tombstoned, preserving order
(#168 scan filter).  Reads the whole tombstone column **once** rather than
a per-row [`is_deleted`](Self::is_deleted) syscall — the bulk liveness
filter a column scan applies to its live-row selection.  A row index past
the committed count is treated as not-live (dropped), matching
`is_deleted(..).unwrap_or(true)`.
