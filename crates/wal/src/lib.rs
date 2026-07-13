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
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(target_arch = "wasm32")]
use std::path::Path;

mod entry;
#[cfg(not(target_arch = "wasm32"))]
mod reader;
#[cfg(not(target_arch = "wasm32"))]
mod writer;

pub use entry::{WalEntry, WalOperation};
#[cfg(not(target_arch = "wasm32"))]
pub use reader::{CorruptionInfo, WalReader};
#[cfg(not(target_arch = "wasm32"))]
pub use writer::WalWriter;

/// Fsync policy determines when a native WAL is flushed to disk.
///
/// Defined at the crate root (not in the native-only `writer` module) because it
/// is part of the schema-agnostic substrate surface `forgedb-storage` re-exports
/// and generated code names (`FsyncPolicy::Always`) — it must exist on **both**
/// the native and the `wasm32` follower target. On `wasm32` there is no file WAL
/// (see the in-memory [`WalManager`] below), so the policy is inert there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsyncPolicy {
    /// Fsync after every write (maximum durability, slower).
    Always,
    /// Fsync periodically based on elapsed time.
    Periodic(std::time::Duration),
    /// Never fsync automatically (fastest, less durable — must call flush manually).
    Never,
}

/// WAL Manager — high-level interface for WAL operations.
///
/// Wraps a [`WalWriter`] and [`WalReader`] over a single WAL file. The
/// manager provides the crash-recovery entry point ([`replay`]) and common
/// lifecycle operations (`flush`, `rotate`, `truncate`).
///
/// [`replay`]: WalManager::replay
#[cfg(not(target_arch = "wasm32"))]
pub struct WalManager {
    writer: WalWriter,
    reader: WalReader,
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
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

/// In-memory `WalManager` for the `wasm32` browser read-replica target (#110).
///
/// The design note (`docs/proposals/wasm-runtime.md`) drops the **file** WAL on
/// wasm: the browser follower is durable at IndexedDB/OPFS *commit* granularity,
/// and its upstream (the server) is the authoritative durable log, so a
/// crash-recovery WAL is unnecessary there. But the generated `database.rs` is
/// emitted **once** and compiled for both targets — it unconditionally
/// constructs a `forgedb_wal::WalManager` in `new_at` and names `FsyncPolicy`.
///
/// So rather than fork the generator (forbidden — the data logic stays
/// byte-identical across targets), the *substrate* absorbs the difference: on
/// wasm the WAL is a functional **in-memory** append log with the same public
/// surface. It framing-encodes entries into a `Vec<u8>` exactly like the file
/// writer, replays them with the same torn-tail-safe parse, and truncates by
/// clearing the buffer. It never touches the (absent) wasm filesystem, so
/// `open`/`recover`/`checkpoint` all succeed; nothing persists across a reload
/// (that is the IDB/OPFS column commit's job, not the WAL's).
#[cfg(target_arch = "wasm32")]
pub struct WalManager {
    buffer: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
impl WalManager {
    /// Open an in-memory WAL. The `path` and `fsync_policy` are accepted for
    /// signature parity with the native constructor and then ignored — there is
    /// no file and no fsync on this target.
    pub fn open<P: AsRef<Path>>(_path: P, _fsync_policy: FsyncPolicy) -> io::Result<Self> {
        Ok(WalManager { buffer: Vec::new() })
    }

    /// Append an entry to the in-memory log (same framing as the file writer).
    pub fn write(&mut self, entry: &WalEntry) -> io::Result<()> {
        self.buffer.extend_from_slice(&entry.to_bytes());
        Ok(())
    }

    /// No-op flush — there is no disk to sync to on wasm.
    pub fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Replay every entry from the in-memory log in order, torn-tail-safe.
    ///
    /// Mirrors [`WalReader::read_all`]: parses framed entries from the buffer and
    /// stops at the first incomplete/corrupt frame, returning the valid prefix.
    pub fn replay<F>(&mut self, mut callback: F) -> io::Result<Vec<WalEntry>>
    where
        F: FnMut(&WalEntry) -> io::Result<()>,
    {
        let mut entries = Vec::new();
        let mut offset = 0;
        while offset < self.buffer.len() {
            match WalEntry::from_bytes(&self.buffer[offset..]) {
                Ok((entry, size)) => {
                    entries.push(entry);
                    offset += size;
                }
                Err(_) => break,
            }
        }
        for entry in &entries {
            callback(entry)?;
        }
        Ok(entries)
    }

    /// Clear the in-memory log.
    pub fn truncate(&mut self) -> io::Result<()> {
        self.buffer.clear();
        Ok(())
    }

    /// Returns `true` if the in-memory log holds no bytes.
    pub fn is_empty(&self) -> io::Result<bool> {
        Ok(self.buffer.is_empty())
    }

    /// Returns the in-memory log size in bytes.
    pub fn size(&self) -> io::Result<u64> {
        Ok(self.buffer.len() as u64)
    }
}
