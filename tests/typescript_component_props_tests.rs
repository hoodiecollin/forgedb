use forgedb::typescript_component_props::ComponentPropsGenerator;
use forgedb::ast::{
    ComponentProtocol, ComponentReference, Field, FieldType, IndexType, Model, RelationInclusion,
    RelationType, Schema,
};

#[test]
fn test_generate_basic_props() {
    let schema = Schema {
        structs: vec![],
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
                    unique: false,
                    indexed: false,
                    constraints: vec![],
                    index_type: IndexType::Hash,
                    is_computed: false,
                    fulltext_indexed: false,
                    is_materialized: false,
                },
                Field {
                    name: "card".to_string(),
                    field_type: FieldType::Component(ComponentReference {
                        protocol: ComponentProtocol::Tsx,
                        path: "components/user/card".to_string(),
                        relations: RelationInclusion::None,
                    }),
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
    };

    let generator = ComponentPropsGenerator::new();
    let output = generator.generate_props_types(&schema);

    assert!(output.contains("export type UserCardProps"));
    assert!(output.contains("data: User;"));
}

#[test]
fn test_generate_props_with_relations() {
    let schema = Schema {
        structs: vec![],
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
                    name: "posts".to_string(),
                    field_type: FieldType::Relation(RelationType::OneToMany("Post".to_string())),
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
                    name: "card".to_string(),
                    field_type: FieldType::Component(ComponentReference {
                        protocol: ComponentProtocol::Tsx,
                        path: "components/user/card".to_string(),
                        relations: RelationInclusion::Specific(vec!["posts".to_string()]),
                    }),
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
    };

    let generator = ComponentPropsGenerator::new();
    let output = generator.generate_props_types(&schema);

    assert!(output.contains("export type UserCardProps"));
    assert!(output.contains("data: User;"));
    assert!(output.contains("relations?"));
    assert!(output.contains("posts?: Post[]"));
}
