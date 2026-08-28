use forgedb_query_params::*;

fn main() {
    println!("=== ForgeDB Query Params - Basic Usage ===\n");

    println!("--- Pagination ---");

    let pagination = Pagination::from_params(Some(25), Some(50));
    println!("From limit=25, offset=50:");
    println!("  Limit: {}", pagination.limit);
    println!("  Offset: {}", pagination.offset);
    println!("  End index: {}", pagination.end());
    println!("  Has next (of 200): {}", pagination.has_next(200));
    println!();

    let default_pagination = Pagination::default();
    println!("Default pagination:");
    println!("  Limit: {} (default)", default_pagination.limit);
    println!("  Offset: {}", default_pagination.offset);
    println!();

    println!("--- Sorting ---");

    if let Some(sort) = Sort::from_params(Some("name".into()), Some("desc".into())) {
        println!("From sort=name, order=desc:");
        println!("  Field: {}", sort.field);
        println!("  Order: {:?}", sort.order);
        println!("  Descending: {}", sort.is_descending());
    }
    println!();

    if let Some(sort) = Sort::from_params(Some("created_at".into()), None) {
        println!("From sort=created_at (default order):");
        println!("  Field: {}", sort.field);
        println!("  Order: {:?}", sort.order);
    }
    println!();

    println!("--- Filtering ---");

    let filtered = QueryParams::from_query_string("status=active").unwrap();
    for filter in &filtered.filters {
        println!("Filter on `status=active`:");
        println!("  Field: {}", filter.field);
        println!("  Value: {:?}", filter.value);
    }
    println!();

    println!("--- Complete Query Parameters ---");

    let complete_query = "limit=10&offset=0&sort=name&order=asc&status=active";
    match QueryParams::from_query_string(complete_query) {
        Ok(params) => {
            println!("Query: {}", complete_query);
            println!("\nPagination:");
            println!("  Limit: {}", params.pagination.limit);
            println!("  Offset: {}", params.pagination.offset);

            if let Some(sort) = &params.sort {
                println!("\nSorting:");
                println!("  Field: {}", sort.field);
                println!("  Order: {:?}", sort.order);
            }

            if params.has_filters() {
                println!("\nFilters:");
                for filter in &params.filters {
                    println!("  {} = {:?}", filter.field, filter.value);
                }
            }
        }
        Err(e) => println!("Error parsing query params: {}", e),
    }
    println!();

    println!("--- Pagination Limits ---");
    println!("Default limit: {}", DEFAULT_LIMIT);
    println!("Maximum limit: {}", MAX_LIMIT);

    let clamped = Pagination::from_params(Some(5000), Some(0));
    println!("\nRequested limit 5000:");
    println!("  Actual limit: {} (capped at {})", clamped.limit, MAX_LIMIT);

    println!("\n✓ Example completed successfully!");
}
