# Proposal: MVCC / Concurrency

**Status:** **Direction A (watermark snapshot reads) AND Direction B (single-writer + concurrent
readers) — both LANDED.** A landed 2026-07-07; the mutation surface (#66) landed; **B landed 2026-07-08.**
The rest (Direction C) remains DESIGN NOTE — product-gated. `forgedb-product-manager` verdict:
**aligned-with-constraints, but re-sequenced**. Maintainer-blessed direction (2026-07-06): **D-framing —
ship A now, mutation surface next, stage B, hold C**; **single-process** scope. Direction A shipped:
substrate `forgedb_storage::Snapshot` (bare `usize` watermark) + generated
`*Storage::{read_at,row_count,snapshot,get_at,all_at}`, junction `pairs_at`, `DatabaseSnapshot` +
`Database::snapshot()`, and one snapshot-scoped M2M traversal `post_tags_at`. **Direction B shipped
(2026-07-08, PM identity gate PASS):** substrate read-only column reader handles
(`FixedColumnReader`/`VariableColumnReader`/`TombstonesReader`, shared-fd positional `&self` reads →
`forgedb-storage` **0.1.4**) + generated `*StorageReader` / junction `*Reader` / `DatabaseReader` bundle +
`Database::reader()`, reusing the *exact* `read_at`/`get_at`/`all_at`/`pairs_at`/M2M-`_at` token streams
(no second decode path).  `Database::snapshot()` capture is routed through the single writer, so the
`DatabaseSnapshot` is cross-model atomic.  Proven by a **live concurrent-writer stress test** (the thing A
deferred): one writer thread running insert/update/delete while N reader threads read via
`DatabaseReader` + a published snapshot, asserting cross-model atomicity + torn-row-free prefixes
(`scratchpad/directionb_compile`, ephemeral); guarded by `test_rust_generation_reader_handles` + substrate
`test_reader_*`. **Next: Direction C is DEFERRED** (no in-codebase caller — see below).
**Issue:** [#56](https://github.com/hoodiecollin/forgedb/issues/56) (`idea`, `plan-next`)
**Date:** 2026-07-06

## Summary

MVCC passes the identity guard — versioning/visibility over rows and transaction IDs is
schema-agnostic engine machinery, the same category as the published `forgedb-storage`/
`forgedb-wal` substrate crates. **But identity is not the binding constraint here; sequencing
is.** MVCC is concurrency control *over mutation*, and ForgeDB has **no mutation surface yet**:
generated `database.rs` has `insert`/`get`/traversal but no `update`/`delete`, and the storage
`Tombstones` are write-once with no in-place setter (`crates/codegen/src/rust.rs:884-886`,
`crates/storage/src/lib.rs:588-640`). Full MVCC would be machinery with no caller.

Meanwhile the foundational property of "snapshot isolation; concurrent readers/writers" —
**consistent lock-free reads under a concurrent writer — is ~80% free already**: append-only +
the `row_count` manifest anchor *is* read snapshot isolation (the exact insight the backup note
leans on). Per-row version chains buy *concurrent writers* and *mutation visibility*, which are
precisely the parts with no in-codebase consumer today.

**Blessed decision:** #56 is **not rejected — it is re-sequenced.**
- **Ship A now** (watermark snapshot reads) — cheap, independently useful, already-blessed category.
- **Build the generated mutation surface next** (`update`/`delete` + one substrate retraction
  primitive), **co-designed with #63**'s editor — this is the real gating prerequisite.
- **Stage B** (single-writer + concurrent readers) on top of the mutation surface.
- **Defer C** (full version-chain MVCC) until an in-codebase caller needs concurrent writers — it
  is reachable by extending B, so building it before B has a writer-contention consumer is
  machinery without a caller.
- **Scope: single-process.** Multi-process is a separately-justified future expansion.

## Identity verdict & invariant mapping

**Inside the guard.** A version index, visibility metadata, and a GC loop know nothing about any
`.forge` model — they operate over rows, columns, and transaction IDs. Generated `database.rs`
would *call* snapshot/transaction APIs the way it already calls `append_uuid`; it embeds no
generic schema-reading engine. No `.forge` file becomes a runtime input.

| Concern | Verdict |
|---|---|
| Does MVCC make `.forge` a runtime input to a generic engine? | **No** — versioning/visibility is schema-agnostic substrate; the tailored per-model read/write stays generated. |
| Is it a legitimate published-runtime class? | **Yes** — Class-1 substrate, same category as `forgedb-storage`/`forgedb-wal`. |
| Is identity the binding constraint? | **No — sequencing is.** It passes the invariant, then runs into "concurrency control for a mutation surface that doesn't exist." |

**The one drift vector to name:** if MVCC ever arrives bundled with a **generic
transaction/query executor** that takes predicates-as-data and interprets them against arbitrary
tables at runtime, *that* crosses the line. MVCC the storage mechanism is fine; MVCC as a
Trojan horse for a runtime query engine is not. Visibility filtering stays per-generated-model in
`database.rs`; the substrate only answers "is version V visible to snapshot S."

## The re-sequencing (blessed framing)

The load-bearing fact: **MVCC is concurrency control over mutation, and there is no mutation to
control.** So the honest unit order is:

1. **A — watermark snapshot reads** (now). Independently valuable; strict subset of B and C.
2. **Mutation surface** (`update`/`delete` + retraction primitive) — the real prerequisite,
   shared with #63's editor. Design once, and MVCC becomes an increment rather than greenfield.
3. **B — single-writer + concurrent readers** (stage on top of mutation).
4. **C — full MVCC** (deferred; reachable by extending B, no in-codebase caller yet).

## Direction A — watermark snapshot reads (ships now)

**What it builds.** A `Snapshot` / `ReadView` handle in `forgedb-storage` = a captured
`row_count` watermark (the manifest anchor). Reads through a snapshot clamp `index < N`; writers
appending past N are invisible. The row's *position* past the watermark **is** its invisibility —
no `xmin`/`xmax`, no version index. Generated `database.rs` gains `db.snapshot()` returning a read
view whose `get`/`all`/traversal observe only rows ≤ watermark.

**Identity fit.** Clean green — pure substrate mechanism the generated code calls; identical
category to the backup note's watermark, already blessed there.

**Scope (files/concepts).** `crates/storage/src/lib.rs` (a `Snapshot`/`ReadView` type + clamped
read paths), `crates/codegen/src/rust.rs` (emit `snapshot()` + snapshot-scoped getters).
Concepts: watermark, prefix-stable reads. Small; no new crate.

**Unlocks.** Consistent concurrent reads under a live appending writer; a natural foundation for
the inspector (#63 read-side) and for consistent multi-table reads.

**Forecloses.** Nothing — it's a strict subset of B and C. It does not give snapshot isolation
*across deletes/updates*, which is exactly why it's fine now (there are none yet).

## Prerequisite — the generated mutation surface (next unit, co-designed with #63)

MVCC's payload (concurrent writers, mutation visibility) needs `update`/`delete` to exist first.
That surface is the shared prerequisite with #63's data editor. It requires **one substrate
retraction primitive** — and choosing it is **the fork that ripples everywhere**, because the
append-only watermark model (backup *and* Direction A) depends on "append-only under an atomic
manifest anchor."

**Retraction primitive — DECISION DEFERRED to the mutation-surface note** (maintainer choice).
The two candidates, recorded here so the fork is explicit:

- **Superseding-version append** — update/delete appends a new version; reads resolve "latest
  live version wins" via a per-id version pointer. **Preserves append-only**, so backup +
  watermark snapshots stay coherent. Costs a per-id version-pointer read path.
- **In-place tombstone `set`** — add `Tombstones::set(index, true)`. Minimal change, but mutates
  existing bytes, **breaking append-only purity** → backup's lock-free prefix-copy and Direction
  A's watermark reads must be redesigned in the same change.

**Constraint carried forward:** whichever is chosen, if it breaks append-only it must redesign
backup (`docs/proposals/backup-restore.md`) and Direction A **in the same change** — do not
discover the breakage later. (The backup note already flags compaction as exactly this trap.)

## Direction B — single-writer + concurrent snapshot readers — LANDED 2026-07-08

Serialize writes behind one writer (a lock/writer-handle discipline), keep many `&self` snapshot
readers (from A). This is SQLite WAL-mode's model — one writer, many readers — well-trodden and
honest. Readers pinned to a pre-mutation watermark still see old state; new readers see committed
mutations. Green (substrate + generated calls; the single-writer serialization is a runtime
discipline of the generated binary, not a generic engine). Deliberately forecloses concurrent
*writers* — a defensible cut most embedded workloads never need.

**What landed (PM identity gate PASS).**
- **Substrate (`forgedb-storage` 0.1.4, class-1):** read-only column reader handles
  `FixedColumnReader` / `VariableColumnReader` / `TombstonesReader`, each holding an independent
  (`try_clone`d) fd to the *same* file as the writer column, exposing only positional `&self`
  `read_*`/`len`.  They know strictly *less* than any other substrate (byte offsets + value sizes;
  no model, no field). `FixedColumn::reader()` / `VariableColumn::reader()` / `Tombstones::reader()`
  open them.  Length is derived **live** (never a cached count); an out-of-range index maps to the
  same `InvalidInput` the writer columns return.
- **Generated code:** each `*Storage` gets `reader() -> *StorageReader` (a struct of shared-fd
  reader columns) that re-emits the **exact same** `read_at`/`get_at`/`all_at` token streams as the
  writer storage — one tailored decode path, not two.  Junctions get `*Reader` + `pairs_at`.  The
  `Database` gets a `DatabaseReader` bundle (one **typed, named** reader field per model AND junction
  — never a runtime string-keyed dispatch) + `Database::reader()`, plus the snapshot-scoped M2M
  `_at` traversal on `DatabaseReader`.
- **Cross-model atomicity (the correctness red line the PM named):** `DatabaseSnapshot` capture is
  routed through the single writer (`Database::snapshot()` is called on the writer, never
  mid-mutation), so the per-collection watermarks are captured as of one commit boundary.  Readers
  consume that immutable snapshot; N independent watermark reads are **not** made atomic on the
  reader side.
- **Proof:** `scratchpad/directionb_compile` (ephemeral) — a live concurrent-writer stress test (the
  thing A explicitly deferred): a single writer thread runs insert/update/delete + M2M link per
  commit and publishes a fresh `db.snapshot()` between commits; 4 reader threads each hold a
  `DatabaseReader` and read via the published snapshot, asserting `post==tag==link` counts
  (cross-model atomicity) and that every decoded row's columns agree (torn-row freedom) — no lock
  held anywhere.  Plus reader snapshot-isolation-across-update and integer-PK reader reads.  Substrate
  guards: `test_reader_*` (incl. the load-bearing fd/page-cache coherence invariant + a lock-free
  read-under-live-append stress test).  Codegen guard: `test_rust_generation_reader_handles`.

**Honest limits / deferred.** Single-process, **single-writer** (concurrent *writers* foreclosed →
Direction C).  Snapshot atomicity holds *because* capture is serialized through the one writer — it
is not a multi-writer guarantee.  The fd-duplication + POSIX page-cache-coherence-across-`try_clone`d-fds
assumption is load-bearing for reader visibility (tested explicitly, documented as a substrate
invariant).  No version chains / transaction manager (Direction C).

## Direction C — full version-chain MVCC (deferred; no in-codebase caller)

> **Designed 2026-07-14 → [`multi-writer-mvcc.md`](./multi-writer-mvcc.md)** (issue #75, PM verdict
> PASS-WITH-CONSTRAINTS). That note supersedes the sketch below: it reframes C as **transactions +
> concurrent writers in three tiers** and finds that **Tiers 1–2 need no on-disk `xmin`/`xmax`
> engine** (serialized commit keeps the watermark valid; conflict detection is in-memory). Tier 1
> (transactions, rollback-by-`truncate_to_rows`) is the recommended first milestone with **zero new
> substrate**. The heavyweight version-chain engine sketched here is only the Tier-3 / segmented-
> column horizon. Read that note for the current design.

Per-row `xmin`/`xmax` visibility, a transaction manager handing out txn IDs + snapshots,
UPDATE = append-new-version + stamp-old-`xmax`, reads walk version chains filtering by snapshot,
a GC/vacuum loop. Unlocks true concurrent writers + full snapshot isolation across updates/
deletes. **Deferred** because: it commits ForgeDB to a heavyweight storage-engine trajectory and an
**on-disk format break**; it has the strongest "generic engine" gravity (guard-with-care — a
transaction manager + visibility filter must stay a mechanism the generated code drives, never a
generic executor); and **every part of C is reachable later by extending B**. Building C's
machinery before the mutation surface and real writer contention exist is exactly the
machinery-without-consumers failure that killed the orphaned `fulltext`/`crud-api` crates in
Phase 3b. If C is ever built, its transaction component is a Class-1 substrate crate (e.g.
`forgedb-txn`, rows/columns/txn-ids only) — the moment it knows a model name, the boundary is
violated.

## Red lines

- **No generic runtime query/transaction executor.** MVCC ships as substrate *mechanism* the
  generated code calls. A predicate-as-data interpreter running arbitrary queries against
  arbitrary tables at runtime is the forbidden generic engine. Visibility filtering stays
  per-generated-model; the substrate only answers "is version V visible to snapshot S."
- **`.forge` never becomes a runtime input** to any transaction/MVCC path.
- **No `db.transaction()` generic-runtime creep** into a shipped library that reconstructs schema
  logic. Generated transaction/snapshot methods on the generated `Database` are fine (tailored
  code); a shipped generic crate offering runtime transactions over an arbitrary schema is not.
- **Preserve the append-only watermark model, or break it consciously and loudly** — redesigning
  backup + Direction A in the same change (see the retraction fork).
- **Authoring surface stays "schema + CLI + config."** Isolation level / GC policy, if
  configurable, live in `forgedb.toml` or are generated defaults — never new `.forge` syntax.
- **Don't build C's machinery without a caller** (no version chains / GC until the mutation
  surface exists and some in-codebase path actually needs concurrent writers — no generated code or
  crate links a version-chain path today).
- **Single-process scope** (blessed). Multi-process concurrency (on-disk locking/visibility) is a
  separately-justified expansion, not a silent scope creep.

## First milestone (Direction A only) — LANDED 2026-07-07

**In scope** (all delivered)
- ✅ `Snapshot` in `forgedb-storage`: captures a `row_count` watermark; clamps reads to `index < N`
  via `visible(index)`. Schema-agnostic bare `usize`; no version metadata.
- ✅ Generated `Database::snapshot()` (→ `DatabaseSnapshot`, one watermark per model + junction) +
  snapshot-scoped `get_at`/`all_at` on each `*Storage`, `pairs_at` on each junction, and one
  snapshot-scoped traversal `post_tags_at` in `database.rs`. Shared `read_at(row_index)` funnels
  `get`/`get_at`/`all_at` through a single column-layout-aware read path.
- **Proof:** `scratchpad/snapshot_compile` (ephemeral) generates through the *current* codegen,
  compiles it, then captures a `db.snapshot()` after N committed rows, appends more rows + links
  (including a post-snapshot link on a pre-snapshot post), and asserts the snapshot view returns
  **exactly** the N-row prefix — across models AND the junction — while the live view sees all
  writes, and that the snapshot is immutable under further appends. This proves the watermark
  exclusion deterministically; a live concurrent-writer stress test is deferred to Direction B
  (single-writer serialization), where cross-process/thread timing is actually in scope.

**Explicitly out**
- **All mutation** (`update`/`delete`) — the next unit, co-designed with #63; blocked on the
  retraction-primitive decision.
- **Direction B** (single-writer serialization) — stages after the mutation surface.
- **Direction C** (version chains, `xmin`/`xmax`, `forgedb-txn`, GC) — deferred; reachable by
  extending B, no in-codebase caller and it commits to an on-disk format break.
- **Multi-process concurrency**; any new `.forge` syntax; any generic transaction executor.

**Success = a snapshot pinned mid-writes yields consistent lock-free reads with zero version
machinery, purely from the append-only watermark.** Reaching for version chains or a transaction
executor to ship this is the drift signal.

## Open questions carried forward (for the mutation-surface / B notes)

1. **Retraction primitive** — superseding-version append vs in-place tombstone `set`. THE fork;
   decide before any mutation code (determines whether backup + Direction A stay coherent).
2. **Commit boundary & durability.** WAL exists but has no auto-replay and no timestamp/LSN. Is a
   commit = manifest rename (current de facto anchor), or = WAL fsync + replay-on-open? MVCC needs
   a defined commit point.
3. **Who coordinates transactions** — generated `database.rs` driving snapshot/commit calls into
   `forgedb-storage` (preferred for A/B) vs a new `forgedb-txn` substrate crate (needed only for
   C). A generic runtime transaction engine that reads schema is rejected outright.
4. **Snapshot span** — once mutation exists, does isolation need to span deletes/updates (B: single
   writer) or full concurrent-writer isolation (C)? Only matters after mutation lands.

## Load-bearing references

- `crates/storage/src/lib.rs:588-640` — `Tombstones` append-only, no in-place setter; `:743`
  atomic `save_manifest` (the `row_count` watermark Direction A pins); append-only fixed/variable
  columns (positional `&self` reads are already concurrent-safe).
- `crates/codegen/src/rust.rs:558` — generated `insert` appends tombstone `false` once; `:884-886`
  — the rationale that there is no `delete`/`unlink` because the engine exposes no in-place
  retraction. This is why MVCC has no mutation to control today.
- `crates/wal/src/lib.rs` — WAL exists (`replay_committed`), no auto-replay, no LSN/timestamp;
  bounds the commit-boundary question.
- `docs/proposals/backup-restore.md` — the watermark = read-snapshot-isolation insight Direction A
  reuses, and the append-only-preservation constraint the retraction fork must honor.
- `CLAUDE.md` → "What ForgeDB is" — the invariant, plus the append-only / no-update-or-delete
  storage-model limits this note inherits.
