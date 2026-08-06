use forgedb_storage_native::*;
use std::fs;

// ---------------------------------------------------------------------------
// S4 durability: flush() tests
//
// These tests verify that data written to column files is readable after an
// explicit flush() call AND after reopening the file (persistent in the OS
// page cache, flushed to disk by flush()). They document the new contract:
// per-append fsync has been removed; flush() is the explicit durability point.
// ---------------------------------------------------------------------------

#[test]
fn test_fixed_column_explicit_flush_durability() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_flush_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_flush.bin");

    // Write, flush, then re-open to confirm data survived.
    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(111).unwrap();
        col.append_u64(222).unwrap();
        col.append_u64(333).unwrap();
        // Explicit flush — this is the durability checkpoint.
        col.flush().unwrap();
    }

    {
        let col = FixedColumn::new(path.clone(), 8).unwrap();
        assert_eq!(col.len(), 3);
        // read_u64 takes &self — no &mut borrow needed.
        assert_eq!(col.read_u64(0).unwrap(), 111);
        assert_eq!(col.read_u64(1).unwrap(), 222);
        assert_eq!(col.read_u64(2).unwrap(), 333);
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_explicit_flush_durability() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_flush_variable");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("data.bin");
    let offsets_path = temp_dir.join("offsets.bin");

    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        col.append_string("alpha").unwrap();
        col.append_string("beta").unwrap();
        // Explicit flush for both data and offsets files.
        col.flush().unwrap();
    }

    {
        let col = VariableColumn::new(data_path, offsets_path).unwrap();
        assert_eq!(col.len(), 2);
        // read_string takes &self.
        assert_eq!(col.read_string(0).unwrap(), "alpha");
        assert_eq!(col.read_string(1).unwrap(), "beta");
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones_explicit_flush_durability() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_flush_tombstones");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("tombstones.bin");

    {
        let mut t = Tombstones::new(path.clone()).unwrap();
        t.append(false).unwrap();
        t.append(true).unwrap();
        t.flush().unwrap();
    }

    {
        let t = Tombstones::new(path).unwrap();
        assert_eq!(t.len(), 2);
        // is_deleted takes &self.
        assert!(!t.is_deleted(0).unwrap());
        assert!(t.is_deleted(1).unwrap());
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_u64() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_u64.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();

    // Append values
    col.append_u64(42).unwrap();
    col.append_u64(100).unwrap();
    col.append_u64(999).unwrap();

    // Read values
    assert_eq!(col.read_u64(0).unwrap(), 42);
    assert_eq!(col.read_u64(1).unwrap(), 100);
    assert_eq!(col.read_u64(2).unwrap(), 999);
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_string() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_variable");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    // Append strings
    col.append_string("hello").unwrap();
    col.append_string("world").unwrap();
    col.append_string("test@example.com").unwrap();

    // Read strings
    assert_eq!(col.read_string(0).unwrap(), "hello");
    assert_eq!(col.read_string(1).unwrap(), "world");
    assert_eq!(col.read_string(2).unwrap(), "test@example.com");
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_tombstones");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("tombstones.bin");
    let mut tombstones = Tombstones::new(path).unwrap();

    tombstones.append(false).unwrap();
    tombstones.append(true).unwrap();
    tombstones.append(false).unwrap();

    assert_eq!(tombstones.is_deleted(0).unwrap(), false);
    assert_eq!(tombstones.is_deleted(1).unwrap(), true);
    assert_eq!(tombstones.is_deleted(2).unwrap(), false);
    assert_eq!(tombstones.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_manifest_roundtrip() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_manifest");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("manifest.json");

    let manifest = Manifest {
        schema_version: 1,
        row_count: 42,
        columns: vec![
            ColumnMetadata {
                name: "id".to_string(),
                column_type: ColumnType::U64,
                column_index: 0,
                ..Default::default()
            },
            ColumnMetadata {
                name: "email".to_string(),
                column_type: ColumnType::String,
                column_index: 1,
                ..Default::default()
            },
        ],
        wal_enabled: false,
        last_checkpoint: 0,
        compaction_epoch: 0,
        format_version: 1,
        row_anchor: None,
        auto_sequences: Default::default(),
    };
    manifest.save_to(&path).unwrap();

    // Reopen and verify the physical-layout manifest round-trips.
    let reopened = Manifest::load_from(&path).unwrap();
    assert_eq!(reopened.row_count, 42);
    assert_eq!(reopened.columns.len(), 2);
    assert_eq!(reopened.columns[0].name, "id");
    assert_eq!(reopened.columns[1].name, "email");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_out_of_bounds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_u64.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();

    col.append_u64(42).unwrap();

    // Reading beyond bounds should error
    let result = col.read_u64(1);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_persistence() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_fixed_persist");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_u64.bin");

    // Write data
    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(100).unwrap();
        col.append_u64(200).unwrap();
        col.append_u64(300).unwrap();
    }

    // Reopen and verify
    {
        let col = FixedColumn::new(path.clone(), 8).unwrap();
        assert_eq!(col.len(), 3);
        assert_eq!(col.read_u64(0).unwrap(), 100);
        assert_eq!(col.read_u64(1).unwrap(), 200);
        assert_eq!(col.read_u64(2).unwrap(), 300);
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_empty_string() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_empty_string");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    col.append_string("").unwrap();
    col.append_string("hello").unwrap();
    col.append_string("").unwrap();

    assert_eq!(col.read_string(0).unwrap(), "");
    assert_eq!(col.read_string(1).unwrap(), "hello");
    assert_eq!(col.read_string(2).unwrap(), "");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_large_string() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_large_string");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    // Test with 1KB string
    let large_string = "x".repeat(1024);
    col.append_string(&large_string).unwrap();
    col.append_string("small").unwrap();

    assert_eq!(col.read_string(0).unwrap(), large_string);
    assert_eq!(col.read_string(1).unwrap(), "small");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_persistence() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_var_persist");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");

    // Write data
    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        col.append_string("first").unwrap();
        col.append_string("second").unwrap();
        col.append_string("third").unwrap();
    }

    // Reopen and verify
    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        assert_eq!(col.len(), 3);
        assert_eq!(col.read_string(0).unwrap(), "first");
        assert_eq!(col.read_string(1).unwrap(), "second");
        assert_eq!(col.read_string(2).unwrap(), "third");

        // Append more after reopening
        col.append_string("fourth").unwrap();
    }

    // Verify new data persisted
    {
        let col = VariableColumn::new(data_path, offsets_path).unwrap();
        assert_eq!(col.len(), 4);
        assert_eq!(col.read_string(3).unwrap(), "fourth");
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_out_of_bounds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_var_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("test_data.bin");
    let offsets_path = temp_dir.join("test_offsets.bin");
    let mut col = VariableColumn::new(data_path, offsets_path).unwrap();

    col.append_string("test").unwrap();

    // Reading beyond bounds should error
    let result = col.read_string(1);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones_out_of_bounds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_tomb_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("tombstones.bin");
    let mut tombstones = Tombstones::new(path).unwrap();

    tombstones.append(false).unwrap();

    // Reading beyond bounds should error
    let result = tombstones.is_deleted(1);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_u32() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_u32");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_u32.bin");
    let mut col = FixedColumn::new(path, 4).unwrap();

    col.append_u32(42).unwrap();
    col.append_u32(100).unwrap();
    col.append_u32(999).unwrap();

    assert_eq!(col.read_u32(0).unwrap(), 42);
    assert_eq!(col.read_u32(1).unwrap(), 100);
    assert_eq!(col.read_u32(2).unwrap(), 999);
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_i32() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_i32");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_i32.bin");
    let mut col = FixedColumn::new(path, 4).unwrap();

    col.append_i32(-42).unwrap();
    col.append_i32(100).unwrap();
    col.append_i32(-999).unwrap();

    assert_eq!(col.read_i32(0).unwrap(), -42);
    assert_eq!(col.read_i32(1).unwrap(), 100);
    assert_eq!(col.read_i32(2).unwrap(), -999);
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_i64() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_i64");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_i64.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();

    col.append_i64(-42000000000).unwrap();
    col.append_i64(100000000000).unwrap();
    col.append_i64(-999).unwrap();

    assert_eq!(col.read_i64(0).unwrap(), -42000000000);
    assert_eq!(col.read_i64(1).unwrap(), 100000000000);
    assert_eq!(col.read_i64(2).unwrap(), -999);
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_f64() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_f64");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_f64.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();

    col.append_f64(3.14159).unwrap();
    col.append_f64(-2.71828).unwrap();
    col.append_f64(1.41421).unwrap();

    assert!((col.read_f64(0).unwrap() - 3.14159).abs() < 0.00001);
    assert!((col.read_f64(1).unwrap() - (-2.71828)).abs() < 0.00001);
    assert!((col.read_f64(2).unwrap() - 1.41421).abs() < 0.00001);
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_bool() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_bool");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_bool.bin");
    let mut col = FixedColumn::new(path, 1).unwrap();

    col.append_bool(true).unwrap();
    col.append_bool(false).unwrap();
    col.append_bool(true).unwrap();

    assert_eq!(col.read_bool(0).unwrap(), true);
    assert_eq!(col.read_bool(1).unwrap(), false);
    assert_eq!(col.read_bool(2).unwrap(), true);
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_uuid() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_uuid");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_uuid.bin");
    let mut col = FixedColumn::new(path, 16).unwrap();

    let uuid1 = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let uuid2 = [16u8, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];

    col.append_uuid(uuid1).unwrap();
    col.append_uuid(uuid2).unwrap();

    assert_eq!(col.read_uuid(0).unwrap(), uuid1);
    assert_eq!(col.read_uuid(1).unwrap(), uuid2);
    assert_eq!(col.len(), 2);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_timestamp() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_timestamp");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_timestamp.bin");
    let mut col = FixedColumn::new(path, 8).unwrap();

    let now = 1704067200i64; // 2024-01-01 00:00:00 UTC
    let later = now + 3600;

    col.append_timestamp(now).unwrap();
    col.append_timestamp(later).unwrap();

    assert_eq!(col.read_timestamp(0).unwrap(), now);
    assert_eq!(col.read_timestamp(1).unwrap(), later);
    assert_eq!(col.len(), 2);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_bytes() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_bytes");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_bytes.bin");
    let mut col = FixedColumn::new(path, 20).unwrap();

    let bytes1 = [1u8; 20];
    let bytes2 = [2u8; 20];
    let bytes3 = [3u8; 20];

    col.append_bytes(&bytes1).unwrap();
    col.append_bytes(&bytes2).unwrap();
    col.append_bytes(&bytes3).unwrap();

    assert_eq!(col.read_bytes(0).unwrap(), bytes1.to_vec());
    assert_eq!(col.read_bytes(1).unwrap(), bytes2.to_vec());
    assert_eq!(col.read_bytes(2).unwrap(), bytes3.to_vec());
    assert_eq!(col.len(), 3);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_bytes_wrong_size() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_bytes_wrong");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_bytes.bin");
    let mut col = FixedColumn::new(path, 20).unwrap();

    let bytes_wrong_size = [1u8; 10]; // Wrong size (should be 20)

    let result = col.append_bytes(&bytes_wrong_size);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_char_array() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_char");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_char.bin");
    let mut col = FixedColumn::new(path, 50).unwrap();

    // Simulate char(50) storage
    let mut str1 = [0u8; 50];
    let mut str2 = [0u8; 50];
    str1[..5].copy_from_slice(b"hello");
    str2[..5].copy_from_slice(b"world");

    col.append_bytes(&str1).unwrap();
    col.append_bytes(&str2).unwrap();

    let read1 = col.read_bytes(0).unwrap();
    let read2 = col.read_bytes(1).unwrap();

    assert_eq!(&read1[..5], b"hello");
    assert_eq!(&read2[..5], b"world");

    fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// Snapshot: watermark read isolation (mvcc-concurrency, Direction A)
