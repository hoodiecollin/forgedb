# Proposal: Product/Existence Calls — crud-api existence, fulltext trigram, ResponseCache, --config

**Status:** APPROVED 2026-07-06 — converted to impl tasks #31 (drop fulltext), #32 (drop ResponseCache), #33 (fold crud-api into generated handlers), #34 (build config feature). Orphan-crate disposition approved as recommended.
**Triage task:** #22
**Date:** 2026-07-06

## Summary
Three of the four items (crud-api's `CrudHandlers`/`CrudOperations`, fulltext's
`FullTextIndex`, and http-server's `ResponseCache`) are **orphaned runtime-library
surface**: they are not referenced by the CLI (`src/`), not by the code generators
(`crates/codegen`), and not emitted into generated code — the generated API uses `axum`
directly and its handlers are `// TODO: Implement` stubs. They are exactly the "library
users import at runtime" anti-pattern the generator identity forbids. **Drop** all three as
runtime crates; the *behaviors worth keeping* (existence→404 mapping; optionally FTS)
should be **generated**, not shipped as a runtime trait/engine. The `--config` feature is
different: config is a first-class part of ForgeDB's stated identity ("schema + CLI +
config") — **keep and build it** as generator configuration (output dir, targets, schema
path). No runtime library involved.

## Decision table
| Item | Current state | Decision | Generated or runtime? | One-line rationale |
|------|---------------|----------|-----------------------|--------------------|
| 1. crud-api existence check | Correct logic (`get`/`update`/`delete` map missing → `NotFound`), but the whole crate is orphaned; generated handlers are TODO stubs | **Redesign as generated** (drop the crate as runtime surface) | Generated | `CrudOperations`/`CrudHandlers` is a runtime trait users would `impl` — the anti-pattern. The existence→404 pattern belongs in generated `api.rs` handlers. |
| 2. fulltext trigram index | `trigrams()` implemented + tested, but `trigram_index` is **built and never queried** (search ignores it); whole crate orphaned | **Drop now** (redesign as generated later, if FTS is greenlit) | Generated (future) | Half-wired/aspirational runtime engine with zero consumers. FTS, if pursued, is generated code keyed off the existing `fulltext_indexed` schema flag. |
| 3. ResponseCache | Implemented (DashMap+TTL) but orphaned **and buggy** (`evict_oldest` is random; `invalidate_prefix` is a no-op) | **Drop** | Neither (deployment layer) | An HTTP response cache is runtime behavior. Caching belongs to the reverse proxy — `serve.rs` already shells out to nginx. |
| 4. `--config` feature | Flag parsed (`main.rs:24`) but ignored; `Config`/`CliError::Config`/`exit_code` unused | **Keep + build** | Generated-tooling config (compile-time) | Config is literally part of the identity ("schema + CLI + config"). Generator config (output dir, targets, schema path) fits perfectly. |

## Per-item analysis

### 1. crud-api existence check
- **Current state:**
  - The "existence check" is the handler layer mapping a missing record to a `NotFound`
    error: `crates/crud-api/src/handlers.rs:54-58` (`get` → `ok_or_else(NotFound)`),
    `:66-70` (`update` → `NotFound`), `:73-83` (`delete` returns `bool`, `false` → `NotFound`).
    The trait contract is `crates/crud-api/src/lib.rs:293-322` (`get`→`Option`,
    `delete`→`bool`, default `count`). This logic is **coherent and correct**.
  - **But the crate is orphaned.** No workspace consumer: `grep` for `crud_api` /
    `CrudOperations` across `src/` and `crates/codegen/src/` returns nothing. Only the
    crate's own tests use it. The root binary's `[dependencies]` does **not** include
    `forgedb-crud-api` (`Cargo.toml` — only parser/codegen/watcher/migrations/compaction).
  - The generated API that *should* embody this logic instead emits stubs:
    `crates/codegen/src/api.rs:83-150` — `get_*`/`list_*`/`create_*` handlers each contain
    `// TODO: Implement` and return `json!({ "data": null })`. There is no generated
    existence check at all.
