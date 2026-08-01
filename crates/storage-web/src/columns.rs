//! Arena-backed columns with byte-identical positional semantics to the native
//! `FixedColumn` / `VariableColumn` / `Tombstones`.
//!
//! Every method matches the native signature so the generated code links
//! unchanged. The only difference is the backing: a keyed [`crate::store`] arena
//! instead of a file. Reads are slice math; appends push to the arena; length is
//! derived **live** from the arena byte length (no cached row count to drift).
//! `flush` is a no-op here — durability happens at the [`crate::store::dump`]
//! commit boundary, not per append.

use std::io;
use std::path::PathBuf;

use crate::store;

fn oob() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Index out of bounds")
}

/// The backing of one columnar export batch — the browser twin of the native
/// [`forgedb_storage_native::ColumnExport`](../../forgedb_storage_native).
///
/// The native backend can hand out a zero-copy `mmap` alias of a dense prefix;
/// this backend is **gather-only** (constraint 7 — no aliasing into the wasm
/// arena / `thread_local` store), so an export is always an owned copy. The
/// public surface (`as_ptr` / `len` / `is_empty` / `as_slice`) matches the
/// native type exactly, so the shared generated columnar-export code links
/// unchanged across targets.
pub struct ColumnExport {
    bytes: Vec<u8>,
}

impl ColumnExport {
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

/// In-memory bulk-loaded selection of a [`FixedColumn`]'s rows (#168), the
/// browser twin of the native
/// [`BufferedFixedColumn`](../../forgedb_storage_native). Same slot-addressed
/// `read_*` surface, so the shared generated column-scan code links unchanged.
pub struct BufferedFixedColumn {
    buf: ColumnExport,
    value_size: usize,
}

impl BufferedFixedColumn {
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len() / self.value_size
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn slot_bytes(&self, slot: usize) -> io::Result<&[u8]> {
        let start = slot * self.value_size;
        let end = start + self.value_size;
        self.buf.as_slice().get(start..end).ok_or_else(oob)
    }

    pub fn read_u32(&self, slot: usize) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.slot_bytes(slot)?[..4].try_into().unwrap()))
    }
    pub fn read_u64(&self, slot: usize) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.slot_bytes(slot)?[..8].try_into().unwrap()))
    }
    pub fn read_i32(&self, slot: usize) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.slot_bytes(slot)?[..4].try_into().unwrap()))
    }
    pub fn read_i64(&self, slot: usize) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.slot_bytes(slot)?[..8].try_into().unwrap()))
    }
    pub fn read_f64(&self, slot: usize) -> io::Result<f64> {
        Ok(f64::from_le_bytes(self.slot_bytes(slot)?[..8].try_into().unwrap()))
    }
    pub fn read_bool(&self, slot: usize) -> io::Result<bool> {
        Ok(self.slot_bytes(slot)?[0] != 0)
    }
    pub fn read_uuid(&self, slot: usize) -> io::Result<[u8; 16]> {
        Ok(self.slot_bytes(slot)?[..16].try_into().unwrap())
    }
    pub fn read_timestamp(&self, slot: usize) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.slot_bytes(slot)?[..8].try_into().unwrap()))
    }
    pub fn read_bytes(&self, slot: usize) -> io::Result<Vec<u8>> {
        Ok(self.slot_bytes(slot)?.to_vec())
    }
}

