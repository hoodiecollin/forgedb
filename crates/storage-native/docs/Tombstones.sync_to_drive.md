Push the file's dirty pages to the drive without forcing a device-cache barrier.

On macOS this is `fsync(2)`; on other platforms it is `File::sync_data`. Pair it with one [`Tombstones::barrier`] per checkpoint.
