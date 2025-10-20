//! API Code Generation for Sprint 9
//!
//! Generates REST API code from schema definitions, including:
//! - Handler functions for CRUD operations
//! - Request/Response types with serde
//! - Router setup with Axum
//! - Query parameter integration
//! - Validation logic
//!
//! NOTE: This module now delegates to codegen::api for the actual implementation.
//! It maintains backward compatibility with existing tests and code.

use crate::ast::{Field, FieldType, Model, Schema};
use crate::codegen::GeneratedFile;

pub struct ApiCodeGenerator;

impl ApiCodeGenerator {
    /// Generate all API files for a schema
    pub fn generate(schema: &Schema) -> Vec<GeneratedFile> {
        crate::codegen::api::ApiCodeGenerator::generate(schema)
    }

    /// Generate request/response types for a model
    pub fn generate_api_types(model: &Model) -> GeneratedFile {
        crate::codegen::api::types::generate_api_types(model)
    }

    /// Generate handler functions for a model
    pub fn generate_handlers(model: &Model) -> GeneratedFile {
        crate::codegen::api::handlers::generate_handlers(model)
    }

    /// Generate router setup
    pub fn generate_router(schema: &Schema) -> GeneratedFile {
        crate::codegen::api::router::generate_router(schema)
    }

    /// Generate API module file
    pub fn generate_api_mod(schema: &Schema) -> GeneratedFile {
        let mut code = String::new();

        code.push_str("//! Auto-generated API module\n\n");

        // Declare submodules
        for model in &schema.models {
            let model_lower = model.name.to_lowercase();
            code.push_str(&format!("pub mod {}_types;\n", model_lower));
            code.push_str(&format!("pub mod {}_handlers;\n", model_lower));
        }
        code.push_str("pub mod router;\n\n");

        // Re-export router
        code.push_str("pub use router::create_router;\n");

        GeneratedFile {
            path: "generated/api/mod.rs".to_string(),
            content: code,
        }
    }

    /// Check if field is virtual (relation or component that doesn't store data)
    pub fn is_virtual_field(field: &Field) -> bool {
        crate::codegen::semantics::is_virtual_field(field)
    }

    /// Map FieldType to Rust type for API
    pub fn map_field_type_to_rust(field_type: &FieldType, for_response: bool) -> String {
        let tokens = crate::codegen::semantics::map_field_type_to_rust_tokens(field_type, for_response);
        // Convert tokens to string - spaces are kept around ; in arrays
        tokens.to_string().replace(" ", "").replace(";", "; ")
    }
}
