# ForgeDB — root entry points. Everything runnable from the repo root; no `cd`.
# Rust workflows use cargo directly (see CLAUDE.md). JS/desktop workflows for the
# inspector app are wrapped here so they never require cd-ing into apps/inspector.

BUN := /Users/collin/.bun/bin/bun
INSPECTOR := apps/inspector
BENCH := benchmarks/Cargo.toml

## Config variants for the matrix bench (epic #126): each becomes a generated
## module under benchmarks/gen/<name>/. Keep in sync with benchmarks/configs/.
BENCH_VARIANTS := default fsync_never replication_on compaction_off compaction_low changefeed_small

.PHONY: inspector-install inspector inspector-build inspector-typecheck \
        inspector-app inspector-app-build \
        bench bench-forgedb bench-sqlite bench-redb bench-matrix bench-regen bench-regen-matrix

## Run every implemented benchmark suite (ForgeDB + SQLite). See docs/BENCHMARKS.md.
bench:
	cargo bench --manifest-path $(BENCH)

## Benchmark the ForgeDB generated code only.
bench-forgedb:
	cargo bench --manifest-path $(BENCH) --bench forgedb_bench

## Benchmark SQLite only.
bench-sqlite:
	cargo bench --manifest-path $(BENCH) --bench sqlite_bench

## Benchmark redb only (pure-Rust embedded KV).
bench-redb:
	cargo bench --manifest-path $(BENCH) --bench redb_bench

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
