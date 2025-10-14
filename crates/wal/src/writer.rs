/// WAL Writer with configurable fsync policies
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::entry::WalEntry;

/// Fsync policy determines when to flush WAL to disk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsyncPolicy {
    /// Fsync after every write (maximum durability, slower)
    Always,
    /// Fsync periodically based on time
    Periodic(Duration),
    /// Never fsync automatically (fastest, less durable - must call flush manually)
    Never,
}

/// WAL Writer
pub struct WalWriter {
    file: File,
    fsync_policy: FsyncPolicy,
    last_fsync: Instant,
    bytes_since_fsync: usize,
}

impl WalWriter {
    /// Create a new WAL writer
    pub fn new<P: AsRef<Path>>(path: P, fsync_policy: FsyncPolicy) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;

        Ok(WalWriter {
            file,
            fsync_policy,
            last_fsync: Instant::now(),
            bytes_since_fsync: 0,
        })
    }

    /// Write an entry to the WAL
    pub fn write(&mut self, entry: &WalEntry) -> io::Result<()> {
        let bytes = entry.to_bytes();
        self.file.write_all(&bytes)?;
        self.bytes_since_fsync += bytes.len();

        // Apply fsync policy
        match self.fsync_policy {
            FsyncPolicy::Always => {
                self.file.sync_all()?;
                self.last_fsync = Instant::now();
                self.bytes_since_fsync = 0;
            }
            FsyncPolicy::Periodic(duration) => {
                if self.last_fsync.elapsed() >= duration {
                    self.file.sync_all()?;
                    self.last_fsync = Instant::now();
                    self.bytes_since_fsync = 0;
                }
            }
            FsyncPolicy::Never => {
                // No automatic fsync
            }
        }

        Ok(())
    }

    /// Manually flush the WAL to disk
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()?;
        self.last_fsync = Instant::now();
        self.bytes_since_fsync = 0;
        Ok(())
    }

    /// Truncate the WAL (clear all entries)
    pub fn truncate(&mut self) -> io::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.sync_all()?;
        self.last_fsync = Instant::now();
        self.bytes_since_fsync = 0;
        Ok(())
    }

    /// Get the current fsync policy
    pub fn fsync_policy(&self) -> FsyncPolicy {
        self.fsync_policy
    }

    /// Get bytes written since last fsync
    pub fn bytes_since_fsync(&self) -> usize {
        self.bytes_since_fsync
    }

    /// Get time since last fsync
    pub fn time_since_fsync(&self) -> Duration {
        self.last_fsync.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::WalValue;
    use std::collections::HashMap;

    #[test]
    fn test_writer_always_fsync() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_writer_always");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), WalValue::String("test@example.com".to_string()));
        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        writer.write(&entry).unwrap();
        assert_eq!(writer.bytes_since_fsync(), 0); // Should be 0 after Always fsync

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_writer_never_fsync() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_writer_never");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Never).unwrap();

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), WalValue::String("test@example.com".to_string()));
        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        writer.write(&entry).unwrap();
        assert!(writer.bytes_since_fsync() > 0); // Should have unflushed bytes

        writer.flush().unwrap();
        assert_eq!(writer.bytes_since_fsync(), 0); // Should be 0 after manual flush

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_writer_periodic_fsync() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_writer_periodic");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Periodic(Duration::from_millis(100))).unwrap();

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), WalValue::String("test@example.com".to_string()));
        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        // First write shouldn't fsync immediately
        writer.write(&entry).unwrap();
        assert!(writer.bytes_since_fsync() > 0);

        // Wait for period to elapse
        std::thread::sleep(Duration::from_millis(150));

        // Next write should trigger fsync
        writer.write(&entry).unwrap();
        assert_eq!(writer.bytes_since_fsync(), 0);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_writer_truncate() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_writer_truncate");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), WalValue::String("test@example.com".to_string()));
        let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

        writer.write(&entry).unwrap();

        let metadata = std::fs::metadata(&wal_path).unwrap();
        assert!(metadata.len() > 0);

        writer.truncate().unwrap();

        let metadata = std::fs::metadata(&wal_path).unwrap();
        assert_eq!(metadata.len(), 0);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }
}
