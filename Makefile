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
        bench-list bench-list-postgres bench-deps-check \
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

## #282 scenario 21: the REST list endpoint across five engines and four boundaries.
## The ForgeDB S1-S4 ladder needs the generated router, hence --features router; the other
## engines' S1/S2 arms live in their existing suites and are selected by the Criterion
## positional filter, so this does NOT re-run insert/point-lookup/m2m.
bench-list:
	cargo bench --manifest-path $(BENCH) --features router --bench list_rest_bench
	cargo bench --manifest-path $(BENCH) \
		--bench sqlite_bench --bench redb_bench --bench duckdb_bench -- '/list_'

## The Postgres half of scenario 21, in an ephemeral devbox cluster. PG cannot run
## in-process, so its S1/S2 already carry socket transport the other four do not pay —
## the honest ForgeDB-vs-Postgres comparison is S4. See docs/BENCHMARKS.md.
bench-list-postgres:
	devbox run -- benchmarks/scripts/pg_run.sh '/list_'

## Section 1's gate criterion in benchmarks/src/lib.rs, made executable (#282 BDD-8):
## `gen/api.rs`'s heavy deps must be OFF by default and ON under --features router.
## Nothing else catches a regression here, because a slower build is not a failing build.
bench-deps-check:
	@! cargo tree --manifest-path $(BENCH) -e normal --prefix none | grep -q '^axum ' \
	  || (echo "FAIL: axum is a normal dep without --features router"; exit 1)
	@cargo tree --manifest-path $(BENCH) -e normal --features router --prefix none | grep -q '^axum ' \
	  || (echo "FAIL: --features router does not pull axum"; exit 1)
	@echo "ok: gen/api.rs deps are gated"

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
bench-matrix: bench-regen-matrix
	cargo bench --manifest-path $(BENCH) --bench matrix_bench --features matrix

## Re-emit BOTH tracked generated artifacts in benchmarks/gen/ from bench.forge through
## the current CLI. Run this after any codegen change so the bench links current output.
##
## A loop over (generator, file) pairs rather than N hardcoded commands (#282): the
## bench project now tracks `database.rs` AND `api.rs`, they come off two DIFFERENT
## emitters (crates/codegen/src/rust.rs and .../api.rs), and a change touching only one
## of them still has to re-emit through here. One hook, so a future regenerate-and-diff
## guard (#285) has a single place to attach.
bench-regen:
	@for g in rust api; do \
		echo "regen $$g"; \
		cargo run -q -- generate $$g --schema benchmarks/bench.forge \
			--output benchmarks/gen --force || exit 1; \
	done

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
## Run at release time BEFORE tagging, with VERSION set: the tag must point AT the
## `chore(release)` commit that carries the changelog, because cargo-dist builds the
## GitHub Release body from the section in the TAGGED tree. Passing `--tag` is what
## lets the not-yet-created version head its own section instead of `[Unreleased]`.
## The website renders the same file at /changelog.
## Needs git-cliff on PATH (`brew install git-cliff`).
##   make changelog VERSION=v0.4.0
changelog:
	git-cliff --config cliff.toml $(if $(VERSION),--tag $(VERSION),) --output CHANGELOG.md
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

.PHONY: test test-ignored

## TIER 1 — the default suite. What every PR is gated on (.github/workflows/test.yml).
##
## `--no-fail-fast` is not decoration: without it cargo halts at the FIRST failing test
## binary, so one break hides every result behind it and a run reports a single failure
## when there may be twenty.
##
## The examples build is not optional either. `--lib`, `--bins`, `--tests` AND `--doc`
## all EXCLUDE examples, so nothing in the test command compiles them — which has
## silently broken the tree twice. It is cheap; it stays.
test:
	$(MAKE) goguard
	cargo test --workspace --no-fail-fast
	cargo build --workspace --examples

## The Go half of the AST guards (#388). `crates/source-guard` builds this on demand at
## first use, so `make test` would work without invoking it — it runs first so the Go
## toolchain is proven present BEFORE a hundred Rust tests do, and so a missing toolchain
## is named directly instead of surfacing inside an unrelated codegen test.
##
## Invoked as a RECIPE LINE rather than declared as a prerequisite of `test:`, because
## tests/ci_gate_test.rs parses the Makefile for a literal `test:` target and a
## `test: goguard` header makes it report "Makefile has no `test:` target". Keeping the
## guard's parser working matters more than the tidier spelling.
##
## Stdlib-only, so there is no module download and no go.sum to keep in step.
goguard:
	cd tools/goguard && go build -o ../../target/goguard/goguard .

## Vet + test the Go helper itself. Not part of `test`: it is the guard's own guard, and
## it runs `go test`, which the Rust suite has no reason to invoke.
goguard-check:
	cd tools/goguard && gofmt -l . && go vet ./... && go test ./...

