Force the drive's write cache to permanent media, issued on the data file.

On macOS this is `fcntl(F_FULLFSYNC)`; on other platforms it is `File::sync_all`. The flush is device-wide, so one call also covers the offsets file pushed by [`VariableColumn::sync_to_drive`].