//
// A Snapshot is a bare committed-row-count watermark. Because storage is
// append-only, a row's index is stable for life, so the watermark alone
// defines a consistent view: index < watermark is visible, everything
// appended later is not. These tests pin the substrate contract; generated
// code composes per-model watermarks into its own snapshot struct.
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_visibility_boundary() {
    let snap = Snapshot::new(3);
    assert_eq!(snap.watermark(), 3);
    assert!(snap.visible(0));
    assert!(snap.visible(2));
    // The row at the watermark and beyond was appended after capture.
    assert!(!snap.visible(3));
    assert!(!snap.visible(9));
}

#[test]
fn test_snapshot_empty_sees_nothing() {
    let snap = Snapshot::new(0);
    assert_eq!(snap.watermark(), 0);
    assert!(!snap.visible(0));
}

#[test]
fn test_snapshot_is_copy_and_stable() {
    // A snapshot is Copy and immutable: capturing does not observe later growth.
    let snap = Snapshot::new(2);
    let also = snap; // Copy, not move
    assert_eq!(snap.watermark(), also.watermark());
    assert!(snap.visible(1));
    assert!(!snap.visible(2));
}

// ---------------------------------------------------------------------------
// #56 Direction B: read-only column reader handles (single writer, many readers)
//
// These prove the load-bearing substrate invariants the generated DatabaseReader
// relies on: (1) a reader opened via `col.reader()` shares the SAME file as the
// writer, so appends made AFTER the reader was created are visible through the
// reader's independent fd (POSIX page-cache coherence across try_clone'd fds);
// (2) positional reads are safe concurrently with a live appending writer and
// never observe a torn value below the committed watermark; (3) an out-of-range
// index surfaces as the same InvalidInput error the writer columns return.
// ---------------------------------------------------------------------------