/// In-memory bulk-loaded selection of a [`VariableColumn`]'s rows (#168), the
/// browser twin of the native
/// [`BufferedVariableColumn`](../../forgedb_storage_native).
pub struct BufferedVariableColumn {
    data: Vec<u8>,
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
    /// The slot's value borrowed from the buffered arena — no allocation (#224).
    /// Browser twin of the native `read_str`; generated code calls this name on
    /// both targets.
    pub fn read_str(&self, slot: usize) -> io::Result<&str> {
        let &(offset, length) = self.slots.get(slot).ok_or_else(oob)?;
        let start = offset as usize;
        let end = start + length as usize;
        let bytes = self.data.get(start..end).ok_or_else(oob)?;
        std::str::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Owned form of [`read_str`](Self::read_str) — one decode path, plus the copy.
    pub fn read_string(&self, slot: usize) -> io::Result<String> {
        self.read_str(slot).map(str::to_owned)
    }
}

/// Fixed-size column storage backed by an in-memory arena.
pub struct FixedColumn {
    path: PathBuf,
    value_size: usize,
}

impl FixedColumn {
    pub fn new(path: PathBuf, value_size: usize) -> io::Result<Self> {
        if value_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "FixedColumn value_size must be non-zero",
            ));
        }
        store::ensure(&path);
        Ok(FixedColumn { path, value_size })
    }

    fn read_raw(&self, index: usize, n: usize) -> io::Result<Vec<u8>> {
        let off = index * self.value_size;
        store::with_bytes(&self.path, |b| {
            if off + n > b.len() {
                return Err(oob());
            }
            Ok(b[off..off + n].to_vec())
        })
    }

    fn append_raw(&mut self, bytes: &[u8]) {
        store::with_bytes_mut(&self.path, |b| b.extend_from_slice(bytes));
    }

    pub fn append_u32(&mut self, value: u32) -> io::Result<()> {
        self.append_raw(&value.to_le_bytes());
        Ok(())
    }
    pub fn read_u32(&self, index: usize) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.read_raw(index, 4)?.try_into().unwrap()))
    }

    pub fn append_u64(&mut self, value: u64) -> io::Result<()> {
        self.append_raw(&value.to_le_bytes());
        Ok(())
    }
    pub fn read_u64(&self, index: usize) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }

    pub fn append_i32(&mut self, value: i32) -> io::Result<()> {
        self.append_raw(&value.to_le_bytes());
        Ok(())
    }
    pub fn read_i32(&self, index: usize) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.read_raw(index, 4)?.try_into().unwrap()))
    }

    pub fn append_i64(&mut self, value: i64) -> io::Result<()> {
        self.append_raw(&value.to_le_bytes());
        Ok(())
    }
    pub fn read_i64(&self, index: usize) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }

    pub fn append_f64(&mut self, value: f64) -> io::Result<()> {
        self.append_raw(&value.to_le_bytes());
        Ok(())
    }
    pub fn read_f64(&self, index: usize) -> io::Result<f64> {
        Ok(f64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }

    pub fn append_bool(&mut self, value: bool) -> io::Result<()> {
        self.append_raw(&[u8::from(value)]);
        Ok(())
    }
    pub fn read_bool(&self, index: usize) -> io::Result<bool> {
        Ok(self.read_raw(index, 1)?[0] != 0)
    }

    pub fn append_uuid(&mut self, value: [u8; 16]) -> io::Result<()> {
        self.append_raw(&value);
        Ok(())
    }
    pub fn read_uuid(&self, index: usize) -> io::Result<[u8; 16]> {
        Ok(self.read_raw(index, 16)?.try_into().unwrap())
    }

    pub fn append_timestamp(&mut self, value: i64) -> io::Result<()> {
        self.append_raw(&value.to_le_bytes());
        Ok(())
    }
    pub fn read_timestamp(&self, index: usize) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }

    /// Append arbitrary fixed-width bytes (char(N), inline struct, fixed array).
    pub fn append_bytes(&mut self, value: &[u8]) -> io::Result<()> {
        if value.len() != self.value_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Expected {} bytes, got {}", self.value_size, value.len()),
            ));
        }
        self.append_raw(value);
        Ok(())
    }
    pub fn read_bytes(&self, index: usize) -> io::Result<Vec<u8>> {
        self.read_raw(index, self.value_size)
    }

    /// Copy the physical rows at `indices` into one contiguous owned buffer, in
    /// the order given (`indices.len() * value_size` bytes). Arena-backed twin of
    /// the native [`FixedColumn::gather`](../../forgedb_storage_native) — same
    /// signature and byte semantics, so the shared generated columnar-export code
    /// links unchanged across targets. Schema-agnostic: `indices` are opaque
    /// physical row positions the caller (generated code) computed.
    ///
    /// # Errors
    ///
    /// Returns `Err(InvalidInput)` if any index is `>= self.len()`.
    pub fn gather(&self, indices: &[usize]) -> io::Result<Vec<u8>> {
        let vs = self.value_size;
        store::with_bytes(&self.path, |b| {
            let mut out = vec![0u8; indices.len() * vs];
            for (slot, &index) in indices.iter().enumerate() {
                let src = index * vs;
                if src + vs > b.len() {
                    return Err(oob());
                }
                out[slot * vs..slot * vs + vs].copy_from_slice(&b[src..src + vs]);
            }
            Ok(out)
        })
    }

    /// Export the physical rows at `indices` as a [`ColumnExport`]. Arena-backed
    /// twin of the native [`FixedColumn::export`](../../forgedb_storage_native) —
    /// same signature, but **gather-only** (this target never aliases the arena),
    /// so it always returns an owned copy. Schema-agnostic: `indices` are opaque
    /// physical row positions the caller (generated code) computed.
    ///
    /// # Errors
    ///
    /// Returns `Err(InvalidInput)` if any index is `>= self.len()`.
    pub fn export(&self, indices: &[usize]) -> io::Result<ColumnExport> {
        Ok(ColumnExport {
            bytes: self.gather(indices)?,
        })
    }

    /// Bulk-load the rows at `indices` into a [`BufferedFixedColumn`] (#168
    /// column scan). Arena twin of the native
    /// [`FixedColumn::gather_buffered`](../../forgedb_storage_native).
    pub fn gather_buffered(&self, indices: &[usize]) -> io::Result<BufferedFixedColumn> {
        Ok(BufferedFixedColumn {
            buf: self.export(indices)?,
            value_size: self.value_size,
        })
    }

    /// No-op on this target — durability is at the [`crate::store::dump`] commit
    /// boundary, not per append.
    pub fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// No-op on the wasm arena backend (#153): there is no device barrier;
    /// durability comes from the transport writing arenas to IndexedDB/OPFS at
    /// the [`crate::store::dump`] boundary. Present for API parity with
    /// `storage-native`, since the generated `checkpoint()` calls it ungated.
    pub fn sync_to_drive(&self) -> io::Result<()> {
        Ok(())
    }

    /// No-op on the wasm arena backend (#153): see [`Self::sync_to_drive`].
    pub fn barrier(&self) -> io::Result<()> {
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        store::byte_len(&self.path) / self.value_size
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn truncate_to_rows(&mut self, rows: usize) -> io::Result<()> {
        let cur = self.len();
        if rows > cur {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate_to_rows beyond current length",
            ));
        }
        // Truncating to the current length is a no-op — return WITHOUT touching
        // the bytes, so a lazily source-backed column (whose length is known from
        // the source) is not needlessly faulted in. This is load-bearing for
        // partial hydrate: recovery calls `truncate_to_rows(anchor)` on every
        // column at open, and only genuinely-torn columns should load.
        if rows == cur {
            return Ok(());
        }
        store::with_bytes_mut(&self.path, |b| b.truncate(rows * self.value_size));
        Ok(())
    }

    /// API-parity no-op for the native `sync_from_disk` (#84 MVCC Tier 3 peer
    /// read-currency). On the arena backend `len()` is derived live from the
    /// store on every call ("the path IS the key"), so there is no cached
    /// `row_count` to re-derive — and a browser replica is a single-follower,
    /// never a multi-process coordinated peer. Present so the shared generated
    /// `database.rs` (which calls it in `__sync_columns_from_disk`) compiles for
    /// `wasm32` identically to native.
    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        Ok(())
    }

    pub fn reader(&self) -> io::Result<FixedColumnReader> {
        Ok(FixedColumnReader {
            path: self.path.clone(),
            value_size: self.value_size,
        })
    }
}

