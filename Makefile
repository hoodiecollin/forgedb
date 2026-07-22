# ForgeDB — root entry points. Everything runnable from the repo root; no `cd`.
# Rust workflows use cargo directly (see CLAUDE.md). JS/desktop workflows for the
# inspector app are wrapped here so they never require cd-ing into apps/inspector.

BUN := $(shell command -v bun 2>/dev/null || echo "$(HOME)/.bun/bin/bun")
INSPECTOR := apps/inspector
WEBSITE := apps/website
EXTENSION := apps/vscode-forgedb
BENCH := benchmarks/Cargo.toml

## Config variants for the matrix bench (epic #126): each becomes a generated
## module under benchmarks/gen/<name>/. Keep in sync with benchmarks/configs/.
BENCH_VARIANTS := default fsync_never replication_on compaction_off compaction_low changefeed_small

.PHONY: inspector-install inspector inspector-build inspector-typecheck \
        inspector-app inspector-app-build \
        website-install website website-build website-typecheck \
        extension-install extension-build extension-typecheck extension-package \
        bench bench-forgedb bench-sqlite bench-redb bench-duckdb bench-postgres \
        bench-pglite bench-matrix bench-regen bench-regen-matrix \
        bench-footprint bench-concurrency

## Run the embedded comparison suites that need no setup (ForgeDB + SQLite + redb +
## DuckDB). PostgreSQL (needs a cluster), the config matrix (needs regen), and the
## JS/PGlite suite are separate targets below — a bare `cargo bench` would also try to
## compile matrix_bench, whose gitignored variant modules only exist after
## `make bench-regen-matrix`. See docs/BENCHMARKS.md.
bench:
	cargo bench --manifest-path $(BENCH) \
		--bench forgedb_bench --bench sqlite_bench --bench redb_bench --bench duckdb_bench

## Benchmark the ForgeDB generated code only.
bench-forgedb:
	cargo bench --manifest-path $(BENCH) --bench forgedb_bench

## Benchmark SQLite only.
bench-sqlite:
	cargo bench --manifest-path $(BENCH) --bench sqlite_bench

## Benchmark redb only (pure-Rust embedded KV).
bench-redb:
	cargo bench --manifest-path $(BENCH) --bench redb_bench

## Benchmark DuckDB only (embedded columnar; bundled build).
bench-duckdb:
	cargo bench --manifest-path $(BENCH) --bench duckdb_bench

## Benchmark PostgreSQL only. Spins an EPHEMERAL cluster from the devbox-provided
## `postgresql` package (no binary download), runs the suite over a unix socket,
## and tears it down. Requires devbox (declarative host deps — see devbox.json).
bench-postgres:
	devbox run -- benchmarks/scripts/pg_run.sh

## Benchmark the JS/Bun suite: PGlite (Postgres WASM, in-process) vs bun:sqlite.
bench-pglite:
	($(BUN) install --cwd benchmarks/js && $(BUN) run --cwd benchmarks/js bench.ts)

## On-disk footprint report (scenario 18): bytes-per-corpus for ForgeDB / SQLite /
## redb / DuckDB + ForgeDB update-churn bloat before/after compaction. A size
## report (not a Criterion timing) — an example so it can use the bench dev-deps.
bench-footprint:
	cargo run --manifest-path $(BENCH) --example footprint --release

## Concurrency report (scenario 16): ForgeDB reader throughput under a live writer
## (#56-B lock-free reads) at 1/2/4/8 reader threads, with and without a writer.
bench-concurrency:
	cargo run --manifest-path $(BENCH) --example concurrency --release

## Config-matrix bench (epic #126): same scenarios across generated config variants.
bench-matrix:
	cargo bench --manifest-path $(BENCH) --bench matrix_bench

## Re-emit benchmarks/gen/database.rs from bench.forge through the current CLI.
## Run this after any codegen change so the bench links current generated output.
bench-regen:
	cargo run -- generate rust --schema benchmarks/bench.forge --output benchmarks/gen --force

## Re-emit every matrix config variant (benchmarks/gen/<variant>/database.rs) from
## bench.forge under its benchmarks/configs/<variant>.toml. Run after codegen changes.
bench-regen-matrix:
	@for v in $(BENCH_VARIANTS); do \
		echo "regen $$v"; \
		cargo run -q -- generate rust --force \
			--config benchmarks/configs/$$v.toml \
			--schema benchmarks/bench.forge \
			--output benchmarks/gen/$$v || exit 1; \
	done

## Install the inspector app's JS dependencies.
inspector-install:
	cd $(INSPECTOR) && $(BUN) install

## Run the inspector frontend in a browser (web-first dev; no desktop shell).
inspector:
	cd $(INSPECTOR) && $(BUN) run dev

## Build the inspector frontend to a static export (apps/inspector/out).
inspector-build:
	cd $(INSPECTOR) && $(BUN) run build

## Typecheck the inspector frontend.
inspector-typecheck:
	cd $(INSPECTOR) && $(BUN) run typecheck

## Run the inspector as a Tauri desktop app. Uses the local `@tauri-apps/cli`
## devDep (via `bun run tauri`) — no global `cargo install tauri-cli` needed.
## Tauri runs `beforeDevCommand` (bun run dev) from apps/inspector automatically.
inspector-app:
	cd $(INSPECTOR) && $(BUN) run tauri dev

## Build the inspector desktop app for release (bundles the static export).
inspector-app-build:
	cd $(INSPECTOR) && $(BUN) run tauri build

## Install the marketing + docs website's JS dependencies.
website-install:
	cd $(WEBSITE) && $(BUN) install

## Run the website in dev mode (http://localhost:3100).
website:
	cd $(WEBSITE) && $(BUN) run dev

## Build the website to a static export (apps/website/out; host-agnostic).
website-build:
	cd $(WEBSITE) && $(BUN) run build

## Typecheck the website.
website-typecheck:
	cd $(WEBSITE) && $(BUN) run typecheck

## Install the VS Code extension's JS dependencies.
extension-install:
	cd $(EXTENSION) && $(BUN) install

## Compile the VS Code extension (tsc -> out/extension.js).
extension-build:
	cd $(EXTENSION) && $(BUN) install && $(BUN) run compile

## Typecheck the VS Code extension without emitting.
extension-typecheck:
	cd $(EXTENSION) && $(BUN) install && $(BUN) x tsc --noEmit -p ./

## Package the VS Code extension into an installable .vsix at the repo root
## (compiles first via vsce's prepublish hook; bundles production deps).
extension-package:
	cd $(EXTENSION) && $(BUN) install && $(BUN) run package
	@echo "Packaged: $(EXTENSION)/forgedb-*.vsix"
