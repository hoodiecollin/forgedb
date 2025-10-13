/// Sprint 5: Advanced Indexing Example
///
/// Demonstrates:
/// 1. Composite indexes for multi-field queries
/// 2. B-tree indexes with range queries
/// 3. Integration with existing indexing features

use sinkdb::parser::Parser;
use sinkdb::codegen::CodeGenerator;

fn main() {
    println!("=== Sprint 5: Advanced Indexing Demo ===\n");

    // Schema with composite indexes and range-queryable fields
    let schema = r#"
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
"#;

    println!("Schema:");
    println!("{}\n", schema);

    // Parse schema
    let mut parser = Parser::new(schema).unwrap();
    let parsed_schema = parser.parse().unwrap();

    println!("✅ Schema parsed successfully\n");

    // Validate
    if let Err(e) = parsed_schema.validate_relations() {
        println!("❌ Validation error: {}", e);
        return;
    }

    println!("✅ Schema validated\n");

    // Generate code
    let codegen = CodeGenerator::new();
    let generated_code = codegen.generate(&parsed_schema);

    println!("✅ Code generated ({} bytes)\n", generated_code.len());

    // Check for composite index fields
    assert!(generated_code.contains("category_name_index:"), "Missing composite index: category_name");
    assert!(generated_code.contains("first_name_last_name_index:"), "Missing composite index: first_name_last_name");
    assert!(generated_code.contains("city_state_index:"), "Missing composite index: city_state");
    println!("✅ Composite index storage fields generated\n");

    // Check for B-tree index fields
    assert!(generated_code.contains("price_btree:"), "Missing B-tree index: price");
    assert!(generated_code.contains("stock_btree:"), "Missing B-tree index: stock");
    assert!(generated_code.contains("created_at_btree:"), "Missing B-tree index: created_at");
    assert!(generated_code.contains("age_btree:"), "Missing B-tree index: age");
    println!("✅ B-tree index storage fields generated\n");

    // Check for composite find_by methods
    assert!(generated_code.contains("pub fn find_by_category_and_name"), "Missing composite find method");
    assert!(generated_code.contains("pub fn find_by_first_name_and_last_name"), "Missing composite find method");
    assert!(generated_code.contains("pub fn find_by_city_and_state"), "Missing composite find method");
    println!("✅ Composite index query methods generated\n");

    // Check for range query methods
    assert!(generated_code.contains("pub fn find_by_price_range"), "Missing range query method: price_range");
    assert!(generated_code.contains("pub fn find_by_price_gt"), "Missing range query method: price_gt");
    assert!(generated_code.contains("pub fn find_by_price_gte"), "Missing range query method: price_gte");
    assert!(generated_code.contains("pub fn find_by_price_lt"), "Missing range query method: price_lt");
    assert!(generated_code.contains("pub fn find_by_price_lte"), "Missing range query method: price_lte");
    println!("✅ Range query methods generated (price)\n");

    assert!(generated_code.contains("pub fn find_by_stock_range"), "Missing range query method: stock_range");
    assert!(generated_code.contains("pub fn find_by_age_range"), "Missing range query method: age_range");
    println!("✅ Range query methods generated (stock, age)\n");

    // Check for ordered_float usage with f64
    assert!(generated_code.contains("ordered_float::OrderedFloat"), "Missing ordered_float for f64 B-tree");
    println!("✅ ordered_float integration for f64 B-tree index\n");

    println!("=== Example Summary ===");
    println!("✅ All Sprint 5 features successfully generated:");
    println!("   • Composite indexes (@index directive)");
    println!("   • B-tree indexes for ordered types");
    println!("   • Range query methods (_range, _gt, _gte, _lt, _lte)");
    println!("   • Composite index query methods");
    println!("   • Proper index type selection (Hash vs BTree)");
    println!("\n🎉 Sprint 5: Advanced Indexing - Complete!");
}
