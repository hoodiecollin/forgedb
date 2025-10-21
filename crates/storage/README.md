# ForgeDB Storage

Columnar storage engine for ForgeDB with memory-mapped file support and write-ahead logging.

## Overview

`forgedb-storage` is the persistence layer for ForgeDB, implementing a high-performance columnar storage format optimized for database workloads. The storage engine separates fixed-size and variable-length data into different file structures, uses tombstone bitmaps for efficient deletions, and integrates with the Write-Ahead Log (WAL) for ACID compliance and crash recovery.

### Key Features

- **Columnar storage format** - Columns stored separately for efficient scans
- **Memory-mapped file support** - Direct file I/O for low-latency access
- **Fixed-size vs variable-length columns** - Optimized layouts for different data types
- **Tombstone bitmap** - Efficient soft deletes without data rewriting
- **WAL integration** - ACID properties and crash recovery
- **Zero-copy reads** - Direct access to file data without intermediate buffers
- **Persistent metadata** - JSON manifest for schema versioning

## Architecture

### Columnar Storage Layout

ForgeDB uses a columnar storage format where each column is stored in separate files. This design enables:
- **Efficient scans**: Read only the columns you need
- **Better compression**: Similar data types compress well
- **Optimized for analytics**: Column-oriented queries are fast

### Directory Structure

For a schema like `User { id: +u64, email: &string }`, the storage layout is:

```
data/
├── manifest.json              # Database metadata and schema
├── tombstones.bin            # Deletion bitmap (1 byte per row)
├── fixed/
│   └── u64_0.bin            # id column (8 bytes per record)
└── variable/
    ├── string_data_0.bin    # email data (append-only)
    └── string_offsets_0.bin # (offset, length) pairs (16 bytes per record)
```

### File Types

#### Fixed-Size Columns (`fixed/`)

Store fixed-width data types like `u64`, `i64`, `f64`:
- Direct file access at computed offsets
- Zero-copy reads: `value_offset = index * value_size`
- 8 bytes per u64 value
- Memory-mapped for fast access

#### Variable-Length Columns (`variable/`)

Store variable-width data like strings:
- **Data file**: Append-only storage of actual string bytes
- **Offsets file**: Index of (offset, length) pairs
- Each offset entry is 16 bytes (8 for offset, 8 for length)
- Enables random access to variable-length data

#### Tombstone Bitmap (`tombstones.bin`)

Tracks deleted records without removing data:
- 1 byte per row (0 = active, 1 = deleted)
- Enables soft deletes
- Can be compacted later to reclaim space

#### Manifest (`manifest.json`)

JSON metadata file storing:
- Schema version
- Row count
- Column definitions (name, type, index)
- WAL configuration
- Last checkpoint information

Example manifest:
```json
{
  "schema_version": 1,
  "row_count": 3,
  "columns": [
    {
      "name": "id",
      "column_type": "U64",
      "column_index": 0
    },
    {
      "name": "email",
      "column_type": "String",
      "column_index": 0
    }
  ],
  "wal_enabled": true,
  "last_checkpoint": 1234567890
}
```

## Usage Examples

### Creating a Database

```rust
use forgedb_storage::{Database, ColumnMetadata, ColumnType};
use std::path::PathBuf;

// Open or create a database
let db_path = PathBuf::from("./my_database");
let mut db = Database::open(db_path)?;

// Define schema
let columns = vec![
    ColumnMetadata {
        name: "id".to_string(),
        column_type: ColumnType::U64,
        column_index: 0,
    },
    ColumnMetadata {
        name: "email".to_string(),
        column_type: ColumnType::String,
        column_index: 0,
    },
];

db.set_columns(columns);
db.save_manifest()?;
```

### Working with Fixed-Size Columns

