use forgedb::api_codegen::ApiCodeGenerator;
use forgedb::ast::{Field, FieldType, IndexType, Model, Schema};

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
