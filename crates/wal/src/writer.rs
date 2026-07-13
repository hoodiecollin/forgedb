//! WAL Writer with configurable fsync policies
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::FsyncPolicy;
use crate::entry::WalEntry;

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