```rust
use forgedb_storage::FixedColumn;
use std::path::PathBuf;

// Create a u64 column
let path = PathBuf::from("./data/fixed/user_ids.bin");
let mut col = FixedColumn::new(path, 8)?;

// Append values
col.append_u64(1001)?;
col.append_u64(1002)?;
col.append_u64(1003)?;

// Read values
let id = col.read_u64(0)?;  // Returns 1001
assert_eq!(id, 1001);

// Get row count
let count = col.len();  // Returns 3
```

### Working with Variable-Length Columns

```rust
use forgedb_storage::VariableColumn;
use std::path::PathBuf;

// Create a string column
let data_path = PathBuf::from("./data/variable/emails_data.bin");
let offsets_path = PathBuf::from("./data/variable/emails_offsets.bin");
let mut col = VariableColumn::new(data_path, offsets_path)?;

// Append strings
col.append_string("alice@example.com")?;
col.append_string("bob@example.com")?;
col.append_string("charlie@example.com")?;

// Read strings
let email = col.read_string(1)?;  // Returns "bob@example.com"
assert_eq!(email, "bob@example.com");

// Handle empty strings
col.append_string("")?;
assert_eq!(col.read_string(3)?, "");
```

### Using Tombstones for Soft Deletes

```rust
use forgedb_storage::Tombstones;
use std::path::PathBuf;

let path = PathBuf::from("./data/tombstones.bin");
let mut tombstones = Tombstones::new(path)?;

// Mark records as active or deleted
tombstones.append(false)?;  // Row 0: active
tombstones.append(true)?;   // Row 1: deleted
tombstones.append(false)?;  // Row 2: active

// Check deletion status
assert_eq!(tombstones.is_deleted(0)?, false);
assert_eq!(tombstones.is_deleted(1)?, true);
assert_eq!(tombstones.is_deleted(2)?, false);
```

### Transaction Support with WAL

```rust
use forgedb_storage::{Database, FsyncPolicy, Transaction, WalValue};
use std::path::PathBuf;

// Open database with WAL support
let db_path = PathBuf::from("./my_database");
let mut db = Database::open_with_wal(db_path, FsyncPolicy::Always)?;

// Begin a transaction
let mut txn = Transaction::begin();

// Create a WalEntry for insert operation
let user_id = uuid::Uuid::new_v4();
let mut fields = std::collections::HashMap::new();
fields.insert("id".to_string(), WalValue::U64(1001));
fields.insert(
    "email".to_string(),
    WalValue::String("user@example.com".to_string()),
);

let insert_entry = forgedb_storage::WalEntry::insert(
    "User".to_string(),
    user_id,
    fields,
);
txn.add_entry(insert_entry)?;

// Commit transaction to WAL
if let Some(wal) = db.wal_mut() {
    txn.commit(wal)?;  // This writes BEGIN, entries, and COMMIT markers
}
```

### WAL Recovery on Startup

```rust
use forgedb_storage::{Database, FsyncPolicy};
use std::path::PathBuf;

let db_path = PathBuf::from("./my_database");
let mut db = Database::open_with_wal(db_path, FsyncPolicy::Always)?;

// Replay WAL entries to recover state
if let Some(wal) = db.wal_mut() {
    let entries = wal.replay(|entry| {
        // Apply each WAL entry to rebuild state
        match entry.operation {
            WalOperation::Insert { .. } => {
                // Reconstruct insert operation
            }
            WalOperation::Update { .. } => {
                // Reconstruct update operation
            }
            WalOperation::Delete { .. } => {
                // Reconstruct delete operation
            }
            _ => {}
        }
        Ok(())
    })?;
    
    println!("Recovered {} WAL entries", entries.len());
}
```

## API Documentation

### Key Types

#### `Database`

Manages the database directory and manifest.