#[test]
fn test_fixed_column_reader_sees_writer_appends() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_reader_fixed_sees");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("c.bin");

    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    col.append_u64(10).unwrap();

    // Reader created while the column holds exactly one row...
    let reader = col.reader().unwrap();
    assert_eq!(reader.len(), 1);
    assert_eq!(reader.read_u64(0).unwrap(), 10);

    // ...still sees rows the writer appends AFTERWARD (shared file, live length).
    col.append_u64(20).unwrap();
    col.append_u64(30).unwrap();
    assert_eq!(reader.len(), 3, "reader derives length live from the shared file");
    assert_eq!(reader.read_u64(1).unwrap(), 20);
    assert_eq!(reader.read_u64(2).unwrap(), 30);

    // An index past the committed prefix is a bounded InvalidInput, not a panic.
    let err = reader.read_u64(3).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_and_tombstone_readers_share_writer_file() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_reader_var_tomb");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("data.bin");
    let offsets_path = temp_dir.join("offsets.bin");
    let tomb_path = temp_dir.join("tomb.bin");

    let mut vc = VariableColumn::new(data_path, offsets_path).unwrap();
    let mut tomb = Tombstones::new(tomb_path).unwrap();
    vc.append_string("alpha").unwrap();
    tomb.append(false).unwrap();

    let vc_reader = vc.reader().unwrap();
    let tomb_reader = tomb.reader().unwrap();
    assert_eq!(vc_reader.read_string(0).unwrap(), "alpha");
    assert!(!tomb_reader.is_deleted(0).unwrap());

    // Appends after the readers exist are visible through them.
    vc.append_string("beta").unwrap();
    tomb.append(true).unwrap();
    assert_eq!(vc_reader.len(), 2);
    assert_eq!(vc_reader.read_string(1).unwrap(), "beta");
    assert_eq!(tomb_reader.len(), 2);
    assert!(tomb_reader.is_deleted(1).unwrap());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_reader_lockfree_concurrent_with_live_writer() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let temp_dir = std::env::temp_dir().join("forgedb_test_reader_concurrent");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("c.bin");

    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    // Seed the value at index i as i*2 so a reader can verify no torn value.
    col.append_u64(0).unwrap();

    // Two reader fds shared across reader threads (pread is thread-safe on &File).
    let reader = Arc::new(col.reader().unwrap());
    let stop = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for _ in 0..4 {
        let reader = Arc::clone(&reader);
        let stop = Arc::clone(&stop);
        handles.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // The committed length is the writer's watermark; every index below
                // it must decode to exactly i*2 — never a torn / half-written value.
                let n = reader.len();
                for i in 0..n {
                    let v = reader.read_u64(i).expect("committed index readable");
                    assert_eq!(v, (i as u64) * 2, "reader saw a torn value at {i}");
                }
            }
        }));
    }

    // Single writer appends the rest while readers run — no lock held anywhere.
    for i in 1u64..5000 {
        col.append_u64(i * 2).unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        h.join().unwrap();
    }

    // Final consistency check through the reader.
    assert_eq!(reader.len(), 5000);
    assert_eq!(reader.read_u64(4999).unwrap(), 4999 * 2);

    fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// truncate_to_rows: crash-recovery realignment for FixedColumn
// ---------------------------------------------------------------------------

#[test]
fn test_fixed_column_truncate_to_rows_basic() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_fixed_basic");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("c.bin");

    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    col.append_u64(10).unwrap();
    col.append_u64(20).unwrap();
    col.append_u64(30).unwrap();
    col.append_u64(40).unwrap();
    assert_eq!(col.len(), 4);

    // Truncate to 2 rows.
    col.truncate_to_rows(2).unwrap();
    assert_eq!(col.len(), 2);
    // Rows below the cut are intact.
    assert_eq!(col.read_u64(0).unwrap(), 10);
    assert_eq!(col.read_u64(1).unwrap(), 20);
    // Rows above the cut are gone.
    assert!(col.read_u64(2).is_err());

    // The next append lands at exactly row 2.
    col.append_u64(99).unwrap();
    assert_eq!(col.len(), 3);
    assert_eq!(col.read_u64(2).unwrap(), 99);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_truncate_to_rows_zero() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_fixed_zero");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("c.bin");

    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    col.append_u64(1).unwrap();
    col.append_u64(2).unwrap();
    assert_eq!(col.len(), 2);

    // Truncate to zero — all rows discarded.
    col.truncate_to_rows(0).unwrap();
    assert_eq!(col.len(), 0);
    assert!(col.is_empty());
    assert!(col.read_u64(0).is_err());

    // Append resumes from row 0.
    col.append_u64(42).unwrap();
    assert_eq!(col.len(), 1);
    assert_eq!(col.read_u64(0).unwrap(), 42);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_truncate_to_rows_beyond_length_errors() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_fixed_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("c.bin");

    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    col.append_u64(1).unwrap();

    let err = col.truncate_to_rows(2).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_truncate_repairs_torn_tail() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_fixed_torn");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("c.bin");

    // Write 3 full rows (8 bytes each = 24 bytes).
    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(10).unwrap();
        col.append_u64(20).unwrap();
        col.append_u64(30).unwrap();
    }

    // Simulate a partial/torn write by manually extending the file with 3 stray bytes,
    // as if a crash happened mid-append of a 4th row.
    {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(24 + 3).unwrap(); // 3 bytes of a partial 8-byte value
    }

    // Reopen: row_count = 27 / 8 = 3 (integer division floors the partial row).
    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    assert_eq!(col.len(), 3, "partial row is excluded from row_count");
    // Verify the file is NOT yet clean (it still has 27 bytes).
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        27,
        "stray bytes still present before truncate"
    );

    // Truncate to the current row count — this removes the torn tail.
    col.truncate_to_rows(col.len()).unwrap();
    assert_eq!(col.len(), 3);
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        24,
        "file length is exact multiple of value_size after truncate"
    );

    // All three original values survive.
    assert_eq!(col.read_u64(0).unwrap(), 10);
    assert_eq!(col.read_u64(1).unwrap(), 20);
    assert_eq!(col.read_u64(2).unwrap(), 30);

    // Next append lands at row 3, not at some torn offset.
    col.append_u64(40).unwrap();
    assert_eq!(col.len(), 4);
    assert_eq!(col.read_u64(3).unwrap(), 40);
    assert_eq!(fs::metadata(&path).unwrap().len(), 32);

    fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// truncate_to_rows: crash-recovery realignment for VariableColumn
// ---------------------------------------------------------------------------

