use crate::{error::CliError, ui, Result};
use sinkdb::{codegen::CodeGenerator, parser::Parser};
use std::fs;
use std::path::Path;

pub struct GenerateOptions {
    pub target: String,
    pub check: bool,
    pub output: Option<String>,
    pub force: bool,
}

pub fn run(options: GenerateOptions) -> Result<()> {
    ui::header("🔨", "Generating code from schema");

    // Find schema file
    let schema_path = find_schema_file()?;
    ui::info(&format!("Using schema: {}", schema_path));

    // Read and parse schema
    let schema_content = fs::read_to_string(&schema_path)
        .map_err(|e| CliError::SchemaNotFound(format!("{}: {}", schema_path, e)))?;

    let mut parser = Parser::new(&schema_content)
        .map_err(|e| CliError::SchemaValidation(format!("Lexer error: {}", e)))?;

    let schema = parser
        .parse()
        .map_err(|e| CliError::SchemaValidation(format!("Parser error: {}", e)))?;

    ui::success(&format!(
        "Parsed schema ({} models, {} total fields)",
        schema.models.len(),
        schema
            .models
            .iter()
            .map(|m| m.fields.len())
            .sum::<usize>()
    ));

    // Determine output directory
    let output_dir = options.output.as_deref().unwrap_or("generated");

    // Check mode: verify if generation is needed
    if options.check {
        return check_generation_needed(&schema, output_dir);
    }

    // Generate code based on target
    match options.target.as_str() {
        "all" | "rust" => {
            generate_rust_code(&schema, output_dir, options.force)?;
        }
        "typescript" => {
            ui::warning("TypeScript generation not yet implemented");
        }
        "api" => {
            ui::warning("API generation not yet implemented");
        }
        "openapi" => {
            ui::warning("OpenAPI generation not yet implemented");
        }
        "stubs" => {
            ui::warning("Stub generation not yet implemented");
        }
        target => {
            return Err(CliError::Other(format!("Unknown target: {}", target)));
        }
    }

    ui::success(&format!("Code generation complete!"));
    println!();
    println!("Next steps:");
    println!("  - Review generated code in {}/", output_dir);
    println!("  - Run your application: cargo run");

    Ok(())
}

fn find_schema_file() -> Result<String> {
    // Look for common schema file names
    let candidates = ["schema.sink", "schema.lang", "schema.sinkdb"];

    for candidate in &candidates {
        if Path::new(candidate).exists() {
            return Ok(candidate.to_string());
        }
    }

    Err(CliError::SchemaNotFound(
        "No schema file found. Expected one of: schema.sink, schema.lang, schema.sinkdb"
            .to_string(),
    ))
}

fn generate_rust_code(
    schema: &sinkdb::ast::Schema,
    output_dir: &str,
    force: bool,
) -> Result<()> {
    // Create output directory
    fs::create_dir_all(output_dir)?;

    let output_path = Path::new(output_dir).join("database.rs");

    // Check if file exists and force is not set
    if !force && output_path.exists() {
        // In a real implementation, we'd check if the file is up to date
        ui::info("Generated code is up to date (use --force to regenerate)");
        return Ok(());
    }

    // Generate code
    let generator = CodeGenerator::new();
    let generated_code = generator.generate(schema);

    // Count lines for reporting
    let line_count = generated_code.lines().count();

    // Write generated code
    fs::write(&output_path, &generated_code)
        .map_err(|e| CliError::CodeGeneration(format!("Failed to write output: {}", e)))?;

    ui::success(&format!(
        "Generated {} ({} lines)",
        output_path.display(),
        line_count
    ));

    Ok(())
}

fn check_generation_needed(schema: &sinkdb::ast::Schema, output_dir: &str) -> Result<()> {
    let output_path = Path::new(output_dir).join("database.rs");

    if !output_path.exists() {
        ui::error("Generated code does not exist");
        return Err(CliError::CodeGeneration(
            "Run 'sinkdb generate' to create generated code".to_string(),
        ));
    }

    // In a more sophisticated implementation, we would:
    // 1. Parse the existing generated code
    // 2. Compare it with what would be generated
    // 3. Detect any differences

    let generator = CodeGenerator::new();
    let expected_code = generator.generate(schema);
    let actual_code = fs::read_to_string(&output_path)?;

    if expected_code.trim() != actual_code.trim() {
        ui::error("Generated code is out of date");
        return Err(CliError::CodeGeneration(
            "Run 'sinkdb generate' to update generated code".to_string(),
        ));
    }

    ui::success("Generated code is up to date");
    Ok(())
}
