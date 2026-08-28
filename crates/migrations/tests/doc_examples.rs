use forgedb_migrations::{
    Migration, MigrationTracker, SchemaChange, SchemaDiffer, SimpleConstraint, SimpleField,
    SimpleModel, SimpleSchema, SimpleType,
};

fn field(name: &str, ty: SimpleType) -> SimpleField {
    SimpleField {
        name: name.to_string(),
        ty,
        nullable: false,
        unique: false,
        indexed: false,
        index_type: "hash".to_string(),
        constraints: Vec::<SimpleConstraint>::new(),
        depends_on: vec![],
    }
}

fn schema(fields: Vec<SimpleField>) -> SimpleSchema {
    SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields,
            composite_indexes: vec![],
        }],
        enums: vec![],
        structs: vec![],
    }
}

#[test]
fn differ_detects_a_single_added_field() {
    let old_schema = schema(vec![field("id", SimpleType::Uuid)]);
    let new_schema = schema(vec![
        field("id", SimpleType::Uuid),
        field("email", SimpleType::Str),
    ]);

    let diff = SchemaDiffer::diff(&old_schema, &new_schema);
    assert_eq!(diff.changes.len(), 1);
}

#[allow(dead_code)]
fn tracker_api_compiles() -> Result<(), String> {
    let tracker = MigrationTracker::new("./migrations")?;

    let _migration = Migration::new(
        "add email".to_string(),
        vec![SchemaChange::AddModel {
            model_name: "Post".to_string(),
        }],
    );

    let all_ids = vec!["20240101_add_email".to_string()];
    let _pending = tracker.pending_migrations(&all_ids);
    let _ = tracker.last_migration();
    Ok(())
}
