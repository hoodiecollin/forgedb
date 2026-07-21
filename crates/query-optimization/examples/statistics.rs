//! Intermediate example for forgedb-query-optimization
//!
//! This example demonstrates tracking index statistics for query optimization.

use forgedb_query_optimization::*;

fn main() {
    println!("=== ForgeDB Query Optimization - Index Statistics ===\n");

    // Initialize statistics tracker
    let mut stats = IndexStatistics::new();
    println!("✓ Statistics tracker initialized\n");

    // Register some indexes
    println!("--- Registering Indexes ---");
    
    stats.register_index(
        "idx_users_email".to_string(),
        "users".to_string(),
        vec!["email".to_string()],
        true, // unique
    );
    println!("✓ Registered idx_users_email (unique)");

    stats.register_index(
        "idx_users_status".to_string(),
        "users".to_string(),
        vec!["status".to_string()],
        false,
    );
    println!("✓ Registered idx_users_status");

    stats.register_index(
        "idx_posts_author".to_string(),
        "posts".to_string(),
        vec!["author_id".to_string()],
        false,
    );
    println!("✓ Registered idx_posts_author\n");

    // Simulate index usage
    println!("--- Simulating Index Usage ---");
    
    // Email index: frequently used for lookups
    for _ in 0..100 {
        stats.record_lookup("idx_users_email");
    }
    println!("✓ Recorded 100 lookups on idx_users_email");

    // Status index: used for range scans
    for _ in 0..50 {
        stats.record_range_scan("idx_users_status");
    }
    println!("✓ Recorded 50 range scans on idx_users_status");

    // Posts author index: mixed usage
    for _ in 0..75 {
        stats.record_lookup("idx_posts_author");
    }
    for _ in 0..25 {
        stats.record_range_scan("idx_posts_author");
    }
    println!("✓ Recorded 75 lookups + 25 range scans on idx_posts_author\n");

    // Update statistics
    println!("--- Updating Index Statistics ---");
    stats.update_index_stats("idx_users_email", 10000, 10000, 1024 * 500);
    stats.update_index_stats("idx_users_status", 10000, 3, 1024 * 10);
    stats.update_index_stats("idx_posts_author", 50000, 1000, 1024 * 1000);
    println!("✓ Updated statistics for all indexes\n");

    // View statistics
    println!("--- Index Statistics ---");
    for index_stats in stats.all_stats() {
        println!("\n{}:", index_stats.name);
        println!("  Table: {}", index_stats.table);
        println!("  Columns: {:?}", index_stats.columns);
        println!("  Unique: {}", index_stats.is_unique);
        println!("  Row count: {}", index_stats.row_count);
        println!("  Cardinality: {}", index_stats.cardinality);
        println!("  Size: {} KB", index_stats.size_bytes / 1024);
        println!("  Lookups: {}", index_stats.lookup_count);
        println!("  Range scans: {}", index_stats.range_scan_count);
        println!("  Total operations: {}", index_stats.total_operations());
        println!("  Avg selectivity: {:.4}", index_stats.avg_selectivity());
    }

    // Get most used indexes
    println!("\n--- Most Used Indexes ---");
    let top_indexes = stats.get_most_used_indexes(5);
    for (i, (name, usage)) in top_indexes.iter().enumerate() {
        println!("{}. {} - {} operations", i + 1, name, usage);
    }

    // Total size
    println!("\n--- Overall Statistics ---");
    println!("Total index size: {} KB", stats.total_index_size() / 1024);
    println!("Number of indexes: {}", stats.all_stats().len());

    // Check for stale statistics
    let stale = stats.get_stale_indexes();
    println!("Stale indexes: {}", if stale.is_empty() {
        "None".to_string()
    } else {
        stale.join(", ")
    });

    println!("\n✓ Example completed successfully!");
}
