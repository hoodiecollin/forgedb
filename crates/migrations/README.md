# forgedb-migrations

Schema migration system for ForgeDB with automated diffing, generation, execution, and tracking.

## Overview

`forgedb-migrations` is the schema migration and evolution system for ForgeDB. It provides a complete migration workflow from detecting schema changes to generating migration files, executing them safely, and tracking their state. The system supports both breaking and non-breaking changes with built-in validation and rollback capabilities.

## Features

- **Schema diffing algorithm** - Automatically detect changes between schema versions
- **Migration generation** - Create timestamped migration files with checksums
- **Migration execution** - Apply or rollback migrations with proper ordering
- **Migration tracking** - Persistent state tracking of applied migrations
- **Breaking change detection** - Automatic identification of potentially dangerous operations
- **Checksum verification** - Ensure migration file integrity
- **Rollback support** - Undo applied migrations (where possible)
- **Detailed reporting** - Human-readable descriptions of all changes

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
forgedb-migrations = "0.1"
```

## Usage Examples

### Diffing Schemas

Compare two schemas to detect changes:

```rust
use forgedb_migrations::{
    SchemaDiffer, SimpleSchema, SimpleModel, SimpleField
};

// Define old schema
let old_schema = SimpleSchema {
    models: vec![
        SimpleModel {
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
                }
            ],
            composite_indexes: vec![],
        }
    ],
};

// Define new schema with added field
let new_schema = SimpleSchema {
    models: vec![
        SimpleModel {
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
                },
                SimpleField {
                    name: "email".to_string(),
                    field_type: "string".to_string(),
                    nullable: false,
                    unique: true,
                    indexed: true,
                    index_type: "Hash".to_string(),
                    constraints: vec![],
                }
            ],
            composite_indexes: vec![],
        }
    ],
};

// Diff the schemas
let changes = SchemaDiffer::diff(&old_schema, &new_schema);

// Inspect changes
for change in &changes {
    println!("{}", change.description());
    if change.is_breaking() {
        println!("  ⚠️  This is a breaking change!");
    }
}
```

### Generating Migrations

Create a migration file from detected changes:

```rust
use forgedb_migrations::{MigrationGenerator, SchemaChange};
use std::path::PathBuf;

// Define changes to apply
let changes = vec![
    SchemaChange::AddModel {
        model_name: "User".to_string(),
    },
    SchemaChange::AddField {
        model_name: "User".to_string(),
        field_name: "email".to_string(),
        field_type: "string".to_string(),
        nullable: false,
        default_value: None,
    },
];

// Generate migration file
let migration = MigrationGenerator::generate(
    PathBuf::from("./migrations"),
    "Create User model".to_string(),
    changes
)?;

println!("Generated migration: {}", migration.filename());
println!("Migration ID: {}", migration.id);
println!("Checksum: {}", migration.checksum);

// Generate a report
let report = MigrationGenerator::generate_report(&migration);
println!("{}", report);
```

### Executing Migrations

Apply migrations to your database:

```rust
use forgedb_migrations::{MigrationExecutor, MigrationGenerator};
use std::path::PathBuf;

let migrations_dir = PathBuf::from("./migrations");
let data_dir = PathBuf::from("./data");

// Load all migrations
let migrations = MigrationGenerator::load_all_migrations(&migrations_dir)?;

for migration in &migrations {
    // Execute the migration (forward)
    MigrationExecutor::execute_up(&migration, &data_dir)?;
    println!("✓ Applied migration: {}", migration.description);
}
```

### Rolling Back Migrations

Undo the last applied migration:

```rust
use forgedb_migrations::{MigrationExecutor, MigrationTracker};
use std::path::PathBuf;

let migrations_dir = PathBuf::from("./migrations");
let data_dir = PathBuf::from("./data");

// Load tracker to find last migration
let mut tracker = MigrationTracker::new(&migrations_dir)?;

if let Some(last) = tracker.last_migration() {
    // Load the migration
    let migrations = MigrationGenerator::load_all_migrations(&migrations_dir)?;
    let migration = migrations
        .iter()
        .find(|m| m.id == last.migration_id)
        .expect("Migration not found");
    
    // Execute rollback (reverse)
    MigrationExecutor::execute_down(&migration, &data_dir)?;
    
    // Update tracker
    tracker.mark_rolled_back()?;
    
    println!("✓ Rolled back migration: {}", migration.description);
}
```

### Tracking Migration History

Track which migrations have been applied:

```rust
use forgedb_migrations::{MigrationTracker, MigrationGenerator};
use std::path::PathBuf;

let migrations_dir = PathBuf::from("./migrations");

