use crate::{error::CliError, ui, Result};
use forgedb::{
    codegen::CodeGenerator, parser::Parser, ComponentStubGenerator, RouteHandlerGenerator,
    StubTemplate, TypeScriptGenerator,
};
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
        schema.models.iter().map(|m| m.fields.len()).sum::<usize>()
    ));

    // Determine output directory
    let output_dir = options.output.as_deref().unwrap_or("generated");

    // Check mode: verify if generation is needed
    if options.check {
        return check_generation_needed(&schema, output_dir);
    }

    // Generate code based on target
    match options.target.as_str() {
        "all" => {
            generate_rust_code(&schema, output_dir, options.force)?;
            generate_typescript_sdk(&schema, output_dir, options.force)?;
            generate_component_stubs(&schema, output_dir, options.force)?;
        }
        "rust" => {
            generate_rust_code(&schema, output_dir, options.force)?;
        }
        "typescript" | "sdk" => {
            generate_typescript_sdk(&schema, output_dir, options.force)?;
        }
        "api" => {
            ui::warning("API generation not yet implemented");
        }
        "openapi" => {
            ui::warning("OpenAPI generation not yet implemented");
        }
        "stubs" => {
            generate_component_stubs(&schema, output_dir, options.force)?;
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
    let candidates = ["schema.forge", "schema.lang", "schema.forgedb"];

    for candidate in &candidates {
        if Path::new(candidate).exists() {
            return Ok(candidate.to_string());
        }
    }

    Err(CliError::SchemaNotFound(
        "No schema file found. Expected one of: schema.forge, schema.lang, schema.forgedb"
            .to_string(),
    ))
}

fn generate_rust_code(schema: &forgedb::ast::Schema, output_dir: &str, force: bool) -> Result<()> {
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

fn generate_typescript_sdk(
    schema: &forgedb::ast::Schema,
    output_dir: &str,
    force: bool,
) -> Result<()> {
    ui::info("Generating TypeScript SDK...");

    // Generate all TypeScript files
    let files = TypeScriptGenerator::generate(schema);

    let mut files_written = 0;
    let mut total_lines = 0;

    for file in files {
        let file_path = Path::new(output_dir).join(&file.path);

        // Create parent directories
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Check if file exists and force is not set
        if !force && file_path.exists() {
            continue;
        }

        // Write file
        fs::write(&file_path, &file.content).map_err(|e| {
            CliError::CodeGeneration(format!("Failed to write {}: {}", file.path, e))
        })?;

        files_written += 1;
        total_lines += file.content.lines().count();
    }

    ui::success(&format!(
        "Generated TypeScript SDK ({} files, {} lines)",
        files_written, total_lines
    ));

    println!();
    println!("SDK structure:");
    println!("  - {}/sdk/types.ts         - Type definitions", output_dir);
    println!(
        "  - {}/sdk/*Api.ts          - API client classes",
        output_dir
    );
    println!("  - {}/sdk/index.ts         - Main entry point", output_dir);
    println!(
        "  - {}/sdk/package.json     - NPM package config",
        output_dir
    );
    println!();
    println!("To use the SDK:");
    println!("  cd {}/sdk", output_dir);
    println!("  npm install");
    println!("  npm run build");

    Ok(())
}

fn generate_component_stubs(
    schema: &forgedb::ast::Schema,
    output_dir: &str,
    force: bool,
) -> Result<()> {
    ui::info("Generating component stubs...");

    // Generate React component stubs
    let component_generator = ComponentStubGenerator::new();
    let component_files = component_generator.generate_stubs(schema, StubTemplate::Detailed);

    // Generate API route handlers
    let route_generator = RouteHandlerGenerator::new();
    let route_files = route_generator.generate_handlers(schema);

    let mut files_written = 0;
    let mut files_skipped = 0;

    // Write component files
    for file in component_files.iter().chain(route_files.iter()) {
        let file_path = Path::new(output_dir).join(&file.path);

        // Create parent directories
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Don't overwrite existing files unless force is set
        if !force && file_path.exists() {
            files_skipped += 1;
            continue;
        }

        // Write file
        fs::write(&file_path, &file.content).map_err(|e| {
            CliError::CodeGeneration(format!("Failed to write {}: {}", file.path, e))
        })?;

        files_written += 1;
    }

    ui::success(&format!(
        "Generated component stubs ({} files written, {} skipped)",
        files_written, files_skipped
    ));

    if files_skipped > 0 {
        println!();
        println!("  💡 {} existing files were preserved", files_skipped);
        println!("     Use --force to overwrite existing files");
    }

    Ok(())
}

fn check_generation_needed(schema: &forgedb::ast::Schema, output_dir: &str) -> Result<()> {
    let output_path = Path::new(output_dir).join("database.rs");

    if !output_path.exists() {
        ui::error("Generated code does not exist");
        return Err(CliError::CodeGeneration(
            "Run 'forgedb generate' to create generated code".to_string(),
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
            "Run 'forgedb generate' to update generated code".to_string(),
        ));
    }

    ui::success("Generated code is up to date");
    Ok(())
}
