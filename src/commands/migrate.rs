use crate::{error::CliError, ui, Result};
use colored::Colorize;
use forgedb_migrations::{HopBodyClass, MigrationGenerator, MigrationTracker, SchemaChange};
use std::path::{Path, PathBuf};

// Helper to convert String errors to CliError
fn map_err(e: String) -> CliError {
    CliError::Migration(e)
}

pub struct MigrateCreateOptions {
    pub description: String,
    pub auto: bool,
    pub schema: Option<PathBuf>,
}

pub struct MigrateStatusOptions;

pub struct MigrateBuildOptions {
    pub from: u32,
    pub to: u32,
    /// Where to emit + build the transformer crate (default `migrations/transform`).
    pub output: Option<PathBuf>,
}

pub struct MigrateRunOptions {
    pub src: PathBuf,
    pub dest: PathBuf,
    /// The transformer crate dir (default `migrations/transform`).
    pub bin_dir: Option<PathBuf>,
}

/// Options for `forgedb migrate engine` (#254): the ForgeDB **byte-format**
/// migration, orthogonal to the schema-version transformer above.
pub struct MigrateEngineOptions {
    /// Source data directory (stamped at the old engine generation).
    pub src: PathBuf,
    /// Destination directory to materialize (must not exist / be empty).
    pub dest: PathBuf,
    /// Schema file (default: auto-discovered `schema.forge`). An engine bump
    /// changes no `.forge`, so the SAME schema is baked on both sides of the hop.
    pub schema: Option<PathBuf>,
    /// Where to emit + build the hop crate (default `migrations/engine`).
    pub output: Option<PathBuf>,
}

