use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a change detected in a schema migration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchemaChange {
    /// A new model was added
    AddModel { model_name: String },
    /// A model was removed
    RemoveModel { model_name: String },
    /// A field was added to a model
    AddField {
        model_name: String,
        field_name: String,
        field_type: String,
        nullable: bool,
        default_value: Option<String>,
    },
    /// A field was removed from a model
    RemoveField {
        model_name: String,
        field_name: String,
    },
    /// A field's type was changed
    ChangeFieldType {
        model_name: String,
        field_name: String,
        old_type: String,
        new_type: String,
    },
    /// A field's nullability was changed
    ChangeFieldNullability {
        model_name: String,
        field_name: String,
        old_nullable: bool,
        new_nullable: bool,
    },
    /// A field was renamed
    RenameField {
        model_name: String,
        old_name: String,
        new_name: String,
    },
    /// A model was renamed
    RenameModel { old_name: String, new_name: String },
    /// An index was added
    AddIndex {
        model_name: String,
        field_name: String,
        index_type: String,
    },
    /// An index was removed
    RemoveIndex {
        model_name: String,
        field_name: String,
    },
    /// A unique constraint was added
    AddUniqueConstraint {
        model_name: String,
        field_name: String,
    },
    /// A unique constraint was removed
    RemoveUniqueConstraint {
        model_name: String,
        field_name: String,
    },
    /// A composite index was added
    AddCompositeIndex {
        model_name: String,
        fields: Vec<String>,
    },
    /// A composite index was removed
    RemoveCompositeIndex {
        model_name: String,
        fields: Vec<String>,
    },
    /// A constraint was added
    AddConstraint {
        model_name: String,
        field_name: String,
        constraint_name: String,
        constraint_params: Vec<String>,
    },
    /// A constraint was removed
    RemoveConstraint {
        model_name: String,
        field_name: String,
        constraint_name: String,
    },
}

impl SchemaChange {
    /// Returns true if this change is considered breaking (requires manual intervention)
    pub fn is_breaking(&self) -> bool {
        match self {
            SchemaChange::RemoveModel { .. } => true,
            SchemaChange::RemoveField { .. } => true,
            SchemaChange::ChangeFieldType { .. } => true,
            SchemaChange::ChangeFieldNullability {
                old_nullable: true,
                new_nullable: false,
                ..
            } => true,
            SchemaChange::RemoveUniqueConstraint { .. } => false, // Safe to remove constraints
            SchemaChange::AddUniqueConstraint { .. } => true,     // May fail if duplicates exist
            _ => false,
        }
    }

    /// Returns a human-readable description of the change
    pub fn description(&self) -> String {
        match self {
            SchemaChange::AddModel { model_name } => {
                format!("Add model '{}'", model_name)
            }
            SchemaChange::RemoveModel { model_name } => {
                format!("Remove model '{}' (⚠️  BREAKING)", model_name)
            }
            SchemaChange::AddField {
                model_name,
                field_name,
                field_type,
                nullable,
                ..
            } => {
                let null_str = if *nullable { "?" } else { "" };
                format!(
                    "Add field '{}.{}{}' ({})",
                    model_name, field_name, null_str, field_type
                )
            }
            SchemaChange::RemoveField {
                model_name,
                field_name,
            } => {
                format!(
                    "Remove field '{}.{}' (⚠️  BREAKING)",
                    model_name, field_name
                )
            }
            SchemaChange::ChangeFieldType {
                model_name,
                field_name,
                old_type,
                new_type,
            } => {
                format!(
                    "Change type of '{}.{}' from {} to {} (⚠️  BREAKING)",
                    model_name, field_name, old_type, new_type
                )
            }
            SchemaChange::ChangeFieldNullability {
                model_name,
                field_name,
                old_nullable,
                new_nullable,
            } => {
                let change = if *new_nullable {
                    "nullable"
                } else {
                    "non-nullable"
                };
                let breaking = if !new_nullable {
                    " (⚠️  BREAKING)"
                } else {
                    ""
                };
                format!(
                    "Make '{}.{}' {}{}",
                    model_name, field_name, change, breaking
                )
            }
            SchemaChange::RenameField {
                model_name,
                old_name,
                new_name,
            } => {
                format!(
                    "Rename field '{}.{}' to '{}'",
                    model_name, old_name, new_name
                )
            }
            SchemaChange::RenameModel { old_name, new_name } => {
                format!("Rename model '{}' to '{}'", old_name, new_name)
            }
            SchemaChange::AddIndex {
                model_name,
                field_name,
                index_type,
            } => {
                format!(
                    "Add {} index on '{}.{}'",
                    index_type, model_name, field_name
                )
            }
            SchemaChange::RemoveIndex {
                model_name,
                field_name,
            } => {
                format!("Remove index from '{}.{}'", model_name, field_name)
            }
            SchemaChange::AddUniqueConstraint {
                model_name,
                field_name,
            } => {
                format!(
                    "Add unique constraint to '{}.{}' (⚠️  may fail)",
                    model_name, field_name
                )
            }
            SchemaChange::RemoveUniqueConstraint {
                model_name,
                field_name,
            } => {
                format!(
                    "Remove unique constraint from '{}.{}'",
                    model_name, field_name
                )
            }
            SchemaChange::AddCompositeIndex { model_name, fields } => {
                format!(
                    "Add composite index on '{}.{}'",
                    model_name,
                    fields.join(", ")
                )
            }
            SchemaChange::RemoveCompositeIndex { model_name, fields } => {
                format!(
                    "Remove composite index from '{}.{}'",
                    model_name,
                    fields.join(", ")
                )
            }
            SchemaChange::AddConstraint {
                model_name,
                field_name,
                constraint_name,
                ..
            } => {
                format!(
                    "Add @{} constraint to '{}.{}'",
                    constraint_name, model_name, field_name
                )
            }
            SchemaChange::RemoveConstraint {
                model_name,
                field_name,
                constraint_name,
            } => {
                format!(
                    "Remove @{} constraint from '{}.{}'",
                    constraint_name, model_name, field_name
                )
            }
        }
    }
}

