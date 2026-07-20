# Storage-model experiment — append-only vs in-place, measured

Status: **active focus** · Decided 2026-07-19 · Tracking epic: TBD (filed alongside this doc)

## Why this exists

The benchmark corpus (`docs/BENCHMARKS.md`) makes ForgeDB's storage layer look like the
root cause of every weak number — slow scans (32 ms), point reads behind redb (3.5 µs vs
0.6 µs), reverse-FK full-materialization (37 µs), barrier-bound writes (3.95 ms). The
instinct is "reconsider append-only."

The honest decomposition is that "storage" conflates **four independent decisions**, and
append-only is carrying blame for problems it doesn't cause:

| Axis | Today | Verdict |
|---|---|---|
| **Columnar layout** | columns, positional | **Keep.** Smallest footprint (1.88 MB vs SQLite 4.8, redb 8.5). Scan slowness is *not* inherent to columnar — DuckDB is columnar and wins scans at 100 µs. |
| **Append-only mutation** | superseding-version append + tombstones | **Under test.** Buys the differentiator spine (snapshots, lock-free readers, MVCC, replica, backup, crash-recovery-by-column-length) with no `xmin`/`xmax`. Costs storage growth (→ compaction) + version-resolution indirection. |
| **Durability policy** | fixed `FsyncPolicy::Always` | Orthogonal to layout. The `F_FULLFSYNC` barrier *is* ~3.5 ms of the 3.95 ms write. In-place doesn't change that you fsync. |
| **Generated read-path quality** | full materialization, hashmap-order scan, hash-only index | Not a storage-model question at all. Fixable in codegen + index choice. |

**Where the append-only decision physically lives:** in codegen, not substrate. `id_versions`,
superseding-version append, watermark resolution, and tombstone-delete are emitted by
`crates/codegen/src/rust.rs` (see ~627, ~1064, ~2888, ~3986). The substrate columns
(`crates/storage-native`) are plain positional byte stores. So "try in-place" is a **generated
mutation-model variant** threaded through `GenConfig` — *not* a second storage crate behind the
`crates/storage` cfg facade (that facade swaps the column impl, which is identical for both
mutation models).

## Decision: measure it, don't assert it

Implement in-place as a `GenConfig` codegen variant so the existing benchmark **config matrix**
(`benchmarks/configs/*.toml` + `make bench-regen-matrix && make bench-matrix`, which already
regenerates the same `bench.forge` under each config and lines them up in Criterion) can compile
both variants and aggregate the scenarios side-by-side.

Two decisions locked with the maintainer (2026-07-19):

1. **Sequencing = Phase 1 (confound fixes) first.** If we build in-place fresh we'll
   unconsciously give it a clean scan/materialization path and misattribute the win to
   "in-place." Fixing the confounds in the append-only path first makes both variants share
   read-path quality, so Phase 2 measures the *real* residual cost of the mutation model.
