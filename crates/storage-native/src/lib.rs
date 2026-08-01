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
//! ## Describing a model's physical layout (`Manifest`)
//!
//! Generated code owns the directory layout and drives the column types directly;
//! the schema-blind [`Manifest`] records the physical layout (read by backup /
//! the inspector), persisted next to the column files.
//!
//! ```rust,no_run
//! use forgedb_storage_native::{Manifest, ColumnMetadata, ColumnType};
//! use std::path::PathBuf;
//!
//! let manifest = Manifest {
//!     schema_version: 1,
//!     row_count: 42,
//!     columns: vec![
//!         ColumnMetadata { name: "id".to_string(), column_type: ColumnType::U64, column_index: 0, ..Default::default() },
//!         ColumnMetadata { name: "email".to_string(), column_type: ColumnType::String, column_index: 1, ..Default::default() },
//!     ],
//!     wal_enabled: false,
//!     last_checkpoint: 0,
//!     compaction_epoch: 0,
//!     format_version: 1,
//!     row_anchor: None,
//! };
//! manifest.save_to(&PathBuf::from("./mydb/manifest.json"))?;
//! let reopened = Manifest::load_from(&PathBuf::from("./mydb/manifest.json"))?;
//! assert_eq!(reopened.row_count, 42);
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! ## Working with Columns
//!
//! ```rust,no_run
//! use forgedb_storage_native::FixedColumn;
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
//! # Durability Model
//!
//! Column files (`FixedColumn`, `VariableColumn`, `Tombstones`) **do not fsync on every append**.
//! Appended bytes are visible to subsequent reads via the OS page cache, but a crash before an
//! explicit [`FixedColumn::flush`] / [`VariableColumn::flush`] / [`Tombstones::flush`] call can
//! lose the most recent appends from those files.
//!
//! The **WAL is the crash-durability boundary**: all mutations should be recorded in the WAL
//! (via [`WalManager`]) before being applied to the column files. On recovery, the WAL is replayed
//! into the columnar materialization. Call `flush()` at commit / checkpoint boundaries to durably
//! persist the column files.
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`Manifest`] - Physical-layout metadata stored in manifest.json
//! - [`FixedColumn`] - Storage for fixed-size column data
//! - [`VariableColumn`] - Storage for variable-length column data
//! - [`Tombstones`] - Deletion tracking bitmap
//!
//! ## WAL Re-exports
//!
//! Types from [`forgedb-wal`](../forgedb_wal) for convenience:
//! - [`FsyncPolicy`] - Controls when WAL is fsynced to disk
//! - [`WalEntry`] - A framed opaque-bytes entry (model tag + `Raw` payload)
//! - [`WalManager`] - High-level WAL interface
//! - [`WalOperation`] - The `Raw { payload }` operation variant
//!
//! # Related Crates
//!
//! - [`forgedb-wal`](../forgedb_wal) - Write-Ahead Log for durability
//! - [`forgedb-compaction`](../forgedb_compaction) - Background compaction for space reclamation

// WAL re-exports (schema-agnostic substrate surface only)
pub use forgedb_wal::{FsyncPolicy, WalEntry, WalManager, WalOperation};

mod dir_lock;
pub use dir_lock::DirLock;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

