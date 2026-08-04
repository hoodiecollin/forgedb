# ForgeDB Architecture

**Audience:** contributors and anyone reasoning about how ForgeDB is put together.

This is the system narrative. The authoritative crate inventory is the "Workspace layout"
section of [`CLAUDE.md`](../CLAUDE.md); the substrate catalog is [PUBLIC_CRATES.md](./PUBLIC_CRATES.md);
the stability policy is [SEMVER.md](./SEMVER.md).

---

## What ForgeDB is

ForgeDB is an **application-database generator** — a compile-time code-generation tool, **not**
a runtime ORM or query engine. A declarative `.forge` schema is transpiled into tailored Rust
database code plus a TypeScript SDK, a REST API, and an OpenAPI spec. End users need only
their schema, the `forgedb` CLI, and config.

**The invariant.** The app's schema is a *compile-time input to generation*, never a *runtime
input to a generic engine*. The schema-specific surface — types, tables, queries, filters,
relations, API routes — is generated and tailored per app. ForgeDB never ships a general-purpose
library that reconstructs that surface at runtime by reflecting over a schema.

Generated code is not dependency-free — it links the schema-agnostic **substrate** crates
(storage, wal, types, …; see [PUBLIC_CRATES.md](./PUBLIC_CRATES.md)) — but it never depends on a
ForgeDB ORM or a runtime that reads the user's schema. A generated, schema-tailored
query/filter builder is fine (it is just generated code); a generic, schema-agnostic query
builder is not.

---

## Generation pipeline

```
schema.forge
   │
   ▼
forgedb-parser        lexer → tokens → AST (crates/parser/src/ast.rs)
   │
   ▼
forgedb-validation    semantic checks (types, relations, directives)
   │
   ▼
forgedb-codegen       one generator per artifact:
   ├─ RustGenerator        → database.rs   (storage, CRUD, indexes, relations, txns)
   ├─ TypeScriptGenerator  → types.ts      (typed SDK client)
   ├─ ApiGenerator         → api.rs        (axum REST + WS routes)
   ├─ StubGenerator        → placeholder stubs README  (no UI/component codegen today)
   ├─ OpenApiGenerator     → openapi.json  (offline OpenAPI 3.1 document)
   ├─ WasmGenerator        → replica/*     (browser read-replica; opt-in)
   └─ TransformGenerator   → migrations/transform/*  (offline data migration bin)
```

Rust output is built with `quote!` + `prettyplease` and snapshot-tested with `insta`
(`crates/codegen/tests/`). **Snapshot pass ≠ output compiles** — codegen changes are also
compile-tested by generating for a real multi-model schema and `cargo check`ing the emitted
crate. That discipline is load-bearing (it has caught real codegen bugs; see `CLAUDE.md`).

The CLI (root crate `forgedb`, `src/`) orchestrates the pipeline: `src/main.rs` (clap),
`src/commands/*` (one module per subcommand). Commands: `init`, `generate`, `validate`, `build`,
`dev`, `migrate`, `compact`, `backup`, `tenant`, `coordinate`.

---

## Crate topology

The workspace splits cleanly into three tiers.

### Substrate (generated code links these; published, a stability surface)

`forgedb-types`, `forgedb-storage` (facade) + `forgedb-storage-native` + `forgedb-storage-web`,
`forgedb-wal`, `forgedb-changefeed`, `forgedb-auth`, `forgedb-query-params`,
`forgedb-compaction`, `forgedb-txn`, `forgedb-coordinator`. Cataloged in
[PUBLIC_CRATES.md](./PUBLIC_CRATES.md).

### Compiler internals (the CLI's implementation; published for install only, NOT a stable API)

`forgedb-parser`, `forgedb-codegen`, `forgedb-validation`, `forgedb-migrations`,
`forgedb-backup`, `forgedb-watcher`, `forgedb-lsp-server`. Published to crates.io only so
`cargo install forgedb` resolves; see [SEMVER.md §4](./SEMVER.md) and [`CLAUDE.md`](../CLAUDE.md)
(the authoritative workspace inventory). (`forgedb-lsp-server` joined this list in epic #173
— the `forgedb` crate optionally depends on it for the bundled `forgedb-lsp` binary.)

### Dependency direction

Compiler internals may depend on substrate (for codegen); **substrate never depends on compiler
internals, and generated code never depends on compiler internals.** This is what keeps the
generator identity honest: nothing at runtime reads a schema.

---

## Storage model

