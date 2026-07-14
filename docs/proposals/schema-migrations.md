# Proposal: Schema Migrations

**Status:** DESIGN NOTE — product-gated. `forgedb-product-manager` verdict: **PASS (all 6 items),
conditional on the red lines + one hard prerequisite** (the manifest version-field-preservation fix).
Awaiting approval to schedule Phase 0.
**Issue:** [#74](https://github.com/hoodiecollin/forgedb/issues/74) (`idea`, `tech-debt`)
**Date:** 2026-07-09

## Summary

A schema change in ForgeDB is fundamentally a **regeneration event**, not a runtime alter: because
`.forge` is a compile-time input to codegen, changing it means *edit `.forge` → regenerate
`database.rs`/`api.rs`/`types.ts` → recompile*. The generated code changes shape. Migrations own only
the **other half** — evolving the existing column files at rest so old data survives under the new
generated code. Those two halves (regenerate-the-code, evolve-the-data) exist as parts today but are
**not composed into one workflow**, and the data-evolution half is mostly stubbed.

The design:

- **`migrate create`** parses the current `.forge` at **dev time** (same class as `validate`/`generate`),
  diffs it against a **committed schema snapshot** of the last-migrated state, and writes a **frozen,
  human-reviewable migration file** — the existing `migrations/{id}_{desc}.json` format (a logical
  `Vec<SchemaChange>` + checksum). It also updates the snapshot. `create` is a pure function of
  `(snapshot, current .forge) → (migration, new snapshot)`.
- **`generate`** regenerates code; codegen emits an opaque `EXPECTED_FORMAT_VERSION` constant.
- **`migrate up`** applies the frozen migration **schema-blind**: it resolves each logical change to
  physical column-file operations from the **target data dir's `manifest.json`** (never from `.forge`),
  reusing `crates/compaction`'s existing schema-blind rewrite primitives for rewrite-class changes. It
  bumps `format_version`/`compaction_epoch` in the same atomic commit as the byte rewrite.
- **On open**, the generated app compares `manifest.format_version` to `EXPECTED_FORMAT_VERSION` and
  **fail-fast refuses** stale data — it never adapts to it. The app links **no** migration crate.

This turns migrations from greenfield into an **increment**: the diff/plan/track layer exists, the
serialized migration format exists, and — the key research finding — `crates/compaction` already ships a
**real, schema-blind column-rewrite primitive** (`compact_model` → `stage_fixed/variable/tombstones_column_write`,
atomic temp+rename over the row-count + tombstone bitmap + column files). Rewrite-class migrations reuse
it rather than reinventing.

## Identity verdict & the single drift vector

**Inside the guard.** Every runtime artifact stays schema-blind, and `.forge` stays a compile-time input.
The dividing lines the PM gate blessed:

- **`migrate create` reading `.forge` is dev-time tooling** — the same class as `validate`/`generate`.
  The invariant is about the *shipped app*, not the CLI. Once written, the migration file is frozen and
  `.forge` is out of the loop for that migration forever (same posture as `generate` emitting
  `database.rs`).
- **`migrate up` (apply) is schema-blind, manifest-driven** — Class-1 byte operations over
  `(row-count, tombstone bitmap, fixed/variable/tombstones files, ColumnMetadata)`, exactly like
  compaction/backup. It never touches `.forge`.
- **The generated app's only use of the manifest on open is an opaque integer compare** — `format_version`
  is a file-format magic number, not schema semantics. The app refuses stale data; it never adapts.
- **The generated app links no migration crate** — migration is a dev/ops CLI action, a peer to
  `migrate`/`compact`/`backup`. The app's failure mode on mismatch is **refuse**, not **adapt**. This is
  the strongest identity property in the design: an app that *adapted* at runtime would be reconstructing
  schema-evolution logic at runtime — a violation; refusing is a `u32` compare.

**The single drift vector (sharp):** a shipped runtime that *applies an arbitrary schema delta by reading
`.forge` (or the schema-shaped manifest fields) at runtime and reshaping decoding behavior*. The
guard-passing mirror image: apply resolves layout from the **manifest's physical `ColumnMetadata`** at
apply time (never `.forge`), and the app's open-time guard reads exactly one opaque integer. The instant
apply re-parses `.forge`, or the open-time guard reads column names/types to *self-heal* rather than
*refuse*, it has crossed the line.

## The two halves (and why they're separate)

| Half | Owner | What it does |
|---|---|---|
| **Evolve the code** | codegen (`forgedb generate`) | Regenerates `database.rs`/`api.rs`/`types.ts` to the new schema shape; emits `EXPECTED_FORMAT_VERSION`. |
| **Evolve the data** | migrations CLI (`forgedb migrate up`) | Rewrites the column files at rest so existing rows survive under the regenerated code. Schema-blind, manifest-driven. |

The **version guard** is the interlock: regenerated code (new `EXPECTED_FORMAT_VERSION`) refuses a data
dir whose `manifest.format_version` hasn't caught up — forcing the two halves to be applied as a matched
pair, instead of silently mis-reading bytes.

## The frozen migration file + the committed schema snapshot

- The migration file stays **logical**: the existing `Vec<SchemaChange>` + checksum, human-reviewable
  intent — **not** frozen physical byte-offsets (see next section for why).
- `migrate create` diffs against a **committed schema snapshot** — a serialized `SimpleSchema` at
  `migrations/.schema-snapshot.json` — and rewrites it on each `create`. This makes `create` deterministic
  and reviewable (a pure function of snapshot + current `.forge`), and — load-bearing — means **apply never
  needs `.forge`**: the checksum is validated against the snapshot the migration was authored against, never
  by re-parsing `.forge`.

## Apply is schema-blind (manifest-driven, compaction-class)

`migrate up` resolves each logical `SchemaChange` to physical column operations using the **target's
`manifest.json`** — its `ColumnMetadata {name, column_type, column_index, value_size, kind, relative_path}`
is the ground truth for on-disk layout. Physical operations are **NOT frozen into the migration file**,
because:

- **The manifest is ground truth; the migration file is not.** A migration authored against schema-state S
  may be applied to a dir a prior migration or a compaction already reshaped (different `compaction_epoch`,
  relocated `relative_path`, changed `value_size`). Frozen offsets go stale; the manifest is always current.
- **It preserves the compaction analogy exactly** — compaction resolves layout from the manifest at run
  time; apply should too. Same primitive, same source of truth.
- The migration file stays at the reviewable logical level, not brittle byte math.

Rewrite-class changes reuse compaction's `stage_fixed_column_write` / `stage_variable_column_write` /
`stage_tombstones_write` (atomic temp+rename). The bump to `format_version`/`compaction_epoch` lands in the
**same atomic manifest commit** as the byte rewrite — no window where evolved data carries a stale version.

## The version guard (closes a real correctness hole)

Confirmed against code: the generated `open`/`new_at` **writes** the manifest but **never reads
`format_version` on reopen** — so regenerated code against un-migrated data silently mis-decodes bytes.
The guard: codegen emits an opaque `EXPECTED_FORMAT_VERSION` constant (sourced from the **migration
lineage**, not a hand-edited literal, so it can't drift), and generated open reads the target manifest and
**fail-fast refuses** if `manifest.format_version != EXPECTED_FORMAT_VERSION`. Generated-per-schema,
compile-time, integer-compare — identity-safe, and it closes the hole.

## What the append-only columnar layout makes cheap vs. rewrite

Columns are independent per-index files keyed by row position, which the current stubbed executor doesn't
exploit:

| Change | Physical cost | Mechanism |
|---|---|---|
| Add nullable/defaulted field | cheap | new column file **backfilled to N rows** with the default/null tag |
| Remove field | cheap | drop/orphan that column file; other columns stay position-aligned |
| Rename field | ~free | usually **pure codegen** (column filenames are `{type}_{index}`, not name-keyed) |
| Rename model | cheap | rename the model dir |
| **Change field type** | **rewrite** | full column rewrite via compaction's `stage_*_column_write` + `format_version` bump |
| Add NOT NULL w/o default (populated) | rewrite/refuse | needs a supplied default, else refuse |
| Add unique constraint | validate | scan for existing dups, else refuse |

## Hard prerequisite (blocks any version-bumping migration)

**The generated manifest writer must preserve existing version fields.** Today codegen hardcodes
`compaction_epoch: 0` / `format_version: 1` on **every** manifest write, including on reopen
(`crates/codegen/src/rust.rs` ~943 and ~1861; the FUTURE-TRAP comment already sits at ~881–886). Since the
writer runs on every `open`/`new_at`, a reopen after a migration bumped those fields would **clobber them
back to baseline** — silently destroying the bump and defeating the open-time guard (the app would reset
the very field it guards on). Before `migrate up` may bump versions, the generated writer must, on reopen,
**read the existing manifest and preserve its `compaction_epoch`/`format_version`** (writing the hardcoded
baseline only when creating a fresh data dir). This is a narrow, additive generated-code change and stays
inside identity — the writer re-emits opaque version integers, never schema semantics.

## Red lines

1. **`migrate up` never reads, parses, or resolves against `.forge`.** Its only inputs are the frozen
   migration file (logical `SchemaChange` list + checksum) and the target data dir's `manifest.json`.
2. **Apply resolves all physical layout from the manifest's `ColumnMetadata` at apply time** — physical
   operations are NOT frozen into the migration file. The migration file carries logical intent only.
3. **No runtime migration engine ships.** The generated application links no migration crate; its generated
   `Cargo.toml` dependency set is unchanged (`storage`/`types`/`wal`/`changefeed`). `forgedb-migrations` may
   exist as CLI-support, but generated code must not depend on it.
4. **On open, the app does exactly one thing with the manifest's version fields:** compare
   `manifest.format_version` to the baked-in `EXPECTED_FORMAT_VERSION` (opaque integer) and fail-fast on
   mismatch. It MUST NOT read column names/types/relations to alter decoding behavior — the app **refuses**
   stale data, it never **adapts** to it.
5. **The `Schema → SimpleSchema` bridge is a dev-time, CLI-only, diff-scoped conversion** invoked only by
   `migrate create`. Its output never ships in generated code and is never read at runtime. It is not a
   general schema-reflection API — name it accordingly (e.g. `migration_snapshot_of(&Schema)`, not
   `SchemaReflection`).
6. **Checksum is validated against the authored-from snapshot, not `.forge`.** `migrate up` validates the
   migration's checksum against the committed schema-snapshot it was authored against, never by re-parsing
   `.forge`. (Backstops red line #1.)
7. **A rewrite-class migration bumps `format_version` (and `compaction_epoch` where it rewrites columns) in
   the same atomic manifest commit as the byte rewrite** — there is no window where evolved data carries a
   stale version. (Makes red line #4's guard trustworthy.)
8. **`EXPECTED_FORMAT_VERSION` is a generated-per-schema constant derived from the migration lineage**, not
   a hand-edited literal, so regeneration and migration stay in lockstep by construction.

## Blessed roadmap (phased)

**Phase 0 — bugfixes (independent, ship ahead of the rest).**
- Fix the **filename-convention mismatch**: the executor writes `fixed/{type}_{field_name}.bin`; codegen
  emits/reads `fixed/{type}_{column_index}.bin`. Resolve the executor against the manifest's
  `relative_path` (which is already index-based), not a bespoke name scheme.
- Fix **non-backfilling `add_column`**: it writes an empty placeholder, leaving a zero-length column
  misaligned with the N-row siblings. Backfill to `row_count` with the default/null-tag encoding.

**Phase 1 — version guard + writer-preservation prerequisite.**
- Generated writer preserves existing `compaction_epoch`/`format_version` on reopen (the prerequisite).
- Codegen emits `EXPECTED_FORMAT_VERSION` (lineage-sourced) + a fail-fast open-time compare.
- No data evolution yet — this alone converts silent mis-decode into a clear "run `forgedb migrate up`".

**Phase 2 — the cheap, append-only-friendly migrations, executed correctly.**
- Add nullable/defaulted field (backfilled), remove field, rename field (codegen/metadata), rename model.
- Manifest-driven apply; `--auto` diff wired via the contained `Schema → SimpleSchema` bridge + committed
  snapshot.

**Phase 3 — rewrite-class migrations (reuse the compaction primitive).**
- Change field type, add NOT NULL w/ supplied default, add unique w/ dup validation.
- Share `stage_*_column_write`; atomic version-bump commit (red line #7).

**Phase 4 — workflow + multi-tenancy.**
- Unify `create → generate → up` (guidance/ordering, guard-enforced); per-tenant **rolling** migration
  across `<root>/<tenant>` dirs (the #59 interaction; the `forgedb tenant` CLI is the natural host).

## First milestone (smallest useful slice)

**Phase 0 + Phase 1.** The two bugfixes make the *existing* executed operations correct, and the version
guard turns the silent-mis-decode hole into a fail-fast — a real, shippable safety win with **no** new
migration semantics and **no** data rewrite. It also lands the writer-preservation prerequisite that
everything else depends on. Identity surface is minimal (a generated integer compare + a generated
manifest-read-preserve).

## Honest deferrals

- **Online migrations** (apply while serving) — deferred; apply assumes an exclusive writer (the #56-B
  single-writer discipline).
- **Multi-step / expression data transforms** (e.g. split a column, compute a new field from others) —
  deferred; Phase 2/3 handle structural changes with constant defaults, not row-wise computation.
  **Now designed → [`data-transform-migrations.md`](./data-transform-migrations.md)** (issue #74, PM
  verdict PASS-WITH-CONSTRAINTS): row-wise transforms are a **generated typed `transform(old) -> new`
  Rust function** (dev-time codegen + one-shot offline compiled binary), not a runtime expression
  interpreter — the manual dump/reload path, generated and type-checked.
- **Lossy down-migrations** for rewrite-class changes (a type narrowing can't round-trip) — `down` may be
  refused or documented-lossy, not silently reconstructed.
- **Cross-tenant coordination** beyond a sequential rolling sweep — deferred (composes with #59's model-C
  registry if/when that lands).
- **Cascade / referential data fixups** on model/field removal — out of scope; migrations move bytes, they
  don't enforce relational integrity.

## Cross-issue dependencies

- **Reuses `crates/compaction`** rewrite primitives + the atomic temp+rename commit (Phase 3).
- **Reuses the #57 `Manifest`** (`format_version`, `compaction_epoch`, `ColumnMetadata`, `RowAnchor`) as the
  schema-blind resolution surface.
- **#59 multi-tenancy** — per-tenant rolling migration (Phase 4); the `forgedb tenant` CLI is the host.
- The `compaction_epoch` bump this feature introduces is the same field #57's deferred **incremental
  backups** reserve — coordinate so a rewrite-class migration and an incremental-backup chain agree on epoch
  semantics.

## Load-bearing references

- `crates/migrations/{types,diff,executor,tracker,generator}.rs` — the logical migration format, the
  `SimpleSchema` diff target, the (mostly stubbed) executor to make manifest-driven, the JSON tracker.
- `src/commands/migrate.rs` — `create/status/up/down`; hardcoded `migrations/` + `data/` paths (make
  config/`open_at`-aware, #59); `detect_schema_changes()` stub (wire the bridge).
- `crates/compaction/src/compactor.rs` — `compact_model` + `stage_fixed/variable/tombstones_column_write`,
  the schema-blind rewrite primitives Phase 3 reuses; the atomic temp+rename commit pattern (red line #7).
- `crates/storage/src/lib.rs` — `Manifest` / `ColumnMetadata` / `ColumnKind` / `RowAnchor` /
  `format_version` / `compaction_epoch`; `save_to`/`load_from`.
- `crates/codegen/src/rust.rs` — version-guard emission target; the manifest write-sites at ~943 and ~1861
  and the FUTURE-TRAP comment at ~881–886 (the writer-preservation prerequisite); the `new_at` rehydrate
  path where the open-time guard slots in.
- `CLAUDE.md` → "What ForgeDB is" — the invariant this note is gated against.
