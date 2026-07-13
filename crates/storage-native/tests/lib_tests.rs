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
