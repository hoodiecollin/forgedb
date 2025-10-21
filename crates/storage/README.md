# forgedb-storage

High-performance columnar storage engine for ForgeDB with memory-mapped file access, WAL integration, and tombstone-based deletion tracking.

## Overview

`forgedb-storage` is the core storage engine for ForgeDB, implementing a columnar storage architecture optimized for both read and write performance. The crate provides:

- **Columnar storage format** - Fixed-size and variable-length columns stored separately for optimal access patterns
- **Memory-mapped file access** - Direct file I/O with plans for true memory mapping for zero-copy reads
- **WAL integration** - Write-Ahead Log support for ACID properties and crash recovery
- **Tombstone tracking** - Efficient soft-delete mechanism without data movement
- **Type-safe API** - Strong typing for columns and database operations

## Architecture

### Columnar Storage Format

The storage engine uses a columnar layout where each column is stored in separate files, optimized for the column's data type:

```
data/
├── manifest.json              # Database metadata (schema, row count, columns)
├── tombstones.bin            # Deletion bitmap (1 byte per row)
├── fixed/
│   └── u64_0.bin            # Fixed-size column (8 bytes per row)
└── variable/
    ├── string_data_0.bin    # Variable-length data (append-only)
    └── string_offsets_0.bin # (offset, length) pairs (16 bytes per row)
```

### Fixed-Size vs Variable-Length Columns

#### Fixed-Size Columns

Fixed-size columns (u64, i64, f64, uuid, etc.) are stored in files where each value occupies a fixed number of bytes:

- **Storage**: `fixed/u64_{index}.bin`
- **Layout**: Sequential values with no overhead
- **Access**: O(1) random access via `offset = index * value_size`
- **Overhead**: Zero bytes per value

Example: For u64 columns, each value takes exactly 8 bytes.

#### Variable-Length Columns

Variable-length columns (strings) use a two-file approach:

1. **Data file** (`variable/string_data_{index}.bin`): Append-only storage of actual string bytes
2. **Offsets file** (`variable/string_offsets_{index}.bin`): Pairs of (offset, length) as u64 values

- **Access**: O(1) random access (read offset pair, then read string from data file)
- **Overhead**: 16 bytes per string (8 bytes offset + 8 bytes length)

### Memory-Mapped Files

The storage engine uses direct file I/O with seek operations. Future optimizations will include true memory-mapped files for:

- Zero-copy reads
- Operating system-managed caching
- Reduced memory footprint
- SIMD-friendly sequential access patterns

### Tombstone Bitmap for Deletions

Instead of physically removing data, deletions are tracked using a tombstone bitmap:

- **Storage**: `tombstones.bin`
- **Format**: 1 byte per row (0 = active, 1 = deleted)
- **Benefits**:
  - No data movement required
  - Fast delete operations
  - Preserves row IDs
- **Compaction**: Background compaction can reclaim space (see `forgedb-compaction`)

## Usage Examples

### Creating a Database

```rust
use forgedb_storage::{Database, ColumnMetadata, ColumnType};
use std::path::PathBuf;

// Create or open a database
let mut db = Database::open(PathBuf::from("./mydb"))?;

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

### Reading and Writing Records

#### Writing to Fixed-Size Columns

```rust
use forgedb_storage::FixedColumn;

// Open a u64 column
let mut id_column = FixedColumn::new(db.fixed_column_path(0), 8)?;

// Append values
id_column.append_u64(1001)?;
id_column.append_u64(1002)?;
id_column.append_u64(1003)?;
```

#### Writing to Variable-Length Columns

```rust
use forgedb_storage::VariableColumn;

// Open a string column
let mut email_column = VariableColumn::new(
    db.variable_data_path(0),
    db.variable_offsets_path(0)
)?;

// Append strings
email_column.append_string("alice@example.com")?;
email_column.append_string("bob@example.com")?;
email_column.append_string("charlie@example.com")?;
```

#### Reading from Columns

```rust
// Read from fixed-size column
let id = id_column.read_u64(0)?;  // Returns 1001
assert_eq!(id, 1001);

// Read from variable-length column
let email = email_column.read_string(1)?;  // Returns "bob@example.com"
assert_eq!(email, "bob@example.com");
```

### Tombstone Tracking

```rust
use forgedb_storage::Tombstones;

// Open tombstone file
let mut tombstones = Tombstones::new(db.tombstones_path())?;

// Mark records as active (false) or deleted (true)
tombstones.append(false)?;  // Record 0: active
tombstones.append(false)?;  // Record 1: active
tombstones.append(true)?;   // Record 2: deleted

// Check deletion status
assert_eq!(tombstones.is_deleted(0)?, false);
assert_eq!(tombstones.is_deleted(1)?, false);
assert_eq!(tombstones.is_deleted(2)?, true);
```

### Transaction Support via WAL Integration

The storage engine integrates with `forgedb-wal` for ACID properties:

```rust
use forgedb_storage::{Database, FsyncPolicy};