/// Variable-length (string) column: an append-only data arena plus a 16-byte
/// `(offset, length)` offsets arena, mirroring the native file layout.
pub struct VariableColumn {
    data_path: PathBuf,
    offsets_path: PathBuf,
}

impl VariableColumn {
    pub fn new(data_path: PathBuf, offsets_path: PathBuf) -> io::Result<Self> {
        store::ensure(&data_path);
        store::ensure(&offsets_path);
        Ok(VariableColumn {
            data_path,
            offsets_path,
        })
    }

    pub fn append_string(&mut self, value: &str) -> io::Result<()> {
        let bytes = value.as_bytes();
        let offset = store::byte_len(&self.data_path) as u64;
        let length = bytes.len() as u64;
        store::with_bytes_mut(&self.data_path, |b| b.extend_from_slice(bytes));
        store::with_bytes_mut(&self.offsets_path, |b| {
            b.extend_from_slice(&offset.to_le_bytes());
            b.extend_from_slice(&length.to_le_bytes());
        });
        Ok(())
    }

    pub fn read_string(&self, index: usize) -> io::Result<String> {
        let (offset, length) = read_offset_pair(&self.offsets_path, index)?;
        let data = store::with_bytes(&self.data_path, |b| {
            let end = offset + length;
            if end > b.len() {
                return Err(oob());
            }
            Ok(b[offset..end].to_vec())
        })?;
        String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Bulk-load the rows at `indices` into a [`BufferedVariableColumn`] (#168
    /// column scan). Arena twin of the native
    /// [`VariableColumn::gather_buffered`](../../forgedb_storage_native): reads
    /// the whole offsets + data arenas once, then records each slot's
    /// `(offset, length)` in selection order.
    pub fn gather_buffered(&self, indices: &[usize]) -> io::Result<BufferedVariableColumn> {
        let data: Vec<u8> =
            store::with_bytes(&self.data_path, |b| Ok::<Vec<u8>, io::Error>(b.to_vec()))?;
        let slots = store::with_bytes(&self.offsets_path, |b| {
            let mut slots = Vec::with_capacity(indices.len());
            for &index in indices {
                let pos = index * 16;
                if pos + 16 > b.len() {
                    return Err(oob());
                }
                let offset = u64::from_le_bytes(b[pos..pos + 8].try_into().unwrap());
                let length = u64::from_le_bytes(b[pos + 8..pos + 16].try_into().unwrap());
                slots.push((offset, length));
            }
            Ok(slots)
        })?;
        Ok(BufferedVariableColumn { data, slots })
    }

    pub fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// No-op on the wasm arena backend (#153): there is no device barrier;
    /// durability comes from the transport writing arenas to IndexedDB/OPFS at
    /// the [`crate::store::dump`] boundary. Present for API parity with
    /// `storage-native`, since the generated `checkpoint()` calls it ungated.
    pub fn sync_to_drive(&self) -> io::Result<()> {
        Ok(())
    }

    /// No-op on the wasm arena backend (#153): see [`Self::sync_to_drive`].
    pub fn barrier(&self) -> io::Result<()> {
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        store::byte_len(&self.offsets_path) / 16
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn truncate_to_rows(&mut self, rows: usize) -> io::Result<()> {
        let cur = self.len();
        if rows > cur {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate_to_rows beyond current length",
            ));
        }
        // No-op truncation must not fault a lazy column in (see FixedColumn).
        if rows == cur {
            return Ok(());
        }
        let data_len = if rows == 0 {
            0
        } else {
            let (offset, length) = read_offset_pair(&self.offsets_path, rows - 1)?;
            offset + length
        };
        store::with_bytes_mut(&self.offsets_path, |b| b.truncate(rows * 16));
        store::with_bytes_mut(&self.data_path, |b| b.truncate(data_len));
        Ok(())
    }

    /// API-parity no-op for the native `sync_from_disk` (#84 MVCC Tier 3). See
    /// `FixedColumn::sync_from_disk` — arena length is always live, so there is
    /// nothing to re-derive; present so the shared generated `database.rs`
    /// compiles for `wasm32`.
    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        Ok(())
    }

