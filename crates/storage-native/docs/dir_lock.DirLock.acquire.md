Take an exclusive advisory lock on `<root>/.forgedb.lock`, creating `root` and the lock file if they are missing.

Returns the held [`DirLock`] on success. The attempt does not block: if the lock is already held through any other handle, in this or another process, it fails with `ErrorKind::WouldBlock` and the message "another ForgeDB writer already has this data directory open", so a caller can report it and exit. Any other error comes from creating the directory or opening the file.
