pub mod answers;
pub mod escape;

use crate::naming::PackageKind;
use crate::{error::CliError, ui, Result};
use colored::Colorize;
use forgedb_migrations::{HopBodyClass, MigrationGenerator, MigrationTracker, SchemaChange};
use std::path::{Path, PathBuf};

fn map_err(e: String) -> CliError {
    CliError::Migration(e)
}

pub struct AppRef {
    pub schema: PathBuf,
    pub config: Option<String>,
}

pub struct MigrateCreateOptions {
    pub app: AppRef,
    pub description: String,
    pub removed_auto: bool,
    pub no_auto: bool,
}

pub struct MigrateStatusOptions {
    pub app: AppRef,
}

pub struct MigrateBuildOptions {
    pub app: AppRef,
    pub from: u32,
    pub to: u32,
    pub removed_output: Option<PathBuf>,
}

pub struct MigrateRunOptions {
    pub app: AppRef,
    pub from: u32,
    pub to: u32,
    pub src: PathBuf,
    pub dest: PathBuf,
    pub removed_bin_dir: Option<PathBuf>,
}

pub struct MigrateEngineOptions {
    pub app: AppRef,
    pub src: PathBuf,
    pub dest: PathBuf,
    pub removed_output: Option<PathBuf>,
}

struct ResolvedApp {
    schema: PathBuf,
    migrations_dir: PathBuf,
    app_name: String,
    reserved: crate::cache::Reserved,
    project_root: PathBuf,
    governing: crate::config::ForgeConfig,
}

impl ResolvedApp {
    fn package(&self, kind: &PackageKind) -> String {
        crate::naming::package_name(&self.app_name, kind)
    }

    fn bin(&self, kind: &PackageKind) -> String {
        crate::naming::bin_name(&self.app_name, kind)
    }

    fn member(&self, kind: &PackageKind) -> PathBuf {
        self.reserved.container.join(kind.dir())
    }
}

fn resolve_app(app: &AppRef) -> Result<ResolvedApp> {
    let schema = &app.schema;
    if !schema.is_file() {
        return Err(CliError::SchemaNotFound(format!(
            "{} does not exist. `--schema` names the app this migration belongs to; \
             it is not discovered and there is no default.",
            schema.display()
        )));
    }

    let governing_chain = crate::project::govern_for_schema(app.config.as_deref(), schema)?;
    let project = governing_chain.identify_reported()?;
    let symbol_naming = governing_chain.symbol_naming();
    let governing = governing_chain.into_config();
    let reserved = crate::cache::reserve(&project.name, &project.root, schema, symbol_naming)?;
    ui::info(&format!("Build cache: {}", reserved.container.display()));

    Ok(ResolvedApp {
        schema: schema.clone(),
        migrations_dir: crate::project::migrations_dir(schema),
        app_name: reserved.app_name.clone(),
        reserved,
        project_root: project.root.clone(),
        governing,
    })
}

fn refuse_removed_flag(flag: &str, given: Option<String>, explanation: &str) -> Result<()> {
    match given {
        None => Ok(()),
        Some(v) => {
            let shown = if v.is_empty() {
                flag.to_string()
            } else {
                format!("{flag} {v}")
            };
            Err(CliError::Migration(format!("`{shown}` was removed. {explanation}")))
        }
    }
}

fn cache_owns_the_build(replacement: &str) -> String {
    format!(
        "ForgeDB owns where this is built, and it is built as a member of the \
         project's own cache workspace rather than into a directory of your \
         choosing (#335). {replacement}"
    )
}

pub fn up() -> Result<()> {
    Err(CliError::Migration(
        "`forgedb migrate up` was removed (#335). It was a wrapper over two commands, \
         so run them:\n  \
         forgedb migrate build --schema <app.forge> --from <F> --to <T>\n  \
         forgedb migrate run   --schema <app.forge> --from <F> --to <T> \
         --src <data> --dest <migrated>\n\
         The per-tenant sweep (--tenant-root) and --from auto-detection are tracked \
         for restoration as #373."
            .to_string(),
    ))
}

