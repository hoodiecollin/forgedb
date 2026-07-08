//! ForgeDB Storage Engine
//!
//! High-performance columnar storage engine for ForgeDB with positional I/O reads,
//! WAL integration, and tombstone-based deletion tracking.
//!
//! # Overview
//!
//! `forgedb-storage` is the core storage engine for ForgeDB, implementing a columnar storage
//! architecture optimized for both read and write performance. The crate provides:
//!
//! - **Columnar storage format** - Fixed-size and variable-length columns stored separately
//! - **Positional I/O reads** - Concurrent-safe reads via `pread(2)`-style positional I/O
//!   (`FileExt::read_exact_at`) that avoids touching the shared file cursor; multiple `&self`
//!   readers on the same column can execute concurrently without synchronisation
//! - **Seek-based writes** - Append methods seek to end-of-file before writing, taking `&mut self`
//!   for exclusive ownership during the seek + write sequence
//! - **WAL integration** - Write-Ahead Log support for ACID properties and crash recovery
//! - **Tombstone tracking** - Efficient soft-delete mechanism without data movement
//! - **Type-safe API** - Strong typing for columns and database operations
//!
//! # Architecture
//!
//! ## Columnar Storage Format
//!
//! The storage engine uses a columnar layout where each column is stored in separate files:
//!
//! ```text
//! data/
//! ├── manifest.json              # Database metadata
//! ├── tombstones.bin            # Deletion bitmap
//! ├── fixed/
//! │   └── u64_0.bin            # Fixed-size column (8 bytes per row)
//! └── variable/
//!     ├── string_data_0.bin    # Variable-length data
//!     └── string_offsets_0.bin # (offset, length) pairs
//! ```
//!
//! ## Fixed-Size vs Variable-Length Columns
//!
//! **Fixed-Size Columns** (u64, i64, f64, uuid):
//! - Storage: `fixed/{type}_{index}.bin`
//! - Layout: Sequential values with no overhead
//! - Access: O(1) random access via `offset = index * value_size`
//!
//! **Variable-Length Columns** (strings):
//! - Data file: `variable/string_data_{index}.bin` (append-only)
//! - Offsets file: `variable/string_offsets_{index}.bin` (offset, length pairs)
//! - Access: O(1) random access (read offset pair, then read string)
//!
//! ## Tombstone Bitmap
//!
//! Deletions are tracked using a tombstone bitmap:
//! - Storage: `tombstones.bin` (1 byte per row)
//! - Format: 0 = active, 1 = deleted
//! - Benefits: No data movement, fast deletes, preserves row IDs
//!
//! # Examples
//!
//! ## Creating a Database
//!
//! ```rust,no_run
//! use forgedb_storage::{Database, ColumnMetadata, ColumnType};
//! use std::path::PathBuf;
//!
//! // Create or open a database
//! let mut db = Database::open(PathBuf::from("./mydb"))?;
//!
//! // Define schema
//! let columns = vec![
//!     ColumnMetadata {
//!         name: "id".to_string(),
//!         column_type: ColumnType::U64,
//!         column_index: 0,
//!         ..Default::default()
//!     },
//!     ColumnMetadata {
//!         name: "email".to_string(),
//!         column_type: ColumnType::String,
//!         column_index: 0,
//!         ..Default::default()
//!     },
//! ];
//!
//! db.set_columns(columns);
//! db.save_manifest()?;
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! ## Working with Columns
//!
//! ```rust,no_run
//! use forgedb_storage::FixedColumn;
//! use std::path::PathBuf;
//!
//! // Fixed-size column
//! let mut id_column = FixedColumn::new(PathBuf::from("./data/fixed/u64_0.bin"), 8)?;
//! id_column.append_u64(1001)?;
//! let id = id_column.read_u64(0)?;  // &self — concurrent reads are safe
//! assert_eq!(id, 1001);
//! id_column.flush()?;  // explicit fsync before a checkpoint or close
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! ## Transaction Support via WAL
//!
//! ```rust,no_run
//! use forgedb_storage::{Database, FsyncPolicy};
//! use std::path::PathBuf;
//!
//! // Open database with WAL enabled
//! let mut db = Database::open_with_wal(
//!     PathBuf::from("./mydb"),
//!     FsyncPolicy::Always
//! )?;
//!
//! // Check if WAL is enabled
//! assert!(db.has_wal());
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! # Durability Model
//!
//! Column files (`FixedColumn`, `VariableColumn`, `Tombstones`) **do not fsync on every append**.
//! Appended bytes are visible to subsequent reads via the OS page cache, but a crash before an
//! explicit [`FixedColumn::flush`] / [`VariableColumn::flush`] / [`Tombstones::flush`] call can
//! lose the most recent appends from those files.
//!
//! The **WAL is the crash-durability boundary**: all mutations should be recorded in the WAL
//! (via [`WalManager`]) before being applied to the column files. On recovery, the WAL is replayed
//! into the columnar materialization. Call `flush()` at commit / checkpoint boundaries (e.g. at the
//! end of a transaction, before advancing the WAL checkpoint) to durably persist the column files.
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`Database`] - Main entry point for storage operations
//! - [`Manifest`] - Database metadata stored in manifest.json
//! - [`FixedColumn`] - Storage for fixed-size column data
//! - [`VariableColumn`] - Storage for variable-length column data
//! - [`Tombstones`] - Deletion tracking bitmap
//!
//! ## WAL Re-exports
//!
//! Types from [`forgedb-wal`](../forgedb_wal) for convenience:
//! - [`FsyncPolicy`] - Controls when WAL is fsynced to disk
//! - [`Transaction`] - Groups WAL operations with atomic commit
//! - [`WalManager`] - High-level WAL interface
//!
//! # Related Crates
//!
//! - [`forgedb-wal`](../forgedb_wal) - Write-Ahead Log for ACID properties
//! - [`forgedb-compaction`](../forgedb_compaction) - Background compaction for space reclamation
//! - [`forgedb-query-optimization`](../forgedb_query_optimization) - SIMD-optimized query execution
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation and examples
//! - [SPRINT2_PERSISTENCE.md](../../archive/sprint-summaries/SPRINT2_PERSISTENCE.md) - Original implementation
//! - [SPRINT7_SUMMARY.md](../../archive/sprint-summaries/SPRINT7_SUMMARY.md) - WAL integration

