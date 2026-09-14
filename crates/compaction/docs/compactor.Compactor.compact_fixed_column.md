Rewrites one fixed-width column, keeping the rows whose `tombstones` entry is `false`.

The row width is the file size divided by `tombstones.len()`, so the slice must hold
exactly one entry per stored row. An empty slice returns `Ok` without touching the file.
The column is written to a `.bin.tmp` sibling and renamed over the original immediately;
`tombstones.bin` and `manifest.json` are not updated, so the caller is responsible for
keeping the model consistent. Returns `Err` when any I/O fails.