- **Product question:** Should the existence→404 behavior exist (yes), and should it be a
  runtime trait crate users implement, or generated handler code?
- **Options:**
  - *Keep crud-api as-is* — leaves a runtime trait (`CrudOperations`) users would `impl`
    and a `CrudHandlers` wrapper they'd bundle. Directly contradicts the generator
    identity; also currently dead weight (0 consumers).
  - *Redesign as generated* — bake the existence→404 pattern into `ApiGenerator`'s handler
    output, replacing the TODO stubs. No runtime dependency; behavior is tailored per model.
  - *Remove entirely* — drop the crate and don't generate existence checks (regresses the
    REST semantics documented in `lib.rs:228-252`).
- **Decision:** **Redesign as generated.** Drop `forgedb-crud-api` as a runtime crate;
  fold its existence→404 semantics into the generated `api.rs` handlers.
- **Rationale:** A trait users `impl` at runtime is the exact "library" shape ForgeDB
  rejects. The behavior is valuable but must be *generated per-schema*, not imported. The
  crate is also currently unwired, so nothing regresses by removing it — this is a net
  simplification that also advances the "restore real API handlers" work (the same TODO
  stubs the OpenAPI/handler backlog already tracks).
- **Scope if actioned:** Delete `crates/crud-api` (3 source files + tests) and its
  `Cargo.toml` + workspace-member/`workspace.dependencies` entries in root `Cargo.toml`
  (2 lines). Separately (larger, own task): teach `crates/codegen/src/api.rs`
  `generate_handlers` to emit real get/update/delete bodies with the existence→404 mapping
  — 1 generator file, plus insta snapshot re-acceptance and a throwaway-crate compile check
  (per the codegen compile-test memory note). The crate deletion and the generator work are
  independent; deletion can land first.

### 2. fulltext trigram index
- **Current state:**
  - `Tokenizer::trigrams` is fully implemented and unit-tested
    (`crates/fulltext/src/lib.rs:65-79`; tests in `crates/fulltext/tests/`).
  - `FullTextIndex` maintains a `trigram_index` field (`lib.rs:87`), **populates** it on
    `add_document` (`lib.rs:124-129`), and reports its size in `stats()` (`lib.rs:284`).
  - **But nothing ever reads it.** `search` (`lib.rs:173-213`) and `search_phrase`
    (`lib.rs:216-265`) only consult the exact-term inverted `index`; the `trigram_index` is
    never queried. The doc comment's promise of "trigram-based indexing for substring
    matching" (`lib.rs:5`) is **aspirational** — the index is built and thrown away.
  - The whole crate is orphaned: no `src/` or `crates/codegen/` consumer; not in the root
    binary's `[dependencies]`. Generated code never references `FullTextIndex`. The parser
    *does* carry a `fulltext_indexed` bool per field (seen throughout
    `crates/codegen/src/rust.rs`), but codegen ignores it (always `false` in fixtures, no
    emission path).
- **Product question:** Should ForgeDB ship full-text search, and if so as a runtime
  in-memory index engine or as generated code?
- **Options:**
  - *Keep + finish trigram wiring* — invest in a runtime FTS engine users would run. Wrong
    shape (runtime library) and speculative (0 consumers).
  - *Drop now* — remove the orphaned crate; revisit FTS as a generated feature if/when
    prioritized.
  - *Redesign as generated (future)* — emit per-schema FTS index/query code for fields
    marked `fulltext_indexed`, tailored and zero-dependency.
- **Decision:** **Drop now.** Treat generated FTS as a separate, unscheduled product bet
  (not part of this triage).
- **Rationale:** A runtime inverted-index engine is a library, not generated output — and
  this one is half-built (trigram path dead) with no consumers. Deleting removes
  aspirational surface that misrepresents capability. The `fulltext_indexed` schema flag is
  the correct future hook, and it lives in the parser regardless of this crate, so dropping
  the crate loses no schema capability.
