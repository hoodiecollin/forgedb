Fsync policy determines when a native WAL is flushed to disk.

Defined at the crate root (not in the native-only `writer` module) because it
is part of the schema-agnostic substrate surface `forgedb-storage` re-exports
and generated code names (`FsyncPolicy::Always`) — it must exist on **both**
the native and the `wasm32` follower target. On `wasm32` there is no file WAL
(see the in-memory [`WalManager`] below), so the policy is inert there.
