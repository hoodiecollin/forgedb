The coordinator server: a Unix-socket listener that serializes commit turns across writer processes.

## What the server owns

- The data directory's single-writer lock: an exclusive advisory `fs2` lock on `<root>/`[`server::DIR_LOCK_FILENAME`], the same file a standalone generated writer locks when it opens the directory itself. Holding it refuses a second coordinator and excludes a standalone writer, so the two modes cannot run on one directory at once. Coordinated clients open the directory without the lock; their mutual exclusion is the serialized turn. This is a filesystem contract on a fixed filename, not a code dependency: the crate links no `forgedb-storage` crate and never opens a column file.
- A [`forgedb_txn::CommitSequencer`], seeded from the replication log's watermark so LSNs keep increasing across coordinator restarts.
- A [`forgedb_changefeed::durable::DurableBroker`] over `<root>/_coordinator_replication.log`, the durable cross-process log the committed row bytes are appended to.
- The single pending-turn slot: at most one `Grant` is outstanding at a time, and one not committed within [`server::CoordConfig::turn_timeout`] is reclaimed.

## Threading

[`server::Coordinator::run`] spawns one thread per connection. Turn state lives under one mutex with a condvar that waiting `RequestTurn` handlers block on; the broker lives under a separate mutex. A `Committed` handler takes the broker lock, releases the turn and wakes the waiters, and only then appends and fsyncs under the broker lock, so a commit's disk barrier never holds the next grant back, while holding the broker lock across the release keeps appends in commit order.

## What the server does not own

Column files, the per-model WAL, and any knowledge of records or schema stay in the writer process.
