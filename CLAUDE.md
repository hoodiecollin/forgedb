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

Two modules answer "where does this build happen", and both are the ONE definition of what
they own (epic #332) — do not re-derive either inline:
- `src/project.rs` (#333) — **which project is this.** One upward walk from **the schema's**
  directory (never the CWD), yielding two different answers that are frequently different
  directories: knobs from `Chain::nearest()`, identity from `Chain::project_root()` (nearest
  `[project].isolated`, else outermost). Also the **single entry point for reading config** —
  `config::{parse_config, load_config_file}` are reached from here and nowhere else, which is
  the greppable form of #361's one-loader invariant. Id order: `[project].name` → exactly one
  ecosystem manifest → hash of the root's **absolute** path. The claim ledger under
  `~/.forgedb/ledger/` **detects** collisions; a resolution is recorded in the project's own
  `forgedb.toml`, never in the cache.
- `src/cache.rs` (#334) — **where generated code is built.** `~/.forgedb/projects/<id>/` as a
  ForgeDB-owned cargo workspace: virtual manifest pinning `resolver = "3"`, one `Cargo.lock`
  and one `target/` shared by every member, `apps/<member-hash>/` per app. The member hash is
  FNV-1a over the **project-relative** schema path (asymmetric with the project fallback id
  above, on purpose) and is pinned by golden vectors — `DefaultHasher` is not stable across
  Rust releases and `cargo install` uses the user's toolchain. The root manifest is
  **rewritten, never patched**, around a member set that **accretes from disk**: each member
  records the absolute schema it belongs to, so liveness is a `stat` per member rather than a
  scan of the user's repo, and a member that recorded nothing is KEPT.

**A relative output directory resolves against the SCHEMA's directory, not the CWD — the
built-in `generated` default included** (`Governing::output` owns all three cases; a
`--output` flag is the invocation's own word and stays verbatim). Under one root config,
`output` is a per-app pattern; the CWD-relative reading had every app in a project
overwriting its siblings.

**`src/main.rs` links the library** (`use forgedb::{…}`) rather than re-declaring its
modules. Re-declaring compiles each twice and makes every `pub` item only the tests use read
as dead code in the binary — do not reintroduce `mod` declarations there.

`forgedb.toml` **rejects unknown tables and keys** (#333) and there is no `[generate].schema`
key; both are breaking, both are in `docs/UPGRADING.md`.

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
  links against, like `changefeed`/`auth`. Generated code requires it, and it is published.
  `backup` (#57) is a **class-1 substrate** peer to `compaction`: lock-free full-snapshot
  create/restore over a data dir as opaque bytes (reads per-model `manifest.json` + column
  files, never the `.forge` schema).
  `changefeed` (#62 Direction A) is a **class-1 substrate** the *generated code links against*
  (like `storage`/`wal`, not like the internal-only crates above): a field-blind
  `tokio::sync::broadcast` of `ChangeEvent { model: &'static str, row_index, kind }`. Published; the
  scaffold pins it by major (derive both numbers — see the *Workspace layout* note). It never decodes
  a field; generated code routes by model name and materializes typed events.

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

Naming is **parser-enforced (fatal)**: models/structs PascalCase, fields snake_case. Every model
must also have an **identity field** — named `id`, or any `+` auto-generate field — which is
likewise fatal (#248); convention is `id: +uuid`. `id` wins by **name** over a `+` field declared
above it (#254 — the single-pass `find` this used to be silently mis-keyed a model whose stamp
came first). Both halves of "what is the identity" now have exactly ONE definition, on the AST
(#251): `Model::identity_field` picks the field (`id` by name, else the first `+`) — it was
open-coded 31× across 8 files — and `FieldType::is_identity_key` names the admitted **type**
allow-list: `uuid`, `u32`/`u64`/`i32`/`i64`, `timestamp(s|ms|us)` (incl. `+timestamp`, `id`-only),
`string(N)`/`string(N!)`, plus a required FK `*Model` (admitted by resolving through, #266).
Anything else is ONE positioned error naming the field + the allowed set: bare `string`, `bytes(N)`,
`f64`, `bool`, `decimal`, `json`, enum, struct, `[T; N]`, any nullable incl. `?Model`, `[Model]`.
The m2m endpoint rule is not a second check — `is_junction_key` **delegates** to `is_identity_key`,
and #252's two `string` rows were folded into the allow-list rather than placed beside it (one
mistake, one diagnostic). `tests/identity_predicate_test.rs` greps the tree and fails the build if
either predicate is open-coded again.
Modifiers (prefix, before the type): `+` auto-generate (u32/u64/uuid/timestamp only — all four
are synthesized on create; integer `+u32`/`+u64` allocate from a per-field counter seeded by an
ungated reopen scan and floored by `Manifest.auto_sequences`, #187. Every shape is valid — a
cross-process double-allocation is a detected conflict via one of three opaque write-set key
classes: `b"r"` (identity), `b"u"` (`&unique`), `b"s"` (a bare integer auto, #260). `0` is the
allocate sentinel, so it cannot be inserted explicitly), `&`
unique, `^` index; `?` nullable (postfix after type, or prefix on a model for an optional
FK). Types: `u32/u64/i32/i64/f64/bool/string/json/decimal/uuid`, `timestamp` / `timestamp(s|ms|us)`
(#254 — an instant; storage is ALWAYS `i64` **microseconds**, the declared key is the *quantum*
a written value is floored to and an allocated identity advances by; bare = `ms`; no `ns`. A
**probe** argument is floored the same way (#389 — `index_value_expr` / `ordered_key_expr` /
`generate_filter_check`), so two instants inside one quantum share an index bucket, match the
same REST filter, and conflict under `&unique`. Wire
form is the **RFC 3339 string** on every serde surface — JSON, TS SDK, OpenAPI `date-time`, the
three REST SDKs, REST filter params — but NOT the index key, which stays the stored number so the
order stays numeric. An instant outside RFC 3339's `0000`–`9999` is a 422. `id: +timestamp(us)` is
a legal identity: it must be named `id` (148/148 corpus `+timestamp` fields are stamps) and must be
`us` (uniqueness comes from the monotonic allocator `next = max(now, last+1)`, never from the
clock, and a coarser quantum runs the counter further ahead of the wall clock). The engine's own
byte-format generation is `Manifest.engine_version`, orthogonal to the app's `schema_version`
(on-disk key `format_version`); `forgedb migrate engine` carries a dir across it with a
**generated** hop crate — a schema-blind column pass cannot see the 81/247 corpus timestamp fields
that are nullable), `bytes(N)` (raw fixed-size bytes,
NOT text; `char(N)` is the deprecated spelling and warns — #233; `bytes` is a *contextual*
keyword, so it is still usable as a field name) — **there is no
`text`**. `string(N)` / `string(N!)` (#238) are the same `String` on every wire as bare `string`, but occupy a
fixed row slot instead of the variable column: N counts **characters**, `!` means *exactly* N (bare = at most),
1..=255, ASCII at one byte/char unless the field carries `@utf8` (four) — a non-ASCII value without it is a 422.
There is no overflow path (experiment #261 measured inline-or-overflow losing 198/200). Length directives are
refused on it (the width IS the bound); `@min`/`@length(min:)` survive on the non-exact form only; above 64 chars
it warns and still generates. Not embeddable in a `struct`/`[T; N]` (the Rust value is a heap `String`). **In a KEY
position it is a `forgedb_types::InlineStr<N>` instead — `Copy`, one byte/char, serde as a plain string (#252):** an
identity (`id: string(26!)`), an FK that resolves to one, and a junction endpoint. A key's value must be RFC 3986
`pchar` minus `%` (so the URL path segment is byte-identical to the key) and non-empty, both enforced at write (422);
`@utf8` on an identity is a schema error, and a bare `string` identity is refused (a key cannot be variable-width and
stay `Copy`). `json` (→ `serde_json::Value`, rides the variable-length string column; NOT indexable/filterable/
sortable — no total order); `decimal` (→ `rust_decimal::Decimal`, exact fixed-point on the 16-byte column, string
serde, IS indexable/sortable via a scale-invariant normalized key — `decimal(p,s)` precision/scale deferred). Enums:
top-level `enum Name { A, B, C }` (PascalCase name + variants), referenced by bare name — 1-byte discriminant column,
serialized as the variant-name string, filterable/sortable (declaration order)/indexable. Relations: `[Model]`
one-to-many, `*Model` required FK, `?Model` optional FK, bidirectional `[..]`/`[..]` = many-to-many; `[type; N]`
fixed array; inline `struct` (fixed-size fields only — no string/relations inside). Directives — **the validating
ones are `@min @max @length @email @url @pattern`/`@regex` `@utf8`, all ENFORCED** (violation → 422; `@length` counts
**chars**, not bytes, and takes named args — `@length(min: a, max: b)`, either alone, or positional `(a, b)`;
single-arg `@length(n)` means **exactly** n, NOT a maximum (#235); `@pattern` is a per-field `LazyLock<Regex>`,
#104). **Semantic-only markers** (parsed, carried,
never checked at write): `@default @index @computed @fulltext @materialized` — for a real index use the `^`
modifier. Per-directive truth table: `docs/SCHEMA.md`. **`@on_delete(restrict|cascade|set_null)`
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

- **Feature status & backlog:** `gh issue list` (types: `improvement`, `bugfix`, `experiment`;
  plus `epic`, `hotfix`, `release-gate`) and `pm-playbook ladder` for the derived rung, which no
  label carries and no filter can compute + git history. The narrative is in
  [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/V1_ROADMAP.md`](docs/V1_ROADMAP.md).
- **What a feature is guarded by:** the guard test itself — grep `crates/codegen/tests/` for
  `test_rust_generation_*` / `test_api_generation_*`. The test is the record; a prose list drifts.
- **Substrate versions / publish status:** derive per the *Workspace layout* note above (grep
  `crates/*/Cargo.toml`, `cargo search forgedb-<crate>`, and the scaffold pins). Never trust a
  version written in prose.
- **What we're working on right now:** open milestones + `gh issue list`. There is deliberately
  **no "current focus" section in this file**, and one must not be re-added. A focus statement is
  temporal state: it is stale the moment priorities move, and because it sits in the agent's
  always-loaded context it gets *acted on* — sending a session off to pick up work that was
  finished or abandoned. This file is for the durable shape; what is live is a query, not prose.

### Project management (the ONE tracking model — no parallel systems)

ForgeDB follows the **ai-pm-playbook** (the portable form of the model worked out here; adopted
2026-07-31, migrated to **v2.0** on 2026-08-12). **The doctrine is vendored at `.pm-playbook/` and
is authoritative** — read `.pm-playbook/AGENT.md` before you create, label, milestone or close an
issue, and do not re-transcribe the model here. Section refs below are to that playbook. All work
is tracked in **GitHub Issues** on `hoodiecollin/forgedb`.

The two axes, in one line each: a **milestone** says *when* and means **committed** (being the
cycle in flight is what means *scheduled*); **labels** say *what kind*. Every work item carries
exactly one of `improvement` / `bugfix` / `experiment`, and that type decides its gate sub-issues.
The commitment ladder is **derived** from gate state — ask `pm-playbook ladder`, never a filter.

Verify with `npx @hoodiecollin/pm-playbook check` — exit 0 means compliant. What follows is only
what is **specific to this repo** and is not in the playbook.

**`release-gate` = blocks the tag (§5.2).** The rung between *closed-into-milestone* and
*released*. An open `release-gate` issue on a milestone means **that milestone cannot be tagged**,
even if every feature on it is closed — it is a release obligation (publish the substrate,
reconcile a version line, rotate a credential), not deferrable work. File one the moment you
*knowingly* defer such an obligation; the deferral is exactly when it gets forgotten, because
everything still builds locally. The complete "can we tag?" query:

```bash
gh issue list --label release-gate --state open      # any row ⇒ blocked
```

**The release-gate issue MUST carry a versioned-asset ledger (§5.2).** Its body holds a table of
**every** independently versioned asset in this repo — all 18 `crates/*` plus the root `forgedb`
crate plus `apps/vscode-forgedb` — each row defaulting to **"no change"**, created when the
milestone opens. **When a change touches one of those, set its row in the same pass that lands the
change.** Deciding "does this need a bump?" with the change in front of you is reliable;
reconstructing it at tag time from a diff is not.

An absent row and a "no change" row look identical at tag time and mean opposite things
(*verified untouched* vs *never considered*), which is why every asset gets a row rather than only
the touched ones. This is not the same check as the publish gap: the gap is about a *missing*
version, the ledger is about a *stale* one. The stale case is the quiet one — the version exists,
so `cargo publish -p forgedb --dry-run` passes, the workspace builds (path deps shadow the
registry), and the release ships old source behind a correct-looking number. Use
`git log --oneline main..develop -- crates/<c>` against the version line; never the version alone.
Template: `.github/ISSUE_TEMPLATE/release-gate.md`.

**Surfaces (§6.1).** A `surface:*` label marks a separately shippable product face. This repo uses
`surface:website` and `surface:ide-extension`; **core is the implicit default and carries no label.**
**Never put a non-core `surface:*` issue on a core `v*` milestone** — it would read as "done,
awaiting vX" though it already shipped on its own line, and would never reach the core changelog.
Non-core surfaces get their own namespace milestones (`vscode-v*`) or deploy continuously. The
core roadmap excludes them by label prefix — `apps/website/lib/roadmap-transform.ts`
(`isCoreScoped`) drops any `surface:*` that isn't `surface:core`, so a newly-added surface is
filtered the moment it exists.

**`experiment` never rides the release spine (§4).** The playbook covers the rule; the ForgeDB
corollary worth stating is this: **never anchor a milestone's theme on an experiment's hoped-for
outcome.** You cannot schedule a feature whose existence the experiment has not yet decided. The
discipline test when typing an issue — if its primary output is a measurement, evaluation or
feasibility verdict, it is an `experiment` and stays off the spine; if it is shippable code that
ships regardless of any measurement, it is an `improvement`.

**Epics decompose via GitHub *native sub-issues*** (the `Parent issue` / `Sub-issues progress`
link — `gh api repos/OWNER/REPO/issues/N/sub_issues`), *not* task-list checkboxes (secondary,
drift-prone) and *not* a labels/fields convention. An epic is a top-level container that MAY span
releases; each child carries its own milestone. The website `/roadmap` reads exactly this shape
(epics collapsible, standalone issues alongside) — see [`apps/website/lib/roadmap-transform.ts`].

**Never parent an unmilestoned work item or an `experiment` to an epic.** Neither can close into a
release, and making one a sub-issue does two bad things: it pins the epic's `done/total` below 100%
permanently (the transform counts every child indiscriminately), and it removes the child from the
standalone list — the *only* path by which an unmilestoned item reaches the roadmap's **Ideas**
section and an `experiment` reaches **Labs** (`claimedChildren`). Parenting an idea to an epic
therefore **hides** it. Link them instead with a plain `#number` reference under a *Related,
deliberately unparented* heading in the epic body: GitHub records a cross-reference event on the
child, so the link is bidirectional and machine-visible with nothing to hand-maintain, and unlike a
checkbox it carries no status to drift. When an experiment's measured conclusion commits real work,
*that* work is a new issue — and it parents normally. (The one coherent exception is an epic that
is *itself* labeled `experiment`, e.g. #167: it rides no release spine, so its children completing
is a meaningful signal.)

**Gate sub-issues are not roadmap entries.** `materialize` creates them under a work item, so they
are children of an issue rather than of an epic and `claimedChildren` does not filter them. The
transform excludes them by label pattern (`isGate`); leave that filter in place or a single
milestone's gate set floods the roadmap.

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

   **Where the gap is allowed to live — the branch model (playbook §5.2).** ForgeDB batches the
   substrate publish at the release rather than publishing per-issue, so a publish gap *does* open
   mid-cycle. It is held off the default branch:

   | Branch | Holds | Invariant |
   |---|---|---|
   | `main` | released state | **Always releasable.** An outside-repo `init → generate → cargo build` resolves entirely from crates.io. |
   | `develop` | the current core release cycle | May knowingly carry a publish gap. That is its job. |

   - **Core work branches off `develop` and merges back to `develop`** (keep branch-per-scope +
     auto-merge; only the base changes). Nothing core lands on `main` except a release merge.
   - **Branches land as merge commits — `git merge --no-ff`, never squash, never rebase.**
     Squash and rebase are disabled in the repo settings, so the GitHub button refuses anything
     else; most work merges locally anyway. Subject form:
     `Merge <branch>: <what it did> (#<issue>)`. `--no-ff` is load-bearing — a branch that is
     merely ahead fast-forwards otherwise, erasing the boundary exactly as a rebase would.
     **Close the issue by hand**: GitHub honours `Closes #N` only for PRs into the *default*
     branch, so a PR merged into `develop` leaves its issue open however the body is written.
   - **The release sequence is ordered, and the order is the whole point:** publish the substrate
     → **then** merge `develop` → `main` → **then** tag. Publishing after the merge reopens the
     window the branch exists to close.
   - **The outside-repo reclose is a check on `main`, not on `develop`** — required on the
     integration branch it would sit red for an entire cycle and stop being read.
     `.github/workflows/substrate-reclose.yml`.

   **Which branch a docs / website / extension change targets is a *coupling* question, not a
   surface one.** The surface-exclusion rule above governs *milestones and changelogs*; this
   governs *when the change becomes visible*:

   - **Independent of unreleased core → straight to `main`.** Typo and link fixes, styling, SEO,
     analytics, dep bumps, corrections to already-shipped docs. These deploy continuously and must
     not wait on a release they have nothing to do with.
   - **Documents, depends on, or demonstrates unreleased core → `develop`, in the same change as
     the feature.** Docs for an unshipped feature, examples using an unreleased API, a schema
     reference for syntax that does not parse yet.

   Getting this backwards publishes documentation for an API nobody can call — worse than no page,
   because it makes the docs a liar in the exact moment someone is trusting them. Pair feature docs
   with the feature.

   **There is exactly ONE `develop`, and its name never contains a version.** No `v0.5-develop`
   beside a `v0.4-develop`. crates.io has one version line per crate and the gap is defined against
   what is *currently published*, so two cycle branches carrying unpublished substrate changes
   cannot both be measured — whichever publishes first silently redefines the other's gap. And the
   milestone already encodes *when*; a version in the branch name is a second scheduling axis,
   which is the parallel-decomposition anti-pattern. Version-agnostic also means self-advancing:
   tagging a release turns `develop` into the next cycle with no rename and no workflow edit.

   So what keeps next-cycle work off `develop` is **the milestone, not the branch**:

   > A PR targeting `develop` may not close an issue milestoned later than the cycle in flight
   > (**derived**: the lowest open `v*` milestone — never configured, never written down).

   A **deny-list on future milestones**, not an allow-list on the current one — so chores, CI fixes
   and typo PRs close no issue and pass silently, correctly: work with no issue cannot be
   next-cycle work. `.github/workflows/cycle-scope.yml` gates PRs into `develop`; since most work
   here merges locally, run it yourself before merging a branch back:

   ```bash
   make cycle-scope ISSUE=245        # what the branch closes
   make cycle-scope PR=250           # or a PR, as CI does
   ```

   Blocked means *early*, not wrong: keep the branch, let the cycle ship, rebase, land it. The only
   other correct response is that the issue was mis-scheduled — move the milestone rather than
   merging past it. **This makes closing the milestone part of the release ritual**: a milestone
   left open after its tag freezes the derived cycle and blocks legitimate next-cycle work.
   (Portable form: `ai-pm-playbook` PLAYBOOK §5.3, rules PM008/PM009.)

3. **Codegen is compile-tested, not just snapshot-tested.** The `insta` snapshots compare
   generated code as *strings* — a snapshot pass does **not** mean the output compiles. When you
   change a generator, generate for a real multi-model schema and `cargo check` the emitted crate
   (`database.rs` + `api.rs`). Also `cargo build --workspace --examples` — the default test flags
   exclude examples. (Both disciplines have caught real bugs; see *Build, test, run*.)

4. **Ground truth over sources of truth.** Code + git history + runtime/DB state are ground truth;
   this file, docs, memory, and issues are *claims* that drift. Run the **`sync-sources`** skill at
   task boundaries; when a claim disagrees with code, fix the claim. Do not pin an exact test count
   in prose (chronic drift source — run the runner).

5. **Design docs are not committed to the repo.** Proposals / design notes live as **gate
   sub-issues**, not files — run the **`design-gates`** skill to file one (it has the dedup check,
   the body template, and the epic cross-link). (The historical `docs/proposals/` set was removed;
   git history holds it. Shipped-feature *architecture* reference belongs in `docs/ARCHITECTURE.md`.)

6. **Gates in series, before any code (§9).** A work item's gates are **native sub-issues** created
   by `pm-playbook materialize` — never by hand, and always as a complete set. For an `improvement`:
   **gate 1 design** (problem, desired behavior, solution *shape*, alternatives, explicit non-goals
   — solution-shaped, not code-shaped; catches *conceptual* gotchas), **gate 2 plan** (files to
   touch, build order, interfaces, blockers, and the BDD scenarios to write; catches *execution*
   gotchas), **gate 3 impl** (scenarios RED → implement to GREEN → refactor under green). A
   `bugfix` takes two: diagnose → fix. **Closing a gate means accepted**, and the rung is derived
   from which gates are closed — never from a status label, and **effort labels are banned**. When
   a feature ships, fold its durable design into `docs/ARCHITECTURE.md`.

7. **Reopening an accepted gate? Purge the issue body FIRST (§9.1).** Gates get redone — new
   information lands, a constraint turns out to be an artifact of an assumption. The moment you
   decide to redo one, the body is purged *before any new thinking*, down to a placeholder saying
   the gate is being redone and that the body deliberately holds no design content. Stashing the
   old body to a scratch file while you work is fine; **delete the stash** once the new gate is
   accepted and the new body is written.

   A withdrawn design left in the body does not read as withdrawn — it reads as **the** accepted
   design, because that is what a body *is*. The correction invariably lands in a comment, and
   top-down readers never reach it, so the next planning pass builds on it silently and the plan
   looks correct (it is internally consistent with the wrong premise). #187 hit exactly this: a
   Gate 2 was written against a body describing a design that acceptance had rejected. Repopulate
   the body **only at acceptance**, from the accepted outcome — never patch it incrementally as
   thinking evolves, which recreates the half-superseded state the purge exists to prevent.

8. **Run `sync-sources` on BOTH sides of every gate (§9.2) — no triviality exemption.** The global
   rule scopes it to non-trivial tasks; gates are exempt from that exemption. `verify` before
   (check every claim source against code, fix what drifted), `propagate` after (push the accepted
   outcome into the issue body, docs, memory, cross-linked issues). A gate's input is the previous
   gate's output, so a stale claim there is not caught downstream — it is *built on*. This burns
   tokens; do it anyway, because the cost is bounded and paid once while planning against a stale
   claim is unbounded and discovered late.

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

<!-- pm-playbook:begin -->
## Project management — pm-playbook v2.2.1

Issue tracking in this repo follows the **pm-playbook** two-axis model. The full doctrine is
vendored at `.pm-playbook/` and is authoritative; this block is only a summary.

**Before you create, label, milestone, or close an issue — read `.pm-playbook/AGENT.md`.**
It is a short router: load only the reference section relevant to what you are doing.

**The two axes, and nothing else, organize work:**
- **Milestone** = *when*. Assigning one means **committed**. *Focus* — the milestone being the
  cycle in flight — is what means scheduled. There is no label for "committed but unscheduled."
- **Labels** = *what kind*. Epics decompose via **native sub-issues**, never checkboxes and never
  a Project field.
- There are **no Priority / Size / Workstream fields**. Do not propose adding any.

**Every work item carries exactly one type, and the type decides its gates:**

| Type | Gates |
|---|---|
| `improvement` | design → plan → impl |
| `bugfix` | diagnose → fix (`hotfix` is a bounded form of this) |
| `experiment` | research → evaluate (never milestoned) |

Each gate is a sub-issue labelled `{type}:gate-{n}`. A closed gate means approved. The tree is
exactly three levels: epic → work item → gate.

**The commitment ladder is DERIVED from gate state — there are no maturity labels.** Walk the
gates in order; the first not closed decides the rung. Ask for it with `pm-playbook ladder`; no
GitHub filter can compute it.

**Invariants — violating one is a bug, not a style preference:**
- Exactly **one** type label per work item — never zero, never two (PM010). An `epic`, a gate and
  a `release-gate` are not work items for this purpose and need no type.
- `experiment` never carries a milestone. A spike's deliverable is a finding; it feeds the
  release spine, it never rides it (PM003).
- **Never create a gate by hand** — `pm-playbook materialize` owns them and creates a complete
  set at once. A hand-made gate destroys the meaning of an absent one.
- A gate's milestone equals its parent's (PM011); an `epic` never carries gates (PM012).
- `release-gate` always has a milestone and never carries `experiment`. An open `release-gate`
  means its milestone **cannot be tagged** (PM004/PM005).
- A non-core `surface:*` issue never rides a core `v*` milestone (PM006).

**Read the backlog from the local mirror when it exists.** `.pm-playbook/backlog/` holds every
issue body and comment as files — grep it instead of spending an API round trip per question. It is
gitignored and machine-local, so its absence means "not pulled here yet", never "no issues", and it
goes stale as soon as anyone else moves an issue. Reading is local; **writing is not** — edit and
`push` (it refuses when both sides moved), or use `gh` directly.

```bash
npx @hoodiecollin/pm-playbook pull     # refresh the mirror (idempotent)
npx @hoodiecollin/pm-playbook check    # verify before opening a PR — exit 0 means compliant
```
<!-- pm-playbook:end -->
