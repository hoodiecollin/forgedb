Filename of the data-directory single-writer lock, `.forgedb.lock`, joined onto the root the coordinator serves.

It must byte-match the filename the `forgedb-storage-native` `DirLock` locks, because the coordinator advisory-locks the same file to exclude a standalone writer. The string is duplicated rather than imported because this crate deliberately has no `forgedb-storage` dependency; a test in this crate pins the value.
