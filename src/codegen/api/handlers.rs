//! API handlers generation (string-based for now)
//!
//! Generates handler functions for CRUD operations.

use crate::ast::Model;
use crate::codegen::GeneratedFile;

/// Generate handler functions for a model
pub fn generate_handlers(model: &Model) -> GeneratedFile {
    let model_lower = model.name.to_lowercase();
    let mut code = String::new();

    // Imports
    code.push_str("use axum::{\n");
    code.push_str("    extract::{Path, Query, State},\n");
    code.push_str("    http::StatusCode,\n");
    code.push_str("    response::{IntoResponse, Json},\n");
    code.push_str("};\n");
    code.push_str("use serde_json::json;\n");
    code.push_str("use std::sync::Arc;\n");
    code.push_str("use uuid::Uuid;\n\n");
    code.push_str(&format!("use super::{}_types::*;\n", model_lower));
    code.push_str("use forgedb_query_params::QueryParams;\n\n");

    // List handler
    code.push_str(&format!("/// List all {}\n", model.name));
    code.push_str(&format!("pub async fn list_{}(\n", model_lower));
    code.push_str("    Query(params): Query<QueryParams>,\n");
    code.push_str(") -> impl IntoResponse {\n");
    code.push_str("    // TODO: Implement list logic with storage\n");
    code.push_str("    // Apply filters from params.filters\n");
    code.push_str("    // Apply sort from params.sort\n");
    code.push_str("    // Apply pagination from params.pagination\n");
    code.push_str("    Json(json!({\n");
    code.push_str(&format!("        \"data\": [],\n"));
    code.push_str("        \"count\": 0\n");
    code.push_str("    }))\n");
    code.push_str("}\n\n");

    // Get by ID handler
    code.push_str(&format!("/// Get {} by ID\n", model.name));
    code.push_str(&format!("pub async fn get_{}(\n", model_lower));
    code.push_str("    Path(id): Path<Uuid>,\n");
    code.push_str(") -> impl IntoResponse {\n");
    code.push_str("    // TODO: Implement get logic with storage\n");
    code.push_str("    (StatusCode::NOT_FOUND, Json(json!({\n");
    code.push_str("        \"error\": \"Not found\"\n");
    code.push_str("    })))\n");
    code.push_str("}\n\n");

    // Create handler
    code.push_str(&format!("/// Create a new {}\n", model.name));
    code.push_str(&format!("pub async fn create_{}(\n", model_lower));
    code.push_str(&format!(
        "    Json(req): Json<Create{}Request>,\n",
        model.name
    ));
    code.push_str(") -> impl IntoResponse {\n");
    code.push_str("    // TODO: Implement create logic with storage\n");
    code.push_str("    // Validate request with forgedb_validation\n");
    code.push_str("    // Call storage.insert()\n");
    code.push_str("    (StatusCode::CREATED, Json(json!({\n");
    code.push_str("        \"id\": Uuid::new_v4()\n");
    code.push_str("    })))\n");
    code.push_str("}\n\n");

    // Update handler
    code.push_str(&format!("/// Update an existing {}\n", model.name));
    code.push_str(&format!("pub async fn update_{}(\n", model_lower));
    code.push_str("    Path(id): Path<Uuid>,\n");
    code.push_str(&format!(
        "    Json(req): Json<Update{}Request>,\n",
        model.name
    ));
    code.push_str(") -> impl IntoResponse {\n");
    code.push_str("    // TODO: Implement update logic with storage\n");
    code.push_str("    (StatusCode::OK, Json(json!({\n");
    code.push_str("        \"id\": id\n");
    code.push_str("    })))\n");
    code.push_str("}\n\n");

    // Delete handler
    code.push_str(&format!("/// Delete a {}\n", model.name));
    code.push_str(&format!("pub async fn delete_{}(\n", model_lower));
    code.push_str("    Path(id): Path<Uuid>,\n");
    code.push_str(") -> impl IntoResponse {\n");
    code.push_str("    // TODO: Implement delete logic with storage\n");
    code.push_str("    StatusCode::NO_CONTENT\n");
    code.push_str("}\n");

    GeneratedFile {
        path: format!("generated/api/{}_handlers.rs", model_lower),
        content: code,
    }
}
