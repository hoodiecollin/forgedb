Refresh the in-memory `row_count` from the on-disk file length (#84).

Reads `read_exact_at` the file positionally, bounded by `row_count`.  For
the Tier 3 multi-process writer path a *peer* process may have appended
values since this handle opened; `sync_from_disk` re-derives `row_count`
from the shared file so the peer's committed rows become readable (paired
with `Tombstones::sync_from_disk` and `VariableColumn::sync_from_disk`).
A no-op for the single-writer path, which maintains `row_count` in memory.