// Sprint 7: WAL re-exports
pub use forgedb_wal::{
    FsyncPolicy, Transaction, TransactionId, WalEntry, WalManager, WalOperation, WalValue,
};

use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

/// Default `format_version` for manifests written before the field existed.
/// Old on-disk manifests deserialize as v1 (the original layout), not v0.
fn default_format_version() -> u32 {
    1
}

/// Manifest stores metadata about the database
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub row_count: usize,
    pub columns: Vec<ColumnMetadata>,
    #[serde(default)]
    pub wal_enabled: bool,
    #[serde(default)]
    pub last_checkpoint: u64,
    /// Generation counter bumped on every compaction. A byte-watermark
    /// incremental backup (#57) is valid only within one epoch — compaction
    /// rewrites files and shifts offsets, so crossing an epoch forces a fresh
    /// full backup. Additive (`#[serde(default)]`) for on-disk back-compat.
    #[serde(default)]
    pub compaction_epoch: u64,
    /// On-disk layout format version, so a schema-blind reader (backup #57,
    /// inspector #63) can refuse mismatched bytes instead of misreading them.
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// Which file's length authoritatively counts committed rows, and how many
    /// bytes it spends per row. For a model this is `tombstones.bin` (1 byte/row,
    /// appended last per insert); for an M2M junction it is `fixed/right.bin`
    /// (16 bytes/row, appended last per link). A schema-blind reader derives the
    /// committed row count as `len(anchor) / bytes_per_row` — the live watermark,
    /// independent of the (possibly stale) `row_count` field above. `None` on
    /// legacy manifests ⇒ fall back to `tombstones.bin`.
    #[serde(default)]
    pub row_anchor: Option<RowAnchor>,
}

