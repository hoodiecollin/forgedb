# Proposal: Data-Transform Migrations (diff-driven, row-wise / computed evolution)

**Status:** DESIGN NOTE — `forgedb-product-manager` verdict **PASS-WITH-CONSTRAINTS** (re-gated
2026-07-14 after the diff-driven + LLM-residue reframe; **14 binding constraints** + **7 drift
vectors**, all folded in below — see "PM gate constraints"). This **supersedes** the note's original
manual-authoring design (a `todo!()` stub the developer filled by hand); that framing is overruled —
see "What changed in the reframe."
Fleshes out the **deferred** tier of [`schema-migrations.md`](./schema-migrations.md) (structural
migrations + version guard, PM-PASSed): its "Honest deferrals" explicitly punt *"multi-step /
expression data transforms (split a column, compute a new field from others) … not row-wise
computation."* This note is the design for that deferred rock — the part the manual **dump/reload**
path (`docs/MIGRATIONS.md`) does by hand today.
**Issue:** [#74](https://github.com/hoodiecollin/forgedb/issues/74) (`idea`, `tech-debt`)
**Date:** 2026-07-14

## Summary

The headline move, which keeps this squarely inside the generator identity: a data transform is a
**generated, typed `transform(old) -> new` function that is compiled and the compiler checks — not
an interpreted expression stored in a migration file and evaluated at runtime.**

What changed from the original note is *who authors the transform body*. Modeled on Drizzle's
`drizzle-kit`, `forgedb migrate create` now **diffs the two `.forge` schemas** (committed snapshot
vs current) and **auto-generates as much of the transform as it can prove**, prompting the developer
only where the diff is genuinely ambiguous, and — for the residual computation no diff can determine
— optionally drafting the code with an LLM the developer test-confirms. The developer no longer
hand-derives structs, wires serde, or types boilerplate transform bodies.

Crucially, **none of this moves the runtime identity line.** The running application is **unchanged**:
it still does exactly the opaque `format_version` compare + fail-fast from `schema-migrations.md`; it
never links a transform, never interprets a schema, never calls an LLM. What the reframe changes is
the **authoring UX of a dev-time, one-shot, frozen artifact** — from "the developer types it" to "the
diff-driver + an optional LLM draft it, the developer confirms it via tests, it freezes into
committed source." That is a change in a *compiler tool*, not in what ships.

**The identity crux (unchanged):** the transform's *code generation* is dev-time tooling (same class
as `forgedb generate`); the transform *binary* is a one-shot offline tool (same class as `backup`);
the *shipped app* gains nothing at runtime. There is **no runtime expression interpreter, no `.forge`
read at runtime, and no LLM on any runtime path** — the exact lines the "generic engine" invariant
forbids.

## What changed in the reframe

