# forgedb-migrations

The **dev-time** half of ForgeDB's schema-migration system: it detects how a
`.forge` schema evolves, records each change as a versioned migration, classifies
every hop, and maintains the serial version lineage.

It does **not** rewrite data at rest. Applying a migration to a data directory is
done by an offline, per-version-range **transformer binary** that
[`forgedb-codegen`](../codegen)'s `TransformGenerator` emits and that the
`forgedb migrate` CLI builds and runs. Keeping the rewrite in generated code
preserves ForgeDB's core invariant — the schema is a compile-time input to
generation, never a runtime input to a generic engine.

**The end-to-end operator workflow lives in
[`docs/MIGRATIONS.md`](../../docs/MIGRATIONS.md).** This README documents the
crate's library API only.

## Overview

`forgedb-migrations` covers the record-and-classify side of schema evolution:

- **Schema diffing** — compare two schema states and produce a list of changes.
- **Migration generation** — write timestamped, versioned (`from → to`) migration
  records to a `migrations/` directory.
- **Hop classification** — tag each change `Auto` (the differ can prove the new-row
  body from the diff alone) or `Authored` (it needs a hand-written transform).
- **Lineage** — order the serial version chain and expand a `--from`/`--to` range
  into the ordered hop sequence the transformer replays.
- **Tracking** — persist which migrations have been applied, with checksum
  verification.

## Installation

```toml
[dependencies]
forgedb-migrations = "0.1"
```

## Dev-time flow

```text
Schema v1 → Schema v2
    │
    ▼
SchemaDiffer::diff  ──►  Vec<SchemaChange>
    │
    ▼
MigrationGenerator::generate_versioned  ──►  Migration { changes, from_version, to_version }
    │
    ▼
hop_body_class()  ──►  Auto | Authored   (Authored hops are scaffolded for you to author + freeze)
    │
    ▼
forgedb-codegen TransformGenerator  ──►  offline transformer bin
    │
    ▼
forgedb migrate build ──► migrate run --from 1 --to 2  ──►  data rewritten
```

## Usage Examples

### Diffing schemas

Compare two schema states and inspect the detected changes:

```rust
use forgedb_migrations::{SchemaDiffer, SimpleSchema, SimpleModel, SimpleField};

let old_schema = SimpleSchema {
    models: vec![SimpleModel {
        name: "User".to_string(),
        fields: vec![SimpleField {
            name: "id".to_string(),
            field_type: "uuid".to_string(),
            nullable: false,
            unique: true,
            indexed: false,
            index_type: "Hash".to_string(),
            constraints: vec![],
            depends_on: vec![],
        }],
        composite_indexes: vec![],
    }],
    enums: vec![],
    structs: vec![],
};

let new_schema = SimpleSchema {
    models: vec![SimpleModel {
        name: "User".to_string(),
        fields: vec![
            SimpleField {
                name: "id".to_string(),
                field_type: "uuid".to_string(),
                nullable: false,
                unique: true,
                indexed: false,
                index_type: "Hash".to_string(),
                constraints: vec![],
                depends_on: vec![],
            },
            SimpleField {
                name: "email".to_string(),
                field_type: "string".to_string(),
                nullable: true,
                unique: false,
                indexed: false,
                index_type: "Hash".to_string(),
                constraints: vec![],
                depends_on: vec![],
            },
        ],
        composite_indexes: vec![],
    }],
    enums: vec![],
    structs: vec![],
};

// `SchemaDiffer` is a unit struct; `diff` is a static method.
let changes = SchemaDiffer::diff(&old_schema, &new_schema);
for change in &changes {
    println!("{}", change.description());
    if change.is_breaking() {
        println!("  ⚠️  breaking change");
    }
}
```

### Recording a versioned migration + classifying its hops

Derive the next version span from the existing lineage, record the migration, then
classify each hop:

