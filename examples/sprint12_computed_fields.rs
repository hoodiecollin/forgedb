// Sprint 12: Computed Fields Example
// Demonstrates @computed directive and runtime computation

use sinkdb::{CodeGenerator, Parser};
use std::fs;
use std::path::Path;

fn main() {
    println!("Sprint 12: Computed Fields Example");
    println!("===================================\n");

    let schema_source = r#"
User {
  id: +uuid
  first_name: string
  last_name: string
  full_name: string @computed
  email: ^&string @email
}
"#;

    // Parse the schema
    let mut parser = Parser::new(schema_source).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    println!("✓ Schema parsed successfully");

    // Check for computed fields
    let user_model = schema.find_model("User").expect("User model not found");
    let computed_fields: Vec<_> = user_model.fields.iter()
        .filter(|f| f.is_computed)
        .collect();

    println!("\nComputed fields detected:");
    for field in &computed_fields {
        println!("  • {} ({})", field.name, field.field_type.to_rust_type());
    }

    // Generate code
    let codegen = CodeGenerator::new();
    let files = codegen.generate_files(&schema);

    println!("\n✓ Generated {} files", files.len());

    // Create output directory
    let output_dir = "generated/sprint12_computed";
    fs::create_dir_all(output_dir).expect("Failed to create output directory");

    // Write generated files
    for file in &files {
        let file_path = Path::new(output_dir).join(&file.path);
        fs::write(&file_path, &file.content).expect("Failed to write file");
        println!("  • {}", file.path);
    }

    println!("\n✓ Files written to {}/", output_dir);

    // Show the generated trait
    println!("\nGenerated Computed Trait:");
    println!("─────────────────────────");

    if let Some(user_storage_file) = files.iter().find(|f| f.path == "user_storage.rs") {
        if let Some(start) = user_storage_file.content.find("/// Computed fields trait") {
            if let Some(end) = user_storage_file.content[start..].find("pub struct UserStorage") {
                let trait_section = &user_storage_file.content[start..start + end];
                println!("{}", trait_section);
            }
        }

        // Show the computed accessor methods
        println!("\nGenerated Accessor Methods:");
        println!("───────────────────────────");
        if let Some(start) = user_storage_file.content.find("/// Get a record with its computed fields") {
            if let Some(end_marker) = user_storage_file.content[start..].find("}\n}\n") {
                let accessor_section = &user_storage_file.content[start..start + end_marker + 2];
                println!("{}", accessor_section);
            }
        }
    }

    println!("\n✓ Computed fields implementation complete!");
    println!("\nUsage Example:");
    println!("──────────────");
    println!("// Implement the trait:");
    println!("impl UserComputed for MyUserComputed {{");
    println!("    fn full_name(instance: &User) -> String {{");
    println!("        format!(\"{{}} {{}}\", instance.first_name, instance.last_name)");
    println!("    }}");
    println!("}}");
    println!("");
    println!("// Use it:");
    println!("let full_name = storage.compute_full_name::<MyUserComputed>(user_id)?;");
}
