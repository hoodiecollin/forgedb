# Configurable Runtime Behavior — Epic Design Note

Status: **batch 1 LANDED** (generate-time Tier A/B knobs, 2026-07-18); Tier-C
(process-start env) batch remains. Tracks the umbrella epic (#126) + child issues
that make ForgeDB's hardcoded runtime behavior configurable **without compromising
the generator identity**. This note is the durable framing; the concrete knobs live
as child issues.

## Status — batch 1 landed (generate-time Tier A/B)

The foundation and every generate-time knob baked into `database.rs` are done and
e2e-proven:

- **#127 foundation** — `GenConfig` (`crates/codegen/src/config.rs`, schema-blind,
  `DEFAULT` byte-identical except the broker) threaded via
  `RustGenerator::generate_with_config` using a thread-local set at the generate
  entry point (no invasive signature churn). CLI: `[runtime]`/`[storage]` tables in
  `src/config.rs` → `ForgeConfig::gen_config()`, threaded through `generate`/`build`.
- **#130 broker-attach gate** (flagship, Tier A, default OFF) — the double-barrier fix.
- **#129 fsync** (Tier B — see the nuance section; true Tier-A omit deferred),
  **#131** WAL checkpoint interval, **#133** compaction threshold, **#134**
  compaction-off (Tier A omit), **#135** changefeed capacity, **#150** cascade depth,
  **#132** txn-journal fsync + **#136** broker fsync/capacity (both ride the unified
  `[storage].fsync` / capacity knobs).
- **#128** — the dead `[database]`/`[api]`/`[dev]`/`[codegen]` scaffold tables removed.

Guard `test_gen_config_knobs` + `src/config.rs` mapping tests; e2e `scratchpad/config_e2e`
(default replication-OFF vs custom all-knobs, both compile + run — no `_replication.log`
when off, present when on). Snapshots re-accepted for the G6 broker-off exception.

**Remaining = the Tier-C batch** (read once at process start, a *different* mechanism):
#138–#142 (server body/WS/CORS/limit, host-port), #143–#146 (coordinator/txn), #147
(auth), #148–#149 (wasm), #151 (observability), #137 (broker retention).

## Why

Almost all of ForgeDB's runtime behavior is hardcoded today. The motivating discovery:
generated durable writes fsync with `FsyncPolicy::Always` (on macOS an `F_FULLFSYNC`
barrier, ~3.5 ms) with **no knob to relax it**, and `open_at` attaches a replication
broker that fsyncs a **second** barrier per write even for apps that never consume
`/replicate` (measured ~7.6 ms/insert ≈ 2× a single barrier — see `docs/BENCHMARKS.md`).
That is one instance of a systemic gap: WAL checkpoint interval fixed at 1000, compaction
threshold fixed at 1000, changefeed capacity fixed at 1024, coordinator timeouts fixed,
pagination `MAX_LIMIT` fixed, cascade depth fixed at 64, and more — none configurable, and
a `forgedb.toml` scaffold that *advertises* knobs (`page_size`, `compaction_threshold`,
`cors_origins`, …) that are consumed **nowhere**.

## The key reframe: "configurable" is a spectrum of *binding time*, not "runtime-mutable"

"Configurable runtime behavior" does **not** primarily mean "change behavior while the
process runs." For a code generator, the first-class reading is: **behavior dictated by
config that is locked at compile time**, so the Rust compiler can optimize around it. We
organize every knob by *when its value is bound*:

- **Tier A — generate-time code *specialization*.** Config changes **what code is
  emitted**, not just a constant's value. `fsync = never` ⇒ don't emit the `sync_all` call
  at all; `replication = off` ⇒ don't attach the broker. The compiler then optimizes as if
  hand-written — the branch *doesn't exist*, not just its value. This is the **most**
  identity-aligned mechanism: config joins the schema as a compile-time tailoring input,
  and there is strictly *less* for any runtime to interpret. Needs a regenerate to change.
- **Tier B — generate-time baked const.** Config sets a `const` the compiler propagates.
  Same code, tailored number. The bulk of knobs ("same behavior, different threshold").
- **Tier C — process-start runtime override.** Read **once** at `open_at`/`connect` (env
  or a flat struct), for deploy-environment knobs that must change without a regenerate
  (host/port, CORS origins, fsync-for-this-deployment). Flexible, but **forfeits the
  const-optimization** — the explicit tradeoff. Follows the #59 `FORGEDB_*` scaffold pattern.
- **Tier D — runtime-mutable control plane.** A live-tunable settings API. **Out of scope**
  (see Scope fences) — documented here because it is a *valid perspective to have
  considered*, not because we adopt it.

The invariant is about the **runtime**, not the generator: ForgeDB is *already* a generator
that makes schema-driven emit/omit decisions. Tier A simply adds a second compile-time
tailoring input (config) alongside the first (schema). It moves *away* from the
generic-engine failure mode, not toward it.

