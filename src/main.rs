pub mod ast;
pub mod lexer;
pub mod parser;
pub mod codegen;
pub mod scaffold;

#[cfg(test)]
mod edge_case_tests;

use parser::Parser;
use codegen::CodeGenerator;
use std::fs;
use std::path::Path;

fn main() {
    // Get schema path from command line or use default
    let args: Vec<String> = std::env::args().collect();
    let schema_path = if args.len() > 1 {
        &args[1]
    } else {
        "schema.sink"
    };

    if !Path::new(schema_path).exists() {
        eprintln!("Error: {} not found", schema_path);

        // Only create default schema if using the default path
        if schema_path == "schema.sink" {
            eprintln!("Creating example schema...");
            let example_schema = r#"User {
  id: +u64
  email: &string
}
"#;
            fs::write(schema_path, example_schema).expect("Failed to write example schema");
            println!("Created example schema at {}", schema_path);
        } else {
            std::process::exit(1);
        }
    }

    // Read schema file
    let schema_content = fs::read_to_string(schema_path)
        .expect("Failed to read schema file");

    // Parse schema
    println!("Parsing schema...");
    let mut parser = match Parser::new(&schema_content) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Lexer error: {}", e);
            std::process::exit(1);
        }
    };

    let schema = match parser.parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Parser error: {}", e);
            std::process::exit(1);
        }
    };

    println!("✓ Parsed {} model(s)", schema.models.len());

    // Generate code
    println!("Generating code...");
    let generator = CodeGenerator::new();
    let generated_code = generator.generate(&schema);

    // Write generated code
    let output_dir = "generated";
    fs::create_dir_all(output_dir).expect("Failed to create output directory");

    let output_path = format!("{}/database.rs", output_dir);
    fs::write(&output_path, &generated_code).expect("Failed to write generated code");

    println!("✓ Generated code written to {}", output_path);
    println!("\nGeneration complete!");
    println!("\nNext steps:");
    println!("  1. Review generated code in {}", output_path);
    println!("  2. Run the example: cargo run --example basic");
}
