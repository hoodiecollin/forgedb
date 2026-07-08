# Proposal: Backup & Restore

**Status:** FIRST MILESTONE IMPLEMENTED (2026-07-07). Product-gated; `forgedb-product-manager`
verdict: **aligned-with-constraints** (2026-07-06).
- **LANDED:** `forgedb-backup` class-1 substrate crate (lock-free full-snapshot create/restore
  over opaque files, driven entirely by each dir's `manifest.json` `row_anchor` + column layout;
  never reads `.forge`), `forgedb backup {create,restore,list}` CLI, the per-model layout manifest
  emitted from generated `*Storage::new()`, and the substrate `Manifest` additive fields
  (`compaction_epoch`/`format_version`/`row_anchor` + `ColumnMetadata { value_size, kind,
  relative_path }`). Concurrency proof (`crates/backup/tests/roundtrip.rs`) + E2E CLI proof
  (generated write → create → restore → reopen) both green. Anchor = tombstone/right-column length
  (from #65), so backup captures a crash-consistent committed prefix with no lock/MVCC/WAL.
- **DEFERRED (out of milestone, unchanged):** incremental backups (the `compaction_epoch` field is
  captured in the archive but **compaction does not yet bump it** and no chain logic exists),
  WAL-replay PITR, cloud `BackupTarget` impls, compression/encryption.
  - **Trap to fix when incrementals land:** generated `write_manifest` hardcodes
    `compaction_epoch: 0` and runs on every open, so once compaction bumps the epoch a reopen
    would reset it to 0 and break the chain. Fix = have the generated writer *preserve* an
    existing manifest's `compaction_epoch` (documented at `generate_write_manifest` in
    `crates/codegen/src/rust.rs`). Harmless today (nothing bumps it).
- **PUBLISH:** requires `forgedb-storage 0.1.3` on crates.io before a fresh `init` builds against
  the registry (see CLAUDE.md "publish gap REOPENED").
