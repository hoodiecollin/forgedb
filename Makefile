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
BENCH_VARIANTS := default fsync_never replication_on compaction_off compaction_low changefeed_small churn_probe

.PHONY: inspector-install inspector inspector-build inspector-typecheck \
        inspector-app inspector-app-build \
        website-install website website-build website-typecheck website-secrets \
        website-rewrite website-rewrite-watch changelog roadmap \
        extension-install extension-build extension-typecheck extension-package \
        bench bench-forgedb bench-sqlite bench-redb bench-duckdb bench-postgres \
        bench-pglite bench-matrix bench-regen bench-regen-matrix bench-list-page \
        bench-footprint bench-concurrency bench-workload bench-workload-var

## Run the embedded comparison suites that need no setup (ForgeDB + SQLite + redb +
## DuckDB). PostgreSQL (needs a cluster), the config matrix (needs regen), the #226
## list-page kill gate, and the JS/PGlite suite are separate targets below, so this
## names its four benches instead of letting `cargo bench` select everything. The
## matrix bench is no longer a reason to hand-list them: its gitignored variant modules
## sit behind `--features matrix` (#279), so cargo SKIPS that target when the feature is
## off rather than failing to compile the library. See docs/BENCHMARKS.md.
bench:
	cargo bench --manifest-path $(BENCH) \
		--bench forgedb_bench --bench sqlite_bench --bench redb_bench --bench duckdb_bench

## Benchmark the ForgeDB generated code only.
bench-forgedb:
	cargo bench --manifest-path $(BENCH) --bench forgedb_bench

## #226 kill gate: split a list request into scan / page-materialize / serialize and
## report the page-materialize share. #226 can only remove that share, so it is a hard
## ceiling on the win — measurable without prototyping the buffered decode.
bench-list-page:
	cargo bench --manifest-path $(BENCH) --bench list_page_bench

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

## Mixed-workload driver (scenario 20, #218 under experiment #167): sustained
## read/create/update/delete/scan at phased arrival rates (warmup -> steady -> burst
## -> recover) across an amplification ladder, vs SQLite + redb at matched durability.
## NOT a Criterion bench: Criterion's closed loop cannot express burstiness or
## queueing delay, which is exactly what the append-only tax shows up as.
##   make bench-workload                       # quick smoke matrix
##   make bench-workload ARGS="--full"         # full ladder (A = 1..32)
##   make bench-workload ARGS="--forgedb-only" # skip the comparison engines
##   make bench-workload ARGS="--scan-sweep"   # fixed-width scan path (Metric subject)
##   make bench-workload ARGS="--verify"       # driver self-checks
## The variable-width scan path (--var-sweep) runs against the gitignored churn_probe
## variant, so it has its own target below rather than an ARGS mode.
bench-workload:
	cargo run --manifest-path $(BENCH) --example workload --release -- $(ARGS)

## Variable-width scan sweep (Doc subject): the one workload mode that links a config
## variant (churn_probe — compaction off, so amplification has a lever arm). Regenerates
## the variants and builds with `--features matrix`, which is what makes v_churn_probe
## exist at all (#279). Add ARGS="--full" for the wider ladder.
bench-workload-var: bench-regen-matrix
	cargo run --manifest-path $(BENCH) --example workload --release --features matrix -- \
		--var-sweep $(ARGS)

## Config-matrix bench (epic #126): same scenarios across generated config variants.
## Needs `make bench-regen-matrix` first — the variant modules are gitignored and only
## compile under `--features matrix` (#279).
bench-matrix:
	cargo bench --manifest-path $(BENCH) --bench matrix_bench --features matrix

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

## Build the website to a static export (apps/website/out; host-agnostic). Wrapped
## so a LOCAL build stashes the gitignored dev rewrite route (incompatible with
## output: export) and restores it after — a no-op on CI where the route is absent.
website-build:
	cd $(WEBSITE) && $(BUN) scripts/website-build.ts

## Typecheck the website.
website-typecheck:
	cd $(WEBSITE) && $(BUN) run typecheck

## Regenerate the root CHANGELOG.md from conventional commits (git-cliff, cliff.toml).
## Run at release time AFTER tagging so the new `v*` tag becomes its own section;
## cargo-dist reads this file for the GitHub Release body and the website renders it
## at /changelog. Needs git-cliff on PATH (`brew install git-cliff`).
changelog:
	git-cliff --config cliff.toml --output CHANGELOG.md
	@echo "✓ CHANGELOG.md regenerated — review the diff before committing."

