Coordinator server — Unix-socket listener that serializes multi-process
commit turns for Tier 3 MVCC (#84).

## What the server owns (schema-agnostic control plane)

- The **#89 single-writer `DirLock`** on the data directory, held on behalf
  of all coordinated clients (Tier 3 mode-switch, spec T3-5): an exclusive
  advisory `fs2` lock on `<root>/.forgedb.lock` — the *exact same file*
  `forgedb_storage::DirLock::acquire` locks. Holding it means (a) a second
  coordinator is refused, and (b) a *standalone* writer (which self-acquires
  the `DirLock` in `open_at`) is mutually excluded — so "coordinated" and
  "standalone" modes can never both run. Coordinated clients therefore open
  **lock-free** (`_lock: None`); their write mutual-exclusion comes from the
  serialized turn-grant, not the file lock. This is pure filesystem interop
  on an opaque path — the coordinator gains NO `forgedb-storage*` dependency
  (T3-8): it never opens a column, it only advisory-locks a known path.
- A [`CommitSequencer`] seeded from the broker watermark, so LSNs continue
  monotonically across restarts.
- A [`DurableBroker`] for `_coordinator_replication.log` — the cross-process
  durable log that remote read-replica followers resume from.
- The pending-turn slot (at most one outstanding `Grant` at a time).

## What the server does NOT own (data plane — stays in the writer process)

- Column files (`FixedColumn`, `VariableColumn`, `Tombstones`).
- The per-model WAL.
- Any record field or schema knowledge.