- **Scope if actioned:** Delete `crates/fulltext` (1 source file + 2 test files) and its
  workspace-member/`workspace.dependencies` entries (2 lines). Leave the parser's
  `fulltext_indexed` field intact (it's a schema concept, not tied to this crate). Optional
  follow-up (separate proposal, not scoped here): a "generate FTS from `fulltext_indexed`"
  feature.

### 3. ResponseCache
- **Current state:**
  - Implemented in `crates/http-server/src/cache.rs`: `ResponseCache` over `DashMap` with
    TTL (`CacheEntry::is_expired`, `:59-64`), size cap + eviction (`set_with_ttl`
    `:127-153`), and `stats`.
  - **Orphaned:** exported (`crates/http-server/src/lib.rs:278`) and unit-tested
    (`tests/cache_tests.rs`), but never constructed by the server (`server.rs` has no cache
    usage), the CLI, or codegen. Generated API code has no cache layer.
  - **Two correctness defects:**
    - `evict_oldest` (`cache.rs:176-189`) does **not** evict the oldest — there is no
      insertion/access timestamp ordering; it `retain`s over `DashMap` in unspecified order
      and drops the first ~10%. The method name and doc ("Evict oldest") are misleading;
      the inline comment even admits "simple random eviction."
    - `invalidate_prefix` (`cache.rs:162-168`) is a **no-op**: the `retain` closure always
      returns `true`, so nothing is invalidated despite the name. Any caller expecting
      prefix invalidation (e.g. bust `/api/users/*` after a write) gets silent staleness.
- **Product question:** Does an HTTP response cache belong in a *generated* API server, and
  is a hand-rolled runtime cache the right vehicle?
- **Options:**
  - *Keep + fix* — repair eviction (real LRU/timestamp) and `invalidate_prefix`, then wire
    it into the generated/served API. Adds runtime library surface and a correctness burden
    (cache invalidation) ForgeDB would own forever.
  - *Drop* — remove it; delegate caching to the deployment layer.
  - *Redesign as generated* — emit optional cache middleware per schema. Possible, but
    response caching is inherently runtime config/behavior, not compile-time-tailored logic.
- **Decision:** **Drop.**
- **Rationale:** A response cache is runtime behavior, not generated database code — it's
  the library shape ForgeDB avoids. It's also unwired and partly broken, so it advertises a
  capability that doesn't work. Caching belongs at the edge: `serve.rs` already launches
  **nginx** as a reverse proxy (`src/commands/serve.rs:316`), which does HTTP caching
  correctly and configurably. Owning a bespoke in-process cache (with its own invalidation
  bugs) is strictly worse.
- **Scope if actioned:** Delete `crates/http-server/src/cache.rs` + `tests/cache_tests.rs`,
  remove the `pub mod cache;` (`lib.rs:256`) and re-export (`lib.rs:278`) plus the two doc
  bullets (`lib.rs:201-202`). Confirm no other http-server module references it (grep shows
  none). Scope: 1 module + 1 test file removed, ~4 lines edited in `lib.rs`.

### 4. `--config` feature
- **Current state:**
  - The global flag is defined at `src/main.rs:22-24` (`config: Option<String>`, "Path to
    forgedb.toml config file") but is **never read** — nothing loads or acts on it.
  - The supporting dead symbols are `CliError::Config` (`src/error.rs:22-23`) and the
    unused `CliError::exit_code` scheme (`src/error.rs:41-56`, config → exit code 10).
  - **Coordination:** the raw *wire-vs-remove of the dead symbols* (`--config`/`Config`/
    `exit_code`) is owned by **triage #15**. This proposal (#22) owns the **product
    question**: should a config feature exist and what would it do. #15 should defer its
    `--config` disposition to this decision.
- **Product question:** In a *generator*, what would a config file configure — and is that
  worth having?
- **Options:**
  - *Remove the flag + plumbing* — simplest; drops `--config`, `Config`, and `exit_code`.
    But throws away a feature the identity explicitly calls for.
  - *Keep + build* — define `forgedb.toml` as generator configuration: default schema path,
    default output dir, which targets to emit (rust/typescript/api/stubs), and codegen
    options. Loaded once, supplies defaults for `generate`/`build`/`validate` flags.
- **Decision:** **Keep + build.** `--config` should load a `forgedb.toml` that configures
  code generation defaults.
- **Rationale:** ForgeDB's own identity statement is "end users need only: their schema,
  the `forgedb` CLI, and **config**" (`CLAUDE.md`). Config is not scope creep — it's the
  third pillar. Crucially, it's *compile-time generator* config (paths, targets, codegen
  options), **not** runtime configuration of a shipped library, so it's fully aligned. A
  `forgedb.toml` that removes the need to repeat `--output`/`target` flags is real DX with
  zero runtime footprint. The `init` command already writes project scaffolding and is the
  natural place to emit a starter `forgedb.toml`.
- **Scope if actioned:** Define a `Config` model (schema path, output dir, enabled targets,
  codegen options) + a TOML loader; wire `--config` (and default `./forgedb.toml`
  discovery) to populate command defaults in `src/commands/generate/mod.rs`, `build`,
  `validate`. Decide the precedence rule (explicit CLI flag > config file > built-in
  default). Optionally emit a starter `forgedb.toml` from `init`. Scope: ~1 new config
  module, edits to 3 command modules + `main.rs`, `init` template addition. Hand the
  dead-symbol cleanup coordination to #15 (which should now *wire* rather than *remove*
  these symbols).