pub fn create(opts: MigrateCreateOptions) -> Result<()> {
    refuse_removed_flag(
        "--auto",
        opts.removed_auto.then(String::new),
        "Detecting schema changes is what `migrate create` DOES (#374), so there is no \
         flag for it and no second mode to choose — drop it. To opt out of the \
         *prompt*, and only the prompt, pass `--no-auto`: detection still runs, and a \
         change ForgeDB cannot prove becomes a hard error naming itself instead of a \
         question.",
    )?;

    let app = resolve_app(&opts.app)?;
    let migrations_dir = app.migrations_dir.clone();
    let schema_path = app.schema.clone();

    ui::info(&format!(
        "Detecting schema changes ({})...",
        schema_path.display()
    ));

    let new_src = std::fs::read_to_string(&schema_path).map_err(|e| {
        CliError::Migration(format!(
            "Failed to read schema '{}': {}",
            schema_path.display(),
            e
        ))
    })?;
    let dest_schema = forgedb_parser::Parser::new(&new_src)
        .and_then(|mut p| p.parse())
        .map_err(|e| CliError::Migration(format!("Failed to parse schema: {}", e)))?;

    let detected = detect_schema_changes(&migrations_dir, &new_src)?;
    let mut changes = detected.changes;
    let rename_proposals = detected.rename_proposals;

    if changes.is_empty() {
        save_schema_snapshot(&migrations_dir, &new_src)?;
        let lineage =
            forgedb_migrations::MigrationLineage::load(&migrations_dir).map_err(map_err)?;
        forgedb_migrations::save_versioned_schema(
            &migrations_dir,
            lineage.current_schema_version(),
            &new_src,
        )
        .map_err(map_err)?;
        ui::success("No schema changes detected (snapshot up to date)");
        return Ok(());
    }

    let lineage = forgedb_migrations::MigrationLineage::load(&migrations_dir).map_err(map_err)?;
    let (from_version, to_version) = lineage.next_version_span();

    let migration_id = forgedb_migrations::Migration::next_id();

    let governing = crate::project::govern(
        opts.app.config.as_deref(),
        crate::project::schema_dir(&schema_path),
    )?;
    let (internal_targets, _) = governing.config().resolved_targets()?;
    let escape_language = escape::language_for(&internal_targets);

    let mut widget = if opts.no_auto {
        None
    } else {
        crate::ask::prompt()
    };
    let reason = if opts.no_auto {
        "--no-auto"
    } else {
        crate::ask::Askability::detect().reason()
    };
    answers::resolve_rename_proposals(
        &rename_proposals,
        &mut changes,
        widget.as_deref_mut(),
    )?;
    ui::info(&format!("Detected {} change(s)", changes.len()));
    for change in &changes {
        let marker = if change.hop_body_class() == HopBodyClass::Authored {
            " [needs an answer]".yellow()
        } else {
            "".normal()
        };
        println!("  • {}{}", change.description(), marker);
    }

    let scaffold = answers::resolve_answers(
        &mut changes,
        &dest_schema,
        escape_language,
        &migrations_dir,
        &migration_id,
        widget.as_deref_mut(),
        reason,
        (from_version, to_version),
    )?;

    let has_breaking = changes.iter().any(|c| c.is_breaking());
    let authored_count = changes
        .iter()
        .filter(|c| c.hop_body_class() == HopBodyClass::Authored)
        .count();

    let migration = MigrationGenerator::write_migration(
        &migrations_dir,
        forgedb_migrations::Migration::with_id(
            migration_id,
            opts.description,
            changes,
            from_version,
            to_version,
        ),
    )
    .map_err(map_err)?;
    save_schema_snapshot(&migrations_dir, &new_src)?;
    forgedb_migrations::save_versioned_schema(&migrations_dir, to_version, &new_src)
        .map_err(map_err)?;

    ui::success(&format!(
        "Created migration: {} (format v{} → v{})",
        migration.filename(),
        from_version,
        to_version
    ));
    println!("\n{}", MigrationGenerator::generate_report(&migration));

    if scaffold.is_some() {
        let from_schema = forgedb_migrations::load_versioned_schema(&migrations_dir, from_version)
            .map_err(map_err)
            .and_then(|src| {
                forgedb_parser::Parser::new(&src)
                    .and_then(|mut p| p.parse())
                    .map_err(|e| {
                        CliError::Migration(format!("committed schema v{from_version}: {e}"))
                    })
            })?;
        escape::write_support_files(
            &migrations_dir,
            &migration.id,
            escape_language,
            &[(from_version, &from_schema), (to_version, &dest_schema)],
        )?;
    }

    if let Some(path) = &scaffold {
        ui::warning(&format!(
            "Write your transform in {} — `forgedb migrate build` refuses this hop \
             until its bytes differ from the scaffold ForgeDB just wrote.",
            path.display()
        ));
    }

    if has_breaking || authored_count > 0 {
        print_migration_next_steps(&app.schema, from_version, to_version, scaffold.is_some());
    } else {
        ui::info(
            "Every change in this hop is provable, so no transform has to be written — \
             but the on-disk format version still moved, and a regenerated app refuses a \
             data dir that has not been migrated.",
        );
        print_migration_next_steps(&app.schema, from_version, to_version, false);
    }

    Ok(())
}

