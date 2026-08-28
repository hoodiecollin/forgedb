use forgedb_query_params::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
    email: String,
    status: String,
    created_at: u64,
}

struct Database {
    users: Vec<User>,
}

impl Database {
    fn new() -> Self {
        Self {
            users: vec![
                User {
                    id: 1,
                    name: "Alice".to_string(),
                    email: "alice@example.com".to_string(),
                    status: "active".to_string(),
                    created_at: 1000000,
                },
                User {
                    id: 2,
                    name: "Bob".to_string(),
                    email: "bob@example.com".to_string(),
                    status: "inactive".to_string(),
                    created_at: 2000000,
                },
                User {
                    id: 3,
                    name: "Charlie".to_string(),
                    email: "charlie@example.com".to_string(),
                    status: "active".to_string(),
                    created_at: 3000000,
                },
                User {
                    id: 4,
                    name: "Diana".to_string(),
                    email: "diana@example.com".to_string(),
                    status: "active".to_string(),
                    created_at: 4000000,
                },
                User {
                    id: 5,
                    name: "Eve".to_string(),
                    email: "eve@example.com".to_string(),
                    status: "inactive".to_string(),
                    created_at: 5000000,
                },
            ],
        }
    }

    fn query(&self, params: &QueryParams) -> (Vec<User>, usize) {
        let mut results = self.users.clone();

        for filter in &params.filters {
            results.retain(|user| match filter.field.as_str() {
                "status" => match &filter.value {
                    FilterValue::String(val) => &user.status == val,
                    _ => true,
                },
                "name" => match &filter.value {
                    FilterValue::String(val) => user.name.contains(val),
                    _ => true,
                },
                _ => true,
            });
        }

        let total = results.len();

        if let Some(sort) = &params.sort {
            match sort.field.as_str() {
                "name" => {
                    results.sort_by(|a, b| match sort.order {
                        SortOrder::Asc => a.name.cmp(&b.name),
                        SortOrder::Desc => b.name.cmp(&a.name),
                    });
                }
                "created_at" => {
                    results.sort_by(|a, b| match sort.order {
                        SortOrder::Asc => a.created_at.cmp(&b.created_at),
                        SortOrder::Desc => b.created_at.cmp(&a.created_at),
                    });
                }
                _ => {}
            }
        }

        let paged = params.pagination.apply(&results).to_vec();

        (paged, total)
    }
}

fn main() {
    println!("=== ForgeDB Query Params - Web Integration ===\n");

    let db = Database::new();
    println!("✓ Database initialized with {} users\n", db.users.len());

    println!("--- Query 1: Default (all users, first page) ---");
    let query1 = "limit=10";
    let params1 = QueryParams::from_query_string(query1).unwrap();
    let (results1, total1) = db.query(&params1);

    println!("Query: {}", query1);
    println!("Results: {} of {} total", results1.len(), total1);
    for user in results1 {
        println!("  - {} ({}) - {}", user.name, user.email, user.status);
    }
    println!();

    println!("--- Query 2: Filter by status=active ---");
    let query2 = "status=active&limit=10";
    let params2 = QueryParams::from_query_string(query2).unwrap();
    let (results2, total2) = db.query(&params2);

    println!("Query: {}", query2);
    println!("Results: {} of {} matching", results2.len(), total2);
    for user in results2 {
        println!("  - {} ({}) - {}", user.name, user.email, user.status);
    }
    println!();

    println!("--- Query 3: Sort by name (ascending) ---");
    let query3 = "sort=name&order=asc&limit=10";
    let params3 = QueryParams::from_query_string(query3).unwrap();
    let (results3, total3) = db.query(&params3);

    println!("Query: {}", query3);
    println!("Results: {} of {} total", results3.len(), total3);
    for user in results3 {
        println!("  - {} ({}) - {}", user.name, user.email, user.status);
    }
    println!();

    println!("--- Query 4: Sort by created_at (descending) ---");
    let query4 = "sort=created_at&order=desc&limit=3";
    let params4 = QueryParams::from_query_string(query4).unwrap();
    let (results4, total4) = db.query(&params4);

    println!("Query: {}", query4);
    println!("Results: {} of {} total", results4.len(), total4);
    for user in results4 {
        println!("  - {} (created: {}) - {}", user.name, user.created_at, user.status);
    }
    println!();

    println!("--- Query 5: Filter + Sort + Pagination ---");
    let query5 = "status=active&sort=name&order=asc&limit=2";
    let params5 = QueryParams::from_query_string(query5).unwrap();
    let (results5, total5) = db.query(&params5);

    println!("Query: {}", query5);
    println!(
        "Results: {} of {} matching (offset {}, limit {})",
        results5.len(),
        total5,
        params5.pagination.offset,
        params5.pagination.limit
    );
    for user in results5 {
        println!("  - {} ({}) - {}", user.name, user.email, user.status);
    }

    println!("\n✓ Example completed successfully!");
}
