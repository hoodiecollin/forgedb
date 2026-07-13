# Proposal: WASM Read-Replica Target (browser / live-synced follower)

**Status:** DESIGN NOTE — product-gated. Prior `forgedb-product-manager` verdict on the
*offline-first* framing: **aligned-with-constraints** (2026-07-06). **Reframed 2026-07-13** to
the maintainer's actual vision — a **live-synced read-replica**, not a standalone offline DB —
which adds a networked replication substrate; **re-gate recorded below.**
**Issue:** [#50](https://github.com/hoodiecollin/forgedb/issues/50) (`plan-next`, `idea`)
**Depends on:** [#62 Direction C](https://github.com/hoodiecollin/forgedb/issues/62) (durable/networked change broker — previously deferred; this note makes it the critical path).

## Summary

One `.forge` schema compiles to **two generated artifacts from the same generator**:

1. the existing **server image** — the tenant's **source of truth** (single writer, authoritative
   append-only columns + WAL); and
2. a **`wasm32` build of the *same* generated `database.rs`** that runs in the browser as a
   **read-only replica** of that server.

The browser replica **opens a durable connection to its tenant's server, catches up from a
watermark, and applies a live change stream** — behaving like a follower in a replication set.
The UI queries its **local WASM instance** (same generated typed query/filter/index/traversal
surface as the server), so reads never take a network hop. **Writes are out of scope for the
first milestone**: the UI proxies writes to the server's existing REST/generated API; they land
at the source of truth and flow back down the replication stream to the local replica.

This is **local-first read-replication**, not an ORM and not an offline standalone DB. Both
artifacts run *generated, schema-tailored code*; **nothing reads a `.forge` schema at runtime.**
The only new shipped pieces are schema-agnostic **substrate** (browser storage backend +
networked replication transport) and thin **transport glue** — the two sanctioned published
classes from the identity guard (`CLAUDE.md` → "What ForgeDB is").

> **Scope guard (footprint).** ForgeDB targets application datasets, **not** huge analytical
> ones, so **whole-(tenant-)working-set replication into the browser is an accepted design
> point.** That deliberately sidesteps *partial-sync / query-scoped replication* — the single
> hardest problem in this space (what makes ElectricSQL/PowerSync complex). The dimension to
> nail is **not** size; it is **scoping/authorization** (which rows a replica may receive),
> handled per-tenant (below), independent of size.

## Roles

| Role | Who | Writer? | State maintained by |
|---|---|---|---|
| **Source of truth** | the v1 generated axum server (per tenant) | **yes** — the single writer (v1 single-writer-per-process contract) | user writes via REST/generated API |
| **Replica (follower)** | the same `database.rs` compiled to `wasm32`, in an OPFS-backed Worker | **no** (read-only in M1) | **applying the server's replication stream** — not local user writes |

The replica's "writes" are **apply-from-stream** (append committed records in order), never
user CRUD. That is what keeps M1 free of conflict resolution: single-writer authority + a
read-only follower ⇒ **no CRDT, no merge, no divergence** to reconcile.

## Identity mapping (invariant check)

The schema is a *compile-time input to generation*, never a *runtime input to a generic
engine*. Every published artifact lands in a sanctioned bucket; no generic engine appears.

| Stratum | What it is | Guard class |
|---|---|---|
| **A. Generated `database.rs`** (insert/get/traversal/filters/indexes/validation) | the app's tailored logic, recompiled for `wasm32` — **same source**, run on *both* server and replica | Generated code (the product) |
| **B. Browser storage backend** (`forgedb-storage-opfs`) | schema-agnostic positional-column store over OPFS; moves opaque typed columns, knows no model | Class-1 *substrate* |
| **C. Replication transport** — networked, resumable snapshot+tail | schema-agnostic wire stream of `{tenant, model, row_index, kind, opaque_bytes, watermark}`; **decodes no field** | Class-1 *substrate* (server broker) + Class-2 *transport* (browser glue) |
| **D. `wasm-bindgen` glue** | exposes only the *generated* per-schema surface (`userPosts(id)`) + a `sync()` handle to browser JS | Class-2 *transport* |

**Milestone success criterion (also the identity proof):** one existing generated crate runs
unmodified in a browser, stays consistent with its server by replaying the server's own change
stream, and **no artifact anywhere reads a schema at runtime.** If achieving it needs a
WASM-specific generator branch, a schema-aware storage backend, or a replication wire format
that decodes fields, the design has drifted and must be revisited before shipping.

## The deciding insight — append-only makes replication a *replay*, not a *diff*

ForgeDB storage is **append-only with superseding-version + tombstone semantics**, and reads
resolve *latest-version-within-a-watermark* (#56 watermark snapshots, #66 mutation surface).
That is exactly a replication log already:

- A replica catches up by **pulling committed records in order from a watermark** and appending
  them into its own columns — the same bytes the server appended. No structural diff, no merge.
- **Updates** are superseding appends; **deletes** are tombstone appends (→ expressible as
  `Removed` deltas). The follower reproduces server state by position.
- `forgedb-changefeed` already broadcasts the field-blind `{model, row_index, kind}` signal;
  `forgedb-wal`'s **opaque `Raw` record path** is already an ordered, CRC-framed log of
  committed row bytes; `forgedb-backup` already produces a **lock-free full snapshot** of a
  data dir as opaque bytes. The replication substrate is largely **assembling existing
  schema-agnostic pieces into a networked, resumable stream**, not inventing a new engine.

This is why the vision is tractable: the hard parts of a sync engine (divergence, partial
scope) are removed by *read-only + whole-working-set + append-only replay*.

## Architecture

### A. Generated code — unchanged, recompiled

The existing `RustGenerator` output is the *same code*, recompiled for `wasm32`. **There must
be no WASM generator branch that changes data logic.** The tailored query/filter/index/traversal
surface is generated once and is target-agnostic; the append-only limits carry over unchanged.
The replica links the same generated `database.rs` the server does — that is the whole point,
and the identity proof.

### B. Browser storage backend — schema-agnostic substrate

Implements the **same column interface** `forgedb-storage` exposes today (`FixedColumn`,
`VariableColumn`, `Tombstones`; positional `append_*`/`read_*`), so generated code links it
with **zero codegen changes**.

**Recommended target: OPFS `createSyncAccessHandle` inside a Web Worker (primary).** OPFS sync
access handles give **synchronous, positional file reads/writes** on a real handle — mapping
*directly* onto `forgedb-storage`'s `FileExt` positional-I/O model, arguably needing *less*
adaptation than any async store. The WASM DB lives in the Worker; the UI thread queries it via
a thin RPC (postMessage / Comlink).

- Async is quarantined to Worker setup + handle acquisition; the per-row API stays
  **synchronous and unchanged**.
- A file column and an OPFS-handle column are the **same bytes at the same offsets** ⇒
  semantics are byte-identical across targets. This is what lets the generated data logic be
  emitted **once** and linked against two backends.
- **Fallback (secondary):** for engines without OPFS sync handles, the prior note's
  **in-memory-arena-over-IndexedDB** design (hydrate on open / flush on commit; async
  quarantined to two boundaries) remains valid. Recommend shipping OPFS first; keep the IDB
  arena as a documented fallback, selected at runtime.
- **Knows no schema.** No `match model_name`, no field/relation awareness. Opaque columns only.

**Backend selection — facade + `cfg`.** Turn today's engine into `forgedb-storage-native` and
make `forgedb-storage` a thin facade:

```rust
#[cfg(target_arch = "wasm32")]      pub use forgedb_storage_opfs::*;   // (+ IDB fallback)
#[cfg(not(target_arch = "wasm32"))] pub use forgedb_storage_native::*;
```

Generated code keeps `use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};` verbatim
and stays byte-identical across targets. Preferred over a `StorageBackend` trait for M1 because
a trait risks **async-coloring** the per-row API. *(Trade-off to validate: the facade must not
break the published `forgedb-storage` surface for existing native consumers.)*

**M1 write path is simpler than a standalone DB.** The replica does **not** take user writes,
so the backend needs only **apply-from-stream (append committed records) + read** — not a
user-facing `commit()` of local mutations. Applied records persist to OPFS as they arrive
(batched); durability is *replication-checkpoint granularity* (the persisted watermark).

### C. Replication transport — the new crux (schema-agnostic)

A **networked, resumable** stream between server and replica. This is the deferred **#62
Direction C** (durable/cross-process broker) made real; it is the critical-path build.

**Wire protocol (field-blind).** Frames carry `{tenant, model: &'static str, row_index, kind,
opaque_bytes, watermark}` — the model *name* and opaque row *bytes*, **never a decoded field**.
Same identity posture as `forgedb-changefeed`.

**Connect / catch-up handshake:**
1. Replica connects (authenticated — see scoping) and sends its persisted **watermark `W`**
   (or "cold").
2. Cold or too-far-behind → server sends a **base snapshot** (reuse `forgedb-backup`'s lock-free
   full-snapshot bytes for the tenant) establishing `W0`; the replica writes it straight into
   OPFS columns.
3. Server then streams the **live tail** past `W0` (the `forgedb-changefeed` signal, upgraded to
   carry the committed opaque bytes + monotonically advancing watermark).
4. Replica **applies in order**, advancing and persisting its watermark. Reconnect resumes from
   the persisted watermark — hence "resumable."

**Server side** is a Class-1 substrate broker (evolve `forgedb-changefeed` from
`tokio::sync::broadcast` into a networked, backpressured, resumable feed reading committed WAL
`Raw` records). **Browser side** is Class-2 transport glue.

**Scoping / authorization = per-tenant, riding #59.** The replication endpoint sits behind the
**existing verify-only JWT tenant guard**: a replica authenticates with the tenant's token and
receives **only that tenant's stream**. This matches process-per-tenant (one replica ↔ one
tenant's server process). **Row-level / per-user filtering is #72 and stays out** — a browser
replica gets the whole tenant working set (the accepted footprint point). This is the security
boundary to get right, and it reuses infrastructure that already exists.

### D. `wasm-bindgen` transport glue — Class-2

A thin JS/TS layer (same spirit as `crates/ffi`) exposing the *generated* read surface
(`get(id)`, `list(...)`, generated traversals like `userPosts(id)`) plus a **sync handle**
(`connect()`, `onChange`, `watermark`, `close`). It exposes **only what codegen already
produced** + the connection lifecycle; it invents no query surface. Ship via `wasm-pack` with
generated `.d.ts` (mirrors the Node/Deno bindings direction, #52/#53).

### Write path (M1: proxy; Phase 2: optimistic-local)

- **M1:** UI writes call the server's existing REST/generated API over the network → land at
  the source of truth → journal → flow back down the replication stream to the local replica.
  Read-your-writes latency = one round trip + stream apply. Simple, correct, no local write
  machinery.
- **Phase 2 (explicitly out of M1):** optimistic local apply + server arbitration + an offline
  write queue. *This* is where conflict handling would live; deferred until read-replication is
  proven. The "distributed swarm / peer" framing (writes originating at the edge, gossip)
  is Phase 2+ and needs its own note.

## Red lines (reject on sight)

- A **wasm blob that ingests a `.forge` schema / serialized manifest at runtime** and dispatches
  generically. That *is* the generic engine.
- A **"ForgeDB browser SDK"** offering `db.query("User").where(…)` over models discovered at
  runtime. A generated `userPosts()` is fine; a generic `.query(modelName)` is the ORM we forbid.
- Backend **B or transport C growing schema knowledge** (any model/field/relation awareness; any
  wire frame that decodes a field rather than carrying opaque bytes + a model *name*).
- A **divergent WASM generator** reimplementing insert/traverse/apply semantics. One generated
  surface, two link targets.
- **Async-coloring** the generated per-row API (`get()` becoming `async` everywhere). Keep async
  at Worker/handle/connection boundaries only.
- **Local writes with client-side conflict resolution smuggled into M1.** M1 is read-only
  replica; writes proxy to the authority. A CRDT/merge layer is a separate, later, gated design.

## Open decisions the implementation must fix

1. **Snapshot vs. tail cutover.** Threshold for "too far behind → resend base snapshot" vs.
   streaming tail from `W`. Reuse `forgedb-backup` for the snapshot; define the watermark-gap
   policy.
2. **Backpressure & ordering across models.** The feed is per-model today; the replica needs a
   **global apply order** (or per-model watermarks) that keeps cross-model reads consistent
   (the #56 `DatabaseSnapshot` is the server-side commit boundary to mirror).
3. **OPFS persistence layout.** Mirror the on-disk `fixed/…bin` / `variable/…bin` layout as OPFS
   files under a per-tenant directory; persist the watermark as a small sidecar. (IDB-fallback
   keeps the prior note's keyed-blob layout.)
4. **Durability granularity.** State explicitly: **durable at persisted-watermark granularity**;
   in-flight applied-but-unpersisted records replay from the server on reconnect (idempotent by
   absolute row index, like server WAL replay).
5. **Facade vs. trait** for backend selection — facade+cfg recommended (above); revisit only if
   one binary must hold both backends.
6. **Reconnect / auth-refresh** — JWT expiry mid-stream; token refresh without a full re-snapshot.
7. **Integer-PK & M2M** — traversal is UUID-only today; M2M junction lookups are linear scans.
   Inherit those limits for M1 (UUID-keyed models only), same as native.

## First milestone (smallest slice that proves the model *and* the guard)

**In scope**
- Browser storage backend `forgedb-storage-opfs` (fixed + variable + tombstone) over OPFS sync
  handles in a Worker, apply-from-stream + read; facade + `forgedb-storage-native` split
  (surface-compatible for native).
- Networked, **resumable** replication: server broker (WAL `Raw` tail + `forgedb-backup` base
  snapshot, tenant-scoped behind the #59 JWT guard) ↔ browser follower with a persisted
  watermark.
- One **UUID-keyed** example schema (2 models + 1 relation from `examples/`) compiling to
  `wasm32`, running the **same generated `database.rs`** the server runs, with **zero data-logic
  codegen changes**.
- Thin `wasm-bindgen` glue: `connect`, `get`, one relation traversal, `onChange`, `watermark`.
- **Browser E2E** (Playwright / Chrome-DevTools MCP available): server inserts a row → replica
  receives it live and `get` returns it locally; **reload page** → replica resumes from its
  watermark (not a full re-snapshot) and is consistent; server `delete` → replica reflects
  `Removed`.

**Explicitly out**
- **Local writes / optimistic apply / conflict resolution** (Phase 2), peer-to-peer "swarm"
  writes, partial/query-scoped replication, row-level (#72) filtering, integer-PK & M2M,
  in-browser compaction, the full `examples/` corpus.
- Whole-tenant-working-set replication and persisted-watermark durability are **accepted,
  documented limits.**

**Success = one existing generated crate runs unmodified in a browser, stays consistent with its
server by replaying the server's own append stream, with no artifact reading a schema at runtime
and no wire frame decoding a field.** Needing a WASM generator branch, a schema-aware backend, or
a field-aware wire format is the drift signal.

## Load-bearing references

- `crates/storage/src/lib.rs` — the column interface the OPFS backend must mirror (positional
  `read_*`/`append_*`), the reader-handle pattern (#56-B), and the sync `io::Result` + `FileExt`
  positional-I/O assumption OPFS sync handles satisfy directly.
- `crates/changefeed/src/lib.rs` — the field-blind `{model, row_index, kind}` signal to evolve
  into the networked, resumable, bytes-carrying replication feed (#62 Dir C).
- `crates/wal/src/lib.rs` — the opaque `Raw` CRC-framed committed-record log that is the
  server-side replication source.
- `crates/backup/` — the lock-free full-snapshot machinery that supplies a replica's base
  snapshot (`W0`).
- `crates/auth/` + multi-tenancy (#59) — the verify-only JWT tenant guard the replication
  endpoint sits behind (scoping boundary).
- `crates/codegen/src/rust.rs` — storage call sites the target inherits (imports, column
  construction from paths, append/read); where the facade dependency wires in.
- `crates/ffi/src/lib.rs` — the sanctioned Class-2 transport pattern the `wasm-bindgen` glue
  mirrors.
- `CLAUDE.md` → "What ForgeDB is" — the invariant, plus the append-only / linear-scan /
  UUID-only-traversal / single-writer limits this note inherits.

---

## Product re-gate (2026-07-13)

*Recorded after the reframing from offline-first standalone → live-synced read-replica.
`forgedb-product-manager` verdict, verified against `crates/changefeed/src/lib.rs` (field-blind
red line, lines 18–24) and `crates/wal/src/lib.rs` (model name as "opaque routing tag" + `Raw`
bytes).*

**Verdict: ALIGNED-WITH-CONSTRAINTS**

**Rationale.** The reframing holds the invariant. The identity proof is unusually clean here:
the replica is *the same generated `database.rs`* recompiled for `wasm32`, so the tailored data
logic remains generated-per-schema at compile time and nothing reads a `.forge` schema at
runtime — the milestone success criterion is literally "one existing generated crate runs
unmodified in a browser." Both new shipped pieces land in sanctioned buckets: backend (B)
mirrors the existing positional-column interface and knows no model; transport (C) carries
`{model_name, opaque_bytes}`, a strict superset of what `forgedb-changefeed`
(`{model: &'static str, row_index, kind}`) and `forgedb-wal` (model name as "opaque routing
tag" + `Raw` bytes) already carry today. **Networking the changefeed does not by itself create
a drift vector** — the drift line is field-decoding, not the process boundary; a networked
broker that reads committed WAL `Raw` records and forwards them by model name decodes exactly
nothing more than the in-process feed does. The append-only-replay insight is correct and
load-bearing: read-only + whole-working-set + single-writer authority removes divergence/merge,
which is what keeps this a *replay*, not a generic sync engine. The "read-only replica, writes
proxy to server" v1 boundary is the right one and smuggles in nothing — it defers CRDT/conflict
machinery cleanly, and the note fences Phase-2 optimistic-local behind its own gate.

**Must-add constraints (bind these before/at implementation):**

1. **Watermark & framing stay opaque and index-based — CI-guardable.** The replication frame
   must be a compile-checked struct with **no field-typed member and no per-model variant**; the
   apply path must be idempotent by *absolute row index* (mirroring server WAL replay), never by
   decoded content. Guard-test that the wire type carries only
   `{tenant, model: &'static str, row_index, kind, opaque_bytes, watermark}` and that no
   `match model_name { ... field ... }` appears in the broker. This is the single highest-risk
   drift surface under implementation pressure ("just peek at the pk to dedup") — forbid the peek
   explicitly.
2. **Cross-model apply ordering must not become a schema-aware planner.** Open decision #2
   (global apply order / per-model watermarks mirroring the server `DatabaseSnapshot` commit
   boundary) is a real correctness gap, but the fix must live in the *generated* apply code or as
   an **opaque ordering token** in the frame — **not** a substrate that understands relations/FKs
   to sequence models. FK-consistency during catch-up is resolved by ordering, never by the
   transport inspecting references.
3. **The `forgedb-storage` facade must not break the published native surface.** The `cfg`-facade
   + `forgedb-storage-native` split (open decision #5) touches a *published class-1 crate with
   existing consumers*. Hard gate: the native surface stays byte- and API-identical, with the
   existing `forgedb-storage` tests passing unchanged against the facade before the split lands.
4. **Auth scoping is the security red line, and #72 must stay out — enforced, not assumed.**
   "A replica gets the whole tenant working set" means a scoping bug leaks the entire tenant to a
   browser. The replication endpoint must reuse the *same* `forgedb-auth` extractor/cross-check as
   the REST/WS routes (no parallel auth path), and row-level filtering is explicitly rejected at
   the transport layer (any per-user predicate is #72, generated, out of M1).
5. **Keep async at the boundary — enforce the no-async-coloring red line in codegen.** Make it a
   build invariant that the generated `get()`/traversal signatures are identical across `wasm32`
   and native (same source, two link targets). If the OPFS backend forces `async fn` up into
   generated code, the facade-vs-trait decision must be revisited before shipping — that is a
   drift signal, not an implementation detail.

**Scope note (not a blocker):** this promotes the previously-deferred **#62 Direction C**
(durable/networked broker) onto the critical path. That is a legitimate substrate evolution, but
it is the largest single build in the note and should be sequenced/gated as its **own substrate
milestone** (with the #56 `DatabaseSnapshot` commit-boundary semantics as the ordering contract)
rather than bundled loosely into "transport glue."
