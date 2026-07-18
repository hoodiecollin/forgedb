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
landed feature is guarded by is recorded in that feature's bullet under **Known issues** below
(guard test names like `test_rust_generation_*` / `test_api_generation_*`) — that named list is
the durable record of *what* is tested; a running total is not. If a number ever looks off,
trust the runner output, never a count written in prose.

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
> The invariant is the **publish-gap rule** (see the publish-gap bullet under "Known issues"): when
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
  coordinator surface out.

**Internal (0.1.0):** (compiler internals — `parser`, `codegen`, `validation`, `migrations`, `backup`, `watcher`
are now **published to crates.io** 0.1.0 as of Phase 5 WS4, but **only** so `cargo install forgedb` can build the CLI
from the registry; per `docs/SEMVER.md` they are explicitly NOT a stable public API, unlike the substrate crates.
`lsp-server` remains unpublished.)
- `parser` — lexer + parser → AST (`crates/parser/src/ast.rs`)
- `codegen` — code generators; exports `RustGenerator`, `TypeScriptGenerator`,
  `ApiGenerator`, `StubGenerator` (each `::generate(&schema) -> GeneratedCode`)
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

## Known issues / backlog

- **Core is not yet production-complete — ForgeDB is built perimeter-first (v1 roadmap: `docs/V1_ROADMAP.md`).**
  The advanced, well-documented features below (live queries, multi-tenancy, snapshots, backup, single-writer/
  many-reader) are real but sit on a **generated core with critical gaps** that the "LANDED" bullets can obscure.
  Verify against code before trusting a maturity claim. As of 2026-07-10, four probes established:
  - **Generated writes are now crash-safe (v1 Phase 1 step 1 — #89 LANDED + published).** Generated
    `insert/update/delete` record an **opaque** row blob to a per-model WAL (`forgedb-wal` `Raw` op) + fsync
    (`FsyncPolicy::Always`) BEFORE touching columns; `new_at` runs generated per-model `recover_from_wal`
    (truncates a torn column tail back to the min-consistent prefix, then idempotently replays the WAL tail by
    absolute row index); `open_at` holds a `forgedb_storage::DirLock` so a second writer refuses (never
    corrupts). Substrate: `forgedb-wal` **0.2.0** (opaque `Raw` path; structured/txn API pruned — #95),
    `forgedb-storage` **0.1.5** (`truncate_to_rows` + `DirLock`). Proven E2E (torn-tail repair, lost-committed-row
    recovery, **`kill -9` mid-write → 0 acked rows lost**, second-writer refused) in `scratchpad/durable_compile`
    through current codegen; guard `test_rust_generation_durable_write_path`. Substrate published 2026-07-10
    (`forgedb-wal 0.2.0` + `forgedb-storage 0.1.5`); reclose proven outside-repo. **Honest limits / deferred:**
    `fsync` policy is fixed (not yet config). Single-writer-per-process is the v1 contract; multi-writer
    (Direction C) stays out.
  - **The WAL is now bounded (v1 Phase 1 step 2 — #96 LANDED + published).** A generated `checkpoint()` fsyncs
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
  - **The generated database is now readable (v1 Phase 2 — #90 LANDED).** Two workstreams, proven E2E:
    - **Real list endpoint + filter/sort/paginate.** The list handler no longer returns `{"data":[]}` — it
      fetches live rows via `all()`, filters through the SAME generated closed-set matcher `<model>_event_matches`
      the change-feed / live-query paths use (no second predicate parser), sorts via a generated per-model
      comparator `<model>_apply_sort` (`Ord::cmp`, or `f64::partial_cmp` for float fields), and paginates via the
      **schema-agnostic `forgedb-query-params` substrate** (`QueryParams::from_map` + `Pagination::apply`, clamped
      to `MAX_LIMIT`). Response is `{data,total,limit,offset}`. query-params parses only the query string into
      generic `Sort`/`Pagination`; every field-aware step is generated — identity-clean (PM audit kept the crate
      precisely to wire here). Guard `test_api_generation_list_endpoint`.
    - **Secondary indexes + `find_by_*` / `get_by_*` probes.** Each indexed scalar field gets an in-memory
      `value-key → {id}` map (`<field>_index`), maintained like `id_to_row`: **after** the #89 WAL commit boundary
      on insert (add), update (remove-old + add-new — superseding-version aware, #66), delete (drop); **rebuilt on
      reopen folded into the same id-scan** as `id_to_row` (`generate_rehydrate_logic`, after #89 recovery so it
      keys only committed rows). Probes are an **O(1) index `get` (not a scan)** that resolve candidates through the
      version-aware read path: `find_by_<field>`/`get_by_<field>` via live `get`,
      `find_by_<field>_at`/`get_by_<field>_at(&Snapshot)` via `get_at` — the `_at` form resolves **the snapshot's
      version, not the live newest row**, and post-filters the resolved value so a candidate whose value changed
      after the snapshot is excluded. Guards `test_rust_generation_secondary_indexes` + `test_rust_generation_index_followups`.
    - **Index-subsystem follow-ups #100–#103 LANDED (2026-07-10).** The four deferred Phase-2 index gaps are now
      wired (pure generated code over existing storage — no new substrate, no publish gap):
      - **#100 FK-scalar indexing.** `*Target` (`RequiredReference` → `Uuid`) and `?Target` (`OptionalReference` →
        `Option<Uuid>`) FK fields are **always** indexed (a reverse one-to-many getter that would otherwise scan
        always exists), and the generated reverse getters (`user_posts_by_author`, …) now **probe** `find_by_<fk>`
        (O(1)) instead of scanning `all()` — the last linear scans in the read path are gone.
      - **#101 composite `@index(a, b)`.** One `HashMap<String,{id}>` per composite, keyed by a **collision-free
        length-prefixed** concat of each component's per-field key (`<byte-len>:<key>` per part, so `("ab","c")` ≠
        `("a","bc")`); `find_by_<a>_and_<b>` (+ `_at`) probes; maintained + reopen-rebuilt alongside single-field
        indexes. Hash exact-match only (answers `a=? AND b=?`, not prefix/range — a B-tree feature, out of scope).
      - **#102 nullable indexing.** `T?` scalar fields are now indexable; probe params are `Option<T>`
        (`Option<&str>` for strings), so `find_by_<field>(None)` probes the unset bucket. **Load-bearing fix:** the
        index key is now **null-distinct + type-tagged** (`\u{0}`=null, `\u{1}`+raw=string, `\u{2}`+text=other) so
        `None` and the literal string `"null"` can no longer collide (the exact hazard that made #90 gate nullable
        out — proven non-colliding E2E).
      - **#103 `DatabaseReader` snapshot index probes.** The read-only reader handle now carries a point-in-time
        **clone** of every index map (captured on the single writer at `reader()` time, same discipline as
        `id_to_row.clone()`) and emits **`_at`-only** probes (a reader has no live `get`) — its snapshot reads no
        longer scan. One shared `generate_index_probes(model, include_live)` emits {writer: live + `_at`} vs
        {reader: `_at`}, so there is no second probe body to drift.

    E2E (list filter+sort+paginate; single/composite/FK/nullable probe + snapshot-version resolution + post-filter +
    null-vs-`"null"` non-collision + update/delete maintenance + reader `_at` + reopen rebuild) proven through
    current codegen in `scratchpad/followups_compile` (ephemeral); full 18-schema `examples/` corpus (incl.
    integer-PK `iot-sensors`, multi-composite `food-delivery`, composite `ecommerce-store`) compile-checked in
    `scratchpad/corpus_check2`. **Honest limits (still deferred):** the hash index is exact-match only (no
    prefix/range); the reader index clone is O(rows) per `reader()` (an `Arc`-swap is the escape hatch if it matters).
    **Publish gap CLOSED (2026-07-10):** generated `api.rs` depends on **`forgedb-query-params` 0.1.0** (scaffold
    pins `= "0.1"`, published); reclose PROVEN by an outside-repo `init → generate rust+api → cargo build` resolving
    `forgedb-query-params 0.1.0` + `forgedb-storage 0.1.5` + `forgedb-wal 0.2.0` + changefeed/auth/types from
    crates.io (0 errors). The #100–#103 follow-ups added **no** substrate dep (indexes are in-memory generated code),
    so they did not reopen it.
  - **Data integrity is now enforced at write (v1 Phase 3 — #91 LANDED).** Generated writes refuse to commit
    a record that would break integrity — proven E2E (`scratchpad/phase3_compile`), full 18-schema corpus
    compile-checked (db+api), guard `test_rust_generation_data_integrity`. Three layers, all **generated
    per-field** (no runtime schema-reading validator):
    - **Field constraints.** A generated `validate_<model>(record) -> Result<(), ValidationError>` enforces
      `@min`/`@max` (numeric), `@length` (string, `@length(max)` or `@length(min,max)`), `@email`, `@url`, and
      **`@pattern`/`@regex`** (per-field `LazyLock<regex::Regex>`, non-match → 422 — #104 RESOLVED, see the dedicated
      bullet; adds a plain `regex = "1"` dep to the generated crate, no publish gap).
      Nullable fields validate only when `Some`. Called at the TOP of `insert`/`update`, before any durable side
      effect.
    - **`&unique`.** `insert`/`update` probe the Phase-2 unique index (`<field>_index`) before committing —
      insert rejects any existing key; update rejects a key held by a *different* id (own-id excluded, since index
      maintenance runs post-commit). Self-contained in `Storage` (it owns the index).
    - **Foreign-key existence.** Generated `Database::create_<model>` / `update_<model>` wrappers check each
      required FK (`*Target`) resolves, and each optional FK (`?Target`) resolves *when set*, via
      `self.<target>.get(fk)` — the one check needing sibling-collection access `Storage` lacks — then delegate.
      Only UUID-keyed targets (an FK is always a `Uuid`). The **REST create/update route through these wrappers**,
      so both the Rust API and REST get full integrity.
    - **Signatures + HTTP.** `insert -> Result<Id, ValidationError>`, `update -> Result<bool, ValidationError>`
      (`Ok(false)` = absent id); `delete` stays `-> bool`. `ValidationError::status_code()` maps Unique/DanglingRef
      → **409**, field Constraint → **422**; the generated handlers return those with the message. **No new
      substrate / scaffold dep** — validation is pure generated std code, so no publish gap.
    - **Honest limits / deferred:** `db.<model>.insert` (direct storage path) enforces
      field + unique but NOT FK (use `db.create_<model>` for full integrity — documented on the method); no
      cross-field / conditional constraints; validation is fail-fast (first violation returned, not a list).
      (`@pattern`/`@regex` are now ENFORCED — #104 RESOLVED.)
  - **Storage is now bounded under update/delete (v1 Phase 4 W1 — #92 LANDED, publish-pending).** Generated
    `Storage::compact()` + `Database::compact()` reclaim the dead (superseded/tombstoned) row versions the #66
    mutation surface leaves behind, **in-process under the single-writer lock** (never a background thread — that
    would compact off the #89 `DirLock`), auto-invoked once `COMPACTION_DEAD_THRESHOLD` (=1000 dead versions, fixed)
    is reached on update/delete. Ordering is load-bearing: `checkpoint()` first (fsync columns + truncate WAL, so no
    index-relative WAL tail survives the renumber), then reclaim, then reopen to rebuild `id_to_row` + indexes.
    **New substrate: `forgedb-compaction` gains `Compactor::compact_model_keeping(model, live_rows)`** — a
    schema-agnostic keep-set GC: generated code computes the LIVE physical-row set from `id_to_row` + tombstone
    liveness (the field-aware decision) and hands the opaque indices over; the substrate keeps exactly those rows.
    This was **not** just wiring — the pre-existing tombstone-based `compact_model` was fundamentally misaligned with
    #66: it reclaimed nothing from updates (superseded rows aren't tombstoned) and **resurrected deletes** (dropped
    the tombstoned marker, kept the old data row). W1 also fixed a pre-existing **variable-column filename bug**
    (compactor matched `*_data.bin` but codegen emits `string_data_<idx>.bin`, so variable columns were never
    compacted → reopen scrambled rows across columns) in `compactor.rs` + `stats.rs`. Guard
    `test_rust_generation_auto_compaction`; E2E `scratchpad/compaction_compile` (auto-compact at threshold, 87% byte
    reclaim, no resurrection, reopen rebuilds indexes); all 18 examples compile. **Publish gap OPEN:** generated code
    links `forgedb-compaction` (scaffold pins `= "0.1"`), so `forgedb-compaction 0.1.0` must publish before the
    reclose is proven (mirrors wal/storage/query-params). Offline `forgedb compact`/`vacuum` CLI (the legacy
    tombstone path, resurrection-prone against #66) is **REMOVED (#105 RESOLVED by deprecation)**: both subcommands
    mutate nothing and exit non-zero (code 6) with guidance pointing to the safe in-process `Database::compact()`
    (auto-invoked, keep-set-based). The substrate `compact_model` fn is doc-deprecated but retained (removing a
    published-crate public fn is breaking — no compaction publish gap reopened). Threshold not yet
    tunable (deferred).
  - **Additive migrations preserve data (v1 Phase 4 W2 — #92 LANDED).** Adding a field (nullable, or appended at the
    end) + regenerating + reopening no longer wipes the DB. Generated recovery anchors on the **tombstone count**
    (authoritative committed rows) and **backfills any column shorter than it** (a newly-added field) with the
    field's default, truncating only torn/ahead columns. (The old `min(...)`-across-columns truncation would collapse
    everything to a new empty column → total data loss.) Per-field default encodings reuse the exact append logic
    (nullable → None tag, numeric → 0, uuid/FK → nil, byte-blob types → zeroed bytes). Guard
    `test_rust_generation_additive_backfill`; E2E `scratchpad/migrate_compile` (v1 writes → v2 with 2 appended fields
    reads, existing rows intact, new fields defaulted). **Honest limits:** new fields must be **appended at the end**
    (columns are position-addressed); non-null new fields backfill to type-zero, not `@default` (follow-up); the
    old-WAL-across-schema-change replay is skip-on-error (needs a clean checkpoint before migrating).
  - **`migrate --auto` additive-vs-breaking gate (v1 Phase 4 W3 — #92 LANDED).** `forgedb migrate create --auto
    --schema <file>` now works: it diffs the schema against a recorded snapshot (`migrations/.schema-snapshot.forge`),
    accepts purely-additive deltas (new model / new nullable field — records the migration + advises regenerate +
    reopen), and **refuses any breaking change** (type change, field/model removal, non-null add, `&unique` add) with
    the dump→reload guidance and a **non-zero exit** (CI-detectable). Wiring only — the `SchemaDiffer` + `is_breaking()`
    already existed; W3 added the AST→`SimpleSchema` converter + snapshot persistence in `src/commands/migrate.rs`.
    **SUPERSEDED by #74 Phase 4:** the breaking gate no longer refuses — it records + classifies + scaffolds (see
    the #74 Phase 4 bullet); the integration test was renamed + repurposed to
    `test_migrate_auto_records_and_scaffolds_authored_hop`.
  - **Breaking-change reload path documented + tested (v1 Phase 4 W4 — #92 LANDED).** `docs/MIGRATIONS.md` documents
    the v1 answer for breaking changes: dump (`all()` → JSON via the generated `Serialize`), regenerate, reload into a
    fresh dir through `Database::create_<model>` with an app-level transform. Proven E2E (`scratchpad/reload_compile`:
    a `u32 → string` type change round-trips, ids preserved). **The data-transform migration engine stays out of v1.**
- **Schema-migrations version guard (#74 Phase 1 — LANDED 2026-07-15).** The first milestone of the unified
  regenerate + evolve-data + version-guard workflow (design `docs/proposals/schema-migrations-impl-plan.md`,
  product-gated ALIGNED-WITH-CONSTRAINTS). Turns a **silent byte mis-decode** of a stale data dir (written under an
  old schema, opened by regenerated code) into a **fail-fast refusal**. Three pure-codegen changes in
  `crates/codegen/src/rust.rs`, **no substrate change / no publish gap** (`Manifest.format_version` already
  published): (1) `write_manifest` (model + junction) load-preserves the on-disk `format_version` on reopen (same
  pattern as `compaction_epoch`) instead of the old hardcoded `format_version: 1` that would clobber a
  migration's bump; a fresh dir baselines to `EXPECTED_FORMAT_VERSION`. (2) An opaque per-schema
  `const EXPECTED_FORMAT_VERSION: u32 = 1` (baseline 1 until the Phase 2 lineage wires the derivation from the
  migration snapshot — red line #8). (3) `__open_with_lock` runs a guard BEFORE opening any column/WAL file: for
  every model/junction manifest that exists it loads exactly `format_version` and **panics** on mismatch with
  guidance pointing at the migration bin — it reads **one opaque integer, never column names/types to self-heal**
  (red line DV-6: refuse, don't adapt). Guards `test_rust_generation_version_guard` +
  `test_rust_generation_manifest_preserves_format_version`; E2E (throwaway path-dep crate `scratchpad/vguard`,
  ephemeral): write at v1 → reopen OK → externally bump manifest to v2 → reopen **refused**.
- **Schema-migrations hop classification + lineage (#74 Phase 2 — LANDED 2026-07-16).** The `migrate create`-time
  machinery that freezes each hop's body class and records the serial version lineage — the dev-time inputs the
  transformer bin (Phase 3) and `EXPECTED_FORMAT_VERSION` (Phase 1) consume. **No substrate change, no publish
  gap** (all in `crates/migrations/` + the `generate` CLI wiring). Three parts: (1) **Classifier (C8/C9):**
  `HopBodyClass::{Auto, Authored}` + `SchemaChange::hop_body_class()` — DISTINCT from `is_breaking()`; only the
  residue the differ cannot prove a value for (`ChangeFieldType`, nullable→NOT-NULL narrowing,
  required-add-without-default) is `Authored`, while breaking-but-provable changes (drop field/model, `&unique`
  add) stay `Auto`. (2) **Lineage (`lineage.rs`):** `Migration` gains serial `from_version`/`to_version`
  (`#[serde(default)]`, checksum-covered); `MigrationLineage::{load, current_format_version, next_version_span,
  expand_range}` — `expand_range` walks the ordered contiguous hop span a `--from B --to G` transformer replays
  (refuses a non-contiguous/out-of-range span — C1, never a synthesized jump); `scaffold_authored_body` writes
  `migrations/{id}/transform.rs` only for `Authored` residue and **never clobbers a frozen body** (C13). (3)
  **Wiring:** `migrate create --auto` stamps each additive migration `v_n→v_{n+1}`; `EXPECTED_FORMAT_VERSION` is
  now **lineage-sourced** (red line #8) — the `generate` CLI threads `current_format_version("migrations")` into
  the new `RustGenerator::generate_with_format_version` (`generate(&schema)` still baselines to 1 so the 72
  codegen snapshots stay byte-identical). Guards `test_diff_classifies_authored_body_residue` +
  `test_migration_lineage_expands_range` + `test_scaffold_authored_body_only_for_residue` + extended
  `test_rust_generation_version_guard`. E2E (`scratchpad/phase2`, ephemeral): baseline →
  `EXPECTED_FORMAT_VERSION = 1`; add nullable field + `migrate create` → migration JSON records `from_version:1,
  to_version:2`; regenerate → `EXPECTED_FORMAT_VERSION = 2`. **Deferred:** the offline transformer bin that
  actually rewrites data + deletes the stubbed/buggy `executor.rs` byte-op surface (Phase 3), and workflow
  unification + per-tenant sweep (Phase 4). The breaking-change `--auto` path still refuses with the reload
  guidance in Phase 2 (the gate is unchanged; the scaffolder is a tested library capability the Phase 3 CLI
  invokes once the transformer can run authored bodies).
- **Schema-migrations transformer bin — uniform typed replay (#74 Phase 3 — LANDED 2026-07-16).** The offline
  **transformer bin** that actually rewrites data-at-rest from an old schema version to a new one — the one
  operator artifact, generated per origin→destination range. **No substrate change, no publish gap** (the
  mechanism is "generate a per-version `database.rs` per version in the range + a straight-line replay over
  them"). New generator `crates/codegen/src/transform.rs` (`TransformGenerator`, joining
  Rust/TS/Api/Stub/OpenApi/Wasm) emits the crate: `Cargo.toml` (**provider-free** — links the app's substrate
  but NO `forgedb-parser` / `forgedb-migrations`), one `src/vN.rs` per version (each a
  `RustGenerator::generate_with_format_version(schema, N)` emission, so its open-guard enforces the version
  interlock — C5/C11), an embedded `src/authored_<id>.rs` per `Authored` hop (**frozen verbatim** — C13), and
  `src/main.rs` = a **fixed straight-line** chain of named `transform_vN_to_vM` hop fns (**no `Vec<Step>`
  interpreter, no runtime dispatch** — C2/C8/DV-11). Each hop reads every row via the `vN` typed structs,
  applies the frozen structural JSON ops (rename / remove / additive-add — field-name keys baked from the
  diff, **never a schema read at runtime** — C1) then the hop's authored transform, decodes into the `vM`
  struct, and writes via `vM`'s `insert` (which **preserves the record's id**). Multi-hop ranges replay
  through intermediate temp dirs and **atomic-rename once at the end** (all-or-nothing; retained source =
  rollback). Per-version schemas are self-describing: `migrate create` now snapshots each version's full
  `.forge` under `migrations/schemas/vN.forge`, which the generator loads for its range. The buggy/stubbed
  `executor.rs` byte-op surface is **DELETED** (its two workflow tests too). CLI: **`forgedb generate transform
  --from F --to T`**, **`forgedb migrate build --from F --to T`** (generate + `cargo build`), **`forgedb
  migrate run --src <data> --dest <migrated>`** (run the built bin); the old executor-backed `migrate up`/`down`
  are removed. Guards (identity trio + more): `test_transform_bin_has_no_schema_runtime` (C1),
  `test_transform_bin_replay_is_straightline` (C2/C8/DV-11), `test_transform_bin_deps_are_provider_free`
  (C4/DV-7), `test_transform_bin_embeds_frozen_authored_body` (C13), `test_transform_generation_snapshot`. E2E
  (`scratchpad/transform_e2e`, ephemeral — **compile-test discipline**): a real 3-version lineage (v1→v2
  additive `bio: string?` = `AutoBody`; v2→v3 `age` u32→string = `AuthoredBody`) → `generate transform --from 1
  --to 3` → **the emitted crate compiles** (all 3 version modules + hop code + substrate) → seed 2 v1 users →
  run the bin → reopen at v3: `age` re-encoded to a string, `bio` additive-defaulted to `None`, ids preserved,
  **source dir retained**; feeding a v3 dir to the v1-expecting bin is **refused** by the open-guard (the
  interlock). **All the prior "Deferred to Phase 4" items now LANDED — see the Phase 4 bullet below.**
  **Honest limit:** the transformer copies id-bearing model rows +
  M2M junction pairs; non-id value tables are out of scope (as they are for the mutation surface), and awkward
  serde-repr non-null additive defaults (char(N), timestamp) are best-effort.
- **Schema-migrations workflow unification + per-tenant sweep (#74 Phase 4 — LANDED 2026-07-16). The #74 epic
  is now COMPLETE.** The one-CLI operator lifecycle over the Phase 3 transformer. **No substrate change, no
  publish gap** (all in `src/commands/migrate.rs` + the migrate CLI + docs). Three parts: (1) **`migrate create
  --auto` breaking gate FLIPPED.** It no longer refuses breaking changes with reload guidance — every non-empty
  diff is recorded as a versioned hop and classified `Auto`/`Authored` (`SchemaChange::hop_body_class()`);
  `Authored` residue (type change / nullable→NOT-NULL / required-add-without-default) is scaffolded at
  `migrations/{id}/transform.rs` via `scaffold_authored_body` (never clobbers a frozen body). Purely-additive
  keeps the cheap reopen-backfill fast path (prints the backfill advice); anything that rewrites data-at-rest
  prints the `migrate up` next steps. (2) **`forgedb migrate up`** — the one-CLI lifecycle: resolves the range
  (`--from` defaults to the version read from the source dir's `<model>/manifest.json`, `--to` to
  `MigrationLineage::current_format_version()`), builds the transformer once (`compile_transformer`), and runs
  it (`run_transformer`) over `--src`/`--dest` **or** every data dir under `--tenant-root` (rolling per-tenant
  sweep → `<root>/<t>-migrated-v<to>`; a failed/wrong-version tenant is reported + skipped, source unchanged,
  non-zero exit if any failed). The lower-level `migrate build` / `migrate run` / `generate transform` stay as
  primitives (both share `compile_transformer`/`run_transformer`). (3) **Docs:** `docs/MIGRATIONS.md` rewritten
  around the single lifecycle (retired the stale "no engine / dump→reload / `migrate up --steps`" framing); the
  `forgedb-migrations` module doc de-references the deleted `MigrationExecutor`. Guard: the #92-W3 integration
  test was renamed + repurposed in place `test_migrate_auto_diff_additive_and_breaking_gate` →
  **`test_migrate_auto_records_and_scaffolds_authored_hop`** (asserts additive succeeds with backfill advice,
  a `u32→string` type change is now RECORDED + `[authored]`-marked + scaffolded + prints `migrate up`, and the
  authored `transform.rs` stub is written — net-0 test count, still 498). **Deferred (documented in
  `MIGRATIONS.md`):** `compaction_epoch` verification before apply (C11 — the format-version guard is the
  interlock today); `@default` on additive backfill; cheap in-place byte-op hops; online (live-writer)
  migration.

  Phases 1–4 of the v1 spine are now LANDED (Phase 4 W1's `forgedb-compaction 0.1.0` published + reclose proven).
  **Phase 5 (#93 — ship) is COMPLETE: all six workstreams landed — WS1 (observability), WS2 (deploy),
  WS3 (docs), WS4 (distribution), WS5 (SDK), WS6 (semver).** WS3 shipped four docs grounded in a real
  `init → generate → build → serve → curl` e2e run: `docs/GETTING_STARTED.md`, `docs/SCHEMA.md` (the
  parser-verified `.forge` reference, promoted from `docs/proposals/corpus/forge-grammar-reference.md`),
  `docs/DEPLOYMENT.md`, and `docs/WHAT_V1_IS.md` (the honest "what v1 is / isn't"). v1 scope is locked:
  **design-partner bar, single-writer-per-process, migrations data-engine deferred** — see `docs/V1_ROADMAP.md`
  and epics #89–#93.

  - **Observability (v1 Phase 5 WS1 — LANDED).** Generated axum router serves unauthenticated ops routes `/health`
    (liveness, DB-free), `/ready` (read-lock probe → 200), `/metrics` (minimal JSON: per-model live `row_count()` +
    totals, generated by naming each storage field). Router restructured — the #59 tenant-auth guard now wraps only
    `__data_routes()`; `__ops_routes()` is merged in *after* the guard, so k8s/LB probes need no JWT. Structured logging
    is the standard stack: `tracing` + `tracing-subscriber` (`env-filter`, honors `RUST_LOG`, default `info`) + a
    `tower_http::trace::TraceLayer` request span on the router — nothing hand-rolled; `FORGEDB_LOG_FORMAT=json` toggles
    JSON lines. Scaffold `Cargo.toml` gains `tower-http`/`tracing`/`tracing-subscriber` (plain crates.io deps, **no**
    substrate publish gap). Guard `test_api_generation_observability_endpoints`; live-server E2E (all three ops routes
    200 + JSON logs) in `scratchpad/ws_compile`.
  - **Deploy story (v1 Phase 5 WS2 — LANDED).** `forgedb init` emits a blessed container path: multi-stage `Dockerfile`
    (rust-slim build → debian-slim runtime, non-root user, `/data` VOLUME, `FORGEDB_HOST=0.0.0.0`, `HEALTHCHECK` on
    `/health`), `.dockerignore`, and `docker-compose.yml` (named data volume + env config incl. commented
    tenancy/JWT/JSON-log knobs). Scaffold `main.rs` hardened: graceful shutdown drains on SIGINT/SIGTERM
    (`axum::serve(..).with_graceful_shutdown`), plus the tracing init. Compile + run-proven through current codegen.
  - **SDK completeness (v1 Phase 5 WS5 — LANDED).** Generated TS SDK (`types.ts`) rewritten to full CRUD faithful to the
    real REST contract: `get` (404→null), `list(options)` with pagination/sort/filters → `ListResult<T>{data,total,limit,
    offset}`, `create` (returns new id; throws `ForgeDBError` on 409/422), `update` (whole-record PUT, 404→false),
    `delete` (204→true/404→false); per-model `<Model>Create = Omit<Model,'id'>` input types; typed `ForgeDBError`
    (status + parsed body). npm-publishable — `forgedb generate {typescript,all}` also emits `package.json` +
    `tsconfig.json` next to `types.ts`, **only if absent** (regenerate/`--force` never clobbers them). `tsc --noEmit`
    (strict) clean; live `list` returns the `ListResult` shape. **Honest limits:** ids are typed `string` uniformly (URL
    paths are strings — integer-PK callers pass `String(n)`); `create` returns the id, not the full record (the REST
    create responds `{id}`); no WS/subscription client yet (REST only).
  - **Distribution (v1 Phase 5 WS4 — LANDED).** `cargo install forgedb` now works from crates.io: the CLI's full
    internal crate closure was published leaves-first — **`forgedb-validation` / `-parser` / `-codegen` / `-migrations`
    / `-backup` / `-watcher` 0.1.0** (joining the already-published substrate) and the root **`forgedb` 0.1.0** — each
    with package metadata + version-pinned path deps. Proven E2E by an isolated-`CARGO_HOME` `cargo install forgedb`
    resolving all 7 from crates.io and running the binary. Prebuilt cross-platform binaries via
    `.github/workflows/release.yml` (tag `v*` → Linux x86_64/aarch64 + macOS Intel/ARM + Windows → GitHub Release).
    `docs/INSTALL.md` covers every install path + the substrate version matrix. **Note:** these 6 compiler crates are
    now on crates.io **only so `cargo install` can build the CLI** — per `docs/SEMVER.md` they are explicitly NOT a
    stable public API (unlike the substrate crates, which are). **Honest limit:** the release workflow is authored +
    YAML-validated but not yet run by a real tag push.
  - **Semver / stability (v1 Phase 5 WS6 — LANDED).** `docs/SEMVER.md` states the compatibility policy across four
    surfaces (schema language, substrate ABI incl. on-disk format, CLI + `--json` outputs, compiler-internals
    carve-out) and what a 1.0 commits to. Currently pre-1.0, so the guarantees are stated as the policy that takes
    effect at 1.0; the schema-language additive-vs-breaking boundary agrees with the #92 migration gate.

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
- **`init → build` publish gap — CLOSED again 2026-07-10 (#90):** Phase 2 made generated `api.rs` require
  **`forgedb-query-params 0.1.0`** (list-endpoint filter/sort/paginate parsing); it is **now published**, and the
  reclose is PROVEN by an outside-repo `forgedb init --template blog → generate rust+api → cargo build` resolving
  `forgedb-query-params 0.1.0` + `forgedb-wal 0.2.0` + `forgedb-storage 0.1.5` + `forgedb-changefeed 0.1.1` +
  `forgedb-auth 0.1.0` + `forgedb-types 0.2.0` from crates.io and compiling the generated list + secondary-index
  code (0 errors). Scaffold pins **`forgedb-query-params = "0.1"`**. Prior close (#89/#96, 2026-07-10): the durable
  write path + WAL checkpoint made generated code require **`forgedb-wal 0.2.0`** (opaque `Raw` path + `truncate`)
  + **`forgedb-storage 0.1.5`** (`truncate_to_rows` + `DirLock` + column `flush`); **both are now published** (wal
  first then storage per the dep order), reclose proven the same way (durable-write + checkpoint code). Prior history below. #56-B (single-writer/many-reader) added
  read-only column reader handles to `forgedb-storage` (`FixedColumnReader`/`VariableColumnReader`/
  `TombstonesReader` + `*::reader()`), bumping it **0.1.3 → 0.1.4**; generated `*StorageReader` /
  `DatabaseReader` call `col.reader()`. **`forgedb-storage 0.1.4` is now published**, and the reclose is
  PROVEN by an outside-repo `forgedb init --template blog → generate rust → cargo build` resolving
  `forgedb-storage 0.1.4` + `forgedb-changefeed 0.1.1` + `forgedb-types 0.2.0` from crates.io and compiling
  the generated reader code. (#62-B live queries needed **no** substrate change — the changefeed already
  carried the coarse signal — so `forgedb-changefeed` stayed 0.1.1.) `wal` 0.1.1 / `types` 0.2.0 unchanged.
  Scaffold pins `forgedb-storage = "0.1.5"`, **`forgedb-changefeed = "0.2"`** (#82 — bumped from "0.1"),
  **`forgedb-wal = "0.2"`** (#89),
  **`forgedb-auth = "0.1"`** (#59), **`forgedb-query-params = "0.1"`** (#90), **`forgedb-compaction = "0.1"`** (#92),
  axum `ws`. **#82 CLOSED 2026-07-13:** realtime Direction C (durable replication broker) made
  generated `database.rs` + `api.rs` link **`forgedb-changefeed 0.2.0`** (the `durable` module + `/replicate`
  endpoint); `forgedb-changefeed 0.2.0` is **published** and the reclose is PROVEN by an outside-repo
  `init --template blog → generate rust+api → cargo build` that **downloaded `forgedb-changefeed 0.2.0`** (+
  storage 0.1.5 / wal 0.2.0 / query-params 0.1.0 / compaction 0.1.0 / auth 0.1.0 / types 0.2.0) from crates.io
  and compiled the generated replication code (0 errors). **#92 CLOSED 2026-07-11:** Phase 4 W1 made generated code link **`forgedb-compaction`** (in-process
  auto-compaction); `forgedb-compaction 0.1.0` is published and the reclose is PROVEN by an outside-repo
  `init → generate rust+api → cargo build` resolving it (+ storage 0.1.5 / wal 0.2.0 / query-params 0.1.0 /
  changefeed / auth / types) from crates.io (0 errors). History: the gap reopened for #57, #62-A, #66, #56-B, #59,
  #89/#96, #90, #92, and #82 — **all now closed** (query-params 0.1.0 closed #90 on 2026-07-10; wal 0.2.0 +
  storage 0.1.5 closed #89/#96; compaction 0.1.0 closed #92; **changefeed 0.2.0 closed #82 on 2026-07-13**).
  #59 closed
  2026-07-09: `forgedb-auth 0.1.0` published + PROVEN by an outside-repo `forgedb init → generate rust+api
  → cargo build` resolving `forgedb-auth 0.1.0` + `forgedb-storage 0.1.4` + `forgedb-changefeed 0.1.1` +
  `forgedb-types 0.2.0` from crates.io and compiling the generated code **and** the env-driven scaffold
  `main.rs` (which links `forgedb-auth`). **#75/#84 MVCC reopened + CLOSED again 2026-07-15:** generated code started
  linking two NEW substrate crates `forgedb-txn 0.1.0` + `forgedb-coordinator 0.1.0` (scaffold pins both `= "0.1"`) +
  additive methods `WalManager::truncate_to` (`forgedb-wal`) + `sync_from_disk` (`forgedb-storage-native`, mirrored as
  a no-op on `forgedb-storage-web`); **all published** (`txn 0.1.0`, `coordinator 0.1.0`, `wal 0.2.2`,
  `storage-native 0.1.1`, `storage-web 0.1.1`) and the scaffold pin moved `forgedb-storage = "0.1.5"` → `"0.2"` (the
  facade, which the monolith 0.1.5 could not provide `sync_from_disk` from). Reclose PROVEN by an outside-repo
  `/tmp init → generate rust+api → cargo build` resolving them all from crates.io (0 errors). **Next thing that will
  reopen it:** any new substrate-crate dep or additive substrate API the generated code starts requiring — publish
  before the scaffold pins it.
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
  (`user_posts(id) -> Vec<Post>`, now an **O(1) FK-index probe** `find_by_<fk>` since #100 — no longer a
  scan; when a child has multiple FKs back to one parent collection the getter disambiguates by child field,
  e.g. `user_posts_by_author`), **many-to-many** persisted junction structs (`PostTagLink` with
  `left`/`right` UUID columns) plus `link_post_tag` / `post_tags` / `tag_posts`, and
  **eager-load** structs (`PostWithRelations { post, author: Option<User>, … }` +
  `post_with_relations(id)`). Honest limits: traversal is generated only between **UUID-keyed**
  models (FK scalars are always `Uuid`, so integer-PK targets are skipped with a comment);
  **M2M** junction lookups (`post_tags`) are still **linear scans** (junction-column indexing is a #100
  follow-up step); there is **no M2M `unlink`** (storage
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
  per-model filter compares **typed per-field** — it parses the `?field=value` string into the field's Rust
  type and compares typed values (`?n=3` matches a stored `3.0`; bool/uuid/decimal/timestamp/enum by value),
  the old `serde_json` stringify float/bool fragility resolved by #84 (see the #84 bullet); Direction B (live
  queries) + C (durable broker) deferred. Guards: substrate `changefeed` unit tests, `test_rust_generation_changefeed_emits`,
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
  event per connection**, no coalescing/debounce (the scaling cliff — #83); single-process. (`Updated`
  detection is now a typed per-field compare via the generated `<model>_record_changed`, #84 — the old
  full-record `serde_json` stringify fragility is gone.) Guards:
  `test_api_generation_live_query`, `test_rust_generation_live_delta_enums`; **live WS round-trip**
  (Init→Added→silent→Updated→Removed) in `scratchpad/directionb_compile` (ephemeral).
- **Durable replication broker (#82 realtime Direction C) — Milestones A + B LANDED (publish-pending).**
  The deferred networked/durable/resumable slice of #62, and the critical-path substrate that unblocks
  the WASM read-replica (#110). Two milestones landed:
  - **A — substrate core (`forgedb-changefeed::durable`).** A `DurableBroker` records each committed
    change to a **CRC-framed, append-only log** (mirroring `forgedb-wal`'s framing + torn-tail recovery)
    at a **monotonic global `u64` offset** — the single **opaque cross-model ordering token** (PM
    constraint #2; single-writer `record` order == commit order). Resumable API: `read_from(after,max)`
    (durable replay), `subscribe()` (live tail), `catch_up_from()` (the race-free stitch: subscribe →
    replay to a `boundary` → drain live **skipping `offset <= boundary`**, i.e. idempotent by absolute
    offset — PM constraint #1), plus `watermark()`/`earliest_retained()` (snapshot-vs-tail cutover) and
    `prune_through()` (retention). Frame `PersistedEvent { offset, model, row_index, kind, bytes }` is
    **field-blind by construction** (no field-typed member; `model` an opaque tag; `bytes` the opaque
    committed row bytes carried verbatim, with a public `to_wire`/`from_wire` binary codec == the log
    framing). Bumped changefeed **0.1.1 → 0.2.0**. 17 unit tests (monotonic offsets, verbatim
    opaque-byte round-trip, cold replay, offset-continuity across reopen, torn-tail recovery, prune,
    gap-free/dup-free catch-up, wire round-trip).
  - **B — generated wiring.** Each generated mutation now **also** records to the shared broker
    alongside the best-effort change-feed emit: `insert`/`update`/`delete` hand it the model name + the
    tombstone/superseding physical `row_index` + the **same opaque `serde_json(record)` bytes the WAL
    journals** (never a decoded field); M2M `link` records the 32-byte opaque pair. `Database` owns one
    `Arc<Mutex<DurableBroker>>` (durable log `<root>/_replication.log`), created in `open_at` and
    attached to every collection (`attach_broker`), `None` on the standalone `new()` path; compaction
    preserves it across reopen. A single schema-wide generated WS endpoint **`GET /replicate?after=<offset>`**,
    **inside `__data_routes` so it sits behind the #59 `forgedb-auth` tenant guard** (reuses the SAME
    extractor — PM constraint #4), does the resumable handshake via `catch_up_from` and streams
    `PersistedEvent::to_wire()` **binary** frames (opaque; the handler decodes nothing). Guards
    `test_rust_generation_replication_broker` + `test_api_generation_replication_endpoint` (both assert
    no `match model_name`). **Compile-verified** (full `init`-style crate: db+api `cargo check` clean via
    path deps) + **server-record E2E** in `scratchpad/dirc_compile` (ephemeral): 6 mutations through real
    generated code → 6 records at monotonic offsets 1..6, correct kinds + append-only row positions
    (insert row 0 / update row 1 / delete row 2), 32-byte link, resumable replay from offset 3.
    **Honest limits / deferred:** the **live WS round-trip** E2E + the browser follower **apply** path
    land with Milestone C (= #110, OPFS/`wasm32`); the durable-log fsync policy is fixed `Always` and the
    retention/`prune_through` trigger is not yet auto-wired (manual today); M2M follower apply stays out
    of M1 (UUID-only-traversal inheritance); single-process source of truth.
    **Publish gap CLOSED (2026-07-13):** generated `database.rs` + `api.rs` link **`forgedb-changefeed 0.2.0`**
    (`durable` module + `/replicate`); it is **published**, scaffold pin bumped `= "0.1"` → **`= "0.2"`**, and
    the reclose is PROVEN by an outside-repo `init --template blog → generate rust+api → cargo build` that
    downloaded `forgedb-changefeed 0.2.0` (+ storage/wal/query-params/compaction/auth/types) from crates.io and
    compiled the generated replication code (0 errors).
- **Browser read-replica follower (#110 realtime Milestone C) — LANDED 2026-07-13 (wasm substrate published 2026-07-14).**
  The `wasm32` build of the SAME generated `database.rs` running in the browser as a **read-only replica** that
  catches up from the server's `/replicate` stream (#82) and persists locally, so the UI queries a local WASM
  instance instead of the network. Proven E2E in a real browser (Playwright) for **BOTH IndexedDB and OPFS**:
  server insert/update/delete/link → real `PersistedEvent::to_wire` frames → WS `/replicate?after=W` → wasm
  `applyWire` → generated `apply_frame` → correct local reads (update/delete/link reflected, reverse traversal)
  → `commit()` to IDB/OPFS → **reload → resume from the persisted watermark, 0 frames re-applied, data intact.**
  Five layers:
  - **L0 — wasm-safe substrate.** `forgedb-types` gains a `cfg(wasm32)` uuid `js`/getrandom feature (additive,
    native unchanged); `forgedb-wal` drops its (unused) uuid dep and splits `WalManager` — the file impl is
    `cfg(not wasm32)`, an **in-memory** impl is `cfg(wasm32)` (the design note drops the file WAL on wasm;
    durable at commit granularity). `FsyncPolicy` moved to the wal crate root (shared, both targets).
  - **L1 — storage facade split.** `crates/storage` → thin `cfg` facade over **`storage-native`** (moved engine,
    renamed, 0.1.0, native surface unchanged) and NEW **`storage-web`** (0.1.0): in-memory arena columns with
    **byte-identical positional semantics** (a `thread_local` path→bytes STORE — the "path IS the key" trick),
    no-op `DirLock`, arena-routed `Manifest`, + a `persist` module (wasm-only: IndexedDB keyed-blobs + OPFS
    **per-column files** — the async **hydrate/commit** boundary; the per-row API stays sync). The OPFS path (#1,
    LANDED 2026-07-13) writes each arena column to its own file `<db>/<flat-path>` via **`createSyncAccessHandle`
    in a dedicated Web Worker** (the only context that exposes the sync OPFS handle API); storage-web spawns that
    Worker itself from an embedded Blob URL (no external `.js` asset, no codegen change — the main-thread replica,
    transport, and generated code are untouched). storage-web's arena core is target-agnostic, so its parity is
    unit-tested natively. Facade bumped 0.1.5 → 0.2.0.
  - **L2 — codegen (target-agnostic, zero branches).** Additive `Database::commit()` (flush all cols; native
    fsync / wasm arena no-op) + the follower apply path: per-model `apply(kind, bytes)` decodes the opaque row
    bytes and **replays through the SAME generated `insert`/`update`/`delete`** (broker/feed are `None` on a
    follower, WAL is in-memory, so those side effects are inert — no second write path, no drift), a schema-wide
    `Database::apply_frame(&PersistedEvent)` dispatching on the **opaque model tag** (never a decoded field), and
    an `ApplyError` type. **The generated `database.rs` compiles to `wasm32-unknown-unknown` with ZERO codegen
    branches** — the facade absorbs the target difference (the identity proof: no WASM generator branch). Guard
    `test_rust_generation_replica_apply_path`; runtime apply round-trip also proven natively.
  - **L3 — `wasm-bindgen` transport (class-2 glue) — now GENERATED per-schema (#3 follow-up LANDED 2026-07-13).**
    A new codegen artifact `WasmGenerator` (`crates/codegen/src/wasm.rs`, joining Rust/TypeScript/Api/Stub/OpenApi)
    emits the `#[wasm_bindgen]` `Replica` per schema: schema-invariant lifecycle (open/applyWire/commit/watermark)
    plus a read surface that MIRRORS the generated `Database`'s live reads EXACTLY — per-model get/count/all + every
    relation traversal (forward FK, reverse one-to-many, M2M queries), reusing `RustGenerator`'s own
    `to_snake_case`/`is_uuid_pk`/`valid_m2m` name derivation (promoted `pub(crate)`) so names never drift. It
    invents no query API and exposes no mutators (a read-only follower) — the identity red line, guard-checked.
    Wired into the CLI as `forgedb generate wasm` (emits `replica/{Cargo.toml (only-if-absent),src/lib.rs,
    src/database.rs}`); opt-in in `generate all` (only when `[generate].targets` lists `wasm`). `wasm-pack build
    --target web` → `.wasm` + ES module + `.d.ts`. Guard `test_wasm_generation_transport`.
  - **L4 — browser E2E.** A native `genframes` bin emits real `to_wire` frames; a Bun WS server streams them
    honoring `?after`; Playwright drives the round-trip + reload-resume for IDB and OPFS. Ephemeral harness
    `scratchpad/wasm_l2/`. **Re-proven end-to-end with the GENERATED transport (#3) AND the OPFS Worker (#1):**
    `forgedb generate wasm` → `wasm-pack build` → both backends green (phase-1 applies all 7 frames incl.
    update/delete/M2M-link + reverse traversal; reload resumes from watermark 7 with 0 frames re-applied, data
    intact). #1 additionally verified in-browser that OPFS writes 25 discrete per-column files (e.g.
    `user/fixed/uuid_0.bin` @ 64 B = 4 uuids, `user/tombstones.bin` @ 4 B), not one snapshot blob.
  PM identity red lines held: the replica is the SAME generated code (no schema read at runtime); dispatch is by
  the opaque model tag; the storage backend + `persist` move opaque path→bytes blobs and know no schema.
  - **#2 — engine-in-Worker + partial hydrate + incremental commit — LANDED 2026-07-13 (wasm substrate published 2026-07-14).**
    The design discussion settled on **Architecture ①** (engine in a dedicated Web Worker, so the storage arena can
    do SYNCHRONOUS fault-in from OPFS sync-access handles, which are Worker-only). PM re-gate =
    **PASS-WITH-CONSTRAINTS** (5 binding constraints, all honored — see `memory/wasm-replica-plan.md`). Five WS
    landed + e2e-proven (Playwright, IDB + OPFS × stream + reload-resume, **0 frames re-applied**):
    - **WS1 substrate (`forgedb-storage-web`):** `store` gains a `LazySource` hook — a lazily source-backed column
      answers `byte_len` from the source's on-disk size WITHOUT reading bytes, and faults the whole column in
      **synchronously** on first real read (`with_bytes`/`with_bytes_mut`), so untouched columns never load and the
      generated per-row API stays sync (PM constraint 1). `persist` (OPFS) opens a sync-access handle per file at
      open (metadata only), registers an `OpfsSource`, and **commits incrementally** — each resident column's grown
      append-only tail, watermark marker written LAST (crash leaves columns ahead of the watermark; re-apply is
      logically idempotent since `all()`/`get()` resolve one record per id). A **no-op `truncate_to_rows(len)`
      early-returns** without faulting the column in — load-bearing, since `recover_from_wal` calls it on every
      column at open. #1's helper-worker OPFS is SUPERSEDED (engine now runs in the Worker). IDB path unchanged
      (eager load + whole-snapshot commit).
    - **WS2 Worker bootstrap (`WasmGenerator::worker_bootstrap`):** a STATIC, schema-agnostic script (NOT emitted by
      any schema-aware path — PM constraint 3). Runs the wasm engine, owns the `/replicate` WS, debounces
      auto-commit (idle 250 ms or 100 frames), probes sync-access-handle semantics to pick OPFS-vs-IDB, and
      dispatches reads generically via `replica[method](...args)` — interprets no schema.
    - **WS3 async client (`WasmGenerator::generate_client`):** a per-schema TS `ReplicaClient` (main thread) that
      RPCs into the Worker and mirrors the `Replica`'s read surface EXACTLY (reuses the ONE `read_surface`
      enumerator — PM constraint 2), invents no query, exposes no mutator. `generate wasm` now also emits
      `replica/client/{replica-client.ts, replica-worker.js}`.
    - **WS4 capability probe:** the Worker's runtime sync-semantics probe (a temp sync-access handle; `getSize()`
      returns a number iff sync) selects the backend; `"auto"` falls back to IndexedDB.
    - **WS5 e2e:** the harness page drives the generated `ReplicaClient` → Worker → engine. **Partial hydrate proven
      in-browser:** after the truncate fix, the unindexed `Tag.label` variable column NO LONGER faults in at open
      (it was in the fault-in set before), i.e. it stays lazy until a Tag is read. Guards
      `test_wasm_generation_async_client_and_worker` + 5 storage-web unit tests.
    **Narrow index rehydrate — LANDED 2026-07-13 (the prior scoped follow-up, now closed).** Reopen index
    rebuild no longer decodes the full record via `db.get()`; it reads **only the indexed columns** (the union of
    single-index fields ∪ composite components) at each id's newest physical row. `generate_rehydrate_logic`
    (rust.rs) resolves `__row` from the already-built `id_to_row`, applies the SAME tombstone liveness gate
    `read_at`/`get` use (so only live values are indexed), then decodes the indexed fields via the **shared
    `field_read_stmt` per-field decode path** — extracted from `generate_read_at_logic`, so full-record reads and
    the narrow rebuild are one body and cannot drift (read_at output stayed byte-identical; only the secondary-index
    snapshots changed). **Result:** an indexed model now faults in only `{id} ∪ {indexed columns}` at open —
    non-indexed columns of an indexed model stay lazy on the wasm backend (partial hydrate now works for User/Post,
    not just index-free Tag); native pays strictly less reopen I/O for any model with fewer indexes than columns.
    Guard `test_rust_generation_reopen_index_rebuild_is_narrow` (asserts no `db.get(__id)` in the loop, the
    `id_to_row`+tombstone gate, indexed columns read at `__row`, non-indexed `bio` column NOT read). E2E
    `scratchpad/rehydrate_compile` (ephemeral): compile + reopen proving every index kind — unique / single scalar /
    nullable / required FK / optional FK / composite — rebuilds correctly after reopen, superseded update values are
    gone, tombstoned deletes are not indexed, full records intact. **Residual limit:** the indexed columns
    themselves still fault in at open (unavoidable — the index needs them). Also: OPFS handles held open for the
    session; debounced durability (crash re-streams from watermark); M2M `link` re-apply on a torn commit can
    duplicate a junction pair (append-only, traversal not deduped).
  **Honest limits / deferred (Milestone C overall):** **read-only** replica (local/optimistic writes = Phase 2).
  (The prior "wasm-bindgen transport hand-written in the harness" limit is RESOLVED by #3; the "whole-DB hydrate /
  main-thread OPFS" limits are RESOLVED by #2 above.)
  **WASM PUBLISH GAP — CLOSED (2026-07-14):** the browser build's substrate set is now all on crates.io —
  **`forgedb-types 0.2.1`** (wasm uuid feature) + **`forgedb-wal 0.2.1`** (wasm in-memory `WalManager`) republished,
  and NEW **`forgedb-storage-native 0.1.0`** + **`forgedb-storage-web 0.1.0`** + the **`forgedb-storage 0.2.0`**
  facade published. The reclose is proven LIVE by each dependent's publish verify-build resolving its freshly-published
  deps from the registry (the facade pulled `storage-native 0.1.0`; `storage-native` pulled `wal 0.2.1`). The generated
  wasm replica scaffold pins (`forgedb-storage = "0.2"`, `-wal = "0.2"`, `-types = "0.2"`, `-changefeed = "0.2"`,
  `-compaction = "0.1"`) now all resolve. **RESIDUAL:** the full outside-repo `wasm-pack build` reclose was NOT run —
  this environment's `wasm32-unknown-unknown` std is broken (`can't find crate for core`); the **host-side registry
  resolution** is proven, the wasm-pack reclose is deferred to a working wasm toolchain. The **native** `init→build`
  gap stays CLOSED — the apply/commit codegen uses only already-published surface (storage `flush`, changefeed 0.2.0
  `PersistedEvent`), the M2M-unlink junction `Tombstones` is the already-published `Tombstones` type, and the
  **server** scaffold still pins published `forgedb-storage = "0.1.5"`. Committed 2026-07-13 (types/wal/storage-split +
  codegen apply/commit); #3 (WasmGenerator + `generate wasm`) committed 2026-07-13; #2 (engine-in-Worker partial
  hydrate) committed; substrate publish (`types 0.2.1` / `wal 0.2.1` / `storage 0.2.0` / `-native` / `-web`) 2026-07-14.
- **Column projection / partial model reads (#113) — LANDED 2026-07-14 (design note removed post-ship; see #113 + git history).**
  A declared model-level directive `@projection(card: title, slug)` generates a tailored `<Model>Card` struct +
  narrow reads (`get_card`/`all_card`/`read_card_at` + snapshot `_at`) that materialize **only PK + the selected
  columns** — never the full record. **No substrate change, no publish gap** (the mechanism is "generate a narrower
  `read_at` over the already-columnar files"). PM-gated **PASS-WITH-CONSTRAINTS** (6 constraints; the binding two:
  **one shared `field_read_stmt` decode body** — `read_at` and the projection decoder both delegate to a new
  `generate_row_read_body`, so there is no second read path and `read_at` output stayed byte-identical; and **REST
  `?projection=<name>` is a generated closed set** — named declared projections only, no ad-hoc `?fields=`, so the
  wire never carries a runtime column list). Full stack: parser (`Projection` AST + directive arm), codegen
  validation (rejects relation/virtual fields at compile time), Rust structs+reads, REST (`?projection=` closed-set
  `match` on get/list; unknown → 400; absent → full), TS SDK (`<Model>Card` type + `get<Model>Card`/`list<Model>Card`,
  `tsc --strict` clean), and WASM (`read_surface` projected reads auto-mirrored into `ReplicaClient`). Strategy: the
  typestate builder + runtime-bitmask-as-Rust-API were **deferred** (identity-ranked #2/#3; declared projections are
  #1 — tightest types, linear in K, no 2^N). Guards `test_rust_generation_column_projection` +
  `test_rust_generation_projection_rejects_relation_field` + 3 parser tests. E2E `scratchpad/projection_compile`
  (native db+api compile + values across scalar/nullable/FK/timestamp + snapshot isolation + reopen + **live REST
  `tower::oneshot`** proving `?projection=card` omits unselected columns); `scratchpad/projection_wasm` compiles the
  projected replica to `wasm32` against `forgedb-storage-web`. **Honest limit:** the browser fault-in *skip* for a
  projected read is proven **compositionally** (guarded narrow decoder ∘ `storage-web` per-column `LazySource`, itself
  browser-proven for #110's `Tag.label`) + the wasm32 build — the full Playwright harness was NOT re-run for a
  projection-specific fault-in log. Also: REST `list` projection shrinks only the **wire** (filter/sort need full
  rows, so the server still reads them); the point-`get` and wasm paths get the full column-skip.
- **Point-in-time (snapshot) REST reads + Inspector scrubber (#85) — LANDED 2026-07-15 (design
  `docs/proposals/snapshot-reads-rest.md`).** Exposes the engine's watermark snapshot reads (#56-A) over the
  generated REST API so the Inspector's decorative "as of" affordances do real time-travel. **No substrate change,
  no publish gap** (`forgedb_storage::Snapshot` already published — the mechanism is "generate a few more per-model
  handlers over read methods the generated `Database` already has"). PM-gated **ALIGNED** (2 binding constraints:
  (1) the `as_of` branch swaps ONLY the row source and routes through the SAME generated filter/sort/paginate body —
  no parallel handler; (2) the wire token stays an opaque `usize` watermark, never a wall-clock instant → non-numeric
  → 400). Codegen (`crates/codegen/src/api.rs`): `GET /api/<model>?as_of=<w>` → `all_at(&Snapshot::new(w))`,
  `GET /api/<model>/{id}?as_of=<w>` → `get_at`, and a schema-wide `GET /snapshot` → `{ "watermarks": { "<Model>":
  <row_count> } }` (models-only; junctions deferred), the read-side peer of `/metrics` in `__ops_routes`
  (unauthenticated — a process serves one tenant). The get handler now ALWAYS owns its `Query(params)` extractor
  (`generate_projection_rest` no longer emits one — it returns `(get_block, list_block)`). Guard
  `test_api_generation_snapshot_reads`. E2E `scratchpad/snapshot_compile` (native db+api compile + **live
  `tower::oneshot`** proving `?as_of` reads a 2-of-3 prefix, `get_at` 404-at-old-watermark vs 200-live, `/snapshot`
  watermarks, non-numeric `as_of` → 400); all 18 `examples/` generate clean; integer-PK `iot-sensors` (u64)
  compile-checked. Inspector (`apps/inspector`): `live.ts` `asOf` on `listRows`/`getRow` + `getSnapshotToken`;
  `snapshotTokenAtom`/`pinnedSnapshotsAtom`/`pinSnapshotAtom`; `useLiveRows` passes the active model's watermark and
  suspends the live-query when pinned; top-bar "as of" dropdown (live + pinned) + Console snapshot tab with a
  discrete live/pinned selector + "Pin current…" (real `GET /snapshot`) + honest row-count-watermark readout
  (replaced the fake-clock slider). `tsc --noEmit` clean. **Honest limits:** the "as of" token is a **row-count
  watermark, not a wall-clock instant** (no wall-clock→watermark index — a separate, heavier feature); watermarks are
  valid only within a compaction epoch (an in-process `compact()` renumbers rows — the client must discard pinned
  tokens on a detected reopen); REST `list ?as_of` still reads full rows server-side (only the point-`get` is cheap);
  the Inspector data-path is **type-checked, not runtime-tested in the Tauri shell** (needs a running generated server
  + desktop build); the compare-vs-current diff is a labeled inspector-level marker, not yet a rendered diff.
- **MVCC Tiers 1–3 — transactions + concurrent writers — LANDED 2026-07-14 (#75/#84; merged to `main` 2026-07-15;
  design `docs/proposals/multi-writer-mvcc.md`, all three tiers).** The Direction-C rock: an atomic transaction
  boundary and multi-writer coordination, built as three strict-superset tiers over the existing append-only/watermark
  engine (no `xmin`/`xmax`, no on-disk format break). PM identity gates PASS / PASS-WITH-CONSTRAINTS throughout.
  - **Tier 1 — generated transactions (single writer, atomic commit/rollback).** `db.transaction(|tx| … )` stages
    appends and makes them visible atomically at commit (advance watermark + WAL fsync); rollback truncates every
    touched column back to its pre-txn length (`truncate_to_rows`) **and** rolls the per-model WAL tail back via a NEW
    additive `WalManager::truncate_to(offset)` (the design's "WAL fsync alone" was insufficient). Atomic **multi-model**
    commit rides a `_txn_journal.log` (class-1, reusing `forgedb-wal`'s opaque `Raw` framing). Guard
    `test_rust_generation_transaction`.
  - **Tier 2 — optimistic concurrent writers (serialized commit).** NEW substrate crate **`forgedb-txn`**
    (`CommitSequencer`): many txns *prepare* concurrently, a serialized commit point assigns a monotonic LSN and detects
    write-write conflicts over an in-memory `id → last-committer` map (rebuilt empty on open — never persisted, no
    format break); losers roll back (Tier-1 truncate) + retry. Guard `test_rust_generation_optimistic_commit`.
  - **Tier 3 — multi-process writers (single-machine).** NEW substrate crate **`forgedb-coordinator`** (control plane,
    **no `forgedb-storage*` dep** — T3-8): a `forgedb coordinate <root>` process holds the #89 `DirLock` on
    `<root>/.forgedb.lock` for all clients + serializes the commit turn + sequences the LSN. Coordinated clients open
    **LOCK-FREE** (`Database::connect(root, socket)` → connect-first, `CoordinatorUnavailable` → 503 if no coordinator;
    `_lock: None`) and are mutually exclusive with a standalone self-locking writer (T3-5). A coordinated writer is also
    a lightweight follower — peer read-currency via a NEW additive `sync_from_disk` on all three storage-native column
    types (re-derive each column's `row_count` from disk so a peer's appends become visible), driven by
    `__sync_columns_from_disk` + `__reindex_committed` on the coordinator log-tail signal. Guard
    `test_rust_generation_coordinated_client` (asserts lock-free open + coordinator has no `forgedb-storage` dep). The
    coordinator *is* the single writer, so data recovery reuses #89/#96 — no distributed 2-phase record.
  Verified: 486 workspace tests green; generated code compile-tested single- **and** multi-model; **genuine two-live-
  process concurrent-writer E2E** (`scratchpad/t3_e2e`, ephemeral) proving no `DirLock` panic, monotonic LSNs, and
  deterministic peer read-currency across two lock-free instances (this caught a real bug the agent's sequential-handoff
  E2E masked — peer refresh had synced only the tombstone count, leaving data-column rows out of bounds/invisible; fixed
  by syncing ALL columns). **Honest limits / deferred:** the ceiling is **one physical append point per column** →
  *concurrent prepare, serialized commit* (the serial section includes the writer's fsync); true parallel append needs
  segmented columns (separate, larger storage effort). Multi-**machine** (network + consensus) is a separate future
  product (v2/v3), not this tier. **PUBLISH GAP OPEN:** generated code links `forgedb-txn 0.1.0` + `forgedb-coordinator
  0.1.0` (scaffold pins both `= "0.1"`) and the additive `WalManager::truncate_to` + storage-native `sync_from_disk` —
  all four are additive (no format break) but must publish before an outside-repo `init → build` resolves from crates.io
  (mirrors the wal/storage/compaction publish sequence).
  **PUBLISH GAP CLOSED (2026-07-15):** published `forgedb-txn 0.1.0` + `forgedb-coordinator 0.1.0` (new) + the
  additive-method bumps `forgedb-wal 0.2.2` (`truncate_to`) + `forgedb-storage-native 0.1.1` (`sync_from_disk`) +
  `forgedb-storage-web 0.1.1` (a no-op `sync_from_disk` for wasm API parity — the shared `database.rs` calls it
  ungated and also compiles to wasm32; arena `len()` is always live so the browser follower needs no re-derive), and
  moved the scaffold pin `forgedb-storage = "0.1.5"` → **`"0.2"`** (0.1.5 = the pre-split monolith without
  `sync_from_disk`; the facade 0.2.0 re-exports storage-native 0.1.1). Reclose PROVEN by an outside-repo (`/tmp`)
  `init --template blog → generate rust+api → cargo build` whose generated `Cargo.lock` resolved
  `forgedb-coordinator 0.1.0` + `forgedb-txn 0.1.0` + `forgedb-storage 0.2.0` (→ `storage-native 0.1.1` +
  `storage-web 0.1.1`) + `forgedb-wal 0.2.2` (+ changefeed 0.2.0 / query-params / compaction / auth / types) all from
  `registry+…/crates.io-index` and compiled the generated MVCC code (0 errors). **WASM regression found + fixed
  (2026-07-15):** compiling the generated `database.rs` to `wasm32` (the browser read-replica shares it, #110)
  revealed MVCC had **broken the wasm replica build** — `database.rs` referenced `forgedb_txn` + `forgedb_coordinator`
  **unconditionally**, but neither was a replica dep and `forgedb-coordinator` (Unix sockets + `fs2`) can't compile to
  wasm at all (21 errors; native-only verification masked it, and the env's wasm toolchain was mis-PATHed — see below).
  Fix (surgical, identity-clean — a read replica is read-only): (1) cfg-gate the **entire Tier-3 coordinator surface**
  (`CoordinatedDatabase` + `impl` + `Database::connect` + `__peer_refresh`) to `#[cfg(not(target_arch = "wasm32"))]` —
  the only `forgedb_coordinator` users; (2) add `forgedb-txn = "0.1"` to the `WasmGenerator` replica scaffold
  (`forgedb-txn` is pure in-memory — zero deps, no fs/net — so it compiles to wasm; the Tier-2 `CommitSequencer` field
  stays but the read-only `Replica` transport never exposes the transaction methods). **NOW PROVEN:** `cargo build
  --target wasm32-unknown-unknown` AND a real `wasm-pack build --target web` both complete clean on the MVCC
  `database.rs`, and native db+api still build (the cfg attrs are no-ops on native — Tier-3 surface unchanged there).
  Guards: `test_rust_generation_coordinated_client` unchanged-green + insta snapshots re-accepted for the 3 cfg lines.
  (The earlier "env wasm32 std broken" was a mis-diagnosis: Homebrew's `rustc` shadowed rustup's on PATH and had no
  wasm std; `brew unlink rust` + `rustup default 1.96` fixed it — the wasm std was there all along under rustup.)
  Minor: `forgedb-coordinator` ships a benign `CONNECT_TIMEOUT` dead-code warning (a follow-up cleanup; published
  harmlessly).
- **`@pattern`/`@regex` validation ENFORCED (#104 RESOLVED — 2026-07-14).** The generated `validate_<model>` now
  compiles a per-(model, field) `LazyLock<regex::Regex>` from the directive's pattern and **rejects a non-matching
  string** → field-`Constraint` `ValidationError` (HTTP **422**), fired at the top of `insert`/`update` alongside the
  #91 `@min`/`@max`/`@length`/`@email`/`@url` checks. Nullable fields validate only when `Some`. The generated crate
  gains a plain `regex = "1"` dep (crates.io — **no substrate / publish gap**). Guard
  `test_rust_generation_pattern_validation`. This closes the last #91 "deferred" item (`@pattern`/`@regex` were the
  parsed-but-unenforced markers).
- **Change-feed / live-query typed event compare (#84 RESOLVED — 2026-07-15).** The generated per-model
  `<model>_event_matches` filter (used by the change-feed WS, live-query WS, **and** REST list) and the live-query
  `Updated` diff no longer compare via `serde_json` stringify — the source of float/bool encoding fragility (a stored
  `3.0` missed `?field=3`). Two pure-codegen changes, **no substrate / publish gap**: (1) the filter now parses each
  `?field=value` string into the field's **Rust type** and compares typed values — `parse::<f64>()` (so `3`≡`3.0`),
  `parse::<bool/i*/u*>()`, `parse::<Uuid>()`, `parse::<rust_decimal::Decimal>()` (value-equal, scale-invariant),
  `i64→Timestamp::from_seconds`, enums via the canonical `serde_json::from_value::<Enum>(String)` variant-name mapping,
  and `char(N)` by zero-padded byte buffer; an unparseable param matches nothing. The **filterable set is unchanged**
  (same `is_filterable_field` predicate — only the per-field body changed). (2) The live-query membership now stores the
  **typed record** (`HashMap<Id, Model>`, not a string hash) and detects `Updated` via a generated
  `<model>_record_changed(a, b)` that compares **every stored field** — `f64` by `to_bits()` (deterministic, NaN-stable),
  everything else by `==`, virtual relation collections excluded. Required deriving **`PartialEq` on generated structs**
  (caught by the compile matrix — a struct-typed field must compare). Guards `test_api_generation_typed_event_filter` +
  extended `test_api_generation_live_query`; compile-proven across the full type matrix (f64/enum/decimal/timestamp/
  char/json/struct/FK + nullable variants + a virtual one-to-many correctly excluded) in
  `scratchpad/typed_filter_compile` (ephemeral). **Honest limit:** the filter is still exact-match (`=`), not range/
  prefix — the coalesce/debounce scaling gap is the separate #83.
- **`json` scalar type — LANDED 2026-07-14.** New schema type: Rust `serde_json::Value` (stored as its serialized
  JSON bytes on the **variable-length string column** path), TS `unknown`, OpenAPI permissive. `json?` uses the same
  1-byte presence tag as `string?` (so `None` vs `Some(Value::Null)` round-trip distinctly). **No new dep**
  (serde_json already linked). **Honest limit:** NOT indexable / filterable / sortable (`^`/`&`/composite index, REST
  `?field=` filter/sort, `find_by_*` all rejected) — JSON has no total order the closed-set matcher can key on. Guard
  `test_rust_generation_json_type`.
- **`decimal` scalar type — LANDED 2026-07-14.** Exact fixed-point (`rust_decimal::Decimal`, plain dep, feature
  `serde-with-str`) for money/quantity where `f64` drifts. Rides the fixed **16-byte column** (uuid storage path);
  serializes to/from JSON as a **string** (precision-preserving; TS `string`, OpenAPI `{type:string}`). Because
  `Decimal` is `Ord`+`Hash` it **is filterable / sortable / indexable** (`^`/`&`/composite `@index` + `find_by_*`) —
  the index key is **scale-invariant** (`.normalize()`, so `1.0`/`1.00` share a bucket). `decimal?` rides the same
  nullable fixed-byte path as `timestamp?`/`u64?`. **Honest limit:** bare `decimal` only — `decimal(p, s)`
  precision/scale metadata is not yet parsed (deferred). Guard `test_rust_generation_decimal_type`.
- **`enum` user-declared type — LANDED 2026-07-14.** New top-level declaration `enum Name { A, B, C }` (a sibling of
  `struct`/model; name + variants PascalCase, unique, non-empty), referenced from a field by its **bare PascalCase
  name**. Stored as a fixed **1-byte `u8` discriminant** (variants → `0..N` in declaration order; **>256 variants is
  a codegen error**); nullable enum is a 2-byte `[present, disc]` column (`None` vs `Some(variant-0)` distinct).
  Serialized as the **variant-name string** (REST / TS / JSON all agree); TS = a closed string union; OpenAPI =
  `{type:string, enum:[...]}`. Filterable / sortable (by **declaration order**) / indexable (variant-name key); an
  invalid variant string on the REST boundary fails serde `Deserialize` → 4xx. A realistic `OrderStatus` enum was
  added to `examples/food-delivery/schema.forge`. Guard `test_rust_generation_enum_type`.
- **Delete semantics — `@on_delete` + M2M unlink — LANDED 2026-07-14.** `@on_delete(restrict|cascade|set_null)` on a
  relation FK field is now **ENFORCED** in the generated `Database::delete_<model>` wrapper (mirroring the #91
  create/update wrappers; the REST `DELETE /{id}` route goes through it):
  - **`restrict`** (the DEFAULT when `@on_delete` is absent): refuse to delete a parent still referenced by a live
    child → new `ValidationError::ReferencedByChildren` → **409** (detected via the O(1) reverse FK index).
  - **`cascade`**: recursively delete every referencing child (each child's own rules fire; a pathological FK cycle is
    bounded by `MAX_CASCADE_DEPTH = 64`).
  - **`set_null`**: null each referencing child's **optional** FK — a **codegen HARD-ERROR** if applied to a required
    `*Target` (use `?Target`, or `cascade`/`restrict`).
  **M2M unlink also LANDED**: junctions gained a `Tombstones` column (using the **already-published** `Tombstones`
  type on both `storage-native` + `storage-web` → **no publish gap**), plus `unlink_<a>_<b>` / `unlink_all_*` and a
  **latest-wins** `pairs()` so a re-link restores a previously-unlinked pair; a cascade delete unlinks its junctions.
  Guards `test_rust_generation_delete_*` (5). **Honest limits / deferred:** the junction has no WAL (a torn unlink
  could duplicate a pair, hidden by the latest-wins traversal); unlink is a linear junction scan. This replaces the
  old "`@on_delete` will not parse" claim and the "no M2M `unlink`" limit.
- **Offline `compact`/`vacuum` DEPRECATED (#105 RESOLVED — 2026-07-14).** The resurrection-prone offline tombstone
  path is removed: `forgedb compact run` / `compact vacuum` now **mutate nothing** and print deprecation guidance to
  stderr + **exit non-zero (code 6)**, pointing to the supported in-process keep-set-based `Database::compact()` (#92)
  — the ONLY supported compaction path. The substrate `compact_model` fn is left in place **doc-deprecated** (removing
  it is breaking → next major; **no compaction publish gap**). Was previously only MITIGATED behind `--force`; the
  `--force` flag / unsafe branch is dropped. Test-count-neutral (the `--force` guard was rewritten in place as
  `test_offline_compact_is_deprecated_and_mutates_nothing`).
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
- **`query-optimization` join predicate pushdown (#48) — REMOVED by the legacy audit (#94).** The
  whole `query-optimization` crate (a speculative predicate-pushdown planner IR with zero consumers —
  never wired into codegen or generated code) was pruned as dead infra. Git history is the audit
  trail; re-add with a real consumer when a query planner is actually designed.
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
- Don't `git commit` without the user's consent (an in-the-moment "commit when done"
  counts as consent for that scope; it doesn't carry to follow-up changes). When you do
  commit, split into small focused, conventional commits and include related lockfiles —
  and ALWAYS `git push` after a batch of commits (never leave the local branch ahead of
  origin). See the user's global git rules for the authoritative wording.
- When closing a TODO item, delete it (git history is the audit trail).
- All workflows runnable from the repo root — no `cd` into subdirs.

## Subagents

- `forgedb-product-manager` — product/architecture decisions; guards the generator identity.
- `rust-core-library` — idiomatic Rust for core library/crate work.
