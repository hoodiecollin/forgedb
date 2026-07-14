# Backup & Point-in-Time Recovery (v1)

ForgeDB's storage is append-only under a row-count anchor, so a backup is a
lock-free byte snapshot and **point-in-time recovery (PITR)** is "restore a base
+ replay a globally-ordered change log up to a chosen point." No MVCC, no
per-row versions — the same substrate that powers snapshots and the browser
read-replica.

The `forgedb` CLI is schema-blind; it snapshots/restores opaque bytes. Replay
during PITR needs the generated per-model decode/apply logic, so the **replay
step runs inside your generated app** (like `checkpoint`/`compact`), not the CLI.

## Backups

```bash
# Full lock-free snapshot of a data dir.
forgedb backup create --data-dir ./data --output ./backups/base.forgebk

# Incremental byte-tail delta against a prior base (cheap within one compaction epoch).
forgedb backup create --data-dir ./data --incremental \
  --base ./backups/base.forgebk --output ./backups/delta-1.forgebk

# Restore a full archive, or a base followed by its deltas in order (positional).
forgedb backup restore ./backups/base.forgebk --output ./restored
forgedb backup restore ./backups/base.forgebk ./backups/delta-1.forgebk \
  --output ./restored

# Inspect an archive header without materializing it.
forgedb backup list ./backups/base.forgebk --json
```

Every archive header records a **`base_offset`** — the durable replication broker
(`_replication.log`) end offset at snapshot time. It is the anchor PITR replays
from: the generated mutation records a change to the broker only **after** the
column commit, so every frame at or below `base_offset` is already durably
present in the archive's committed column bytes. The base is therefore a superset
of all changes through `base_offset`, and replay starts strictly after it.

## Point-in-time recovery

PITR = **base backup (at offset `O_base`) + replay `_replication.log` frames
`(O_base .. target]` through the generated `apply_frame`.** M1 is **offset-based**
("recover to broker offset N"), which is precise and durable.

### Prerequisites

1. A full base backup (`forgedb backup create`). Its header carries `O_base`
   (`forgedb backup list --json` → `base_offset`).
2. The **retained `_replication.log`** covering `(O_base .. target]`. The backup
   archive does **not** include it (it is a root-level file, not a model dir), so
   you must retain it separately — see *Retention contract* below.

### Recipe

```bash
# 1. Restore the base into a fresh directory.
forgedb backup restore ./backups/base.forgebk --output ./recovered

# 2. Place the retained broker log alongside the restored data (from the
#    still-running source, or an archived copy of _replication.log).
cp ./archived-logs/_replication.log ./recovered/_replication.log
```

```rust
// 3. In your GENERATED app, open the restored dir and replay to the target.
let mut db = Database::open_at("./recovered".into());
let o_base = /* the archive header's base_offset (forgedb backup list --json) */;
let target = /* the broker offset you want to recover to */;
let applied = db.recover_to(o_base, target)?;   // applies frames (o_base .. target]
assert_eq!(applied, target);                     // == O_base if the tail is empty
```

`recover_to(base_offset, target_offset)`:

- reads frames via `DurableBroker::read_from` in bounded batches;
- applies only frames with `base_offset < offset <= target_offset` through the
  **same** `apply_frame` the browser read-replica uses (dispatch by the opaque
  model tag — the ordering key is the broker offset, never a decoded field);
- **detaches the broker during replay** so replayed mutations do not re-record
  into the log it is reading, then re-attaches it;
- `commit()`s and returns the highest offset actually applied.

`recover_to(o_base, now)` reproduces the live state exactly. Recovery is
idempotent by id: even if a boundary frame were re-applied, the
superseding-version append means `get`/`all` still resolve one record per id, so
double-applying is read-safe.

## Retention contract

`_replication.log` must be **retained back to the oldest base you intend to
recover to.** Concretely:

- Do not let the broker's `prune_through` prune below the `base_offset` of any
  base you still rely on for recovery.
- The archive does not carry the log, so archive it (or the source dir's live
  log) alongside your bases. A base at `O_base` plus a log covering
  `(O_base .. target]` is the complete recovery input.
- A follower/recovery whose `base_offset` is below the log's
  `earliest_retained()` has fallen off the retained tail and must re-baseline
  from a newer full base.

## Honest limits (deferred)

- **Time-based recovery** ("recover to timestamp T") is **not** in M1. It needs a
  timestamp on `PersistedEvent`, a `forgedb-changefeed` substrate change. M1 is
  offset-based only.
- **Auto-prune** is not wired to a retention window; `prune_through` is manual.
- A DB opened via the standalone `Database::new()` path has no `_replication.log`
  and therefore nothing to replay — `recover_to` errors. PITR requires the
  `open_at` (broker-backed) path.
- Junction (M2M) link crash recovery inherits the existing #89 boundary; a torn
  broker commit can duplicate a junction pair (append-only; traversal is
  latest-wins / not deduped).
