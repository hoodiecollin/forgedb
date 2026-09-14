Refresh the in-memory counters from the on-disk file lengths (#84).

Re-derives `row_count` from the offsets file (16 bytes/row) and
`current_data_offset` from the data file, so a *peer* process's appended
rows (Tier 3 multi-process writer path) become readable through this
handle.  Paired with `FixedColumn::sync_from_disk` /
`Tombstones::sync_from_disk`.  A no-op for the single-writer path.
