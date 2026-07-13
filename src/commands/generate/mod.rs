use crate::{error::CliError, ui, Result};
use forgedb_codegen::{
    ApiGenerator, OpenApiGenerator, RustGenerator, StubGenerator, TypeScriptGenerator,
};
use forgedb_parser::Parser;
use std::fs;
use std::path::{Path, PathBuf};

pub struct GenerateOptions {
    pub target: String,
    pub check: bool,
    pub output: Option<String>,
    /// Explicit schema file path (from CLI `--schema` or config `[generate].schema`).
    /// When `None`, `find_schema_file()` searches for the default names.
    pub schema: Option<String>,
    /// Target list from `[generate].targets` in `forgedb.toml`.
    /// When present and `target` is `"all"`, only these targets are generated.
    /// Ignored for explicit single-target invocations.
    pub config_targets: Option<Vec<String>>,
    pub force: bool,
}

pub fn run(options: GenerateOptions) -> Result<()> {
    ui::header("🔨", "Generating code from schema");

    // Find schema file — explicit path takes priority over auto-discovery.
    let schema_path = match options.schema.as_deref() {
        Some(p) => p.to_string(),
        None => find_schema_file()?,
    };
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
            // When config restricts which targets to emit, honour that list;
            // otherwise generate everything.
            let allowed = options.config_targets.as_deref();
            generated_files.extend(generate_all(&schema, &output_path, options.force, allowed)?);
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
            write_ts_package_scaffold(&output_path)?;
        }
        "api" => {
            let result = ApiGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("api.rs");
            write_file(&path, &result.code, options.force)?;
            generated_files.push((path, result));
        }
        "openapi" => {
            let result = OpenApiGenerator::generate(&schema)
                .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
            let path = output_path.join("openapi.json");
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

/// Generate all (or a filtered subset of) artifacts.
///
/// `target_filter` — when `Some`, only generates targets whose names appear in
/// the slice; `None` means generate everything.  Valid names: `rust`,
/// `typescript`, `api`, `openapi`, `stubs`.
fn generate_all(
    schema: &forgedb_parser::Schema,
    output_path: &PathBuf,
    force: bool,
    target_filter: Option<&[String]>,
) -> Result<Vec<(PathBuf, forgedb_codegen::GeneratedCode)>> {
    let enabled = |name: &str| -> bool {
        target_filter.map_or(true, |ts| ts.iter().any(|t| t.as_str() == name))
    };

    let mut files = Vec::new();

    // Generate Rust database code
    if enabled("rust") {
        let rust_result = RustGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let rust_path = output_path.join("database.rs");
        write_file(&rust_path, &rust_result.code, force)?;
        files.push((rust_path, rust_result));
    }

    // Generate TypeScript types
    if enabled("typescript") {
        let ts_result = TypeScriptGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let ts_path = output_path.join("types.ts");
        write_file(&ts_path, &ts_result.code, force)?;
        files.push((ts_path, ts_result));
        write_ts_package_scaffold(output_path)?;
    }

    // Generate API
    if enabled("api") {
        let api_result = ApiGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let api_path = output_path.join("api.rs");
        write_file(&api_path, &api_result.code, force)?;
        files.push((api_path, api_result));
    }

    // Generate OpenAPI spec
    if enabled("openapi") {
        let openapi_result = OpenApiGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let openapi_path = output_path.join("openapi.json");
        write_file(&openapi_path, &openapi_result.code, force)?;
        files.push((openapi_path, openapi_result));
    }

    // Generate stubs
    if enabled("stubs") {
        let stub_result = StubGenerator::generate(schema)
            .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
        let stubs_dir = output_path.join("stubs");
        fs::create_dir_all(&stubs_dir)?;
        let stub_path = stubs_dir.join("README.md");
        write_file(&stub_path, &stub_result.code, force)?;
        files.push((stub_path, stub_result));
    }

    Ok(files)
}

/// Write the npm packaging scaffold for the generated TypeScript SDK (Phase 5
/// WS5): `package.json` + `tsconfig.json` alongside `types.ts`.  These are
/// user-editable config, so they are written ONLY when absent — a regenerate
/// (even `--force`, which overwrites `types.ts`) never clobbers them.
fn write_ts_package_scaffold(output_path: &Path) -> Result<()> {
    let files = [
        ("package.json", TypeScriptGenerator::package_json_scaffold()),
        ("tsconfig.json", TypeScriptGenerator::tsconfig_scaffold()),
    ];
    for (name, content) in files {
        let path = output_path.join(name);
        if !path.exists() {
            fs::write(&path, content)?;
            ui::info(&format!("  ✓ {} (npm SDK scaffold)", path.display()));
        }
    }
    Ok(())
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