The engine is **append-only columnar** storage over positional files (not row-based). Writes are
always positional `pwrite`-style appends; *bulk reads* may map a bounded span of a column when the
span is large enough to be worth it, which is an optimization inside the read path and not a change
to the layout or the write path. Each model and each many-to-many junction is a directory:

```
<data-root>/
  <model>/
    manifest.json            # physical layout: columns, value sizes, kinds, row anchor,
                             # format_version, compaction_epoch
    tombstones.bin           # 1 byte per row (liveness / delete marker)
    fixed/
      uuid_0.bin             # fixed-width columns (uuid=16B, u64/i64/f64=8B, bool=1B, …)
      u64_1.bin
    string_data_0.bin        # variable-length column payloads
    string_offsets_0.bin     # + offsets, one pair per variable column
```

Key properties that the rest of the system is built on:

- **Append-only.** A write appends; it never mutates committed bytes. Updates and deletes are
  *superseding-version appends* (a new row version; delete appends a tombstoned version).
  Latest-version-per-id resolution is generated per model.
- **Self-describing length.** Every column's committed byte length is a pure function of the
  row count + layout, so a reader derives the durable prefix from file lengths — no persisted
  checkpoint marker is load-bearing.
- **Watermark snapshots.** A snapshot is just a row-count watermark (`forgedb_storage::Snapshot`);
  point-in-time reads resolve the newest version *within* the watermark. No `xmin`/`xmax`, no
  version chains.
- **Durability.** Generated writes journal an opaque row blob to a per-model WAL (`forgedb-wal`
  `Raw` op) + fsync *before* touching columns; recovery truncates a torn column tail and replays
  the WAL tail by absolute row index. A `DirLock` refuses a second writer.

The on-disk layout is part of the substrate ABI: a change a prior binary cannot read bumps the
owning crate's major and requires a migration path (see [SEMVER.md §2](./SEMVER.md) and the
version interlock in [MIGRATIONS.md](./MIGRATIONS.md)).

---

## Request path (generated server)

The generated `api.rs` builds its own axum router — there is no shipped generic HTTP server.

```
HTTP request
   │
   ▼
axum router (generated in api.rs)
   ├─ __ops_routes()   /health /ready /metrics /snapshot   (unauthenticated)
   └─ tenant guard ──► __data_routes()
        │                 REST CRUD + list (?filter/sort/paginate)
        │                 WS /subscribe /live-query /replicate
        ▼
   forgedb-auth (verify JWT + tenant cross-check, when configured)
        │
        ▼
   generated per-model handlers
        ├─ forgedb-query-params  (parse the query string → generic Filter/Sort/Pagination)
        ├─ generated closed-set matcher / comparator (all field-aware logic)
        └─ generated Database (read/write path over forgedb-storage + forgedb-wal)
```

Every field-aware step — filtering, sorting, the event matcher, index probes — is *generated
per model*. The substrate crates on this path (`auth`, `query-params`) interpret no schema.

### The list path is a scan *scope*, and never materializes a row

A list request does **not** decode every column of every row. Codegen emits a *narrow scan view*
per model — `<Model>ScanRef<'a>`, the identity field plus the filterable/sortable columns, with
`string` as `&'a str` — and each scan column is bulk-loaded once (one `gather_buffered` per
column, hoisted out of the row loop) rather than read per row.

The scan is a **scope**, not a producer:

```rust
pub fn __with_scan<R>(
    &self,
    sel: Option<Vec<usize>>,                       // index-pushdown rows, or every live row
    keep: impl Fn(&<Model>ScanRef<'_>) -> bool,    // runs during decode
    f:    impl FnOnce(&mut Vec<<Model>ScanRef<'_>>) -> R,
) -> R
```

The handler filters, sorts, counts and paginates *inside* `f`, and returns `(total, Vec<Id>)`; the
page is then re-read through `get`, so only `limit` rows ever pay a full decode. Nothing borrowed
crosses the boundary — the view's lifetime is higher-ranked, so `R` cannot name it.

That shape is what removes the copies rather than narrowing who pays them. `keep` running during
decode means a **rejected** row never allocates a string. Keeping the sort and the page inside the
scope means a **surviving** one does not either — and on an unfiltered `GET /model?limit=50` every
row is a survivor, which is exactly the case a filter-only optimization wins nothing on. The
strings a scan row used to allocate were read for three things (the sort comparator, `.len()`,
`.id`) and dropped; now the comparator reads the buffer's bytes in place.