## Litmus test (apply to every proposed knob)

A knob is an in-bounds runtime-behavior config knob iff **all** hold:

1. **Schema-blind** — its meaning is independent of any model/field/relation. *Two apps
   with entirely different schemas could share it verbatim with identical effect.*
2. **Behavior-selecting, not logic-defining** — it picks a parameter/mode among behaviors
   the generated/substrate code already contains; it supplies no new logic, predicates,
   field lists, or routing.
3. **No runtime schema read** — honoring it never requires the process to inspect `.forge`
   or reflect over model structure.
4. **Scalar/enum, not an interpreted document** — a primitive or closed enum, not a nested
   options bag walked generically by generated code.

Fail any one → reject or redirect.

## Failure modes to reject (config as a back door)

- **FM-1** Config that changes *what data logic runs* (per-model predicates/filters/
  indexes/validations) → belongs in `.forge`.
- **FM-2** Config that requires the runtime to read the schema (e.g. a `forgedb.toml` table
  keyed by model name → forces a runtime model-registry lookup). Per-model tuning, if ever
  justified, is a **`.forge` directive the generator specializes off the AST**, never a
  model-keyed config table.
- **FM-3** The generic options bag: a `RuntimeOptions` struct threaded through generated
  code that starts branching generically to reconstruct tailored behavior — a runtime
  interpreter wearing a config hat.
- **FM-4** Runtime-mutable control plane (`PATCH /config`, hot-reload, per-request behavior
  switches like `?fsync=off`).
- **FM-5** Config as schema-shape smuggling (add a field/route/index/computed via config).

## Guardrails for Tier A (code specialization)

- **G1 Schema-blind.** A Tier A axis's *value meaning* is schema-independent. A per-model
  specialization decision is **not** a config knob — it is a `.forge` directive
  (`@soft_delete` already is exactly this), specialized off the parsed AST as normal codegen.
  (The *generator* consuming the knob is of course schema-aware; only the *knob's meaning*
  must be schema-blind.)
- **G3 No runtime residue.** A true Tier A specialization leaves its config value *absent*
  from the emitted code (the decision was consumed at generate time). Mechanically
  checkable: grep the generated output — the value should not appear.
- **G4 Tier A ⊥ Tier C per knob.** A knob is bound at exactly one time. A Tier A knob's
  omitted code isn't there to re-enable, so it **cannot** also accept a runtime override.
  Pick each knob's tier once; don't straddle. (This is *why* Tier A gets the optimization.)
- **G5 Combinatorial fence — orthogonal axes only.** N independent boolean specialization
  axes ⇒ 2^N possible programs. Keep the compile-test cost **O(N), not O(2^N)** by: (a)
  testing each axis on/off in isolation against a fixed baseline; (b) *requiring axes to be
  orthogonal* — two Tier A axes must not co-modify the same emitted region such that their
  combination differs from either alone; if they interact, **merge them into one enum axis**
  (enumerate the variants) or refactor the emission to be disjoint; (c) a new Tier A axis is
  admissible only if demonstrably disjoint from every existing axis.
