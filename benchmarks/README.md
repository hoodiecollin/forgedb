# ForgeDB benchmarks

Comparative benchmarks of ForgeDB's **generated** database code against established
databases. Design, targets, scenarios, and methodology live in
[`docs/BENCHMARKS.md`](../docs/BENCHMARKS.md); this README is just how to run it.

This is a **detached cargo project** (its own empty `[workspace]`) so the heavy bench
deps (SQLite, later DuckDB / Postgres client libs) never enter the root workspace build
or test baseline. Run everything from the repo root — never `cd` in here.

```bash
make bench            # every implemented suite (ForgeDB + SQLite)
make bench-forgedb    # ForgeDB generated code only
make bench-sqlite     # SQLite only
make bench-redb       # redb (pure-Rust embedded KV)
make bench-duckdb     # DuckDB (embedded columnar; bundled C++ build, ~2.5 min first time)
make bench-postgres   # PostgreSQL — ephemeral cluster via devbox (see below)
make bench-regen      # re-emit gen/database.rs from bench.forge (after codegen changes)

# Config matrix (epic #126): same scenarios across generated config variants.
make bench-regen-matrix   # regenerate gen/<variant>/ from configs/<variant>.toml (REQUIRED first)
make bench-matrix         # run the write-path / churn / reopen scenarios across variants
```

The matrix variant modules (`gen/<variant>/`) are **gitignored** (regenerable from
`bench.forge` + `configs/*.toml`) — run `make bench-regen-matrix` before `make bench-matrix`.
Config axes live in `benchmarks/configs/*.toml`; results + interpretation are in
[`docs/BENCHMARKS.md`](../docs/BENCHMARKS.md) under "Configuration matrix".

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

## Status

**First cut: ForgeDB + SQLite.** redb, DuckDB, PostgreSQL, and the PGlite (WASM,
in-process) variant are designed in `docs/BENCHMARKS.md` and added incrementally against
the same shared scenarios.

`gen/database.rs` is generated and drifts as codegen changes — its compiling here is
itself a codegen guard (snapshot pass ≠ output compiles). Rerun `make bench-regen` after
touching the generators.
