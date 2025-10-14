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

    /// Regenerate code from the schema file
    ///
    /// This function reads the schema, parses it, generates code, and writes
    /// the output. It returns detailed information about success or failure.
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

        // Create output directory
        if let Err(e) = fs::create_dir_all(&self.output_dir) {
            return RegenerateResult {
                success: false,
                message: format!("Failed to create output directory: {}", e),
                output_path: None,
            };
        }

        // Call the code generation (this would normally use the sinkdb library)
        // For now, we'll invoke it as a command since we're in a separate crate
        let output_path = self.output_dir.join("database.rs");

        // Here we use the internal regeneration logic
        // In production, this would call the parser and codegen directly
        match self.regenerate_internal(&schema_content) {
            Ok(code) => {
                match fs::write(&output_path, code) {
                    Ok(_) => RegenerateResult {
                        success: true,
                        message: format!("✓ Code regenerated successfully"),
                        output_path: Some(output_path),
                    },
                    Err(e) => RegenerateResult {
                        success: false,
                        message: format!("Failed to write generated code: {}", e),
                        output_path: None,
                    }
                }
            }
            Err(e) => RegenerateResult {
                success: false,
                message: format!("Generation failed: {}", e),
                output_path: None,
            }
        }
    }

    /// Internal regeneration logic
    fn regenerate_internal(&self, schema_content: &str) -> Result<String, RegenerateError> {
        // Parse the schema using sinkdb parser
        let mut parser = sinkdb::parser::Parser::new(schema_content)
            .map_err(|e| RegenerateError::ParseError(format!("Lexer error: {}", e)))?;

        let schema = parser.parse()
            .map_err(|e| RegenerateError::ParseError(format!("Parser error: {}", e)))?;

        // Generate code using sinkdb codegen
        let generator = sinkdb::codegen::CodeGenerator::new();
        let generated_code = generator.generate(&schema);

        Ok(generated_code)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regenerator_creation() {
        let regen = SchemaRegenerator::new("schema.sink", "generated");
        assert_eq!(regen.schema_path(), Path::new("schema.sink"));
        assert_eq!(regen.output_dir(), Path::new("generated"));
    }

    #[test]
    fn test_regenerate_missing_file() {
        let regen = SchemaRegenerator::new("/nonexistent/schema.sink", "generated");
        let result = regen.regenerate();
        assert!(!result.success);
        assert!(result.message.contains("not found"));
    }
}