- **G6 Default = today's emitted code, byte-identical.** Absent config, the generator emits
  exactly what it emits now (the insta snapshot tests enforce this for free). **One
  sanctioned exception:** the replication-broker attach defaults to *off* — a correctness/
  waste fix (removing an unused broker's fsync loses no data, since nothing is subscribed),
  not a durability weakening.
- **G7 Durability-weakening is loud opt-in.** `fsync = never` *omits* the barrier from the
  artifact — a stronger operator commitment than a runtime toggle. Such variants are never
  the default and must document their exact data-loss window.

### Implementation nuance for fsync specifically

`FsyncPolicy` is currently a **runtime enum** matched per-write inside the WAL writer
(`crates/wal/src/writer.rs:42`). So passing `FsyncPolicy::Never` from generated code is only
Tier B (a baked value, still a runtime branch in the substrate). Realizing true Tier A for
`fsync = never` (the barrier *gone* from the binary, compiler-optimized) requires either a
`const`-generic `FsyncPolicy` or emitting the sync call **conditionally at the generated
call site**. The child issue must choose; this note flags it so "Tier A fsync" isn't
assumed free.

**Decision (batch 1, #129): Tier B.** `[storage].fsync` binds the `FsyncPolicy` variant
baked into every `WalManager::open` (per-model WALs + txn journal) and the durable broker —
a working, default-byte-identical knob today. The substrate-side true-Tier-A form
(`sync_all` removed from the binary via a const-generic policy) is a deferred substrate
follow-up on #129, not required for the knob to function.

## Delivery mechanism (layered resolution)

Generate-time (Tier A/B, baked into the app's own code) is the default home; process-start
(Tier C) overrides only the deploy-environment subset; both plumb through substrate
constructor params where those already exist (`FsyncPolicy` on `WalManager`,
`ChangeFeed::new(cap)`, `DurableBroker::open(.., cap)`, `AuthConfig`) — for many knobs the
work is threading a value through **codegen**, not changing the substrate. New config lives
in `forgedb.toml` `[runtime]/[storage]/[server]` (schema-blind sections), **never** in
`.forge`.

## Taxonomy + per-knob tier calls

| Bucket | Knob | Tier | Note |
| --- | --- | --- | --- |
| Durability | fsync policy | **A** (`never` omit) + **B** (cadence) | see fsync nuance above; default Always, byte-identical |
| Durability | **replication-broker attach** | **A**, default **OFF** | flagship; fixes the double-barrier; G6 sanctioned exception |
| Durability | WAL checkpoint interval | **B** | threshold count; code always exists |
| Durability | txn-journal fsync | **A/B** | follows fsync policy |
| Storage | compaction dead-row threshold | **B** | trigger count |
| Storage | `compaction = off` | **A** | optional omit of the auto-trigger |
| Realtime | changefeed broadcast capacity | **B** | buffer size |
| Realtime | durable-broker capacity + fsync | **B** | buffer + `_replication.log` policy |
| Realtime | durable-log retention / `prune_through` | **B** + wiring | trigger not auto-wired today |
| Server | request-body-size limit | **C** | **absent today** (no `DefaultBodyLimit`) |
| Server | WS-frame-size limit | **C** | **absent today** |
| Server | CORS layer + origins | **C** | **absent today** (dead `cors_origins`) |
| Server | `MAX_LIMIT`/`DEFAULT_LIMIT` | **B** (opt **C**) | pagination clamp |
| Server | host/port/shutdown-drain | **C** | mostly env already; kill dead `[api]` table |
| Concurrency | coordinator `CONNECT`/`IO`/`TURN` timeouts | **C** | deploy-environment timing; resolves the dead-code warning |
| Concurrency | coordinator `MAX_FRAME` | **B** | wire frame cap |
| Concurrency | txn `max_retries` | **B** | wire residual `[transaction]` config |
| Auth | JWT leeway / alg-list / tenant-claim | **C** | mostly env; fix scaffold single-alg limit; reconcile dead `[auth]` table |
| Wasm | commit debounce (250 ms / 100 frames) | **B** | baked into `Replica`/worker bootstrap (keep bootstrap schema-agnostic) |
| Wasm | backend force-select (OPFS/IDB) | **B** | override the runtime probe |
| Misc | `MAX_CASCADE_DEPTH` | **B** | structural safety bound |
| Misc | log level/format, metrics toggle | **C** | mostly env (`RUST_LOG`, `FORGEDB_LOG_FORMAT`); formalize |

**Tier distribution sanity check:** Tier A is intentionally *small* (fsync `never`,
broker attach, optional `compaction=off`) — and those axes touch disjoint regions
(write-path sync vs `open_at` attach vs compaction trigger), so G5 holds with isolated
compile tests, no cross-product. Most knobs are Tier B ("same code, different number").
Tier C is the deploy-environment set. Tier D is empty.

## Scope fences — what this epic does NOT become

- Not a dynamic runtime-tunable control plane (no `PATCH /config`, no hot-reload).
- Not per-request behavior switches (no `?fsync=`, `?consistency=`).
- Not a generic settings engine / plugin config / interpreted options bag.
- Not a back door for schema-shaped config (fields/routes/indexes/validations/computed →
  `.forge`).
- Not a durability-default weakening — the epic adds *knobs*; safe defaults stay.

## v1-tier vs deferred

**v1-tier** (real discovered pain, clean scalar/enum knobs): the `forgedb.toml`
`[runtime]/[storage]/[server]` surface + generate-time plumbing (foundation); the
replication-broker attach gate (flagship); fsync policy + WAL checkpoint interval;
compaction threshold; the absent server safety knobs (body/WS/CORS) + `MAX_LIMIT`;
formalizing the already-env server/log knobs; **and removing the dead scaffold config**.

**Deferred** (land after the surface exists): changefeed capacity/retention/broker-fsync;
coordinator timeouts; auth JWKS/skew knobs (JWKS-over-HTTP is separate — #81); wasm
debounce/backend knobs; relaxed fsync *mode implementations* beyond wiring the policy point.

## Relationship to the perf-triage sweep

Some perf findings' fixes *are* config knobs (the double-barrier → broker-attach gate;
broadcast capacity). Most are algorithmic (junction indexing, projection wiring, lock
scope, buffered I/O) and are tracked as **separate perf issues**, cross-linked where they
meet this epic. See `docs/BENCHMARKS.md` for the seeded triage list.
