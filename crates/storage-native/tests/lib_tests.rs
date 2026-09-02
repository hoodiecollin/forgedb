use forgedb_storage_native::*;
use std::fs;

#[test]
fn test_fixed_column_explicit_flush_durability() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_flush_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let path = temp_dir.join("test_flush.bin");

    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(111).unwrap();
        col.append_u64(222).unwrap();
        col.append_u64(333).unwrap();
        col.flush().unwrap();
    }

    {
        let col = FixedColumn::new(path.clone(), 8).unwrap();
        assert_eq!(col.len(), 3);
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
        col.flush().unwrap();
    }

    {
        let col = VariableColumn::new(data_path, offsets_path).unwrap();
        assert_eq!(col.len(), 2);
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

    col.append_u64(42).unwrap();
    col.append_u64(100).unwrap();
    col.append_u64(999).unwrap();

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

    col.append_string("hello").unwrap();
    col.append_string("world").unwrap();
    col.append_string("test@example.com").unwrap();

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
        engine_version: 1,
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
        row_anchor: None,
        auto_sequences: Default::default(),
    };
    manifest.save_to(&path).unwrap();

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

    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(100).unwrap();
        col.append_u64(200).unwrap();
        col.append_u64(300).unwrap();
    }

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

    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        col.append_string("first").unwrap();
        col.append_string("second").unwrap();
        col.append_string("third").unwrap();
    }

    {
        let mut col = VariableColumn::new(data_path.clone(), offsets_path.clone()).unwrap();
        assert_eq!(col.len(), 3);
        assert_eq!(col.read_string(0).unwrap(), "first");
        assert_eq!(col.read_string(1).unwrap(), "second");
        assert_eq!(col.read_string(2).unwrap(), "third");

        col.append_string("fourth").unwrap();
    }

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

    let now = 1704067200i64;
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

    let bytes_wrong_size = [1u8; 10];

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

#[test]
fn test_snapshot_visibility_boundary() {
    let snap = Snapshot::new(3);
    assert_eq!(snap.watermark(), 3);
    assert!(snap.visible(0));
    assert!(snap.visible(2));
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
    let snap = Snapshot::new(2);
    let also = snap;
    assert_eq!(snap.watermark(), also.watermark());
    assert!(snap.visible(1));
    assert!(!snap.visible(2));
}

