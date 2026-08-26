# ForgeDB Architecture

**Audience:** contributors and anyone reasoning about how ForgeDB is put together.

This is the system narrative. The authoritative crate inventory is the "Workspace layout"
section of [`CLAUDE.md`](../CLAUDE.md); the substrate catalog is [PUBLIC_CRATES.md](./PUBLIC_CRATES.md);
the stability policy is [SEMVER.md](./SEMVER.md).

---

## What ForgeDB is

ForgeDB is an **application-database generator** — a compile-time code-generation tool, **not**
a runtime ORM or query engine. A declarative `.forge` schema is transpiled into tailored Rust
database code plus a TypeScript SDK, a REST API, and an OpenAPI spec. End users need only
their schema, the `forgedb` CLI, and config.

**The invariant.** The app's schema is a *compile-time input to generation*, never a *runtime
input to a generic engine*. The schema-specific surface — types, tables, queries, filters,
relations, API routes — is generated and tailored per app. ForgeDB never ships a general-purpose
library that reconstructs that surface at runtime by reflecting over a schema.

Generated code is not dependency-free — it links the schema-agnostic **substrate** crates
(storage, wal, types, …; see [PUBLIC_CRATES.md](./PUBLIC_CRATES.md)) — but it never depends on a
ForgeDB ORM or a runtime that reads the user's schema. A generated, schema-tailored
query/filter builder is fine (it is just generated code); a generic, schema-agnostic query
builder is not.

---

## Generation pipeline

```
schema.forge
   │
   ▼
forgedb-parser        lexer → tokens → AST (crates/parser/src/ast.rs)
   │
   ▼
forgedb-validation    semantic checks (types, relations, directives)
   │
   ▼
forgedb-codegen       one generator per artifact:
   ├─ RustGenerator        → database.rs   (storage, CRUD, indexes, relations, txns)
   ├─ ApiGenerator         → api.rs        (axum REST + WS routes)
   ├─ CorePackage          → core/         (database.rs as a library crate — everything links it)
   ├─ ServerPackage        → server/       (api.rs + a generated main.rs)
   ├─ Napi/Pyo3/Ffi/Wasm   → the four wrapper packages over core/
   ├─ GoGenerator          → go/           (cgo source over the ffi staticlib)
   ├─ TypeScriptGenerator  → types.ts      (typed SDK client)
   ├─ StubGenerator        → placeholder stubs README  (no UI/component codegen today)
   ├─ OpenApiGenerator     → openapi.json  (offline OpenAPI 3.1 document)
   ├─ {Rust,Python,Go}SdkGenerator → the three REST client SDKs (opt-in)
   ├─ TransformGenerator   → transform-<a>-<b>/  (offline data-migration bin)
   └─ EngineGenerator      → engine-<a>-<b>/     (engine byte-format hop bin)
```

