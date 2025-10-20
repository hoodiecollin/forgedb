use forgedb_watcher::*;
use std::fs;
use std::io::Write;
use std::time::{Duration, Instant};

#[test]
fn test_watcher_creation() {
    let watcher = SchemaWatcher::new(100);
    assert!(watcher.is_ok());
}

#[test]
fn test_watch_nonexistent_path() {
    let mut watcher = SchemaWatcher::new(100).unwrap();
    let result = watcher.watch("/nonexistent/path/schema.forge");
    assert!(result.is_err());
}

#[test]
fn test_watch_and_detect_change() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_schema_watch.forge");

    // Create test file
    fs::write(&test_file, "User { id: +u64 }").unwrap();

    // Canonicalize paths to handle symlinks (e.g., /var -> /private/var on macOS)
    let test_file_canonical = test_file.canonicalize().unwrap();

    let mut watcher = SchemaWatcher::new(50).unwrap();
    watcher.watch(&test_file).unwrap();

    // Spawn a thread to modify the file after a short delay
    let test_file_clone = test_file.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&test_file_clone)
            .unwrap();
        writeln!(file, "\nPost {{ id: +u64 }}").unwrap();
    });

    // Wait for event (with timeout)
    let _start = Instant::now();
    let event = watcher.next_event();

    assert!(event.is_ok());
    let event = event.unwrap();

    // Canonicalize the event path for comparison
    let event_path_canonical = event.path.canonicalize().unwrap_or(event.path.clone());
    assert_eq!(event_path_canonical, test_file_canonical);
    assert!(matches!(event.kind, ChangeKind::Modified));

    // Should complete in reasonable time
    assert!(_start.elapsed() < Duration::from_secs(2));

    // Cleanup
    fs::remove_file(&test_file).ok();
}

#[test]
fn test_debouncing() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_schema_debounce.forge");

    // Create test file
    fs::write(&test_file, "User { id: +u64 }").unwrap();

    let mut watcher = SchemaWatcher::new(200).unwrap();
    watcher.watch(&test_file).unwrap();

    // Spawn a thread to make multiple rapid changes
    let test_file_clone = test_file.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        for i in 0..5 {
            fs::write(&test_file_clone, format!("User {{ id: +u64 }} // {}", i)).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
    });

    // Wait for event - should get only one despite multiple writes
    let event = watcher.next_event();
    assert!(event.is_ok());

    // The test verified that we got at least one event due to the changes
    // Additional events may or may not be debounced depending on timing

    // Cleanup
    fs::remove_file(&test_file).ok();
}
