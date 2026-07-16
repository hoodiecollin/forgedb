# Implementation Plan: Schema Migrations e2e (#74)

**Status:** **COMPLETE — Phases 1–4 LANDED (Phase 4: 2026-07-16).** The whole #74 spine (version
guard → hop classification/lineage → offline transformer bin → one-CLI workflow + per-tenant sweep)
is shipped.
**Product gate:** `forgedb-product-manager` verdict **ALIGNED-WITH-CONSTRAINTS** on the whole system
(2026-07-15), re-gated after the **single-bin unification** refinement (the transformer bin encodes the
WHOLE migration — one operator action). The unification is *cleaner, not riskier* — it deletes the runtime
dispatcher C8/DV-3 had to fence. Amended constraint set: **C3 retired**, **C8 strengthened**, **C10 →
C10′** (vacuous under uniform-only), **DV-3 re-pointed**, **DV-10/DV-11 added**; C1/C2/C4/C5/C6/C7/C11/C12
and DV-1/DV-6/DV-7/DV-9 carry over. Primary guard-test targets: **DV-1**, **DV-6**, **C8/DV-3**.
**Supersedes/consolidates:** the phased roadmaps in
[`schema-migrations.md`](./schema-migrations.md) (red lines #1–#8) and
[`data-transform-migrations.md`](./data-transform-migrations.md) (constraints 1–14). Those two notes
remain the authoritative *design*; this note is the grounded *implementation plan* that ties them
together under the locked two-bin decision and maps each PM constraint to a guard test.

---

## 1. The locked architecture (recap)

Two binaries. The schema is a compile-time input to the generation of **both** — neither is a generic
runtime engine.

- **App bin** — generated `database.rs`/`api.rs`, compiled against exactly ONE schema version. On open
  it does one thing with the manifest version field: compare `manifest.format_version` against a
  codegen-baked opaque `EXPECTED_FORMAT_VERSION` (lineage-derived) and **fail-fast refuse** on
  mismatch. Links no migration crate, no transformer, no provider. Never adapts.
- **Transformer bin** — an offline, one-shot dev/ops binary generated for a **specific origin→destination
  range**. Because schema versions are serial/incremented, the sequence between origin and destination is
  deterministic; the generated bin embeds **only the schema versions in that sequence** (a B→G transformer
  embeds exactly B, C, D, E, F, G — not the whole history) as generated typed structs, plus the frozen
  per-hop transforms that bridge them, and **replays that sequence** to migrate a data dir from origin to
  destination. Origin/destination are **generation-time inputs** (`forgedb generate transform --from B
  --to G`), so the embedded set is fixed at codegen time and a different range is a different generated
  bin. Compiled once, run against data-at-rest. Never shipped with, or linked into, the app.

**One operator action.** The transformer bin encodes the WHOLE migration — trivial hops, additive adds,
backfills, drops/renames, AND semantic rewrites — as one generated straight-line replay. There is **no
separate `migrate up` byte-op tool**; the operator performs exactly one action: *run the migration bin*.
The app bin is unchanged (still just refuses on version mismatch — DV-6).

**Default mechanism: uniform typed replay.** Every hop is emitted as a typed transform — read each row via
the version-N structs, produce the version-(N+1) record, write via the version-(N+1) writer. Additive
adds, backfills, drops, and renames all fall out as trivial transform bodies (copy fields; set the new
field's default; omit a field; map a name); semantic hops (split/merge/compute) carry a
developer-authored (or LLM-drafted-and-confirmed) body. This is **identity-neutral** (both a cheap byte-op
and a typed re-encode are tailored generated code — re-gate Q5) and it dissolves the buggy
direct-column-manipulation surface the old `executor.rs` carried. A cheap, manifest-driven **in-place
byte-op** path (for O(rows)-wasteful bulk drop/rename on huge tables) is a **guard-fenced optimization
deferred past v1** — under uniform-only, C10′/DV-10 are vacuous.

**Per-hop body classification (frozen at `migrate create`, C8/C9).** A hop whose new-row body the differ
can PROVE from the diff (additive / constant / structural — set default, omit, rename) gets an
**auto-generated** transform body; a hop needing semantic understanding (split / merge / compute) is
**scaffolded for the developer** to author and freeze. This is a dev-time codegen classification baked
into the emitted hop — the bin has **no runtime mechanism-selection site** and never inspects target data
to decide a hop's behavior (C8, strengthened).

**Multi-version gaps** (A→G) replay sequentially through the range's intermediate versions (never a
synthesized direct jump) — the bin walks its embedded version sequence in order over a temp dir, stamps
each intermediate version atomically, and **atomic-renames once at the end** (all-or-nothing at the range
level; the retained source dir is the rollback). #92's generated reopen-backfill demotes to a **safety
net** — the bin produces a fully-materialized destination-version dir, so the app opens it with a matching
version and needs no backfill.

---

## 2. Binding constraints → guard tests

Every constraint below becomes an assertion in a codegen guard or a crate unit test. This table is the
acceptance contract for the whole feature.

| # | Constraint | Guard test |
|---|---|---|
| **C1** | Transformer's embedded version set is a closed compile-fixed **range** (origin→destination), fixed by the `--from`/`--to` **generation** inputs; the bin embeds only that serial sequence and constructs no decoder from runtime input. No `.forge` parse, no `Schema`/`SimpleSchema` in its runtime path. | `test_transform_bin_has_no_schema_runtime` (grep emitted transformer src: no `.forge` read, no `forgedb_parser`/`SimpleSchema` symbol in the replay path) |
| **C2** | Replay is a generated straight-line / `match`-dispatched chain of named typed hop fns — not a loop interpreting a persisted plan descriptor. | `test_transform_bin_replay_is_straightline` (assert emitted `run(from,to)` is a `match`/call-chain, no `Vec<Step>` interpreter) |
| ~~**C3**~~ | **RETIRED** by unification — its referent (a separate schema-blind byte-op tool that must never touch embedded schemas) no longer exists. Stale-offset intent → C10′; schema-isolation intent → DV-10. | — |
| **C10′** | *(only if an in-place byte-op hop is ever emitted — vacuous under uniform-only v1)* a byte-op hop resolves physical layout from the target dir's live `ColumnMetadata` at apply time; offsets are NOT frozen into the emitted hop. | `test_byteop_hop_resolves_layout_from_manifest` |
| **C4 / DV-7** | Transformer links no provider crate and no `forgedb-migrations` crate at runtime; provider-free. | `test_transform_bin_deps_are_provider_free` (assert emitted `Cargo.toml` closure) |
| **C5 / C11** | Per-hop `format_version` interlock preserved across the whole replay; each hop stamps its version atomically with its byte write; a crash mid-replay leaves a real, refusable intermediate version. `compaction_epoch`/`format_version` verified before apply; a compaction-reshaped dir is refused, not mis-decoded. | `test_migrate_up_version_interlock` + crash-between-hops e2e |
| **C6 / DV-7** | Transformer is generated per origin→destination range (never hand-edited); bridging a new range means generating a new bin, not editing an existing one. | codegen determinism: `forgedb generate transform --from B --to G` twice → byte-identical; no hand-maintained dispatch table |
| **C7** | Authoring surface stays "schema + CLI + config"; multi-version embedding invisible to the app author. | doc + review gate (no `.forge` knob for versions) |
| **C8 / DV-3** (strengthened) | Each hop's behavior is frozen at `migrate create` time and BAKED into the emitted hop fn; the bin has **no runtime mechanism-selection site** (not merely "a dispatcher reads a frozen class" — no dispatcher exists). | `test_transform_bin_replay_has_no_dispatch_branch` (`run(from,to)` is a named-hop call-chain; no class-dispatch/`match change.class` branch over a persisted plan) |
| **C9 / DV-4** | A hop whose body the differ cannot PROVE structural/constant MUST be authored as a typed rewrite body (not auto-derived); the auto-body path only handles provable additive/constant/structural. Dev-time classification, never runtime. | `test_diff_classifies_authored_body_residue` |
| **C12** | Both offline paths honor the `DirLock` single-writer contract; migration is exclusive-writer/offline; no live app/coordinator writer against the dir. | `test_migrate_up_acquires_dirlock` |
| **C13** | The one-bin transformer is generated FROM the committed per-hop transform sources — consolidation replaces per-hop *invocation*, never per-hop *authoring/review/freeze*. | the transformer generator reads `migrations/{id}/transform.rs` sources; guard asserts it does not re-synthesize them |
| **DV-1** (fatal) | Transformer never becomes a generic multi-schema replay engine. | = C1 + C2 + C6 |
| **DV-6** (fatal) | App-open refuses, never self-heals a version mismatch. | `test_rust_generation_version_guard` (generated open reads one opaque int and refuses; never reads column names/types to reshape) |
| **DV-9** | The transformer bin never becomes an app dependency. | `test_app_closure_excludes_migration` (app `Cargo.toml` closure has no transformer/migration/`forgedb-migrations` crate) |
| **DV-10** (new) | A byte-op hop resolves layout from an embedded `v_n::` struct instead of the manifest's `ColumnMetadata` (stale-offset risk + schema-at-runtime whiff). Vacuous under uniform-only. | `test_byteop_hop_reads_manifest_not_struct` |
| **DV-11** (new) | The consolidated chain grows a generic `Vec<HopDescriptor>` interpreter to DRY the repetitive hops. Already forbidden by C2; unification raises the temptation. | = C2 guard (`test_transform_bin_replay_is_straightline`) |

---

## 3. Current state (grounded, 2026-07-15)

What exists, what is stubbed, what a prior epoch already fixed. File:line from the current tree.

**Already correct / better than the notes claimed:**
- **`compaction_epoch` is ALREADY preserved on reopen.** `crates/codegen/src/rust.rs:2232-2234` (model)
  and `:6985` (junction) load the existing manifest and carry `compaction_epoch` forward. The design
  note's "hard prerequisite" listed BOTH fields as clobbered — that is now **half-resolved**: only
  `format_version` still needs the same treatment. **This shrinks Phase 1.**
- **`--auto` diff gate is wired** (`src/commands/migrate.rs:28-114`): `detect_schema_changes()`
  (`:385-407`) diffs current `.forge` vs `.schema-snapshot.forge`, splits breaking/additive, refuses
  breaking with dump→regenerate→reload guidance. Issue #74's original gap #1 is closed (#92 W3).
- **`SchemaDiffer::diff()`** (`crates/migrations/src/diff.rs:43-122`) already produces the full
  `SchemaChange` set (16 variants incl. renames via heuristics, constraint-param changes), deterministic
  (BTreeMap-ordered).
- **`SchemaChange::is_breaking()`** (`crates/migrations/src/types.rs:94-115`) already classifies
  breaking vs additive — the seed of the C8 mechanism classifier.
- **Compaction byte-op primitives exist and are schema-blind:** `compact_model_keeping(keep)`
  (`crates/compaction/src/compactor.rs:131-152`), `stage_fixed_column_write` (`:446-478`),
  `stage_variable_column_write` (`:376-437`), `stage_tombstones_write` (`:487-502`) — atomic
  `.tmp`+rename. **These are the byte-op rewrite primitives Phase 2/3 reuse.**
- **`Manifest` carries the schema-blind layout surface:** `format_version` (`storage-native/src/lib.rs:173`),
  `compaction_epoch`, `ColumnMetadata { name, column_type, column_index, value_size, kind, relative_path }`
  (`:248-267`), `RowAnchor` (`:189-195`). Enough to resolve layout without `.forge`.
- **WasmGenerator is the template** for emitting a new compiled binary: `generate()` →
  `replica/src/lib.rs`, `cargo_toml_scaffold()` written only-if-absent, wired in
  `src/commands/generate/mod.rs` (opt-in in `generate all`). The transformer generator follows this shape.

**Broken / stubbed:**
- **`executor.rs::add_column`** (`:224-265`): writes an empty placeholder file named
  `{subdir}/{type}_{name}.bin` and does NOT backfill. **Two bugs:** (a) filename convention mismatch —
  codegen writes/reads `fixed/{type}_{idx}.bin` and variable `variable/string_data_{idx}.bin`, so the
  placeholder is orphaned garbage the app never reads; (b) no backfill to `row_count`.
- **`executor.rs` byte-op stubs return "not yet supported":** `remove_column` (`:268-279`),
  `change_column_type` (`:282-294`), `create_index` (`:297-308`), `drop_index` (`:311-321`),
  `add_unique_constraint` (`:324-334`), `remove_unique_constraint` (`:337-347`),
  `create_composite_index` (`:350-361`), `drop_composite_index` (`:364-375`).
- **No version guard anywhere.** `format_version` is hardcoded `1` on write (`rust.rs:2245`, `:6986`);
  generated `open_at`/`__open_with_lock` (`:4941-5020`) **never reads it**. No `EXPECTED_FORMAT_VERSION`
  const is emitted (grep-confirmed). Today a stale data dir silently mis-decodes on reopen.

**The #92 reconciliation (resolved by unification):** #92 W2 backfills additive columns at generated
*reopen* time (recovery anchors on the tombstone count and backfills short columns with the field
default). Under the unified transformer bin the migration produces a **fully-materialized**
destination-version dir, so the app opens it with a matching version and needs no backfill — **#92's
reopen-backfill demotes to a defense-in-depth safety net** (for the regenerate-and-reopen-without-
migrating case), not the mechanism. The buggy/stubbed `executor.rs` byte-op surface above is **superseded
and deleted** (Phase 3), not repaired — uniform typed replay absorbs additive/backfill/drop/rename as
trivial typed hop bodies.

---

## 4. Phased implementation

Dependency order. Each phase is independently shippable and testable. **First milestone = Phase 1 alone**
(the version guard — smallest useful slice: turns today's silent stale-data mis-decode into a fail-fast
refuse and lands the `format_version` prerequisite everything downstream needs; app-side, no migration
mechanism required). Under **uniform typed replay** the old Phase 0 byte-op bugfixes are moot — the
`executor.rs` direct-column-manipulation surface (the `{type}_{name}.bin` filename bug, the
non-backfilling `add_column`, the eight "not yet supported" stubs) is **superseded and deleted in Phase
3**, not repaired.

### Phase 1 — Version guard + `format_version` writer preservation (the prerequisite) — **LANDED 2026-07-15**

**Goal:** the app refuses stale data instead of mis-decoding (DV-6); `format_version` stops being
clobbered on reopen (C11 prerequisite).

**Landed.** All three steps below shipped in `crates/codegen/src/rust.rs`:
- `write_manifest` (model **and** junction) now loads any existing manifest and preserves its
  `format_version` (same load-preserve pattern as `compaction_epoch`), baselining a fresh dir to
  `EXPECTED_FORMAT_VERSION` — no more hardcoded `format_version: 1` clobber on reopen.
- An opaque `const EXPECTED_FORMAT_VERSION: u32 = 1;` is emitted per schema (baseline 1 until the
  Phase 2 lineage wires the derivation).
- `__open_with_lock` runs a per-collection guard BEFORE opening any column/WAL file: for every
  model/junction manifest that exists it loads exactly `format_version` and **panics fail-fast** on
  mismatch (never reads column shape — DV-6). A fresh, never-written dir is skipped.

Guards `test_rust_generation_version_guard` + `test_rust_generation_manifest_preserves_format_version`
(codegen suite 72 tests, workspace 490, 0 fail). E2E (throwaway crate, path-dep compile): write at v1 →
reopen OK → externally bump manifest to v2 → reopen **refused** with migration guidance. **No substrate
change, no publish gap** (`Manifest.format_version` already published).

1. **Preserve `format_version` on reopen.** `rust.rs:2245` (model) + `:6986` (junction): replace the
   hardcoded `format_version: 1` with the same load-existing-manifest-and-preserve pattern already used
   for `compaction_epoch` at `:2232-2234`. Fresh dir → baseline (the codegen-baked version); reopen →
   preserve existing.
2. **Emit `EXPECTED_FORMAT_VERSION`.** codegen emits an opaque `const EXPECTED_FORMAT_VERSION: u32 = N;`
   where N is **derived from the migration lineage** (red line #8), NOT hand-edited. Source of N: the
   count/hash of applied version-bumping migrations recorded in the snapshot lineage. For the first
   milestone (no migrations authored yet) N=1; the wiring to lineage lands with Phase 2.
3. **Read + refuse on open.** Generated `open_at`/`__open_with_lock` (`:4941-5020`): after loading the
   manifest, compare `manifest.format_version` to `EXPECTED_FORMAT_VERSION`; on mismatch return a
   fail-fast error ("data dir is at format vN, this binary expects vM — run the migration bin").
   Read exactly one opaque integer; never inspect column names/types (DV-6).

**Guards:** `test_rust_generation_version_guard` (open reads one int + refuses; no column-shape read);
`test_rust_generation_manifest_preserves_format_version` (reopen does not clobber). E2E in a throwaway
crate: write at vN, bump manifest to vN+1, reopen → refused.

### Phase 2 — Hop classification + migration lineage (feeds create & the version guard) — **LANDED 2026-07-16**

**Goal:** the `migrate create`-time machinery that freezes each hop's body and records the lineage — the
dev-time inputs the transformer generator (Phase 3) and `EXPECTED_FORMAT_VERSION` (Phase 1) consume. No
runtime mechanism.

**Landed.** All three steps shipped in `crates/migrations/` + the `generate` CLI wiring:
- **Hop-body classifier (C8/C9):** `HopBodyClass::{Auto, Authored}` + `SchemaChange::hop_body_class()`
  (`crates/migrations/src/types.rs`). It is **distinct from `is_breaking()`**: only the residue the differ
  genuinely cannot prove a value for is `Authored` (`ChangeFieldType`, nullable→NOT-NULL narrowing,
  required-add-without-default); breaking-but-provable changes (drop field/model, `&unique` add) stay `Auto`.
  `Migration::authored_changes()` exposes the residue.
- **Migration lineage (`crates/migrations/src/lineage.rs`):** `Migration` gains serial
  `from_version`/`to_version` (`#[serde(default)]`, checksum-covered); `MigrationLineage::load` orders the
  records; `current_format_version()` (highest `to_version`, baseline 1), `next_version_span()`, and
  `expand_range(from, to)` (the ordered contiguous hop subsequence a `--from B --to G` transformer walks —
  refuses a non-contiguous/out-of-range span, never synthesizes a jump — C1). `scaffold_authored_body`
  writes `migrations/{id}/transform.rs` for authored residue **only if absent** (never clobbers a frozen
  body — C13).
- **Wiring:** `migrate create --auto` stamps each additive migration with its version span (records
  `format v1 → v2`); `EXPECTED_FORMAT_VERSION` is now **lineage-sourced** (red line #8) — the `generate` CLI
  threads `MigrationLineage::current_format_version("migrations")` into
  `RustGenerator::generate_with_format_version` (`generate(&schema)` still baselines to 1 so snapshot tests
  are byte-stable).

Guards `test_diff_classifies_authored_body_residue`, `test_migration_lineage_expands_range`,
`test_scaffold_authored_body_only_for_residue` (migrations crate) + the extended
`test_rust_generation_version_guard` (asserts the version threads, not hardcoded). E2E (CLI): baseline →
`EXPECTED_FORMAT_VERSION = 1`; add a nullable field + `migrate create` → migration records `v1→v2`;
regenerate → `EXPECTED_FORMAT_VERSION = 2`. **No substrate change, no publish gap.** **Deferred to Phase
3:** the breaking-change path still refuses with the reload guidance (the `--auto` gate is unchanged) —
flipping it to *scaffold-and-record authored bodies* lands when the transformer bin exists to run them; the
scaffolder is a tested library capability the Phase 3 CLI will invoke.

1. **Per-hop body classification, frozen at create (C8/C9).** Extend `migrate create` so each
   `SchemaChange` is tagged `AutoBody` (the differ can PROVE the new-row body — additive/constant/
   structural: set default, omit field, rename) or `AuthoredBody` (semantic: split/merge/compute). Built
   on the existing `is_breaking()` + a "provable structural/constant" predicate over `SchemaDiffer`
   output. `AuthoredBody` hops scaffold a `migrations/{id}/transform.rs` for the developer to author +
   freeze (data-transform-migrations.md constraints 1/9); `AutoBody` hops need no authoring.
2. **Migration lineage / snapshot infra.** The committed snapshot lineage (`migrations/.schema-snapshot.*`
   + per-migration records) becomes the ordered source of truth the transformer generator walks to expand
   a `--from B --to G` range into B..G and pull each hop's frozen body (C13). Records the version-bumping
   lineage that `EXPECTED_FORMAT_VERSION` (Phase 1) derives from (red line #8).
3. **DirLock note (C12).** Migration is offline/exclusive-writer; the running of the bin (Phase 3)
   acquires `forgedb_storage::DirLock` (or refuses a live writer). Recorded here as a cross-phase invariant.

**Guards:** `test_diff_classifies_authored_body_residue` (C9 — unprovable ⇒ `AuthoredBody`),
`test_migration_lineage_expands_range` (B→G expands to the correct ordered sequence + frozen bodies).

### Phase 3 — The transformer bin (the WHOLE migration, uniform typed replay) — **LANDED 2026-07-16**

**Goal:** a new `TransformGenerator` (WasmGenerator-shaped) emits the per-range bin that encodes the whole
migration — every hop (additive, backfill, drop, rename, semantic rewrite) as a **uniform typed transform**
— and **deletes** the superseded `executor.rs` byte-op surface. This is `data-transform-migrations.md`
realized, aggregated per range.

**Landed.** `crates/codegen/src/transform.rs` (`TransformGenerator`, joining Rust/TS/Api/Stub/OpenApi/Wasm)
emits the per-range crate: `Cargo.toml` (provider-free — no `forgedb-parser`, no `forgedb-migrations`),
one `src/vN.rs` per version in the range (each a `RustGenerator::generate_with_format_version(schema, N)`
emission, so its open-guard enforces the version interlock), an embedded `src/authored_<id>.rs` per
`Authored` hop (frozen verbatim — C13), and `src/main.rs` = a **fixed straight-line** chain of named
`transform_vN_to_vM` hop fns (no `Vec<Step>` interpreter, no runtime dispatch — C2/C8/DV-11). Each hop reads
every row via the `vN` typed structs, applies the frozen structural JSON ops (rename/remove/additive-add
baked from the diff — no schema at runtime, C1) then the authored transform, decodes into the `vM` struct,
and writes via `vM`'s `insert` (which preserves the record's id). Multi-hop ranges replay through
intermediate temp dirs and **atomic-rename once at the end** (all-or-nothing; retained source = rollback).
Per-version schemas are self-describing: `migrate create` now snapshots each version's full `.forge` under
`migrations/schemas/vN.forge`, which the generator loads for the range. The buggy/stubbed `executor.rs`
byte-op surface is **DELETED** (its two workflow tests too). CLI: `forgedb generate transform --from F
--to T`, `forgedb migrate build --from F --to T` (generate + `cargo build`), `forgedb migrate run --src
--dest` (run the built bin); `migrate up`/`down` (the old executor path) removed. Guards (the identity
trio + more): `test_transform_bin_has_no_schema_runtime` (C1), `test_transform_bin_replay_is_straightline`
(C2/C8/DV-11), `test_transform_bin_deps_are_provider_free` (C4/DV-7),
`test_transform_bin_embeds_frozen_authored_body` (C13), `test_transform_generation_snapshot`. **E2E**
(`scratchpad/transform_e2e`, ephemeral — compile-test discipline): a real 3-version lineage (v1→v2 additive
`bio: string?` = `AutoBody`; v2→v3 `age` u32→string = `AuthoredBody`) → `forgedb generate transform
--from 1 --to 3` → **the emitted crate compiles** (all 3 version modules + hop code + substrate) → seed 2
v1 users → run the bin → reopen at v3: `age` re-encoded to a string, `bio` additive-defaulted to `None`,
ids preserved, **source dir retained**; feeding a v3 dir to the v1-expecting bin is **refused** by the
open-guard (interlock). **No substrate change, no publish gap.** **Deferred to Phase 4:** flipping
`migrate create --auto`'s breaking gate to scaffold-and-record authored bodies (today the E2E authored
lineage is recorded via the tested `MigrationGenerator`/`scaffold_authored_body` library path); the
one-CLI `migrate up --bin` orchestration; per-tenant sweep; `compaction_epoch` verification before apply
(the format-version guard is the interlock today); the cheap in-place byte-op optimization (C10′/DV-10
vacuous under uniform-only).

1. **`crates/codegen/src/transform.rs` — `TransformGenerator`** (new generator, joining
   Rust/TypeScript/Api/Stub/OpenApi/Wasm). Takes origin/destination as **generation inputs**
   (`forgedb generate transform --from B --to G`); reads the committed snapshot lineage (Phase 2) to
   expand the range into its deterministic serial sequence (B..G) and pull the frozen `AuthoredBody`
   sources (C13). Emits `transform/` = `Cargo.toml` (only-if-absent, provider-free closure — C4/DV-7),
   `src/main.rs` (the CLI runs the fixed embedded sequence over a src→dest data dir; no runtime schema
   selection — C1), and one generated module per version **in the range only** (`v_b/`, `v_c/`, … `v_g/`),
   each a `RustGenerator` emission of that version's typed structs + readers/writers.
2. **Uniform typed replay (C2/C8).** Emit `run(from, to)` as a straight-line / `match` call-chain of
   named hop fns `transform_b_to_c`, `transform_c_to_d`, …, each hop = **read every row via `v_N` structs
   → produce the `v_{N+1}` record → write via `v_{N+1}` writer**. `AutoBody` hops get an auto-generated
   body (copy fields; set new field default; omit; rename); `AuthoredBody` hops embed the committed
   `migrations/{id}/transform.rs` (C13 — generated FROM the frozen source, not re-synthesized). **No
   runtime dispatch branch, no `Vec<Step>` interpreter** (C8/DV-3/DV-11). The bin never resolves layout
   from an embedded struct for an in-place byte-op because there are no in-place byte-ops in v1 (DV-10
   vacuous).
3. **Per-hop version interlock across replay (C5/C11).** Each hop writes to a temp dir via #91 `create_*`
   wrappers and stamps `format_version` atomically; the range **atomic-renames once at the end**
   (all-or-nothing at the range level; retained source dir = rollback). A crash mid-replay leaves either
   the pristine source or a cleanly-versioned intermediate the app refuses (DV-6). Verifies incoming
   `format_version`/`compaction_epoch` == the expected start-of-range before running (C11) — a
   compaction-reshaped dir is refused, not mis-decoded.
4. **Delete the superseded byte-op executor.** Remove `executor.rs`'s direct-column-manipulation path (the
   `add_column` placeholder + the eight "not yet supported" stubs, `crates/migrations/src/executor.rs`
   `:224-375`) — the transformer's uniform typed hops replace it. Keep the diff/snapshot/tracker
   machinery.
5. **Wire into the CLI** — `forgedb generate transform` in `src/commands/generate/mod.rs` (opt-in in
   `generate all`); `forgedb migrate build --from B --to G` (generate + `cargo build` the bin) and
   `forgedb migrate run <src> <dest>` (or the `forgedb-transform` bin runs standalone). One operator
   action: run the migration bin.

**Guards (the identity trio — DV-1):** `test_transform_bin_has_no_schema_runtime` (C1 — no `.forge`
parse, no `Schema`/`SimpleSchema` in the runtime path; embedded `v_n::` typed structs are fine),
`test_transform_bin_replay_is_straightline` (C2/C8/DV-3/DV-11 — named-hop call-chain, no dispatch branch,
no persisted-plan interpreter), `test_transform_bin_deps_are_provider_free` (C4/DV-7),
`test_transform_generation_snapshot`. E2E (compile-test discipline — snapshot pass ≠ compiles): generate a
3-version lineage mixing an additive-add (`AutoBody`) and a `u32→string` type-change (`AuthoredBody`),
throwaway-crate compile of the emitted transformer, run A→C over a backup dir, reopen at vC through
generated code — values transformed, additive column defaulted, ids preserved, source dir intact.

### Phase 4 — Workflow unification + per-tenant sweep — **LANDED 2026-07-16**

**Goal:** one coherent operator workflow driven entirely through `forgedb migrate`; multi-tenant rolling
migration.

**What landed:** (1) The `migrate create --auto` breaking gate is **flipped** — every non-empty diff is
recorded as a versioned hop and classified `Auto`/`Authored` (via `hop_body_class()`); `Authored` residue
is scaffolded at `migrations/{id}/transform.rs` (never clobbering a frozen body). Purely-additive keeps the
reopen-backfill fast path; anything that rewrites data-at-rest prints the `migrate up` next steps. The old
refuse-with-reload-guidance path is gone. (2) **`forgedb migrate up`** is the one-CLI lifecycle: it resolves
the range (`--from` defaults to the version detected from the source manifests, `--to` to the lineage's
current version), builds the transformer once, and runs it over `--src`/`--dest` or every data dir under
`--tenant-root` (rolling sweep — a failed/wrong-version tenant is reported and skipped, source unchanged,
non-zero exit if any failed). The lower-level `migrate build`/`migrate run`/`generate transform` stay as
primitives. (3) `docs/MIGRATIONS.md` rewritten around the one lifecycle; the migrations-crate module doc
de-references the deleted `MigrationExecutor`. Guard: integration test
`test_migrate_auto_records_and_scaffolds_authored_hop`. **Deferred (documented in `MIGRATIONS.md`):**
`compaction_epoch` verification before apply (C11 — the format-version guard is the interlock today);
`@default` on additive backfill; cheap in-place byte-op hops; online (live-writer) migration.

1. **One CLI for the whole lifecycle.** `forgedb migrate` orchestrates every step so the operator never
   hand-invokes the generated bin: `migrate create --auto` (classify + scaffold any `AuthoredBody`) →
   author/freeze authored bodies → `migrate build --from B --to G` (generate + `cargo build` the range's
   bin) → **`migrate up --bin <path> <src> <dest>`** (the CLI invokes the built bin with the right flags +
   env; acquires/relays the `DirLock` semantics; surfaces its exit status) → app reopen at the new
   version. Documented as one flow in `docs/MIGRATIONS.md`. This recovers the "single CLI for the whole
   migration lifecycle" DX — the two-bin split stays an implementation detail, not an operator burden.
2. **Per-tenant sweep (#59):** `migrate up` runs the bin per `<root>/<tenant>` dir independently (each
   tenant process still does its own opaque `format_version` compare). Rolling sweep, no cross-tenant
   coordination (deferred online migration stays deferred).

---

## 5. Cross-subsystem interactions (from the gate)

- **Backup/restore (#57):** reused as the pre-migration snapshot + rollback net; transformer reads via
  #56-B `reader()` handles, writes via #91 `create_*` wrappers. Retained original dir = rollback.
- **Compaction epoch (#92) — sharpest interaction (C11):** `compact()` renumbers rows + bumps
  `compaction_epoch`. The migration bin must verify the incoming dir's version/epoch matches the
  snapshot's expected start-of-range and refuse a compaction-reshaped dir rather than mis-decode. The
  `compaction_epoch` this feature bumps must agree with #76 incremental-backup epoch semantics (DV-8).
- **Multi-tenancy (#59):** independent per-tenant dirs; sequential sweep (Phase 4).
- **DirLock / MVCC (#89/#56-B/#75) (C12):** migration is offline/exclusive-writer; the migration bin
  acquires the `DirLock` (or refuses a live writer); the MVCC coordinator does not participate. Online
  migration deferred.

---

## 6. Deferred / out of scope

- **Cheap manifest-driven in-place byte-op hops** (drop/rename without an O(rows) typed rewrite) — a
  perf optimization over uniform typed replay for bulk changes on huge tables; deferred past v1, and only
  then does C10′/DV-10 become non-vacuous.
- Direct A→G composition (pure-hop optimization) — stays deferred; if it lands it must be materialized
  generated code, never a runtime fold over hop descriptors (DV-3).
- Online / zero-downtime migration (both notes defer it; C12 assumes exclusive writer).
- Timestamp→version lookup; a runtime migration engine of any kind.
- Provider-authored transforms at replay time — authoring happens at `migrate create`, frozen (C13).
- **A bespoke remote-migration receiver/agent — NOT built.** A persistent daemon on remote hosts that
  receives/streams the migration bin and executes it was considered and rejected for v1 (and likely
  beyond) on **scope/security** grounds (not identity — a receiver streaming an opaque bin is class-2
  transport glue, not a schema interpreter). Three reasons: (a) the single-writer contract (C12) means a
  remote migration is an orchestrated `stop-app → migrate → start-app` sequence, i.e. deployment
  orchestration — not "run a bin"; (b) it reinvents what Kubernetes **Jobs** / `docker run --rm` / systemd
  `Type=oneshot` / Ansible / CI already do, with worse auth/logging/rollout; (c) a daemon that downloads +
  executes streamed binaries is an RCE-by-design surface needing signing/authz/sandboxing — a whole
  deployment-agent product, not a migration feature. **Blessed remote pattern instead** (document in
  `docs/DEPLOYMENT.md` + `docs/MIGRATIONS.md`): CI builds the range's bin → ship as an artifact / bake into
  the image → run as a one-shot Job / `docker run --rm` / systemd `oneshot` against the data volume with
  the app stopped (drops straight into the #115 deploy paths). **Future option if unified remote DX is
  ever wanted:** have `forgedb migrate up --target ssh://host | k8s://job/... | docker://...` shell out to
  the operator's EXISTING transport (a thin orchestrator over trusted tools), never a bespoke agent. Even
  this is deferred; it is the buy-not-build shape of the remote idea.
