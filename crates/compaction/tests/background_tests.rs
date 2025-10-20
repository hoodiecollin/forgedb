use forgedb_compaction::background::*;
use forgedb_compaction::types::CompactionStatus;
use std::fs;
use std::io::Write;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_background_compactor_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    let config = CompactionConfig {
        dead_space_threshold: 0.3,
        auto_compact: true,
        check_interval_secs: 1, // Short interval for testing
        max_compaction_time_secs: 60,
    };

    let bg_compactor = BackgroundCompactor::new(data_dir, config);

    assert!(!bg_compactor.is_running());
    assert_eq!(bg_compactor.status(), CompactionStatus::Idle);

    bg_compactor.start();
    assert!(bg_compactor.is_running());

    // Wait a bit
    thread::sleep(Duration::from_millis(100));

    bg_compactor.stop();
    thread::sleep(Duration::from_millis(200));

    assert!(!bg_compactor.is_running());
}

#[test]
fn test_manual_trigger() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let model_dir = data_dir.join("User");

    // Create test model with dead space
    fs::create_dir_all(model_dir.join("fixed")).unwrap();

    let mut tombstone_file = fs::File::create(model_dir.join("tombstones.bin")).unwrap();
    tombstone_file.write_all(&[0b01010101]).unwrap(); // 50% dead

    let mut id_file = fs::File::create(model_dir.join("fixed/id.bin")).unwrap();
    id_file.write_all(&[0u8; 128]).unwrap(); // 8 rows * 16 bytes

    let config = CompactionConfig {
        dead_space_threshold: 0.3,
        auto_compact: false,
        check_interval_secs: 3600,
        max_compaction_time_secs: 60,
    };

    let bg_compactor = BackgroundCompactor::new(data_dir, config);

    bg_compactor.trigger_manual().unwrap();

    // Wait for compaction to complete
    thread::sleep(Duration::from_secs(1));

    let results = bg_compactor.last_results();
    assert!(!results.is_empty());

    // Should have compacted User model
    let user_result = results.iter().find(|r| r.model_name == "User");
    assert!(user_result.is_some());
    assert!(user_result.unwrap().success);
}