2. **In-place scope = fixed-width only.** Variable-width columns (`string`/`json`) can't be
   overwritten in place (a new value of different length won't fit the slot) — true variable
   in-place is a free-list/heap, i.e. reinventing the row-store machinery append-only let us
   skip. Fixed-width-in-place (id / int / timestamp / enum / decimal / bool) isolates the
   mutation-model cost for the common numeric case without that rewrite. Variable columns stay
   append-with-GC in the in-place variant.

## Identity note (not implicated, stated for the record)

Storage layout is class-1 substrate — schema-agnostic. Changing the mutation model does **not**
touch the generator-identity invariant (schema is a compile-time input; generated code is
per-schema; nothing ships a runtime schema interpreter). The `GenConfig` knob is schema-blind
(guardrail G1) — two different schemas share its value with identical effect. Run the plan past
`forgedb-product-manager` before Phase 2 anyway, since it adds a substrate primitive.

---

## Phase 1 — confound fixes (append-only, strict wins, no feature loss)

These improve the shipping product regardless of the experiment's outcome, and they control the
experiment. Each is generated-code or index work over existing substrate.

- **P1a — column-pruned sequential scan (`__scan`).** The generated scan walks
  `id_to_row.values()` in hashmap (random) order and full-materializes every row *including a
  `String` alloc for unused columns*. A scan that (a) reads only the columns the operation needs
  and (b) iterates in physical row order fixes `scan_aggregate` (32 ms → target ~SQLite/redb
  range). Biggest single lever.
- **P1b — ordered/range index kind.** Hash indexes are exact-match only, so `views: ^u64` can't
  serve `WHERE views >= T ORDER BY views DESC LIMIT 10` — it full-scans (`scan_sort_top10` 32 ms
  vs SQLite's index-served 4.35 µs). Add an ordered index kind that serves range + top-N. This
  is the hash-vs-B-tree tradeoff made a config/directive choice, not a hard limit.
- **P1c — narrow materialization on the residual read paths.** Reverse-FK getters and live-query
  re-runs still full-materialize (the #160 residual, tracked there). Fold into the narrow-decode
  path #160 already built for the list scan.
- **P1d — group/batch commit.** Bulk load fsyncs per row (one barrier each). One barrier per N
  rows is the standard fix and would transform the bulk-load numbers. Orthogonal to layout;
  additive to the WAL path. (`[storage].fsync=never` already exists as the durability-trading
  knob; group-commit keeps durability while amortizing the barrier.)

**Phase 1 done when** the append-only benchmark path no longer loses scans/top-N by orders of
magnitude for reasons unrelated to the mutation model — i.e. the remaining gap vs redb/SQLite is
attributable to append-only itself, not read-path quality.

## Phase 2 — the experiment (fixed-width in-place variant)

- **P2a — `write_at` positional overwrite primitive** on `FixedColumn` (substrate,
  `crates/storage-native`, additive). Columns today are append + positional-read; in-place needs
  a positional write. Mirrored as a no-op-safe impl on `storage-web` for API parity.
- **P2b — `GenConfig storage_model: AppendOnly | InPlace`** (Tier A codegen variant). In-place
  `update` overwrites at `id_to_row[id]` for fixed columns; `delete` marks the slot; no
  `id_versions`. Variable columns keep the append path. Snapshot / `_at` / reader-isolation /
  live-query-removal / replica are **compiled off** for the in-place variant (they rely on
  committed bytes never changing) — their absence is a *result*, not a bug.
- **P2c — (optional third axis) `versioning` off variant.** A lot of the point-read indirection
  is the *versioning* tax (`id_versions` binary search for `_at`), not the *append* tax — live
  `get` is already O(1) `id_to_row`. A "append-only, no snapshot machinery" variant separates the
  MVCC cost from the append cost, so we don't misread one as the other.
- **P2d — benchmark wiring.** `benchmarks/configs/in_place.toml` (+ optionally
  `no_versioning.toml`), run through the existing matrix. The aggregate table gains a
  **feature-support column**: concurrency (scenario 16), transactions (17), snapshot/`as_of`
  reads, and replica are N/A for in-place — that gap is the measured price of the differentiator
  spine append-only buys.

**Phase 2 done when** we have a side-by-side matrix (write latency, point read, scan, footprint,
churn) for append-only vs fixed-width-in-place at matched read-path quality, *plus* the
feature-support column, and can state the append-only tax as a measured number rather than an
assertion.

## Expected shape (stated up front)

- **In-place should win:** update churn footprint (no dead versions → no compaction), possibly
  point reads if `id_versions` indirection is on the hot path.
- **Append-only should win / in-place can't offer:** every differentiator scenario (snapshots,
  lock-free readers, MVCC, replica) — these are N/A for in-place, which is the point.
- **Should be ~equal after Phase 1:** scans, bulk load, single-write latency (barrier-bound,
  same for both).

If Phase 1 closes most of the gap (likely), the honest conclusion may be "keep append-only; its
residual tax is small and it buys the features" — in which case Phase 2's value is the *measured*
evidence for that, not a redesign.
