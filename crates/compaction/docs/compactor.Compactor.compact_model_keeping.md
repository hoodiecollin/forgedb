Compacts a model keeping exactly the physical rows whose indices are in `keep`, dropping
every other row.

This is the reclaim primitive a generated database's `Database::compact()` calls, and the
only entry point generated code links. The caller decides which rows are live; this crate
reads no schema. Indices may be in any order and may repeat; an index at or beyond the row
count is ignored. The existing tombstone bitmap is not consulted: a tombstoned row named in
`keep` survives with its tombstone cleared.

Survivors keep their relative order, so a kept row's new index is its rank among the kept
indices in ascending order. The rewrite is staged: every column is written to a `.bin.tmp`
sibling first, then each temp file is renamed over its original, then `tombstones.bin` is
rewritten as one zero byte per surviving row, `row_count` in `manifest.json` is updated
when that file exists, and `.last_compaction` is written.

Returns a [`CompactionResult`] whose `bytes_before` and `bytes_after` are the model's
`total_disk_bytes` measured before and after the pass. Returns `Err` when the model
directory or its `tombstones.bin` is missing, when a variable column's offsets point past
its data file, or when any read, write or rename fails. Takes no lock: nothing else may
read or write the model directory during the call.