/// Physical descriptor of the file whose length counts committed rows.
/// Layout fact only — names a file and a per-row byte stride, never a schema
/// semantic. See [`Manifest::row_anchor`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RowAnchor {
    /// Anchor file path relative to the model/junction directory.
    pub relative_path: String,
    /// Bytes the anchor file spends per committed row (`1` for tombstones,
    /// `16` for a junction's uuid `right` column).
    pub bytes_per_row: usize,
}

impl Manifest {
    /// Load a manifest from an explicit path (e.g. a per-model
    /// `<model>/manifest.json`), without opening a full [`Database`]. Used by
    /// schema-blind ops tooling — `forgedb-backup` (#57), the inspector (#63) —
    /// that reads layout metadata for one model directory.
    pub fn load_from(path: &std::path::Path) -> io::Result<Manifest> {
        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Atomically write this manifest to `path` via temp-file + `fsync` +
    /// `rename`. A crash mid-write leaves the intact previous manifest rather
    /// than a truncated/garbage one that would fail to parse on the next open.
    /// The temp file is `<path>.tmp`.
    pub fn save_to(&self, path: &std::path::Path) -> io::Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut tmp_path = path.as_os_str().to_owned();
        tmp_path.push(".tmp");
        let tmp_path = PathBuf::from(tmp_path);
        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            tmp.write_all(content.as_bytes())?;
            tmp.sync_all()?;
        }
        fs::rename(&tmp_path, path)?;
        Ok(())
    }
}

/// Whether a column is fixed-width (positional I/O) or variable-length
/// (offset-indexed data file). Physical-layout fact only — never a schema
/// semantic. Lets a schema-blind reader bound each file without guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ColumnKind {
    #[default]
    Fixed,
    Variable,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub column_type: ColumnType,
    pub column_index: usize,
    /// Bytes per row for a fixed column; `0` for a variable column (its width
    /// lives in the offsets file). Lets backup compute a fixed column's exact
    /// committed length as `row_count * value_size` without guessing.
    #[serde(default)]
    pub value_size: usize,
    /// Fixed vs. variable — selects how a reader bounds the column's files.
    #[serde(default)]
    pub kind: ColumnKind,
    /// Column file path relative to the model directory (e.g.
    /// `fixed/u32_0.bin` or `variable/string_data_1.bin`). For a variable
    /// column this is the data file; the offsets file is the same path with
    /// `_data.bin` → `_offsets.bin` (the storage-layout convention `stats.rs`
    /// already relies on).
    #[serde(default)]
    pub relative_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColumnType {
    #[default]
    U32,
    U64,
    I32,
    I64,
    F64,
    Bool,
    Uuid,
    Timestamp,
    String,
    /// Fixed-size byte array (for char(N), structs, fixed arrays)
    FixedBytes(usize),
}

/// A read snapshot: an immutable row-count watermark captured at a point in time.
///
/// This is the substrate primitive behind lock-free snapshot-isolated reads
/// (see the `mvcc-concurrency` proposal, Direction A). Because the storage
/// engine is **append-only** — rows are only ever appended, never moved or
/// mutated in place — a row's *position* is stable for its whole lifetime.
/// So a single integer, the row count at capture time, fully defines a
/// consistent view: exactly the rows whose index is below the watermark were
/// committed when the snapshot was taken, and every one of them is still at the
/// same index now.
///
/// A `Snapshot` carries **no** per-row version metadata and no reference to any
/// column data — it is a bare `usize`. Readers pass it to `*_at` accessors,
/// which clamp their scan to `visible(index)`. Rows appended after capture have
/// `index >= watermark` and are therefore invisible, so concurrent writers
/// cannot make a reader observe a torn or partial row.
///
/// It is schema-agnostic substrate: it knows nothing about any `.forge` model,
/// only about row positions. Generated code composes per-model watermarks into
/// its own snapshot struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    watermark: usize,
}

