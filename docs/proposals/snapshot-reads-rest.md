# Proposal: Point-in-time (snapshot) reads over the generated REST API

**Status:** LANDED (2026-07-15) — both build phases complete. `forgedb-product-manager` verdict: **ALIGNED** (2 binding constraints; 2026-07-15).
**Issue:** [#85](https://github.com/hoodiecollin/forgedb/issues/85) (`enhancement`) — "Inspector: wire the snapshot scrubber to real point-in-time reads"
**Date:** 2026-07-15

## Summary

Expose the engine's **watermark snapshot reads** (`#56-A`) over the generated REST API, so
the ForgeDB Inspector's currently-decorative "as of" affordances (top-bar button + Console
"Time-travel" scrubber) become **real point-in-time reads**. The mechanism is entirely
"generate a few more per-model handlers over read methods the generated `Database` already
has" — **no new substrate crate, no new engine capability, no schema read at runtime.**

The load-bearing observation: the generic Inspector cannot decode arbitrary-schema rows
itself (it has no per-schema generated code), so its **only** path to typed row data is the
running generated API. Today that API serves only `.all()` / `.get(id)` — the newest committed
version. The snapshot machinery (`all_at`/`get_at` over a `forgedb_storage::Snapshot`) exists
in generated Rust but is unreachable over HTTP. This note adds the three handlers that make it
reachable, then wires the Inspector to them.

## What already exists (nothing to build here)

Generated `database.rs` emits, per model collection:

- `row_count() -> usize` — committed row-count watermark.
- `snapshot() -> forgedb_storage::Snapshot` where `Snapshot::new(watermark: usize)` is the
  **already-published** substrate type (`{ watermark }`, `visible(i) = i < watermark`).
- `get_at(&Snapshot, id) -> Option<Model>` — resolves the newest version **within** the
  watermark (a value changed after capture is excluded).
- `all_at(&Snapshot) -> Vec<Model>`.
- `Database::snapshot() -> DatabaseSnapshot` — a bundle of one watermark per model + junction,
  captured atomically on the single writer (a cross-model commit boundary).

Generated `api.rs` today: `GET/POST /api/<model>`, `GET/PUT/DELETE /api/<model>/{id}`,
`/metrics` (per-model `row_count()`), and the WS routes. **No point-in-time surface.**

## The wire token is a watermark, not a clock

