use crate::types::{Migration, SchemaChange};
use std::fs;
use std::path::Path;

/// Generates migration files from schema changes
pub struct MigrationGenerator;

impl MigrationGenerator {
    /// Generate a new migration file
    pub fn generate<P: AsRef<Path>>(
        migrations_dir: P,
        description: String,
        changes: Vec<SchemaChange>,
    ) -> Result<Migration, String> {
        let migrations_dir = migrations_dir.as_ref();

        // Create migrations directory if it doesn't exist
        if !migrations_dir.exists() {
            fs::create_dir_all(migrations_dir)
                .map_err(|e| format!("Failed to create migrations directory: {}", e))?;
        }

        // Create migration
        let migration = Migration::new(description, changes);

        // Write migration file
        let migration_path = migrations_dir.join(migration.filename());
        let json = serde_json::to_string_pretty(&migration)
            .map_err(|e| format!("Failed to serialize migration: {}", e))?;

        fs::write(&migration_path, json)
            .map_err(|e| format!("Failed to write migration file: {}", e))?;

        Ok(migration)
    }

    /// Load a migration from a file
    pub fn load_migration<P: AsRef<Path>>(path: P) -> Result<Migration, String> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read migration file: {}", e))?;

        let migration: Migration = serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse migration file: {}", e))?;

        // Verify checksum
        if !migration.verify_checksum() {
            return Err(
                "Migration file checksum verification failed - file may be corrupted".to_string(),
            );
        }

        Ok(migration)
    }

    /// Load all migrations from a directory
    pub fn load_all_migrations<P: AsRef<Path>>(
        migrations_dir: P,
    ) -> Result<Vec<Migration>, String> {
        let migrations_dir = migrations_dir.as_ref();

        if !migrations_dir.exists() {
            return Ok(Vec::new());
        }

        let mut migrations = Vec::new();
        let entries = fs::read_dir(migrations_dir)
            .map_err(|e| format!("Failed to read migrations directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            // Only process .json files
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match Self::load_migration(&path) {
                    Ok(migration) => migrations.push(migration),
                    Err(e) => eprintln!("Warning: Failed to load migration {:?}: {}", path, e),
                }
            }
        }

        // Sort migrations by ID (timestamp)
        migrations.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(migrations)
    }

    /// Generate a migration summary report
    pub fn generate_report(migration: &Migration) -> String {
        let mut report = String::new();

        report.push_str(&format!(
            "Migration: {} ({})\n",
            migration.description, migration.id
        ));
        report.push_str(&format!(
            "Created: {}\n",
            migration.created_at.format("%Y-%m-%d %H:%M:%S")
        ));
        report.push_str(&format!("Changes: {}\n", migration.changes.len()));

        if migration.has_breaking_changes() {
            report.push_str("\n⚠️  WARNING: This migration contains BREAKING CHANGES\n");
            report.push_str("Breaking changes:\n");
            for change in migration.breaking_changes() {
                report.push_str(&format!("  - {}\n", change.description()));
            }
        }

        if !migration.safe_changes().is_empty() {
            report.push_str("\nSafe changes:\n");
            for change in migration.safe_changes() {
                report.push_str(&format!("  - {}\n", change.description()));
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                field_type: "string".to_string(),
                nullable: false,
                default_value: None,
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
                field_type: "string".to_string(),
                nullable: false,
                default_value: None,
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
}
