//! OpenAPI 3.0 Specification Generation
//!
//! Generates OpenAPI/Swagger documentation from schema definitions, including:
//! - Complete API specification in OpenAPI 3.0 format
//! - Request/Response schemas with validation
//! - CRUD endpoint definitions
//! - Markdown documentation

pub mod markdown;
pub mod spec;

use crate::ast::Schema;
use crate::codegen::GeneratedFile;

pub struct OpenApiGenerator;

impl OpenApiGenerator {
    /// Generate OpenAPI specification file
    pub fn generate(schema: &Schema) -> Vec<GeneratedFile> {
        let mut files = vec![];

        // Generate OpenAPI spec (JSON)
        files.push(spec::generate_openapi_spec(schema));

        // Generate markdown documentation
        files.push(markdown::generate_markdown_docs(schema));

        files
    }
}
