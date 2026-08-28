use forgedb_compaction::*;
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
fn test_maintenance_api() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    write_manifest(&model_dir, 3);

    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[0x00, 0x01, 0x00]).unwrap();

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[0u8; 48]).unwrap();

    let api = MaintenanceApi::new(data_dir, CompactionConfig::default());

    let db_stats = api.stats().unwrap();
    assert_eq!(db_stats.models.len(), 1);

    let model_stats = api.model_stats("User").unwrap();
    assert_eq!(model_stats.name, "User");
    assert_eq!(model_stats.total_rows, 3);
    assert_eq!(model_stats.deleted_rows, 1);

    let result = api.compact_model("User").unwrap();
    assert!(result.success);
    assert!(result.bytes_reclaimed > 0);

    let model_stats_after = api.model_stats("User").unwrap();
    assert_eq!(model_stats_after.active_rows, 2);
    assert_eq!(model_stats_after.deleted_rows, 0);
}

#[test]
fn test_vacuum_and_analyze() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    write_manifest(&model_dir, 8);

    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[1u8, 0, 1, 0, 1, 0, 1, 0]).unwrap();

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[0u8; 128]).unwrap();

    let api = MaintenanceApi::new(data_dir, CompactionConfig::default());

    let stats_before = api.analyze().unwrap();
    assert!(stats_before.dead_bytes > 0);

    let results = api.vacuum().unwrap();
    assert!(!results.is_empty());
    assert!(results[0].success);

    let stats_after = api.analyze().unwrap();
    assert_eq!(stats_after.dead_bytes, 0);
}
