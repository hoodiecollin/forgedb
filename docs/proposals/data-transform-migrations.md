# Proposal: Data-Transform Migrations (row-wise / computed evolution)

**Status:** DESIGN NOTE — `forgedb-product-manager` verdict **PASS-WITH-CONSTRAINTS** (2026-07-14;
4 binding constraints + 2 correctness refinements, all folded in below — see "PM gate constraints").
Fleshes out the **deferred** tier of [`schema-migrations.md`](./schema-migrations.md) (structural
migrations + version guard, PM-PASSed): its "Honest deferrals" explicitly punt *"multi-step /
expression data transforms (split a column, compute a new field from others) … not row-wise
computation."* This note is the design for that deferred rock — the part the manual **dump/reload**
path (`docs/MIGRATIONS.md`) does by hand today.
**Issue:** [#74](https://github.com/hoodiecollin/forgedb/issues/74) (`idea`, `tech-debt`)
**Date:** 2026-07-14

## Summary

The headline move, which keeps this squarely inside the generator identity: a data transform is a
**generated, typed `transform(old) -> new` function that the developer fills in and the compiler
checks — not an interpreted expression stored in a migration file and evaluated at runtime.**

Today, a breaking change with row-wise computation (e.g. `qty: u32 → qty: string`, or split
`full_name` into `first`/`last`, or compute `total` from `price * count`) is handled by the
**manual dump/reload** recipe: the developer hand-writes an `OldWidget` deserialize struct, loops
`all()`, transforms each row in application code, and reloads via `create_<model>` into a fresh dir.
That works and is identity-clean (the transform is app Rust, the app links no migration engine), but
it is **all boilerplate**: the developer re-derives the old struct by hand, wires serde, manages two
data dirs, and preserves ids/FKs manually.

This note **generates that boilerplate** and nothing else. `forgedb migrate create --transform`
emits a one-shot **transform crate** containing (a) generated `Old*` structs + readers from the
committed schema snapshot, (b) generated `New*` structs + writers from the current `.forge`, and
(c) a `transform.rs` **stub** with a typed `fn transform(old: &OldDatabase) -> NewDatabase` the
developer completes — the field-aware intent only they know. `forgedb migrate up` **compiles and
runs it once, offline**, reading the old columns and writing a new dir atomically (temp + rename,
like `backup restore`). The running application is **unchanged**: it still does exactly the opaque
`format_version` compare + fail-fast from `schema-migrations.md`; it never links a transform, never
interprets a schema.

**The identity crux:** the transform's *code generation* is dev-time tooling (same class as
`forgedb generate`); the transform *binary* is a one-shot offline tool (same class as `backup`);
the *shipped app* gains nothing at runtime. There is **no runtime expression interpreter and no
`.forge` read at runtime** — the exact line the "generic engine" invariant forbids.

## Where this sits in the migration story

`schema-migrations.md` establishes three change classes; this note owns exactly one:

| Change class | Owner | Mechanism |
|---|---|---|
| **Structural / metadata** (add nullable field, remove field, rename field/model) | `schema-migrations.md` Phase 2 | codegen + cheap manifest-driven column ops |
| **Constant-default rewrite** (type change *to a constant*, add NOT NULL *with a literal default*, add unique) | `schema-migrations.md` Phase 3 | reuse `crates/compaction` `stage_*_column_write`, atomic version bump |
| **Row-wise / computed transform** (value depends on the row's *other* fields, or a column split/merge, or a cross-model derivation) | **THIS NOTE** | generated typed `transform(old) -> new`, compiled + run once offline |

The boundary is sharp and testable: **if the new value is a pure function of the old row's other
fields, it needs a transform (this note); if it is a constant or a structural move, `schema-migrations.md`
already handles it.** The version guard + snapshot machinery are shared verbatim.

## Why NOT an expression grammar in the migration file

The tempting alternative — store `@transform(total = price * count)` in the migration JSON and
evaluate it at `migrate up` — is rejected on **two** independent grounds:

1. **Identity.** An engine that reads an expression + a schema and evaluates it against arbitrary
   rows at runtime **is** the forbidden generic runtime interpreter (CLAUDE.md "Still rejected").
   It is the migration-shaped version of the dynamic query builder the guard exists to prevent.
2. **It's blocked anyway.** ForgeDB has **no expression grammar** — `@computed` is deferred for
   exactly this reason (the lexer only parses number / bare-ident / string directive args). Building
   an expression evaluator for migrations would front-load that entire deferred effort *and* land it
   on the wrong side of the identity line.

**Generated typed Rust sidesteps both.** The "expression" is ordinary Rust the developer writes and
`rustc` type-checks (`old.price * old.count`), compiled ahead of time. No grammar to build, no
runtime interpreter to ship. The transform's *power* is unbounded (it's Rust), its *safety* is the
compiler, and its *identity* is clean (dev-time codegen + one-shot offline binary).