impl Snapshot {
    /// Capture a snapshot at the given committed row count.
    pub fn new(row_count: usize) -> Self {
        Self {
            watermark: row_count,
        }
    }

    /// The captured row count. Rows at index `0..watermark` are visible.
    pub fn watermark(&self) -> usize {
        self.watermark
    }

    /// Whether the row at `index` was committed as of this snapshot.
    pub fn visible(&self, index: usize) -> bool {
        index < self.watermark
    }
}

/// Fixed-size column storage backed by a seek-based file.
///
/// # Durability
///
/// Append methods write to the OS page cache but do **not** call `fsync` per append.
/// Data written by `append_*` is immediately readable (via `read_*`) on the same or
/// any other file descriptor to the same file, but a crash before [`flush`](FixedColumn::flush)
/// may lose the most recent appends. Call `flush()` at commit boundaries or before
/// advancing the WAL checkpoint.
pub struct FixedColumn {
    file: File,
    row_count: usize,
    value_size: usize, // bytes per value
}

impl FixedColumn {
    pub fn new(path: PathBuf, value_size: usize) -> io::Result<Self> {
        // A zero value_size would divide-by-zero when deriving row_count below,
        // and makes every offset computation meaningless.
        if value_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "FixedColumn value_size must be non-zero",
            ));
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let row_count = file.metadata()?.len() as usize / value_size;

        Ok(FixedColumn {
            file,
            row_count,
            value_size,
        })
    }

    pub fn append_u32(&mut self, value: u32) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value.to_le_bytes())?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_u32(&self, index: usize) -> io::Result<u32> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 4];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn append_u64(&mut self, value: u64) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value.to_le_bytes())?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_u64(&self, index: usize) -> io::Result<u64> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 8];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn append_i32(&mut self, value: i32) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value.to_le_bytes())?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_i32(&self, index: usize) -> io::Result<i32> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 4];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(i32::from_le_bytes(buf))
    }

    pub fn append_i64(&mut self, value: i64) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value.to_le_bytes())?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_i64(&self, index: usize) -> io::Result<i64> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 8];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(i64::from_le_bytes(buf))
    }

    pub fn append_f64(&mut self, value: f64) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value.to_le_bytes())?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_f64(&self, index: usize) -> io::Result<f64> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 8];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(f64::from_le_bytes(buf))
    }

    pub fn append_bool(&mut self, value: bool) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&[if value { 1u8 } else { 0u8 }])?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_bool(&self, index: usize) -> io::Result<bool> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 1];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(buf[0] != 0)
    }

    pub fn append_uuid(&mut self, value: [u8; 16]) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value)?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_uuid(&self, index: usize) -> io::Result<[u8; 16]> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 16];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(buf)
    }

    pub fn append_timestamp(&mut self, value: i64) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&value.to_le_bytes())?;
        self.row_count += 1;
        Ok(())
    }

    pub fn read_timestamp(&self, index: usize) -> io::Result<i64> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = [0u8; 8];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(i64::from_le_bytes(buf))
    }

    /// Append arbitrary bytes for complex fixed-size types (char arrays, structs, fixed arrays)
    pub fn append_bytes(&mut self, value: &[u8]) -> io::Result<()> {
        if value.len() != self.value_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Expected {} bytes, got {}", self.value_size, value.len()),
            ));
        }

        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(value)?;
        self.row_count += 1;
        Ok(())
    }

    /// Read arbitrary bytes for complex fixed-size types (char arrays, structs, fixed arrays)
    pub fn read_bytes(&self, index: usize) -> io::Result<Vec<u8>> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let offset = (index * self.value_size) as u64;
        let mut buf = vec![0u8; self.value_size];
        self.file.read_exact_at(&mut buf, offset)?;
        Ok(buf)
    }

    /// Flush all pending writes to disk (fsync).
    ///
    /// This is the explicit durability checkpoint: after `flush()` returns `Ok(())`,
    /// all previous appends are guaranteed to survive a crash. Call this at transaction
    /// commit boundaries or before advancing the WAL checkpoint.
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.row_count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Open a read-only, positionally-reading view over this column's file
    /// (#56 Direction B).  The returned [`FixedColumnReader`] shares the file
    /// via an independent (`try_clone`d) descriptor, so a single `&mut self`
    /// writer can keep appending while many `&self` readers read concurrently
    /// without a lock.  See [`FixedColumnReader`] for the full model.
    pub fn reader(&self) -> io::Result<FixedColumnReader> {
        Ok(FixedColumnReader {
            file: self.file.try_clone()?,
            value_size: self.value_size,
        })
    }
}

