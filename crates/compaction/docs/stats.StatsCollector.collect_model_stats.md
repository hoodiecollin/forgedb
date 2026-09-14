Measures one model, addressed by its directory name under the data directory.

`total_rows` is the length of `tombstones.bin`, `deleted_rows` the count of nonzero bytes
in it, and `columns` lists the fixed columns first, then the variable ones. `index_sizes`
is always empty: the collector does not measure indexes. `last_compaction` is read from
the model's `.last_compaction` marker when present. Returns `Err` when the model directory
or its `tombstones.bin` is missing, or when a column file cannot be read.
