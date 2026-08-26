# Schema Migrations

ForgeDB is a **code generator**, so schema evolution follows the generate-then-compile
model: you edit `schema.forge`, regenerate, and recompile your app. What happens to the
*data on disk* depends on the change:

- **Provable** changes need no input from you: a new model, a new nullable field, a drop, a
  rename you confirm, a value-preserving widening, or an add whose field carries a `@default`.
- **Everything else** (a type re-encode that is not a widening, a nullable→NOT-NULL narrowing, a
  required field with no default, a removed enum variant) needs a decision ForgeDB cannot make —
  and it **asks you at `migrate create` time**, when you have the change in your head, recording
  your answer as data in the migration record.

Either way the data-at-rest rewrite is done by a per-version **offline transformer bin** ForgeDB
generates, driven end-to-end by `forgedb migrate`.

**Writing Rust is the advanced escape hatch, not the default path.** When a transform genuinely
needs code, you write it in the language your project already generates for — TypeScript or
Python — against the generated types, and ForgeDB runs it on the interpreter you already have.

The invariant (the generator-identity red line): the schema is never a runtime input to a
generic engine. The transformer is **generated code** — one straight-line typed replay per
origin→destination version range — not a runtime interpreter reading your schema. See
[V1_ROADMAP.md](./V1_ROADMAP.md).

---

## The version interlock

Every generated app bakes in an `EXPECTED_SCHEMA_VERSION`, derived from your migration
lineage. On open it reads one opaque integer (on-disk key `format_version`) from each data dir
manifest and **refuses a mismatch** rather than mis-decoding stale bytes. This is fail-fast by
design: a regenerated app will not silently read a not-yet-migrated dir, and the transformer will
not apply the wrong range to a dir. The version is the only cross-schema handshake — it never
reads column names or types to "self-heal".

A fresh database baselines at `format_version = 1`; each recorded migration bumps it by one.

