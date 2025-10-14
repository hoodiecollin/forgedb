//! API Code Generation for Sprint 9
//!
//! Generates REST API code from schema definitions, including:
//! - Handler functions for CRUD operations
//! - Request/Response types with serde
//! - Router setup with Axum
//! - Query parameter integration
//! - Validation logic

use crate::ast::{Field, FieldType, Model, Schema};
use crate::codegen::GeneratedFile;

pub struct ApiCodeGenerator;

impl ApiCodeGenerator {
    /// Generate all API files for a schema
    pub fn generate(schema: &Schema) -> Vec<GeneratedFile> {
        let mut files = vec![];

        // Generate request/response types for each model
        for model in &schema.models {
            files.push(Self::generate_api_types(model));
            files.push(Self::generate_handlers(model));
        }

        // Generate main router
        files.push(Self::generate_router(schema));

        // Generate main API module
        files.push(Self::generate_api_mod(schema));

        files
    }

    /// Generate request/response types for a model
    fn generate_api_types(model: &Model) -> GeneratedFile {
        let model_lower = model.name.to_lowercase();
        let mut code = String::new();

        // Imports
        code.push_str("use serde::{Deserialize, Serialize};\n");
        code.push_str("use uuid::Uuid;\n\n");

        // CreateRequest type (fields without auto-generated or computed ones)
        code.push_str(&format!("#[derive(Debug, Deserialize)]\n"));
        code.push_str(&format!("pub struct Create{}Request {{\n", model.name));
        for field in &model.fields {
            if !field.auto_generate && !Self::is_virtual_field(field) && !field.is_computed {
                let field_type = Self::map_field_type_to_rust(&field.field_type, false);
                code.push_str(&format!("    pub {}: {},\n", field.name, field_type));
            }
        }
        code.push_str("}\n\n");

        // UpdateRequest type (all non-auto, non-computed fields are optional)
        code.push_str(&format!("#[derive(Debug, Deserialize)]\n"));
        code.push_str(&format!("pub struct Update{}Request {{\n", model.name));
        for field in &model.fields {
            if !field.auto_generate && !Self::is_virtual_field(field) && !field.is_computed {
                let field_type = Self::map_field_type_to_rust(&field.field_type, false);
                code.push_str(&format!(
                    "    pub {}: Option<{}>,\n",
                    field.name, field_type
                ));
            }
        }
        code.push_str("}\n\n");

        // Response type (reuse the model struct with Serialize)
        code.push_str(&format!("#[derive(Debug, Serialize)]\n"));
        code.push_str(&format!("pub struct {}Response {{\n", model.name));
        for field in &model.fields {
            if !Self::is_virtual_field(field) {
                let field_type = Self::map_field_type_to_rust(&field.field_type, true);
                code.push_str(&format!("    pub {}: {},\n", field.name, field_type));
            }
        }

        // Add computed fields to response (Sprint 12)
        let computed_fields: Vec<_> = model.fields.iter().filter(|f| f.is_computed).collect();
        if !computed_fields.is_empty() {
            code.push_str("\n    // Computed fields\n");
            for field in computed_fields {
                let field_type = Self::map_field_type_to_rust(&field.field_type, true);
                code.push_str(&format!("    pub {}: {},\n", field.name, field_type));
            }
        }

        code.push_str("}\n");

        GeneratedFile {
            path: format!("generated/api/{}_types.rs", model_lower),
            content: code,
        }
    }

