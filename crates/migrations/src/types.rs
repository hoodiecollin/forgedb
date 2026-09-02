use crate::diff::SimpleType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const RECORD_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Answer {
    Constant { json: String },
    CopyField { field: String },
    Escape {
        language: EscapeLanguage,
        file: String,
        scaffold_checksum: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscapeLanguage {
    Rust,
    TypeScript,
    Python,
}

impl EscapeLanguage {
    pub fn extension(&self) -> &'static str {
        match self {
            EscapeLanguage::Rust => "rs",
            EscapeLanguage::TypeScript => "ts",
            EscapeLanguage::Python => "py",
        }
    }

    pub fn transform_file(&self) -> String {
        format!("transform.{}", self.extension())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchemaChange {
    AddModel { model_name: String },
    RemoveModel { model_name: String },
    AddField {
        model_name: String,
        field_name: String,
        field_type: SimpleType,
        nullable: bool,
        #[serde(rename = "default_value")]
        default_json: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer: Option<Answer>,
    },
    RemoveField {
        model_name: String,
        field_name: String,
    },
    ChangeFieldType {
        model_name: String,
        field_name: String,
        old_type: SimpleType,
        new_type: SimpleType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer: Option<Answer>,
    },
    ChangeFieldNullability {
        model_name: String,
        field_name: String,
        old_nullable: bool,
        new_nullable: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer: Option<Answer>,
    },
    RenameField {
        model_name: String,
        old_name: String,
        new_name: String,
    },
    RenameModel { old_name: String, new_name: String },
    AddIndex {
        model_name: String,
        field_name: String,
        index_type: String,
    },
    RemoveIndex {
        model_name: String,
        field_name: String,
    },
    AddUniqueConstraint {
        model_name: String,
        field_name: String,
    },
    RemoveUniqueConstraint {
        model_name: String,
        field_name: String,
    },
    AddCompositeIndex {
        model_name: String,
        fields: Vec<String>,
    },
    RemoveCompositeIndex {
        model_name: String,
        fields: Vec<String>,
    },
    AddConstraint {
        model_name: String,
        field_name: String,
        constraint_name: String,
        constraint_params: Vec<String>,
    },
    RemoveConstraint {
        model_name: String,
        field_name: String,
        constraint_name: String,
    },
    ChangeEnumVariants {
        model_name: String,
        field_name: String,
        enum_name: String,
        old_variants: Vec<String>,
        new_variants: Vec<String>,
    },
    ChangeStructLayout {
        model_name: String,
        field_name: String,
        struct_name: String,
        old_fields: Vec<(String, String)>,
        new_fields: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PositionalDelta {
    Unchanged,
    Appended { added: Vec<String> },
    Renamed { old_name: String, new_name: String },
    Reordered { moved: Vec<String> },
    Dropped { dropped: Vec<String> },
}

pub fn classify_positional(old: &[String], new: &[String]) -> PositionalDelta {
    if old == new {
        return PositionalDelta::Unchanged;
    }

    let index_in = |list: &[String], name: &str| list.iter().position(|n| n == name);

    let dropped: Vec<String> = old.iter().filter(|n| !new.contains(n)).cloned().collect();
    let added: Vec<String> = new.iter().filter(|n| !old.contains(n)).cloned().collect();
    let moved: Vec<String> = old
        .iter()
        .filter(|n| match (index_in(old, n), index_in(new, n)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        })
        .cloned()
        .collect();

    if moved.is_empty() && dropped.len() == 1 && added.len() == 1 {
        let old_name = &dropped[0];
        let new_name = &added[0];
        if index_in(old, old_name) == index_in(new, new_name) {
            return PositionalDelta::Renamed {
                old_name: old_name.clone(),
                new_name: new_name.clone(),
            };
        }
    }
    if !dropped.is_empty() {
        return PositionalDelta::Dropped { dropped };
    }
    if !moved.is_empty() {
        return PositionalDelta::Reordered { moved };
    }
    if !added.is_empty() {
        return PositionalDelta::Appended { added };
    }
    PositionalDelta::Unchanged
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutDelta {
    Retyped { fields: Vec<String> },
    Names(PositionalDelta),
}

pub fn classify_layout(old: &[(String, String)], new: &[(String, String)]) -> LayoutDelta {
    let retyped: Vec<String> = old
        .iter()
        .filter_map(|(name, old_ty)| {
            new.iter()
                .find(|(n, _)| n == name)
                .filter(|(_, new_ty)| new_ty != old_ty)
                .map(|_| name.clone())
        })
        .collect();
    if !retyped.is_empty() {
        return LayoutDelta::Retyped { fields: retyped };
    }
    let names = |list: &[(String, String)]| -> Vec<String> {
        list.iter().map(|(n, _)| n.clone()).collect()
    };
    LayoutDelta::Names(classify_positional(&names(old), &names(new)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HopBodyClass {
    Auto,
    Authored,
}

impl SchemaChange {
    fn enum_verdict(old: &[String], new: &[String]) -> (bool, HopBodyClass) {
        match classify_positional(old, new) {
            PositionalDelta::Unchanged => (false, HopBodyClass::Auto),
            PositionalDelta::Appended { .. } => (false, HopBodyClass::Auto),
            PositionalDelta::Reordered { .. } => (true, HopBodyClass::Auto),
            PositionalDelta::Dropped { .. } | PositionalDelta::Renamed { .. } => {
                (true, HopBodyClass::Authored)
            }
        }
    }

    fn struct_verdict(old: &[(String, String)], new: &[(String, String)]) -> (bool, HopBodyClass) {
        match classify_layout(old, new) {
            LayoutDelta::Names(PositionalDelta::Unchanged) => (false, HopBodyClass::Auto),
            LayoutDelta::Names(PositionalDelta::Reordered { .. }) => (true, HopBodyClass::Auto),
            _ => (true, HopBodyClass::Authored),
        }
    }

    pub fn hop_body_class(&self) -> HopBodyClass {
        match self {
            SchemaChange::ChangeFieldType {
                old_type, new_type, ..
            } => {
                if old_type.widens_to(new_type) {
                    HopBodyClass::Auto
                } else {
                    HopBodyClass::Authored
                }
            }
            SchemaChange::ChangeFieldNullability {
                old_nullable: true,
                new_nullable: false,
                ..
            } => HopBodyClass::Authored,
            SchemaChange::ChangeFieldNullability { .. } => HopBodyClass::Auto,
            SchemaChange::AddField {
                nullable: false,
                default_json: None,
                ..
            } => HopBodyClass::Authored,
            SchemaChange::AddField { .. } => HopBodyClass::Auto,
            SchemaChange::ChangeEnumVariants {
                old_variants,
                new_variants,
                ..
            } => Self::enum_verdict(old_variants, new_variants).1,
            SchemaChange::ChangeStructLayout {
                old_fields,
                new_fields,
                ..
            } => Self::struct_verdict(old_fields, new_fields).1,
            SchemaChange::AddModel { .. }
            | SchemaChange::RemoveModel { .. }
            | SchemaChange::RemoveField { .. }
            | SchemaChange::RenameField { .. }
            | SchemaChange::RenameModel { .. }
            | SchemaChange::AddIndex { .. }
            | SchemaChange::RemoveIndex { .. }
            | SchemaChange::AddUniqueConstraint { .. }
            | SchemaChange::RemoveUniqueConstraint { .. }
            | SchemaChange::AddCompositeIndex { .. }
            | SchemaChange::RemoveCompositeIndex { .. }
            | SchemaChange::AddConstraint { .. }
            | SchemaChange::RemoveConstraint { .. } => HopBodyClass::Auto,
        }
    }

    pub fn answer(&self) -> Option<&Answer> {
        match self {
            SchemaChange::AddField { answer, .. }
            | SchemaChange::ChangeFieldType { answer, .. }
            | SchemaChange::ChangeFieldNullability { answer, .. } => answer.as_ref(),
            _ => None,
        }
    }

    pub fn set_answer(&mut self, value: Answer) -> bool {
        match self {
            SchemaChange::AddField { answer, .. }
            | SchemaChange::ChangeFieldType { answer, .. }
            | SchemaChange::ChangeFieldNullability { answer, .. } => {
                *answer = Some(value);
                true
            }
            _ => false,
        }
    }

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
            | SchemaChange::RemoveConstraint { model_name, .. }
            | SchemaChange::ChangeEnumVariants { model_name, .. }
            | SchemaChange::ChangeStructLayout { model_name, .. } => model_name,
            SchemaChange::RenameModel { new_name, .. } => new_name,
        }
    }

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
            SchemaChange::ChangeFieldNullability { .. } => false,
            SchemaChange::AddField {
                nullable: false,
                default_json: None,
                ..
            } => true,
            SchemaChange::AddField { .. } => false,
            SchemaChange::RenameField { .. } => true,
            SchemaChange::RenameModel { .. } => true,
            SchemaChange::RemoveUniqueConstraint { .. } => false,
            SchemaChange::AddUniqueConstraint { .. } => true,
            SchemaChange::ChangeEnumVariants {
                old_variants,
                new_variants,
                ..
            } => Self::enum_verdict(old_variants, new_variants).0,
            SchemaChange::ChangeStructLayout {
                old_fields,
                new_fields,
                ..
            } => Self::struct_verdict(old_fields, new_fields).0,
            SchemaChange::AddModel { .. }
            | SchemaChange::AddIndex { .. }
            | SchemaChange::RemoveIndex { .. }
            | SchemaChange::AddCompositeIndex { .. }
            | SchemaChange::RemoveCompositeIndex { .. }
            | SchemaChange::AddConstraint { .. }
            | SchemaChange::RemoveConstraint { .. } => false,
        }
    }

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
                ..
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
                ..
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
            SchemaChange::ChangeEnumVariants {
                model_name,
                field_name,
                enum_name,
                old_variants,
                new_variants,
            } => {
                let breaking =
                    Self::breaking_marker(Self::enum_verdict(old_variants, new_variants).0);
                let what = match classify_positional(old_variants, new_variants) {
                    PositionalDelta::Unchanged => "unchanged".to_string(),
                    PositionalDelta::Appended { added } => {
                        format!("append {}", added.join(", "))
                    }
                    PositionalDelta::Renamed { old_name, new_name } => format!(
                        "RENAME '{}' to '{}' — rows holding the old name have nothing to decode into",
                        old_name, new_name
                    ),
                    PositionalDelta::Reordered { moved } => format!(
                        "REORDER {} — every stored discriminant re-maps",
                        moved.join(", ")
                    ),
                    PositionalDelta::Dropped { dropped } => format!(
                        "REMOVE {} — stored discriminants re-map and some fall out of range",
                        dropped.join(", ")
                    ),
                };
                format!(
                    "Enum '{}' behind '{}.{}': {}{}",
                    enum_name, model_name, field_name, what, breaking
                )
            }
            SchemaChange::ChangeStructLayout {
                model_name,
                field_name,
                struct_name,
                old_fields,
                new_fields,
            } => {
                let breaking =
                    Self::breaking_marker(Self::struct_verdict(old_fields, new_fields).0);
                let what = match classify_layout(old_fields, new_fields) {
                    LayoutDelta::Retyped { fields } => format!(
                        "RETYPE {} — the stored bytes are reinterpreted",
                        fields.join(", ")
                    ),
                    LayoutDelta::Names(PositionalDelta::Unchanged) => "unchanged".to_string(),
                    LayoutDelta::Names(PositionalDelta::Appended { added }) => format!(
                        "ADD {} — the row width changes, so every stored row re-frames",
                        added.join(", ")
                    ),
                    LayoutDelta::Names(PositionalDelta::Renamed { old_name, new_name }) => format!(
                        "RENAME '{}' to '{}' — the old JSON key no longer decodes",
                        old_name, new_name
                    ),
                    LayoutDelta::Names(PositionalDelta::Reordered { moved }) => format!(
                        "REORDER {} — every field reads its neighbour's bytes",
                        moved.join(", ")
                    ),
                    LayoutDelta::Names(PositionalDelta::Dropped { dropped }) => format!(
                        "REMOVE {} — the row width shrinks, so every stored row re-frames",
                        dropped.join(", ")
                    ),
                };
                format!(
                    "Struct '{}' behind '{}.{}': {}{}",
                    struct_name, model_name, field_name, what, breaking
                )
            }
        }
    }

    fn breaking_marker(breaking: bool) -> &'static str {
        if breaking { " (⚠️  BREAKING)" } else { "" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub changes: Vec<SchemaChange>,
    pub checksum: String,
    #[serde(default)]
    pub from_version: u32,
    #[serde(default)]
    pub to_version: u32,
    #[serde(default, skip_serializing_if = "is_legacy_record_version")]
    pub record_version: u32,
}

fn is_legacy_record_version(v: &u32) -> bool {
    *v == 0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChecksumStatus {
    Verified,
    Mismatch,
    Unverifiable,
    UnknownAlgorithm(String),
}

impl Migration {
    pub fn new(description: String, changes: Vec<SchemaChange>) -> Self {
        Self::new_versioned(description, changes, 0, 0)
    }

    pub fn new_versioned(
        description: String,
        changes: Vec<SchemaChange>,
        from_version: u32,
        to_version: u32,
    ) -> Self {
        Self::with_id(Self::next_id(), description, changes, from_version, to_version)
    }

    pub fn next_id() -> String {
        format!("{}", Utc::now().format("%Y%m%d%H%M%S"))
    }

    pub fn with_id(
        id: String,
        description: String,
        changes: Vec<SchemaChange>,
        from_version: u32,
        to_version: u32,
    ) -> Self {
        let mut migration = Migration {
            id,
            description,
            created_at: Utc::now(),
            changes,
            checksum: String::new(),
            from_version,
            to_version,
            record_version: RECORD_VERSION,
        };

        migration.checksum = migration.calculate_checksum();
        migration
    }

    pub fn unanswered(&self) -> Vec<&SchemaChange> {
        self.changes
            .iter()
            .filter(|c| c.hop_body_class() == HopBodyClass::Authored && c.answer().is_none())
            .collect()
    }

    pub fn authored_changes(&self) -> Vec<&SchemaChange> {
        self.changes
            .iter()
            .filter(|c| c.hop_body_class() == HopBodyClass::Authored)
            .collect()
    }

    fn calculate_checksum(&self) -> String {
        let mut temp = self.clone();
        temp.checksum = String::new();
        let json = serde_json::to_string(&temp).unwrap_or_default();
        checksum::compute(json.as_bytes())
    }

    pub fn verify_checksum(&self) -> bool {
        matches!(
            self.checksum_status(),
            ChecksumStatus::Verified | ChecksumStatus::Unverifiable
        )
    }

    pub fn checksum_status(&self) -> ChecksumStatus {
        match checksum::classify(&self.checksum) {
            checksum::Kind::Current => {
                if self.checksum == self.calculate_checksum() {
                    ChecksumStatus::Verified
                } else {
                    ChecksumStatus::Mismatch
                }
            }
            checksum::Kind::Legacy => ChecksumStatus::Unverifiable,
            checksum::Kind::Unknown(algo) => ChecksumStatus::UnknownAlgorithm(algo.to_string()),
        }
    }

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

    pub fn has_breaking_changes(&self) -> bool {
        self.changes.iter().any(|c| c.is_breaking())
    }

    pub fn breaking_changes(&self) -> Vec<&SchemaChange> {
        self.changes.iter().filter(|c| c.is_breaking()).collect()
    }

    pub fn safe_changes(&self) -> Vec<&SchemaChange> {
        self.changes.iter().filter(|c| !c.is_breaking()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub migration_id: String,
    pub applied_at: DateTime<Utc>,
    pub checksum: String,
}

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
    pub fn is_applied(&self, migration_id: &str) -> bool {
        self.applied_migrations
            .iter()
            .any(|r| r.migration_id == migration_id)
    }

    pub fn add_migration(&mut self, migration_id: String, checksum: String) {
        self.applied_migrations.push(MigrationRecord {
            migration_id,
            applied_at: Utc::now(),
            checksum,
        });
    }

    pub fn remove_last_migration(&mut self) -> Option<MigrationRecord> {
        self.applied_migrations.pop()
    }

    pub fn last_migration(&self) -> Option<&MigrationRecord> {
        self.applied_migrations.last()
    }
}

pub mod checksum {
    pub const TAG: &str = "fnv1a64";

    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x100_0000_01b3;

    pub fn compute(data: &[u8]) -> String {
        let mut hash = FNV_OFFSET_BASIS;
        for byte in data {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        format!("{TAG}:{hash:016x}")
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Kind<'a> {
        Current,
        Legacy,
        Unknown(&'a str),
    }

    pub fn classify(stored: &str) -> Kind<'_> {
        match stored.split_once(':') {
            Some((TAG, _)) => Kind::Current,
            Some((other, _)) => Kind::Unknown(other),
            None => Kind::Legacy,
        }
    }
}
