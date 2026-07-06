use forgedb_codegen::{ApiGenerator, RustGenerator, StubGenerator, TypeScriptGenerator};
use forgedb_parser::Schema;
use std::fs;
use std::path::{Path, PathBuf};

/// Error types for regeneration
#[derive(Debug)]
pub enum RegenerateError {
    IoError(std::io::Error),
    ParseError(String),
    GenerationError(String),
}

impl From<std::io::Error> for RegenerateError {
    fn from(err: std::io::Error) -> Self {
        RegenerateError::IoError(err)
    }
}

impl std::fmt::Display for RegenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegenerateError::IoError(e) => write!(f, "I/O error: {}", e),
            RegenerateError::ParseError(e) => write!(f, "Parse error: {}", e),
            RegenerateError::GenerationError(e) => write!(f, "Generation error: {}", e),
        }
    }
}

impl std::error::Error for RegenerateError {}

/// Result of a regeneration attempt
#[derive(Debug)]
pub struct RegenerateResult {
    pub success: bool,
    pub message: String,
    pub output_path: Option<PathBuf>,
}

/// Handles auto-regeneration of code from schema changes
pub struct SchemaRegenerator {
    schema_path: PathBuf,
    output_dir: PathBuf,
}

impl SchemaRegenerator {
    /// Create a new regenerator
    pub fn new<P: AsRef<Path>>(schema_path: P, output_dir: P) -> Self {
        SchemaRegenerator {
            schema_path: schema_path.as_ref().to_path_buf(),
            output_dir: output_dir.as_ref().to_path_buf(),
        }
    }

    /// Regenerate code from the schema file.
    ///
    /// Reads the schema, parses it, runs all four generators (Rust, TypeScript,
    /// API, stubs) and writes the output files to the configured output
    /// directory.  Returns a [`RegenerateResult`] describing what happened.
    pub fn regenerate(&self) -> RegenerateResult {
        // Verify schema file exists
        if !self.schema_path.exists() {
            return RegenerateResult {
                success: false,
                message: format!("Schema file not found: {}", self.schema_path.display()),
                output_path: None,
            };
        }

        // Read schema content
        let schema_content = match fs::read_to_string(&self.schema_path) {
            Ok(content) => content,
            Err(e) => {
                return RegenerateResult {
                    success: false,
                    message: format!("Failed to read schema: {}", e),
                    output_path: None,
                }
            }
        };

        // Parse the schema
        let mut parser =
            match forgedb_parser::parser::Parser::new(&schema_content) {
                Ok(p) => p,
                Err(e) => {
                    return RegenerateResult {
                        success: false,
                        message: format!("Lexer error: {}", e),
                        output_path: None,
                    }
                }
            };

        let schema = match parser.parse() {
            Ok(s) => s,
            Err(e) => {
                return RegenerateResult {
                    success: false,
                    message: format!("Parser error: {}", e),
                    output_path: None,
                }
            }
        };

        // Create output directory
        if let Err(e) = fs::create_dir_all(&self.output_dir) {
            return RegenerateResult {
                success: false,
                message: format!("Failed to create output directory: {}", e),
                output_path: None,
            };
        }

        match self.regenerate_internal(&schema) {
            Ok(()) => RegenerateResult {
                success: true,
                message: "Code regenerated successfully".to_string(),
                output_path: Some(self.output_dir.clone()),
            },
            Err(e) => RegenerateResult {
                success: false,
                message: format!("Generation failed: {}", e),
                output_path: None,
            },
        }
    }

    /// Run all generators and write output files.
    ///
    /// Output layout mirrors the CLI `generate all` command:
    /// - `{output_dir}/database.rs` — Rust database implementation
    /// - `{output_dir}/types.ts`    — TypeScript types and SDK
    /// - `{output_dir}/api.rs`      — REST API implementation
    /// - `{output_dir}/stubs/README.md` — Component stubs index
    fn regenerate_internal(&self, schema: &Schema) -> Result<(), RegenerateError> {
        // Rust database code
        let rust_result = RustGenerator::generate(schema)
            .map_err(|e| RegenerateError::GenerationError(e.to_string()))?;
        fs::write(self.output_dir.join("database.rs"), &rust_result.code)?;

        // TypeScript types and SDK
        let ts_result = TypeScriptGenerator::generate(schema)
            .map_err(|e| RegenerateError::GenerationError(e.to_string()))?;
        fs::write(self.output_dir.join("types.ts"), &ts_result.code)?;

        // REST API implementation
        let api_result = ApiGenerator::generate(schema)
            .map_err(|e| RegenerateError::GenerationError(e.to_string()))?;
        fs::write(self.output_dir.join("api.rs"), &api_result.code)?;

        // Stubs index
        let stub_result = StubGenerator::generate(schema)
            .map_err(|e| RegenerateError::GenerationError(e.to_string()))?;
        let stubs_dir = self.output_dir.join("stubs");
        fs::create_dir_all(&stubs_dir)?;
        fs::write(stubs_dir.join("README.md"), &stub_result.code)?;

        Ok(())
    }

    /// Get the schema file path
    pub fn schema_path(&self) -> &Path {
        &self.schema_path
    }

    /// Get the output directory path
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }
}

/// Callback type for regeneration events
pub type RegenerateCallback = Box<dyn Fn(&RegenerateResult) + Send>;
