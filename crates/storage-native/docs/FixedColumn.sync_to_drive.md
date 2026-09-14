Push the column's dirty pages to the drive without forcing a device-cache barrier.

On macOS this is `fsync(2)`, which hands the data to the drive's cache; on other platforms it is `File::sync_data`. Pair it with a single [`FixedColumn::barrier`] on any file of the same device, so a checkpoint touching many files pays one barrier instead of one per file.
