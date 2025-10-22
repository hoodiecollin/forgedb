//! REST API server code generator

use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;

/// API code generator
pub struct ApiGenerator;

impl ApiGenerator {
    /// Generate REST API server implementation from schema
    ///
    /// # Arguments
    ///
    /// * `schema` - Parsed schema AST
    ///
    /// # Returns
    ///
    /// Generated API code as a string
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let code = Self::generate_code(schema)?;

        Ok(GeneratedCode {
            code,
            description: format!("REST API server ({} models)", schema.models.len()),
        })
    }

    /// Generate API server code as a string
    fn generate_code(schema: &Schema) -> Result<String> {
        let mut code = String::new();

        // File header
        code.push_str("//! Generated API server by ForgeDB\n");
        code.push_str("//! DO NOT EDIT - This file is auto-generated\n\n");

        // Imports
        code.push_str("use axum::{\n");
        code.push_str("    extract::{Path, Query, State},\n");
        code.push_str("    http::StatusCode,\n");
        code.push_str("    response::Json,\n");
        code.push_str("    routing::{get, post, put, delete},\n");
        code.push_str("    Router,\n");
        code.push_str("};\n");
        code.push_str("use serde::{Deserialize, Serialize};\n");
        code.push_str("use std::sync::Arc;\n");
        code.push_str("use tokio::sync::RwLock;\n\n");

        // Generate router function
        code.push_str("pub fn create_router(db: Arc<RwLock<super::Database>>) -> Router {\n");
        code.push_str("    Router::new()\n");

        for model in &schema.models {
            let route_path = format!("/api/{}", Self::to_kebab_case(&model.name));
            code.push_str(&format!(
                "        .route(\"{}\", get(list_{}))\n",
                route_path,
                Self::to_snake_case(&model.name)
            ));
            code.push_str(&format!(
                "        .route(\"{}\", post(create_{}))\n",
                route_path,
                Self::to_snake_case(&model.name)
            ));
            code.push_str(&format!(
                "        .route(\"{}/:id\", get(get_{}))\n",
                route_path,
                Self::to_snake_case(&model.name)
            ));
        }

        code.push_str("        .with_state(db)\n");
        code.push_str("}\n\n");

        // Generate handler functions for each model
        for model in &schema.models {
            code.push_str(&Self::generate_handlers(model)?);
            code.push_str("\n");
        }

        Ok(code)
    }

    /// Generate handler functions for a model
    fn generate_handlers(model: &forgedb_parser::Model) -> Result<String> {
        let mut code = String::new();
        let model_snake = Self::to_snake_case(&model.name);

        // List handler
        code.push_str(&format!(
            "async fn list_{}(State(db): State<Arc<RwLock<super::Database>>>) -> Json<serde_json::Value> {{\n",
            model_snake
        ));
        code.push_str("    // TODO: Implement list\n");
        code.push_str("    Json(serde_json::json!({ \"data\": [] }))\n");
        code.push_str("}\n\n");

        // Get handler
        code.push_str(&format!(
            "async fn get_{}(Path(id): Path<String>, State(db): State<Arc<RwLock<super::Database>>>) -> Json<serde_json::Value> {{\n",
            model_snake
        ));
        code.push_str("    // TODO: Implement get\n");
        code.push_str("    Json(serde_json::json!({ \"data\": null }))\n");
        code.push_str("}\n\n");

        // Create handler
        code.push_str(&format!(
            "async fn create_{}(State(db): State<Arc<RwLock<super::Database>>>, Json(payload): Json<serde_json::Value>) -> Json<serde_json::Value> {{\n",
            model_snake
        ));
        code.push_str("    // TODO: Implement create\n");
        code.push_str("    Json(serde_json::json!({ \"data\": null }))\n");
        code.push_str("}\n");

        Ok(code)
    }

    /// Convert PascalCase to snake_case
    fn to_snake_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        }
        result
    }

    /// Convert PascalCase to kebab-case
    fn to_kebab_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('-');
            }
            result.push(c.to_ascii_lowercase());
        }
        result
    }
}
