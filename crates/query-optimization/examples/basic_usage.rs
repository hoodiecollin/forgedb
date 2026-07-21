//! Basic usage example for forgedb-query-optimization
//!
//! This example demonstrates efficient columnar scanning with SIMD optimization
//! for filtering numeric data.

use forgedb_query_optimization::*;

fn main() {
    println!("=== ForgeDB Query Optimization - Basic Usage ===\n");

    // Create some sample data (u64 column)
    println!("--- Creating Sample Data ---");
    let ages: Vec<u64> = vec![
        25, 30, 22, 35, 28, 40, 19, 33, 27, 31,
        45, 29, 38, 24, 32, 36, 26, 41, 23, 34,
    ];
    
    println!("✓ Created column with {} rows\n", ages.len());

    // Example 1: Filter - Equal to 30
    println!("--- Filter: age == 30 ---");
    let filter1 = ScanFilter::Eq(30);
    let result1 = ColumnScan::scan_u64(&ages, filter1, None);
    println!("Matching rows: {}", result1.matching_rows.len());
    println!("Rows scanned: {}", result1.rows_scanned);
    println!("Matches at indices: {:?}", result1.matching_rows);
    println!();

    // Example 2: Filter - Greater than 35
    println!("--- Filter: age > 35 ---");
    let filter2 = ScanFilter::Gt(35);
    let result2 = ColumnScan::scan_u64(&ages, filter2, None);
    println!("Matching rows: {}", result2.matching_rows.len());
    println!("Rows scanned: {}", result2.rows_scanned);
    for idx in &result2.matching_rows {
        println!("  Index {}: age = {}", idx, ages[*idx]);
    }
    println!();

    // Example 3: Filter - Greater than or equal to 30
    println!("--- Filter: age >= 30 ---");
    let filter3 = ScanFilter::Gte(30);
    let result3 = ColumnScan::scan_u64(&ages, filter3, None);
    println!("Matching rows: {}", result3.matching_rows.len());
    println!("Rows scanned: {}", result3.rows_scanned);
    println!("Selectivity: {:.2}%", 
        (result3.matching_rows.len() as f64 / result3.rows_scanned as f64) * 100.0);
    println!();

    // Example 4: Filter with LIMIT
    println!("--- Filter: age >= 25 (LIMIT 5) ---");
    let filter4 = ScanFilter::Gte(25);
    let result4 = ColumnScan::scan_u64(&ages, filter4, Some(5));
    println!("Matching rows: {} (limited to 5)", result4.matching_rows.len());
    println!("Rows scanned: {} (early termination: {})", 
        result4.rows_scanned, result4.early_termination);
    println!("First 5 matches: {:?}", result4.matching_rows);
    println!();

    // Example 5: Range filter
    println!("--- Filter: 25 <= age <= 35 ---");
    let filter5 = ScanFilter::Range(25, 35);
    let result5 = ColumnScan::scan_u64(&ages, filter5, None);
    println!("Matching rows in range: {}", result5.matching_rows.len());
    println!("Values: {:?}", 
        result5.matching_rows.iter().map(|&i| ages[i]).collect::<Vec<_>>());
    println!();

    // Example 6: Not equal filter
    println!("--- Filter: age != 30 ---");
    let filter6 = ScanFilter::Ne(30);
    let result6 = ColumnScan::scan_u64(&ages, filter6, None);
    println!("Matching rows: {}", result6.matching_rows.len());
    println!();

    // Example 7: Less than filter
    println!("--- Filter: age < 25 ---");
    let filter7 = ScanFilter::Lt(25);
    let result7 = ColumnScan::scan_u64(&ages, filter7, None);
    println!("Matching rows: {}", result7.matching_rows.len());
    for idx in &result7.matching_rows {
        println!("  Index {}: age = {}", idx, ages[*idx]);
    }

    println!("\n✓ Example completed successfully!");
    println!("\nNote: This crate uses SIMD (AVX2) optimization on x86_64 platforms");
    println!("      for faster columnar scanning of large datasets.");
}
