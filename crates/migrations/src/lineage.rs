use crate::generator::MigrationGenerator;
use crate::types::{Answer, EscapeLanguage, Migration, checksum};
use std::fs;
use std::path::{Path, PathBuf};

pub const BASELINE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct MigrationLineage {
    migrations: Vec<Migration>,
}

impl MigrationLineage {
    pub fn load<P: AsRef<Path>>(migrations_dir: P) -> Result<Self, String> {
        let mut migrations = MigrationGenerator::load_all_migrations(migrations_dir)?;
        migrations.sort_by(|a, b| {
            a.to_version
                .cmp(&b.to_version)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(Self { migrations })
    }

    pub fn migrations(&self) -> &[Migration] {
        &self.migrations
    }

    pub fn current_schema_version(&self) -> u32 {
        self.migrations
            .iter()
            .map(|m| m.to_version)
            .max()
            .filter(|&v| v != 0)
            .unwrap_or(BASELINE_SCHEMA_VERSION)
    }

    pub fn next_version_span(&self) -> (u32, u32) {
        let from = self.current_schema_version();
        (from, from + 1)
    }

    pub fn expand_range(&self, from: u32, to: u32) -> Result<Vec<Migration>, String> {
        if to < from {
            return Err(format!(
                "invalid migration range: --to {to} is below --from {from}"
            ));
        }
        let mut expanded = Vec::new();
        let mut cursor = from;
        while cursor < to {
            let hop = self
                .migrations
                .iter()
                .find(|m| m.from_version == cursor && m.to_version == cursor + 1)
                .ok_or_else(|| {
                    format!(
                        "migration lineage has no hop from format v{cursor} to v{} — \
                         the range v{from}..v{to} is not contiguous (a version step is \
                         missing; the transformer replays the recorded sequence, never a \
                         synthesized jump)",
                        cursor + 1
                    )
                })?;
            expanded.push(hop.clone());
            cursor += 1;
        }
        Ok(expanded)
    }
}

pub fn migration_body_dir(migrations_dir: &Path, migration_id: &str) -> PathBuf {
    migrations_dir.join(migration_id)
}

pub fn authored_body_path(migrations_dir: &Path, migration_id: &str) -> PathBuf {
    migration_body_dir(migrations_dir, migration_id).join("transform.rs")
}

pub fn versioned_schema_dir(migrations_dir: &Path) -> PathBuf {
    migrations_dir.join("schemas")
}

pub fn versioned_schema_path(migrations_dir: &Path, version: u32) -> PathBuf {
    versioned_schema_dir(migrations_dir).join(format!("v{version}.forge"))
}

pub fn save_versioned_schema(
    migrations_dir: &Path,
    version: u32,
    schema_src: &str,
) -> Result<(), String> {
    let dir = versioned_schema_dir(migrations_dir);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create versioned-schema dir {:?}: {}", dir, e))?;
    let path = versioned_schema_path(migrations_dir, version);
    fs::write(&path, schema_src)
        .map_err(|e| format!("Failed to write versioned schema {:?}: {}", path, e))
}

pub fn load_versioned_schema(migrations_dir: &Path, version: u32) -> Result<String, String> {
    let path = versioned_schema_path(migrations_dir, version);
    fs::read_to_string(&path).map_err(|e| {
        format!(
            "No committed schema snapshot for format v{version} ({:?}): {}. \
             The transformer needs every version's `.forge` in its range; \
             re-run `forgedb migrate create` so the lineage records them.",
            path, e
        )
    })
}

pub fn scaffold_authored_body(
    migrations_dir: &Path,
    migration: &Migration,
) -> Result<Option<(PathBuf, bool)>, String> {
    let authored = migration.authored_changes();
    if authored.is_empty() {
        return Ok(None);
    }

    let dir = migration_body_dir(migrations_dir, &migration.id);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create migration body dir {:?}: {}", dir, e))?;
    let path = authored_body_path(migrations_dir, &migration.id);
    if path.exists() {
        return Ok(Some((path, false)));
    }

    let mut by_model: std::collections::BTreeMap<&str, Vec<String>> =
        std::collections::BTreeMap::new();
    for change in &authored {
        by_model
            .entry(change.target_model())
            .or_default()
            .push(change.description());
    }

    let mut stub = String::new();
    stub.push_str(&format!(
        "// Authored transform for migration {} (format v{} -> v{}).\n",
        migration.id, migration.from_version, migration.to_version
    ));
    stub.push_str(
        "//\n\
         // The ForgeDB differ could not PROVE a new-row value for the hop(s) below\n\
         // from the schema diff alone, so YOU must author the transform.  The\n\
         // transformer crate (`forgedb generate transform`) embeds this file verbatim\n\
         // and calls `authored_transform` for EVERY row of EVERY model in this hop,\n\
         // AFTER the automatic (additive / rename / drop) field ops have been applied.\n\
         // `row` is the record as JSON; return it reshaped into the NEXT version.  A\n\
         // model/row you don't need to touch: return `row` unchanged.\n//\n\
         // Fill in every TODO, then this migration is ready to `forgedb migrate build`.\n\n",
    );
    stub.push_str(
        "pub fn authored_transform(model: &str, mut row: serde_json::Value) -> serde_json::Value {\n\
         \x20   match model {\n",
    );
    for (model, todos) in &by_model {
        stub.push_str(&format!("        {:?} => {{\n", model));
        for todo in todos {
            stub.push_str(&format!("            // TODO: {}\n", todo));
        }
        stub.push_str("            // e.g. re-encode a changed field:\n");
        stub.push_str(
            "            // if let Some(v) = row.get(\"<field>\").and_then(|x| x.as_u64()) {\n\
             \x20           //     row[\"<field>\"] = serde_json::Value::String(v.to_string());\n\
             \x20           // }\n",
        );
        stub.push_str("            row\n");
        stub.push_str("        }\n");
    }
    stub.push_str("        _ => row,\n    }\n}\n");

    fs::write(&path, stub)
        .map_err(|e| format!("Failed to write authored-body scaffold {:?}: {}", path, e))?;
    Ok(Some((path, true)))
}

pub fn current_schema_version<P: AsRef<Path>>(migrations_dir: P) -> u32 {
    MigrationLineage::load(migrations_dir)
        .map(|l| l.current_schema_version())
        .unwrap_or(BASELINE_SCHEMA_VERSION)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Unanswered {
    NoAnswer {
        change: String,
    },
    EscapeFileMissing { path: PathBuf, change: String },
    EscapeFileUnedited { path: PathBuf, change: String },
    LegacyBodyMissing { path: PathBuf },
}

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unanswered::NoAnswer { change } => write!(
                f,
                "{change}\n    no answer was recorded. Re-run `forgedb migrate create` \
                 for this hop, or answer it there when it is first detected."
            ),
            Unanswered::EscapeFileMissing { path, change } => write!(
                f,
                "{change}\n    its transform {} is missing.",
                path.display()
            ),
            Unanswered::EscapeFileUnedited { path, change } => write!(
                f,
                "{change}\n    {} is byte-identical to the scaffold ForgeDB wrote, \
                 so nothing has been authored yet.",
                path.display()
            ),
            Unanswered::LegacyBodyMissing { path } => write!(
                f,
                "this migration predates #374 and has authored residue, but its \
                 transform {} is missing.",
                path.display()
            ),
        }
    }
}

