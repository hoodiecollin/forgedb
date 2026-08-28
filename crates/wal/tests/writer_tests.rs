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
    assert_eq!(writer.bytes_since_fsync(), 0);

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
    assert!(writer.bytes_since_fsync() > 0);

    writer.flush().unwrap();
    assert_eq!(writer.bytes_since_fsync(), 0);

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

    writer.write(&entry).unwrap();
    assert!(writer.bytes_since_fsync() > 0);

    std::thread::sleep(Duration::from_millis(150));

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
    let temp_dir = std::env::temp_dir().join("forgedb_wal_writer_buffered");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wal_path = temp_dir.join("buffered.wal");
    let mut writer = WalWriter::new(&wal_path, FsyncPolicy::Always).unwrap();

    for i in 0..5u8 {
        writer
            .write_buffered(&WalEntry::raw("User", vec![i; 8]))
            .unwrap();
    }
    assert!(
        writer.bytes_since_fsync() > 0,
        "buffered appends leave bytes un-synced (no per-record fsync under Always)"
    );

    writer.flush().unwrap();
    assert_eq!(writer.bytes_since_fsync(), 0, "flush clears the un-synced accounting");

    let mut reader = WalReader::new(&wal_path).unwrap();
    let entries = reader.read_all().unwrap();
    assert_eq!(entries.len(), 5, "all buffered-then-flushed entries are durable + replayable");
    assert!(entries.iter().all(|e| e.model_name == "User"));

    std::fs::remove_dir_all(&temp_dir).unwrap();
}
