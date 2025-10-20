use forgedb_query_optimization::planner::*;

#[test]
fn test_cost_estimation_table_scan() {
    let mut planner = QueryPlanner::new();

    planner.register_table(TableStats {
        name: "users".to_string(),
        row_count: 10000,
        avg_row_size: 100,
    });

    let estimates = planner.estimate_scan_cost("users", "email", 0.01);

    // Should have at least table scan estimate
    assert!(!estimates.is_empty());
    assert_eq!(estimates[0].rows_scanned, 10000);
}

#[test]
fn test_cost_estimation_with_index() {
    let mut planner = QueryPlanner::new();

    planner.register_table(TableStats {
        name: "users".to_string(),
        row_count: 10000,
        avg_row_size: 100,
    });

    planner.register_index(IndexInfo {
        name: "email_idx".to_string(),
        table: "users".to_string(),
        columns: vec!["email".to_string()],
        is_unique: true,
        cardinality: 10000,
    });

    let estimates = planner.estimate_scan_cost("users", "email", 0.01);

    // Should have both table scan and index scan
    assert!(estimates.len() >= 2);

    // Index scan should be cheaper for low selectivity
    let best = &estimates[0];
    assert!(best.uses_index);
}

#[test]
fn test_choose_index() {
    let mut planner = QueryPlanner::new();

    planner.register_table(TableStats {
        name: "users".to_string(),
        row_count: 10000,
        avg_row_size: 100,
    });

    planner.register_index(IndexInfo {
        name: "email_idx".to_string(),
        table: "users".to_string(),
        columns: vec!["email".to_string()],
        is_unique: true,
        cardinality: 10000,
    });

    let index = planner.choose_index("users", "email", 0.01);
    assert_eq!(index, Some("email_idx".to_string()));
}

#[test]
fn test_join_order_optimization() {
    let mut planner = QueryPlanner::new();

    planner.register_table(TableStats {
        name: "users".to_string(),
        row_count: 10000,
        avg_row_size: 100,
    });

    planner.register_table(TableStats {
        name: "posts".to_string(),
        row_count: 100000,
        avg_row_size: 200,
    });

    planner.register_table(TableStats {
        name: "comments".to_string(),
        row_count: 1000,
        avg_row_size: 50,
    });

    let tables = vec![
        "users".to_string(),
        "posts".to_string(),
        "comments".to_string(),
    ];

    let optimized = planner.optimize_join_order(&tables);

    // Should order by size: comments, users, posts
    assert_eq!(optimized[0], "comments");
    assert_eq!(optimized[1], "users");
    assert_eq!(optimized[2], "posts");
}

#[test]
fn test_create_plan() {
    let mut planner = QueryPlanner::new();

    planner.register_table(TableStats {
        name: "users".to_string(),
        row_count: 1000,
        avg_row_size: 100,
    });

    let plan = planner.create_plan(
        "users",
        vec!["email = 'test@example.com'".to_string()],
        Some(10),
    );

    assert!(!plan.operations.is_empty());
    assert!(plan.total_cost > 0.0);
}
