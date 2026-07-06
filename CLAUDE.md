# CLAUDE.md

Guidance for Claude Code when working in this repository. Keep it accurate — if you
change the build, layout, or commands, update this file in the same change.

## What ForgeDB is

ForgeDB is an **application database generator** — a compile-time code generation tool,
**not** a runtime library or ORM. A declarative `.forge` schema is transpiled into
tailored Rust database code plus a TypeScript SDK, a REST API, and React component
stubs. End users need only: their schema, the `forgedb` CLI, and config. Generated code
carries **zero ForgeDB runtime dependency**.

Guard this identity when evaluating features (see the `forgedb-product-manager` subagent):
prefer *better generated code* over *runtime functionality*. Reject anything that turns
ForgeDB into a library users import at runtime.

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
cargo clippy --workspace             # lints (9 dead-code warnings remain — see Known issues)
```

CLI commands: `init`, `generate`, `validate`, `build`, `dev`, `migrate`, `compact`, `serve`.
Example: `cargo run -- generate all --output ./generated`.

### Test baseline

Plain `cargo test --workspace` is **green** — doctests included:

```bash
cargo test --workspace --lib --bins --tests --no-fail-fast   # 481 pass (non-doctest)
cargo test --workspace                                        # + 40 doctests, all pass
```

- **`--no-fail-fast`** surfaces all results — cargo halts at the first failing binary
  otherwise.
- The integration tests (`tests/integration_test.rs`) are **hermetic** — the
  CWD-dependent cases invoke the `forgedb` binary as a subprocess with an explicit
  `current_dir`, so they pass in parallel. No `--test-threads=1` workaround needed.
- The ~21 stale doctests (Phase 3c) are **fixed** — all doc examples across the workspace
  now compile and pass against current APIs.
- **Codegen caveat:** the `crates/codegen` insta snapshot tests only compare generated
  code as *strings* — they do not compile it. When changing generators, compile the
  emitted Rust in a throwaway crate (see the memory note); snapshot pass ≠ output compiles.

**Baseline: 521 tests pass** (481 non-doctest + 40 doctest). Ignore older doc claims of
"466" or "241/241".

## Workspace layout

Root crate `forgedb` (`src/`) is the CLI: `src/main.rs` (clap), `src/commands/*`
(one module per subcommand), `src/{templates,ui,error}.rs`. It orchestrates the crates
in `crates/`:

**Published to crates.io (0.1.1 — independent version lines, do NOT normalize):**
- `types` — core type system (uuid, timestamp, primitives)
- `storage` — columnar storage engine (memory-mapped fixed columns + append-only variable)
- `wal` — write-ahead log

**Internal (0.1.0):**
- `parser` — lexer + parser → AST (`crates/parser/src/ast.rs`)
- `codegen` — code generators; exports `RustGenerator`, `TypeScriptGenerator`,
  `ApiGenerator`, `StubGenerator` (each `::generate(&schema) -> GeneratedCode`)
- `validation`, `migrations`, `compaction`, `fulltext`, `query-optimization`,
  `query-params`, `crud-api`, `http-server` (axum), `watcher`, `lsp-server`, `ffi`

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

Symbols: `+` auto-generate on create, `~` auto-update on modify, `^` index, `&` unique,
`?` nullable, `*` required foreign key, `@` directive (`@min`, `@max`, `@email`,
`@pattern`, `@index`, `@relations`). Types: `u32/u64/i32/i64/f64/bool/string`, plus
`uuid/timestamp/char(n)/text`; `[Model]` one-to-many, `Model` FK, `[type; N]` fixed array,
inline structs. Component refs: `tsx://`, `jsx://`, `api://`.

## Known issues / backlog

- **9 dead-code warnings** — not cruft: unwired-but-live CLI flags (`build --no-api/--no-db`,
  `init --typescript`, `validate --implementations/--components`), an unused error
  exit-code scheme (`CliError::exit_code`/`Config` + the ignored `--config` flag), an
  unwired `rust_main_template` init scaffold, and populated-but-unread LSP fields
  (`Document.uri/version`, `get_document`, `Struct.name/fields/position`). Each needs a
  **wire-vs-remove product decision** — deliberately deferred out of the 3c bug-fix sweep
  (don't blindly delete). Tracked separately.
- **Relation storage is unbuilt.** Generated model structs include FK scalar fields (e.g.
  `author_id`), but they are not persisted — `get()` returns `Default::default()`/`None`
  for them. The generated code compiles and round-trips non-relation fields correctly;
  actually storing/loading FK values is an unimplemented feature (a product-direction item).
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
