// Sprint 5: Constraints & Validation Example
//
// This example demonstrates the constraint validation features:
// - @email - Email format validation
// - @url - URL format validation
// - @min/@max - Numeric range constraints
// - @min/@max - String length constraints
//
// Run this example: cargo run --example sprint5_constraints

use sinkdb::parser::Parser;
use sinkdb::codegen::CodeGenerator;
use std::fs;

fn main() {
    println!("=== Sprint 5: Schema Constraints & Validation ===\n");

    // Define schema with various constraints
    let schema = r#"
User {
  id: +uuid
  email: ^&string @email
  website: string @url
  age: u32 @min(13) @max(120)
  password: string @min(8) @max(100)
  bio: string @max(500)
}
"#;

    println!("Schema:");
    println!("{}", schema);

    // Parse schema
    println!("\n1. Parsing schema with constraints...");
    let mut parser = Parser::new(schema).unwrap();
    let parsed_schema = parser.parse().unwrap();

    println!("   ✓ Parsed successfully");
    println!("   - Found {} model(s)", parsed_schema.models.len());

    // Check constraints
    let user_model = &parsed_schema.models[0];
    for field in &user_model.fields {
        if !field.constraints.is_empty() {
            println!("   - Field '{}' has {} constraint(s)", field.name, field.constraints.len());
            for constraint in &field.constraints {
                print!("     - @{}", constraint.name);
                if !constraint.params.is_empty() {
                    print!("(");
                    for (i, param) in constraint.params.iter().enumerate() {
                        if i > 0 { print!(", "); }
                        match param {
                            sinkdb::ast::ConstraintParam::Number(n) => print!("{}", n),
                            sinkdb::ast::ConstraintParam::String(s) => print!("\"{}\"", s),
                        }
                    }
                    print!(")");
                }
                println!();
            }
        }
    }

    // Generate code
    println!("\n2. Generating Rust code with validation...");
    let generator = CodeGenerator::new();
    let generated_code = generator.generate(&parsed_schema);

    // Write generated code
    fs::create_dir_all("generated").expect("Failed to create output directory");
    let output_path = "generated/sprint5_constraints.rs";
    fs::write(output_path, &generated_code).expect("Failed to write generated code");

    println!("   ✓ Generated code written to {}", output_path);

    // Show what validation was generated
    println!("\n3. Generated validation features:");
    if generated_code.contains("use regex;") {
        println!("   ✓ Regex import added for pattern validation");
    }
    if generated_code.contains("fn validate_email") {
        println!("   ✓ Email validation function");
    }
    if generated_code.contains("fn validate_url") {
        println!("   ✓ URL validation function");
    }
    if generated_code.contains("if age < 13") {
        println!("   ✓ Age minimum constraint (13)");
    }
    if generated_code.contains("if age > 120") {
        println!("   ✓ Age maximum constraint (120)");
    }
    if generated_code.contains("if password.len() < 8") {
        println!("   ✓ Password minimum length (8 characters)");
    }
    if generated_code.contains("if bio.len() > 500") {
        println!("   ✓ Bio maximum length (500 characters)");
    }

    println!("\n4. Testing validation (conceptual - generated code would enforce):");
    println!("   ✓ Valid email: user@example.com");
    println!("   ✗ Invalid email: not-an-email");
    println!("   ✓ Valid website: https://example.com");
    println!("   ✗ Invalid website: not-a-url");
    println!("   ✓ Valid age: 25 (between 13-120)");
    println!("   ✗ Invalid age: 10 (below minimum)");
    println!("   ✓ Valid password: \"securepass123\" (8+ chars)");
    println!("   ✗ Invalid password: \"short\" (too short)");
    println!("   ✓ Valid bio: \"Hello, world!\" (under 500 chars)");
    println!("   ✗ Invalid bio: [501+ character string] (too long)");

    println!("\n=== Example complete! ===");
    println!("\nNext steps:");
    println!("  1. Review generated validation code in {}", output_path);
    println!("  2. The generated insert() method will automatically validate all constraints");
    println!("  3. Invalid data will return descriptive error messages");
}
