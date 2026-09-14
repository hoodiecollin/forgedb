The columnar storage facade: a compile-time re-export of one backend crate, selected by target.

This crate owns no engine code. On host targets (`cfg(not(target_arch = "wasm32"))`) it
re-exports the whole public surface of [`forgedb-storage-native`](https://docs.rs/forgedb-storage-native),
the positional file-I/O columnar engine, and that crate's page is where every item below is
documented. On `wasm32` it re-exports `forgedb-storage-web` instead; that backend is not
documented here.

Generated code writes `use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};` and
calls the backend directly, so it stays byte-identical across targets. Exactly one backend is
linked per build, which is why the two backends can share their type names without colliding
and why the facade is a `cfg` re-export rather than a trait.