/// Push a file's dirty page-cache pages to the storage device **without** a
/// device-cache barrier (#153 — single-barrier checkpoint).
///
/// On macOS, `File::sync_all`/`sync_data` both map to `fcntl(F_FULLFSYNC)` — an
/// expensive device barrier (~3.5 ms). `libc::fsync` is the *cheap* flush
/// (~27 µs) that only pushes the file's data into the drive's (volatile) write
/// cache; a single [`device_barrier`] afterward flushes that whole cache to
/// permanent media, covering **every** file `fsync`ed this way on the same
/// device. So a checkpoint of N columns costs N cheap fsyncs + ONE barrier
/// instead of N barriers. On non-macOS, `sync_data` (fdatasync) is the cheapest
/// available durable flush.
#[cfg(target_os = "macos")]
fn fsync_to_drive(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: `file` is a live, open descriptor for the duration of the call.
    if unsafe { libc::fsync(file.as_raw_fd()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn fsync_to_drive(file: &File) -> io::Result<()> {
    file.sync_data()
}

/// Issue ONE device-cache barrier (#153). On macOS `fcntl(F_FULLFSYNC)` tells
/// the drive to flush its entire write cache to permanent media — which makes
/// durable every file previously handed to [`fsync_to_drive`] on the same
/// device, so a checkpoint needs only one of these. On non-macOS, `sync_all` is
/// the barrier.
#[cfg(target_os = "macos")]
fn device_barrier(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: `file` is a live, open descriptor for the duration of the call.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn device_barrier(file: &File) -> io::Result<()> {
    file.sync_all()
}

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
    /// `<model>/manifest.json`). Used by
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
/// The backing of one columnar export batch — the substrate half of the
/// language bindings' Arrow / columnar-export path.
///
/// A columnar export hands the caller (generated FFI glue) a single contiguous,
/// stable-pointer byte buffer for one column at one live-row selection. This
/// type owns that buffer through *whichever* of the two production paths
/// produced it, behind an identical `as_ptr()` / `len()` surface so the consumer
/// (e.g. an Arrow `ArrowArray`) is **alias-or-gather transparent**:
///
/// - [`ColumnExport::Mapped`] — a **zero-copy `mmap` alias** of the column
///   file's dense prefix, taken when the live selection is exactly the
///   contiguous prefix `[0, n)` (no updates, no tombstones). No bytes are
///   copied; the pointer aliases the page cache directly. Because numeric
///   columns are stored little-endian (`to_le_bytes`), the mapped prefix *is* a
///   valid Arrow primitive buffer as-is.
/// - [`ColumnExport::Owned`] — a gathered contiguous copy (`FixedColumn::gather`),
///   the fallback whenever the selection is not an aliasable dense prefix
///   (update-heavy or tombstoned tables), or is empty.
///
/// # Safety / aliasing invariant (`Mapped`)
///
/// The mapped prefix aliases live file pages, so it relies on the same
/// single-writer, append-only discipline the reader handles already assume:
/// prefix rows `[0, n)` are committed and immutable (appends only ever extend
/// past `n`, never rewrite mapped bytes), and a compaction/rollback that would
/// renumber or truncate below `n` does not run while an export is outstanding
/// (exports are synchronous reads taken on the single writer). Under that
/// discipline the alias observes a stable snapshot.
pub enum ColumnExport {
    /// A gathered / copied contiguous buffer.
    Owned(Vec<u8>),
    /// A zero-copy read-only `mmap` alias of the column file's dense prefix.
    Mapped(memmap2::Mmap),
}

impl ColumnExport {
    /// Pointer to the first byte of the exported buffer. Stable across moving
    /// the `ColumnExport` (both a `Vec`'s heap allocation and an `Mmap`'s mapped
    /// address are independent of where the owner struct itself lives), so the
    /// FFI layer can capture it, then move `self` into an owner box.
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        match self {
            ColumnExport::Owned(v) => v.as_ptr(),
            ColumnExport::Mapped(m) => m.as_ptr(),
        }
    }

    /// Total exported byte length (`live_rows * value_size`).
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            ColumnExport::Owned(v) => v.len(),
            ColumnExport::Mapped(m) => m.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The exported bytes as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            ColumnExport::Owned(v) => v.as_slice(),
            ColumnExport::Mapped(m) => &m[..],
        }
    }
}

/// An in-memory bulk-loaded selection of a [`FixedColumn`]'s rows (#168).
///
/// Produced by [`FixedColumn::gather_buffered`], it holds one column's selected
/// rows as a single contiguous buffer (a zero-copy `mmap` alias of the dense
/// prefix, or a gathered copy) and exposes the **same** positional
/// `read_*` accessors as [`FixedColumn`], addressed by **slot** (`0..n` over the
/// selection order) rather than physical row index.  Decoding one column of a
/// scan from this buffer costs no syscalls; it lets the generated column scan
/// decode with the identical per-field logic it uses for per-row reads.
///
/// Errors mirror [`FixedColumn`]: an out-of-range slot returns `InvalidInput`.
pub struct BufferedFixedColumn {
    buf: ColumnExport,
    value_size: usize,
}

impl BufferedFixedColumn {
    /// Number of buffered slots (`indices.len()` from the gather).
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len() / self.value_size
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The `value_size`-wide bytes at `slot`, or `InvalidInput` if out of range.
    fn slot_bytes(&self, slot: usize) -> io::Result<&[u8]> {
        let start = slot * self.value_size;
        let end = start + self.value_size;
        self.buf.as_slice().get(start..end).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Slot out of bounds")
        })
    }

    pub fn read_u32(&self, slot: usize) -> io::Result<u32> {
        let b = self.slot_bytes(slot)?;
        Ok(u32::from_le_bytes(b[..4].try_into().unwrap()))
    }

    pub fn read_u64(&self, slot: usize) -> io::Result<u64> {
        let b = self.slot_bytes(slot)?;
        Ok(u64::from_le_bytes(b[..8].try_into().unwrap()))
    }

    pub fn read_i32(&self, slot: usize) -> io::Result<i32> {
        let b = self.slot_bytes(slot)?;
        Ok(i32::from_le_bytes(b[..4].try_into().unwrap()))
    }

    pub fn read_i64(&self, slot: usize) -> io::Result<i64> {
        let b = self.slot_bytes(slot)?;
        Ok(i64::from_le_bytes(b[..8].try_into().unwrap()))
    }

    pub fn read_f64(&self, slot: usize) -> io::Result<f64> {
        let b = self.slot_bytes(slot)?;
        Ok(f64::from_le_bytes(b[..8].try_into().unwrap()))
    }

    pub fn read_bool(&self, slot: usize) -> io::Result<bool> {
        let b = self.slot_bytes(slot)?;
        Ok(b[0] != 0)
    }

    pub fn read_uuid(&self, slot: usize) -> io::Result<[u8; 16]> {
        let b = self.slot_bytes(slot)?;
        Ok(b[..16].try_into().unwrap())
    }

    pub fn read_timestamp(&self, slot: usize) -> io::Result<i64> {
        let b = self.slot_bytes(slot)?;
        Ok(i64::from_le_bytes(b[..8].try_into().unwrap()))
    }

    /// The whole `value_size`-wide value at `slot` (for char(N) / struct /
    /// fixed-array / nullable / optional-FK columns decoded via `read_unaligned`).
    pub fn read_bytes(&self, slot: usize) -> io::Result<Vec<u8>> {
        Ok(self.slot_bytes(slot)?.to_vec())
    }
}

