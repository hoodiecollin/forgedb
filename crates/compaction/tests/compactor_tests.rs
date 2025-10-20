use forgedb_compaction::compactor::*;
use forgedb_compaction::stats::StatsCollector;
use forgedb_compaction::types::*;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_compact_variable_column() {
    let temp_dir = TempDir::new().unwrap();
    let variable_dir = temp_dir.path();

    // Create test data with 3 rows, middle row deleted
    let data_path = variable_dir.join("email_data.bin");
    let offset_path = variable_dir.join("email_offsets.bin");

    let mut data_file = fs::File::create(&data_path).unwrap();
    data_file
        .write_all(b"alice@example.combob@example.comcharlie@example.com")
        .unwrap();

    let mut offset_file = fs::File::create(&offset_path).unwrap();
    offset_file.write_all(&0u64.to_le_bytes()).unwrap();
    offset_file.write_all(&17u64.to_le_bytes()).unwrap();
    offset_file.write_all(&17u64.to_le_bytes()).unwrap();
    offset_file.write_all(&15u64.to_le_bytes()).unwrap();
    offset_file.write_all(&32u64.to_le_bytes()).unwrap();
    offset_file.write_all(&19u64.to_le_bytes()).unwrap();

    let tombstones = vec![false, true, false]; // Middle row deleted

    let compactor = Compactor::new(temp_dir.path(), CompactionConfig::default());
    compactor
        .compact_variable_column(&data_path, &offset_path, &tombstones)
        .unwrap();

    // Read compacted data
    let compacted_data = fs::read(&data_path).unwrap();
    let expected = b"alice@example.comcharlie@example.com";
    assert_eq!(compacted_data, expected);

    // Read compacted offsets
    let offset_bytes = fs::read(&offset_path).unwrap();
    assert_eq!(offset_bytes.len(), 32); // 2 entries * 16 bytes

    let offset1 = u64::from_le_bytes(offset_bytes[0..8].try_into().unwrap());
    let length1 = u64::from_le_bytes(offset_bytes[8..16].try_into().unwrap());
    assert_eq!(offset1, 0);
    assert_eq!(length1, 17);

    let offset2 = u64::from_le_bytes(offset_bytes[16..24].try_into().unwrap());
    let length2 = u64::from_le_bytes(offset_bytes[24..32].try_into().unwrap());
    assert_eq!(offset2, 17);
    assert_eq!(length2, 19);
}

#[test]
fn test_compact_fixed_column() {
    let temp_dir = TempDir::new().unwrap();
    let column_path = temp_dir.path().join("id.bin");

    // Create test data with 3 UUIDs (16 bytes each), middle row deleted
    let mut file = fs::File::create(&column_path).unwrap();
    file.write_all(&[1u8; 16]).unwrap(); // UUID 1
    file.write_all(&[2u8; 16]).unwrap(); // UUID 2 (deleted)
    file.write_all(&[3u8; 16]).unwrap(); // UUID 3

    let tombstones = vec![false, true, false]; // Middle row deleted

    let compactor = Compactor::new(temp_dir.path(), CompactionConfig::default());
    compactor
        .compact_fixed_column(&column_path, &tombstones)
        .unwrap();

    // Read compacted data
    let compacted_data = fs::read(&column_path).unwrap();
    assert_eq!(compacted_data.len(), 32); // 2 UUIDs * 16 bytes

    assert_eq!(&compacted_data[0..16], &[1u8; 16]);
    assert_eq!(&compacted_data[16..32], &[3u8; 16]);
}

#[test]
fn test_compact_model() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    // Create model directory structure
    fs::create_dir_all(model_dir.join("fixed")).unwrap();
    fs::create_dir_all(model_dir.join("variable")).unwrap();

    // Create tombstone file (3 rows, middle row deleted)
    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[0b00000010]).unwrap(); // Second bit set

    // Create fixed column
    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[1u8; 16]).unwrap();
    id_file.write_all(&[2u8; 16]).unwrap();
    id_file.write_all(&[3u8; 16]).unwrap();

    // Create variable column
    let mut email_data = fs::File::create(model_dir.join("variable/email_data.bin")).unwrap();
    email_data
        .write_all(b"alice@example.combob@example.comcharlie@example.com")
        .unwrap();

    let mut email_offsets =
        fs::File::create(model_dir.join("variable/email_offsets.bin")).unwrap();
    email_offsets.write_all(&0u64.to_le_bytes()).unwrap();
    email_offsets.write_all(&17u64.to_le_bytes()).unwrap();
    email_offsets.write_all(&17u64.to_le_bytes()).unwrap();
    email_offsets.write_all(&15u64.to_le_bytes()).unwrap();
    email_offsets.write_all(&32u64.to_le_bytes()).unwrap();
    email_offsets.write_all(&19u64.to_le_bytes()).unwrap();

    let compactor = Compactor::new(data_dir, CompactionConfig::default());
    let result = compactor.compact_model("User").unwrap();

    assert!(result.success);
    assert!(result.bytes_reclaimed > 0);
    assert_eq!(result.model_name, "User");

    // Verify compacted data
    let id_data = fs::read(model_dir.join("fixed/id.bin")).unwrap();
    assert_eq!(id_data.len(), 32); // 2 rows * 16 bytes

    let email_data = fs::read(model_dir.join("variable/email_data.bin")).unwrap();
    assert_eq!(email_data, b"alice@example.comcharlie@example.com");

    // Verify tombstone bitmap reset
    let tombstones = fs::read(model_dir.join("tombstones.bin")).unwrap();
    assert_eq!(tombstones, vec![0u8]); // All clear (2 active rows fit in 1 byte)
}

#[test]
fn test_compact_reclaims_space() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    // Create 10 rows with 5 deleted (50% dead space)
    let mut tombstone_bytes = vec![0u8; 2]; // 10 rows needs 2 bytes
    tombstone_bytes[0] = 0b01010101; // Alternating pattern
    tombstone_bytes[1] = 0b00000001; // One more deleted

    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&tombstone_bytes).unwrap();

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[0u8; 160]).unwrap(); // 10 rows * 16 bytes

    let collector = StatsCollector::new(data_dir);
    let _stats_before = collector.collect_model_stats("User").unwrap();

    let compactor = Compactor::new(data_dir, CompactionConfig::default());
    let result = compactor.compact_model("User").unwrap();

    let stats_after = collector.collect_model_stats("User").unwrap();

    // Should reclaim ~50% of space
    assert!(result.bytes_reclaimed > 0);
    let reclaim_pct = result.reclaim_percentage();
    assert!(reclaim_pct > 40.0 && reclaim_pct < 60.0);

    // Active rows should be preserved
    assert_eq!(stats_after.active_rows, 5);
    assert_eq!(stats_after.deleted_rows, 0);
}