**The output has two destinations, and which one a file goes to is not a preference.** Every
Rust file ForgeDB compiles is written into the project's build cache; the output directory
holds the text a user reads, commits and imports, plus a read-only mirror of
`database.rs`/`api.rs`. See [What it builds](#what-it-builds-and-who-runs-cargo).

Rust output is built with `quote!` + `prettyplease` and snapshot-tested with `insta`
(`crates/codegen/tests/`). **Snapshot pass ≠ output compiles** — codegen changes are also
compile-tested by generating for a real multi-model schema and `cargo check`ing the emitted
crate. That discipline is load-bearing (it has caught real codegen bugs; see `CLAUDE.md`).

The CLI (root crate `forgedb`, `src/`) orchestrates the pipeline: `src/main.rs` (clap),
`src/commands/*` (one module per subcommand). Commands: `init`, `generate`, `validate`, `build`,
`dev`, `migrate`, `compact`, `backup`, `tenant`, `coordinate`.

### Which project is this? (`src/project.rs`)

A **project** is the unit of build cache, lockfile and target directory. It is not a
directory the user declares: a `.forge` schema declares an *app*, and a `forgedb.toml`
carries knobs for the apps beneath it plus one statement about grouping.

Everything follows from that premise, and each point below is a place an obvious
implementation is wrong:

- **The walk starts at the schema, never at the CWD.** Otherwise `cd` changes an app's
  project id, and therefore its build-cache key — an invocation artifact leaking into a
  fact about the tree.
- **One walk, two answers.** Knobs come from the *nearest* config; identity from the
  *project root* — the nearest `isolated = true` config, else the outermost. In any
  monorepo those are different directories, so code that resolves "the project config"
  once and uses it for both compiles, runs, and mis-keys the cache silently. `Chain`
  exposes `nearest()` and `project_root()` and no way to ask for "the" config.
- **An absent `isolated` means `true`.** Inverting `bool`'s own default is deliberate:
  grouped-by-default would let a root config added for an unrelated reason absorb every
  app beneath it into one lockfile. `ProjectConfig`'s `Default` impl is hand-written for
  the same reason — the `#[serde(default)]` on an absent `[project]` table does not reach
  the field-level default.
- **The ledger detects; the config records.** A resolved id collision is written into the
  colliding project's *own* `forgedb.toml`. A resolution held in `~/.forgedb` would be
  erased by the GC that owns that directory, and the collision would come back as a silent
  merge rather than an error.
- **The walk has a stop boundary** — a repository root, `$HOME`, or the filesystem root —
  or a stray `~/forgedb.toml` captures every project on the machine.
- **Unknown config keys are errors.** Ignoring them was never forward compatibility: a knob
  an older CLI does not know reads as applied and is not, and a misspelled `[projekt]`
  merges two projects through the mechanism meant to protect them.

Identity resolves at the project root in two branches, and **both are collision-free by
construction**:

1. `[project].id`, generated once by `forgedb init` and committed with the config
2. otherwise, a hash of the root's **absolute** path

That second one is deliberately asymmetric with the member hash below, which is over a
*project-relative* path: a member must resolve identically on another machine, while a
fallback project id only has to be unique on this one.

#### Why the id is minted rather than derived (`forgedb project show`)

It used to be derived — `[project].name`, else a single ecosystem manifest's package name,
else the path hash. Two of those three could produce the same id for two unrelated
projects, and the id keys `~/.forgedb/projects/<id>/`: one build cache, one `Cargo.lock`,
one `target/`. So an entire apparatus grew around the possibility. A claim ledger to detect
it. An ambiguity branch for two manifests naming one root. Two collision diagnostics, one
for an unrelated project holding the name and one for the project's own ghost after a move.
A `forgedb project claim --take-over` remedy for the second, a `release` inverse for
symmetry, a `forgedb project name` to record a chosen answer, an `init --project-name` for
the CI case that could not answer a prompt, and a consent boundary so a diagnostic could
offer to perform the write.

**Every one of those was a consequence of derivation, not of the ledger.** Minting the id
at `init` deleted all of them at once: there is nothing to disambiguate, because a package
name is no longer an identity source; nothing to contest, because a generated id is not a
name two projects can independently arrive at; and nothing to record, because the answer
exists before the first `generate` rather than being negotiated at it.

What remains:

- **`forgedb project show`** — reports the facts and decides nothing. It must work in
  precisely the cases resolution does not, so it reports rather than resolves. It is also
  the only non-mutating window onto the ledger, and it names the cache directory the id
  keys — previously findable only by knowing the layout.
- **The ledger, as a pure detector.** A minted id still collides one way: a project
  directory is copied and the copy inherits the original's `[project].id`. That is worth an
  error, and it gets one — naming the holding root, the config file and the key to change,
  and suggesting a freshly minted value. There is no remedy *command*, because the remedy
  is a one-key edit in a file the user owns.
- **The boundary** — `Askability`, a pure predicate over four booleans (stdin is a
  terminal, stderr is a terminal, not `--quiet`, not `forbid()`-ed). The four exist for
  four different reasons and must not be collapsed: `--print-artifact` deadlocks a
  `$(…)` capture on a prompt *reading* stdin, not on one writing stderr. `ask::forbid()`
  latches the contexts that are wrong to ask from however the process was started —
  `build`'s machine-readable modes and `dev`'s watch loop. It outlived the identity
  questions because `migrate create` still asks about values only an operator knows.

Two rules that survive the change:

- **"Cannot ask" and "declined" are the same path.** The result is the *unchanged*
  diagnostic and exit status; a prompt only ever fills an answer that is otherwise absent.
  There is no timeout-and-default and no blanket `--yes`: a prompt that answers itself is
  the silent-guess failure with extra steps.
- **The ledger stays a detector, and identity stays in the project.** The ledger records
  *who currently holds an id*, which is machine-local state a GC may empty at any time. The
  id itself is a committed fact in the project's own `forgedb.toml`, or wiping
  `~/.forgedb` would change what a project *is*.

### Where it builds (`src/cache.rs`)

`~/.forgedb/projects/<id>/` is a cargo workspace **ForgeDB owns**: a virtual manifest, one
member per app, one `Cargo.lock` and one `target/` shared by every member. Sharing those
is the entire point — it is what makes the substrate compile once per *project* rather than
once per *app*.

```
~/.forgedb/projects/<id>/
  Cargo.toml            # the workspace ROOT — virtual, rewritten in full each time
  Cargo.lock            # one resolution shared by every app
  target/               # one target dir shared by every app
  apps/<member-hash>/   # one CONTAINER per app — a marker file, and no manifest
    core/ server/ napi/ pyo3/ ffi/ wasm/        # the members are one level deeper
    transform-<a>-<b>/ engine-<a>-<b>/
```

- **A member path is a pure function of `(project id, project-relative schema path)`**, so
  the cache needs no index of its own contents and a clone or CI runner resolves to the
  same directory.
- **The hash is hand-rolled FNV-1a, pinned by golden vectors, and never `DefaultHasher`** —
  which is explicitly not stable across Rust releases, and `cargo install` builds with the
  user's toolchain, so `rust-toolchain.toml` does not reach them. Re-keying every member on
  a rustup upgrade presents as "ForgeDB got slow" and is nearly undiagnosable. #366 is that
  mistake already shipped elsewhere in this repo.
- **`resolver = "3"` is correctness, not cosmetics.** A virtual manifest without it falls
  back to resolver 1, which unifies features *more* aggressively than 2/3 — making C11's
  cross-app coupling worse exactly where the design contains it — and warns on every cargo
  invocation in a directory the user never opened.
- **The root manifest is derived, never remembered.** `write_workspace_root` takes the
  whole member set; there is deliberately no "add a member" entry point, because a patched
  manifest accumulates entries for apps whose schemas were deleted and that state survives
  a regenerate.
- **The member set accretes from disk.** Generating one app has no knowledge of its
  siblings, so the live set is rebuilt each time from the member directories whose recorded
  schema still exists — a `stat` per member rather than a subtree scan of the user's repo.
- **An empty live set refuses to GC**, rather than reporting every member as garbage. The
  dangerous case is a GC run reached from an error path that produced no members.
- **Nothing in the cache is an input.** Deleting it and regenerating reproduces identical
  generated *source*. It may reproduce a different dependency *resolution* — that is C1 as
  scoped by #343 §4, and nothing may depend on the lockfile surviving.
- **A data root that resolves inside the cache is refused** (C4). The trap is not a bad
  decision but a relative default: `[tenant].root` defaults to `"data"`, so running from
  inside the cache dir puts a database there without anyone choosing it. The refusal is
  **generated code in the server's own `main.rs`**, not documentation: the population that
  would hit it is exactly the population following a path ForgeDB printed.

---

### What it builds, and who runs cargo

`src/naming.rs` · `src/commands/build/driver.rs` · `crates/codegen/src/{core_pkg,server_pkg}.rs`

#335 moved one more thing behind the CLI: **ForgeDB owns the build.** `forgedb build` used to
run a bare `cargo build` in the current directory and was right by coincidence in exactly one
scaffold shape — point it at a directory holding an unrelated crate and it compiled *that*,
printed `✓ Compiled database`, and exited 0. `forgedb init` no longer scaffolds a cargo package
at all, and there is nothing left for a user to `cargo build` by hand.

**One app becomes a layered set of packages, not one crate**, each a member of the project
workspace above:

| Package | Target | Emitted when | Holds |
|---|---|---|---|
| `core/` | rlib | any Rust target is declared | the one `database.rs`, verbatim as `src/lib.rs` |
| `server/` | bin | the API target is declared | `api.rs` + a generated `main.rs` |
| `napi/` `pyo3/` `ffi/` `wasm/` | cdylib (`ffi` also staticlib) | that runtime is declared | the wrapper only |
| `transform-<a>-<b>/` `engine-<a>-<b>/` | bin | `migrate build` / `migrate engine` | version-stamped databases |

**Layering is a correctness property, not a build-time optimization.** Before it, five code
paths emitted `database.rs` and two of them emitted a *different* database: `generate_all`
threaded the app's `GenConfig` while the four binding arms called
`generate_with_schema_version`, i.e. `GenConfig::DEFAULT`. Under default config all five are
byte-identical, which is why it went unnoticed — set `[storage] fsync = "never"` and one
`generate` run wrote two databases with different durability semantics. With one `core` that
every wrapper links, the divergence is *unrepresentable* rather than merely fixed. The wrappers
pin **zero** substrate crates and reach the substrate through `core`'s re-exports, which is what
makes their `Uuid`/`ColumnExport` types genuinely unify instead of happening to resolve to the
same version in one lockfile.

The two class-C packages deliberately **do not** link `core`: a hop must be pinned to the
version range it was planned for, not to whatever the current schema happens to be.

**Every name is derived, in one place** (`src/naming.rs`): `<app-name>-<kind>` for packages and
bins, and a per-app prefix for the FFI C symbols. The app name is `<project-id>_<path
segments…>` — `acme_services_blog` — so uniqueness is **structural** rather than probabilistic:
two distinct relative paths cannot reduce to the same segment list. That is also why the file
name alone cannot do the job; `forgedb init` writes `schema.forge` for every project it
scaffolds, so a stem-derived name would be the same constant for every app in a project.

**The member *directory* is still keyed by `member_hash`, and only it.** It is an internal
storage key nobody reads, so it keeps the stability a path-derived name gives up — adding an app
can shorten or lengthen a sibling's name under `Minimal`, re-keying its packages and changing
every exported C symbol, which is the accepted price (`[project].symbol_naming = "uniform"`
narrows it to renames caused by *moving* a schema, and cannot eliminate it). The hash reaches no
public name, and `tests/cache_dir_test.rs` asserts it does not leak into generated source.

Two forces make derivation non-optional. Cargo package names cannot begin with a digit, so a
segment that does is rescued by an `app` prefix. And cargo only *warns* on a duplicate artifact
name, exits 0 and leaves one file, which is how one app's `transform/` and `engine/` shipped
declaring the same bin: the CLI could run the wrong hop over a user's data at exit 0. ForgeDB
refuses what cargo tolerates — a `cargo metadata --no-deps` pass collects every
bin/cdylib/staticlib name before any compile, and a duplicate is a hard error naming both
packages.

**The member set is derived by a scan of the cache**, expanding each live container into the
subdirectories that hold a `Cargo.toml`. This is not the downward walk of the *user's* tree the
epic rejects: it is one `read_dir` per live container, on the same axis `live_members` already
walked. A declared set cannot work (generating app A cannot know app B's targets); a recorded
set is the second record `write_workspace_root` exists to refuse; a glob detonates on one stray
directory. Two ordering rules follow, and cargo's failure modes are what force them:

- **Reserve before emission, render the root after.** A root rendered from a scan *before*
  anything is written lists the previous run's packages, so an app's first `generate` produces a
  root without its own packages. `place()` therefore split into `reserve()` (make the container,
  before emission) and `sync_root()` (scan, expand, render, after).
- **De-list before deleting.** A member the root names but that does not exist is
  **project-wide fatal** — every app in the project, not just the one being pruned. A package on
  disk that the root does not name is inert. So a prune rewrites the root first and deletes
  after, and the prune judges the **declared** target set (`[generate].targets`, which is why it
  is required) rather than the target one invocation selected — otherwise `generate rust` would
  delete `napi/`.

`default-members` is derived by **filtering the already-computed `members` vector in the same
function**, excluding `wasm`/`transform-*`/`engine-*` so a bare `cargo build` at the cache root
does not fail on the replica's `wasm32`-only imports. Two derivations is how a skew happens, and
a `default-members` that is not a subset of `members` is project-wide fatal.

**The driver is the only thing that runs cargo** (`src/commands/build/driver.rs`), split into a
pure `plan()` + `parse_artifacts()` and an impure `execute()`. `forgedb build --plan` prints the
invocations and compiles nothing, which is both the C7 answer ("show me what you are about to run
in a directory I cannot see") and the seam that makes the driver testable without mocking cargo —
mocking it would encode the same misunderstanding of cargo the change exists to fix.

- **One invocation per target triple**, not per package: `--target` is invocation-wide, so a
  package set containing the replica cannot be expressed in one call. The split is on the target
  axis, which is forced; splitting per package would forfeit the shared graph.
- **`[profile.*]` is never emitted into a member manifest**, because cargo silently ignores it
  there (`warning: profiles for the non root package will be ignored`) — three shipped scaffolds
  carried one that read as applied and was not. The profile floor lives on the invocation instead:
  every call carries `--config 'profile.release.panic="unwind"'`, which **beats** a machine-wide
  `$CARGO_HOME/config.toml` that would otherwise turn every FFI panic into a process abort, and
  the wasm32 call alone carries the whole-graph `opt-level="s"`. The flag form is chosen over the
  environment variable because it is visible in what `--plan` prints.
- **Artifact paths are read out of cargo's JSON message stream and existence-checked**, never
  composed by joining `target/release/…` — `CARGO_TARGET_DIR` and `[build] target-dir` move it
  machine-wide, which is #292. Kind matters: an rlib reports both `.rlib` and `.rmeta`, and `ffi`
  reports three filenames, so Go delivery has to filter for the **staticlib** specifically.

**Where generation writes.** Every Rust file ForgeDB compiles goes to the cache, and the cache
copy is the only one a ForgeDB-driven build reads. `[generate].output` keeps its meaning and
holds the text a user reads, commits and imports — `types.ts`, `openapi.json`, `stubs/`, the REST
SDKs, `go/` — plus a **mirror** of `database.rs`/`api.rs` written from the *same* `GeneratedCode`
value that wrote the cache copy. One value, two writes: two writes of one value cannot drift,
which is precisely how the shipped `generated/database.rs` vs `generated/ffi/src/database.rs`
disagreement happened.

Two rules keep that honest:

- **Nothing in the cache is user-editable**, and every cache manifest is rewritten in full on
  every generate. The only-when-absent rule that protects a user's `go.mod` is exactly wrong for
  a file in a directory the user never opens, where a stale substrate pin would never be reached
  by a CLI upgrade. For the same reason the project records the CLI version that wrote it and
  **drops `Cargo.lock` when that changes** — a rewritten pin re-resolves, but a newly published
  patch under an unchanged pin does not.
- **ForgeDB never leaves a Rust file it generated in a place it has stopped writing.** When the
  four wrapper directories left `output/`, the files there would otherwise have stayed frozen,
  never regenerated, and **still compilable** — green CI against a database that no longer tracks
  the schema. Instead the file's contents are replaced by a `compile_error!` naming what happened,
  idempotently, leaving the user-editable scaffolds beside it untouched.

### What it delivers, and what makes a stale artifact loud (#337)

`src/commands/build/deliver.rs` · `src/fingerprint.rs`

Building an artifact nobody can reach is the same as not building it. **Delivery is a
projection of the build report**: for every reported artifact whose package kind has a
destination, the file is copied out of the cache and into that app's `output`, beside the
generated text that describes it, and every delivered path is printed (C7).

| Package | Artifact | Destination | Delivered name |
|---|---|---|---|
| `napi` | cdylib | `<output>/napi/` | `forgedb.node` |
| `pyo3` | cdylib | `<output>/pyo3/` | `_forgedb_native.abi3.so` |
| `ffi` | **staticlib** | `<output>/ffi/` | `libforgedb.a` |
| `ffi` (again) | staticlib | `<output>/go/` | `libforgedb.a` |
| `core` `server` `wasm` `transform-*` `engine-*` | — | none | — |

- **The match over `PackageKind` is total, with no wildcard arm**, so adding a kind is a
  compile error rather than a silent non-delivery. The undelivered kinds are listed
  explicitly: an absent arm and an empty arm read alike and mean opposite things.
- **Delivery joins no path.** Every one of them is read from the report, which was built
  from cargo's own JSON stream — #292's defect class, one layer down.
- **The delivered name is always a rename.** Cargo writes `lib<pkg>.dylib`; CPython will not
  import a `.dylib` and Node requires a `.node`. The pyo3 name is *composed* from
  `PyO3Generator::EXTENSION_STEM`, because CPython resolves `PyInit_<stem>` from the
  delivered filename — the name and the `#[pymodule]` function are one decision, and a
  second spelling is how they come apart.
- **`ffi` delivers the archive, not the cdylib**, and Go's row is the same archive at a
  second destination. Dynamic delivery was measured and rejected: rustc stamps an
  **absolute** `LC_ID_DYLIB`, so a consumer that *links* a copied dylib records the cache
  path and dangles the moment the cache is GC'd — which C8 permits at any time and which CI
  cannot see, because the cache still exists while it runs. Node and CPython are unaffected:
  they `dlopen` their extension by path and record no dependency on it, verified with
  `otool -L`/`ldd` rather than assumed.

**Both halves carry a fingerprint of the source they were generated from**, so a consumer
that loads an artifact built from different source than the code beside it gets a named
error instead of a method set that silently no longer matches.

- **It is over generated SOURCE, never the CLI version.** An upgrade that changes nothing
  about the output must invalidate nothing, or the error trains people to ignore it.
- **One per (app, package)**: the app's `core` plus that package's own directory, manifests
  included, since a substrate pin change alters the compiled artifact while leaving every
  `.rs` byte identical. Per-app would couple targets that have nothing to do with each other,
  and would let a `migrate build` invalidate every binding in the app.
- **The value is computed from what the run is about to emit, before any of it reaches
  disk** — not by scanning the container afterwards, which describes the *previous* run.
- **The constant lives in each wrapper, never in `core`.** `core/src/lib.rs` is the exact
  `String` also written to `<output>/database.rs` as the mirror, and a `mod fingerprint;`
  line there would name a file that does not exist beside it.
- **Where each half checks the other:** Node's `index.js` and Python's `forgedb.py` compare
  at `require`/`import` and throw with the remedy; Go compares in package `init()`; C gets
  the value as a macro plus an inline check that is **advisory — ForgeDB generates no C that
  executes, so nothing can force it to run.**

What Go's `init()` buys over the linker is narrower than "load-time verification" suggests:
a schema change that removes a symbol already fails to link. It adds a readable message, and
the cases the linker cannot see at all — a `[storage]`/`[runtime]` knob that changes
durability semantics without changing one symbol name.

**Generated text is committed; compiled output is ignored**, by a `.gitignore` ForgeDB
writes *inside* the directory it owns. Extensions only — never a directory, never `*.rs`,
never a manifest — because #338 writes a ForgeDB-owned cargo package into the consumer's
tree and that package is committed source. It re-includes `*.js` and `*.d.ts`, without which
the shims are uncommittable: the scaffolded root `.gitignore` ignores both project-wide.

**Limits, stated so they are not discovered.** The fingerprint proves *source* identity, not
artifact identity: it does not cover `Cargo.lock` (project-wide, and a `= "0.3"` pin that
resolved 0.3.1 and 0.3.2 fingerprints alike), the compiler, the profile or the target triple.
It is not provenance — anything that can write the artifact can write the constant. Only one
platform's artifact can occupy a directory at a time, which is the direct consequence of
ignoring compiled output: **CI needs the Rust toolchain for any in-process consumer.** And
`forgedb dev` will fail the check continuously for such a consumer, because the watcher
regenerates and never compiles — the report is honest, and #335's choice to keep cargo out of
the watcher stands.

**`migrate` joined the cache with everything else**, so it is no longer CWD-relative: every
subcommand takes a required `--schema`, the transformer and engine hops are range-stamped members
compiled by the same driver, and there is no fallback to a `migrations/transform` beside you — a
fallback would emit a `[package]` under whatever foreign workspace root the shell is standing in
(#328), on the *mandatory* engine-upgrade command. `forgedb migrate up` was removed rather than
re-homed; the per-tenant sweep that was its only unique capability is #373.

---

## Crate topology

The workspace splits cleanly into three tiers.

### Substrate (generated code links these; published, a stability surface)

`forgedb-types`, `forgedb-storage` (facade) + `forgedb-storage-native` + `forgedb-storage-web`,
`forgedb-wal`, `forgedb-changefeed`, `forgedb-auth`, `forgedb-query-params`,
`forgedb-compaction`, `forgedb-txn`, `forgedb-coordinator`. Cataloged in
[PUBLIC_CRATES.md](./PUBLIC_CRATES.md).

### Compiler internals (the CLI's implementation; published for install only, NOT a stable API)

`forgedb-parser`, `forgedb-codegen`, `forgedb-validation`, `forgedb-migrations`,
`forgedb-backup`, `forgedb-watcher`, `forgedb-lsp-server`. Published to crates.io only so
`cargo install forgedb` resolves; see [SEMVER.md §4](./SEMVER.md) and [`CLAUDE.md`](../CLAUDE.md)
(the authoritative workspace inventory). (`forgedb-lsp-server` joined this list in epic #173
— the `forgedb` crate optionally depends on it for the bundled `forgedb-lsp` binary.)

### Dependency direction

Compiler internals may depend on substrate (for codegen); **substrate never depends on compiler
internals, and generated code never depends on compiler internals.** This is what keeps the
generator identity honest: nothing at runtime reads a schema.

---

## Storage model

The engine is **append-only columnar** storage over positional files (not row-based). Writes are
always positional `pwrite`-style appends; *bulk reads* may map a bounded span of a column when the
span is large enough to be worth it, which is an optimization inside the read path and not a change
to the layout or the write path. Each model and each many-to-many junction is a directory:

```
<data-root>/
  <model>/
    manifest.json            # physical layout: columns, value sizes, kinds, row anchor,
                             # format_version, engine_version, compaction_epoch
    tombstones.bin           # 1 byte per row (liveness / delete marker)
    fixed/
      uuid_0.bin             # fixed-width columns (uuid=16B, u64/i64/f64=8B, bool=1B, …)
      u64_1.bin
    string_data_0.bin        # variable-length column payloads
    string_offsets_0.bin     # + offsets, one pair per variable column
```

Key properties that the rest of the system is built on:

- **Append-only.** A write appends; it never mutates committed bytes. Updates and deletes are
  *superseding-version appends* (a new row version; delete appends a tombstoned version).
  Latest-version-per-id resolution is generated per model.
- **Self-describing length.** Every column's committed byte length is a pure function of the
  row count + layout, so a reader derives the durable prefix from file lengths — no persisted
  checkpoint marker is load-bearing.
- **A foreign key is not its own type.** An FK column is physically identical to the column the
  *target model's identity field* occupies — same width, same accessor, same manifest entry. The
  generator resolves `*Target` / `?Target` to that key type once, at the boundary, so no layout
  rule and no relation capability is conditioned on the key being a uuid. A many-to-many junction
  is the same idea applied twice: one fixed column per endpoint, each that endpoint's own width.
- **A key is `Copy`, including a string one.** Every identity type materializes as a fixed-size,
  hashable, totally-ordered Rust value, because a key sits in the row index, in a junction
  `HashMap`, and in a fixed-width replication frame. That is why a `string(N)` identity is a
  `forgedb_types::InlineStr<N>` — a fixed-capacity `Copy` string — rather than the heap `String`
  the same declared type produces in an ordinary column (#252). One consequence is worth naming:
  the resolution above means every inline-string *layout* rule has to run on the **resolved** type,
  or an FK to a string-keyed model silently misses the packing path it physically needs. The wire
  form is unchanged — `InlineStr` serializes as a plain JSON string, by hand rather than by derive,
  since serde's array derive stops at 32 elements and a key may be wider.
- **Which field is the identity, and which types may be one, are each decided in exactly one
  place.** `Model::identity_field` (`crates/parser/src/ast.rs`) picks the field — a field named
  `id`, else the first `+` field, in that order — and `FieldType::is_identity_key` names the
  admitted set (`uuid`, the four integers, `timestamp`, `string(N)`), with a required FK admitted
  by resolution rather than by type. Both used to be open-coded: the picker in 31 places across 8
  files, and the key-type test twice (once as the many-to-many endpoint rule). Neither duplication
  was a style problem. A *single-pass* picker — `find(|f| f.name == "id" || f.auto_generate)` —
  keys `Event { seq: +u64, id: u32 }` on `seq` while every generated signature still says `id`,
  which compiles, runs, round-trips, and diffs clean in a snapshot; only reading a row back by the
  key the author meant can see it. And two independent key-type tests produce two diagnostics for
  one mistake, then drift. So the endpoint test now *delegates* to the identity test rather than
  restating it, and a grep-based guard (`tests/identity_predicate_test.rs`) fails the build if
  either predicate is open-coded again (#251).
- **Watermark snapshots.** A snapshot is just a row-count watermark (`forgedb_storage::Snapshot`);
  point-in-time reads resolve the newest version *within* the watermark. No `xmin`/`xmax`, no
  version chains.
- **Durability.** Generated writes journal an opaque row blob to a per-model WAL (`forgedb-wal`
  `Raw` op) + fsync *before* touching columns; recovery truncates a torn column tail and replays
  the WAL tail by absolute row index. A `DirLock` refuses a second writer.

- **Two orthogonal version counters, and they are not the same axis** (#254). A manifest carries
  both, and confusing them silently skips migrations:

  | Manifest field | Owned by | Counts | Migrated by |
  |---|---|---|---|
  | `schema_version` (on-disk key `format_version`) | the **app's** `migrations/` lineage | applied schema migrations | `forgedb migrate build` + `migrate run` |
  | `engine_version` | **ForgeDB's** release line | the engine's byte-format generation | `forgedb migrate engine` |

  A manifest with no `engine_version` baselines to generation 1, so the counter is additive rather
  than a second format break. Generation 2 is #254: timestamp columns hold **microseconds**, where
  generation 1 held seconds.

  **The engine hop is a generated bin, not a schema-blind column pass.** Only a *bare*
  `timestamp` field becomes `ColumnType::Timestamp`; every shape that merely *contains* one —
  `timestamp?`, `[timestamp; N]`, a struct field — is written as an opaque `FixedBytes` transmute
  of the Rust value, which `repr(Rust)` gives no decodable layout for. 81 of the 247 timestamp
  fields in the example corpus are nullable, so a schema-blind pass would leave a third of them in
  the old unit while the regenerated code read the new one. Which leaves are timestamps, and where
  they sit inside an `Option` / array / struct, is *schema* knowledge — so it belongs in generated
  code. `forgedb migrate engine` emits a crate embedding **two** generated modules of the same
  schema, differing only in the baked `EXPECTED_ENGINE_VERSION`; the reader half opens the stale
  dir legally, the writer half stamps the new generation, and the existing open-guard interlock
  does the enforcement for free.

The on-disk layout is part of the substrate ABI: a change a prior binary cannot read bumps the
owning crate's major and requires a migration path (see [SEMVER.md §2](./SEMVER.md) and the
version interlock in [MIGRATIONS.md](./MIGRATIONS.md)).

---

## Request path (generated server)

The generated `api.rs` builds its own axum router — there is no shipped generic HTTP server.

```
HTTP request
   │
   ▼
axum router (generated in api.rs)
   ├─ __ops_routes()   /health /ready /metrics /snapshot   (unauthenticated)
   └─ tenant guard ──► __data_routes()
        │                 REST CRUD + list (?filter/sort/paginate)
        │                 WS /subscribe /live-query /replicate
        ▼
   forgedb-auth (verify JWT + tenant cross-check, when configured)
        │
        ▼
   generated per-model handlers
        ├─ forgedb-query-params  (parse the query string → generic Filter/Sort/Pagination)
        ├─ generated closed-set matcher / comparator (all field-aware logic)
        └─ generated Database (read/write path over forgedb-storage + forgedb-wal)
```

Every field-aware step — filtering, sorting, the event matcher, index probes — is *generated
per model*. The substrate crates on this path (`auth`, `query-params`) interpret no schema.

### The list path is a scan *scope*, and never materializes a row

A list request does **not** decode every column of every row. Codegen emits a *narrow scan view*
per model — `<Model>ScanRef<'a>`, the identity field plus the filterable/sortable columns, with
`string` as `&'a str` — and each scan column is bulk-loaded once (one `gather_buffered` per
column, hoisted out of the row loop) rather than read per row. The identity is the one
string-typed field that does *not* borrow: the scope returns a vector of ids that outlives the
buffers, and a `Copy` key costs the scan nothing to hold by value.

The scan is a **scope**, not a producer:

```rust
pub fn __with_scan<R>(
    &self,
    sel: Option<Vec<usize>>,                       // index-pushdown rows, or every live row
    keep: impl Fn(&<Model>ScanRef<'_>) -> bool,    // runs during decode
    f:    impl FnOnce(&mut Vec<<Model>ScanRef<'_>>) -> R,
) -> R
```

The handler filters, sorts, counts and paginates *inside* `f`, and returns `(total, Vec<Id>)`.
Nothing borrowed crosses the boundary — the view's lifetime is higher-ranked, so `R` cannot name it.

**The page is serialized inside the scope too, and does not go back through `get`.** Returning ids
and re-reading them was still a full decode per returned row, so the default list arm is a second
scope that keeps the page borrowed as well:

```rust
pub fn __with_page<R>(
    &self,
    sel:    Option<Vec<usize>>,
    keep:   impl Fn(&<Model>ScanRef<'_>) -> bool,
    sort:   impl FnOnce(&mut Vec<<Model>ScanRef<'_>>),
    offset: usize,
    limit:  usize,
    f:      impl FnOnce(usize, &[<Model>PageRef<'_>]) -> R,   // (total, the page)
) -> R
```

`<Model>PageRef` is the *wide* borrowed view — every stored column, still pointing into the
buffers, with one-to-many relations left as unit placeholders exactly as they are on the record — so
the response serializes straight out of them. The `__with_scan` + `get(id)` shape above is retained
only where the page genuinely needs owned rows: `@projection` models and the live-query re-run.

**The "is there any filter at all?" question is answered once per request, not once per row.** The
generated matcher short-circuits only on an *empty* query map, and `?limit=50` — the default page
size a client is told to send — makes the map non-empty without naming a single filterable field.
So an unfiltered list request used to run one hash lookup per filterable field per scanned row, all
of them guaranteed to miss: 502 µs on a 10,000-row table, 59% of the request, scaling with the
*table* rather than the page. Codegen now emits `__<model>_is_unfiltered` from the **same** field
iteration that builds the per-field checks, and the handler evaluates it once before the scan:

```rust
let __keep_all: bool = __post_is_unfiltered(&params);
… __with_page(__sel, |r| __keep_all || __post_scan_matches(r, &params), …)
```

Deriving the predicate from that same iteration is what makes it impossible for the two to disagree
about which names are filterable — and it is why the predicate is *positive* ("does any key name a
filterable field of this model?") rather than a maintained list of reserved query keys. A model may
legally declare a field named `limit`; for that model `?limit=3` genuinely is a filter, and an
exclusion list would silently return unfiltered rows.

**And once that question has an answer, the unfiltered request stops scanning the table at all.**
`keep` and `sort` are opaque closures, so `__with_page` cannot tell that they are trivial: it has
to gather and decode every live row's scan columns before it knows which `limit` of them the page
wants. But the *handler* knows, one line earlier — with no filter and no sort, the page is
`__rows[offset .. offset + limit]` of the live row set and nothing else can change it. So it takes
a third scope that skips the scan entirely:

```rust
pub fn __with_fast_page<R>(
    &self,
    offset: usize,
    limit:  usize,
    f:      impl FnOnce(usize, &[<Model>PageRef<'_>]) -> R,   // (total, the page)
) -> R
```

No `sel`, no `keep`, no `sort` — a signature that *cannot* express a filtered request, which is
what makes the specialization safe to reason about. One `gather_buffered` per column bounded to the
page's rows, `total` from the live row count.

Two structural notes, both deliberate:

- The branch sits **above** the index-pushdown binding, not beside it. Pushdown resolves only
  fields the filter predicate admits, so a request naming none of them resolves no index — placing
  the branch first makes that unreachable rather than merely cheap.
- It is **below** the `?as_of=` arm. A snapshot read is a different row set, and the fast page
  reads the live one; hoisting the branch above the match would silently serve live data to a
  client that asked for a watermark, which is a correctness trap rather than a slow path.

The reason this was worth a third scope is that #226 had already removed almost everything else.
After it, an unfiltered `GET /model?limit=50` over 10,000 rows was **97–99% phase A** — the request
had become the scan, and the scan was gathering 10,000 rows to answer with 50. Measured in one
paired run: 86% of that request removed on a four-`string` model, 64% on a model whose scan view is
narrower, and 89% / 70% at `offset=10&limit=5`. The win scales with `1 − page/rows`, so at
`limit=1000` on a 1,000-row table — where the page *is* the table — it is zero by construction, and
measures as zero.

What it does **not** touch is the selection itself — collecting the live row indices, sorting them,
and dropping the dead ones is O(live rows) and still runs in full. That is now the floor of an
unfiltered request, and it is a large fraction of what remains (≈70% of the post-#281 request at
10,000 rows), dominated by the one sort that exists only because the id→row map is a `HashMap`.
Tracked as its own follow-on (#289) rather than folded in here.

That shape is what removes the copies rather than narrowing who pays them. `keep` running during
decode means a **rejected** row never allocates a string. Keeping the sort and the page inside the
scope means a **surviving** one does not either — and on an unfiltered `GET /model?limit=50` every
row is a survivor, which is exactly the case a filter-only optimization wins nothing on. The
strings a scan row used to allocate were read for three things (the sort comparator, `.len()`,
`.id`) and dropped; now the comparator reads the buffer's bytes in place.

The constraint this commits to: **only scalars leave a scan.** A future list feature that wants
more than ids out of one has to come inside the callback.

Three properties keep it safe and non-viral:

- The buffered columns live in a local holder inside the generated scan, so a borrowed view cannot
  escape it. `ScanRef` is internal — no wire derives, never reachable from REST/TS/OpenAPI, and
  only ever named behind a `&` in a closure argument. **No lifetime appears in any user-facing
  generated signature.**
- The scan filter is emitted from the *same* per-field checks the change-feed matcher uses, so
  there is one predicate source and two operand views — never a second parser.
- The index-pushdown arm (`__rows_by_<field>`, O(matches) via the secondary index) resolves
  candidate *rows* and feeds them to the same scope, so there is one scan body and one decode
  path. Pushing that arm through `gather_buffered` needed a matching substrate change: bounding a
  bulk read to the selection's row span is right for a dense scan and wrong for a handful of
  scattered candidates, so `VariableColumn::gather_buffered` gained a packed sparse path
  (`SPARSE_OFFSETS_SPAN_FACTOR`) below which offsets and bytes are read per row.

The same scope backs the live-query re-run, which re-evaluates the closed-set query on every
change to the model.

---

## Concurrency & realtime (layered)

Each capability is a strict superset built over the append-only/watermark core, with no on-disk
format break:

- **Snapshot reads** (watermark) → **single-writer + concurrent readers** (read-only column
  reader handles) → **transactions** (Tier 1, atomic commit/rollback) → **optimistic concurrent
  writers** (Tier 2, `forgedb-txn` commit sequencer) → **multi-process writers** (Tier 3,
  `forgedb-coordinator` holds the `DirLock` and serializes the commit turn).
- **Change feed** (in-process, field-blind broadcast) → **live queries** (stateful,
  removal-aware result sets) → **durable replication broker** (`forgedb-changefeed::durable`,
  resumable by global offset) → **browser read-replica** (the same generated `database.rs`
  compiled to wasm32 against `forgedb-storage-web`, catching up from `/replicate`).

The ceiling is one physical append point per column — *concurrent prepare, serialized commit*.
Multi-machine replication/consensus is a separate future product, not these tiers.

**Control plane vs data plane (multi-process writers).** Tier 3 splits cleanly: the
`forgedb-coordinator` process is a pure **control plane** — it holds the `DirLock`, serializes
the commit turn, and sequences the LSN, but it has **no `forgedb-storage` dependency** and never
decodes a row byte. The schema-aware column write stays in generated **data-plane** code, run
lock-free by each coordinated client under a granted turn (clients open with `_lock: None`,
mutually exclusive with a standalone self-locking writer). This is what keeps the identity honest
at Tier 3: the coordinated writer is still the *same generated code*, and the coordinator — like
every substrate crate — knows nothing about any schema. It is the symmetric inverse of the durable
replication broker (control over the write turn, vs. an ordered feed of committed changes).

**The two deadlines are coupled, and a failed request fails closed.** A coordinated client blocks
waiting for its turn while the coordinator blocks waiting for the pending turn to clear, so both
sides hold a deadline — and only one of them can see both. The **client declares its own I/O
deadline** on every `RequestTurn` (`client_deadline_ms`), and the **coordinator clamps its grant
wait** to `min(turn_timeout, declared − 500ms)`, so a `Busy` reply always reaches the client before
it stops reading. Without that coupling, raising `--turn-timeout` past the client's deadline made
the client give up first and left the connection **desynchronized**: the coordinator's eventual
`Grant` stayed on the socket, to be read as the answer to the client's *next* request — a turn it
did not hold. A client that declares nothing is a pre-coupling build and is assumed to hold the
legacy 35s, so old clients are fixed without being recompiled. This is an additive wire *field*
rather than a connect handshake because the protocol is internally-tagged JSON with no version
field: an unknown field is ignored in both directions, while an unknown *variant* breaks whichever
peer ships second.

Independently, any failed request **poisons** the client connection — it refuses further requests
until `reconnect()` replaces the stream — because a timeout leaves a reply in flight no matter why
it happened. Poisoning is deliberately not paired with automatic retry inside the substrate:
recovery policy lives in the generated commit loop, beside the `Busy` budget and retry limit
already there, and the generated code calls `reconnect()` in both coordinator error arms so the
failure stays loud for the current transaction and invisible to the next one.

### Integer auto-increment allocates per process, and is made conflict-*visible*

`+u32`/`+u64` fields allocate from an in-memory counter held per field, per process — there is
no shared allocator and no coordinator-side sequence. A counter is seeded at open to
`max(persisted floor, scanned max)`:

- **The scan is ungated by tombstones** and walks every *physical* row, including superseded
  versions. A deleted row still spent its number: rehydrating from live rows alone would hand a
  retired value to a different row, and that value is visible in the replication log, in backups,
  and in any URL that still holds it. (The secondary-index rebuild beside it *is* tombstone-gated
  — the max must not come from it.)
- **The persisted floor** is `Manifest.auto_sequences`, an opaque `field name -> highest value
  issued` map. It exists because compaction physically drops the rows the scan reads, so after a
  compaction the scan alone would regress. The floor only ever moves up; gaps are allowed.
  Generated `compact()` **writes the floor before** handing the live set to the byte GC, and
  refuses the compaction if that write fails — the reverse order leaves a crash window in which
  the rows and the floor are both gone.

Across processes the design does **not** prevent two coordinated writers deriving the same next
value; it relies on the collision being *detected*. Detection runs entirely off the opaque
write-set the coordinator equality-compares, via three key classes — `b"r"` for the model's
identity, `b"u"` for a `&unique` field, and `b"s"` for an integer auto that is neither (#260).
Any of them turns a duplicate into a `Nack`. `^` contributes nothing: an index makes a value fast
to *find* but claims nothing at commit time — which is now immaterial, since the sequence claim
covers the bare shape regardless of whether it is indexed.

A `Nack`ed sequence claim triggers a `__peer_refresh` before the retry. This is not an
optimization but a **termination** requirement: the retry re-runs the prepare closure, which
allocates the *next* value, so a writer N values behind a peer would need N attempts and exhaust
a bounded retry budget. It cannot rely on the ordinary peer-refresh gate, because a client's
view of the coordinator LSN advances only on its own `Ack` — a `Nack` never trips it. Nor can it
read the winning value out of the returned key: the coordinator hands back the key that
*collided*, which is the one **we** sent, carrying our own proposal. Re-reading the shared
columns is what actually re-derives the counter past every committed value.

`&unique` remains the stronger marking where uniqueness must hold against all history: its index
is durable, while the coordinator's conflict map is rebuilt empty on restart.

Identity-wise, `Manifest.auto_sequences` is **inert substrate**: the two `Manifest` backends store
and return the map and never parse a key or branch on a value. Which fields appear, what the
numbers mean, and every read and write of them belong to generated code — the rule is enforced by
a guard test, not only by a doc comment.

---

## Design decisions (and their trade-offs)

- **Compile-time generation over a runtime engine.** Type safety and monomorphized, per-schema
  code; the cost is that schema changes require regeneration + recompilation (handled by the
  migration workflow, [MIGRATIONS.md](./MIGRATIONS.md)).
- **Append-only + superseding versions over in-place mutation.** Keeps snapshots, backup, and
  the change feed simple and correct; the cost is that storage grows with dead versions until
  in-process compaction reclaims them (`forgedb-compaction`).
- **Watermark snapshots over MVCC version chains.** No per-row version metadata; the cost is
  that a compaction renumbers rows within an epoch, so pinned watermarks are epoch-scoped.
- **Storage facade over a target branch in codegen.** The generated `database.rs` compiles to
  both native and wasm32 with zero codegen branches; the facade absorbs the difference.
- **Per-process auto-increment counters made conflict-visible, over a shared allocator.** A
  coordinator-side sequence would put a schema-shaped concern inside schema-agnostic substrate;
  instead each process allocates locally and the *collision* is detected through the opaque
  write-set. The cost is one extra opaque key per insert per bare auto field, and a conflict map
  that grows with committed keys.
- **Substrate / compiler-internals split.** Generated code links only schema-agnostic crates;
  the compiler crates stay off the runtime path, which is what makes the generator identity
  verifiable.
- **ForgeDB owns the build, over scaffolding a crate the user compiles.** One `core` per app
  makes a second, differently-configured `database.rs` unrepresentable, and the driver can
  enforce a profile floor a manifest cannot. The costs are stated rather than discovered: the
  editable `src/main.rs` is **gone**, and in-tree placement (#338) does not bring it back — a
  Rust consumer may have the generated *database* emitted into their tree as a ForgeDB-owned
  cargo package (`[placement].rust_package`), but that package carries `core` alone: no
  `api.rs`, no `main.rs`, no `[[bin]]`. A user's existing scaffold is never deleted, and the
  mirror keeps its `#[path]` modules resolving. `panic` is irreducibly project-wide because
  cargo makes it so, and one shared `target/` serializes concurrent builds of sibling apps.

---

## References

- [`CLAUDE.md`](../CLAUDE.md) — authoritative workspace inventory + feature status
- [PUBLIC_CRATES.md](./PUBLIC_CRATES.md) — substrate crate catalog
- [SEMVER.md](./SEMVER.md) — stability policy
- [MIGRATIONS.md](./MIGRATIONS.md) — schema evolution + the version interlock
- [V1_ROADMAP.md](./V1_ROADMAP.md) — scope and honest current state
