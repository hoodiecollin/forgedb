# CLAUDE.md

Guidance for Claude Code when working in this repository. Keep it accurate — if you
change the build, layout, or commands, update this file in the same change.

## What ForgeDB is

ForgeDB is an **application database generator** — a compile-time code generation tool,
**not** a runtime ORM or query engine. A declarative `.forge` schema is transpiled into
tailored Rust database code plus a TypeScript SDK, a REST API, and an OpenAPI spec.
End users need only: their schema, the `forgedb` CLI, and config.

**The invariant:** the app's schema is a *compile-time input to generation*, never a
*runtime input to a generic engine*. The schema-specific surface — types, tables, queries,
filters, relations, API routes — is generated and tailored per app. ForgeDB must never ship
a general-purpose library that reconstructs that surface at runtime by reflecting over a
schema.

**Publishing runtimes with programmatic APIs is expected, not forbidden** — two kinds, both
fine:
1. *Schema-agnostic substrate* the generated code links against: `forgedb-storage`,
   `forgedb-types`, `forgedb-wal`, and future peers (a stable FFI ABI, a change-feed /
   subscription transport, a backup format). Real programmatic APIs, but they know nothing
   about any specific schema.
2. *Access/transport layers over the generated surface*: language bindings (Python / Node /
   Deno FFI), a WASM host, a subscription socket. They expose the already-generated,
   schema-specific API to another language or channel; the tailored logic stays generated.

So generated code is **not** dependency-free — it depends on the schema-agnostic substrate
crates — but it never depends on a ForgeDB ORM or a runtime that reads the user's schema. A
generated, schema-tailored query/filter builder is fine (it is just generated code); a
generic, schema-agnostic query builder / ORM is not.

**Still rejected:** a generic ORM or dynamic query builder that interprets an arbitrary
schema at runtime; making the schema a runtime input to a general-purpose engine; or
hollowing out generated code by moving tailored logic into a shipped generic library.

Guard this when evaluating features (see the `forgedb-product-manager` subagent). The test:
(1) is the app's tailored data logic still **generated per-schema at compile time**, and
(2) does every published artifact stay **schema-agnostic substrate or transport glue**
rather than a generic runtime that interprets schemas? If either fails → reject or redesign.

## Toolchain

- **Rust edition 2024** across the whole workspace, inherited via `edition.workspace = true`
  from `[workspace.package]` in the root `Cargo.toml`. Requires Rust ≥ 1.85.
- Toolchain pinned in `rust-toolchain.toml` (channel `1.96`, with `rustfmt` + `clippy`).
- `Cargo.lock` **is committed** (this is a CLI/binary workspace).
- JS/TS tooling (vscode extension, inspector app, generated TS SDK): use **Bun**, not npm/node.
  TypeScript only, never plain JS.

## Build, test, run

```bash
cargo build --workspace              # build everything
cargo test  --workspace              # NOTE: see caveat below
cargo run   -- <command>             # run the CLI (binary is `forgedb`)
cargo run   -- --help                # list commands
cargo clippy --workspace             # no dead-code warnings (style lints remain, pre-existing)
```

