//! Basic usage example for forgedb-query-optimization
//!
//! This example demonstrates columnar scanning and filtering
//! for efficient query execution.

use forgedb_query_optimization::*;
use forgedb_types::Value;
use uuid::Uuid;

fn main() {
    println!("=== ForgeDB Query Optimization - Basic Usage ===\n");

    // Create some sample data (columnar format)
    println!("--- Creating Sample Data ---");
    
    // Column 1: User IDs
    let user_ids: Vec<Value> = (1..=10)
        .map(|i| Value::I64(i))
        .collect();
    
    // Column 2: Ages
    let ages: Vec<Value> = vec![
        Value::I32(25),
        Value::I32(30),
        Value::I32(22),
        Value::I32(35),
        Value::I32(28),
        Value::I32(40),
        Value::I32(19),
        Value::I32(33),
        Value::I32(27),
        Value::I32(31),
    ];
    
    // Column 3: Scores
    let scores: Vec<Value> = vec![
        Value::F64(85.5),
        Value::F64(92.0),
        Value::F64(78.5),
        Value::F64(95.5),
        Value::F64(88.0),
        Value::F64(91.5),
        Value::F64(76.0),
        Value::F64(89.5),
        Value::F64(84.0),
        Value::F64(93.5),
    ];

    println!("✓ Created {} records with 3 columns\n", user_ids.len());

    // Example 1: Scan all records
    println!("--- Scan All Records ---");
    let scan1 = ColumnScan::new(vec![
        ("id".to_string(), user_ids.clone()),
        ("age".to_string(), ages.clone()),
        ("score".to_string(), scores.clone()),
    ]);
    
    let result1 = scan1.scan(None);
    println!("Total records scanned: {}", result1.row_count);
    println!("Columns scanned: {}", result1.columns_scanned);
    println!();

    // Example 2: Filter by age >= 30
    println!("--- Filter: age >= 30 ---");
    let filter2 = ScanFilter::GreaterThanOrEqual {
        column: "age".to_string(),
        value: Value::I32(30),
    };
    
    let scan2 = ColumnScan::new(vec![
        ("id".to_string(), user_ids.clone()),
        ("age".to_string(), ages.clone()),
        ("score".to_string(), scores.clone()),
    ]);
    
    let result2 = scan2.scan(Some(&filter2));
    println!("Rows matching filter: {}", result2.row_count);
    println!("Total rows scanned: {}", result2.rows_scanned);
    println!("Selectivity: {:.2}%", (result2.row_count as f64 / result2.rows_scanned as f64) * 100.0);
    println!();

    // Example 3: Filter by score > 90.0
    println!("--- Filter: score > 90.0 ---");
    let filter3 = ScanFilter::GreaterThan {
        column: "score".to_string(),
        value: Value::F64(90.0),
    };
    
    let scan3 = ColumnScan::new(vec![
        ("id".to_string(), user_ids.clone()),
        ("age".to_string(), ages.clone()),
        ("score".to_string(), scores.clone()),
    ]);
    
    let result3 = scan3.scan(Some(&filter3));
    println!("High scorers (>90): {}", result3.row_count);
    println!("Total rows scanned: {}", result3.rows_scanned);
    println!();

    // Example 4: Filter by exact value
    println!("--- Filter: age == 25 ---");
    let filter4 = ScanFilter::Equals {
        column: "age".to_string(),
        value: Value::I32(25),
    };
    
    let scan4 = ColumnScan::new(vec![
        ("id".to_string(), user_ids.clone()),
        ("age".to_string(), ages.clone()),
        ("score".to_string(), scores.clone()),
    ]);
    
    let result4 = scan4.scan(Some(&filter4));
    println!("Rows with age 25: {}", result4.row_count);
    println!();

    // Example 5: Range scan (age between 25 and 30)
    println!("--- Range: 25 <= age <= 30 ---");
    let filter5_low = ScanFilter::GreaterThanOrEqual {
        column: "age".to_string(),
        value: Value::I32(25),
    };
    
    // Note: For a true range, you'd need to combine filters
    // This example shows the lower bound filter
    let scan5 = ColumnScan::new(vec![
        ("id".to_string(), user_ids.clone()),
        ("age".to_string(), ages.clone()),
        ("score".to_string(), scores.clone()),
    ]);
    
    let result5 = scan5.scan(Some(&filter5_low));
    println!("Rows with age >= 25: {}", result5.row_count);
    println!();

    // Statistics and cost estimation
    println!("--- Index Statistics ---");
    let mut stats = IndexStatistics::new();
    
    // Record some statistics
    stats.record_scan("age", result2.rows_scanned, result2.row_count);
    stats.record_scan("score", result3.rows_scanned, result3.row_count);
    
    let age_stats = stats.get_column_stats("age");
    println!("Age column:");
    println!("  Total scans: {}", age_stats.scan_count);
    println!("  Total rows scanned: {}", age_stats.total_rows_scanned);
    println!("  Avg selectivity: {:.2}%", age_stats.selectivity() * 100.0);
    println!();

    let score_stats = stats.get_column_stats("score");
    println!("Score column:");
    println!("  Total scans: {}", score_stats.scan_count);
    println!("  Total rows scanned: {}", score_stats.total_rows_scanned);
    println!("  Avg selectivity: {:.2}%", score_stats.selectivity() * 100.0);

    println!("\n✓ Example completed successfully!");
}
