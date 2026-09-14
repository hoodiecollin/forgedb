Force the drive's write cache to permanent media, making durable every file previously pushed with `sync_to_drive` on the same device.

On macOS this is `fcntl(F_FULLFSYNC)`; on other platforms it is `File::sync_all`. One call per checkpoint suffices.
