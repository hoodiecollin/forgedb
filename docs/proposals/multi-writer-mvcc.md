# Proposal: Multi-Writer Concurrency & Transactions (MVCC Direction C)

**Status:** DESIGN NOTE — `forgedb-product-manager` verdict **PASS-WITH-CONSTRAINTS** (2026-07-14;
3 binding constraints, folded in below — see "PM gate constraints"). Tier 1 is a clean PASS with
zero new artifacts; Tier 2's `forgedb-txn` is PASS conditioned on enforced schema-agnosticism +
caller-gating. Fleshes out the **Direction C** section held "on demand only" in
[`mvcc-concurrency.md`](./mvcc-concurrency.md) (Directions A + B LANDED). This note is the design
for the deferred rock: **transactions + concurrent writers**.
**Issue:** [#75](https://github.com/hoodiecollin/forgedb/issues/75) (MVCC Direction C)
**Date:** 2026-07-14

## Summary

The headline finding, which reframes the whole feature: **ForgeDB already has ~90% of an MVCC
engine — it just runs single-writer.** Append-only columns are immutable versions; the
superseding-version append (#66) already writes a *new* version per update/delete; the
watermark `Snapshot` (#56-A) is already a visibility cutoff. What is missing is not a storage
engine — it is a **transaction boundary** (atomic multi-row commit + rollback) and, above that, a
**commit-coordination discipline** so more than one writer can prepare work at once.

This yields a staged design in three tiers, smallest → largest, and a load-bearing insight that
**the first two tiers need essentially no storage-substrate format change** (hence a small-to-zero
publish gap), because *serialized commit keeps the row-count watermark valid for read visibility*
and *conflict detection is over in-flight transactions only* (in-memory, ephemeral):

- **Tier 1 — Transactions (single writer, atomic commit/rollback).** `db.transaction(|tx| … )`:
  a set of appends made visible atomically at commit; rollback = **truncate columns back to their
  pre-transaction length** (reusing the already-published `truncate_to_rows`). No per-row version
  metadata, no format break. This is what most "I want transactions" demand actually needs.
- **Tier 2 — Optimistic concurrent writers (serialized commit).** Many transactions *prepare*
  concurrently (read a snapshot, validate, stage into private buffers); a **serialized commit
  point** assigns a monotonic **commit LSN** and detects write-write conflicts against an in-memory
  id→last-committer map; losers roll back (Tier-1 truncate) and retry. Still no on-disk format
  break — commit remains serialized, so appends stay contiguous and the watermark stays valid.
- **Tier 3 — Multi-process writers (held).** Removes single-process by giving one process the
  commit point (a **write coordinator**, the symmetric inverse of #82's durable broker). This is
  the only tier that breaks single-process scope and warrants a format/protocol commitment — held
  until real multi-process demand.

**Recommendation:** design + build **Tier 1** (transactions) as the first milestone; it is
independently valuable, append-only-preserving, and low identity-risk. **Tier 2** on demonstrated
concurrent-writer demand. **Tier 3 held.** The honest ceiling that bounds Tiers 1–2 —
**one physical append point per column** → *concurrent prepare, serialized commit* — is named
loudly below; true parallel append needs segmented columns (a separate, larger storage effort).

## Where the engine already is (current-state anchor)

From the storage/concurrency map (`crates/storage-native/src/lib.rs`, `crates/codegen/src/rust.rs`):

| MVCC ingredient | Already present as | Location |
|---|---|---|
| Immutable versions | append-only fixed/variable/tombstone columns (no in-place mutation) | `storage-native/src/lib.rs` (FixedColumn 338–630, VariableColumn 640–798, Tombstones 806–893) |
| New-version-per-mutation | superseding-version append (#66): `update` appends + repoints `id_to_row`; `delete` appends a tombstoned version | `rust.rs` update ~2992–3124, delete ~3126–3226 |
| Visibility cutoff | `Snapshot { watermark: usize }`, `visible(i) = i < watermark` | `storage-native/src/lib.rs` 305–327 |
| Per-id newest-version resolution | `get_at`/`all_at` scan `0..watermark`, last-write-wins per id | `rust.rs` get_at ~814–863 |
| Durable commit boundary | WAL `Raw` opaque path + fsync-before-columns + checkpoint (#89/#96) | `crates/wal`, `rust.rs` checkpoint ~2711–2750 |
| Rollback primitive | `truncate_to_rows(rows)` on every column type (added for #89 recovery) | `storage-native/src/lib.rs` (per column) |
| Monotonic commit ordering token | `DurableBroker` global `u64` offset (#82) | `crates/changefeed` `durable` |
| Single-writer enforcement | `DirLock` advisory exclusive lock on `open_at` | `storage-native/src/dir_lock.rs` 22–114 |

The gap is precisely the two things append-only-plus-single-writer never needed: **a transaction
that groups appends atomically**, and **coordination for >1 preparer**. Everything else is reuse.

## The load-bearing insight: why Tiers 1–2 don't break the on-disk format

Two facts, together, keep the cheap watermark model valid even with transactions and optimistic
concurrency:

1. **Serialized commit keeps the row-count watermark a valid visibility cutoff.** A transaction
   stages rows, then at commit appends them *contiguously* under a commit lock and advances the
   watermark in one step (5 → 8 for a 3-row txn). Because commit is serialized, no two
   transactions' appends interleave, so "row index < watermark" still exactly means "committed
   before this snapshot." No per-row `xmin`/`xmax` is required for **read visibility**.
2. **Conflict detection is over in-flight transactions only → in-memory, ephemeral.** Write-write
   conflict = "did another transaction commit a write to id X between my read-snapshot and my
   commit?" That question only concerns transactions currently in flight; after a crash/restart
   there are no in-flight transactions, so the conflict state does not need to survive reopen and
   therefore needs **no on-disk commit-version column**. The map is rebuilt empty on open.

**Consequence:** Tier 1 needs *zero* storage-substrate change (rollback = existing
`truncate_to_rows`, commit = existing watermark advance + WAL fsync). Tier 2 adds only an in-memory
commit sequencer + conflict map — still no format break, so **no storage publish gap** (like
column-projection #113, the mechanism is "generate an orchestration over primitives the substrate
already ships"). This is the single biggest reason to prefer this staging over a textbook
`xmin`/`xmax` engine.

## Tier 1 — Transactions (first milestone)

### API (generated, per-schema)

```rust
// Generated on the tailored Database. The closure gets a tailored TxHandle exposing
// exactly the write surface the schema already generates (create_/update_/delete_/link_),
// scoped to the transaction; reads inside see the txn's own uncommitted writes + the
// pre-txn snapshot.
let new_id = db.transaction(|tx| {
    let uid = tx.create_user(User { … })?;      // staged, not yet visible outside
    tx.create_post(Post { author: uid, … })?;   // atomic with the user
    Ok(uid)                                       // Ok → commit (advance watermark, fsync)
})?;                                              // Err / panic → rollback (truncate to pre-txn len)
```

- **`db.transaction` is generated code on the tailored `Database`** — not a shipped generic
  `db.begin()` over an arbitrary schema. `TxHandle` exposes the same per-model methods codegen
  already emits (`create_user`, `update_post`, `link_post_tag`, …), scoped to the transaction. No
  predicate-as-data, no runtime schema. Identity-clean by construction (same category as the
  existing generated mutation surface).

### Mechanics

- **Begin:** record each collection's current row-count as the transaction's rollback marks
  (`{col → len}`). Capture a read snapshot (current watermark) for the txn's reads.
- **Stage writes:** the txn's `create_/update_/delete_` append to columns + WAL exactly as today,
  **but do not advance the public watermark** and **do not emit changefeed/broker events yet**
  (those fire on commit, so subscribers never see rolled-back work). `id_to_row` / index updates
  are buffered in the txn and applied on commit.
- **Reads inside the txn** resolve against `max(pre-txn snapshot, this txn's staged rows)` — the
  txn sees its own writes (read-your-writes) plus the committed prefix.
- **Commit:** fsync the WAL (durability boundary already exists), **advance the public watermark**
  to the new row-count for every touched collection in one step, apply buffered `id_to_row`/index
  updates, then emit the batched changefeed/broker events. The watermark advance *is* the atomic
  visibility flip.
- **Rollback (Err/panic/drop-without-commit):** `truncate_to_rows(mark)` every touched column back
  to its pre-txn length; drop the WAL tail past the mark; discard buffered index updates. Because
  columns are append-only and nothing external saw the rows, this is a clean erase.

### What Tier 1 delivers / forecloses

- **Delivers:** atomic multi-row / multi-model writes, rollback on error, all-or-nothing FK
  integrity (create parent + child atomically), and a clean batching point for changefeed/broker
  (one event burst per commit). Snapshot isolation across the transaction falls out of the existing
  watermark for free.
- **Forecloses nothing in Tier 2/3** — it is a strict subset. Concurrent *writers* are still
  serialized (Tier 1 is single-writer with a transaction boundary); that is the next tier.
- **Substrate cost:** none (reuses `truncate_to_rows` + watermark + WAL). **Publish gap:** none.

## Tier 2 — Optimistic concurrent writers (serialized commit)

Adds genuine *concurrent transaction preparation* with a *serialized commit point*:

- **Prepare concurrently:** N transactions each capture a read snapshot, validate, and stage into
  **private buffers** (not the shared columns yet) — validation/reads run in parallel.
- **Commit serially:** a commit mutex (or, for Tier 3, a coordinator) hands out a monotonic
  **commit LSN** (reuse #82's `DurableBroker` offset), runs **conflict detection**, then does the
  Tier-1 contiguous append + watermark advance.
- **Conflict detection (first-committer-wins):** each txn records the set of ids it *read for
  update* / wrote. At commit, if any of those ids has a `last-committer-LSN > my read-snapshot
  LSN`, abort → the caller retries. The `id → last-committer-LSN` map is **in-memory, ephemeral**
  (see the load-bearing insight) and — critically — **keyed on opaque bytes the generated code
  computes**, never on a schema-typed id the substrate interprets (same discipline as #82's
  field-blind row bytes and #92's opaque keep-set).
- **Isolation level:** snapshot isolation (optimistic). Serializable is a later knob (add
  read-set tracking); not first-cut.

**Substrate cost:** an in-memory commit sequencer + conflict map. If it lives in a new class-1
crate `forgedb-txn` (rows / opaque keys / LSNs only — *the moment it knows a model name the
boundary is violated*), that crate publishes once; storage-native/web are **untouched** → still no
storage format break. **Publish gap:** at most one small new substrate crate, no format bump.

**Honest ceiling (the physics):** because each column has **one physical append point**, commit is
serialized even in Tier 2 — "concurrent writers" = concurrent *prepare*, serialized *commit*
(group-commit-style). This is a real and common MVCC shape, but it is not parallel append. Parallel
append needs **segmented columns** (each writer owns a segment file, merged on read) — a distinct,
larger storage redesign explicitly **out of this note**. Name this in every external description so
"multi-writer" is never oversold.

## Tier 3 — Multi-process writers (held)

Removes single-process. One process owns the commit point (holds `DirLock`, the append cursor, and
the LSN sequencer) and acts as a **write coordinator**; other processes submit prepared
transactions to it. This is the exact symmetric inverse of #82's durable broker (which broadcasts
committed changes *out*; the coordinator accepts proposed changes *in*, serializes, appends,
assigns LSNs) and of #110's replica follower. It is the only tier that:

- breaks the blessed **single-process scope**,
- needs a wire protocol + a durable in-flight/2-phase record (so a coordinator crash mid-commit is
  recoverable),
- and plausibly a persisted commit-LSN (a format touch).

**Held** until multi-process concurrent-write demand is demonstrated. Reachable by extending Tier 2
(the coordinator replaces the in-process commit mutex). Recorded here so the staging is explicit,
not designed in detail.

## Identity verdict & the substrate/generated split

Mapping to the guard (`CLAUDE.md` → "What ForgeDB is"; carries forward
`mvcc-concurrency.md`'s Direction-C red lines):

| Piece | Class | Where it lives |
|---|---|---|
| `db.transaction(\|tx\| …)` closure + `TxHandle` per-model methods | **Generated** (tailored) | `rust.rs` |
| Rollback-by-truncate orchestration; which columns to truncate/append | **Generated** | `rust.rs` |
| Version/visibility resolution per id (LSN or watermark compare) | **Generated** | `rust.rs` (extends `get_at`) |
| Conflict-key computation (id → opaque bytes) | **Generated** | `rust.rs` |
| `truncate_to_rows`, watermark, WAL fsync | **Substrate (published)** — reused as-is | `storage-native`, `wal` |
| Commit LSN sequencer + opaque-keyed conflict map (Tier 2) | **Substrate class-1** — new `forgedb-txn` (rows/keys/LSNs only) | new crate |
| Visibility predicate "is version V visible to snapshot S" | **Substrate class-1** — pure integer compare | `forgedb-txn` / storage |

**The single drift vector (sharp):** MVCC arriving bundled with a **generic transaction/query
executor** that takes predicates-as-data and interprets them against arbitrary tables at runtime.
Guard-passing mirror image: the substrate only ever answers *"assign the next LSN," "is V visible
to S," "truncate to mark,"* and *"has opaque-key K been committed since LSN L"*; the generated
`Database` drives the transaction, computes keys, and resolves versions per model. The instant a
shipped crate offers `db.transaction()` **over an arbitrary schema it reads at runtime**, the line
is crossed.

## Red lines (carried + sharpened from `mvcc-concurrency.md`)

1. **No generic runtime transaction/query executor.** Transactions ship as a *generated* method on
   the tailored `Database`; the substrate is mechanism only (LSN, visibility, truncate, opaque
   conflict keys). A predicate-as-data interpreter is the forbidden generic engine.
2. **`.forge` never becomes a runtime input** to any transaction/MVCC path.
3. **`forgedb-txn` (if created) is class-1 schema-agnostic** — rows, columns, opaque keys, LSNs.
   The moment it knows a model name, the boundary is violated.
4. **Preserve the append-only watermark model.** Tiers 1–2 are designed *specifically* to keep it
   valid (serialized commit); if a future tier breaks it (segmented columns, persisted commit-LSN),
   redesign backup + snapshot reads **in the same change** (the retraction-fork discipline).
5. **Authoring surface stays "schema + CLI + config."** Isolation level / retry policy, if
   configurable, live in `forgedb.toml` or are generated defaults — **never new `.forge` syntax**.
6. **Don't build Tier 2/3 machinery without a caller.** Ship Tier 1 (transactions) first; add
   optimistic concurrency only on demonstrated concurrent-writer demand (the
   machinery-without-consumers failure that killed `fulltext`/`crud-api`).
7. **Single-process scope holds through Tiers 1–2.** Multi-process (Tier 3) is a separately
   justified expansion, not silent scope creep.
8. **"Multi-writer" is never oversold.** Every external description states the ceiling: concurrent
   *prepare*, serialized *commit*, until segmented columns land.

## PM gate constraints (binding — 2026-07-14)

The identity gate passed the design and named three binding constraints. They close the gap between
this note's *stated* discipline and *enforced* discipline; none require redesign:

1. **Tier 1 rollback must be proven safe against auto-checkpoint (#96) and auto-compaction (#92)
   firing mid-transaction, and must leak zero broker/changefeed offsets on abort.** The first-
   milestone E2E must additionally assert: (a) a transaction whose staged appends cross the WAL-
   checkpoint or dead-row threshold still rolls back cleanly (no committed prefix corrupted, no
   orphaned WAL tail — the "drop the WAL tail past the mark" rollback must be safe against an auto-
   checkpoint that truncated the WAL underneath it); (b) a rolled-back transaction consumes **no**
   `DurableBroker` offset and emits **no** changefeed event (staging must not consume a monotonic
   offset until commit, else rollback leaves an offset gap). Add codegen guards for both, beyond the
   existing "truncates on error / emits only on commit" guard.
2. **`forgedb-txn` (Tier 2) must be structurally schema-agnostic and guard-enforced, not merely
   documented.** Its public API may reference only rows, columns, opaque byte-keys, and integer
   LSNs — **no `&str` model name, no field, no generic predicate parameter, no `Fn`-over-schema.**
   Conflict resolution stays equality / `last-committer-LSN > read-snapshot-LSN` comparison over
   opaque keys — **no schema-meaningful ordering, hashing-by-model, or ranging inside the crate**
   (the fig-leaf risk). Add a guard asserting the generated transaction commit/conflict path
   contains no `match model_name` (mirroring `test_rust_generation_replication_broker`). **Do not
   create the crate until Tier 2 has a real caller** (red line #6).
3. **Compaction-respects-live-transaction-snapshot is computed on the GENERATED side, handed to the
   substrate as opaque indices.** The #92 keep-set stays generated: generated code computes the live
   physical-row set *including versions pinned by the oldest in-flight transaction snapshot* and
   passes opaque indices to `compact_model_keeping`. `forgedb-compaction` gains **no** awareness of
   "transactions" or "snapshot LSNs." Ship this in the same change as transaction snapshots (do not
   defer past Tier 1 if long-lived read snapshots become reachable), so a live transaction cannot
   observe a GC'd version. (Promotes the "GC vs live snapshot" deferral bullet to a correctness-and-
   identity constraint.)

## First milestone (smallest useful slice)

**Tier 1 transactions only.** Generated `db.transaction(|tx| …)` + `TxHandle` (scoped per-model
write methods), commit = watermark advance + WAL fsync + batched event emit, rollback =
`truncate_to_rows` to pre-txn marks + WAL-tail drop. **No new substrate, no format break, no
publish gap.** Success = a transaction that creates a parent + child atomically, rolls back cleanly
on an error in the middle (no partial rows survive, no changefeed event leaks), and is snapshot-
isolated from a concurrent reader — proven E2E through current codegen (compile + run), with a
codegen guard asserting the txn body truncates on the error path and emits events only on commit.

## Honest deferrals

- **Parallel column append** (segmented columns) — the only path past serialized commit; a
  separate, larger storage redesign. Out of scope.
- **Serializable isolation** (read-set tracking) — Tier 2 ships snapshot isolation; serializable is
  a later knob.
- **Multi-process / Tier 3 coordinator** — held until demand; reachable by extending Tier 2.
- **Long-running / interactive transactions** holding a read snapshot for a long time — GC of
  superseded versions (compaction #92) must not reclaim versions a live snapshot still needs; a
  transaction registers its snapshot LSN so compaction respects the oldest live reader (a small
  addition to the #92 keep-set computation).
- **Distributed / cross-node** anything — explicitly not ForgeDB's domain.

## Cross-issue dependencies

- **Reuses #89/#96** WAL + fsync + checkpoint as the durability boundary (commit = existing fsync).
- **Reuses #56-A/B** watermark `Snapshot` + reader handles (transactions generalize the watermark;
  Tier 1 keeps it verbatim).
- **Reuses #82** `DurableBroker` monotonic offset as the commit-LSN source (Tier 2).
- **Interacts with #92 compaction** — GC must respect the oldest live transaction snapshot (see
  deferrals). Coordinate the keep-set computation.
- **Interacts with #74 migrations** — only Tier 3's optional persisted commit-LSN would touch the
  on-disk format / `format_version`; Tiers 1–2 do not, so they need no migration.

## Load-bearing references

- `crates/storage-native/src/lib.rs` — `truncate_to_rows` (rollback primitive), `Snapshot`
  (305–327), append-only columns; `dir_lock.rs` 22–114 (`DirLock`).
- `crates/codegen/src/rust.rs` — mutation surface (update ~2992–3124, delete ~3126–3226), `get_at`
  version resolution (~814–863), checkpoint (~2711–2750); the emission target for `transaction`.
- `crates/changefeed/src/durable` — `DurableBroker` monotonic offset (the Tier-2 commit-LSN).
- `docs/proposals/mvcc-concurrency.md` — Directions A + B (landed) and the Direction-C red lines
  this note fleshes out.
- `docs/proposals/backup-restore.md` — the append-only-watermark invariant Tiers 1–2 preserve.
- `CLAUDE.md` → "What ForgeDB is" — the invariant this note is gated against.