pub fn status(opts: MigrateStatusOptions) -> Result<()> {
    let app = resolve_app(&opts.app)?;
    let migrations_dir = app.migrations_dir.clone();

    let all_migrations =
        MigrationGenerator::load_all_migrations(&migrations_dir).map_err(map_err)?;

    if all_migrations.is_empty() {
        ui::info("No migrations found");
        return Ok(());
    }

    let tracker = MigrationTracker::new(&migrations_dir).map_err(map_err)?;

    println!("{}", "Migration Status".bold());
    println!("{}", "=".repeat(60));
    println!("{}\n", tracker.status_summary(all_migrations.len()));

    for migration in &all_migrations {
        let is_applied = tracker.is_applied(&migration.id);
        let status = if is_applied {
            "✓".green()
        } else {
            "○".yellow()
        };

        let breaking_marker = if migration.has_breaking_changes() {
            " ⚠️ ".red()
        } else {
            "".normal()
        };

        println!(
            "{} {} - {}{} ({} changes)",
            status,
            migration.id.cyan(),
            migration.description,
            breaking_marker,
            migration.changes.len()
        );

        if is_applied {
            if let Some(record) = tracker
                .applied_migrations()
                .iter()
                .find(|r| r.migration_id == migration.id)
            {
                println!(
                    "   Applied at: {}",
                    record
                        .applied_at
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                        .dimmed()
                );
            }
        }
    }

    Ok(())
}

pub fn build(opts: MigrateBuildOptions) -> Result<()> {
    refuse_removed_flag(
        "--output",
        opts.removed_output.as_ref().map(|p| p.display().to_string()),
        &cache_owns_the_build(
            "Pass `--schema <app.forge>` instead: the transformer is emitted into that app's \
             build-cache container as `transform-<from>-<to>/`, and the command prints the path.",
        ),
    )?;
    let app = resolve_app(&opts.app)?;
    let kind = PackageKind::Transform {
        from: opts.from,
        to: opts.to,
    };

    ui::detail(&format!(
        "--schema selects the app; the transformer itself is generated from the committed \
         per-version schemas under {}",
        app.migrations_dir.display()
    ));

    emit_transform_crate(&app, &kind)?;
    sync_cache_root(&app)?;
    let bin = build_member_bin(&app, &kind, "transformer")?;

    ui::success(&format!("Built transformer: {}", bin.display()));
    ui::info(&format!(
        "Run it with the app STOPPED: `forgedb migrate run --schema {} --from {} --to {} \
         --src <data> --dest <migrated>` (or directly: `{} <src> <dest>`)",
        app.schema.display(),
        opts.from,
        opts.to,
        bin.display()
    ));
    Ok(())
}

fn sync_cache_root(app: &ResolvedApp) -> Result<()> {
    let synced = crate::cache::sync_root(&app.reserved.project, &app.reserved.container)?;
    if synced.lock_dropped {
        ui::info(&format!(
            "CLI version changed — dropped {}/Cargo.lock so dependencies re-resolve",
            app.reserved.project.display()
        ));
    }
    for orphan in &synced.orphans {
        ui::detail(&format!(
            "Orphaned member (its schema is gone): {}",
            orphan.display()
        ));
    }
    Ok(())
}

fn build_member_bin(app: &ResolvedApp, kind: &PackageKind, what: &str) -> Result<PathBuf> {
    use crate::commands::build::driver::{self, Selected, TargetKind};

    let package = app.package(kind);
    let member = app.member(kind);

    driver::assert_no_duplicate_artifact_names(&app.reserved.project)?;

    let artifacts = driver::execute(&driver::plan(
        &app.reserved.project,
        &[Selected {
            package: package.clone(),
            kind: kind.clone(),
        }],
        true,
    ))?;

    artifacts
        .into_iter()
        .find(|a| a.package == package && a.kind == TargetKind::Bin)
        .map(|a| a.path)
        .ok_or_else(|| {
            CliError::Migration(format!(
                "the {what} package `{package}` built, but cargo reported no `bin` \
                 artifact for it. Its manifest is at {}.",
                member.join("Cargo.toml").display()
            ))
        })
}

