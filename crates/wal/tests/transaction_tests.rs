use forgedb_wal::*;
use crate::entry::WalValue;
use crate::writer::FsyncPolicy;
use std::collections::HashMap;

#[test]
fn test_transaction_basic() {
    let mut txn = Transaction::begin();
    assert!(txn.is_active());
    assert_eq!(txn.len(), 0);

    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

    txn.add_entry(entry).unwrap();
    assert_eq!(txn.len(), 1);
}

#[test]
fn test_transaction_commit() {
    let temp_dir = std::env::temp_dir().join("forgedb_txn_commit");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut wal = crate::WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

    let mut txn = Transaction::begin();
    let txn_id = txn.id();

    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

    txn.add_entry(entry).unwrap();
    txn.commit(&mut wal).unwrap();

    // Verify WAL contains BEGIN, entry, and COMMIT
    let entries = wal.replay(|_| Ok(())).unwrap();

    assert_eq!(entries.len(), 3); // BEGIN + INSERT + COMMIT

    match &entries[0].operation {
        crate::entry::WalOperation::BeginTransaction { txn_id: id } => {
            assert_eq!(*id, txn_id);
        }
        _ => panic!("Expected BeginTransaction"),
    }

    match &entries[1].operation {
        crate::entry::WalOperation::Insert { .. } => {}
        _ => panic!("Expected Insert"),
    }

    match &entries[2].operation {
        crate::entry::WalOperation::CommitTransaction { txn_id: id } => {
            assert_eq!(*id, txn_id);
        }
        _ => panic!("Expected CommitTransaction"),
    }

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_transaction_rollback() {
    let temp_dir = std::env::temp_dir().join("forgedb_txn_rollback");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut wal = crate::WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

    let mut txn = Transaction::begin();
    let txn_id = txn.id();

    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        WalValue::String("test@example.com".to_string()),
    );
    let entry = WalEntry::insert("User".to_string(), uuid::Uuid::new_v4(), fields);

    txn.add_entry(entry).unwrap();
    txn.rollback(&mut wal).unwrap();

    // Verify WAL contains BEGIN and ROLLBACK
    let entries = wal.replay(|_| Ok(())).unwrap();

    assert_eq!(entries.len(), 2); // BEGIN + ROLLBACK

    match &entries[0].operation {
        crate::entry::WalOperation::BeginTransaction { txn_id: id } => {
            assert_eq!(*id, txn_id);
        }
        _ => panic!("Expected BeginTransaction"),
    }

    match &entries[1].operation {
        crate::entry::WalOperation::RollbackTransaction { txn_id: id } => {
            assert_eq!(*id, txn_id);
        }
        _ => panic!("Expected RollbackTransaction"),
    }

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_transaction_replay() {
    let mut replay = TransactionReplay::new();

    let txn_id = 1;

    // Process BEGIN
    let begin = WalEntry::begin_transaction(txn_id);
    replay.process_entry(&begin);
    assert!(replay.is_active(txn_id));

    // Process COMMIT
    let commit = WalEntry::commit_transaction(txn_id);
    replay.process_entry(&commit);
    assert!(replay.is_committed(txn_id));
    assert!(!replay.is_active(txn_id));
}

#[test]
fn test_transaction_empty_rollback() {
    let temp_dir = std::env::temp_dir().join("forgedb_txn_empty_rollback");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut wal = crate::WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

    let txn = Transaction::begin();
    assert!(txn.is_empty());

    txn.rollback(&mut wal).unwrap();

    // Empty transaction rollback should not write anything to WAL
    let entries = wal.replay(|_| Ok(())).unwrap();
    assert_eq!(entries.len(), 0);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}
