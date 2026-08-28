use std::fs;
use std::path::{Path, PathBuf};

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

#[derive(Debug)]
pub struct RegenerateResult {
    pub success: bool,
    pub message: String,
    pub output_path: Option<PathBuf>,
}

pub struct SchemaRegenerator {
    schema_path: PathBuf,
    output_dir: PathBuf,
}

impl SchemaRegenerator {
    pub fn new<P: AsRef<Path>>(schema_path: P, output_dir: P) -> Self {
        SchemaRegenerator {
            schema_path: schema_path.as_ref().to_path_buf(),
            output_dir: output_dir.as_ref().to_path_buf(),
        }
    }

    pub fn regenerate(&self) -> RegenerateResult {
        if !self.schema_path.exists() {
            return RegenerateResult {
                success: false,
                message: format!("Schema file not found: {}", self.schema_path.display()),
                output_path: None,
            };
        }

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

        RegenerateResult {
            success: true,
            message: format!(
                "Schema parsed ({} models) — regenerating",
                schema.models.len()
            ),
            output_path: Some(self.output_dir.clone()),
        }
    }

    pub fn schema_path(&self) -> &Path {
        &self.schema_path
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }
}

pub type RegenerateCallback = Box<dyn Fn(&RegenerateResult) + Send>;

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "forgedb-watcher-{}-{}",
            std::process::id(),
            name
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_clean_check_writes_nothing_at_all() {
        let dir = scratch("writes-nothing");
        let schema = dir.join("schema.forge");
        fs::write(&schema, "User {\n  id: +uuid\n  email: string\n}\n").expect("write schema");
        let out = dir.join("generated");

        let result = SchemaRegenerator::new(schema.as_path(), out.as_path()).regenerate();

        assert!(
            result.success,
            "a schema that parses must check clean: {}",
            result.message
        );
        assert!(
            !out.exists(),
            "the watcher must not create — let alone write into — the output \
             directory; generation belongs to `commands::generate`"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_schema_that_does_not_parse_fails_the_check() {
        let dir = scratch("bad-schema");
        let schema = dir.join("schema.forge");
        fs::write(&schema, "User {\n  id: +uuid\n").expect("write schema");

        let result =
            SchemaRegenerator::new(schema.as_path(), dir.join("generated").as_path()).regenerate();

        assert!(!result.success, "an unparseable schema must not check clean");
        assert!(
            result.message.contains("error"),
            "the parser's own message is what reaches the user: {}",
            result.message
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
