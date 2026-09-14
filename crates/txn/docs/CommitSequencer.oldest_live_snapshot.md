The oldest LSN for which at least one read snapshot is still open.

Returns `Lsn(0)` when no snapshots are live.  Used by the compaction
keep-set guard: any row version visible as of this LSN must not be GC'd.