A ForgeDB snapshot is a **row-count watermark**, not a wall-clock instant. You can read "as
of when this model held N committed rows," not "as of 14:03." The Inspector's mock scrubber
implies clock-time travel; that is not a capability the engine has and the UI must not claim
it (see §Inspector, and inspector-design-review correction #3). The honest axis is:

- **Per model:** the row-count axis `0..=row_count()` — every integer is a legal "as of."
- **Cross-model consistent:** a `DatabaseSnapshot` = the vector of per-model watermarks
  captured together. This is the token the Inspector freezes when the user "pins a snapshot."

## Generated endpoints (all per-model, generated, tailored)

1. **`GET /api/<model>?as_of=<watermark>`** — when `as_of` is present and parses to a `usize`,
   the list handler reads
   `db.<field>.all_at(&forgedb_storage::Snapshot::new(watermark))` instead of `.all()`, then
   runs the **same** existing generated `#filter_fn` / `#sort_fn` / substrate pagination over
   those rows. `as_of` is not a declared model field, so the closed-set field filter already
   ignores it (same as `sort`/`limit`/`offset`). Absent → unchanged live read.
2. **`GET /api/<model>/{id}?as_of=<watermark>`** — present → `get_at(&Snapshot::new(w), id)`;
   absent → `get(id)`. 404 semantics unchanged (a row not yet visible at the watermark reads
   as absent — correct).
3. **`GET /snapshot`** — captures the current per-model watermarks under one read guard and
   returns them as JSON: `{ "watermarks": { "<Model>": <row_count>, ... } }` (models only —
   junction watermarks deferred, consistent with the single-model-grid limit below). This is
   the atomic "as of now" token; the client freezes it to pin a snapshot, then passes each
   model's watermark to that model's list/get. (`/metrics` already reports the same
   `row_count()` values; `/snapshot` is the purpose-named token endpoint, keyed by PascalCase
   model name, capturing all models under a single guard so the bundle is a coherent instant.)

Invalid `as_of` (non-numeric) → **400** (do not silently fall back to live — a client asking
for a snapshot and getting live data is a correctness trap). `as_of` larger than the current
watermark clamps to "now" via `Snapshot`'s `visible` check (reads at most the committed rows).

### Codegen shape

- The list/get handlers gain a small branch keyed on `params.get("as_of")`. The
  filter/sort/paginate tail is **unchanged and shared** — no second read path, no second
  predicate parser (the #90 red line holds).
- `/snapshot` is one schema-wide handler emitted alongside `/metrics`, naming each storage
  field's `row_count()` (same generator technique as the metrics handler).
- **Compile-test discipline (load-bearing):** snapshot tests only compare strings. The
  emitted `api.rs` must be `cargo check`ed in a throwaway crate against a real multi-model
  schema (and a live `tower::oneshot` proving `?as_of` at an old watermark omits rows appended
  after it), exactly as #90/#113 were verified. Guard: `test_api_generation_snapshot_reads`
  (asserts the `all_at`/`Snapshot::new` branch, the `/snapshot` route, the shared filter tail,
  400-on-bad-`as_of`).

## Inspector wiring (`apps/inspector`)

- **`live.ts`:** `listRows`/`getRow` gain an optional `asOf?: number`, appended as `&as_of=`;
  add `getSnapshotToken(base): Promise<Record<string,number>>` hitting `GET /snapshot`.
- **Atoms:** a `snapshotTokenAtom` (the frozen watermark map, or `null` = live) + a
  `pinnedSnapshotsAtom` (named captures). `useLiveRows` passes the current model's watermark
  from the active token into `listRows`. When a snapshot is pinned, the live-query
  subscription is **suspended** (a point-in-time view is not a live tail — mixing them is
  incoherent).
- **Top-bar "as of":** the button opens a menu — "now (live)" or a pinned snapshot; the label
  reflects the active token (`now` vs `@ <model> rows: N`).
- **Console "Time-travel" tab:** the slider maps to the **row-count axis** of the active model
  (`0..=row_count`), not fake clock stops; the readout shows "as of <N> rows" honestly. "pin
  snapshot" calls `GET /snapshot` and stores the bundle.
- **"Compare vs current" diff:** client-side id-diff over `all_at(w_pinned)` vs `all()`
  (added / removed / changed rows) — explicitly an **inspector-level construct** (design-review
  correction #3), labeled "tool builds this," not an engine feature.

## Product verdict & invariant mapping

`forgedb-product-manager` gate (2026-07-15): **ALIGNED.** The handlers are generated per-model
tailored code calling read methods themselves generated per-schema (`all_at`/`get_at`);
`?as_of=<watermark>` is an opaque `usize` position (same class as `?after=<offset>` on
`/replicate` and `limit`/`offset`) carrying no field/model/predicate and deliberately not a
model field, so the closed filter set is untouched; `/snapshot` is the read-side peer of
`/metrics` — opaque watermarks, a fixed per-schema key set, captured under one guard. No new
substrate, no publish gap, "schema + CLI + config" preserved.

**Two binding constraints (from the gate):**
1. **`as_of` routes through the same generated per-model read/filter/sort/paginate path — no
   parallel handler body.** The branch swaps only the row source (`all_at`/`get_at` vs
   `all`/`get`); the filter (`#filter_fn`), sort (`#sort_fn`), and substrate pagination stages
   are the identical generated code. A second snapshot-specific filter body = drift → reject.
2. **The watermark stays an opaque scalar on the wire — never a wall-clock instant, never
   carrying field/model/predicate.** Parse as `usize`; non-numeric → **400**; a value past
   `row_count()` clamps to current (consistent with `Snapshot::visible`). "As of" must not
   grow into a timestamp-resolved lookup (no wall-clock→watermark index exists; that is a
   separate, heavier proposal needing its own gate).

Non-binding: the endpoint docs should note these reads are single-process / single-writer
consistent (the same honest limit as the existing snapshot machinery).

## Honest limits (must be surfaced, not hidden)

- **Watermark, not wall-clock.** No mapping from a timestamp to a watermark exists; the UI
  time-travels over the row-count axis / pinned captures only.
- **No persisted watermark history.** The server does not remember past watermarks-over-time;
  the client can read *any* past watermark, but "meaningful past instants" (pre-compaction,
  pre-migration) only exist if the client pinned them, or the user picks a row-count.
- **Compaction renumbers rows.** After an in-process `compact()` (#92) the physical row
  positions change, so a watermark captured before a compaction is **not** comparable across
  it. `/snapshot` tokens are valid only within a compaction epoch; the Inspector must discard
  pinned tokens on a detected reopen/compaction (out of scope to detect here — documented).
- **List `?as_of` still reads full rows server-side** (filter/sort need them); it shrinks
  nothing on the wire beyond what the snapshot's row set already excludes. The point-`get`
  path is the cheap one.
- **Single-model grid.** The scrubber pins one cross-model token but the grid views one model
  at a time; true cross-model consistent joins-at-a-snapshot are the generated `_at` traversal
  surface, not re-exposed here.

## Out of scope / deferred

- Timestamp→watermark indexing (a "read as of <clock time>" needs a persisted time→watermark
  map — a separate feature).
- Snapshot reads over the WS/replication surface.
- Exposing the cross-model `_at` M2M traversal over REST (the Inspector diffs per model).

## Build order (both phases LANDED)

1. **Codegen — DONE.** `/snapshot` handler + `?as_of` branch on list/get + guard
   `test_api_generation_snapshot_reads`; the get handler now always owns its `Query(params)`
   extractor (`generate_projection_rest` no longer emits one). Verified: throwaway-crate
   compile + live `tower::oneshot` proof (`scratchpad/snapshot_compile` — `?as_of` reads a
   2-of-3 prefix, `get_at` 404-vs-200, `/snapshot` watermarks, bad `as_of` → 400); all 18
   `examples/` generate clean; an integer-PK example (`iot-sensors`, u64) compile-checked.
   487 workspace tests (486 + the guard), examples build clean. **No substrate/publish gap**
   (`forgedb_storage::Snapshot` already published).
2. **Inspector — DONE.** `live.ts` `asOf` on `listRows`/`getRow` + `getSnapshotToken`;
   `snapshotTokenAtom`/`pinnedSnapshotsAtom`/`pinSnapshotAtom`; `useLiveRows` passes the active
   model's watermark and suspends the live-query when pinned; top-bar "as of" dropdown (live +
   pinned); Console snapshot tab replaced the fake-clock slider with a discrete live/pinned
   selector + "Pin current…" (real `GET /snapshot` capture) + honest row-count-watermark
   readout. `tsc --noEmit` clean. **Not runtime-tested in the Tauri shell** (needs a running
   generated server + desktop build) — the data-path wiring is type-checked; the compare-vs-
   current diff view is left as a labeled inspector-level marker, not yet a rendered diff.
