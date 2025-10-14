use thiserror::Error;

pub type Result<T> = std::result::Result<T, CliError>;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Schema validation error: {0}")]
    SchemaValidation(String),

    #[error("Code generation error: {0}")]
    CodeGeneration(String),

    #[error("Build error: {0}")]
    Build(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Project already exists: {0}")]
    ProjectExists(String),

    #[error("Schema file not found: {0}")]
    SchemaNotFound(String),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Compaction error: {0}")]
    Compaction(String),

    #[error("{0}")]
    Other(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Io(_) => 1,
            CliError::Anyhow(_) => 1,
            CliError::SchemaValidation(_) => 2,
            CliError::CodeGeneration(_) => 3,
            CliError::Build(_) => 4,
            CliError::Config(_) => 10,
            CliError::ProjectExists(_) => 11,
            CliError::SchemaNotFound(_) => 11,
            CliError::Migration(_) => 5,
            CliError::Compaction(_) => 6,
            CliError::Other(_) => 1,
        }
    }
}
