//! Write-Ahead Log (WAL) Implementation
//!
//! `forgedb-wal` is a **schema-agnostic substrate** crate. It stores and
//! returns opaque byte records and knows nothing about any application schema.
//!
//! ## Wire format
//!
//! Each entry on disk:
//! ```text
//! [4 bytes: total entry length (excluding this field)]
//! [1 byte: operation type]
//! [2 bytes: model name length]
//! [N bytes: model name (UTF-8, opaque routing tag)]
//! [M bytes: serialized operation data]
//! [4 bytes: CRC32 checksum]
//! ```
//!
//! Only one operation type exists:
//! - `Raw` (`0x20`): `[payload_len (4 bytes LE)][payload_bytes...]`
//!
//! The WAL breaks at the first corrupt or incomplete entry and returns the
//! valid prefix (torn-tail crash safety).
use std::io;
use std::path::{Path, PathBuf};

mod entry;
mod reader;
mod writer;

pub use entry::{WalEntry, WalOperation};
pub use reader::{CorruptionInfo, WalReader};
pub use writer::{FsyncPolicy, WalWriter};

/// WAL Manager — high-level interface for WAL operations.
///
/// Wraps a [`WalWriter`] and [`WalReader`] over a single WAL file. The
/// manager provides the crash-recovery entry point ([`replay`]) and common
/// lifecycle operations (`flush`, `rotate`, `truncate`).
///
/// [`replay`]: WalManager::replay
pub struct WalManager {
    writer: WalWriter,
    reader: WalReader,
    path: PathBuf,
}

impl WalManager {
    /// Open or create a WAL at `path`.
    ///
    /// Creates any missing parent directories. The WAL file is opened for
    /// append-only writes; an existing file is not truncated.
    pub fn open<P: AsRef<Path>>(path: P, fsync_policy: FsyncPolicy) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let writer = WalWriter::new(&path, fsync_policy)?;
        let reader = WalReader::new(&path)?;

        Ok(WalManager { writer, reader, path })
    }

    /// Append an entry to the WAL.
    pub fn write(&mut self, entry: &WalEntry) -> io::Result<()> {
        self.writer.write(entry)
    }

    /// Flush buffered bytes to disk (fsync).
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    /// Replay every entry from the WAL in file order, passing each to
    /// `callback`.
    ///
    /// The replay is **schema-blind**: entries are returned as `WalEntry`
    /// values carrying opaque `Raw` payloads. No decode step is applied.
    /// Stops at the first corrupt or incomplete entry (torn tail) and returns
    /// the entries recovered up to that point.
    pub fn replay<F>(&mut self, mut callback: F) -> io::Result<Vec<WalEntry>>
    where
        F: FnMut(&WalEntry) -> io::Result<()>,
    {
        let entries = self.reader.read_all()?;

        for entry in &entries {
            callback(entry)?;
        }

        Ok(entries)
    }

    /// Truncate the WAL, removing all entries.
    pub fn truncate(&mut self) -> io::Result<()> {
        self.writer.truncate()?;
        self.reader = WalReader::new(&self.path)?;
        Ok(())
    }

    /// Rotate the WAL: archive the current file under a timestamped name and
    /// start a fresh, empty WAL at the original path.
    ///
    /// Returns the path of the archived WAL file.
    pub fn rotate(&mut self) -> io::Result<PathBuf> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let archive_path = self.path.with_extension(format!("log.{}", timestamp));

        self.writer.flush()?;
        std::fs::rename(&self.path, &archive_path)?;

        self.writer = WalWriter::new(&self.path, self.writer.fsync_policy())?;
        self.reader = WalReader::new(&self.path)?;

        Ok(archive_path)
    }

    /// Returns `true` if the WAL file contains no entries (file length is 0).
    pub fn is_empty(&self) -> io::Result<bool> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len() == 0)
    }

    /// Returns the size of the WAL file in bytes.
    pub fn size(&self) -> io::Result<u64> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len())
    }
}
