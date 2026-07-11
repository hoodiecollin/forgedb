# Schema Migrations (v1)

ForgeDB is a **code generator**, so schema evolution follows the generate-then-compile
model: you edit `schema.forge`, regenerate, and recompile your app. What the *data* does on
reopen depends on whether the change is **additive** or **breaking**.

v1 ships an **additive** migration path that preserves existing rows automatically, plus a
documented **dump → regenerate → reload** path for breaking changes. There is deliberately
**no runtime data-transform engine** in v1 (see [V1_ROADMAP.md](./V1_ROADMAP.md)).

---

## Additive changes — automatic, data-preserving

An additive change is one existing rows can satisfy without a value being invented for them:

- **A new model.**
- **A new nullable field** (`field: T?`) — existing rows read it as `None`.
- **A new field with a zero-meaningful type**, appended at the end — existing rows read the
  type default (numeric `0`, `false`, `""`, nil uuid).

### How it works

When you regenerate and reopen an existing data directory, generated recovery **anchors on
the tombstone row count** (the authoritative committed row count) and **backfills any column
that is shorter than the anchor** — a newly-added field — with that field's default. A column
*longer* than the anchor (a torn insert tail) is truncated as before. Existing rows are never
touched; the new column is simply filled in.

### Procedure

```bash
# 1. Edit schema.forge — add the new (nullable) field AT THE END of the model.
# 2. Record + verify the change is additive:
forgedb migrate create "add note field" --auto --schema schema.forge
# 3. Regenerate and rebuild your app:
forgedb generate
# 4. Restart. Existing rows are backfilled with defaults on first open.
```

`forgedb migrate create --auto` diffs `schema.forge` against the recorded snapshot
(`migrations/.schema-snapshot.forge`). If every change is additive it records the migration
and updates the snapshot. If **any** change is breaking it refuses and prints the reload
guidance below (and exits non-zero, so CI catches it).

### Constraints (important)

- **Append new fields at the end of the model.** Column files are addressed by position;
  inserting a field in the middle shifts every later column's identity and desynchronizes the
  existing data. Always add to the end.
- **New non-null fields backfill to the type zero, not to `@default`.** Honoring `@default`
  on backfill is a follow-up; until then, prefer nullable new fields when the zero value is
  not meaningful.
- **Reopen cleanly before the WAL is stale across a schema change.** Backfill runs before WAL
  replay; if the old app left an un-checkpointed WAL, replay of old-schema records is skipped
  (non-fatal, but those torn-tail rows are dropped). Let the old binary recover + checkpoint
  before migrating.

---

## Breaking changes — dump → regenerate → reload

A breaking change is one existing rows cannot satisfy automatically:

- **Removing a model or field.**
- **Changing a field's type** (e.g. `u32` → `string`).
- **Making a nullable field non-nullable.**
- **Adding a new non-null field without a default** (existing rows have no value).
- **Adding a `&unique` constraint** (existing data may already violate it).

v1 has no engine that transforms existing bytes for these. The supported path is an explicit
**dump → regenerate → reload**, with the transform written in your app (you know the intent of
the change; a generic engine cannot).

### Procedure

```
1. BEFORE editing the schema, dump every row with the CURRENT binary:
     let db = Database::open_at(old_dir);
     let rows = db.widget.all();                 // per model
     let json = serde_json::to_string(&rows)?;   // generated structs derive Serialize
     // write json to a file per model

2. Edit schema.forge for the breaking change, then regenerate + rebuild:
     forgedb generate

3. Reload into a FRESH data directory, transforming each row for the new shape:
     #[derive(Deserialize)] struct OldWidget { id: Uuid, sku: String, qty: u32 }
     let old: Vec<OldWidget> = serde_json::from_str(&json)?;
     let mut db = Database::open_at(new_dir);     // freshly generated schema
     for r in &old {
         db.create_widget(Widget { id: r.id, sku: r.sku, qty: r.qty.to_string() })?; // transform
     }
     db.checkpoint();
```

The `id`s are preserved, so foreign keys across models stay valid as long as you reload every
model. Reload through `Database::create_<model>` (not `db.<model>.insert`) so full integrity —
including foreign-key existence — is enforced on the way in.

This exact round-trip (a `u32 → string` type change) is covered end-to-end by the Phase 4
Workstream 4 harness.

---

## Command reference

| Command | Purpose |
|---|---|
| `forgedb migrate create <desc> --auto --schema <file>` | Diff against the snapshot; accept additive, refuse breaking. |
| `forgedb migrate create <desc>` | Create an empty manual migration template. |
| `forgedb migrate status` | Show applied / pending migrations. |
| `forgedb migrate up [--steps N]` | Apply pending migrations. |
| `forgedb migrate down [--steps N]` | Roll back migrations. |

---

## Explicitly out of v1

A runtime data-transform migration engine (in-place type changes, column drops with data
rewrite, index/constraint rewrites over live data). These are deferred; the reload path above
is the v1 answer for anything non-additive.