fn locate_member_bin(app: &ResolvedApp, kind: &PackageKind, what: &str) -> Result<PathBuf> {
    let member = app.member(kind);
    let bin_name = app.bin(kind);

    if !member.join("Cargo.toml").is_file() {
        return Err(CliError::Migration(format!(
            "no {what} for this range at {} — nothing has emitted it. Run \
             `forgedb migrate build --schema {} --from <F> --to <T>` first.",
            member.display(),
            app.schema.display()
        )));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(dir) = crate::commands::build::driver::target_directory(&app.reserved.project) {
        candidates.push(dir.join("release").join(&bin_name));
    }

    let local = app
        .reserved
        .project
        .join("target")
        .join("release")
        .join(&bin_name);
    if !candidates.contains(&local) {
        candidates.push(local);
    }

    if let Some(hit) = candidates.iter().find(|p| p.is_file()) {
        return Ok(hit.clone());
    }
    Err(CliError::Migration(format!(
        "{what} bin `{bin_name}` not found — its package is at {}, and the binary was \
         looked for in:\n{}\nRun `forgedb migrate build --schema {} --from <F> --to <T>` \
         first.",
        member.display(),
        candidates
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n"),
        app.schema.display()
    )))
}

fn run_transformer(bin: &Path, src: &Path, dest: &Path) -> Result<()> {
    let status = std::process::Command::new(bin)
        .arg(src)
        .arg(dest)
        .status()
        .map_err(|e| CliError::Migration(format!("failed to run transformer: {e}")))?;
    if !status.success() {
        return Err(CliError::Migration(
            "transformer exited non-zero (see output above); the source dir is unchanged"
                .to_string(),
        ));
    }
    Ok(())
}

pub fn run(opts: MigrateRunOptions) -> Result<()> {
    refuse_removed_flag(
        "--bin-dir",
        opts.removed_bin_dir.as_ref().map(|p| p.display().to_string()),
        &cache_owns_the_build(
            "Pass `--schema <app.forge> --from <F> --to <T>` instead: those name the exact \
             cache member `migrate build` produced, which a directory alone cannot.",
        ),
    )?;
    let app = resolve_app(&opts.app)?;
    let kind = PackageKind::Transform {
        from: opts.from,
        to: opts.to,
    };
    let bin = locate_member_bin(&app, &kind, "transformer")?;

    ui::info(&format!(
        "Migrating {} → {} via {}",
        opts.src.display(),
        opts.dest.display(),
        bin.display()
    ));
    run_transformer(&bin, &opts.src, &opts.dest)?;
    ui::success("Migration complete — point the regenerated app at the destination dir");
    Ok(())
}

pub fn engine(opts: MigrateEngineOptions) -> Result<()> {
    refuse_removed_flag(
        "--output",
        opts.removed_output.as_ref().map(|p| p.display().to_string()),
        &cache_owns_the_build(
            "Pass `--schema <app.forge>` instead: the hop crate is emitted into that app's \
             build-cache container as `engine-<from>-<to>/`, and the command prints the path.",
        ),
    )?;

    let Some((schema_version, from_engine)) = detect_src_versions(&opts.src)? else {
        return Err(CliError::Migration(format!(
            "{} does not look like a ForgeDB data dir (no <model>/manifest.json)",
            opts.src.display()
        )));
    };
    let to_engine = forgedb_codegen::rust::CURRENT_ENGINE_VERSION;

    if from_engine == to_engine {
        ui::success(&format!(
            "{} is already at engine generation {to_engine} — nothing to migrate.",
            opts.src.display()
        ));
        return Ok(());
    }
    if from_engine > to_engine {
        return Err(CliError::Migration(format!(
            "{} is stamped at engine generation {from_engine}, newer than this build's {to_engine} \
             — upgrade the ForgeDB CLI rather than migrating backwards",
            opts.src.display()
        )));
    }

    let app = resolve_app(&opts.app)?;
    let kind = PackageKind::Engine {
        from: from_engine,
        to: to_engine,
    };

    ui::info(&format!(
        "Engine migration: {} (schema v{schema_version}, engine generation {from_engine} → {to_engine})",
        opts.src.display()
    ));

    emit_engine_crate(&app, &kind, schema_version)?;
    sync_cache_root(&app)?;
    let bin = build_member_bin(&app, &kind, "engine-hop")?;

    ui::info(&format!(
        "Migrating {} → {} (the source dir is left untouched — it is your rollback)",
        opts.src.display(),
        opts.dest.display()
    ));
    run_transformer(&bin, &opts.src, &opts.dest)?;
    ui::success("Engine migration complete — point the regenerated app at the destination dir");
    Ok(())
}

fn emit_engine_crate(app: &ResolvedApp, kind: &PackageKind, schema_version: u32) -> Result<()> {
    use forgedb_codegen::{EngineHopPlan, EngineMigrationGenerator};

    let (from_engine, to_engine) = match kind {
        PackageKind::Engine { from, to } => (*from, *to),
        other => {
            return Err(CliError::Migration(format!(
                "internal error: the engine hop was asked to emit a `{}` package",
                other.dir()
            )));
        }
    };

    let src = std::fs::read_to_string(&app.schema)
        .map_err(|e| CliError::SchemaNotFound(format!("{}: {}", app.schema.display(), e)))?;
    let parsed = forgedb_parser::Parser::new(&src)
        .and_then(|mut p| p.parse())
        .map_err(|e| CliError::SchemaValidation(format!("{}: {e}", app.schema.display())))?;

    let plan = EngineHopPlan {
        schema: &parsed,
        schema_version,
        from_engine,
        to_engine,
    };
    let package = app.package(kind);
    let crate_out = EngineMigrationGenerator::generate(&plan, &package)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;

    write_class_c_member(app, kind, &crate_out)?;
    Ok(())
}

fn write_class_c_member(
    app: &ResolvedApp,
    kind: &PackageKind,
    crate_out: &forgedb_codegen::TransformCrate,
) -> Result<()> {
    let member = app.member(kind);
    std::fs::create_dir_all(&member)?;
    std::fs::write(member.join("Cargo.toml"), &crate_out.cargo_toml)?;
    for (rel, content) in &crate_out.sources {
        let path = member.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    ui::success(&format!(
        "Generated {} ({} source files) at {}",
        crate_out.crate_name,
        crate_out.sources.len(),
        member.display()
    ));
    Ok(())
}

fn detect_src_versions(data_dir: &Path) -> Result<Option<(u32, u32)>> {
    if !data_dir.is_dir() {
        return Ok(None);
    }
    let entries = std::fs::read_dir(data_dir)
        .map_err(|e| CliError::Migration(format!("failed to read {}: {}", data_dir.display(), e)))?;
    for entry in entries {
        let manifest = entry
            .map_err(|e| CliError::Migration(e.to_string()))?
            .path()
            .join("manifest.json");
        if manifest.is_file() {
            let txt = std::fs::read_to_string(&manifest).map_err(|e| {
                CliError::Migration(format!("failed to read {}: {}", manifest.display(), e))
            })?;
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                if let Some(fv) = v.get("format_version").and_then(|x| x.as_u64()) {
                    let ev = v
                        .get("engine_version")
                        .and_then(|x| x.as_u64())
                        .unwrap_or(1);
                    return Ok(Some((fv as u32, ev as u32)));
                }
            }
        }
    }
    Ok(None)
}

pub fn emit_transform(from: u32, to: u32, _output: &Path, _force: bool) -> Result<()> {
    Err(CliError::Migration(format!(
        "`forgedb generate transform` was removed (#335): the transformer is a member of \
         ForgeDB's build cache now, not a crate emitted into a directory you choose. \
         Run `forgedb migrate build --schema <app.forge> --from {from} --to {to}` instead \
         — it prints the cache path it wrote to."
    )))
}

fn refuse_unanswered_hops(
    migrations_dir: &Path,
    hops: &[forgedb_migrations::Migration],
) -> Result<()> {
    let mut lines = Vec::new();
    for m in hops {
        if m.record_version == 0 && !m.authored_changes().is_empty() {
            ui::warning(&format!(
                "Migration {} predates the answer format; its transform is read from \
                 disk with no recorded answer to check it against. The next \
                 `forgedb migrate create` records answers for the hops it detects.",
                m.id
            ));
        }
        if let Err(problems) = forgedb_migrations::hop_answer_status(migrations_dir, m) {
            for p in problems {
                lines.push(format!("  • [{} v{}→v{}] {p}", m.id, m.from_version, m.to_version));
            }
        }
    }
    if lines.is_empty() {
        return Ok(());
    }
    Err(CliError::Migration(format!(
        "this range cannot be built — {} change(s) in it have no answer:\n{}\n\n\
         An answer is recorded at `migrate create` time, when you have the change in \
         your head; it is not something `migrate build` can infer. Nothing was written \
         to the build cache.",
        lines.len(),
        lines.join("\n"),
    )))
}

fn emit_transform_crate(app: &ResolvedApp, kind: &PackageKind) -> Result<()> {
    use forgedb_codegen::{HopPlan, TransformGenerator, TransformPlan, VersionSchema};
    use forgedb_migrations::MigrationLineage;

    let (from, to) = match kind {
        PackageKind::Transform { from, to } => (*from, *to),
        other => {
            return Err(CliError::Migration(format!(
                "internal error: the transformer was asked to emit a `{}` package",
                other.dir()
            )));
        }
    };

    let migrations_dir = app.migrations_dir.clone();
    let lineage = MigrationLineage::load(&migrations_dir).map_err(map_err)?;
    let hop_migrations = lineage.expand_range(from, to).map_err(map_err)?;
    if hop_migrations.is_empty() {
        return Err(CliError::Migration(format!(
            "empty migration range (v{from} → v{to}): nothing to transform"
        )));
    }

    refuse_unanswered_hops(&migrations_dir, &hop_migrations)?;

    let mut parsed: Vec<(u32, forgedb_parser::Schema)> = Vec::new();
    for v in from..=to {
        let src = forgedb_migrations::load_versioned_schema(&migrations_dir, v).map_err(map_err)?;
        let schema = forgedb_parser::Parser::new(&src)
            .and_then(|mut p| p.parse())
            .map_err(|e| {
                CliError::Migration(format!("failed to parse committed schema v{v}: {e}"))
            })?;
        parsed.push((v, schema));
    }

    let member = app.member(kind);
    let mut escape_sources: Vec<(String, String)> = Vec::new();

    let mut hops = Vec::new();
    for m in &hop_migrations {
        let dest_schema = &parsed
            .iter()
            .find(|(v, _)| *v == m.to_version)
            .expect("range parsed every version")
            .1;
        let model_ops = build_model_ops(&m.changes, dest_schema);
        let language = escape_language_of(m);
        let is_legacy = m.record_version == 0;
        let authored_src = if language == Some(forgedb_migrations::EscapeLanguage::Rust)
            || (is_legacy && !m.authored_changes().is_empty())
        {
            let p = forgedb_migrations::authored_body_path(&migrations_dir, &m.id);
            Some(std::fs::read_to_string(&p).map_err(|e| {
                CliError::Migration(format!(
                    "migration {} has authored residue but its transform {:?} is missing: {}. \
                     Author the body (see the scaffold `forgedb migrate create` wrote), then rebuild.",
                    m.id, p, e
                ))
            })?)
        } else {
            None
        };

        let escape = match language {
            Some(lang) if lang != forgedb_migrations::EscapeLanguage::Rust => Some(
                stage_escape(app, &member, &migrations_dir, m, lang, &parsed, &mut escape_sources)?,
            ),
            _ => None,
        };

        hops.push(HopPlan {
            from_version: m.from_version,
            to_version: m.to_version,
            migration_id: m.id.clone(),
            model_ops,
            authored_src,
            escape,
        });
    }

    let versions: Vec<VersionSchema> = parsed
        .iter()
        .map(|(v, s)| VersionSchema {
            version: *v,
            schema: s,
        })
        .collect();
    let plan = TransformPlan { versions, hops };

    let package = app.package(kind);
    let mut crate_out = TransformGenerator::generate(&plan, &package)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;
    crate_out.sources.extend(escape_sources);

    write_class_c_member(app, kind, &crate_out)
}

fn escape_language_of(m: &forgedb_migrations::Migration) -> Option<forgedb_migrations::EscapeLanguage> {
    use forgedb_migrations::Answer;
    m.changes.iter().find_map(|c| match c.answer() {
        Some(Answer::Escape { language, .. }) => Some(*language),
        _ => None,
    })
}

fn stage_escape(
    app: &ResolvedApp,
    member: &Path,
    migrations_dir: &Path,
    m: &forgedb_migrations::Migration,
    lang: forgedb_migrations::EscapeLanguage,
    parsed: &[(u32, forgedb_parser::Schema)],
    sources: &mut Vec<(String, String)>,
) -> Result<forgedb_codegen::EscapeBridge> {
    let versions: Vec<(u32, &forgedb_parser::Schema)> = parsed
        .iter()
        .filter(|(v, _)| *v == m.from_version || *v == m.to_version)
        .map(|(v, s)| (*v, s))
        .collect();
    escape::write_support_files(migrations_dir, &m.id, lang, &versions)?;

    let body_dir = forgedb_migrations::migration_body_dir(migrations_dir, &m.id);
    let rel_dir = format!("escape/{}", m.id);
    for entry in std::fs::read_dir(&body_dir)
        .map_err(|e| CliError::Migration(format!("reading {}: {e}", body_dir.display())))?
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let content = std::fs::read_to_string(&path)
            .map_err(|e| CliError::Migration(format!("reading {}: {e}", path.display())))?;
        sources.push((format!("{rel_dir}/{name}"), content));
    }

    let interpreter = crate::toolchain::resolve(
        &app.governing.toolchain,
        &app.project_root,
        lang,
    )?;
    ui::detail(&format!(
        "Escape runtime: {} ({}) — {}",
        interpreter.name,
        interpreter.version,
        interpreter.program.display()
    ));

    Ok(forgedb_codegen::EscapeBridge {
        program: interpreter.program.display().to_string(),
        args: vec![
            member
                .join(&rel_dir)
                .join(lang.transform_file())
                .display()
                .to_string(),
        ],
    })
}

