The data-directory single-writer lock filename.

**Load-bearing cross-crate contract:** this MUST byte-match the filename
`forgedb_storage::DirLock::acquire` locks (`storage-native/src/dir_lock.rs`,
`<root>/.forgedb.lock`). The coordinator advisory-locks the *same* file so a
standalone writer and a coordinator are mutually exclusive (spec T3-5). The
constant is duplicated rather than imported because the coordinator must not
depend on `forgedb-storage*` (T3-8) — a filesystem contract, the same class
of coupling as the on-disk column format the backup crate reads. A parity
test in each crate pins the string so a rename cannot silently desync.