// Open database with WAL enabled
let mut db = Database::open_with_wal(
    PathBuf::from("./mydb"),
    FsyncPolicy::EveryWrite
)?;

// Check if WAL is enabled
assert!(db.has_wal());

// Begin a transaction
if let Some(wal) = db.wal_mut() {
    // Use WAL for transactional writes
    // (See forgedb-wal documentation for transaction API)
}
```

### WAL Integration Example

```rust
use forgedb_storage::{
    Database, FsyncPolicy, Transaction, WalEntry, 
    WalOperation, WalValue
};

// Open database with WAL
let mut db = Database::open_with_wal(
    PathBuf::from("./mydb"),
    FsyncPolicy::EveryCommit
)?;

// Create a transaction
let mut txn = Transaction::new();

// Add operations to transaction
txn.insert("User", uuid::Uuid::new_v4(), vec![
    ("email".to_string(), WalValue::String("user@example.com".to_string()))
]);

// Commit transaction
if let Some(wal) = db.wal_mut() {
    txn.commit(wal)?;
}
```

## API Documentation

### Key Types

#### `Database`

The main entry point for storage operations. Manages the database directory and manifest.

**Methods:**
- `open(path: PathBuf) -> io::Result<Self>` - Open or create a database
- `open_with_wal(path: PathBuf, fsync_policy: FsyncPolicy) -> io::Result<Self>` - Open with WAL enabled
- `save_manifest() -> io::Result<()>` - Save manifest to disk
- `get_manifest() -> &Manifest` - Get current manifest
- `set_columns(columns: Vec<ColumnMetadata>)` - Update column definitions
- `update_row_count(count: usize)` - Update row count in manifest
- `wal() -> Option<&WalManager>` - Get immutable reference to WAL
- `wal_mut() -> Option<&mut WalManager>` - Get mutable reference to WAL
- `has_wal() -> bool` - Check if WAL is enabled
- `fixed_column_path(index: usize) -> PathBuf` - Get path for fixed-size column file
- `variable_data_path(index: usize) -> PathBuf` - Get path for variable-length data file
- `variable_offsets_path(index: usize) -> PathBuf` - Get path for variable-length offsets file
- `tombstones_path() -> PathBuf` - Get path for tombstone bitmap file

#### `Manifest`

Database metadata stored in `manifest.json`.

**Fields:**
- `schema_version: u32` - Schema version number
- `row_count: usize` - Total number of rows (including deleted)
- `columns: Vec<ColumnMetadata>` - Column definitions
- `wal_enabled: bool` - Whether WAL is enabled
- `last_checkpoint: u64` - Last WAL checkpoint position

#### `ColumnMetadata`

Column definition in the manifest.

**Fields:**
- `name: String` - Column name
- `column_type: ColumnType` - Column data type
- `column_index: usize` - Index within columns of the same type

#### `ColumnType`

Enumeration of supported column types.

**Variants:**
- `U64` - Unsigned 64-bit integer
- `String` - Variable-length UTF-8 string

#### `FixedColumn`

Storage for fixed-size column data.

**Methods:**
- `new(path: PathBuf, value_size: usize) -> io::Result<Self>` - Create or open column file
- `append_u64(value: u64) -> io::Result<()>` - Append a u64 value
- `read_u64(index: usize) -> io::Result<u64>` - Read value at index
- `len() -> usize` - Get number of values

#### `VariableColumn`

Storage for variable-length column data.

**Methods:**
- `new(data_path: PathBuf, offsets_path: PathBuf) -> io::Result<Self>` - Create or open column files
- `append_string(value: &str) -> io::Result<()>` - Append a string value
- `read_string(index: usize) -> io::Result<String>` - Read string at index
- `len() -> usize` - Get number of strings

#### `Tombstones`

Deletion tracking bitmap.

**Methods:**
- `new(path: PathBuf) -> io::Result<Self>` - Create or open tombstone file
- `append(deleted: bool) -> io::Result<()>` - Mark new row as active/deleted
- `is_deleted(index: usize) -> io::Result<bool>` - Check if row is deleted
- `len() -> usize` - Get number of rows tracked

### WAL Re-exports

The following types are re-exported from `forgedb-wal` for convenience:

- `FsyncPolicy` - Controls when WAL is fsynced to disk
- `Transaction` - Groups WAL operations with atomic commit
- `TransactionId` - Unique identifier for transactions
- `WalEntry` - Single entry in the WAL
- `WalManager` - High-level WAL interface
- `WalOperation` - Type of operation (insert, update, delete)
- `WalValue` - Value types supported in WAL

## Performance Characteristics

### Memory Mapping Benefits

The columnar storage format is designed to take advantage of memory-mapped files:

- **Zero-copy reads**: Direct access to file contents without copying to memory
- **OS-managed caching**: Operating system handles page cache efficiently
- **Lazy loading**: Only accessed pages are loaded into memory
- **SIMD operations**: Sequential column layout enables vectorized operations

### Columnar Scan Performance

Scanning columns is highly efficient due to:

- **Sequential access**: Each column is stored contiguously
- **Cache-friendly**: Modern CPUs prefetch sequential data effectively
- **Selective reads**: Only read columns needed for query
- **Vectorization**: SIMD operations on fixed-size numeric columns

**Performance Goals:**
- Random access: < 1μs per value
- Full table scan (1M rows): < 100ms
- Insert throughput: > 100K rows/sec

### Write Performance

- **Fixed columns**: O(1) append (single seek + write)
- **Variable columns**: O(1) append (data write + offset write)
- **Tombstones**: O(1) append (single byte write)
- **Manifest**: Small JSON file, negligible overhead

### Read Performance

- **Fixed columns**: O(1) random access (single seek + read)
- **Variable columns**: O(1) random access (two seeks: offset, then data)
- **Full scan**: O(n) sequential reads per column

### Space Efficiency

- **Fixed columns**: Zero overhead (exact type size per value)
- **Variable columns**: 16 bytes overhead per value (offset + length)
- **Tombstones**: 1 byte per row
- **Manifest**: ~100-500 bytes (JSON metadata)

## File Layout

### Directory Structure

```
<database_root>/
├── manifest.json              # Database metadata
├── tombstones.bin            # Row deletion bitmap
├── wal.log                   # Write-Ahead Log (if WAL enabled)
├── fixed/                    # Fixed-size columns directory
│   ├── u64_0.bin            # First u64 column
│   ├── u64_1.bin            # Second u64 column
│   └── ...                  # Additional fixed-size columns
└── variable/                 # Variable-length columns directory
    ├── string_data_0.bin    # First string column data
    ├── string_offsets_0.bin # First string column offsets
    ├── string_data_1.bin    # Second string column data
    ├── string_offsets_1.bin # Second string column offsets
    └── ...                  # Additional variable-length columns
