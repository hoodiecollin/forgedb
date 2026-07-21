use forgedb_parser::ast::*;

#[test]
fn test_detect_many_to_many() {
    // Create a simple schema with M:N relations
    let schema = Schema {
        structs: vec![],
        models: vec![
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
                        name: "tags".to_string(),
                        field_type: FieldType::Relation(RelationType::OneToMany(
                            "Tag".to_string(),
                        )),
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
            Model {
                name: "Tag".to_string(),
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
                        field_type: FieldType::Relation(RelationType::OneToMany(
                            "Post".to_string(),
                        )),
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
    };

    let m2m = schema.detect_many_to_many_relations();
    assert_eq!(m2m.len(), 1);
    assert_eq!(m2m[0].model1, "Post");
    assert_eq!(m2m[0].field1, "tags");
    assert_eq!(m2m[0].model2, "Tag");
    assert_eq!(m2m[0].field2, "posts");
}

#[test]
fn test_no_m2m_with_fk() {
    // Schema with 1:N relationship (should not be detected as M:N)
    let schema = Schema {
        structs: vec![],
        models: vec![
            Model {
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
                        field_type: FieldType::Relation(RelationType::OneToMany(
                            "Post".to_string(),
                        )),
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
                        name: "author".to_string(),
                        field_type: FieldType::Relation(RelationType::RequiredReference(
                            "User".to_string(),
                        )),
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
    };

    // Should not detect M:N because User.posts has a corresponding FK in Post.author
    let m2m = schema.detect_many_to_many_relations();
    assert_eq!(m2m.len(), 0);
}
