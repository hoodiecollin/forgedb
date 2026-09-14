The single-writer lock filename under a data directory (`<root>/.forgedb.lock`).

**Cross-crate contract:** `forgedb-coordinator` advisory-locks this same file
(by a duplicated constant, since it must not depend on `forgedb-storage*` —
MVCC spec T3-8) so a coordinator and a standalone writer are mutually
exclusive (T3-5). Renaming it here without updating
`forgedb_coordinator::server::DIR_LOCK_FILENAME` breaks that exclusion; a
parity test in each crate pins the string.