The constraint this commits to: **only scalars leave a scan.** A future list feature that wants
more than ids out of one has to come inside the callback.

Three properties keep it safe and non-viral:

- The buffered columns live in a local holder inside the generated scan, so a borrowed view cannot
  escape it. `ScanRef` is internal — no wire derives, never reachable from REST/TS/OpenAPI, and
  only ever named behind a `&` in a closure argument. **No lifetime appears in any user-facing
  generated signature.**
- The scan filter is emitted from the *same* per-field checks the change-feed matcher uses, so
  there is one predicate source and two operand views — never a second parser.
- The index-pushdown arm (`__rows_by_<field>`, O(matches) via the secondary index) resolves
  candidate *rows* and feeds them to the same scope, so there is one scan body and one decode
  path. Pushing that arm through `gather_buffered` needed a matching substrate change: bounding a
  bulk read to the selection's row span is right for a dense scan and wrong for a handful of
  scattered candidates, so `VariableColumn::gather_buffered` gained a packed sparse path
  (`SPARSE_OFFSETS_SPAN_FACTOR`) below which offsets and bytes are read per row.

The same scope backs the live-query re-run, which re-evaluates the closed-set query on every
change to the model.

---

## Concurrency & realtime (layered)

Each capability is a strict superset built over the append-only/watermark core, with no on-disk
format break:

- **Snapshot reads** (watermark) → **single-writer + concurrent readers** (read-only column
  reader handles) → **transactions** (Tier 1, atomic commit/rollback) → **optimistic concurrent
  writers** (Tier 2, `forgedb-txn` commit sequencer) → **multi-process writers** (Tier 3,
  `forgedb-coordinator` holds the `DirLock` and serializes the commit turn).
- **Change feed** (in-process, field-blind broadcast) → **live queries** (stateful,
  removal-aware result sets) → **durable replication broker** (`forgedb-changefeed::durable`,
  resumable by global offset) → **browser read-replica** (the same generated `database.rs`
  compiled to wasm32 against `forgedb-storage-web`, catching up from `/replicate`).

The ceiling is one physical append point per column — *concurrent prepare, serialized commit*.
Multi-machine replication/consensus is a separate future product, not these tiers.

**Control plane vs data plane (multi-process writers).** Tier 3 splits cleanly: the
`forgedb-coordinator` process is a pure **control plane** — it holds the `DirLock`, serializes
the commit turn, and sequences the LSN, but it has **no `forgedb-storage` dependency** and never
decodes a row byte. The schema-aware column write stays in generated **data-plane** code, run
lock-free by each coordinated client under a granted turn (clients open with `_lock: None`,
mutually exclusive with a standalone self-locking writer). This is what keeps the identity honest
at Tier 3: the coordinated writer is still the *same generated code*, and the coordinator — like
every substrate crate — knows nothing about any schema. It is the symmetric inverse of the durable
replication broker (control over the write turn, vs. an ordered feed of committed changes).

---

## Design decisions (and their trade-offs)

- **Compile-time generation over a runtime engine.** Type safety and monomorphized, per-schema
  code; the cost is that schema changes require regeneration + recompilation (handled by the
  migration workflow, [MIGRATIONS.md](./MIGRATIONS.md)).
- **Append-only + superseding versions over in-place mutation.** Keeps snapshots, backup, and
  the change feed simple and correct; the cost is that storage grows with dead versions until
  in-process compaction reclaims them (`forgedb-compaction`).
- **Watermark snapshots over MVCC version chains.** No per-row version metadata; the cost is
  that a compaction renumbers rows within an epoch, so pinned watermarks are epoch-scoped.
- **Storage facade over a target branch in codegen.** The generated `database.rs` compiles to
  both native and wasm32 with zero codegen branches; the facade absorbs the difference.
- **Substrate / compiler-internals split.** Generated code links only schema-agnostic crates;
  the compiler crates stay off the runtime path, which is what makes the generator identity
  verifiable.

---

## References

- [`CLAUDE.md`](../CLAUDE.md) — authoritative workspace inventory + feature status
- [PUBLIC_CRATES.md](./PUBLIC_CRATES.md) — substrate crate catalog
- [SEMVER.md](./SEMVER.md) — stability policy
- [MIGRATIONS.md](./MIGRATIONS.md) — schema evolution + the version interlock
- [V1_ROADMAP.md](./V1_ROADMAP.md) — scope and honest current state
