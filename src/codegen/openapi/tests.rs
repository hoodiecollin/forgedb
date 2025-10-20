//! Tests for OpenAPI specification generation

#[cfg(test)]
mod tests {
    use crate::ast::{Field, FieldType, IndexType, Model, Schema};
    use crate::codegen::openapi::spec::generate_openapi_spec;
    use insta::assert_snapshot;

    fn create_test_field(name: &str, field_type: FieldType, auto_generate: bool) -> Field {
        Field {
            name: name.to_string(),
            field_type,
            auto_generate,
            unique: false,
            indexed: false,
            constraints: vec![],
            index_type: IndexType::Hash,
            is_computed: false,
            fulltext_indexed: false,
            is_materialized: false,
        }
    }

    #[test]
    fn test_openapi_spec_basic_model() {
        let schema = Schema {
            structs: vec![],
            models: vec![Model {
                name: "User".to_string(),
                fields: vec![
                    create_test_field("id", FieldType::Uuid, true),
                    create_test_field("name", FieldType::String, false),
                    create_test_field("email", FieldType::String, false),
                    create_test_field("age", FieldType::I32, false),
                ],
                composite_indexes: vec![],
                soft_delete: false,
            }],
        };

        let file = generate_openapi_spec(&schema);
        assert_snapshot!(file.content);
    }
}