/// Selection size at or above which [`FixedColumn::gather`] maps the spanned
/// region once and copies out of memory, instead of issuing one `read_exact_at`
/// per index (#221).
///
/// Below this a handful of `pread`s beats a mapping's setup + teardown; above
/// it the per-row syscall loop dominates completely — a 22-column scan of
/// 10 000 live rows was 220 000 syscalls (~112 ms) before #221. Deliberately
/// conservative rather than tuned: the point is to leave tiny gathers exactly
/// as they were while removing the per-row syscall from bulk scans.
const GATHER_MMAP_MIN_ROWS: usize = 8;

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

    /// Copy the physical rows at `indices` into one contiguous owned buffer, in
    /// the order given (`indices.len() * value_size` bytes).
    ///
    /// This is the class-1 columnar-read primitive behind the language bindings'
    /// Arrow / columnar-export **gather** path — the fallback taken whenever the
    /// live selection is *not* an aliasable dense prefix (update-heavy or
    /// tombstoned tables) and a contiguous copy of exactly the live rows is
    /// needed. It is schema-agnostic: `indices` are opaque physical row
    /// positions the caller computed (generated code derives the live set from
    /// `id_to_row` + tombstone liveness, exactly as `all()` / `all_at()` do
    /// today); `gather` never reads a field name, type, or schema.
    ///
    /// # Performance (#221)
    ///
    /// For a selection of at least [`GATHER_MMAP_MIN_ROWS`] rows this maps the
    /// spanned region `[min(indices), max(indices)]` **once** and copies out of
    /// that mapping in contiguous runs, rather than issuing one `read_exact_at`
    /// per index. Live selections under append-only churn are long runs broken
    /// by the holes superseded rows leave behind, so runs are typically far
    /// longer than one row and the copy collapses to a handful of `memcpy`s.
    ///
    /// The mapping is held only for the duration of the call and the returned
    /// buffer is owned, so this relies on a strictly weaker form of the
    /// aliasing invariant documented on [`ColumnExport`]: the spanned rows are
    /// committed (`< row_count`, so within the file's length) and no concurrent
    /// truncation runs, which the single-writer discipline already guarantees.
    ///
    /// Known-unmeasured case: a *sparse* selection spanning a very large file
    /// faults in one page per isolated row, reading more bytes than the
    /// per-row path would. It still trades syscalls for faults plus readahead,
    /// and real live sets stay dense (amplification is bounded by the generated
    /// compaction ceiling), but it has not been benchmarked.
    ///
    /// # Errors
    ///
    /// Returns `Err(InvalidInput)` if any index is `>= self.len()`.
    pub fn gather(&self, indices: &[usize]) -> io::Result<Vec<u8>> {
        if indices.is_empty() {
            return Ok(Vec::new());
        }

        // Bounds-check the whole selection up front: the mapped path spans
        // `min..=max`, so an out-of-range index has to be rejected before the
        // span is computed (and `gather` is all-or-nothing either way).
        let mut lo = usize::MAX;
        let mut hi = 0usize;
        for &index in indices {
            if index >= self.row_count {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Index out of bounds",
                ));
            }
            lo = lo.min(index);
            hi = hi.max(index);
        }

        let mut out = vec![0u8; indices.len() * self.value_size];

        if indices.len() >= GATHER_MMAP_MIN_ROWS {
            let offset = (lo * self.value_size) as u64;
            let map_len = (hi + 1 - lo) * self.value_size;
            // A failed mapping is not fatal — the per-row loop below produces
            // byte-identical output — so fall through instead of propagating.
            let mapped = unsafe {
                memmap2::MmapOptions::new()
                    .offset(offset)
                    .len(map_len)
                    .map(&self.file)
            };
            if let Ok(mmap) = mapped {
                let src = &mmap[..];
                let mut slot = 0usize;
                while slot < indices.len() {
                    // Extend the run while the selection stays consecutive.
                    let start = slot;
                    while slot + 1 < indices.len() && indices[slot + 1] == indices[slot] + 1 {
                        slot += 1;
                    }
                    slot += 1;
                    let len = (slot - start) * self.value_size;
                    let s = (indices[start] - lo) * self.value_size;
                    let d = start * self.value_size;
                    out[d..d + len].copy_from_slice(&src[s..s + len]);
                }
                return Ok(out);
            }
        }

        for (slot, &index) in indices.iter().enumerate() {
            let src = (index * self.value_size) as u64;
            let dst = slot * self.value_size;
            self.file
                .read_exact_at(&mut out[dst..dst + self.value_size], src)?;
        }
        Ok(out)
    }

    /// Export the physical rows at `indices` as a [`ColumnExport`], taking the
    /// **zero-copy `mmap` alias** fast path when `indices` is the contiguous
    /// dense prefix `[0, n)` and falling back to [`gather`](Self::gather)
    /// (an owned copy) otherwise. The returned buffer is identical bytes either
    /// way (`indices.len() * value_size`), so the FFI / Arrow consumer is
    /// alias-or-gather transparent.
    ///
    /// This is the class-1 columnar-read primitive the language bindings' Arrow
    /// export links: `indices` are opaque physical row positions the caller
    /// (generated code) computed from the live set; `export` reads no field
    /// name, type, or schema — it only inspects the indices' shape to decide
    /// whether they alias a prefix.
    ///
    /// # Errors
    ///
    /// Returns `Err(InvalidInput)` if any index is `>= self.len()`, or an I/O
    /// error if the `mmap` of the aliasable prefix fails.
    pub fn export(&self, indices: &[usize]) -> io::Result<ColumnExport> {
        let n = indices.len();
        // A dense prefix is `indices == [0, 1, ..., n-1]`, entirely within the
        // committed rows. Empty selections take the trivial owned path.
        let is_dense_prefix =
            n > 0 && n <= self.row_count && indices.iter().enumerate().all(|(i, &x)| x == i);

        if is_dense_prefix {
            // Alias exactly the first `n * value_size` bytes of the column file
            // — no copy. Safe under the single-writer/append-only invariant
            // documented on `ColumnExport`.
            let map_len = n * self.value_size;
            let mmap = unsafe { memmap2::MmapOptions::new().len(map_len).map(&self.file)? };
            return Ok(ColumnExport::Mapped(mmap));
        }

        Ok(ColumnExport::Owned(self.gather(indices)?))
    }

    /// Bulk-load the physical rows at `indices` into an in-memory
    /// [`BufferedFixedColumn`] whose positional `read_*(slot)` accessors read
    /// from memory instead of the file (#168 column scan).
    ///
    /// This is the class-1 read primitive behind the generated column-pruned
    /// sequential scan: a scan over `n` rows pays **one** bulk read per column
    /// (a zero-copy `mmap` alias when `indices` is the dense prefix `[0, n)` — the
    /// common append-only case — or a single gathered copy otherwise) rather than
    /// `n` per-row `read_exact_at` syscalls.  `indices` are opaque physical row
    /// positions the caller (generated code) computed from the live set; the
    /// returned buffer's `read_*` methods take a **slot** `0..indices.len()`
    /// indexing into that selection, in the given order.  Reads no field name,
    /// type, or schema.
    ///
    /// # Errors
    ///
    /// Propagates [`export`](Self::export)'s errors (out-of-bounds index or a
    /// failed `mmap`).
    pub fn gather_buffered(&self, indices: &[usize]) -> io::Result<BufferedFixedColumn> {
        Ok(BufferedFixedColumn {
            buf: self.export(indices)?,
            value_size: self.value_size,
        })
    }

    /// Flush all pending writes to disk (fsync).
    ///
    /// This is the explicit durability checkpoint: after `flush()` returns `Ok(())`,
    /// all previous appends are guaranteed to survive a crash. Call this at transaction
    /// commit boundaries or before advancing the WAL checkpoint.
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    /// Push this column's data to the drive cache **without** a device barrier
    /// (#153).  Pair with a single [`FixedColumn::barrier`] (on any column of the
    /// same device) to make a whole checkpoint's columns durable with ONE
    /// barrier instead of N.  See [`fsync_to_drive`].
    pub fn sync_to_drive(&self) -> io::Result<()> {
        fsync_to_drive(&self.file)
    }

    /// Issue the single device-cache barrier for a checkpoint (#153): flushes the
    /// drive's write cache to permanent media, making durable every column
    /// previously `sync_to_drive`d on the same device.  See [`device_barrier`].
    pub fn barrier(&self) -> io::Result<()> {
        device_barrier(&self.file)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.row_count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Refresh the in-memory `row_count` from the on-disk file length (#84).
    ///
    /// Reads `read_exact_at` the file positionally, bounded by `row_count`.  For
    /// the Tier 3 multi-process writer path a *peer* process may have appended
    /// values since this handle opened; `sync_from_disk` re-derives `row_count`
    /// from the shared file so the peer's committed rows become readable (paired
    /// with `Tombstones::sync_from_disk` and `VariableColumn::sync_from_disk`).
    /// A no-op for the single-writer path, which maintains `row_count` in memory.
    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        self.row_count = self.file.metadata()?.len() as usize / self.value_size;
        Ok(())
    }

    /// Truncate the column to hold exactly `rows` rows, discarding everything
    /// after that point.
    ///
    /// This is the crash-recovery primitive: after replaying a WAL, generated
    /// code calls `truncate_to_rows` on every column to realign them back to the
    /// last consistent row watermark before resuming appends.  Any partial
    /// trailing value left by a torn write is physically removed so the next
    /// `append_*` lands at exactly row `rows`.
    ///
    /// # Errors
    ///
    /// Returns `Err(InvalidInput)` if `rows > self.len()` — truncation cannot
    /// extend a column.  All other errors come from the underlying `set_len`
    /// syscall.
    pub fn truncate_to_rows(&mut self, rows: usize) -> io::Result<()> {
        if rows > self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate_to_rows beyond current length",
            ));
        }
        self.file.set_len((rows * self.value_size) as u64)?;
        self.row_count = rows;
        Ok(())
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