#[test]
fn test_variable_column_truncate_to_rows_basic() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_var_basic");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let data_path = temp_dir.join("data.bin");
    let offsets_path = temp_dir.join("offsets.bin");

    let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
    col.append_string("alpha").unwrap();
    col.append_string("beta").unwrap();
    col.append_string("gamma").unwrap();
    col.append_string("delta").unwrap();
    assert_eq!(col.len(), 4);

    // Truncate to 2 rows.
    col.truncate_to_rows(2).unwrap();
    assert_eq!(col.len(), 2);
    assert_eq!(col.read_string(0).unwrap(), "alpha");
    assert_eq!(col.read_string(1).unwrap(), "beta");
    assert!(col.read_string(2).is_err());

    // The next append lands at row 2 with the correct data offset.
    col.append_string("new").unwrap();
    assert_eq!(col.len(), 3);
    assert_eq!(col.read_string(2).unwrap(), "new");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_truncate_to_rows_zero() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_var_zero");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let data_path = temp_dir.join("data.bin");
    let offsets_path = temp_dir.join("offsets.bin");

    let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
    col.append_string("hello").unwrap();
    col.append_string("world").unwrap();
    assert_eq!(col.len(), 2);

    col.truncate_to_rows(0).unwrap();
    assert_eq!(col.len(), 0);
    assert!(col.is_empty());
    assert!(col.read_string(0).is_err());

    // Files are empty.
    assert_eq!(fs::metadata(&data_path).unwrap().len(), 0);
    assert_eq!(fs::metadata(&offsets_path).unwrap().len(), 0);

    // Append resumes cleanly from row 0.
    col.append_string("fresh").unwrap();
    assert_eq!(col.len(), 1);
    assert_eq!(col.read_string(0).unwrap(), "fresh");

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_truncate_to_rows_beyond_length_errors() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_var_oob");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let data_path = temp_dir.join("data.bin");
    let offsets_path = temp_dir.join("offsets.bin");

    let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
    col.append_string("x").unwrap();

    let err = col.truncate_to_rows(2).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_variable_column_truncate_to_rows_persistence() {
    // Verify that truncate + reopen + append is consistent.
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_var_persist");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let data_path = temp_dir.join("data.bin");
    let offsets_path = temp_dir.join("offsets.bin");

    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        col.append_string("one").unwrap();
        col.append_string("two").unwrap();
        col.append_string("three").unwrap();
        col.truncate_to_rows(2).unwrap();
    }

    // After reopen the state reflects the truncation.
    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        assert_eq!(col.len(), 2);
        assert_eq!(col.read_string(0).unwrap(), "one");
        assert_eq!(col.read_string(1).unwrap(), "two");
        assert!(col.read_string(2).is_err());

        col.append_string("after").unwrap();
    }

    {
        let col = VariableColumn::new(data_path, offsets_path).unwrap();
        assert_eq!(col.len(), 3);
        assert_eq!(col.read_string(2).unwrap(), "after");
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// truncate_to_rows: crash-recovery realignment for Tombstones
// ---------------------------------------------------------------------------

#[test]
fn test_tombstones_truncate_to_rows_basic() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_tomb_basic");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("tombstones.bin");

    let mut t = Tombstones::new(path.clone()).unwrap();
    t.append(false).unwrap();
    t.append(true).unwrap();
    t.append(false).unwrap();
    t.append(true).unwrap();
    assert_eq!(t.len(), 4);

    // Truncate to 2 rows.
    t.truncate_to_rows(2).unwrap();
    assert_eq!(t.len(), 2);
    assert!(!t.is_deleted(0).unwrap());
    assert!(t.is_deleted(1).unwrap());
    assert!(t.is_deleted(2).is_err());

    // Next append lands at row 2.
    t.append(false).unwrap();
    assert_eq!(t.len(), 3);
    assert!(!t.is_deleted(2).unwrap());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones_truncate_to_rows_zero() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_tomb_zero");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("tombstones.bin");

    let mut t = Tombstones::new(path.clone()).unwrap();
    t.append(true).unwrap();
    t.append(false).unwrap();
    assert_eq!(t.len(), 2);

    t.truncate_to_rows(0).unwrap();
    assert_eq!(t.len(), 0);
    assert!(t.is_empty());
    assert_eq!(fs::metadata(&path).unwrap().len(), 0);

    // Append resumes from row 0.
    t.append(true).unwrap();
    assert_eq!(t.len(), 1);
    assert!(t.is_deleted(0).unwrap());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones_truncate_to_rows_beyond_length_errors() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_trunc_tomb_oob2");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("tombstones.bin");

    let mut t = Tombstones::new(path.clone()).unwrap();
    t.append(false).unwrap();

    let err = t.truncate_to_rows(5).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// DirLock: single-writer advisory lock
// ---------------------------------------------------------------------------

#[test]
fn test_dir_lock_acquire_fresh_dir_succeeds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_dirlock_fresh");
    let _ = fs::remove_dir_all(&temp_dir);

    let lock = DirLock::acquire(&temp_dir).unwrap();
    // The lock file must exist.
    assert!(temp_dir.join(".forgedb.lock").exists());
    drop(lock);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_dir_lock_second_acquire_while_held_fails() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_dirlock_conflict");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let lock1 = DirLock::acquire(&temp_dir).unwrap();

    // A second acquire on the same directory — different file handle, same advisory
    // lock target — must fail with WouldBlock.
    let err = DirLock::acquire(&temp_dir).unwrap_err();
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::WouldBlock,
        "second acquire must be refused while first DirLock is alive"
    );

    // The first lock is still alive.
    drop(lock1);
    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_dir_lock_reacquire_after_drop_succeeds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_dirlock_reacquire");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let lock1 = DirLock::acquire(&temp_dir).unwrap();
    drop(lock1); // OS releases the advisory lock here.

    // After the first lock is dropped, a new acquire must succeed.
    let lock2 = DirLock::acquire(&temp_dir).unwrap();
    drop(lock2);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_dir_lock_creates_parent_dir() {
    let temp_dir =
        std::env::temp_dir().join("forgedb_test_dirlock_mkdir").join("nested");
    let _ = fs::remove_dir_all(
        std::env::temp_dir().join("forgedb_test_dirlock_mkdir"),
    );

    // The directory does not exist — acquire must create it.
    let lock = DirLock::acquire(&temp_dir).unwrap();
    assert!(temp_dir.exists());
    assert!(temp_dir.join(".forgedb.lock").exists());
    drop(lock);

    fs::remove_dir_all(std::env::temp_dir().join("forgedb_test_dirlock_mkdir")).unwrap();
}

// ---------------------------------------------------------------------------
// gather(): the class-1 columnar-read primitive for the bindings' Arrow /
// columnar-export gather path (#51/#52/#117). Copies selected physical rows
// contiguously; schema-agnostic (opaque indices, never a field/type/schema).
// ---------------------------------------------------------------------------

