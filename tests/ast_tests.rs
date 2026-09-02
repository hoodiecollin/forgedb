use forgedb_parser::ast::*;

#[test]
fn test_detect_many_to_many() {
    let schema = Schema {
        structs: vec![],
        enums: vec![],
        models: vec![
            Model { position: None,
                name: "Post".to_string(),
                fields: vec![
                    Field { position: None,
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
                    Field { position: None,
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
                projections: Vec::new(),
                soft_delete: false,
            },
            Model { position: None,
                name: "Tag".to_string(),
                fields: vec![
                    Field { position: None,
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
                    Field { position: None,
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
                projections: Vec::new(),
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
    let schema = Schema {
        structs: vec![],
        enums: vec![],
        models: vec![
            Model { position: None,
                name: "User".to_string(),
                fields: vec![
                    Field { position: None,
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
                    Field { position: None,
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
                projections: Vec::new(),
                soft_delete: false,
            },
            Model { position: None,
                name: "Post".to_string(),
                fields: vec![
                    Field { position: None,
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
                    Field { position: None,
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
                projections: Vec::new(),
                soft_delete: false,
            },
        ],
    };

    let m2m = schema.detect_many_to_many_relations();
    assert_eq!(m2m.len(), 0);
}