    pub fn reader(&self) -> io::Result<VariableColumnReader> {
        Ok(VariableColumnReader {
            data_path: self.data_path.clone(),
            offsets_path: self.offsets_path.clone(),
        })
    }
}

/// Read the `(offset, length)` pair for `index` from an offsets arena.
fn read_offset_pair(offsets_path: &std::path::Path, index: usize) -> io::Result<(usize, usize)> {
    let pos = index * 16;
    store::with_bytes(offsets_path, |b| {
        if pos + 16 > b.len() {
            return Err(oob());
        }
        let offset = u64::from_le_bytes(b[pos..pos + 8].try_into().unwrap()) as usize;
        let length = u64::from_le_bytes(b[pos + 8..pos + 16].try_into().unwrap()) as usize;
        Ok((offset, length))
    })
}

/// Tombstone bitmap (1 byte per row) backed by an arena.
pub struct Tombstones {
    path: PathBuf,
}

impl Tombstones {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        store::ensure(&path);
        Ok(Tombstones { path })
    }

    pub fn append(&mut self, deleted: bool) -> io::Result<()> {
        store::with_bytes_mut(&self.path, |b| b.push(u8::from(deleted)));
        Ok(())
    }

    pub fn is_deleted(&self, index: usize) -> io::Result<bool> {
        store::with_bytes(&self.path, |b| {
            if index >= b.len() {
                return Err(oob());
            }
            Ok(b[index] != 0)
        })
    }

    /// Return the subset of `rows` that are **not** tombstoned, preserving order
    /// (#168 scan filter). Arena twin of the native
    /// [`Tombstones::live_indices`](../../forgedb_storage_native).
    pub fn live_indices(&self, rows: &[usize]) -> io::Result<Vec<usize>> {
        store::with_bytes(&self.path, |b| {
            Ok(rows
                .iter()
                .copied()
                .filter(|&r| b.get(r) == Some(&0))
                .collect())
        })
    }

    pub fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// No-op on the wasm arena backend (#153): there is no device barrier;
    /// durability comes from the transport writing arenas to IndexedDB/OPFS at
    /// the [`crate::store::dump`] boundary. Present for API parity with
    /// `storage-native`, since the generated `checkpoint()` calls it ungated.
    pub fn sync_to_drive(&self) -> io::Result<()> {
        Ok(())
    }

    /// No-op on the wasm arena backend (#153): see [`Self::sync_to_drive`].
    pub fn barrier(&self) -> io::Result<()> {
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        store::byte_len(&self.path)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn truncate_to_rows(&mut self, rows: usize) -> io::Result<()> {
        let cur = self.len();
        if rows > cur {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate_to_rows beyond current length",
            ));
        }
        // No-op truncation must not fault a lazy column in (see FixedColumn).
        if rows == cur {
            return Ok(());
        }
        store::with_bytes_mut(&self.path, |b| b.truncate(rows));
        Ok(())
    }

    /// API-parity no-op for the native `sync_from_disk` (#84 MVCC Tier 3). See
    /// `FixedColumn::sync_from_disk` — arena length is always live; present so
    /// the shared generated `database.rs` compiles for `wasm32`.
    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        Ok(())
    }

    pub fn reader(&self) -> io::Result<TombstonesReader> {
        Ok(TombstonesReader {
            path: self.path.clone(),
        })
    }
}