## The transform workflow

```
edit .forge (a breaking, computed change)
        │
        ▼
forgedb migrate create --transform "split name into first/last"
        │  generates migrations/{id}_{desc}/  (a self-contained crate)
        │    ├─ Cargo.toml            (path/version-pinned substrate; NOT shipped with the app)
        │    ├─ src/old.rs            GENERATED from migrations/.schema-snapshot.forge
        │    │                          → Old<Model> structs + OldDatabase read-only opener
        │    ├─ src/new.rs            GENERATED from the current .forge
        │    │                          → New<Model> structs + NewDatabase writer (create_*)
        │    └─ src/transform.rs      STUB the developer completes:
        │                                fn transform(old: &OldDatabase, new: &mut NewDatabase)
        │                                              -> Result<(), TransformError>
        ▼
(developer fills transform.rs — the field-aware intent, e.g.
   for u in old.users.all() {
       let (first, last) = split(&u.name);
       new.create_user(NewUser { id: u.id, first, last, ..map(u) })?;
   })
        ▼
forgedb migrate up
        │  1. snapshot the old dir (reuse #57 backup) — rollback safety
        │  2. cargo build + run the transform crate against a fresh temp dir
        │  3. the transform reads old columns (old.rs readers) → writes new columns (new.rs writers)
        │  4. new dir's manifests carry the new format_version (matched to regenerated code)
        │  5. atomic swap: rename temp → data dir (old dir retained as backup)
        ▼
forgedb generate && cargo build   (regenerate the APP's database.rs to the new schema)
        ▼
app opens the migrated dir; format_version matches EXPECTED_FORMAT_VERSION → runs
```

Key properties:

