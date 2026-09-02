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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsyncPolicy {
    Always,
    Periodic(std::time::Duration),
    Never,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct WalManager {
    writer: WalWriter,
    reader: WalReader,
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl WalManager {
    pub fn open<P: AsRef<Path>>(path: P, fsync_policy: FsyncPolicy) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let writer = WalWriter::new(&path, fsync_policy)?;
        let reader = WalReader::new(&path)?;

        Ok(WalManager { writer, reader, path })
    }

    pub fn write(&mut self, entry: &WalEntry) -> io::Result<()> {
        self.writer.write(entry)
    }

    pub fn write_buffered(&mut self, entry: &WalEntry) -> io::Result<()> {
        self.writer.write_buffered(entry)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

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

    pub fn truncate(&mut self) -> io::Result<()> {
        self.writer.truncate()?;
        self.reader = WalReader::new(&self.path)?;
        Ok(())
    }

    pub fn truncate_to(&mut self, offset: u64) -> io::Result<()> {
        self.writer.truncate_to(offset)?;
        self.reader = WalReader::new(&self.path)?;
        Ok(())
    }

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

    pub fn is_empty(&self) -> io::Result<bool> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len() == 0)
    }

    pub fn size(&self) -> io::Result<u64> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len())
    }
}

#[cfg(target_arch = "wasm32")]
pub struct WalManager {
    buffer: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
impl WalManager {
    pub fn open<P: AsRef<Path>>(_path: P, _fsync_policy: FsyncPolicy) -> io::Result<Self> {
        Ok(WalManager { buffer: Vec::new() })
    }

    pub fn write(&mut self, entry: &WalEntry) -> io::Result<()> {
        self.buffer.extend_from_slice(&entry.to_bytes());
        Ok(())
    }

    pub fn write_buffered(&mut self, entry: &WalEntry) -> io::Result<()> {
        self.write(entry)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

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

    pub fn truncate(&mut self) -> io::Result<()> {
        self.buffer.clear();
        Ok(())
    }

    pub fn truncate_to(&mut self, offset: u64) -> io::Result<()> {
        let offset = offset as usize;
        if offset < self.buffer.len() {
            self.buffer.truncate(offset);
        }
        Ok(())
    }

    pub fn is_empty(&self) -> io::Result<bool> {
        Ok(self.buffer.is_empty())
    }

    pub fn size(&self) -> io::Result<u64> {
        Ok(self.buffer.len() as u64)
    }
}
