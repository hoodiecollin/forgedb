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
| **redb** | 1 | embedded, pure-Rust KV | Closest on *deployment shape* — generated Rust links a substrate crate, no external process. | **Implemented** |
| **DuckDB** | 1 | embedded, **columnar** | Closest on *storage model* (our columns vs. their vectorized columnar). Expected to win scans. | **Implemented** |
| **PostgreSQL** (`postgres`, localhost socket) | 2 | client/server, row-store | The industry reference. Carries parse + planning + socket overhead — anchors the RDBMS axis. | **Implemented** |
| **PGlite** (`@electric-sql/pglite`) | 2 (variant) | **Postgres compiled to WASM, in-process** | A Postgres variant that runs in-process (no server, no socket). Isolates "what does the Postgres engine cost" from "what does the client/server boundary cost" when read against the server PG numbers. | **Implemented** (JS/Bun suite) |

**All five comparison targets are implemented** (SQLite, redb, DuckDB, PostgreSQL, PGlite),
each over the same shared scenarios. First cross-engine result cuts are recorded per engine
below. The **scan/aggregate** (scenario 7), **concurrency** (scenario 16), and **footprint**
(scenario 18) scenarios are now also implemented and cut cross-engine — see "Scan / aggregate",
"Concurrency", and "Footprint" below. Remaining gaps (bulk-load sweeps at 1e6, crash-recovery
timing) are filled incrementally.

### redb (first cross-engine cut, 2026-07-18, macOS Apple Silicon)

`make bench-redb`. redb models the relational shape explicitly (secondary index +
reverse-FK/M2M as multimap tables); reads decode a packed value blob so they
materialize the FULL record, matching the SQLite suite. Numbers next to the ForgeDB
default-config baseline (same host/run epoch) — **relative** shape is the signal:

| scenario | redb | ForgeDB (default) | note |
| --- | --- | --- | --- |
| `insert_user` (barrier) | 4.04 ms (`immediate`) | 3.95 ms | matched `F_FULLFSYNC`; ~parity |
| `insert_user` (relaxed) | 0.50 ms (`eventual`) | — (no relaxed tier) | redb defers the per-commit fsync |
| `point_lookup` | 0.62 µs | 3.48 µs | redb B-tree get beats the generated `id_to_row`+column read here |
| `index_probe` | 1.11 µs | 3.67 µs | redb 2-table hop (email→id→row) still faster |
| `reverse_fk` | 3.94 µs | 36.9 µs | ForgeDB materializes full records per child (`find_by`+`get`) — the gap is real |
| `m2m` | 1.62 µs | 5.94 µs | redb multimap vs the generated junction index |

Honest read: on this corpus redb's B-tree reads are faster than ForgeDB's generated
read path (notably `reverse_fk`, where ForgeDB full-materializes each child — the #160
residual). ForgeDB's differentiators (typed generated API, no query parser, columnar
footprint, snapshot/replica) are not what these point/traversal micro-latencies capture;
the value of the comparison is exactly surfacing where the generated path is slower.

### DuckDB (first cross-engine cut, 2026-07-18, macOS Apple Silicon)

`make bench-duckdb` (the `duckdb` crate's `bundled` feature builds the C++ amalgamation —
a ~2.5 min one-time compile). DuckDB is here for its **columnar** storage model; it is
expected to be **weak at single-row OLTP** (point insert/lookup) and to **win vectorized
scans/aggregates** — the mirror image of the embedded row engines.

| scenario | DuckDB | redb | ForgeDB (default) | note |
| --- | --- | --- | --- | --- |
| `insert_user` | 208 µs | 4.04 ms (barrier) / 0.50 ms | 3.95 ms (barrier) | DuckDB WAL, not a per-row `F_FULLFSYNC` barrier — not like-for-like |
| `bulk_load/10000` | 1.39 s | 4.9 s (eventual) | 50.7 s (barrier) | row-at-a-time INSERT is DuckDB's worst case (its bulk path is the Appender) |
| `point_lookup` | 88.4 µs | 0.62 µs | 3.48 µs | **columnar point lookup is ~25× slower than ForgeDB, ~140× slower than redb** |
| `index_probe` | 82.6 µs | 1.11 µs | 3.67 µs | same columnar point-op weakness |
| `reverse_fk` | 106 µs | 3.94 µs | 36.9 µs | — |
| `m2m` | 196 µs | 1.62 µs | 5.94 µs | — |

Honest read: DuckDB loses every point/traversal micro-op here by 1–2 orders of magnitude —
**as expected for a columnar analytical engine**; row-at-a-time access is not what it
optimizes. The scenario where DuckDB should win — a filtered **scan + sort + aggregate**
over the whole table (scenario 7) — is a cross-engine addition still TODO across all
suites; adding it is where DuckDB's model pays off and is the honest place to show it.

