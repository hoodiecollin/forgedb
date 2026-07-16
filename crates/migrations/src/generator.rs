use crate::types::{Migration, SchemaChange};
use std::fs;
use std::path::Path;

/// Generates migration files from schema changes
pub struct MigrationGenerator;

impl MigrationGenerator {
    /// Generate a new migration file (version fields defaulted to `0`).
    pub fn generate<P: AsRef<Path>>(
        migrations_dir: P,
        description: String,
        changes: Vec<SchemaChange>,
    ) -> Result<Migration, String> {
        Self::generate_versioned(migrations_dir, description, changes, 0, 0)
    }

    /// Generate a new migration file stamped with its serial version interlock
    /// (#74 Phase 2).  The caller derives `from_version`/`to_version` from the
    /// committed lineage ([`MigrationLineage::next_version_span`]).
    ///
    /// [`MigrationLineage::next_version_span`]: crate::MigrationLineage::next_version_span
    pub fn generate_versioned<P: AsRef<Path>>(
        migrations_dir: P,
        description: String,
        changes: Vec<SchemaChange>,
        from_version: u32,
        to_version: u32,
    ) -> Result<Migration, String> {
        let migrations_dir = migrations_dir.as_ref();

        // Create migrations directory if it doesn't exist
        if !migrations_dir.exists() {
            fs::create_dir_all(migrations_dir)
                .map_err(|e| format!("Failed to create migrations directory: {}", e))?;
        }

        // Create migration
        let migration =
            Migration::new_versioned(description, changes, from_version, to_version);

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
