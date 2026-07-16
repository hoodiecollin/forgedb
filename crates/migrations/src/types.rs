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

/// How the transformer generator (#74 Phase 3) will produce the new-row body for
/// a single schema change ("hop"), decided ONCE at `migrate create` time and
/// frozen into the migration record (C8/C9).  This is a **dev-time codegen
/// classification** — the transformer bin has no runtime mechanism-selection
/// site; it just runs whichever body was baked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HopBodyClass {
    /// The differ can PROVE the new-row body from the diff alone —
    /// **additive / constant / structural**: set a field's default, omit a
    /// removed column, rename a field/model, or an index/constraint change that
    /// leaves the row bytes identical.  The transformer generator emits the body
    /// automatically; no developer authoring is required.
    Auto,
    /// The new-row body needs **semantic understanding the diff cannot supply** —
    /// re-encode a value whose type changed, choose a fill value when narrowing a
    /// nullable field to NOT NULL, or supply a value for a newly-required field
    /// with no default.  Scaffolded at `migrate create` for the developer to
    /// author + freeze (`migrations/{id}/transform.rs`); the transformer embeds
    /// that frozen source verbatim (C13).
    Authored,
}

impl SchemaChange {
    /// Classify how this hop's new-row body is produced (#74 Phase 2, C8/C9).
    ///
    /// This is deliberately **distinct from [`is_breaking`](Self::is_breaking)**:
    /// a change can be breaking yet still `Auto` (dropping a column or a model is
    /// breaking for readers but the row transform is a pure structural omit; a
    /// `&unique` add may fail on duplicates but the row bytes are identity —
    /// uniqueness is *validated* during replay, not *transformed*).  Only the
    /// residue the differ genuinely cannot PROVE a value for is `Authored`.
    pub fn hop_body_class(&self) -> HopBodyClass {
        match self {
            // Re-encoding a value from one type to another is semantic — the
            // differ cannot know the mapping (e.g. `u32 -> string`, `string ->
            // enum`), so the developer authors it.
            SchemaChange::ChangeFieldType { .. } => HopBodyClass::Authored,
            // Narrowing nullable -> NOT NULL needs a fill value for the existing
            // `None`s; the differ has none to offer.
            SchemaChange::ChangeFieldNullability {
                old_nullable: true,
                new_nullable: false,
                ..
            } => HopBodyClass::Authored,
            // A newly-required field with no default has no value the differ can
            // synthesize for existing rows.
            SchemaChange::AddField {
                nullable: false,
                default_value: None,
                ..
            } => HopBodyClass::Authored,
            // Everything else is provable structural/constant: additive nullable/
            // defaulted adds (backfill), removes (omit), renames (name map), and
            // index/constraint changes (identity row body).
            _ => HopBodyClass::Auto,
        }
    }

    /// The model this change targets, if it is a single-model change.  Used by the
    /// authored-body scaffold (#74 Phase 2/3) to group `Authored` hops per model.
    /// `RenameModel` reports its *new* name (the destination shape); `AddModel`/
    /// `RemoveModel` report the model itself.
    pub fn target_model(&self) -> &str {
        match self {
            SchemaChange::AddModel { model_name }
            | SchemaChange::RemoveModel { model_name }
            | SchemaChange::AddField { model_name, .. }
            | SchemaChange::RemoveField { model_name, .. }
            | SchemaChange::ChangeFieldType { model_name, .. }
            | SchemaChange::ChangeFieldNullability { model_name, .. }
            | SchemaChange::RenameField { model_name, .. }
            | SchemaChange::AddIndex { model_name, .. }
            | SchemaChange::RemoveIndex { model_name, .. }
            | SchemaChange::AddUniqueConstraint { model_name, .. }
            | SchemaChange::RemoveUniqueConstraint { model_name, .. }
            | SchemaChange::AddCompositeIndex { model_name, .. }
            | SchemaChange::RemoveCompositeIndex { model_name, .. }
            | SchemaChange::AddConstraint { model_name, .. }
            | SchemaChange::RemoveConstraint { model_name, .. } => model_name,
            SchemaChange::RenameModel { new_name, .. } => new_name,
        }
    }

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
            // M4: Adding a NOT NULL column without a default to a populated table is
            // breaking — existing rows have no value to fill in.
            SchemaChange::AddField {
                nullable: false,
                default_value: None,
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
                old_nullable: _,
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
    /// On-disk `format_version` this migration expects BEFORE it runs (#74
    /// Phase 2 — the serial version interlock).  `0` for a legacy record written
    /// before versioning existed; a real lineage is contiguous (`to_version` of
    /// one migration == `from_version` of the next).
    #[serde(default)]
    pub from_version: u32,
    /// On-disk `format_version` this migration stamps AFTER it runs.  The current
    /// expected version of the whole database is the highest `to_version` in the
    /// lineage (see [`MigrationLineage`](crate::MigrationLineage)); that is what
    /// codegen bakes into `EXPECTED_FORMAT_VERSION` (red line #8 — lineage-sourced,
    /// never hand-edited).
    #[serde(default)]
    pub to_version: u32,
}

impl Migration {
    /// Create a new migration (version fields defaulted to `0` — used by callers
    /// that do not track the lineage, and by the crate's own unit tests).
    pub fn new(description: String, changes: Vec<SchemaChange>) -> Self {
        Self::new_versioned(description, changes, 0, 0)
    }

    /// Create a new migration stamped with its serial version interlock (#74
    /// Phase 2).  `from_version`/`to_version` come from the committed lineage at
    /// `migrate create` time; the checksum covers them like every other field.
    pub fn new_versioned(
        description: String,
        changes: Vec<SchemaChange>,
        from_version: u32,
        to_version: u32,
    ) -> Self {
        let now = Utc::now();
        let id = format!("{}", now.format("%Y%m%d%H%M%S"));

        let mut migration = Migration {
            id: id.clone(),
            description,
            created_at: now,
            changes,
            checksum: String::new(),
            from_version,
            to_version,
        };

        // Calculate checksum
        migration.checksum = migration.calculate_checksum();
        migration
    }

    /// Classify each change's hop body (#74 Phase 2).  A migration is "fully
    /// automatic" when every hop is [`HopBodyClass::Auto`]; any [`Authored`]
    /// residue must be authored + frozen before the transformer can be generated.
    ///
    /// [`Authored`]: HopBodyClass::Authored
    pub fn authored_changes(&self) -> Vec<&SchemaChange> {
        self.changes
            .iter()
            .filter(|c| c.hop_body_class() == HopBodyClass::Authored)
            .collect()
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