// Create tracker
let mut tracker = MigrationTracker::new(&migrations_dir)?;

// Load all available migrations
let all_migrations = MigrationGenerator::load_all_migrations(&migrations_dir)?;

for migration in &all_migrations {
    if !tracker.is_applied(&migration.id) {
        // Execute migration...
        // MigrationExecutor::execute_up(&migration, &data_dir)?;
        
        // Mark as applied
        tracker.mark_applied(
            migration.id.clone(),
            migration.checksum.clone()
        )?;
        
        println!("✓ Applied and tracked: {}", migration.description);
    } else {
        println!("⊘ Already applied: {}", migration.description);
    }
}

// Get status summary
println!("{}", tracker.status_summary(all_migrations.len()));

// List pending migrations
let pending_ids: Vec<String> = all_migrations
    .iter()
    .map(|m| m.id.clone())
    .collect();
let pending = tracker.pending_migrations(&pending_ids);
println!("Pending migrations: {}", pending.len());
```

## Diffing Algorithm

The schema diffing algorithm compares two schema states and produces a list of changes. It works by:

### Detection Process

1. **Build lookup maps** - Index models and fields by name for efficient comparison
2. **Detect model changes** - Compare model presence in old vs new schema
3. **Detect field changes** - For each common model, compare field definitions
4. **Detect index changes** - Compare index definitions and constraints
5. **Order changes** - Ensure dependent changes are applied in correct order

### Supported Changes

The differ can detect the following schema changes:

#### Model-Level Changes
- **AddModel** - A new model was added to the schema
- **RemoveModel** - A model was removed (⚠️ BREAKING)
- **RenameModel** - A model was renamed (preserves data)

#### Field-Level Changes
- **AddField** - A new field was added to a model
- **RemoveField** - A field was removed (⚠️ BREAKING)
- **RenameField** - A field was renamed (preserves data)
- **ChangeFieldType** - Field's data type changed (⚠️ BREAKING)
- **ChangeFieldNullability** - Field's nullability changed (⚠️ BREAKING if making non-nullable)

#### Index-Level Changes
- **AddIndex** - An index was added to a field
- **RemoveIndex** - An index was removed from a field
- **AddCompositeIndex** - A composite index was added
- **RemoveCompositeIndex** - A composite index was removed

#### Constraint-Level Changes
- **AddUniqueConstraint** - A unique constraint was added (⚠️ may fail if duplicates exist)
- **RemoveUniqueConstraint** - A unique constraint was removed
- **AddConstraint** - A validation constraint was added
- **RemoveConstraint** - A validation constraint was removed

### Breaking vs Non-Breaking Changes

The system automatically categorizes changes as breaking or non-breaking:

#### Breaking Changes (require careful review)
- Removing models or fields (data loss)
- Changing field types (data conversion required)
- Making fields non-nullable (may fail if nulls exist)
- Adding unique constraints (may fail if duplicates exist)

#### Non-Breaking Changes (safe to apply)
- Adding models or fields
- Adding indexes
- Removing constraints
- Making fields nullable
- Renaming (with proper mapping)

### Change Detection Example

```rust
use forgedb_migrations::{SchemaDiffer, SimpleSchema, SimpleModel, SimpleField};

let old_schema = SimpleSchema {
    models: vec![SimpleModel {
        name: "User".to_string(),
        fields: vec![SimpleField {
            name: "age".to_string(),
            field_type: "u32".to_string(),
            nullable: true,
            unique: false,
            indexed: false,
            index_type: "Hash".to_string(),
            constraints: vec![],
        }],
        composite_indexes: vec![],
    }],
};

let new_schema = SimpleSchema {
    models: vec![SimpleModel {
        name: "User".to_string(),
        fields: vec![SimpleField {
            name: "age".to_string(),
            field_type: "u32".to_string(),
            nullable: false,  // Changed to non-nullable
            unique: false,
            indexed: true,    // Added index
            index_type: "Hash".to_string(),
            constraints: vec![],
        }],
        composite_indexes: vec![],
    }],
};

let changes = SchemaDiffer::diff(&old_schema, &new_schema);

