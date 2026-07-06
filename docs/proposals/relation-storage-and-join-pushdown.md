# Proposal: Relation/FK Storage & Join Pushdown

**Status:** APPROVED 2026-07-06 — FK minimal fix converted to impl task #25 (highest priority; relation traversal deferred). Join pushdown: LEAVE honest stub, no task created (query-optimization is a zero-consumer leaf crate).
**Triage task:** #24
**Date:** 2026-07-06

## Summary
FK scalar fields (e.g. `author` on a `Post` referencing `Author`) are declared in the
generated model struct as `Uuid` / `Option<Uuid>`, but they are **never persisted** — the
insert loop has no branch that writes them, and `get()` reconstructs them from a hardcoded
default. This is silent data loss in generated code and the highest-impact item in this
triage. The minimal correctness fix is purely a codegen change: route FK reference types
through the existing `Uuid` fixed-column path (storage needs **zero** changes). Relation
*traversal* (joins/eager-load/back-collections) is a separate, additive generator feature —
defer it. Join pushdown should be **left as an honest stub**: `query-optimization` is a leaf
crate with zero consumers, not on the critical path, and building it requires a structured
predicate IR that nothing currently produces.

## Feature 1: Relation / FK storage

- **Current state (with evidence):**
  A FK reference is `FieldType::Relation(RelationType::RequiredReference(_))` (required, `*`)
  or `OptionalReference(_)` (nullable). The generated *struct field type is already correct* —
  `map_field_type_ident` maps both to `Uuid` (`crates/codegen/src/rust.rs:612-618`) and
  `Field::is_nullable()` returns `true` for `OptionalReference`
  (`crates/parser/src/ast.rs:576-583`), so the struct field renders as `Option<Uuid>` via
  `rust.rs:91-94`. The value is dropped in **three** places, all keyed on the fact that
  `is_fixed_size_type` and `is_string_type` both return `false` for `Relation`
  (`rust.rs:493` catch-all `_ => false`; `rust.rs:498-504`):
  - **Storage column never declared** — `generate_storage_fields` emits a column only for
    fixed/string fields; the trailing comment is literally `// Skip relations and other
    non-storage types` (`rust.rs:148-164`). `generate_storage_inits` skips it the same way
    (`rust.rs:175-225`).
  - **Value dropped on insert** — `generate_insert_logic` only appends when
    `is_string_type` or `is_fixed_size_type` is true (`rust.rs:282,287`). A `Relation` field
    matches neither branch, so **nothing is appended** — the FK the user set is silently
    discarded (`rust.rs:267-330`).
  - **Value faked on read** — `generate_get_logic` routes `Relation` fields to the final
    `else` (`rust.rs:413-418`), calling `default_for_unstored_field`, which returns
    `Default::default()` (i.e. `Uuid::nil()`) for `RequiredReference` and `None` for
    `OptionalReference` (`rust.rs:459-475`).

  Net effect: `post.author = X; db.post.insert(post); db.post.get(id)` returns
  `author == Uuid::nil()`. Non-relation fields round-trip correctly.

- **Severity:** High, silent. No panic, no compile error, no warning — the generated code
  compiles and passes its own snapshot tests (snapshots compare strings only; they never
  execute a round-trip). Any application with a foreign key loses that FK on every write.
  This is a correctness/data-integrity bug in shipped generated code, not a missing
  convenience.

- **Minimal correctness fix:** Treat a FK reference as an ordinary scalar column:
  - `RequiredReference(_)` behaves **exactly** like `FieldType::Uuid` — a 16-byte
    `FixedColumn`, written with `append_uuid(*v.as_bytes())`, read with `read_uuid` +
    `Uuid::from_bytes`.
  - `OptionalReference(_)` behaves **exactly** like `FieldType::Nullable(Uuid)` — the existing
    nullable byte-blob path (`stored_type = Option<Uuid>`, `size_of::<Option<Uuid>>()`,
    `append_bytes` / `ptr::read_unaligned`), which the generator already supports for other
    nullable fixed types.

  The storage engine already has everything required — `FixedColumn::append_uuid` /
  `read_uuid` exist (`crates/storage/src/lib.rs:370-392`). **No storage changes.** This is
  the right minimal fix: the FK *scalar* is the only thing that needs to round-trip.

