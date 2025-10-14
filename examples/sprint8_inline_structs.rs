// Sprint 8: Inline Structs & Fixed Arrays Example
// Demonstrates struct definitions, inline struct fields, fixed arrays, and zero-copy access

use sinkdb::{CodeGenerator, Parser};

fn main() -> Result<(), String> {
    println!("=== Sprint 8: Inline Structs & Fixed Arrays ===\n");

    // Define test schema with structs and fixed arrays
    let schema_source = r#"
struct Address {
  street: char(100)
  city: char(50)
  zip: char(10)
}

struct Location {
  lat: f64
  lon: f64
}

User {
  id: +uuid
  email: &string
  address: Address
  location: Location?
  tags: [char(20); 5]
}
"#;

    println!("Schema:\n{}\n", schema_source);

    // Parse schema
    println!("Parsing schema...");
    let mut parser = Parser::new(schema_source)?;
    let schema = parser.parse()?;

    println!("✓ Schema parsed successfully");
    println!("  - {} struct(s) defined", schema.structs.len());
    println!("  - {} model(s) defined\n", schema.models.len());

    // Validate struct definitions
    println!("Validating structs...");
    for struct_def in &schema.structs {
        println!("  Struct '{}' with {} fields:", struct_def.name, struct_def.fields.len());
        for field in &struct_def.fields {
            let type_str = field.field_type.to_rust_type();
            println!("    - {}: {}", field.name, type_str);
        }
    }
    println!();

    // Calculate struct sizes
    println!("Struct sizes and alignment:");
    for struct_def in &schema.structs {
        let size = sinkdb::ast::Struct::calculate_size(struct_def, &schema);
        let align = sinkdb::ast::Struct::calculate_alignment(struct_def, &schema);
        println!("  {} - size: {} bytes, alignment: {} bytes", struct_def.name, size, align);
    }
    println!();

    // Validate model fields
    println!("Model fields:");
    for model in &schema.models {
        println!("  Model '{}':", model.name);
        for field in &model.fields {
            let type_str = field.field_type.to_rust_type();
            if field.field_type.is_fixed_size() {
                let size = field.field_type.size_in_bytes(&schema);
                let align = field.field_type.alignment(&schema);
                println!("    - {}: {} (size: {} bytes, align: {} bytes)",
                    field.name, type_str, size, align);
            } else {
                println!("    - {}: {} (variable-size)", field.name, type_str);
            }
        }
    }
    println!();

    // Generate code
    println!("Generating Rust code...");
    let generator = CodeGenerator::new();
    let generated_code = generator.generate(&schema);

    println!("✓ Code generated successfully");
    println!("\nGenerated code preview (first 100 lines):\n");
    println!("{}", generated_code.lines().take(100).collect::<Vec<_>>().join("\n"));

    // Demonstrate multi-file generation
    println!("\n\nGenerating multi-file output...");
    let files = generator.generate_files(&schema);
    println!("✓ Generated {} files:", files.len());
    for file in &files {
        println!("  - {} ({} bytes)", file.path, file.content.len());
    }

    // Test struct features
    println!("\n=== Testing Struct Features ===\n");

    // 1. Fixed-size validation
    println!("1. Fixed-size validation:");
    for struct_def in &schema.structs {
        let all_fixed = struct_def.fields.iter().all(|f| f.field_type.is_fixed_size());
        println!("   Struct '{}': all fields fixed-size = {}", struct_def.name, all_fixed);
    }
    println!();

    // 2. Nested struct support
    println!("2. Nested struct references:");
    for model in &schema.models {
        for field in &model.fields {
            if let Some(struct_name) = field.field_type.struct_name() {
                let is_optional = matches!(field.field_type, sinkdb::ast::FieldType::OptionalStructType(_));
                println!("   Model '{}' field '{}' references struct '{}' (optional: {})",
                    model.name, field.name, struct_name, is_optional);
            }
        }
    }
    println!();

    // 3. Fixed array support
    println!("3. Fixed arrays:");
    for model in &schema.models {
        for field in &model.fields {
            if let sinkdb::ast::FieldType::FixedArray(inner, count) = &field.field_type {
                println!("   Model '{}' field '{}': array of {} with size {}",
                    model.name, field.name, inner.to_rust_type(), count);
            }
        }
    }
    println!();

    // 4. Zero-copy characteristics
    println!("4. Zero-copy storage characteristics:");
    println!("   - Structs use #[repr(C)] for predictable layout");
    println!("   - Fixed arrays stored inline in parent struct");
    println!("   - All struct fields have known sizes and offsets");
    println!("   - Direct memory access without serialization");
    println!();

    println!("=== Sprint 8 Demo Complete ===");

    Ok(())
}