/// Create a new migration
pub fn create(opts: MigrateCreateOptions) -> Result<()> {
    let migrations_dir = PathBuf::from("migrations");

    if opts.auto {
        // Auto-detect changes by diffing the current schema against the recorded
        // snapshot.  Since #74 Phase 4 the gate no longer REFUSES breaking changes:
        // every non-empty diff is recorded as a versioned hop (`from -> to`), and
        // any residue the differ cannot prove a value for (a type change, a
        // nullable→NOT-NULL narrowing, a required-add-without-default) is
        // classified `Authored` and gets a `migrations/{id}/transform.rs` scaffold
        // the operator fills in.  Purely-additive changes still get the cheap
        // reopen-backfill fast path; anything that rewrites data-at-rest routes
        // through the offline transformer (`forgedb migrate up`).
        let schema_path = opts.schema.clone().unwrap_or_else(|| PathBuf::from("schema.forge"));
        ui::info(&format!(
            "Auto-detecting schema changes ({})...",
            schema_path.display()
        ));

        let new_src = std::fs::read_to_string(&schema_path).map_err(|e| {
            CliError::Migration(format!(
                "Failed to read schema '{}': {}",
                schema_path.display(),
                e
            ))
        })?;

        let changes = detect_schema_changes(&migrations_dir, &new_src)?;

        if changes.is_empty() {
            // First run records a baseline snapshot with no prior schema to diff.
            // Also record the full-schema snapshot for the CURRENT format version
            // (the baseline is v1 for a fresh lineage) so the transformer (#74
            // Phase 3) has every version's `.forge` in its range.
            save_schema_snapshot(&migrations_dir, &new_src)?;
            let lineage = forgedb_migrations::MigrationLineage::load(&migrations_dir)
                .map_err(map_err)?;
            forgedb_migrations::save_versioned_schema(
                &migrations_dir,
                lineage.current_schema_version(),
                &new_src,
            )
            .map_err(map_err)?;
            ui::success("No schema changes detected (snapshot up to date)");
            return Ok(());
        }

        // Classify the diff.  `Authored` residue is the semantic mapping the differ
        // cannot synthesize; a "breaking" change may still be fully `Auto` (drop a
        // field/model, add `&unique`) — the row transform is provable, uniqueness
        // is *validated* during replay.  The two classifications are independent.
        let authored: Vec<_> = changes
            .iter()
            .filter(|c| c.hop_body_class() == HopBodyClass::Authored)
            .collect();
        let has_breaking = changes.iter().any(|c| c.is_breaking());
        let authored_count = authored.len();

        ui::info(&format!("Detected {} change(s)", changes.len()));
        for change in &changes {
            let marker = if change.hop_body_class() == HopBodyClass::Authored {
                " [authored]".yellow()
            } else {
                "".normal()
            };
            println!("  • {}{}", change.description(), marker);
        }

        // Assign the serial version interlock (#74 Phase 2) from the committed
        // lineage: this migration bumps the on-disk format version by one, so a
        // regenerated app expects the new version and refuses a not-yet-migrated
        // data dir (the Phase 1 open guard).  `EXPECTED_SCHEMA_VERSION` is derived
        // from this lineage at `generate` time — never hand-edited (red line #8).
        let lineage = forgedb_migrations::MigrationLineage::load(&migrations_dir)
            .map_err(map_err)?;
        let (from_version, to_version) = lineage.next_version_span();

        let migration = MigrationGenerator::generate_versioned(
            &migrations_dir,
            opts.description,
            changes.clone(),
            from_version,
            to_version,
        )
        .map_err(map_err)?;
        save_schema_snapshot(&migrations_dir, &new_src)?;
        // Record the destination version's full-schema snapshot so the transformer
        // (#74 Phase 3) can emit this version's typed structs for the range.
        forgedb_migrations::save_versioned_schema(&migrations_dir, to_version, &new_src)
            .map_err(map_err)?;

        // Scaffold an authored-transform stub for any `Authored` residue (#74 Phase
        // 2/4).  Never clobbers a frozen body — an existing `transform.rs` is left
        // untouched.
        let scaffold = forgedb_migrations::scaffold_authored_body(&migrations_dir, &migration)
            .map_err(map_err)?;

        ui::success(&format!(
            "Created migration: {} (format v{} → v{})",
            migration.filename(),
            from_version,
            to_version
        ));
        println!("\n{}", MigrationGenerator::generate_report(&migration));

        // A change needs the offline transformer when it rewrites data-at-rest:
        // any `Authored` residue, or any breaking-but-`Auto` change (drop/unique).
        // A purely-additive diff keeps the cheap reopen-backfill path.
        let needs_transform = has_breaking || authored_count > 0;
        if !needs_transform {
            ui::info(
                "Additive change: run `forgedb generate` and restart your app — existing \
                 rows are backfilled with defaults on reopen (no data step needed).",
            );
        } else {
            if let Some((path, created)) = &scaffold {
                if *created {
                    ui::warning(&format!(
                        "Authored transform scaffolded at {} — fill in every TODO before building.",
                        path.display()
                    ));
                } else {
                    ui::info(&format!(
                        "Authored transform already present at {} (left unchanged).",
                        path.display()
                    ));
                }
            }
            print_migration_next_steps(from_version, to_version, authored_count > 0);
        }
    } else {
        // Manual migration - create empty template
        ui::warning("Manual migration mode - you'll need to edit the migration file manually");

        let migration = MigrationGenerator::generate(
            migrations_dir,
            opts.description,
            vec![], // Empty changes
        )
        .map_err(map_err)?;

        ui::success(&format!(
            "Created migration template: {}",
            migration.filename()
        ));
        ui::info("Edit the migration file to add your changes");
    }

    Ok(())
}