/// Variable-length column storage (for strings).
///
/// Stores data in an append-only data file plus a separate offset index file.
///
/// # Durability
///
/// Append methods write to the OS page cache but do **not** call `fsync` per append.
/// Call [`flush`](VariableColumn::flush) at commit boundaries to guarantee durability.
pub struct VariableColumn {
    data_file: File,
    offsets_file: File,
    row_count: usize,
    current_data_offset: u64,
}

impl VariableColumn {
    pub fn new(data_path: PathBuf, offsets_path: PathBuf) -> io::Result<Self> {
        // Create parent directories
        if let Some(parent) = data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&data_path)?;

        let offsets_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&offsets_path)?;

        let current_data_offset = data_file.metadata()?.len();
        let row_count = offsets_file.metadata()?.len() as usize / 16; // 16 bytes per (offset, length)

        Ok(VariableColumn {
            data_file,
            offsets_file,
            row_count,
            current_data_offset,
        })
    }

    pub fn append_string(&mut self, value: &str) -> io::Result<()> {
        let bytes = value.as_bytes();
        let length = bytes.len() as u64;

        // Write data to data file
        self.data_file.seek(SeekFrom::End(0))?;
        self.data_file.write_all(bytes)?;

        // Write (offset, length) to offsets file
        self.offsets_file.seek(SeekFrom::End(0))?;
        self.offsets_file
            .write_all(&self.current_data_offset.to_le_bytes())?;
        self.offsets_file.write_all(&length.to_le_bytes())?;

        self.current_data_offset += length;
        self.row_count += 1;

        Ok(())
    }

    pub fn read_string(&self, index: usize) -> io::Result<String> {
        if index >= self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        // Read offset and length from offsets file using positional I/O
        let offsets_pos = (index * 16) as u64;
        let mut offset_buf = [0u8; 8];
        let mut length_buf = [0u8; 8];
        self.offsets_file.read_exact_at(&mut offset_buf, offsets_pos)?;
        self.offsets_file
            .read_exact_at(&mut length_buf, offsets_pos + 8)?;

        let offset = u64::from_le_bytes(offset_buf);
        let length = u64::from_le_bytes(length_buf);

        // Read data from data file using positional I/O
        let mut data = vec![0u8; length as usize];
        self.data_file.read_exact_at(&mut data, offset)?;

        String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Flush all pending writes to disk (fsync both data and offsets files).
    ///
    /// After `flush()` returns `Ok(())`, all previous appends are guaranteed to
    /// survive a crash. Call at commit boundaries or before advancing the WAL checkpoint.
    pub fn flush(&mut self) -> io::Result<()> {
        self.data_file.sync_all()?;
        self.offsets_file.sync_all()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.row_count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Open a read-only, positionally-reading view over this column's data +
    /// offsets files (#56 Direction B).  The returned [`VariableColumnReader`]
    /// shares both files via independent (`try_clone`d) descriptors, so a single
    /// `&mut self` writer can keep appending while many `&self` readers read
    /// concurrently without a lock.  See [`FixedColumnReader`] for the model.
    pub fn reader(&self) -> io::Result<VariableColumnReader> {
        Ok(VariableColumnReader {
            data_file: self.data_file.try_clone()?,
            offsets_file: self.offsets_file.try_clone()?,
        })
    }
}

/// Tombstone bitmap for tracking deleted records.
///
/// # Durability
///
/// Append methods write to the OS page cache but do **not** call `fsync` per append.
/// Call [`flush`](Tombstones::flush) at commit boundaries to guarantee durability.
pub struct Tombstones {
    file: File,
    count: usize,
}

impl Tombstones {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let count = file.metadata()?.len() as usize;

        Ok(Tombstones { file, count })
    }

    pub fn append(&mut self, deleted: bool) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&[if deleted { 1u8 } else { 0u8 }])?;
        self.count += 1;
        Ok(())
    }

    pub fn is_deleted(&self, index: usize) -> io::Result<bool> {
        if index >= self.count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index out of bounds",
            ));
        }

        let mut buf = [0u8; 1];
        self.file.read_exact_at(&mut buf, index as u64)?;
        Ok(buf[0] != 0)
    }

    /// Flush all pending writes to disk (fsync).
    ///
    /// After `flush()` returns `Ok(())`, all previous appends are guaranteed to
    /// survive a crash. Call at commit boundaries or before advancing the WAL checkpoint.
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Open a read-only, positionally-reading view over this tombstone file
    /// (#56 Direction B).  See [`FixedColumn::reader`] for the concurrency model.
    pub fn reader(&self) -> io::Result<TombstonesReader> {
        Ok(TombstonesReader {
            file: self.file.try_clone()?,
        })
    }
}