**Methods:**
- `open(path) -> Result<Database>` - Open or create a database
- `open_with_wal(path, fsync_policy) -> Result<Database>` - Open with WAL support
- `save_manifest() -> Result<()>` - Save manifest to disk
- `get_manifest() -> &Manifest` - Get current manifest
- `update_row_count(count)` - Update row count in manifest
- `set_columns(columns)` - Set column metadata
- `wal() -> Option<&WalManager>` - Get WAL manager reference
- `wal_mut() -> Option<&mut WalManager>` - Get mutable WAL manager reference
- `has_wal() -> bool` - Check if WAL is enabled

**Path helpers:**
- `fixed_column_path(index) -> PathBuf` - Get path for fixed column
- `variable_data_path(index) -> PathBuf` - Get path for variable data
- `variable_offsets_path(index) -> PathBuf` - Get path for variable offsets
- `tombstones_path() -> PathBuf` - Get path for tombstones

#### `Manifest`

Database metadata and schema information.

**Fields:**
- `schema_version: u32` - Schema version number
- `row_count: usize` - Total number of rows
- `columns: Vec<ColumnMetadata>` - Column definitions
- `wal_enabled: bool` - Whether WAL is enabled
- `last_checkpoint: u64` - Last checkpoint timestamp

#### `ColumnMetadata`

Metadata for a single column.

**Fields:**
- `name: String` - Column name
- `column_type: ColumnType` - Column data type
- `column_index: usize` - Column index within its type group

#### `ColumnType`

Supported column data types.

**Variants:**
- `U64` - 64-bit unsigned integer
- `String` - Variable-length UTF-8 string

#### `FixedColumn`

Storage for fixed-size columns.

**Methods:**
- `new(path, value_size) -> Result<FixedColumn>` - Create or open a fixed column
- `append_u64(value) -> Result<()>` - Append a u64 value
- `read_u64(index) -> Result<u64>` - Read value at index
- `len() -> usize` - Get number of values

#### `VariableColumn`

Storage for variable-length columns.

**Methods:**
- `new(data_path, offsets_path) -> Result<VariableColumn>` - Create or open a variable column
- `append_string(value) -> Result<()>` - Append a string value
- `read_string(index) -> Result<String>` - Read string at index
- `len() -> usize` - Get number of values

#### `Tombstones`

Deletion bitmap for tracking deleted records.

**Methods:**
- `new(path) -> Result<Tombstones>` - Create or open tombstones file
- `append(deleted) -> Result<()>` - Mark a new row as active/deleted
- `is_deleted(index) -> Result<bool>` - Check if a row is deleted
- `len() -> usize` - Get number of rows tracked

### WAL Types (Re-exported)

The storage crate re-exports WAL types from `forgedb-wal`:

- `WalManager` - High-level WAL interface
- `WalEntry` - A single WAL entry
- `WalOperation` - Operation type (Insert, Update, Delete, etc.)
- `WalValue` - Typed value in WAL entry
- `Transaction` - Transaction builder
- `TransactionId` - Unique transaction identifier
- `FsyncPolicy` - Fsync behavior configuration

## Performance Characteristics

### Memory Mapping Benefits

- **Zero-copy reads**: Data accessed directly from memory-mapped files
- **OS page cache**: Automatically cached by the operating system
- **Lazy loading**: Only accessed pages are loaded into memory
- **Reduced memory pressure**: OS manages memory more efficiently than user-space buffers

### Columnar Scan Performance

- **Column projection**: Read only the columns you need
- **Sequential I/O**: Column files can be read sequentially for better disk performance
- **Cache locality**: Similar data types together improve CPU cache hits
- **Compression-friendly**: Homogeneous data types compress better (future optimization)

### Write Performance

- **Append-only**: All writes are appends, no random writes
- **Batched fsyncs**: Configurable fsync policy for throughput vs. durability trade-off
- **WAL batching**: Group operations in transactions for better performance

**WAL Performance Modes:**
- `FsyncPolicy::Always`: Maximum durability, ~4s for 1,000 writes
- `FsyncPolicy::Never`: Maximum throughput, ~12ms for 1,000 writes (346x faster)
- `FsyncPolicy::Periodic(duration)`: Balanced approach

