# ForgeDB Benchmarks

Comparative benchmarks of ForgeDB's **generated** database code against established
databases. The harness lives in `benchmarks/` (a detached cargo project — see
"Harness architecture" below); run it with `make bench` (or the per-engine targets).

## Why this is subtle

ForgeDB is not a general-purpose database. It is a **compile-time generator**: a
`.forge` schema is transpiled into tailored Rust code that links a schema-agnostic
substrate (columns, WAL, change-feed). There is no query parser, no planner, and no
network hop in the generated read/write path — a `get(id)` is a direct index probe +
column read compiled for exactly one schema.

That shapes both *who* it is fair to compare against and *what* is honest to measure:

- The fair fights are **embedded, in-process, single-writer** engines (SQLite, redb).
- Columnar analytical engines (DuckDB) will win the scan/aggregate scenarios — showing
  that is honest, not a failure.
- A full client/server RDBMS (PostgreSQL) carries parse + planning + network cost we
  don't; it anchors the "what a real RDBMS costs" axis rather than being head-to-head.

Every result is reported **alongside a durability setting and an on-disk footprint** so
throughput can't be read in isolation.

## Comparison targets

| Engine | Tier | Model | Why it's here | Status |
| --- | --- | --- | --- |
| **SQLite** (`rusqlite`) | 1 | embedded, single-writer, row-store | The canonical embedded DB; closest to ForgeDB's write model. The primary comparison. | **Implemented** |
| **redb** | 1 | embedded, pure-Rust KV | Closest on *deployment shape* — generated Rust links a substrate crate, no external process. | Planned |
| **DuckDB** | 1 | embedded, **columnar** | Closest on *storage model* (our columns vs. their vectorized columnar). Expected to win scans. | Planned |
| **PostgreSQL** (`postgres`, localhost socket) | 2 | client/server, row-store | The industry reference. Carries parse + planning + socket overhead — anchors the RDBMS axis. | Planned |
| **PGlite** (`@electric-sql/pglite`) | 2 (variant) | **Postgres compiled to WASM, in-process** | A Postgres variant that runs in-process (no server, no socket). Isolates "what does the Postgres engine cost" from "what does the client/server boundary cost" when read against the server PG numbers. | Planned (JS/Bun suite) |

**First cut = SQLite only.** The other four are documented + designed here; their bench
files are added incrementally against the same shared scenarios.

### A note on PGlite

