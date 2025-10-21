use crate::{error::CliError, ui, Result};
use forgedb_parser::Parser;
use std::fs;
use std::path::Path;

pub struct GenerateOptions {
    pub target: String,
    pub check: bool,
    pub output: Option<String>,
    pub force: bool,
}

pub fn run(_options: GenerateOptions) -> Result<()> {
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

    ui::warning("Code generation has been temporarily removed during refactoring");
    ui::info("The parser is working, but code generation will be reimplemented");

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
