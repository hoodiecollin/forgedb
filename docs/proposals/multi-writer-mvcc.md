# Proposal: Multi-Writer Concurrency & Transactions (MVCC Direction C)

**Status:** DESIGN NOTE — `forgedb-product-manager` verdict **PASS-WITH-CONSTRAINTS** (2026-07-14;
3 binding constraints, folded in below — see "PM gate constraints"). Tier 1 is a clean PASS with
zero new artifacts; Tier 2's `forgedb-txn` is PASS conditioned on enforced schema-agnosticism +
caller-gating. Fleshes out the **Direction C** section deferred (no in-codebase caller) in
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
  metadata, no format break. This is the smallest slice that delivers a real transaction boundary.
- **Tier 2 — Optimistic concurrent writers (serialized commit).** Many transactions *prepare*
  concurrently (read a snapshot, validate, stage into private buffers); a **serialized commit
  point** assigns a monotonic **commit LSN** and detects write-write conflicts against an in-memory
  id→last-committer map; losers roll back (Tier-1 truncate) and retry. Still no on-disk format
  break — commit remains serialized, so appends stay contiguous and the watermark stays valid.
- **Tier 3 — Multi-process writers (held).** Removes single-process by giving one process the
  commit point (a **write coordinator**, the symmetric inverse of #82's durable broker). This is
  the only tier that breaks single-process scope and warrants a format/protocol commitment — a wire
  protocol plus a durable 2-phase in-flight record — so it is deferred until an in-codebase caller
  needs multi-process writes.

**Recommendation:** design + build **Tier 1** (transactions) as the first milestone; it is
independently valuable, append-only-preserving, and low identity-risk. **Tier 2** once a Tier-1
caller (generated code or a crate) actually needs concurrent preparers — it is a strict superset of
Tier 1, so building it before Tier 1 has a caller is machinery without a consumer. **Tier 3
deferred** (breaks single-process scope; see below). The honest ceiling that bounds Tiers 1–2 —
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

**Deferred** because it breaks the single-process scope and needs a durable 2-phase commit protocol
plus a wire protocol — a format/protocol commitment no in-codebase caller requires yet. Reachable by
extending Tier 2 (the coordinator replaces the in-process commit mutex). Recorded here so the staging
is explicit, not designed in detail.

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
   optimistic concurrency only once something in the codebase (generated code or a crate) actually
   links it — no generated code or crate links a concurrent-writer path today. Building it first is
   the machinery-without-consumers failure that killed `fulltext`/`crud-api`.
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
- **Multi-process / Tier 3 coordinator** — deferred; breaks single-process scope and needs a
  durable 2-phase protocol. Reachable by extending Tier 2.
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
