# ForgeDB v1 Roadmap — "close the core, then ship"

Status: planning · Decided 2026-07-10 · Tracking epics: [#89](https://github.com/hoodiecollin/forgedb/issues/89) · [#90](https://github.com/hoodiecollin/forgedb/issues/90) · [#91](https://github.com/hoodiecollin/forgedb/issues/91) · [#92](https://github.com/hoodiecollin/forgedb/issues/92) · [#93](https://github.com/hoodiecollin/forgedb/issues/93)

## The situation

ForgeDB has been built **perimeter-first**. The advanced features are real and impressive — live queries, multi-tenancy, snapshot isolation, backup/restore, single-writer/many-reader. But they sit on top of a core that cannot yet durably store, cannot list, cannot query by anything but primary key, and enforces no constraints. The sophistication is at the edges; the middle has holes.

The good news: **almost every gap is a codegen-wiring problem, and codegen is ForgeDB's core competency.** The hard substrate already exists (columnar storage with flush primitives, a complete `wal` crate, a working compaction engine, an unused `query-params` crate). Most of the work is emitting generated code that calls substrate that is already there — squarely inside the generator-identity model.

## Scope (locked)

| Axis | Decision |
|---|---|
| **Release bar** | Design-partner / early-adopter — public but explicitly caveated. Tiers 0–1 must fully close; a targeted subset of Tier 2/3. |
| **Concurrency** | Single-writer-per-process, documented, with a safe two-writer guard (lock/refuse, never corrupt). |
| **Migrations** | Additive changes work end-to-end + a documented dump→regenerate→reload path for breaking changes. |

### Explicitly out of v1 (state it out loud in the docs)

Multi-writer / Direction C · migrations data-transform engine · PITR / incremental backup · row-level authz (#72) · JWT issuance (#73) · cross-process broker.

## Current-state gap map

Established by four code probes on 2026-07-10 (see the `core-gaps-vs-claudemd` project memory). ✅ solid · 🟡 partial · 🔴 missing/stub.

| Tier | Requirement | State | Evidence |
|---|---|---|---|
| **0** Data you can trust | Crash-safe durability | ✅ | #89 LANDED: WAL commit + fsync + reopen recovery + single-writer lock (`kill -9` E2E: 0 acked rows lost) |
| | Bounded storage | ✅ | #96 (WAL) + #92 W1 (columns): generated in-process auto-compaction under the writer lock reclaims #66 dead versions past a threshold (keep-set substrate primitive); `forgedb-compaction 0.1.0` published + reclose proven |
| | Schema evolution w/o data loss | ✅ | #92 W2–W4: additive fields backfill on reopen (no wipe), `migrate --auto` gates additive-vs-breaking, breaking → documented+tested dump→reload path. Data-transform engine deferred (out of v1) |
| | Constraint integrity (unique/FK) | ✅ | #91 LANDED: `&unique` (via Phase-2 index) + required/optional-FK existence (Database-level wrappers) rejected at write → 409 |
| **1** Real-app capability | Query surface (filter/sort/paginate) | ✅ | #90 LANDED: real list endpoint filters (generated closed-set matcher) + sorts (generated comparator) + paginates (`query-params` substrate) |
| | Indexed lookups | ✅ | #90 LANDED + #100–#103 follow-ups LANDED: scalar / **nullable** / **FK** / **composite `@index(a,b)`** fields → in-memory `value→{id}` index + `find_by_*`/`get_by_*` O(1) probes (writer live+`_at`, reader `_at`); reverse-relation getters now probe, not scan |
| | Validation enforced | ✅ | #91 LANDED: `@min`/`@max`/`@length`/`@email`/`@url` enforced in generated `validate_<model>` → 422 (`@pattern`/`@regex` deferred → #104) |
| | Types apps need (enum/decimal/json) | 🔴 | missing from the schema language |
| | Delete semantics (cascade / M2M unlink) | 🟡 | delete exists; no cascade, no unlink |

*(Tier 2/3 — observability, deploy, docs, distribution, SDK completeness, semver — not yet probed; they come into focus once the core closes.)*

## The roadmap

Ordered by "what makes it a database at all," not by what is most visible.

### Phase 1 — Durable write path · [#89](https://github.com/hoodiecollin/forgedb/issues/89) · *Tier 0 · the blocker* · ✅ COMPLETE
Wire WAL → flush/fsync → recovery-on-open into the generated write path; add the single-writer lock. **Done when** a `kill -9` writer-stress test shows zero lost/corrupted committed rows and a second writer is safely refused.
- **Step 1 (#89):** WAL commit boundary + fsync + reopen recovery (torn-tail truncate + idempotent replay) + `DirLock` single-writer. `kill -9` E2E: 0 acked rows lost.
- **Step 2 (#96):** bound the WAL — generated `checkpoint()` (fsync columns → truncate WAL) auto-invoked past a fixed interval; reopen no longer replays the whole history. E2E: WAL sawtoothed/bounded under sustained writes; 23 rows recovered from a 309-byte WAL.
- **Published (2026-07-10):** `forgedb-wal 0.2.0` + `forgedb-storage 0.1.5`; the reclose is proven by an outside-repo `init → generate → cargo build` resolving them from crates.io. Phase 1 is fully closed.

### Phase 2 — Readable database · [#90](https://github.com/hoodiecollin/forgedb/issues/90) · *Tier 1 · co-critical* · ✅ COMPLETE
Real list endpoint; filter/sort/pagination (wired `query-params`); generated secondary indexes + `find_by_*`. **Done when** list+filter+sort+paginate work over a real schema and an indexed lookup is a probe, not a scan.
- **List endpoint:** `all()` + generated closed-set filter (`<model>_event_matches`, reused — no second predicate parser) + generated per-model sort comparator + `query-params` `Pagination` (clamped). Response `{data,total,limit,offset}`.
- **Secondary indexes:** per `^index`/`&unique` scalar, an in-memory `value→{id}` map maintained after the #89 commit boundary (insert/update/delete, superseding-version aware) and rebuilt into the reopen id-scan; `find_by_*`/`get_by_*` (live) + `_at` (snapshot-version-resolving, post-filtered) probes — an index `get`, not a scan.
- **Proven E2E** through current codegen (`scratchpad/phase2_compile`: axum-router list filter/sort/paginate + probe/snapshot/reopen; `scratchpad/corpus_check`: full `examples/` corpus compiles). Guards `test_api_generation_list_endpoint`, `test_rust_generation_secondary_indexes`.
- **Publish gap CLOSED:** generated `api.rs` links `forgedb-query-params` **0.1.0 (published 2026-07-10)**; reclose proven by an outside-repo `init → generate → cargo build` resolving it (+ storage 0.1.5 / wal 0.2.0 / changefeed / auth / types) from crates.io.
- **Index follow-ups #100–#103 LANDED (2026-07-10):** FK-scalar indexing + reverse-getter probe (#100), composite `@index(a,b)` (#101), nullable-field indexing with a null-distinct key (#102), and `DatabaseReader` snapshot probes (#103). Pure generated code over existing storage — **no new substrate, no publish gap reopened.** Proven E2E in `scratchpad/followups_compile` + full-corpus compile in `scratchpad/corpus_check2`; guard `test_rust_generation_index_followups`.

### Phase 3 — Data integrity · [#91](https://github.com/hoodiecollin/forgedb/issues/91) · *Tier 0 constraints + Tier 1 validation* · ✅ COMPLETE
Enforce `&unique` (rides on Phase 2's index), required-FK existence, and validation directives at write + API boundary. **Done when** duplicate-unique, dangling-FK, and invalid-field writes are all rejected with clear errors.
- **Field constraints:** generated `validate_<model>` enforces `@min`/`@max`/`@length`/`@email`/`@url` at the top of `insert`/`update` (nullable → only when `Some`). `@pattern`/`@regex` deferred (need a `regex` dep) → #104.
- **`&unique`:** `insert`/`update` probe the Phase-2 unique index before committing (insert: any hit; update: a *different* id's hit) — self-contained in `Storage`.
- **Foreign-key existence:** generated `Database::create_<model>`/`update_<model>` wrappers verify each required/optional FK resolves via `self.<target>.get(fk)` (sibling access), then delegate. The REST boundary routes through them, so Rust API + REST both get full integrity.
- **Signatures/HTTP:** `insert`/`update` return `Result<_, ValidationError>`; `ValidationError::status_code()` → 409 (unique/dangling FK) / 422 (field). **No new substrate or scaffold dep** — pure generated std code, no publish gap. Proven E2E (`scratchpad/phase3_compile`) + full-corpus db+api compile; guard `test_rust_generation_data_integrity`.

> **Coordination:** Phases 1–3 all rewrite generated `insert/update/delete` (and Phase 2 the read path). Do them as **one coordinated core-rework thrust**, not three scattered passes — otherwise the same generated write logic churns three times.
>
> **Sequencing (concrete).** "One thrust" means *co-own the write-path/reopen skeleton, fan out the rest* — not one giant change:
> 1. **#89 first** — land the write-path skeleton: the WAL commit boundary + the reopen recovery scan (WAL replay + torn-tail truncation). This *defines* the two seams everything else hangs off.
> 2. **#90 read surface, in parallel** — the list endpoint (`api.rs:168-171`) + `query-params` wiring is a clean seam that never touches the write body; it can proceed alongside #89.
> 3. **#90 indexes, then** — index maintenance hooks *after* #89's commit boundary; the index rebuild folds *into* #89's reopen scan. Secondary indexes must respect #66's superseding-version append (remove-old/add-new on update, drop on delete) and #56-A's watermark (a `find_by_*` probe must resolve the snapshot's version, not the live newest row). See #90's "Index maintenance constraints."
> 4. **#91 last** — rides #90's unique index.

### Phase 4 — Bounded storage + additive evolution · [#92](https://github.com/hoodiecollin/forgedb/issues/92) · *Tier 0 remainder, scoped* · ✅ COMPLETE
Auto-invoke compaction; make additive migrations backfill existing rows; gate additive-vs-breaking auto-diff; document + test the breaking-change reload path. All four workstreams LANDED + proven E2E.
- **W1 — bounded storage (in-process auto-compaction).** Generated `Storage::compact()`/`Database::compact()` reclaim #66's dead versions under the single-writer lock (checkpoint → keep-set reclaim → reopen/reindex), auto-invoked past `COMPACTION_DEAD_THRESHOLD`. **NOT just wiring:** the tombstone-based engine was misaligned with #66 (reclaimed nothing from updates; *resurrected* deletes) and never compacted generated variable columns (filename mismatch → row scramble). Fix: new schema-agnostic substrate primitive `Compactor::compact_model_keeping(model, live_rows)` (generated code computes the live set) + variable-column-match fix in `compactor.rs`/`stats.rs`. Guard `test_rust_generation_auto_compaction`; E2E `scratchpad/compaction_compile`. **Publish gap CLOSED:** `forgedb-compaction 0.1.0` published 2026-07-11, reclose proven by an outside-repo `init → generate → cargo build`. Offline `forgedb compact`/`vacuum` CLI (tombstone-based, resurrection-prone) is now **REMOVED (#105 resolved by deprecation)** — both mutate nothing and exit non-zero pointing to in-process `Database::compact()`; the substrate `compact_model` fn is doc-deprecated but retained (no publish-gap reopen).
- **W2 — additive backfill.** Generated recovery anchors on the tombstone count and backfills any short (newly-added) column with the field default, instead of the old min-truncation that wiped data on a new empty column. Guard `test_rust_generation_additive_backfill`; E2E `scratchpad/migrate_compile`. Limits: append new fields at the end; non-null backfills to type-zero (not `@default`).
- **W3 — additive-vs-breaking auto-diff gate.** `forgedb migrate create --auto --schema <f>` diffs against a recorded snapshot, accepts additive, refuses breaking with reload guidance + non-zero exit. Wiring over the existing `SchemaDiffer`/`is_breaking()` + a new AST→`SimpleSchema` converter. Integration test `test_migrate_auto_diff_additive_and_breaking_gate`.
- **W4 — breaking-change reload path.** `docs/MIGRATIONS.md` documents dump (`all()`→JSON) → regenerate → reload-with-transform; proven E2E (`scratchpad/reload_compile`, `u32→string` round-trip). Data-transform engine stays out of v1.

**Done when** ✅ storage stays bounded under sustained update/delete, an additive migration preserves existing rows, and the reload path is documented + tested.

### Phase 5 — Ship · [#93](https://github.com/hoodiecollin/forgedb/issues/93) · *targeted Tier 2/3 for the design-partner bar* · ✅ COMPLETE (all six workstreams landed)
Observability, deploy story, docs (incl. an honest "what v1 is / isn't"), distribution, SDK completeness, semver policy. **Done when** a design partner can install, scaffold, deploy, use the typed SDK, and read an honest account of the guarantees and limits.

- **WS1 — Observability. ✅ LANDED.** Generated axum router gains unauthenticated ops routes `/health` (liveness — never touches the DB), `/ready` (acquires a read lock → 200), and `/metrics` (minimal JSON: per-model live row counts + totals, generated by naming each model's storage field). Restructured router so the tenant-auth guard wraps only `__data_routes()`; `__ops_routes()` is merged in *after* the guard so infra probes/scrapers reach them without a JWT. Structured logging via the standard stack — `tracing` + `tracing-subscriber` (env-filter, honors `RUST_LOG`) + a `tower_http::trace::TraceLayer` request span on the router; `FORGEDB_LOG_FORMAT=json` switches to JSON lines. Guard `test_api_generation_observability_endpoints`; E2E: live server serves 200 on all three ops routes + emits JSON logs (`scratchpad/ws_compile`). No new substrate/scaffold-substrate dep (tower-http/tracing are plain crates.io deps) → no publish gap.
- **WS2 — Deploy story. ✅ LANDED.** `forgedb init` now emits a blessed container path: a multi-stage `Dockerfile` (rust-slim build → debian-slim runtime, non-root user, `/data` volume, `FORGEDB_HOST=0.0.0.0`, `HEALTHCHECK` against `/health`), a `.dockerignore`, and a `docker-compose.yml` (named data volume + env config incl. commented tenancy/JWT/JSON-log knobs). Scaffold `main.rs` hardened with graceful shutdown (drains on SIGINT/SIGTERM) + the tracing-subscriber init. Compile-proven through current codegen.
- **WS5 — SDK completeness. ✅ LANDED.** Generated TS SDK rewritten to full CRUD faithful to the real REST contract: `get` (404→null), `list(options)` with pagination/sort/filters returning `ListResult<T> = {data,total,limit,offset}`, `create` (returns new id; throws on 409/422), `update` (whole-record PUT, 404→false), `delete` (204→true/404→false). Typed `ForgeDBError` (status + parsed body) thrown on non-2xx; per-model `<Model>Create = Omit<Model,'id'>` input types. npm-publishable: `forgedb generate` emits `package.json` + `tsconfig.json` alongside `types.ts` (only if absent — never clobbers user edits). Guards in `test_typescript_generation_snapshot`; `tsc --noEmit` (strict) clean; live list returns the `ListResult` shape.
- **WS4 — Distribution. ✅ LANDED.** `cargo install forgedb` works from crates.io — the CLI's full internal crate closure is now published (`forgedb-validation`/`-parser`/`-codegen`/`-migrations`/`-backup`/`-watcher` 0.1.0, joining the already-published substrate; root `forgedb` 0.1.0), each with package metadata + version-pinned path deps, published leaves-first. Proven E2E by an isolated-CARGO_HOME `cargo install forgedb` resolving all 7 from crates.io + running the binary. Prebuilt cross-platform binaries via `.github/workflows/release.yml` (tag → Linux x86_64/aarch64, macOS Intel/ARM, Windows → GitHub Release). `docs/INSTALL.md` documents every install path (`cargo install`, prebuilt, `--git`, from-clone) + the substrate version matrix. **Honest limit:** the release workflow is authored + YAML-validated but not yet exercised by a real tag push (no CI run in-repo).
- **WS6 — Semver / stability policy. ✅ LANDED.** `docs/SEMVER.md` states the compatibility policy across four surfaces (schema language, substrate ABI incl. on-disk format, CLI + `--json` outputs, and an explicit carve-out that the now-published compiler internals are NOT a stable public API) and what a 1.0 commits to. The schema-language additive-vs-breaking boundary agrees with the #92 migration gate.
- **WS3 — Docs (incl. "what v1 is / isn't"). ✅ LANDED.** Four docs, grounded in a real `init → generate → build → serve → curl` e2e run against the published crates: **`docs/GETTING_STARTED.md`** (the full loop, with verified command output), **`docs/SCHEMA.md`** (the parser-verified `.forge` reference — promoted from `docs/proposals/corpus/forge-grammar-reference.md` to a top-level home), **`docs/DEPLOYMENT.md`** (containers, env config, ops routes, multi-tenancy, JWT, single-writer contract), and **`docs/WHAT_V1_IS.md`** (the honest scope: single-writer-per-process contract, additive-only migrations + dump/reload path, verify-only auth — no issuance #73 / no row-level authz #72, plus every deferred limit). README docs index restructured (Start here / Operating / Internals). Done deliberately last so it documents the real shipped results of WS1/2/4/5/6.

## Critical path

```
Phase 1 (#89) ─┐
               ├─→ Phase 3 (#91) ─→ Phase 4 (#92) ─→ Phase 5 (#93)
Phase 2 (#90) ─┘
```

Phases 1 and 2 both gate everything; Phase 3 depends on Phase 2. The two big rocks (multi-writer, migrations data engine) are already deferred, so this is a tractable core-rework followed by a release layer.
