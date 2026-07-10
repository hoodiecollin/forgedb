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

// ---- predicate IR (#48) ----------------------------------------------------

#[test]
fn test_predicate_parse_qualified_columns() {
    let p = Predicate::parse("post.author_id = author.id").unwrap();
    assert_eq!(p.op, PredicateOp::Eq);
    assert_eq!(
        p.left,
        Operand::Column {
            table: Some("post".to_string()),
            column: "author_id".to_string(),
        }
    );
    assert_eq!(
        p.right,
        Operand::Column {
            table: Some("author".to_string()),
            column: "id".to_string(),
        }
    );
    // A join condition references both tables.
    let tables: Vec<_> = p.tables_referenced().into_iter().collect();
    assert_eq!(tables, vec!["author".to_string(), "post".to_string()]);
}

#[test]
fn test_predicate_parse_column_vs_literal() {
    let p = Predicate::parse("author.age >= 18").unwrap();
    assert_eq!(p.op, PredicateOp::Gte);
    assert_eq!(
        p.left,
        Operand::Column {
            table: Some("author".to_string()),
            column: "age".to_string(),
        }
    );
    assert_eq!(p.right, Operand::Literal("18".to_string()));
    // The literal contributes no table; only `author` is referenced.
    let tables: Vec<_> = p.tables_referenced().into_iter().collect();
    assert_eq!(tables, vec!["author".to_string()]);
}

#[test]
fn test_predicate_parse_multichar_operator_not_split() {
    // `>=` must not be parsed as `>` with a right operand of `= 18`.
    let p = Predicate::parse("x.n <= 5").unwrap();
    assert_eq!(p.op, PredicateOp::Lte);
    assert_eq!(p.right, Operand::Literal("5".to_string()));

    let ne = Predicate::parse("x.n != 5").unwrap();
    assert_eq!(ne.op, PredicateOp::Ne);
}

#[test]
fn test_predicate_parse_no_operator_is_none() {
    assert!(Predicate::parse("just_a_bare_token").is_none());
}

#[test]
fn test_predicate_unqualified_column_references_no_table() {
    let p = Predicate::parse("age > 18").unwrap();
    assert!(p.tables_referenced().is_empty());
}

// ---- join predicate pushdown (#48) -----------------------------------------

/// Build `author ⋈ post` with `author` on the left and `post` on the right.
fn author_post_join() -> QueryPlanOp {
    QueryPlanOp::Join {
        left: Box::new(QueryPlanOp::TableScan {
            table: "author".to_string(),
            row_count: 100,
        }),
        right: Box::new(QueryPlanOp::TableScan {
            table: "post".to_string(),
            row_count: 1000,
        }),
        join_type: JoinType::Inner,
        estimated_rows: 1000,
    }
}

#[test]
fn test_pushdown_splits_single_side_predicates_into_scans() {
    let planner = QueryPlanner::new();
    let preds = vec![
        "author.age > 18".to_string(),      // left only
        "post.published = true".to_string(), // right only
    ];

    // No cross-side predicate remains, so the root is the Join itself.
    let out = planner.pushdown_predicates(author_post_join(), preds);
    let (left, right) = match out {
        QueryPlanOp::Join { left, right, .. } => (*left, *right),
        other => panic!("expected a Join at the root, got {other:?}"),
    };

    // The left-only predicate was pushed into the left scan.
    match left {
        QueryPlanOp::Filter { input, predicate, .. } => {
            assert_eq!(predicate, "author.age > 18");
            assert!(matches!(*input, QueryPlanOp::TableScan { ref table, .. } if table == "author"));
        }
        other => panic!("expected left Filter(TableScan), got {other:?}"),
    }

    // The right-only predicate was pushed into the right scan.
    match right {
        QueryPlanOp::Filter { input, predicate, .. } => {
            assert_eq!(predicate, "post.published = true");
            assert!(matches!(*input, QueryPlanOp::TableScan { ref table, .. } if table == "post"));
        }
        other => panic!("expected right Filter(TableScan), got {other:?}"),
    }
}

#[test]
fn test_pushdown_keeps_cross_join_predicate_as_wrapping_filter() {
    let planner = QueryPlanner::new();
    let preds = vec![
        "author.age > 18".to_string(),               // left only  → pushed
        "post.published = true".to_string(),          // right only → pushed
        "post.author_id = author.id".to_string(),     // cross-side → wraps
    ];

    let out = planner.pushdown_predicates(author_post_join(), preds);

    // The cross-side join condition stays as a Filter wrapping the join, and the
    // wrapped join still carries the pushed-down single-side predicates.
    let (predicate, inner) = match out {
        QueryPlanOp::Filter { predicate, input, .. } => (predicate, *input),
        other => panic!("expected a wrapping Filter, got {other:?}"),
    };
    assert_eq!(predicate, "post.author_id = author.id");

    let (left, right) = match inner {
        QueryPlanOp::Join { left, right, .. } => (*left, *right),
        other => panic!("expected a Join under the Filter, got {other:?}"),
    };
    assert!(matches!(left, QueryPlanOp::Filter { ref predicate, .. } if predicate == "author.age > 18"));
    assert!(matches!(right, QueryPlanOp::Filter { ref predicate, .. } if predicate == "post.published = true"));
}

#[test]
fn test_pushdown_unqualified_predicate_stays_at_join() {
    let planner = QueryPlanner::new();
    // A bare, unqualified column can't be attributed to a side → kept at output.
    let preds = vec!["created_at > 0".to_string()];

    let out = planner.pushdown_predicates(author_post_join(), preds);
    match out {
        QueryPlanOp::Filter { predicate, input, .. } => {
            assert_eq!(predicate, "created_at > 0");
            assert!(matches!(*input, QueryPlanOp::Join { .. }));
        }
        other => panic!("expected a wrapping Filter, got {other:?}"),
    }
}

#[test]
fn test_pushdown_preserves_all_predicates() {
    // Every input predicate must appear exactly once across the rewritten plan —
    // pushdown never drops or duplicates a predicate.
    let planner = QueryPlanner::new();
    let preds = vec![
        "author.age > 18".to_string(),
        "post.published = true".to_string(),
        "post.author_id = author.id".to_string(),
        "created_at > 0".to_string(),
    ];

    let out = planner.pushdown_predicates(author_post_join(), preds.clone());

    let mut seen = Vec::new();
    collect_predicates(&out, &mut seen);
    seen.sort();
    let mut expected = preds;
    expected.sort();
    assert_eq!(seen, expected);
}

/// Walk a plan and gather every predicate string (splitting `AND`-joined ones).
fn collect_predicates(op: &QueryPlanOp, out: &mut Vec<String>) {
    match op {
        QueryPlanOp::Filter { input, predicate, .. } => {
            for part in predicate.split(" AND ") {
                out.push(part.to_string());
            }
            collect_predicates(input, out);
        }
        QueryPlanOp::Join { left, right, .. } => {
            collect_predicates(left, out);
            collect_predicates(right, out);
        }
        QueryPlanOp::Limit { input, .. } => collect_predicates(input, out),
        _ => {}
    }
}