The original design generated `old.rs`/`new.rs` structs and a `transform.rs` whose body was a
`todo!()` stub the developer **filled entirely by hand**, and it *explicitly rejected all inference*
(old binding constraint #1 + old drift vector #3). The user overruled that: they do not want manual
authoring. The PM re-gate corrected the over-broad anti-inference stance with a sharp distinction:

- **Deterministic inference is codegen, and it is allowed.** A provable cast (`u32 → string`), an
  additive `None` fill, a confirmed rename — these are the *same class of work* as the already-shipped
  `SchemaDiffer::is_breaking()` and the #92 additive-backfill codegen: read two `.forge` files at dev
  time, emit tailored Rust. This is ForgeDB's core competency, not the forbidden inference.
- **Semantic row-mapping inference is never silently emitted.** Splitting `full_name` into
  `first`/`last`, computing `total = price * count` — the diff *cannot prove* these, so they are
  never auto-emitted as if deterministic. They are residue: developer-written, or LLM-drafted and
  **test-confirmed by the developer**.

So "never infer" becomes "**only emit deterministic or developer-confirmed transforms**." The
anti-inference intent survives exactly where it belongs (silent semantic guessing), and the
boilerplate the developer used to type is now generated.

## Where this sits in the migration story

`schema-migrations.md` establishes three change classes; this note owns exactly one:

| Change class | Owner | Mechanism |
|---|---|---|
| **Structural / metadata** (add nullable field, remove field, rename field/model) | `schema-migrations.md` Phase 2 | codegen + cheap manifest-driven column ops |
| **Constant-default rewrite** (type change *to a constant*, add NOT NULL *with a literal default*, add unique) | `schema-migrations.md` Phase 3 | reuse `crates/compaction` `stage_*_column_write`, atomic version bump |
| **Row-wise / computed transform** (value depends on the row's *other* fields, or a column split/merge, or a cross-model derivation) | **THIS NOTE** | diff-driven generated typed `transform(old) -> new`, compiled + run once offline |

The boundary is sharp and testable: **if the new value is a pure function of the old row's other
fields, it needs a transform (this note); if it is a constant or a structural move, `schema-migrations.md`
already handles it.** The version guard + snapshot machinery are shared verbatim.

## Why NOT an expression grammar in the migration file

The tempting alternative — store `@transform(total = price * count)` in the migration JSON and
evaluate it at `migrate up` — is rejected on **two** independent grounds:

1. **Identity.** An engine that reads an expression + a schema and evaluates it against arbitrary
   rows at runtime **is** the forbidden generic runtime interpreter (CLAUDE.md "Still rejected").
   It is the migration-shaped version of the dynamic query builder the guard exists to prevent.
2. **It's blocked anyway.** ForgeDB has **no expression grammar** — `@computed` is deferred for
   exactly this reason (the lexer only parses number / bare-ident / string directive args). Building
   an expression evaluator for migrations would front-load that entire deferred effort *and* land it
   on the wrong side of the identity line.

**Generated typed Rust sidesteps both.** The "expression" is ordinary Rust — whether the diff-driver
proves it, the developer types it, or an LLM drafts it — and `rustc` type-checks it
(`old.price * old.count`), compiled ahead of time. No grammar to build, no runtime interpreter to
ship. The transform's *power* is unbounded (it's Rust), its *safety* is the compiler + confirmed
tests, and its *identity* is clean (dev-time codegen + one-shot offline binary).

## The residue pipeline

`forgedb migrate create` resolves the schema diff through **three layers**, each of which materializes
**concrete, `rustc`-type-checkable Rust** into `transform.rs` — never a descriptor a `migrate up`
engine evaluates:

1. **Deterministic delta** — a change the differ can *prove* (a scalar cast, an additive `None` fill,
   a rename the differ resolves). Auto-generated transform Rust, **no prompt, no LLM**. Same class as
   the #92 additive-backfill codegen.
2. **Structural ambiguity** — the differ sees a change but cannot prove *which* structural move it is
   (rename-vs-drop+add, or which cast among several). An **interactive Drizzle-style CLI prompt**
   collapses the ambiguity into a single choice, which is then emitted as the same concrete
   deterministic Rust. Reads no schema at runtime; ships nothing; its whole product is a baked-in
   choice.
3. **Non-inferable residue** — a row computation the diff *cannot* determine (split/merge/compute
   from siblings). Emitted as a **per-field reviewable stub**. If — and only if — an LLM provider is
   configured in `forgedb.toml`, the CLI *offers* to draft that stub from a developer NL description,
   under the TDD gate below. **With no provider configured, the residue stays the reviewable stub and
   the whole feature is fully functional offline / air-gapped** — the stub is the correctness
   fallback, not a degraded mode.

The boundary between layer 1 and layer 3 is load-bearing and guard-tested (drift vector DV-C): layer
1 is **only** mappings the differ can prove; anything requiring semantic understanding is residue.

## The LLM-residue authoring layer

For layer-3 residue only, and only when a provider is configured, the CLI can draft the transform
body with an LLM. The design is a **dev-time codegen assistant**: the LLM occupies the exact slot a
human occupies today — it *drafts* the body of `transform.rs`; the artifact class (app-level Rust the
developer owns) is unchanged.

### The authoring loop (TDD-gated)

```
developer describes intent for a residue field in natural language
        │  ("split full_name on the last space into first/last")
        ▼
CLI derives candidate test cases (input row → expected output row)
        │
        ▼
DEVELOPER CONFIRMS / EDITS the test cases   ← the developer owns the spec
        │  (breaks the circularity of an LLM grading its own understanding)
        ▼
LLM drafts the transform body against old.rs/new.rs defs + the confirmed cases
        │
        ▼
cargo test the generated tests  ──failure──▶  feed compile/test error back to the LLM
        │  (bounded repair iterations)          └────────────── loop ──────────────┘
        ▼ green
present transform + tests as a diff → developer reviews → commit (both freeze)
```

The correctness floor is **not** "it compiles." It is "compiles **and** passes a
**developer-confirmed** executable spec." Every LLM-authored transform ships with committed tests; a
provider-authored transform with a stale/unconfirmed or failing test set is **not eligible to
commit**. Once committed, the transform is ordinary source — never regenerated on deploy, never
re-invoked at runtime.

### The LLM never sees data

The provider context is built **only** from: the two `.forge` schemas, the generated `old.rs`/`new.rs`
defs, the residue field(s), the developer's NL description, and the confirmed test cases. **Actual
database rows are never sent to a provider** — a privacy line, and an identity line (it keeps the LLM
a code author, never a data processor).

## Provider architecture

The provider layer is **CLI-internal compiler tooling** — the peer of `parser` / `codegen` /
`migrations` / `watcher`, *not* class-1 substrate and *not* class-2 transport. Both of those
categories are defined by their relationship to generated code / the running app; the provider layer
has neither relationship. Generated `database.rs` never links it, no app ever calls it, it is on no
runtime path. Per `docs/SEMVER.md` it lives in the "published only so `cargo install forgedb` can
build the CLI, explicitly NOT a stable public API" carve-out, exactly like `parser`/`codegen`.

> **Why a *general* provider abstraction is identity-clean.** Generality is forbidden only for
> *shipped runtime artifacts that reconstruct schema-specific logic*. A general dev-time code *author*
> is analogous to a general-purpose compiler — the parser is general over all schemas too. The
> tailored logic (the transform body) is **materialized and committed**, not resolved by calling a
> library at use-time; the author writes the code once and is then out of the picture. A generic
> transform *library the app calls* would be the violation; a generic *code-authoring tool that writes
> app code* is not.

One trait, **two transport mechanisms**:

| Mechanism | Providers |
|---|---|
| **Subprocess CLI** | `claude-code` CLI, Google **Antigravity `agy`** (the `agy` bin on PATH — replaced Gemini CLI), Codex CLI, ollama (local) |
| **HTTP API** | Anthropic API, Gemini / AI-Studio, OpenAI API |

Future gateways (**Vercel AI Gateway, OpenRouter, Fireworks**) are the *same* HTTP mechanism with a
different base URL + auth — config, not new code.

**Config** lives in `forgedb.toml` `[migrate.llm]` (`provider` / `model` / `api_key_env` /
`base_url`) — **never in `.forge`**. Secrets are **env-referenced** (`api_key_env` names an env var);
API keys are never stored in config or schema.

**Verified-vs-blind (honest labeling).** Providers actually exercised in-repo — `claude-code` CLI,
Anthropic API, Gemini — are marked **verified**. Providers architected to the trait but not runnable
on the development machine — **Codex CLI, OpenAI API, ollama local** — ship **explicitly labeled
blind** (unrun round-trip). No provider is claimed working without an executed round-trip.

## The transform workflow

```
edit .forge (a breaking, computed change)
        │
        ▼
forgedb migrate create           (diff snapshot vs current .forge)
        │  generates migrations/{id}_{desc}/  (a self-contained crate)
        │    ├─ Cargo.toml            (path/version-pinned substrate; NOT shipped with the app;
        │    │                          does NOT link the provider crate)
        │    ├─ src/old.rs            GENERATED from migrations/.schema-snapshot.forge
        │    │                          → Old<Model> structs + OldDatabase read-only opener
        │    ├─ src/new.rs            GENERATED from the current .forge
        │    │                          → New<Model> structs + NewDatabase writer (create_*)
        │    ├─ src/transform.rs      layered residue pipeline output:
        │    │                          • deterministic delta → concrete Rust (no prompt)
        │    │                          • structural ambiguity → interactive prompt → concrete Rust
        │    │                          • residue → reviewable stub, optionally LLM-drafted
        │    └─ tests/transform.rs    confirmed test cases for any LLM-authored residue
        ▼
(deterministic layers already written; residue either a reviewable stub the
 developer completes, or an LLM draft the developer test-confirmed — see above)
        ▼
forgedb migrate up
        │  0. verify the old dir's format_version == the snapshot's expected version
        │       (opaque integer compare, never a .forge re-read)
        │  1. snapshot the old dir (reuse #57 backup; #76 incremental + #77 PITR as the net)
        │  2. cargo build + run the transform crate against a fresh temp dir
        │       (NO provider in this path — migrate up is provider-free)
        │  3. the transform reads old columns (old.rs readers) → writes new columns (new.rs writers)
        │  4. new dir's manifests carry the new format_version (matched to regenerated code)
        │  5. atomic swap: rename temp → data dir (old dir retained as backup)
        ▼
forgedb generate && cargo build   (regenerate the APP's database.rs to the new schema)
        ▼
app opens the migrated dir; format_version matches EXPECTED_FORMAT_VERSION → runs
```

Key properties:

- **Two schema versions coexist only inside the transform crate**, namespaced `old::` / `new::` —
  both are generated code, never hand-derived. The old readers are the `reader()` handles (#56-B)
  restricted to reads; the new writers are the integrity-enforcing `create_<model>` wrappers (#91),
  so FK existence + validation apply to the migrated data for free.
- **The transform is `all()`-based (load-transform-write) in M1**, matching dump/reload semantics. A
  streaming/iterator form (for datasets that don't fit memory) is a deferred refinement, not a first
  cut.
- **Ids/FKs are preserved by the developer** (or the auto-generated deterministic mapping) exactly as
  in dump/reload — but the generated `New*` types make the mapping type-checked instead of
  stringly-typed serde.
- **Atomicity** reuses the `backup restore` temp-dir + rename discipline; a crash mid-transform
  leaves the original dir untouched (the app's version guard still refuses the un-migrated dir, so
  there is no silent half-state).
- **`migrate up` is provider-free.** The LLM ran once at `migrate create`; `up` compiles and runs
  already-committed Rust and has no provider in its dependency closure (the sharpest identity line).

## Identity verdict & the drift vectors

Mapping to the guard (`CLAUDE.md` → "What ForgeDB is"), inheriting `schema-migrations.md`'s red
lines:

| Piece | Class | Rationale |
|---|---|---|
| Diff-driving + auto-generating `old.rs`/`new.rs`/deterministic `transform.rs` | **Dev-time codegen** | same class as `forgedb generate` + `SchemaDiffer` — reads `.forge` at *dev* time, emits tailored Rust |
| Interactive disambiguation prompt | **CLI UX over the differ** | resolves an ambiguity into deterministic Rust; reads no schema at runtime, ships nothing |
| The LLM-residue authoring layer | **Dev-time codegen assistant** | drafts app-level Rust at `migrate create`; materialized/tested/committed/frozen; no LLM at runtime |
| The provider trait | **CLI-internal compiler tooling** | peer of `parser`/`codegen`; not substrate, not transport; not linkable by generated code |
| The developer-confirmed transform body + tests | **App-level Rust** | compiled, type-checked, empirically test-gated; the field-aware intent only the app knows |
| The compiled transform binary | **One-shot offline tool** | same class as `backup`/`compact` — reads/writes opaque column bytes, run by an operator, not the app |
| Old readers / new writers | **Generated** (reused #56-B/#91 surfaces) | tailored per-schema, no runtime schema read |
| The running app | **Unchanged** | still only an opaque `format_version` compare + refuse (`schema-migrations.md` red line #4) |

The entire rejection surface is **drift**, concentrated in one place: any path that lets the LLM or
an interpreted plan reach `migrate up` or app runtime.

- **DV-A — LLM invoked at `migrate up` / app runtime (the fatal one).** If a provider can be called
  outside `migrate create` — to "repair" a transform on the fly, or regenerate on deploy — the freeze
  is fiction and you've shipped a runtime code generator that reads schemas. *Designed out by
  constraint 8; guard-enforced by asserting the `migrate up` path and the transform crate have no
  provider in their dependency closure.*
- **DV-B — the residue pipeline emits an interpreted "migration plan" instead of Rust.** Having tiers
  1/2 produce a data-structure descriptor a generic `migrate up` executor walks *is* the forbidden
  schema-interpreting runtime in migration clothing. *Designed out by constraint 1; guard-enforced by
  asserting every tier's output is type-checkable Rust and no interpreted-expression descriptor is
  persisted.*
- **DV-C — deterministic-inference scope creep.** Tier-1 "deterministic" quietly grows to cover
  semantic mappings (guessing `full_name`→`first`/`last`) and emits them silently as if provable —
  the original drift-vector-3 hazard under a new name. *Designed out by a sharp documented boundary:
  tier-1 = only mappings the differ can **prove**; split/merge/compute must land in residue (tier-3),
  never auto-emitted. Guard-tested.*
- **DV-D — the provider crate drifts toward substrate/public API.** Someone pins it as a stable
  published dependency, or generated code starts linking it. *Designed out by constraint 12
  (CLI-internal compiler-tooling class, semver carve-out) + constraint 3 (transform crate must not
  link it).*
- **DV-E — an LLM-authored transform committed without a confirmed passing spec.** The repair loop
  "gives up" and commits red/untested code, or the LLM authors the tests it grades itself against.
  *Designed out by constraint 9: the developer confirms the spec (not the LLM), and commit is gated
  on green confirmed tests.*
- **DV-F — actual rows sent to the provider.** For "better context," someone feeds sample rows to the
  LLM — crossing the privacy line and nudging the LLM from code-author toward data-interpreter.
  *Designed out by constraint 10; enforceable by asserting the provider context builder is
  constructed only from schema text + defs + developer-supplied cases, never from a `Database` read.*
- **DV-G — the zero-LLM path bit-rots.** The offline stub path silently degrades until "works
  air-gapped" is false. *Designed out by constraint 11; guard-tested by exercising the full
  `migrate create → up` cycle with no provider configured in CI.*

## PM gate constraints (binding — re-gated 2026-07-14)

The identity re-gate passed the reframe **PASS-WITH-CONSTRAINTS** and named **14 binding
constraints**. Constraints 1–7 govern the diff-driver + offline transform; 8–14 govern the LLM
layer. All are folded into the workflow / drift vectors above; restated here as the checklist a first
milestone must satisfy.

1. **Layered residue pipeline; every layer emits concrete committed Rust, never a runtime-interpreted
   directive.** (a) deterministic delta → concrete Rust, no prompt; (b) structural ambiguity →
   interactive prompt resolving to the same concrete Rust; (c) residue → reviewable per-field stub,
   optionally LLM-drafted. **With no provider configured, residue stays the reviewable stub and the
   offline path is fully functional.** Guard test: (a) and (b) emit type-checkable Rust (no
   interpreted-expression descriptor is ever persisted), and with no provider the residue body is a
   plain reviewable stub, not a guess. *(Supersedes the original `todo!()`-only constraint, which the
   diff-driven reframe overrules; anti-inference intent preserved as: deterministic inference is
   codegen and allowed, semantic row-mapping is never silently emitted.)*
2. **Migration-scoped snapshot→codegen bridge, off the runtime path.** The `old.rs` generator reads
   `.schema-snapshot.forge` at dev time only (name it a peer of `generate`, e.g.
   `transform_scaffold_of(&Schema)`, never `SchemaReflection`); its output is never reachable from
   generated `database.rs`.
3. **The generated transform crate links only published class-1 substrate + the two generated
   modules — and NEVER the LLM/provider crate or any schema-reading helper.** Its `Cargo.toml` pins
   `forgedb-storage`/`-backup`/`-types` from crates.io; it must not link `forgedb-migrations` or the
   provider crate.
4. **Version bump is atomic with the swap; the new dir carries the new `format_version` before it
   goes live.** A crash between "new dir written" and "version stamped" leaves a dir the regenerated
   app *refuses*. Guard-tested.
5. **`migrate up` verifies the old dir's `format_version` == the snapshot's expected version before
   running** — an opaque integer compare, never a `.forge` re-read. If a prior half-migration or
   compaction bumped the old dir's epoch, refuse rather than mis-decode under an assumed layout.
6. **Atomicity + rollback mandatory.** Snapshot before (reuse #57; now also #76 incremental + #77
   PITR as the recovery net), temp-dir + rename swap, old dir retained. A failed transform (including
   a failed LLM-authored one that somehow reaches `up`) leaves a refused dir, never a half-migration.
7. **Authoring surface stays "schema + CLI + config."** The transform crate and all LLM /
   disambiguation config live in the CLI + `forgedb.toml`, never in `.forge`.
8. **LLM invocation is dev-time-only, at `migrate create`, and materializes a plain committed file —
   then freezes.** `migrate up`, the app runtime, and every published artifact are provider-free.
   Once written, the transform is ordinary committed source, never regenerated on deploy and never
   re-invoked at runtime. **Guard test: the `migrate up` code path and the generated transform crate
   have no provider dependency in their closure** (the sharpest identity line in the proposal).
9. **TDD mandatory; the developer owns the spec.** Every LLM-authored transform ships with an
   executable spec (input row → expected output row) that **the developer confirms** before the LLM
   writes code. The repair loop keys on **test failure and compile failure**, bounded iterations;
   green confirmed tests + transform commit together. No provider-authored transform may be committed
   without a passing confirmed test set.
10. **The LLM sees schema + intent + confirmed cases, never actual rows.** Context = the two `.forge`
    schemas, the generated `old.rs`/`new.rs` defs, the residue field(s), the developer's NL
    description, and the confirmed test cases. Actual database rows are never sent to a provider.
11. **Strictly additive over the offline stub; zero-LLM stays fully functional.** No provider
    configured → residue is the reviewable per-field stub and the whole feature works offline /
    air-gapped. The LLM path may never become the only path to a working migration. CI exercises the
    full `create → up` cycle with no provider.
12. **Provider config lives in `forgedb.toml` `[migrate.llm]`, never in `.forge`, and secrets are
    env-referenced.** The provider abstraction is CLI-internal compiler tooling (peer of
    `parser`/`codegen`), not substrate and not transport — it must not be published as a stable public
    API and must not be linkable by generated code.
13. **Verified-vs-blind providers are labeled honestly.** Providers actually exercised in-repo
    (claude-code CLI / Anthropic API / Gemini) are marked verified; trait-only-untested providers
    (Codex CLI / OpenAI API / ollama) ship explicitly labeled blind. No provider is claimed working
    without an executed round-trip.
14. **Non-determinism is contained by freeze + confirmed tests.** The only defense against LLM
    non-determinism is (8) freeze + (9) developer-confirmed tests. A provider-authored transform with
    a stale or unconfirmed test set is not eligible to commit; re-running `migrate create` on the same
    diff may produce different code — which is why the **committed artifact, not the generator, is
    authoritative.**

## Staged milestones

- **M1 — diff-driven ergonomic dump/reload (1:1 model transform).** Diff snapshot vs current, and
  for a schema where every model maps to itself with per-field computation (the `qty: u32 → string`
  class), auto-generate `old.rs`/`new.rs` + the **deterministic layer** of `transform.rs`, leaving
  residue as reviewable stubs. `migrate up` builds + runs it over a temp dir, atomic swap, version
  bump. **Highest value, lowest risk** — proves the whole seam with zero LLM dependency.
- **M2 — interactive disambiguation.** The Drizzle-style prompt for rename-vs-drop+add and ambiguous
  casts; resolves each into the deterministic layer. Extends M1's differ; no new runtime surface.
- **M3 — column split / merge / compute-from-siblings (residue stubs).** The residue body reads
  several old fields and writes several new ones. No new mechanism — the generated `Old*`/`New*`
  structs already expose all fields; M3 is the reviewable-stub UX + examples + test coverage.
- **M4 — LLM-residue authoring layer.** The provider trait + the two transport mechanisms, the
  TDD-gated authoring loop (developer-confirmed spec → draft → compile/test repair → review →
  freeze), config in `forgedb.toml [migrate.llm]`. First verified provider(s): claude-code CLI +
  Anthropic API; blind-labeled: Codex CLI / OpenAI API / ollama. **Strictly additive over M3's stub.**
- **M5 — cross-model transforms.** The transform reads model A to populate model B (denormalize,
  extract a table). `OldDatabase`/`NewDatabase` already bundle all models, so this is mechanism-complete
  after M1; M5 hardens FK-integrity ordering (create parents before children).
- **M6 — streaming transform** (datasets exceeding memory): an iterator form of the generated readers
  so the transform processes row-batches instead of `all()`. Deferred refinement.

## Honest deferrals

- **Online transform (apply while serving)** — deferred; the offline transform assumes an exclusive
  writer (the #56-B single-writer discipline / a #75 Tier-1 transaction). Cross-reference #75.
- **Blind-labeled providers** — Codex CLI, OpenAI API, and ollama local are architected to the
  provider trait but not runnable on the development machine; they ship **unverified** until an
  executed round-trip is possible (constraint 13).
- **Persisting resolved prompt/disambiguation answers** — not done in M1–M4. The committed transform
  `.rs` (+ its confirmed tests) *is* the durable artifact; re-running `migrate create` on the same
  diff may re-prompt / re-draft, which is why the committed artifact is authoritative (constraint 14).
- **Reusable transform rules** — explicitly **not** captured. A captured rule gets misapplied to rows
  it shouldn't; each LLM-authored transform is a one-shot for *this* migration, then frozen.
- **Lossy down-transforms** — a `down` transform is symmetric (generate the reverse scaffold) but may
  be genuinely lossy (a narrowing) — documented-lossy or refused, never silently reconstructed
  (inherits `schema-migrations.md`).
- **Very large datasets** — M1–M5 are load-transform-write (`all()`); streaming is M6.
- **Multi-tenant rolling transform** — composes with `schema-migrations.md` Phase 4 (per-`<root>/<tenant>`
  sweep); each tenant dir is transformed independently.

## Cross-issue dependencies

- **Builds on `schema-migrations.md`** — shares the committed schema snapshot
  (`migrations/.schema-snapshot.forge`), the version guard + `EXPECTED_FORMAT_VERSION`, and the
  breaking-vs-additive gate. This note is its computed-rewrite tier.
- **Reuses #56-B reader handles** (old readers) + **#91 integrity wrappers** (new writers) — the
  migrated data gets validation + FK checks for free.
- **Reuses #57 backup** for the pre-transform snapshot + the atomic temp-dir/rename swap; **#76
  incremental + #77 PITR** are the recovery net under the migrate path.
- **Interacts with #75** — an *online* transform needs a transaction (Tier-1) or the single-writer
  lock; the offline transform needs neither.
- **Coordinates `compaction_epoch`/`format_version`** with `schema-migrations.md` and the
  incremental-backup chain (#76) so a rewrite and a backup chain agree on epoch semantics.

## Load-bearing references

- `docs/proposals/schema-migrations.md` — the structural + version-guard foundation this note
  extends; its red lines are inherited.
- `docs/MIGRATIONS.md:61-103` — the manual dump/reload recipe this note diff-drives + type-checks.
- `crates/codegen/src/rust.rs` — the emission target: generate `Old*`/`New*` structs + readers
  (reuse the `reader()` #56-B token stream) + writers (reuse `create_<model>` #91); the recovery /
  backfill path (~2430–2604) that handles the *constant*-default case this note does NOT duplicate.
- `crates/migrations/{types,diff}.rs` — `SchemaChange` / `SchemaDiffer` already classify which
  changes are computed-rewrite class (type change, etc.), the trigger + the diff source for the
  residue pipeline. The provider layer belongs here (or a sibling CLI-internal `migrate-llm` crate).
- `src/commands/migrate.rs` — `create` (the diff-driven residue pipeline), the `.schema-snapshot.forge`
  persistence (`old.rs` source), `to_simple_schema`.
- `crates/backup` — the snapshot + atomic temp-dir/rename primitives the swap reuses.
- `CLAUDE.md` → "What ForgeDB is" — the invariant this note is gated against, and the source of the
  substrate / transport / compiler-tooling taxonomy the provider layer is classified under.
