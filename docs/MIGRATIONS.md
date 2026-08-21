# Schema Migrations

ForgeDB is a **code generator**, so schema evolution follows the generate-then-compile
model: you edit `schema.forge`, regenerate, and recompile your app. What happens to the
*data on disk* depends on the change:

- **Additive** changes (a new model, a new nullable field) are preserved automatically — the
  regenerated app backfills them on reopen. No data step.
- **Everything else** (a type change, a column/model drop, a nullable→NOT-NULL narrowing, a
  `&unique` add, a required field with no default) rewrites data-at-rest. ForgeDB generates a
  per-version **offline transformer bin** that does the rewrite, driven end-to-end by
  `forgedb migrate`.

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

## Additive changes — automatic, data-preserving

An additive change is one existing rows can satisfy without a value being invented for them:
**a new model**, or **a new nullable field** (`field: T?`, read as `None` by existing rows).

```bash
# 1. Edit schema.forge — add the new nullable field AT THE END of the model.
# 2. Record the change (baselines the lineage on first run):
forgedb migrate create "add note field" --auto --schema schema.forge
# 3. Regenerate and rebuild your app:
forgedb generate all --schema schema.forge
forgedb build --schema schema.forge
# 4. Restart. Existing rows are backfilled with defaults on first open.
```

On reopen, generated recovery **anchors on the tombstone row count** (the authoritative
committed count) and **backfills any column shorter than the anchor** — the new field — with
its default. Existing rows are never touched.

**Constraints:** append new fields at the **end** of the model (columns are position-addressed);
new non-null fields backfill to the type zero, not `@default` (prefer nullable when the zero is
not meaningful); let the old binary checkpoint its WAL before migrating.

---

## Data-rewriting changes — the transformer bin

For anything the reopen backfill cannot do, `forgedb migrate create --auto` still **records**
the change as a versioned hop and classifies it:

- **`Auto`** — the differ can prove the new-row body (drop a field/model, rename, add a
  `&unique`). No authoring needed.
- **`Authored`** — the differ cannot know the value (a type re-encode, a nullable→NOT-NULL
  fill, a required-add-without-default). `migrate create` writes a scaffold at
  `migrations/<id>/transform.rs` for you to fill in and freeze.

### Lifecycle

```bash
# 1. Edit schema.forge, then record + classify the change:
forgedb migrate create "qty to string" --auto --schema schema.forge
#    → records migrations/<id>_*.json (from_version -> to_version)
#    → snapshots migrations/schemas/v<n>.forge
#    → for Authored residue, scaffolds migrations/<id>/transform.rs

# 2. If an authored body was scaffolded, edit it. `authored_transform(model, row)`
#    receives each row as JSON AFTER the automatic (rename/drop/additive) ops and
#    returns it reshaped for the next version. Fill in every TODO.

# 3. Regenerate your app (its EXPECTED_SCHEMA_VERSION advances to the new version):
forgedb generate all --schema schema.forge

# 4. Build the transformer for the range, then run it with the app STOPPED:
forgedb migrate build --from 1 --to 2 --schema schema.forge
forgedb migrate run   --from 1 --to 2 --schema schema.forge \
  --src ./data --dest ./data-migrated

# 5. Point the regenerated app at ./data-migrated.
```

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
| `forgedb migrate create <desc> --auto --schema <file>` | Diff against the snapshot; record + classify the change; scaffold any authored body. |
| `forgedb migrate create <desc> --schema <file>` | Create an empty manual migration template. |
| `forgedb migrate status --schema <file>` | Show applied / pending migrations. |
| `forgedb migrate build --from F --to T --schema <file>` | Generate + compile the transformer for one range. |
| `forgedb migrate run --from F --to T --src <data> --dest <migrated> --schema <file>` | Run the transformer built for that range. |
| `forgedb generate transform --from F --to T --schema <file>` | (Lower-level) emit the transformer source without compiling. |
| `forgedb migrate engine --src <data> --dest <new-dir> --schema <file>` | Carry a dir across a ForgeDB **engine** byte-format generation (orthogonal to the schema serial). |

`--schema` is required on every row above. Two flags that used to appear here are **gone, and
error rather than no-op**: `migrate build -o/--output` and `migrate run --bin-dir` both named a
directory for a crate ForgeDB now places itself. `forgedb migrate up` is gone with them — its
one-command wrapper is the two rows above it, and its per-tenant sweep is **#373**.

---

## Deferred

- Honoring `@default` (not just the type zero) on additive backfill.
- `compaction_epoch` verification before apply (the format-version guard is the interlock
  today; an in-process `compact()` renumbers rows within an epoch).
- Cheap in-place byte-op hops (drop/rename without an O(rows) typed rewrite) — a perf
  optimization over uniform typed replay.
- Online (live-writer) migration — the transformer is offline/exclusive-writer.
- A one-command lineage migration with version auto-detection and a per-tenant sweep (the removed
  `migrate up`) — **#373**.
