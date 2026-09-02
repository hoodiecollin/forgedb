use crate::types::{ChecksumStatus, Migration, SchemaChange};
use std::fs;
use std::path::Path;

pub struct MigrationGenerator;

impl MigrationGenerator {
    pub fn generate<P: AsRef<Path>>(
        migrations_dir: P,
        description: String,
        changes: Vec<SchemaChange>,
    ) -> Result<Migration, String> {
        Self::generate_versioned(migrations_dir, description, changes, 0, 0)
    }

    pub fn generate_versioned<P: AsRef<Path>>(
        migrations_dir: P,
        description: String,
        changes: Vec<SchemaChange>,
        from_version: u32,
        to_version: u32,
    ) -> Result<Migration, String> {
        let migrations_dir = migrations_dir.as_ref();

        if !migrations_dir.exists() {
            fs::create_dir_all(migrations_dir)
                .map_err(|e| format!("Failed to create migrations directory: {}", e))?;
        }

        Self::write_migration(
            migrations_dir,
            Migration::new_versioned(description, changes, from_version, to_version),
        )
    }

    pub fn write_migration<P: AsRef<Path>>(
        migrations_dir: P,
        migration: Migration,
    ) -> Result<Migration, String> {
        let migrations_dir = migrations_dir.as_ref();
        if !migrations_dir.exists() {
            fs::create_dir_all(migrations_dir)
                .map_err(|e| format!("Failed to create migrations directory: {}", e))?;
        }
        let migration_path = migrations_dir.join(migration.filename());
        let json = serde_json::to_string_pretty(&migration)
            .map_err(|e| format!("Failed to serialize migration: {}", e))?;

        fs::write(&migration_path, json)
            .map_err(|e| format!("Failed to write migration file: {}", e))?;

        Ok(migration)
    }

    pub fn load_migration<P: AsRef<Path>>(path: P) -> Result<Migration, String> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read migration file: {}", e))?;

        let migration: Migration = serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse migration file: {}", e))?;

        match migration.checksum_status() {
            ChecksumStatus::Verified | ChecksumStatus::Unverifiable => {}
            ChecksumStatus::Mismatch => {
                return Err(format!(
                    "Migration file checksum does not match its contents: {}\n\
                     The file was modified after it was created. Restore it from version \
                     control, or delete the checksum field to accept the current contents.",
                    path.as_ref().display()
                ));
            }
            ChecksumStatus::UnknownAlgorithm(algo) => {
                return Err(format!(
                    "Migration file {} carries a `{algo}` checksum, which this build of \
                     forgedb does not know how to compute.\n\
                     It was almost certainly written by a NEWER forgedb; upgrade rather \
                     than editing the file.",
                    path.as_ref().display()
                ));
            }
        }

        Ok(migration)
    }

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

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match Self::load_migration(&path) {
                    Ok(migration) => migrations.push(migration),
                    Err(e) => eprintln!("Warning: Failed to load migration {:?}: {}", path, e),
                }
            }
        }

        migrations.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(migrations)
    }

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
