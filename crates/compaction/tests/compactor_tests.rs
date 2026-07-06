use forgedb_compaction::compactor::*;
use forgedb_compaction::stats::StatsCollector;
use forgedb_compaction::types::*;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

/// Write a minimal `manifest.json` with the given row_count into `model_dir`.
fn write_manifest(model_dir: &std::path::Path, row_count: usize) {
    let manifest = format!(
        r#"{{"schema_version":1,"row_count":{},"columns":[],"wal_enabled":false,"last_checkpoint":0}}"#,
        row_count
    );
    fs::write(model_dir.join("manifest.json"), manifest).unwrap();
}

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

    // Write manifest with row_count=3 (required by C1 fix)
    write_manifest(&model_dir, 3);

    // Create tombstone file (3 rows, middle row deleted): one byte per row.
    // 0x00 = alive, 0x01 = deleted.
    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[0x00, 0x01, 0x00]).unwrap(); // Row 1 deleted

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

    // Verify tombstone file reset: one byte per surviving row, all 0x00.
    let tombstones = fs::read(model_dir.join("tombstones.bin")).unwrap();
    assert_eq!(tombstones, vec![0u8, 0u8]); // 2 active rows, 1 byte each
}

#[test]
fn test_compact_reclaims_space() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    // Write manifest with row_count=10 (required by C1 fix)
    write_manifest(&model_dir, 10);

    // Create 10 rows with 5 deleted (50% dead space): one byte per row.
    // Rows 0,2,4,6,8 deleted (0x01); rows 1,3,5,7,9 alive (0x00).
    let tombstone_bytes: [u8; 10] = [1, 0, 1, 0, 1, 0, 1, 0, 1, 0];
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

    // Active rows should be preserved (manifest updated to active_count)
    assert_eq!(stats_after.active_rows, 5);
    assert_eq!(stats_after.deleted_rows, 0);
}

/// C1 regression test: mixed-width fixed columns (u32 + uuid) with tombstoned rows.
///
/// Before the fix, `read_tombstone_bitmap` guessed the row count by trying element
/// sizes [16,8,4,1] on the first fixed column file.  When the first file was a u32
/// column (4 bytes/row), the heuristic could pick element_size=8 → row_count=5 instead
/// of 10, then `compact_fixed_column` computed element_size=160/5=32 for the uuid
/// column — corrupting every row.
///
/// After the fix the row count comes exclusively from `manifest.json`, so both columns
/// get the correct element size regardless of file ordering.
#[test]
fn test_compact_mixed_width_columns_preserves_data() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("TestModel");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    // 10 rows, rows 1,3,5,7,9 deleted (even-indexed rows are active: 0,2,4,6,8).
    // One byte per row: 0x00 = alive, 0x01 = deleted.
    let tombstone_bytes: [u8; 10] = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1];
    fs::write(model_dir.join("tombstones.bin"), tombstone_bytes).unwrap();

    // Manifest with row_count=10
    write_manifest(&model_dir, 10);

    // u32 column (4 bytes/row): values [0, 1, 2, ..., 9]
    // Use an alphabetically-early name so the directory iterator is likely to
    // visit this file before the uuid column — the heuristic path that the fix
    // replaces would mis-derive row_count from this 40-byte file.
    let mut score_file =
        fs::File::create(model_dir.join("fixed/aa_score.bin")).unwrap();
    for v in 0u32..10 {
        score_file.write_all(&v.to_le_bytes()).unwrap();
    }

    // uuid column (16 bytes/row): row N is [N as u8; 16]
    let mut id_file =
        fs::File::create(model_dir.join("fixed/zz_id.bin")).unwrap();
    for n in 0u8..10 {
        id_file.write_all(&[n; 16]).unwrap();
    }

    let compactor = Compactor::new(data_dir, CompactionConfig::default());
    compactor.compact_model("TestModel").unwrap();

    // --- Verify u32 column ---
    // Active rows: 0,2,4,6,8 → values [0,2,4,6,8]
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

    // --- Verify uuid column ---
    // Active rows: 0,2,4,6,8 → [0;16], [2;16], [4;16], [6;16], [8;16]
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