fn build_model_ops(
    changes: &[SchemaChange],
    _dest_schema: &forgedb_parser::Schema,
) -> Vec<forgedb_codegen::ModelOp> {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Acc {
        source_model: Option<String>,
        renames: Vec<(String, String)>,
        removes: Vec<String>,
        adds: Vec<(String, String)>,
        copies: Vec<(String, String)>,
        null_fills: Vec<(String, String)>,
    }
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();

    for change in changes {
        match change {
            SchemaChange::RenameModel { old_name, new_name } => {
                acc.entry(new_name.clone()).or_default().source_model = Some(old_name.clone());
            }
            SchemaChange::RenameField {
                model_name,
                old_name,
                new_name,
            } => {
                acc.entry(model_name.clone())
                    .or_default()
                    .renames
                    .push((old_name.clone(), new_name.clone()));
            }
            SchemaChange::RemoveField {
                model_name,
                field_name,
            } => {
                acc.entry(model_name.clone())
                    .or_default()
                    .removes
                    .push(field_name.clone());
            }
            SchemaChange::AddField {
                model_name,
                field_name,
                nullable,
                default_json,
                answer,
                ..
            } => {
                let entry = acc.entry(model_name.clone()).or_default();
                match lower_fill(*nullable, default_json.as_deref(), answer.as_ref()) {
                    Some(Fill::Json(json)) => entry.adds.push((field_name.clone(), json)),
                    Some(Fill::Copy(from)) => entry.copies.push((from, field_name.clone())),
                    None => {}
                }
            }
            SchemaChange::ChangeFieldNullability {
                model_name,
                field_name,
                old_nullable: true,
                new_nullable: false,
                answer,
            } => {
                let entry = acc.entry(model_name.clone()).or_default();
                match lower_fill(false, None, answer.as_ref()) {
                    Some(Fill::Json(json)) => {
                        entry.null_fills.push((field_name.clone(), json))
                    }
                    Some(Fill::Copy(from)) => entry.copies.push((from, field_name.clone())),
                    None => {}
                }
            }
            SchemaChange::ChangeEnumVariants { .. }
            | SchemaChange::ChangeStructLayout { .. } => {}
            _ => {}
        }
    }

    acc.into_iter()
        .map(|(model, a)| forgedb_codegen::ModelOp {
            source_model: a.source_model.unwrap_or_else(|| model.clone()),
            model,
            field_renames: a.renames,
            field_removes: a.removes,
            field_adds: a.adds,
            field_copies: a.copies,
            field_null_fills: a.null_fills,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    Json(String),
    Copy(String),
}

pub fn lower_fill(
    nullable: bool,
    default_json: Option<&str>,
    answer: Option<&forgedb_migrations::Answer>,
) -> Option<Fill> {
    use forgedb_migrations::Answer;
    if let Some(j) = default_json {
        return Some(Fill::Json(j.to_string()));
    }
    match answer {
        Some(Answer::Constant { json }) => return Some(Fill::Json(json.clone())),
        Some(Answer::CopyField { field }) => return Some(Fill::Copy(field.clone())),
        Some(Answer::Escape { .. }) => return None,
        None => {}
    }
    nullable.then(|| Fill::Json("null".to_string()))
}

fn snapshot_path(migrations_dir: &std::path::Path) -> PathBuf {
    migrations_dir.join(".schema-snapshot.forge")
}

fn save_schema_snapshot(migrations_dir: &std::path::Path, schema_src: &str) -> Result<()> {
    std::fs::create_dir_all(migrations_dir).map_err(|e| {
        CliError::Migration(format!("Failed to create migrations dir: {}", e))
    })?;
    std::fs::write(snapshot_path(migrations_dir), schema_src).map_err(|e| {
        CliError::Migration(format!("Failed to write schema snapshot: {}", e))
    })?;
    Ok(())
}

fn is_storage_backed(field: &forgedb_parser::Field) -> bool {
    use forgedb_parser::{FieldType, RelationType};
    !matches!(
        &field.field_type,
        FieldType::Relation(RelationType::OneToMany(_))
            | FieldType::Relation(RelationType::ManyToMany(_))
    )
}

fn type_dependencies(
    schema: &forgedb_parser::Schema,
    ty: &forgedb_parser::FieldType,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
) {
    use forgedb_parser::FieldType;
    match ty {
        FieldType::Nullable(inner) => type_dependencies(schema, inner, seen, out),
        FieldType::FixedArray(inner, _) => type_dependencies(schema, inner, seen, out),
        FieldType::Enum(name) => {
            if seen.insert(name.clone()) {
                out.push(name.clone());
            }
        }
        FieldType::StructType(name) | FieldType::OptionalStructType(name) => {
            if seen.insert(name.clone()) {
                out.push(name.clone());
                if let Some(def) = schema.find_struct(name) {
                    for f in &def.fields {
                        type_dependencies(schema, &f.field_type, seen, out);
                    }
                }
            }
        }
        _ => {}
    }
}

fn to_simple_type(ty: &forgedb_parser::FieldType) -> forgedb_migrations::SimpleType {
    use forgedb_migrations::SimpleType as S;
    use forgedb_parser::{FieldType as F, RelationType as R, TimestampPrecision as P};
    match ty {
        F::Nullable(inner) => to_simple_type(inner),
        F::OptionalStructType(name) => S::Struct(name.clone()),
        F::Relation(R::OptionalReference(m)) => S::Relation(m.clone()),
        F::U32 => S::U32,
        F::U64 => S::U64,
        F::I32 => S::I32,
        F::I64 => S::I64,
        F::F64 => S::F64,
        F::Bool => S::Bool,
        F::String => S::Str,
        F::StringN { chars, exact } => S::StrN {
            chars: *chars as usize,
            exact: *exact,
        },
        F::Bytes(n) => S::Bytes(*n),
        F::Uuid => S::Uuid,
        F::Timestamp(P::Seconds) => S::Timestamp(0),
        F::Timestamp(P::Millis) => S::Timestamp(1),
        F::Timestamp(P::Micros) => S::Timestamp(2),
        F::Json => S::Json,
        F::Decimal => S::Decimal,
        F::Enum(n) => S::Enum(n.clone()),
        F::StructType(n) => S::Struct(n.clone()),
        F::FixedArray(inner, n) => S::Array(Box::new(to_simple_type(inner)), *n),
        F::Relation(R::RequiredReference(m)) => S::Relation(m.clone()),
        F::Relation(R::OneToMany(m)) | F::Relation(R::ManyToMany(m)) => S::Collection(m.clone()),
        F::Component(c) => S::Opaque(format!("component {c:?}")),
    }
}

fn to_simple_field(
    schema: &forgedb_parser::Schema,
    f: &forgedb_parser::Field,
) -> forgedb_migrations::SimpleField {
    let mut seen = std::collections::HashSet::new();
    let mut depends_on = Vec::new();
    type_dependencies(schema, &f.field_type, &mut seen, &mut depends_on);
    forgedb_migrations::SimpleField {
        name: f.name.clone(),
        ty: to_simple_type(&f.field_type),
        nullable: f.is_nullable(),
        unique: f.unique,
        indexed: f.indexed,
        index_type: format!("{:?}", f.index_type),
        constraints: f
            .constraints
            .iter()
            .map(|c| forgedb_migrations::SimpleConstraint {
                name: c.name.clone(),
                params: c.params.iter().map(|p| format!("{:?}", p)).collect(),
            })
            .collect(),
        depends_on,
    }
}

fn to_simple_schema(schema: &forgedb_parser::Schema) -> forgedb_migrations::SimpleSchema {
    let models = schema
        .models
        .iter()
        .map(|m| forgedb_migrations::SimpleModel {
            name: m.name.clone(),
            fields: m
                .fields
                .iter()
                .filter(|f| is_storage_backed(f))
                .map(|f| to_simple_field(schema, f))
                .collect(),
            composite_indexes: m
                .composite_indexes
                .iter()
                .map(|ci| ci.fields.clone())
                .collect(),
        })
        .collect();
    let enums = schema
        .enums
        .iter()
        .map(|e| forgedb_migrations::SimpleEnum {
            name: e.name.clone(),
            variants: e.variants.clone(),
        })
        .collect();
    let structs = schema
        .structs
        .iter()
        .map(|s| forgedb_migrations::SimpleStruct {
            name: s.name.clone(),
            fields: s
                .fields
                .iter()
                .filter(|f| is_storage_backed(f))
                .map(|f| to_simple_field(schema, f))
                .collect(),
        })
        .collect();
    forgedb_migrations::SimpleSchema {
        models,
        enums,
        structs,
    }
}

fn print_migration_next_steps(schema: &Path, from: u32, to: u32, has_authored: bool) {
    let schema = schema.display();
    println!("\n{}", "Next steps (offline data migration):".bold());
    let mut step = 1;
    if has_authored {
        println!(
            "  {step}. Author the transform body in migrations/<id>/transform.rs \
             (fill in each TODO)."
        );
        step += 1;
    }
    println!("  {step}. Regenerate your app:  forgedb generate --schema {schema}");
    step += 1;
    println!(
        "  {step}. Build the transformer:\n         \
         forgedb migrate build --schema {schema} --from {from} --to {to}"
    );
    step += 1;
    println!(
        "  {step}. Migrate the data with the app STOPPED:\n         \
         forgedb migrate run --schema {schema} --from {from} --to {to} \
         --src <data-dir> --dest <migrated-dir>"
    );
    step += 1;
    println!("  {step}. Point the regenerated app at <migrated-dir>.");
    println!("  See docs/MIGRATIONS.md for the full lifecycle.");
}

fn detect_schema_changes(
    migrations_dir: &std::path::Path,
    new_src: &str,
) -> Result<forgedb_migrations::DiffResult> {
    let new_schema = forgedb_parser::Parser::new(new_src)
        .and_then(|mut p| p.parse())
        .map_err(|e| CliError::Migration(format!("Failed to parse schema: {}", e)))?;
    let new_simple = to_simple_schema(&new_schema);

    let snap = snapshot_path(migrations_dir);
    let Ok(old_src) = std::fs::read_to_string(&snap) else {
        return Ok(forgedb_migrations::DiffResult {
            changes: Vec::new(),
            rename_proposals: Vec::new(),
        });
    };
    let old_schema = forgedb_parser::Parser::new(&old_src)
        .and_then(|mut p| p.parse())
        .map_err(|e| {
            CliError::Migration(format!("Failed to parse recorded schema snapshot: {}", e))
        })?;
    let old_simple = to_simple_schema(&old_schema);

    let mut diff = forgedb_migrations::SchemaDiffer::diff(&old_simple, &new_simple);
    resolve_schema_defaults(&new_schema, &mut diff.changes);
    Ok(diff)
}

fn resolve_schema_defaults(
    dest_schema: &forgedb_parser::Schema,
    changes: &mut [forgedb_migrations::SchemaChange],
) {
    for change in changes.iter_mut() {
        let SchemaChange::AddField {
            model_name,
            field_name,
            default_json,
            ..
        } = change
        else {
            continue;
        };
        *default_json = dest_schema
            .models
            .iter()
            .find(|m| &m.name == model_name)
            .and_then(|m| m.fields.iter().find(|f| &f.name == field_name))
            .and_then(|f| forgedb_codegen::default_fill(dest_schema, f))
            .map(|fill| fill.json_literal());
    }
}
