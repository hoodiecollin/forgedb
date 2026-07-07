# Proposal: Database Inspection Tool (Tauri)

**Status:** DESIGN NOTE — product-gated. `forgedb-product-manager` verdict: **aligned-with-constraints** (Option B-leaning hybrid; Option A rejected as the typed-data path). Awaiting maintainer approval to schedule.
**Issue:** [#63](https://github.com/hoodiecollin/forgedb/issues/63) (`idea`, `plan-next`)
**Date:** 2026-07-06

## Summary

A **standalone Tauri desktop inspector** for ForgeDB databases: schema explorer, data viewer,
relation graph, performance views, and a query UI. It is **dev/ops tooling that operates *on*
generated artifacts and a data directory** — a GUI peer to the `forgedb` CLI's
`validate`/`generate`/`compact`/`migrate`. It is **never compiled into the user's application**
and **never published as a crate the generated app links.**

The design rests on one load-bearing distinction that resolves the core identity question:
**decoding storage columns to raw scalars needs only `manifest.json` (substrate);
reconstructing the *tailored, schema-shaped* surface — typed records, relations, filters — must
come from generated code, not a generic decoder in the inspector.** The per-model `Manifest`
already carries `columns: Vec<ColumnMetadata>` (`name` + `ColumnType` + `column_index`) and
`row_count` (`crates/storage/src/lib.rs:172-206`), so structural and raw-scalar views are pure
substrate, exactly like `forgedb-backup`/`forgedb-compaction`. The moment you overlay `.forge`
to render `User { name, posts: [...] }`, you are reconstructing the tailored surface
generically — that belongs to the generated API server.

## Ruling on the core identity question

**A schema-reading *tool* is categorically fine; a schema-reading *runtime shipped as the
product or into the app* is the red line.** The guard forbids ForgeDB shipping "a
general-purpose library that reconstructs the schema-specific surface at runtime." That
prohibition is about **the deliverable the app depends on** — not about what a dev-time tool
may read. The `forgedb` CLI *is* a program that reads `.forge` schemas and manifests;
`validate`, `generate`, and `compact` all read schemas/manifests and none violate the
invariant, because none is linked into the generated app or sold as its runtime data engine.

So the inspector reading a schema/manifest to render a DB **does not cross the line** — on the
same grounds the CLI doesn't. What *would* cross the line:

1. Extracting the inspector's read path into **a crate the generated app imports** to access
   its own data generically at runtime. (It stays a devtool binary, never a library dependency.)
2. Letting the inspector's generic decoder become **the app's production data-access path**
   ("just point the SDK at the DB dir"). That is the generic engine wearing a GUI.

Invariant restated for this note: **the schema is a compile-time input to *generation* (which
produces the API the inspector talks to); the inspector may read schema/manifest as *tooling*,
but no artifact it uses may become a runtime dependency of the app or a shipped generic data
engine.**

The sharpest trap is Option A's typed/relational decode: reading the *manifest* to show raw
scalars is substrate (allowed, like backup); reading the *`.forge` schema* in the Tauri backend
to reconstruct the typed, relational surface is precisely the "generic engine" shape and is the
seed that gets promoted into an app runtime. We reject it as the primary typed-data path and
route typed/relational views through generated code instead.

## Product verdict & invariant mapping

**Aligned-with-constraints**, redesigned toward **Option B (inspector-over-generated-API)** with
an **Option C substrate layer for structural/performance views only**. **Option A (generic
schema-driven typed reader) is rejected** as the typed-data path; a manifest-only *raw-scalar*
at-rest view is permitted as clearly-labeled substrate tooling.

| Concern | Verdict |
|---|---|
| Does the inspector make `.forge` a *runtime input to a generic engine*? | **No** as designed — typed/relational data comes from the *generated* API; the schema is read only as tooling (schema explorer / relation graph), like the CLI. |
| Is any published artifact a generic runtime reconstructing schema logic? | **Must not be.** Ships as a **standalone binary**, never a crate the app links. Its structural backend links `forgedb-storage` as *substrate* only. |
| Does it add to "schema + CLI + config" authoring? | **No** — consumes existing artifacts (data dir, running `forgedb serve`, a generated introspection blob). |
| Does it touch generated code? | **Additively, once:** an optional generated **introspection/metadata endpoint** (schema shape as JSON). Generated code describing itself, not a runtime interpreter. |

The traps (why "with constraints"):
1. **"Query builder" is the drift word** — a hand-rolled generic query engine in the Tauri
   backend is the ORM red line wearing a GUI. Reconciled by routing all querying through the
   *generated, schema-tailored* filter surface exposed by the API server.
2. **"Data editor" collides with append-only storage** — generated code has no `update`/`delete`
   (`crates/codegen/src/rust.rs` — tombstones are append-only with no in-place setter), so a
   true editor has nothing to call. Milestone is read-only + create; editing is gated on a
   generated mutation surface existing at all.
3. **Option A is the seductive shortcut** — one generic decoder that opens any DB at rest is the
   most flexible and the most "generic engine." Permitted only at the manifest/raw-scalar
   substrate level; the typed/relational overlay stays generated.

## Where it lives

| Stratum | What it is | Guard class |
|---|---|---|
| **A. Tauri app (`src-tauri/` + frontend)** | Standalone desktop inspector binary. **Not** shipped into user apps; **not** a crate the app links. | Standalone tooling (like the CLI) |
| **B. Structural backend** — Tauri Rust links `forgedb-storage` + a manifest reader | `manifest.json` per model dir → row counts, column types/sizes, tombstone counts, file sizes. **Schema-agnostic.** | Class-1 *substrate* |
| **C. Typed-data path** — frontend → generated API (`forgedb serve`) + generated **introspection** | Schema-tailored records, filters, traversals stay **generated**; the inspector is a schema-agnostic client. | Class-2 *transport over the generated surface* |
| **D. Generated `database.rs` / `api.rs`** | Untouched except the **additive introspection endpoint** (schema-shape JSON). | Generated code (the product) |

**Explicitly NOT** a `forgedb-inspector` runtime crate the app imports, and **NOT** a generic
`Db::open(dir).query(model)` engine. The inspector is a separate binary; the app never knows it
exists.

## Frontend architecture & house style

The inspector's frontend follows the established house stack (see the sibling projects under
`~/Projects/*`), with the one divergence Tauri forces (static export).

