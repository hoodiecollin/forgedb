//! API code generation module
//!
//! Generates REST API code from schema definitions, including:
//! - Handler functions for CRUD operations
//! - Request/Response types with serde
//! - Router setup with Axum

pub mod handlers;
pub mod router;
pub mod types;

use crate::ast::{Model, Schema};
use crate::codegen::GeneratedFile;

pub struct ApiCodeGenerator;

impl ApiCodeGenerator {
    /// Generate all API files for a schema
    pub fn generate(schema: &Schema) -> Vec<GeneratedFile> {
        let mut files = vec![];

        // Generate request/response types for each model
        for model in &schema.models {
            files.push(types::generate_api_types(model));
            files.push(handlers::generate_handlers(model));
        }

        // Generate main router
        files.push(router::generate_router(schema));

        // Generate main API module
        files.push(Self::generate_api_mod(schema));

        files
    }

    /// Generate API module file
    fn generate_api_mod(schema: &Schema) -> GeneratedFile {
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
}
