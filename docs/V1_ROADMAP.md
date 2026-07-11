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
| | Bounded storage | 🟡 | **WAL** bounded (#96 checkpoint LANDED); **column** storage still manual-compaction-only → grows until `forgedb compact` |
| | Schema evolution w/o data loss | 🔴 | infra only; no data-transform engine; `AddField` doesn't backfill |
| | Constraint integrity (unique/FK) | 🔴 | unchecked at write |
| **1** Real-app capability | Query surface (filter/sort/paginate) | ✅ | #90 LANDED: real list endpoint filters (generated closed-set matcher) + sorts (generated comparator) + paginates (`query-params` substrate) |
| | Indexed lookups | ✅ | #90 LANDED + #100–#103 follow-ups LANDED: scalar / **nullable** / **FK** / **composite `@index(a,b)`** fields → in-memory `value→{id}` index + `find_by_*`/`get_by_*` O(1) probes (writer live+`_at`, reader `_at`); reverse-relation getters now probe, not scan |
| | Validation enforced | 🔴 | `@min`/`@email`/… ignored at write |
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

### Phase 3 — Data integrity · [#91](https://github.com/hoodiecollin/forgedb/issues/91) · *Tier 0 constraints + Tier 1 validation*
Enforce `&unique` (rides on Phase 2's index), required-FK existence, and validation directives at write + API boundary. **Done when** duplicate-unique, dangling-FK, and invalid-field writes are all rejected with clear errors.

> **Coordination:** Phases 1–3 all rewrite generated `insert/update/delete` (and Phase 2 the read path). Do them as **one coordinated core-rework thrust**, not three scattered passes — otherwise the same generated write logic churns three times.
>
> **Sequencing (concrete).** "One thrust" means *co-own the write-path/reopen skeleton, fan out the rest* — not one giant change:
> 1. **#89 first** — land the write-path skeleton: the WAL commit boundary + the reopen recovery scan (WAL replay + torn-tail truncation). This *defines* the two seams everything else hangs off.
> 2. **#90 read surface, in parallel** — the list endpoint (`api.rs:168-171`) + `query-params` wiring is a clean seam that never touches the write body; it can proceed alongside #89.
> 3. **#90 indexes, then** — index maintenance hooks *after* #89's commit boundary; the index rebuild folds *into* #89's reopen scan. Secondary indexes must respect #66's superseding-version append (remove-old/add-new on update, drop on delete) and #56-A's watermark (a `find_by_*` probe must resolve the snapshot's version, not the live newest row). See #90's "Index maintenance constraints."
> 4. **#91 last** — rides #90's unique index.

### Phase 4 — Bounded storage + additive evolution · [#92](https://github.com/hoodiecollin/forgedb/issues/92) · *Tier 0 remainder, scoped*
Auto-invoke compaction (engine exists — this is wiring); make additive migrations backfill existing rows; document + test the breaking-change reload path. **Done when** storage stays bounded under sustained update/delete, an additive migration preserves existing rows, and the reload path is documented.

### Phase 5 — Ship · [#93](https://github.com/hoodiecollin/forgedb/issues/93) · *targeted Tier 2/3 for the design-partner bar*
Observability, deploy story, docs (incl. an honest "what v1 is / isn't"), distribution, SDK completeness, semver policy. **Done when** a design partner can install, scaffold, deploy, use the typed SDK, and read an honest account of the guarantees and limits.

## Critical path

```
Phase 1 (#89) ─┐
               ├─→ Phase 3 (#91) ─→ Phase 4 (#92) ─→ Phase 5 (#93)
Phase 2 (#90) ─┘
```

Phases 1 and 2 both gate everything; Phase 3 depends on Phase 2. The two big rocks (multi-writer, migrations data engine) are already deferred, so this is a tractable core-rework followed by a release layer.
