# Proposal: Multi-Writer Concurrency & Transactions (MVCC Direction C)

**Status:** **ALL THREE TIERS LANDED (2026-07-14, #75/#84; merged to `main` 2026-07-15).** Tier 1
(generated transactions + atomic multi-model commit journal), Tier 2 (`forgedb-txn` optimistic
concurrent prepare / serialized commit), and Tier 3 (`forgedb-coordinator` multi-process control
plane + generated lock-free coordinated data plane) all shipped, PM identity gates PASS/PASS-WITH-
CONSTRAINTS, verified E2E (incl. a genuine two-live-process concurrent-writer run) and compile-tested
single- + multi-model. **Correction to the design's "zero substrate change" claim below:** the tiers
landed with **two _additive_ substrate methods** the design under-counted — `WalManager::truncate_to`
in `forgedb-wal` (partial WAL-tail rollback on transaction abort — Tier 1/2; the design assumed WAL
fsync alone sufficed) and `sync_from_disk` on all three `forgedb-storage-native` column types (Tier 3
peer read-currency: re-derive each column's `row_count` from disk so a coordinated peer's appends
become visible) — plus **two new substrate crates** `forgedb-txn 0.1.0` (Tier 2 commit sequencer) and
`forgedb-coordinator 0.1.0` (Tier 3 control plane, **no `forgedb-storage*` dep** — T3-8). All four are
**additive** (no on-disk format break), so the design's core claim — *no `xmin`/`xmax`, no format
break, watermark stays valid* — holds; but they **do open a publish gap**: generated code links
`forgedb-txn`/`-coordinator` (scaffold pins `= "0.1"`) and the additive `wal`/`storage-native` methods,
so those must publish before an outside-repo `init → build` resolves from crates.io. **Publish gap
CLOSED (2026-07-15):** published `forgedb-txn 0.1.0` + `forgedb-coordinator 0.1.0` + `forgedb-wal 0.2.2`
(`truncate_to`) + `forgedb-storage-native 0.1.1` (`sync_from_disk`) + `forgedb-storage-web 0.1.1` (no-op
`sync_from_disk` for wasm parity), scaffold pin `forgedb-storage 0.1.5 → 0.2`; reclose PROVEN by an
outside-repo `/tmp init → generate → cargo build` resolving them all from crates.io (0 errors). The
design reasoning below is preserved
as-authored; where it says "zero substrate change," read it against this correction.

**Original design-gate status:** DESIGN NOTE — Tiers 1–2 carried a `forgedb-product-manager` verdict
**PASS-WITH-CONSTRAINTS** (2026-07-14; 3 binding constraints, folded in below — see "PM gate
constraints") and are now **deepened to implementation-ready** (2026-07-14, #83): the concrete
`TxHandle` surface, the **atomic multi-model commit journal** (`_txn_journal.log`, the T3-4
prerequisite), the precise commit/rollback/recovery sequences with crash-window analysis, the
auto-checkpoint/compaction deferral + broker-offset buffering (PM constraint 1), the concrete
`forgedb-txn` API + `try_commit` algorithm + retry loop, and four ship-in-order sub-milestones (M1a–d).
**Tier 3 is fully designed** (2026-07-14, expanded from the earlier sketch at the user's direction —
all three tiers thorough). **The full 3-tier note passed the PM re-gate (#84, 2026-07-14):
PASS-WITH-CONSTRAINTS** — the control-plane/data-plane split is sound (no hidden path forces the
coordinator to know column layout), the `_txn_journal.log` classification is a clean class-1 split
(verified against `forgedb-wal`'s opaque `Raw` path), and the build order matches the in-codebase-caller
test. The re-gate **blessed T3-1..T3-7 with refinements, added a required T3-8** (the `opaque_row_bytes`
back-application tripwire = new drift vector 3), reclassified T3-6 as architecture-quality (not
identity-critical), and required **three structural guards** (coordinator no-column-write, `forgedb-txn`/
`-coordinator` public-API schema-agnosticism, strict-superset byte-identity) plus a broker-offset
contiguity-across-abort assertion — all folded in below. Tier 1 is a clean PASS with zero new *substrate*
artifacts (the journal reuses the published `forgedb-wal` `Raw` path); Tier 2's `forgedb-txn` is PASS
conditioned on enforced schema-agnosticism. Fleshes out the **Direction C** section from
[`mvcc-concurrency.md`](./mvcc-concurrency.md) (Directions A + B LANDED). This note is the design for
the Direction-C rock: **transactions + concurrent writers**.
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
  metadata, no format break. This is the smallest slice that delivers a real transaction boundary.
- **Tier 2 — Optimistic concurrent writers (serialized commit).** Many transactions *prepare*
  concurrently (read a snapshot, validate, stage into private buffers); a **serialized commit
  point** assigns a monotonic **commit LSN** and detects write-write conflicts against an in-memory
  id→last-committer map; losers roll back (Tier-1 truncate) and retry. Still no on-disk format
  break — commit remains serialized, so appends stay contiguous and the watermark stays valid.
- **Tier 3 — Multi-process writers (single-machine).** Removes single-process *within one machine*
  by giving one process the commit point (a **write coordinator**, the symmetric inverse of #82's
  durable broker). The coordinator is a **control plane** (conflict map + serialized commit turn +
  LSN sequencer + owner of the opaque `_replication.log`) that **never writes columns**; the
  schema-aware column write stays in **generated writer code** (the data plane), run under a granted
  exclusive turn. Because of the forks below it adds **no new on-disk format** and **no distributed
  2-phase record** — the coordinator *is* the single writer, so data recovery reuses #89/#96, and its
  only state (the conflict map) is in-memory. Multi-*machine* (network + consensus) is explicitly a
  separate future product (v2/v3), not this tier.

**Recommendation:** all three tiers are now designed thoroughly. **Build order is Tier 1 → Tier 2 →
Tier 3**, and it is a *technical* prerequisite chain, not a demand gate: Tier 2 extends Tier 1's
transaction boundary with a serialized commit point, and Tier 3 replaces Tier 2's in-process commit
mutex with the out-of-process coordinator (and inherits Tier 1's atomic multi-model commit for its
crash story). So each tier is a strict superset of the one before, and building an outer tier before
its inner tier exists is machinery without its own substrate. The legitimate sequencing test stays
the **in-codebase-caller** one (does generated code / a crate actually link this path yet), never a
market-demand one. The honest ceiling that bounds all three tiers — **one physical append point per
column** → *concurrent prepare, serialized commit* (the serial section includes the writer's `fsync`)
— is named loudly below; true parallel append needs segmented columns (a separate, larger storage
effort), and the commit-side optimization ladder that raises the ceiling *without* segmenting is
designed into Tier 3's assign-order/materialize-bytes seam.

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

**Consequence:** Tier 1 needs *zero on-disk-format* change (rollback = existing `truncate_to_rows` +
WAL-tail `truncate_to`, commit = existing watermark advance + WAL fsync). Tier 2 adds only an in-memory
commit sequencer + conflict map — still no format break. This is the single biggest reason to prefer
this staging over a textbook `xmin`/`xmax` engine. **(Implementation correction — see the Status
header:** Tier 1 rollback also needed an _additive_ `WalManager::truncate_to`, so the "no publish gap"
originally claimed here does **not** hold as-implemented; the additive methods + the two new crates are
a publish gap. The *format*-stability claim — the actual load-bearing point — is unaffected.)

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

### The transaction handle (generated, concrete surface)

`db.transaction(f)` captures the rollback marks + read snapshot, builds a `TxHandle`, runs `f`, then
commits on `Ok` / rolls back on `Err`-or-panic. The handle is **entirely generated per schema** — one
scoped write method per model (mirroring the standalone `create_/update_/delete_/link_`) and one
scoped read method per model:

```rust
impl Database {
    pub fn transaction<T>(
        &mut self,
        f: impl FnOnce(&mut TxHandle) -> Result<T, TxError>,
    ) -> Result<T, TxError> {
        let mut tx = TxHandle::begin(self);   // capture marks + read snapshot
        match f(&mut tx) {
            Ok(v)  => { tx.commit()?; Ok(v) }        // journal + watermark flip + events
            Err(e) => { tx.rollback();  Err(e) }     // truncate to marks; nothing observed
        }
        // Drop is the panic-safety backstop: an un-committed handle rolls back on unwind.
    }
}

pub struct TxHandle<'db> {
    db: &'db mut Database,
    snapshot: DatabaseSnapshot,          // read snapshot captured at begin (#56-A watermark vector)
    marks: BTreeMap<ModelTag, usize>,    // per-collection pre-txn physical row-count (lazy: on first touch)
    touched: BTreeSet<ModelTag>,         // collections that got appends → drives commit/rollback iteration
    pending_index:  Vec<IndexOp>,        // id_to_row / secondary-index updates, applied on commit
    pending_events: Vec<PendingChange>,  // changefeed + broker records, emitted (offsets consumed) on commit
    write_set: Vec<OpaqueKey>,           // Tier-2 conflict keys (unused/empty in Tier 1)
    committed: bool,
}

impl<'db> TxHandle<'db> {
    // exactly the generated write surface, scoped to the txn (stages, records the mark on first touch):
    pub fn create_user(&mut self, u: User)         -> Result<Uuid, TxError> { … }
    pub fn update_post(&mut self, id: Uuid, p: Post) -> Result<bool, TxError> { … }
    pub fn delete_comment(&mut self, id: Uuid)     -> Result<bool, TxError> { … }
    pub fn link_post_tag(&mut self, p: Uuid, t: Uuid) -> Result<(), TxError> { … }
    // read-your-writes: resolve newest-version-per-id over the committed prefix ∪ this txn's staged rows:
    pub fn get_user(&self, id: Uuid) -> Option<User> { … }
    // …one get_/all_ per model.
}
```

- **`TxError`** wraps the #91 `ValidationError` plus a `Conflict` variant (Tier 2). Because the scoped
  write methods call the **same generated validated write path** (`validate_<model>` + FK-existence),
  a `?` on any staged write propagates the validation failure and the outer `transaction()` rolls back
  — Tier 1 composes with #91 for free, no second validator.
- **Read-your-writes has one decode path.** `TxHandle::get_user` funnels through the existing
  `read_at`/`get_at` with the visibility watermark **raised to the current staged physical length**
  (not the public watermark), so it sees the txn's own uncommitted appends plus the committed prefix.
  Same "one shared `field_read_stmt` body" discipline as #113/#110 — the txn read is `get_at` with a
  different watermark, never a forked decoder.

### Mechanics — begin / stage / read

- **Begin:** capture the `DatabaseSnapshot` (current per-model watermarks — #56-A) for the txn's
  reads. Rollback marks are recorded **lazily**, the first time a given collection is written, as that
  collection's current physical row-count.
- **Stage writes:** the scoped `create_/update_/delete_/link_` append to columns + the per-model WAL
  **exactly as the #89 path today**, but (1) do **not** advance the public watermark, (2) do **not**
  emit changefeed/broker events (buffered into `pending_events`), and (3) buffer `id_to_row`/index
  mutations into `pending_index`. Staged rows are physically appended (past the watermark) but
  invisible to outside readers, whose `get`/`all` clamp at the public watermark.
- **Reads inside the txn** resolve against the raised watermark (staged length), giving
  read-your-writes over the committed prefix — see the handle surface above.

### The atomic multi-model commit journal (the crash-atomicity mechanism, T3-4)

The in-process visibility flip — advancing the N touched watermarks under the commit lock — is
trivially atomic *within a live process*: no other thread observes a half-advanced vector. The hard
part is **crash atomicity across models**: if the process dies after model `A`'s rows are durable but
before model `B`'s, recovery must make the txn **all-or-nothing**, not leave `A` committed and `B`
lost. The single-model #89 path (per-model WAL + column-length-prefix recovery) has no cross-model
notion of "this txn," so Tier 1 adds one thin, opaque, **additive** structure:

- **`_txn_journal.log`** — a shared, CRC-framed, append-only log at the data-dir root, **holding one
  record per committed txn**: the txn's assigned LSN + an opaque vector `[(model_tag, post_len)]` of
  every touched collection's post-commit row-count. It is framed and fsynced by **`forgedb-wal`'s
  existing `Raw` opaque-record path reused verbatim** (the record bytes are the generated encoding of
  the length vector) — **so it needs no new substrate API and no column-format change; `forgedb-wal
  0.2.1` already ships the `Raw` + torn-tail `read_all` primitives.** The journal is additive: a
  data dir without it uses the plain #89 single-model recovery (backward-compatible).

The journal record carries **lengths, not row bytes** — the per-model WAL already holds the bytes for
torn-append repair, so the journal only needs to flip visibility. This is the classic commit-record
design adapted to columns-as-truth.

*(Alternative considered and rejected: a rewrite-on-commit watermark-vector file made durable by
temp-write + rename. It is simpler per record but is not append-only, loses commit ordering, costs
file-fsync + dir-fsync per commit, and does not compose with Tier 3's `_replication.log` unification.
The append-only CRC journal matches every other durable structure in ForgeDB (WAL, broker), so its
recovery is a known pattern, and Tier 3 can later unify it with `_replication.log`.)*

### Commit / rollback / recovery — precise sequences + crash windows

**Commit** (under the commit lock; ordering is load-bearing — columns durable *before* the journal
marker, mirroring #96's checkpoint order and #7's WAL→columns→log):

```
1. flush + fsync every touched model's columns and its per-model WAL     // #89 per-model durability
2. append TxnCommitRecord{ lsn, [(model_tag, post_len)] } to _txn_journal.log; fsync   // ATOMIC COMMIT POINT
3. advance every touched model's public watermark to post_len            // in-memory visibility flip
4. apply pending_index (id_to_row + secondary indexes)
5. drain pending_events → changefeed + DurableBroker (broker offsets consumed HERE, not before)
```

**Rollback** (Err / panic / drop-without-commit):

```
1. truncate_to_rows(mark) every touched model's columns                  // erase staged rows
2. truncate each touched model's per-model WAL tail back past the mark   // drop staged WAL records
3. discard pending_index + pending_events
   → no journal record written, no watermark advanced, no event emitted, NO broker offset consumed
```

**Recovery at `open_at`** (extends #89):

```
1. read _txn_journal.log via forgedb-wal read_all → last CRC-valid record = authoritative
   durable { model → committed_len }.  (absent/empty journal ⇒ plain #89 per-model recovery.)
2. per model: run #89 recovery (replay per-model WAL tail, repair torn column tail).
3. per model: truncate columns down to committed_len — erasing any ahead-rows from a txn whose
   journal record never landed (a torn/aborted commit).
4. rebuild id_to_row + secondary indexes from the committed prefix (existing narrow rehydrate #110).
5. set each watermark = committed_len.
```

Crash windows — every one resolves all-or-nothing:

- **During stage-writes** (before commit step 1): per-model WALs/columns may be torn; #89 repairs each,
  and with **no journal record** step 3 truncates every touched model back to `committed_len` → the
  whole txn vanishes. Clean rollback.
- **After some models fsynced, before the journal fsync** (commit steps 1→2, the atomicity-critical
  window): several columns hold ahead-rows but there is **no journal record** → step 3 truncates them
  *all* back → all-or-nothing preserved even mid-commit. **This is the property #89 alone cannot give.**
- **After the journal fsync** (commit step 2 done, before/mid step 3): the journal record is present
  and — because columns were fsynced in step 1 *before* the journal — **all** touched columns are
  already durable → recovery reads the journal and advances every watermark to `committed_len` → the
  txn is fully visible. Clean forward-recovery commit.

One `fsync` (step 2) is the switch: **journal record present ⟺ txn committed ⟺ all its columns
durable.** No per-row `xmin`/`xmax`, no column-format change.

### Interaction with auto-checkpoint (#96) and auto-compaction (#92) — deferred for the txn (PM constraint 1a)

Both auto-triggers would corrupt a live transaction's invariants, so a transaction is a **critical
section that defers both**:

- **Auto-checkpoint (#96)** truncates the WAL when `writes_since_checkpoint ≥ 1000`. If it fired
  mid-txn it would truncate the per-model WAL *underneath* the rollback (whose "drop the WAL tail past
  the mark" then has nothing to drop, or drops a re-checkpointed prefix). **Fix:** an `in_transaction`
  guard suppresses auto-checkpoint for the txn's duration; on commit/rollback a pending trigger runs
  once. Rollback's WAL-tail truncation is therefore always valid (the WAL is never checkpointed mid-txn).
- **Auto-compaction (#92)** renumbers physical rows and reopens — which invalidates the txn's rollback
  marks (physical lengths) outright. Same `in_transaction` guard defers it to after commit/rollback.

Honest limit surfaced by this: with auto-checkpoint deferred, a **pathologically large single txn**
grows the WAL until it commits. Acceptable at the v1 design-partner bar; a size-bounded/streaming txn
is a later refinement (see deferrals).

### Broker / changefeed offset buffering (PM constraint 1b)

Staging must consume **no** `DurableBroker` offset and emit **no** changefeed event — otherwise a
rolled-back txn leaves an offset gap that #82 followers / #110 replicas would trip on. So staged
mutations buffer their `(model_tag, row_index, kind, opaque_bytes)` into `pending_events`, and the
commit **drains them to the broker in one burst at step 5**, consuming the contiguous offset run
`L..L+k` there and only there. Rollback drops the buffer → zero offsets consumed, no gap. (This also
gives the "one event burst per commit" batching the note already promised.)

### What Tier 1 delivers / forecloses

- **Delivers:** atomic multi-row / **multi-model** writes with **crash atomicity** (the journal),
  rollback on error, all-or-nothing FK integrity (create parent + child atomically), and a clean
  changefeed/broker batching point (one burst per commit). Snapshot isolation across the transaction
  falls out of the existing watermark for free.
- **Forecloses nothing in Tier 2/3** — it is a strict subset. Concurrent *writers* are still
  serialized (Tier 1 is single-writer with a transaction boundary); that is the next tier. The commit
  journal is exactly the structure Tier 3's coordinator drives for multi-model crash atomicity (T3-4),
  so building it here is a prerequisite, not throwaway.
- **Substrate cost:** **none new** — the journal reuses `forgedb-wal`'s published `Raw` path;
  rollback/commit reuse `truncate_to_rows` + watermark + WAL fsync. **Publish gap:** none (no format
  bump, no new crate). The one honest caveat: `_txn_journal.log` is a new (additive, opt-in-by-presence)
  file; a *downgraded* binary opening a dir mid-torn-multi-model-commit would ignore the journal and
  over-recover — a cross-version-across-crash edge, out of the single-writer contract, documented.

## Tier 2 — Optimistic concurrent writers (serialized commit)

Adds genuine *concurrent transaction preparation* with a *serialized commit point*:

- **Prepare concurrently:** N transactions each capture a read snapshot, validate, and stage into
  **private buffers** (not the shared columns yet) — validation/reads run in parallel.
- **Commit serially:** a commit mutex (or, for Tier 3, a coordinator) hands out a monotonic
  **commit LSN** (reuse #82's `DurableBroker` offset), runs **conflict detection**, then does the
  Tier-1 contiguous append + journal + watermark advance.
- **Isolation level:** snapshot isolation, first-committer-wins. Serializable (SSI) is a later knob
  (add read-set tracking); not first-cut, so **write-skew is the disclosed SI anomaly** (documented).

### `forgedb-txn` — the concrete substrate API (schema-agnostic by construction, constraint 2)

The whole commit-coordination discipline lives in one small class-1 crate whose public surface names
only **rows, opaque byte-keys, and integer LSNs** — no `&str` model name, no field, no predicate, no
`Fn`-over-schema. The generated `Database` computes the opaque keys and drives it:

```rust
// forgedb-txn — the boundary is violated the instant any of this knows a model's fields.
pub struct Lsn(pub u64);
pub type OpaqueKey = Box<[u8]>;   // generated code encodes (model_tag, id_bytes) etc.; substrate never interprets

pub struct WriteSet { pub keys: Vec<OpaqueKey>, pub snapshot_lsn: Lsn }

pub enum CommitOutcome { Committed(Lsn), Conflict { key: OpaqueKey } }

pub struct CommitSequencer {
    next_lsn: u64,
    conflicts: HashMap<OpaqueKey, Lsn>,   // opaque_key → LSN of its last committer
    live_snapshots: BTreeMap<Lsn, u32>,   // refcounted open read-snapshots, for GC
}

impl CommitSequencer {
    pub fn register_snapshot(&mut self) -> Lsn;          // = current committed LSN; refcount++
    pub fn release_snapshot(&mut self, s: Lsn);          // refcount--
    pub fn oldest_live_snapshot(&self) -> Lsn;           // compaction keep-set bound (constraint 3)

    /// The serialized commit. Pure opaque-key equality + integer compare — no schema anything.
    pub fn try_commit(&mut self, ws: &WriteSet) -> CommitOutcome {
        for k in &ws.keys {
            if matches!(self.conflicts.get(k), Some(&l) if l > ws.snapshot_lsn) {
                return CommitOutcome::Conflict { key: k.clone() };   // first-committer-wins
            }
        }
        let l = Lsn(self.next_lsn); self.next_lsn += 1;
        for k in &ws.keys { self.conflicts.insert(k.clone(), l); }
        CommitOutcome::Committed(l)
    }

    /// Prune keys no live snapshot can still conflict against (bounds the map — non-durable).
    pub fn gc(&mut self);   // drop entries with LSN < oldest_live_snapshot()
}
```

- **Conflict detection is first-committer-wins over opaque keys.** The generated prepare computes the
  write set as `(model_tag, id_bytes)` for every row inserted/superseded/deleted and
  `(model_tag, index_tag, key_bytes)` for every `&unique` key claimed — the *field-aware* "what did I
  touch" is generated; the crate sees only `OpaqueKey` bytes (same field-blind discipline as #82's row
  bytes and #92's keep-set). `try_commit` never orders, hashes-by-model, or ranges — equality +
  `last_committer > snapshot` compare only (the fig-leaf the PM gate names).
- **The map is in-memory, ephemeral, and bounded.** It catches conflicts among *concurrently live*
  txns; after restart there are none in flight, so it rebuilds empty → no durable commit-version
  column, no format touch. `gc()` prunes any key older than the oldest live snapshot.

### The commit path (generated, wrapping the sequencer between stage and Tier-1 commit)

```rust
pub fn transaction_retrying<T>(
    &mut self, retries: u32,
    f: impl Fn(&mut TxHandle) -> Result<T, TxError>,   // Fn (re-runnable) so auto-retry can re-execute
) -> Result<T, TxError> {
    for _ in 0..=retries {
        let s = self.seq.register_snapshot();               // read-snapshot LSN
        let mut tx = TxHandle::begin_at(self, s);
        let out = f(&mut tx);                               // prepare + stage (concurrent w/ other procs)
        let val = match out { Ok(v) => v, Err(e) => { tx.rollback(); self.seq.release_snapshot(s); return Err(e) } };
        // ---- serialized section (commit mutex in Tier 2; coordinator turn in Tier 3) ----
        match self.seq.try_commit(&tx.write_set_with_snapshot(s)) {
            CommitOutcome::Committed(l) => { tx.commit_with_lsn(l)?; self.seq.release_snapshot(s); return Ok(val) }
            CommitOutcome::Conflict { .. } => { tx.rollback(); self.seq.release_snapshot(s); /* retry */ }
        }
    }
    Err(TxError::Conflict)   // exhausted retries
}
```

- **Auto-retry contract:** bounded automatic retry (re-snapshot at the new watermark, re-run the `Fn`
  closure), falling back to a typed `TxError::Conflict` after a small default `N`. The default is
  **app-overridable via `forgedb.toml`** (never `.forge` — red line #5). The `Fn` (not `FnOnce`) bound
  is what makes the body re-runnable.
- **Commit-LSN source:** `next_lsn` unifies with #82's `DurableBroker` offset (the same monotonic token
  the journal record and the broker burst already use), so the write LSN, the journal LSN, and the
  broker offset are one sequence.
- **Where the serialized section lives:** an in-process commit mutex in Tier 2; the out-of-process
  coordinator turn in Tier 3 (`try_commit` moves into `forgedb-coordinator`, the only change) — which
  is exactly why Tier 3 is "replace the mutex," not a rewrite.

**Substrate cost:** the one small `forgedb-txn` crate (in-memory sequencer + opaque conflict map);
storage-native/web are **untouched** → no storage format break. **Publish gap:** at most one small new
substrate crate published once before a Tier-2 caller links it (red line #6 — do not create it until
Tier 2 has a real caller), no format bump.

**Honest ceiling (the physics):** because each column has **one physical append point**, commit is
serialized even in Tier 2 — "concurrent writers" = concurrent *prepare*, serialized *commit*
(group-commit-style). This is a real and common MVCC shape, but it is not parallel append. Parallel
append needs **segmented columns** (each writer owns a segment file, merged on read) — a distinct,
larger storage redesign explicitly **out of this note**. Name this in every external description so
"multi-writer" is never oversold.

## Tier 3 — Multi-process writers (single-machine)

Removes single-process *within one machine*: N processes write concurrently to one data dir,
coordinated by one process that owns the commit point. It is the symmetric inverse of #82's durable
broker (the broker broadcasts committed changes *out*; the coordinator accepts proposed changes *in*,
serializes them, and assigns LSNs) and it *reuses* that broker as its own commit log (fork #7). It is
reached by extending Tier 2 — the coordinator replaces Tier 2's in-process commit mutex — and it
inherits Tier 1's atomic multi-model commit for crash safety.

### Locked design forks

The design rests on decisions taken deliberately (each cascades into the mechanism):

1. **Single-machine only.** N processes on one host, shared filesystem, OS primitives. **Multi-machine
   (network transport, no shared FS, consensus/partition-tolerance) is a separate future product
   (v2/v3)** — it invalidates the shared-FS assumptions the whole engine rests on.
2. **Dedicated coordinator process** (not shared-memory turn-taking). The value is in the *process*
   (single-owner conflict map, fault isolation, trust boundary), not the transport — see
   "Transport & placement."
3. **Optimistic, concurrent-prepare / serialized-commit, snapshot isolation** — extends Tier 2's model
   across processes. Serializable (SSI, read-set tracking) is a confirmed **future** enhancement.
7. **Commit log unified with `_replication.log`** (#82 broker) — the coordinator's serialized commit
   log *is* the broker log, so #82 followers and #110 replicas consume Tier 3's output unchanged, and
   the monotonic broker offset is the commit LSN for free.
9. **Strict superset of Tiers 1–2 (MANDATORY).** With no coordinator configured, a process takes the
   in-process single-writer path **byte-identical to today**. The coordinator is opt-in; single-writer
   deployments pay nothing.

### The identity crux — control plane vs data plane

A commit ultimately appends row bytes into **per-schema columns** (which columns, what types, what
layout — all schema-aware). If the coordinator materialized columns, it would have to know the layout
→ it becomes a schema-interpreting engine → identity violation. So the column write **cannot** live in
the coordinator. The design splits the commit into two planes:

- **Coordinator = control plane** (schema-agnostic, class-1 substrate). Owns the conflict map (#6), the
  LSN sequence, the opaque `_replication.log` (#7), and grants **serialized exclusive commit turns**.
  It moves opaque keys, opaque row bytes, and integer LSNs — it never touches a column, never decodes a
  field, never dispatches on a model except as an opaque tag (same discipline as the #82 broker and
  `DirLock`).
- **Generated writer code = data plane** (schema-aware, tailored). Does the actual durable column + WAL
  write, under a granted turn — the *same* code path as today's single-writer commit.

The coordinator is thus a **lock manager + sequencer + conflict checker**, not a data-plane writer.
This is what lets Tier 3 stay inside the generator identity while columns-as-truth holds (the writer's
column+WAL fsync is the durable commit point). It is the single decision the PM re-gate will scrutinize
hardest.

### The commit handshake

```
prepare  (concurrent, no turn):  writer computes opaque write-set keys + committed row bytes @ snapshot S
   │
   ▼
RequestTurn{ write_set_keys, snapshot_lsn S }  ──▶ coordinator
   │                                                 conflict-check vs map (#6)
   │            ◀── Nack{ conflict_key }  ── writer aborts, re-snapshots, retries (bounded, auto)
   │            ◀── Grant{ turn_id, reserved_lsn L }   (exclusive turn held)
   ▼
writer does column + WAL fsync         ← columns-as-truth durable commit (existing #89 path)
   │
   ▼
Committed{ turn_id, model_tags, row_indices, opaque_row_bytes }  ──▶ coordinator
   │                                          append _replication.log @ L, release turn
   │            ◀── Ack{ L }
   ▼
writer applies buffered index updates, returns
```

Crash windows all fall on the safe side of the locked invariants:

- Writer dies **after Grant, before column commit** → torn/absent column tail truncated by existing
  #89 recovery; the coordinator's reserved `L` was never logged → reclaimed on **turn timeout**; txn
  aborts.
- Writer dies **after column commit, before Committed** → columns durable (committed) but the log is
  missing the entry → **reconcile-forward** on restart (log never ahead of durable data, #7).

The serial section **includes the writer's fsync** — only one writer commits at a time. That is inherent
to columns-as-truth + prefix recovery (narrowing it further is the deferred out-of-order-durability
bridge), and it is exactly the "not raw throughput" ceiling in "Performance envelope." Parallel prepare
stays *outside* the turn, so the prepare-heavy gain is intact.

### Conflict detection (extends Tier 2 across processes)

- **Prepare (client, concurrent):** the generated client runs the txn body at snapshot LSN `S` and
  computes its **write set as opaque keys** — `(model_tag, id_bytes)` for every row it inserts /
  supersedes / deletes, and `(model_tag, index_tag, key_bytes)` for every unique key it claims. The
  field-aware "what did I touch" is generated; the coordinator sees only opaque bytes.
- **Commit (coordinator, serialized):** validate that no key in the write set was committed by *another*
  txn at `LSN > S`. Clean → assign `L`, append, ack. Conflict → nack; the client auto-retries.
- **Isolation:** snapshot isolation, first-committer-wins. Write-skew is the disclosed SI anomaly
  (documented, not hidden); serializable/SSI (read-set tracking) is a future enhancement, so prepare
  does **not** report a read set (keeping the wire frame and client glue small).
- **The conflict map is in-memory only, and bounded.** It catches conflicts among *concurrently live*
  txns; after a coordinator restart there are none in-flight, so it rebuilds empty (no durable
  commit-version column → no format touch). Prune any key whose `last_committed_LSN` is older than the
  **oldest live snapshot** (no live txn can conflict against something already visible to it) — GC'd by
  the oldest open snapshot.
- **Abort/retry contract:** bounded **automatic** retry in the generated client (re-snapshot at the new
  watermark, re-run the prepare closure — which composes with Tier 1's re-runnable
  `transaction(|tx| …)` closure), falling back to a typed `ConflictError` after a small default `N`.
  The default is app-overridable via config.
- **FK-ordering conflicts** (delete-parent vs insert-child) are **not** a separate coordinator concern:
  the #91 FK-existence check fires at the child's commit and nacks if the parent is gone — a generated
  commit-time check, keeping the coordinator FK-blind.

### Durability & crash recovery — no distributed 2-phase record

The original sketch posited a durable 2-phase in-flight record. With the locked forks it is **not
needed**, and the reason is precise: a durable prepare record exists to recover a commit spanning
**multiple independently-failing resources** (distributed commit). Our commit is **single-node atomic**
— one coordinator, one filesystem; clients hold no durable state. So:

- **The coordinator *is* the single writer**, so committed-data crash recovery = the existing #89 WAL +
  #96 checkpoint + column-length-prefix path, **unchanged**. A torn mid-append commit truncates to the
  min-consistent prefix and the WAL tail replays, exactly as a single writer's torn write does today.
  **No new durable data structure, no format touch.**
- **The conflict map is non-durable** — rebuilt empty on restart.
- **In-flight client txns abort on coordinator restart** — the client sees the disconnect, re-snapshots,
  and retries. A txn is not durable until the serial commit's WAL fsync completes; anything short of that
  is lost and retried. (The clean payoff of serialized commit: prepare is client-local and disposable.)
- **Multi-model atomicity is inherited from Tier 1, not reinvented.** A txn spanning `User` + `Post`
  needs an atomic multi-model commit across a crash — exactly Tier 1's transaction boundary (a txn-scoped
  atomic commit journal listing all `(model, row_index)` appends, fsynced once, then applied). Tier 3's
  coordinator drives that Tier-1 commit; it does not build its own. **Cross-tier dependency: Tier 3
  requires Tier 1's atomic multi-model commit** (a concrete requirement handed to the Tier-1 deepening).
- **#7 ordering (columns-as-truth):** the durable commit point is the writer's column + WAL fsync;
  `_replication.log` is appended *after* (order: **WAL fsync → columns → log**) and, if a crash leaves it
  behind, **reconciled forward** from the committed columns/WAL tail on restart. The log is never *ahead*
  of durable data, so followers/#110 replicas never see an offset unbacked by committed rows.

### Transport & placement

- **Transport: unix domain socket** at `<root>/_coordinator.sock` (single-machine ⇒ no network;
  filesystem-namespaced, permission-controlled). The frames are all opaque:
  `RequestTurn{ write_set_keys:[(model_tag,opaque_key)], snapshot_lsn }` →
  `Grant{ turn_id, reserved_lsn }` | `Nack{ conflict_key }`;
  `Committed{ turn_id, model_tags, row_indices, opaque_row_bytes }` → `Ack{ lsn }`. Nothing on the wire
  is a decoded field.
- **Why a process (not shared-memory coordination):** the conflict-check is a whole-map
  check-and-insert (not a single atomic op), so shared memory would need a **robust mutex** anyway —
  inheriting crash-with-lock-held repair of a shared concurrent structure — without escaping
  serialization. A process gives a **single-owner conflict map** in ordinary heap memory, **fault
  isolation** (a writer crash can't corrupt coordinator state; only a coordinator restart matters, and
  it's clean), and a **trust boundary** (writers submit opaque frames; they can't scribble on the
  invariants). Decisively, the IPC latency shared memory would save is **dwarfed by the writer's fsync**
  in the serial section, so it optimizes a non-bottleneck. Shared memory becomes attractive only in the
  rung-3 regime (fsync amortized away) — the opaque-frame protocol keeps the transport swappable if that
  day comes.
- **Placement: a new class-1 substrate crate `forgedb-coordinator`** — owns the socket server, the
  conflict map, and the turn/LSN sequencer; links `forgedb-changefeed` (the `_replication.log` / broker
  it fans out) + `forgedb-wal`. It ships as a standalone process, a **`forgedb coordinate <root>`**
  subcommand (or the generated server hosts it). The **client glue is generated per-schema**: it
  materializes typed writes into opaque intents, runs the handshake, and does the data-plane column
  write under the turn — same class as the generated `/replicate` endpoint. Substrate + generated glue =
  the identity-clean split. (Chosen as a *separate* crate rather than folding into `forgedb-changefeed`
  because the coordinator is a much larger concern than the broker.)

### Peer read-currency — how one writer sees another's commits

Single machine ⇒ **one set of column files**. When writer A commits, its rows are physically on disk in
the shared columns; writer B does not re-materialize anything. B stays current by:

- **Reading the shared, growing column files** — exactly #56-B (shared-fd positional reads with live
  length; the read-under-live-append machinery already exists), driven by
- **the coordinator's log tail as an index-refresh signal** — the log frame says "id X changed at row
  `p`, model `M`, kind `K`," and B refreshes its in-memory `id_to_row`/indexes for those ids by reading
  position `p` from the shared file.

So a writer process is *also* a lightweight follower — but a **same-machine** peer uses the frame only as
a **which-ids-to-refresh signal** (data comes from the shared columns via #56-B), *not* as a data channel
(the frame's opaque bytes are what a **remote** #110 replica needs; a local peer ignores them and reads
the shared file). A writer's **snapshot LSN `S` = the log offset it has refreshed up to**; it prepares
against `S`, the coordinator conflict-checks against `S`, and offset-dedup means a writer never
double-counts its own commit.

### Operational model

- **Lifecycle: an explicit supervised process** (`forgedb coordinate <root>` under systemd/container),
  not auto-spawn (which brings ownership ambiguity, spawn races, and zombies). Multi-writer is an opt-in
  topology you set up deliberately.
- **Mode switch via the `DirLock`, for free:** the **coordinator holds the #89 `DirLock`** on behalf of
  all clients. A client writer (coordinator socket configured) connects and does **not** take the lock; a
  standalone writer (no coordinator) takes the lock itself — so a coordinator can't also run. The lock
  makes "coordinated" and "standalone" modes **mutually exclusive** with zero new machinery, and the #9
  strict-superset switch is literally "is a coordinator socket configured?"
- **Coordinator-down behavior:** **reads keep working** at the writer's last-refreshed snapshot
  (stale-but-consistent — reads hit local shared columns, which don't need the coordinator);
  **commits block with bounded retry/backoff, then surface a typed `CoordinatorUnavailable`** (the app
  decides whether to keep retrying or fail the request). The snapshot cannot advance while the
  coordinator is down (no new log frames) → a frozen-but-consistent read view.
- **Liveness both ways:** a writer detects coordinator loss via socket disconnect; the coordinator
  detects a dead/stuck writer via socket close or a **turn timeout** — a granted-but-unfinished turn is
  reclaimed and its LSN reservation dropped, so one stuck writer cannot wedge all commits (the #5 reclaim
  doubling as the liveness mechanism).

### The assign-order / materialize-bytes seam (keeps the commit side open)

The coordinator serializes exactly one thing that *must* be serial: **the ordering decision** (assign
LSN `L` and the append position). That decision is tiny and cheap; everything downstream — writing row
bytes, fsyncing the K column files, checksums, index updates, log framing — is *materializing an order
already decided*, and need not be single-threaded. Keeping this seam clean is a **design invariant**,
because it is what leaves the commit side open to later optimization without touching the coordinator's
contract. The optimization ladder (increasing size):

1. **Intra-commit parallel column flush** — fan the K-column flush of one commit across helper workers.
   Pure work parallelism, **no model change**.
2. **Group commit** — assign `p..p+M` to a batch, write all rows, **one fsync** for the batch, advance
   the watermark past the whole batch. The clean way to beat the fsync ceiling; **compatible, no model
   change**.
3. **Async materialization with the log as commit point** — the coordinator's log fsync becomes the
   durability point and columns become a projection built by generated materializers (the #82/#110 apply
   path pointed inward). This is a genuine architectural move (**log-as-truth**), with read-visibility and
   durable-marker consequences — **deferred**.

**Rungs 1 and 2 need neither bridge** — columns stay the source of truth, recovery stays column-length
derived, no durable LSN marker becomes load-bearing. Only **rung 3** and **out-of-order durable commits**
require crossing the durability-marker/format bridge; both are deferred, and the seam keeps them reachable
without a redesign.

### Performance envelope (honest)

Tier 3 delivers **(1)** multi-process writers as a *capability* that does not exist today (a second writer
is refused now), and **(2)** **parallel prepare** — validation, regex, serialization, FK-existence reads,
and unique/index candidate lookups run concurrently across processes, with only the commit serialized.
It does **not** deliver higher raw commit throughput: the commit is serialized and its serial section
includes the per-commit **fsync**, so aggregate commit rate is bounded by a **single committer's fsync
rate** regardless of process count, and mode-A adds a (negligible-vs-fsync) IPC round-trip per commit —
which is exactly why the #9 strict-superset path must keep single-writer deployments off the coordinator.
The win is real only when **prepare is the bottleneck**; a commit/fsync-bound workload gains the
*capability* but not throughput until **group commit** (rung 2) lands. The *magnitude* depends on the
workload's prepare/commit ratio and the disk's fsync latency (not quantifiable without measurement); the
*shape* is fixed: parallel prepare, serial commit+fsync+IPC.

### Honest limits (Tier 3)

- **The coordinator is a single point of failure for writes** (reads survive on stale snapshots). A
  highly-available coordinator (failover / leader election) is **multi-machine territory → v2/v3**. For
  the single-machine cut, restart is cheap (rebuild the empty conflict map, reconcile the log forward) and
  writes resume — the SPOF is a brief write-pause on restart, not data loss.
- **The serial section includes the writer's fsync** until group commit (rung 2) or log-as-truth (rung 3)
  land — so "multi-writer" buys safe concurrency and parallel prepare, *not* linear write throughput.
- **Same-machine only** (fork #1); no cross-node writers.

### Tier 3 — PM constraints (BLESSED / REFINED — PM re-gate #84, 2026-07-14)

The `forgedb-product-manager` re-gate of the full 3-tier note returned **PASS-WITH-CONSTRAINTS**: the
control-plane/data-plane split is sound (no hidden path forces the coordinator to know column layout),
the `_txn_journal.log` classification is a clean class-1 split (verified against `forgedb-wal`'s `Raw`
path — `[payload_len][payload_bytes]`, no decode step), and the build order matches the in-codebase-caller
test (red line #6). The blessed/refined set:

- **T3-1 — The coordinator never writes a column and never decodes a field.** All schema-aware
  materialization stays in generated writer code (the data plane); the coordinator moves opaque keys,
  opaque row bytes, and integer LSNs only. **Guard (refined — the model-name string-check alone is
  necessary-not-sufficient, since the coordinator's real hazard is column *materialization from opaque
  bytes*, not model-name dispatch):** the `no `match model_name`` discipline of
  `test_rust_generation_replication_broker` **plus** the structural no-column-write guard of T3-8.
- **T3-2 — `forgedb-coordinator` is class-1 schema-agnostic**, like `forgedb-txn` — rows, columns, opaque
  keys, LSNs; the instant it knows a model's fields the boundary is violated. **Guard (refined to
  structural, not documented):** assert the public API references only `Lsn`/`OpaqueKey`/`WriteSet`/
  `CommitOutcome`-class types — no `&str` model-name parameter, no `Fn`-over-schema, no generic predicate
  type (an API-signature / compile-fixture assertion, held to the same bar as `forgedb-txn`).
- **T3-3 — Columns stay the source of truth; no new durable format artifact.** Data recovery reuses
  #89/#96; the conflict map is non-durable; the `_replication.log` is appended last and reconciled forward
  (never ahead of durable data). Rung-3 (log-as-truth) is a *separate future change* that must redesign
  read-visibility + durability in the same commit (retraction-fork discipline). **(PM: the most important
  T3 constraint — keep verbatim.)**
- **T3-4 — Tier 3 depends on Tier 1's atomic multi-model commit**, inherited not reinvented; do not build
  Tier 3 before that Tier-1 commit journal exists.
- **T3-5 — Strict superset of Tiers 1–2 (the #9 mandate):** no coordinator configured ⇒ the in-process
  single-writer path is byte-identical to today; the `DirLock` enforces coordinated-vs-standalone mutual
  exclusion. **Guard (refined to byte-identity, matching #110's "zero codegen branches" proof):**
  snapshot-assert the generated single-writer commit/`transaction` body with no coordinator configured
  **equals the pre-Tier-3 baseline output** — so "pay nothing if single-writer" is enforced, not promised.
- **T3-6 — The assign-order/materialize-bytes seam is preserved** so rungs 1–2 (intra-commit fan-out,
  group commit) remain drop-in and rung 3 remains reachable without a coordinator-contract change.
  **(PM reclassification: this is an architecture-quality / performance constraint, NOT identity-critical
  — violating it does not make the coordinator schema-aware; no identity guard is required, only the
  perf-envelope discipline.)**
- **T3-7 — "Multi-writer" is never oversold:** every external description states the single-machine scope,
  the write-SPOF, and the serial-commit-includes-fsync ceiling.
- **T3-8 (NEW — REQUIRED by the re-gate) — The coordinator forwards `opaque_row_bytes` to the
  `_replication.log` and NEVER decodes them.** The `Committed` frame's row bytes exist solely for remote
  #110 replicas; the coordinator treats them as write-through-to-log payload only, and a local same-machine
  peer reads the shared columns via #56-B and ignores the frame bytes. This is the precise seam where a
  "helpful" optimization (coordinator applies the bytes itself to save the peer a read) would silently turn
  the coordinator into a data-plane writer — see drift vector 3. **Guard:** assert `forgedb-coordinator`
  contains **no** `forgedb-storage*` column-write dependency (`FixedColumn`/`VariableColumn`/`Tombstones`
  append) and **no** decode of the `opaque_row_bytes` frame member (it may only append/forward them). This
  is the drift-vector-2 tripwire made concrete — the one thing T3-1's model-name guard does not catch.

## Identity verdict & the substrate/generated split

Mapping to the guard (`CLAUDE.md` → "What ForgeDB is"; carries forward
`mvcc-concurrency.md`'s Direction-C red lines):

| Piece | Class | Where it lives |
|---|---|---|
| `db.transaction(\|tx\| …)` closure + `TxHandle` per-model methods | **Generated** (tailored) | `rust.rs` |
| Rollback-by-truncate orchestration; which columns to truncate/append | **Generated** | `rust.rs` |
| Commit-journal record contents (opaque `[(model_tag, post_len)]` vector) + recovery decode | **Generated** | `rust.rs` |
| `_txn_journal.log` framing / fsync / torn-tail read (the atomic-commit marker, M1b) | **Substrate (published)** — reuses `forgedb-wal` `Raw` verbatim | `wal` |
| Version/visibility resolution per id (LSN or watermark compare) | **Generated** | `rust.rs` (extends `get_at`) |
| Conflict-key computation (id → opaque bytes) | **Generated** | `rust.rs` |
| `truncate_to_rows`, watermark, WAL fsync | **Substrate (published)** — reused as-is | `storage-native`, `wal` |
| Commit LSN sequencer + opaque-keyed conflict map (Tier 2) | **Substrate class-1** — new `forgedb-txn` (rows/keys/LSNs only) | new crate |
| Visibility predicate "is version V visible to snapshot S" | **Substrate class-1** — pure integer compare | `forgedb-txn` / storage |
| Write coordinator: socket server + serialized turn + LSN + opaque conflict map + owns `_replication.log` (Tier 3, control plane) | **Substrate class-1** — new `forgedb-coordinator` (opaque keys/bytes/LSNs; no column, no field) | new crate + `forgedb coordinate` process |
| Client glue: materialize typed writes → opaque intents, run the handshake, do the data-plane column write under the turn (Tier 3) | **Generated** (tailored) | `rust.rs` |
| Peer read-currency: which-ids-to-refresh from the log tail + shared-column reads (Tier 3) | **Generated** over #56-B reads + #82 fan-out | `rust.rs` |

**Drift vector 1 (Tiers 1–2, sharp):** MVCC arriving bundled with a **generic transaction/query
executor** that takes predicates-as-data and interprets them against arbitrary tables at runtime.
Guard-passing mirror image: the substrate only ever answers *"assign the next LSN," "is V visible
to S," "truncate to mark,"* and *"has opaque-key K been committed since LSN L"*; the generated
`Database` drives the transaction, computes keys, and resolves versions per model. The instant a
shipped crate offers `db.transaction()` **over an arbitrary schema it reads at runtime**, the line
is crossed.

**Drift vector 2 (Tier 3, sharp):** the **write coordinator materializing columns** — the moment it
knows the per-schema column layout to apply a write itself, it becomes a schema-interpreting engine.
Guard-passing mirror image: the coordinator is *control plane only* (serialized turn + LSN + opaque
conflict map + opaque log); the **generated writer** does the schema-aware column write, under a
granted turn. The coordinator answers only *"grant a turn," "assign LSN L," "has opaque-key K been
committed since LSN S," "append these opaque bytes to the log."* The instant `forgedb-coordinator`
decodes a field or writes a column, the line is crossed (constraints T3-1/T3-2).

**Drift vector 3 (Tier 3, introduced by the deepened design — flagged by the #84 re-gate):** the
coordinator **back-applying `opaque_row_bytes`**. Because the `Committed` frame already carries the row
bytes (for remote #110 replicas) and the coordinator already owns the log, there is a standing temptation
to have it *apply* those bytes to bring local peers current — "it has them anyway, why make peer B
re-read the file?" The instant it does, it must know the per-schema column layout to place them → it
becomes a data-plane writer → identity violation. Guard-passing mirror image: local same-machine peers
read the shared, growing columns via #56-B (schema-blind positional reads) and use the coordinator's log
frame only as a *which-ids-to-refresh signal*; the coordinator treats `opaque_row_bytes` as
write-through-to-log payload it never decodes (constraint **T3-8**, the concrete tripwire).

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
6. **Build outer tiers only after their inner tier's substrate exists (in-codebase-caller test).**
   Ship Tier 1 (transactions) first; add Tier 2 optimistic concurrency once a Tier-1 caller links it;
   add Tier 3 once Tier 2's commit point exists to be replaced by the coordinator. Each tier is a
   strict superset of the one before, so building an outer tier first is machinery without its own
   substrate — the failure that killed `fulltext`/`crud-api`. Sequencing is the internal-caller test,
   never a market-demand one.
7. **The write coordinator is control-plane-only and schema-agnostic.** It never writes a column, never
   decodes a field (including **never decoding the `opaque_row_bytes` it forwards to the log** — T3-8),
   and dispatches on a model only as an opaque tag; the schema-aware column write stays in generated
   code. `forgedb-coordinator` is class-1 substrate (opaque keys/bytes/LSNs) — the moment it knows a
   field or back-applies the row bytes it holds, the boundary is violated (constraints T3-1/T3-2/T3-8,
   drift vectors 2 + 3).
8. **Single-machine scope for Tier 3; multi-machine is a separate product.** Tiers 1–2 are
   single-process; Tier 3 is single-*machine* multi-process (shared FS, OS primitives). Cross-node
   writers (network + consensus) are v2/v3, not silent scope creep.
9. **Columns stay the source of truth through all three tiers; no durable format artifact.** Recovery
   stays column-length derived; the conflict map is non-durable; the `_replication.log` is never ahead
   of durable data. A future rung-3 (log-as-truth) is a separate change that must redesign
   read-visibility + durability together (retraction-fork discipline; see also red line #4).
10. **"Multi-writer" is never oversold.** Every external description states the ceiling: single-machine
    scope, write-SPOF, concurrent *prepare* / serialized *commit* (serial section includes fsync) until
    group commit or segmented columns land.

## PM gate constraints — Tiers 1–2 (binding — 2026-07-14)

The identity gate passed the Tiers 1–2 design and named three binding constraints (Tier 3's proposed
constraints are the T3-* set under "Tier 3 — PM constraints," pending the full-note re-gate). They
close the gap between this note's *stated* discipline and *enforced* discipline; none require redesign:

1. **Tier 1 rollback must be proven safe against auto-checkpoint (#96) and auto-compaction (#92)
   firing mid-transaction, and must leak zero broker/changefeed offsets on abort.** The first-
   milestone E2E must additionally assert: (a) a transaction whose staged appends cross the WAL-
   checkpoint or dead-row threshold still rolls back cleanly (no committed prefix corrupted, no
   orphaned WAL tail — the "drop the WAL tail past the mark" rollback must be safe against an auto-
   checkpoint that truncated the WAL underneath it); (b) a rolled-back transaction consumes **no**
   `DurableBroker` offset and emits **no** changefeed event (staging must not consume a monotonic
   offset until commit, else rollback leaves an offset gap). Add codegen guards for both, beyond the
   existing "truncates on error / emits only on commit" guard. **(#84 re-gate — extend to offset
   *contiguity across an abort*:** a committed txn *following* an aborted one must leave **no** offset
   gap — its offset run must be contiguous with the *last committed* run, not the aborted attempt (the
   substrate-contract #82 followers / #110 replicas depend on).)
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

## Implementation plan — Tier 1 (implementation-ready sub-milestones)

Tier 1 decomposes into four ship-in-order slices, each with its guard(s). No new substrate crate, no
column-format break; the only new artifact is the additive `_txn_journal.log` framed by
`forgedb-wal`'s published `Raw` path.

- **M1a — the generated handle + single-model commit/rollback.** Emit `Database::transaction` +
  `TxHandle` (scoped `create_/update_/delete_/link_` + `get_/all_`), staged-append + read-your-writes
  (raised watermark), commit = fsync + watermark advance + `pending_index`/`pending_events` apply,
  rollback = `truncate_to_rows` to marks + WAL-tail drop. Guard `test_rust_generation_transaction`:
  the txn body truncates on the error path, advances the watermark only on the `Ok` path, and reads
  see staged rows. E2E: create-then-error rolls back to zero surviving rows.
- **M1b — the atomic multi-model commit journal.** Add `_txn_journal.log` (reused `forgedb-wal`
  `Raw`), the ordered commit sequence (columns fsync → journal fsync → watermark), and journal-driven
  recovery in `open_at` (truncate every touched model to `committed_len`; absent journal ⇒ #89 path).
  Guard `test_rust_generation_txn_commit_journal`: the emitted commit fsyncs columns *before* the
  journal record and recovery truncates all touched models to the journalled length. E2E: kill mid-
  commit **between** two models' column fsyncs and after some are durable → reopen shows **neither**
  model's staged rows (all-or-nothing); kill *after* the journal fsync → reopen shows **both**.
- **M1c — auto-checkpoint/compaction deferral + broker-offset buffering (PM constraint 1).** Gate the
  #96/#92 auto-triggers behind `in_transaction`; run a pending trigger once on commit/rollback. Buffer
  broker/changefeed records; consume offsets only in the commit burst. Guards
  `test_rust_generation_txn_defers_maintenance` + extend the changefeed-emit guard: a txn crossing the
  WAL-checkpoint / dead-row threshold still rolls back cleanly (no orphaned WAL tail), and a rolled-
  back txn consumes **no** `DurableBroker` offset and emits **no** changefeed event — **and (#84 re-gate)
  a commit *following* an abort has an offset run contiguous with the last committed run (no gap)**. E2E:
  a rollback after crossing the checkpoint threshold leaves the WAL/columns consistent; broker offset
  unchanged; the next commit's offsets are contiguous.
- **M1d — compaction respects the live-transaction snapshot (PM constraint 3).** Generated keep-set
  includes versions pinned by `oldest_live_snapshot()`; `forgedb-compaction` stays snapshot-unaware
  (opaque indices). Guard: the generated compaction keep-set includes rows below the oldest live txn
  snapshot. E2E: a long-lived txn's snapshot still resolves old versions after an intervening
  auto-compaction.

Ship M1a–M1d before Tier 2. Only after a Tier-1 caller exists does `forgedb-txn` (Tier 2) get created
(red line #6).

## Honest deferrals

- **Parallel column append** (segmented columns) — the only path to true parallel *append* (past even
  group commit); a separate, larger storage redesign. Out of scope.
- **Serializable isolation** (SSI / read-set tracking) — all tiers ship snapshot isolation;
  serializable is a later knob (Tier 3 would add read-set reporting to the prepare frame).
- **Group commit + intra-commit column fan-out (Tier 3 rungs 1–2)** — the throughput levers that beat
  the single-committer fsync ceiling *without* a format change; designed into the assign-order seam but
  not first-cut. **Log-as-truth async materialization (rung 3)** and **out-of-order durable commits** —
  the larger levers that *do* need the durability-marker/format bridge; deferred, seam-reachable.
- **Highly-available coordinator** (failover / leader election) and any **multi-machine / cross-node**
  writers — multi-machine territory, a separate future product (v2/v3); Tier 3 is single-machine, so its
  coordinator is a write-SPOF (reads survive on stale snapshots).
- **Shared-memory coordinator transport** — reserved for the rung-3 regime (once fsync no longer
  dominates the serial section); the unix-socket transport is the first cut, kept swappable behind the
  opaque-frame protocol.
- **Long-running / interactive transactions** holding a read snapshot for a long time — GC of
  superseded versions (compaction #92) must not reclaim versions a live snapshot still needs; a
  transaction registers its snapshot LSN so compaction respects the oldest live reader (constraint 3 /
  a small addition to the #92 keep-set computation).
- **Distributed / cross-node** anything — explicitly not ForgeDB's domain (see the multi-machine
  deferral above).
- **Size-bounded / streaming transactions** — because Tier 1 defers auto-checkpoint (#96) for a txn's
  duration, a pathologically large single txn grows the per-model WAL until it commits. Acceptable at
  the design-partner bar; a streaming/chunked-commit txn that lets checkpoint reclaim its committed
  prefix mid-flight is a later refinement.

## Cross-issue dependencies

- **Reuses #89/#96** WAL + fsync + checkpoint as the durability boundary (commit = existing fsync);
  Tier 3's data crash-recovery is #89/#96 unchanged (the coordinator *is* the single writer). Tier 1's
  atomic multi-model commit journal (`_txn_journal.log`, M1b) **reuses `forgedb-wal`'s published `Raw`
  path verbatim** — no new substrate API, no column-format bump; it is an additive root-level file.
- **Reuses #56-A/B** watermark `Snapshot` + reader handles (transactions generalize the watermark;
  Tier 1 keeps it verbatim; **Tier 3 peer read-currency is #56-B shared-column live reads** driven by
  the log tail as a which-ids-to-refresh signal).
- **Reuses #82** `DurableBroker` monotonic offset as the commit-LSN source (Tier 2), and its
  `_replication.log` **as Tier 3's unified commit log** (fork #7) — so #82 followers and #110 replicas
  consume Tier 3's output unchanged.
- **Depends on Tier 1** — Tier 3's multi-model crash atomicity inherits Tier 1's atomic multi-model
  commit journal (constraint T3-4); it does not build its own.
- **New substrate `forgedb-coordinator` (Tier 3)** — class-1 control-plane crate (socket server +
  serialized turn + LSN + opaque conflict map, owning `_replication.log`); links `forgedb-changefeed` +
  `forgedb-wal`; ships as a `forgedb coordinate <root>` process. Publishes once before generated client
  glue links it (the standard publish-gap discipline) — but adds **no on-disk format change**.
- **Interacts with #92 compaction** — GC must respect the oldest live transaction snapshot (see
  deferrals). Coordinate the keep-set computation.
- **Interacts with #74 migrations** — **no tier touches the on-disk format** (Tier 3 keeps columns as
  the source of truth, conflict map non-durable), so none needs a `format_version` bump or a migration;
  only a *future* rung-3 (log-as-truth) or segmented columns would, and that is a separate change.

## Load-bearing references

- `crates/storage-native/src/lib.rs` — `truncate_to_rows` (rollback primitive), `Snapshot`
  (305–327), append-only columns; `dir_lock.rs` 22–114 (`DirLock`).
- `crates/codegen/src/rust.rs` — mutation surface (update ~2992–3124, delete ~3126–3226), `get_at`
  version resolution (~814–863), checkpoint (~2711–2750); the emission target for `transaction`.
- `crates/changefeed/src/durable` — `DurableBroker` monotonic offset (the Tier-2 commit-LSN) +
  `_replication.log` framing (Tier 3's unified commit log, fork #7); `to_wire`/`from_wire` the wire codec
  the coordinator reuses for opaque frames.
- `crates/storage-native/src/reader.rs` (#56-B) — shared-fd positional reads with live length: the
  Tier-3 peer read-currency primitive (read a file another process is appending to).
- `src/commands/` — the future `coordinate` subcommand host for the `forgedb-coordinator` process.
- `docs/proposals/mvcc-concurrency.md` — Directions A + B (landed) and the Direction-C red lines
  this note fleshes out.
- `docs/proposals/backup-restore.md` — the append-only-watermark invariant all three tiers preserve.
- `docs/proposals/realtime-subscriptions.md` — #82 durable broker (Tier 3's symmetric inverse) + #110
  replica apply (the fan-out Tier 3 feeds; the "apply path pointed inward" of rung 3).
- `CLAUDE.md` → "What ForgeDB is" — the invariant this note is gated against.
