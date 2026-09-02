use forgedb_migrations::*;
use tempfile::TempDir;

#[test]
fn test_migration_tracker() {
    let temp_dir = TempDir::new().unwrap();
    let mut tracker = MigrationTracker::new(temp_dir.path()).unwrap();

    assert!(!tracker.is_applied("20241014000000"));

    tracker
        .mark_applied("20241014000000".to_string(), "abc123".to_string())
        .unwrap();
    assert!(tracker.is_applied("20241014000000"));

    let tracker2 = MigrationTracker::new(temp_dir.path()).unwrap();
    assert!(tracker2.is_applied("20241014000000"));
}

#[test]
fn test_rollback() {
    let temp_dir = TempDir::new().unwrap();
    let mut tracker = MigrationTracker::new(temp_dir.path()).unwrap();

    tracker
        .mark_applied("20241014000000".to_string(), "abc123".to_string())
        .unwrap();
    tracker
        .mark_applied("20241014000001".to_string(), "def456".to_string())
        .unwrap();

    assert_eq!(tracker.applied_migrations().len(), 2);

    let rolled_back = tracker.mark_rolled_back().unwrap();
    assert!(rolled_back.is_some());
    assert_eq!(rolled_back.unwrap().migration_id, "20241014000001");
    assert_eq!(tracker.applied_migrations().len(), 1);
}

#[test]
fn test_pending_migrations() {
    let temp_dir = TempDir::new().unwrap();
    let mut tracker = MigrationTracker::new(temp_dir.path()).unwrap();

    tracker
        .mark_applied("20241014000000".to_string(), "abc123".to_string())
        .unwrap();

    let all = vec![
        "20241014000000".to_string(),
        "20241014000001".to_string(),
        "20241014000002".to_string(),
    ];

    let pending = tracker.pending_migrations(&all);
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0], "20241014000001");
    assert_eq!(pending[1], "20241014000002");
}