## TIER 2 — the ignored suite (#390). Every test that is `#[ignore]`d out of tier 1
## because it generates and compiles a crate. Minutes, not seconds. Run nightly by
## .github/workflows/nightly-ignored.yml, which invokes THIS target so the command has
## exactly one definition.
##
## ONE WORKSPACE-LEVEL INVOCATION, NEVER A LOOP OVER `--test <name>`.
##
## That is the whole point of this target and it is load-bearing. The obvious way to
## write it is a loop over the ten per-scenario targets below — and that form looks
## complete while covering 13 of the 20 ignored tests. It silently drops all four in
## build_cache_compile_test plus one each from pyo3_component_compile_test,
## placement_flip_test and migrate_tests, because those four files have no target at
## all. `--workspace` needs no list and so cannot have a stale one.
##
## (`cargo test -- --ignored --list` reports 21, not 20. The extra is a `rust,ignore`
## doc-block in crates/codegen/src/rust.rs: under `--ignored` it reports ok in 0.00s
## compiling nothing. It is a vacuous entry — harmless to include, and evidence of
## nothing. Do not cite it as a reason for anything.)
##
## migrate_tests is SKIPPED here and runs on `main` instead (substrate-reclose.yml).
## It compiles a real transformer against the PUBLISHED substrate, so on `develop` —
## which is allowed to carry a publish gap by design — it fails for a reason that has
## nothing to do with the commit under test. A job that is legitimately red for most of
## every cycle stops being read, which is the failure this whole issue exists to remove.
## tests/ci_gate_test.rs asserts this skip still matches exactly one real test: if the
## test is renamed the pattern matches NOTHING and migrate_tests quietly rejoins the
## nightly, red every cycle, for a correct reason.
test-ignored:
	cargo test --workspace --no-fail-fast -- --ignored \
		--skip test_migrate_build_reports_the_path_cargo_actually_wrote

.PHONY: crash-test

## End-to-end crash-recovery proof (#16): generate + compile real database code,
## insert rows, abort the process uncleanly, and assert committed rows survive the
## reopen (plus torn-WAL-tail resilience). #[ignore]d out of the fast default
## suite because it compiles a generated crate — run it explicitly here.
crash-test:
	cargo test --test crash_recovery_test -- --ignored --nocapture

.PHONY: index-test

## Index contract, end to end: generate a model carrying every indexable type in both
## its plain and nullable form (plus both FK forms), compile it, store a row per field
## and resolve each one back through the REAL generated `find_by_*`. Proves the record
## and probe sides of the emitted key agree, that distinct values do not collide, and
## that the null bucket stays distinct from the literal string "null" (#102).
## Byte-comparison against the pre-#230 key form used to live here and was removed
## (#381) — see the file header for why it should not come back.
## #[ignore]d out of the fast suite (compiles a crate).
index-test:
	cargo test --test index_test -- --ignored --nocapture

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
## the list path against silent wire changes from #226/#228. Two tests, so two
## generated crates: #229's baseline, plus #226's list-page guard over a schema
## carrying a nullable string / decimal / enum / timestamp / bytes(N) / [T; N] /
## inline struct / required FK / virtual [Model], a model whose identity field is
## declared SECOND, and a churned model whose live rows are sparse enough to send
## `gather_buffered` down `gather_sparse`. Also #[ignore]d out of the fast suite.
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

.PHONY: list-wire-test

## Benchmark-harness fidelity (#282 BDD-1/BDD-5): scenario 21's S1/S2 arms call the
## generated page scope directly and supply the filter, comparator and index selection BY
## HAND, mirroring what the handler derives from a query string. A mirror that admits a row
## the handler rejects makes the whole ladder's subtractions meaningless while every arm
## still runs and every number still looks plausible. This rebuilds the envelope both ways
## over a freshly generated crate and compares the bytes, for all four shapes.
##
## The bench asserts the same thing in-run, which is stronger — but only when someone runs
## the bench, and no baseline or CI job does. Also #[ignore]d out of the fast suite.
list-wire-test:
	cargo test --test list_wire_parity_test -- --ignored --nocapture

.PHONY: page-identity-test

## Page-construction-site identity (#281): #281 adds a SECOND place that builds a
## `<Model>PageRef` — `__with_fast_page`, which skips the scan for an unfiltered,
## unsorted request — and its whole contract is that it is indistinguishable from
## `__with_page`. That is what makes it safe and what makes it hard to test: no wire
## test can tell the two apart. So this compares them against EACH OTHER, byte for
## byte, over a grid of (offset, limit) windows on a churned 1,000+-row corpus of
## every field class. A frozen literal catches a change that moves both sites; only
## this catches one that moves one. #[ignore]d out of the fast suite (compiles and
## runs a crate).
page-identity-test:
	cargo test --test page_identity_test -- --ignored --nocapture

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

.PHONY: enum-remap-test

## Enum discriminant positionality (#438): an enum is stored as a 1-byte discriminant
## keyed by DECLARATION ORDER, so reordering two variants re-maps every already-stored
## row while changing no byte on disk. Nothing that compares generated code as strings
## can see that — a snapshot shows the two match arms swapping, which is the change the
## author intended. So this generates and compiles TWO crates over ONE data directory:
## one writes a row, the other reads it back through the reordered schema. It also
## proves the other half — that the transformer's JSON-by-name round trip repairs it,
## which is the entire justification for classifying a reorder `Auto` rather than
## `Authored`. #[ignore]d out of the fast suite (compiles two crates).
enum-remap-test:
	cargo test --test enum_discriminant_remap_test -- --ignored --nocapture

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
