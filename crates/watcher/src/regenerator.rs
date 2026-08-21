//! Schema change detection for the watch loop.
//!
//! # This module used to generate code, and that was the bug (#364, #335 §16)
//!
//! `regenerate_internal` ran four generators of its own — `RustGenerator`,
//! `TypeScriptGenerator`, `ApiGenerator`, `StubGenerator` — reachable from
//! `forgedb dev` and from nowhere else.  It called `RustGenerator::generate`,
//! which hardcodes `schema_version = 1` **and** `GenConfig::DEFAULT`, so a `dev`
//! save overwrote `database.rs` with a database that read no `forgedb.toml` at
//! all: wrong durability, wrong cascade depth, no replication broker, and an
//! open guard that refuses the very data dir the app is running against on any
//! project with a migration lineage.
//!
//! It was a *fourth* independent emission path, and parameterizing it — the fix
//! #364 originally proposed — would have grown this published crate an API that
//! duplicated the CLI's config resolution.  So the generation is **deleted**
//! instead: `forgedb dev` now routes every regeneration through
//! `commands::generate`, which is the same code path `forgedb generate` runs.
//!
//! What survives here is the half only the watcher can do: decide whether the
//! file that changed is a schema worth acting on, and report a parse failure
//! without letting a broken schema reach the generator.

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

    /// Check the schema and report whether a regeneration should follow.
    ///
    /// Reads and parses the schema file, and **writes nothing**.  Generation is
    /// the CLI's (`forgedb dev` → `commands::generate`), so that a watch-driven
    /// regeneration is the same emission a hand-run `forgedb generate` produces
    /// — see this module's header for why it used to be otherwise.
    ///
    /// The returned [`RegenerateResult`] is what the caller's
    /// [`RegenerateCallback`] receives: `success` means the schema parsed and the
    /// caller may regenerate; a failure carries the lexer/parser message, and the
    /// caller must **not** generate from a schema that did not parse.
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

        // NOT `fs::create_dir_all(&self.output_dir)`: this module writes nothing,
        // and creating the directory anyway would leave an empty `generated/`
        // behind for a project whose resolved output is somewhere else entirely
        // (a config `output`, or a `--output` flag — neither of which reaches
        // this crate).
        RegenerateResult {
            success: true,
            message: format!(
                "Schema parsed ({} models) — regenerating",
                schema.models.len()
            ),
            output_path: Some(self.output_dir.clone()),
        }
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

    /// #364 / #335 §16 — the guard for the deletion, asserted on the FILESYSTEM
    /// rather than on the source.
    ///
    /// Four generators used to run here and write `database.rs`, `types.ts`,
    /// `api.rs` and `stubs/README.md` into `output_dir`.  If any of them (or the
    /// `create_dir_all` that preceded them) comes back, the directory appears
    /// and this fails — which a grep for the generator names could not promise,
    /// since a reintroduction under a different generator would slip through.
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

    /// A schema that does not parse must report failure, so `forgedb dev`'s
    /// callback can decline to regenerate from it.  Without this the deletion
    /// above would be indistinguishable from "always say yes".
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
