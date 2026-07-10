# CLAUDE.md

Guidance for Claude Code when working in this repository. Keep it accurate — if you
change the build, layout, or commands, update this file in the same change.

## What ForgeDB is

ForgeDB is an **application database generator** — a compile-time code generation tool,
**not** a runtime ORM or query engine. A declarative `.forge` schema is transpiled into
tailored Rust database code plus a TypeScript SDK, a REST API, and React component stubs.
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
- JS/TS tooling (npm-package, vscode extension, Bun runtime): use **Bun**, not npm/node.
  TypeScript only, never plain JS.

## Build, test, run

```bash
cargo build --workspace              # build everything
cargo test  --workspace              # NOTE: see caveat below
cargo run   -- <command>             # run the CLI (binary is `forgedb`)
cargo run   -- --help                # list commands
cargo clippy --workspace             # no dead-code warnings (style lints remain, pre-existing)
```

CLI commands: `init`, `generate`, `validate`, `build`, `dev`, `migrate`, `compact`, `backup`, `serve`,
`tenant` (`create|list|drop` — #59 multi-tenancy dir management).
Example: `cargo run -- generate all --output ./generated`.

### Test baseline

Plain `cargo test --workspace --no-fail-fast` is **green**:

```bash
cargo test --workspace --no-fail-fast   # 434 pass, 0 fail (incl. doctests)
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

**Baseline: 461 tests pass** (workspace, incl. doctests). 434→447 with #48 join predicate
pushdown (9 planner tests) + #49 restored OpenAPI generation (4 codegen tests); 447→460 with #89
durable write path + #95 wal prune (net +13: new `forgedb-wal` `Raw`-path + `forgedb-storage`
`truncate_to_rows`/`DirLock` tests + 1 `test_rust_generation_durable_write_path` codegen guard, minus
the deleted structured/transaction wal tests); 460→461 with #96 WAL checkpoint (1
`test_rust_generation_wal_checkpoint` codegen guard). Earlier: 419→432 with #59 multi-tenancy (11
`forgedb-auth` verify tests + 2 codegen guards); 432→434 with #69 generated REST update/delete
(1 codegen guard) + #71 inspector db-name (1 src-tauri test). Dropped from 531
when the orphaned `fulltext` + `crud-api` crates were removed in Phase 3b. Ignore older claims of
"531"/"521"/"466"/"447"/"434"/"432"/"419"/"417"/"411"/"409"/"403"/"399"/"398"/"394"/"380".

## Workspace layout

Root crate `forgedb` (`src/`) is the CLI: `src/main.rs` (clap), `src/commands/*`
(one module per subcommand), `src/{templates,ui,error}.rs`. It orchestrates the crates
in `crates/`:

**Published to crates.io (independent version lines, do NOT normalize):**
- `types` — core type system (uuid, timestamp, primitives) — **0.2.0**
- `storage` — columnar storage engine (positional-I/O fixed columns + append-only variable) — **0.1.5
  (unpublished; 0.1.4 published 2026-07-08)** (0.1.5 adds `truncate_to_rows` on the three column types +
  `DirLock` single-writer advisory lock, both for #89 durable writes; 0.1.4 adds read-only column reader handles
  `FixedColumnReader`/`VariableColumnReader`/`TombstonesReader` + `*::reader()` for #56-B single-writer/
  many-reader; 0.1.3 added `Manifest` layout fields + `Manifest::save_to/load_from` + `Snapshot` for #57
  backup / #56-A snapshot reads)
- `changefeed` — field-blind change-feed broadcast substrate (#62-A) — **0.1.1 (published 2026-07-08;
  0.1.1 adds `ChangeKind::Updated`/`Deleted` for #66; 0.1.0 published 2026-07-07)**
- `auth` — verify-only JWT + tenant cross-check substrate (#59) — **0.1.0 (published 2026-07-09)**.
  Schema-agnostic axum extractor/middleware: verifies an asymmetric JWT (JWKS or static PEM,
  algorithm-pinned, `exp`/`nbf`/`iss`/`aud`+skew), extracts a configured tenant claim, cross-checks it
  against the process's tenant → 403, injects an opaque `Principal`. Knows nothing of
  models/rows/schema — same class as `http-server`/`changefeed`.
- `wal` — write-ahead log — **0.2.0 (unpublished; 0.1.1 published)**. The generated durable write path (#89)
  links only the **opaque `Raw`** record path (schema-agnostic bytes + CRC framing + fsync policy + torn-tail
  `read_all` + `replay`); the pre-existing structured/field-decoding API (`WalValue`, `WalOperation::{Insert,
  Update,Delete}`, `Transaction`/`replay_committed`) was **pruned as a drift vector** (#95, legacy-audit epic
  #94) — 0.2.0 is that breaking removal.

**Internal (0.1.0):**
- `parser` — lexer + parser → AST (`crates/parser/src/ast.rs`)
- `codegen` — code generators; exports `RustGenerator`, `TypeScriptGenerator`,
  `ApiGenerator`, `StubGenerator` (each `::generate(&schema) -> GeneratedCode`)
- `validation`, `migrations`, `compaction`, `backup`, `changefeed`, `query-optimization`,
  `query-params`, `http-server` (axum), `watcher`, `lsp-server`, `ffi`
  (`fulltext` + `crud-api` were removed in Phase 3b — orphaned runtime-library crates
  with zero consumers; the API existence/404 logic now lives in the generated handlers.)
  `backup` (#57) is a **class-1 substrate** peer to `compaction`: lock-free full-snapshot
  create/restore over a data dir as opaque bytes (reads per-model `manifest.json` + column
  files, never the `.forge` schema).
  `changefeed` (#62 Direction A) is a **class-1 substrate** the *generated code links against*
  (like `storage`/`wal`, not like the internal-only crates above): a field-blind
  `tokio::sync::broadcast` of `ChangeEvent { model: &'static str, row_index, kind }`. **Published
  0.1.0 (2026-07-07)**; the scaffold pins `forgedb-changefeed = "0.1"`. It never decodes a field;
  generated code routes by model name and materializes typed events.

Deeper docs live in `docs/` (`ARCHITECTURE.md`, `PUBLIC_CRATES.md`, `INTERNAL_CRATES.md`,
`DEVELOPMENT.md`, `PUBLISHING.md`, `CONTRIBUTING.md`).

### Generation pipeline

```
schema.forge → parser (lexer→AST) → validation → codegen
  ├─ RustGenerator       → database.rs
  ├─ TypeScriptGenerator → types.ts
  ├─ ApiGenerator        → api.rs
  ├─ StubGenerator       → React/route stubs
  └─ OpenAPI             → DISABLED (see Known issues)
```

Codegen uses `quote!`/`prettyplease` for Rust output and is snapshot-tested with `insta`
(`crates/codegen/tests/`). When changing generated output, review and accept snapshots.

## Schema language quick reference

Naming is **parser-enforced (fatal)**: models/structs PascalCase, fields snake_case.
Modifiers (prefix, before the type): `+` auto-generate (u32/u64/uuid/timestamp only), `&`
unique, `^` index; `?` nullable (postfix after type, or prefix on a model for an optional
FK). Types: `u32/u64/i32/i64/f64/bool/string/uuid/timestamp`, `char(N)` — **there is no
`text`**. Relations: `[Model]` one-to-many, `*Model` required FK, `?Model` optional FK,
bidirectional `[..]`/`[..]` = many-to-many; `[type; N]` fixed array; inline `struct`
(fixed-size fields only — no string/relations inside). Directives: `@min @max @length @email
@url @pattern @regex @default @index @computed @fulltext @materialized` (field-level, mostly
semantic-only), `@soft_delete` + composite `@index(a,b)` (model-level), `@relations(*|fields)`
(component fields only). Component refs `tsx:// jsx:// api://`. Only `//` comments. Directive
args accept numbers, bare identifiers, **and quoted string literals** (`@pattern("^[0-9]+$")`,
`@default("pending")` — escapes `\" \\ \n \t \r`; still semantic-only markers). **NOT
supported despite older docs:** `~` auto-update, `text` type, `@on_delete`, block comments
`/* */`. Full verified reference: `docs/proposals/corpus/forge-grammar-reference.md`. **18 worked example schemas
across many domains live in `examples/` — see `examples/README.md`.**

## Known issues / backlog

- **Core is not yet production-complete — ForgeDB is built perimeter-first (v1 roadmap: `docs/V1_ROADMAP.md`).**
  The advanced, well-documented features below (live queries, multi-tenancy, snapshots, backup, single-writer/
  many-reader) are real but sit on a **generated core with critical gaps** that the "LANDED" bullets can obscure.
  Verify against code before trusting a maturity claim. As of 2026-07-10, four probes established:
  - **Generated writes are now crash-safe (v1 Phase 1 step 1 — #89 LANDED, unpublished).** Generated
    `insert/update/delete` record an **opaque** row blob to a per-model WAL (`forgedb-wal` `Raw` op) + fsync
    (`FsyncPolicy::Always`) BEFORE touching columns; `new_at` runs generated per-model `recover_from_wal`
    (truncates a torn column tail back to the min-consistent prefix, then idempotently replays the WAL tail by
    absolute row index); `open_at` holds a `forgedb_storage::DirLock` so a second writer refuses (never
    corrupts). Substrate: `forgedb-wal` **0.2.0** (opaque `Raw` path; structured/txn API pruned — #95),
    `forgedb-storage` **0.1.5** (`truncate_to_rows` + `DirLock`). Proven E2E (torn-tail repair, lost-committed-row
    recovery, **`kill -9` mid-write → 0 acked rows lost**, second-writer refused) in `scratchpad/durable_compile`
    through current codegen; guard `test_rust_generation_durable_write_path`. **Honest limits / deferred:**
    `fsync` policy is fixed (not yet config); requires publishing `forgedb-wal 0.2.0` + `forgedb-storage 0.1.5`
    (see the publish gap). Single-writer-per-process is the v1 contract; multi-writer (Direction C) stays out.
  - **The WAL is now bounded (v1 Phase 1 step 2 — #96 LANDED, unpublished).** A generated `checkpoint()` fsyncs
    every column + tombstone **then** truncates the WAL (order load-bearing: columns durable before the WAL is
    discarded), auto-invoked once `writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL` (fixed generated const =
    1000; not config — same posture as the fixed fsync policy); `Database::checkpoint()` forces it across all
    collections (junctions have no WAL — a #89 boundary — but still fsync their id columns). **No new substrate** —
    reuses `forgedb_storage` column `flush()` + `forgedb_wal::WalManager::truncate()`. Recovery is unchanged and
    still correct: it derives the durable prefix from the *column file lengths* (self-describing append-only
    columns), NOT a persisted checkpoint LSN, so no durable marker is load-bearing; the manifest's `last_checkpoint`
    is now set truthfully (was hardcoded `0`) for observability only. Proven E2E (explicit checkpoint truncates the
    WAL; auto-checkpoint keeps it bounded/sawtoothed under 2137 rows — final 15 KB < peak 107 KB; a crash after
    checkpoint recovers 23 rows from a 309-byte WAL, no whole-history replay) in `scratchpad/durable_compile`;
    guard `test_rust_generation_wal_checkpoint`. **Deferred:** the interval is not yet tunable (a knob rides the
    Phase 4 bounded-storage follow-up #92); junction (M2M) link *crash recovery* remains a #89 boundary.
  - **The generated REST list endpoint is a stub** returning `{"data":[]}` (`crates/codegen/src/api.rs:168-171`) —
    the generated API can currently only get-by-id. → v1 Phase 2 (#90).
  - **Indexes are decorative.** `^index`/`&unique`/`@index(a,b)` parse but no index structures are generated;
    every non-PK lookup is a full scan. → v1 Phase 2 (#90).
  - **No constraint/validation enforcement.** Duplicate `&unique` + dangling required-FK are allowed;
    `@min/@max/@length/@email/@pattern` are ignored at write. → v1 Phase 3 (#91).
  - **Migrations are infrastructure without an engine.** The executor errors on type-change/remove/index ops;
    `AddField` doesn't backfill; auto-diff is hardcoded-disabled. v1 ships **additive + documented reload** only
    (data-transform engine deferred). → v1 Phase 4 (#92).
  - **Compaction works but is manual-only** — storage grows unbounded until `forgedb compact` is run. → v1 Phase 4 (#92).

  The gaps are mostly codegen *wiring* to substrate that already exists (`wal`, `compaction`, `query-params`).
  v1 scope is locked: **design-partner bar, single-writer-per-process, migrations data-engine deferred** — see
  `docs/V1_ROADMAP.md` and epics #89–#93.

- **Dead-code warnings: 0** (all 9 from the Phase 3b sweep resolved). Eight were WIRED
  (`build --no-api`, `validate --components`, the `--config`/`Config`/`CliError::exit_code`
  config feature, LSP struct-awareness) or REMOVED (`build --no-db`, `init --typescript`,
  `rust_main_template`, LSP `Document.uri/version` + `get_document`). The ninth,
  `validate --implementations`, is **kept but `#[allow(dead_code)]`-annotated**: the flag is
  accepted as a documented no-op until the `@computed` convention lands.
- **`@computed` convention = schema expressions (deferred).** The chosen design is
  `@computed(<expr>)` — the field carries an expression the generator compiles into a getter
  (skipping storage), and `validate --implementations` then checks the expression parses and
  its field refs resolve. Blocked on expanding the lexer beyond number/bare-ident directive
  args to a real expression grammar (string literals + operators). Until then `@computed` is a
  parsed-but-unenforced marker and `validate --implementations` is a no-op. Tracked as a
  backlog task; do **not** invent a stopgap impl-location convention (companion `.rs` stubs /
  `api://` refs) — it would be torn out when expressions land.
- **`init → build` publish gap — REOPENED by #89 (2026-07-10):** the durable write path made generated code
  require **`forgedb-wal 0.2.0`** (opaque `Raw` path) + **`forgedb-storage 0.1.5`** (`truncate_to_rows` +
  `DirLock`), neither published yet; the scaffold now pins them (`forgedb-wal = "0.2"`, `forgedb-storage =
  "0.1.5"`), so `forgedb init → generate → cargo build` against crates.io will not resolve until both are
  published. Proven in-workspace via path deps (`scratchpad/durable_compile`); publish is the gated follow-up.
  Prior history below. #56-B (single-writer/many-reader) added
  read-only column reader handles to `forgedb-storage` (`FixedColumnReader`/`VariableColumnReader`/
  `TombstonesReader` + `*::reader()`), bumping it **0.1.3 → 0.1.4**; generated `*StorageReader` /
  `DatabaseReader` call `col.reader()`. **`forgedb-storage 0.1.4` is now published**, and the reclose is
  PROVEN by an outside-repo `forgedb init --template blog → generate rust → cargo build` resolving
  `forgedb-storage 0.1.4` + `forgedb-changefeed 0.1.1` + `forgedb-types 0.2.0` from crates.io and compiling
  the generated reader code. (#62-B live queries needed **no** substrate change — the changefeed already
  carried the coarse signal — so `forgedb-changefeed` stayed 0.1.1.) `wal` 0.1.1 / `types` 0.2.0 unchanged.
  Scaffold pins `forgedb-storage = "0.1.5"`, `forgedb-changefeed = "0.1"`, **`forgedb-wal = "0.2"`** (#89),
  **`forgedb-auth = "0.1"`** (#59), axum `ws`. History: the gap reopened for #57, #62-A, #66, #56-B, #59, and
  now **#89** (open — pending `wal 0.2.0` + `storage 0.1.5` publish); the earlier reopenings all closed. #59 closed
  2026-07-09: `forgedb-auth 0.1.0` published + PROVEN by an outside-repo `forgedb init → generate rust+api
  → cargo build` resolving `forgedb-auth 0.1.0` + `forgedb-storage 0.1.4` + `forgedb-changefeed 0.1.1` +
  `forgedb-types 0.2.0` from crates.io and compiling the generated code **and** the env-driven scaffold
  `main.rs` (which links `forgedb-auth`). **Next thing that will reopen it:** any new substrate-crate dep or
  additive substrate API the generated code starts requiring — publish before the scaffold pins it.
- **Generated code now compiles for the whole `examples/` corpus.** The three codegen gaps
  that a full-corpus compile-test exposed are FIXED: nullable variable-length strings
  (`string?` → `Option<String>`, encoded with a 1-byte presence tag so `None` vs `Some("")`
  round-trip distinctly), inline `struct` types (now emitted as `#[repr(C)]` definitions),
  and integer (`+u64`/`+u32`) primary keys (the identity type now threads through the
  `id_to_row` key, `insert` return, `get` param, and the generated API path parse). Proven by
  a build-time compile harness + insert→get round-trip test (repro: `scratchpad/corpus_compile`,
  regen through the *current* codegen on every `cargo build`). **The discipline stands:
  codegen must be compile-tested, not just snapshot-tested** — snapshot pass ≠ output compiles,
  which is exactly how these three gaps hid.
- **Relation traversal is generated** (forward + reverse + M2M + eager-load). FK scalars
  (`RequiredReference`/`OptionalReference`) persist and round-trip (Task #25); on top of that
  the `RustGenerator` now emits, on `Database`: **forward FK getters** (`post_author(&post)
  -> Option<User>`, optional FKs thread through `and_then`), **reverse one-to-many** getters
  (`user_posts(id) -> Vec<Post>`, a linear scan via a generated `Storage::all()`; when a child
  has multiple FKs back to one parent collection the getter disambiguates by child field, e.g.
  `user_posts_by_author`), **many-to-many** persisted junction structs (`PostTagLink` with
  `left`/`right` UUID columns) plus `link_post_tag` / `post_tags` / `tag_posts`, and
  **eager-load** structs (`PostWithRelations { post, author: Option<User>, … }` +
  `post_with_relations(id)`). Honest limits: traversal is generated only between **UUID-keyed**
  models (FK scalars are always `Uuid`, so integer-PK targets are skipped with a comment);
  reverse/M2M lookups are **linear scans**, not indexed; there is **no M2M `unlink`** (storage
  is append-only — `Tombstones` has no in-place setter, the same reason generated models have
  no `delete`); and the `OneToMany`/`ManyToMany` fields *inside* the model struct remain virtual
  `()` (the collection lives in the traversal helpers / junction table, not the record).
  Proven compile-clean + insert→link→traverse across the whole `examples/` corpus by the
  `scratchpad/corpus_compile` harness, and snapshot+assertion-tested by
  `test_rust_generation_relation_traversal`.
- **Backup/restore (#57) — full-snapshot milestone LANDED; incrementals/PITR/cloud deferred.**
  `forgedb backup {create,restore,list}` over a data dir, backed by the class-1 `forgedb-backup`
  crate. Lock-free hot snapshot: each model/junction dir's `manifest.json` names a `row_anchor`
  (`tombstones.bin` @1 B/row for models, `fixed/right.bin` @16 B/row for junctions, both
  appended-last) → committed `N` = anchor length; every column's committed byte length is a pure
  function of `N` + layout, so concurrent appends past the watermark are excluded (no torn row) —
  proven by `crates/backup/tests/roundtrip.rs`. Restore is atomic (temp dir + rename), refuses a
  non-empty target without `--overwrite`. Codegen now emits the per-model layout manifest from
  `*Storage::new()` (`generate_write_manifest`, guarded by `test_rust_generation_layout_manifest`);
  the substrate `Manifest` gained `compaction_epoch`/`format_version`/`row_anchor` +
  `ColumnMetadata { value_size, kind, relative_path }` + `ColumnKind`/`RowAnchor` +
  `Manifest::save_to/load_from` (all additive `#[serde(default)]`). Manifest carries **physical
  layout only** — no relations/directives/routes (identity red line). **Deferred:** incremental
  backups (the `compaction_epoch` field is captured but compaction does not yet bump it, and no
  incremental chain logic exists), WAL-replay PITR, cloud `BackupTarget`, compression/encryption.
  E2E proof (generated write → `backup create` → `restore` → reopen restored dir → rows+M2M+int-PK
  survive) reproduced in `scratchpad/manifest_compile` (ephemeral).
- **Watermark snapshot reads (#56 Direction A) — LANDED.** Lock-free read snapshot isolation with
  **zero version machinery**, purely from the append-only row-count watermark. Substrate
  `forgedb_storage::Snapshot` is a bare `{ watermark: usize }` (`new`/`watermark`/`visible(index) ->
  index < watermark`) — schema-agnostic class-1, no relations, no per-row versions. Codegen emits, per
  schema: a shared `read_at(row_index)` that `get`/`get_at`/`all_at` all funnel through, plus
  `row_count`/`snapshot`/`get_at(&Snapshot,id)`/`all_at(&Snapshot)` on each `*Storage`, `pairs_at` on
  each junction, a `DatabaseSnapshot` bundle (one watermark per model + junction) + `Database::snapshot()`,
  and **one snapshot-scoped M2M traversal** `<left>_<right>s_at(&DatabaseSnapshot,id)` that clamps BOTH
  the junction (`pairs_at`) and the resolved target (`get_at`) — cross-table consistency. Honest limits:
  **single-process/single-thread** (captures are trivially atomic today); only **one** traversal is
  snapshot-scoped (forward M2M) — reverse/eager/FK-forward getters are not yet; and the milestone proof
  is a **deterministic capture-then-append** test, not a live concurrent-writer stress test (that lands
  with Direction B). PM identity gate PASS. Guards: substrate `test_snapshot_*`,
  `test_rust_generation_snapshot_reads`; compile + isolation E2E in `scratchpad/snapshot_compile`
  (ephemeral). Next per roadmap: **#62 Direction A** (change notifications), then the mutation surface.
- **Change notifications (#62 Direction A) — LANDED.** In-process, best-effort, **insert-only**
  real-time subscriptions. New **class-1 substrate crate `forgedb-changefeed`** (field-blind
  `tokio::sync::broadcast` of `ChangeEvent { model: &'static str, row_index, kind }`, `ChangeKind =
  Inserted | Linked`). Generated `insert()`/`link_*` emit the field-blind `(model, row_index)` signal
  (carrying the model *name*, never a field); `Database` owns one shared feed + hands each collection a
  clone; generated per-model typed event structs (`PostInserted { post }`); generated axum WS endpoint
  `GET /subscribe/<model-kebab>` that routes by model name, materializes via the now-public
  `read_at(row_index)`, applies a **generated per-model filter** (`<model>_event_matches` — each declared
  scalar field checked by name, relations excluded, closed compile-time set), and streams typed JSON;
  nginx `location /` forwards `Upgrade`. PM identity gate PASS (substrate schema-agnostic; all field-aware
  logic generated; no drift vectors). Honest limits: **single-process** only (no cross-process broker);
  Update/Delete events now exist via the mutation surface (#66, `ChangeKind::Updated`/`Deleted`); the
  per-model filter compares via `serde_json` stringify (exact-match, fine for common scalars; fragile for
  some float/bool encodings — typed per-field compare is a future refinement); Direction B (live queries)
  + C (durable broker) deferred. Guards: substrate `changefeed` unit tests, `test_rust_generation_changefeed_emits`,
  `test_api_generation_websocket_subscription`; **live WebSocket round-trip** E2E (client receives a
  filtered typed event) compile-tested through current codegen in `scratchpad/changefeed_compile`
  (ephemeral). Requires the `forgedb-changefeed 0.1.0` publish (see the publish gap).
- **Mutation surface (#66) — first milestone LANDED.** Generated **`update`/`delete`** per model via
  **superseding-version append** (the retraction primitive the fork resolved to): a mutation appends a new
  row version rather than mutating committed bytes, so append-only holds and backup (#57) / watermark
  snapshots (#56-A) / change feed (#62-A) stay unchanged. **No `forgedb-storage` change was needed** —
  the retraction is pure generated code over the existing append + `id_to_row` machinery; the only
  substrate change is `forgedb-changefeed` `ChangeKind` += `Updated`/`Deleted` (→ **0.1.1**, published
  2026-07-08). `update(id, record) -> bool` appends a live version and repoints the id
  (no-op false on absent id); `delete(id) -> bool` appends a **tombstoned** version so `get` reads absent
  (no-op false when already absent). **Snapshot isolation across mutation** falls out of the #56-A
  watermark for free: `get_at`/`all_at` resolve newest-version-*within-the-watermark* per id (read the id
  column across the committed prefix), so a snapshot captured *before* an update/delete still resolves the
  old value — no version chains / `xmin`/`xmax`. #62 gains `<Model>Updated`/`<Model>Deleted` typed events
  (the WS handler branches on kind; a `Deleted` emits the pre-delete row so the record is still
  materializable). PM identity gate PASS (retraction primitive stays schema-agnostic — row position +
  per-id pointer, never decodes a field; latest-version resolution is generated per model). **Honest
  limits / explicitly deferred:** storage **grows** with superseded versions until compaction (GC deferred
  to the `compaction` crate + reserved `compaction_epoch`; documented, not hidden); **no M2M `unlink`**;
  **single-process** (no concurrent-writer serialization — Direction B); no version chains / transaction
  manager (Direction C); no field-level partial update; no cascade delete. Guards:
  `test_rust_generation_mutation_surface`, extended snapshot-reads / changefeed-emits / websocket tests,
  changefeed `mutation_kinds_carry_through_the_feed`; **compile + insert→update/delete→snapshot-isolation
  + reopen + backup-roundtrip** E2E through current codegen in `scratchpad/mutation_compile` (ephemeral).
- **Single-writer + concurrent readers (#56 Direction B) — LANDED.** Lock-free concurrent reads under a
  live single writer, no version machinery. New class-1 substrate: read-only column reader handles
  (`FixedColumnReader`/`VariableColumnReader`/`TombstonesReader`, shared-fd positional `&self` reads,
  live length) + `FixedColumn/VariableColumn/Tombstones::reader()` → **`forgedb-storage` 0.1.4
  (published 2026-07-08)**. Codegen emits, per schema: `*Storage::reader() -> *StorageReader` (reusing the
  *exact* `read_at`/`get_at`/`all_at` token streams — one decode path), junction `*Reader` + `pairs_at`, a
  `DatabaseReader` bundle (one **typed named** reader field per model AND junction — never string-keyed
  dispatch) + `Database::reader()`, and the snapshot-scoped M2M `_at` traversal on `DatabaseReader`.
  **Cross-model atomicity:** `Database::snapshot()` is captured on the single writer (never mid-mutation),
  so the `DatabaseSnapshot` is a commit boundary; readers consume it immutably. PM identity gate PASS
  (reader handles know *less* than any other substrate; single-writer serialization is a runtime
  discipline, not a shipped engine). Honest limits: single-process/**single-writer** (concurrent writers →
  Direction C); atomicity holds *because* capture routes through the one writer; fd/page-cache coherence
  across `try_clone`d fds is a load-bearing substrate invariant (tested). Guards:
  `test_rust_generation_reader_handles`, substrate `test_reader_*` (incl. a lock-free read-under-live-append
  stress test); **live concurrent-writer stress test** (writer thread + N reader threads, cross-model
  atomic, torn-row-free) in `scratchpad/directionb_compile` (ephemeral).
- **Live queries (#62 Direction B) — LANDED.** Stateful, removal-aware result-set subscriptions.
  **No substrate change** (the changefeed already carried the coarse `event.model` signal → no
  `forgedb-changefeed` bump). Codegen emits, per model: a WS handler `GET /live-query/<model>?field=value`
  that binds params to the **same** generated closed-set filter (`<model>_event_matches`) as REST list /
  #62-A (**no second predicate parser** — the load-bearing red line), re-runs the generated `all()`+filter
  query on the **coarse** signal, diffs by id over opaque hashes, and pushes typed `<Model>LiveDelta`
  (`Init`/`Added`/`Updated`/`Removed`) deltas — `Removed` expressible thanks to #66's tombstone append.
  PM identity gate PASS (green-with-care): only generated code re-executes generated code on a coarse
  signal; never resolves `row_index→id` via the substrate. Honest limits: **O(rows) re-run per matched
  event per connection**, no coalescing/debounce (the scaling cliff); `Updated` uses full-record
  `serde_json` compare (#62-A fragility inherited); single-process. Guards:
  `test_api_generation_live_query`, `test_rust_generation_live_delta_enums`; **live WS round-trip**
  (Init→Added→silent→Updated→Removed) in `scratchpad/directionb_compile` (ephemeral).
- **Multi-tenancy Layer 1 (#59) — LANDED.** Physical, dir-per-tenant isolation + a verify-only JWT
  tenant guard, **process-per-tenant (model B)** — one `forgedb serve` process serves one tenant's data
  dir, N processes behind a dumb host/subdomain proxy. (This **supersedes** the note's earlier
  "one-process + registry" plan; the in-process registry = model C = a **deferred strict superset** on the
  same `Database::open_at` seam. PM re-gated the reversal + the auth layer: **PASS**, strictly stronger on
  identity — process-per-tenant deletes the multiplexer, generated code is tenant-oblivious.) Pieces:
  (1) new Class-1 substrate **`forgedb-auth`** (see crate list); (2) generated **`Database::open_at(root)`**
  + `*Storage::new_at(root)` + junction `new_at` + root-scoped `write_manifest(root)` — threads a data
  root through every column/tombstone/manifest path (also fixes the CWD-relative wart; `new()` == 
  `open_at(".")`, byte-identical); (3) generated **`create_router_with_auth(db, auth)`** layering
  `forgedb_auth::axum_mw::require_tenant` over every REST + WS route (401 no/bad token, 403 wrong tenant,
  principal injected); (4) config **`[tenant]`/`[auth]`** in `forgedb.toml` (never in `.forge`); (5) the
  scaffold `main.rs` is now a **real env-driven server** (`FORGEDB_TENANT`/`FORGEDB_DATA`/`FORGEDB_JWT_*`,
  resolved once → feeds both `open_at` and the auth cross-check); (6) **`forgedb tenant create|list|drop`**
  CLI. Identity red line held: `forgedb-auth` decodes no field, dispatches on no model, the cross-check is
  opaque string equality — verify-only (no issuance → #73; RLS-style per-principal authz → #72). Honest
  limits: **single tenant per process** (cross-tenant reads = fan-out; registry deferred); WS clients must
  send the token in the `Authorization` header; JWKS-over-HTTP fetch not yet wired in the scaffold (crate
  parses JWKS offline via `from_jwks_json`; static PEM is the wired path). Guards: 11 `forgedb-auth` verify
  tests, `test_rust_generation_root_threading`, `test_api_generation_tenant_auth_router`; **live e2e**
  (two isolated tenant roots + JWT tenant=A→200 / tenant=B→403 on the generated router) in
  `scratchpad/tenancy_compile` (ephemeral). Requires the `forgedb-auth 0.1.0` publish (see the publish gap).
- **`query-optimization` join predicate pushdown — LANDED (#48).** A predicate IR
  (`Predicate`/`Operand`/`PredicateOp` in `crates/query-optimization/src/planner.rs`) parses plan
  predicate strings into `table.column <op> operand` and attributes each to a join side by the
  tables its qualified columns reference. `partition_predicates_for_join` now splits predicates
  into `(left, right, remaining)`; `pushdown_predicates` pushes single-side predicates into the
  matching sub-plan (`collect_tables` walks each side for its table set) and keeps genuinely
  cross-side conditions (join conditions, unqualified columns, or unparseable strings) as the
  wrapping `Filter`. The `QueryPlanOp::Filter` node gained an `input: Box<QueryPlanOp>` so it
  actually **wraps** its sub-plan (previously childless → the optimized join was discarded).
  Honest limits: the crate still has **zero consumers** (speculative infra — not wired into
  codegen or generated code), predicates are attributed only by qualified `table.column` refs
  (bare columns stay at the join), and there is no selectivity re-estimation after pushdown.
  Guards: 9 `planner_tests` (IR parse + pushdown split/preserve/wrap).
- **OpenAPI generation — RESTORED (#49).** Standalone `OpenApiGenerator` in
  `crates/codegen/src/openapi.rs` emits an OpenAPI **3.1.0** document (`openapi.json`, pretty JSON
  via `serde_json`) at schema-compile time — no compiling/running the generated crate. **3.1.0 to
  match the runtime `utoipa` path** (utoipa 5.x serializes only 3.1.0), so both artifacts agree on
  version; nullability is JSON-Schema-2020-12 style (`type: ["string","null"]` / `anyOf` with
  `{type:null}`, **no** `nullable` keyword). Paths mirror the real generated routes exactly
  (`/api/<kebab>` list+create, `/api/<kebab>/{id}` get+replace+delete, `{id}` string param);
  component schemas cover each model + inline struct, map every `.forge` scalar/FK/nullable type,
  skip virtual collection + component fields, and mark non-nullable fields `required`. Both call
  sites re-enabled in `src/commands/generate/mod.rs` (the `openapi` single target + the `generate
  all` path). This is **distinct from** the runtime `utoipa` `ApiDoc`/`openapi_json()` in
  `crates/codegen/src/api.rs` (left untouched) — that path needs the app built and running; this is
  the offline artifact. Compile-test analogue for a non-Rust artifact:
  `test_openapi_generation_is_valid_document` parses the output back and asserts OpenAPI structure +
  `$ref` resolution. Guards: 2 snapshot + 2 structural codegen tests; e2e proven by `generate
  openapi`/`generate all` over `examples/ecommerce-store` (9 models → 18 paths, 36 `$ref`s all
  resolved, 0 stray `nullable` keywords).

## Conventions

- No time estimates (hours/days/weeks) anywhere — describe scope, not duration.
- Never `git commit`/`push` unless explicitly asked; then split into small focused,
  conventional commits and include related lockfiles.
- When closing a TODO item, delete it (git history is the audit trail).
- All workflows runnable from the repo root — no `cd` into subdirs.

## Subagents

- `forgedb-product-manager` — product/architecture decisions; guards the generator identity.
- `rust-core-library` — idiomatic Rust for core library/crate work.
