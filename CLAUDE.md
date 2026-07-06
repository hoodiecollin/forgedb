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
cargo clippy --workspace             # lints (1 dead-code warning remains — see Known issues)
```

CLI commands: `init`, `generate`, `validate`, `build`, `dev`, `migrate`, `compact`, `serve`.
Example: `cargo run -- generate all --output ./generated`.

### Test baseline

Plain `cargo test --workspace --no-fail-fast` is **green**:

```bash
cargo test --workspace --no-fail-fast   # 378 pass, 0 fail (incl. doctests)
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

**Baseline: 378 tests pass** (workspace, incl. doctests). Dropped from 531 when the orphaned
`fulltext` + `crud-api` crates were removed in Phase 3b. Ignore older claims of "531"/"521"/"466".

## Workspace layout

Root crate `forgedb` (`src/`) is the CLI: `src/main.rs` (clap), `src/commands/*`
(one module per subcommand), `src/{templates,ui,error}.rs`. It orchestrates the crates
in `crates/`:

**Published to crates.io (0.1.1 — independent version lines, do NOT normalize):**
- `types` — core type system (uuid, timestamp, primitives)
- `storage` — columnar storage engine (positional-I/O fixed columns + append-only variable)
- `wal` — write-ahead log

**Internal (0.1.0):**
- `parser` — lexer + parser → AST (`crates/parser/src/ast.rs`)
- `codegen` — code generators; exports `RustGenerator`, `TypeScriptGenerator`,
  `ApiGenerator`, `StubGenerator` (each `::generate(&schema) -> GeneratedCode`)
- `validation`, `migrations`, `compaction`, `query-optimization`, `query-params`,
  `http-server` (axum), `watcher`, `lsp-server`, `ffi`
  (`fulltext` + `crud-api` were removed in Phase 3b — orphaned runtime-library crates
  with zero consumers; the API existence/404 logic now lives in the generated handlers.)

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
(component fields only). Component refs `tsx:// jsx:// api://`. Only `//` comments. **NOT
supported despite older docs:** `~` auto-update, `text` type, `@on_delete`, block comments
`/* */`, quoted-string directive args (`@pattern("…")`, `@default("…")`). Full verified
reference: `docs/proposals/corpus/forge-grammar-reference.md`. **18 worked example schemas
across many domains live in `examples/` — see `examples/README.md`.**

## Known issues / backlog

- **1 dead-code warning** (down from 9 after the Phase 3b tooling sweep): only
  `validate --implementations` remains — deferred pending a decision on where `@computed`
  implementations live. The other 8 were resolved — WIRED (`build --no-api`, `validate
  --components`, the `--config`/`Config`/`CliError::exit_code` config feature, LSP
  struct-awareness) or REMOVED (`build --no-db`, `init --typescript`, `rust_main_template`,
  LSP `Document.uri/version` + `get_document`).
- **`init → build` needs an unpublished release.** Generated projects call storage/types
  methods (`append_uuid`/`read_uuid`/…) that exist only in the **local** crates; published
  `forgedb-storage`/`forgedb-types` `0.1.1` on crates.io lack them, so a freshly `init`ed
  project fails to compile against crates.io (`E0432`/`E0599`). Against local crates via
  `[patch.crates-io]` it builds cleanly (exit 0). Fix = publish **storage 0.1.2 + types
  0.2.0** and bump the init-scaffold version pins — deliberately DEFERRED (fix-in-tree,
  defer-publish). Until then generated projects need a local path/patch dependency.
- **Generated-code compilation gaps (codegen).** Surfaced by compile-testing the whole
  `examples/` corpus (18 schemas): the emitted `database.rs` does NOT compile for three
  common features — nullable variable-length strings (`string?` → `Option<String>`: insert
  passes `&Option<String>` to `append_string(&str)`, get assigns `String` to
  `Option<String>`), inline `struct` types (referenced in models but never emitted →
  `E0425`), and `u64` auto-generate PKs (`id: +u64` → `expected Uuid, found u64`).
  Schemas parse + `generate` fine; only compiling the output fails. Fix in a dedicated
  codegen pass, gated on the corpus compiling (repro: `scratchpad/corpus_compile`). This
  is why **codegen must be compile-tested, not just snapshot-tested.**
- **No string-literal constraint arguments (lexer).** `@`-directive args accept only
  numbers and bare identifiers — `@pattern("regex")` and `@default("text")` fail at lex
  time. Use `@default(identifier)`; model regex/enum intent via `@length` + a comment or a
  lookup model. (Lower priority; the grammar reference in `docs/proposals/corpus/` is
  corrected accordingly.)
- **Relation traversal is unbuilt.** FK scalar fields (`RequiredReference`/`OptionalReference`,
  e.g. `author_id: Uuid` / `editor_id: Option<Uuid>`) are now persisted and round-trip
  correctly (Task #25). What remains unbuilt: `OneToMany`/`ManyToMany` back-collections
  are virtual (stored as `()`, never persisted — intentional), and traversal helpers
  (join-by-FK at read time, eager-load, M2M junction tables) have not been generated.
  These are additive generation features, not correctness gaps.
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
