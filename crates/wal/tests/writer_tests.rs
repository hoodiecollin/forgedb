use forgedb_wal::*;
use std::time::Duration;

#[test]
fn test_writer_always_fsync() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_always");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

    let entry = WalEntry::raw("User", b"row bytes".to_vec());
    writer.write(&entry).unwrap();
    assert_eq!(writer.bytes_since_fsync(), 0); // flushed immediately

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_writer_never_fsync() {
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_never");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("test.wal");
    let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Never).unwrap();

    let entry = WalEntry::raw("User", b"row bytes".to_vec());
    writer.write(&entry).unwrap();
    assert!(writer.bytes_since_fsync() > 0); // unflushed bytes remain

    writer.flush().unwrap();
    assert_eq!(writer.bytes_since_fsync(), 0); // flushed after explicit call

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

    let entry = WalEntry::raw("User", b"row bytes".to_vec());

    // First write should not fsync immediately
    writer.write(&entry).unwrap();
    assert!(writer.bytes_since_fsync() > 0);

    // Wait for the period to elapse
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

    let entry = WalEntry::raw("User", b"row bytes".to_vec());
    writer.write(&entry).unwrap();

    let metadata = std::fs::metadata(&wal_path).unwrap();
    assert!(metadata.len() > 0);

    writer.truncate().unwrap();

    let metadata = std::fs::metadata(&wal_path).unwrap();
    assert_eq!(metadata.len(), 0);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_writer_write_buffered_defers_fsync_but_persists_after_flush() {
    // #170 group commit: write_buffered appends WITHOUT fsync (regardless of
    // policy Always), and an explicit flush makes the batch durable + replayable.
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_buffered");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("buffered.wal");
    let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

    // Buffered appends do not reset the fsync accounting (no sync happened).
    for i in 0..5u8 {
        writer
            .write_buffered(&WalEntry::raw("User", vec![i; 8]))
            .unwrap();
    }
    assert!(
        writer.bytes_since_fsync() > 0,
        "buffered appends leave bytes un-synced (no per-record fsync under Always)"
    );

    // The bytes are still in the file (write_all happened); one flush makes the
    // batch durable and resets the accounting.
    writer.flush().unwrap();
    assert_eq!(writer.bytes_since_fsync(), 0, "flush clears the un-synced accounting");

    // All five entries replay in order (a reopened reader sees the flushed batch).
    let mut reader = WalReader::new(&wal_path).unwrap();
    let entries = reader.read_all().unwrap();
    assert_eq!(entries.len(), 5, "all buffered-then-flushed entries are durable + replayable");
    assert!(entries.iter().all(|e| e.model_name == "User"));

    std::fs::remove_dir_all(&temp_dir).unwrap();
}