/// Show migration status
pub fn status(_opts: MigrateStatusOptions) -> Result<()> {
    let migrations_dir = PathBuf::from("migrations");

    // Load all migrations
    let all_migrations =
        MigrationGenerator::load_all_migrations(&migrations_dir).map_err(map_err)?;

    if all_migrations.is_empty() {
        ui::info("No migrations found");
        return Ok(());
    }

    // Load tracker
    let tracker = MigrationTracker::new(&migrations_dir).map_err(map_err)?;

    println!("{}", "Migration Status".bold());
    println!("{}", "=".repeat(60));
    println!("{}\n", tracker.status_summary(all_migrations.len()));

    // List all migrations
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

/// Generate + compile the offline transformer bin for a version range (#74 Phase
/// 3).  One operator artifact that migrates a data dir from `--from` to `--to`.
pub fn build(opts: MigrateBuildOptions) -> Result<()> {
    let migrations_dir = PathBuf::from("migrations");
    let output = opts
        .output
        .unwrap_or_else(|| migrations_dir.join("transform"));

    let bin = compile_transformer(opts.from, opts.to, &output)?;
    ui::success(&format!("Built transformer: {}", bin.display()));
    ui::info(&format!(
        "Run it with the app STOPPED: `forgedb migrate up --from {} --to {} --src <data> --dest <migrated>` \
         (or directly: `{} <src> <dest>`)",
        opts.from,
        opts.to,
        bin.display()
    ));
    Ok(())
}

/// Emit + `cargo build --release` the transformer crate for a version range,
/// returning the built binary's path (#74 Phase 3/4).  Shared by `migrate build`
/// and `migrate up`.
fn compile_transformer(from: u32, to: u32, output: &Path) -> Result<PathBuf> {
    ui::info(&format!(
        "Generating transformer for format v{from} → v{to} into {}",
        output.display()
    ));
    emit_transform(from, to, output, true)?;

    ui::info("Compiling the transformer (cargo build --release)...");
    cargo_build_transform_bin(output, "transformer")
}

/// The transformer bin's name, fixed by the `[[bin]]` section of the manifest
/// `TransformGenerator::cargo_toml` emits (the engine-hop crate reuses it).
const TRANSFORM_BIN: &str = "forgedb-transform";

/// `cargo build --release` in `crate_dir`, returning the bin's path **as cargo
/// reports it** rather than as we guess it (#292).
///
/// The path is not ours to compute.  Cargo resolves its target directory from
/// `CARGO_TARGET_DIR` and from `[build] target-dir` in every `config.toml` on its
/// discovery chain — including `$CARGO_HOME/config.toml`, which is machine-wide —
/// so any path we join by hand is wrong for those users, and wrong *silently*:
/// this function used to return a constructed path it never checked, so `migrate
/// build` exited 0 while naming a file that was not there.
///
/// `--message-format=json-render-diagnostics` puts one JSON message per line on
/// stdout while leaving human-readable diagnostics and progress on stderr, so
/// capturing stdout costs the user no output.  The `compiler-artifact` message for
/// the bin carries its real `executable`, and it is emitted on a fully-cached
/// rebuild too — a no-op build still has to report where the binary is.
fn cargo_build_transform_bin(crate_dir: &Path, what: &str) -> Result<PathBuf> {
    let child = std::process::Command::new("cargo")
        .args([
            "build",
            "--release",
            "--message-format=json-render-diagnostics",
        ])
        .current_dir(crate_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| CliError::Migration(format!("failed to run cargo: {e}")))?;
    let out = child
        .wait_with_output()
        .map_err(|e| CliError::Migration(format!("failed to run cargo: {e}")))?;
    if !out.status.success() {
        return Err(CliError::Migration(format!(
            "{what} build failed (see cargo output above)"
        )));
    }

    let mut executable: Option<PathBuf> = None;
    for line in out.stdout.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if msg.get("reason").and_then(|r| r.as_str()) != Some("compiler-artifact") {
            continue;
        }
        if msg.pointer("/target/name").and_then(|n| n.as_str()) != Some(TRANSFORM_BIN) {
            continue;
        }
        if let Some(exe) = msg.get("executable").and_then(|e| e.as_str()) {
            executable = Some(PathBuf::from(exe));
        }
    }

    let bin = executable.ok_or_else(|| {
        CliError::Migration(format!(
            "the {what} build emitted no `{TRANSFORM_BIN}` executable — check that {} still \
             declares `[[bin]] name = \"{TRANSFORM_BIN}\"`",
            crate_dir.join("Cargo.toml").display()
        ))
    })?;
    // Never hand back a path without checking it: reporting success for a file that
    // is not there is the whole of #292.
    if !bin.is_file() {
        return Err(CliError::Migration(format!(
            "cargo reported the {what} bin at {} but nothing is there",
            bin.display()
        )));
    }
    Ok(bin)
}

/// Locate an already-built transformer bin **without building** (#292).
///
/// `migrate run` is the one site that cannot ask cargo what it just produced,
/// because it deliberately produces nothing — it runs a bin an earlier `migrate
/// build` left behind.  `cargo metadata` reports the same resolved
/// `target_directory` a build would use, so this honours the redirect without
/// compiling anything.  The crate-local `target/` is searched as well, so a tree
/// built under different configuration is still found, and every location looked in
/// is named if none of them hit — the old error named only the one guess, which is
/// what made this a loop with no exit.
fn locate_transform_bin(crate_dir: &Path) -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    let out = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(crate_dir)
        .output();
    if let Ok(out) = &out
        && out.status.success()
        && let Ok(meta) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        && let Some(dir) = meta.get("target_directory").and_then(|t| t.as_str())
    {
        candidates.push(PathBuf::from(dir).join("release").join(TRANSFORM_BIN));
    }

    let local = crate_dir.join("target").join("release").join(TRANSFORM_BIN);
    if !candidates.contains(&local) {
        candidates.push(local);
    }

    if let Some(hit) = candidates.iter().find(|p| p.is_file()) {
        return Ok(hit.clone());
    }
    Err(CliError::Migration(format!(
        "transformer bin not found — looked in:\n{}\nRun `forgedb migrate build --from <F> --to <T>` first.",
        candidates
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    )))
}

