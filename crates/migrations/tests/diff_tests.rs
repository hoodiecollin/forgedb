use forgedb_migrations::*;

#[test]
fn test_detect_added_model() {
    let old_schema = SimpleSchema { models: vec![] };
    let new_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![],
            composite_indexes: vec![],
        }],
    };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema);
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::AddModel { .. }));
}

#[test]
fn test_detect_removed_model() {
    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![],
            composite_indexes: vec![],
        }],
    };
    let new_schema = SimpleSchema { models: vec![] };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema);
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::RemoveModel { .. }));
}

#[test]
fn test_detect_added_field() {
    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![],
            composite_indexes: vec![],
        }],
    };
    let new_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "email".to_string(),
                field_type: "string".to_string(),
                nullable: false,
                unique: false,
                indexed: false,
                index_type: "Hash".to_string(),
                constraints: vec![],
            }],
            composite_indexes: vec![],
        }],
    };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema);
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::AddField { .. }));
}

#[test]
fn test_detect_type_change() {
    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "age".to_string(),
                field_type: "u32".to_string(),
                nullable: false,
                unique: false,
                indexed: false,
                index_type: "Hash".to_string(),
                constraints: vec![],
            }],
            composite_indexes: vec![],
        }],
    };
    let new_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "age".to_string(),
                field_type: "u64".to_string(),
                nullable: false,
                unique: false,
                indexed: false,
                index_type: "Hash".to_string(),
                constraints: vec![],
            }],
            composite_indexes: vec![],
        }],
    };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema);
    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], SchemaChange::ChangeFieldType { .. }));
    assert!(changes[0].is_breaking());
}
