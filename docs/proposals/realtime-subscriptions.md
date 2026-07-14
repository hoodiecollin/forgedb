# Proposal: Real-time Subscriptions

**Status:** **Direction A (change notifications) AND Direction B (live queries) — both LANDED.** A
landed 2026-07-07; **B landed 2026-07-08** (after the mutation surface #66). Direction C (durable
broker) remains DESIGN NOTE — lowest priority. `forgedb-product-manager` verdict:
**aligned-with-constraints** (notifications clean-green; live-queries **green-with-care**, PM gate PASS
conditioned on reusing the closed-set filter; generic subscription engine rejected). Direction A
shipped: new Class-1 substrate crate **`forgedb-changefeed`** (field-blind `(model, row_index)`
broadcast) + generated `insert()`/`link_*` emits + generated per-model typed event structs + generated
axum WS `GET /subscribe/<model>` with a generated per-model filter + nginx `Upgrade` forwarding.
**Direction B shipped (2026-07-08):** generated per-model live-query WS handler `GET
/live-query/<model>` that binds `?field=value` to the **same** generated closed-set filter
(`<model>_event_matches`) as REST list / #62-A (no second parser), re-runs the generated `all()`+filter
query on the **coarse** `event.model` signal, diffs by id over opaque hashes, and pushes typed
removal-aware `<Model>LiveDelta` (`Init`/`Added`/`Updated`/`Removed`) deltas. **No substrate change / no
`forgedb-changefeed` version bump** — the coarse signal was already there. Proven by a **live WebSocket
round-trip** exercising Init→Added→(non-match silent)→Updated→Removed through insert/update/delete
(`scratchpad/directionb_compile`, ephemeral). **Next: Direction C is DEFERRED** (lowest-priority
slice — networked/durable/resumable broker over a new on-disk log format; the in-process feed above
already covers every current caller).
**Issue:** [#62](https://github.com/hoodiecollin/forgedb/issues/62) (`idea`, `plan-next`)
**Date:** 2026-07-07

## Summary

The append-only storage engine **is** a change log — every write is positionally an event
("row N now exists in model M"). Subscriptions ride that fact through the two legitimate
published-runtime classes plus generated code:

- **Change-feed substrate (Class-1):** a field-blind broadcast primitive carrying
  `(model_name, row_index)`. Same category as `forgedb-storage`/`forgedb-wal`; it knows nothing
  about fields.
- **Generated per-model typed emitters (tailored code):** generated `insert()` already holds the
  typed record, so it emits `UserInserted { user: User }` and turns a row index back into a typed
  model for subscribers.
- **WebSocket transport (Class-2):** streams the already-generated event surface over a socket —
  pure transport, like the FFI bindings.

**The line that keeps it inside the guard:** *the substrate signals "M row N"; generated code
decides who cares and what it means.* Hold that and #62 is in-bounds; break it and it becomes the
forbidden generic engine, event-shaped.

**Blessed plan:**
- **Direction A — change notifications — ship now.** In-process, best-effort, insert-only.
- **Direction B — live queries — queued entirely behind the mutation surface.** Re-running a
  *generated* query on a coarse change signal; deferred until `update`/`delete` exist so it can be
  co-designed with removal semantics (shared prerequisite with #56/#63).
- **Direction C — durable broker — lowest priority.** Replay-from-offset using row-index as
  offset space; deferred because it needs a new durable on-disk log format and resumable wire
  protocol, and the in-process feed (A) already serves every current in-codebase caller.

## The key contrast: subscriptions are NOT hollow pre-mutation

Unlike MVCC (#56) and multi-tenancy Layer 2 (#59) — which were machinery for a mutation surface
that doesn't exist — **the events A streams already have a producer in the codebase today: every
`insert()` / M2M `link` is positionally an event**, so the change feed has a real caller the moment
it ships (activity feeds, "N new items" badges, message arrival, audit/event logs, live-appending
dashboards all fall out of it). An insert-only change feed is a **complete surface, not a stub** —
it wires an existing producer, it isn't machinery waiting for one. That is why A ships now rather
than waiting.

Only *live queries* split on the mutation gate: without `update`/`delete`, a live result set can
only ever grow (rows added, never changed or removed). The maintainer's call is to **queue all of
B behind the mutation surface** rather than ship a grow-only partial — keeping the live-query
semantics whole when it lands.

## Identity verdict & the drift vectors

**Passes in the notification shape; passes with-care in the live-query shape; fails as a generic
subscription engine.** Subscriptions has the strongest "always-on runtime engine" gravity in the
stack, so the ruling is about *which mechanism runs*. Three drift vectors — all the same mistake
(moving tailored logic into a shipped runtime that reads the schema):

1. **Substrate-side field filtering.** The moment the substrate filters events by field values
   (`subscribe(User, where age>18)`) it must decode rows against the schema — it now knows fields.
   Red line. Filtering happens in generated code (deserialize via the generated model, apply a
   **generated per-model filter**).
2. **Predicate-as-data subscriptions.** A client sending a query/filter *string* a shipped runtime
   parses and evaluates against arbitrary models = the generic query engine, event-shaped. Red line.
3. **A generic subscription registry.** A shipped `Map<ArbitraryPredicate, Connection>` re-evaluating
   predicates at runtime is the engine hiding in a hash map. The registry keys only on *generated*
   subscription identities.

## Direction A — change notifications (ships now)

**What it builds.**
- A **schema-agnostic broadcast primitive** — `tokio::sync::broadcast` carrying
  `(model_name, row_index)` (or opaque bytes). Field-blind, best-effort, bounded buffer,
  dropped-on-lag.
- **Signal origin = generated `insert()` (and `link_*`) emits** (blessed): after appending, the
  generated `Database` method — the unit that knows "a `User` was inserted" — calls the substrate
  broadcast primitive. Typing stays generated; the substrate stays schema-agnostic. (A storage-layer
  append hook was rejected: columns are per-field files with no model-boundary knowledge, so it
  couldn't say "a `User` was inserted" without pushing schema awareness into substrate.)
- **Generated per-model typed event structs** (`UserInserted { user: User }`) — generated code turns
  the row index into a typed payload.
- **Generated WebSocket endpoint** on the rust axum server (`GET /subscribe/:model` or per-model
  routes) streaming typed JSON events; subscribers may narrow via a **generated per-model filter**.
- **nginx:** add `Upgrade`/`Connection: upgrade` forwarding to the rust `location /` block
  (`src/commands/serve.rs:295` — today those headers are only on the bun
  `location ~ ^/(pages|routes|components)` block at `:285-286`).

**Identity fit.** Clean green — substrate Class-1, emitters generated, socket Class-2. `.forge`
never a runtime input; no re-evaluation.

**Scope (files/concepts).** A small broadcast type (substrate-adjacent);
`crates/codegen/src/rust.rs` (emit event structs + broadcast calls in `insert`/`link`);
`crates/codegen/src/api.rs` + `crates/http-server` (WS upgrade handler, per-model event
serialization); `src/commands/serve.rs` nginx block. Concepts: broadcast channel, per-model event
type, WS transport. Small-to-medium; likely no new *published* crate.

**Unlocks.** Activity feeds, new-row/notification streams, live dashboards — today, with real value.
It is the substrate every richer option extends.

**Forecloses.** Nothing structural. No result-set semantics, no durability/replay (those are B/C).

## Direction B — live queries — LANDED 2026-07-08

Re-evaluate a query and push updated results when data changes. **Legal only** if: the substrate
signals coarse **"model M changed"** (field-blind); on that signal, generated code **re-runs a
GENERATED query** — one of the finite, compile-time, per-model tailored queries already in
`database.rs`, selected from a **closed generated set**, never an arbitrary runtime query string —
then diffs and pushes deltas over generated model types. That is "generated code re-executing
generated code on a coarse signal," and it is the exact place the red line lives.

**What landed (PM identity gate PASS — green-with-care).**
- **No substrate change.** The changefeed already carries the coarse `event.model` signal; the
  handler consults **only** `event.model` (never `row_index`/`kind`), so no logical-row identity is
  resolved through the substrate and `ChangeEvent` is **not** widened. `forgedb-changefeed` unchanged
  (no version bump).
- **Generated code only.** Per model, a WS handler at `GET /live-query/<model-kebab>?field=value`.
  The `?field=value` params bind to the **exact same** generated closed-set filter
  (`<model>_event_matches`) used by REST `list` / #62-A — **no second predicate parser** (the single
  most important red line: filterable keys are the finite declared-scalar set, exact-match by name;
  no operator grammar, no undeclared fields). On connect it runs the generated `all()`+filter query,
  sends `<Model>LiveDelta::Init { rows }`, and records membership as `HashMap<Id, opaque-hash>`. On
  each matching coarse signal it re-runs the same generated query, diffs by id, and pushes typed
  `Added`/`Updated`/`Removed` deltas. `Removed` is now expressible because #66's superseding-tombstone
  append makes `all()` exclude retracted rows.
- **Delta wire type is generated:** per-model `<Model>LiveDelta` enum (`Init`/`Added`/`Updated`/
  `Removed`), typed records + the model's own id type, tagged JSON. Diff/membership plumbing is
  opaque ids + opaque hashes, kept **inline in the handler** (no shared live-query crate — that
  gravity is sidestepped entirely for M1).
- **Proof:** `scratchpad/directionb_compile` (ephemeral) boots the generated axum server, connects a
  WS client to `/live-query/post?title=live`, and asserts Init(empty) → Added (matching insert) →
  *silent* (non-matching insert) → Updated (record changed) → Removed (title change leaves the set)
  through real insert/update/delete on the shared DB. Guards: `test_api_generation_live_query`
  (reuses the closed-set filter; coarse `event.model`; no predicate parser),
  `test_rust_generation_live_delta_enums`.

**Honest limits / deferred.** O(rows) full re-run per matched event per connection — **no
coalescing/debounce** (open question #3), the real scaling cliff under rapid mutation; documented, not
hidden. `Updated` detection uses full-record `serde_json` stringify comparison, inheriting #62-A's
exact-match fragility for some float/bool encodings (typed per-field compare is the future
refinement). Live-query reads run against the live `Database` under the existing read lock, not a
`DatabaseReader` snapshot — each re-run is a fresh consistent read, deltas across re-runs are
eventually-consistent. Single-process; Direction C (durable broker) deferred.

## Direction C — durable broker (lowest priority)

A durable, replayable feed using the append-only log as the **offset space** — row index N *is* the
offset, so a subscriber can reconnect with "everything after offset K" and replay from storage;
topics, fan-out, backpressure, at-least-once. Green *as substrate* (offsets/model-name strings, no
fields) and the offset idea is elegantly native — but heavy, likely a new published
`forgedb-changefeed` crate. **Lowest priority** (blessed): building a durable broker before
best-effort in-process streaming has users is the machinery-beyond-consumers failure that killed
`fulltext`/`crud-api` in Phase 3b. Reachable later by extending A.

## Red lines

- **No generic runtime subscription/query engine.** No shipped crate that takes predicates-as-data
  and evaluates them against arbitrary models at runtime — event-shaped or not.
- **Live-query re-evaluation runs GENERATED queries**, from a closed compile-time set — never a
  runtime-interpreted query string.
- **The change-feed substrate stays schema-agnostic.** It signals `(model_name, row_index)` /
  opaque bytes; never decodes fields or filters by field values. Field-aware filtering lives in
  generated code via generated per-model filters.
- **The subscription registry keys only on generated subscription identities**, not arbitrary
  runtime predicates.
- **No new `.forge` syntax and no new authoring requirement.** Subscriptions are a property of the
  generated server; buffer size / delivery policy live in `forgedb.toml` or are generated defaults.
- **The app imports nothing beyond the generated server + the substrate/transport it already
  links.** No "ForgeDB realtime runtime" the app calls to reconstruct schema-specific streaming.
- **Preserve append-only.** A durable/replay feed (C) reads the append-only log as offset space; it
  does not mutate it.
- **Update/delete events are gated on the mutation surface** — don't fake them before `update`/
  `delete` + a retraction primitive exist (the #56/#59-Layer2 trap).
- **Single-process scope** — cross-process fan-out needs a broker and is a separately-justified
  expansion.

## First milestone (Direction A — in-process, insert-only) — LANDED 2026-07-07

**In scope** (all delivered)
- ✅ Schema-agnostic broadcast primitive — the new **`forgedb-changefeed`** crate (`tokio::sync::broadcast`)
  carrying `ChangeEvent { model: &'static str, row_index: usize, kind: ChangeKind }`, field-blind,
  best-effort, bounded buffer. `ChangeKind` is `Inserted | Linked` only — Update/Delete are gated on
  the mutation surface.
- ✅ Generated per-model typed event structs (`PostInserted { post: Post }`) + `emit` calls in
  generated `insert()` (carries the model *name*, never a field) and M2M `link_*`. `Database::new()`
  owns one shared feed and hands each collection a clone; `read_at(row_index)` made public so the WS
  handler can materialize a typed record from the broadcast row index.
- ✅ Generated WebSocket endpoint `GET /subscribe/<model-kebab>` on the axum server streaming typed
  JSON events; narrowing via a **generated per-model filter** (`<model>_event_matches`, each declared
  scalar field checked by name — relations excluded, closed per-model set); nginx `location /` now
  forwards `Upgrade`/`Connection: upgrade`.
- **Proof:** `scratchpad/changefeed_compile` (ephemeral) generates through the *current* codegen,
  compiles `database.rs` + `api.rs` together, then runs a **live WebSocket round-trip**: a client
  connects to `/subscribe/post?title=live`, two rows are inserted (one matching the filter, one not),
  and the client receives *exactly* the matching `PostInserted` JSON event over the socket. The
  substrate feed never decodes a field. Substrate unit tests cover the broadcast primitive.

**Explicitly out**
- **Live queries / result-set re-evaluation** (Direction B) — queued behind the mutation surface.
- **Durable replay, offsets, topics, backpressure, at-least-once** (Direction C, lowest priority).
- **Cross-process fan-out / any broker** — single-process `serve` only.
- **Update/delete events** — until the mutation surface + retraction primitive land (co-designed
  with #56/#63).
- **Any client-supplied query/predicate string; any runtime-interpreted filter; any `.forge`
  syntax addition.**

**Success = a WS client watching model M gets typed, generated, per-model events driven purely by
the append-only insert path, with the substrate never touching a field.** Reaching for a durable
broker, an offset log, or an arbitrary runtime predicate to ship this is the drift signal.

## Open questions carried forward (for Direction B)

1. **Closed generated-query set:** exactly which generated queries a live subscription may bind to
   (all generated getters/filters? a marked subset?) — define when B is scheduled.
2. **Diff granularity:** re-push full result set vs compute deltas; and how removals/updates surface
   once the mutation surface exists.
3. **Backpressure for live queries:** re-evaluation cost under rapid change — coalescing/debounce
   policy.

## Cross-issue dependency

Direction B (live queries with removal/update deltas) is gated on the **generated mutation
surface** (`update`/`delete` + retraction primitive) — the same prerequisite named in the MVCC
(#56) and inspector (#63) notes. Sequence B after that lands.

## Load-bearing references

- `crates/storage/src/lib.rs:174,251,743` — `row_count` manifest anchor + atomic save; append =
  positional event (the offset space a future C would reuse).
- `crates/wal/src/lib.rs` — WAL ops incl. Insert/Update/Delete but **no timestamp/LSN, no
  auto-replay**; a durability log, not a wired change feed today.
- `crates/codegen/src/rust.rs:173,558,884-887` — generated `insert` (the emit point), tombstone
  appended once, and the no-`delete`/no-`unlink` rationale that makes changes insert-only.
- `crates/http-server/src/server.rs`, generated `api.rs` `State<Arc<RwLock<Database>>>` — the axum
  host for the WS endpoint.
- `src/commands/serve.rs:285-296` — nginx forwards `Upgrade` to the bun location only; the rust
  `location /` must gain it for a WS endpoint.
- `docs/proposals/mvcc-concurrency.md` — the sequencing template and the shared mutation-surface
  prerequisite this note inherits for live-query removals.
- `CLAUDE.md` → "What ForgeDB is" — the invariant, plus the append-only / no-update-or-delete limit
  that gates Direction B.