## Rebuild the website's roadmap snapshot (apps/website/public/roadmap.json) from
## live GitHub issues/milestones/releases. Gitignored (build-generated, like the
## search index + changelog); the `prebuild` step runs this on every site build.
## Needs `gh` authenticated (read-only, public issues).
roadmap:
	cd $(WEBSITE) && $(BUN) run roadmap

## LOCAL DEV: one command for the in-browser prose-rewrite loop — brings up the dev
## server (background, reused across cycles) AND the wake watcher. Edit docs prose or
## landing copy with ⌥E in the browser. Run this in Claude Code's session so it wakes
## to draft each proposal. Prefer this over running `website` + `website-rewrite-watch`
## separately. See apps/website/lib/dev/README.md.
website-rewrite:
	cd $(WEBSITE) && $(BUN) run rewrite:dev

## LOCAL DEV: wake watcher only (no dev server). Blocks until the overlay posts a
## rewrite request, then exits so Claude Code can draft the proposal. Use when the dev
## server is already running elsewhere; otherwise prefer `website-rewrite` above.
website-rewrite-watch:
	cd $(WEBSITE) && $(BUN) run rewrite:watch

## Push deploy secrets from 1Password ("Private/forgedb.dev deploy") into the repo's
## GitHub Actions secrets. Each value pipes straight from `op` to `gh` — it is never
## printed, echoed, or stored in a shell variable (this is why it's a direct shell pipe,
## not a TS wrapper: keeping the secret off any intermediate is the safer default here).
## Run once the 1Password item's values are filled in (+ `vercel link` for the two IDs).
website-secrets:
	@op read "op://Private/forgedb.dev deploy/posthog project key" | gh secret set NEXT_PUBLIC_POSTHOG_KEY
	@op read "op://Private/forgedb.dev deploy/vercel token"        | gh secret set VERCEL_TOKEN
	@op read "op://Private/forgedb.dev deploy/vercel org id"       | gh secret set VERCEL_ORG_ID
	@op read "op://Private/forgedb.dev deploy/vercel project id"   | gh secret set VERCEL_PROJECT_ID
	@echo "✓ Pushed NEXT_PUBLIC_POSTHOG_KEY, VERCEL_TOKEN, VERCEL_ORG_ID, VERCEL_PROJECT_ID to GitHub Actions"

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

.PHONY: crash-test

## End-to-end crash-recovery proof (#16): generate + compile real database code,
## insert rows, abort the process uncleanly, and assert committed rows survive the
## reopen (plus torn-WAL-tail resilience). #[ignore]d out of the fast default
## suite because it compiles a generated crate — run it explicitly here.
crash-test:
	cargo test --test crash_recovery_test -- --ignored --nocapture

.PHONY: index-key-parity

## Index-key parity proof (#230): generate a model carrying every indexable type,
## compile it, and assert the monomorphic key emission is byte-identical to the
## `serde_json::Value` form it replaced — plus a round-trip through the real
## generated `find_by_*`. `f64` is the one exception: its legacy key was broken and
## was replaced (#242), so it is asserted against its own contract, not against
## legacy. Also #[ignore]d out of the fast suite (compiles a crate).
index-key-parity:
	cargo test --test index_key_parity_test -- --ignored --nocapture

.PHONY: oversized-array-test

## Oversized-array proof (#243): serde implements `[T; N]` only to N = 32, so a
## `bytes(64)` or `[u32; 40]` field made the derive on the generated struct fail to
## resolve and the whole crate failed to compile. Generates every shape past the
## ceiling (plus an under-ceiling twin for each), compiles it, and round-trips the
## wire form. Also #[ignore]d out of the fast suite (compiles a crate).
oversized-array-test:
	cargo test --test oversized_array_test -- --ignored --nocapture

.PHONY: f64-index-key

## f64 total-order key proof (#242): a non-finite `f64` keyed into the NULL bucket
## (`serde_json::Number::from_f64` returns `None`), so NaN/±Inf were indistinguishable
## from an unset optional — and `^f64` got no ordered index at all, since `f64: !Ord`.
## Generates every f64 index shape (hash, nullable, unique, composite component),
## compiles it, and asserts the IEEE 754 total-order encoding both separates the
## non-finites and orders them. Also #[ignore]d out of the fast suite (compiles a crate).
f64-index-key:
	cargo test --test f64_index_key_test -- --ignored --nocapture

.PHONY: api-wire-test

