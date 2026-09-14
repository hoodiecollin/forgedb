ForgeDB Storage Engine

High-performance columnar storage engine for ForgeDB with positional I/O reads,
WAL integration, and tombstone-based deletion tracking.

# Overview

`forgedb-storage` is the core storage engine for ForgeDB, implementing a columnar storage
architecture optimized for both read and write performance. The crate provides:

- **Columnar storage format** - Fixed-size and variable-length columns stored separately
- **Positional I/O reads** - Concurrent-safe reads via `pread(2)`-style positional I/O
  (`FileExt::read_exact_at`) that avoids touching the shared file cursor; multiple `&self`
  readers on the same column can execute concurrently without synchronisation
- **Seek-based writes** - Append methods seek to end-of-file before writing, taking `&mut self`
  for exclusive ownership during the seek + write sequence
- **WAL integration** - Write-Ahead Log support for ACID properties and crash recovery
- **Tombstone tracking** - Efficient soft-delete mechanism without data movement
- **Type-safe API** - Strong typing for columns and database operations

# Architecture

## Columnar Storage Format

The storage engine uses a columnar layout where each column is stored in separate files:

```text
data/
├── manifest.json              # Database metadata
├── tombstones.bin            # Deletion bitmap
├── fixed/
│   └── u64_0.bin            # Fixed-size column (8 bytes per row)
└── variable/
    ├── string_data_0.bin    # Variable-length data
    └── string_offsets_0.bin # (offset, length) pairs
```

## Fixed-Size vs Variable-Length Columns

**Fixed-Size Columns** (u64, i64, f64, uuid):
- Storage: `fixed/{type}_{index}.bin`
- Layout: Sequential values with no overhead
- Access: O(1) random access via `offset = index * value_size`

**Variable-Length Columns** (strings):
- Data file: `variable/string_data_{index}.bin` (append-only)
- Offsets file: `variable/string_offsets_{index}.bin` (offset, length pairs)
- Access: O(1) random access (read offset pair, then read string)

## Tombstone Bitmap

Deletions are tracked using a tombstone bitmap:
- Storage: `tombstones.bin` (1 byte per row)
- Format: 0 = active, 1 = deleted
- Benefits: No data movement, fast deletes, preserves row IDs

# Examples

## Describing a model's physical layout (`Manifest`)

Generated code owns the directory layout and drives the column types directly;
the schema-blind [`Manifest`] records the physical layout (read by backup /
the inspector), persisted next to the column files.

```rust,no_run
use forgedb_storage_native::{Manifest, ColumnMetadata, ColumnType};
use std::path::PathBuf;

let manifest = Manifest {
    schema_version: 1,
    engine_version: 1,
    row_count: 42,
    columns: vec![
        ColumnMetadata { name: "id".to_string(), column_type: ColumnType::U64, column_index: 0, ..Default::default() },
        ColumnMetadata { name: "email".to_string(), column_type: ColumnType::String, column_index: 1, ..Default::default() },
    ],
    wal_enabled: false,
    last_checkpoint: 0,
    compaction_epoch: 0,
    row_anchor: None,
    auto_sequences: Default::default(),
};
manifest.save_to(&PathBuf::from("./mydb/manifest.json"))?;
let reopened = Manifest::load_from(&PathBuf::from("./mydb/manifest.json"))?;
assert_eq!(reopened.row_count, 42);
# Ok::<(), std::io::Error>(())
```

## Working with Columns

```rust,no_run
use forgedb_storage_native::FixedColumn;
use std::path::PathBuf;

// Fixed-size column
let mut id_column = FixedColumn::new(PathBuf::from("./data/fixed/u64_0.bin"), 8)?;
id_column.append_u64(1001)?;
let id = id_column.read_u64(0)?;  // &self — concurrent reads are safe
assert_eq!(id, 1001);
id_column.flush()?;  // explicit fsync before a checkpoint or close
# Ok::<(), std::io::Error>(())
```

# Durability Model

Column files (`FixedColumn`, `VariableColumn`, `Tombstones`) **do not fsync on every append**.
Appended bytes are visible to subsequent reads via the OS page cache, but a crash before an
explicit [`FixedColumn::flush`] / [`VariableColumn::flush`] / [`Tombstones::flush`] call can
lose the most recent appends from those files.

The **WAL is the crash-durability boundary**: all mutations should be recorded in the WAL
(via [`WalManager`]) before being applied to the column files. On recovery, the WAL is replayed
into the columnar materialization. Call `flush()` at commit / checkpoint boundaries to durably
persist the column files.

# Public API

## Core Types

- [`Manifest`] - Physical-layout metadata stored in manifest.json
- [`FixedColumn`] - Storage for fixed-size column data
- [`VariableColumn`] - Storage for variable-length column data
- [`Tombstones`] - Deletion tracking bitmap

## WAL Re-exports

Types from [`forgedb-wal`](../forgedb_wal) for convenience:
- [`FsyncPolicy`] - Controls when WAL is fsynced to disk
- [`WalEntry`] - A framed opaque-bytes entry (model tag + `Raw` payload)
- [`WalManager`] - High-level WAL interface
- [`WalOperation`] - The `Raw { payload }` operation variant

# Related Crates

- [`forgedb-wal`](../forgedb_wal) - Write-Ahead Log for durability
- [`forgedb-compaction`](../forgedb_compaction) - Background compaction for space reclamation
