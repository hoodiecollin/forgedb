# Proposal: Column Projection / Partial Model Reads

**Status:** LANDED (2026-07-14) — full stack, PM-gated **PASS-WITH-CONSTRAINTS**. Needs **no
substrate change and no publish**; the mechanism is "generate a narrower `read_at` over columnar
files the substrate already reads per column." Strategy: **declared `@projection(...)`** (typed
Rust surface) + REST `?projection=<name>` (named declared projections only — no ad-hoc `?fields=`);
typestate builder and runtime-bitmask-as-Rust-API deferred.

**Locked decisions (2026-07-14):** (1) syntax `@projection(card: title, slug)` — named-with-colon;
(2) REST surface `?projection=<name>` **only** (no ad-hoc `?fields=`), absent = full record; (3)
first milestone = **full stack in one pass** (Rust + REST + TS SDK + WASM).

## What landed (2026-07-14)

- **Parser + AST:** `Projection { name, fields }` on `Model`; `@projection(name: a, b)` parses via a
  new `parse_directive` arm (mirrors `@index`); structural checks (field exists, unique name) in the
  parser. Guards: 3 parser unit tests.
- **Validation:** `RustGenerator::validate_projections` hard-errors at codegen (the always-run
  compile step) on a projected relation/virtual field (PM constraint 2). Guard
  `test_rust_generation_projection_rejects_relation_field`.
- **Rust (`rust.rs`):** per `@projection`, a tight `<Model><Name>` struct (PK + selected, real
  types) + `read_<name>_at` / `get_<name>` / `all_<name>` / `get_<name>_at` / `all_<name>_at`. The
  narrow decoder reuses the shared `field_read_stmt` via a new `generate_row_read_body` that
  `read_at` also delegates to — **one decode path** (constraint 1); `read_at` output stayed
  byte-identical (snapshots unchanged). PK always materialized (constraint 3). Guard
  `test_rust_generation_column_projection` (struct = PK+selected, decoder touches only selected
  columns, no `db.get()`, snapshot variants present).
- **REST (`api.rs`):** `?projection=<name>` closed-set `match` on `get` (routes through the narrow
  `get_<name>` — full server-side skip) and `list` (filter/sort/paginate full rows, then field-copy
  the page to the projection — wire shrink only, since filter/sort need full rows); unknown name →
  400; absent → full record (constraint 6 — no ad-hoc field list on the wire).
- **TS SDK (`typescript.rs`):** `<Model><Name>` interface + `get<Model><Name>` / `list<Model><Name>`
  hitting `?projection=`. `tsc --noEmit --strict` clean.
- **WASM (`wasm.rs`):** `read_surface` gains projected `get<Model><Name>` / `all<Model><Name>`,
  auto-mirrored into the `ReplicaClient`. The projected decoder touches only selected columns, so on
  `storage-web` the unselected columns stay lazy (constraint 5).

**Proof.**
- Native E2E `scratchpad/projection_compile` (ephemeral): db + api compile together; projected reads
  return correct values across scalar / nullable (None + Some) / FK / timestamp; tight types; list
  projection; snapshot `_at` isolation (pre-update value); survives reopen; **live REST round-trip**
  via `tower::oneshot` — `?projection=card` omits `bio`/`created_at`, unknown → 400, full record
  keeps all columns, list rows are the projection shape.
- `scratchpad/projection_wasm` (ephemeral): the generated projection replica **compiles to
  `wasm32-unknown-unknown`** against `forgedb-storage-web` — the projected reads run in the browser
  build.
- Whole 18-schema `examples/` corpus regenerates cleanly (no regression); workspace 432 → 437.

