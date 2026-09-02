BUN := $(shell command -v bun 2>/dev/null || echo "$(HOME)/.bun/bin/bun")
INSPECTOR := apps/inspector
WEBSITE := apps/website
EXTENSION := apps/vscode-forgedb
BENCH := benchmarks/Cargo.toml

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

bench:
	cargo bench --manifest-path $(BENCH) \
		--bench forgedb_bench --bench sqlite_bench --bench redb_bench --bench duckdb_bench

bench-forgedb:
	cargo bench --manifest-path $(BENCH) --bench forgedb_bench

bench-list-page:
	cargo bench --manifest-path $(BENCH) --bench list_page_bench

bench-list:
	cargo bench --manifest-path $(BENCH) --features router --bench list_rest_bench
	cargo bench --manifest-path $(BENCH) \
		--bench sqlite_bench --bench redb_bench --bench duckdb_bench -- '/list_'

bench-list-postgres:
	devbox run -- benchmarks/scripts/pg_run.sh '/list_'

bench-deps-check:
	@! cargo tree --manifest-path $(BENCH) -e normal --prefix none | grep -q '^axum ' \
	  || (echo "FAIL: axum is a normal dep without --features router"; exit 1)
	@cargo tree --manifest-path $(BENCH) -e normal --features router --prefix none | grep -q '^axum ' \
	  || (echo "FAIL: --features router does not pull axum"; exit 1)
	@echo "ok: gen/api.rs deps are gated"

bench-sqlite:
	cargo bench --manifest-path $(BENCH) --bench sqlite_bench

bench-redb:
	cargo bench --manifest-path $(BENCH) --bench redb_bench

bench-duckdb:
	cargo bench --manifest-path $(BENCH) --bench duckdb_bench

bench-postgres:
	devbox run -- benchmarks/scripts/pg_run.sh

bench-pglite:
	($(BUN) install --cwd benchmarks/js && $(BUN) run --cwd benchmarks/js bench.ts)

bench-footprint:
	cargo run --manifest-path $(BENCH) --example footprint --release

bench-concurrency:
	cargo run --manifest-path $(BENCH) --example concurrency --release

bench-workload:
	cargo run --manifest-path $(BENCH) --example workload --release -- $(ARGS)

bench-workload-var: bench-regen-matrix
	cargo run --manifest-path $(BENCH) --example workload --release --features matrix -- \
		--var-sweep $(ARGS)

bench-matrix: bench-regen-matrix
	cargo bench --manifest-path $(BENCH) --bench matrix_bench --features matrix

bench-regen:
	@for g in rust api; do \
		echo "regen $$g"; \
		cargo run -q -- generate $$g --schema benchmarks/bench.forge \
			--output benchmarks/gen --force || exit 1; \
	done

bench-regen-matrix:
	@for v in $(BENCH_VARIANTS); do \
		echo "regen $$v"; \
		cargo run -q -- generate rust --force \
			--config benchmarks/configs/$$v.toml \
			--schema benchmarks/bench.forge \
			--output benchmarks/gen/$$v || exit 1; \
	done

inspector-install:
	cd $(INSPECTOR) && $(BUN) install

inspector:
	cd $(INSPECTOR) && $(BUN) run dev

inspector-build:
	cd $(INSPECTOR) && $(BUN) run build

inspector-typecheck:
	cd $(INSPECTOR) && $(BUN) run typecheck

inspector-app:
	cd $(INSPECTOR) && $(BUN) run tauri dev

inspector-app-build:
	cd $(INSPECTOR) && $(BUN) run tauri build

website-install:
	cd $(WEBSITE) && $(BUN) install

website:
	cd $(WEBSITE) && $(BUN) run dev

website-build:
	cd $(WEBSITE) && $(BUN) scripts/website-build.ts

website-typecheck:
	cd $(WEBSITE) && $(BUN) run typecheck

changelog:
	git-cliff --config cliff.toml $(if $(VERSION),--tag $(VERSION),) --output CHANGELOG.md
	@echo "✓ CHANGELOG.md regenerated — review the diff before committing."

roadmap:
	cd $(WEBSITE) && $(BUN) run roadmap

website-rewrite:
	cd $(WEBSITE) && $(BUN) run rewrite:dev

