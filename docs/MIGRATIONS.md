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
[V1_ROADMAP.md](./V1_ROADMAP.md) and the design note
[`proposals/schema-migrations-impl-plan.md`](./proposals/schema-migrations-impl-plan.md).

---

## The version interlock

Every generated app bakes in an `EXPECTED_FORMAT_VERSION`, derived from your migration
lineage. On open it reads one opaque integer (`format_version`) from each data dir manifest
and **refuses a mismatch** rather than mis-decoding stale bytes. This is fail-fast by design:
a regenerated app will not silently read a not-yet-migrated dir, and the transformer will not
apply the wrong range to a dir. The version is the only cross-schema handshake — it never
reads column names or types to "self-heal".

A fresh database baselines at `format_version = 1`; each recorded migration bumps it by one.

---

## Additive changes — automatic, data-preserving

An additive change is one existing rows can satisfy without a value being invented for them:
**a new model**, or **a new nullable field** (`field: T?`, read as `None` by existing rows).

```bash
# 1. Edit schema.forge — add the new nullable field AT THE END of the model.
# 2. Record the change (baselines the lineage on first run):
forgedb migrate create "add note field" --auto --schema schema.forge
# 3. Regenerate and rebuild your app:
forgedb generate
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

# 3. Regenerate your app (its EXPECTED_FORMAT_VERSION advances to the new version):
forgedb generate

# 4. With the app STOPPED, migrate the data in one step:
forgedb migrate up --from 1 --to 2 --src ./data --dest ./data-migrated

# 5. Point the regenerated app at ./data-migrated.
```

`migrate up` generates the transformer crate for the version range, `cargo build`s it, and
runs it over the data dir. It writes a **fresh destination** and leaves the source untouched,
so the original *is* your rollback. `--from` defaults to the version detected from the source
manifests and `--to` to the lineage's current version, so `forgedb migrate up --src ./data
--dest ./data-migrated` usually suffices.

### How the transformer works

For a `--from B --to G` range, ForgeDB emits a self-contained crate (`migrations/transform/`):
one typed module per version (`vN.rs`, each carrying its own version open-guard), any frozen
authored bodies embedded verbatim, and a `main.rs` that is a **fixed straight-line chain** of
named `transform_vN_to_vM` hop functions — no runtime step interpreter. Each hop reads every
row through the `vN` typed structs, applies the baked structural ops then the authored
transform, and writes through `vM`'s `insert` (which preserves record ids, so foreign keys stay
valid). Multi-hop ranges replay through temp dirs and publish with a single atomic rename.

The crate depends only on your app's substrate (storage/types/etc.) — never on
`forgedb-parser` or `forgedb-migrations`, and it never parses a `.forge` at runtime.

---

## Per-tenant sweep

Under multi-tenancy (#59), each tenant is an independent data dir under one root. `migrate up`
sweeps them in one command:

```bash
forgedb migrate up --tenant-root ./tenants --to 2
# migrates ./tenants/<t> → ./tenants/<t>-migrated-v2 for every tenant data dir
```

The sweep builds the transformer once and runs it per tenant independently: a tenant that
fails (or is at an unexpected version — the bin's open-guard refuses it) is reported and
skipped with its source unchanged, and the command exits non-zero if any tenant failed. Use
`--dest-suffix` to change the output naming.

---

## Alternative: manual dump → reload

For a one-off change where you would rather not build a transformer, you can still dump with
the old binary and reload into a fresh dir through `Database::create_<model>` (ids preserved,
full integrity enforced), transforming each row in app code. This is the same typed replay the
generated transformer automates; prefer `migrate up` for anything you will run more than once
or across many tenants.

---

## Command reference

| Command | Purpose |
|---|---|
| `forgedb migrate create <desc> --auto --schema <file>` | Diff against the snapshot; record + classify the change; scaffold any authored body. |
| `forgedb migrate create <desc>` | Create an empty manual migration template. |
| `forgedb migrate status` | Show applied / pending migrations. |
| `forgedb migrate up --src <data> --dest <migrated> [--from F --to T]` | Build the transformer + run it over one data dir. |
| `forgedb migrate up --tenant-root <root> [--to T]` | Per-tenant sweep. |
| `forgedb migrate build --from F --to T [--output <dir>]` | (Lower-level) generate + compile the transformer only. |
| `forgedb migrate run --src <data> --dest <migrated> [--bin-dir <dir>]` | (Lower-level) run an already-built transformer. |
| `forgedb generate transform --from F --to T` | (Lower-level) emit the transformer crate without compiling. |

---

## Deferred

- Honoring `@default` (not just the type zero) on additive backfill.
- `compaction_epoch` verification before apply (the format-version guard is the interlock
  today; an in-process `compact()` renumbers rows within an epoch).
- Cheap in-place byte-op hops (drop/rename without an O(rows) typed rewrite) — a perf
  optimization over uniform typed replay.
- Online (live-writer) migration — `migrate up` is offline/exclusive-writer.