// Will detect:
// 1. ChangeFieldNullability (BREAKING - making non-nullable)
// 2. AddIndex (safe)
```

## API Reference

### Core Types

#### `SchemaDiffer`

Compares two schema states and generates a list of changes.

**Methods:**
- `diff(old_schema: &SimpleSchema, new_schema: &SimpleSchema) -> Vec<SchemaChange>` - Compare schemas and return list of changes

#### `SimpleSchema`

Simplified schema representation for diffing.

**Fields:**
- `models: Vec<SimpleModel>` - List of models in the schema

#### `SimpleModel`

Model definition for schema comparison.

**Fields:**
- `name: String` - Model name
- `fields: Vec<SimpleField>` - List of fields
- `composite_indexes: Vec<Vec<String>>` - Composite indexes (field name lists)

#### `SimpleField`

Field definition for schema comparison.

**Fields:**
- `name: String` - Field name
- `field_type: String` - Field data type (e.g., "string", "u64", "uuid")
- `nullable: bool` - Whether field can be null
- `unique: bool` - Whether field has unique constraint
- `indexed: bool` - Whether field is indexed
- `index_type: String` - Type of index (e.g., "Hash", "BTree")
- `constraints: Vec<SimpleConstraint>` - Field constraints

#### `SimpleConstraint`

Constraint definition for field validation.

**Fields:**
- `name: String` - Constraint name (e.g., "email", "minLength")
- `params: Vec<String>` - Constraint parameters

### Migration Types

#### `MigrationGenerator`

Generates and loads migration files.

**Methods:**
- `generate<P: AsRef<Path>>(migrations_dir: P, description: String, changes: Vec<SchemaChange>) -> Result<Migration, String>` - Generate a new migration file
- `load_migration<P: AsRef<Path>>(path: P) -> Result<Migration, String>` - Load a migration from file
- `load_all_migrations<P: AsRef<Path>>(migrations_dir: P) -> Result<Vec<Migration>, String>` - Load all migrations from directory
- `generate_report(migration: &Migration) -> String` - Generate human-readable migration report

#### `Migration`

Represents a migration with changes and metadata.

**Fields:**
- `id: String` - Unique identifier (timestamp-based: YYYYMMDDHHMMSS)
- `description: String` - Human-readable description
- `created_at: DateTime<Utc>` - Creation timestamp
- `changes: Vec<SchemaChange>` - List of schema changes
- `checksum: String` - File integrity checksum

**Methods:**
- `new(description: String, changes: Vec<SchemaChange>) -> Self` - Create new migration
- `filename() -> String` - Get filename (e.g., "20240101120000_create_user_model.json")
- `verify_checksum() -> bool` - Verify migration file integrity
- `has_breaking_changes() -> bool` - Check if migration contains breaking changes
- `breaking_changes() -> Vec<&SchemaChange>` - Get list of breaking changes
- `safe_changes() -> Vec<&SchemaChange>` - Get list of safe changes

#### `SchemaChange`

Enum representing a single schema change.

**Variants:** See [Supported Changes](#supported-changes) section above.

**Methods:**
- `is_breaking() -> bool` - Returns true if change requires manual intervention
- `description() -> String` - Human-readable description of the change

### Execution Types

#### `MigrationExecutor`

Executes migrations against a database.

**Methods:**
- `execute_up<P: AsRef<Path>>(migration: &Migration, data_dir: P) -> Result<(), String>` - Apply migration (forward)
- `execute_down<P: AsRef<Path>>(migration: &Migration, data_dir: P) -> Result<(), String>` - Rollback migration (reverse)

### Tracking Types

#### `MigrationTracker`

Tracks applied migrations and their state.

**Methods:**
- `new<P: AsRef<Path>>(migrations_dir: P) -> Result<Self, String>` - Create tracker with state file
- `is_applied(migration_id: &str) -> bool` - Check if migration is applied
- `mark_applied(migration_id: String, checksum: String) -> Result<(), String>` - Mark migration as applied
- `mark_rolled_back() -> Result<Option<MigrationRecord>, String>` - Mark last migration as rolled back
- `applied_migrations() -> &[MigrationRecord]` - Get list of applied migrations
- `last_migration() -> Option<&MigrationRecord>` - Get last applied migration
- `pending_migrations(all_migrations: &[String]) -> Vec<String>` - Get list of pending migrations
- `verify_checksum(migration_id: &str, expected_checksum: &str) -> Result<(), String>` - Verify migration checksum
- `status_summary(total_migrations: usize) -> String` - Get status summary string

#### `MigrationState`

Persistent state of applied migrations.

**Fields:**
- `applied_migrations: Vec<MigrationRecord>` - Chronological list of applied migrations

**Methods:**
- `is_applied(migration_id: &str) -> bool` - Check if migration is applied
- `add_migration(migration_id: String, checksum: String)` - Record a migration as applied
- `remove_last_migration() -> Option<MigrationRecord>` - Remove last migration (for rollback)
- `last_migration() -> Option<&MigrationRecord>` - Get last applied migration

#### `MigrationRecord`

Record of an applied migration.

**Fields:**
- `migration_id: String` - Migration identifier
- `applied_at: DateTime<Utc>` - When migration was applied
- `checksum: String` - Checksum at time of application

## Migration Format

### File Structure

Migrations are stored as JSON files in a migrations directory:

```
migrations/
├── .migration_state.json         # Tracking state file
├── 20240101120000_create_user.json
├── 20240101130000_add_email_field.json
└── 20240102090000_add_user_index.json
```

### Migration File Format

Each migration file is a JSON document:

```json
{
  "id": "20240101120000",
  "description": "Create User model",
  "created_at": "2024-01-01T12:00:00Z",
  "changes": [
    {
      "AddModel": {
        "model_name": "User"
      }
    },
    {
      "AddField": {
        "model_name": "User",
        "field_name": "email",
        "field_type": "string",
        "nullable": false,
        "default_value": null
      }
    },
    {
      "AddIndex": {
        "model_name": "User",
        "field_name": "email",
        "index_type": "Hash"
      }
    }
  ],
  "checksum": "a1b2c3d4e5f6"
}
```

### Versioning Scheme

Migration IDs use timestamp-based versioning (YYYYMMDDHHMMSS):

- **Format**: 14-digit timestamp (year, month, day, hour, minute, second)
- **Example**: `20240101120000` = January 1, 2024, 12:00:00
- **Sorting**: Natural chronological order
- **Uniqueness**: Second-level granularity (avoid generating multiple in same second)

### Filename Convention

Migration filenames combine ID with sanitized description:

```
{id}_{sanitized_description}.json
```

Examples:
- `20240101120000_create_user_model.json`
- `20240101130000_add_email_field.json`
- `20240102090000_remove_old_field.json`

The description is sanitized by:
- Converting to lowercase
- Replacing spaces with underscores
- Removing non-alphanumeric characters (except underscores)

### State Tracking File

The `.migration_state.json` file tracks applied migrations:

```json
{
  "applied_migrations": [
    {
      "migration_id": "20240101120000",
      "applied_at": "2024-01-01T12:05:30Z",
      "checksum": "a1b2c3d4e5f6"
    },
    {
      "migration_id": "20240101130000",
      "applied_at": "2024-01-01T13:10:15Z",
      "checksum": "b2c3d4e5f6a7"
    }
  ]
}
```

### Metadata Tracking

Each migration file includes:

- **ID**: Unique timestamp-based identifier
- **Description**: Human-readable purpose of migration
- **Created At**: ISO 8601 timestamp of generation
- **Changes**: Array of schema change operations
- **Checksum**: Hash of migration contents for integrity verification

The checksum ensures:
- Migration files haven't been modified after creation
- Applied migrations match their original definitions
- Migration history integrity is maintained

## Safety

The migration system includes several safety features:

### Validation Before Execution

Before executing a migration:

1. **Checksum verification** - Ensure file hasn't been modified
   ```rust
   if !migration.verify_checksum() {
       return Err("Migration file corrupted!".to_string());
   }
   ```

2. **Breaking change warnings** - Alert on potentially dangerous operations
   ```rust
   if migration.has_breaking_changes() {
       println!("⚠️  WARNING: This migration contains breaking changes:");
       for change in migration.breaking_changes() {
           println!("  - {}", change.description());
       }
   }
   ```

3. **Duplicate detection** - Prevent applying same migration twice
   ```rust
   if tracker.is_applied(&migration.id) {
       return Err("Migration already applied!".to_string());
   }
   ```

### Rollback Support

The system supports rolling back migrations:

**Supported Rollbacks:**
- Add/Remove models (can be reversed)
- Add/Remove fields (field removal rollback loses data)
- Add/Remove indexes (fully reversible)
- Add/Remove constraints (fully reversible)

**Limited Rollbacks:**
- Type changes (requires data conversion logic)
- Nullability changes (may fail if data incompatible)

**Rollback Example:**
```rust
// Execute migration
MigrationExecutor::execute_up(&migration, &data_dir)?;
tracker.mark_applied(migration.id.clone(), migration.checksum.clone())?;