- **Larger feature (deferred?):** Relation *traversal* is genuinely unbuilt and is separate
  from correctness:
  - `OneToMany` (`[Post]` back-collection) and `ManyToMany` are **derived/virtual** — they
    are not columns and must not be stored (today they map to `()` in the struct via
    `rust.rs:616` and are defaulted to `()`). Populating them means querying the child
    table by FK at read time.
  - Generated traversal helpers (e.g. `Database::author_of(&post) -> Option<Author>`,
    eager-load, join queries) and `ManyToMany` junction tables.

  All of these stay inside the generate-then-compile identity (they are *more generated
  code*, not a runtime ORM), but none are needed for correctness. Defer.

- **Recommended scope:** Build **only** the minimal FK-scalar round-trip now
  (`RequiredReference` + `OptionalReference`). Defer OneToMany/ManyToMany population,
  traversal helpers, joins, and eager-loading to a follow-up proposal. Separately, decide
  whether virtual `OneToMany`/`ManyToMany` fields should even appear as `()` fields in the
  struct (see Open questions) — but that is cosmetic, not correctness.

- **Design:** Single-file codegen change in `crates/codegen/src/rust.rs`. The cleanest
  approach is a small normalization helper, e.g. `fk_scalar_equivalent(&FieldType) ->
  Option<FieldType>` returning `Uuid` for `RequiredReference` and `Nullable(Uuid)` for
  `OptionalReference`, then feed reference fields through the *existing* Uuid / nullable
  paths so column indexing stays identical across all four generators. Concretely:
  - `is_fixed_size_type` (`rust.rs:478-495`): return `true` for `RequiredReference` /
    `OptionalReference`. This alone makes `generate_storage_fields` and
    `generate_storage_inits` emit the column (they key off `is_fixed_size_type`), keeping the
    `column_index` counter consistent across storage/insert/get — the load-bearing invariant.
  - `type_name` (`rust.rs:507-524`): map both reference types to `"uuid"` so the column path
    and `append_uuid`/`read_uuid` method idents resolve.
  - `column_value_size_expr` (`rust.rs:230-264`): `RequiredReference` → `16`;
    `OptionalReference` → `size_of::<Option<Uuid>>()`.
  - `generate_insert_logic` (`rust.rs:267-330`): route `RequiredReference` into the existing
    Uuid special-case (`*record.field.as_bytes()`); route `OptionalReference` into the
    nullable byte-blob branch (`needs_byte_conversion`).
  - `generate_get_logic` (`rust.rs:351-419`): same routing — `RequiredReference` via the Uuid
    special-case, `OptionalReference` via the nullable `stored_type = Option<Uuid>` branch —
    and **remove** the `RequiredReference`/`OptionalReference` arms from
    `default_for_unstored_field` (they must no longer be reached; keep only
    `OneToMany`/`ManyToMany`/`Component`).
  - **Storage:** no changes.

- **Proposed impl tasks:**
  1. Add `fk_scalar_equivalent` (or inline arms) and update the five classifier/emitter
     helpers in `crates/codegen/src/rust.rs` (`is_fixed_size_type`, `type_name`,
     `column_value_size_expr`, `generate_insert_logic`, `generate_get_logic`).
  2. Trim `default_for_unstored_field` to virtual relations + components only.
  3. Add an insta snapshot fixture in `crates/codegen/tests/` for a schema with both a
     required and an optional FK; review/accept the new snapshots.
  4. **Compile-test the emitted Rust in a throwaway crate** (per the codegen memory note —
     snapshot pass ≠ output compiles) and assert an actual insert→get FK round-trip for both
     required and optional references.
  5. Update `CLAUDE.md` "Known issues": narrow the "Relation storage is unbuilt" entry to
     cover only traversal/back-collections; the scalar FK gap is now fixed.

