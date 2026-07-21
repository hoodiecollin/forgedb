# Browser State Store — WASM Replica vs. TS SDK Cache

An informational analysis of what the browser **wasm read-replica** buys
you versus a well-cached **TypeScript SDK**, and where WebSockets fit. This is
*not* a design doc — it makes no decisions and blesses no plan. It exists to help
steer product shape and to name the gaps this comparison exposes in the current
system. When a claim here matters, verify it against the code; the replica's
honest limits live in `WHAT_V1_IS.md`.

The core framing, stated once: **the wasm replica is not a "faster store" — it is
a local-first, identity-preserving store.** It runs the *same generated
`database.rs`* compiled to `wasm32`, in a Web Worker, over `storage-web` arenas,
fed by the `/replicate` durable broker. Justify it on
arbitrary-local-query / offline / cross-entity-consistency / high-local-query-volume
— **not** on beating V8 at raw compute. For most apps, SDK cache + a subscribe-WS
is the lighter, faster-to-first-paint default.

---

## 1. WASM loading: first-paint, load time, JS bundle

The load cost splits into three things that arrive on different timelines.

- **JS bundle — small.** The `.wasm` binary is a *sibling asset*, not part of the
  JS chunk graph. What lands in the bundle is the wasm-bindgen shim, the generated
  `ReplicaClient` (an RPC shim to the Worker), and the static worker-bootstrap
  script — KB-scale glue, tree-shakeable. The replica does **not** bloat the
  parse/JIT-bound JS bundle the way an equivalent pure-TS engine would.

- **The `.wasm` binary — the real weight.** A schema-tailored Rust engine links
  the storage arena code, wal, `changefeed::durable`, `txn`, plus `serde_json`,
  `regex` (from `@pattern`), `uuid`, `rust_decimal`. Realistically hundreds of KB
  to low-MB depending on schema size and features used. Fetched separately,
  compiled via streaming compilation (compile overlaps download), then
  instantiated. On low-end mobile the compile CPU is non-trivial.

- **First paint is architecturally protected.** Because the engine runs in a **Web
  Worker** (by design), wasm fetch/compile/instantiate happens off the
  main thread. First paint / FCP is *not* blocked by it — the initial render uses
  SSR data, a loading state, or a network fetch, and the local store becomes
  queryable a beat later. What *is* gated by cold start is
  **time-to-locally-answerable-query**: Worker spawn → wasm ready → OPFS/IDB
  hydrate → `/replicate` catch-up from watermark.

**Cold vs. warm is the whole story:**

- *Cold* (first visit): pay wasm download + compile + hydrate + catch-up. A genuine
  tax versus a plain network fetch.
- *Warm* (return visit): HTTP cache (and browser compiled-module caching) skips
  re-download/most re-compile; OPFS is already persisted; resume from the stored
  watermark with **0 frames re-applied** — fast *and* you skipped re-downloading
  the dataset. The case where the replica clearly wins.

---

## 2. Where wasm actually beats a hypothetical pure-TS generated store

Be honest: for a browser **state store**, wasm is usually **not** a raw-compute win
on small data. V8 is very fast at object manipulation, and crossing the
JS↔wasm↔Worker boundary has real per-call cost. The wins are specific, and mostly
*not* "it's faster":

1. **Identity / single implementation (the real reason).** The replica is
   *literally the same generated code* as the server — same
   filter/sort/index/traversal/snapshot/validation/version-guard semantics,
   byte-for-byte. A pure-TS store is a **second implementation** of all that query
   logic that must stay bit-identical to the Rust engine forever: a permanent drift
   surface. This — not FLOPS — is what justifies wasm here.

2. **Scale: columnar + GC-free + memory footprint.** At tens-of-thousands of rows
   and up, packed columnar `ArrayBuffer`s with tight decode loops and lazy fault-in
   beat a JS object graph: far lower memory (no per-object header/hidden-class
   overhead, no pointer chasing) and no GC pauses on scan/filter/sort. The one place
   wasm wins on *performance* — large working sets, predictable latency.

3. **Opaque-byte replication + persistence format.** `apply_frame` replays the
   `/replicate` opaque row bytes through the same write path; `storage-web`
   persists per-column files to OPFS with byte-identical positional layout (so
   partial fault-in and the manifest/backup format just work). A pure-TS store
   re-invents both the decode path and an IDB schema.

**Where pure-TS wins:** cold start, bundle size, small-dataset latency, and
*fine-grained chatty reads* (an in-main-thread TS store has no Worker RPC /
structured-clone cost per read). An app holding a few thousand rows and doing many
small reads is plausibly *faster and lighter* in pure-TS.

---

## 3. How far good TS SDK caching gets you