/// An in-memory bulk-loaded selection of a [`VariableColumn`]'s rows (#168).
///
/// Produced by [`VariableColumn::gather_buffered`]; holds the column's data
/// bytes plus each selected slot's `(absolute offset, length)` so `read_string`
/// slices from memory without a syscall.  The variable-column peer of
/// [`BufferedFixedColumn`]: same `read_string` name as [`VariableColumn`],
/// addressed by **slot** (`0..n` over the selection order).
pub struct BufferedVariableColumn {
    /// The spanned data region — a lazily-paged `mmap` alias when it is large
    /// enough to be worth mapping, an owned copy otherwise (#222).
    data: ColumnExport,
    /// File offset `data` starts at, so the absolute offsets in `slots` can be
    /// rebased onto it. Zero when the whole region is buffered.
    base: u64,
    // (absolute offset in the data FILE, byte length) per slot, in selection order.
    slots: Vec<(u64, u64)>,
}

impl BufferedVariableColumn {
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The slot's value **borrowed from the buffered span** — no allocation, no
    /// copy (#224).
    ///
    /// #222 made the span an `mmap` alias of the data region, so a scan already
    /// holds every live string's bytes; this is the read that stops copying them
    /// back out. The borrow is tied to `&self` and the mapping is owned by `self`,
    /// so the compiler guarantees the `&str` cannot outlive the pages it points at
    /// — a *tighter* constraint than the type already operates under, not a new
    /// aliasing exposure.
    ///
    /// UTF-8 is validated on every read (cheap next to the allocation it replaces)
    /// so an on-disk corruption surfaces as [`io::ErrorKind::InvalidData`] rather
    /// than as an unchecked `&str`.
    pub fn read_str(&self, slot: usize) -> io::Result<&str> {
        let &(offset, length) = self.slots.get(slot).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Slot out of bounds")
        })?;
        // Slots carry absolute file offsets; `base` is where the buffered span
        // starts. Touching a mapped page here is what pulls it in — dead versions
        // between live rows are never addressed, so they are never read.
        let start = offset.saturating_sub(self.base) as usize;
        let end = start + length as usize;
        let bytes = self.data.as_slice().get(start..end).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Variable slot out of data bounds")
        })?;
        std::str::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// The slot's value as an owned `String`. Kept for callers that genuinely need
    /// ownership; it is [`read_str`](Self::read_str) plus the copy, so there is one
    /// decode path and the two can never disagree.
    pub fn read_string(&self, slot: usize) -> io::Result<String> {
        self.read_str(slot).map(str::to_owned)
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
/// Spanned-data-region size at or above which [`VariableColumn::gather_buffered`]
/// maps rather than reads (#222).
///
/// Below this the span is small enough that a single bounded read costs less than a
/// mapping's setup and teardown; above it, mapping is what keeps dead versions inside
/// the span from being paid for — untouched pages are never faulted in. Conservative
/// rather than tuned; the shape of the win does not depend on the exact crossover.
const VAR_MMAP_MIN_BYTES: usize = 64 * 1024;

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

    /// Bulk-load the rows at `indices` into an in-memory
    /// [`BufferedVariableColumn`] whose `read_string(slot)` reads from memory
    /// (#168 column scan).
    ///
    /// Reads only the **spanned** slice of the offsets index and maps only the
    /// **spanned** data region, so a scan over `n` variable rows pays one bounded
    /// read plus one mapping rather than `3n` per-row `read_exact_at` syscalls
    /// (offset + length + data). `indices` are opaque physical row positions;
    /// reads no schema.
    ///
    /// # Performance (#222)
    ///
    /// This used to read the **entire** committed offsets index and the **entire**
    /// data region into owned buffers on every call, ignoring `indices` — dead
    /// versions included. That made a scan cost `~A x live_bytes`, a slope in
    /// amplification rather than the fixed columns' step, and it was measured as
    /// exactly that: at 2 000 live rows the narrow scan went 563 µs -> 5.8 ms
    /// across A = 1 -> 16, doubling with each doubling of A.
    ///
    /// Both reads are now bounded to `[min(indices), max(indices)]`. Append order
    /// makes data offsets monotonic in row index, so that row span maps to one
    /// contiguous byte span. The data span is **mapped** rather than read when it
    /// is large enough to be worth it, which is what removes the slope: dead
    /// versions inside the span cost address space, not I/O, because only pages
    /// actually addressed by `read_string` are ever faulted in.
    ///
    /// The residual cost is page granularity — a live row drags in its whole page,
    /// including any dead bytes sharing it — so the win depends on how clustered
    /// the live set is. Append-only helps here: an updated row is re-appended at
    /// the tail, so churn tends to concentrate live rows rather than scatter them.
    ///
    /// # Errors
    ///
    /// `InvalidInput` if any index is `>= self.len()`; other errors come from the
    /// underlying reads.
    pub fn gather_buffered(&self, indices: &[usize]) -> io::Result<BufferedVariableColumn> {
        if indices.is_empty() {
            return Ok(BufferedVariableColumn {
                data: ColumnExport::Owned(Vec::new()),
                base: 0,
                slots: Vec::new(),
            });
        }

        // Bounds-check the whole selection first, and take the row span.
        let mut lo = usize::MAX;
        let mut hi = 0usize;
        for &index in indices {
            if index >= self.row_count {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Index out of bounds",
                ));
            }
            lo = lo.min(index);
            hi = hi.max(index);
        }

        // Only the spanned slice of the offsets index (16 bytes/row), not all
        // `row_count` rows.
        let mut offsets = vec![0u8; (hi + 1 - lo) * 16];
        self.offsets_file
            .read_exact_at(&mut offsets, (lo * 16) as u64)?;

        let mut slots = Vec::with_capacity(indices.len());
        let mut data_lo = u64::MAX;
        let mut data_hi = 0u64;
        for &index in indices {
            let b = (index - lo) * 16;
            let offset = u64::from_le_bytes(offsets[b..b + 8].try_into().unwrap());
            let length = u64::from_le_bytes(offsets[b + 8..b + 16].try_into().unwrap());
            data_lo = data_lo.min(offset);
            data_hi = data_hi.max(offset + length);
            slots.push((offset, length));
        }

        // Every selected row empty (or a zero-length span) — nothing to map.
        if data_hi <= data_lo {
            return Ok(BufferedVariableColumn {
                data: ColumnExport::Owned(Vec::new()),
                base: data_lo.min(data_hi),
                slots,
            });
        }

        let span = (data_hi - data_lo) as usize;
        let data = if span >= VAR_MMAP_MIN_BYTES {
            // A failed mapping is not fatal — the bounded read below yields the
            // same bytes — so fall through rather than propagate.
            let mapped = unsafe {
                memmap2::MmapOptions::new()
                    .offset(data_lo)
                    .len(span)
                    .map(&self.data_file)
            };
            match mapped {
                Ok(m) => ColumnExport::Mapped(m),
                Err(_) => {
                    let mut v = vec![0u8; span];
                    self.data_file.read_exact_at(&mut v, data_lo)?;
                    ColumnExport::Owned(v)
                }
            }
        } else {
            let mut v = vec![0u8; span];
            self.data_file.read_exact_at(&mut v, data_lo)?;
            ColumnExport::Owned(v)
        };

        Ok(BufferedVariableColumn { data, base: data_lo, slots })
    }

    /// Flush all pending writes to disk (fsync both data and offsets files).
    ///
    /// After `flush()` returns `Ok(())`, all previous appends are guaranteed to
    /// survive a crash. Call at commit boundaries or before advancing the WAL checkpoint.
    pub fn flush(&mut self) -> io::Result<()> {
        self.data_file.sync_all()?;
        self.offsets_file.sync_all()
    }

    /// Push both backing files to the drive cache without a device barrier
    /// (#153).  Pair with one [`VariableColumn::barrier`] per checkpoint.
    pub fn sync_to_drive(&self) -> io::Result<()> {
        fsync_to_drive(&self.data_file)?;
        fsync_to_drive(&self.offsets_file)
    }

    /// Issue the single device-cache barrier for a checkpoint (#153).  Barriers
    /// are device-wide, so one call on the data file covers the offsets file too.
    pub fn barrier(&self) -> io::Result<()> {
        device_barrier(&self.data_file)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.row_count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Refresh the in-memory counters from the on-disk file lengths (#84).
    ///
    /// Re-derives `row_count` from the offsets file (16 bytes/row) and
    /// `current_data_offset` from the data file, so a *peer* process's appended
    /// rows (Tier 3 multi-process writer path) become readable through this
    /// handle.  Paired with `FixedColumn::sync_from_disk` /
    /// `Tombstones::sync_from_disk`.  A no-op for the single-writer path.
    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        self.row_count = self.offsets_file.metadata()?.len() as usize / 16;
        self.current_data_offset = self.data_file.metadata()?.len();
        Ok(())
    }

    /// Truncate the column to hold exactly `rows` rows, discarding everything
    /// after that point.
    ///
    /// The data file is cut at the byte immediately after the last byte of row
    /// `rows - 1` (read from the offsets entry for that row).  The offsets file
    /// is cut to `rows * 16` bytes.  Both in-memory counters are updated so the
    /// next `append_string` lands at exactly row `rows`.
    ///
    /// # Errors
    ///
    /// Returns `Err(InvalidInput)` if `rows > self.len()`.  All other errors
    /// come from underlying I/O or `set_len` syscalls.
    pub fn truncate_to_rows(&mut self, rows: usize) -> io::Result<()> {
        if rows > self.row_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate_to_rows beyond current length",
            ));
        }

        let data_len: u64 = if rows == 0 {
            0
        } else {
            // Positionally read the (offset, length) pair for row `rows - 1`.
            let offsets_pos = ((rows - 1) * 16) as u64;
            let mut offset_buf = [0u8; 8];
            let mut length_buf = [0u8; 8];
            self.offsets_file
                .read_exact_at(&mut offset_buf, offsets_pos)?;
            self.offsets_file
                .read_exact_at(&mut length_buf, offsets_pos + 8)?;
            let offset = u64::from_le_bytes(offset_buf);
            let length = u64::from_le_bytes(length_buf);
            offset + length
        };

        self.offsets_file.set_len((rows * 16) as u64)?;
        self.data_file.set_len(data_len)?;
        self.current_data_offset = data_len;
        self.row_count = rows;
        Ok(())
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

    /// Return the subset of `rows` that are **not** tombstoned, preserving order
    /// (#168 scan filter).  Reads the whole tombstone column **once** rather than
    /// a per-row [`is_deleted`](Self::is_deleted) syscall — the bulk liveness
    /// filter a column scan applies to its live-row selection.  A row index past
    /// the committed count is treated as not-live (dropped), matching
    /// `is_deleted(..).unwrap_or(true)`.
    pub fn live_indices(&self, rows: &[usize]) -> io::Result<Vec<usize>> {
        let mut bytes = vec![0u8; self.count];
        if !bytes.is_empty() {
            self.file.read_exact_at(&mut bytes, 0)?;
        }
        Ok(rows
            .iter()
            .copied()
            .filter(|&r| bytes.get(r) == Some(&0))
            .collect())
    }

    /// Flush all pending writes to disk (fsync).
    ///
    /// After `flush()` returns `Ok(())`, all previous appends are guaranteed to
    /// survive a crash. Call at commit boundaries or before advancing the WAL checkpoint.
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    /// Push the tombstone column to the drive cache without a barrier (#153).
    pub fn sync_to_drive(&self) -> io::Result<()> {
        fsync_to_drive(&self.file)
    }

    /// Issue the single device-cache barrier for a checkpoint (#153).
    pub fn barrier(&self) -> io::Result<()> {
        device_barrier(&self.file)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Truncate the tombstone bitmap to hold exactly `rows` rows, discarding
    /// everything after that point.
    ///
    /// # Errors
    ///
    /// Returns `Err(InvalidInput)` if `rows > self.len()`.  All other errors
    /// come from the underlying `set_len` syscall.
    pub fn truncate_to_rows(&mut self, rows: usize) -> io::Result<()> {
        if rows > self.count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate_to_rows beyond current length",
            ));
        }
        self.file.set_len(rows as u64)?;
        self.count = rows;
        Ok(())
    }

    /// Re-read the on-disk tombstone count from the file metadata and update the
    /// cached `count`.
    ///
    /// Normally `count` is maintained in memory by `append`/`truncate_to_rows`, so
    /// this is a no-op for the single-writer path.  For the Tier 3 multi-process
    /// writer path (#84) a peer process may have appended tombstone bytes since this
    /// writer opened the file; calling `sync_from_disk` before `len()` lets the
    /// writer see the peer's committed rows without reopening the storage.
    ///
    /// # Errors
    ///
    /// Propagates any error from `file.metadata()`.
    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        self.count = self.file.metadata()?.len() as usize;
        Ok(())
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
