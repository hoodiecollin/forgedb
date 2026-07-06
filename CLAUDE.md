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
cargo clippy --workspace             # no dead-code warnings (style lints remain, pre-existing)
```

CLI commands: `init`, `generate`, `validate`, `build`, `dev`, `migrate`, `compact`, `serve`.
Example: `cargo run -- generate all --output ./generated`.

### Test baseline

Plain `cargo test --workspace --no-fail-fast` is **green**:

```bash
cargo test --workspace --no-fail-fast   # 379 pass, 0 fail (incl. doctests)
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

**Baseline: 379 tests pass** (workspace, incl. doctests). Dropped from 531 when the orphaned
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
- **`init → build` — in-tree bumps done; awaiting the actual crates.io publish.** Generated
  projects call storage/types methods (`append_uuid`/`read_uuid`/…) and use `Value::U32/U64`
  that exist only in the **local** crates; published `forgedb-storage`/`forgedb-types` `0.1.1`
  on crates.io lack them, so a freshly `init`ed project fails to compile against crates.io
  (`E0432`/`E0599`). The in-tree half is now COMPLETE: `crates/storage` is bumped to **0.1.2**
  (additive methods) and `crates/types` to **0.2.0** (breaking — new `Value::U32/U64`
  variants), the init-scaffold pins are `forgedb-storage = "0.1.2"` / `forgedb-types = "0.2"`,
  and both crates pass `cargo publish --dry-run` (storage 0.1.2 verifies against the
  already-published `wal 0.1.1`). What remains is the **outward-facing, irreversible** publish
  itself (crates.io versions can't be deleted, only yanked), which must be run with crates.io
  credentials: `cargo publish -p forgedb-types` then `cargo publish -p forgedb-storage` (order
  is free — storage does not depend on types; `wal` is unchanged at 0.1.1 and is NOT
  republished). Until published, generated projects still need a local path/patch dependency.
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
- **No string-literal constraint arguments (lexer).** `@`-directive args accept only
  numbers and bare identifiers — `@pattern("regex")` and `@default("text")` fail at lex
  time. Use `@default(identifier)`; model regex/enum intent via `@length` + a comment or a
  lookup model. (Lower priority; the grammar reference in `docs/proposals/corpus/` is
  corrected accordingly.)
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