**Stack (version-pinned to house canonical):**
- **Next.js 16.2.10**, app router, **static export** (`output: "export"` in `next.config.ts`) —
  Tauri serves the exported assets, so **client components only**; no server components,
  no API routes. React **19.2.7**, TypeScript **5.7** (strict, `moduleResolution: Bundler`,
  `paths: {"@/*": ["./*"]}`).
- **Tailwind v4** CSS-first: `@import "tailwindcss"` + `@theme { --color-* }` in
  `app/globals.css`; `postcss.config.mjs` = `@tailwindcss/postcss` only. Dark palette by default.
- **shadcn/ui** `style: new-york`, `rsc: false` (static/client), `cssVariables: true`; icons =
  **lucide-react**; `cn()` (clsx + tailwind-merge) in `lib/utils.ts`.
- **jotai 2.11** — `lib/atoms.ts`: `attachedServerUrlAtom` (the `forgedb serve` URL, or null),
  `openDataDirAtom`, `selectedModelAtom`, `viewModeAtom`; `atomWithStorage` for last-opened dir
  + recent servers; `<Provider>` in `app/providers.tsx` (`"use client"`).
- **bun 1.3.14 + Turbo 2.5** workspace; **Playwright** e2e (`e2e/`, `playwright.config.ts` with a
  `webServer` that boots the static export + a seeded `forgedb serve` fixture). Lint via **Biome**
  (newer house choice) or `tsgo --noEmit`.

**Two data paths, kept strictly separate (this separation IS the identity boundary):**
1. **Tauri IPC → Rust `src-tauri` commands** for **Stratum B substrate** (structural/perf, at
   rest): `open_data_dir(path)`, `list_models()`, `model_stats(model)` → manifest + `fs::metadata`.
   Schema-agnostic; never parses `.forge`.
2. **`fetch()` → the running generated API** (`forgedb serve`) for **Stratum C typed data**:
   list/`get`/traversal + the generated filter surface. The frontend is a schema-agnostic client
   of endpoints codegen produced.

Schema-shape (for the explorer + relation graph) comes from a **`forgedb schema --json` /
`generate introspection`** blob loaded via IPC — at rest, no server required.

**Placement:** a new `apps/inspector/` in the ForgeDB repo. `apps/inspector/src-tauri/` is a
**Cargo workspace member** (links `forgedb-storage` by path, substrate only); the Next.js
frontend sits at `apps/inspector/`. This introduces the repo's **first bun/Turbo JS workspace**
alongside the existing Cargo workspace. Per the "runnable from root" rule, add root entry points
(`bun inspector:dev`, `bun inspector:build`, or a Makefile target) wiring Tauri's
`beforeDevCommand`/`beforeBuildCommand` to bun; do not require `cd apps/inspector`.