#[test]
fn test_fixed_column_reader_sees_writer_appends() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_reader_fixed_sees");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let path = temp_dir.join("c.bin");

    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    col.append_u64(10).unwrap();

    let reader = col.reader().unwrap();
    assert_eq!(reader.len(), 1);
    assert_eq!(reader.read_u64(0).unwrap(), 10);

    col.append_u64(20).unwrap();
    col.append_u64(30).unwrap();
    assert_eq!(reader.len(), 3, "reader derives length live from the shared file");
    assert_eq!(reader.read_u64(1).unwrap(), 20);
    assert_eq!(reader.read_u64(2).unwrap(), 30);

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
    col.append_u64(0).unwrap();

    let reader = Arc::new(col.reader().unwrap());
    let stop = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for _ in 0..4 {
        let reader = Arc::clone(&reader);
        let stop = Arc::clone(&stop);
        handles.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let n = reader.len();
                for i in 0..n {
                    let v = reader.read_u64(i).expect("committed index readable");
                    assert_eq!(v, (i as u64) * 2, "reader saw a torn value at {i}");
                }
            }
        }));
    }

    for i in 1u64..5000 {
        col.append_u64(i * 2).unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(reader.len(), 5000);
    assert_eq!(reader.read_u64(4999).unwrap(), 4999 * 2);

    fs::remove_dir_all(&temp_dir).unwrap();
}

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

    col.truncate_to_rows(2).unwrap();
    assert_eq!(col.len(), 2);
    assert_eq!(col.read_u64(0).unwrap(), 10);
    assert_eq!(col.read_u64(1).unwrap(), 20);
    assert!(col.read_u64(2).is_err());

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

    col.truncate_to_rows(0).unwrap();
    assert_eq!(col.len(), 0);
    assert!(col.is_empty());
    assert!(col.read_u64(0).is_err());

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

    {
        let mut col = FixedColumn::new(path.clone(), 8).unwrap();
        col.append_u64(10).unwrap();
        col.append_u64(20).unwrap();
        col.append_u64(30).unwrap();
    }

    {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(24 + 3).unwrap();
    }

    let mut col = FixedColumn::new(path.clone(), 8).unwrap();
    assert_eq!(col.len(), 3, "partial row is excluded from row_count");
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        27,
        "stray bytes still present before truncate"
    );

    col.truncate_to_rows(col.len()).unwrap();
    assert_eq!(col.len(), 3);
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        24,
        "file length is exact multiple of value_size after truncate"
    );

    assert_eq!(col.read_u64(0).unwrap(), 10);
    assert_eq!(col.read_u64(1).unwrap(), 20);
    assert_eq!(col.read_u64(2).unwrap(), 30);

    col.append_u64(40).unwrap();
    assert_eq!(col.len(), 4);
    assert_eq!(col.read_u64(3).unwrap(), 40);
    assert_eq!(fs::metadata(&path).unwrap().len(), 32);

    fs::remove_dir_all(&temp_dir).unwrap();
}

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

    col.truncate_to_rows(2).unwrap();
    assert_eq!(col.len(), 2);
    assert_eq!(col.read_string(0).unwrap(), "alpha");
    assert_eq!(col.read_string(1).unwrap(), "beta");
    assert!(col.read_string(2).is_err());

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

    assert_eq!(fs::metadata(&data_path).unwrap().len(), 0);
    assert_eq!(fs::metadata(&offsets_path).unwrap().len(), 0);

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

    t.truncate_to_rows(2).unwrap();
    assert_eq!(t.len(), 2);
    assert!(!t.is_deleted(0).unwrap());
    assert!(t.is_deleted(1).unwrap());
    assert!(t.is_deleted(2).is_err());

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

#[test]
fn test_dir_lock_acquire_fresh_dir_succeeds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_dirlock_fresh");
    let _ = fs::remove_dir_all(&temp_dir);

    let lock = DirLock::acquire(&temp_dir).unwrap();
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

    let err = DirLock::acquire(&temp_dir).unwrap_err();
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::WouldBlock,
        "second acquire must be refused while first DirLock is alive"
    );

    drop(lock1);
    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_dir_lock_reacquire_after_drop_succeeds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_dirlock_reacquire");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let lock1 = DirLock::acquire(&temp_dir).unwrap();
    drop(lock1);

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

    let lock = DirLock::acquire(&temp_dir).unwrap();
    assert!(temp_dir.exists());
    assert!(temp_dir.join(".forgedb.lock").exists());
    drop(lock);

    fs::remove_dir_all(std::env::temp_dir().join("forgedb_test_dirlock_mkdir")).unwrap();
}

