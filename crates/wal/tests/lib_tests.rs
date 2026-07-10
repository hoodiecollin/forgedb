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
    // Write two Raw entries, flush, reopen, replay.
    // Assert both come back in order with payloads byte-identical.
    let temp_dir = std::env::temp_dir().join("forgedb_wal_test_raw_mixed");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");

    let payload_a: Vec<u8> = vec![0x01, 0x02, 0x03, 0x00, 0xFF];
    let payload_b: Vec<u8> = vec![]; // empty payload

    // --- write phase ---
    {
        let mut wal = WalManager::open(&wal_path, FsyncPolicy::Always).unwrap();

        wal.write(&WalEntry::raw("Post", payload_a.clone())).unwrap();
        wal.write(&WalEntry::raw("Comment", payload_b.clone()))
            .unwrap();

        wal.flush().unwrap();
    }

    // --- replay phase (fresh WalManager, simulates process restart) ---
    {
        let mut wal = WalManager::open(&wal_path, FsyncPolicy::Never).unwrap();
        let entries = wal.replay(|_| Ok(())).unwrap();

        assert_eq!(entries.len(), 2, "should replay both entries in order");

        // First entry: Raw with payload_a
        match &entries[0].operation {
            WalOperation::Raw { payload } => {
                assert_eq!(payload, &payload_a, "payload_a must be byte-identical");
            }
        }
        assert_eq!(entries[0].model_name, "Post");

        // Second entry: Raw with empty payload
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
