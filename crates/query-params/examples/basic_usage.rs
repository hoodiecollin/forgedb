//! Basic usage example for forgedb-query-params
//!
//! This example demonstrates parsing and validating query parameters
//! for filtering, sorting, and pagination.

use forgedb_query_params::*;

fn main() {
    println!("=== ForgeDB Query Params - Basic Usage ===\n");

    // Example 1: Pagination parameters
    println!("--- Pagination ---");
    
    // Parse pagination from query string
    let query_str = "page=2&limit=25";
    match serde_urlencoded::from_str::<Pagination>(query_str) {
        Ok(pagination) => {
            println!("Query: {}", query_str);
            println!("  Page: {}", pagination.page);
            println!("  Limit: {}", pagination.limit);
            println!("  Offset: {}", pagination.offset());
        }
        Err(e) => println!("Error parsing pagination: {}", e),
    }
    println!();

    // Default pagination
    let default_pagination = Pagination::default();
    println!("Default pagination:");
    println!("  Page: {}", default_pagination.page);
    println!("  Limit: {} (default)", default_pagination.limit);
    println!();

    // Example 2: Sorting parameters
    println!("--- Sorting ---");
    
    let sort_query = "sort=name&order=desc";
    match serde_urlencoded::from_str::<Sort>(sort_query) {
        Ok(sort) => {
            println!("Query: {}", sort_query);
            println!("  Field: {}", sort.field);
            println!("  Order: {:?}", sort.order);
        }
        Err(e) => println!("Error parsing sort: {}", e),
    }
    println!();

    // Ascending sort
    let asc_query = "sort=created_at&order=asc";
    match serde_urlencoded::from_str::<Sort>(asc_query) {
        Ok(sort) => {
            println!("Query: {}", asc_query);
            println!("  Field: {}", sort.field);
            println!("  Order: {:?}", sort.order);
        }
        Err(e) => println!("Error parsing sort: {}", e),
    }
    println!();

    // Example 3: Filter parameters
    println!("--- Filtering ---");
    
    // Simple equality filter
    let filter_query = "filter=status:active";
    match serde_urlencoded::from_str::<Filter>(filter_query) {
        Ok(filter) => {
            println!("Query: {}", filter_query);
            println!("  Field: {}", filter.field);
            println!("  Value: {:?}", filter.value);
        }
        Err(e) => println!("Error parsing filter: {}", e),
    }
    println!();

    // Example 4: Complete query parameters
    println!("--- Complete Query Parameters ---");
    
    let complete_query = "page=1&limit=10&sort=name&order=asc&filter=status:active";
    match serde_urlencoded::from_str::<QueryParams>(complete_query) {
        Ok(params) => {
            println!("Query: {}", complete_query);
            println!("\nPagination:");
            println!("  Page: {}", params.pagination.page);
            println!("  Limit: {}", params.pagination.limit);
            
            if let Some(sort) = params.sort {
                println!("\nSorting:");
                println!("  Field: {}", sort.field);
                println!("  Order: {:?}", sort.order);
            }
            
            if let Some(filter) = params.filter {
                println!("\nFilter:");
                println!("  Field: {}", filter.field);
                println!("  Value: {:?}", filter.value);
            }
        }
        Err(e) => println!("Error parsing query params: {}", e),
    }
    println!();

    // Example 5: Pagination limits
    println!("--- Pagination Limits ---");
    println!("Default limit: {}", DEFAULT_LIMIT);
    println!("Maximum limit: {}", MAX_LIMIT);
    
    // Try to use a limit that exceeds MAX_LIMIT
    let excessive_query = "page=1&limit=500";
    match serde_urlencoded::from_str::<Pagination>(excessive_query) {
        Ok(pagination) => {
            println!("\nQuery: {}", excessive_query);
            println!("  Requested limit: would be capped at {}", MAX_LIMIT);
            println!("  Actual limit: {}", pagination.limit);
        }
        Err(e) => println!("Error: {}", e),
    }

    println!("\n✓ Example completed successfully!");
}
