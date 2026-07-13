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

**Baseline: 414 tests pass** (workspace, incl. doctests). 400→414 with #82 realtime Direction C (durable
replication broker): +9 `forgedb-changefeed` `durable` unit tests (offsets / opaque round-trip / reopen /
torn-tail / prune / catch-up / wire codec) and +2 codegen guards (`test_rust_generation_replication_broker`
+ `test_api_generation_replication_endpoint`). 399→400 with v1 Phase 5 WS1 observability (1 codegen
guard `test_api_generation_observability_endpoints`; the TS SDK rewrite (WS5) extended `test_typescript_generation_snapshot`
in place). 398→399 with #105 offline-compact `--force` guard (1 test `test_offline_compact_refuses_without_force`).
395→398 with v1 Phase 4 (#92): +2 codegen guards
(`test_rust_generation_auto_compaction` W1 + `test_rust_generation_additive_backfill` W2) and +1 integration test
(`test_migrate_auto_diff_additive_and_breaking_gate` W3). 394→395 with v1 Phase 3 (#91) data integrity (1 codegen
guard `test_rust_generation_data_integrity`). 393→394 with the #100–#103 index follow-ups (1 codegen
guard `test_rust_generation_index_followups`). 391→393 with #90 Phase 2 (2 codegen guards:
`test_rust_generation_secondary_indexes` + `test_api_generation_list_endpoint`). 461→391 with the legacy-audit prunes
(#94): −70 from deleting the two dead crates `query-optimization` (32 unit + doctests, incl. the
former #48 join-predicate pushdown) and `http-server` (30 unit + doctests) plus the dead
`storage::Database` wrapper's 1 test — all zero-consumer dead code, no production regression. Prior
history (pre-prune, at 461): 434→447 with #48 join predicate pushdown + #49 restored OpenAPI
generation (4 codegen tests; #49 OpenAPI stays, in codegen); 447→460 with #89
durable write path + #95 wal prune (net +13: new `forgedb-wal` `Raw`-path + `forgedb-storage`
`truncate_to_rows`/`DirLock` tests + 1 `test_rust_generation_durable_write_path` codegen guard, minus
the deleted structured/transaction wal tests); 460→461 with #96 WAL checkpoint (1
`test_rust_generation_wal_checkpoint` codegen guard). Earlier: 419→432 with #59 multi-tenancy (11
`forgedb-auth` verify tests + 2 codegen guards); 432→434 with #69 generated REST update/delete
(1 codegen guard) + #71 inspector db-name (1 src-tauri test). Dropped from 531
when the orphaned `fulltext` + `crud-api` crates were removed in Phase 3b. Ignore older claims of
"531"/"521"/"466"/"447"/"434"/"432"/"419"/"417"/"411"/"409"/"403"/"399"/"398"/"395"/"394"/"393"/"380".

## Workspace layout

Root crate `forgedb` (`src/`) is the CLI: `src/main.rs` (clap), `src/commands/*`
(one module per subcommand), `src/{templates,ui,error}.rs`. It orchestrates the crates
in `crates/`:

