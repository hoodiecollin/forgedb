Open the WAL at `path`, creating the file and any missing parent directories.

An existing file is kept and appended to, never truncated. `fsync_policy`
governs [`Self::write`]. Fails with the underlying `io::Error` if the directory
cannot be created or the file cannot be opened.
