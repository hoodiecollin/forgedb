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
                             # format_version, engine_version, compaction_epoch
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
- **A foreign key is not its own type.** An FK column is physically identical to the column the
  *target model's identity field* occupies — same width, same accessor, same manifest entry. The
  generator resolves `*Target` / `?Target` to that key type once, at the boundary, so no layout
  rule and no relation capability is conditioned on the key being a uuid. A many-to-many junction
  is the same idea applied twice: one fixed column per endpoint, each that endpoint's own width.
- **A key is `Copy`, including a string one.** Every identity type materializes as a fixed-size,
  hashable, totally-ordered Rust value, because a key sits in the row index, in a junction
  `HashMap`, and in a fixed-width replication frame. That is why a `string(N)` identity is a
  `forgedb_types::InlineStr<N>` — a fixed-capacity `Copy` string — rather than the heap `String`
  the same declared type produces in an ordinary column (#252). One consequence is worth naming:
  the resolution above means every inline-string *layout* rule has to run on the **resolved** type,
  or an FK to a string-keyed model silently misses the packing path it physically needs. The wire
  form is unchanged — `InlineStr` serializes as a plain JSON string, by hand rather than by derive,
  since serde's array derive stops at 32 elements and a key may be wider.
- **Which field is the identity, and which types may be one, are each decided in exactly one
  place.** `Model::identity_field` (`crates/parser/src/ast.rs`) picks the field — a field named
  `id`, else the first `+` field, in that order — and `FieldType::is_identity_key` names the
  admitted set (`uuid`, the four integers, `timestamp`, `string(N)`), with a required FK admitted
  by resolution rather than by type. Both used to be open-coded: the picker in 31 places across 8
  files, and the key-type test twice (once as the many-to-many endpoint rule). Neither duplication
  was a style problem. A *single-pass* picker — `find(|f| f.name == "id" || f.auto_generate)` —
  keys `Event { seq: +u64, id: u32 }` on `seq` while every generated signature still says `id`,
  which compiles, runs, round-trips, and diffs clean in a snapshot; only reading a row back by the
  key the author meant can see it. And two independent key-type tests produce two diagnostics for
  one mistake, then drift. So the endpoint test now *delegates* to the identity test rather than
  restating it, and a grep-based guard (`tests/identity_predicate_test.rs`) fails the build if
  either predicate is open-coded again (#251).
- **Watermark snapshots.** A snapshot is just a row-count watermark (`forgedb_storage::Snapshot`);
  point-in-time reads resolve the newest version *within* the watermark. No `xmin`/`xmax`, no
  version chains.
- **Durability.** Generated writes journal an opaque row blob to a per-model WAL (`forgedb-wal`
  `Raw` op) + fsync *before* touching columns; recovery truncates a torn column tail and replays
  the WAL tail by absolute row index. A `DirLock` refuses a second writer.

- **Two orthogonal version counters, and they are not the same axis** (#254). A manifest carries
  both, and confusing them silently skips migrations:

  | Manifest field | Owned by | Counts | Migrated by |
  |---|---|---|---|
  | `schema_version` (on-disk key `format_version`) | the **app's** `migrations/` lineage | applied schema migrations | `forgedb migrate up` |
  | `engine_version` | **ForgeDB's** release line | the engine's byte-format generation | `forgedb migrate engine` |

  A manifest with no `engine_version` baselines to generation 1, so the counter is additive rather
  than a second format break. Generation 2 is #254: timestamp columns hold **microseconds**, where
  generation 1 held seconds.

  **The engine hop is a generated bin, not a schema-blind column pass.** Only a *bare*
  `timestamp` field becomes `ColumnType::Timestamp`; every shape that merely *contains* one —
  `timestamp?`, `[timestamp; N]`, a struct field — is written as an opaque `FixedBytes` transmute
  of the Rust value, which `repr(Rust)` gives no decodable layout for. 81 of the 247 timestamp
  fields in the example corpus are nullable, so a schema-blind pass would leave a third of them in
  the old unit while the regenerated code read the new one. Which leaves are timestamps, and where
  they sit inside an `Option` / array / struct, is *schema* knowledge — so it belongs in generated
  code. `forgedb migrate engine` emits a crate embedding **two** generated modules of the same
  schema, differing only in the baked `EXPECTED_ENGINE_VERSION`; the reader half opens the stale
  dir legally, the writer half stamps the new generation, and the existing open-guard interlock
  does the enforcement for free.

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
column, hoisted out of the row loop) rather than read per row. The identity is the one
string-typed field that does *not* borrow: the scope returns a vector of ids that outlives the
buffers, and a `Copy` key costs the scan nothing to hold by value.

The scan is a **scope**, not a producer:

```rust
pub fn __with_scan<R>(
    &self,
    sel: Option<Vec<usize>>,                       // index-pushdown rows, or every live row
    keep: impl Fn(&<Model>ScanRef<'_>) -> bool,    // runs during decode
    f:    impl FnOnce(&mut Vec<<Model>ScanRef<'_>>) -> R,
) -> R
```

The handler filters, sorts, counts and paginates *inside* `f`, and returns `(total, Vec<Id>)`.
Nothing borrowed crosses the boundary — the view's lifetime is higher-ranked, so `R` cannot name it.

**The page is serialized inside the scope too, and does not go back through `get`.** Returning ids
and re-reading them was still a full decode per returned row, so the default list arm is a second
scope that keeps the page borrowed as well:

```rust
pub fn __with_page<R>(
    &self,
    sel:    Option<Vec<usize>>,
    keep:   impl Fn(&<Model>ScanRef<'_>) -> bool,
    sort:   impl FnOnce(&mut Vec<<Model>ScanRef<'_>>),
    offset: usize,
    limit:  usize,
    f:      impl FnOnce(usize, &[<Model>PageRef<'_>]) -> R,   // (total, the page)
) -> R
```

`<Model>PageRef` is the *wide* borrowed view — every stored column, still pointing into the
buffers, with one-to-many relations left as unit placeholders exactly as they are on the record — so
the response serializes straight out of them. The `__with_scan` + `get(id)` shape above is retained
only where the page genuinely needs owned rows: `@projection` models and the live-query re-run.

**The "is there any filter at all?" question is answered once per request, not once per row.** The
generated matcher short-circuits only on an *empty* query map, and `?limit=50` — the default page
size a client is told to send — makes the map non-empty without naming a single filterable field.
So an unfiltered list request used to run one hash lookup per filterable field per scanned row, all
of them guaranteed to miss: 502 µs on a 10,000-row table, 59% of the request, scaling with the
*table* rather than the page. Codegen now emits `__<model>_is_unfiltered` from the **same** field
iteration that builds the per-field checks, and the handler evaluates it once before the scan:

```rust
let __keep_all: bool = __post_is_unfiltered(&params);
… __with_page(__sel, |r| __keep_all || __post_scan_matches(r, &params), …)
```

Deriving the predicate from that same iteration is what makes it impossible for the two to disagree
about which names are filterable — and it is why the predicate is *positive* ("does any key name a
filterable field of this model?") rather than a maintained list of reserved query keys. A model may
legally declare a field named `limit`; for that model `?limit=3` genuinely is a filter, and an
exclusion list would silently return unfiltered rows.

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

**The two deadlines are coupled, and a failed request fails closed.** A coordinated client blocks
waiting for its turn while the coordinator blocks waiting for the pending turn to clear, so both
sides hold a deadline — and only one of them can see both. The **client declares its own I/O
deadline** on every `RequestTurn` (`client_deadline_ms`), and the **coordinator clamps its grant
wait** to `min(turn_timeout, declared − 500ms)`, so a `Busy` reply always reaches the client before
it stops reading. Without that coupling, raising `--turn-timeout` past the client's deadline made
the client give up first and left the connection **desynchronized**: the coordinator's eventual
`Grant` stayed on the socket, to be read as the answer to the client's *next* request — a turn it
did not hold. A client that declares nothing is a pre-coupling build and is assumed to hold the
legacy 35s, so old clients are fixed without being recompiled. This is an additive wire *field*
rather than a connect handshake because the protocol is internally-tagged JSON with no version
field: an unknown field is ignored in both directions, while an unknown *variant* breaks whichever
peer ships second.

Independently, any failed request **poisons** the client connection — it refuses further requests
until `reconnect()` replaces the stream — because a timeout leaves a reply in flight no matter why
it happened. Poisoning is deliberately not paired with automatic retry inside the substrate:
recovery policy lives in the generated commit loop, beside the `Busy` budget and retry limit
already there, and the generated code calls `reconnect()` in both coordinator error arms so the
failure stays loud for the current transaction and invisible to the next one.

### Integer auto-increment allocates per process, and is made conflict-*visible*

`+u32`/`+u64` fields allocate from an in-memory counter held per field, per process — there is
no shared allocator and no coordinator-side sequence. A counter is seeded at open to
`max(persisted floor, scanned max)`:

- **The scan is ungated by tombstones** and walks every *physical* row, including superseded
  versions. A deleted row still spent its number: rehydrating from live rows alone would hand a
  retired value to a different row, and that value is visible in the replication log, in backups,
  and in any URL that still holds it. (The secondary-index rebuild beside it *is* tombstone-gated
  — the max must not come from it.)
- **The persisted floor** is `Manifest.auto_sequences`, an opaque `field name -> highest value
  issued` map. It exists because compaction physically drops the rows the scan reads, so after a
  compaction the scan alone would regress. The floor only ever moves up; gaps are allowed.
  Generated `compact()` **writes the floor before** handing the live set to the byte GC, and
  refuses the compaction if that write fails — the reverse order leaves a crash window in which
  the rows and the floor are both gone.

Across processes the design does **not** prevent two coordinated writers deriving the same next
value; it relies on the collision being *detected*. Detection runs entirely off the opaque
write-set the coordinator equality-compares, via three key classes — `b"r"` for the model's
identity, `b"u"` for a `&unique` field, and `b"s"` for an integer auto that is neither (#260).
Any of them turns a duplicate into a `Nack`. `^` contributes nothing: an index makes a value fast
to *find* but claims nothing at commit time — which is now immaterial, since the sequence claim
covers the bare shape regardless of whether it is indexed.

A `Nack`ed sequence claim triggers a `__peer_refresh` before the retry. This is not an
optimization but a **termination** requirement: the retry re-runs the prepare closure, which
allocates the *next* value, so a writer N values behind a peer would need N attempts and exhaust
a bounded retry budget. It cannot rely on the ordinary peer-refresh gate, because a client's
view of the coordinator LSN advances only on its own `Ack` — a `Nack` never trips it. Nor can it
read the winning value out of the returned key: the coordinator hands back the key that
*collided*, which is the one **we** sent, carrying our own proposal. Re-reading the shared
columns is what actually re-derives the counter past every committed value.

`&unique` remains the stronger marking where uniqueness must hold against all history: its index
is durable, while the coordinator's conflict map is rebuilt empty on restart.

Identity-wise, `Manifest.auto_sequences` is **inert substrate**: the two `Manifest` backends store
and return the map and never parse a key or branch on a value. Which fields appear, what the
numbers mean, and every read and write of them belong to generated code — the rule is enforced by
a guard test, not only by a doc comment.

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
- **Per-process auto-increment counters made conflict-visible, over a shared allocator.** A
  coordinator-side sequence would put a schema-shaped concern inside schema-agnostic substrate;
  instead each process allocates locally and the *collision* is detected through the opaque
  write-set. The cost is one extra opaque key per insert per bare auto field, and a conflict map
  that grows with committed keys.
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
