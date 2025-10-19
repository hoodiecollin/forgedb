use forgedb::typescript_codegen::TypeScriptGenerator;
use forgedb::ast::{Field, FieldType, IndexType, Model, Schema};

fn create_test_schema() -> Schema {
    Schema {
        structs: vec![],
        models: vec![Model {
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
        }],
    }
}

#[test]
fn test_generate_types() {
    let schema = create_test_schema();
    let file = TypeScriptGenerator::generate_types(&schema);

    assert_eq!(file.path, "generated/sdk/types.ts");
    assert!(file.content.contains("export interface User"));
    assert!(file.content.contains("export interface CreateUserRequest"));
    assert!(file.content.contains("export interface UpdateUserRequest"));
    assert!(file.content.contains("id: string"));
    assert!(file.content.contains("email: string"));
}

#[test]
fn test_generate_api_client() {
    let schema = create_test_schema();
    let model = &schema.models[0];
    let file = TypeScriptGenerator::generate_api_client(model, &schema);

    assert_eq!(file.path, "generated/sdk/UserApi.ts");
    assert!(file.content.contains("export class UserApi"));
    assert!(file.content.contains("async list("));
    assert!(file.content.contains("async get("));
    assert!(file.content.contains("async create("));
    assert!(file.content.contains("async update("));
    assert!(file.content.contains("async delete("));
}

#[test]
fn test_map_field_type_to_ts() {
    assert_eq!(
        TypeScriptGenerator::map_field_type_to_ts(&FieldType::String),
        "string"
    );
    assert_eq!(
        TypeScriptGenerator::map_field_type_to_ts(&FieldType::U32),
        "number"
    );
    assert_eq!(
        TypeScriptGenerator::map_field_type_to_ts(&FieldType::Bool),
        "boolean"
    );
    assert_eq!(
        TypeScriptGenerator::map_field_type_to_ts(&FieldType::Uuid),
        "string"
    );
    assert_eq!(
        TypeScriptGenerator::map_field_type_to_ts(&FieldType::OptionalStructType(
            "Address".to_string()
        )),
        "Address | null"
    );
}
