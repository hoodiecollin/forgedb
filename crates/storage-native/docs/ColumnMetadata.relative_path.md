Column file path relative to the model directory (e.g.
`fixed/u32_0.bin` or `variable/string_data_1.bin`). For a variable
column this is the data file; the offsets file is the same path with
`_data.bin` → `_offsets.bin` (the storage-layout convention `stats.rs`
already relies on).