**Honest limit on the browser fault-in proof.** The in-browser *skip of unselected-column fault-in*
for a projected read is established **compositionally**, not by a fresh Playwright drive: (a) the
codegen guard proves the projected decoder references only the selected columns; (b) the
`storage-web` `LazySource` unit test proves reading one column never faults another; (c) #110 WS5
already proved per-column lazy fault-in works in a real browser (`Tag.label` stayed lazy); (d) the
`wasm32` build above proves the projected reads compile into that same follower. The full Playwright
harness was **not** re-run for a projection-specific fault-in log — a belt-and-suspenders follow-up.
**Issue:** [#113](https://github.com/hoodiecollin/forgedb/issues/113).
**Date:** 2026-07-14

## Problem

Every generated read — `get`, `all`, `get_at`, `all_at`, `find_by_*`, `get_by_*`, the REST list
endpoint, and the wasm-replica reads — funnels through one decoder, `read_at(row_index) ->
Option<Model>` (`crates/codegen/src/rust.rs`), which decodes **every** stored field and constructs
the full `Model { .. }`. There is no way to read a subset of columns. Because ForgeDB storage is
**columnar** (each field is a physically separate file), full-record-only reads forfeit the column
store's central advantage: reading `k` of `N` columns is physically `O(k)` at every layer, but
codegen never exposes it.

### Why it matters (where the columnar bet pays off)

In a **row store** a partial read still pulls the whole row's page, so projection only saves
serialization/network. In a **column store** projection saves at *every layer*: disk/OPFS I/O, page
cache / working set, decode CPU, allocation, JSON serialization, wire bytes, client memory.

- **Process side.** `read_at` allocates a full owned `Model` per row (every `String` field
  heap-allocates + copies out of the arena). For `all() -> Vec<Model>` that is `rows × cols`
  allocations — the dominant cost. Projection cuts allocation, decode CPU (variable/`@fulltext`
  columns especially), and peak RSS proportionally.
- **Wasm read-replica (sharpest).** `forgedb-storage-web` faults columns in **per column**
  (path-keyed `LazySource`, `byte_len` without reading bytes). A full `getUser(id)` faults in every
  column of that record, so a list view needing `{name, avatar_url}` drags in `bio`/`content`
  blobs. A projecting read touches only the selected `field_read_stmt` branches, so unselected
  columns **stay lazy** — this extends the narrow-index-rehydrate win (only load indexed columns at
  reopen) to *every query*.
- **Client side.** Over-fetch is multiplicative in row count (a 5 KB column × 1000-row list ≈ 5 MB
  wasted on the wire — the GraphQL / `?fields=` motivation); dead JS heap + parse cost in the TS
  SDK; a column you never select never transits to a client that shouldn't cache it (a cheap
  partial substitute for the field-level authz of #72); a narrower, more stable contract that
  composes with additive migrations (#92 W2).

## Identity gate (summary of the PM verdict)

**PASS-WITH-CONSTRAINTS.** Two-part test:

1. *Tailored data logic stays generated per-schema at compile time?* Yes — a projecting read is
   `read_at` with a **subset** of the `field_read_stmt` branches emitted. Every field decode stays a
   per-schema generated statement; there is no generic "read columns X,Y,Z from an arbitrary
   schema" engine.
2. *Every published artifact stays schema-agnostic substrate or transport glue?* Yes — and **no
   substrate change is needed**. `forgedb-storage-native` already does positional per-column reads;
   `forgedb-storage-web` already does per-column lazy fault-in. So there is **no `init→build`
   publish gap** (unlike #89/#90/#92/#82). "Generate less of the code we already generate, against
   unchanged schema-agnostic substrate" is the platonic ForgeDB feature.

### Strategy decision

| Strategy | Selection is | Codegen cost | Type safety | Verdict |
|---|---|---|---|---|
| **1. Declared `@projection(...)`** | compile-time schema fact | O(K) named/model | tight (PK + selected only) | **SHIP (v1)** |
| **2. Typestate builder** | compile-time, call-site | Θ(N) source, O(D-used) monomorph | tight | defer (flexibility escape hatch) |
| **3. Runtime bitmask (Rust API)** | runtime value | Θ(N) | loose (all-`Option`) | **do not ship as Rust API** |

Strategy 3's runtime mask does **not** cross the red line (it gates a *closed, generated* set of
per-field decode branches — same class as the `forgedb-query-params` runtime filter over generated
matchers), but its loose all-`Option` return type is a strict downgrade to ForgeDB's compile-time
type-safety bar and it is the variant most prone to future drift. Its *only* legitimate home is the
**REST `?fields=` wire**, where a runtime value is unavoidable — implemented as a generated
closed-set selector, never a reflective serialize.

## Binding constraints (from the gate — MUST honor)

1. **One decode body, no second read path.** Projections MUST reuse `field_read_stmt` (the helper
   `read_at` and the reopen index-rebuild already share). No parallel projecting-decode — same "no
   second predicate parser" discipline as live queries. A projecting read is a *subset emission* of
   the canonical decode.
2. **Relations / virtual fields are not projectable.** `OneToMany`/`ManyToMany`/virtual `()` fields
   have no column. FK **scalars** (`RequiredReference` → `Uuid`, `OptionalReference` →
   `Option<Uuid>`) do and ARE projectable. Validation MUST reject `@projection(<relation_field>)` at
   compile time with a clear error. Eager-load/traversal is a join, out of scope.
3. **PK always materialized.** Every projection includes the identity column unconditionally (row
   handle for result resolution, index probes, REST addressing).
4. **Nullable / FK decode round-trips identically** — falls out for free from reusing
   `field_read_stmt` (the `string?` presence tag, the FK UUID decode). Another reason #1 is
   load-bearing.
5. **WASM lazy fault-in is the payoff — realize it, don't fight it.** The projecting read must not
   eagerly touch unselected columns (e.g. don't derive a value off a column you're skipping when
   another materialized column would serve). Verify in-browser (as #110 WS5 did for `Tag.label`)
   that a projection genuinely skips fault-in of excluded columns. **Do NOT** wire projection into
   the reopen index-rebuild path — recovery must still read id + indexed columns; projection is a
   *read-surface* feature, not a *recovery* feature.
6. **REST `?fields=` is a runtime value → generated closed-set only.** The handler maps `?fields=`
   onto a **generated per-model column-name → per-field emit** (only known column names accepted;
   unknown or relation field → 400) and serializes only the generated per-field JSON for the
   selected columns. Same class as the existing generated list filter/sort. Never a reflective
   "serialize whatever the mask says."

## Design (strategy 1: declared projections)

### Schema surface

A model-level directive naming a projection and its columns, mirroring the existing model-level
`@index(a, b)`:

```
Post {
  id: +uuid
  title: string
  slug: ^string
  content: string @fulltext
  excerpt: string?
  author: *User
  created_at: +timestamp

  @projection(card: title, slug, author, created_at)
  @projection(list_row: title, excerpt)
}
```

- `@projection(<name>: <field>, <field>, ...)` — `<name>` is a snake_case identifier used to name
  the generated struct/method; the fields are an explicit, ordered, compile-time-known subset.
- **PK is implicit** (constraint 3) — `id` is always included even if omitted from the list; listing
  it is allowed and idempotent.
- Naming diverges from `@index` (which derives its name from field names) because a projection
  needs a stable, readable type name (`PostCard`, not `PostTitleSlugAuthorCreatedAt`).

### Parser + AST (`crates/parser`)

- **AST** (`ast.rs`): add `pub struct Projection { pub name: String, pub fields: Vec<String> }` and
  `pub projections: Vec<Projection>` on `Model` — directly parallel to `CompositeIndex` /
  `composite_indexes`.
- **Parser** (`parser/core.rs`): `parse_directive` currently hard-errors on any directive but
  `"index"` (core.rs:397). Add a `"projection"` arm → `parse_projection_directive` that parses
  `(<name>: <field>, ...)` (a leading `Ident :` then the same comma-separated ident list
  `parse_index_directive` already reads). Collect into `Model.projections` in `parse_model`
  alongside `composite_indexes` (core.rs:709–792).

### Validation (`crates/validation`)

- Each projection field name resolves to a declared field of the model (else error).
- Each field is a **stored scalar or FK scalar** — reject `OneToMany`/`ManyToMany`/virtual/component
  fields (constraint 2) with a message pointing at eager-load for relations.
- Projection names are unique within a model and don't collide with a generated method/type
  (`card` → `PostCard` / `post_card`; guard against clashing with `find_by_*`, eager-load structs).
- Empty projection (PK only) allowed? Yes — a valid degenerate case (id-existence probe); document.

### Codegen (`crates/codegen/src/rust.rs`)

Per projection `P` with selected fields `F` (∪ PK):

- **Projection struct** `#[derive(Serialize, ...)] pub struct <Model><Pascal(name)> { pub id: ..,
  <selected typed fields> }` — tight types, PK + selected only.
- **Narrow decoder** `read_<snake(name)>_at(&self, row_index: usize) -> Option<<Model><Name>>`,
  emitted exactly like `generate_read_at_logic` but iterating only `PK ∪ F` and calling the shared
  `field_read_stmt` for each (constraint 1). Same tombstone gate as `read_at`.
- **Read methods** funnel through it, mirroring the full-record surface:
  `get_<name>(id) -> Option<Proj>`, `get_<name>_at(&Snapshot, id)`, `all_<name>() -> Vec<Proj>`,
  `all_<name>_at(&Snapshot)`. (Reader-handle `_at` variants on `DatabaseReader` optional, follow-up.)
- **No new decode logic** — this is subset emission over `field_read_stmt`. Factor the field-list →
  (struct fields, read stmts) loop so `generate_read_at_logic` and the projection path share it
  (the full record is just the projection whose field set = all stored fields).

### REST (`crates/codegen/src/api.rs`, constraint 6)

**`?projection=<name>` only** (locked decision 2) — no ad-hoc `?fields=`. This is the tightest
possible identity posture: the wire carries a **declared projection name**, not a runtime field
list, so there is *no* runtime column-set to parse. A generated per-model `match name { "card" =>
… }` invokes the same generated `read_<name>_at` decoder used by the Rust surface and serializes the
matching projection struct; an unknown name → 400; absent `?projection=` → full record (backward
compatible). The runtime-bitmask concern (strategy 3) evaporates entirely — every accepted value is
a compile-time-declared projection, so the closed-set requirement is satisfied by construction.
Applies to both `GET /api/<model>/{id}` and the list endpoint (`GET /api/<model>?projection=card` →
`{data: [PostCard], total, limit, offset}`).

### TS SDK (`crates/codegen/src/typescript.rs`)

- Emit a `<Model><Name>` TS type per declared projection and typed `getCard(id)` / `listCard(opts)`
  returns (calling `?projection=card`). Named projections keep tight typing on the client; there is
  no ad-hoc `Partial<Model>` path since the wire only accepts declared names.

### WASM replica (`crates/codegen/src/wasm.rs`, constraint 5)

- Extend `read_surface` so the generated `Replica` (and the async `ReplicaClient`) expose the
  projected reads. Because the projecting decoder only calls `field_read_stmt` on selected columns,
  the storage-web `LazySource` faults in only those columns — the headline working-set win. The
  `read_surface` enumerator stays the single source of truth mirrored by client + worker.

## Testing (codegen-must-be-compile-tested)

- **Guard** `test_rust_generation_column_projection`: a declared `@projection` emits the tight
  struct + narrow `read_<name>_at` that reads **only** PK + selected columns (assert selected column
  reads present, an unselected column's `read_*(row)` absent — same shape as
  `test_rust_generation_reopen_index_rebuild_is_narrow`), no `db.get(` full-record read, PK always
  present, and no second decode body (asserts reuse markers).
- **Validation** tests: reject relation-field projection; reject unknown field; duplicate name.
- **Snapshot** update for a projection-bearing schema.
- **E2E** (`scratchpad/projection_compile`, ephemeral): compile + read proving a projection
  materializes only PK + selected fields, values correct, across scalar/nullable/FK/timestamp; full
  `examples/` corpus compile-check.
- **WASM E2E** (extends #110 harness): a projected read on the follower does **not** fault in the
  unselected columns (console-diagnostic before/after, as `Tag.label` was proven).

## Explicitly out (this milestone)

- **Typestate builder** (strategy 2) — arbitrary ad-hoc compile-time-checked selections without
  declaring them. Deferred; composes on top later (shares the projection-struct emission). Backlog
  as the flexibility escape hatch once a partner hits the declare-too-many wall.
- **Runtime-bitmask Rust API** (strategy 3) — rejected as a typed Rust surface (loose all-`Option`,
  drift risk); its semantics live only in REST `?fields=`.
- **Projected relation traversal / eager-load** — a join, not a column read (constraint 2).
- **Reader-handle projected `_at` probes** on `DatabaseReader` — optional follow-up.
- **Projection push-down into `find_by_*` result shape** — probes still resolve full records for
  now; a `find_<name>_by_*` variant is a natural follow-up.

## Decisions (locked 2026-07-14)

1. **Directive syntax:** `@projection(card: title, slug)` — named-with-colon (reuses existing lexer
   tokens; generates `PostCard` / `post_card`).
2. **REST surface:** `?projection=<name>` **only** — no ad-hoc `?fields=`; absent = full record.
3. **First milestone:** full stack in one pass — Rust typed surface + REST + TS SDK + WASM
   fault-in proof.
