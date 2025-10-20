use forgedb_wal::*;
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
