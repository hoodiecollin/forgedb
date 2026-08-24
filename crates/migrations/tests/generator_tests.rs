use forgedb_migrations::*;
use tempfile::TempDir;

#[test]
fn test_generate_and_load_migration() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path();

    let changes = vec![
        SchemaChange::AddModel {
            model_name: "User".to_string(),
        },
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "email".to_string(),
            field_type: "string".parse().unwrap(),
            nullable: false,
            default_json: None,
        },
    ];

    // Generate migration
    let migration = MigrationGenerator::generate(
        migrations_dir,
        "Create User model".to_string(),
        changes.clone(),
    )
    .unwrap();

    assert_eq!(migration.changes.len(), 2);
    assert_eq!(migration.description, "Create User model");

    // Load it back
    let migrations = MigrationGenerator::load_all_migrations(migrations_dir).unwrap();
    assert_eq!(migrations.len(), 1);
    assert_eq!(migrations[0].id, migration.id);
    assert_eq!(migrations[0].changes, changes);
}

#[test]
fn test_migration_report() {
    let changes = vec![
        SchemaChange::AddField {
            model_name: "User".to_string(),
            field_name: "name".to_string(),
            field_type: "string".parse().unwrap(),
            nullable: false,
            default_json: None,
        },
        SchemaChange::RemoveField {
            model_name: "User".to_string(),
            field_name: "old_field".to_string(),
        },
    ];

    let migration = Migration::new("Update User model".to_string(), changes);
    let report = MigrationGenerator::generate_report(&migration);

    assert!(report.contains("Update User model"));
    assert!(report.contains("BREAKING CHANGES"));
    assert!(report.contains("Add field"));
    assert!(report.contains("Remove field"));
}