pub fn hop_answer_status(
    migrations_dir: &Path,
    migration: &Migration,
) -> Result<(), Vec<Unanswered>> {
    let authored = migration.authored_changes();
    if authored.is_empty() {
        return Ok(());
    }

    if migration.record_version == 0 {
        let path = authored_body_path(migrations_dir, &migration.id);
        return if path.exists() {
            Ok(())
        } else {
            Err(vec![Unanswered::LegacyBodyMissing { path }])
        };
    }

    let mut problems = Vec::new();
    for change in authored {
        let description = change.description();
        match change.answer() {
            None => problems.push(Unanswered::NoAnswer {
                change: description,
            }),
            Some(Answer::Escape {
                file,
                scaffold_checksum,
                ..
            }) => {
                let path = migration_body_dir(migrations_dir, &migration.id).join(file);
                match fs::read(&path) {
                    Err(_) => problems.push(Unanswered::EscapeFileMissing {
                        path,
                        change: description,
                    }),
                    Ok(bytes) if &checksum::compute(&bytes) == scaffold_checksum => problems
                        .push(Unanswered::EscapeFileUnedited {
                            path,
                            change: description,
                        }),
                    Ok(_) => {}
                }
            }
            Some(Answer::Constant { .. } | Answer::CopyField { .. }) => {}
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

pub fn escape_body_path(
    migrations_dir: &Path,
    migration_id: &str,
    lang: EscapeLanguage,
) -> PathBuf {
    migration_body_dir(migrations_dir, migration_id).join(lang.transform_file())
}