## The shape of each sub-feature

### Schema explorer
Render models, fields, types, directives, relations from a **schema-shape JSON** produced by a
generated **introspection endpoint** (`GET /_introspect`) or a **CLI target**
(`forgedb schema --json`). Both green: the endpoint is *generated code describing its own
schema*; the CLI target is the generator reading `.forge` at tool-time. The frontend is a
schema-agnostic renderer. **Green.**

### Data viewer / editor
- **Viewer (milestone):** typed rows come from the **generated API** list/`get(id)` handlers —
  tailored logic executes in generated code; the inspector paginates and renders. **Green.**
- **Editor (deferred, gated):** generated code has **no `update` and no `delete`** — tombstones
  are append-only with no in-place setter, the same reason generated models have no delete.
  Ruling: (a) **create** (insert) can be supported via the generated insert/`POST` handler —
  that surface exists; (b) **edit/delete are blocked** and are a **storage + codegen decision**
  (add a generated mutation surface first), not an inspector problem. The inspector must **never**
  open a write path into storage that bypasses generated code. **Milestone = read + create;
  update/delete out.**

### Query builder
A **generic query builder in the Tauri backend is the ORM red line** and is rejected. The
"query builder" is a **frontend that composes calls to the generated, schema-tailored filter
surface** the API server exposes. The UI lets the user pick a model + generated filter
predicates and renders the response; **execution is generated code**, discovered via the
introspection JSON. No query AST is interpreted in the inspector. **Green with this constraint;
red if it grows its own execution engine.**

