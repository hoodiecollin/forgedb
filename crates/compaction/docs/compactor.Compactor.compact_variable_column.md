Rewrites one variable-length column, keeping the rows whose `tombstones` entry is `false`.

`data_path` is the `<column>_data.bin` file and `offset_path` its `<column>_offsets.bin`.
`tombstones` must hold one entry per row, `true` marking a row to drop; a row with no entry
is dropped, so an empty slice empties the column. Both files are written to `.bin.tmp`
siblings and renamed over the originals immediately. Unlike a model-level pass, nothing
else is staged and neither `tombstones.bin` nor `manifest.json` is touched, so the caller
is responsible for keeping the model consistent. Returns `Err` when an offset points past
the data file or any I/O fails.
