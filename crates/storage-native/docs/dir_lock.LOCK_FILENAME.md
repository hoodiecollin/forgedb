Name of the single-writer lock file inside a data directory: `<root>/.forgedb.lock`.

This is a cross-crate filesystem contract. `forgedb-coordinator` advisory-locks the same filename through its own duplicated constant (it must not depend on the storage crates), so a coordinator and a standalone writer are mutually exclusive on one directory. A parity test in each crate pins the string; renaming it here without the coordinator breaks that exclusion.
