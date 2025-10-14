# Sprint 5: Advanced Indexing

## Overview

Sprint 5 extends Sprint 3's hash indexing with two major enhancements:
1. **Composite Indexes** - Multi-field indexes for queries on multiple columns
2. **Range Queries** - B-tree indexes for ordered range operations

## Goals

- Support composite (multi-column) indexes
- Add B-tree indexes for range queries on ordered types
- Generate type-safe range query methods
- Maintain all existing indexing functionality

## Features

### 1. Composite Indexes

#### Syntax

Use `@index(field1, field2, ...)` directive at model level:

```
User {
  id: +uuid
  first_name: string
  last_name: string
  city: string
  state: string
  country: string

  @index(first_name, last_name)
  @index(city, state, country)
}
```

#### Generated Code

**Index Storage:**
```rust
pub struct UserStorage {
    // ... existing fields ...

    // Composite indexes use tuple as key
    first_name_last_name_index: HashMap<(String, String), Vec<usize>>,
    city_state_country_index: HashMap<(String, String, String), Vec<usize>>,
}
```

**Query Methods:**
```rust
pub fn find_by_first_name_and_last_name(
    &self,
    first_name: String,
    last_name: String
) -> Vec<User> {
    // O(1) lookup on composite key
}

pub fn find_by_city_and_state_and_country(
    &self,
    city: String,
    state: String,
    country: String
) -> Vec<User> {
    // O(1) lookup on composite key
}
```

#### Benefits

- Fast lookups on multiple columns together
- Ideal for queries like "find users by city AND state"
- More efficient than separate indexes + filtering

### 2. Range Queries with B-tree Indexes

#### Supported Types

**Ordered types** automatically support range queries when indexed:
- Numeric: `u32`, `u64`, `i32`, `i64`, `f64`
- Temporal: `timestamp`
- Text: `string` (lexicographic ordering)

#### Syntax

Same as single-field indexes, but generates additional range methods:

```
Product {
  id: +uuid
  name: string
  price: ^f64           // Indexed with B-tree
  stock: ^u32           // Indexed with B-tree
  created_at: ^timestamp // Indexed with B-tree
}
```

#### Generated Code

**Index Storage:**
```rust
use std::collections::BTreeMap;

pub struct ProductStorage {
    // ... existing fields ...

    // B-tree indexes for range queries
    price_btree: BTreeMap<OrderedFloat<f64>, Vec<usize>>,
    stock_btree: BTreeMap<u32, Vec<usize>>,
    created_at_btree: BTreeMap<i64, Vec<usize>>,
}
```

**Query Methods:**
```rust
// Exact match (existing)
pub fn find_by_price(&self, price: f64) -> Vec<Product>

// Range queries (NEW)
pub fn find_by_price_range(&self, min: f64, max: f64) -> Vec<Product>
pub fn find_by_price_gt(&self, min: f64) -> Vec<Product>  // greater than
pub fn find_by_price_gte(&self, min: f64) -> Vec<Product> // greater or equal
pub fn find_by_price_lt(&self, max: f64) -> Vec<Product>  // less than
pub fn find_by_price_lte(&self, max: f64) -> Vec<Product> // less or equal

// Timestamp range queries
pub fn find_by_created_at_range(
    &self,
    start: i64,
    end: i64
) -> Vec<Product>

pub fn find_by_created_at_after(&self, timestamp: i64) -> Vec<Product>
pub fn find_by_created_at_before(&self, timestamp: i64) -> Vec<Product>
```

#### Index Type Selection

**Automatic selection based on query patterns:**

- **Hash Index** (default): O(1) exact-match lookups
  - Used for: strings (non-ordered), bool, uuid
  - Generated method: `find_by_X(value)`

- **B-tree Index**: O(log n) ordered operations
  - Used for: numeric types, timestamp
  - Generated methods: `find_by_X(value)`, `find_by_X_range()`, `find_by_X_gt()`, etc.

## Implementation Plan

### 1. Lexer Extensions

**New Tokens:**
```rust
Token::At,           // @ for directives
Token::Comma,        // , for directive arguments
Token::LParen,       // ( for directive args
Token::RParen,       // ) for directive args
```

