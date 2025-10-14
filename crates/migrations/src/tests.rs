use super::*;
use tempfile::TempDir;

#[test]
fn test_full_migration_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("migrations");
    let data_dir = temp_dir.path().join("data");

    // Create a migration
    let changes = vec![
        SchemaChange::AddModel {
            model_name: "User".to_string(),
        },
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "email".to_string(),
            field_type: "string".to_string(),
            nullable: false,
            default_value: None,
        },
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "age".to_string(),
            field_type: "u32".to_string(),
            nullable: true,
            default_value: Some("0".to_string()),
        },
    ];

    // Generate migration
    let migration = MigrationGenerator::generate(
        &migrations_dir,
        "Create User model".to_string(),
        changes,
    )
    .unwrap();

    assert_eq!(migration.changes.len(), 3);

    // Create tracker
    let mut tracker = MigrationTracker::new(&migrations_dir).unwrap();
    assert!(!tracker.is_applied(&migration.id));

    // Execute migration
    MigrationExecutor::execute_up(&migration, &data_dir).unwrap();

    // Mark as applied
    tracker
        .mark_applied(migration.id.clone(), migration.checksum.clone())
        .unwrap();

    assert!(tracker.is_applied(&migration.id));

    // Verify we can load migrations
    let all_migrations = MigrationGenerator::load_all_migrations(&migrations_dir).unwrap();
    assert_eq!(all_migrations.len(), 1);
    assert_eq!(all_migrations[0].id, migration.id);
}

#[test]
fn test_breaking_change_detection() {
    let changes = vec![
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "name".to_string(),
            field_type: "string".to_string(),
            nullable: false,
            default_value: None,
        },
        SchemaChange::RemoveField {
            model_name: "User".to_string(),
            field_name: "old_field".to_string(),
        },
        SchemaChange::ChangeFieldType {
            model_name: "User".to_string(),
            field_name: "age".to_string(),
            old_type: "u32".to_string(),
            new_type: "u64".to_string(),
        },
    ];

    let migration = Migration::new("Update User model".to_string(), changes);

    assert!(migration.has_breaking_changes());
    assert_eq!(migration.breaking_changes().len(), 2);
    assert_eq!(migration.safe_changes().len(), 1);
}

#[test]
fn test_migration_rollback() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("migrations");
    let data_dir = temp_dir.path().join("data");

    let changes = vec![SchemaChange::AddModel {
        model_name: "TestModel".to_string(),
    }];

    let migration = MigrationGenerator::generate(
        &migrations_dir,
        "Add test model".to_string(),
        changes,
    )
    .unwrap();

    // Execute up
    MigrationExecutor::execute_up(&migration, &data_dir).unwrap();

    // Track it
    let mut tracker = MigrationTracker::new(&migrations_dir).unwrap();
    tracker
        .mark_applied(migration.id.clone(), migration.checksum.clone())
        .unwrap();

    assert!(tracker.is_applied(&migration.id));

    // Rollback
    MigrationExecutor::execute_down(&migration, &data_dir).unwrap();
    tracker.mark_rolled_back().unwrap();

    assert!(!tracker.is_applied(&migration.id));
}

#[test]
fn test_schema_differ() {
    use diff::*;

    let old_schema = SimpleSchema {
        models: vec![SimpleModel {
            name: "User".to_string(),
            fields: vec![SimpleField {
                name: "id".to_string(),
                field_type: "uuid".to_string(),
                nullable: false,
                unique: true,
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
            fields: vec![
                SimpleField {
                    name: "id".to_string(),
                    field_type: "uuid".to_string(),
                    nullable: false,
                    unique: true,
                    indexed: false,
                    index_type: "Hash".to_string(),
                    constraints: vec![],
                },
                SimpleField {
                    name: "email".to_string(),
                    field_type: "string".to_string(),
                    nullable: false,
                    unique: true,
                    indexed: true,
                    index_type: "Hash".to_string(),
                    constraints: vec![SimpleConstraint {
                        name: "email".to_string(),
                        params: vec![],
                    }],
                },
            ],
            composite_indexes: vec![],
        }],
    };

    let changes = SchemaDiffer::diff(&old_schema, &new_schema);

    // Should detect: AddField, AddIndex, AddUniqueConstraint, AddConstraint
    assert!(changes.len() >= 1);

    // Check that we detected the new field
    let has_add_field = changes.iter().any(|c| matches!(c, SchemaChange::AddField { .. }));
    assert!(has_add_field);
}
