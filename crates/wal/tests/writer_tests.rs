use forgedb_wal::*;
use std::collections::HashMap;
use std::time::Duration;

#[test]
fn test_writer_always_fsync() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_always");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

    writer.write(&entry).unwrap();
    assert_eq!(writer.bytes_since_fsync(), 0); // Should be 0 after Always fsync

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_writer_never_fsync() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_never");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Never).unwrap();

    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

    writer.write(&entry).unwrap();
    assert!(writer.bytes_since_fsync() > 0); // Should have unflushed bytes

    writer.flush().unwrap();
    assert_eq!(writer.bytes_since_fsync(), 0); // Should be 0 after manual flush

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_writer_periodic_fsync() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_periodic");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut writer =
        WalWriter::new(&wal_path, FsyncPolicy::Periodic(Duration::from_millis(100))).unwrap();

    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
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
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_truncate");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

    writer.write(&entry).unwrap();

    let metadata = std::fs::metadata(&wal_path).unwrap();
    assert!(metadata.len() > 0);

    writer.truncate().unwrap();

    let metadata = std::fs::metadata(&wal_path).unwrap();
    assert_eq!(metadata.len(), 0);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}