    /// Generate handler functions for a model
    fn generate_handlers(model: &Model) -> GeneratedFile {
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
        code.push_str("use sinkdb_query_params::QueryParams;\n\n");

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
        code.push_str("    // Validate request with sinkdb_validation\n");
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

    /// Generate router setup
    fn generate_router(schema: &Schema) -> GeneratedFile {
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
            let plural = format!("{}s", model_lower); // Simple pluralization

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

    /// Check if field is virtual (relation that doesn't store data)
    fn is_virtual_field(field: &Field) -> bool {
        matches!(
            &field.field_type,
            FieldType::Relation(crate::ast::RelationType::OneToMany(_))
                | FieldType::Relation(crate::ast::RelationType::ManyToMany(_))
        )
    }

    /// Map FieldType to Rust type for API
    fn map_field_type_to_rust(field_type: &FieldType, for_response: bool) -> String {
        match field_type {
            FieldType::U32 => "u32".to_string(),
            FieldType::U64 => "u64".to_string(),
            FieldType::I32 => "i32".to_string(),
            FieldType::I64 => "i64".to_string(),
            FieldType::F64 => "f64".to_string(),
            FieldType::Bool => "bool".to_string(),
            FieldType::String => "String".to_string(),
            FieldType::Uuid => "Uuid".to_string(),
            FieldType::Timestamp => "i64".to_string(), // Unix timestamp
            FieldType::Char(size) => format!("[u8; {}]", size),
            FieldType::FixedArray(inner, count) => {
                format!(
                    "[{}; {}]",
                    Self::map_field_type_to_rust(inner, for_response),
                    count
                )
            }
            FieldType::StructType(name) => name.clone(),
            FieldType::OptionalStructType(name) => format!("Option<{}>", name),
            FieldType::Relation(rel_type) => {
                use crate::ast::RelationType;
                match rel_type {
                    RelationType::RequiredReference(_) => "Uuid".to_string(), // FK stored as UUID
                    RelationType::OptionalReference(_) => "Option<Uuid>".to_string(), // Optional FK
                    _ => "()".to_string(),                                    // Virtual fields
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Field, IndexType, Model, Schema};

    fn create_test_model() -> Model {
        Model {
            name: "User".to_string(),
            fields: vec![
                Field {
                    name: "id".to_string(),
                    field_type: FieldType::Uuid,
                    unique: false,
                    indexed: false,
                    auto_generate: true,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "email".to_string(),
                    field_type: FieldType::String,
                    unique: true,
                    indexed: true,
                    auto_generate: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    unique: false,
                    indexed: false,
                    auto_generate: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            soft_delete: false,
        }
    }

    #[test]
    fn test_generate_api_types() {
        let model = create_test_model();
        let file = ApiCodeGenerator::generate_api_types(&model);

        assert_eq!(file.path, "generated/api/user_types.rs");
        assert!(file.content.contains("CreateUserRequest"));
        assert!(file.content.contains("UpdateUserRequest"));
        assert!(file.content.contains("UserResponse"));
        assert!(file.content.contains("pub email: String"));
        // CreateRequest shouldn't have auto-generated fields
        assert!(!file.content.contains("CreateUserRequest {\n    pub id:"));
        // But UserResponse should have all fields including id
        assert!(file.content.contains("pub id: Uuid"));
    }

    #[test]
    fn test_generate_handlers() {
        let model = create_test_model();
        let file = ApiCodeGenerator::generate_handlers(&model);

        assert_eq!(file.path, "generated/api/user_handlers.rs");
        assert!(file.content.contains("pub async fn list_user"));
        assert!(file.content.contains("pub async fn get_user"));
        assert!(file.content.contains("pub async fn create_user"));
        assert!(file.content.contains("pub async fn update_user"));
        assert!(file.content.contains("pub async fn delete_user"));
    }

    #[test]
    fn test_generate_router() {
        let schema = Schema {
            structs: vec![],
            models: vec![create_test_model()],
        };
        let file = ApiCodeGenerator::generate_router(&schema);

        assert_eq!(file.path, "generated/api/router.rs");
        assert!(file.content.contains("pub fn create_router"));
        assert!(file.content.contains("/api/users"));
        assert!(file.content.contains("list_user"));
        assert!(file.content.contains("get_user"));
    }

    #[test]
    fn test_generate_api_mod() {
        let schema = Schema {
            structs: vec![],
            models: vec![create_test_model()],
        };
        let file = ApiCodeGenerator::generate_api_mod(&schema);

        assert_eq!(file.path, "generated/api/mod.rs");
        assert!(file.content.contains("pub mod user_types"));
        assert!(file.content.contains("pub mod user_handlers"));
        assert!(file.content.contains("pub mod router"));
        assert!(file.content.contains("pub use router::create_router"));
    }

    #[test]
    fn test_map_field_type_to_rust() {
        assert_eq!(
            ApiCodeGenerator::map_field_type_to_rust(&FieldType::U32, false),
            "u32"
        );
        assert_eq!(
            ApiCodeGenerator::map_field_type_to_rust(&FieldType::String, false),
            "String"
        );
        assert_eq!(
            ApiCodeGenerator::map_field_type_to_rust(&FieldType::Uuid, false),
            "Uuid"
        );
        assert_eq!(
            ApiCodeGenerator::map_field_type_to_rust(
                &FieldType::OptionalStructType("Address".to_string()),
                false
            ),
            "Option<Address>"
        );
        assert_eq!(
            ApiCodeGenerator::map_field_type_to_rust(&FieldType::Char(50), false),
            "[u8; 50]"
        );
    }
}
