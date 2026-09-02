use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::FsyncPolicy;
use crate::entry::WalEntry;

pub struct WalWriter {
    file: File,
    fsync_policy: FsyncPolicy,
    last_fsync: Instant,
    bytes_since_fsync: usize,
}

impl WalWriter {
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

    pub fn write(&mut self, entry: &WalEntry) -> io::Result<()> {
        let bytes = entry.to_bytes();
        self.file.write_all(&bytes)?;
        self.bytes_since_fsync += bytes.len();

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
            }
        }

        Ok(())
    }

    pub fn write_buffered(&mut self, entry: &WalEntry) -> io::Result<()> {
        let bytes = entry.to_bytes();
        self.file.write_all(&bytes)?;
        self.bytes_since_fsync += bytes.len();
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.file.sync_all()?;
        self.last_fsync = Instant::now();
        self.bytes_since_fsync = 0;
        Ok(())
    }

    pub fn truncate(&mut self) -> io::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.sync_all()?;
        self.last_fsync = Instant::now();
        self.bytes_since_fsync = 0;
        Ok(())
    }

    pub fn truncate_to(&mut self, offset: u64) -> io::Result<()> {
        let current = self.file.metadata()?.len();
        if offset >= current {
            return Ok(());
        }
        self.file.set_len(offset)?;
        self.file.sync_all()?;
        self.last_fsync = Instant::now();
        self.bytes_since_fsync = 0;
        Ok(())
    }

    pub fn fsync_policy(&self) -> FsyncPolicy {
        self.fsync_policy
    }

    pub fn bytes_since_fsync(&self) -> usize {
        self.bytes_since_fsync
    }

    pub fn time_since_fsync(&self) -> Duration {
        self.last_fsync.elapsed()
    }
}