```

### File Naming Conventions

#### Fixed-Size Columns

Pattern: `fixed/{type}_{index}.bin`

- `type`: Data type (e.g., `u64`, `i64`, `f64`)
- `index`: Zero-based index among columns of the same type

Examples:
- `fixed/u64_0.bin` - First u64 column
- `fixed/u64_1.bin` - Second u64 column

#### Variable-Length Columns

Pattern: `variable/{type}_data_{index}.bin` and `variable/{type}_offsets_{index}.bin`

- `type`: Data type (currently only `string`)
- `index`: Zero-based index among columns of the same type

Examples:
- `variable/string_data_0.bin` - First string column data
- `variable/string_offsets_0.bin` - First string column offsets

#### Manifest

- **File**: `manifest.json`
- **Format**: JSON
- **Content**: Schema version, row count, column metadata, WAL settings

Example `manifest.json`:
```json
{
  "schema_version": 1,
  "row_count": 42,
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
  "last_checkpoint": 0
}
```

#### Tombstones

- **File**: `tombstones.bin`
- **Format**: Binary bitmap (1 byte per row)
- **Values**: `0x00` = active, `0x01` = deleted

#### Write-Ahead Log

- **File**: `wal.log` (when WAL is enabled)
- **Format**: Binary log entries with checksums
- **See**: `forgedb-wal` crate documentation for details

## Testing

### Running Tests

Run all storage tests:

```bash
cargo test --package forgedb-storage
```

Run specific test:

```bash
cargo test --package forgedb-storage test_fixed_column_u64
```

Run with output:

```bash
cargo test --package forgedb-storage -- --nocapture
```

### Test Coverage

The test suite includes:

- ✅ Fixed column operations (append, read, persistence)
- ✅ Variable column operations (append, read, persistence, empty/large strings)
- ✅ Tombstone tracking (append, check, persistence)
- ✅ Database manifest (save, load, column definitions)
- ✅ Out-of-bounds access error handling
- ✅ Data persistence across database reopens

### Example Test

```rust
use forgedb_storage::FixedColumn;
use std::path::PathBuf;

#[test]
fn test_fixed_column_u64() {
    let path = PathBuf::from("/tmp/test_u64.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();
    
    col.append_u64(42).unwrap();
    col.append_u64(100).unwrap();
    
    assert_eq!(col.read_u64(0).unwrap(), 42);
    assert_eq!(col.read_u64(1).unwrap(), 100);
    assert_eq!(col.len(), 2);
}
```

## Related Crates

- **[forgedb-wal](../wal)**: Write-Ahead Log for ACID properties and crash recovery
- **[forgedb-compaction](../compaction)**: Background compaction for reclaiming tombstone space
- **[forgedb-query-optimization](../query-optimization)**: SIMD-optimized query execution over columnar data
- **[forgedb-crud-api](../crud-api)**: High-level CRUD operations built on storage primitives

## Links to Documentation

- [SPRINT2_PERSISTENCE.md](../../archive/sprint-summaries/SPRINT2_PERSISTENCE.md) - Original storage implementation
- [SPRINT7_SUMMARY.md](../../archive/sprint-summaries/SPRINT7_SUMMARY.md) - WAL integration
- [CRATE_ORGANIZATION_PLAN.md](../../CRATE_ORGANIZATION_PLAN.md) - Overall crate organization

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