**Issue:** [#57](https://github.com/hoodiecollin/forgedb/issues/57) (`idea`, `plan-next`)
**Date:** 2026-07-06 (design) · 2026-07-07 (milestone)

## Summary

Add **backup & restore as `forgedb` CLI ops tooling** that operates on a database *directory*
as structured-but-opaque storage — a peer to the existing `compact`/`migrate` surface. The
bulk is CLI work; a thin **`forgedb-backup` substrate crate** (class-1) understands the
on-disk *layout* (fixed/variable/tombstones/manifest), never the `.forge` *schema*.

The design rests on one load-bearing observation: **the storage engine is append-only under
an atomically-rewritten manifest anchor.** Data files (`fixed/*.bin`, `variable/*_data.bin` +
`*_offsets.bin`, `tombstones.bin`) only ever grow by appending; `manifest.json` (with
`row_count`) is the single commit anchor, written via temp-file + `fsync` + atomic `rename`
(`crates/storage/src/lib.rs:743-762`). Therefore a **consistent snapshot = read the manifest
anchor, then copy each column/tombstone file's *prefix* truncated to the length implied by
`row_count`.** Concurrent writers only append *past* that prefix, so the copied bytes never
change under you. This gives **lock-free hot backup with no MVCC and no WAL** — the
append-only model *is* the snapshot isolation.

## Product verdict & invariant mapping

**Aligned-with-constraints.** Mostly schema-agnostic byte/file work — but real traps make it
"with constraints" rather than a clean green.

Invariant check (schema is a *compile-time input to generation*, never a *runtime input to a
generic engine*):

| Concern | Verdict |
|---|---|
| Does backup make the `.forge` schema a runtime input? | **No** — reads `manifest.json` (storage substrate), the same layout knowledge `forgedb-compaction` already has. Never parses `.forge`. |
| Is the published artifact a generic runtime reconstructing schema logic? | **No** — `forgedb-backup` moves opaque file prefixes and WAL bytes; it knows "there are fixed columns and a `row_count`," not any model's fields. |
| Does it add to "schema + CLI + config" authoring? | **No** — an ops command on an existing DB dir, like `compact`. |
| Does it add runtime deps to the *generated app*? | **Must not.** Cloud clients, schedulers, archive codecs are **CLI-side only.** This is the primary constraint. |

The traps (why "with constraints"):
1. **"Cloud targets" is the drift word** — the pull is toward an SDK the generated app imports
   to stream itself to S3. That crosses the line. Cloud targets stay CLI-side, over opaque archives.
2. **"Point-in-time recovery" over-promises** — WAL entries carry no timestamp/LSN, so true
   wall-clock PITR isn't backed by the current substrate.
3. **Compaction rewrites files** — quietly breaking the byte-watermark model that makes
   incrementals cheap. Must be designed for, not discovered.

## Where it lives

| Stratum | What it is | Guard class |
|---|---|---|
| **A. `forgedb backup {create,restore,list,verify}`** — CLI over a data dir | Orchestration, target selection, credentials, archive I/O | CLI tooling (not shipped into apps) |
| **B. `forgedb-backup`** — snapshot/restore engine over opaque files | Reads `manifest.json` + copies file prefixes + bundles WAL bytes. Peer to `forgedb-compaction`. | Class-1 *substrate* |
| **C. Generated `database.rs`** | **Untouched.** Zero backup logic. | Generated code (the product) |

**Explicitly NOT** generated methods on `Database`. A generated app does not back itself up;
the operator (or an external cron/systemd timer) runs the CLI. `db.backup()` in `database.rs`
is runtime-library creep and is rejected. Cloud targets are a **CLI-side trait**
(`BackupTarget { put(archive), get(id), list() }`) with a local-filesystem impl and, later, an
object-store impl — the generated binary never links any of it.

`forgedb-backup` mirrors `forgedb-compaction`, which already proves the pattern:
`MaintenanceApi::new(&data_dir, config)` (`src/commands/compact.rs:40`) operates on the
directory as structured-but-opaque storage without touching the schema.

## The shape of each sub-feature

### Hot backup (consistency while writes happen)

No MVCC and no WAL needed:

1. Read `manifest.json` (atomic rename ⇒ always a coherent `row_count = N`, never torn).
2. For each column, compute the **exact byte length for N rows**:
   - Fixed: `N * value_size`.
   - Variable: read the `N`-th `(offset, length)` pair from the offsets file (16 B/row,
     `lib.rs:506`) ⇒ data length = `offset + length`; offsets length = `N * 16`.
   - Tombstones: `N` bytes (`lib.rs:606`, 1 B/row).
3. Copy `[0, computed_len)` of each file into the archive.

Concurrent appends land *beyond* `computed_len` and are excluded; a half-finished multi-column
insert (columns appended but `row_count` not yet bumped) is invisible because the anchor is
`N`, not raw file size. **Result: lock-free, crash-consistent full backup.** The only writer
that violates append-only is **compaction** (it rewrites files) — so backup and compaction
must not overlap (see risks).

**Risk to verify before build:** that the generated insert path bumps `row_count` and calls
`save_manifest()` *after* the column appends, making the manifest the true commit anchor. If
generated code batches manifest saves, the anchor lags and backup is consistent-but-stale
(acceptable, but must be documented). Confirm in `crates/codegen/src/rust.rs`.

### Point-in-time recovery (what "point" means here)

Honest framing: **there is no wall-clock PITR today.** `WalEntry` has no timestamp and no LSN
(`crates/wal/src/entry.rs:499` — only `model_name` + `operation`), only a monotonic
`TransactionId: u64` (`entry.rs:309`). So the recovery point is either:

- **A snapshot boundary** (the manifest anchor `N`) — restore-to-snapshot, no replay. **Milestone target.**
- **A committed `TransactionId`** — snapshot + forward replay of `WalManager::replay_committed`
  (`crates/wal/src/lib.rs:124`) up to a chosen txn id. Later increment; requires backup to also
  capture the `wal.log` tail as opaque bytes.

True time-based PITR ("restore to 14:03") requires adding a timestamp to `WalEntry` — a
`forgedb-wal` change, out of scope here and named as the enabling dependency. Do **not** fake
it; document the recovery point as *transaction-granular*, not time-granular.

Also: `open_with_wal` does **not** auto-replay (`lib.rs:684-691` — replay is the caller's job).
So a restored dir + `wal.log` won't roll forward unless generated code drives replay. Milestone
restore targets the checkpointed snapshot state and drops/archives the WAL.

### Incremental backups (delta unit)

The delta unit falls out of append-only: **per-file byte range `[prev_watermark,
new_watermark)`**, where a watermark is the vector of `computed_len` values from the last
backup. Bytes below `prev_watermark` are immutable, so an incremental ships only the appended
tails + the new manifest.

**The trap — compaction invalidates watermarks.** `compact` rewrites fixed/variable files to
drop tombstoned rows, shifting every offset. After compaction, "byte K" no longer means the
same row, so a watermark-based incremental chain is silently corrupt across a compaction.
**Fix: a `compaction_epoch` (generation counter) in the `Manifest`, bumped on every
compaction.** An incremental is valid only within the same epoch as its base; crossing an
epoch forces a fresh full backup. Land the manifest field **now** (in the milestone) even
though incrementals ship later, so the anchor is future-proof and old backups are
self-describing.

### Cloud targets (does this cross a line?)

Only if done wrong. **Green:** the `forgedb` CLI gains an object-store backend behind the
`BackupTarget` trait — the CLI process (an ops tool) pushes/pulls opaque archive blobs;
credentials come from the operator's environment, never baked into generated code. Analogous
to the CLI already writing files to disk. **Red:** a `forgedb-backup`-runtime crate the
*generated app* imports to stream itself to S3 on a timer. The generated binary stays unaware
that backups exist.

## Red lines (reject on sight)

- An **always-on backup/replication daemon shipped as a runtime library the generated app
  links.** Backup is an external ops action (CLI + scheduler), not an app feature compiled into
  `database.rs`.
- A **cloud-sync SDK the generated app imports** (app streams itself to a bucket). Cloud
  targets live in the CLI, over opaque archives.
- **Backup or restore reading the `.forge` schema.** Reading `manifest.json` is substrate
  (allowed, like compaction); parsing the schema to decide what/how to back up is the
  generic-engine line.
- **"Restore into a different/evolved schema" (migrate-on-restore) driven by a runtime schema
  interpreter.** Restore is byte-level file materialization; schema evolution stays `migrate`'s
  job.
- **A restore that instantiates a generic query/data engine to "logically replay" rows.**
  Restore materializes files and (later) applies WAL ops opaquely; it does not rebuild data
  through a schema-driven writer.

## Open questions / risks the design must resolve

1. **Compaction ↔ incremental interaction (the #1 risk).** Land `compaction_epoch` in
   `Manifest`; define chain semantics (incremental valid only within-epoch; epoch change ⇒
   forced full). Recommend: read the epoch from the manifest at start, re-read at end; if it
   changed, the copy is invalid → retry or fail loudly.
2. **Is `manifest.row_count` truly the single commit anchor for a multi-column row?** Confirm
   generated insert ordering (columns appended, then `row_count` bumped, then `save_manifest`).
   If manifest saves are batched, backups are consistent but may lag the newest committed rows
   — acceptable if documented; verify in codegen.
3. **Does hot backup even need the WAL?** For a crash-consistent *snapshot*, **no**
   (append-only + manifest anchor suffices). The WAL is needed only for forward replay past the
   anchor. Decide whether `backup create` also captures the `wal.log` tail (opaque) so a future
   restore can replay committed txns. Recommend capturing it (cheap, opaque bytes) but not
   *replaying* it in the milestone.
4. **Restore atomicity & safety.** Restore into a fresh temp dir, then atomic `rename` into
   place; never partially overwrite a live dir. Define overwrite-vs-refuse policy and
   interaction with a running app holding the dir.
5. **Archive format versioning.** The archive must carry a format version + the source
   `schema_version`/`compaction_epoch` so a restore can refuse mismatches instead of silently
   materializing incompatible bytes. Compression/encryption deferred; the container is versioned
   from day one.
6. **PITR enablement.** Transaction-granular replay needs the WAL tail captured; time-granular
   PITR needs a `WalEntry` timestamp (a `forgedb-wal` change) — explicitly out of this note,
   named so it isn't forgotten.

## First milestone (smallest slice that proves the model *and* the guard)

**In scope**
- `forgedb-backup` substrate crate: given a data dir, compute the consistent length-vector from
  `manifest.json`, and pack/unpack a **full snapshot archive** of file prefixes + manifest.
  Reads the manifest only; **zero schema awareness.**
- `forgedb backup create <data_dir> --output <archive>` — lock-free full snapshot, local
  filesystem target only.
- `forgedb backup restore <archive> --output <data_dir>` — materialize atomically (temp +
  rename) to the snapshot anchor; **no WAL replay.**
- Add `compaction_epoch` to `Manifest` (additive, `#[serde(default)]` for on-disk
  back-compat), stamped into the archive — foundation for later incrementals even though
  incrementals don't ship here.
- **Round-trip + concurrency proof (also the identity proof):** `init` a project, insert rows
  via *generated* code, run `backup create` **while a second writer is appending**, restore to
  a new dir, open with generated code, verify the restored rows are exactly a clean row-boundary
  prefix (no torn row, no mid-insert bleed). Confirm no artifact reads a `.forge` schema.

**Explicitly out**
- **Incremental backups** (needs the epoch-chain semantics finalized — but `compaction_epoch`
  lands now).
- **PITR / WAL forward replay** (WAL entries lack timestamps; transaction-granular replay is a
  later increment, time-granular needs a `forgedb-wal` change).
- **Cloud targets** — design the `BackupTarget` trait so object-store is a drop-in later, but
  ship **local-fs only.**
- **Compression / encryption** of the archive (container is versioned so these are additive).
- **Any generated-code change**; any `backup()` on `Database`. Zero runtime footprint in the app.
- **Backup concurrent with compaction** — required not to overlap; epoch-change mid-copy ⇒
  retry/fail.

**Success = a snapshot taken mid-writes restores to a clean, crash-consistent row boundary via
the CLI, with the backup engine having read only the manifest and opaque file bytes — never the
schema.** Needing schema knowledge to produce a consistent backup, or any backup code compiled
into the generated app, is the drift signal.

## Load-bearing references

- `crates/storage/src/lib.rs` — append-only column/tombstone files (`FixedColumn`/
  `VariableColumn`/`Tombstones` at `:213-648`); `manifest.json` atomic write (`:743-762`) = the
  commit anchor; `Manifest.row_count`/`last_checkpoint` (`:172-180`); variable-column 16 B/row
  offsets (`:506`) needed to bound the data file; `open_with_wal` does **not** auto-replay
  (`:684-691`).
- `crates/wal/src/entry.rs:499` — `WalEntry` has no timestamp/LSN, only `model_name` +
  `operation`; `TransactionId = u64` at `:309`. Bounds PITR to transaction-granular.
- `crates/wal/src/lib.rs:124` — `replay_committed` (the forward-replay primitive a later PITR
  increment builds on).
- `src/commands/compact.rs:40` — `MaintenanceApi::new(&data_dir, config)`: the existing
  precedent of a substrate crate operating on a DB dir as opaque storage; `forgedb-backup`
  mirrors it and must coordinate epochs with it.
- `CLAUDE.md` → "What ForgeDB is" — the invariant, plus the append-only / no-in-place-delete /
  tombstone model this note relies on.

## Scheduling note

The `Manifest` change (`compaction_epoch`) is a real additive edit to
`crates/storage/src/lib.rs:172` and wants `#[serde(default)]` to stay backward-compatible with
existing on-disk manifests — flag to `rust-core-library` when scheduled. `storage` is published
(0.1.2); this additive field is a minor bump, not a break.