PGlite is Postgres 16 built to WebAssembly, run in a single in-process instance with no
server. It is driven from JavaScript/TypeScript, so it does **not** fit the native Rust
Criterion suite — it lives in a separate Bun-driven suite (`benchmarks/js/`, planned)
alongside `bun:sqlite` (as a JS-side sanity anchor) and, later, the ForgeDB **WASM
read-replica** (#110) so the in-process/browser story is measured on its own terms.
Reading PGlite (in-process WASM PG) against server PostgreSQL (native, over a socket)
separates the engine cost from the transport cost — that contrast is the point of
including it.

## Scenarios

Each scenario is *designed* to run across three row counts — **1e3 / 1e5 / 1e6** — so we
observe scaling, not a single point. **First-cut sizing note:** the checked-in defaults
use a smaller corpus (read corpus 1e4; bulk-load sweep 1e3 / 1e4) because ForgeDB's
`FsyncPolicy::Always` makes populate/setup fsync-bound (one fsync per insert), so a 1e6
setup dominates wall-clock. Point/probe latency is O(1), so the smaller corpus is
representative for those; the larger sweeps are opt-in as the harness grows (and are the
right place to measure the fsync-bound write ceiling deliberately, not incidentally). Records are generated from one deterministic, seeded source
(`benchmarks/src/lib.rs`) shared by every engine, and every engine uses the **same data
model** (`bench.forge` ↔ `schema.sql`, a hand-verified 1:1 mapping).

### Writes
1. **Bulk load** — insert N rows cold (WAL fsync + column append + checkpoint sawtooth).
2. **Single-row insert latency** — p50/p99 per commit.
3. **Update** — ForgeDB appends a superseding version (updates *grow* storage — measured, not hidden).
4. **Delete** — tombstone append.

### Reads
5. **Point lookup by PK** — `get(id)` (O(1) `id_to_row`) vs. indexed `WHERE id = ?`.
6. **Secondary-index lookup** — `find_by_views` / `get_by_email` (O(1) probe) vs. indexed column.
7. **Filtered scan + sort + paginate** — the generated list path (closed-set matcher + comparator). Expected DuckDB win.
8. **FK-index lookup** — `find_by_author` (O(1) since #100).

### Relations (ForgeDB's differentiator — generated traversal, no runtime join planner)
9. **Forward FK traversal** — `post_author`.
10. **Reverse one-to-many** — `user_posts` (O(1) FK-index probe).
11. **Many-to-many** — `post_tags` (**currently a linear junction scan** — a known weak spot; benchmarking it exposes it).
12. **Eager load** — `post_with_relations` vs. an equivalent SQL join.

### Durability / lifecycle
13. **Reopen / recovery time** — cold `open_at` (WAL replay + `id_to_row`/index rehydrate) vs. journal recovery.
14. **Crash recovery** — `kill -9` mid-write; assert 0 acked rows lost + measure recovery time. (Correctness benchmark.)
15. **Compaction** — bytes reclaimed + pause cost of in-process `compact()` after update/delete churn.

### Concurrency
16. **Concurrent readers under a live writer** (#56-B) — reader throughput while one writer appends.
17. **Transaction throughput** (MVCC Tiers 1–2) — commits/s under optimistic concurrent writers.

### Footprint
18. **On-disk size** per N rows (columnar packing; and update/delete bloat before compaction).
19. **Peak RSS** during load and during reopen (index rehydrate is O(rows) in memory — measured).

## Methodology guardrails

- **In-process for everyone possible** — SQLite/DuckDB/redb linked in-process; PG over a
  localhost socket with prepared statements, clearly labeled as carrying transport cost.
- **Matched fsync semantics per group** — ForgeDB's default `FsyncPolicy::Always` is
  compared against SQLite `synchronous=FULL` + `journal_mode=WAL`; a separate relaxed
  group compares the looser tiers. Never mix durability levels in one chart.
- **Criterion** for micro-timings (point ops, probes, traversals); a separate driver for
  wall-clock end-to-end (bulk load, reopen, footprint).
- **Report distributions (p50/p99), not just means**, always next to a footprint number.
- **Structural caveats stated inline** in the results: our updates/deletes grow storage
  until compaction; M2M is a scan; PG pays parse + network; hash indexes are exact-match
  only (no range/prefix).

## Open methodology items (must resolve before publishing write numbers)

- **macOS fsync parity (`F_FULLFSYNC` vs `fsync`).** Rust's `File::sync_all()` on macOS
  issues `F_FULLFSYNC` — a true barrier flush that costs several ms on Apple SSDs — while
  SQLite's `synchronous = FULL` uses plain `fsync()` unless `PRAGMA fullfsync = 1`. Early
  runs show ForgeDB single-insert latency ~100× SQLite's; a large part of that is likely
  **ForgeDB buying a stronger durability guarantee**, not raw inefficiency. Before any
  write comparison is reported, confirm what `forgedb-wal` actually calls and either set
  SQLite `fullfsync = 1` to match or state the durability difference explicitly. Do not
  publish the raw gap as-is.
- **Per-insert fsync count.** ForgeDB appends per-column files; verify whether a single
  insert fsyncs once (WAL only) or multiple times, since that changes the write story
  independently of the `F_FULLFSYNC` question.

## Expected shape of results (stated up front, so results don't read as cherry-picked)

- **ForgeDB wins:** point lookups, index/FK probes, reverse traversal, cold bulk load
  (no parser), no-network latency.
- **ForgeDB loses (fine to show):** analytical scans/aggregates vs. DuckDB; M2M joins
  (linear scan); storage footprint under update-heavy churn pre-compaction; anything
  needing a range/prefix index (we are hash exact-match only).

## Harness architecture

`benchmarks/` is a **detached cargo project** (its own empty `[workspace]`, substrate
path-patched to the local crates). It is deliberately *not* a member of the root
workspace, so the heavy native deps (SQLite, later DuckDB/PG client libs) never enter
`cargo build --workspace` or the test baseline.

```
benchmarks/
  Cargo.toml            detached; path-patches forgedb-* to ../crates/*
  bench.forge           the shared benchmark schema (source of truth)
  schema.sql            hand-verified 1:1 SQL mapping of bench.forge (SQLite/PG DDL)
  gen/database.rs       ForgeDB-GENERATED code (regenerate via `make bench-regen`)
  src/lib.rs            shared: seeded data generation + scenario row counts + helpers
  benches/
    forgedb_bench.rs    Criterion suite driving the generated Database API
    sqlite_bench.rs     Criterion suite driving rusqlite over schema.sql
  README.md             how to run
```

**`gen/database.rs` is generated, not hand-edited.** It drifts as codegen changes;
`make bench-regen` re-emits it from `bench.forge` through the current CLI. (Same
discipline as the `scratchpad/*_compile` harnesses — snapshot pass ≠ output compiles, so
the bench compiling the generated code is itself a codegen guard.)

The ForgeDB and SQLite suites implement the **same numbered scenarios over the same
seeded data**, so their Criterion groups line up for direct comparison.

## Running

```bash
make bench            # run every implemented engine suite (currently ForgeDB + SQLite)
make bench-forgedb    # ForgeDB generated code only
make bench-sqlite     # SQLite only
make bench-regen      # re-emit benchmarks/gen/database.rs from bench.forge
```

All targets run from the repo root (they pass `--manifest-path benchmarks/Cargo.toml`);
never `cd` into `benchmarks/`.