## REST wire-format proof (#229): generate + compile a real API, boot the generated
## router in-process, and assert the exact response bytes of every read path —
## envelope key order, record key order, projections, and the error bodies. Guards
## the list path against silent wire changes from #226/#228. Also #[ignore]d out of
## the fast suite (compiles a crate).
api-wire-test:
	cargo test --test api_wire_test -- --ignored --nocapture

.PHONY: cors-test

## Cross-origin proof (#140): generate + compile a real API and drive each router
## variant through tower::oneshot. Three of #140's decisions are invisible to a
## snapshot — that an unconfigured router still answers OPTIONS with 405 (omitting
## the layer is NOT the same as emitting an empty one), that a preflight carrying no
## Authorization header is answered 200 rather than 401 (the layer must sit outside
## the tenant guard), and that a WebSocket handshake from a disallowed origin is
## refused 403 (browsers neither preflight nor CORS-enforce a handshake). Also
## #[ignore]d out of the fast suite (compiles a crate).
cors-test:
	cargo test --test cors_test -- --ignored --nocapture

.PHONY: list-scan-test

## List-selection proof (#228): boot the generated router over a CHURNED corpus
## (updates leaving dead versions, deletes leaving holes) and check the ids + `total`
## of ~20 filter/sort/pagination combinations against an independently computed
## oracle. A snapshot compares emitted strings and cannot prove ordering, tie
## behaviour, or which rows survive a filter — this can. Also #[ignore]d out of the
## fast suite (compiles a crate).
list-scan-test:
	cargo test --test list_scan_test -- --ignored --nocapture

.PHONY: auto-increment-test

## Integer auto-increment proof (#187): the `+u32`/`+u64` counter is a value handed
## out over time — across a compaction, a reopen, a rollback, threads, and separate
## processes — so every property it promises is a property of what generated code
## DOES, which a snapshot cannot see. Covers monotonicity, the `0` sentinel, the
## deliberate gap on rollback, restart safety, the compaction high-water mark (the
## one that fails silently), overflow refusal, Tier-2 concurrency, and the Tier-3
## multi-process case that makes RFC #187's identity-or-`&unique` rule meaningful.
## Also #[ignore]d out of the fast suite (compiles a crate; the second spawns
## `forgedb coordinate` plus two writer processes).
auto-increment-test:
	cargo test --test auto_increment_test -- --ignored --nocapture
	cargo test --test auto_increment_coordinated_test -- --ignored --nocapture
	cargo test --test sequence_claim_test -- --ignored --nocapture

.PHONY: scripts-typecheck

## Typecheck the root-level repo tooling in scripts/. The scripts run under bun with no
## install step; the deps are types only, so this is the one thing that needs them.
scripts-typecheck:
	@$(BUN) install --cwd scripts
	@(cd scripts && $(BUN) x tsc --noEmit)

.PHONY: cycle-scope

## Cycle-scope gate: does this work belong in the release cycle currently in flight?
## `develop` carries ONE cycle at a time, so work milestoned for a later version must wait
## on its own branch. The cycle is derived (lowest open v* milestone), never configured.
##
##   make cycle-scope ISSUE=245        before merging a branch locally into develop
##   make cycle-scope ISSUE=233,245    several at once
##   make cycle-scope PR=250           what CI runs on PRs targeting develop
##
## Portable form: ai-pm-playbook §5.3 (PM008/PM009).
cycle-scope:
ifdef PR
	@$(BUN) scripts/check-cycle-scope.ts --pr $(PR)
else ifdef ISSUE
	@$(BUN) scripts/check-cycle-scope.ts --issue $(ISSUE)
else
	@echo "usage: make cycle-scope ISSUE=<n[,n...]>   (or PR=<n>)"; exit 2
endif

.PHONY: experiment-261

## Experiment #261 — inline `string(N)` slot vs pointer indirection, the gate on
## #238's soft form. Runs the full capacity x overflow-length x mix grid, then
## renders the SVG figures and rasterizes them. Detached crate: it measures the
## storage substrate directly and must not pull in benchmarks/'s comparative-DB
## deps. See benchmarks/experiments/261/README.md; the verdict lives on the issue.
experiment-261:
	@(cd benchmarks/experiments/261 && \
	  mkdir -p results && \
	  cargo run --release -- target/data > results/raw.json && \
	  $(BUN) plot.ts && \
	  $(BUN) svg2png.ts results/grid.svg results/grid.png && \
	  $(BUN) svg2png.ts results/summary.svg results/summary.png)
