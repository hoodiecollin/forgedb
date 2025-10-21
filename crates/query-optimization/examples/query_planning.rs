//! Intermediate example for forgedb-query-optimization
//!
//! This example demonstrates query planning and cost estimation
//! for optimizing query execution.

use forgedb_query_optimization::*;
use forgedb_types::Value;

fn main() {
    println!("=== ForgeDB Query Optimization - Query Planning ===\n");

    // Initialize statistics tracker
    let mut stats = IndexStatistics::new();
    println!("✓ Statistics tracker initialized\n");

    // Simulate historical scan data for different columns
    println!("--- Recording Historical Scan Data ---");
    
    // Email column: low selectivity (many unique values)
    stats.record_scan("email", 1000, 850);
    stats.record_scan("email", 1000, 820);
    stats.record_scan("email", 1000, 900);
    println!("✓ Recorded email column scans (high selectivity)");

    // Status column: high selectivity (few unique values)
    stats.record_scan("status", 1000, 50);
    stats.record_scan("status", 1000, 45);
    stats.record_scan("status", 1000, 55);
    println!("✓ Recorded status column scans (low selectivity)");

    // Age column: medium selectivity
    stats.record_scan("age", 1000, 150);
    stats.record_scan("age", 1000, 180);
    stats.record_scan("age", 1000, 160);
    println!("✓ Recorded age column scans (medium selectivity)");

    // Created_at column: variable selectivity
    stats.record_scan("created_at", 1000, 200);
    stats.record_scan("created_at", 1000, 300);
    println!("✓ Recorded created_at column scans (variable selectivity)\n");

    // Analyze column statistics
    println!("--- Column Statistics ---");
    
    for column in &["email", "status", "age", "created_at"] {
        let col_stats = stats.get_column_stats(column);
        println!("{}:", column);
        println!("  Scan count: {}", col_stats.scan_count);
        println!("  Avg rows scanned: {:.0}", 
            col_stats.total_rows_scanned as f64 / col_stats.scan_count as f64);
        println!("  Avg matches: {:.0}",
            col_stats.total_matching_rows as f64 / col_stats.scan_count as f64);
        println!("  Selectivity: {:.2}%", col_stats.selectivity() * 100.0);
        println!();
    }

    // Create a query planner
    println!("--- Query Planning ---");
    let planner = QueryPlanner::new(stats.clone());
    println!("✓ Query planner initialized\n");

    // Plan 1: Single column scan
    println!("Plan 1: Filter by status");
    let filters1 = vec![
        ScanFilter::Equals {
            column: "status".to_string(),
            value: Value::String("active".to_string()),
        },
    ];
    
    let plan1 = planner.create_plan(filters1, vec!["email".to_string(), "status".to_string()]);
    println!("  Estimated cost: {:.2}", plan1.estimated_cost.total_cost);
    println!("  Estimated rows: {}", plan1.estimated_cost.estimated_rows);
    println!("  Filter order: {:?}", 
        plan1.filters.iter().map(|f| match f {
            ScanFilter::Equals { column, .. } => column.as_str(),
            _ => "other"
        }).collect::<Vec<_>>());
    println!();

    // Plan 2: Multiple column filters
    println!("Plan 2: Filter by status AND age");
    let filters2 = vec![
        ScanFilter::Equals {
            column: "status".to_string(),
            value: Value::String("active".to_string()),
        },
        ScanFilter::GreaterThan {
            column: "age".to_string(),
            value: Value::I32(25),
        },
    ];
    
    let plan2 = planner.create_plan(filters2, vec![
        "email".to_string(),
        "status".to_string(),
        "age".to_string(),
    ]);
    println!("  Estimated cost: {:.2}", plan2.estimated_cost.total_cost);
    println!("  Estimated rows: {}", plan2.estimated_cost.estimated_rows);
    println!("  Projection columns: {:?}", plan2.columns);
    println!();

    // Plan 3: Many filters (planner should optimize order)
    println!("Plan 3: Multiple filters (optimized order)");
    let filters3 = vec![
        ScanFilter::Equals {
            column: "email".to_string(),
            value: Value::String("user@example.com".to_string()),
        },
        ScanFilter::Equals {
            column: "status".to_string(),
            value: Value::String("active".to_string()),
        },
        ScanFilter::GreaterThan {
            column: "age".to_string(),
            value: Value::I32(30),
        },
    ];
    
    let plan3 = planner.create_plan(filters3, vec![
        "email".to_string(),
        "status".to_string(),
        "age".to_string(),
        "created_at".to_string(),
    ]);
    println!("  Estimated cost: {:.2}", plan3.estimated_cost.total_cost);
    println!("  Estimated rows: {}", plan3.estimated_cost.estimated_rows);
    println!("  Optimized filter order:");
    for (i, filter) in plan3.filters.iter().enumerate() {
        let column = match filter {
            ScanFilter::Equals { column, .. } => column,
            ScanFilter::GreaterThan { column, .. } => column,
            ScanFilter::GreaterThanOrEqual { column, .. } => column,
            _ => "unknown",
        };
        println!("    {}. {} (selectivity: {:.2}%)", 
            i + 1, 
            column,
            stats.get_column_stats(column).selectivity() * 100.0);
    }
    println!();

    // Compare plans
    println!("--- Plan Comparison ---");
    println!("Plan 1 (status only):");
    println!("  Cost: {:.2}", plan1.estimated_cost.total_cost);
    println!("Plan 2 (status + age):");
    println!("  Cost: {:.2}", plan2.estimated_cost.total_cost);
    println!("Plan 3 (email + status + age):");
    println!("  Cost: {:.2}", plan3.estimated_cost.total_cost);
    println!();
    println!("Note: Lower cost indicates more efficient query execution");

    // Demonstrate cost estimation
    println!("\n--- Cost Estimation Details ---");
    let cost_example = CostEstimate {
        scan_cost: 100.0,
        filter_cost: 50.0,
        total_cost: 150.0,
        estimated_rows: 25,
    };
    println!("Example cost breakdown:");
    println!("  Scan cost: {:.2}", cost_example.scan_cost);
    println!("  Filter cost: {:.2}", cost_example.filter_cost);
    println!("  Total cost: {:.2}", cost_example.total_cost);
    println!("  Expected result rows: {}", cost_example.estimated_rows);

    println!("\n✓ Example completed successfully!");
    println!("\nKey Takeaways:");
    println!("  • Filters with higher selectivity should be applied first");
    println!("  • Query planning reduces overall execution cost");
    println!("  • Statistics help optimize filter ordering");
    println!("  • Cost estimation guides query execution strategy");
}
