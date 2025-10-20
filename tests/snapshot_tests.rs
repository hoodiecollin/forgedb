use forgedb::api_codegen::ApiCodeGenerator;
use forgedb::ast::{Field, FieldType, IndexType, Model, RelationType, Schema};
use forgedb::codegen::openapi::OpenApiGenerator;
use forgedb::typescript_codegen::TypeScriptGenerator;

fn create_test_schema() -> Schema {
    Schema {
        structs: vec![],
        models: vec![
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
                    Field {
                        name: "posts".to_string(),
                        field_type: FieldType::Relation(RelationType::OneToMany("Post".to_string())),
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
            },
            Model {
                name: "Post".to_string(),
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
                        name: "title".to_string(),
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
                    Field {
                        name: "content".to_string(),
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
                    Field {
                        name: "author_id".to_string(),
                        field_type: FieldType::Relation(RelationType::RequiredReference(
                            "User".to_string(),
                        )),
                        unique: false,
                        indexed: true,
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
            },
        ],
    }
}

#[test]
fn test_api_types_snapshot() {
    let schema = create_test_schema();
    let user_model = &schema.models[0];
    let file = ApiCodeGenerator::generate_api_types(user_model);
    
    insta::assert_snapshot!("api_types_user", file.content);
}

#[test]
fn test_api_handlers_snapshot() {
    let schema = create_test_schema();
    let user_model = &schema.models[0];
    let file = ApiCodeGenerator::generate_handlers(user_model);
    
    insta::assert_snapshot!("api_handlers_user", file.content);
}

#[test]
fn test_api_router_snapshot() {
    let schema = create_test_schema();
    let file = ApiCodeGenerator::generate_router(&schema);
    
    insta::assert_snapshot!("api_router", file.content);
}

#[test]
fn test_typescript_types_snapshot() {
    let schema = create_test_schema();
    let file = TypeScriptGenerator::generate_types(&schema);
    
    insta::assert_snapshot!("typescript_types", file.content);
}

#[test]
fn test_typescript_client_snapshot() {
    let schema = create_test_schema();
    let user_model = &schema.models[0];
    let file = TypeScriptGenerator::generate_api_client(user_model, &schema);
    
    insta::assert_snapshot!("typescript_client_user", file.content);
}

#[test]
fn test_openapi_spec_snapshot() {
    let schema = create_test_schema();
    let files = OpenApiGenerator::generate(&schema);
    let file = files.iter().find(|f| f.path.ends_with("openapi.json")).unwrap();
    
    // Parse and pretty-print JSON for stable formatting
    let json_value: serde_json::Value = serde_json::from_str(&file.content).unwrap();
    let pretty_json = serde_json::to_string_pretty(&json_value).unwrap();
    
    insta::assert_snapshot!("openapi_spec", pretty_json);
}