### 2. AST Extensions

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeIndex {
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndexType {
    Hash,      // Default for exact matches
    BTree,     // For range queries on ordered types
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub auto_generate: bool,
    pub unique: bool,
    pub indexed: bool,
    pub index_type: IndexType,  // NEW: track index type
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub composite_indexes: Vec<CompositeIndex>,  // NEW
}
```

### 3. Parser Extensions

**Parse `@index` directive:**
```rust
fn parse_directive(&mut self) -> Result<Directive, String> {
    // @index(field1, field2, ...)
    self.expect(Token::At)?;
    let ident = self.expect_ident()?;

    match ident.as_str() {
        "index" => self.parse_index_directive(),
        _ => Err(format!("Unknown directive: @{}", ident)),
    }
}

fn parse_index_directive(&mut self) -> Result<CompositeIndex, String> {
    self.expect(Token::LParen)?;
    let fields = self.parse_field_list()?;
    self.expect(Token::RParen)?;
    Ok(CompositeIndex { fields })
}
```

**Determine index type for fields:**
```rust
fn determine_index_type(field_type: &FieldType) -> IndexType {
    match field_type {
        FieldType::U32 | FieldType::U64
        | FieldType::I32 | FieldType::I64
        | FieldType::F64 | FieldType::Timestamp => IndexType::BTree,
        _ => IndexType::Hash,
    }
}
```

### 4. Codegen Extensions

**Generate composite index storage:**
```rust
fn generate_composite_index_field(
    &self,
    index: &CompositeIndex,
    model: &Model
) -> String {
    let field_types = index.fields
        .iter()
        .map(|f| self.get_field_rust_type(f, model))
        .collect::<Vec<_>>();

    let tuple_type = format!("({})", field_types.join(", "));
    let index_name = index.fields.join("_");

    format!(
        "{}_index: std::collections::HashMap<{}, Vec<usize>>",
        index_name, tuple_type
    )
}
```

**Generate B-tree index storage:**
```rust
fn generate_btree_index_field(&self, field: &Field) -> String {
    let rust_type = self.field_type_to_rust(&field.field_type);
    let key_type = if field.field_type == FieldType::F64 {
        "ordered_float::OrderedFloat<f64>".to_string()
    } else {
        rust_type.clone()
    };

    format!(
        "{}_btree: std::collections::BTreeMap<{}, Vec<usize>>",
        field.name, key_type
    )
}
```

**Generate range query methods:**
```rust
fn generate_range_query_methods(&self, field: &Field, model: &Model) -> String {
    let method_name = format!("find_by_{}_range", field.name);
    let param_type = self.field_type_to_rust(&field.field_type);

    format!(r#"
    pub fn {}(&self, min: {}, max: {}) -> Vec<{}> {{
        let mut results = Vec::new();
        for (_key, indices) in self.{}_btree.range(min..=max) {{
            for &idx in indices {{
                if !self.tombstones[idx] {{
                    results.push(self.records[idx].clone());
                }}
            }}
        }}
        results
    }}
    "#, method_name, param_type, param_type, model.name, field.name)
}
```

### 5. Dependencies

**Add to Cargo.toml:**
```toml
[dependencies]
ordered-float = "4.0"  # For f64 in BTreeMap
```

## Test Schema

```
Product {
  id: +uuid
  name: string
  category: string
  price: ^f64
  stock: ^u32
  created_at: ^timestamp

  @index(category, name)
}

User {
  id: +uuid
  first_name: string
  last_name: string
  city: string
  state: string
  age: ^u32

  @index(first_name, last_name)
  @index(city, state)
}
```

## Example Usage

```rust
// Composite index queries
let users = storage.find_by_city_and_state("Seattle".to_string(), "WA".to_string());
let products = storage.find_by_category_and_name("Electronics".to_string(), "Laptop".to_string());

// Range queries
let cheap_products = storage.find_by_price_lte(50.0);
let expensive_products = storage.find_by_price_gte(1000.0);
let mid_range = storage.find_by_price_range(100.0, 500.0);

// Timestamp ranges
let recent = storage.find_by_created_at_after(start_timestamp);
let old = storage.find_by_created_at_before(end_timestamp);
let period = storage.find_by_created_at_range(start, end);
```

## Performance Characteristics

| Operation | Hash Index | B-tree Index | Composite Hash |
|-----------|-----------|--------------|----------------|
| Exact match | O(1) | O(log n) | O(1) |
| Range query | N/A | O(log n + k) | N/A |
| Insert | O(1) | O(log n) | O(1) |
| Delete | O(1) | O(log n) | O(1) |
| Memory | Low | Medium | Medium |

Where k = number of results in range.

## Success Criteria

- ✅ Parse `@index(field1, field2)` directives
- ✅ Generate composite index storage structures
- ✅ Generate composite index query methods
- ✅ Determine B-tree vs Hash index based on type
- ✅ Generate range query methods for ordered types
- ✅ All indexes maintained on insert/update/delete
- ✅ Comprehensive test coverage
- ✅ Example demonstrating all features

## Known Limitations

1. **No partial composite index matching** - Must query all fields in composite index
2. **No index hints** - Cannot manually specify hash vs btree
3. **No covering indexes** - All queries return full records
4. **No index statistics** - No query planner optimization

These may be addressed in future sprints.

## Next Steps (Sprint 6+)

- Index persistence (WAL integration)
- Query optimizer with cost-based index selection
- Covering indexes (index-only scans)
- Spatial indexes for geo queries
- Full-text search indexes

---

**Sprint Status**: 🚧 In Progress
**Target Completion**: Sprint 5
**Dependencies**: Sprint 3 (indexing), Sprint 4 (relations)
