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

CLI commands: `init`, `generate`, `validate`, `build`, `dev`, `migrate`, `compact`, `backup`, `serve`.
Example: `cargo run -- generate all --output ./generated`.

### Test baseline

Plain `cargo test --workspace --no-fail-fast` is **green**:

```bash
cargo test --workspace --no-fail-fast   # 399 pass, 0 fail (incl. doctests)
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

**Baseline: 399 tests pass** (workspace, incl. doctests). Dropped from 531 when the orphaned
`fulltext` + `crud-api` crates were removed in Phase 3b. Ignore older claims of "531"/"521"/"466"/"398"/"394"/"380".

## Workspace layout

Root crate `forgedb` (`src/`) is the CLI: `src/main.rs` (clap), `src/commands/*`
(one module per subcommand), `src/{templates,ui,error}.rs`. It orchestrates the crates
in `crates/`:

**Published to crates.io (independent version lines, do NOT normalize):**
- `types` — core type system (uuid, timestamp, primitives) — **0.2.0**
- `storage` — columnar storage engine (positional-I/O fixed columns + append-only variable) — **0.1.3
  in-workspace, 0.1.2 on crates.io** (0.1.3 adds `Manifest` layout fields + `Manifest::save_to/load_from`
  for #57 backup; NOT YET PUBLISHED — see the reopened publish gap in Known issues)
- `wal` — write-ahead log — **0.1.1**

**Internal (0.1.0):**
- `parser` — lexer + parser → AST (`crates/parser/src/ast.rs`)
- `codegen` — code generators; exports `RustGenerator`, `TypeScriptGenerator`,
  `ApiGenerator`, `StubGenerator` (each `::generate(&schema) -> GeneratedCode`)
- `validation`, `migrations`, `compaction`, `backup`, `query-optimization`, `query-params`,
  `http-server` (axum), `watcher`, `lsp-server`, `ffi`
  (`fulltext` + `crud-api` were removed in Phase 3b — orphaned runtime-library crates
  with zero consumers; the API existence/404 logic now lives in the generated handlers.)
  `backup` (#57) is a **class-1 substrate** peer to `compaction`: lock-free full-snapshot
  create/restore over a data dir as opaque bytes (reads per-model `manifest.json` + column
  files, never the `.forge` schema).

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
- **`init → build` publish gap REOPENED by #57 (needs `forgedb-storage 0.1.3` publish).** The
  #57 layout manifest made generated `*Storage::new()` emit a `<model>/manifest.json` built from
  `forgedb_storage::{Manifest, ColumnMetadata, ColumnKind, RowAnchor}` — fields/types that only
  exist on the **in-workspace 0.1.3**, not the published **0.1.2**. So a freshly `init`ed project
  (now pinned `forgedb-storage = "0.1.3"`) can't resolve/compile against crates.io until 0.1.3 is
  published. **To reclose:** `cargo publish -p forgedb-storage` (additive minor, 0.1.2→0.1.3;
  `wal` 0.1.1 and `types` 0.2.0 unchanged), then re-run the outside-repo `init → generate rust →
  cargo build` proof. In-workspace everything builds (path deps); the gap is only against
  crates.io. Prior close (2026-07-06): published **`forgedb-types 0.2.0`** (breaking `Value::U32/U64`)
  + **`forgedb-storage 0.1.2`** (additive `append_uuid`/`read_uuid`/… methods) and pinned the
  scaffold to those.
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
- **`query-optimization` join pushdown is a stub.** `partition_predicates_for_join` returns
  no partition (predicates are unstructured strings), so join predicates are preserved
  correctly as a `Filter` wrapping the join output but are **not** pushed into either side.
  Correct results, no pushdown optimization yet.
- **OpenAPI generation is disabled.** The generator was lost during the crate-extraction
  refactor; `src/commands/generate/mod.rs` skips it with a warning and the `openapi`
  target errors clearly. Restore = re-implement in `crates/codegen`. Deferred (the live
  `utoipa` derives in `crates/codegen/src/api.rs` are unrelated — leave them).

## Conventions

- No time estimates (hours/days/weeks) anywhere — describe scope, not duration.
- Never `git commit`/`push` unless explicitly asked; then split into small focused,
  conventional commits and include related lockfiles.
- When closing a TODO item, delete it (git history is the audit trail).
- All workflows runnable from the repo root — no `cd` into subdirs.

## Subagents

- `forgedb-product-manager` — product/architecture decisions; guards the generator identity.
- `rust-core-library` — idiomatic Rust for core library/crate work.
