# Legacy Audit — Step 0 Findings (epic #94)

**Status:** AUDIT COMPLETE 2026-07-10 · Tracking epic [#94](https://github.com/hoodiecollin/forgedb/issues/94)
**Baseline at audit time:** `cargo build --workspace` green; 461 tests pass.

## Purpose

On reviving ForgeDB we inherited a substantial implementation and are not confident all
of it is product-identity aligned or load-bearing. This is the **step-0 sweep** of epic
#94: classify every crate in `crates/`, the CLI `src/`, and notable public APIs into
exactly one bucket, then file one child prune issue per confirmed **Dead** /
**Product-misaligned** / **Deferred-scope-creep** target. **Keep** items are recorded with
rationale so they are never re-audited; genuinely ambiguous items are listed as **NEEDS
DECISION** for a human (no issue filed).

The **identity test** applied to every target (from `CLAUDE.md`): a published/shipped crate
must be either (1) **schema-agnostic substrate** the generated code links against, or (2)
**transport/access glue** over the already-generated surface. Generated per-schema code is
fine. Rejected: a generic runtime engine/ORM that interprets a schema at runtime; a
self-describing record/format (`model_name` + `HashMap<field, Value>`) in a substrate crate;
runtime `model_name`/schema dispatch; anything hollowing generated code into a shipped
generic library.

**Completed exemplar:** [#95](https://github.com/hoodiecollin/forgedb/issues/95) — pruned the
`forgedb-wal` structured/transaction API (`WalValue`, `WalOperation::{Insert,Update,Delete}`
field maps, `Transaction`/`replay_committed`) down to the opaque `Raw` byte path → wal 0.2.0.
Both problems it fixed recur as the two failure modes this audit hunts: (1) a self-describing
field-map record format in class-1 substrate (identity drift), and (2) a live-but-unconsumed
API encoding a deferred capability (transactions = Direction C). **Already done — not re-filed.**

## Classification table

| Target | Bucket | Rationale | Consumer evidence |
|---|---|---|---|
| `forgedb-wal` structured/txn API | Dead + misaligned | field-map record in substrate + Direction-C txns | **DONE (#95)** — opaque `Raw` only, wal 0.2.0 |
| `crates/query-optimization` | **Dead** | speculative planner IR; not in codegen, generated code, CLI, or the v1 roadmap/#90 | only self + a root `[dev-dependencies]` line; zero `forgedb_query_optimization` refs in `crates/`+`src/` |
| `crates/http-server` | **Dead** | full general-purpose axum server lib (`Server`, auth hooks, rate-limit, TLS, health, metrics) that **duplicates** what generated `api.rs` emits; generated code builds its own router | zero real symbol consumers (`forgedb_http_server::` = 0 hits outside its own crate); only root `[dev-dependencies]` + stale doc-comment links; `serve` spawns the *generated* binary, not this crate; scaffold `init.rs` does not pin it |
| `storage::Database` + `open_with_wal`/`wal`/`wal_mut`/`has_wal`/`save_manifest`/`get_manifest` | **Dead** (partial-surface) | convenience `Database` in the storage crate; generated code wires columns + WAL directly and never touches it | zero consumers outside `crates/storage/` (the `Database::new()` codegen hits are the *generated* Database, unrelated); only `storage`'s own tests/`basic_usage.rs` use `Database::open` |
| `crates/query-params` | Keep (not-yet-wired) | explicitly named in `V1_ROADMAP.md` Phase 2 (#90) as the filter/sort/paginate wiring | unused today; only referrer is the (dead) http-server; roadmap-planned → not dead |
| `crates/ffi` | **Keep-as-planned** (PM) | category-2 bindings anchor for #51/#52/#53 (transport glue over the *generated* surface); `forgedb_version()`-only stub does no identity harm today | zero consumers today; KEEP per PM verdict — tracked on the #94 epic comment |
| `validate --implementations` | Keep (documented no-op) | `#[allow(dead_code)]` placeholder pending `@computed` (#88); epic seed asked to confirm | accepted flag, no-op until #88; keep per prior triage #15 WIRE call |
| `crates/lsp-server` | Keep | `.forge` editor tooling; the `vscode-forgedb` extension spawns the `forgedb-lsp` binary as its language server | `vscode-forgedb/src/extension.ts` + `out/extension.js` spawn `target/{debug,release}/forgedb-lsp` via `LanguageClient` |
| `crates/parser` | Keep | pipeline stage: lexer→AST for every generate/validate | used by `codegen`, `validation`, CLI |
| `crates/codegen` | Keep | the product — emits database.rs/types.ts/api.rs/stubs/openapi | used by every `generate`/`build` path |
| `crates/validation` | Keep | schema validation stage | used by `parser`, `migrations`, CLI |
| `crates/migrations` | Keep | `forgedb migrate` CLI | `src/commands/migrate.rs` |
| `crates/compaction` | Keep | `forgedb compact` CLI (+ Phase-4 auto-compaction #92) | `src/commands/compact.rs`, `storage` |
| `crates/backup` | Keep | `forgedb backup` CLI; class-1 snapshot substrate (#57) | `src/main.rs`, `src/commands/backup.rs` |
| `crates/watcher` | Keep | `forgedb dev` file watcher | `src/commands/dev.rs` |
| `crates/changefeed` | Keep | class-1 field-blind broadcast substrate; linked by generated code (#62) | `crates/codegen/src/rust.rs` (`attach_changefeed`, `emit`) |
| `crates/auth` | Keep | class-1 verify-only JWT/tenant substrate (#59); linked by generated router + scaffold | `crates/codegen/src/api.rs`, `src/commands/init.rs` |
| `crates/storage` (columns/tombstones/readers/Snapshot/DirLock/Manifest) | Keep | core columnar substrate the generated code links against | published 0.1.5; generated `*Storage`/readers |
| `crates/types` | Keep | core type system substrate | published 0.2.0; used everywhere |
| `crates/wal` (opaque `Raw` path post-#95) | Keep | durable write substrate (#89) | published 0.2.0; generated durable write path |

## Keep set (with rationale — do not re-audit)

- **`parser` / `codegen` / `validation`** — the generation pipeline itself. Non-negotiable.
- **`migrations` / `compaction` / `backup` / `watcher`** — each backs a live CLI subcommand
  (`migrate` / `compact` / `backup` / `dev`); `compaction`+`backup` are also class-1 substrate
  with a Phase-4 (#92) wiring path.
- **`changefeed` / `auth`** — class-1 substrate **actually linked by generated code** (proven:
  `attach_changefeed`/`emit` in `codegen/src/rust.rs`; `forgedb_auth::Authenticator`/`Principal`
  in `codegen/src/api.rs` + `init.rs`). Identity-clean; keep.
- **`storage` / `types` / `wal`** — the published schema-agnostic substrate the generated code
  links against. (Note: `storage::Database` *convenience wrapper* is Dead — see below — but the
  columnar/reader/snapshot/lock/manifest surface is load-bearing.)
- **`query-params`** — unused today but **explicitly Phase 2 (#90)** in `V1_ROADMAP.md`. This is
  *not-yet-wired-but-planned*, not dead. Do not prune; wire it in #90.
- **`lsp-server`** — live `.forge` editor tooling; consumed by the `vscode-forgedb` extension
  (spawns `forgedb-lsp`). Schema-agnostic authoring transport; keep.
- **`validate --implementations`** — keep as the documented `#[allow(dead_code)]` no-op until the
  `@computed` expression convention (#88) lands. (Confirms the epic-seed question: keep, not remove.)

## RESOLVED by PM gate (was NEEDS DECISION)

- **`crates/ffi` → KEEP-AS-PLANNED** (PM verdict, 2026-07-10). Legitimate **category-2** access/
  transport: bindings expose the *already-generated* schema-specific surface to another language,
  which the identity model explicitly blesses. It is the anchor for the language-bindings work
  (Python #51 / Node #52 / Deno #53). No identity harm today — a `forgedb_version()`-only stub that
  links nothing and reflects over no schema; unlike `http-server` it has no wrong-direction gravity
  (the correct future shape is the direction it already points). Do **not** prune. Tracked on the
  epic (#94 comment) so the next audit reads it as the bindings anchor, not an orphan. Design-review
  gate lives on #51/#52/#53: bindings must expose generated exports, never a generic schema-reading
  ABI that reconstructs queries at runtime.

## Confidence notes for the human

- **`http-server` = Dead → PM-CONFIRMED PRUNE** (verdict 2026-07-10). Zero real consumers; the
  generated `api.rs` builds its own axum router, so nothing links it. `CLAUDE.md` *listed* it as
  "class-1 substrate" peer to `changefeed`/`auth`, but that claim is provably false: those two are
  linked by generated code; `http-server` is not, and its `Server`/auth/rate-limit/TLS surface
  **duplicates** generated output rather than being linked by it. The PM verdict: the only "aligned"
  future (routing behind a shipped generic http-server crate) is exactly the hollowing-out the
  identity forbids — so there is no KEEP-AS-PLANNED path; it is dead duplication with wrong-direction
  gravity. **Prune scope adds** correcting the CLAUDE.md class-1-substrate line and clearing the two
  stale doc-comment "class-1 substrate peer" mentions in `crates/auth/src/lib.rs` and
  `crates/validation/src/lib.rs`. Tracked on **#98**.

## Issues filed

- **#95** (prior) — wal structured/txn API. *Done exemplar; not re-filed.*
- **#97** — Prune `crates/query-optimization` (Dead).
- **#98** — Prune `crates/http-server` (Dead; PM-confirm caveat).
- **#99** — Prune `storage::Database` convenience wrapper (Dead, partial-surface).

`crates/ffi` → **KEEP-AS-PLANNED** (PM-resolved; tracked via the #94 epic comment as the bindings
anchor for #51/#52/#53, no standalone issue). No issue filed for any **Keep** item.
</content>
</invoke>
