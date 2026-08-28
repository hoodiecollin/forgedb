use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::entry::WalEntry;

pub struct WalReader {
    file: File,
}

impl WalReader {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(WalReader { file })
    }

    pub fn read_all(&mut self) -> io::Result<Vec<WalEntry>> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut entries = Vec::new();
        let mut buffer = Vec::new();

        self.file.read_to_end(&mut buffer)?;

        let mut offset = 0;

        while offset < buffer.len() {
            match WalEntry::from_bytes(&buffer[offset..]) {
                Ok((entry, size)) => {
                    entries.push(entry);
                    offset += size;
                }
                Err(_) => {
                    break;
                }
            }
        }

        Ok(entries)
    }

    pub fn read_with_validation(&mut self) -> io::Result<(Vec<WalEntry>, Vec<CorruptionInfo>)> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut entries = Vec::new();
        let mut corruptions = Vec::new();
        let mut buffer = Vec::new();

        self.file.read_to_end(&mut buffer)?;

        let mut offset = 0;

        while offset < buffer.len() {
            match WalEntry::from_bytes(&buffer[offset..]) {
                Ok((entry, size)) => {
                    entries.push(entry);
                    offset += size;
                }
                Err(e) => {
                    corruptions.push(CorruptionInfo {
                        offset,
                        error: e.to_string(),
                    });
                    offset += 1;
                }
            }
        }

        Ok((entries, corruptions))
    }

    pub fn position(&mut self) -> io::Result<u64> {
        self.file.stream_position()
    }

    pub fn seek(&mut self, pos: u64) -> io::Result<u64> {
        self.file.seek(SeekFrom::Start(pos))
    }

    pub fn read_one(&mut self) -> io::Result<Option<WalEntry>> {
        let mut buffer = vec![0u8; 4];

        match self.file.read_exact(&mut buffer) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        }

        let length = u32::from_le_bytes(buffer.try_into().unwrap()) as usize;

        let position = self.file.stream_position()?;
        let file_len = self.file.metadata()?.len();
        let remaining = file_len.saturating_sub(position);
        if length as u64 > remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "WAL entry length exceeds remaining file size",
            ));
        }

        let mut entry_buffer = vec![0u8; length];
        self.file.read_exact(&mut entry_buffer)?;

        let mut full_buffer = Vec::new();
        full_buffer.extend_from_slice(&(length as u32).to_le_bytes());
        full_buffer.extend_from_slice(&entry_buffer);

        let (entry, _) = WalEntry::from_bytes(&full_buffer)?;
        Ok(Some(entry))
    }
}

#[derive(Debug, Clone)]
pub struct CorruptionInfo {
    pub offset: usize,
    pub error: String,
}
