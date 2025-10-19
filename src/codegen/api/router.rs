//! API router generation (string-based for now)
//!
//! Generates router setup with all CRUD endpoints.

use crate::ast::Schema;
use crate::codegen::{naming, GeneratedFile};

/// Generate router setup
pub fn generate_router(schema: &Schema) -> GeneratedFile {
    let mut code = String::new();

    // Imports
    code.push_str("use axum::{\n");
    code.push_str("    routing::{delete, get, post, put},\n");
    code.push_str("    Router,\n");
    code.push_str("};\n\n");

    // Import all handlers
    for model in &schema.models {
        let model_lower = model.name.to_lowercase();
        code.push_str(&format!("use super::{}_handlers;\n", model_lower));
    }
    code.push_str("\n");

    // Router creation function
    code.push_str("/// Create the API router with all endpoints\n");
    code.push_str("pub fn create_router() -> Router {\n");
    code.push_str("    Router::new()\n");

    // Add routes for each model
    for model in &schema.models {
        let model_lower = model.name.to_lowercase();
        let plural = naming::pluralize(&model_lower);

        code.push_str(&format!(
            "        .route(\"/api/{}\", get({}_handlers::list_{}))\n",
            plural, model_lower, model_lower
        ));
        code.push_str(&format!(
            "        .route(\"/api/{}\", post({}_handlers::create_{}))\n",
            plural, model_lower, model_lower
        ));
        code.push_str(&format!(
            "        .route(\"/api/{}/:id\", get({}_handlers::get_{}))\n",
            plural, model_lower, model_lower
        ));
        code.push_str(&format!(
            "        .route(\"/api/{}/:id\", put({}_handlers::update_{}))\n",
            plural, model_lower, model_lower
        ));
        code.push_str(&format!(
            "        .route(\"/api/{}/:id\", delete({}_handlers::delete_{}))\n",
            plural, model_lower, model_lower
        ));
    }

    code.push_str("}\n");

    GeneratedFile {
        path: "generated/api/router.rs".to_string(),
        content: code,
    }
}
