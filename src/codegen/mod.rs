pub mod computed;
pub mod crud;
pub mod generator;
pub mod model_gen;
pub mod output;
pub mod query;
pub mod relations;
pub mod storage_gen;
pub mod utils;
pub mod validation_gen;

pub use generator::CodeGenerator;

/// Represents a generated file
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
}