// Later, rollback if needed
MigrationExecutor::execute_down(&migration, &data_dir)?;
tracker.mark_rolled_back()?;
```

### Data Preservation

The executor implements strategies to preserve data:

1. **Additive changes** - New fields get default/null values
2. **Column removal** - Data files aren't immediately deleted
3. **Type changes** - Require explicit migration logic
4. **Unique constraints** - Validated before application

### Best Practices

For safe migrations:

1. **Review breaking changes** - Manually inspect before applying
   ```rust
   let report = MigrationGenerator::generate_report(&migration);
   println!("{}", report);
   // Review output, ensure it's safe to proceed
   ```

2. **Test on staging** - Apply to non-production environment first
3. **Backup data** - Before applying breaking changes
4. **Use transactions** - Apply migrations within database transactions (when available)
5. **Monitor application** - Track migration progress and errors

### Error Handling

All operations return `Result` types:

```rust
match MigrationExecutor::execute_up(&migration, &data_dir) {
    Ok(_) => println!("✓ Migration applied successfully"),
    Err(e) => {
        eprintln!("✗ Migration failed: {}", e);
        // Consider rollback or manual intervention
    }
}
```

### Checksum Integrity

Checksums prevent tampering:

```rust
// Verify checksum before applying
if !migration.verify_checksum() {
    return Err("Migration file has been modified!".to_string());
}

