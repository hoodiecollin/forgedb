# Proposal: Persistence Reopen (generated `open()` + per-model layout manifest)

**Status:** PARTIALLY IMPLEMENTED (2026-07-07). Product-gated (`forgedb-product-manager`: **APPROVED, identity-pure**). Recommended shape: **hybrid — tombstone-file length is the authoritative row-count anchor; a per-model manifest carries physical layout metadata only, written at open/schema-change/compaction, never per-insert.**
- **LANDED (step 0):** generated `*Storage::new()` + junction `new()` rehydrate from the tombstone anchor; compaction + stats derive `row_count` from the tombstone-file length instead of a manifest they required but codegen never wrote. Verified: insert→reopen→read round-trips (models + M2M) and `compact` works against a manifest-less generated DB.
- **FOLDED INTO #57 — NOW IMPLEMENTED (2026-07-07):** the per-model manifest emission + the substrate `Manifest` additive fields shipped with the #57 backup milestone (its first real reader). Generated `*Storage::new()` (and junction `new()`) now emit `<dir>/manifest.json` via `generate_write_manifest`; the substrate `Manifest` gained `compaction_epoch`/`format_version`/`row_anchor` + `ColumnMetadata { value_size, kind, relative_path }` + `ColumnKind`/`RowAnchor` + `Manifest::save_to/load_from`. Reopen/compaction still anchor purely on the tombstone/right-column length (consume no manifest); backup consumes the manifest for column widths + the `row_anchor`. Guarded by `test_rust_generation_layout_manifest`; compile-proven in `scratchpad/manifest_compile`.
**Issue:** [#65](https://github.com/hoodiecollin/forgedb/issues/65) (`bug`, `tech-debt`, `plan-next`)
**Date:** 2026-07-07

## Summary

**Generated ForgeDB databases cannot survive a process restart** — proven empirically:
process A inserts 3 rows (data written durably to the columnar files) → process B over the
*same directory* reads **0/3** rows. The generated per-model `*Storage` struct has only
`new()` (starts `row_count: 0`, empty `id_to_row: HashMap<Id, usize>`); there is **no generated
`open()` that rehydrates the in-memory identity index from the persisted columns**. The columns
persist and grow correctly (appends seek to true EOF), but the index is never rebuilt, so a
fresh process reads nothing and the next `insert` computes `row_index = self.row_count = 0`
while columns append at real EOF — silently corrupting the id→row mapping.

Two findings make this **systemic, not isolated**:

1. **Generated code writes no manifest at all** (`grep crates/codegen` = zero references). It
   does not use the substrate `Database`/`Manifest`; it manages per-model `*Storage` structs
   with hardcoded CWD-relative column paths and an in-memory `row_count`.
2. **Compaction already requires a per-model `<model_dir>/manifest.json` with `row_count`**
   (`crates/compaction/src/compactor.rs:28-50` hard-errors without it). Because codegen never
   emits it, **compaction is already broken against real generated DBs** — same root cause.

So a **per-model manifest convention is half-built**: reopen, compaction, backup (#57), and the
inspector (#63) all assume `<model>/manifest.json` exists; codegen simply never emits it. A
generator that emits `insert` but not `open` is **incomplete generation**, not a boundary
question.

## Identity verdict & the red line

**Inside the invariant — not a close call.** A generated `open()` that rehydrates `row_count`
and `id_to_row` is **schema-tailored generated code** — the mirror image of the already-generated
`insert`/`get`, reading *this* model's *named* columns with compiled-in `value_size` constants.
Nothing interprets a schema at runtime. The per-model manifest is **schema-agnostic substrate
metadata** — the same category as the existing `Manifest`/`ColumnMetadata`/`ColumnType`
(`crates/storage/src/lib.rs:170-202`).

**The red line (sharp), because a schema-blind backup (#57) and inspector (#63) will read this
manifest:** the manifest may carry **physical layout metadata only** — facts about *bytes on
disk*, derivable by inspecting the files themselves:

- **MAY carry:** `row_count` / row-count anchor, per-column `{ column_index, ColumnType,
  value_size, kind: fixed|variable, relative_path }`, `compaction_epoch`, an opaque
  `schema_version` integer/hash, `format_version`.
- **MUST NOT carry:** anything that lets a reader *reconstruct application data logic* — no
  `.forge` field semantics, no directive payloads (`@min`/`@computed`/`@email`), no relation
  graph (FK target models, junction wiring, cardinality), no validation rules, no API routes, no
  computed expressions.

**The discriminator, as a test:** *the manifest describes the storage substrate's physical shape
(columns, sizes, row counts, epochs) — never the application's semantic shape (fields, relations,
constraints, routes).* `ColumnType::Uuid` is a physical width-and-encoding fact (fine); "this
Uuid is a FK to `User.id`" is schema logic (forbidden in the manifest). A tool reading only the
former stays schema-blind. This keeps the inspector's "render rows as a table" honest (physical)
and blocks drift into "traverse relations" (semantic) via the manifest.

## Recommended shape — HYBRID (tombstone anchor + layout manifest)

**Chosen over per-insert manifest-as-commit-anchor.** The tombstone file length is the
authoritative live-row-count anchor; the manifest carries layout metadata + `compaction_epoch`
and is written at `open()` / schema-change / compaction — **never on the insert hot path.**

**Why not per-insert manifest (rejected).** Writing a durable manifest "N committed" via
`fsync`+rename per insert, while column bytes for row N are still only in the OS page cache
(`FixedColumn` appends are page-cache-only, `lib.rs:206-212`), is a **durability inversion**: a
crash can leave a manifest durably claiming N rows over columns that only reached N−2 — the
anchor *overstates* committed data (strictly worse than understating), so `open()` rebuilds
`id_to_row` for rows whose bytes are gone and the next insert writes over a hole. Safe only with
a full per-insert multi-file fsync barrier — the fsync storm the engine deliberately avoids.

**Why the tombstone length is the right anchor.** `tombstones.bin` is 1 byte/row, appended
**last** in each insert (`rust.rs:558`, after all column appends), append-only. Its length *is*
the count of physically-committed rows; because it's written last, a present tombstone byte for
row K implies all of row K's column appends preceded it in program order. Therefore:

- `open()` sets `row_count = tombstones.len()`, then rebuilds `id_to_row` by scanning the id
  column over `[0, row_count)`.
- A **torn insert** (columns written, process died before the tombstone append) is correctly
  **excluded** — the anchor undercounts by exactly the torn row, which self-heals: the next
  insert overwrites the orphaned column tails at `row_index = row_count`. This is the *safe*
  direction of inequality.
- **Compaction** changes from reading `manifest.row_count` (`compactor.rs:96`) to deriving
  `row_count` from the tombstone-file length, then updates the manifest's `compaction_epoch`.
  This removes its hard dependency on a file codegen never wrote — fixing compaction against
  generated DBs in the same stroke.

**The one hardening.** Because the tombstone length is the anchor, the generated flush discipline
must fsync **columns before tombstones** at durable commit boundaries (WAL checkpoint, backup),
so the anchor never outruns the data it certifies. Encode that ordering in the generator and the
codegen compile-harness. The manifest itself is written via the existing atomic temp+fsync+rename
(`lib.rs:748-762`) — rare, cheap, off the hot path.

**What the downstream notes actually need — served better by the hybrid.** Backup (#57) needs a
coherent watermark N + per-column byte lengths: N from the tombstone length (or the manifest's
cached copy), widths from the manifest layout. Snapshot reads (#56-A) pin a `row_count`
watermark — the tombstone length *is* that watermark. The inspector (#63) needs column layout
(types/widths/paths) to render rows — the manifest's layout section. **None needs per-insert
manifest durability; all need stable layout metadata + a row-count anchor.**

## Co-design with the mutation / retraction fork

The anchor = **tombstone-file length (physical row count)** is **neutral to both branches** of the
retraction fork named in `mvcc-concurrency.md:90-102`:

- **Superseding-version append** (append-only-preserving): compatible as-is. `row_count` stays
  "physical rows appended"; "latest-live-version-wins" resolves via a per-id version pointer
  *above* the anchor. No manifest change required. This is the branch backup + snapshot already
  lean on.
- **In-place tombstone `set`** (append-only-breaking): tombstone *length* still equals physical
  row count (fine as anchor), but tombstone *contents* stop being write-once, so backup's
  prefix-copy invariant breaks for the tombstone file — which, per the mvcc note's own carried
  constraint, **must redesign backup + Direction A in the same change.**

**Guidance:** keep the manifest **minimal — do NOT bake version pointers in now** (machinery
without a consumer = the `fulltext`/`crud-api` failure mode). Instead add **`format_version` and
`compaction_epoch` now** so that when the mutation surface lands, a `visibility_model` /
version-pointer descriptor slots in as an additive `#[serde(default)]` field and old manifests
self-describe as "v1, no versions." The manifest anticipates the fork by being **versioned and
additive**, not by pre-building version state (same discipline `backup-restore.md:119-123`
prescribes for `compaction_epoch`).

## Red lines

- **The manifest carries physical layout only** (columns/sizes/paths/row-count/epoch/versions) —
  never `.forge` semantics, relations, directives, validation, or routes. A backup/inspector
  reading it must stay schema-blind.
- **No per-insert manifest fsync** and no durability inversion — the tombstone length is the
  anchor; the manifest is written at open/schema-change/compaction only.
- **`open()` is generated per-model code**, not a shipped generic "database opener" that reflects
  over a schema. It reads *this model's named columns* with compiled-in constants.
- **Preserve the append-only tombstone anchor**, or break it consciously and loudly (redesign
  backup + #56-A in the same change) — the retraction fork's constraint.
- **`.forge` never becomes a runtime input** to `open()`/rehydrate/manifest — the manifest is
  emitted at *generation* time; the reopen logic is *generated*, not schema-interpreting.

## First milestone

**Landed (step 0)**
- Generated per-model rehydration on each `*Storage::new()` (which already opened the existing
  column files — so fixing `new()` corrects every call site with no API break): set
  `row_count = tombstones.len()`, rebuild `id_to_row` by reading the id column `[0, row_count)`.
  Junction `new()` rehydrates `row_count` from `right_col.len()` (the last-appended column) so
  M2M links survive a restart.
- Compaction + stats derive `row_count` from the tombstone-file length (`crates/compaction`),
  dropping the hard dependency on a manifest `row_count` codegen never wrote; the post-compaction
  manifest refresh is now best-effort (no-op when absent).
- Regression guard `test_rust_generation_reopen_rehydration` (codegen) locks the emitted shape;
  runtime proof is the insert→reopen→read compile-and-run harness (models, integer PK, and M2M).

**Folded into #57 (deferred to its first consumer)**
- Emit a per-model `<model>/manifest.json` (substrate `Manifest` + `ColumnMetadata`) describing
  physical columns (index, `ColumnType`, value_size, kind, relative path) + `compaction_epoch` +
  `format_version`, written at open/schema-change via the atomic temp+fsync+rename path.
- Add `compaction_epoch` + `format_version` to the substrate `Manifest`, additive
  `#[serde(default)]` for on-disk back-compat. `storage` is published (0.1.2) — additive minor
  bump.
- Generated flush discipline: fsync columns before tombstones at durable commit boundaries (only
  meaningful once backup / a WAL checkpoint provides such a boundary — none exists in the
  generated flow today, so it lands with #57).

**Explicitly out**
- Any mutation surface (`update`/`delete`), version chains, or `visibility_model` manifest field
  — deferred to the mutation-surface note; the manifest only stays *ready* for them (additive).
- Per-insert manifest durability / manifest-as-commit-anchor (rejected — durability inversion).
- `Database::open_at(root)` root-parameterization (the #59 CWD-relative wart) — orthogonal;
  reopen works within a consistent CWD. Combine later if convenient.
- WAL auto-replay on open (still the caller's job; unchanged).

**Success = insert rows in one process, reopen in a fresh process via generated `open()`, and
read back exactly those rows — with the row count anchored on the tombstone file and the manifest
describing only physical layout.** Needing a per-insert manifest fsync, or schema knowledge to
reopen, is the drift signal.

## Sequencing

**Step 0 — precedes #57 and #56-A.**
- **#57 backup is unobservable without reopen** — its milestone proof is "restore to a new dir,
  **open with generated code**, verify the rows"; today that reads 0 rows regardless of backup
  correctness.
- **#56-A snapshot is unbuildable without the anchor** — it pins a durable `row_count` watermark
  that today resets to 0 every process.
- **Compaction is already broken** against generated DBs for the same root cause — this also
  unblocks the shipped `compact` command.
- **Resolves `backup-restore.md` risk #2** ("is `manifest.row_count` the commit anchor? verify in
  codegen") by construction: anchor = tombstone length, ordering = tombstone-last, proven in the
  compile-harness.

Land it **minimal, versioned, fork-neutral**; then #56-A watermark snapshots and #57 backup build
on the now-durable anchor; then the mutation surface / retraction fork extends it additively.

## Load-bearing references

- `crates/codegen/src/rust.rs:338-389` (`generate_storage_inits` — where `open()` belongs;
  `row_count: 0` / `id_to_row: HashMap::new()` hardcoded at `:382-383`), `:442-566`
  (`generate_insert_logic`; tombstone appended last at `:558`, `row_index = self.row_count` at
  `:551`). Zero references to substrate `Database`/`Manifest`.
- `crates/storage/src/lib.rs:170-202` (`Manifest`/`ColumnMetadata`/`ColumnType` — the
  schema-agnostic layout descriptor to emit), `:206-212` (`FixedColumn` page-cache-only appends —
  the inversion argument), `:658-682` (`Database::open`), `:743-762` (atomic `save_manifest`).
- `crates/compaction/src/compactor.rs:28-50` (`read_manifest_row_count` — the hard dependency to
  replace with tombstone-derived count), `:96,105` (row_count usage).
- `docs/proposals/backup-restore.md:119-123` (additive-manifest `compaction_epoch` discipline),
  `:157-160` (risk #2 = this task).
- `docs/proposals/mvcc-concurrency.md:90-102` (retraction fork + append-only-preservation
  constraint the anchor must honor).
- `CLAUDE.md` → "What ForgeDB is" — the invariant this note is measured against.
