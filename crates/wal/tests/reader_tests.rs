use forgedb_wal::*;


use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

#[test]
fn test_reader_read_all() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_all");
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
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_one");
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
    let temp_dir = std::env::temp_dir().join("forgedb_wal_reader_corrupt");
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
