/// Sprint 7: Write-Ahead Log (WAL) Implementation
///
/// This module provides ACID properties and crash recovery through a WAL system.
///
/// Architecture:
/// - WAL entries are written before modifying data files
/// - Each entry has: [length][type][model][data][checksum]
/// - Checksums detect corruption
/// - WAL replay on startup ensures durability
/// - Transactions group operations with atomic commit
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

mod entry;
mod reader;
mod transaction;
mod writer;

pub use entry::{WalEntry, WalOperation, WalValue};
pub use reader::WalReader;
pub use transaction::{Transaction, TransactionId};
pub use writer::{FsyncPolicy, WalWriter};

/// WAL file format:
///
/// Each entry:
/// [4 bytes: total entry length (excluding this field)]
/// [1 byte: operation type]
/// [2 bytes: model name length]
/// [N bytes: model name (UTF-8)]
/// [M bytes: serialized data (varies by operation)]
/// [4 bytes: CRC32 checksum]
///
/// Operations:
/// - Insert (0x01): [record_id (16 bytes UUID)] [field_count] [field_data...]
/// - Update (0x02): [record_id] [field_count] [field_data...]
/// - Delete (0x03): [record_id]
/// - BeginTxn (0x10): [txn_id (8 bytes)]
/// - CommitTxn (0x11): [txn_id]
/// - RollbackTxn (0x12): [txn_id]

/// WAL Manager - High-level interface for WAL operations
pub struct WalManager {
    writer: WalWriter,
    reader: WalReader,
    path: PathBuf,
}

impl WalManager {
    /// Open or create a WAL at the given path
    pub fn open<P: AsRef<Path>>(path: P, fsync_policy: FsyncPolicy) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let writer = WalWriter::new(&path, fsync_policy)?;
        let reader = WalReader::new(&path)?;

        Ok(WalManager {
            writer,
            reader,
            path,
        })
    }

    /// Write an entry to the WAL
    pub fn write(&mut self, entry: &WalEntry) -> io::Result<()> {
        self.writer.write(entry)
    }

    /// Flush the WAL to disk (fsync)
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    /// Replay all entries from the WAL
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

    /// Truncate the WAL (remove all entries)
    pub fn truncate(&mut self) -> io::Result<()> {
        self.writer.truncate()?;
        self.reader = WalReader::new(&self.path)?;
        Ok(())
    }

    /// Rotate the WAL (archive current and start fresh)
    pub fn rotate(&mut self) -> io::Result<PathBuf> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let archive_path = self.path.with_extension(format!("log.{}", timestamp));

        // Close current writer
        self.writer.flush()?;

        // Rename current file
        std::fs::rename(&self.path, &archive_path)?;

        // Create new WAL
        self.writer = WalWriter::new(&self.path, self.writer.fsync_policy())?;
        self.reader = WalReader::new(&self.path)?;

        Ok(archive_path)
    }

    /// Check if WAL is empty
    pub fn is_empty(&self) -> io::Result<bool> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len() == 0)
    }

    /// Get the size of the WAL in bytes
    pub fn size(&self) -> io::Result<u64> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_wal_basic_write_read() {
        let temp_dir = std::env::temp_dir().join("forgedb_wal_test_basic");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

        // Write an insert entry
        let mut fields = HashMap::new();
        fields.insert(
            "email".to_string(),
            WalValue::String("test@example.com".to_string()),
        );

        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        wal.write(&entry).unwrap();

        // Read back
        let entries = wal.replay(|_| Ok(())).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].operation, WalOperation::Insert { .. }));

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_wal_empty() {
        let temp_dir = std::env::temp_dir().join("forgedb_wal_test_empty");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

        assert!(wal.is_empty().unwrap());

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }
}
