//! Migration lineage (#74 Phase 2).
//!
//! The committed migration records under `migrations/` form an **ordered,
//! serial version lineage**: the fresh-database baseline is format version `1`,
//! and every recorded migration bumps the version by one (`from_version ->
//! to_version`).  This module is the dev-time source of truth two consumers walk:
//!
//! - **`EXPECTED_SCHEMA_VERSION` (Phase 1).** codegen bakes the *current* version
//!   — the highest `to_version` in the lineage — into the generated app so open
//!   refuses a stale data dir (red line #8: lineage-sourced, never hand-edited).
//! - **The transformer generator (Phase 3).** given a `--from B --to G` range it
//!   [`expand_range`](MigrationLineage::expand_range)s the lineage into the ordered
//!   hop sequence between those versions, pulling each hop's frozen body (C1/C13).
//!
//! Nothing here runs at app time; it is `migrate create`/`generate`-time only.

use crate::generator::MigrationGenerator;
use crate::types::{Answer, EscapeLanguage, Migration, checksum};
use std::fs;
use std::path::{Path, PathBuf};

/// The fresh-database baseline format version (no migrations applied yet).  Kept
/// in agreement with codegen's `EXPECTED_SCHEMA_VERSION` baseline (#74 Phase 1).
pub const BASELINE_SCHEMA_VERSION: u32 = 1;

/// The ordered serial version lineage loaded from a `migrations/` directory.
#[derive(Debug, Clone)]
pub struct MigrationLineage {
    /// Migrations sorted in application order (by `to_version`, then id).
    migrations: Vec<Migration>,
}

