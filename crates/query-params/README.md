# ForgeDB Query Params

Query parameter parsing for REST APIs with filtering, sorting, and pagination.

## Overview

The `forgedb-query-params` crate provides a robust solution for parsing URL query strings into structured filter, sort, and pagination parameters. It's designed for building REST APIs that need to handle common query operations in a type-safe and ergonomic way.

## Features

- **Filter parsing** - Parse field-value pairs with automatic type detection
- **Sort parsing** - Parse sort field and order (`asc`/`desc`) from query parameters
- **Pagination** - Handle `limit` and `offset` parameters with sensible defaults
- **Type safety** - Strong typing for filter values (String, Number, Bool)
- **Query string support** - Direct parsing from URL-encoded query strings
- **Flexible API** - Parse from query strings or HashMap

## Usage

### Basic Example

```rust
use forgedb_query_params::QueryParams;

// Parse query parameters from a URL query string
let query = "name=John&age=25&sort=created_at&order=desc&limit=20&offset=10";
let params = QueryParams::from_query_string(query).unwrap();

// Access filters
assert_eq!(params.filters.len(), 2);
if let Some(name_filter) = params.get_filter("name") {
    println!("Filter by name: {:?}", name_filter.value);
}

// Access sort
if let Some(sort) = &params.sort {
    println!("Sort by: {} ({})", sort.field, 
        if sort.is_ascending() { "asc" } else { "desc" });
}

// Access pagination
println!("Limit: {}, Offset: {}", 
    params.pagination.limit, params.pagination.offset);
```

### Parsing Filters

Filters are automatically extracted from query parameters and typed based on their values:

```rust
use forgedb_query_params::{QueryParams, FilterValue};

let params = QueryParams::from_query_string(
    "name=Alice&age=30&active=true"
).unwrap();

// String filter
let name = params.get_filter("name").unwrap();
assert!(matches!(name.value, FilterValue::String(_)));
assert!(name.matches_string("Alice"));

// Number filter
let age = params.get_filter("age").unwrap();
assert!(matches!(age.value, FilterValue::Number(_)));
assert!(age.matches_number(30.0));

// Boolean filter
let active = params.get_filter("active").unwrap();
assert!(matches!(active.value, FilterValue::Bool(_)));
assert!(active.matches_bool(true));
```

### Sort Parameter Handling

Sort parameters use the `sort` field for the column name and `order` for the direction:

```rust
use forgedb_query_params::{QueryParams, SortOrder};

// Sort ascending (default)
let params = QueryParams::from_query_string("sort=created_at").unwrap();
assert!(params.has_sort());
let sort = params.sort.unwrap();
assert_eq!(sort.field, "created_at");
assert_eq!(sort.order, SortOrder::Asc);

// Sort descending
let params = QueryParams::from_query_string(
    "sort=price&order=desc"
).unwrap();
let sort = params.sort.unwrap();
assert_eq!(sort.field, "price");
assert!(sort.is_descending());
```

### Pagination Setup

Pagination uses `limit` and `offset` parameters with sensible defaults and bounds:

```rust
use forgedb_query_params::{Pagination, DEFAULT_LIMIT, MAX_LIMIT};

// Default pagination (limit=50, offset=0)
let p = Pagination::default();
assert_eq!(p.limit, DEFAULT_LIMIT); // 50
assert_eq!(p.offset, 0);

// Custom pagination
let p = Pagination::new(100, 50);
assert_eq!(p.limit, 100);
assert_eq!(p.offset, 50);

// Limits are clamped to MAX_LIMIT (1000)
let p = Pagination::new(5000, 0);
assert_eq!(p.limit, MAX_LIMIT); // 1000

// Apply pagination to a slice
let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let p = Pagination::new(3, 2);
let page = p.apply(&items);
assert_eq!(page, &[3, 4, 5]);

// Navigate pages
let next = p.next_page();
assert_eq!(next.offset, 5);

if let Some(prev) = p.prev_page() {
    assert_eq!(prev.offset, 0);
}
```

### Combining All Three

Use `QueryParams` to parse and access filters, sorting, and pagination together:

```rust
use forgedb_query_params::QueryParams;

let query = "status=active&category=electronics&sort=price&order=asc&limit=25&offset=50";
let params = QueryParams::from_query_string(query).unwrap();

// Check what's present
assert!(params.has_filters());
assert!(params.has_sort());

// Use filters
for filter in &params.filters {
    println!("Filter: {} = {:?}", filter.field, filter.value);
}

// Use sort
if let Some(sort) = &params.sort {
    println!("Sort: {} {}", sort.field, 
        if sort.is_ascending() { "ASC" } else { "DESC" });
}

// Use pagination
let total_count = 1000;
if params.pagination.has_next(total_count) {
    println!("More results available");
}
```

## Filter Operators

### Equality Operators

The current implementation supports equality filtering with automatic type detection:

```rust
use forgedb_query_params::{Filter, FilterValue};

// String equality
let filter = Filter::new("status", FilterValue::String("active".to_string()));
assert!(filter.matches_string("active"));

// Number equality
let filter = Filter::new("price", FilterValue::Number(99.99));
assert!(filter.matches_number(99.99));

// Boolean equality
let filter = Filter::new("featured", FilterValue::Bool(true));
assert!(filter.matches_bool(true));
```

### Type Detection

Filter values are automatically typed when parsed from query strings:

- **Numbers** - Valid numeric strings (e.g., `"25"`, `"99.99"`) → `FilterValue::Number`
- **Booleans** - `"true"` or `"false"` → `FilterValue::Bool`
- **Strings** - Everything else → `FilterValue::String`

