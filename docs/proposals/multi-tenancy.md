# Proposal: Multi-tenancy

**Status:** DESIGN NOTE — product-gated. `forgedb-product-manager` verdict: **aligned-with-constraints** (one of the strongest codegen stories in the backlog). Maintainer-blessed direction (2026-07-06): **layered roadmap — Layer 1 physical isolation first (milestone), Layer 2 row-level `@tenant` scoping next (gated on the mutation surface); per-tenant *schemas* rejected**. Tenant lifecycle = **explicit `forgedb tenant` CLI**; multi-tenant serve = **one process + tenant registry**. Awaiting approval to schedule Layer 1.
**Issue:** [#59](https://github.com/hoodiecollin/forgedb/issues/59) (`idea`, `plan-next`)
**Date:** 2026-07-06

## Summary

Multi-tenancy is a **generation** story: emit tenant scoping *into* the already-tailored
`database.rs`, and open a tenant's data under its own root. The `.forge` schema stays a
compile-time input; the tenant *value* is a runtime input **to generated code**, never to a
generic policy interpreter. That is the invariant's happy path, not its edge.

The issue bundles three architecturally-distinct isolation models. The blessed plan is a
**layered roadmap**:

- **Layer 1 — Physical isolation (milestone).** Each tenant gets its own data directory;
  `Database` is opened per-tenant-root. Isolation is **filesystem-enforced**, needs **zero
  `.forge` change**, and carries **no query-logic leak risk**. Its one prerequisite is a real,
  independently-useful fix (below).
- **Layer 2 — Row-level tenancy (next).** A `@tenant` model directive; the generator emits a
  stored `tenant_id`, tenant-scoped getters, and **no unscoped getter** for tenant models
  (compile-time RLS). **Gated on the generated mutation surface** — "RLS" is half-hollow until
  `update`/`delete` exist to protect. Composes *within* a physical dir.
- **Per-tenant schemas — rejected (category error).** "Different schema per tenant" = different
  generated applications. A single artifact serving N schemas *is* the forbidden runtime-schema
  engine.

## The independently-useful prerequisite (and a wart fix)

The generated `Database` today builds every column from a **hardcoded, CWD-relative path** —
`FixedColumn::new(PathBuf::from("user/fixed/id_0.bin"))`, `Tombstones::new(PathBuf::from(
"user/tombstones.bin"))` (`crates/codegen/src/rust.rs:355-390`). It does **not** call substrate
`Database::open(root)` and **ignores** the `FORGEDB_DATA=./data` env `forgedb serve` sets
(`src/commands/serve.rs:180`). So a generated DB can only live at CWD.

**Parameterizing the generated root — `Database::open_at(root: PathBuf)` threading `root`
through every column/tombstone init — is both the Layer 1 prerequisite and a standalone fix for
that wart.** The substrate is already tenant-root-shaped (`Database::open(root_path)` holds a
`root_path` and joins under it, `crates/storage/src/lib.rs:658`); only the generated wrapper
isn't.

## Identity verdict & the single drift vector

**Inside the guard — and a model codegen story.** Row scoping and per-tenant directories are
exactly the tailored logic a generator should bake into `database.rs` per model. The substrate
never learns the word "tenant."

**The one drift vector (sharp):** a **runtime tenant-resolution / RLS *engine*** — a shipped
library that at request time reads `.forge` (or a model-keyed policy file mirroring it) to decide
"which column is the tenant key for model X and how do I filter it." That is the ORM red line
wearing a security hat: the schema becomes a runtime input to a generic engine. The guard-passing
shape is the mirror image — the generator reads `.forge` at **compile time** and emits, per
model, a scoping predicate hardcoded into that model's getters. **Tenant scoping is a
compile-time input to generation; the tenant value is a runtime input to generated code — never a
runtime input to a generic policy interpreter.**

## The three isolation models

### Physical / per-tenant-database (Layer 1) — cleanest, strongest
Each tenant's data lives under `<root>/<tenant>/`; a `Database` per tenant reads/writes there.
Isolation is **filesystem-enforced, not logic-enforced** — tenants share no columns, so no
generated query can leak across tenants. No `.forge` change, no directive, no new query
semantics. Cost: no cross-tenant queries (usually the point), per-tenant resource overhead, and a
tenant lifecycle to manage (CLI). This is the milestone.

### Row-level tenancy (Layer 2) — green with discipline, gated on mutation
Tenant models carry a `tenant_id`; the generator emits scoped `get`/`all`/traversal taking a
`TenantId` and — the RLS-defining move — **no unscoped getter at all** for tenant models, so
unscoped access is *uncompilable*, not merely discouraged. That is compile-time RLS, genuinely
more than a doc convention. **Honest limit:** RLS's value is enforced isolation *across all
operations*, but the mutation surface (`update`/`delete`) doesn't exist — so a Layer-2 milestone
could only guarantee **scoped insert + scoped read**. Cross-tenant update/delete protection
comes free later precisely because those methods don't exist to be unscoped yet. Hence Layer 2 is
sequenced **after** the generated mutation surface (shared prerequisite with #56 and #63) and
co-designed with it — turning it from greenfield into an increment. It composes *within* a
physical dir (hard isolation between customer classes, row-level within).

### Per-tenant schemas — rejected (category error)
ForgeDB generates code *from a schema at compile time*. "Different schema per tenant" means
*different generated applications*; the single-artifact version requires exactly the
runtime-schema-interpretation engine the invariant forbids (the reddest line in the project). The
two legitimate reframes — (a) a superset schema with nullable optional columns (pure generation,
no new feature), or (b) genuinely separate generated apps deployed side by side (an ops pattern)
— point *away* from a ForgeDB feature. The note rejects 2c outright so it doesn't reappear as
"just let the engine read a per-tenant schema fragment."

## Blessed roadmap (layered)

**Layer 1 — Physical (milestone).**
- Generated `Database::open_at(root: PathBuf)` — thread `root` through every column/tombstone
  init (replaces today's hardcoded relative paths).
- **Tenant registry in `serve`:** map tenant identifier → `Database::open_at(root.join(tenant))`;
  a single process holds the registry (natural step from today's one `Arc<RwLock<Database>>`
  state, `crates/codegen/src/api.rs:273`).
- **Tenant resolution:** an axum extractor derives the tenant from the request
  (header/subdomain/claim) — **transport glue** (Class-2) — and dispatches to that tenant's
  `Database`. Filtering/isolation is the directory boundary, not query logic.
- **`forgedb tenant create | list | drop <name>` CLI** — makes/lists/removes tenant dirs; a peer
  to `migrate`/`compact`. Tenant existence is explicit and auditable (blessed over
  dir-on-first-write).

**Layer 2 — Row-level (next, gated on mutation surface).**
- `@tenant` model-level directive (parser + AST flag on `Model`, mirroring `soft_delete` at
  `crates/parser/src/ast.rs:124-129`).
- Codegen: stored `tenant_id` column; scoped `get`/`all`/traversal taking `TenantId`; **suppress
  unscoped getters** for tenant models. API tenant extractor feeds the scoped calls.
- Sequenced after `update`/`delete` land, so RLS protects the full operation set.
- Composes within a physical dir (Layer 1 + Layer 2 = hard isolation between classes, row-level
  within).

**Build the layers sequentially, not at once** (blessed) — Layer 1 is a self-contained milestone;
Layer 2 is a well-motivated increment once mutation exists.

## Red lines

- **No runtime tenant-resolution / RLS engine that reflects over schema.** No shipped library that
  at request time reads `.forge` (or a model-keyed policy file) to decide the tenant column/filter
  for arbitrary models. Scoping is generated per model at compile time.
- **RLS = generated per-model scoping, not a generic policy interpreter.** A
  `PolicyEngine::check(model, row, tenant)` dispatching on model name is the ORM red line. The
  generated `User` getters simply *are* scoped; there is no dispatcher.
- **Tenant context must not require the app to import a ForgeDB runtime.** The tenant is a plain
  value (a method arg / an axum extractor in generated code). No `forgedb-tenancy` crate the
  generated app links to resolve tenants.
- **`.forge` never becomes a runtime input** to any tenant path — not even a "just the
  tenant-relevant fragment" fast path.
- **No cross-tenant leak via a generic query path.** Don't add a generic `query(model, filter)`
  "so admins can go cross-tenant." Cross-tenant admin reads, if needed, are *generated* explicit
  methods.
- **Physical isolation stays substrate + generated call + CLI ops** — `Database::open(root)` per
  tenant, never a schema-reading multiplexer.
- **Authoring surface stays "schema + CLI + config."** Tenant policy config (if any) lives in
  `forgedb.toml` or is generated; the only acceptable `.forge` addition is the `@tenant` marker
  (Layer 2 only).

## First milestone (Layer 1 — physical, smallest slice)

**In scope**
- **Parameterize the generated `Database` root:** `Database::open_at(root: PathBuf)` threading
  `root` through every column/tombstone init (replacing hardcoded relative paths). Independently
  useful; fixes the CWD-relative wart.
- **Per-tenant instantiation in `serve` + tenant registry:** tenant identifier →
  `Database::open_at(root.join(tenant))`; a request extractor resolves the tenant and dispatches.
- **`forgedb tenant create | list | drop <name>` CLI** (a `migrate`/`compact` peer).
- **Identity proof:** two tenants, one running app; tenant A's requests observe only A's dir; no
  generated query code references `.forge` at runtime; the only substrate call is `Database::open`
  per tenant; the generated app links no tenancy crate.

**Explicitly out**
- **Row-level tenancy / `@tenant` / any `.forge` change** (Layer 2) — deferred until the generated
  mutation surface exists, then co-designed with it.
- **Per-tenant schemas** (rejected category error; reframes documented above).
- **Cross-tenant queries / admin multiplexer.**
- **Any runtime policy engine or `forgedb-tenancy` crate.**
- **Process-per-tenant serve** (blessed: one process + registry).
- **Tenant scoping of `update`/`delete`** — those methods don't exist yet.

**Success = one `forgedb serve` process serving N filesystem-isolated tenants, isolation
guaranteed by the directory boundary (not by query logic), with `.forge` untouched, no schema
read at runtime, and no tenancy crate linked into the app.** Reaching for a schema-reading tenant
resolver, or a `@tenant` directive to ship *this* slice, is the drift signal.

## Open questions carried forward (for Layer 2)

1. **Tenant context delivery for row-level:** compile-time-required method arg (`db.users(tenant)`)
   vs API-request-derived extractor — likely both (the arg is the primitive; the extractor feeds
   it). Decide when Layer 2 is scheduled.
2. **Isolation-guarantee strength expected of row-level:** generated-code-only (a codegen bug =
   a leak) — acceptable given Layer 1 provides the filesystem-level guarantee underneath. Confirm
   at Layer 2 time.
3. **Composition semantics:** how `@tenant` row-scoping and physical dirs interact when both are
   used (nested tenancy) — define when Layer 2 lands.

## Cross-issue dependency

Layer 2's "RLS" is gated on the **generated mutation surface** (`update`/`delete`) — the same
prerequisite named in the MVCC (#56) and inspector (#63) notes. Sequence Layer 2 after that
lands; it becomes an increment, not greenfield.

## Load-bearing references

- `crates/codegen/src/rust.rs:355-390` — generated column/tombstone inits at hardcoded
  model-relative paths; the no-root `Database::new()` Layer 1 must parameterize into `open_at`.
- `crates/storage/src/lib.rs:658` — substrate `Database::open(root_path)`, already tenant-root-
  shaped (holds `root_path`, joins under it) — what the generated wrapper links per tenant.
- `crates/codegen/src/api.rs:136-181,273` — axum `State<Arc<RwLock<Database>>>` + extractors +
  `create_router(db)`: where the tenant registry + resolution extractor land.
- `crates/parser/src/ast.rs:124-129` — `Model.soft_delete`: the precedent for a `@tenant` model
  directive if Layer 2 is built.
- `src/commands/serve.rs:180` — `FORGEDB_DATA=./data` env, currently ignored by generated code
  (the wart `open_at` closes).
- `CLAUDE.md` → "What ForgeDB is" — the invariant, plus the no-update-or-delete storage limit
  that gates Layer 2.