**There are two counters, and they are orthogonal.** The one above is *the app's* schema-migration
serial. Beside it sits `engine_version`, *ForgeDB's* own byte-format generation — see
[the engine-format hop](#the-other-axis--forgedb-changed-not-your-schema) below. Everything in the
next two sections is about the app's serial.

---

## Additive changes — nothing to answer

An additive change is one existing rows can satisfy without a value being invented for them:
**a new model**, or **a new nullable field** (`field: T?`, read as `None` by existing rows).
ForgeDB proves the value, so it asks you nothing.

```bash
# 1. Edit schema.forge — add the new nullable field AT THE END of the model.
# 2. Record the change (baselines the lineage on first run):
forgedb migrate create "add note field" --schema schema.forge
# 3. Regenerate and rebuild your app:
forgedb generate all --schema schema.forge
forgedb build --schema schema.forge
# 4. Point the regenerated app at a dir the transformer produced (see below).
```

On reopen, generated recovery **anchors on the tombstone row count** (the authoritative
committed count) and **backfills any column shorter than the anchor** — the new field — with
its `@default` when the schema declares a resolvable one, and its type zero otherwise. Existing
rows are never touched.

> **The reopen backfill is not a substitute for the transformer, and never was.** Every recorded
> migration bumps the schema serial, `generate` bakes the new value into the app, and the
> generated open guard **panics** on a mismatch. So the moment `migrate create` records a hop —
> which it does for *any* non-empty diff, additive included — the "just restart the app" path is
> closed. The backfill is what keeps a column well-formed *within* a version; the offline
> transformer is what carries a data dir *across* one. Earlier revisions of this page said
> otherwise.

**Constraints:** append new fields at the **end** of the model (columns are position-addressed);
let the old binary checkpoint its WAL before migrating.

---

## Data-rewriting changes — the transformer bin

`forgedb migrate create` **records** every detected change as a versioned hop and classifies it:

- **`Auto`** — ForgeDB can prove the new-row body, so nothing is asked. Drop a field or model,
  rename, add a `&unique`, `T` → `T?`, a value-preserving widening (`u32`→`u64`, `i32`→`i64`,
  `u32`→`i64`, `string(N)`→`string(M)` for `M > N`, `timestamp(s)`→`timestamp(us)`), or an add
  whose field carries a resolvable `@default`.
- **`Authored`** — ForgeDB cannot derive the value (a type re-encode that is not a widening, a
  nullable→NOT-NULL fill, a required add with no default, a removed or renamed enum variant).
  **It asks you, at `create` time**, and records your answer as data in the migration record.

### What it asks, and what it records

```
Post.slug — ForgeDB cannot derive a value for the rows that already exist.
What should existing rows get?
  1) a constant value
  2) copy another field           title, summary
  3) leave it — I'll write the transform in TypeScript   (advanced)
```

Option 2 is offered only when the model actually has a field of the same type; an option that
cannot work is an invitation to an answer the build would then refuse.

Your answer is recorded **as data** in `migrations/<id>_*.json`, beside the change it answers, and
the record's checksum covers it. `migrate build` *lowers* it into the emitted transformer — the
answer is a compile-time input to code generation, never something the transformer matches on at
run time.

**In a session with no terminal — a CI run, a pipe, or `--no-auto` — the FIRST change needing an
answer is a hard error naming it, and nothing is written.** `--no-auto` suppresses the *prompt*,
not detection: a migration whose every change is provable succeeds identically with and without
it.

**A rename is proposed, never assumed.** One field dropped and one added of the same type is
usually a rename, and the two readings produce opposite data — a rename carries every stored
value across, a drop+add empties the column. So ForgeDB asks. With nobody to ask it records the
drop+add, which is what the schema literally says.

**`migrate build` refuses a hop whose answer is missing**, before it generates a line and before
cargo is invoked. The check is against the record and the recorded scaffold hash — never a grep
for `TODO`, which you can delete without answering anything.

Changes to an `enum`'s variants or an inline `struct`'s layout are diffed too, and most of them
are breaking — see [below](#enum-and-struct-definitions-are-part-of-the-diff).

### Lifecycle

```bash
# 1. Edit schema.forge, then record + classify the change. It ASKS about
#    anything it cannot prove:
forgedb migrate create "qty to string" --schema schema.forge
#    → records migrations/<id>_*.json (from_version -> to_version, + your answers)
#    → snapshots migrations/schemas/v<n>.forge
#    → if you chose "I'll write the transform", scaffolds
#      migrations/<id>/transform.{ts,py,rs} plus ForgeDB's own host.* and v<n>.*

# 2. If you chose the transform option, write it. `transform(model, row)` receives
#    each row AFTER the automatic (rename/drop/additive) ops and returns it
#    reshaped for the next version. It is typed against the generated v<n> module.

# 3. Regenerate your app (its EXPECTED_SCHEMA_VERSION advances to the new version):
forgedb generate all --schema schema.forge

# 4. Build the transformer for the range, then run it with the app STOPPED:
forgedb migrate build --from 1 --to 2 --schema schema.forge
forgedb migrate run   --from 1 --to 2 --schema schema.forge \
  --src ./data --dest ./data-migrated

# 5. Point the regenerated app at ./data-migrated.
```

### Enum and struct definitions are part of the diff

An `enum` variant is stored as a **1-byte discriminant keyed by declaration order**, and an
inline `struct` is `#[repr(C)]` with every field's offset a function of the whole declaration.
Neither carries a name on disk. So an edit to a *definition* moves what the already-written
bytes MEAN, while changing no byte and no field's declared type — and the reference that names
it (`status: Status`) does not move either.

`migrate create` compares the definitions themselves, and reports the change against
every field that stores one:

```
  • Enum 'Status' behind 'Post.status': REORDER Draft, Published — every stored
    discriminant re-maps (⚠️  BREAKING)
```

| edit | at rest | recorded as |
|---|---|---|
| **enum**: append a variant at the end | benign — every existing byte still decodes to its own variant | not breaking, `Auto` |
| **enum**: insert a variant in the middle | every byte at or past it decodes as its predecessor | breaking, `Auto` |
| **enum**: reorder two variants | each swapped byte decodes as the other variant; nothing is ever out of range, so nothing fails | breaking, `Auto` |
| **enum**: remove a variant | most rows re-map; a row holding the retired last variant is out of range | breaking, **`Authored`** |
| **enum**: rename a variant in place | the byte still decodes to the same slot, but the *name* is the wire form | breaking, **`Authored`** |
| **struct**: reorder fields | each field reads its neighbour's bytes | breaking, `Auto` |
| **struct**: retype / add / remove a field | the bytes are reinterpreted, or the row width changes and every row re-frames | breaking, **`Authored`** |

**A struct has no benign edit except a rename.** The enum's one safe case — appending — has no
struct analogue, because a struct's every offset depends on the whole declaration.

The `Auto` rows are automatic for a reason worth knowing: an enum crosses the transformer's
JSON transport as its variant **name** and a struct as its field **names**, so the identity hop
body re-encodes them to the new discriminant / the new offsets with nothing for you to write.
The `Authored` rows are the ones where a name has gone away, so there is nothing for the old
value to decode into and you must say what it becomes.

An enum or struct that **no stored field reaches** produces no change. Nothing of it is on
disk, so there is nothing to migrate.

Append at the end when you can. It is the only enum edit that leaves stored rows alone — and
even then the hop is recorded, so the schema version moves and an older binary meeting the new
byte is told to migrate instead of panicking on a discriminant it does not know.

`migrate build` emits the transformer crate for the version range and compiles it; `migrate run`
executes it over the data dir. It writes a **fresh destination** and leaves the source untouched,
so the original *is* your rollback.

**Every `migrate` subcommand takes `--schema`, and it is required.** Nothing is resolved from the
current directory: the schema names the app, the app decides which project owns it, and the
project decides which build cache the transformer is compiled in. There is no fallback to a
`migrations/transform` beside you — a fallback would emit a cargo package under whatever workspace
your shell happens to be standing in, which is the defect (#328) that this ownership change exists
to remove.

`--from`/`--to` are required on **both** commands, and `run` needs them for the same reason
`build` does: one app can have several built transformers, one per range, and the range is how
`run` names the one to execute.

### How the transformer works

For a `--from B --to G` range, ForgeDB emits a self-contained crate — one typed module per version
(`vN.rs`, each carrying its own version open-guard), any frozen authored bodies embedded verbatim,
and a `main.rs` that is a **fixed straight-line chain** of named `transform_vN_to_vM` hop functions
— no runtime step interpreter. Each hop reads every row through the `vN` typed structs, applies the
baked structural ops then the authored transform, and writes through `vM`'s `insert` (which
preserves record ids, so foreign keys stay valid). Multi-hop ranges replay through temp dirs and
publish with a single atomic rename.

**The crate is a member of your project's build cache**, at
`~/.forgedb/projects/<id>/apps/<hash>/transform-<from>-<to>/`, sharing one `Cargo.lock` and one
`target/` with every other app in the project. It is **range-stamped**, so building a second range
does not overwrite the first, and `migrate build` prints the path it wrote plus the binary it
produced. What stays in your tree is the *lineage* — `migrations/<id>_*.json`,
`migrations/schemas/v<n>.forge` and any authored `migrations/<id>/transform.rs` — because that is
the part you author and commit.

The crate depends only on your app's substrate (storage/types/etc.) — never on
`forgedb-parser` or `forgedb-migrations`, and it never parses a `.forge` at runtime.

---

## Many tenants

Under multi-tenancy each tenant is an independent data dir under one root, and each one is
migrated the same way any single dir is. Build the transformer once, then run it per tenant:

```bash
forgedb migrate build --from 1 --to 2 --schema schema.forge

for t in ./tenants/*/; do
  forgedb migrate run --from 1 --to 2 --schema schema.forge \
    --src "$t" --dest "${t%/}-migrated-v2" || echo "FAILED: $t"
done
```

Each run is independent: a tenant at an unexpected version is refused by the transformer's own
open-guard with its source unchanged, so a failure stops that tenant and no other.

**There is no built-in sweep command.** `forgedb migrate up --tenant-root` used to do this in one
invocation and was removed along with the rest of `migrate up`; restoring the sweep — with version
auto-detection and per-tenant failure accounting — is tracked as **#373**.

---

## Alternative: manual dump → reload

For a one-off change where you would rather not build a transformer, you can still dump with
the old binary and reload into a fresh dir through `Database::create_<model>` (ids preserved,
full integrity enforced), transforming each row in app code. This is the same typed replay the
generated transformer automates; prefer the generated transformer for anything you will run more
than once or across many tenants.

---

## The other axis — ForgeDB changed, not your schema

Everything above replays *your* `migrations/` lineage. A second, orthogonal counter tracks
**ForgeDB's own on-disk byte-format generation** (`engine_version`), and it is owned by the
released version line rather than by your app.

The distinction is the whole point, because the two failures have different remedies:

| mismatch | what happened | remedy |
|---|---|---|
| `format_version` (schema serial) | *your schema* changed since the dir was written | `forgedb migrate build` + `migrate run` — replay your lineage |
| `engine_version` | *ForgeDB* changed its byte format; your schema is fine | `forgedb migrate engine` |

Conflating them would send you to regenerate a schema that is already correct. An engine bump
changes no `.forge`, so it produces no lineage hop at all — the lineage transformer would run
nothing.

```bash
# with the app STOPPED
forgedb migrate engine --src ./data --dest ./data-gen2 --schema schema.forge
```

- `--dest` **must not already exist** (or must be empty) — it is materialized, not written into.
- `--src` is **left untouched**; it is your rollback. Nothing migrates in place.
- The hop crate is generated into your project's build cache as `engine-<from>-<to>/` and compiled
  there as part of the command — the same place, and the same shared `target/`, as the lineage
  transformer. Generated, not schema-blind, on purpose: a nullable, arrayed, or struct-nested
  timestamp is an opaque fixed-byte blob no schema-agnostic column pass can find.
- Already at the current generation → no-op. Stamped *newer* than your CLI → refused, telling you
  to upgrade the CLI rather than migrate backwards.

Generations are assigned in merge order, not at design time, so two format changes in one cycle
cannot both claim the same number:

| gen | change |
|---|---|
| 1 | baseline — every dir written before the counter existed |
| 2 | timestamp values are microseconds, not seconds (#254, v0.4.0) |

The guard compares one integer and has no opinion about your schema, so **every** dir at an older
generation is refused — including one whose models contain no `timestamp` at all. Per-release
upgrade steps live in [UPGRADING.md](UPGRADING.md).

---

## Command reference

| Command | Purpose |
|---|---|
| `forgedb migrate create <desc> --schema <file>` | Diff against the snapshot; record + classify the change; ask about anything it cannot prove. |
| `forgedb migrate create <desc> --no-auto --schema <file>` | The same, but never ask: the first unprovable change is a hard error naming it. |
| `forgedb migrate status --schema <file>` | Show applied / pending migrations. |
| `forgedb migrate build --from F --to T --schema <file>` | Generate + compile the transformer for one range. |
| `forgedb migrate run --from F --to T --src <data> --dest <migrated> --schema <file>` | Run the transformer built for that range. |
| `forgedb migrate engine --src <data> --dest <new-dir> --schema <file>` | Carry a dir across a ForgeDB **engine** byte-format generation (orthogonal to the schema serial). |

`--schema` is required on every row above. Three flags that used to appear here are **gone, and
error rather than no-op**: `migrate build -o/--output` and `migrate run --bin-dir` both named a
directory for a crate ForgeDB now places itself, and `migrate create --auto` named a mode that is
now simply what the command does. `forgedb migrate up` and `forgedb generate transform` are gone
with them — the first's one-command wrapper is the two `migrate build`/`migrate run` rows and its
per-tenant sweep is **#373**; the second's job is `migrate build`.

There is deliberately **no way to create a migration ForgeDB did not detect.** The old
`--auto`-less branch wrote an empty record with `changes: []` for you to hand-edit; a record's
`changes` array is derived from a schema diff, so it cannot disagree with
`migrations/schemas/vN.forge`.

---

## Writing the transform in your own language

Rust authoring is the **advanced** escape hatch, not the default path. When you choose "I'll
write the transform", the language is **derived from `[generate].targets`** — a project that
generates a TypeScript SDK writes its transforms in TypeScript — and ForgeDB scaffolds:

```
migrations/<id>/
  transform.ts     ← YOURS. ForgeDB never rewrites it once it exists.
  host.ts          ← ForgeDB's. The stdin/stdout loop. Rewritten every build.
  v1.ts  v2.ts     ← ForgeDB's, from migrations/schemas/v{1,2}.forge. Rewritten every build.
```

```ts
import { runTransform, type Row } from "./host";
import type * as From from "./v1";
import type * as To from "./v2";

export function transform(model: string, row: Row): Row {
  switch (model) {
    case "Post": {
      const from = row as unknown as From.Post;
      const to: To.Post = { ...from, views: String(from.views) };
      return to as unknown as Row;
    }
    default:
      return row;
  }
}

runTransform(transform);
```

**ForgeDB embeds no interpreter.** No QuickJS, no CPython, no bundled runtime — the transformer
links the runtime *you already have*, so where it lives is a config concern:

```toml
[toolchain]
bun    = { path = "/opt/homebrew/bin/bun", min_version = "1.1" }
node   = { path = "/usr/local/bin/node",   min_version = "20" }
python = { path = ".venv/bin/python",      min_version = "3.11" }
```

Location and version only — the language is never declared here. A relative `path` resolves
against the **project root**, not your working directory; an absent one means "resolve the bare
name on `PATH`". A missing or too-old interpreter is a clear error naming what was expected and
what was found, raised **before** any code is generated.

`migrate build` verifies your transform against the recorded scaffold hash, copies it into the
build cache, and bakes the interpreter's absolute path into the transformer — so `migrate run`
executes exactly what `migrate build` checked. The two processes speak one JSON object per line,
strictly in order, one child per hop. A non-zero exit, a malformed reply, or an early exit each
fail the whole hop and reproduce the child's own output; the destination is not published and the
source dir is your rollback.

**Go-target projects get the Rust escape.** Go is compiled, so "run the author's own runtime out
of process" would mean invoking a toolchain and linking generated packages — materially more than
a line-oriented host loop.

---

## Deferred

- `compaction_epoch` verification before apply (the format-version guard is the interlock
  today; an in-process `compact()` renumbers rows within an epoch).
- Cheap in-place byte-op hops (drop/rename without an O(rows) typed rewrite) — a perf
  optimization over uniform typed replay.
- Online (live-writer) migration — the transformer is offline/exclusive-writer.
- A one-command lineage migration with version auto-detection and a per-tenant sweep (the removed
  `migrate up`) — **#373**.
