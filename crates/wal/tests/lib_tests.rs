use forgedb_wal::*;

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

#[test]
fn test_raw_entries_survive_write_flush_reopen_replay() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_test_raw_mixed");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");

    let payload_a: Vec<u8> = vec![0x01, 0x02, 0x03, 0x00, 0xFF];
    let payload_b: Vec<u8> = vec![];

    {
        let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

        wal.write(&WalEntry::raw("Post", payload_a.clone())).unwrap();
        wal.write(&WalEntry::raw("Comment", payload_b.clone()))
            .unwrap();

        wal.flush().unwrap();
    }

    {
        let mut wal = WalManager::open(&wal_path, FsyncPolicy::Never).unwrap();
        let entries = wal.replay(|_| Ok(())).unwrap();

        assert_eq!(entries.len(), 2, "should replay both entries in order");

        match &entries[0].operation {
            WalOperation::Raw { payload } => {
                assert_eq!(payload, &payload_a, "payload_a must be byte-identical");
            }
        }
        assert_eq!(entries[0].model_name, "Post");

        match &entries[1].operation {
            WalOperation::Raw { payload } => {
                assert!(payload.is_empty(), "payload_b must be empty");
            }
        }
        assert_eq!(entries[1].model_name, "Comment");
    }

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_wal_basic_write_read_raw() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_test_basic_raw");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

    let payload = b"serialized row bytes".to_vec();
    let entry = WalEntry::raw("User", payload.clone());

    wal.write(&entry).unwrap();

    let entries = wal.replay(|_| Ok(())).unwrap();
    assert_eq!(entries.len(), 1);
    match &entries[0].operation {
        WalOperation::Raw { payload: p } => assert_eq!(p, &payload),
    }
    assert_eq!(entries[0].model_name, "User");

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_wal_rotate() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_test_rotate");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

    wal.write(&WalEntry::raw("Model", b"row".to_vec())).unwrap();
    assert!(!wal.is_empty().unwrap());

    let archive = wal.rotate().unwrap();
    assert!(archive.exists());
    assert!(wal.is_empty().unwrap());

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_wal_size_and_truncate_to() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_test_truncate_to");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wal_path = temp_dir.join("test.wal");

    let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();
    wal.write(&WalEntry::raw("m", b"committed-a".to_vec())).unwrap();
    wal.write(&WalEntry::raw("m", b"committed-b".to_vec())).unwrap();
    let mark = wal.size().unwrap();
    assert!(mark > 0, "size() reflects the committed prefix size");

    wal.write(&WalEntry::raw("m", b"staged-c".to_vec())).unwrap();
    wal.write(&WalEntry::raw("m", b"staged-d".to_vec())).unwrap();
    assert!(wal.size().unwrap() > mark, "staged records grew the WAL");

    wal.truncate_to(mark).unwrap();
    assert_eq!(wal.size().unwrap(), mark, "truncate_to rolled back to the mark");
    let entries = wal.replay(|_| Ok(())).unwrap();
    assert_eq!(entries.len(), 2, "only the committed prefix survives");
    if let WalOperation::Raw { payload } = &entries[0].operation {
        assert_eq!(payload, b"committed-a");
    } else {
        panic!("expected Raw");
    }
    if let WalOperation::Raw { payload } = &entries[1].operation {
        assert_eq!(payload, b"committed-b");
    } else {
        panic!("expected Raw");
    }

    let now = wal.size().unwrap();
    wal.truncate_to(now + 100).unwrap();
    assert_eq!(wal.size().unwrap(), now, "truncate_to past the end is a no-op");

    wal.write(&WalEntry::raw("m", b"new-e".to_vec())).unwrap();
    let entries = wal.replay(|_| Ok(())).unwrap();
    assert_eq!(entries.len(), 3, "a post-truncate write appends cleanly");

    std::fs::remove_dir_all(&temp_dir).unwrap();
}