### PostgreSQL (first cross-engine cut, 2026-07-18, macOS Apple Silicon)

`make bench-postgres` — spins an **ephemeral cluster from the devbox-provided `postgresql`
package** (declarative host dep, no binary download): `benchmarks/scripts/pg_run.sh` runs
`initdb` → `pg_ctl start` on a unix socket in a tempdir → the suite → `pg_ctl stop`, all
under `devbox run`. The suite connects over the unix socket (`FORGEDB_BENCH_PG_URL`).

PostgreSQL is a full **client/server** RDBMS — every op carries **parse + plan + protocol
round-trip** the embedded engines do not — so it anchors the "what a real RDBMS costs"
axis, not a head-to-head.

| scenario | PostgreSQL | ForgeDB (default) | redb | note |
| --- | --- | --- | --- | --- |
| `insert_user` (`sync_on`) | 56.1 µs | 3.95 ms (barrier) | 4.04 ms (barrier) | ⚠ PG `synchronous_commit=on` on macOS uses plain `fsync`, **not** `F_FULLFSYNC` — a weaker durability tier, NOT barrier-matched |
| `insert_user` (`sync_off`) | 37.4 µs | — | 0.50 ms | relaxed (group commit) |
| `point_lookup` | 26.0 µs | 3.48 µs | 0.62 µs | the ~7.5× over ForgeDB is the **client/server round-trip** (parse+plan+IPC over a socket) |
| `index_probe` | 28.9 µs | 3.67 µs | 1.11 µs | — |
| `reverse_fk` | 32.4 µs | 36.9 µs | 3.94 µs | PG's planned index scan ≈ ForgeDB's full-materialize here |
| `m2m` | 59.5 µs | 5.94 µs | 1.62 µs | PG plans a hash/nested-loop join; the others probe an index directly |

Honest read: PostgreSQL's read latencies (26–60 µs) are dominated by the protocol
round-trip — it is 1–2 orders of magnitude slower than the in-process embedded engines on
these point/traversal micro-ops, **which is the client/server cost the axis exists to
show**, not an engine deficiency. Two durability caveats: (1) PG's `sync_on` here is
plain-`fsync` durability (weaker than the `F_FULLFSYNC` barrier ForgeDB/redb/SQLite-`fullfsync`
use), so its ~56 µs insert is NOT a like-for-like barrier comparison — a `wal_sync_method =
fsync_writethrough` (macOS F_FULLFSYNC) tier is the barrier-matched follow-up; (2) PG still
beats columnar DuckDB's 88 µs point lookup despite the socket, because DuckDB's columnar
model is the wrong shape for point ops.

### PGlite + bun:sqlite — the JS/Bun suite (2026-07-18, macOS Apple Silicon)