website-rewrite-watch:
	cd $(WEBSITE) && $(BUN) run rewrite:watch

website-secrets:
	@op read "op://Private/forgedb.dev deploy/posthog project key" | gh secret set NEXT_PUBLIC_POSTHOG_KEY
	@op read "op://Private/forgedb.dev deploy/vercel token"        | gh secret set VERCEL_TOKEN
	@op read "op://Private/forgedb.dev deploy/vercel org id"       | gh secret set VERCEL_ORG_ID
	@op read "op://Private/forgedb.dev deploy/vercel project id"   | gh secret set VERCEL_PROJECT_ID
	@echo "✓ Pushed NEXT_PUBLIC_POSTHOG_KEY, VERCEL_TOKEN, VERCEL_ORG_ID, VERCEL_PROJECT_ID to GitHub Actions"

extension-install:
	cd $(EXTENSION) && $(BUN) install

extension-build:
	cd $(EXTENSION) && $(BUN) install && $(BUN) run compile

extension-typecheck:
	cd $(EXTENSION) && $(BUN) install && $(BUN) x tsc --noEmit -p ./

extension-package:
	cd $(EXTENSION) && $(BUN) install && $(BUN) run package
	@echo "Packaged: $(EXTENSION)/forgedb-*.vsix"

.PHONY: test test-ignored comment-check

comment-check:
	@$(BUN) scripts/strip-comments.ts --check

test:
	$(MAKE) goguard
	cargo test --workspace --no-fail-fast
	cargo build --workspace --examples

goguard:
	cd tools/goguard && go build -o ../../target/goguard/goguard .

goguard-check:
	cd tools/goguard && gofmt -l . && go vet ./... && go test ./...

test-ignored:
	cargo test --workspace --no-fail-fast -- --ignored \
		--skip test_migrate_build_reports_the_path_cargo_actually_wrote

.PHONY: crash-test

crash-test:
	cargo test --test crash_recovery_test -- --ignored --nocapture

.PHONY: index-test

index-test:
	cargo test --test index_test -- --ignored --nocapture

.PHONY: oversized-array-test

oversized-array-test:
	cargo test --test oversized_array_test -- --ignored --nocapture

.PHONY: f64-index-key

f64-index-key:
	cargo test --test f64_index_key_test -- --ignored --nocapture

.PHONY: api-wire-test

api-wire-test:
	cargo test --test api_wire_test -- --ignored --nocapture

.PHONY: cors-test

cors-test:
	cargo test --test cors_test -- --ignored --nocapture

.PHONY: list-scan-test

list-scan-test:
	cargo test --test list_scan_test -- --ignored --nocapture

.PHONY: list-wire-test

list-wire-test:
	cargo test --test list_wire_parity_test -- --ignored --nocapture

.PHONY: page-identity-test

page-identity-test:
	cargo test --test page_identity_test -- --ignored --nocapture

.PHONY: auto-increment-test

auto-increment-test:
	cargo test --test auto_increment_test -- --ignored --nocapture
	cargo test --test auto_increment_coordinated_test -- --ignored --nocapture
	cargo test --test sequence_claim_test -- --ignored --nocapture

.PHONY: enum-remap-test

enum-remap-test:
	cargo test --test enum_discriminant_remap_test -- --ignored --nocapture

.PHONY: scripts-typecheck

scripts-typecheck:
	@$(BUN) install --cwd scripts
	@(cd scripts && $(BUN) x tsc --noEmit)

.PHONY: cycle-scope

cycle-scope:
ifdef PR
	@$(BUN) scripts/check-cycle-scope.ts --pr $(PR)
else ifdef ISSUE
	@$(BUN) scripts/check-cycle-scope.ts --issue $(ISSUE)
else
	@echo "usage: make cycle-scope ISSUE=<n[,n...]>   (or PR=<n>)"; exit 2
endif

.PHONY: experiment-261

experiment-261:
	@(cd benchmarks/experiments/261 && \
	  mkdir -p results && \
	  cargo run --release -- target/data > results/raw.json && \
	  $(BUN) plot.ts && \
	  $(BUN) svg2png.ts results/grid.svg results/grid.png && \
	  $(BUN) svg2png.ts results/summary.svg results/summary.png)