#[test]
fn test_fixed_column_gather_selects_and_orders() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_gather_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("g.bin"), 8).unwrap();
    for v in [10u64, 11, 12, 13, 14] {
        col.append_u64(v).unwrap();
    }

    let out = col.gather(&[3, 0, 4]).unwrap();
    assert_eq!(out.len(), 3 * 8);
    assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 13);
    assert_eq!(u64::from_le_bytes(out[8..16].try_into().unwrap()), 10);
    assert_eq!(u64::from_le_bytes(out[16..24].try_into().unwrap()), 14);

    assert!(col.gather(&[]).unwrap().is_empty());

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

    let out = col.gather(&[0, 1, 2, 3]).unwrap();
    for i in 0..4usize {
        let got = u32::from_le_bytes(out[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(got, col.read_u32(i).unwrap());
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_gather_mapped_path_matches_per_row_reads() {
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

    let holey: Vec<usize> = (0..1000).filter(|i| i % 3 != 0).collect();
    assert_eq!(decode(&col.gather(&holey).unwrap()), expect(&holey));

    let run: Vec<usize> = (500..900).collect();
    assert_eq!(decode(&col.gather(&run).unwrap()), expect(&run));

    let rev: Vec<usize> = (0..64).rev().collect();
    assert_eq!(decode(&col.gather(&rev).unwrap()), expect(&rev));

    let sparse: Vec<usize> = (0..1000).step_by(97).collect();
    assert_eq!(decode(&col.gather(&sparse).unwrap()), expect(&sparse));

    let dupes: Vec<usize> = vec![9, 9, 9, 4, 4, 800, 800, 0, 0, 0];
    assert_eq!(decode(&col.gather(&dupes).unwrap()), expect(&dupes));

    for n in [7usize, 8, 9] {
        let idx: Vec<usize> = (0..n).map(|i| i * 3).collect();
        assert_eq!(decode(&col.gather(&idx).unwrap()), expect(&idx));
    }

    let mut bad: Vec<usize> = (0..100).collect();
    bad.push(1000);
    assert!(col.gather(&bad).is_err());

    let buf = col.gather_buffered(&holey).unwrap();
    for (slot, &i) in holey.iter().enumerate() {
        assert_eq!(buf.read_u64(slot).unwrap(), col.read_u64(i).unwrap());
    }

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_fixed_column_export_aliases_dense_prefix_else_gathers() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_export_alias");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("e.bin"), 8).unwrap();
    for v in [100u64, 101, 102, 103, 104] {
        col.append_u64(v).unwrap();
    }

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

    let full = col.export(&[0, 1, 2, 3, 4]).unwrap();
    assert!(matches!(full, ColumnExport::Mapped(_)), "the whole-column prefix aliases too");
    assert_eq!(full.len(), 5 * 8);

    let gathered = col.export(&[3, 0, 4]).unwrap();
    assert!(
        matches!(gathered, ColumnExport::Owned(_)),
        "a non-prefix selection must gather an owned copy"
    );
    let g = gathered.as_slice();
    assert_eq!(u64::from_le_bytes(g[0..8].try_into().unwrap()), 103);
    assert_eq!(u64::from_le_bytes(g[8..16].try_into().unwrap()), 100);
    assert_eq!(u64::from_le_bytes(g[16..24].try_into().unwrap()), 104);

    assert!(matches!(col.export(&[0, 1]).unwrap(), ColumnExport::Mapped(_)));
    assert!(matches!(col.export(&[1, 2, 3]).unwrap(), ColumnExport::Owned(_)));

    let empty = col.export(&[]).unwrap();
    assert!(matches!(empty, ColumnExport::Owned(_)));
    assert!(empty.is_empty());

    assert!(col.export(&[0, 5]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_fixed_column_matches_per_row_reads() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_fixed");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("bf.bin"), 8).unwrap();
    for v in [10u64, 11, 12, 13, 14] {
        col.append_u64(v).unwrap();
    }

    let buf = col.gather_buffered(&[0, 1, 2, 3, 4]).unwrap();
    assert_eq!(buf.len(), 5);
    for slot in 0..5usize {
        assert_eq!(buf.read_u64(slot).unwrap(), col.read_u64(slot).unwrap());
    }

    let buf = col.gather_buffered(&[3, 0, 4]).unwrap();
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.read_u64(0).unwrap(), 13);
    assert_eq!(buf.read_u64(1).unwrap(), 10);
    assert_eq!(buf.read_u64(2).unwrap(), 14);
    assert!(buf.read_u64(3).is_err());

    assert!(col.gather_buffered(&[0, 5]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_fixed_column_all_read_kinds() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_kinds");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut ucol = FixedColumn::new(temp_dir.join("u.bin"), 16).unwrap();
    ucol.append_uuid([1u8; 16]).unwrap();
    ucol.append_uuid([2u8; 16]).unwrap();
    let ubuf = ucol.gather_buffered(&[0, 1]).unwrap();
    assert_eq!(ubuf.read_uuid(0).unwrap(), [1u8; 16]);
    assert_eq!(ubuf.read_bytes(1).unwrap(), vec![2u8; 16]);

    let mut bcol = FixedColumn::new(temp_dir.join("b.bin"), 1).unwrap();
    bcol.append_bool(true).unwrap();
    bcol.append_bool(false).unwrap();
    let bbuf = bcol.gather_buffered(&[0, 1]).unwrap();
    assert!(bbuf.read_bool(0).unwrap());
    assert!(!bbuf.read_bool(1).unwrap());

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

    let buf = col.gather_buffered(&[0, 1, 2, 3]).unwrap();
    assert_eq!(buf.len(), 4);
    for slot in 0..4usize {
        assert_eq!(buf.read_string(slot).unwrap(), col.read_string(slot).unwrap());
    }

    let buf = col.gather_buffered(&[3, 0, 2]).unwrap();
    assert_eq!(buf.read_string(0).unwrap(), "unicode ✓ é");
    assert_eq!(buf.read_string(1).unwrap(), "");
    assert_eq!(buf.read_string(2).unwrap(), "hello world");
    assert!(buf.read_string(3).is_err());

    assert!(col.gather_buffered(&[0, 4]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_column_mapped_path_matches_per_row_reads() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_var_mapped");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = VariableColumn::new(
        temp_dir.join("v_data.bin"),
        temp_dir.join("v_offsets.bin"),
    )
    .unwrap();

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

    let holey: Vec<usize> = (0..n).filter(|i| i % 3 != 0).collect();
    check(&holey);

    let tail: Vec<usize> = (3000..3500).collect();
    check(&tail);

    let rev: Vec<usize> = (2000..2400).rev().collect();
    check(&rev);

    let sparse: Vec<usize> = (0..n).step_by(97).collect();
    check(&sparse);

    check(&[3999, 3999, 0, 0, 2500, 2500]);

    let empties: Vec<usize> = (0..n).filter(|i| i % 37 == 0).collect();
    check(&empties);

    let mut bad: Vec<usize> = (0..100).collect();
    bad.push(n);
    assert!(col.gather_buffered(&bad).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_column_sparse_offsets_path_matches_spanned() {
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

    check("two rows, full span", &[3, n - 2]);
    check("single row", &[n / 2]);
    check("20 scattered", &(0..20).map(|i| i * (n / 20)).collect::<Vec<_>>());
    check("dense run", &(1000..1400).collect::<Vec<_>>());
    check("just dense", &(0..8).map(|i| i * 100).collect::<Vec<_>>());
    check("just sparse", &(0..8).map(|i| i * 200).collect::<Vec<_>>());
    check("reversed sparse", &(0..10).rev().map(|i| i * 450).collect::<Vec<_>>());
    check("duplicates", &[n - 1, 0, n - 1, 0]);
    check("sparse empties", &(0..n).filter(|i| i % 41 == 0).step_by(30).collect::<Vec<_>>());

    assert!(col.gather_buffered(&[0, n]).is_err());
    assert!(col.gather_buffered(&[n - 1, n + 5]).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_read_str_matches_read_string() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_buffered_read_str");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

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
    let err = buf.read_str(4).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(buf.read_string(4).unwrap_err().kind(), err.kind());

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
    check(&(0..n).filter(|i| i % 3 != 0).collect::<Vec<_>>());
    check(&(3000..3500).collect::<Vec<_>>());
    check(&(2000..2400).rev().collect::<Vec<_>>());
    check(&(0..n).filter(|i| i % 37 == 0).collect::<Vec<_>>());

    let buf = big.gather_buffered(&[10, 11]).unwrap();
    let (a, b) = (buf.read_str(0).unwrap(), buf.read_str(1).unwrap());
    assert_ne!(a, b);

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn test_buffered_variable_read_str_rejects_invalid_utf8() {
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
    for deleted in [false, true, false, true, false] {
        t.append(deleted).unwrap();
    }

    assert_eq!(t.live_indices(&[0, 1, 2, 3, 4]).unwrap(), vec![0, 2, 4]);
    assert_eq!(t.live_indices(&[4, 3, 2, 1, 0]).unwrap(), vec![4, 2, 0]);
    assert_eq!(t.live_indices(&[0, 9, 2]).unwrap(), vec![0, 2]);
    assert!(t.live_indices(&[]).unwrap().is_empty());

    fs::remove_dir_all(&temp_dir).unwrap();
}

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

        ca.sync_to_drive().unwrap();
        cb.sync_to_drive().unwrap();
        cv.sync_to_drive().unwrap();
        ct.sync_to_drive().unwrap();
        ct.barrier().unwrap();
    }

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
        new_col.append_tagged(1, &value).unwrap();
        new_col.append_tagged(0, "").unwrap();

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

    for (i, value) in corpus.iter().enumerate() {
        let some = col.read_string(i * 2).unwrap();
        assert_eq!(some, format!("\u{1}{value}"));
        let none = col.read_string(i * 2 + 1).unwrap();
        assert_eq!(none, "\u{0}");
        assert_ne!(some, none, "Some(\"\") must not read back as None");
    }

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

    let mut col = VariableColumn::new(data, offs).unwrap();
    assert_eq!(col.len(), 2);
    col.append_tagged(1, "after").unwrap();
    assert_eq!(col.read_string(0).unwrap(), "\u{0}");
    assert_eq!(col.read_string(1).unwrap(), "\u{1}kept");
    assert_eq!(col.read_string(2).unwrap(), "\u{1}after");

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn buffered_fixed_column_read_slice_borrows_the_slot() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_bf_read_slice");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("s.bin"), 4).unwrap();
    for v in [b"aaaa", b"bbbb", b"cccc", b"dddd"] {
        col.append_bytes(v).unwrap();
    }

    let buf = col.gather_buffered(&[0, 1, 2, 3]).unwrap();
    for slot in 0..4usize {
        assert_eq!(buf.read_slice(slot).unwrap(), &buf.read_bytes(slot).unwrap()[..]);
    }
    assert_eq!(buf.read_slice(0).unwrap(), b"aaaa");
    assert_eq!(buf.read_slice(3).unwrap(), b"dddd");

    let buf = col.gather_buffered(&[3, 0]).unwrap();
    assert_eq!(buf.read_slice(0).unwrap(), b"dddd");
    assert_eq!(buf.read_slice(1).unwrap(), b"aaaa");

    assert!(buf.read_slice(2).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn buffered_fixed_column_read_str_is_the_whole_slot() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_bf_read_str");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("s.bin"), 3).unwrap();
    for v in [b"USD", b"EUR", b"JPY"] {
        col.append_bytes(v).unwrap();
    }

    let buf = col.gather_buffered(&[0, 1, 2]).unwrap();
    assert_eq!(buf.read_str(0).unwrap(), "USD");
    assert_eq!(buf.read_str(1).unwrap(), "EUR");
    assert_eq!(buf.read_str(2).unwrap(), "JPY");
    assert!(buf.read_str(3).is_err());

    fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn buffered_fixed_column_read_str_rejects_invalid_utf8() {
    let temp_dir = std::env::temp_dir().join("forgedb_test_bf_read_str_bad");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let mut col = FixedColumn::new(temp_dir.join("s.bin"), 2).unwrap();
    col.append_bytes(&[0xff, 0xfe]).unwrap();

    let buf = col.gather_buffered(&[0]).unwrap();
    let err = buf.read_str(0).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(buf.read_slice(0).unwrap(), &[0xff, 0xfe]);

    fs::remove_dir_all(&temp_dir).unwrap();
}
