use forgedb_query_optimization::statistics::*;
use std::thread;
use std::time::Duration;

#[test]
fn test_index_stats_creation() {
    let stats = IndexStats::new(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    assert_eq!(stats.name, "email_idx");
    assert_eq!(stats.table, "users");
    assert_eq!(stats.is_unique, true);
    assert_eq!(stats.lookup_count, 0);
}

#[test]
fn test_record_operations() {
    let mut stats = IndexStats::new(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    stats.record_lookup();
    stats.record_lookup();
    stats.record_range_scan();

    assert_eq!(stats.lookup_count, 2);
    assert_eq!(stats.range_scan_count, 1);
    assert_eq!(stats.total_operations(), 3);
}

#[test]
fn test_selectivity_calculation() {
    let mut stats = IndexStats::new(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    stats.update_row_count(1000);
    stats.update_cardinality(1000);

    // Unique index should have selectivity of 1.0
    assert_eq!(stats.avg_selectivity(), 1.0);

    stats.update_cardinality(100);
    // Non-unique index
    assert_eq!(stats.avg_selectivity(), 0.1);
}

#[test]
fn test_staleness() {
    let mut stats = IndexStats::new(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    // Fresh statistics
    assert!(!stats.is_stale(Duration::from_secs(1)));

    // Simulate old statistics
    thread::sleep(Duration::from_millis(100));
    assert!(stats.is_stale(Duration::from_millis(50)));
}

#[test]
fn test_statistics_collector() {
    let mut collector = IndexStatistics::new();

    collector.register_index(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    collector.register_index(
        "username_idx".to_string(),
        "users".to_string(),
        vec!["username".to_string()],
        true,
    );

    assert_eq!(collector.all_stats().len(), 2);
}

#[test]
fn test_record_operations_collector() {
    let mut collector = IndexStatistics::new();

    collector.register_index(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    collector.record_lookup("email_idx");
    collector.record_lookup("email_idx");
    collector.record_range_scan("email_idx");

    let stats = collector.get_stats("email_idx").unwrap();
    assert_eq!(stats.lookup_count, 2);
    assert_eq!(stats.range_scan_count, 1);
}

#[test]
fn test_most_used_indexes() {
    let mut collector = IndexStatistics::new();

    collector.register_index(
        "idx1".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    collector.register_index(
        "idx2".to_string(),
        "users".to_string(),
        vec!["username".to_string()],
        true,
    );

    // Use idx1 more
    collector.record_lookup("idx1");
    collector.record_lookup("idx1");
    collector.record_lookup("idx1");
    collector.record_lookup("idx2");

    let most_used = collector.get_most_used_indexes(2);
    assert_eq!(most_used[0].0, "idx1");
    assert_eq!(most_used[0].1, 3);
    assert_eq!(most_used[1].0, "idx2");
    assert_eq!(most_used[1].1, 1);
}

#[test]
fn test_update_index_stats() {
    let mut collector = IndexStatistics::new();

    collector.register_index(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    collector.update_index_stats("email_idx", 1000, 950, 50000);

    let stats = collector.get_stats("email_idx").unwrap();
    assert_eq!(stats.row_count, 1000);
    assert_eq!(stats.cardinality, 950);
    assert_eq!(stats.size_bytes, 50000);
}

#[test]
fn test_total_index_size() {
    let mut collector = IndexStatistics::new();

    collector.register_index(
        "idx1".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    collector.register_index(
        "idx2".to_string(),
        "users".to_string(),
        vec!["username".to_string()],
        true,
    );

    collector.update_index_stats("idx1", 1000, 1000, 10000);
    collector.update_index_stats("idx2", 1000, 1000, 15000);

    assert_eq!(collector.total_index_size(), 25000);
}

#[test]
fn test_clear_stats() {
    let mut collector = IndexStatistics::new();

    collector.register_index(
        "email_idx".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true,
    );

    collector.record_lookup("email_idx");
    collector.record_range_scan("email_idx");

    collector.clear_stats("email_idx");

    let stats = collector.get_stats("email_idx").unwrap();
    assert_eq!(stats.lookup_count, 0);
    assert_eq!(stats.range_scan_count, 0);
}