**Published to crates.io (independent version lines, do NOT normalize):**
- `types` — core type system (uuid, timestamp, primitives) — **0.2.0**
- `storage` — columnar storage engine (positional-I/O fixed columns + append-only variable) — **0.1.5
  (published 2026-07-10; 0.1.4 published 2026-07-08)** (0.1.5 adds `truncate_to_rows` on the three column types +
  `DirLock` single-writer advisory lock, both for #89 durable writes; 0.1.4 adds read-only column reader handles
  `FixedColumnReader`/`VariableColumnReader`/`TombstonesReader` + `*::reader()` for #56-B single-writer/
  many-reader; 0.1.3 added `Manifest` layout fields + `Manifest::save_to/load_from` + `Snapshot` for #57
  backup / #56-A snapshot reads). **In-tree since 0.1.5 published:** the legacy audit (#94/#99) removed the
  dead `Database` directory-manager wrapper (`open_with_wal`/`wal_mut`/`has_wal`/`save_manifest`/… — zero
  production consumers; generated code drives `FixedColumn`/`VariableColumn`/`Tombstones`/`Manifest`/`DirLock`
  directly). Published 0.1.5 still contains it harmlessly; removing it is breaking, so the **next** storage
  publish bumps accordingly (do not republish 0.1.5).
- `changefeed` — field-blind change-feed broadcast + **durable replication broker** substrate
  (#62-A / #82) — **0.2.0 (published 2026-07-13)**. 0.2.0 adds the
  `durable` module for #82 realtime Direction C: a `DurableBroker` that records each change to a
  CRC-framed, append-only log at a **monotonic global offset** (the opaque cross-model ordering
  token) + a resumable subscription (`read_from`/`subscribe`/`catch_up_from`, idempotent by absolute
  offset) — the substrate the WASM read-replica follower (#110) resumes from. Field-blind:
  `PersistedEvent { offset, model, row_index, kind, bytes }` carries the model name as an opaque tag
  and the committed row bytes verbatim (same class as the in-process feed). The generated `/replicate`
  WS endpoint streams its `to_wire()` binary frames. (0.1.1 added `ChangeKind::Updated`/`Deleted` for
  #66 — published 2026-07-08; 0.1.0 published 2026-07-07.)
- `auth` — verify-only JWT + tenant cross-check substrate (#59) — **0.1.0 (published 2026-07-09)**.
  Schema-agnostic axum extractor/middleware: verifies an asymmetric JWT (JWKS or static PEM,
  algorithm-pinned, `exp`/`nbf`/`iss`/`aud`+skew), extracts a configured tenant claim, cross-checks it
  against the process's tenant → 403, injects an opaque `Principal`. Knows nothing of
  models/rows/schema — same class as `changefeed`.
- `wal` — write-ahead log — **0.2.0 (published 2026-07-10; 0.1.1 published)**. The generated durable write path (#89)
  links only the **opaque `Raw`** record path (schema-agnostic bytes + CRC framing + fsync policy + torn-tail
  `read_all` + `replay`); the pre-existing structured/field-decoding API (`WalValue`, `WalOperation::{Insert,
  Update,Delete}`, `Transaction`/`replay_committed`) was **pruned as a drift vector** (#95, legacy-audit epic
  #94) — 0.2.0 is that breaking removal.
- `query-params` — REST query-string parser (#90) — **0.1.0 (published 2026-07-10)**. Schema-agnostic:
  parses a URL query string into generic `Filter`/`Sort`/`Pagination` (limit clamped to `MAX_LIMIT`); the generated
  `api.rs` list endpoint links it for filter/sort/paginate. Interprets no schema — all field-aware filtering/sorting
  is generated per-model — so it is class-1 substrate, same class as `changefeed`/`auth`.
- `compaction` — in-process dead-row reclaim (#92 Phase 4 W1) — **0.1.0 (published 2026-07-11)**.
  Schema-agnostic byte GC keyed by model *directory name*: `Compactor::compact_model_keeping(model, live_rows)` keeps
  exactly the caller-supplied opaque row indices (the generated code computes the live set); `compact_model` is the
  legacy tombstone path (CLI-only, resurrection-prone against #66 — now **guarded**: `forgedb compact`/`vacuum`
  refuse without `--force`, #105). Deps are serde/chrono/thiserror/log only —
  reads no `.forge`. Generated `Database`/`Storage::compact()` link **only** `compact_model_keeping` (never
  `BackgroundCompactor`). Scaffold pins `forgedb-compaction = "0.1"`; **reclose PROVEN** by an outside-repo
  `init → generate rust+api → cargo build` resolving `forgedb-compaction 0.1.0` (+ the rest) from crates.io. Was
  internal; promoted to published substrate by #92 W1.

**Internal (0.1.0):** (compiler internals — `parser`, `codegen`, `validation`, `migrations`, `backup`, `watcher`
are now **published to crates.io** 0.1.0 as of Phase 5 WS4, but **only** so `cargo install forgedb` can build the CLI
from the registry; per `docs/SEMVER.md` they are explicitly NOT a stable public API, unlike the substrate crates.
`lsp-server` + `ffi` remain unpublished.)
- `parser` — lexer + parser → AST (`crates/parser/src/ast.rs`)
- `codegen` — code generators; exports `RustGenerator`, `TypeScriptGenerator`,
  `ApiGenerator`, `StubGenerator` (each `::generate(&schema) -> GeneratedCode`)
- `validation`, `migrations`, `backup`, `changefeed`,
  `watcher`, `lsp-server`, `ffi`  (`query-params` + `compaction` are now **published** — see the
  published-crates list above)
  (`fulltext` + `crud-api` were removed in Phase 3b; `query-optimization` + `http-server` were
  removed by the legacy audit (#94) as zero-consumer dead code — the API existence/404 logic lives
  in the generated handlers, and the generated `api.rs` builds its own router.)
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
      `@min`/`@max` (numeric), `@length` (string, `@length(max)` or `@length(min,max)`), `@email`, `@url`.
      Nullable fields validate only when `Some`. Called at the TOP of `insert`/`update`, before any durable side
      effect. **Deferred:** `@pattern`/`@regex` (need a `regex` dep in the generated crate — #104).
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
    - **Honest limits / deferred:** `@pattern`/`@regex` (#104); `db.<model>.insert` (direct storage path) enforces
      field + unique but NOT FK (use `db.create_<model>` for full integrity — documented on the method); no
      cross-field / conditional constraints; validation is fail-fast (first violation returned, not a list).
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
    reclose is proven (mirrors wal/storage/query-params). Manual `forgedb compact`/`vacuum` CLI is the legacy
    tombstone path (resurrection-prone against #66) and is now **guarded — it refuses without `--force`** and points
    to the safe in-process path (#105 mitigated; full schema-aware offline compaction deferred). Threshold not yet
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
    Integration test `test_migrate_auto_diff_additive_and_breaking_gate`.
  - **Breaking-change reload path documented + tested (v1 Phase 4 W4 — #92 LANDED).** `docs/MIGRATIONS.md` documents
    the v1 answer for breaking changes: dump (`all()` → JSON via the generated `Serialize`), regenerate, reload into a
    fresh dir through `Database::create_<model>` with an app-level transform. Proven E2E (`scratchpad/reload_compile`:
    a `u32 → string` type change round-trips, ids preserved). **The data-transform migration engine stays out of v1.**

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
- Never `git commit`/`push` unless explicitly asked; then split into small focused,
  conventional commits and include related lockfiles.
- When closing a TODO item, delete it (git history is the audit trail).
- All workflows runnable from the repo root — no `cd` into subdirs.

## Subagents

- `forgedb-product-manager` — product/architecture decisions; guards the generator identity.
- `rust-core-library` — idiomatic Rust for core library/crate work.
