# Proposal: Wire-vs-Remove the 9 Dead-Code Items

**Status:** APPROVED 2026-07-06 — converted to impl tasks #34–#40. Reconciliation: `--config` is WIRED (not removed) per triage #22's "build the config feature" call; `CliError::exit_code` WIRED. All other WIRE/REMOVE calls approved as written.
**Triage task:** #15
**Date:** 2026-07-06

## Summary
Of the 9 confirmed-dead items, the split is **4 WIRE, 4 REMOVE, 1 SPLIT** (item 6 wires
its exit-code half and removes its `--config` half). The WIREs are all real
compile-time/authoring-tooling wins for a generator CLI — schema validation of component
refs and computed fields, distinct CI exit codes, and struct-aware LSP. The REMOVEs are
incoherent knobs (`--no-db` on a database generator), misleading always-true no-ops
(`init --typescript`), a stale duplicate template with a wrong `Database::new` signature,
and redundant LSP bookkeeping that duplicates the map key and inline lock access.

## Decision table
| # | Item | Location | Decision | One-line rationale |
|---|------|----------|----------|--------------------|
| 1 | `build --no-api` | `src/commands/build.rs:9`, `src/main.rs:108` | **WIRE** | Skipping the optional REST layer is a real want; API is a layer, DB is the core. |
| 2 | `build --no-db` | `src/commands/build.rs:9`, `src/main.rs:111` | **REMOVE** | Incoherent — the database is the reason a database generator exists. |
| 3 | `init --typescript` | `src/commands/init.rs:9`, `src/main.rs:47` | **REMOVE** | Defaults `true`, gates nothing; there is no TS scaffolding to toggle. |
| 4 | `validate --implementations` | `src/commands/validate.rs:8` | **WIRE** | AST has `is_computed`; validating computed-field impls is compile-time safety (needs impl-location convention). |
| 5 | `validate --components` | `src/commands/validate.rs:9` | **WIRE** | AST has `ComponentReference.path`; check referenced tsx/jsx/api files exist. |
| 6 | `CliError::exit_code` / `--config` flag | `src/error.rs:42`, `src/main.rs:23` | **SPLIT** | WIRE exit codes (CI scriptability); REMOVE the ignored `--config` no-op (see #22). |
| 7 | `rust_main_template` | `src/templates.rs:181` | **REMOVE** | Stale duplicate; uses wrong `Database::new(db_path)?` (generated `new()` takes no args). |
| 8 | LSP `Document.uri`/`version` + `get_document` | `crates/lsp-server/src/main.rs:29,32,48` | **REMOVE** | `uri` duplicates the map key; `get_document` duplicates inline lock access; `version` unused. |
| 9 | LSP `Struct.name`/`fields`/`position` | `crates/lsp-server/src/parser.rs:22-26` | **WIRE** | Inline structs are a real, already-parsed schema feature; make completion/hover/goto struct-aware. |

## Per-item analysis

### 1. `build --no-api`
- **Current state:** Field `no_api: bool` on `BuildOptions` (`src/commands/build.rs:8`),
  parsed at `src/main.rs:108`, threaded into the struct at `main.rs:311`, and then never
  read. `build::run` hardcodes `generate::run(... target: "all" ...)` (`build.rs:28-33`)
  and runs `cargo build` on everything. Clippy: "fields `no_api` and `no_db` are never read".
- **Intended behavior:** Skip generating/compiling the REST API layer (`api.rs`) so an app
  that only wants the typed DB doesn't carry an axum server.
- **Decision:** WIRE.
- **Rationale:** The generator emits layered artifacts — `database.rs` (core), `types.ts`,
  `api.rs` (REST), and stubs. The REST API is an optional layer; many embedders want the
  typed DB and nothing else. Letting `build` produce the app without the API is a genuine,
  generator-aligned scope toggle (it changes *what code is generated*, not runtime behavior).
- **Scope if actioned:** `build.rs` — replace the hardcoded `target: "all"` with a target
  set derived from `no_api` (e.g. run `generate` for rust/typescript/stubs, skip `api`).
  `generate::run` currently takes a single `target: String`; wiring cleanly needs either a
  target-set input or `build` issuing multiple single-target `generate` calls. 1 file
  (2 if `generate` is extended). No new user-facing surface.

### 2. `build --no-db`
- **Current state:** Field `no_db: bool` (`src/commands/build.rs:9`), parsed at
  `src/main.rs:111`, threaded through, never read.
- **Intended behavior:** Skip building the database layer.
- **Decision:** REMOVE.
- **Rationale:** ForgeDB *is* an application database generator. A `build` that omits the
  database produces nothing meaningful — this flag contradicts the product's reason to exist.
  There is no coherent artifact set that is "everything except the database."
- **Scope if actioned:** Delete the field in `build.rs`, the `#[arg]` in `main.rs`, and the
  destructure/threading in `main.rs`. 2 files, mechanical.

### 3. `init --typescript`
- **Current state:** `InitOptions.typescript` (`src/commands/init.rs:8`), declared with
  `#[arg(long, default_value = "true")]` (`main.rs:46`), threaded in, never read. `init`
  scaffolds a Rust project (`create_rust_files`) but has **zero** TypeScript scaffolding.
  Clippy: "field `typescript` is never read".
- **Intended behavior:** Presumably scaffold a TS SDK-consumer project (package.json,
  tsconfig, an entry that imports the generated `types.ts`).
- **Decision:** REMOVE.
- **Rationale:** The flag is an always-true no-op that implies TS is set up when nothing is
  emitted — actively misleading. Scaffolding a TS consumer is a legitimate future DX feature,
  but it must be designed deliberately (what files, what runtime assumptions), not surfaced
  as a dangling boolean. Removing now stops the lie; the feature returns with real behavior
  when built.
- **Scope if actioned:** Delete the field in `init.rs`, the `#[arg]` in `main.rs`, and the
  destructure/threading. 2 files, mechanical.

### 4. `validate --implementations`
- **Current state:** `ValidateOptions.implementations` (`src/commands/validate.rs:8`),
  parsed at `main.rs:85`, threaded in, never read. `validate::run` returns early in
  schema-only mode and otherwise only runs name/relation lint. Clippy: "fields
  `implementations` and `components` are never read". The AST **does** model computed fields:
  `Field.is_computed` (`crates/parser/src/ast.rs:111`, `@computed` directive).
- **Intended behavior:** Fail/warn when a `@computed` field has no provided implementation
  (the CLI already advertises "Fail on unimplemented computed/views" via `--strict`).
- **Decision:** WIRE (pending one convention decision — see open questions).
- **Rationale:** Compile-time validation is a core green-light for a generator CLI: catch
  missing computed-field implementations before codegen rather than at `cargo build`. The
  schema construct already exists in the AST, so this is wiring a real check, not inventing
  a runtime feature.
- **Scope if actioned:** `validate.rs` — add a pass over `schema.models[].fields` filtering
  `is_computed`, and check each against wherever implementations are expected to live. 1
  file, plus a small convention decision blocking the check (open question #1).

### 5. `validate --components`
- **Current state:** `ValidateOptions.components` (`src/commands/validate.rs:9`), parsed at
  `main.rs:88`, threaded in, never read. AST models component refs concretely:
  `ComponentReference { protocol, path, relations }` (`crates/parser/src/ast.rs:82-88`),
  with `FieldType::Component(...)` (`ast.rs:63`) and protocols tsx/jsx/api.
- **Intended behavior:** Verify that files referenced by `tsx://` / `jsx://` / `api://`
  component refs actually exist on disk (broken-ref detection at validate time).
- **Decision:** WIRE.
- **Rationale:** This is shovel-ready compile-time validation — `ComponentReference.path` is
  a concrete file path. Catching a dangling component ref at `validate`/`dev` time is exactly
  the kind of tooling that strengthens the generate-then-compile model. No runtime surface.
- **Scope if actioned:** `validate.rs` — walk fields for `FieldType::Component`, resolve each
  `.path` (relative to the schema file), and error/warn on missing files, gated by the flag
  (and honoring `--strict`). 1 file, self-contained.

### 6. `CliError::exit_code` + `Config` variant + ignored `--config` flag
- **Current state:** `CliError::exit_code()` (`src/error.rs:42-56`) maps each variant to a
  distinct code but is **never called** — `fn main() -> Result<()>` (`main.rs:255`) relies on
  `Result`'s default `Termination`, which prints the `Debug` error and always exits `1`.
  Clippy: "method `exit_code` is never used". The global `--config` flag
  (`main.rs:22-24`) is parsed into `cli.config` and **never read** (only the field
  declaration matches in grep). `CliError::Config` (`error.rs:23`) is never constructed
  anywhere; nothing in the CLI reads `forgedb.toml` at runtime (only `init` *writes* it).
- **Intended behavior:** (a) distinct process exit codes per failure class; (b) point the
  CLI at an alternate config path and have commands honor it.
- **Decision:** SPLIT — **WIRE** the exit-code scheme; **REMOVE** the `--config` flag. Keep
  the `CliError::Config` variant (it is referenced by the `exit_code` match and is a
  reasonable error class to retain).
- **Rationale:** Distinct exit codes are a real CLI-DX/CI win (a pipeline can branch on
  validation-fail=2 vs build-fail=4) and cost almost nothing — pure green-light tooling.
  The `--config` flag, by contrast, is an accept-and-silently-ignore no-op: there is no
  config-loading system behind it, so it misleads. Whether ForgeDB should load `forgedb.toml`
  at all is a broader product question owned by triage **#22** — this task removes the dead
  flag now; if #22 decides to build config loading, the flag returns with real behavior.
- **Scope if actioned:** WIRE half — introduce a thin `run() -> Result<()>` and have `main`
  call `process::exit(err.exit_code())` on error (1 file, `main.rs`; keep `error.rs`). REMOVE
  half — delete the `config` field + `#[arg]` in `main.rs` (1 file). Cross-reference #22 in
  the commit.

### 7. `rust_main_template`
- **Current state:** `templates::rust_main_template()` (`src/templates.rs:181-203`) is
  defined and never called (grep: only the definition). `init::create_rust_files`
  (`src/commands/init.rs:135-159`) writes its **own** inline `main.rs` string instead. The
  two diverge: the template uses `Database::new(db_path)?`, but generated code defines
  `pub fn new() -> Self` with **no args** (`crates/codegen/src/rust.rs:126,567`) — so the
  template would not compile against generated output, while the inline version
  (`Database::new()`) is correct. Clippy: "function `rust_main_template` is never used".
- **Intended behavior:** A single source of truth for the scaffolded `main.rs`.
- **Decision:** REMOVE the template function; keep `init`'s inline (correct) version.
- **Rationale:** This is stale duplication, not a missing wire. The dead copy carries a wrong
  `Database::new` signature; promoting it would break generated projects. Consolidation is
  desirable, but consolidate onto the *correct* string, which already lives in `init`.
- **Scope if actioned:** Delete `rust_main_template` from `templates.rs`. 1 file. (Optional
  follow-up: if a shared constant is wanted, extract `init`'s inline string into
  `templates.rs` — but that is cleanup, not required to clear the warning.)

### 8. LSP `Document.uri` / `Document.version` + `get_document`
- **Current state:** In `crates/lsp-server/src/main.rs`: `Document.uri` (line 29) and
  `Document.version` (line 32) are populated in `update_document` (lines 57,59) but never
  read — handlers take `uri` from request params and re-derive the map key
  (`docs.get(&uri.to_string())`). `get_document` (lines 48-51) clones a whole `Document`
  and is never called; every handler instead holds the read lock inline
  (`self.documents.read().await` at lines 143,156,171,202). Clippy: "fields `uri` and
  `version` are never read" + "method `get_document` is never used".
- **Intended behavior:** `get_document` was a convenience accessor; `version` could gate
  stale/out-of-order document updates.
- **Decision:** REMOVE all three.
- **Rationale:** `uri` duplicates the `HashMap<String, Document>` key; `get_document`
  duplicates the inline lock pattern *and* is strictly worse (it clones the full `content`
  String); `version` is bookkeeping with no reader. A stale-update guard keyed on `version`
  is a legitimate future LSP-correctness improvement, but it is speculative and should be
  added deliberately when needed, not left as dead scaffolding. Trimming keeps the internal
  schema-authoring tool lean.
- **Scope if actioned:** `crates/lsp-server/src/main.rs` — drop `uri` and `version` from the
  `Document` struct and its construction, delete `get_document`. 1 file. (If the stale-guard
  is desired instead, that is a separate WIRE — see open questions #2.)

### 9. LSP `Struct.name` / `Struct.fields` / `Struct.position`
- **Current state:** `Struct { name, fields, position }`
  (`crates/lsp-server/src/parser.rs:22-26`) is parsed and pushed into `Schema.structs`
  (`parser.rs:89`), but `schema.structs` is never consumed — grep finds no `.structs` reads
  in `completion.rs`, `hover.rs`, or `diagnostics.rs`. Clippy: "fields `name`, `fields`, and
  `position` are never read". Inline structs are a documented, supported schema feature
  (CLAUDE.md schema reference; AST `Struct` at `crates/parser/src/ast.rs:117`).
- **Intended behavior:** Struct-aware editor support — completion of struct names/fields,
  hover, and go-to-definition on struct references.
- **Decision:** WIRE.
- **Rationale:** Inline structs are a real schema construct the LSP already parses; dropping
  the parsed data would regress toward a schema-authoring tool that silently ignores part of
  the language. Extending completion/hover/goto to consume `schema.structs` is exactly the
  schema-tooling DX that reinforces the generator model. This is a modest feature, not a
  runtime library addition.
- **Scope if actioned:** More than a one-line wire — implement struct-awareness in the LSP:
  `find_model_definition`-style lookup extended to structs for goto (`main.rs`), struct-name
  completion (`completion.rs`), and struct hover (`hover.rs`). ~2-3 files in
  `crates/lsp-server/src/`. Reuses existing `Struct.position`/`name`/`fields`.

## Proposed impl tasks

### WIRE tasks
1. **Exit-code scheme (item 6a).** Add `run() -> Result<()>` in `src/main.rs`; have `main`
   call `std::process::exit(err.exit_code())` on `Err`. Keep `error.rs` as-is. (1 file)
2. **`validate --components` (item 5).** In `src/commands/validate.rs`, add a pass over
   `FieldType::Component` fields that resolves `ComponentReference.path` relative to the
   schema file and errors/warns on missing files; gate on `components`, honor `--strict`.
   (1 file)
3. **`validate --implementations` (item 4).** After open-question #1 is answered, add a pass
   over `is_computed` fields that checks each has a resolvable implementation; gate on
   `implementations`, honor `--strict`. (1 file; blocked on convention)
4. **`build --no-api` (item 1).** In `src/commands/build.rs`, derive the generate target set
   from `no_api` instead of hardcoding `"all"`. If `generate::run` needs to accept a target
   set (rather than a single string), extend it. (1-2 files)
5. **LSP struct-awareness (item 9).** Consume `Schema.structs` in
   `crates/lsp-server/src/{main.rs,completion.rs,hover.rs}` for goto-definition, completion,
   and hover on structs. (2-3 files)

### REMOVE tasks
6. **`build --no-db` (item 2).** Delete the field (`build.rs`), `#[arg]` and threading
   (`main.rs`). (2 files)
7. **`init --typescript` (item 3).** Delete the field (`init.rs`), `#[arg]` and threading
   (`main.rs`). (2 files)
8. **`--config` flag (item 6b).** Delete the global `config` field + `#[arg]` in
   `src/main.rs`; cross-reference triage #22 in the commit message. Keep `CliError::Config`.
   (1 file)
9. **`rust_main_template` (item 7).** Delete the function from `src/templates.rs`. (1 file)
10. **LSP `Document` bookkeeping (item 8).** Remove `uri`/`version` fields (and their
    construction) and `get_document` from `crates/lsp-server/src/main.rs`. (1 file)

Tasks 6-10 are mechanical dead-code removals and can land as one focused `chore:`/`refactor:`
commit each (or grouped by area: build/init/main flags, templates, lsp). Tasks 1-5 are
`feat:` and each carry a snapshot/behavior check.

## Open questions for the user
1. **Computed-field implementation location (blocks task 3 / item 4).** Where is a
   `@computed` field's implementation expected to live — an `impls/` directory, a companion
   `.rs` file, an `@impl` directive/path, or an `api://` component ref? `validate
   --implementations` cannot check "is it implemented?" until this convention is defined.
   Until then item 4 is WIRE-pending; if the convention is deemed out of scope, item 4 flips
   to REMOVE.
2. **LSP stale-update guard (item 8).** Confirm we do *not* want to keep `Document.version`
   for out-of-order `did_change` protection. Recommendation is REMOVE now and add a proper
   version guard later if the LSP shows stale-diagnostic bugs. If you'd rather keep it,
   item 8 becomes a small WIRE (compare incoming vs stored version before applying).
3. **`build` target selection surface (item 1).** Preferred shape: keep `--no-api` on
   `build`, or drop it and steer users to `generate <targets>` + `build`? Recommendation is
   WIRE `--no-api` on `build` (natural app-build toggle), REMOVE `--no-db` regardless.
4. **`--config` vs triage #22.** This proposal removes the dead `--config` flag now. Confirm
   that the broader "should the CLI load `forgedb.toml`" decision stays with #22; if #22 is
   likely to build config loading imminently, we could instead leave the flag and wire it
   there rather than remove-then-readd.
