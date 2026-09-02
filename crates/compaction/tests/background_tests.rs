use forgedb_compaction::background::*;
use forgedb_compaction::types::{CompactionConfig, CompactionStatus};
use std::fs;
use std::io::Write;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn write_manifest(model_dir: &std::path::Path, row_count: usize) {
    let manifest = format!(
        r#"{{"format_version":1,"row_count":{},"columns":[],"wal_enabled":false,"last_checkpoint":0}}"#,
        row_count
    );
    fs::write(model_dir.join("manifest.json"), manifest).unwrap();
}

#[test]
fn test_background_compactor_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    let config = CompactionConfig {
        dead_space_threshold: 0.3,
        auto_compact: true,
        check_interval_secs: 1,
        max_compaction_time_secs: 60,
    };

    let bg_compactor = BackgroundCompactor::new(data_dir, config);

    assert!(!bg_compactor.is_running());
    assert_eq!(bg_compactor.status(), CompactionStatus::Idle);

    bg_compactor.start();
    assert!(bg_compactor.is_running());

    thread::sleep(Duration::from_millis(100));

    bg_compactor.stop();
}

#[test]
fn test_manual_trigger() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    write_manifest(&model_dir, 8);

    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[1u8, 0, 1, 0, 1, 0, 1, 0]).unwrap();

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[0u8; 128]).unwrap();

    let config = CompactionConfig {
        dead_space_threshold: 0.3,
        auto_compact: false,
        check_interval_secs: 3600,
        max_compaction_time_secs: 60,
    };

    let bg_compactor = BackgroundCompactor::new(data_dir, config);

    bg_compactor.trigger_manual().unwrap();

    thread::sleep(Duration::from_secs(1));

    let results = bg_compactor.last_results();
    assert!(!results.is_empty());

    let user_result = results.iter().find(|r| r.model_name == "User");
    assert!(user_result.is_some());
    assert!(user_result.unwrap().success);
}