// Verify checksum of applied migration
tracker.verify_checksum(&migration.id, &migration.checksum)?;
```

## Testing

### Running Migration Tests

```bash
# Run all tests
cargo test --package forgedb-migrations

# Run specific test
cargo test --package forgedb-migrations test_full_migration_workflow

# Run with output
cargo test --package forgedb-migrations -- --nocapture

# Run doc tests
cargo test --package forgedb-migrations --doc
```

### Test Coverage

The test suite includes:

- ✅ Full migration workflow (generate, track, execute)
- ✅ Breaking change detection
- ✅ Migration rollback
- ✅ Schema diffing (models, fields, indexes)
- ✅ Checksum verification
- ✅ State tracking and persistence
- ✅ Migration loading and ordering

### Example Tests

#### Testing Schema Diffing

```rust
use forgedb_migrations::{SchemaDiffer, SimpleSchema, SimpleModel, SimpleField};

#[test]
fn test_detect_added_field() {
    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![],
            composite_indexes: vec![],
        }],
    };
    
    let new_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "email".to_string(),
                field_type: "string".to_string(),
                nullable: false,
                unique: false,
                indexed: false,
                index_type: "Hash".to_string(),
                constraints: vec![],
            }],
            composite_indexes: vec![],
        }],
    };
    
    let changes = SchemaDiffer::diff(&old_schema, &new_schema);
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::AddField { .. }));
}
```

#### Testing Migration Workflow

```rust
use forgedb_migrations::*;
use tempfile::TempDir;

#[test]
fn test_full_migration_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("migrations");
    let data_dir = temp_dir.path().join("data");
    
    // Define changes
    let changes = vec![
        SchemaChange::AddModel {
            model_name: "User".to_string(),
        },
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "email".to_string(),
            field_type: "string".to_string(),
            nullable: false,
            default_value: None,
        },
    ];
    
    // Generate migration
    let migration = MigrationGenerator::generate(
        &migrations_dir,
        "Create User model".to_string(),
        changes
    ).unwrap();
    
    // Create tracker
    let mut tracker = MigrationTracker::new(&migrations_dir).unwrap();
    assert!(!tracker.is_applied(&migration.id));
    
    // Execute migration
    MigrationExecutor::execute_up(&migration, &data_dir).unwrap();
    
    // Mark as applied
    tracker.mark_applied(
        migration.id.clone(),
        migration.checksum.clone()
    ).unwrap();
    
    assert!(tracker.is_applied(&migration.id));
}
```

#### Testing Breaking Changes

```rust
#[test]
fn test_breaking_change_detection() {
    let changes = vec![
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "name".to_string(),
            field_type: "string".to_string(),
            nullable: false,
            default_value: None,
        },
        SchemaChange::RemoveField {
            model_name: "User".to_string(),
            field_name: "old_field".to_string(),
        },
    ];
    
    let migration = Migration::new("Update User model".to_string(), changes);
    
    assert!(migration.has_breaking_changes());
    assert_eq!(migration.breaking_changes().len(), 1);
    assert_eq!(migration.safe_changes().len(), 1);
}
```

## Related Crates

- **[forgedb-types](../types)**: Core type definitions used in schema changes
- **[forgedb-storage](../storage)**: Storage layer modified by migration executor
- **[forgedb-crud-api](../crud-api)**: High-level API that uses migrations
- **[forgedb-validation](../validation)**: Validation constraints referenced in migrations

## Dependencies

- `serde` - Serialization/deserialization of migration files
- `serde_json` - JSON format for migration files
- `chrono` - Timestamp handling for migration IDs and tracking
- `uuid` - UUID support in schema types

Dev dependencies:
- `tempfile` - Temporary directories for testing

## Contributing

This crate is part of the ForgeDB project. For contribution guidelines, see the main repository.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT License ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