#[test]
fn test_fixed_column_gather_selects_and_orders() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_gather_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("g.bin"), 8).unwrap();
    for v in [10u64, 11, 12, 13, 14] {
        col.append_u64(v).unwrap();
    }

    // A non-contiguous, reordered subset — the update-heavy live set the
    // dense-prefix alias path can't cover.
    let out = col.gather(&[3, 0, 4]).unwrap();
    assert_eq!(out.len(), 3 * 8);
    assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 13);
    assert_eq!(u64::from_le_bytes(out[8..16].try_into().unwrap()), 10);
    assert_eq!(u64::from_le_bytes(out[16..24].try_into().unwrap()), 14);

    // Empty selection → empty buffer.
    assert!(col.gather(&[]).unwrap().is_empty());

    // Out-of-bounds index → error.
    assert!(col.gather(&[0, 5]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_gather_full_prefix_matches_reads() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_gather_prefix");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("g.bin"), 4).unwrap();
    for v in [1u32, 2, 3, 4] {
        col.append_u32(v).unwrap();
    }

    // Gathering the whole [0, len) prefix must equal reading each row in order.
    let out = col.gather(&[0, 1, 2, 3]).unwrap();
    for i in 0..4usize {
        let got = u32::from_le_bytes(out[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(got, col.read_u32(i).unwrap());
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_gather_mapped_path_matches_per_row_reads() {
    // #221: at/above the mmap threshold `gather` maps the spanned region once
    // and copies contiguous runs out of it. That path must be byte-identical to
    // the per-row `read_exact_at` loop for every selection shape — the tests
    // above only exercise selections below the threshold.
    let temp_dir = std::env::temp_dir().join("forgedb_test_gather_mapped");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("g.bin"), 8).unwrap();
    for v in 0..1000u64 {
        col.append_u64(v * 7 + 1).unwrap();
    }

    let decode = |bytes: &[u8]| -> Vec<u64> {
        bytes
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
            .collect()
    };
    let expect = |idx: &[usize]| -> Vec<u64> { idx.iter().map(|&i| col.read_u64(i).unwrap()).collect() };

    // The shape the cliff actually produces: a live set that is long runs
    // broken by the holes superseded rows left behind.
    let holey: Vec<usize> = (0..1000).filter(|i| i % 3 != 0).collect();
    assert_eq!(decode(&col.gather(&holey).unwrap()), expect(&holey));

    // A dense run that is *not* a prefix — `export`'s alias fast path rejects
    // it, so it lands here as one single-run copy.
    let run: Vec<usize> = (500..900).collect();
    assert_eq!(decode(&col.gather(&run).unwrap()), expect(&run));

    // Reversed: no run ever extends, so every slot is its own copy.
    let rev: Vec<usize> = (0..64).rev().collect();
    assert_eq!(decode(&col.gather(&rev).unwrap()), expect(&rev));

    // Sparse across the whole file — the mapped span is far larger than the
    // selection.
    let sparse: Vec<usize> = (0..1000).step_by(97).collect();
    assert_eq!(decode(&col.gather(&sparse).unwrap()), expect(&sparse));

    // Duplicated indices must be honoured positionally, not deduplicated.
    let dupes: Vec<usize> = vec![9, 9, 9, 4, 4, 800, 800, 0, 0, 0];
    assert_eq!(decode(&col.gather(&dupes).unwrap()), expect(&dupes));

    // Both paths agree across the threshold boundary (GATHER_MMAP_MIN_ROWS = 8;
    // private to the crate, so spelled out here).
    for n in [7usize, 8, 9] {
        let idx: Vec<usize> = (0..n).map(|i| i * 3).collect();
        assert_eq!(decode(&col.gather(&idx).unwrap()), expect(&idx));
    }

    // An out-of-bounds index anywhere in a large selection still errors — the
    // whole selection is bounds-checked before the span is mapped.
    let mut bad: Vec<usize> = (0..100).collect();
    bad.push(1000);
    assert!(col.gather(&bad).is_err());

    // And the generated scan path (gather_buffered → export → gather) agrees.
    let buf = col.gather_buffered(&holey).unwrap();
    for (slot, &i) in holey.iter().enumerate() {
        assert_eq!(buf.read_u64(slot).unwrap(), col.read_u64(i).unwrap());
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

// export(): the alias-or-gather columnar-export primitive (the zero-copy fast
// path behind the bindings' Arrow export). A contiguous dense prefix `[0, n)`
// aliases the column file via mmap (`ColumnExport::Mapped`); any other selection
// falls back to a gathered owned copy (`ColumnExport::Owned`). Both expose the
// same bytes through the same `as_ptr`/`len`/`as_slice` surface.
#[test]
fn test_fixed_column_export_aliases_dense_prefix_else_gathers() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_export_alias");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("e.bin"), 8).unwrap();
    for v in [100u64, 101, 102, 103, 104] {
        col.append_u64(v).unwrap();
    }

    // A dense prefix [0, 3) → zero-copy mmap alias, exact prefix bytes.
    let prefix = col.export(&[0, 1, 2]).unwrap();
    assert!(
        matches!(prefix, ColumnExport::Mapped(_)),
        "a contiguous [0, n) prefix must alias via mmap"
    );
    assert_eq!(prefix.len(), 3 * 8);
    let s = prefix.as_slice();
    for i in 0..3usize {
        assert_eq!(u64::from_le_bytes(s[i * 8..i * 8 + 8].try_into().unwrap()), 100 + i as u64);
    }

    // The FULL prefix [0, len) also aliases.
    let full = col.export(&[0, 1, 2, 3, 4]).unwrap();
    assert!(matches!(full, ColumnExport::Mapped(_)), "the whole-column prefix aliases too");
    assert_eq!(full.len(), 5 * 8);

    // A reordered / non-prefix selection → gathered owned copy.
    let gathered = col.export(&[3, 0, 4]).unwrap();
    assert!(
        matches!(gathered, ColumnExport::Owned(_)),
        "a non-prefix selection must gather an owned copy"
    );
    let g = gathered.as_slice();
    assert_eq!(u64::from_le_bytes(g[0..8].try_into().unwrap()), 103);
    assert_eq!(u64::from_le_bytes(g[8..16].try_into().unwrap()), 100);
    assert_eq!(u64::from_le_bytes(g[16..24].try_into().unwrap()), 104);

    // A prefix that stops short but is still contiguous-from-0 aliases; a subset
    // NOT starting at 0 does not.
    assert!(matches!(col.export(&[0, 1]).unwrap(), ColumnExport::Mapped(_)));
    assert!(matches!(col.export(&[1, 2, 3]).unwrap(), ColumnExport::Owned(_)));

    // Empty selection → owned empty buffer (no zero-length mmap).
    let empty = col.export(&[]).unwrap();
    assert!(matches!(empty, ColumnExport::Owned(_)));
    assert!(empty.is_empty());

    // Out-of-bounds → error (bytes-identical to gather's contract).
    assert!(col.export(&[0, 5]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// #168: bulk-buffered column scan primitives — gather_buffered +
// BufferedFixedColumn / BufferedVariableColumn + Tombstones::live_indices.
// The generated column scan bulk-loads each column once and decodes from
// memory; these prove the buffered decode is byte-identical to the per-row
// read_* path for both the dense-prefix (mmap) and reordered (gather) cases.
// ---------------------------------------------------------------------------

#[test]
fn test_buffered_fixed_column_matches_per_row_reads() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("bf.bin"), 8).unwrap();
    for v in [10u64, 11, 12, 13, 14] {
        col.append_u64(v).unwrap();
    }

    // Dense prefix [0, n): zero-copy mmap path. Slot i decodes to row i.
    let buf = col.gather_buffered(&[0, 1, 2, 3, 4]).unwrap();
    assert_eq!(buf.len(), 5);
    for slot in 0..5usize {
        assert_eq!(buf.read_u64(slot).unwrap(), col.read_u64(slot).unwrap());
    }

    // Reordered / non-prefix selection: gathered owned copy, slots follow the
    // selection order.
    let buf = col.gather_buffered(&[3, 0, 4]).unwrap();
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.read_u64(0).unwrap(), 13);
    assert_eq!(buf.read_u64(1).unwrap(), 10);
    assert_eq!(buf.read_u64(2).unwrap(), 14);
    // Slot past the selection → error, like a per-row out-of-bounds read.
    assert!(buf.read_u64(3).is_err());

    // Out-of-bounds index → error (mirrors export/gather).
    assert!(col.gather_buffered(&[0, 5]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_fixed_column_all_read_kinds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_kinds");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // uuid/bytes column (16-byte): read_uuid + read_bytes decode identically.
    let mut ucol = FixedColumn::new(temp_dir.join("u.bin"), 16).unwrap();
    ucol.append_uuid([1u8; 16]).unwrap();
    ucol.append_uuid([2u8; 16]).unwrap();
    let ubuf = ucol.gather_buffered(&[0, 1]).unwrap();
    assert_eq!(ubuf.read_uuid(0).unwrap(), [1u8; 16]);
    assert_eq!(ubuf.read_bytes(1).unwrap(), vec![2u8; 16]);

    // bool column (1-byte).
    let mut bcol = FixedColumn::new(temp_dir.join("b.bin"), 1).unwrap();
    bcol.append_bool(true).unwrap();
    bcol.append_bool(false).unwrap();
    let bbuf = bcol.gather_buffered(&[0, 1]).unwrap();
    assert!(bbuf.read_bool(0).unwrap());
    assert!(!bbuf.read_bool(1).unwrap());

    // i32 column (4-byte).
    let mut icol = FixedColumn::new(temp_dir.join("i.bin"), 4).unwrap();
    icol.append_i32(-7).unwrap();
    let ibuf = icol.gather_buffered(&[0]).unwrap();
    assert_eq!(ibuf.read_i32(0).unwrap(), -7);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_column_matches_per_row_reads() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_var");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = VariableColumn::new(
        temp_dir.join("v_data.bin"),
        temp_dir.join("v_offsets.bin"),
    )
    .unwrap();
    for s in ["", "a", "hello world", "unicode ✓ é"] {
        col.append_string(s).unwrap();
    }

    // Dense selection: slot i == row i.
    let buf = col.gather_buffered(&[0, 1, 2, 3]).unwrap();
    assert_eq!(buf.len(), 4);
    for slot in 0..4usize {
        assert_eq!(buf.read_string(slot).unwrap(), col.read_string(slot).unwrap());
    }

    // Reordered selection preserves order.
    let buf = col.gather_buffered(&[3, 0, 2]).unwrap();
    assert_eq!(buf.read_string(0).unwrap(), "unicode ✓ é");
    assert_eq!(buf.read_string(1).unwrap(), "");
    assert_eq!(buf.read_string(2).unwrap(), "hello world");
    assert!(buf.read_string(3).is_err());

    // Out-of-bounds index → error.
    assert!(col.gather_buffered(&[0, 4]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_column_mapped_path_matches_per_row_reads() {
    // #222: `gather_buffered` now bounds both reads to the selection's row span and
    // MAPS the data span once it is large enough, instead of reading the whole region.
    // The test above only covers tiny corpora, which stay on the owned path and always
    // have base == 0 — so it cannot catch a bad rebase. This one does.
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_var_mapped");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = VariableColumn::new(
        temp_dir.join("v_data.bin"),
        temp_dir.join("v_offsets.bin"),
    )
    .unwrap();

    // ~1 MB of data, comfortably past VAR_MMAP_MIN_BYTES, with empty strings sprinkled
    // in so zero-length slots are exercised inside a mapped span.
    let n = 4000usize;
    for i in 0..n {
        if i % 37 == 0 {
            col.append_string("").unwrap();
        } else {
            col.append_string(&format!("row-{i}-{}", "x".repeat(240))).unwrap();
        }
    }

    let check = |idx: &[usize]| {
        let buf = col.gather_buffered(idx).unwrap();
        assert_eq!(buf.len(), idx.len());
        for (slot, &i) in idx.iter().enumerate() {
            assert_eq!(
                buf.read_string(slot).unwrap(),
                col.read_string(i).unwrap(),
                "slot {slot} (row {i})"
            );
        }
    };

    // The churn shape: long runs broken by holes where superseded rows were skipped.
    let holey: Vec<usize> = (0..n).filter(|i| i % 3 != 0).collect();
    check(&holey);

    // A span that starts late — base is large, so an unrebased offset would read the
    // wrong bytes (or fall outside the mapping and error).
    let tail: Vec<usize> = (3000..3500).collect();
    check(&tail);

    // Selection order is preserved, including fully reversed.
    let rev: Vec<usize> = (2000..2400).rev().collect();
    check(&rev);

    // Sparse across the whole file: the mapped span is nearly everything, but only the
    // addressed pages are ever touched.
    let sparse: Vec<usize> = (0..n).step_by(97).collect();
    check(&sparse);

    // Duplicated indices are positional, not deduplicated.
    check(&[3999, 3999, 0, 0, 2500, 2500]);

    // A selection of only empty strings has a zero-length data span.
    let empties: Vec<usize> = (0..n).filter(|i| i % 37 == 0).collect();
    check(&empties);

    // Out-of-bounds anywhere still errors, before anything is mapped.
    let mut bad: Vec<usize> = (0..100).collect();
    bad.push(n);
    assert!(col.gather_buffered(&bad).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_column_sparse_offsets_path_matches_spanned() {
    // #228: below `SPARSE_OFFSETS_SPAN_FACTOR` density, `gather_buffered` reads each
    // offsets entry on its own rather than the whole spanned slice — a two-row
    // selection at opposite ends of a large column was reading hundreds of KB of
    // offsets to use 32 bytes of it, which made the index-pushdown arm SLOWER than
    // the per-row positional decode it replaced.
    //
    // The branch is a pure performance choice, so the guard is that it is invisible:
    // both paths must produce byte-identical slots for the same selection. The
    // selections below are chosen to land deliberately on each side of the threshold
    // and right at it, since a density branch that is wrong at its own boundary is
    // exactly the bug this could introduce.
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_var_sparse");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col =
        VariableColumn::new(temp_dir.join("v_data.bin"), temp_dir.join("v_offsets.bin")).unwrap();

    let n = 5000usize;
    for i in 0..n {
        if i % 41 == 0 {
            col.append_string("").unwrap();
        } else {
            col.append_string(&format!("row-{i}-{}", "z".repeat(180)))
                .unwrap();
        }
    }

    let check = |label: &str, idx: &[usize]| {
        let buf = col.gather_buffered(idx).unwrap();
        assert_eq!(buf.len(), idx.len(), "{label}: slot count");
        for (slot, &i) in idx.iter().enumerate() {
            assert_eq!(
                buf.read_str(slot).unwrap(),
                col.read_string(i).unwrap(),
                "{label}: slot {slot} (row {i})"
            );
        }
    };

    // Maximally sparse: two rows at opposite ends. This is the case that regressed.
    check("two rows, full span", &[3, n - 2]);
    // One row in the middle — span of 1, so it takes the spanned path trivially.
    check("single row", &[n / 2]);
    // A scattered pushdown-shaped candidate set.
    check("20 scattered", &(0..20).map(|i| i * (n / 20)).collect::<Vec<_>>());
    // Dense enough to stay on the spanned read (span/n well under the factor).
    check("dense run", &(1000..1400).collect::<Vec<_>>());
    // Straddling the threshold from both sides: with 8 indices the branch flips at a
    // span of 8*128 = 1024 rows, so these two differ only in which path they take.
    check("just dense", &(0..8).map(|i| i * 100).collect::<Vec<_>>());
    check("just sparse", &(0..8).map(|i| i * 200).collect::<Vec<_>>());
    // Order is positional on both paths, including reversed and duplicated.
    check("reversed sparse", &(0..10).rev().map(|i| i * 450).collect::<Vec<_>>());
    check("duplicates", &[n - 1, 0, n - 1, 0]);
    // Empty strings inside a sparse selection: a zero-length data span still decodes.
    check("sparse empties", &(0..n).filter(|i| i % 41 == 0).step_by(30).collect::<Vec<_>>());

    // Bounds are still checked before any read, on either path.
    assert!(col.gather_buffered(&[0, n]).is_err());
    assert!(col.gather_buffered(&[n - 1, n + 5]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_read_str_matches_read_string() {
    // #224: `read_str` borrows out of the buffered span instead of allocating a
    // `String` per slot.  It is the decode both paths now share — `read_string` is
    // `read_str(..).map(str::to_owned)` — so the guard is that they agree on every
    // slot, over BOTH `ColumnExport` arms: the owned copy (small span) and the
    // `mmap` alias (span >= VAR_MMAP_MIN_BYTES, where `base` rebasing is live).
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_read_str");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // --- owned arm: a handful of tiny values, base == 0 ---
    let mut small = VariableColumn::new(
        temp_dir.join("s_data.bin"),
        temp_dir.join("s_offsets.bin"),
    )
    .unwrap();
    for s in ["", "a", "hello world", "unicode ✓ é"] {
        small.append_string(s).unwrap();
    }
    let buf = small.gather_buffered(&[0, 1, 2, 3]).unwrap();
    for slot in 0..4usize {
        assert_eq!(buf.read_str(slot).unwrap(), buf.read_string(slot).unwrap());
        assert_eq!(buf.read_str(slot).unwrap(), small.read_string(slot).unwrap());
    }
    // Out-of-bounds slot: same error kind as `read_string`.
    let err = buf.read_str(4).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(buf.read_string(4).unwrap_err().kind(), err.kind());

    // --- mapped arm: past VAR_MMAP_MIN_BYTES, so `base` is non-zero on a late span ---
    let mut big = VariableColumn::new(
        temp_dir.join("b_data.bin"),
        temp_dir.join("b_offsets.bin"),
    )
    .unwrap();
    let n = 4000usize;
    for i in 0..n {
        if i % 37 == 0 {
            big.append_string("").unwrap();
        } else {
            big.append_string(&format!("row-{i}-{}", "x".repeat(240))).unwrap();
        }
    }
    let check = |idx: &[usize]| {
        let buf = big.gather_buffered(idx).unwrap();
        for (slot, &i) in idx.iter().enumerate() {
            assert_eq!(
                buf.read_str(slot).unwrap(),
                big.read_string(i).unwrap(),
                "slot {slot} (row {i})"
            );
        }
    };
    // Churn shape (holes between live rows), a late span, reversed order, and a
    // selection of only empty strings — the same shapes #222's rebase guard uses.
    check(&(0..n).filter(|i| i % 3 != 0).collect::<Vec<_>>());
    check(&(3000..3500).collect::<Vec<_>>());
    check(&(2000..2400).rev().collect::<Vec<_>>());
    check(&(0..n).filter(|i| i % 37 == 0).collect::<Vec<_>>());

    // Two borrows from one buffer coexist — the borrow is tied to `&self`, so
    // nothing about `read_str` narrows how a caller may hold slots.
    let buf = big.gather_buffered(&[10, 11]).unwrap();
    let (a, b) = (buf.read_str(0).unwrap(), buf.read_str(1).unwrap());
    assert_ne!(a, b);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_read_str_rejects_invalid_utf8() {
    // Validation is per read and stays that way (#224 open question 4): a corrupt
    // on-disk byte must surface as `InvalidData`, not as unchecked `&str`.
    use std::io::{Seek, Write};

    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_read_str_utf8");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("v_data.bin");
    let mut col =
        VariableColumn::new(data_path.clone(), temp_dir.join("v_offsets.bin")).unwrap();
    col.append_string("valid").unwrap();
    col.append_string("also valid").unwrap();
    col.flush().unwrap();

    // Corrupt the first byte of slot 1 (`data` is a flat append-only file, so slot 1
    // starts right after "valid").
    let mut f = fs::OpenOptions::new().write(true).open(&data_path).unwrap();
    f.seek(std::io::SeekFrom::Start(5)).unwrap();
    f.write_all(&[0xFF]).unwrap();
    f.flush().unwrap();
    drop(f);

    let buf = col.gather_buffered(&[0, 1]).unwrap();
    assert_eq!(buf.read_str(0).unwrap(), "valid");
    let err = buf.read_str(1).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(buf.read_string(1).unwrap_err().kind(), err.kind());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_tombstones_live_indices_filters_deleted() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_live_indices");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut t = Tombstones::new(temp_dir.join("t.bin")).unwrap();
    // rows: 0 live, 1 deleted, 2 live, 3 deleted, 4 live
    for deleted in [false, true, false, true, false] {
        t.append(deleted).unwrap();
    }

    // Preserves order, drops deleted, agrees with is_deleted row-by-row.
    assert_eq!(t.live_indices(&[0, 1, 2, 3, 4]).unwrap(), vec![0, 2, 4]);
    assert_eq!(t.live_indices(&[4, 3, 2, 1, 0]).unwrap(), vec![4, 2, 0]);
    // A row index past the committed count is treated as not-live (dropped),
    // matching is_deleted(..).unwrap_or(true).
    assert_eq!(t.live_indices(&[0, 9, 2]).unwrap(), vec![0, 2]);
    // Empty input → empty output.
    assert!(t.live_indices(&[]).unwrap().is_empty());

    fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// #153: coalesced-barrier durability primitives (sync_to_drive + barrier).
//
// A checkpoint pushes every column to the drive cache (sync_to_drive, cheap,
// no device barrier) then issues ONE device barrier that flushes the whole
// drive cache — making durable every column synced this way. These tests prove
// data written via sync_to_drive + a single barrier survives a reopen for all
// three column types with ONE barrier covering multiple columns.
// ---------------------------------------------------------------------------

#[test]
fn test_coalesced_barrier_durability_across_columns() {
    let dir = std::env::temp_dir().join("forgedb_test_153_coalesced");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let a = dir.join("a.bin");
    let b = dir.join("b.bin");
    let data = dir.join("v_data.bin");
    let offs = dir.join("v_offsets.bin");
    let tomb = dir.join("t.bin");

    {
        let mut ca = FixedColumn::new(a.clone(), 8).unwrap();
        let mut cb = FixedColumn::new(b.clone(), 16).unwrap();
        let mut cv = VariableColumn::new(data.clone(), offs.clone()).unwrap();
        let mut ct = Tombstones::new(tomb.clone()).unwrap();
        ca.append_u64(7).unwrap();
        cb.append_uuid([9u8; 16]).unwrap();
        cv.append_string("hello").unwrap();
        ct.append(true).unwrap();

        // Push every column to the drive cache (no barrier each).
        ca.sync_to_drive().unwrap();
        cb.sync_to_drive().unwrap();
        cv.sync_to_drive().unwrap();
        ct.sync_to_drive().unwrap();
        // ONE device barrier makes ALL of the above durable on media.
        ct.barrier().unwrap();
    }

    // Reopen — every column's data survived under the single barrier.
    {
        let ca = FixedColumn::new(a, 8).unwrap();
        let cb = FixedColumn::new(b, 16).unwrap();
        let cv = VariableColumn::new(data, offs).unwrap();
        let ct = Tombstones::new(tomb).unwrap();
        assert_eq!(ca.read_u64(0).unwrap(), 7);
        assert_eq!(cb.read_uuid(0).unwrap(), [9u8; 16]);
        assert_eq!(cv.read_string(0).unwrap(), "hello");
        assert_eq!(ct.is_deleted(0).unwrap(), true);
    }

    fs::remove_dir_all(&dir).unwrap();
}

// ---------------------------------------------------------------------------
// #231 — append_tagged: byte-identical to the concatenation it replaces
//
// This is the stored format, so a divergence here is a data bug, not a perf
// regression: every nullable string/json column in every generated database
// writes through this path, and rows written before and after the change sit
// in the same file. The guards compare the raw data + offsets bytes against a
// column built the old way, and round-trip through both readers.
// ---------------------------------------------------------------------------

/// The corpus the byte-identity guards run over: empty, ASCII, multi-byte
/// unicode, an embedded NUL (which the tag byte must not be confused with), and
/// a value long enough to span more than one page.
fn tagged_corpus() -> Vec<String> {
    vec![
        String::new(),
        "a".to_string(),
        "hello world".to_string(),
        "unicode ✓ é 日本語".to_string(),
        "embedded\u{0}nul".to_string(),
        "x".repeat(9_000),
    ]
}

#[test]
fn test_append_tagged_is_byte_identical_to_the_concatenation() {
    let dir = std::env::temp_dir().join("forgedb_test_append_tagged_bytes");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let (new_data, new_offs) = (dir.join("new_data.bin"), dir.join("new_offs.bin"));
    let (old_data, old_offs) = (dir.join("old_data.bin"), dir.join("old_offs.bin"));

    let mut new_col = VariableColumn::new(new_data.clone(), new_offs.clone()).unwrap();
    let mut old_col = VariableColumn::new(old_data.clone(), old_offs.clone()).unwrap();

    for value in tagged_corpus() {
        // The `Some` arm, and the `None` arm interleaved with it, so the offsets
        // index has to stay aligned across a zero-length tagged row.
        new_col.append_tagged(1, &value).unwrap();
        new_col.append_tagged(0, "").unwrap();

        // Exactly what generated code used to build by hand.
        let mut encoded = String::with_capacity(value.len() + 1);
        encoded.push('\u{1}');
        encoded.push_str(&value);
        old_col.append_string(&encoded).unwrap();
        old_col.append_string(&String::from('\u{0}')).unwrap();
    }
    new_col.flush().unwrap();
    old_col.flush().unwrap();

    assert_eq!(
        fs::read(&new_data).unwrap(),
        fs::read(&old_data).unwrap(),
        "append_tagged must write the same data bytes as the concatenation"
    );
    assert_eq!(
        fs::read(&new_offs).unwrap(),
        fs::read(&old_offs).unwrap(),
        "append_tagged must write the same (offset, length) entries"
    );

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_append_tagged_round_trips_through_both_readers() {
    let dir = std::env::temp_dir().join("forgedb_test_append_tagged_roundtrip");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    let mut col =
        VariableColumn::new(dir.join("data.bin"), dir.join("offs.bin")).unwrap();

    let corpus = tagged_corpus();
    for value in &corpus {
        col.append_tagged(1, value).unwrap();
        col.append_tagged(0, "").unwrap();
    }

    // read_string: the tagged row decodes to tag + value, and a None row is
    // exactly one byte — the distinction `Some("")` vs `None` depends on it.
    for (i, value) in corpus.iter().enumerate() {
        let some = col.read_string(i * 2).unwrap();
        assert_eq!(some, format!("\u{1}{value}"));
        let none = col.read_string(i * 2 + 1).unwrap();
        assert_eq!(none, "\u{0}");
        assert_ne!(some, none, "Some(\"\") must not read back as None");
    }

    // read_str (#224): the borrowed path generated scans use must agree.
    let indices: Vec<usize> = (0..corpus.len() * 2).collect();
    let buf = col.gather_buffered(&indices).unwrap();
    for slot in 0..indices.len() {
        assert_eq!(buf.read_str(slot).unwrap(), buf.read_string(slot).unwrap());
    }

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_append_tagged_survives_reopen() {
    let dir = std::env::temp_dir().join("forgedb_test_append_tagged_reopen");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let (data, offs) = (dir.join("data.bin"), dir.join("offs.bin"));

    {
        let mut col = VariableColumn::new(data.clone(), offs.clone()).unwrap();
        col.append_tagged(0, "").unwrap();
        col.append_tagged(1, "kept").unwrap();
        col.flush().unwrap();
    }

    // The row count and the running data offset are both recovered from file
    // lengths on open; a tagged row's length includes its tag byte, so an
    // off-by-one here would silently corrupt every subsequent append.
    let mut col = VariableColumn::new(data, offs).unwrap();
    assert_eq!(col.len(), 2);
    col.append_tagged(1, "after").unwrap();
    assert_eq!(col.read_string(0).unwrap(), "\u{0}");
    assert_eq!(col.read_string(1).unwrap(), "\u{1}kept");
    assert_eq!(col.read_string(2).unwrap(), "\u{1}after");

    fs::remove_dir_all(&dir).unwrap();
}
