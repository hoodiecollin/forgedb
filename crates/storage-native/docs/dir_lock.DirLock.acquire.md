Try to acquire an exclusive advisory lock on `<root>/.forgedb.lock`,
creating the file (and any missing parent directories) if needed.

Returns `Ok(DirLock)` on success.  If another process already holds the
lock, returns `Err` with `kind() == io::ErrorKind::WouldBlock` so the
caller can print a clear "another writer already has this data dir open"
message and exit.

# Errors

- `WouldBlock` — another process already holds the exclusive lock.
- Any other `io::Error` from creating the directory or opening the file.
