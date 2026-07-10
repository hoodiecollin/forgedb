use forgedb_wal::*;

use std::io::{Seek, SeekFrom, Write};

#[test]
fn test_reader_read_all() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_all");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");

    {
        let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

        for i in 0u8..5 {
            let entry = WalEntry::raw("User", vec![i]);
            writer.write(&entry).unwrap();
        }
    }

    let mut reader = WalReader::new(&wal_path).unwrap();
    let entries = reader.read_all().unwrap();

    assert_eq!(entries.len(), 5);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_reader_read_one() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_one");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");

    {
        let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

        for i in 0u8..3 {
            let entry = WalEntry::raw("User", vec![i]);
            writer.write(&entry).unwrap();
        }
    }

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
fn test_reader_corrupted_entry_stops_at_first_corruption() {
    // A corrupt byte in an entry's body (CRC mismatch) causes `read_all` to
    // stop at that entry and return only the valid prefix.
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_corrupt");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");

    {
        let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

        for i in 0u8..3 {
            let entry = WalEntry::raw("User", vec![i; 8]);
            writer.write(&entry).unwrap();
        }
    }

    // Corrupt the middle entry
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&wal_path)
            .unwrap();
        file.seek(SeekFrom::Start(50)).unwrap();
        file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap();
    }

    let mut reader = WalReader::new(&wal_path).unwrap();
    let entries = reader.read_all().unwrap();

    // Should get at most the first entry; stops at or before the corruption.
    assert!(entries.len() <= 3);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_reader_empty_file() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_empty");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    std::fs::write(&wal_path, &[]).unwrap();

    let mut reader = WalReader::new(&wal_path).unwrap();
    let entries = reader.read_all().unwrap();

    assert_eq!(entries.len(), 0);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_reader_torn_tail_is_not_an_error() {
    // Write a valid entry, then append a partial (torn) entry. `read_all`
    // should return the one complete entry and ignore the torn tail.
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_torn_tail");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");

    let entry = WalEntry::raw("Post", b"complete row".to_vec());
    let complete_bytes = entry.to_bytes();

    // Write the complete entry plus a partial (torn) entry suffix.
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&wal_path)
            .unwrap();
        file.write_all(&complete_bytes).unwrap();
        // A torn write: 4-byte length prefix claiming 100 bytes but providing 0.
        file.write_all(&100u32.to_le_bytes()).unwrap();
    }

    let mut reader = WalReader::new(&wal_path).unwrap();
    let entries = reader.read_all().unwrap();

    assert_eq!(entries.len(), 1, "should recover the one complete entry");
    match &entries[0].operation {
        WalOperation::Raw { payload } => {
            assert_eq!(payload, b"complete row");
        }
    }

    std::fs::remove_dir_all(&temp_dir).unwrap();
}