CLI commands: `init`, `generate`, `validate`, `build`, `dev`, `migrate`, `compact`, `backup`,
`tenant` (`create|list|drop` — #59 multi-tenancy dir management),
`coordinate <root>` (#75/#84 MVCC Tier 3 — run the multi-process write coordinator for a data dir).
Example: `cargo run -- generate all --output ./generated`.

### Test baseline

Plain `cargo test --workspace --no-fail-fast` is **green**:

```bash
cargo test --workspace --no-fail-fast   # run the whole suite; read the `test result:` lines
cargo build --workspace --examples      # exit 0 — ALWAYS check examples too
```

- **`--no-fail-fast`** surfaces all results — cargo halts at the first failing binary
  otherwise.
- The integration tests (`tests/integration_test.rs`) are **hermetic** — CWD-dependent
  cases invoke the `forgedb` binary as a subprocess with an explicit `current_dir`, so
  they pass in parallel. No `--test-threads=1` workaround needed.
- **Compile the examples.** `--lib --bins --tests` and `--doc` both EXCLUDE examples, which
  silently broke twice; `cargo build --workspace --examples` is part of the baseline.
- **Codegen caveat (load-bearing):** the `crates/codegen` insta snapshot tests only compare
  generated code as *strings* — they do NOT compile it. When changing generators, generate
  for a real multi-model schema and `cargo check` the emitted Rust (`database.rs` +
  `api.rs`) in a throwaway crate; snapshot pass ≠ output compiles. This discipline caught
  3 real codegen bugs during Phase 3b.

**The exact pass count is intentionally NOT pinned here** — it changes every time a guard is
added and has been a chronic drift source (this doc has claimed 531 / 521 / 498 / 434 / … over
time; all were stale within a session or two). To get the *current* baseline, RUN it — the
runner is ground truth, prose is not:

```bash
# total passed/failed across the whole workspace (unit + integration + doctests)
cargo test --workspace --no-fail-fast 2>&1 \
  | awk '/^test result:/ {p+=$4; f+=$6} END {print p" passed, "f" failed"}'
```

(or read the per-binary `test result: ok. N passed; M failed; …` lines directly). What each
landed feature is guarded by is the guard test itself — grep `crates/codegen/tests/` for
`test_rust_generation_*` / `test_api_generation_*`; the test is the durable record of *what* is
tested, a running total is not. If a number ever looks off, trust the runner output, never a
count written in prose.

## Workspace layout

Root crate `forgedb` (`src/`) is the CLI: `src/main.rs` (clap), `src/commands/*`
(one module per subcommand), `src/{templates,ui,error}.rs`. It orchestrates the crates
in `crates/`:

**Published to crates.io — schema-agnostic substrate (independent version lines, do NOT normalize):**

> **Do not trust any version number, publish date, or publish-gap status written inline below —
> they drift. Derive the ground truth instead:**
> - *In-tree version* (source of truth for the working copy): `grep -H '^version' crates/*/Cargo.toml`
>   (or `cargo metadata --format-version 1`).
> - *What is actually published*: `cargo search forgedb-<crate>`, or the crates.io page.
> - *What generated code requires*: the scaffold pins emitted into the generated project's
>   `Cargo.toml` — grep the scaffolders for `forgedb-` dep lines (`src/commands/init.rs` for the
>   server scaffold; `crates/codegen/src/{wasm,napi,pyo3,transform,ffi}.rs` for the per-runtime ones).
>
> The invariant is the **publish-gap rule** (see **Operating disciplines** below): when
> generated code starts requiring a new substrate dep or additive substrate API, publish it *before*
> the scaffold pins it, then prove the reclose with an outside-repo `init → generate → cargo build`.
> The notes below describe each crate's **role and identity class**, which do not drift.

- `types` — core type system (uuid, timestamp, primitives). Carries a `cfg(wasm32)` uuid
  `js`/getrandom feature for the browser build (additive; native unchanged).
- `storage` — columnar storage **facade** (#110 Milestone C): `crates/storage` is a thin `cfg`
  re-export — `forgedb-storage-native` on host targets, `forgedb-storage-web` on `wasm32`. Generated
  code keeps `use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};` verbatim and stays
  byte-identical across targets. The historical monolithic engine is the native backend
  (`storage-native`, public surface unchanged — the facade is surface-compatible for host consumers);
  the browser arena backend is `storage-web` (in-memory arenas + IndexedDB/OPFS `persist`). Native
  capabilities the generated code drives directly (over `FixedColumn`/`VariableColumn`/`Tombstones`/
  `Manifest`/`DirLock`): `truncate_to_rows` + `DirLock` single-writer lock (#89), read-only column
  reader handles `*Reader` + `*::reader()` (#56-B), `Manifest` layout + `save_to/load_from` + `Snapshot`
  (#57 / #56-A), `sync_from_disk` peer read-currency (#75/#84). The legacy `Database` directory-manager
  wrapper was removed as zero-consumer dead code (#94/#99).
- `changefeed` — field-blind change-feed broadcast + **durable replication broker** substrate
  (#62-A / #82). The `durable` module (#82) is a `DurableBroker` that records each change to a
  CRC-framed, append-only log at a **monotonic global offset** (the opaque cross-model ordering token)
  + a resumable subscription (`read_from`/`subscribe`/`catch_up_from`, idempotent by absolute offset) —
  the substrate the WASM read-replica follower (#110) resumes from. Field-blind:
  `PersistedEvent { offset, model, row_index, kind, bytes }` carries the model name as an opaque tag and
  the committed row bytes verbatim (same class as the in-process feed). The generated `/replicate` WS
  endpoint streams its `to_wire()` binary frames. In-process feed carries
  `ChangeKind::{Inserted,Linked,Updated,Deleted}` (#66).
- `auth` — verify-only JWT + tenant cross-check substrate (#59). Schema-agnostic axum
  extractor/middleware: verifies an asymmetric JWT (JWKS or static PEM, algorithm-pinned,
  `exp`/`nbf`/`iss`/`aud`+skew), extracts a configured tenant claim, cross-checks it against the
  process's tenant → 403, injects an opaque `Principal`. Knows nothing of models/rows/schema — same
  class as `changefeed`.
- `wal` — write-ahead log. The generated durable write path (#89) links only the **opaque `Raw`**
  record path (schema-agnostic bytes + CRC framing + fsync policy + torn-tail `read_all` + `replay`);
  the pre-existing structured/field-decoding API (`WalValue`, `WalOperation`, `Transaction`/
  `replay_committed`) was **pruned as a drift vector** (#95, epic #94). `WalManager` is split per-target
  for the browser build — file impl `cfg(not wasm32)`, in-memory impl `cfg(wasm32)` (durable at commit
  granularity), `FsyncPolicy` at the crate root (shared). Additive `truncate_to` for MVCC Tier-1 txn
  rollback (#75/#84).
- `query-params` — REST query-string parser (#90). Schema-agnostic: parses a URL query string into
  generic `Filter`/`Sort`/`Pagination` (limit clamped to `MAX_LIMIT`); the generated `api.rs` list
  endpoint links it. Interprets no schema — all field-aware filtering/sorting is generated per-model —
  class-1 substrate, same class as `changefeed`/`auth`.
- `compaction` — in-process dead-row reclaim (#92 Phase 4 W1). Schema-agnostic byte GC keyed by model
  *directory name*: `Compactor::compact_model_keeping(model, live_rows)` keeps exactly the
  caller-supplied opaque row indices (generated code computes the live set). Generated
  `Database`/`Storage::compact()` link **only** `compact_model_keeping` (never `BackgroundCompactor`).
  The legacy tombstone path `compact_model` (resurrection-prone against #66) is **DEPRECATED** (doc-noted
  to keep the published API stable): the offline `forgedb compact`/`vacuum` CLI returns a deprecation
  error pointing to in-process `Database::compact()` (#105 RESOLVED). Deps serde/chrono/thiserror/log
  only — reads no `.forge`.
- `txn` — Tier 2 optimistic-concurrency commit sequencer (#75 MVCC). Schema-agnostic: a
  `CommitSequencer` that assigns a monotonic commit LSN and detects write-write conflicts over an
  in-memory `id → last-committer` map (rebuilt empty on open — conflict state is over in-flight txns
  only, never persisted). Knows no model/field; generated `Database::transaction` + concurrent-prepare
  code links `try_commit`/retry. Pure in-memory (no fs/net) → also linked by the wasm replica scaffold.
- `coordinator` — Tier 3 multi-process write **control plane** (#75/#84 MVCC). A standalone coordinator
  process (`forgedb coordinate <root>`) that holds the #89 `DirLock` on `<root>/.forgedb.lock` on behalf
  of all coordinated clients, serializes the commit turn, and sequences the LSN — the symmetric inverse
  of #82's durable broker. **NO `forgedb-storage*` dep (T3-8)** — it never writes columns or decodes
  opaque row bytes; the schema-aware column write stays in generated data-plane code run under a granted
  turn (coordinated clients open LOCK-FREE, `_lock: None`, mutually exclusive with a standalone
  self-locking writer — T3-5). Native-only (Unix sockets + `fs2`); the wasm replica cfg-gates the entire
  coordinator surface out. **#156 perf (2026-07-18):** the replication-log append + fsync barrier now runs
  under a **separate broker mutex, off the turn/condvar critical section** (Option A) — a committing client's
  disk barrier no longer blocks other writers from being granted a turn (broker appends stay in commit order
  because the handler holds the broker lock across the turn release, and only one turn is ever outstanding).
  The broker is opened `FsyncPolicy::Never` and the coordinator drives the barrier via a configurable
  **`CoordFsync`** (`forgedb coordinate --fsync always|never|periodic` / `FORGEDB_COORDINATOR_FSYNC`, default
  `always`; Option C) — which also fixed a latent N+1-fsyncs-per-commit (per-record + explicit flush) down to
  ≤1. The `_replication.log` is resumable/secondary (clients fsync their own columns+WAL before `Committed`),
  so `never`/`periodic` never risk committed client data — only rewind replication on a coordinator crash.

**Internal (compiler internals):** `parser`, `codegen`, `validation`, `migrations`, `backup`, `watcher`,
`lsp-server` are **published to crates.io** but **only** so `cargo install forgedb` can build the CLI from the
registry; per `docs/SEMVER.md` they are explicitly NOT a stable public API, unlike the substrate crates. Their
version lines drift independently — derive the current numbers per the *Workspace layout* note above (do not trust
prose). Historical shape: the internals were republished 2026-07-22 to carry the epic #173 API changes (positioned AST,
`enums`, `parse_recover`, `EnumDef`); `lsp-server` first-published under epic #173 when the root `forgedb` crate
gained an **optional** dependency on it (the non-default `lsp` feature drives the bundled `forgedb-lsp` binary, and
crates.io requires optional deps to resolve, so `forgedb-lsp-server` had to be published before `forgedb` next was).
The root `forgedb` crate is now published **with** that `lsp` feature (2026-07-27 republish cascade, which also bumped
`codegen` for the REST-SDK generators and `compaction`/`query-params` for bugfixes), so
`cargo install forgedb --features lsp` builds both `forgedb` + `forgedb-lsp` from the registry.
- `parser` — lexer + parser → AST (`crates/parser/src/ast.rs`)
- `codegen` — code generators; exports `RustGenerator`, `TypeScriptGenerator`,
  `ApiGenerator`, `StubGenerator`, and the REST client SDK generators
  `RustSdkGenerator` / `PythonSdkGenerator` / `GoSdkGenerator` (#206/#118/#205 — the
  Rust/Python/Go siblings of the TS SDK; `generate <rust|python|go> --sdk`) (each
  `::generate(&schema) -> GeneratedCode`)
- `validation`, `migrations`, `backup`, `changefeed`,
  `watcher`, `lsp-server`  (`query-params` + `compaction` are now **published** — see the
  published-crates list above)
  (`fulltext` + `crud-api` were removed in Phase 3b; `query-optimization` + `http-server` were
  removed by the legacy audit (#94) as zero-consumer dead code — the API existence/404 logic lives
  in the generated handlers, and the generated `api.rs` builds its own router. `ffi` — the pre-v1
  C-ABI bindings crate — and the legacy `npm-package/` Bun FFI runtime were removed 2026-07-15 as a
  clean slate for the bindings phase (#50–#53); they predated the generator-identity discipline
  (the npm-package shipped a generic runtime `QueryBuilder` — a red-line violation).)
  `query-params` (#90) is now **wired**: a schema-agnostic query-string parser (URL → generic
  `Filter`/`Sort`/`Pagination`) that the generated `api.rs` list endpoint links against — it interprets no
  schema (all field-aware filter/sort is generated per-model), so it is class-1 substrate the generated code
  links against, like `changefeed`/`auth`. Generated code requires it; **published 0.1.0 (2026-07-10)**.
  `backup` (#57) is a **class-1 substrate** peer to `compaction`: lock-free full-snapshot
  create/restore over a data dir as opaque bytes (reads per-model `manifest.json` + column
  files, never the `.forge` schema).
  `changefeed` (#62 Direction A) is a **class-1 substrate** the *generated code links against*
  (like `storage`/`wal`, not like the internal-only crates above): a field-blind
  `tokio::sync::broadcast` of `ChangeEvent { model: &'static str, row_index, kind }`. **Published
  0.1.0 (2026-07-07)**; the scaffold pins `forgedb-changefeed = "0.1"`. It never decodes a field;
  generated code routes by model name and materializes typed events.

Deeper docs live in `docs/` (`ARCHITECTURE.md`, `PUBLIC_CRATES.md`,
`DEVELOPMENT.md`, `PUBLISHING.md`, `CONTRIBUTING.md`).

### Generation pipeline

```
schema.forge → parser (lexer→AST) → validation → codegen
  ├─ RustGenerator       → database.rs
  ├─ TypeScriptGenerator → types.ts
  ├─ ApiGenerator        → api.rs
  ├─ StubGenerator       → placeholder stubs README (no UI/component codegen today)
  ├─ OpenApiGenerator    → openapi.json (offline OpenAPI 3.1)
  ├─ WasmGenerator       → replica/* (browser read-replica; opt-in)
  ├─ RustSdkGenerator    → rust-sdk/*   (reqwest REST client crate; `rust --sdk`, opt-in)
  ├─ PythonSdkGenerator  → python-sdk/* (stdlib-urllib REST client; `python --sdk`, opt-in)
  ├─ GoSdkGenerator      → go-sdk/*     (net/http REST client; `go --sdk`, opt-in)
  └─ TransformGenerator  → migrations/transform/* (offline data-migration bin)
```

Codegen uses `quote!`/`prettyplease` for Rust output and is snapshot-tested with `insta`
(`crates/codegen/tests/`). When changing generated output, review and accept snapshots.

## Schema language quick reference

Naming is **parser-enforced (fatal)**: models/structs PascalCase, fields snake_case.
Modifiers (prefix, before the type): `+` auto-generate (u32/u64/uuid/timestamp only — but only
`uuid`/`timestamp` are actually synthesized on create; integer `+u32`/`+u64` parse+mark but are
NOT auto-incremented, RFC #187), `&`
unique, `^` index; `?` nullable (postfix after type, or prefix on a model for an optional
FK). Types: `u32/u64/i32/i64/f64/bool/string/json/decimal/uuid/timestamp`, `char(N)` — **there is no
`text`**. `json` (→ `serde_json::Value`, rides the variable-length string column; NOT indexable/filterable/
sortable — no total order); `decimal` (→ `rust_decimal::Decimal`, exact fixed-point on the 16-byte column, string
serde, IS indexable/sortable via a scale-invariant normalized key — `decimal(p,s)` precision/scale deferred). Enums:
top-level `enum Name { A, B, C }` (PascalCase name + variants), referenced by bare name — 1-byte discriminant column,
serialized as the variant-name string, filterable/sortable (declaration order)/indexable. Relations: `[Model]`
one-to-many, `*Model` required FK, `?Model` optional FK, bidirectional `[..]`/`[..]` = many-to-many; `[type; N]`
fixed array; inline `struct` (fixed-size fields only — no string/relations inside). Directives: `@min @max @length
@email @url @default @index @computed @fulltext @materialized` (field-level, mostly semantic-only), **`@pattern`/
`@regex` ENFORCED** (per-field `LazyLock<Regex>`, non-match → 422; #104), **`@on_delete(restrict|cascade|set_null)`
ENFORCED** (relation-FK field; default `restrict` refuses deleting a referenced parent → 409, `cascade` recursive,
`set_null` optional-FK only), `@soft_delete` + composite `@index(a,b)` + `@projection(name: a, b)` (#113 —
model-level; generates a partial-read struct/methods over PK + the named columns), `@relations(*|fields)`
(component fields only). Component refs `tsx:// jsx:// api://`. Only `//` comments. Directive
args accept numbers, bare identifiers, **and quoted string literals** (`@pattern("^[0-9]+$")`,
`@default("pending")` — escapes `\" \\ \n \t \r`; `@default` still a semantic-only marker). **NOT
supported despite older docs:** `~` auto-update, `text` type, block comments
`/* */`. Full verified reference: `docs/SCHEMA.md`. **18 worked example schemas
across many domains live in `examples/` — see `examples/README.md`.**

## Current state & operating disciplines

ForgeDB is built **perimeter-first**: the advanced features (durable writes, MVCC transactions
+ multi-process writers, snapshots, live queries, multi-tenancy, backup, browser read-replica,
column projections, schema migrations) are real and shipped, but they sit on a generated core
that is still maturing. **Verify a maturity claim against code before trusting it.** The honest
scope — the single-writer-per-process contract, additive-only migrations, verify-only auth, and
every deferred limit — is stated in [`docs/WHAT_V1_IS.md`](docs/WHAT_V1_IS.md) and
[`docs/V1_ROADMAP.md`](docs/V1_ROADMAP.md).

### Where the truth lives (derive it; don't trust prose)

This file describes the **durable shape** of the project. Point-in-time state — what's landed,
what's next, exact versions — lives in ground truth, not here:

- **Feature status & backlog:** `gh issue list` (labels: `epic`, `plan-next`, `perf`, `config`,
  `idea`, `tech-debt`, `experiment`, `rfc`) + git history. The narrative is in
  [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/V1_ROADMAP.md`](docs/V1_ROADMAP.md).
- **What a feature is guarded by:** the guard test itself — grep `crates/codegen/tests/` for
  `test_rust_generation_*` / `test_api_generation_*`. The test is the record; a prose list drifts.
- **Substrate versions / publish status:** derive per the *Workspace layout* note above (grep
  `crates/*/Cargo.toml`, `cargo search forgedb-<crate>`, and the scaffold pins). Never trust a
  version written in prose.

### Project management (the ONE tracking model — no parallel systems)

ForgeDB follows the **ai-pm-playbook** (the portable form of the model worked out here; adopted
2026-07-31, which added `.github/ISSUE_TEMPLATE/*` and the `surface:*` labels). Section refs below
(§3.2, §4, §6.1, §9) are to that playbook. All work is tracked in **GitHub Issues** on
`hoodiecollin/forgedb`. There are exactly **two orthogonal axes**, and nothing else decomposes work:

- **Milestone = *when* (the release spine).** A core release milestone (`v0.3.0`, …) means
  *scheduled for that version*. Assigning a milestone is the only signal that something is
  scheduled.
- **Labels = *what kind / maturity*.** `epic` (umbrella), `rfc` (design captured as an issue —
  proposals are **not** committed as repo files), `experiment` (a spike to measure), `idea`
  (speculative; needs a design note first), `bug`, `tech-debt`, `perf`, `config`. Plus
  **`plan-next` = "committed but not yet scheduled to a version."**

**The commitment ladder** is the spine of the label axis — work is ranked by distance from shipped:
`idea` → *(Gate 1: accepted `rfc`)* → `plan-next` → *(assign milestone, drop `plan-next`)* →
in flight → closed-into-milestone → **released** (the GitHub Release tag). "Closed" and "shipped"
are distinct rungs: an issue closes into its milestone when code merges, and the roadmap keeps
reading *pending release* until the tag exists.

**Label invariants (§3.2) — enforce on every issue; violations are the #1 drift smell:**
`plan-next` ⊕ milestone · `idea` ⊕ `plan-next` · `idea` ⊕ milestone (scheduled implies committed) ·
`experiment` ⊕ {`idea`, `plan-next`, milestone}.

**Surfaces (§6.1).** A `surface:*` label marks a separately shippable product face. This repo uses
`surface:website` and `surface:ide-extension`; **core is the implicit default and carries no label.**
**Never put a non-core `surface:*` issue on a core `v*` milestone** — it would read as "done,
awaiting vX" though it already shipped on its own line, and would never reach the core changelog.
Non-core surfaces get their own namespace milestones (`vscode-v*`) or deploy continuously. The
core roadmap excludes them by label prefix — `apps/website/lib/roadmap-transform.ts`
(`isCoreScoped`) drops any `surface:*` that isn't `surface:core`, so a newly-added surface is
filtered the moment it exists.

**`experiment` never rides the release spine.** A milestone ships features/fixes/perf — things that
produce a binary a user installs. An `experiment` is *a spike to measure*; its deliverable is a
**decision**, not a shippable artifact. So an `experiment`-labeled issue is **never** put on a `v*`
milestone (same mutual-exclusion class as `idea` + `plan-next`), and it is likewise mutually exclusive
with `idea` (a spike you've committed to running is no longer merely speculative) and with `plan-next`.
Experiments run as an **unscheduled research track**; the *measured conclusion* may then commit new
feature work — and **that** work, not the spike, is what gets scheduled. Corollary: **never anchor a
milestone's theme on an experiment's hoped-for outcome** — you cannot schedule a feature whose existence
the experiment has not yet decided. The `experiment` label is the discipline test: if an issue's primary
output is a measurement/evaluation/feasibility verdict, it is an experiment (off the spine); if it is
shippable code that ships regardless of any measurement, it is a feature/perf/`config` item (on the spine).

**Epics decompose via GitHub *native sub-issues*** (the `Parent issue` / `Sub-issues progress`
link — `gh api repos/OWNER/REPO/issues/N/sub_issues`), *not* task-list checkboxes (secondary,
drift-prone) and *not* a labels/fields convention. An epic is a top-level container that MAY span
releases; each child carries its own milestone. The website `/roadmap` reads exactly this shape
(epics collapsible, standalone issues alongside) — see [`apps/website/lib/roadmap-transform.ts`].

**Retired — do NOT reintroduce:** the "workstream" decomposition (`WS1`/`WS2`/`Workstream 2` sub-
tasking of an epic) and the flat 5-bucket roadmap are **dead patterns**. Break an epic into real
child issues linked as native sub-issues; never invent `WSn` sub-labels or a parallel Project
field to slice work. The Project board (project #3) is a *view* over issues, never a second source
of truth.

### Operating disciplines (guidance, not changelog — follow these)

1. **Generator identity.** Every feature must keep the app's data logic *generated per-schema at
   compile time*, and every published artifact must stay *schema-agnostic substrate or transport
   glue*. Run the **`forgedb-product-manager`** subagent for any feature/architecture decision; if
   a change would make the schema a runtime input to a generic engine, reject or redesign. (Full
   statement: *What ForgeDB is*, above.)

2. **Publish-gap rule (most load-bearing operational discipline).** Generated code links the
   substrate crates by their scaffold-pinned versions. When generated code starts requiring a
   **new substrate dep or an additive substrate API**, publish the substrate crate to crates.io
   *before* the scaffold pins it, then prove the reclose with an **outside-repo**
   `forgedb init → generate → cargo build` that resolves the deps from the registry. An in-tree
   `cargo build` passing does **not** prove an installed user can build — only the outside-repo
   reclose does. Additive substrate changes (no on-disk format break) keep the scaffold pin
   (`= "0.2"`) resolving; a format break bumps the major and needs a migration path.

3. **Codegen is compile-tested, not just snapshot-tested.** The `insta` snapshots compare
   generated code as *strings* — a snapshot pass does **not** mean the output compiles. When you
   change a generator, generate for a real multi-model schema and `cargo check` the emitted crate
   (`database.rs` + `api.rs`). Also `cargo build --workspace --examples` — the default test flags
   exclude examples. (Both disciplines have caught real bugs; see *Build, test, run*.)

4. **Ground truth over sources of truth.** Code + git history + runtime/DB state are ground truth;
   this file, docs, memory, and issues are *claims* that drift. Run the **`sync-sources`** skill at
   task boundaries; when a claim disagrees with code, fix the claim. Do not pin an exact test count
   in prose (chronic drift source — run the runner).

5. **Design docs are not committed to the repo.** Proposals / design notes live as **`rfc`-labeled
   GitHub issues**, not files — run the **`rfc-workflow`** skill to file one (it has the gate, the
   dedup check, the body template, and the epic cross-link). (The historical `docs/proposals/` set
   was removed; git history holds it. Shipped-feature *architecture* reference belongs in
   `docs/ARCHITECTURE.md`.)

6. **Design → plan → spec, in series, before any code (§9).** Three gates, none of them a file:
   **Gate 1 — design-doc** (`rfc` issue: problem, desired behavior, solution *shape*, alternatives,
   explicit non-goals). Solution-shaped, not code-shaped; catches *conceptual* gotchas. Accepted →
   drop `idea`, add `plan-next`. **Gate 2 — implementation-plan** (files to touch, build order,
   interfaces, blockers, and the BDD scenarios to write), on the issue, after scheduling and before
   code; catches *execution* gotchas. **Gate 3 — BDD spec-first**: scenarios RED → implement to
   GREEN → refactor under green. State is **derived from the artifacts' existence**, never from a
   `needs-design`-style status label, and **effort labels are banned**. Templates for gates 1–2 live
   in `.github/ISSUE_TEMPLATE/`. When a feature ships, fold its durable design into
   `docs/ARCHITECTURE.md` and **close the `rfc`**.

### Current primary focus — storage-model experiment (epic #167)

Measure append-only vs in-place mutation rather than assert it. "Storage" conflates four
independent axes — **columnar layout** (keep; smallest footprint), **append-only mutation** (the
axis under test; buys snapshots / lock-free readers / MVCC / replica with no `xmin`/`xmax`, costs
churn growth + version indirection), **durability policy** (the `F_FULLFSYNC` barrier, orthogonal),
and **generated-read-path quality** (fixable in codegen). The append-only decision lives in codegen
(`crates/codegen/src/rust.rs`), not substrate, so in-place is a `GenConfig` variant through the
benchmark config matrix — not a second storage crate. **Phase 1** (read-path confound fixes, strict
wins) is **complete** (#168 column-pruned scan, #169 ordered/range index, #160 narrow
materialization, #170 group commit). **Phase 2** (the fixed-width in-place variant) is **RFC #172**.
Benchmark findings: [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md).

## Conventions

- No time estimates (hours/days/weeks) anywhere — describe scope, not duration.
- Don't `git commit` without the user's consent (an in-the-moment "commit when done"
  counts as consent for that scope; it doesn't carry to follow-up changes). When you do
  commit, split into small focused, conventional commits and include related lockfiles —
  and ALWAYS `git push` after a batch of commits (never leave the local branch ahead of
  origin). See the user's global git rules for the authoritative wording.
- When closing a TODO item, delete it (git history is the audit trail).
- All workflows runnable from the repo root — no `cd` into subdirs.

## Subagents

- `forgedb-product-manager` — product/architecture decisions; guards the generator identity.
- `forgedb-schema-author` — authors realistic `.forge` example schemas.
- `rust-core-library` — idiomatic Rust for core library/crate work.
