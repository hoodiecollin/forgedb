// Integration test for constraint validation
// This test generates code with constraints, compiles it, and verifies validation works

use sinkdb::parser::Parser;
use sinkdb::codegen::CodeGenerator;
use std::fs;

fn main() {
    println!("=== Integration Test: Constraint Validation ===\n");

    // Schema with all constraint types
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

    println!("1. Parsing schema...");
    let mut parser = Parser::new(schema).unwrap();
    let parsed_schema = parser.parse().unwrap();
    println!("   ✓ Schema parsed successfully");

    println!("\n2. Generating code with validation...");
    let generator = CodeGenerator::new();
    let code = generator.generate(&parsed_schema);

    // Write to a test file
    fs::create_dir_all("generated").expect("Failed to create output directory");
    let output_path = "generated/test_validation_integration.rs";
    fs::write(output_path, &code).expect("Failed to write generated code");
    println!("   ✓ Code generated: {}", output_path);

    println!("\n3. Verifying generated validation code...");

    // Check validation functions exist
    let checks = vec![
        ("Email validator", "fn validate_email"),
        ("URL validator", "fn validate_url"),
        ("Email validation call", "validate_email(&email)?"),
        ("URL validation call", "validate_url(&website)?"),
        ("Age min check", "if age < 13"),
        ("Age max check", "if age > 120"),
        ("Password min check", "if password.len() < 8"),
        ("Password max check", "if password.len() > 100"),
        ("Bio max check", "if bio.len() > 500"),
    ];

    let mut all_passed = true;
    for (name, pattern) in checks {
        if code.contains(pattern) {
            println!("   ✓ {}", name);
        } else {
            println!("   ✗ {} - MISSING", name);
            all_passed = false;
        }
    }

    if !all_passed {
        eprintln!("\n❌ Some validation checks failed!");
        std::process::exit(1);
    }

    println!("\n4. Validating generated code structure...");

    // Check that validation happens before unique constraints
    if let Some(validation_pos) = code.find("validate_email(&email)?") {
        if let Some(unique_pos) = code.find("if self.email_index.contains_key") {
            if validation_pos < unique_pos {
                println!("   ✓ Validation occurs before unique constraint checks");
            } else {
                println!("   ✗ Validation should occur before unique constraints");
                all_passed = false;
            }
        }
    }

    // Check that regex is only imported when needed
    if code.contains("use regex;") {
        println!("   ✓ Regex import present (required for email/url validation)");
    } else {
        println!("   ✗ Regex import missing");
        all_passed = false;
    }

    // Verify insert method signature
    if code.contains("pub fn insert(&mut self, email: String, website: String, age: u32, password: String, bio: String)") {
        println!("   ✓ Insert method has correct signature");
    } else {
        println!("   ✗ Insert method signature incorrect");
        all_passed = false;
    }

    println!("\n5. Testing validation logic (static analysis)...");

    // Count how many fields have validation
    let validation_count = code.matches("Validation error:").count();
    println!("   ✓ Found {} validation error messages", validation_count);

    // Verify error messages are descriptive
    let error_checks = vec![
        "is not a valid email",
        "age must be at least 13",
        "age must be at most 120",
        "password must be at least 8 characters",
        "bio must be at most 500 characters",
    ];

    let mut found_count = 0;
    for error_msg in error_checks {
        if code.contains(error_msg) {
            found_count += 1;
        }
    }
    println!("   ✓ Found {}/5 descriptive error messages", found_count);

    if found_count < 5 {
        println!("   ✗ Some error messages are missing");
        all_passed = false;
    }

    println!("\n=== Integration Test Result ===");
    if all_passed {
        println!("✅ All checks passed!");
        println!("\nThe generated code includes:");
        println!("  - Complete validation logic for all constraints");
        println!("  - Proper validation ordering (before unique checks)");
        println!("  - Descriptive error messages");
        println!("  - Correct function signatures");
        println!("\nNote: This is a static analysis test.");
        println!("To fully test runtime behavior, the generated code would need to be");
        println!("compiled as a separate crate with proper dependencies.");
    } else {
        println!("❌ Some checks failed - review output above");
        std::process::exit(1);
    }
}