## API Reference

### QueryParams

Main type for parsed query parameters.

```rust
pub struct QueryParams {
    pub filters: Vec<Filter>,
    pub sort: Option<Sort>,
    pub pagination: Pagination,
}
```

**Methods:**
- `from_query_string(query: &str)` - Parse from URL-encoded query string
- `from_map(params: HashMap<String, String>)` - Parse from HashMap
- `new(filters, sort, pagination)` - Create with all components
- `has_filters()` - Check if any filters are present
- `has_sort()` - Check if sort is present
- `get_filter(field: &str)` - Get filter by field name

### Filter

Represents a field-value filter pair.

```rust
pub struct Filter {
    pub field: String,
    pub value: FilterValue,
}
```

**Methods:**
- `new(field, value)` - Create a new filter
- `from_params(params: HashMap<String, String>)` - Parse multiple filters
- `matches_string(value: &str)` - Check if filter matches a string
- `matches_number(value: f64)` - Check if filter matches a number
- `matches_bool(value: bool)` - Check if filter matches a boolean

### FilterValue

Enum representing possible filter value types.

```rust
pub enum FilterValue {
    String(String),
    Number(f64),
    Bool(bool),
}
```

### Sort

Represents sorting parameters.

```rust
pub struct Sort {
    pub field: String,
    pub order: SortOrder,
}
```

**Methods:**
- `new(field, order)` - Create a new sort
- `from_params(sort_field, order_str)` - Parse from optional query params
- `is_ascending()` - Check if sort order is ascending
- `is_descending()` - Check if sort order is descending

### SortOrder

Enum for sort direction.

```rust
pub enum SortOrder {
    Asc,
    Desc,
}
```

**Methods:**
- `from_str(s: &str)` - Parse from string (`"asc"`, `"desc"`, `"ascending"`, `"descending"`)

### Pagination

Represents pagination parameters.

```rust
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}
```

**Constants:**
- `DEFAULT_LIMIT` - Default limit (50)
- `MAX_LIMIT` - Maximum allowed limit (1000)

**Methods:**
- `new(limit, offset)` - Create with clamped limit
- `from_params(limit, offset)` - Parse from optional query params
- `end()` - Get the end index (offset + limit)
- `has_next(total_count)` - Check if there's a next page
- `next_page()` - Get next page pagination
- `prev_page()` - Get previous page pagination (returns None if at start)
- `apply(&[T])` - Apply pagination to a slice

## REST Integration

### URL Encoding

Query parameters are URL-encoded. Use standard URL encoding for special characters:

```
/api/users?name=John%20Doe&status=active&sort=created_at&order=desc&limit=50&offset=0
```

### Query String Format

The expected query string format:

```
field1=value1&field2=value2&sort=field_name&order=asc|desc&limit=N&offset=M
```

**Reserved Parameters:**
- `sort` - Field to sort by
- `order` - Sort order (`asc` or `desc`)
- `limit` - Number of results to return
- `offset` - Number of results to skip

**Filter Parameters:**
- Any other parameter is treated as a filter
- Values are automatically typed (number, boolean, or string)

### REST API Example

```rust
use forgedb_query_params::QueryParams;
use std::collections::HashMap;

// In a web framework handler
fn list_users(query_string: &str) -> Vec<User> {
    let params = QueryParams::from_query_string(query_string).unwrap();
    
    // Apply filters
    let mut users = get_all_users();
    for filter in &params.filters {
        users.retain(|user| {
            match filter.field.as_str() {
                "status" => filter.matches_string(&user.status),
                "age" => filter.matches_number(user.age as f64),
                "active" => filter.matches_bool(user.active),
                _ => true,
            }
        });
    }
    
    // Apply sorting
    if let Some(sort) = &params.sort {
        users.sort_by(|a, b| {
            let cmp = match sort.field.as_str() {
                "name" => a.name.cmp(&b.name),
                "age" => a.age.cmp(&b.age),
                "created_at" => a.created_at.cmp(&b.created_at),
                _ => std::cmp::Ordering::Equal,
            };
            if sort.is_descending() {
                cmp.reverse()
            } else {
                cmp
            }
        });
    }
    
    // Apply pagination
    params.pagination.apply(&users).to_vec()
}
```

### Example URLs

```
# Get all active users, sorted by name ascending, first page
GET /api/users?status=active&sort=name&order=asc&limit=50&offset=0

# Get featured products over $100, sorted by price descending
GET /api/products?featured=true&min_price=100&sort=price&order=desc

# Get second page of results
GET /api/items?limit=25&offset=25

# Multiple filters
GET /api/articles?category=tech&published=true&author=john&sort=date&order=desc
```

## Testing

```bash
# Run all tests
cargo test -p forgedb-query-params

# Run with output
cargo test -p forgedb-query-params -- --nocapture

# Run specific test file
cargo test -p forgedb-query-params --test filter_tests
```

## Dependencies

- `serde` - Serialization/deserialization support
- `serde_urlencoded` - URL query string parsing

## Documentation

For more information about ForgeDB:

- **[ForgeDB Architecture](../../docs/ARCHITECTURE.md)** - System design and component architecture
- **[Public Crates Guide](../../docs/PUBLIC_CRATES.md)** - Complete runtime library documentation
- **[Development Guide](../../docs/DEVELOPMENT.md)** - Development setup and workflow

## License

Part of the ForgeDB project.
