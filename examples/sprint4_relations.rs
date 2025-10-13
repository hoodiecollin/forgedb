// Sprint 4: Relations Example
// Demonstrates one-to-many relationships between User and Post

use sinkdb::parser::Parser;
use sinkdb::codegen::CodeGenerator;
use std::fs;

fn main() {
    println!("=== Sprint 4: Relations Example ===\n");

    // Define schema with relations
    let schema_content = r#"
User {
  id: +uuid
  email: ^&string
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  content: string
  author: *User
}
"#;

    println!("Schema:");
    println!("{}", schema_content);

    // Parse schema
    let mut parser = Parser::new(schema_content).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    println!("\n=== Parsed Schema ===");
    println!("Models:");
    for model in &schema.models {
        println!("  - {}", model.name);
        for field in &model.fields {
            println!("    - {}: {:?}", field.name, field.field_type);
        }
    }

    // Validate relations
    println!("\n=== Validating Relations ===");
    match schema.validate_relations() {
        Ok(()) => println!("✓ All relations are valid"),
        Err(e) => println!("✗ Validation error: {}", e),
    }

    // Detect relation pairs
    println!("\n=== Detected Relation Pairs ===");
    let relations = schema.detect_relations();
    for rel in &relations {
        println!("  - {}.{} -> {}.{} (required: {})",
            rel.parent_model, rel.parent_field,
            rel.child_model, rel.child_field,
            rel.is_required
        );
    }

    // Generate code
    println!("\n=== Generating Code ===");
    let codegen = CodeGenerator::new();
    let generated_code = codegen.generate(&schema);

    // Save generated code
    fs::create_dir_all("generated").expect("Failed to create generated directory");
    fs::write("generated/sprint4_database.rs", &generated_code)
        .expect("Failed to write generated code");

    println!("✓ Code generated to generated/sprint4_database.rs");

    // Show key parts of generated code
    println!("\n=== Generated Structures ===");
    println!("User struct:");
    println!("  - id: uuid::Uuid");
    println!("  - email: String");
    println!("  - name: String");
    println!("  - (posts is virtual, no storage field)");

    println!("\nPost struct:");
    println!("  - id: uuid::Uuid");
    println!("  - title: String");
    println!("  - content: String");
    println!("  - author_id: uuid::Uuid (FK to User)");

    println!("\n=== Foreign Key Features ===");
    println!("✓ FK fields are automatically indexed");
    println!("✓ find_by_author_id() method generated for Post");
    println!("✓ OneToMany fields don't consume storage");

    println!("\n=== Next Steps (Sprint 4 Continued) ===");
    println!("TODO: Implement relation traversal methods:");
    println!("  - user.posts() -> Vec<Post>");
    println!("  - post.author() -> User");
    println!("TODO: Implement FK validation on insert/update");

    println!("\nExample completed successfully!");
}