### Space Efficiency

- **Tombstone overhead**: 1 byte per row (minimal)
- **String overhead**: 16 bytes per string for offset/length
- **No padding**: Fixed-size columns are tightly packed
- **Compaction**: Can reclaim space from deleted records (separate process)

## File Layout Reference

### File Naming Conventions

```
<database_root>/
├── manifest.json              # Always present
├── tombstones.bin            # Always present (1 byte per row)
├── wal.log                   # Present if WAL is enabled
├── fixed/
│   ├── u64_0.bin            # First u64 column (8 bytes per row)
│   ├── u64_1.bin            # Second u64 column
│   └── u64_N.bin            # Nth u64 column
└── variable/
    ├── string_data_0.bin    # First string column data
    ├── string_offsets_0.bin # First string column offsets (16 bytes per row)
    ├── string_data_1.bin    # Second string column data
    ├── string_offsets_1.bin # Second string column offsets
    └── ...
```

### Column Index Rules

- Each column type has its own index sequence starting at 0
- Example: `id: +u64, age: u64, name: &string, email: &string`
  - `id` → `fixed/u64_0.bin`
  - `age` → `fixed/u64_1.bin`
  - `name` → `variable/string_data_0.bin`, `variable/string_offsets_0.bin`
  - `email` → `variable/string_data_1.bin`, `variable/string_offsets_1.bin`

### File Size Calculations

- **Fixed column**: `file_size = row_count * value_size`
- **Tombstones**: `file_size = row_count * 1`
- **String offsets**: `file_size = row_count * 16`
- **String data**: `file_size = sum(string_lengths)`

## Testing

### Running Tests

```bash
# Run all storage tests
cargo test -p forgedb-storage

# Run with output
cargo test -p forgedb-storage -- --nocapture

# Run specific test
cargo test -p forgedb-storage test_fixed_column_u64
```

### Test Coverage

The crate includes comprehensive tests for:
- ✅ Fixed column operations (append, read, bounds checking)
- ✅ Variable column operations (strings, empty strings, large strings)
- ✅ Tombstone operations (marking, checking deletion status)
- ✅ Database manifest (save, load, persistence)
- ✅ Persistence across reopens
- ✅ Error handling (out of bounds, invalid data)

### Example Test

```rust
#[test]
fn test_fixed_column_persistence() {
    let temp_dir = std::env::temp_dir().join("forgedb_test");
    let path = temp_dir.join("test_u64.bin");

    // Write data
    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(100).unwrap();
        col.append_u64(200).unwrap();
    }

    // Reopen and verify persistence
    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        assert_eq!(col.len(), 2);
        assert_eq!(col.read_u64(0).unwrap(), 100);
        assert_eq!(col.read_u64(1).unwrap(), 200);
    }
}
```

## Integration with ForgeDB

The storage crate is used by:
- **forgedb-crud-api**: High-level CRUD operations
- **forgedb-compaction**: Background compaction of tombstoned rows
- **forgedb-query-optimization**: Query planning and execution
- **Generated code**: User-defined models compile to storage operations

## Related Documentation

- [SPRINT2_PERSISTENCE.md](../../archive/sprint-summaries/SPRINT2_PERSISTENCE.md) - Original persistence implementation
- [SPRINT7_SUMMARY.md](../../archive/sprint-summaries/SPRINT7_SUMMARY.md) - Write-Ahead Log implementation
- [CRATE_ORGANIZATION_PLAN.md](../../CRATE_ORGANIZATION_PLAN.md) - Overall crate structure
- [forgedb-wal README](../wal/README.md) - Write-Ahead Log documentation (future)

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT License ([LICENSE-MIT](../../LICENSE-MIT))

at your option.

## Contributing

This is part of the ForgeDB project. See the main repository for contribution guidelines.
