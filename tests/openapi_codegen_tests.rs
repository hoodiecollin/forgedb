use forgedb::openapi_codegen::OpenApiGenerator;
use forgedb::ast::{Field, FieldType, IndexType, Model, Schema};
use serde_json::json;

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
        ],
        composite_indexes: vec![],
        soft_delete: false,
    }
}

#[test]
fn test_openapi_generation() {
    let schema = Schema {
        structs: vec![],
        models: vec![create_test_model()],
    };

    let files = OpenApiGenerator::generate(&schema);
    assert_eq!(files.len(), 2); // openapi.json and API.md

    // Check OpenAPI file
    let openapi_file = files
        .iter()
        .find(|f| f.path.contains("openapi.json"))
        .unwrap();
    assert!(openapi_file.content.contains("openapi"));
    assert!(openapi_file.content.contains("User"));

    // Check markdown file
    let md_file = files.iter().find(|f| f.path.contains("API.md")).unwrap();
    assert!(md_file.content.contains("# API Documentation"));
    assert!(md_file.content.contains("## User"));
}

#[test]
fn test_type_conversion() {
    assert_eq!(
        OpenApiGenerator::type_to_openapi_type(&FieldType::String),
        json!({ "type": "string" })
    );
    assert_eq!(
        OpenApiGenerator::type_to_openapi_type(&FieldType::U32),
        json!({ "type": "integer" })
    );
    assert_eq!(
        OpenApiGenerator::type_to_openapi_type(&FieldType::Bool),
        json!({ "type": "boolean" })
    );
}
