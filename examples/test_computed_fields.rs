use sinkdb::{CodeGenerator, Parser};

fn main() {
    let schema_source = r#"
User {
  id: +uuid
  first_name: string
  last_name: string
  full_name: string @computed

  posts: [Post]
  post_count: u32 @computed
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;

    // Parse the schema
    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    println!("✓ Schema parsed successfully");
    println!();

    // Check for computed fields
    let user_model = schema.find_model("User").expect("User model not found");
    let computed_fields: Vec<_> = user_model.fields.iter()
        .filter(|f| f.is_computed)
        .collect();

    println!("Computed fields in User model:");
    for field in &computed_fields {
        println!("  - {}: {}", field.name, field.field_type.to_rust_type());
    }
    println!();

    // Generate code
    let codegen = CodeGenerator::new();
    let generated = codegen.generate(&schema);

    println!("Generated code preview (computed trait section):");
    println!("─────────────────────────────────────────────────");

    // Extract and print the computed trait section
    if let Some(start) = generated.find("/// Computed fields trait for User") {
        if let Some(end) = generated[start..].find("pub struct UserStorage") {
            let trait_section = &generated[start..start + end];
            println!("{}", trait_section);
        }
    }

    println!("✓ Code generation successful");
    println!();
    println!("Full generated code length: {} bytes", generated.len());
}