```rust,no_run
use forgedb_migrations::{
    MigrationGenerator, MigrationLineage, SchemaChange, HopBodyClass,
};

// 1. The lineage tells you what version the DB is at and the span a new hop carries.
let lineage = MigrationLineage::load("./migrations")?;
let (from, to) = lineage.next_version_span();

// 2. Record the detected changes as a versioned migration record on disk.
let changes = vec![SchemaChange::AddField {
    model_name: "User".to_string(),
    field_name: "email".to_string(),
    field_type: "string".to_string(),
    nullable: true,
    default_value: None,
}];
let migration = MigrationGenerator::generate_versioned(
    "./migrations",
    "add user email".to_string(),
    changes,
    from,
    to,
)?;
println!("wrote {} (v{} → v{})", migration.filename(), from, to);

// 3. Classify. `Authored` residue must be hand-written and frozen before the
//    transformer can rewrite data at rest; `Auto` hops need nothing.
for change in &migration.changes {
    match change.hop_body_class() {
        HopBodyClass::Auto => println!("auto:     {}", change.description()),
        HopBodyClass::Authored => println!("authored: {}", change.description()),
    }
}
# Ok::<(), String>(())
```

### Tracking applied migrations

```rust,no_run
use forgedb_migrations::{MigrationGenerator, MigrationTracker};

let mut tracker = MigrationTracker::new("./migrations")?;
let all = MigrationGenerator::load_all_migrations("./migrations")?;

for migration in &all {
    if tracker.is_applied(&migration.id) {
        continue;
    }
    // ... apply the migration to data via the generated transformer bin ...
    tracker.mark_applied(migration.id.clone(), migration.checksum.clone())?;
}

println!("{}", tracker.status_summary(all.len()));
let pending_ids: Vec<String> = all.iter().map(|m| m.id.clone()).collect();
println!("pending: {}", tracker.pending_migrations(&pending_ids).len());
# Ok::<(), String>(())
```

## Public API

### Diffing

