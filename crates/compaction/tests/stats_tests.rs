use forgedb_compaction::stats::*;
use forgedb_compaction::types::ColumnType;
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
fn test_collect_model_stats() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();
    fs::create_dir_all(model_dir.join("variable")).unwrap();

    write_manifest(&model_dir, 3);

    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[0x00, 0x01, 0x00]).unwrap();

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[0u8; 48]).unwrap();

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

    let collector = StatsCollector::new(data_dir);
    let stats = collector.collect_model_stats("User").unwrap();

    assert_eq!(stats.name, "User");
    assert_eq!(stats.total_rows, 3);
    assert_eq!(stats.active_rows, 2);
    assert_eq!(stats.deleted_rows, 1);
    assert_eq!(stats.columns.len(), 2);

    let id_col = stats.columns.iter().find(|c| c.name == "id").unwrap();
    assert_eq!(id_col.column_type, ColumnType::Fixed);
    assert_eq!(id_col.total_bytes, 48);
    assert_eq!(id_col.used_bytes, 32);
    assert_eq!(id_col.dead_bytes, 16);

    let email_col = stats.columns.iter().find(|c| c.name == "email").unwrap();
    assert_eq!(email_col.column_type, ColumnType::Variable);
    assert!(email_col.dead_bytes > 0);
}

#[test]
fn test_collect_database_stats() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    for model_name in &["User", "Post"] {
        let model_dir = data_dir.join(model_name);
        fs::create_dir_all(model_dir.join("fixed")).unwrap();

        write_manifest(&model_dir, 1);

        let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
        tombstone_file.write_all(&[0u8]).unwrap();

        let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
        id_file.write_all(&[0u8; 16]).unwrap();
    }

    let collector = StatsCollector::new(data_dir);
    let stats = collector.collect_database_stats().unwrap();

    assert_eq!(stats.models.len(), 2);
    assert!(stats.total_disk_bytes > 0);
}
