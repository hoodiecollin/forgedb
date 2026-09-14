Push both files' dirty pages to the drive without forcing a device-cache barrier, data file first.

On macOS this is `fsync(2)`; on other platforms it is `File::sync_data`. Pair it with one [`VariableColumn::barrier`] per checkpoint.
