# Proposal: WASM Runtime Target (browser / IndexedDB)

**Status:** DESIGN NOTE — product-gated. `forgedb-product-manager` verdict: **aligned-with-constraints** (2026-07-06). Awaiting maintainer approval to schedule.
**Issue:** [#50](https://github.com/hoodiecollin/forgedb/issues/50) (`plan-next`, `idea`)
**Date:** 2026-07-06

## Summary

Make **`wasm32-unknown-unknown` a compilation target for the already-generated
`database.rs`**, backed by IndexedDB, for offline-first browser apps. This is a *new
compile target for tailored generated code* — **not** a runtime engine and **not** a
generic browser SDK. Nothing reads a `.forge` schema at runtime.

The design rests on one load-bearing observation: **positional I/O over an in-memory byte
buffer is naturally synchronous.** IndexedDB is async and offset-less, but if each column is
an in-memory arena (`Vec<u8>` / `ArrayBuffer`) *hydrated from IndexedDB on open and flushed
to IndexedDB on commit*, then async is quarantined to two boundary calls and the per-row API
the generated code calls (`append_uuid`, `read_string(index)`, …) stays synchronous and
**unchanged**. A file column and an ArrayBuffer column are the same bytes, so semantics are
byte-identical across targets. That is what lets the generated data logic be emitted **once**
and linked against two backends.

Three strata, mapping onto the two legitimate published-runtime classes from the identity
guard (`CLAUDE.md` → "What ForgeDB is") plus generated code:

| Stratum | What it is | Guard class |
|---|---|---|
| **A. Generated** `database.rs` (insert/get/traversal/filters) | The app's tailored logic, recompiled for `wasm32` — same source | Generated code (the product) |
| **B. `forgedb-storage-idb`** — IndexedDB column backend | Schema-agnostic; moves opaque typed columns, knows no model | Class-1 *substrate* |
| **C. `wasm-bindgen` transport** — JS/TS glue | Exposes the *generated* surface (`insert(user)`, `userPosts(id)`) to browser JS | Class-2 *transport* |

**Milestone success criterion (also the identity proof):** one existing generated crate runs
unmodified in a browser over IndexedDB, and **no artifact anywhere reads a schema at
runtime.** If achieving it requires a WASM-specific generator branch or a schema-aware
backend, the design has drifted and must be revisited before shipping.

## Product verdict & invariant mapping

Aligned *as framed* — "generating WASM-targetable code, not shipping a runtime library" is
the invariant restated. Two things make it "with constraints":

1. The title word **"runtime"** is a trap; the deliverable is a *compile target*, not a runtime.
2. IndexedDB's async, offset-less nature is a forcing-function that can quietly push the
   design toward "just interpret the schema in JS." The shape below exists to resist that.

Invariant check (schema is a *compile-time input to generation*, never a *runtime input to a
generic engine*):
- Schema stays compile-time: WASM is `cargo build --target wasm32-…` on the generated crate.
- IndexedDB backend is schema-agnostic substrate (class-1).
- JS/TS glue exposes only the generated per-schema surface (class-2), like the existing
  `crates/ffi` C ABI.

Every published artifact lands in a sanctioned bucket; no generic engine appears.

## Architecture

### A. Generated code — unchanged, recompiled

The existing `RustGenerator` output is the *same code*, recompiled for `wasm32`. **There must
be no WASM generator branch that changes the data logic.** The tailored logic is generated
once and is target-agnostic. Reverse/M2M traversal stays the existing linear scan
(`Storage::all()` over hydrated arenas = in-memory iteration); the append-only limits carry
over unchanged (no `delete`, no M2M `unlink`, tombstones-based deletion).

### B. `forgedb-storage-idb` — schema-agnostic substrate (the crux)

A new crate implementing the **same column interface** `forgedb-storage` exposes today
(`crates/storage/src/lib.rs:251-651`): `FixedColumn` (`append_u32`/`read_uuid`/…),
`VariableColumn` (`append_string`/`read_string` + offsets), `Tombstones`.

- **In-memory arenas.** Each column is a byte buffer. Positional reads/writes are offset math
  over the buffer — identical to the file engine.
- **Async quarantined to two boundaries:** `hydrate()` (load column blobs from IndexedDB on
  open) and `commit()` (write dirty buffers back). The per-row API stays **synchronous**.
- **Signature preservation trick:** the file constructors take a `PathBuf`
  (`FixedColumn::new(PathBuf, size)`, `VariableColumn::new(data_path, offsets_path)`). In the
  browser the same path string becomes the **IndexedDB blob key**. Same signatures ⇒ the
  generated `FixedColumn::new(PathBuf::from(col_path), size)` compiles **without codegen
  changes**.
- **Knows no schema.** No `match model_name`, no field/relation awareness. Opaque columns only.

**Backend selection — recommended: a `forgedb-storage` facade + cfg.** Turn the current
engine into `forgedb-storage-native` and make `forgedb-storage` a thin facade:

```rust
#[cfg(target_arch = "wasm32")]      pub use forgedb_storage_idb::*;
#[cfg(not(target_arch = "wasm32"))] pub use forgedb_storage_native::*;
```

Generated code keeps `use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};`
verbatim and stays **byte-identical across targets**. This is preferred over a
`StorageBackend` trait for milestone 1 because a trait risks **async coloring** the per-row
API (see red lines). A trait can be revisited later if a use case needs both backends in one
binary. *(Trade-off to validate: the facade must not break the published `forgedb-storage`
API for existing native consumers — the re-export must be surface-compatible.)*

**Commit lifecycle.** The generated `Database` needs an explicit **`commit()`** that flushes
all column buffers + tombstones. This is a small *additive, target-agnostic* codegen change —
natively it's the explicit `flush()` that storage task S4 already moved toward; in the browser
it wraps one IndexedDB `readwrite` transaction (all-or-nothing). This is the only generated
change and it is not a fork.

### C. `wasm-bindgen` transport — class-2 glue

A thin JS/TS layer (same spirit as `crates/ffi`) exposing the *generated* surface plus
lifecycle: `open()`, `insert(record)`, `get(id)`, generated traversals (`userPosts(id)`),
`commit()`. It exposes **only what codegen already produced**; it invents no query surface.
Ship via `wasm-pack` with generated `.d.ts` (mirrors the Node/Deno bindings direction in
#52/#53).

## Red lines (reject on sight)

- A **wasm blob that ingests a `.forge` schema or serialized manifest at runtime** and
  dispatches generically. That *is* the generic engine.
- A **"ForgeDB browser SDK"** offering `db.query("User").where(…)` / CRUD over models
  discovered at runtime. A generated `userPosts()` is fine; a generic `.query(modelName)` is
  the ORM we forbid.
- Backend **B growing schema knowledge** (any model/field/relation awareness).
- A **divergent WASM generator** that reimplements insert/traverse semantics. One generated
  surface, two link targets.
- **Making the storage interface `async`** and letting it color the generated per-row API
  (`get()` becoming `async` everywhere). The native path would pay for the browser path, the
  shared interface fractures, and pressure builds toward runtime schema interpretation. Keep
  async at the load/commit boundary only.

## Open decisions the implementation must fix

1. **Working-set / hydration scope.** Milestone 1: **whole-DB hydrate into memory on open**,
   documented as a deliberate constraint (fine for offline-first small apps). Lazy/partial
   hydration deferred.
2. **Persistence granularity.** **One IndexedDB database, one (or few) object store(s),
   keyed blobs** — key `"{model}/{fixed|variable}/{column}"`, value = ArrayBuffer, mirroring
   the on-disk `fixed/…bin` / `variable/…bin` layout. Variable-length **offsets** and
   **tombstones** each get their own key. *Not* one IndexedDB database per model.
3. **WAL.** **Drop the file WAL on this target** for milestone 1; IndexedDB transactions are
   themselves atomic/durable, so wrap each `commit` in one `readwrite` transaction. Durability
   guarantee to state explicitly: **durable at `commit` granularity; uncommitted in-memory
   mutations are lost on tab close.** A WAL object store can be added later if intra-session
   crash recovery is needed — but do not port the file-offset WAL.
4. **Backend selection** — facade+cfg (recommended) vs `StorageBackend` trait. Decided above;
   revisit only if a single binary must hold both backends.
5. **Compaction / tombstone growth in a tab session** — when (if ever) `compact` runs
   in-browser. Deferred; named here so it isn't forgotten.

## First milestone (smallest slice that proves the model *and* the guard)

**In scope**
- `forgedb-storage-idb`: fixed + variable + tombstone interface over in-memory arenas with
  `hydrate()`/`commit()` over IndexedDB, single object store, keyed-blob layout.
- `forgedb-storage` facade + `forgedb-storage-native` split (surface-compatible for native).
- Additive generated `Database::commit()`.
- One **UUID-keyed** example schema (2 models + 1 relation from `examples/`) compiling to
  `wasm32` against the IDB backend with **zero data-logic codegen changes**.
- Thin `wasm-bindgen` transport: `open`, `insert`, `get`, one relation traversal, `commit`.
- Browser smoke test (Playwright / Chrome-DevTools MCP already available): open → insert →
  commit → **reload page** → `get` returns the row (proves offline persistence round-trips).

**Explicitly out**
- WAL, in-browser compaction, **integer-PK models** (traversal is UUID-only today anyway),
  partial/lazy hydration, the full `examples/` corpus, M2M, subscriptions/change-feed.
- Whole-DB hydrate and commit-granularity durability are **accepted, documented limits.**

**Success = one existing generated crate runs unmodified in a browser over IndexedDB, with no
artifact reading a schema at runtime.** Needing a WASM generator branch or a schema-aware
backend is the drift signal.

## Load-bearing references

- `crates/storage/src/lib.rs` — the interface the IDB backend must mirror (positional
  `read_*`/`append_*` at `:251-651`; `Database::open`/`open_with_wal` at `:658-692`) and the
  sync `io::Result` + `FileExt` positional-I/O assumption async IndexedDB must be reconciled
  against.
- `crates/codegen/src/rust.rs` — storage call sites the target inherits (`:51-52` imports,
  `:319-386` column construction from paths, `:444-555` append, `:570+` read); the place an
  additive `commit()` and the facade dependency are wired.
- `crates/ffi/src/lib.rs` — the existing sanctioned class-2 transport pattern the
  `wasm-bindgen` layer mirrors.
- `CLAUDE.md` → "What ForgeDB is" — the invariant, plus the append-only / linear-scan /
  UUID-only-traversal limits this note inherits.
