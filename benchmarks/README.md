# ForgeDB benchmarks

Comparative benchmarks of ForgeDB's **generated** database code against established
databases. Design, targets, scenarios, and methodology live in
[`docs/BENCHMARKS.md`](../docs/BENCHMARKS.md); this README is just how to run it.

This is a **detached cargo project** (its own empty `[workspace]`) so the heavy bench
deps (SQLite, later DuckDB / Postgres client libs) never enter the root workspace build
or test baseline. Run everything from the repo root — never `cd` in here.

```bash
make bench            # the no-setup embedded suites (ForgeDB + SQLite + redb + DuckDB)
make bench-forgedb    # ForgeDB generated code only
make bench-sqlite     # SQLite only
make bench-redb       # redb (pure-Rust embedded KV)
make bench-duckdb     # DuckDB (embedded columnar; bundled C++ build, ~2.5 min first time)
make bench-postgres   # PostgreSQL — ephemeral cluster via devbox (see below)
make bench-pglite     # JS/Bun suite: PGlite (Postgres WASM, in-process) vs bun:sqlite
make bench-footprint  # on-disk bytes per corpus (all engines) + ForgeDB churn bloat (scenario 18)
make bench-concurrency# ForgeDB reader throughput under a live writer (#56-B, scenario 16)
make bench-workload   # sustained MIXED r/c/u/d/scan under phased load (scenario 20, #218)
make bench-regen      # re-emit gen/database.rs from bench.forge (after codegen changes)

# Config matrix (epic #126): same scenarios across generated config variants.
make bench-regen-matrix   # regenerate gen/<variant>/ from configs/<variant>.toml (REQUIRED first)
make bench-matrix         # run the write-path / churn / reopen scenarios across variants
```

The matrix variant modules (`gen/<variant>/`) are **gitignored** (regenerable from
`bench.forge` + `configs/*.toml`) — run `make bench-regen-matrix` before `make bench-matrix`.
Config axes live in `benchmarks/configs/*.toml`; results + interpretation are in
[`docs/BENCHMARKS.md`](../docs/BENCHMARKS.md) under "Configuration matrix".

## Mixed-workload driver (`make bench-workload`)

Every Criterion scenario here times **one** operation in isolation on a pristine or
trivially-shaped database. That cannot measure an append-only engine's churn cost, because
that cost is *history-dependent* — it depends on how mutated the database already is when
the next operation lands. `make bench-workload` is the one scenario that builds the history
first: a seeded mix of reads/creates/updates/deletes/scans, offered at **phased arrival
rates** (`warmup → steady → burst → recover`), replayed identically by ForgeDB, SQLite and
redb at matched durability.

```bash
make bench-workload                       # quick smoke matrix (fsync-bound)
make bench-workload ARGS="--full"         # full amplification ladder A = 1..32
make bench-workload ARGS="--forgedb-only" # skip the comparison engines
make bench-workload ARGS="--scan-sweep"   # scan path, FIXED-width subject (Metric)
make bench-workload ARGS="--var-sweep"    # scan path, VARIABLE-width subject (Doc)
make bench-workload ARGS="--verify"       # driver self-checks
```

### The two scan sweeps

Scans are ~1 % of a realistic op mix, far too few samples to locate a cliff, so the scan path
gets its own modes. They use **different subjects on purpose**, because the fixed-width and
variable-width column reads are separate code paths with separate failure modes — measuring a
model that has both reports their sum and separates nothing:

| mode | subject | isolates | shape found |
|---|---|---|---|
| `--scan-sweep` | `Metric` (22 fixed columns, no string) | `FixedColumn::export` | a **step** — a lost `mmap` fast path (#221) |
| `--var-sweep` | `Doc` (4 string columns + 2 fixed) | `VariableColumn::gather_buffered` | a **slope** — cost ∝ amplification (#222) |

`Doc` declares `@projection(meta: seq, kind)` over its *fixed* columns only, so the same model
under the same churn gives two scan paths: the projection never touches a `VariableColumn` and
acts as an **in-run control**, which is what makes a slope in the narrow scan attributable to
the variable path rather than to machine state.

`--var-sweep` runs against the **`churn_probe`** generated variant (`compaction = false` +
`fsync = "never"`), so it needs `make bench-regen-matrix` first. Compaction-off is required, not
convenient: the default build caps amplification at `1 + 4000/live_rows`, which on a 10k-row
corpus is a 1.0×–1.4× range — no lever arm to establish a slope over. Fsync-never only affects
the preload, and this mode measures reads.

Deliberately **not** a Criterion bench: Criterion is a closed loop (fire, wait, fire), which
by construction removes the arrival-time variance that burstiness is made of and cannot
express queueing delay. Latency is recorded against *intended* submission time, so a stall
lands in the tail instead of being silently absorbed (coordinated omission).

Runs are fsync-bound by design — the durability barrier is matched across engines rather
than tuned away, so wall-clock is dominated by `F_FULLFSYNC`. That is the honest cost of the
comparison, not a harness defect.

## PostgreSQL via devbox (declarative host deps)

`make bench-postgres` runs the PG suite against an **ephemeral local cluster** with no
system Postgres install and no binary download: the repo's `devbox.json` declares
`postgresql@16`, and `benchmarks/scripts/pg_run.sh` (run under `devbox run`) does
`initdb` → `pg_ctl start` on a unix socket in a tempdir → the bench → `pg_ctl stop`,
tearing the cluster down on exit. Install devbox (https://www.jetify.com/devbox) once;
the package set is pinned in `devbox.lock`. The suite reads the DSN from
`FORGEDB_BENCH_PG_URL`, so it is a no-op (prints guidance) if run without the cluster.

Filter to one scenario and shorten the run while iterating:

```bash
cargo bench --manifest-path benchmarks/Cargo.toml --bench forgedb_bench -- \
  "forgedb/point_lookup" --measurement-time 3 --warm-up-time 1
```

Criterion writes HTML reports under `benchmarks/target/criterion/`.

## Layout

| Path | What |
| --- | --- |
| `bench.forge` | The shared benchmark schema (source of truth for the data model). |
| `schema.sql` | Hand-verified 1:1 SQL mapping of `bench.forge` (SQLite/PG DDL). |
| `gen/database.rs` | **Generated** ForgeDB code — do not hand-edit; regenerate with `make bench-regen`. |
| `src/lib.rs` | Shared seeded data generation + the generated module. |
| `benches/forgedb_bench.rs` | ForgeDB Criterion suite. |
| `benches/sqlite_bench.rs` | SQLite Criterion suite. |
| `benches/redb_bench.rs` | redb Criterion suite. |
| `benches/duckdb_bench.rs` | DuckDB Criterion suite. |
| `benches/pg_bench.rs` | PostgreSQL Criterion suite (needs a running cluster — `make bench-postgres`). |
| `benches/matrix_bench.rs` | ForgeDB config-matrix Criterion suite (needs `make bench-regen-matrix`). |
| `examples/footprint.rs` | On-disk footprint report (all engines) + ForgeDB churn bloat (scenario 18). A size report, not a Criterion timing — an example so it can use the bench dev-deps. |
| `examples/concurrency.rs` | ForgeDB reader-throughput-under-a-live-writer report (#56-B, scenario 16). |
| `scripts/pg_run.sh` | Ephemeral-PostgreSQL lifecycle (initdb → start → bench → stop) for `make bench-postgres`. |
| `js/bench.ts` | JS/Bun suite: PGlite vs `bun:sqlite` (`mitata` timings). Not part of the Rust Criterion harness. |

## Status

**All five comparison targets implemented** (ForgeDB, SQLite, redb, DuckDB, PostgreSQL) plus
the PGlite (WASM, in-process) JS variant, over the shared scenarios. Point/probe/traversal +
**scan/aggregate** (scenario 7), **concurrency** (scenario 16), and **footprint** (scenario 18)
are cut cross-engine; results + interpretation live in `docs/BENCHMARKS.md`.

`gen/database.rs` is generated and drifts as codegen changes — its compiling here is
itself a codegen guard (snapshot pass ≠ output compiles). Rerun `make bench-regen` after
touching the generators.