## Proposed impl tasks
1. **Delete `crates/crud-api`** (runtime-library surface, 0 consumers): remove the crate,
   its workspace-member line, and its `workspace.dependencies` entry in root `Cargo.toml`.
2. **Delete `crates/fulltext`** (orphaned, trigram path dead): remove the crate + its two
   workspace entries. Leave the parser's `fulltext_indexed` field untouched.
3. **Delete http-server's cache** (`cache.rs` + `cache_tests.rs`), and drop `pub mod cache;`
   / the re-export / the two doc bullets from `crates/http-server/src/lib.rs`.
4. **Generate real API handlers with existence→404** in `crates/codegen/src/api.rs`
   `generate_handlers` (replace the `// TODO: Implement` stubs); re-accept insta snapshots
   and compile the output in a throwaway crate. (Independent of task 1; larger.)
5. **Build the `--config` feature**: add a `Config` model + TOML loader, wire `--config`
   and `./forgedb.toml` default discovery into `generate`/`build`/`validate` defaults, and
   emit a starter `forgedb.toml` from `init`. Coordinate with triage #15 so the
   `Config`/`exit_code`/`--config` symbols are *wired*, not removed.

## Open questions for the user
1. **Blast radius of dropping three crates.** `forgedb-crud-api`, `forgedb-fulltext`, and
   `forgedb-http-server` — is any of these already published to crates.io or referenced by
   downstream/example projects outside this repo? (In-repo they're orphaned; external
   consumers would change the calculus. Note: only `types`/`storage`/`wal` are documented as
   published.)
2. **Is `forgedb-http-server` as a whole still the intended serving path?** We're only
   dropping its cache module here, but `serve.rs` orchestrates external nginx + Bun
   processes and the generated API uses `axum` directly — clarify whether the axum
   `http-server` crate is a runtime dependency of generated apps (library shape) or an
   internal dev/serve helper. That answer may widen this proposal.
3. **Full-text search as a product bet.** Do you want a *generated* FTS feature (driven by
   the `fulltext_indexed` schema flag) on the roadmap, or is FTS out of scope entirely? This
   determines whether task 2 is a plain deletion or the first step of a redesign.
4. **`forgedb.toml` surface.** What should config actually cover — just generation defaults
   (schema path, output dir, targets), or also codegen options (formatting, feature toggles
   like soft-delete defaults)? Where should config discovery look (cwd only vs. walk up)?