impl MigrationLineage {
    /// Load and order every migration record under `migrations_dir`.  An absent
    /// directory is an empty lineage (a brand-new project at the baseline).
    pub fn load<P: AsRef<Path>>(migrations_dir: P) -> Result<Self, String> {
        let mut migrations = MigrationGenerator::load_all_migrations(migrations_dir)?;
        // `load_all_migrations` already sorts by id (timestamp == creation order).
        // Version order follows creation order; sort by (to_version, id) so a
        // lineage that stored explicit versions is walked by version, while a
        // legacy lineage (versions 0) falls back to id order.
        migrations.sort_by(|a, b| {
            a.to_version
                .cmp(&b.to_version)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(Self { migrations })
    }

    /// The migrations in application order.
    pub fn migrations(&self) -> &[Migration] {
        &self.migrations
    }

    /// The current on-disk schema serial the whole database is expected to be
    /// at: the highest `to_version` in the lineage, or the baseline when empty.
    /// This is exactly what codegen bakes into `EXPECTED_SCHEMA_VERSION`.
    pub fn current_schema_version(&self) -> u32 {
        self.migrations
            .iter()
            .map(|m| m.to_version)
            .max()
            .filter(|&v| v != 0)
            .unwrap_or(BASELINE_SCHEMA_VERSION)
    }

    /// The `(from_version, to_version)` a *new* migration created now would carry:
    /// it starts at the current version and bumps by one.
    pub fn next_version_span(&self) -> (u32, u32) {
        let from = self.current_schema_version();
        (from, from + 1)
    }

    /// Expand a `--from`/`--to` version range into the ordered migration
    /// subsequence that bridges it (#74 Phase 3 input; C1 — a closed, compile-fixed
    /// serial range, not a runtime-interpreted plan).  The range must be
    /// **contiguous and fully covered** by recorded migrations: every version step
    /// from `from` to `to` has a migration whose `from_version -> to_version`
    /// matches, or this returns an error (never a synthesized direct jump).
    pub fn expand_range(&self, from: u32, to: u32) -> Result<Vec<Migration>, String> {
        if to < from {
            return Err(format!(
                "invalid migration range: --to {to} is below --from {from}"
            ));
        }
        // An empty range (from == to) is a valid no-op (nothing to replay).
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

/// Directory holding a migration's authored transform bodies (#74 Phase 2/3):
/// `migrations/{id}/`.
pub fn migration_body_dir(migrations_dir: &Path, migration_id: &str) -> PathBuf {
    migrations_dir.join(migration_id)
}

/// The frozen authored-transform source for a migration (#74 Phase 2/3, C13):
/// `migrations/{id}/transform.rs`.  The transformer generator embeds this file
/// verbatim; it is never re-synthesized.
pub fn authored_body_path(migrations_dir: &Path, migration_id: &str) -> PathBuf {
    migration_body_dir(migrations_dir, migration_id).join("transform.rs")
}

/// The directory holding the committed **full-schema snapshots** per version
/// (#74 Phase 3): `migrations/schemas/`.  The transformer generator loads
/// `v{n}.forge` for every version in its `--from`/`--to` range to emit that
/// version's typed structs + reader/writer (C1 — a closed compile-fixed range,
/// no runtime schema construction).
pub fn versioned_schema_dir(migrations_dir: &Path) -> PathBuf {
    migrations_dir.join("schemas")
}

/// Path of the committed full-schema snapshot for one schema version (#74
/// Phase 3): `migrations/schemas/v{version}.forge`.  Stored as raw `.forge` so it
/// re-parses through the same grammar the app schema does.
pub fn versioned_schema_path(migrations_dir: &Path, version: u32) -> PathBuf {
    versioned_schema_dir(migrations_dir).join(format!("v{version}.forge"))
}

/// Persist the full `.forge` source as the snapshot for `version` (#74 Phase 3).
/// Written by `migrate create` for both the baseline (`v1`) and each new hop's
/// destination version, so the lineage is self-describing and the transformer
/// never has to reconstruct an intermediate schema from diffs.
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

/// Load the committed full-schema `.forge` source for `version` (#74 Phase 3).
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

/// Scaffold the authored-transform source for a migration that carries at least
/// one [`HopBodyClass::Authored`](crate::HopBodyClass::Authored) hop (#74 Phase
/// 2, C8/C9/C13).
///
/// Writes `migrations/{id}/transform.rs` with a documented stub **only if it does
/// not already exist** — an authored body, once written and frozen, is never
/// clobbered (the developer's semantic transform is authoritative).  Returns the
/// path (whether newly written or pre-existing) and whether it was created.
pub fn scaffold_authored_body(
    migrations_dir: &Path,
    migration: &Migration,
) -> Result<Option<(PathBuf, bool)>, String> {
    let authored = migration.authored_changes();
    if authored.is_empty() {
        // Fully automatic migration — no body to author.
        return Ok(None);
    }

    let dir = migration_body_dir(migrations_dir, &migration.id);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create migration body dir {:?}: {}", dir, e))?;
    let path = authored_body_path(migrations_dir, &migration.id);
    if path.exists() {
        // Never clobber a frozen authored body.
        return Ok(Some((path, false)));
    }

    // Group the authored residue by model so each gets one `match` arm.
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

/// Convenience: the current expected format version for a `migrations/` directory
/// (loads the lineage, returns its current version — the baseline when empty or
/// absent).  This is what the `generate` command threads into codegen.
pub fn current_schema_version<P: AsRef<Path>>(migrations_dir: P) -> u32 {
    MigrationLineage::load(migrations_dir)
        .map(|l| l.current_schema_version())
        .unwrap_or(BASELINE_SCHEMA_VERSION)
}

/// Why a hop cannot be built (#374 step 6).
///
/// `Ok(())` from [`hop_answer_status`] is the ONLY state that admits a build.
#[derive(Debug, Clone, PartialEq)]
pub enum Unanswered {
    /// The record carries no answer for a change that needs one.
    NoAnswer {
        /// The change's own description, so the operator reads the same
        /// sentence `migrate create` printed.
        change: String,
    },
    /// [`Answer::Escape`] names a file that is not there.
    EscapeFileMissing { path: PathBuf, change: String },
    /// The file is byte-identical to the scaffold ForgeDB wrote, so nothing was
    /// authored.
    EscapeFileUnedited { path: PathBuf, change: String },
    /// A pre-#374 record with authored residue and no body on disk.
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

/// Can this hop be built? (#374 step 6, gate 1 decision 4.)
///
/// Decided from the **record** plus the recorded scaffold hash. Never from the
/// file's text: a `TODO` grep is satisfied by deleting a comment, which is the
/// reason gate 1 forbade it.
///
/// Three rules, in the order they apply:
///
/// 1. **`record_version == 0`** — a record written before #374, which could not
///    have recorded an answer. The legacy rule applies instead: `transform.rs`
///    must exist, with no hash to compare it against. Written first, not last:
///    every lineage already committed is in this state, and skipping this rule
///    refuses all of them with a message about an answer that could never have
///    been recorded.
/// 2. **`answer == None`** on an `Authored` change — `NoAnswer`. That is the
///    whole decision for `Constant` and `CopyField`: the answer is *in* the
///    record, so nothing on disk is consulted.
/// 3. **`Answer::Escape`** — the file must exist, and its bytes must differ
///    from the recorded `scaffold_checksum`.
///
/// # The honest limit of rule 3
///
/// Hash equality proves **untouched**. It cannot prove **answered**. An author
/// who deletes the `// TODO:` lines and changes nothing else passes this check
/// — which is exactly the case a `TODO` grep also gets wrong, in the opposite
/// direction. What covers it is the *other* half of the same pair: with the
/// defensive type-zero gone, that hop's rows reach the destination decode
/// without the required key and the hop **fails, naming the field**, instead of
/// writing `""` and exiting 0. Decisions 4 and 5 are one mechanism; do not land
/// one and describe it as the guarantee of both.
pub fn hop_answer_status(
    migrations_dir: &Path,
    migration: &Migration,
) -> Result<(), Vec<Unanswered>> {
    let authored = migration.authored_changes();
    if authored.is_empty() {
        return Ok(());
    }

    // Rule 1 — the legacy arm.
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
            // Rule 2.
            None => problems.push(Unanswered::NoAnswer {
                change: description,
            }),
            // Rule 3.
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
            // The answer is in the record; nothing on disk is consulted.
            Some(Answer::Constant { .. } | Answer::CopyField { .. }) => {}
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// The path of an escape transform inside a migration's body dir (#374).
pub fn escape_body_path(
    migrations_dir: &Path,
    migration_id: &str,
    lang: EscapeLanguage,
) -> PathBuf {
    migration_body_dir(migrations_dir, migration_id).join(lang.transform_file())
}
