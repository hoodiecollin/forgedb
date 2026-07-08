# Proposal: The Generated Mutation Surface (`update` / `delete`)

**Status:** FIRST MILESTONE LANDED (2026-07-07). Single-process, superseding-version `update` /
`delete` generated per model; snapshot-isolated reads across mutation; #62 `Updated`/`Deleted`
events. Deferred items (GC/compaction of superseded versions, M2M `unlink`, concurrent writers,
version chains, field-level patch, cascade delete) remain out — see "Explicitly out". This note
resolved **the retraction-primitive fork** that #56 (MVCC), #57 (backup), #62 (subscriptions), #59
(multi-tenancy L2), and #63 (inspector editor) all defer to.
**Issue:** [#66](https://github.com/hoodiecollin/forgedb/issues/66).
**Date:** 2026-07-07

## What landed (milestone 1)

- **No `forgedb-storage` change was needed** — the retraction is pure generated code over the
  existing append + `id_to_row` machinery. Only substrate change: `forgedb-changefeed`
  `ChangeKind` gains `Updated`/`Deleted` (additive, 0.1.0 → **0.1.1**, published 2026-07-08; the
  scaffold's `= "0.1"` pin resolves it from crates.io — publish gap CLOSED, see CLAUDE.md).
- **Generated per model:** `update(id, record) -> bool` (append a live superseding version, repoint
  the id; no-op false on an absent id) and `delete(id) -> bool` (append a *tombstoned* superseding
  version; no-op false when already absent). Append-only holds — no committed byte is mutated in
  place (guarded by a test asserting the generated code contains no `write_at`/`write_all_at`).
- **Snapshot isolation across mutation** falls out of the #56-A watermark for free: `get_at`/`all_at`
  resolve newest-version-*within-the-watermark* per id (reading the id column across the committed
  prefix), so a snapshot captured *before* an update/delete still resolves the old value. No version
  chains, no `xmin`/`xmax`.
- **#62 integration:** `ChangeKind::Updated`/`Deleted` emitted by the generated `update`/`delete`;
  generated `<Model>Updated`/`<Model>Deleted` typed structs; the WS handler branches on kind and
  streams the matching typed event (a `Deleted` carries the pre-delete row so the deleted record is
  still materializable). `Linked` is skipped for a model subscription.
- **Proof:** `scratchpad/mutation_compile` — compile-tests `database.rs` + `api.rs` through the
  current codegen and runtime-proves: insert→update→get returns the new value; delete→get returns
  `None`; a snapshot taken before an update/delete still sees the old state; `all_at` yields one
  newest version per id (no duplicates); reopen resolves newest-version-wins and keeps deletions;
  integer-PK models get the same surface; and a CLI `backup create → restore → reopen` over a
  mutated dir restores the exact logical state. Guards: `test_rust_generation_mutation_surface`,
  extended `test_rust_generation_snapshot_reads` / `test_rust_generation_changefeed_emits` /
  `test_api_generation_websocket_subscription`, changefeed `mutation_kinds_carry_through_the_feed`.

The original design note follows unchanged.

## Summary

Generated models today are **insert-only**. There is no generated `update` and no generated
`delete`; the storage engine exposes no in-place retraction (`Tombstones` has `append`/`is_deleted`
but no setter), which is exactly why the inspector's editor (#63), MVCC's mutation visibility
(#56-B/C), subscriptions' `Updated`/`Deleted` events (#62-B), and multi-tenancy's write isolation
(#59 L2) are all blocked. This note picks **the one substrate retraction primitive** the whole
mutation surface is built on, because that choice ripples through every append-only-dependent feature
already shipped (backup #57, watermark snapshots #56-A, change notifications #62-A).

**Recommendation: superseding-version append.** `update`/`delete` append a *new version* of a row
rather than mutating committed bytes; reads resolve "latest live version wins." It is the only
option that keeps the append-only invariant the three shipped milestones depend on — and it makes
snapshot-isolated updates/deletes **fall out for free** from the watermark #56-A already built. The
honest cost is storage growth (superseded versions accumulate until compacted) and a latest-version
read path. The alternative — an in-place tombstone/column `set` — is analyzed below and rejected.

## The fork, sharpened

The two candidates were recorded in `mvcc-concurrency.md` as "superseding-version append
(preserves append-only)" vs "in-place tombstone `set` (breaks append-only)." Inspecting the actual
storage code sharpens *why* in-place is worse than the one-line framing suggests — and it is not
mainly about file lengths.

### Candidate A — superseding-version append (RECOMMENDED)

`delete(id)` appends a new row for the same logical id, marked deleted (tombstone byte = 1).
`update(id, record)` appends a new row for the same id with the new field values. The per-id index
(`id_to_row`) points at the **newest** row for that id; `get` resolves that row; a newest-row that
is tombstoned reads as absent.

- **Preserves append-only fully.** Every column and the tombstone file still only ever grow by
  whole rows appended at EOF. So **backup #57** (committed-prefix copy bounded by the tombstone
  anchor), **watermark reads #56-A** (`index < watermark`), and **change notifications #62-A**
  (positional insert = event) keep working *unchanged*. No redesign of shipped code.
- **Snapshot isolation across updates/deletes falls out for free.** A superseding version is just
  another appended row, so it sits *past* an older snapshot's watermark and is invisible to it. A
  reader pinned at watermark N automatically sees the row's state as-of-N — the exact thing #56
  Direction C otherwise needs `xmin`/`xmax` version chains to achieve. `all_at`/`get_at` already
  scan the committed prefix; they need only resolve "newest version *within the watermark*" per id.
- **`#62` update/delete events are natural.** `ChangeKind` gains `Updated`/`Deleted`; the generated
  `update`/`delete` emit them exactly like `insert` emits `Inserted`. Direction B live queries then
  get true add/remove/update deltas because *every* mutation is an appended, positional event.
- **Costs (honest).** (1) Storage grows with every update/delete until compacted — but the
  `compaction` crate already exists and #57's `Manifest` already reserves `compaction_epoch` for
  exactly this GC. (2) The read path must resolve latest-live-version per id (a per-id pointer, or
  newest-wins during the reopen id-scan). (3) `id_to_row` must map id → newest row, and the reopen
  rehydration scan must keep the *last* occurrence of each id, not the first.

### Candidate B — in-place tombstone / column `set` (REJECTED)

`delete(id)` flips the existing tombstone byte in place (`write_all_at(index, &[1])`); `update`
overwrites the row's column bytes in place.

- **It does *not* break backup's anchors — the real breaks are subtler.** A positional tombstone
  flip does not change any file length, so backup's committed-prefix copy and the row-count anchor
  survive byte-wise. The framing "breaks append-only" is imprecise; the actual disqualifiers are:
  1. **Snapshot isolation is broken for mutations.** `read_at` checks `is_deleted` against *live*
     tombstone state, so a snapshot captured before an in-place delete would still observe the
     deletion — a pre-snapshot reader cannot un-see a post-snapshot mutation. #56-A's whole value
     (a snapshot immutable under later writes) is lost the moment mutation is in-place. Recovering
     it requires versioning tombstone state at snapshot time — i.e. reinventing Candidate A or
     jumping straight to Direction C version chains.
  2. **In-place `update` is impossible for variable-length columns.** A `string` whose new value has
     a different length cannot overwrite the old bytes without shifting every following row. Only
     fixed columns could be updated in place; strings would have to append-a-version anyway — so
     in-place buys an asymmetric, half-working `update` that still needs the append machinery.
  3. **It mutates committed bytes under concurrent readers.** A reader mid-`read_at` could observe a
     torn value/tombstone during a Direction B writer's in-place update — the append-only model's
     "committed bytes never change" is what makes lock-free reads safe.
- **Only upside:** no storage growth, no GC. Real, but it buys that by sacrificing snapshot
  isolation and lock-free reads — the properties #56-A and #57 were built to provide.

### Verdict on the fork

Candidate A is the only choice coherent with what already shipped. It converts "snapshot-isolated
mutation" from a hard problem into a property of the watermark we already have, and leaves backup,
snapshots, and the change feed untouched. Candidate B's storage saving is not worth re-opening
three shipped, product-gated milestones.

## Identity verdict & invariant mapping

**Green — the mutation surface stays generated; the retraction primitive stays schema-agnostic
substrate.**

- **Substrate (schema-agnostic).** The version-append + latest-version-resolution mechanism operates
  on **row positions and a per-id newest-row pointer** — it never reads a field or interprets a
  schema. Same class as the tombstone/anchor machinery it extends. Likely just additive methods on
  `forgedb-storage` (a version-aware id index; no new published crate expected).
- **Generated (per-schema).** `update(id, record)` / `delete(id)` are generated per model, tailored
  to that model's columns (they append the model's exact column set, just like the generated
  `insert`). "Latest live version wins" is compiled into the generated `get`/`get_at`/`all_at` read
  path per model — not a runtime interpreter walking arbitrary schemas.
- **The line:** the substrate says "row N supersedes the prior row for this id"; generated code
  decides what a row *is*. A generic runtime that took a schema + a mutation and reflected over
  fields would be the forbidden engine — this is not that.

## First milestone (mutation surface — single-process, superseding-version)

**In scope**
- **Substrate:** a version-aware append + a per-id newest-row resolution (e.g. `id_to_row` keeps the
  newest row; a helper to resolve newest-live-version-within-a-watermark for `get_at`). Additive to
  `forgedb-storage`; append-only preserved.
- **Generated per model:** `update(id, record)` (append new version, repoint id → newest) and
  `delete(id)` (append tombstoned version). Read-path changes are **asymmetric — know which already
  work:**
  - `get`/`get_at` resolve via the per-id pointer; `get` is already newest-live-safe (unique id →
    newest row → `read_at`'s `is_deleted` filters a tombstoned newest). `get_at` must resolve
    newest-*within-watermark* so #56 snapshots see mutation state as-of capture.
  - `all()` (`rust.rs:274`, iterates `id_to_row.keys()` → `get(id)`) is **already** newest-live-safe
    and needs **no change** — which is why the reverse-relation / M2M linear-scan traversal helpers
    (built on `all()`) also need no change. Do not rework it.
  - `all_at()` (`rust.rs:261`) is the one that changes: its raw `0..watermark` physical scan would
    return **both** old and new versions of an updated id (duplicates) — it must resolve
    newest-within-watermark per id instead.
  - Reopen rehydration already keeps last-occurrence per id (see open question #5) — guard, don't rewrite.
- **`#62` integration:** `ChangeKind` gains `Updated`/`Deleted`; generated `update`/`delete` emit
  them; the WS handler streams `<Model>Updated`/`<Model>Deleted` typed events.
- **`#57` integration:** none required — backup still copies the committed prefix; superseded
  versions are ordinary committed rows and are backed up/restored faithfully.
- **Proof:** compile-tested through current codegen; insert → update → get returns the new value;
  delete → get returns `None`; a snapshot captured before an update/delete still sees the OLD state
  (snapshot isolation across mutation, the property Candidate B cannot provide); backup after
  mutations restores the exact logical state. Live-writer stress is Direction B.

**Explicitly out**
- **GC / compaction of superseded versions** — deferred; `compaction` crate + `compaction_epoch`
  (reserved by #57) are the designated home. Until then storage grows; documented honestly.
- **M2M `unlink`** — decide during impl: either a superseding junction-version (same primitive) or
  keep it out of the first milestone (links stay append-only-add). Flag, don't silently drop.
- **Concurrent writers / Direction B writer serialization** — stages after this lands.
- **Direction C version chains / `xmin`/`xmax` / transaction manager** — held.
- **Field-level / partial update** (patch semantics), cascade-delete across relations, any `.forge`
  syntax addition, any generic runtime mutation executor.

**Success = generated `update`/`delete` that append superseding versions, with reads resolving
newest-live-version and #56 snapshots seeing mutation state as-of-capture — backup, watermark reads,
and the change feed all unchanged.** Reaching for in-place byte mutation, version chains, or a
transaction manager to ship this is the drift signal.

## Red lines

- **Preserve append-only.** Committed column/tombstone bytes are never mutated in place; mutation is
  always an append. This is the whole reason Candidate A was chosen — do not "optimize" it back to
  in-place and silently break #56-A/#57.
- **The retraction primitive stays schema-agnostic.** It resolves versions by row position + per-id
  pointer; it never decodes a field or reads the `.forge` schema.
- **Latest-version resolution is generated per model**, compiled into the read path — never a runtime
  interpreter over arbitrary schemas.
- **No generic mutation/query engine**, no predicate-as-data update/delete, no schema-reading runtime.
- **Storage growth is acknowledged, not hidden.** Superseded versions accumulate until compaction;
  say so in generated docs and the CLI, and route GC through the existing `compaction` crate.
- **Snapshot isolation is a feature, not incidental** — `get_at`/`all_at` must resolve
  newest-live-version *within the watermark*, or #56-A regresses.

## Open questions (resolve during impl / design review)

1. **Newest-version resolution mechanism:** id_to_row-keeps-newest (simple, O(1) live reads; the
   reopen scan must keep the *last* occurrence) vs an explicit per-id version list (needed if
   `get_at` must find newest-*within-watermark* efficiently rather than scanning). The `all_at`
   prefix scan already visits every row, so newest-within-watermark for scans is free; point
   `get_at` is the case that may want a version list.
2. **Delete + M2M:** does deleting a model row cascade to / orphan its junction links? First
   milestone likely leaves links (documented), with cascade as a later, explicitly-designed step.
3. **Update granularity:** full-record replace (simplest, matches `insert`'s whole-row append) vs
   field-level patch (needs a partial-column story). Recommend full-record replace for the milestone.
4. **GC trigger + `compaction_epoch`:** when compaction collapses superseded versions it must bump
   `compaction_epoch` (the #57 field) and rebuild the id index; sequence this with the deferred
   incremental-backup chain that also consumes that epoch.
5. **id_to_row reopen semantics — already correct, needs a GUARD not a change.** The rehydration
   scan (`generate_rehydrate_logic`, `rust.rs:1109`) is `for i in 0..n { id_to_row.insert(id, i); }`
   — an overwriting insert in ascending physical order, which **already yields last-occurrence-wins**.
   Under superseding-version append the newest version is the highest physical index for that id, so
   the reopened index already resolves it. Do **not** describe this as a behavior change to write (it
   is working code; "fixing" it would break reopen). The real invariant to protect is *"versions for
   an id are appended in monotonic physical order, so highest index = newest"* — lock it with a
   **reopen-after-update regression test**, not a rewrite.
6. **Change-feed event identity under repositioning (#62 integration detail).** `ChangeEvent` carries
   `(model, row_index, kind)` only. Under superseding append an `Updated`/`Deleted` event references
   the **new** physical `row_index`, not the logical row's original position. A #62 Direction-B
   consumer keying deltas by `row_index` cannot map `Updated{row_index: 57}` back to the logical row
   it supersedes without resolving `57 → id`. `kind` disambiguates insert-vs-update, but logical-row
   identity for live-query removal/replacement needs the id. Resolution stays generated + schema-
   agnostic (the generated WS handler reads the id from the materialized record; the `ChangeEvent`
   struct stays positional), so this is **not** an identity problem — but it is an unresolved
   integration detail to settle when Direction B is designed, not discovered mid-impl.

## Load-bearing references

- `crates/storage/src/lib.rs` — `Tombstones::{append,is_deleted}` (no setter today, line ~760);
  positional `read_exact_at` (why an in-place flip is physically possible but semantically wrong);
  `Snapshot` (#56-A watermark the version model rides on); `Manifest::compaction_epoch` (#57,
  reserved for the GC this note defers).
- `crates/codegen/src/rust.rs` — generated `insert` :~740 (the append template `update`/`delete`
  extend); `read_at` :~222 + its `is_deleted` filter (newest-tombstoned reads as absent); `get`/`all()`
  :274 (**already** newest-live-safe — no change) vs `all_at()` :261 (the one that gains
  newest-within-watermark); `generate_rehydrate_logic` :1109 (id-scan that **already** does
  last-occurrence-wins — guard with a reopen-after-update test, do not rewrite).
- `crates/changefeed/src/lib.rs` — `ChangeKind { Inserted, Linked }` gains `Updated`/`Deleted`.
- `docs/proposals/mvcc-concurrency.md` — Direction B/C this unblocks; the fork framing this note
  resolves.
- `docs/proposals/backup-restore.md` — the committed-prefix model Candidate A leaves intact (and the
  compaction-trap warning this note honors).
- `docs/proposals/realtime-subscriptions.md` — Direction B live-query deltas gated on this surface.
- `docs/proposals/database-inspector.md` — the editor that has "nothing to call" until this lands.
- `CLAUDE.md` → "What ForgeDB is" — the generator invariant + the append-only / no-update-or-delete
  limit this note lifts.
