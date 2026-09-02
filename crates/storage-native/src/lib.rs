pub use forgedb_wal::{FsyncPolicy, WalEntry, WalManager, WalOperation};

mod dir_lock;
pub use dir_lock::DirLock;

use std::fs::{self, File, OpenOptions};
use std::io::{self, IoSlice, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
fn fsync_to_drive(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    if unsafe { libc::fsync(file.as_raw_fd()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn fsync_to_drive(file: &File) -> io::Result<()> {
    file.sync_data()
}

#[cfg(target_os = "macos")]
fn device_barrier(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn device_barrier(file: &File) -> io::Result<()> {
    file.sync_all()
}

fn default_schema_version() -> u32 {
    1
}

fn default_engine_version() -> u32 {
    1
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    #[serde(rename = "format_version", default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_engine_version")]
    pub engine_version: u32,
    pub row_count: usize,
    pub columns: Vec<ColumnMetadata>,
    #[serde(default)]
    pub wal_enabled: bool,
    #[serde(default)]
    pub last_checkpoint: u64,
    #[serde(default)]
    pub compaction_epoch: u64,
    #[serde(default)]
    pub row_anchor: Option<RowAnchor>,
    #[serde(default)]
    pub auto_sequences: std::collections::BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RowAnchor {
    pub relative_path: String,
    pub bytes_per_row: usize,
}

impl Manifest {
    pub fn load_from(path: &std::path::Path) -> io::Result<Manifest> {
        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

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
    #[serde(default)]
    pub value_size: usize,
    #[serde(default)]
    pub kind: ColumnKind,
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
    FixedBytes(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    watermark: usize,
}

impl Snapshot {
    pub fn new(row_count: usize) -> Self {
        Self {
            watermark: row_count,
        }
    }

    pub fn watermark(&self) -> usize {
        self.watermark
    }

    pub fn visible(&self, index: usize) -> bool {
        index < self.watermark
    }
}

pub enum ColumnExport {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl ColumnExport {
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        match self {
            ColumnExport::Owned(v) => v.as_ptr(),
            ColumnExport::Mapped(m) => m.as_ptr(),
        }
    }

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

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            ColumnExport::Owned(v) => v.as_slice(),
            ColumnExport::Mapped(m) => &m[..],
        }
    }
}

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

    pub fn read_bytes(&self, slot: usize) -> io::Result<Vec<u8>> {
        Ok(self.slot_bytes(slot)?.to_vec())
    }

    pub fn read_slice(&self, slot: usize) -> io::Result<&[u8]> {
        self.slot_bytes(slot)
    }

    pub fn read_str(&self, slot: usize) -> io::Result<&str> {
        std::str::from_utf8(self.slot_bytes(slot)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

const GATHER_MMAP_MIN_ROWS: usize = 8;

pub struct FixedColumn {
    file: File,
    row_count: usize,
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

    pub fn gather(&self, indices: &[usize]) -> io::Result<Vec<u8>> {
        if indices.is_empty() {
            return Ok(Vec::new());
        }

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

    pub fn export(&self, indices: &[usize]) -> io::Result<ColumnExport> {
        let n = indices.len();
        let is_dense_prefix =
            n > 0 && n <= self.row_count && indices.iter().enumerate().all(|(i, &x)| x == i);

        if is_dense_prefix {
            let map_len = n * self.value_size;
            let mmap = unsafe { memmap2::MmapOptions::new().len(map_len).map(&self.file)? };
            return Ok(ColumnExport::Mapped(mmap));
        }

        Ok(ColumnExport::Owned(self.gather(indices)?))
    }

    pub fn gather_buffered(&self, indices: &[usize]) -> io::Result<BufferedFixedColumn> {
        Ok(BufferedFixedColumn {
            buf: self.export(indices)?,
            value_size: self.value_size,
        })
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    pub fn sync_to_drive(&self) -> io::Result<()> {
        fsync_to_drive(&self.file)
    }

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

    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        self.row_count = self.file.metadata()?.len() as usize / self.value_size;
        Ok(())
    }

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

    pub fn reader(&self) -> io::Result<FixedColumnReader> {
        Ok(FixedColumnReader {
            file: self.file.try_clone()?,
            value_size: self.value_size,
        })
    }
}

pub struct BufferedVariableColumn {
    data: ColumnExport,
    base: u64,
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

    pub fn read_str(&self, slot: usize) -> io::Result<&str> {
        let &(offset, length) = self.slots.get(slot).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Slot out of bounds")
        })?;
        let start = offset.saturating_sub(self.base) as usize;
        let end = start + length as usize;
        let bytes = self.data.as_slice().get(start..end).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Variable slot out of data bounds")
        })?;
        std::str::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn read_string(&self, slot: usize) -> io::Result<String> {
        self.read_str(slot).map(str::to_owned)
    }
}

const VAR_MMAP_MIN_BYTES: usize = 64 * 1024;

const SPARSE_OFFSETS_SPAN_FACTOR: usize = 128;

pub struct VariableColumn {
    data_file: File,
    offsets_file: File,
    row_count: usize,
    current_data_offset: u64,
}

impl VariableColumn {
    pub fn new(data_path: PathBuf, offsets_path: PathBuf) -> io::Result<Self> {
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
        let row_count = offsets_file.metadata()?.len() as usize / 16;

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

        self.data_file.seek(SeekFrom::End(0))?;
        self.data_file.write_all(bytes)?;

        self.offsets_file.seek(SeekFrom::End(0))?;
        self.offsets_file
            .write_all(&self.current_data_offset.to_le_bytes())?;
        self.offsets_file.write_all(&length.to_le_bytes())?;

        self.current_data_offset += length;
        self.row_count += 1;

        Ok(())
    }

    pub fn append_tagged(&mut self, tag: u8, value: &str) -> io::Result<()> {
        let bytes = value.as_bytes();
        let length = bytes.len() as u64 + 1;

        self.data_file.seek(SeekFrom::End(0))?;
        let tag = [tag];
        let mut slices = [IoSlice::new(&tag), IoSlice::new(bytes)];
        let mut slices: &mut [IoSlice<'_>] = &mut slices;
        while !slices.is_empty() {
            let written = self.data_file.write_vectored(slices)?;
            if written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole tagged value",
                ));
            }
            IoSlice::advance_slices(&mut slices, written);
        }

        let mut entry = [0u8; 16];
        entry[..8].copy_from_slice(&self.current_data_offset.to_le_bytes());
        entry[8..].copy_from_slice(&length.to_le_bytes());
        self.offsets_file.seek(SeekFrom::End(0))?;
        self.offsets_file.write_all(&entry)?;

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

        let offsets_pos = (index * 16) as u64;
        let mut offset_buf = [0u8; 8];
        let mut length_buf = [0u8; 8];
        self.offsets_file.read_exact_at(&mut offset_buf, offsets_pos)?;
        self.offsets_file
            .read_exact_at(&mut length_buf, offsets_pos + 8)?;

        let offset = u64::from_le_bytes(offset_buf);
        let length = u64::from_le_bytes(length_buf);

        let mut data = vec![0u8; length as usize];
        self.data_file.read_exact_at(&mut data, offset)?;

        String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn gather_buffered(&self, indices: &[usize]) -> io::Result<BufferedVariableColumn> {
        if indices.is_empty() {
            return Ok(BufferedVariableColumn {
                data: ColumnExport::Owned(Vec::new()),
                base: 0,
                slots: Vec::new(),
            });
        }

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

        if hi + 1 - lo > indices.len().saturating_mul(SPARSE_OFFSETS_SPAN_FACTOR) {
            return self.gather_sparse(indices);
        }

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

        if data_hi <= data_lo {
            return Ok(BufferedVariableColumn {
                data: ColumnExport::Owned(Vec::new()),
                base: data_lo.min(data_hi),
                slots,
            });
        }

        let span = (data_hi - data_lo) as usize;
        let data = if span >= VAR_MMAP_MIN_BYTES {
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

    fn gather_sparse(&self, indices: &[usize]) -> io::Result<BufferedVariableColumn> {
        let mut slots = Vec::with_capacity(indices.len());
        let mut data: Vec<u8> = Vec::new();
        let mut entry = [0u8; 16];
        for &index in indices {
            self.offsets_file
                .read_exact_at(&mut entry, (index * 16) as u64)?;
            let offset = u64::from_le_bytes(entry[..8].try_into().unwrap());
            let length = u64::from_le_bytes(entry[8..].try_into().unwrap());
            let start = data.len();
            data.resize(start + length as usize, 0);
            if length > 0 {
                self.data_file.read_exact_at(&mut data[start..], offset)?;
            }
            slots.push((start as u64, length));
        }
        Ok(BufferedVariableColumn {
            data: ColumnExport::Owned(data),
            base: 0,
            slots,
        })
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.data_file.sync_all()?;
        self.offsets_file.sync_all()
    }

    pub fn sync_to_drive(&self) -> io::Result<()> {
        fsync_to_drive(&self.data_file)?;
        fsync_to_drive(&self.offsets_file)
    }

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

    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        self.row_count = self.offsets_file.metadata()?.len() as usize / 16;
        self.current_data_offset = self.data_file.metadata()?.len();
        Ok(())
    }

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

    pub fn reader(&self) -> io::Result<VariableColumnReader> {
        Ok(VariableColumnReader {
            data_file: self.data_file.try_clone()?,
            offsets_file: self.offsets_file.try_clone()?,
        })
    }
}

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

    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    pub fn sync_to_drive(&self) -> io::Result<()> {
        fsync_to_drive(&self.file)
    }

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

    pub fn sync_from_disk(&mut self) -> io::Result<()> {
        self.count = self.file.metadata()?.len() as usize;
        Ok(())
    }

    pub fn reader(&self) -> io::Result<TombstonesReader> {
        Ok(TombstonesReader {
            file: self.file.try_clone()?,
        })
    }
}

fn map_out_of_bounds(e: io::Error) -> io::Error {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        io::Error::new(io::ErrorKind::InvalidInput, "Index out of bounds")
    } else {
        e
    }
}

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