/// Map a positional read that ran past end-of-file back to the same
/// out-of-bounds error the cached-count writer columns return (#56 Direction B).
///
/// A [`FixedColumnReader`] / [`VariableColumnReader`] / [`TombstonesReader`]
/// derives length live rather than caching a row count, so an index beyond the
/// committed prefix surfaces as `UnexpectedEof` from `read_exact_at`; normalize
/// it to `InvalidInput` for parity with the writer's explicit bound check.
fn map_out_of_bounds(e: io::Error) -> io::Error {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        io::Error::new(io::ErrorKind::InvalidInput, "Index out of bounds")
    } else {
        e
    }
}

/// Read-only, positional view over a [`FixedColumn`]'s backing file
/// (#56 Direction B — single writer, many concurrent readers).
///
/// Created by [`FixedColumn::reader`]; holds an independent file descriptor
/// (`try_clone`) to the **same** file as the writer.  A single `&mut self`
/// writer can keep appending while any number of `&self` readers positionally
/// read the committed prefix concurrently, with no lock:
///
/// - Reads use `read_exact_at` (positional `pread`) — they never touch a shared
///   file cursor, so they are safe against a concurrent seek-based append and
///   against each other across threads.
/// - The engine is append-only: bytes at an already-committed offset never move,
///   so a reader observing an offset below the writer's committed length reads
///   stable bytes (POSIX page-cache coherence makes the writer's not-yet-fsync'd
///   appends visible through this independent fd).
/// - Length is derived **live** on every access (never a cached count), so a
///   reader created before an append still sees rows the writer commits
///   afterward — once the caller's watermark admits them.
///
/// Callers clamp reads to a captured watermark (the row-count anchor); because
/// the anchor column is appended last per row, a row within the watermark has
/// all its columns fully written, so a reader never observes a torn row.
pub struct FixedColumnReader {
    file: File,
    value_size: usize,
}

impl FixedColumnReader {
    #[must_use]
    pub fn len(&self) -> usize {
        self.file
            .metadata()
            .map(|m| m.len() as usize / self.value_size)
            .unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read_u32(&self, index: usize) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn read_u64(&self, index: usize) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(u64::from_le_bytes(buf))
    }

    pub fn read_i32(&self, index: usize) -> io::Result<i32> {
        let mut buf = [0u8; 4];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(i32::from_le_bytes(buf))
    }

