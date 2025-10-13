use sinkdb::parser::Parser;

fn main() {
    println!("Testing validation with invalid schema...\n");

    // Test 1: Invalid model name (should be PascalCase)
    let invalid_model = r#"
user_model {
  name: string
}
"#;

    println!("Test 1: Invalid model name");
    let mut parser = Parser::new(invalid_model).unwrap();
    match parser.parse() {
        Ok(_) => println!("  ✗ Should have failed"),
        Err(e) => println!("  ✓ Error: {}\n", e),
    }

    // Test 2: Invalid field name (should be snake_case)
    let invalid_field = r#"
User {
  UserName: string
}
"#;

    println!("Test 2: Invalid field name");
    let mut parser = Parser::new(invalid_field).unwrap();
    match parser.parse() {
        Ok(_) => println!("  ✗ Should have failed"),
        Err(e) => println!("  ✓ Error: {}\n", e),
    }

    // Test 3: Valid schema
    let valid_schema = r#"
User {
  id: +u64
  email: &string
  user_name: string
}

Post {
  id: +u64
  title: string
}
"#;

    println!("Test 3: Valid schema");
    let mut parser = Parser::new(valid_schema).unwrap();
    match parser.parse() {
        Ok(schema) => println!("  ✓ Parsed {} models successfully\n", schema.models.len()),
        Err(e) => println!("  ✗ Unexpected error: {}\n", e),
    }

    // Test 4: Multiple errors (model and field)
    let multiple_errors = r#"
user_model {
  id: +u64
  BadFieldName: string
}
"#;

    println!("Test 4: Multiple validation errors");
    let mut parser = Parser::new(multiple_errors).unwrap();
    match parser.parse() {
        Ok(_) => println!("  ✗ Should have failed"),
        Err(e) => println!("  ✓ Error: {}\n", e),
    }

    println!("Validation testing complete!");
}