- **Two schema versions coexist only inside the transform crate**, namespaced `old::` / `new::` —
  both are generated code, never hand-derived. The old readers are the `reader()` handles (#56-B)
  restricted to reads; the new writers are the integrity-enforcing `create_<model>` wrappers (#91),
  so FK existence + validation apply to the migrated data for free.
- **The transform is `all()`-based (load-transform-write) in M1**, matching dump/reload semantics. A
  streaming/iterator form (for datasets that don't fit memory) is a deferred refinement, not a first
  cut.
- **Ids/FKs are preserved by the developer** exactly as in dump/reload — but the generated `New*`
  types make the mapping type-checked instead of stringly-typed serde.
- **Atomicity** reuses the `backup restore` temp-dir + rename discipline; a crash mid-transform
  leaves the original dir untouched (the app's version guard still refuses the un-migrated dir, so
  there is no silent half-state).

## Identity verdict & the drift vectors

Mapping to the guard (`CLAUDE.md` → "What ForgeDB is"), inheriting `schema-migrations.md`'s red
lines:

| Piece | Class | Rationale |
|---|---|---|
| Generating `old.rs` / `new.rs` / `transform.rs` from (snapshot, `.forge`) | **Dev-time codegen** | same class as `forgedb generate` — reads `.forge` at *dev* time, emits tailored code |
| The developer-authored `transform` body | **App-level Rust** | compiled, type-checked; the field-aware intent only the app knows (same as dump/reload) |
| The compiled transform binary | **One-shot offline tool** | same class as `backup`/`compact` — reads/writes opaque column bytes, run by an operator, not the app |
| Old readers / new writers | **Generated** (reused #56-B/#91 surfaces) | tailored per-schema, no runtime schema read |
| The running app | **Unchanged** | still only an opaque `format_version` compare + refuse (`schema-migrations.md` red line #4) |

**Drift vector #1 (the sharp one):** shipping the transform *inside the application* or making the
app *apply* a transform on open. Guard-passing mirror: the transform is a **separate one-shot crate**
the operator runs; the app links no transform and **refuses** un-migrated data, never adapts to it.

**Drift vector #2:** letting the migration file carry an *expression* the `migrate up` tool
*evaluates*. Guard-passing mirror: the migration carries a **reference to compiled transform code**,
never an interpreted expression. `migrate up` compiles + runs Rust; it evaluates nothing itself.

**Drift vector #3:** a "smart" transform generator that *infers* the row mapping from the diff.
Rejected — the generator emits a **stub with a `todo!()`** and the structural context; inferring the
developer's intent is where a generic engine would creep in. The generator scaffolds; the developer
decides.

## Red lines

1. **No runtime expression interpreter.** The transform is compiled Rust, type-checked ahead of
   time. The migration file references compiled code; it never stores an expression `migrate up`
   evaluates.
2. **The application never links a transform** and never applies one on open. Its only migration
   behavior is the opaque `format_version` compare + fail-fast (`schema-migrations.md` red line #4).
3. **The transform binary is schema-blind at the substrate layer** — it drives generated typed
   readers/writers; the storage/backup substrate it links sees only opaque column bytes.
4. **`old.rs`/`new.rs` are generated, never hand-derived**, from the committed snapshot + current
   `.forge` — dev-time codegen, the same class as `generate`. They never ship in the app.
5. **The generator scaffolds, it does not infer.** The `transform.rs` stub is `todo!()` + the
   structural diff as comments; ForgeDB never guesses the row mapping.
6. **Atomicity + rollback are mandatory** — snapshot before (reuse #57), temp-dir + rename swap
   (reuse `backup restore`), old dir retained. A failed transform leaves a dir the app *refuses*,
   never a silent half-migration.
7. **Authoring surface stays "schema + CLI + config."** The transform crate is generated tooling;
   no new `.forge` syntax encodes the transform.
8. **Version bump is atomic with the swap** — the new dir carries the new `format_version` before it
   becomes the live dir (matches `schema-migrations.md` red line #7).

## PM gate constraints (binding — 2026-07-14)

The identity gate passed the design and named four binding constraints + two correctness
refinements. All are folded into the red lines / milestones above; restated here as the checklist a
first milestone must satisfy:

1. **`transform.rs` is `todo!()`-stub-only — enforced, not just documented.** The generator emits
   the structural diff as comments + a `todo!()` body and **never a guessed field mapping**, not
   even the "obvious" 1:1 case (`new.field = old.field` speculatively **is** the thin end of
   inference). A **guard test asserts the emitted body is `todo!()` + comments only** (strengthens
   red line #5).
2. **Name the snapshot→codegen bridge migration-scoped, and keep it off the runtime path.** The
   `old.rs` generator reads `.schema-snapshot.forge`; it is dev-time CLI-only (name it e.g.
   `transform_scaffold_of(&Schema)`, a peer of `generate` — never `SchemaReflection`), and its
   output is **never reachable from generated `database.rs`** (inherits `schema-migrations.md` red
   line #5).
3. **The transform crate links only published class-1 substrate + the two generated modules.** Its
   `Cargo.toml` pins `forgedb-storage`/`-backup`/`-types` from crates.io (same publish-gap
   discipline as `init → build`); it **must not link `forgedb-migrations`** or any schema-reading
   helper (strengthens red line #3).
4. **Version bump is atomic with the swap; the new dir carries the new `format_version` BEFORE it
   goes live.** Correctness-critical: a crash between "new dir written" and "version stamped" must
   leave a dir the regenerated app *refuses*, never one it accepts mid-migration. **Guard test**
   proves the temp dir carries the new version before the rename, and a crash mid-transform leaves
   the *original* dir untouched (red lines #6/#8).
5. **(Refinement) `migrate up` verifies the old dir's `format_version` == the snapshot's expected
   version before running the transform** — an **opaque integer compare, never a `.forge` re-read**.
   `old.rs` is generated from the snapshot, so if a prior half-migration or compaction bumped the
   old dir's epoch, the generated readers could mis-decode; refuse in that case rather than read
   bytes under an assumed layout.
6. **(Refinement) M3 cross-model FK ordering surfaces a typed `TransformError`.** A child written
   before its parent hits the #91 FK-existence check and fails — which is *correct*; ordering is the
   developer's responsibility (consistent with "scaffold, don't infer"). Document it as an honest
   limit, not a mechanism gap.

## Staged milestones

- **M1 — ergonomic dump/reload (1:1 model transform).** Generate `old.rs`/`new.rs`/`transform.rs`
  for a schema where every model maps to itself with per-field computation (the `qty: u32 → string`
  class). `migrate up` builds + runs it over a temp dir, atomic swap, version bump. This is exactly
  today's manual dump/reload with the boilerplate generated and type-checked. **Highest value,
  lowest risk** — proves the whole seam.
- **M2 — column split / merge / compute-from-siblings.** Same machinery; the transform body reads
  several old fields and writes several new ones (split `name`, compute `total`). No new mechanism —
  the generated `Old*`/`New*` structs already expose all fields; M2 is documentation + examples +
  test coverage that the seam handles it.
- **M3 — cross-model transforms.** The transform reads model A to populate model B (denormalize,
  extract a table). `OldDatabase`/`NewDatabase` already bundle all models, so this is also
  mechanism-complete after M1; M3 hardens FK-integrity ordering (create parents before children).
- **M4 — streaming transform** (for datasets exceeding memory): an iterator form of the generated
  readers so the transform processes row-batches instead of `all()`. Deferred refinement.

## Honest deferrals

- **Online transform (apply while serving)** — deferred; the offline transform assumes an exclusive
  writer (the #56-B single-writer discipline / a #75 Tier-1 transaction). Cross-reference #75.
- **Automatic transform inference** — explicitly rejected (drift vector #3): ForgeDB scaffolds, the
  developer writes the mapping.
- **Lossy down-transforms** — a `down` transform is symmetric (generate the reverse scaffold) but
  may be genuinely lossy (a narrowing) — documented-lossy or refused, never silently reconstructed
  (inherits `schema-migrations.md`).
- **Very large datasets** — M1–M3 are load-transform-write (`all()`); streaming is M4.
- **Multi-tenant rolling transform** — composes with `schema-migrations.md` Phase 4 (per-`<root>/<tenant>`
  sweep); each tenant dir is transformed independently.

## Cross-issue dependencies

- **Builds on `schema-migrations.md`** — shares the committed schema snapshot
  (`migrations/.schema-snapshot.forge`), the version guard + `EXPECTED_FORMAT_VERSION`, and the
  breaking-vs-additive gate. This note is its computed-rewrite tier.
- **Reuses #56-B reader handles** (old readers) + **#91 integrity wrappers** (new writers) — the
  migrated data gets validation + FK checks for free.
- **Reuses #57 backup** for the pre-transform snapshot + the atomic temp-dir/rename swap.
- **Interacts with #75** — an *online* transform needs a transaction (Tier-1) or the single-writer
  lock; the offline transform needs neither.
- **Coordinates `compaction_epoch`/`format_version`** with `schema-migrations.md` and the deferred
  incremental-backup chain (#76) so a rewrite and a backup chain agree on epoch semantics.

## Load-bearing references

- `docs/proposals/schema-migrations.md` — the structural + version-guard foundation this note
  extends; its red lines are inherited.
- `docs/MIGRATIONS.md:61-103` — the manual dump/reload recipe this note generates + type-checks.
- `crates/codegen/src/rust.rs` — the emission target: generate `Old*`/`New*` structs + readers
  (reuse the `reader()` #56-B token stream) + writers (reuse `create_<model>` #91); the recovery /
  backfill path (~2430–2604) that handles the *constant*-default case this note does NOT duplicate.
- `crates/migrations/{types,diff}.rs` — `SchemaChange` / `SchemaDiffer` already classify which
  changes are computed-rewrite class (type change, etc.), the trigger for `--transform`.
- `src/commands/migrate.rs` — `create` (add the `--transform` arm), the `.schema-snapshot.forge`
  persistence (`old.rs` source), `to_simple_schema`.
- `crates/backup` — the snapshot + atomic temp-dir/rename primitives the swap reuses.
- `CLAUDE.md` → "What ForgeDB is" — the invariant this note is gated against.