    pub fn read_i64(&self, index: usize) -> io::Result<i64> {
        let mut buf = [0u8; 8];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(i64::from_le_bytes(buf))
    }

    pub fn read_f64(&self, index: usize) -> io::Result<f64> {
        let mut buf = [0u8; 8];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(f64::from_le_bytes(buf))
    }

    pub fn read_bool(&self, index: usize) -> io::Result<bool> {
        let mut buf = [0u8; 1];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(buf[0] != 0)
    }

    pub fn read_uuid(&self, index: usize) -> io::Result<[u8; 16]> {
        let mut buf = [0u8; 16];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(buf)
    }

    pub fn read_timestamp(&self, index: usize) -> io::Result<i64> {
        let mut buf = [0u8; 8];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(i64::from_le_bytes(buf))
    }

    pub fn read_bytes(&self, index: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; self.value_size];
        self.file
            .read_exact_at(&mut buf, (index * self.value_size) as u64)
            .map_err(map_out_of_bounds)?;
        Ok(buf)
    }
}

/// Read-only, positional view over a [`VariableColumn`]'s data + offsets files
/// (#56 Direction B).  See [`FixedColumnReader`] for the concurrency model.
pub struct VariableColumnReader {
    data_file: File,
    offsets_file: File,
}

impl VariableColumnReader {
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets_file
            .metadata()
            .map(|m| m.len() as usize / 16)
            .unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read_string(&self, index: usize) -> io::Result<String> {
        let offsets_pos = (index * 16) as u64;
        let mut offset_buf = [0u8; 8];
        let mut length_buf = [0u8; 8];
        self.offsets_file
            .read_exact_at(&mut offset_buf, offsets_pos)
            .map_err(map_out_of_bounds)?;
        self.offsets_file
            .read_exact_at(&mut length_buf, offsets_pos + 8)
            .map_err(map_out_of_bounds)?;

        let offset = u64::from_le_bytes(offset_buf);
        let length = u64::from_le_bytes(length_buf);

        let mut data = vec![0u8; length as usize];
        self.data_file.read_exact_at(&mut data, offset)?;

        String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// Read-only, positional view over a [`Tombstones`] file (#56 Direction B).
/// See [`FixedColumnReader`] for the concurrency model.
pub struct TombstonesReader {
    file: File,
}

impl TombstonesReader {
    #[must_use]
    pub fn len(&self) -> usize {
        self.file.metadata().map(|m| m.len() as usize).unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_deleted(&self, index: usize) -> io::Result<bool> {
        let mut buf = [0u8; 1];
        self.file
            .read_exact_at(&mut buf, index as u64)
            .map_err(map_out_of_bounds)?;
        Ok(buf[0] != 0)
    }
}

/// Database directory manager
pub struct Database {
    root_path: PathBuf,
    manifest: Manifest,
    wal_manager: Option<forgedb_wal::WalManager>,
}

impl Database {
    pub fn open(root_path: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root_path)?;

        let manifest_path = root_path.join("manifest.json");
        let manifest = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            // Create default manifest
            Manifest {
                schema_version: 1,
                row_count: 0,
                columns: Vec::new(),
                wal_enabled: false,
                last_checkpoint: 0,
                compaction_epoch: 0,
                format_version: default_format_version(),
                row_anchor: None,
            }
        };

        Ok(Database {
            root_path,
            manifest,
            wal_manager: None,
        })
    }

    /// Open database with WAL support.
    ///
    /// This attaches a [`WalManager`] but does **not** itself replay outstanding
    /// WAL entries into the columnar files: the storage layer has no knowledge of
    /// how logged operations map onto column writes. Crash recovery is the
    /// caller's responsibility — after opening, drive
    /// [`WalManager::replay_committed`] (via [`Database::wal_mut`]) and apply each
    /// committed entry before serving reads.
    pub fn open_with_wal(
        root_path: PathBuf,
        fsync_policy: forgedb_wal::FsyncPolicy,
    ) -> io::Result<Self> {
        fs::create_dir_all(&root_path)?;

        let manifest_path = root_path.join("manifest.json");
        let mut manifest = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        } else {
            // Create default manifest
            Manifest {
                schema_version: 1,
                row_count: 0,
                columns: Vec::new(),
                wal_enabled: true,
                last_checkpoint: 0,
                compaction_epoch: 0,
                format_version: default_format_version(),
                row_anchor: None,
            }
        };

        // Mark WAL as enabled
        manifest.wal_enabled = true;

        // Open WAL
        let wal_path = root_path.join("wal.log");
        let wal_manager = forgedb_wal::WalManager::open(wal_path, fsync_policy)?;

        Ok(Database {
            root_path,
            manifest,
            wal_manager: Some(wal_manager),
        })
    }

