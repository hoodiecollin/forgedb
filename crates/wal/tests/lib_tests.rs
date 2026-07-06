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
fn test_replay_committed_filters_transactions() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_test_replay_committed");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

    // Committed transaction: BEGIN, insert, COMMIT.
    wal.write(&WalEntry::begin_transaction(1)).unwrap();
    let mut f1 = HashMap::new();
    f1.insert(
        "email".to_string(),
        WalValue::String("keep@example.com".to_string()),
    );
    wal.write(&WalEntry::insert(
        "User".to_string(),
        uuid::Uuid::new_v4(),
        f1,
    ))
    .unwrap();
    wal.write(&WalEntry::commit_transaction(1)).unwrap();

    // Rolled-back transaction: BEGIN, insert, ROLLBACK.
    wal.write(&WalEntry::begin_transaction(2)).unwrap();
    let mut f2 = HashMap::new();
    f2.insert(
        "email".to_string(),
        WalValue::String("drop@example.com".to_string()),
    );
    wal.write(&WalEntry::insert(
        "User".to_string(),
        uuid::Uuid::new_v4(),
        f2,
    ))
    .unwrap();
    wal.write(&WalEntry::rollback_transaction(2)).unwrap();

    // Non-transactional insert is always applied.
    let mut f3 = HashMap::new();
    f3.insert(
        "email".to_string(),
        WalValue::String("plain@example.com".to_string()),
    );
    wal.write(&WalEntry::insert(
        "User".to_string(),
        uuid::Uuid::new_v4(),
        f3,
    ))
    .unwrap();
    wal.flush().unwrap();

    let applied = wal.replay_committed(|_| Ok(())).unwrap();
    // Committed insert + non-transactional insert = 2; the rolled-back insert is
    // skipped and transaction markers are excluded from the applied set.
    assert_eq!(applied.len(), 2);
    assert!(
        applied
            .iter()
            .all(|e| matches!(e.operation, WalOperation::Insert { .. }))
    );

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