- **`SchemaDiffer`** — `diff(old: &SimpleSchema, new: &SimpleSchema) -> Vec<SchemaChange>`.
- **`SimpleSchema`** / **`SimpleModel`** / **`SimpleField`** / **`SimpleConstraint`** /
  **`SimpleEnum`** / **`SimpleStruct`** — the simplified schema representation the
  differ compares. The CLI builds these from parsed `.forge` schemas; you can also
  construct them directly.

  `SimpleSchema` carries the `enum` / `struct` **definitions**, not just the models,
  and `SimpleField` carries `depends_on` — the enum/struct names its type reaches
  transitively (#438). Both are load-bearing: a field's `field_type` string names an
  enum but carries none of its contents, so without them two schemas differing only
  in `Status`'s variant order compare **equal**, and a reorder that re-maps every
  stored 1-byte discriminant produces no change at all.

### Change model

- **`SchemaChange`** — an enum of a single detected change. Variants:
  `AddModel`, `RemoveModel`, `RenameModel`, `AddField`, `RemoveField`,
  `RenameField`, `ChangeFieldType`, `ChangeFieldNullability`, `AddIndex`,
  `RemoveIndex`, `AddUniqueConstraint`, `RemoveUniqueConstraint`,
  `AddCompositeIndex`, `RemoveCompositeIndex`, `AddConstraint`, `RemoveConstraint`,
  `ChangeEnumVariants`, `ChangeStructLayout`.
  Methods: `is_breaking()`, `hop_body_class()`, `target_model()`, `description()`.
- **`classify_positional`** / **`classify_layout`** (→ `PositionalDelta` /
  `LayoutDelta`) — the ONE classifier behind `ChangeEnumVariants` and
  `ChangeStructLayout` (#438). Both variants carry the two ordered *lists* rather
  than a pre-baked verdict, and `is_breaking` / `hop_body_class` / `description` all
  read the classifier rather than re-deriving it — so a stored record re-classifies
  identically forever, which is what `HopBodyClass`'s frozen-at-`migrate create`
  contract requires.
- **`HopBodyClass`** — `Auto` (the differ can prove the new-row body from the diff)
  or `Authored` (the body needs semantic understanding the diff cannot supply, e.g.
  re-encoding a changed type, filling a narrowed `NOT NULL`, or a newly-required
  field with no default). Distinct from `is_breaking()`: a change can be breaking
  yet still `Auto`.

### Migration records

- **`Migration`** — a recorded set of changes plus metadata: `id` (timestamp),
  `description`, `created_at`, `changes`, `checksum`, and the serial version
  interlock `from_version` / `to_version`. Constructed via `new` /
  `new_versioned`. Methods include `filename()`, `verify_checksum()`,
  `has_breaking_changes()`, `breaking_changes()`, `safe_changes()`, and
  `authored_changes()`.
- **`MigrationGenerator`** — writes and loads migration records: `generate`,
  `generate_versioned`, `load_migration`, `load_all_migrations`,
  `generate_report`.

### Lineage

- **`MigrationLineage`** — the ordered serial version chain loaded from a
  `migrations/` directory: `load`, `migrations`, `current_schema_version`,
  `next_version_span`, and `expand_range(from, to)` (the ordered hop subsequence
  the transformer replays; refuses a non-contiguous range).
- **`BASELINE_FORMAT_VERSION`** — the fresh-database baseline (`1`).
- Path + scaffold helpers: `current_schema_version`, `versioned_schema_dir` /
  `versioned_schema_path`, `save_versioned_schema` / `load_versioned_schema`,
  `migration_body_dir`, `authored_body_path`, and `scaffold_authored_body`
  (writes a documented `transform.rs` stub for any `Authored` residue, never
  clobbering an already-frozen body).

### Tracking

- **`MigrationTracker`** — persistent applied-migration state: `is_applied`,
  `mark_applied`, `mark_rolled_back`, `applied_migrations`, `last_migration`,
  `pending_migrations`, `verify_checksum`, `status_summary`.
- **`MigrationRecord`** / **`MigrationState`** — the persisted records and their
  container.

## On-disk layout

A `migrations/` directory holds one JSON record per migration, plus per-version
schema snapshots and any authored transform bodies:

```
migrations/
├── .migration_state.json          # applied-migration tracking state
├── 20240101120000_add_user_email.json
├── 20240101120000/
│   └── transform.rs               # frozen authored body (only for Authored hops)
└── schemas/
    ├── v1.forge                   # full schema snapshot per version
    └── v2.forge
```

Each migration record carries its detected changes and its serial version span:

```json
{
  "id": "20240101120000",
  "description": "add user email",
  "changes": [
    {
      "AddField": {
        "model_name": "User",
        "field_name": "email",
        "field_type": "string",
        "nullable": true,
        "default_value": null
      }
    }
  ],
  "from_version": 1,
  "to_version": 2
}
```

Migration IDs use timestamp-based versioning (`YYYYMMDDHHMMSS`), so they sort in
chronological order. The `checksum` field covers every other field (including the
version span) and lets `verify_checksum` detect a tampered record.

## Safety model

- **Version interlock** — a regenerated app bakes the lineage's current
  `format_version` into `EXPECTED_FORMAT_VERSION` and refuses to open a data
  directory at a different version rather than mis-decoding it. It fails fast; it
  never self-heals.
- **Rollback by retention** — the generated transformer writes a fresh destination
  directory and leaves the source untouched, so the original data is the rollback.
- **Atomic publish** — a multi-hop range materializes through temporary
  directories and a single final rename (all-or-nothing).
- **Checksums** — recorded and applied migrations are checksum-verified to catch
  tampering.

## Related crates

- [`forgedb-parser`](../parser) — parses `.forge` schema files.
- [`forgedb-codegen`](../codegen) — emits the offline transformer bin that rewrites
  data at rest.
- [`forgedb-validation`](../validation) — validates schemas and schema changes.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT License ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
</content>
</invoke>