    /// Get a mutable reference to the WAL manager
    pub fn wal_mut(&mut self) -> Option<&mut forgedb_wal::WalManager> {
        self.wal_manager.as_mut()
    }

    /// Get a reference to the WAL manager
    pub fn wal(&self) -> Option<&forgedb_wal::WalManager> {
        self.wal_manager.as_ref()
    }

    /// Check if WAL is enabled
    pub fn has_wal(&self) -> bool {
        self.wal_manager.is_some()
    }

    pub fn save_manifest(&self) -> io::Result<()> {
        self.manifest
            .save_to(&self.root_path.join("manifest.json"))
    }

    pub fn get_manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn update_row_count(&mut self, count: usize) {
        self.manifest.row_count = count;
    }

    pub fn set_columns(&mut self, columns: Vec<ColumnMetadata>) {
        self.manifest.columns = columns;
    }

    /// Returns the storage path for a fixed-size column.
    ///
    /// # Deprecation
    ///
    /// This helper always uses `u64` as the type component of the path (e.g. `fixed/u64_0.bin`),
    /// regardless of the actual column type. Prefer [`fixed_column_path_typed`](Database::fixed_column_path_typed)
    /// which uses the real column type so that file names reflect the stored data type.
    ///
    /// Note: generated code already uses type-aware paths; this helper is used only by examples
    /// and tests that pre-date the typed variant.
    #[deprecated(
        since = "0.1.1",
        note = "use `fixed_column_path_typed` for type-aware file naming"
    )]
    pub fn fixed_column_path(&self, column_index: usize) -> PathBuf {
        self.root_path
            .join(format!("fixed/u64_{}.bin", column_index))
    }

    /// Returns the storage path for a fixed-size column, using the actual column type
    /// in the file name (e.g. `fixed/i32_0.bin` for an `I32` column).
    ///
    /// Generated code already uses this naming convention. The older
    /// [`fixed_column_path`](Database::fixed_column_path) always used `u64_` regardless of type
    /// and is deprecated.
    pub fn fixed_column_path_typed(
        &self,
        column_index: usize,
        column_type: &ColumnType,
    ) -> PathBuf {
        let type_name = match column_type {
            ColumnType::U32 => "u32",
            ColumnType::U64 => "u64",
            ColumnType::I32 => "i32",
            ColumnType::I64 => "i64",
            ColumnType::F64 => "f64",
            ColumnType::Bool => "bool",
            ColumnType::Uuid => "uuid",
            ColumnType::Timestamp => "timestamp",
            ColumnType::String => "string",
            ColumnType::FixedBytes(_) => "bytes",
        };
        self.root_path
            .join(format!("fixed/{}_{}.bin", type_name, column_index))
    }

    pub fn variable_data_path(&self, column_index: usize) -> PathBuf {
        self.root_path
            .join(format!("variable/string_data_{}.bin", column_index))
    }

    pub fn variable_offsets_path(&self, column_index: usize) -> PathBuf {
        self.root_path
            .join(format!("variable/string_offsets_{}.bin", column_index))
    }

    pub fn tombstones_path(&self) -> PathBuf {
        self.root_path.join("tombstones.bin")
    }
}