/// Run a built transformer bin over a single `src → dest` data dir (#74 Phase
/// 3/4).  The bin writes `dest` atomically and leaves `src` untouched (rollback).
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

/// Run a previously-built transformer bin over a src→dest data dir (#74 Phase 3).
/// The app must be stopped (offline, exclusive-writer migration — C12).
pub fn run(opts: MigrateRunOptions) -> Result<()> {
    let bin_dir = opts
        .bin_dir
        .unwrap_or_else(|| PathBuf::from("migrations/transform"));
    let bin = locate_transform_bin(&bin_dir)?;

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

/// `forgedb migrate engine` (#254): carry a data dir across a ForgeDB
/// **byte-format generation** boundary.
///
/// This is NOT the schema-version transformer. The two counters are orthogonal:
/// `migrate up` replays the app's own `migrations/` lineage, and this replays
/// ForgeDB's engine generations. An engine bump changes no `.forge`, produces no
/// lineage hop, and would therefore run nothing at all through the transformer —
/// which is exactly why it needs its own command rather than a fold-in.
///
/// The hop crate is generated (never schema-blind): a nullable, arrayed, or
/// struct-nested timestamp is written as an opaque `FixedBytes` transmute that no
/// schema-agnostic reader may decode, and 81 of the 247 timestamp fields in the
/// example corpus are nullable. See `forgedb_codegen::engine`.
pub fn engine(opts: MigrateEngineOptions) -> Result<()> {
    let output = opts
        .output
        .unwrap_or_else(|| PathBuf::from("migrations").join("engine"));

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

    ui::info(&format!(
        "Engine migration: {} (schema v{schema_version}, engine generation {from_engine} → {to_engine})",
        opts.src.display()
    ));

    emit_engine(opts.schema.as_deref(), schema_version, from_engine, to_engine, &output)?;

    ui::info("Compiling the engine-hop bin (cargo build --release)...");
    let bin = cargo_build_transform_bin(&output, "engine-hop")?;

    ui::info(&format!(
        "Migrating {} → {} (the source dir is left untouched — it is your rollback)",
        opts.src.display(),
        opts.dest.display()
    ));
    run_transformer(&bin, &opts.src, &opts.dest)?;
    ui::success("Engine migration complete — point the regenerated app at the destination dir");
    Ok(())
}

/// Emit the engine-hop crate into `output`. The same schema is baked on both
/// sides; only `EXPECTED_ENGINE_VERSION` differs between the two modules.
fn emit_engine(
    schema: Option<&Path>,
    schema_version: u32,
    from_engine: u32,
    to_engine: u32,
    output: &Path,
) -> Result<()> {
    use forgedb_codegen::{EngineHopPlan, EngineMigrationGenerator};

    let schema_path = crate::project::find_schema(schema.map(|p| p.to_string_lossy()).as_deref())?;
    let src = std::fs::read_to_string(&schema_path)
        .map_err(|e| CliError::SchemaNotFound(format!("{}: {}", schema_path.display(), e)))?;
    let parsed = forgedb_parser::Parser::new(&src)
        .and_then(|mut p| p.parse())
        .map_err(|e| CliError::SchemaValidation(format!("{}: {e}", schema_path.display())))?;

    let plan = EngineHopPlan {
        schema: &parsed,
        schema_version,
        from_engine,
        to_engine,
    };
    let crate_out = EngineMigrationGenerator::generate(&plan, "forgedb-engine-migrate")
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;

    std::fs::create_dir_all(output)?;
    let cargo_path = output.join("Cargo.toml");
    if !cargo_path.exists() {
        std::fs::write(&cargo_path, &crate_out.cargo_toml)?;
    }
    for (rel, content) in &crate_out.sources {
        let path = output.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    ui::success(&format!(
        "Generated engine-hop crate ({} source files) at {}",
        crate_out.sources.len(),
        output.display()
    ));
    Ok(())
}

/// Options for the one-CLI `migrate up` orchestration (#74 Phase 4).
pub struct MigrateUpOptions {
    /// Origin format version.  Defaults to the version detected from the source
    /// data dir's manifests.
    pub from: Option<u32>,
    /// Destination format version.  Defaults to the lineage's current version.
    pub to: Option<u32>,
    /// Source data directory (single-dir mode).
    pub src: Option<PathBuf>,
    /// Destination directory to materialize (single-dir mode).
    pub dest: Option<PathBuf>,
    /// Transformer crate dir (default `migrations/transform`).
    pub output: Option<PathBuf>,
    /// Per-tenant sweep: migrate every data dir directly under this root.
    pub tenant_root: Option<PathBuf>,
    /// Destination-name suffix for the per-tenant sweep (default
    /// `-migrated-v<to>`): tenant `t`'s output is `<root>/t<suffix>`.
    pub dest_suffix: Option<String>,
}

/// One-CLI migration lifecycle (#74 Phase 4): generate + `cargo build` the
/// transformer for the resolved version range, then run it over one data dir
/// (`--src`/`--dest`) or every tenant dir under `--tenant-root` (a rolling,
/// independent sweep — a failed tenant is reported and skipped, its source
/// unchanged).  The app must be STOPPED (offline, exclusive-writer — C12).
pub fn up(opts: MigrateUpOptions) -> Result<()> {
    let migrations_dir = PathBuf::from("migrations");
    let lineage = forgedb_migrations::MigrationLineage::load(&migrations_dir).map_err(map_err)?;
    let to = opts.to.unwrap_or_else(|| lineage.current_schema_version());
    let output = opts
        .output
        .clone()
        .unwrap_or_else(|| migrations_dir.join("transform"));

    // Enumerate the (src, dest) jobs: a per-tenant sweep, or a single dir.
    let jobs: Vec<(PathBuf, PathBuf)> = if let Some(root) = &opts.tenant_root {
        collect_tenant_jobs(root, to, opts.dest_suffix.as_deref())?
    } else {
        let src = opts.src.clone().ok_or_else(|| {
            CliError::Migration(
                "`migrate up` needs --src and --dest (or --tenant-root for a per-tenant sweep)"
                    .to_string(),
            )
        })?;
        let dest = opts
            .dest
            .clone()
            .ok_or_else(|| CliError::Migration("`migrate up` needs --dest".to_string()))?;
        vec![(src, dest)]
    };

    // Resolve the origin version once: explicit --from, else detect it from the
    // first job's source manifests.  A tenant at a different version is refused by
    // the built bin's open-guard (counted as a per-tenant failure, sweep continues).
    let from = match opts.from {
        Some(f) => f,
        None => {
            let first = &jobs[0].0;
            detect_src_schema_version(first)?.ok_or_else(|| {
                CliError::Migration(format!(
                    "could not detect the source format version of {} — pass --from explicitly",
                    first.display()
                ))
            })?
        }
    };

    if from == to {
        ui::success(&format!(
            "Data is already at format v{to} — nothing to migrate."
        ));
        return Ok(());
    }
    if to < from {
        return Err(CliError::Migration(format!(
            "refusing to migrate backwards: source is at v{from}, target is v{to} \
             (the transformer only replays forward)"
        )));
    }

    ui::info(&format!(
        "Migrating {} data dir(s): format v{from} → v{to}",
        jobs.len()
    ));

    // Build the transformer once for the whole range, then run it per job.
    let bin = compile_transformer(from, to, &output)?;

    let mut failures = Vec::new();
    for (src, dest) in &jobs {
        ui::info(&format!("→ {} ⇒ {}", src.display(), dest.display()));
        match run_transformer(&bin, src, dest) {
            Ok(()) => ui::success(&format!("  migrated {}", dest.display())),
            Err(e) => {
                ui::error(&format!("  FAILED {}: {}", src.display(), e));
                failures.push(src.clone());
            }
        }
    }

    if !failures.is_empty() {
        return Err(CliError::Migration(format!(
            "{} of {} data dir(s) failed to migrate (their originals are unchanged): {}",
            failures.len(),
            jobs.len(),
            failures
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    ui::success(&format!(
        "Migration complete ({} dir(s)) — regenerate your app and point it at the destination(s).",
        jobs.len()
    ));
    Ok(())
}

/// Enumerate the per-tenant sweep jobs under `root` (#74 Phase 4): one
/// `(src, dest)` per immediate subdirectory that looks like a data dir (has a
/// `<model>/manifest.json`), skipping any dir already named like a migration
/// output.  `dest` is `<root>/<tenant><suffix>`.
fn collect_tenant_jobs(
    root: &Path,
    to: u32,
    dest_suffix: Option<&str>,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let suffix = dest_suffix
        .map(str::to_string)
        .unwrap_or_else(|| format!("-migrated-v{to}"));

    let mut jobs = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| {
        CliError::Migration(format!(
            "failed to read tenant root {}: {}",
            root.display(),
            e
        ))
    })?;
    for entry in entries {
        let path = entry.map_err(|e| CliError::Migration(e.to_string()))?.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        // Skip outputs of a prior sweep.
        if name.ends_with(&suffix) {
            continue;
        }
        // Only sweep dirs that actually hold data (a model manifest).
        if detect_src_schema_version(&path)?.is_none() {
            continue;
        }
        jobs.push((path, root.join(format!("{name}{suffix}"))));
    }
    jobs.sort();
    if jobs.is_empty() {
        return Err(CliError::Migration(format!(
            "no tenant data dirs found under {} (a data dir has a <model>/manifest.json)",
            root.display()
        )));
    }
    Ok(jobs)
}

/// Detect the on-disk schema serial of a data directory by reading the first
/// `<model>/manifest.json` under it (#74 Phase 4).  Every model/junction manifest
/// in a consistent dir carries the same version (the app open-guard enforces it),
/// so the first one found is authoritative.  `None` when the dir holds no manifest
/// (not a data dir, or empty).
fn detect_src_schema_version(data_dir: &Path) -> Result<Option<u32>> {
    Ok(detect_src_versions(data_dir)?.map(|(schema, _engine)| schema))
}

/// Both on-disk counters, read from the first model manifest under `data_dir`.
///
/// They are orthogonal (#254): `schema_version` (on-disk key `format_version`) is
/// the **app's** migration serial, `engine_version` is **ForgeDB's** byte-format
/// generation.  A manifest written before the engine counter existed has none,
/// which baselines to generation 1 — the same default the `Manifest` struct
/// applies, kept identical here so the CLI and the engine cannot disagree about
/// what an old dir is.
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

/// Generate the transformer crate for a version range into `output` (#74 Phase 3).
/// Shared by `migrate build` and `generate transform`.  Loads the committed
/// lineage, expands the contiguous range (C1), parses each version's committed
/// full schema, builds the frozen per-hop plan, and emits the crate.
pub fn emit_transform(from: u32, to: u32, output: &Path, force: bool) -> Result<()> {
    use forgedb_codegen::{HopPlan, TransformGenerator, TransformPlan, VersionSchema};
    use forgedb_migrations::MigrationLineage;

    let migrations_dir = PathBuf::from("migrations");
    let lineage = MigrationLineage::load(&migrations_dir).map_err(map_err)?;
    let hop_migrations = lineage.expand_range(from, to).map_err(map_err)?;
    if hop_migrations.is_empty() {
        return Err(CliError::Migration(format!(
            "empty migration range (v{from} → v{to}): nothing to transform"
        )));
    }

    // Parse each version's committed full schema (from..=to).  These own the
    // parsed ASTs the plan borrows, so they must outlive the plan.
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

    // Build one frozen hop plan per recorded migration in the range.
    let mut hops = Vec::new();
    for m in &hop_migrations {
        let dest_schema = &parsed
            .iter()
            .find(|(v, _)| *v == m.to_version)
            .expect("range parsed every version")
            .1;
        let model_ops = build_model_ops(&m.changes, dest_schema);
        let authored_src = if m.authored_changes().is_empty() {
            None
        } else {
            let p = forgedb_migrations::authored_body_path(&migrations_dir, &m.id);
            Some(std::fs::read_to_string(&p).map_err(|e| {
                CliError::Migration(format!(
                    "migration {} has authored residue but its transform {:?} is missing: {}. \
                     Author the body (see the scaffold `forgedb migrate create` wrote), then rebuild.",
                    m.id, p, e
                ))
            })?)
        };
        hops.push(HopPlan {
            from_version: m.from_version,
            to_version: m.to_version,
            migration_id: m.id.clone(),
            model_ops,
            authored_src,
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

    let crate_out = TransformGenerator::generate(&plan, "forgedb-transform")
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;

    // Write the crate.  `Cargo.toml` is a user-editable scaffold (only-if-absent);
    // the `src/*.rs` are always (re)written.
    std::fs::create_dir_all(output)?;
    let cargo_path = output.join("Cargo.toml");
    if !cargo_path.exists() {
        std::fs::write(&cargo_path, &crate_out.cargo_toml)?;
    }
    for (rel, content) in &crate_out.sources {
        let path = output.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() && !force {
            return Err(CliError::Other(format!(
                "File exists: {}. Use --force to overwrite",
                path.display()
            )));
        }
        std::fs::write(&path, content)?;
    }

    ui::success(&format!(
        "Generated transformer crate ({} source files) at {}",
        crate_out.sources.len(),
        output.display()
    ));
    Ok(())
}

/// Build the frozen per-model structural ops for one hop from its recorded schema
/// changes + the destination schema's field types (#74 Phase 3).  Only the
/// provable structural ops (rename / remove / additive add) are emitted here;
/// semantic residue is carried by the hop's embedded authored transform.
fn build_model_ops(
    changes: &[SchemaChange],
    dest_schema: &forgedb_parser::Schema,
) -> Vec<forgedb_codegen::ModelOp> {
    use std::collections::BTreeMap;

    // Accumulate ops per destination model name.
    #[derive(Default)]
    struct Acc {
        source_model: Option<String>,
        renames: Vec<(String, String)>,
        removes: Vec<String>,
        adds: Vec<(String, String)>,
    }
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();

    for change in changes {
        match change {
            SchemaChange::RenameModel { old_name, new_name } => {
                // Copy from the old collection into the renamed one.
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
                default_value,
                ..
            } => {
                let json = add_field_default_json(
                    dest_schema,
                    model_name,
                    field_name,
                    *nullable,
                    default_value.as_deref(),
                );
                acc.entry(model_name.clone())
                    .or_default()
                    .adds
                    .push((field_name.clone(), json));
            }
            // Structural no-ops for the row body (index/constraint/type changes):
            // index & constraint changes leave the row bytes identical; a type
            // change / nullable-narrowing is Authored residue carried separately.
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
        })
        .collect()
}

/// Compute the JSON default literal for an additive field, resolving the field's
/// real type from the destination schema (#74 Phase 3).  A nullable add with no
/// default is `null`; a defaulted add is encoded to match the field's serde
/// representation (numbers for numeric types, quoted strings for string-repr
/// types like `string`/`uuid`/`decimal`/enum).
fn add_field_default_json(
    dest_schema: &forgedb_parser::Schema,
    model_name: &str,
    field_name: &str,
    nullable: bool,
    default_value: Option<&str>,
) -> String {
    use forgedb_parser::FieldType;

    // Resolve the (non-nullable) base type of the field from the dest schema.
    fn base(ft: &FieldType) -> &FieldType {
        match ft {
            FieldType::Nullable(inner) => base(inner),
            other => other,
        }
    }
    let ftype = dest_schema
        .models
        .iter()
        .find(|m| m.name == model_name)
        .and_then(|m| m.fields.iter().find(|f| f.name == field_name))
        .map(|f| base(&f.field_type));

    let quote_str = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string());

    match default_value {
        None => {
            if nullable {
                return "null".to_string();
            }
            // Non-null add with no default is Authored residue, not Auto — but be
            // defensive: emit a type-zero so a mis-tagged hop still compiles.
            match ftype {
                Some(FieldType::Bool) => "false".to_string(),
                Some(FieldType::F64) => "0.0".to_string(),
                Some(
                    FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64
                    | FieldType::Timestamp(_),
                ) => "0".to_string(),
                // #238: an inline `string(N)` is a `String` in the generated
                // struct, so its type-zero is the empty string like any other.
                // (`string(N!)` would reject `""` at write, but this arm is
                // defensive residue for a mis-tagged hop, not a real default.)
                Some(FieldType::String | FieldType::StringN { .. } | FieldType::Bytes(_)) => {
                    "\"\"".to_string()
                }
                _ => "null".to_string(),
            }
        }
        Some(d) => match ftype {
            Some(FieldType::Bool) => {
                if d == "true" || d == "false" {
                    d.to_string()
                } else {
                    "false".to_string()
                }
            }
            Some(FieldType::F64) => {
                if d.parse::<f64>().is_ok() {
                    d.to_string()
                } else {
                    "0.0".to_string()
                }
            }
            Some(
                FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64
                | FieldType::Timestamp(_),
            ) => {
                if d.parse::<i64>().is_ok() {
                    d.to_string()
                } else {
                    "0".to_string()
                }
            }
            Some(FieldType::Json) => {
                // A raw JSON default is used verbatim; otherwise quote it.
                if serde_json::from_str::<serde_json::Value>(d).is_ok() {
                    d.to_string()
                } else {
                    quote_str(d)
                }
            }
            // string / uuid / decimal / enum: string serde repr → quote.
            _ => quote_str(d),
        },
    }
}

/// Path of the recorded schema snapshot (the last schema a migration was created
/// against), stored as raw `.forge` so it re-parses through the same grammar.
fn snapshot_path(migrations_dir: &std::path::Path) -> PathBuf {
    migrations_dir.join(".schema-snapshot.forge")
}

/// Persist the current schema source as the snapshot for the next auto-diff.
fn save_schema_snapshot(migrations_dir: &std::path::Path, schema_src: &str) -> Result<()> {
    std::fs::create_dir_all(migrations_dir).map_err(|e| {
        CliError::Migration(format!("Failed to create migrations dir: {}", e))
    })?;
    std::fs::write(snapshot_path(migrations_dir), schema_src).map_err(|e| {
        CliError::Migration(format!("Failed to write schema snapshot: {}", e))
    })?;
    Ok(())
}

/// Parse a `.forge` source and project it onto the migration differ's
/// `SimpleSchema`.  Only **storage-backed** fields are included — pure collection
/// relations (`[Model]` one-to-many / many-to-many) carry no column, so adding or
/// removing one is not a data migration and must not register as a field change.
/// FK scalars (`*Model` / `?Model`) DO carry a `Uuid` column, so they are kept.
fn to_simple_schema(schema: &forgedb_parser::Schema) -> forgedb_migrations::SimpleSchema {
    use forgedb_parser::{FieldType, RelationType};
    let models = schema
        .models
        .iter()
        .map(|m| forgedb_migrations::SimpleModel {
            name: m.name.clone(),
            fields: m
                .fields
                .iter()
                .filter(|f| {
                    !matches!(
                        &f.field_type,
                        FieldType::Relation(RelationType::OneToMany(_))
                            | FieldType::Relation(RelationType::ManyToMany(_))
                    )
                })
                .map(|f| forgedb_migrations::SimpleField {
                    name: f.name.clone(),
                    // Debug repr is a stable, consistent stringification for both
                    // sides of the diff — equality is all the differ needs.
                    field_type: format!("{:?}", f.field_type),
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
                })
                .collect(),
            composite_indexes: m
                .composite_indexes
                .iter()
                .map(|ci| ci.fields.clone())
                .collect(),
        })
        .collect();
    forgedb_migrations::SimpleSchema { models }
}

/// Print the offline data-migration next steps for a recorded hop that rewrites
/// data-at-rest (#74 Phase 4).  `has_authored` toggles the "author the body" step.
fn print_migration_next_steps(from: u32, to: u32, has_authored: bool) {
    println!("\n{}", "Next steps (offline data migration):".bold());
    let mut step = 1;
    if has_authored {
        println!(
            "  {step}. Author the transform body in migrations/<id>/transform.rs \
             (fill in each TODO)."
        );
        step += 1;
    }
    println!("  {step}. Regenerate your app:  forgedb generate");
    step += 1;
    println!(
        "  {step}. Migrate the data with the app STOPPED:\n         \
         forgedb migrate up --from {from} --to {to} --src <data-dir> --dest <migrated-dir>"
    );
    step += 1;
    println!("  {step}. Point the regenerated app at <migrated-dir>.");
    println!("  See docs/MIGRATIONS.md for the full lifecycle.");
}

/// Detect schema changes by diffing the new schema source against the recorded
/// snapshot.  Returns an empty list when there is no prior snapshot (first run —
/// the caller records a baseline) or when nothing changed.
fn detect_schema_changes(
    migrations_dir: &std::path::Path,
    new_src: &str,
) -> Result<Vec<SchemaChange>> {
    let new_schema = forgedb_parser::Parser::new(new_src)
        .and_then(|mut p| p.parse())
        .map_err(|e| CliError::Migration(format!("Failed to parse schema: {}", e)))?;
    let new_simple = to_simple_schema(&new_schema);

    let snap = snapshot_path(migrations_dir);
    let Ok(old_src) = std::fs::read_to_string(&snap) else {
        // No prior snapshot — nothing to diff against (baseline recorded by caller).
        return Ok(Vec::new());
    };
    let old_schema = forgedb_parser::Parser::new(&old_src)
        .and_then(|mut p| p.parse())
        .map_err(|e| {
            CliError::Migration(format!("Failed to parse recorded schema snapshot: {}", e))
        })?;
    let old_simple = to_simple_schema(&old_schema);

    Ok(forgedb_migrations::SchemaDiffer::diff(&old_simple, &new_simple))
}


