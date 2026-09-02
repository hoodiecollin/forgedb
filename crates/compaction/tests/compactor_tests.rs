use forgedb_compaction::compactor::*;
use forgedb_compaction::stats::StatsCollector;
use forgedb_compaction::types::*;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_manifest(model_dir: &std::path::Path, row_count: usize) {
    let manifest = format!(
        r#"{{"format_version":1,"row_count":{},"columns":[],"wal_enabled":false,"last_checkpoint":0}}"#,
        row_count
    );
    fs::write(model_dir.join("manifest.json"), manifest).unwrap();
}

#[test]
fn test_compact_variable_column() {
    let temp_dir = TempDir::new().unwrap();
    let variable_dir = temp_dir.path();

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

    let tombstones = vec![false, true, false];

    let compactor = Compactor::new(temp_dir.path(), CompactionConfig::default());
    compactor
        .compact_variable_column(&data_path, &offset_path, &tombstones)
        .unwrap();

    let compacted_data = fs::read(&data_path).unwrap();
    let expected = b"alice@example.comcharlie@example.com";
    assert_eq!(compacted_data, expected);

    let offset_bytes = fs::read(&offset_path).unwrap();
    assert_eq!(offset_bytes.len(), 32);

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

    let mut file = fs::File::create(&column_path).unwrap();
    file.write_all(&[1u8; 16]).unwrap();
    file.write_all(&[2u8; 16]).unwrap();
    file.write_all(&[3u8; 16]).unwrap();

    let tombstones = vec![false, true, false];

    let compactor = Compactor::new(temp_dir.path(), CompactionConfig::default());
    compactor
        .compact_fixed_column(&column_path, &tombstones)
        .unwrap();

    let compacted_data = fs::read(&column_path).unwrap();
    assert_eq!(compacted_data.len(), 32);

    assert_eq!(&compacted_data[0..16], &[1u8; 16]);
    assert_eq!(&compacted_data[16..32], &[3u8; 16]);
}

#[test]
fn test_compact_model() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();
    fs::create_dir_all(model_dir.join("variable")).unwrap();

    write_manifest(&model_dir, 3);

    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[0x00, 0x01, 0x00]).unwrap();

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[1u8; 16]).unwrap();
    id_file.write_all(&[2u8; 16]).unwrap();
    id_file.write_all(&[3u8; 16]).unwrap();

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

    let id_data = fs::read(model_dir.join("fixed/id.bin")).unwrap();
    assert_eq!(id_data.len(), 32);

    let email_data = fs::read(model_dir.join("variable/email_data.bin")).unwrap();
    assert_eq!(email_data, b"alice@example.comcharlie@example.com");

    let tombstones = fs::read(model_dir.join("tombstones.bin")).unwrap();
    assert_eq!(tombstones, vec![0u8, 0u8]);
}

#[test]
fn test_compact_reclaims_space() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    write_manifest(&model_dir, 10);

    let tombstone_bytes: [u8; 10] = [1, 0, 1, 0, 1, 0, 1, 0, 1, 0];
    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&tombstone_bytes).unwrap();

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[0u8; 160]).unwrap();

    let collector = StatsCollector::new(data_dir);
    let _stats_before = collector.collect_model_stats("User").unwrap();

    let compactor = Compactor::new(data_dir, CompactionConfig::default());
    let result = compactor.compact_model("User").unwrap();

    let stats_after = collector.collect_model_stats("User").unwrap();

    assert!(result.bytes_reclaimed > 0);
    let reclaim_pct = result.reclaim_percentage();
    assert!(reclaim_pct > 40.0 && reclaim_pct < 60.0);

    assert_eq!(stats_after.active_rows, 5);
    assert_eq!(stats_after.deleted_rows, 0);
}

#[test]
fn test_compact_mixed_width_columns_preserves_data() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("TestModel");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    let tombstone_bytes: [u8; 10] = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
    fs::write(model_dir.join("tombstones.bin"), tombstone_bytes).unwrap();

    write_manifest(&model_dir, 10);

    let mut score_file =
        fs::File::create(model_dir.join("fixed/aa_score.bin")).unwrap();
    for v in 0u32..10 {
        score_file.write_all(&v.to_le_bytes()).unwrap();
    }

    let mut id_file =
        fs::File::create(model_dir.join("fixed/zz_id.bin")).unwrap();
    for n in 0u8..10 {
        id_file.write_all(&[n; 16]).unwrap();
    }

    let compactor = Compactor::new(data_dir, CompactionConfig::default());
    compactor.compact_model("TestModel").unwrap();

    let score_data = fs::read(model_dir.join("fixed/aa_score.bin")).unwrap();
    assert_eq!(
        score_data.len(),
        5 * 4,
        "u32 column: expected 5 rows × 4 bytes = 20 bytes after compaction"
    );
    let score_vals: Vec<u32> = score_data
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(
        score_vals,
        vec![0u32, 2, 4, 6, 8],
        "u32 active rows must be preserved in order"
    );

    let id_data = fs::read(model_dir.join("fixed/zz_id.bin")).unwrap();
    assert_eq!(
        id_data.len(),
        5 * 16,
        "uuid column: expected 5 rows × 16 bytes = 80 bytes after compaction"
    );
    assert_eq!(&id_data[0..16], &[0u8; 16], "Row 0 UUID preserved");
    assert_eq!(&id_data[16..32], &[2u8; 16], "Row 2 UUID preserved");
    assert_eq!(&id_data[32..48], &[4u8; 16], "Row 4 UUID preserved");
    assert_eq!(&id_data[48..64], &[6u8; 16], "Row 6 UUID preserved");
    assert_eq!(&id_data[64..80], &[8u8; 16], "Row 8 UUID preserved");
}
