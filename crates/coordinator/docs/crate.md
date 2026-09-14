MVCC Tier 3 coordinator for ForgeDB (#84).

Schema-agnostic control plane: owns the conflict map (via `forgedb-txn`),
the LSN sequence, the opaque `_replication.log` (via `forgedb-changefeed`),
and grants serialized exclusive commit turns over a Unix domain socket.

## Architecture

One `forgedb coordinate <root>` process per data directory.  Generated
writers connect over a Unix socket and follow a three-message turn protocol:

1. **`RequestTurn`** — client sends its write-set keys + read-snapshot LSN.
   The coordinator does a conflict check (`CommitSequencer::try_commit`), then
   either **`Grant { turn_id, reserved_lsn }`** (exclusive turn) or
   **`Nack { conflict_key }`** (retry required).

2. *Data-plane write* (by the client, after Grant) — the client writes to
   the shared column files + WAL on its own, then sends `Committed`.

3. **`Committed`** — client announces durability, hands opaque row bytes to
   the coordinator, which appends them to `_replication.log` and replies with
   **`Ack { lsn }`**.  The turn is released; the next queued client may proceed.

## Identity red line

The coordinator NEVER writes a column, NEVER decodes `opaque_row_bytes`, and
NEVER interprets a model name as anything other than an opaque routing tag.
Every `unsafe` block is forbidden by clippy (`#![forbid(unsafe_code)]`).