/// Represents a migration file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    /// Unique identifier (timestamp-based)
    pub id: String,
    /// Human-readable description
    pub description: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// List of changes in this migration
    pub changes: Vec<SchemaChange>,
    /// Checksum of the migration file for integrity
    pub checksum: String,
}

impl Migration {
    /// Create a new migration
    pub fn new(description: String, changes: Vec<SchemaChange>) -> Self {
        let now = Utc::now();
        let id = format!("{}", now.format("%Y%m%d%H%M%S"));

        let mut migration = Migration {
            id: id.clone(),
            description,
            created_at: now,
            changes,
            checksum: String::new(),
        };

        // Calculate checksum
        migration.checksum = migration.calculate_checksum();
        migration
    }

    /// Calculate checksum for migration integrity
    fn calculate_checksum(&self) -> String {
        let mut temp = self.clone();
        temp.checksum = String::new();
        let json = serde_json::to_string(&temp).unwrap_or_default();
        md5::compute(json.as_bytes())
    }

    /// Verify migration integrity
    pub fn verify_checksum(&self) -> bool {
        let expected = self.calculate_checksum();
        self.checksum == expected
    }

    /// Get the filename for this migration
    pub fn filename(&self) -> String {
        let safe_desc = self
            .description
            .to_lowercase()
            .replace(' ', "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();
        format!("{}_{}.json", self.id, safe_desc)
    }

    /// Check if migration has breaking changes
    pub fn has_breaking_changes(&self) -> bool {
        self.changes.iter().any(|c| c.is_breaking())
    }

    /// Get list of breaking changes
    pub fn breaking_changes(&self) -> Vec<&SchemaChange> {
        self.changes.iter().filter(|c| c.is_breaking()).collect()
    }

    /// Get list of safe changes
    pub fn safe_changes(&self) -> Vec<&SchemaChange> {
        self.changes.iter().filter(|c| !c.is_breaking()).collect()
    }
}

/// Migration status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub migration_id: String,
    pub applied_at: DateTime<Utc>,
    pub checksum: String,
}

/// Migration state file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationState {
    pub applied_migrations: Vec<MigrationRecord>,
}

impl Default for MigrationState {
    fn default() -> Self {
        MigrationState {
            applied_migrations: Vec::new(),
        }
    }
}

impl MigrationState {
    /// Check if a migration has been applied
    pub fn is_applied(&self, migration_id: &str) -> bool {
        self.applied_migrations
            .iter()
            .any(|r| r.migration_id == migration_id)
    }

    /// Add a migration record
    pub fn add_migration(&mut self, migration_id: String, checksum: String) {
        self.applied_migrations.push(MigrationRecord {
            migration_id,
            applied_at: Utc::now(),
            checksum,
        });
    }

    /// Remove the last migration record (for rollback)
    pub fn remove_last_migration(&mut self) -> Option<MigrationRecord> {
        self.applied_migrations.pop()
    }

    /// Get the last applied migration
    pub fn last_migration(&self) -> Option<&MigrationRecord> {
        self.applied_migrations.last()
    }
}

// Simple MD5 implementation for checksums
mod md5 {
    pub fn compute(data: &[u8]) -> String {
        // Simple hash for now - in production, use proper MD5
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}
