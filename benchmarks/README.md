# ForgeDB benchmarks

Comparative benchmarks of ForgeDB's **generated** database code against established
databases. Design, targets, scenarios, and methodology live in
[`docs/BENCHMARKS.md`](../docs/BENCHMARKS.md); this README is just how to run it.

This is a **detached cargo project** (its own empty `[workspace]`) so the heavy bench
deps (SQLite, later DuckDB / Postgres client libs) never enter the root workspace build
or test baseline. Run everything from the repo root — never `cd` in here.

```bash
make bench            # every implemented suite (currently ForgeDB + SQLite)
make bench-forgedb    # ForgeDB generated code only
make bench-sqlite     # SQLite only
make bench-regen      # re-emit gen/database.rs from bench.forge (after codegen changes)
```

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
