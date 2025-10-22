use crate::{error::CliError, ui, Result};
use forgedb_codegen::{
    ApiGenerator, OpenApiFormat, OpenApiGenerator, RustGenerator, StubGenerator,
    TypeScriptGenerator,
};
use forgedb_parser::Parser;
use std::fs;
use std::path::{Path, PathBuf};

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
    let output_dir = options.output.as_deref().unwrap_or("./generated");
    let output_path = PathBuf::from(output_dir);

    // Create output directory
    fs::create_dir_all(&output_path)?;

    // Check mode - verify nothing needs regeneration
    if options.check {
        ui::info("Running in check mode...");
        // TODO: Implement check logic
        ui::warning("Check mode not yet implemented");
        return Ok(());
    }

    // Generate based on target
    let target = options.target.to_lowercase();
    let mut generated_files = Vec::new();

    match target.as_str() {
        "all" => {
            // Generate everything
            generated_files.extend(generate_all(&schema, &output_path, options.force)?);
        }
        "rust" => {
            let result = RustGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("database.rs");
            write_file(&path, &result.code, options.force)?;
            generated_files.push((path, result));
        }
        "typescript" => {
            let result = TypeScriptGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("types.ts");
            write_file(&path, &result.code, options.force)?;
            generated_files.push((path, result));
        }
        "api" => {
            let result = ApiGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("api.rs");
            write_file(&path, &result.code, options.force)?;
            generated_files.push((path, result));
        }
        "openapi" => {
            let result = OpenApiGenerator::generate(&schema, OpenApiFormat::Yaml)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("openapi.yaml");
            write_file(&path, &result.code, options.force)?;
            generated_files.push((path, result));
        }
        "stubs" => {
            let result = StubGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let stubs_dir = output_path.join("stubs");
            fs::create_dir_all(&stubs_dir)?;
            let path = stubs_dir.join("README.md");
            write_file(&path, &result.code, options.force)?;
            generated_files.push((path, result));
        }
        _ => {
            return Err(CliError::Other(format!(
                "Unknown target: {}. Valid targets: all, rust, typescript, api, openapi, stubs",
                target
            )));
        }
    }

    // Report results
    ui::success(&format!("Generated {} files:", generated_files.len()));
    for (path, result) in &generated_files {
        ui::info(&format!(
            "  ✓ {} ({} lines) - {}",
            path.display(),
            result.line_count(),
            result.description
        ));
    }

    Ok(())
}

fn generate_all(
    schema: &forgedb_parser::Schema,
    output_path: &PathBuf,
    force: bool,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let mut files = Vec::new();

    // Generate Rust database code
    let rust_result = RustGenerator::generate(schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let rust_path = output_path.join("database.rs");
    write_file(&rust_path, &rust_result.code, force)?;
    files.push((rust_path, rust_result));

    // Generate TypeScript types
    let ts_result = TypeScriptGenerator::generate(schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let ts_path = output_path.join("types.ts");
    write_file(&ts_path, &ts_result.code, force)?;
    files.push((ts_path, ts_result));

    // Generate API
    let api_result = ApiGenerator::generate(schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let api_path = output_path.join("api.rs");
    write_file(&api_path, &api_result.code, force)?;
    files.push((api_path, api_result));

    // Generate OpenAPI spec
    let openapi_result = OpenApiGenerator::generate(schema, OpenApiFormat::Yaml)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let openapi_path = output_path.join("openapi.yaml");
    write_file(&openapi_path, &openapi_result.code, force)?;
    files.push((openapi_path, openapi_result));

    // Generate stubs
    let stub_result = StubGenerator::generate(schema)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    let stubs_dir = output_path.join("stubs");
    fs::create_dir_all(&stubs_dir)?;
    let stub_path = stubs_dir.join("README.md");
    write_file(&stub_path, &stub_result.code, force)?;
    files.push((stub_path, stub_result));

    Ok(files)
}

fn write_file(path: &PathBuf, content: &str, force: bool) -> Result<()> {
    // Check if file exists and we're not forcing
    if path.exists() && !force {
        return Err(CliError::Other(format!(
            "File exists: {}. Use --force to overwrite",
            path.display()
        )));
    }

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write file
    fs::write(path, content)?;

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