Very far — for most apps it is the right answer, and dramatically cheaper (no wasm,
tiny bundle, instant cold start). A well-cached REST client (TanStack Query / SWR /
normalized cache) gives instant renders from cache, stale-while-revalidate, request
dedup, optimistic mutations, pagination. That covers the large majority of app UX.

The ceiling — where a *cache* stops being enough and you actually want a *replica*:

- **Novel queries.** A query cache only knows what it has fetched (or prefetched).
  A new filter / sort / relation-traversal → cache miss → round trip. The replica
  answers arbitrary generated queries locally.
- **Offline / local-first.** The cache holds the slices you've seen; the replica
  holds the whole (tenant's) dataset and stays queryable with no network.
- **Cross-entity consistency.** A normalized cache assembles views from
  independently-fetched slices that can disagree. The replica gives watermark
  snapshot consistency (incl. M2M traversal) locally.
- **Write model is a wash in v1.** The replica is read-only today, so writes go to
  the server either way — optimistic UI is the same client-side reconcile problem
  in both. The replica's advantage is entirely on the *read/query* side.

Framing: **SDK cache + subscribe-WS is a caching layer; the replica is a local-first
architecture.** Reach for the replica only for arbitrary local querying, offline
reads, consistent local snapshots, or when local-query volume is high enough that
round-trips dominate. Otherwise the cache is the better trade.

---

## 4. How WebSockets factor in

There are **two different WS surfaces**, and conflating them hides the trade.

- **`/subscribe/<model>` + `/live-query/<model>`** — the realtime layer *for the
  SDK path*. In-process, best-effort, **not** durable/resumable. Upgrades a polling
  cache to push-fresh: subscribe, patch/invalidate the cache on each event. Cost: a
  live-query re-runs `all()+filter` **O(rows) per matched event per connection**
  (the live-query scaling gap), and you tend to hold one subscription per view. On drop →
  refetch.

- **`/replicate?after=<offset>`** — the durable broker stream that *feeds the
  replica*. Ordered opaque `PersistedEvent` frames at monotonic offsets, resumable
  from a persisted watermark, idempotent by offset. Integral, not optional: cold
  path is hydrate-from-OPFS-at-W → connect `after=W` → catch up the bounded gap
  (durable replay stitched to live tail) → stay live. On drop → resume from offset,
  no refetch.

How they shape each situation:

- **Replica:** one durable connection streams *everything*; query cost lives on the
  client. WS setup adds to cold-start-to-fresh, but catch-up is bounded to frames
  since W.
- **SDK:** WS is the optional freshness upgrade. Without it, refetch-on-revalidate;
  with it, realtime but you pay per-view live-query cost server-side and lose
  durability/resumability.
- **Connection economics at scale:** replica = 1 durable stream/client (query cost
  pushed to clients); SDK = potentially many per-view live-queries/client (query
  cost stays on the server). And the SDK's *plain* REST reads ride the entire HTTP
  caching stack (CDN, HTTP cache, ETags); WS bypasses all of it. The replica doesn't
  need that stack because it holds data locally.

The clean way to see it: **the replica replaces both the SDK read-cache and its
subscribe-WS with one local queryable store fed by one durable WS.** You move query
execution and a durable stream onto the client in exchange for the wasm cold-start
tax and losing HTTP-layer caching.

---

## Gaps this comparison exposes in the current system

These fall out of the analysis above and are worth tracking as product-shape
signals, not just replica internals:

- **Live-query scaling cliff.** O(rows) re-run per matched event per
  connection, no coalescing/debounce. This caps how far the *SDK realtime* path
  scales before the replica (which moves that cost client-side) becomes the only
  answer — so it partly decides *when you're forced onto* the replica.
- **Replica is read-only (Phase 2 gap).** Writes still round-trip in both paths, so
  the replica gives no write-latency or offline-write advantage yet. A local-first
  pitch is incomplete until local/optimistic writes land.
- **No wall-clock time-travel.** Snapshot tokens are row-count watermarks, not
  instants — "as of a timestamp" is not answerable locally
  or over REST without a separate index.
- **Cold-start tax is unmeasured.** The wasm binary size, compile time, and
  time-to-first-local-query are not benchmarked per representative schema. Any
  perf pitch for the replica needs real cold/warm numbers, not the compile-test
  proofs we have today.
- **No shared decode path with a TS store.** The `/replicate` frames are opaque
  serde_json row bytes; only the wasm engine decodes them. If a lighter pure-TS
  consumer is ever wanted, it would need its own decode — worth noting before the
  bindings work assumes wasm is the only browser consumer.

---

*Related:* the browser read-replica and language bindings work, and
`WHAT_V1_IS.md` (honest v1 limits).
