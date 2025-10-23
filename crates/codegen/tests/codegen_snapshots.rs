//! Snapshot tests for code generation
//!
//! Uses insta for snapshot testing to ensure generated code remains stable.

use forgedb_codegen::{ApiGenerator, RustGenerator};
use forgedb_parser::ast::IndexType;
use forgedb_parser::{Field, FieldType, Model, Schema};

/// Helper to create a simple test schema with one model
fn simple_user_schema() -> Schema {
    Schema {
        models: vec![Model {
            name: "User".to_string(),
            fields: vec![
                Field {
                    name: "id".to_string(),
                    field_type: FieldType::Uuid,
                    auto_generate: true,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "email".to_string(),
                    field_type: FieldType::String,
                    auto_generate: false,
                    unique: true,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "age".to_string(),
                    field_type: FieldType::OptionalStructType("u32".to_string()),
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            soft_delete: false,
        }],
        structs: vec![],
    }
}

/// Helper to create a schema with multiple models
fn multi_model_schema() -> Schema {
    Schema {
        models: vec![
            Model {
                name: "User".to_string(),
                fields: vec![Field {
                    name: "id".to_string(),
                    field_type: FieldType::Uuid,
                    auto_generate: true,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                }],
                composite_indexes: vec![],
                soft_delete: false,
            },
            Model {
                name: "Post".to_string(),
                fields: vec![
                    Field {
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field {
                        name: "title".to_string(),
                        field_type: FieldType::String,
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                soft_delete: false,
            },
        ],
        structs: vec![],
    }
}

#[test]
fn test_rust_generation_simple_model() {
    let schema = simple_user_schema();
    let result = RustGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_rust_generation_multiple_models() {
    let schema = multi_model_schema();
    let result = RustGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_rust_generation_has_utoipa_derives() {
    let schema = simple_user_schema();
    let result = RustGenerator::generate(&schema).unwrap();

    // Verify utoipa imports and derives are present (formatted output)
    assert!(result.code.contains("use utoipa::ToSchema"));
    assert!(result.code.contains("use serde::{Deserialize, Serialize}"));
    assert!(result.code.contains("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]"));
}

#[test]
fn test_api_generation_simple_model() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_api_generation_multiple_models() {
    let schema = multi_model_schema();
    let result = ApiGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}

#[test]
fn test_api_generation_has_utoipa_attributes() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

    // Verify utoipa imports are present (formatted output)
    assert!(result.code.contains("use utoipa::OpenApi"));

    // Verify utoipa path attributes are present
    assert!(result.code.contains("#[utoipa::path"));

    // Verify OpenAPI derive is present
    assert!(result.code.contains("#[derive(OpenApi)]"));
    assert!(result.code.contains("#[openapi"));

    // Verify openapi_json function exists
    assert!(result.code.contains("pub fn openapi_json"));
}

#[test]
fn test_api_generation_has_all_crud_operations() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

    // Verify all CRUD handlers are generated
    assert!(result.code.contains("async fn list_user"));
    assert!(result.code.contains("async fn get_user"));
    assert!(result.code.contains("async fn create_user"));

    // Verify router function exists
    assert!(result.code.contains("pub fn create_router"));
}

#[test]
fn test_api_openapi_doc_structure() {
    let schema = simple_user_schema();
    let result = ApiGenerator::generate(&schema).unwrap();

    // Verify OpenAPI doc struct has correct structure (formatted output)
    assert!(result.code.contains("pub struct ApiDoc"));
    assert!(result.code.contains("paths("));
    assert!(result.code.contains("components("));
    assert!(result.code.contains("schemas("));
    assert!(result.code.contains("tags("));
}

#[test]
fn test_different_field_types() {
    let schema = Schema {
        models: vec![Model {
            name: "ComplexModel".to_string(),
            fields: vec![
                Field {
                    name: "id".to_string(),
                    field_type: FieldType::Uuid,
                    auto_generate: true,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "count".to_string(),
                    field_type: FieldType::I64,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "price".to_string(),
                    field_type: FieldType::F64,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "active".to_string(),
                    field_type: FieldType::Bool,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "created_at".to_string(),
                    field_type: FieldType::Timestamp,
                    auto_generate: false,
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
            ],
            composite_indexes: vec![],
            soft_delete: false,
        }],
        structs: vec![],
    };

    let result = RustGenerator::generate(&schema).unwrap();
    insta::assert_snapshot!(result.code);
}
