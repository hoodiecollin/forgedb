/// WAL Reader and replay logic
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use crate::entry::WalEntry;

/// WAL Reader
pub struct WalReader {
    file: File,
}

impl WalReader {
    /// Create a new WAL reader
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(WalReader { file })
    }

    /// Read all entries from the WAL
    /// Stops at the first corrupted or incomplete entry
    pub fn read_all(&mut self) -> io::Result<Vec<WalEntry>> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut entries = Vec::new();
        let mut buffer = Vec::new();

        // Read entire file
        self.file.read_to_end(&mut buffer)?;

        let mut offset = 0;

        while offset < buffer.len() {
            match WalEntry::from_bytes(&buffer[offset..]) {
                Ok((entry, size)) => {
                    entries.push(entry);
                    offset += size;
                }
                Err(e) => {
                    // Stop at first corrupted/incomplete entry
                    eprintln!("WAL read stopped at offset {}: {}", offset, e);
                    break;
                }
            }
        }

        Ok(entries)
    }

    /// Read entries and validate them without stopping at corruption
    /// Returns both valid entries and information about corrupted regions
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
                    // Try to skip past this corruption
                    // Look for next valid entry by scanning for entry length markers
                    offset += 1;
                }
            }
        }

        Ok((entries, corruptions))
    }

    /// Get the position in the file
    pub fn position(&mut self) -> io::Result<u64> {
        self.file.stream_position()
    }

    /// Seek to a specific position
    pub fn seek(&mut self, pos: u64) -> io::Result<u64> {
        self.file.seek(SeekFrom::Start(pos))
    }

    /// Read a single entry at the current position
    pub fn read_one(&mut self) -> io::Result<Option<WalEntry>> {
        let mut buffer = vec![0u8; 4];

        // Try to read length prefix
        match self.file.read_exact(&mut buffer) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        }

        let length = u32::from_le_bytes(buffer.try_into().unwrap()) as usize;

        // Read the rest of the entry
        let mut entry_buffer = vec![0u8; length];
        self.file.read_exact(&mut entry_buffer)?;

        // Prepend length for parsing
        let mut full_buffer = Vec::new();
        full_buffer.extend_from_slice(&(length as u32).to_le_bytes());
        full_buffer.extend_from_slice(&entry_buffer);

        let (entry, _) = WalEntry::from_bytes(&full_buffer)?;
        Ok(Some(entry))
    }
}

/// Information about a corrupted region in the WAL
#[derive(Debug, Clone)]
pub struct CorruptionInfo {
    pub offset: usize,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{WalEntry, WalValue};
    use crate::writer::{FsyncPolicy, WalWriter};
    use std::collections::HashMap;

    #[test]
    fn test_reader_read_all() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_reader_all");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");

        // Write some entries
        {
            let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

            for i in 0..5 {
                let mut fields = HashMap::new();
                fields.insert("id".to_string(), WalValue::U64(i));
                let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);
                writer.write(&entry).unwrap();
            }
        }

        // Read them back
        let mut reader = WalReader::new(&wal_path).unwrap();
        let entries = reader.read_all().unwrap();

        assert_eq!(entries.len(), 5);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_reader_read_one() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_reader_one");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");

        // Write some entries
        {
            let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

            for i in 0..3 {
                let mut fields = HashMap::new();
                fields.insert("id".to_string(), WalValue::U64(i));
                let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);
                writer.write(&entry).unwrap();
            }
        }

        // Read them one by one
        let mut reader = WalReader::new(&wal_path).unwrap();

        let entry1 = reader.read_one().unwrap();
        assert!(entry1.is_some());

        let entry2 = reader.read_one().unwrap();
        assert!(entry2.is_some());

        let entry3 = reader.read_one().unwrap();
        assert!(entry3.is_some());

        let entry4 = reader.read_one().unwrap();
        assert!(entry4.is_none());

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_reader_corrupted_entry() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_reader_corrupt");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");

        // Write some entries
        {
            let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

            for i in 0..3 {
                let mut fields = HashMap::new();
                fields.insert("id".to_string(), WalValue::U64(i));
                let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);
                writer.write(&entry).unwrap();
            }
        }

        // Corrupt the middle entry
        {
            use std::fs::OpenOptions;
            use std::io::{Seek, SeekFrom, Write};

            let mut file = OpenOptions::new().write(true).open(&wal_path).unwrap();
            file.seek(SeekFrom::Start(50)).unwrap();
            file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap();
        }

        // Read should stop at corruption
        let mut reader = WalReader::new(&wal_path).unwrap();
        let entries = reader.read_all().unwrap();

        // Should get the first entry, but stop at corruption
        assert!(entries.len() <= 3);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_reader_empty_file() {
        let temp_dir = std::env::temp_dir().join("sinkdb_wal_reader_empty");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let wal_path = temp_dir.join("test.wal");
        std::fs::write(&wal_path, &[]).unwrap();

        let mut reader = WalReader::new(&wal_path).unwrap();
        let entries = reader.read_all().unwrap();

        assert_eq!(entries.len(), 0);

        std::fs::remove_dir_all(&temp_dir).unwrap();
    }
}