/// Read-only positional view over a [`FixedColumn`]'s arena. On this target a
/// reader is just another handle to the same keyed arena — single-threaded, so
/// there is no fd/lock story, but the API matches the native reader so the
/// generated single-writer/many-reader code links unchanged. Length is derived
/// live, so a reader observes appends made after it was created.
pub struct FixedColumnReader {
    path: PathBuf,
    value_size: usize,
}

impl FixedColumnReader {
    fn read_raw(&self, index: usize, n: usize) -> io::Result<Vec<u8>> {
        let off = index * self.value_size;
        store::with_bytes(&self.path, |b| {
            if off + n > b.len() {
                return Err(oob());
            }
            Ok(b[off..off + n].to_vec())
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        store::byte_len(&self.path) / self.value_size
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read_u32(&self, index: usize) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.read_raw(index, 4)?.try_into().unwrap()))
    }
    pub fn read_u64(&self, index: usize) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }
    pub fn read_i32(&self, index: usize) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.read_raw(index, 4)?.try_into().unwrap()))
    }
    pub fn read_i64(&self, index: usize) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }
    pub fn read_f64(&self, index: usize) -> io::Result<f64> {
        Ok(f64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }
    pub fn read_bool(&self, index: usize) -> io::Result<bool> {
        Ok(self.read_raw(index, 1)?[0] != 0)
    }
    pub fn read_uuid(&self, index: usize) -> io::Result<[u8; 16]> {
        Ok(self.read_raw(index, 16)?.try_into().unwrap())
    }
    pub fn read_timestamp(&self, index: usize) -> io::Result<i64> {
        Ok(i64::from_le_bytes(self.read_raw(index, 8)?.try_into().unwrap()))
    }
    pub fn read_bytes(&self, index: usize) -> io::Result<Vec<u8>> {
        self.read_raw(index, self.value_size)
    }
}

/// Read-only positional view over a [`VariableColumn`]'s arenas.
pub struct VariableColumnReader {
    data_path: PathBuf,
    offsets_path: PathBuf,
}

impl VariableColumnReader {
    #[must_use]
    pub fn len(&self) -> usize {
        store::byte_len(&self.offsets_path) / 16
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn read_string(&self, index: usize) -> io::Result<String> {
        let (offset, length) = read_offset_pair(&self.offsets_path, index)?;
        let data = store::with_bytes(&self.data_path, |b| {
            let end = offset + length;
            if end > b.len() {
                return Err(oob());
            }
            Ok(b[offset..end].to_vec())
        })?;
        String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// Read-only positional view over a [`Tombstones`] arena.
pub struct TombstonesReader {
    path: PathBuf,
}

impl TombstonesReader {
    #[must_use]
    pub fn len(&self) -> usize {
        store::byte_len(&self.path)
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn is_deleted(&self, index: usize) -> io::Result<bool> {
        store::with_bytes(&self.path, |b| {
            if index >= b.len() {
                return Err(oob());
            }
            Ok(b[index] != 0)
        })
    }
}