## Feature 2: Join pushdown

- **Current state (with evidence):** `partition_predicates_for_join` unconditionally returns
  `(Vec::new(), Vec::new())` (`crates/query-optimization/src/planner.rs:300-305`). As a
  result, in `pushdown_predicates` every predicate lands in `remaining` and is preserved as a
  `Filter` wrapping the join output (`planner.rs:236-275`). Results are **correct**; the
  pushdown optimization simply does not happen. The behavior is honestly documented in the
  code comments, not hidden.

- **Root cause:** Predicates are modeled as plain `Vec<String>` (`planner.rs:227`, and the
  `Filter { predicate: String }` op at `planner.rs:45-50`). A string like
  `"post.author = author.id"` carries no structured table/column attribution, so the
  partitioner cannot decide which side a predicate belongs to. Pushdown requires a structured
  predicate IR (column refs, operator, operand, source-table tags) that the planner can
  inspect — nothing in the codebase produces such an IR today.

- **Is it on the critical path?:** No. `forgedb-query-optimization` is a **leaf crate with
  zero consumers**: it is declared in the root `Cargo.toml` workspace members and as a
  path dependency (`Cargo.toml:17,64`), but **no crate lists it under `[dependencies]`**
  (`grep -rln forgedb-query-optimization crates/*/Cargo.toml src` returns only its own
  manifest). It is **not** referenced by `codegen` (no `QueryPlan`/`pushdown`/`optimize_join`
  in `crates/codegen/src`) and **not** emitted into generated code. Generated databases do
  not use a query planner at all — they use direct columnar reads (`get()` by id). This crate
  is speculative infrastructure ahead of what the generator actually consumes.

- **Options:**
  - **(A) Leave the honest stub.** Zero risk, results already correct, comments already
    explain the gap. Cost: a real-but-dormant optimization stays unimplemented. *Preferred.*
  - **(B) Build structured-predicate pushdown.** Introduce a predicate IR (enum with column
    refs + table tags), replace `Vec<String>`/`Filter.predicate: String` throughout the
    planner, and implement `partition_predicates_for_join`. Large surface change to a crate
    that nobody uses, and it would still not affect generated output until a query pipeline is
    designed to consume it. Premature.
  - **(C) Remove the speculative crate.** Deleting `query-optimization` (and its
    `scan`/`statistics` siblings) would shrink the workspace to match what the generator
    actually needs. Reasonable but a bigger product call than this triage; the crate is
    harmless where it sits and may inform a future query story.

- **Decision:** **Leave the honest stub (Option A). Do not build pushdown now.** It is off
  the critical path, results are correct, and building it demands a predicate IR that no
  producer exists for — classic ahead-of-need work. Whether to eventually build (B) or prune
  (C) is a strategic call to make when/if a generated query pipeline is designed, not in a
  bug-fix triage.

- **Proposed impl tasks:** Defer — no implementation tasks. (Optional, non-code: fold the
  "query-optimization is speculative / zero consumers" observation into the revival backlog
  so a future keep-vs-remove decision is on record.)

## Open questions for the user

1. **Scope confirmation:** Ship only the FK-scalar round-trip now and defer all traversal
   (back-collections, joins, eager-load, M2M junctions) to a separate proposal — agreed?
2. **Virtual relation fields in the struct:** `OneToMany`/`ManyToMany` currently render as
   `field: ()` and default to `()`. Should the generator instead **omit** these virtual
   fields from the struct entirely (cleaner API), or keep the `()` placeholder until
   traversal is built? This is cosmetic, not correctness — but it changes the generated
   public API.
3. **FK integrity:** When we persist `author: Uuid`, do you want the generated `insert` to
   validate that the referenced `Author` id exists (referential integrity), or persist the
   raw FK unchecked for now? Unchecked is the minimal fix; validation is an additive feature.
4. **query-optimization crate fate:** Keep the leaf crate as dormant infrastructure (Option
   A), or schedule a keep-vs-remove decision (Option C) as its own revival triage item?