`make bench-pglite` (or `bun run benchmarks/js/bench.ts`). PGlite doesn't fit the native
Rust Criterion harness — it is Postgres 16 compiled to WASM, driven from JS — so it lives
in a separate Bun suite (`benchmarks/js/`, `mitata` timings) alongside `bun:sqlite` (native
SQLite via Bun's binding) as the JS-side anchor. The corpus generator mirrors the Rust seed
logic (splitmix64 + `id_for`); the read corpus is smaller (2 000 posts) because PGlite's
per-query WASM-boundary cost makes a 1e4 row-at-a-time load slow (point/probe latency is
O(1), so the smaller corpus is representative).

| scenario | PGlite (PG-in-WASM, in-process) | bun:sqlite (native) | ratio |
| --- | --- | --- | --- |
| `point_lookup` | 113.5 µs | 1.33 µs | 85× |
| `index_probe` | 95.1 µs | 0.90 µs | 105× |
| `reverse_fk` | 110.9 µs | 1.78 µs | 62× |
| `m2m` | 148.9 µs | 2.70 µs | 55× |

**The headline finding (why PGlite is here): PGlite in-process (95–149 µs) is actually
*slower* than native server PostgreSQL over a unix socket (26–60 µs above).** Removing the
client/server transport does NOT win, because the WASM engine's execution overhead exceeds
the socket cost it saves — i.e. for this Postgres, the *engine-in-WASM* cost dominates the
*transport* cost. That is exactly the engine-vs-transport separation PGlite was included to
expose. Meanwhile `bun:sqlite` (native SQLite through the JS boundary) lands at 0.9–2.7 µs —
in the same range as the native embedded engines, confirming the JS boundary itself is cheap;
it is WASM execution, not "being called from JS", that makes PGlite slow.

### A note on PGlite

PGlite is Postgres 16 built to WebAssembly, run in a single in-process instance with no
server. It is driven from JavaScript/TypeScript, so it does **not** fit the native Rust
Criterion suite — it lives in a separate Bun-driven suite (`benchmarks/js/`, **implemented**)
alongside `bun:sqlite` (as a JS-side sanity anchor) and, later, the ForgeDB **WASM
read-replica** (#110) so the in-process/browser story is measured on its own terms.
Reading PGlite (in-process WASM PG) against server PostgreSQL (native, over a socket)
separates the engine cost from the transport cost — that contrast is the point of
including it.

### Scan / aggregate (scenario 7, 2026-07-18, macOS Apple Silicon)

Two shapes over the 10 000-post corpus, run in every embedded suite (`make bench-<engine> --
scan`): **`scan_aggregate`** = `COUNT + SUM(views) WHERE published` (a full-table aggregate),
and **`scan_sort_top10`** = top-10 posts by `views (>= 50 000)` (a filtered scan + sort + page).
This is the scenario the analytical/columnar engine is built to win.

| scenario | ForgeDB (pre-#168) | ForgeDB (now) | SQLite | redb | DuckDB |
| --- | --- | --- | --- | --- | --- |
| `scan_aggregate` | 32.4 ms | **568 µs** (#168) | 352 µs | 632 µs | **100 µs** |
| `scan_sort_top10` | 32.6 ms | **37 µs** (#169) | **4.35 µs** | 809 µs | 491 µs |

**#168 landed the column-pruned sequential scan (2026-07-20), a ~56×/~42× win** — the
`scan_aggregate` dropped 32.4 ms → 568 µs and `scan_sort_top10` 32.6 ms → 763 µs (now *faster*
than redb's honest full scan, and within ~1.6× of SQLite on the aggregate). The mechanism (below)
was measured, not asserted; the pre-#168 column keeps the honest starting point.

The original diagnosis and what changed:

1. **DuckDB wins the pure aggregate (100 µs)** — vectorized columnar aggregation over pruned
   columns, exactly as predicted for an analytical engine. (It is ~3× slower than SQLite on the
   `top10` because that one is index-served for SQLite; see below.) ForgeDB is now ~5.7× off
   DuckDB on the aggregate (was ~320×).
2. **The 32 ms was ~80 000 per-row `read_*` syscalls in hashmap order, not a columnar-layout
   penalty.** `__scan_all` walked `id_to_row.values()` (random order) and read every column
   positionally per row (`pread` each) — neither columnar sequential locality nor row-store
   contiguity, and the files were cache-hot so ordering alone barely helped (a physical-order sort
   measured only −3%). **#168** iterates the live rows in physical order and **bulk-loads each
   scan column once** (`FixedColumn::gather_buffered` / `VariableColumn::gather_buffered` — a
   zero-copy `mmap` alias of the dense prefix in the churn-free common case, else one gathered
   copy), decoding from memory via the *same* per-field decoder `read_at` uses (no second decode
   path). One bulk `Tombstones::live_indices` read replaces the per-row liveness check. Result:
   two bulk reads per column instead of a syscall per row × column. **Residual vs DuckDB/SQLite:**
   the scan still materializes the full narrow row (id + all filterable columns, incl. a `title`
   `String` alloc) rather than only the queried columns, and aggregates row-wise rather than
   vectorized — the remaining gap is that, not the mutation model.
3. **`scan_sort_top10`: #169 made it index-served (763 µs → 37 µs).** SQLite has a B-tree
   `idx_post_views`, so `WHERE views >= T ORDER BY views DESC LIMIT 10` is an ordered range scan +
   limit (O(10)). **#169** gives `views: ^u64` a **parallel `BTreeMap` ordered index** (keyed by the
   typed value, alongside the untouched exact-match hash index), and a generated
   `find_by_views_range(min, max, descending, limit)` that walks it — the same O(offset+limit)
   ordered-range shape. The benchmark now calls it (fair apples-to-apples with SQLite's index-served
   number). The residual ~37 µs vs SQLite's 4.35 µs is the 10 full-record `get()`s (each ≈ the 3.5 µs
   point-lookup: full column materialize + version resolution), not the index walk — the same
   materialization residual as `scan_aggregate`. Ordered index covers non-nullable
   `u32/u64/i32/i64/timestamp/decimal`; **f64** (no clean total order) and **nullable** are deferred.

The JS suite (2 000-post corpus, **no `views` index** in its DDL) adds the same two shapes for
PGlite vs `bun:sqlite`:

| scenario | PGlite | bun:sqlite | ratio |
| --- | --- | --- | --- |
| `scan_aggregate` | 241 µs | ~72 µs | 3.4× |
| `scan_sort_top10` | 318 µs | ~103 µs | 3.1× |

The PGlite-vs-SQLite scan gap (~3×) is FAR smaller than its point-op gap (~55–110× above) —
because PGlite's WASM-engine overhead is largely **fixed per query**, so it amortizes over a
scan's many rows but dominates a single point op. (JS row count + missing `views` index mean
these are not 1:1 comparable to the Rust suite's absolute numbers.)

### Concurrency (scenario 16, 2026-07-18, macOS Apple Silicon)

`make bench-concurrency`. ForgeDB reader throughput under a live writer (#56-B: single writer +
lock-free concurrent readers). A `DatabaseReader` + `DatabaseSnapshot` are captured, the writable
`Database` is moved into a background writer thread that inserts continuously, and N reader threads
hammer point reads on the captured handle. If reads were serialized against the writer, the
`(+writer)` column would collapse toward the writer's rate.

| readers | reads/s (idle) | reads/s (+writer) | writer writes/s |
| --- | --- | --- | --- |
| 1 | 274 644 | 271 262 | 273 |
| 2 | 295 867 | 302 747 | 265 |
| 4 | 338 970 | 334 061 | 264 |
| 8 | 139 621 | 164 031 | 271 |

Honest read: the `(+writer)` column stays within noise of the `idle` column at 1/2/4 readers —
**reads are lock-free; a live writer does not throttle them**, confirming #56-B. Read throughput
scales to ~4 threads then falls off at 8 (past this host's performance-core count — scheduling/
oversubscription, not a lock). The writer runs at ~270 inserts/s, which independently corroborates
the **single** `F_FULLFSYNC` barrier per insert (~3.7 ms ⇒ ~270/s; the pre-#130 double barrier
would be ~135/s). Honest limit: this is one writer + N readers (the v1 contract); it is a snapshot
read (rows appended after capture are invisible — that is the isolation guarantee, not a miss).

### Footprint (scenario 18, 2026-07-18, macOS Apple Silicon)

`make bench-footprint`. On-disk bytes for the shared 41 500-row corpus (1 000 users + 10 000 posts
+ 500 tags + 30 000 M2M links), each engine loaded via its simplest durable path and measured by
summing its data files (SQLite checkpointed, DuckDB `CHECKPOINT`, ForgeDB `checkpoint()`):

| engine | on-disk |
| --- | --- |
| **ForgeDB** | **1.88 MB** |
| DuckDB | 4.01 MB |
| SQLite | 4.80 MB |
| redb | 8.54 MB |

Honest read: **ForgeDB has the smallest footprint (1.88 MB — ~2.6× smaller than SQLite, ~4.5×
smaller than redb)** — the flip side of the slower point reads. Columnar files pack a column's
values contiguously with no per-row B-tree page overhead or per-row key repetition; redb is largest
because a B-tree KV stores the full 16-byte key beside every value in every table AND repeats ids in
the secondary/multimap index tables. This is the footprint dividend ForgeDB trades its point-read
latency for.

ForgeDB update-churn bloat (superseding-version append; #66) — 2 000 updates to one user, then
`compact()`:

| | bytes |
| --- | --- |
| before compact | 247.3 KB |
| after compact | 84.3 KB |
| reclaimed | 162.9 KB (66%) |

Honest read: updates **grow** storage (each update appends a new row version; measured, not hidden),
and in-process `compact()` reclaims the dead versions (66% here). (Implementing this scenario surfaced
and fixed a real divide-by-zero in `forgedb-compaction` when `compact()` ran over a database with an
untouched/empty model — see the compaction bullet in CLAUDE.md.)

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
- **Matched fsync semantics per group** — ForgeDB is fixed at `FsyncPolicy::Always`,
  which on macOS is an `F_FULLFSYNC` barrier. The like-for-like SQLite config is
  `journal_mode=WAL` + `synchronous=FULL` + **`fullfsync=1`** (also a barrier); SQLite's
  default (no `fullfsync`) is a *weaker* guarantee (plain `fsync`) and belongs in a
  separate row, not the matched comparison. The harness runs SQLite inserts at both.
  Never mix durability levels in one chart. (See "fsync-parity findings" above.)
- **Criterion** for micro-timings (point ops, probes, traversals); a separate driver for
  wall-clock end-to-end (bulk load, reopen, footprint).
- **Report distributions (p50/p99), not just means**, always next to a footprint number.
- **Structural caveats stated inline** in the results: our updates/deletes grow storage
  until compaction; M2M is a scan; PG pays parse + network; hash indexes are exact-match
  only (no range/prefix).

## fsync-parity findings (RESOLVED)

The write comparison hinged on a durability-guarantee mismatch. Measured on this macOS
host (Apple SSD), per durable op:

| Primitive | per-op | What it is |
| --- | --- | --- |
| Rust `File::sync_all()` | ~3.5 ms | On macOS this is `fcntl(F_FULLFSYNC)` — a true barrier flush. |
| `libc fcntl(F_FULLFSYNC)` | ~3.7 ms | The macOS barrier primitive directly (matches `sync_all`). |
| `libc::fsync()` | ~27 µs | Plain fsync — flushes to the drive cache, **not** a barrier. |
| SQLite `synchronous=FULL` (default) | ~113 µs | Uses plain `fsync()`. |
| SQLite `FULL` + `fullfsync=1` | ~4.0 ms | Uses `F_FULLFSYNC` — same barrier as ForgeDB. |

**Conclusions:**

1. **`forgedb-wal` (`FsyncPolicy::Always`) calls `File::sync_all()`** (`writer.rs`), which
   on macOS is `F_FULLFSYNC` — a real barrier. SQLite's out-of-box `synchronous=FULL`
   uses plain `fsync()` (weaker on macOS). So the raw ~100× single-insert gap was
   **almost entirely a durability-level mismatch, not engine inefficiency.** At matched
   durability (SQLite `fullfsync=1`), SQLite is ~4.0 ms.
2. **By default ForgeDB does ONE barrier per insert; a second only when replication is on.**
   The generated `insert` fsyncs the WAL (`sync_all`, ~3.5 ms). *When `[runtime].replication
   = true`* it ALSO records to the durable replication broker (`DurableBroker::record` →
   `sync_all` under `FsyncPolicy::Always`, `durable.rs`) — a second ~3.5 ms barrier. Since
   **#130 made `[runtime].replication` default OFF**, `open_at` no longer attaches the broker
   unconditionally, so the default single-insert is **~3.95 ms ≈ one barrier** (on par with
   SQLite-`fullfsync`'s ~4.0 ms); the residual over the raw barrier (~0.5 ms) is serde +
   column appends + index maintenance — reasonable. With `replication_on` it is ~7.35 ms ≈
   2 × a barrier (see the configuration matrix). (Historically — pre-#130 — the broker was
   attached unconditionally, so *every* insert paid two barriers ~7.6 ms; that is no longer
   the default.)

**How the harness reports it now:** `sqlite/insert_user` runs at **both** durability
levels — `default` (plain fsync, SQLite's out-of-box guarantee) and `fullfsync` (matched
barrier) — so every write comparison is explicit about which durability it's at. ForgeDB
is always at the barrier level (its fsync policy is fixed `Always`, no relaxed tier).

## Performance-triage sweep — status

> **Primary focus (2026-07-19): the storage-model experiment — epic #167**, design
> `docs/proposals/storage-model-experiment.md`. It reframes the weak numbers below as four
> independent axes (columnar = keep; append-only = the axis under test; durability policy +
> generated-read-path quality = orthogonal confounds). **Phase 1** fixes the read-path confounds
> as strict wins (**#168 column-pruned scan — LANDED 2026-07-20, 32 ms → 568 µs / 763 µs**;
> **#169 ordered/range index — LANDED 2026-07-20, top-N 763 µs → 37 µs**; #160 narrow
> materialization, #170 group-commit); **Phase 2** adds a fixed-width in-place `GenConfig` variant
> to the config
> matrix and measures append-only-vs-in-place side-by-side. Several candidates listed further
> below (group/batch commit, relaxed durability) are now Phase-1 items of that epic.

The seeded candidates below were promoted to the tracked sweep **#152** (12 children).
**Landed:**

- **#162 — deferred compaction + in-place remap (2026-07-18, Options A + C).** Auto-compaction ran INLINE on
  the update/delete that crossed the dead-row threshold: checkpoint + rewrite every column file (temp+rename)
  + a full `*self = Self::new_at()` reopen (an O(rows × indexes) rehydrate rescan) — one write per ~1000
  mutations ate a multi-ms tail-latency spike. **(A defer)** Crossing the SOFT threshold now only sets a
  `compaction_due` flag (the triggering write does not stall); `Database::maintain()` runs the reclaim off the
  hot write turn at an operator/coordinator idle point, and a HARD ceiling
  (`COMPACTION_DEAD_THRESHOLD × COMPACTION_DEAD_CEILING_FACTOR`, ×4) still forces an inline compaction as a
  growth-bounding safety net if `maintain()` is never called. **(C in-place remap)** When compaction does run,
  it no longer rehydrate-rescans: it reopens ONLY the column handles (stale fds after the rename) via
  `new_at_no_rehydrate`, then remaps `id_to_row`/`id_versions` in place — a surviving row's dense new index is
  `keep_sorted.partition_point(|&r| r < old_row)` — and reinstalls the secondary index maps **verbatim** (they
  are value→id keyed, so a physical-row renumber leaves them correct). **Zero column reads** on the reopen.
  Guards `test_rust_generation_auto_compaction` (defer + ceiling + maintain) + `test_rust_generation_incremental_rehydrate`
  (in-place remap); E2E `scratchpad/perf162` (explicit compact preserves values/deletes/all index kinds + reopen
  consistency; soft-threshold defers, `maintain()` reclaims, ceiling forces inline). Corpus incl. integer-PK
  compile-checked. **Honest limit:** single physical append point per column stands — the reclaim is still
  serialized under the writer lock (just off the *write* turn); true background compaction needs the segmented-
  column effort.
- **#161 — incremental delta peer refresh (2026-07-18, Option B).** The Tier-3 coordinated peer refresh
  (`__sync_columns_from_disk` + `__reindex_committed`) CLEARED every in-memory map and rescanned all rows ×
  indexes on *every* refresh (before each prepare when a peer advanced). It now captures the pre-sync watermark
  and folds ONLY the new rows `[from..row_count)` via `__reindex_delta` — **O(new rows), not O(all rows ×
  indexes)** — replaying the SAME live update/delete maintenance per row (remove the pre-row version's keys via
  `self.get(id)`, add the folded row's keys via `self.read_at`), so the maps end up identical to a full rebuild
  with no second maintenance source to drift. Pure generated code, no on-disk artifact, no write amplification.
  Guard in `test_rust_generation_coordinated_client`; E2E `scratchpad/perf162` (delta fold over `__stage_append`ed
  "peer" rows == full `__reindex_committed`; supersede + tombstone + insert within the range; no-op tail).
  **Chosen over A (persist-per-checkpoint, which adds `(N/1000)×rows` write-amp exceeding the one-time reopen
  scan) and C (lazy index, which collides with the `&self` read API — `reader()`/`find_by_*` can't build
  continuously-mutated maps without write-path locks).** Cold-start reopen keeps the existing #110 narrow
  indexed-columns-only scan.
- **#154 — M2M traversal index (2026-07-18).** `post_tags`/`tag_posts` now probe in-memory
  `left_index`/`right_index` maps (O(degree)) instead of scanning every link row via `pairs()`.
  Measured **`m2m/post_tags`: ~38 ms → 5.94 µs** (~6400×; now within ~2× of SQLite's ~2.7 µs).
- **#160 — narrow list scan + index pushdown (2026-07-18, Options A + C).** The REST list handler
  full-materialized every column of every row via `all()`, then filtered/sorted/paginated and discarded all
  but `limit` rows — decoding N full records to return a page. **(A)** The live list path now filters + sorts
  an internal narrow `<Model>ScanRow` (id + only the filterable/sortable columns, decoded via the shared
  `generate_row_read_body` — the same body `read_at` uses, no drift) and **full-materializes only the paginated
  page** (`N` full decodes → `N` narrow decodes + `limit` full). The narrow filter/sort reuse the SAME per-field
  checks/arms as `_event_matches`/`_apply_sort` (one predicate source). **(C)** When the filter names an eligible
  single-indexed field (string/uuid/int/bool/decimal/timestamp/enum), candidates resolve from that field's
  secondary index (**O(matches)**) instead of scanning every row; a parse failure falls back to the full scan, so
  a match is never missed. The `<Model>ScanRow` is internal — never wired to REST `?projection=` / TS / OpenAPI.
  `?as_of` list reads keep the existing full-record path (a rarer inspector read; #159 already made its resolution
  sub-linear). Guard `test_rust_generation_list_scan_narrow`; E2E `scratchpad/perf160` (live `tower::oneshot`:
  narrow filter/sort/paginate returns full-record pages, unique-index + composite-fallthrough + no-match, projection
  + `as_of` intact); full `examples/` corpus (nullable/char/decimal/enum/composite/M2M/integer-PK) compile-checked.
  **Honest limit:** only the **live list** path is wired; reverse-FK getters (return full records by API contract)
  and live-query re-runs still full-materialize — a documented follow-up.
- **#159 — snapshot reads via per-id version index (2026-07-18, Option A).** `get_at`/`all_at`/
  `find_by_*_at` (REST `?as_of`, #85) resolved "newest version as of a watermark" by scanning `0..watermark`
  of the id column — O(watermark) per point read, and the FK snapshot-probe called `get_at` per candidate →
  **O(candidates × watermark)** (quadratic). Each id-bearing model now carries a per-id ascending list of its
  physical row versions (`id_versions: Arc<HashMap<Id, Vec<usize>>>`, appended in lockstep with `id_to_row`,
  rebuilt on reopen, Arc-shared into readers per #158). `get_at` binary-searches it for the newest row
  `< watermark` → **O(log versions)**; the FK-probe drops to O(candidates × log v); `all_at`/projection `_at`
  resolve per-id → O(distinct_ids × log v) (a win under update/delete churn, where dead versions inflate the
  watermark). No on-disk format change; the vec stays sorted for free (monotonic row_count). Guard
  `test_rust_generation_version_index` + updated snapshot-reads guard; E2E `scratchpad/perf158` proves
  snapshot-version resolution respects the watermark (an old snapshot resolves the pre-update version on the
  live writer via binary search) + reader isolation + reopen rebuild. Honest limit: `id_versions` grows with
  dead versions until compaction reclaims + reopen rebuilds (same bound as `id_to_row`).
- **#158 — reader() index-map sharing (2026-07-18, Option A).** `Database::reader()` no longer deep-clones
  every secondary/composite index map + `id_to_row` per call (was O(total live rows across the DB)). The maps
  are `Arc<HashMap<..>>`, so `reader()` capture is an O(1) `Arc::clone` (refcount bump); the single writer
  mutates via `Arc::make_mut` — copy-on-write, cloning a map once only while a reader still holds the prior
  snapshot, then in place. Probe reads are unchanged (`Arc` derefs to `HashMap`). Honest limit: a
  **persistently-held reader under continuous writes** degrades to a whole-map CoW clone per write — the
  cliff a persistent HAMT (Option B) would avoid, filed as the **#166** explore/experiment (needs an A-vs-B
  bench across both reader profiles before adoption).
- **#157 — redundant write-path serialization (2026-07-18).** Part A: serialize the record ONCE per
  insert/update/delete (WAL borrows the buffer, the broker moves it) instead of twice. Part B: a field in
  ≥2 index structures (single + composite) derives its tagged index key once per mutation and reuses it. No
  format change. The *encoding* half (JSON→binary codec across all internal wasteful-JSON sites) is the
  separate, larger **#165** (sweep catalogued there).
- **#156 — coordinator fsync off the turn lock (2026-07-18).** The Tier-3 coordinator's replication-log
  append + fsync now runs under a separate broker mutex, OUTSIDE the turn/condvar critical section (Option A),
  so a committing client's barrier no longer blocks other writers from a turn; the next writer's turn +
  client I/O overlap the barrier. The barrier is also configurable — `forgedb coordinate --fsync
  always|never|periodic` (Option C, default `always`) — and the broker is opened `Never` so the coordinator
  drives ≤1 barrier per commit (was N+1: per-record + explicit flush). `never`/`periodic` never risk committed
  client data (the log is resumable; clients fsync their own columns+WAL first).
- **#155 — M2M cascade-unlink (2026-07-18).** Fell out of #154: `unlink_all_*` + `unlink` now use the
  junction index (O(degree)) instead of O(link_rows²) scans.
- **#153 — single-barrier checkpoint (2026-07-18).** `checkpoint()`/`commit()` push every column
  to the drive cache with a plain `fsync` (macOS `libc::fsync`, ~27 µs) then issue ONE device
  barrier (`F_FULLFSYNC`), instead of an `F_FULLFSYNC` per column — one barrier per collection
  checkpoint, not N. Substrate `forgedb-storage-native` gains `sync_to_drive()`/`barrier()`
  (no-ops on `storage-web`); publish gap open until storage-native/web/facade republish. (The
  userspace-`BufWriter` half of the original direction was rejected: buffered appends break
  positional `read_exact_at`/mmap read-after-write and the lock-free Direction-B reader view —
  unbuffered page-cache writes are load-bearing.)

## Configuration matrix (epic #126) — how generate-time knobs shift performance

`make bench-regen-matrix && make bench-matrix` regenerates the SAME `bench.forge`
under each `benchmarks/configs/<variant>.toml` (a distinct generated module) and runs
the write-path / churn / reopen scenarios against every relevant variant, so Criterion
lines the configs up side by side. Numbers below are macOS (Apple Silicon, APFS SSD),
2026-07-18, `--measurement-time 3` — **relative** deltas are the signal, not absolute
values (fsync-bound numbers are storage-hardware-specific).

**Write-path latency** (per single durable op):

| scenario | `default` | `fsync_never` | `replication_on` | `changefeed_small` |
| --- | --- | --- | --- | --- |
| `insert_one` | 3.95 ms | **36.1 µs** (~109×) | 7.35 ms (~1.9×) | 3.89 ms (≈) |
| `update_one` | 3.80 ms | **47.9 µs** (~79×) | 7.55 ms (~2.0×) | — |
| `delete_reinsert` (2 durable ops) | 7.83 ms | **80.5 µs** (~97×) | 14.79 ms (~1.9×) | — |

- **`[storage].fsync = "never"` (#129) is the dominant lever: ~80–110× faster writes.** The
  `F_FULLFSYNC` device barrier (~3.5 ms) *is* essentially the entire write latency; without it a
  write is a page-cache append (tens of µs). It trades a real power-loss durability window for that.
- **`[runtime].replication = true` (#130) ~doubles write latency** — the durable broker adds a SECOND
  `F_FULLFSYNC` per write (default OFF exists precisely to avoid this; the flagship #126 knob).
- **`[runtime].changefeed_capacity` has no write-latency effect** (it is a broadcast-buffer size, not
  on the durable path) — confirms the knob is free to tune for subscriber fan-out.

**Compaction churn** (250 updates to one id, then `maintain()`) and **cold-start reopen** (2 000
seeded rows):

| scenario | `default` | `compaction_off` | `compaction_low` |
| --- | --- | --- | --- |
| `churn_250_updates` | 1.003 s | 0.990 s | 1.081 s |
| `reopen_2000_rows` | 28.4 ms | 25.2 ms | — |

- Churn is fsync-dominated (250 × ~3.9 ms ≈ 0.98 s), so compaction is a small delta on top: threshold
  1000 (`default`) never auto-fires over 250 updates (one `maintain()` reclaim at the end), `compaction_off`
  never reclaims, and threshold 100 (`compaction_low`) reclaims — costing ~+8% (~90 ms for the extra
  reclaim). The #162-C **in-place remap** is what keeps each reclaim that cheap (no O(rows × indexes)
  rehydrate rescan); a low threshold trades a bit of throughput for bounded storage.
- `reopen_2000_rows` ≈ 28 ms is the rehydrate cost (id-scan + secondary-index rebuild) that #161-B (peer
  refresh) and #162-C (post-compaction) avoid re-paying on their hot paths.

## Performance-triage candidates (seeded from the fsync work)

Feed these into the broader perf sweep (not fixes — triage items):

- **Double fsync barrier per write — RESOLVED by #130 (the `[runtime].replication` knob).**
  Historically `open_at` attached a synchronous `FsyncPolicy::Always` replication broker
  unconditionally, so every insert/update/delete paid a second `F_FULLFSYNC` on the critical
  path even for apps that never consume `/replicate`. #130 made `[runtime].replication` default
  **OFF** — the broker is now attached only when replication is enabled — which took the
  default durable-write latency from ~7.6 ms to ~3.95 ms (one barrier, on par with
  SQLite-`fullfsync`). The `replication_on` variant (~7.35 ms) is the opt-in two-barrier cost.
  Still open as design items: a batched/async broker fsync, and coalescing the WAL + broker
  append behind a single barrier when replication IS on.
- **No relaxed durability tier.** ForgeDB's fsync policy is a fixed generated constant
  (`Always`); there is no per-deployment knob to trade the barrier for `Periodic`/grouped
  commit the way SQLite exposes `synchronous`/`fullfsync`. Worth a design look for the
  write-throughput story.
- **Group/batch commit (#170, Phase 1 of epic #167).** Bulk load fsyncs per row (one — now two —
  barriers each). A batched-commit path (one barrier per N rows) is the standard fix and would
  transform the bulk-load numbers; currently absent. Unlike `[storage].fsync=never` it keeps the
  barrier guarantee while amortizing it.

## Expected shape of results (stated up front, so results don't read as cherry-picked)

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
make bench            # the no-setup embedded suites (ForgeDB + SQLite + redb + DuckDB)
make bench-forgedb    # ForgeDB generated code only
make bench-sqlite     # SQLite only
make bench-footprint  # on-disk footprint (all engines) + ForgeDB churn bloat (scenario 18)
make bench-concurrency# ForgeDB reader throughput under a live writer (#56-B, scenario 16)
make bench-regen      # re-emit benchmarks/gen/database.rs from bench.forge
```

All targets run from the repo root (they pass `--manifest-path benchmarks/Cargo.toml`);
never `cd` into `benchmarks/`.