### Relation graph
Nodes = models, edges = relations (FK, one-to-many, M2M) from the **schema-shape JSON**
(relations aren't in the manifest). Pure visualization of introspected structure. **Green.**

### Performance views
Row counts, column sizes, tombstone (dead-row) counts + ratio, variable data/offset file sizes,
"compaction would reclaim N rows." Source: **manifest + file stat, schema-agnostic substrate**
(Stratum B) — the layout knowledge `compaction`/`backup` already use. Reads at rest, no server,
no schema. *(A `compaction_epoch` view is available once #57 lands that manifest field.)*
**Green — the cleanest part of the tool.**

## Red lines (specific to a GUI tool)

- **A `forgedb-inspector` crate the generated app links.** The inspector is a standalone binary;
  the app never depends on it.
- **A generic query/data engine in the Tauri backend** that opens a DB dir and answers
  `query(model).where(...)` by interpreting `.forge` at runtime. Querying rides the generated
  surface.
- **Option A's typed/relational decode as the primary data path** — reading `.forge` in the
  backend to reconstruct typed records + relations generically. Permitted **only** at
  manifest/raw-scalar substrate level, clearly labeled "raw column dump, no schema semantics."
- **A write/edit path into storage that bypasses generated code** (poking tombstones or
  appending columns directly). Mutations go through generated API handlers only.
- **Promoting any inspector read path into an app-runtime SDK** ("point this at your DB in
  prod"). The moment the decoder is offered as app data-access, it is the forbidden engine.

## Open questions / risks the design must resolve

1. **At-rest vs running-server access.** Structural/perf work at rest (manifest substrate);
   typed/relational/query require a running `forgedb serve`. Ruling: **accept the split** — open
   a data dir for structural views and *optionally* attach to (or launch) `forgedb serve` for
   typed views. Do **not** close the gap with an at-rest typed decoder (that's Option A).
2. **Data-editor vs append-only storage (the #1 conceptual risk).** Editing is blocked until a
   **generated `update`/`delete` surface exists** — a storage + codegen decision (tombstone
   setter / delete semantics), not an inspector feature. Flag to `rust-core-library` as the
   enabling dependency.
3. **Generic-query-builder-as-tooling vs the ORM red line.** Resolved by delegation: the UI
   composes requests against generated filters; it never interprets a query. Forbid a backend
   executor "for convenience."
4. **Own read path vs reuse generated code.** Structural/perf: **own substrate read** (manifest
   + stat), schema-agnostic, matches backup/compaction. Typed/relational: **reuse generated
   code** via the API. Keep the two paths strictly separated so the substrate read never grows
   schema awareness.
5. **Introspection source: generated endpoint vs CLI target.** Prefer a **CLI/generate target
   emitting schema-shape JSON** (works without a running server; feeds explorer + relation graph
   at rest) *and* a generated endpoint for live attach. Recommend the CLI/generate JSON first.
6. **Multi-model layout.** A ForgeDB DB dir is per-model subdirectories each with a manifest; the
   inspector enumerates model dirs to assemble the app view. Confirm the on-disk directory
   contract before building the enumerator.

## First milestone (smallest slice that proves the model *and* the guard)

**In scope**
- **Standalone Tauri app** at `apps/inspector/` (its own workspace; **not** a dependency of any
  generated app), frontend on the house stack (Next.js 16 static export + Tailwind v4 + shadcn +
  jotai), root-level `bun inspector:dev` entry point.
- **Structural/performance views at rest** (Stratum B, via Tauri IPC): enumerate model dirs, read
  each `manifest.json`, render model list, per-column type/size, row count, tombstone/dead-row
  count + ratio, file sizes. **Manifest substrate only — zero `.forge` reads, no running server.**
- **Schema explorer + relation graph** from a **`forgedb schema --json`** blob — a new, additive
  CLI/generate target emitting models/fields/types/relations as JSON. Frontend renders it; no
  runtime interpretation.
- **Read-only typed data viewer** for **one UUID-keyed example schema** by attaching to a running
  `forgedb serve` and consuming generated list/`get` + **one generated relation traversal**.
- **Identity proof:** verify the Tauri backend links only `forgedb-storage` (substrate) and never
  parses `.forge`; all typed/relational data arrives from the generated API; the inspector ships
  as a binary with **no crate the example app depends on**.

**Explicitly out**
- **Any data editing beyond (optionally) create** — update/delete blocked on a generated mutation
  surface that doesn't exist yet.
- **Any generic query builder that executes queries in the inspector** — querying rides generated
  filters only.
- **Option A at-rest typed/relational decode** (raw-scalar manifest dump may appear later,
  clearly labeled substrate).
- **Packaging any inspector read path as a crate the app links**, or as a prod data-access SDK.
- **Integer-PK typed views / full `examples/` corpus / M2M editing** — inherit the generator's
  UUID-only-traversal and append-only limits.

**Success = the inspector shows structural/perf views of a DB at rest from the manifest alone,
and typed rows + one relation from a running generated server, with its backend never parsing
`.forge` and never shipped as a crate the app depends on.** Needing a generic schema-driven
decoder to show typed data, or a query executor in the Tauri backend, is the drift signal.

## Load-bearing references

- `crates/storage/src/lib.rs:172-206` — `Manifest { schema_version, row_count, columns, … }` +
  `ColumnMetadata { name, column_type, column_index }` + `ColumnType`: the substrate powering
  structural/perf and raw-scalar views **without** `.forge`. (`compaction_epoch` for a perf view
  arrives with #57.)
- `crates/codegen/src/rust.rs` — generated append-only tombstone writes with **no in-place setter
  / no delete**: why a data *editor* has nothing to call today.
- `crates/codegen/src/api.rs` — the generated API/handler surface the inspector consumes; where a
  generated introspection endpoint would land.
- `src/commands/serve.rs` — `forgedb serve` launching the generated API over sockets: the
  running-server the typed-data path attaches to.
- `docs/proposals/backup-restore.md` — the precedent that reading `manifest.json` as opaque
  layout is *substrate*, not a schema-runtime; the inspector's Stratum B mirrors it.
- Frontend house style: sibling projects under `~/Projects/*` (Next.js 16.2.10 / Tailwind v4
  `@theme` / shadcn new-york / jotai 2.11 / bun 1.3.14 + Turbo 2.5). Tauri is a new pattern —
  static export (`output: "export"`), client-components-only, data via IPC + the generated API.
- `CLAUDE.md` → "What ForgeDB is" — the invariant, plus the append-only / no-update-or-delete /
  UUID-only-traversal limits this note inherits.

## Enabling dependencies (cross-issue)

- **Generated introspection/metadata** (schema-shape JSON): a new additive CLI/generate target
  and/or `GET /_introspect` endpoint. Feeds the schema explorer, relation graph, and query UI.
- **Generated mutation surface** (`update`/`delete`): storage (tombstone setter / delete
  semantics) + codegen work; unblocks the data *editor*. Currently absent by design (append-only).
- **`compaction_epoch` in `Manifest`** (from [#57](https://github.com/hoodiecollin/forgedb/issues/57)):
  enables the compaction/epoch performance view.
