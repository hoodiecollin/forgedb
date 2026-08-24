use crate::naming::PackageKind;
use crate::{error::CliError, ui, Result};
use colored::Colorize;
use forgedb_migrations::{HopBodyClass, MigrationGenerator, MigrationTracker, SchemaChange};
use std::path::{Path, PathBuf};

// Helper to convert String errors to CliError
fn map_err(e: String) -> CliError {
    CliError::Migration(e)
}

/// The app one `migrate` invocation acts on (#335 §9).
///
/// **No `migrate` command is CWD-relative, and none of them discovers
/// anything.** The operator names the app, and every path derives from that:
/// `migrations/` is read beside the schema rather than beside wherever the
/// operator happened to be standing.
///
/// The schema path is already the app's identity everywhere else in epic #332
/// — `cache::member_hash` is FNV-1a over the project-relative schema path — so
/// requiring it here states the existing key explicitly instead of inferring it
/// from a working directory.
pub struct AppRef {
    /// `--schema`, **required on every arm**, and exactly one per invocation:
    /// no `migrate` command takes a list and none sweeps. The per-tenant sweep
    /// left with `migrate up` and is the substance of #373.
    pub schema: PathBuf,
    /// The global `--config` override, threaded so `migrate` governs exactly
    /// the way `generate` does. It overrides the knobs, never the identity —
    /// which project a schema belongs to is a fact about the tree.
    pub config: Option<String>,
}

pub struct MigrateCreateOptions {
    pub app: AppRef,
    pub description: String,
    pub auto: bool,
}

pub struct MigrateStatusOptions {
    pub app: AppRef,
}

pub struct MigrateBuildOptions {
    pub app: AppRef,
    pub from: u32,
    pub to: u32,
    /// TOMBSTONE for the deleted `-o/--output` (#335 §9).
    ///
    /// It still parses so that passing it is an **error naming the
    /// replacement** rather than clap's "unexpected argument", which names
    /// none. Its old default value was `migrations/transform` — precisely the
    /// path that reproduces #328 — so keeping the flag was never an option.
    pub removed_output: Option<PathBuf>,
}

pub struct MigrateRunOptions {
    pub app: AppRef,
    /// Origin format version — **required**, because the transformer member is
    /// range-stamped (`transform-<from>-<to>`) and cannot be named without it.
    pub from: u32,
    /// Destination format version.
    pub to: u32,
    pub src: PathBuf,
    pub dest: PathBuf,
    /// TOMBSTONE for the deleted `--bin-dir` (#335 §9). See
    /// [`MigrateBuildOptions::removed_output`].
    pub removed_bin_dir: Option<PathBuf>,
}

/// Options for `forgedb migrate engine` (#254): the ForgeDB **byte-format**
/// migration, orthogonal to the schema-version transformer above.
pub struct MigrateEngineOptions {
    /// The app. Unlike the transformer's, this schema is not location-only —
    /// an engine bump changes no `.forge`, so the SAME schema is baked on both
    /// sides of the hop.
    pub app: AppRef,
    /// Source data directory (stamped at the old engine generation).
    pub src: PathBuf,
    /// Destination directory to materialize (must not exist / be empty).
    pub dest: PathBuf,
    /// TOMBSTONE for the deleted `-o/--output` (#335 §9). See
    /// [`MigrateBuildOptions::removed_output`].
    pub removed_output: Option<PathBuf>,
}

/// One `migrate` invocation's app, resolved: which project governs it, which
/// container in the build cache is its, and every name it builds under.
struct ResolvedApp {
    /// The `.forge` the operator named.
    schema: PathBuf,
    /// `<the schema's directory>/migrations` — derived from the given app,
    /// never from the CWD and never walked for.
    migrations_dir: PathBuf,
    /// The app's legible derived identity, e.g. `foo_services_blog`.
    app_name: String,
    reserved: crate::cache::Reserved,
}

impl ResolvedApp {
    /// The cargo `[package] name` for one of this app's cache members.
    fn package(&self, kind: &PackageKind) -> String {
        crate::naming::package_name(&self.app_name, kind)
    }

    /// The `[[bin]]` name for a class-C member.
    ///
    /// `naming::bin_name` **is** `naming::package_name` today; both are called
    /// rather than one derived from the other here, so that if they ever
    /// diverge it is one edit in `naming.rs` and not a search of this file.
    fn bin(&self, kind: &PackageKind) -> String {
        crate::naming::bin_name(&self.app_name, kind)
    }

    /// The member directory: `<container>/transform-1-2`, `<container>/engine-1-2`.
    fn member(&self, kind: &PackageKind) -> PathBuf {
        self.reserved.container.join(kind.dir())
    }
}

/// Run the governance chain every `migrate` arm now runs — the same one the
/// Generate arm runs: `govern -> identify_reported -> reserve` (#335 §9).
///
/// One new failure mode, stated deliberately rather than discovered: `migrate`
/// gaining `identify_reported` means it can hard-fail on a #333 project-id
/// collision — including on `migrate engine`, which carries the *mandatory*
/// 0.4.0 engine upgrade. That is consistent with `generate`, and the
/// diagnostic names `[project].name` as the remedy.
fn resolve_app(app: &AppRef) -> Result<ResolvedApp> {
    let schema = &app.schema;
    // The schema is not discovered, so a missing one is the operator naming the
    // wrong app — never a cue to go looking for another.
    if !schema.is_file() {
        return Err(CliError::SchemaNotFound(format!(
            "{} does not exist. `--schema` names the app this migration belongs to; \
             it is not discovered and there is no default.",
            schema.display()
        )));
    }

    let governing = crate::project::govern(app.config.as_deref(), crate::project::schema_dir(schema))?;
    let project = governing.identify_reported()?;
    let reserved =
        crate::cache::reserve(&project.name, &project.root, schema, governing.symbol_naming())?;
    // C7: every command that writes into the cache prints where it wrote.
    // Once the build happens in a hashed directory the user has never seen, a
    // silent path is the difference between a build they can inspect and one
    // they cannot.
    ui::info(&format!("Build cache: {}", reserved.container.display()));

    Ok(ResolvedApp {
        schema: schema.clone(),
        migrations_dir: crate::project::migrations_dir(schema),
        app_name: reserved.app_name.clone(),
        reserved,
    })
}

/// A deleted flag that still **parses**, so that passing it is an error naming
/// the replacement (#335 §9).
///
/// Removing the flag outright hands the user clap's "unexpected argument",
/// which names no replacement. Keeping it and ignoring it is worse still: a
/// flag that reads as applied and is not is the exact failure mode this design
/// deletes everywhere else. So the flag survives as a tombstone whose entire
/// behavior is this refusal.
fn refuse_removed_flag(flag: &str, value: Option<&PathBuf>, replacement: &str) -> Result<()> {
    match value {
        None => Ok(()),
        Some(v) => Err(CliError::Migration(format!(
            "`{flag} {}` was removed (#335): ForgeDB owns where this is built, and it is \
             built as a member of the project's own cache workspace rather than into a \
             directory of your choosing. {replacement}",
            v.display()
        ))),
    }
}

/// `forgedb migrate up`, removed (#335 §9, maintainer decision 3).
///
/// It was a convenience wrapper over `migrate build` + `migrate run`, not a
/// capability — one fewer command needing a cache-placement answer, and one
/// fewer that builds a transformer. What is actually lost is the per-tenant
/// sweep and the source-version auto-detection, and that is the substance of
/// **#373**; git history is their record.
///
/// This exists as a **tombstone rather than a deleted subcommand** so the
/// removal errors and names its replacement: an operator following a runbook
/// gets told what to run, not clap's "unrecognized subcommand".
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

/// Create a new migration
pub fn create(opts: MigrateCreateOptions) -> Result<()> {
    let app = resolve_app(&opts.app)?;
    // Derived from the app the operator named, never from the working directory
    // (#335 §9): one root config can govern many apps, and a CWD-relative
    // `migrations/` gave whichever of them the operator happened to `cd` into.
    let migrations_dir = app.migrations_dir.clone();

    if opts.auto {
        // Auto-detect changes by diffing the current schema against the recorded
        // snapshot.  Since #74 Phase 4 the gate no longer REFUSES breaking changes:
        // every non-empty diff is recorded as a versioned hop (`from -> to`), and
        // any residue the differ cannot prove a value for (a type change, a
        // nullable→NOT-NULL narrowing, a required-add-without-default) is
        // classified `Authored` and gets a `migrations/{id}/transform.rs` scaffold
        // the operator fills in.  Purely-additive changes still get the cheap
        // reopen-backfill fast path; anything that rewrites data-at-rest routes
        // through the offline transformer (`forgedb migrate build` + `run`).
        //
        // The schema is the one the operator named. The CWD-relative
        // `schema.forge` default that used to sit here died with the rest of
        // the discovery (#335 §9): a defaulted schema path is the same defect
        // wearing a different name — it picks an app out of the working
        // directory and diffs *that* app's snapshot.
        let schema_path = app.schema.clone();
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
            print_migration_next_steps(&app.schema, from_version, to_version, authored_count > 0);
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
pub fn status(opts: MigrateStatusOptions) -> Result<()> {
    let app = resolve_app(&opts.app)?;
    let migrations_dir = app.migrations_dir.clone();

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
///
/// The transformer is emitted into this app's build-cache container as
/// `transform-<from>-<to>/` and compiled as a **member of the project's cargo
/// workspace** (#335 §1/§9) — not into a directory the operator chooses, which
/// is how a generated `[package]` came to land under a foreign cargo root with
/// no `[workspace]` table (#328).
pub fn build(opts: MigrateBuildOptions) -> Result<()> {
    refuse_removed_flag(
        "--output",
        opts.removed_output.as_ref(),
        "Pass `--schema <app.forge>` instead: the transformer is emitted into that app's \
         build-cache container as `transform-<from>-<to>/`, and the command prints the path.",
    )?;
    let app = resolve_app(&opts.app)?;
    let kind = PackageKind::Transform {
        from: opts.from,
        to: opts.to,
    };

    // Said out loud, because the alternative reading is the natural one and it
    // is wrong: `--schema` names the APP, and nothing else. The transformer is
    // generated from the versioned snapshots the lineage committed, so a
    // drifted HEAD schema cannot change a historical hop.
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

/// Rewrite the project's workspace root around what is now on disk (#335 §3).
///
/// Runs **after** emission: a root rendered before it lists the *previous*
/// run's packages, so an app's first `migrate build` would write a root naming
/// no `transform-<a>-<b>` at all and `cargo build -p <it>` would fail with
/// `did not match any packages`.
///
/// **It prunes nothing.** `PruneOwner::MigrateBuild` / `MigrateEngine` exist so
/// that `generate` does not reap a transformer it did not emit (§3 rule 4);
/// they do not oblige `migrate` to reap one either. A sibling range is not
/// garbage — `transform-1-2` is still valid work after `transform-2-3` is
/// built, and the epic's asymmetry says to fail toward keeping: a package on
/// disk that `members` does not name is inert, while a wrongly-deleted one
/// costs a rebuild.
fn sync_cache_root(app: &ResolvedApp) -> Result<()> {
    let synced = crate::cache::sync_root(&app.reserved.project, &app.reserved.container)?;
    // C9: reported rather than silent, because it happened in a directory the
    // user never opens.
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

/// Compile one class-C cache member's `[[bin]]` and return the path **cargo
/// reported**, never one we joined by hand.
///
/// # One driver, not two
///
/// `cargo_build_transform_bin` used to live here. It was already the correct
/// driver — `.current_dir`, streamed diagnostics,
/// `--message-format=json-render-diagnostics` on stdout, the real `executable`
/// read out of the `compiler-artifact` message, and a refusal to return a path
/// it had not `is_file()`-checked, with #292 named as the reason. Its one
/// limitation was that it knew exactly one package, in exactly one directory.
///
/// So it was **absorbed by the general driver rather than duplicated here**
/// (#335 step 6/8). Everything `migrate` needs from cargo now goes through
/// [`crate::commands::build::driver`], and this function is a thin adapter:
/// name the member, ask the driver to build it, and pick the `bin` back out of
/// what cargo reported.
///
/// Three things come for free by routing rather than re-implementing, and each
/// one is a defect this file used to own:
///
/// * **the duplicate-artifact guard runs first.** One app can hold both
///   `transform-1-2` and `engine-1-2`; if two members ever declared the same
///   `[[bin]]` name, cargo would `warning: output filename collision`, **exit
///   0**, and leave one of them behind — after which resolving a bin by name
///   runs the *wrong hop* over a user's data dir at exit 0. That is scenario
///   25, and it needs no compile to catch.
/// * **the release profile floor is the same one `forgedb build` uses.**
///   `[profile.release]` in a workspace *member* is silently ignored by cargo,
///   which is why `transform.rs` no longer writes one; the floor is applied by
///   `driver::plan` as `--config`, at the root, where cargo honours it.
/// * **the build runs at the cache workspace root**, so one `Cargo.lock` and
///   one `target/` are shared with every other package of the project instead
///   of the member resolving its own.
///
/// `release` is always `true` here: a transformer and an engine hop each make
/// one full pass over a user's data directory, and a debug build of that is not
/// a build anyone wants to wait through.
fn build_member_bin(app: &ResolvedApp, kind: &PackageKind, what: &str) -> Result<PathBuf> {
    use crate::commands::build::driver::{self, Selected, TargetKind};

    let package = app.package(kind);
    let member = app.member(kind);

    // Refuse before compiling, not after: this is the guard that catches the
    // `transform`/`engine` pair, and cargo's own report of the condition is a
    // warning that exits 0.
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

/// Locate an already-built class-C bin **without building** (#292).
///
/// `migrate run` is the one site that cannot ask cargo what it just produced,
/// because it deliberately produces nothing — it runs a bin an earlier `migrate
/// build` left behind. `cargo metadata` reports the same resolved
/// `target_directory` a build would use, so this honours `CARGO_TARGET_DIR` and
/// every `[build] target-dir` on cargo's discovery chain without compiling
/// anything.
///
/// Two things changed with the cache (#335 §9):
///
/// * the bin is resolved by the **derived, range-stamped** name, never by the
///   old `const TRANSFORM_BIN = "forgedb-transform"` — a literal that one app's
///   `transform/` and `engine/` both answered to, so the CLI could run the
///   wrong hop over a user's data dir at exit 0;
/// * a miss is a **hard error naming the cache path**, and there is *no*
///   fallback to `migrations/transform`. The fallback is the tempting move and
///   it is wrong twice: it re-emits a `[package]` with no `[workspace]` under
///   whatever foreign cargo root the CWD sits in (#328 verbatim), and it does
///   so on `migrate engine`, the mandatory upgrade path.
fn locate_member_bin(app: &ResolvedApp, kind: &PackageKind, what: &str) -> Result<PathBuf> {
    let member = app.member(kind);
    let bin_name = app.bin(kind);

    // The member itself missing is a different situation with a different
    // remedy than a member that is present and unbuilt, so it gets its own
    // message rather than being folded into the candidate list below.
    if !member.join("Cargo.toml").is_file() {
        return Err(CliError::Migration(format!(
            "no {what} for this range at {} — nothing has emitted it. Run \
             `forgedb migrate build --schema {} --from <F> --to <T>` first.",
            member.display(),
            app.schema.display()
        )));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    // Asked through the SAME driver that ran the build, at the workspace ROOT —
    // because that is where the build runs and therefore where cargo resolves
    // the target directory from. `migrate` spawns no cargo of its own; if this
    // disagreed with `driver::execute` about the target directory, `run` would
    // look for the bin somewhere `build` never wrote it.
    if let Ok(dir) = crate::commands::build::driver::target_directory(&app.reserved.project) {
        candidates.push(dir.join("release").join(&bin_name));
    }

    // The cache root's own `target/`, in case `cargo metadata` could not run at
    // all (a root that has never been synced). Every location looked in is
    // named if none of them hit — the error that named only one guess is what
    // made this a loop with no exit (#292).
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

/// Run a previously-built transformer over a src→dest data dir (#74 Phase 3).
/// The app must be stopped (offline, exclusive-writer migration — C12).
///
/// `--from`/`--to` are required and are not a convenience: the transformer is a
/// **range-stamped** cache member (`transform-<from>-<to>`), and naming a
/// member without its range is impossible. One `transform/` per app collided
/// across ranges and `run` got whichever built last — a collision that
/// pre-existed at the old shared `migrations/transform` default, but one that
/// moving into a directory the user never opens would have turned from visible
/// into invisible.
pub fn run(opts: MigrateRunOptions) -> Result<()> {
    refuse_removed_flag(
        "--bin-dir",
        opts.removed_bin_dir.as_ref(),
        "Pass `--schema <app.forge> --from <F> --to <T>` instead: those name the exact \
         cache member `migrate build` produced, which a directory alone cannot.",
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

/// `forgedb migrate engine` (#254): carry a data dir across a ForgeDB
/// **byte-format generation** boundary.
///
/// This is NOT the schema-version transformer. The two counters are orthogonal:
/// `migrate build`/`run` replay the app's own `migrations/` lineage, and this
/// replays ForgeDB's engine generations. An engine bump changes no `.forge`,
/// produces no lineage hop, and would therefore run nothing at all through the
/// transformer — which is exactly why it needs its own command rather than a
/// fold-in.
///
/// The hop crate is generated (never schema-blind): a nullable, arrayed, or
/// struct-nested timestamp is written as an opaque `FixedBytes` transmute that no
/// schema-agnostic reader may decode, and 81 of the 247 timestamp fields in the
/// example corpus are nullable. See `forgedb_codegen::engine`.
///
/// **This is the mandatory 0.4.0 upgrade path**, which is why it gets no
/// fallback of any kind: every failure here is a hard error naming the cache
/// path, and the old `-o/--output` default (`migrations/engine`) — a generated
/// `[package]` with no `[workspace]` under whatever cargo root the operator
/// stood in — is #328 verbatim.
pub fn engine(opts: MigrateEngineOptions) -> Result<()> {
    refuse_removed_flag(
        "--output",
        opts.removed_output.as_ref(),
        "Pass `--schema <app.forge>` instead: the hop crate is emitted into that app's \
         build-cache container as `engine-<from>-<to>/`, and the command prints the path.",
    )?;

    let Some((schema_version, from_engine)) = detect_src_versions(&opts.src)? else {
        return Err(CliError::Migration(format!(
            "{} does not look like a ForgeDB data dir (no <model>/manifest.json)",
            opts.src.display()
        )));
    };
    let to_engine = forgedb_codegen::rust::CURRENT_ENGINE_VERSION;

    // Both no-op arms answer before the app is resolved, so a dir that needs no
    // migration is not also a project-id claim and a cache reservation.
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

/// Emit the engine-hop crate as a cache member. The same schema is baked on
/// both sides; only `EXPECTED_ENGINE_VERSION` differs between the two modules.
///
/// Unlike the transformer's, this schema is **not** location-only: an engine
/// bump changes no `.forge`, so the app's current schema is the correct one to
/// bake on both sides of the hop.
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

/// Write one class-C package into the cache, in full.
///
/// **Every file is rewritten on every emission, the manifest included**, and
/// there is deliberately no only-if-absent branch. Every scaffolder in the
/// *output* directory has one, and is right to: those files are the user's.
/// Nothing in the cache is. Carried forward unchanged, a CLI upgrade that bumps
/// a substrate pin would never reach an existing member, and the stale pin
/// would sit in a directory the user never opens — where the publish-gap check
/// cannot see it.
///
/// This is also what retires the `--force` question for these crates: there is
/// no "File exists, use --force" branch, because there is no file here that
/// could be the operator's to protect.
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

/// `forgedb generate transform` — **removed** (#335 §9).
///
/// This is the last caller-facing shape that emitted a `[package]` into a
/// directory of the operator's choosing, and it is the shape of #328: a
/// generated manifest with no `[workspace]` table, landing under whatever
/// foreign cargo root the working directory happened to sit in, which refuses
/// to build. The resolution is the cache root, so there is nowhere left for an
/// arbitrary `--output` to point.
///
/// It survives as a tombstone rather than a deleted target for the same reason
/// every other removal in this design does: an error naming the replacement is
/// the whole value of the removal. Its one caller is
/// `commands::generate::run`'s `"transform"` arm.
pub fn emit_transform(from: u32, to: u32, _output: &Path, _force: bool) -> Result<()> {
    Err(CliError::Migration(format!(
        "`forgedb generate transform` was removed (#335): the transformer is a member of \
         ForgeDB's build cache now, not a crate emitted into a directory you choose. \
         Run `forgedb migrate build --schema <app.forge> --from {from} --to {to}` instead \
         — it prints the cache path it wrote to."
    )))
}

/// Refuse to build any hop in the range that has no answer (#374 step 6).
///
/// The classification and the file-level checks are
/// `forgedb_migrations::hop_answer_status`; this is the CLI's half — the
/// diagnostic, and the decision to check the whole range up front.
///
/// It runs before `emit_transform_crate` parses a schema, so a refused range
/// leaves **nothing** in the cache member and never invokes cargo.
fn refuse_unanswered_hops(
    migrations_dir: &Path,
    hops: &[forgedb_migrations::Migration],
) -> Result<()> {
    let mut lines = Vec::new();
    for m in hops {
        // A pre-#374 record is admitted by the legacy rule inside
        // `hop_answer_status`; say so once, here, rather than letting the
        // operator discover the format bump from a diff.
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

/// Generate the transformer crate for a version range and write it into the
/// app's cache container as `transform-<from>-<to>/` (#74 Phase 3, #335 §9).
///
/// Loads the committed lineage **from the app's own `migrations/`**, expands the
/// contiguous range (C1), parses each version's committed full schema, builds
/// the frozen per-hop plan, and emits the crate.
///
/// Nothing here reads the app's current schema: the versioned snapshots are the
/// input, so a drifted HEAD schema cannot change a historical hop.
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

    // #374 step 6 — REFUSE an unanswered hop, over the WHOLE range, before a
    // line is generated and before cargo is ever invoked.
    //
    // Range-wide rather than per-hop: a range containing one unanswered hop
    // cannot produce a correct migration, so failing at the first one after
    // emitting the others would leave a half-written member in the cache.
    refuse_unanswered_hops(&migrations_dir, &hop_migrations)?;

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

    let package = app.package(kind);
    let crate_out = TransformGenerator::generate(&plan, &package)
        .map_err(|e| CliError::CodeGeneration(e.to_string()))?;

    write_class_c_member(app, kind, &crate_out)
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
        copies: Vec<(String, String)>,
        null_fills: Vec<(String, String)>,
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
                default_json,
                answer,
                ..
            } => {
                let entry = acc.entry(model_name.clone()).or_default();
                match lower_fill(*nullable, default_json.as_deref(), answer.as_ref()) {
                    Some(Fill::Json(json)) => entry.adds.push((field_name.clone(), json)),
                    Some(Fill::Copy(from)) => entry.copies.push((from, field_name.clone())),
                    // Nothing to emit: an unanswered required add. The key is
                    // absent, so the destination decode fails NAMING the field
                    // rather than accepting a substituted zero. Reachable only
                    // if the refusal above was bypassed.
                    None => {}
                }
            }
            // A narrowing to NOT NULL: the answer is the fill for the existing
            // `None`s, written over the key only when it is null.
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
            // #438 — named explicitly rather than absorbed by the catch-all
            // below, because "a new variant silently swallowed by `_ => {}`" is
            // exactly the silent-capability-hole shape: a skipped branch reads
            // as a missing feature rather than as an error.
            //
            // They need NO structural op, and that is a positive fact, not a
            // gap. The transformer's hop reads every row through the `v_from`
            // typed structs, serializes to JSON, and decodes into `v_to`
            // (`crates/codegen/src/transform.rs`) — and an enum crosses that
            // boundary as its variant **name**, a struct as its field
            // **names**. So the re-encode to the new discriminant / the new
            // offsets is performed by the identity body already. Emitting a
            // rename/remove/add op here would be re-implementing a path that
            // works.
            SchemaChange::ChangeEnumVariants { .. }
            | SchemaChange::ChangeStructLayout { .. } => {}
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
            field_copies: a.copies,
            field_null_fills: a.null_fills,
        })
        .collect()
}

/// What a hop emits for one field's value.
enum Fill {
    /// A JSON literal inserted verbatim.
    Json(String),
    /// A per-row copy of another field of the same model.
    Copy(String),
}

/// Lower the ONE source of a field's value into what the hop emits (#374).
///
/// The precedence is not arbitrary and there is deliberately no merging:
///
/// 1. the schema's `@default`, already lowered by
///    `forgedb_codegen::default_fill` and recorded as a JSON literal;
/// 2. the operator's recorded answer;
/// 3. `null` for a nullable add — the only value a nullable field can be given
///    without asking;
/// 4. nothing. A required add with no default and no answer contributes no op,
///    so the key is ABSENT and the destination decode fails with `missing
///    field`. Reachable only if `refuse_unanswered_hops` was bypassed, which is
///    exactly what scenario 15 does on purpose.
///
/// `Answer::Escape` returns `None` here too: an escape's value comes from the
/// author's own transform, which runs after these structural ops.
fn lower_fill(
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

/// True for a field that occupies a column on disk.
///
/// Pure collection relations (`[Model]` one-to-many / many-to-many) carry no
/// column, so adding or removing one is not a data migration and must not
/// register as a field change.  FK scalars (`*Model` / `?Model`) DO carry a
/// `Uuid` column, so they are kept.
fn is_storage_backed(field: &forgedb_parser::Field) -> bool {
    use forgedb_parser::{FieldType, RelationType};
    !matches!(
        &field.field_type,
        FieldType::Relation(RelationType::OneToMany(_))
            | FieldType::Relation(RelationType::ManyToMany(_))
    )
}

/// Every `enum` / `struct` name this type reaches, **transitively** (#438).
///
/// The walk descends `Nullable`, `FixedArray`, `StructType`/`OptionalStructType`
/// and `Enum`, carrying a visited set so a struct that (illegally) recurses
/// cannot spin.
///
/// Depth is the trap and the reason this is a walk rather than a match. An enum
/// is `is_fixed_size`, so it may sit inside an inline struct, and a struct may
/// contain a struct. When the innermost enum changes, the *outer* field's own
/// declaration text does not move at all — so a dependency list that stops at
/// depth one is the same blindness #438 is about, one level down.
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

/// Project one AST type onto the differ's [`SimpleType`], with its
/// **nullability removed** (#374 steps 1 and 2).
///
/// Two jobs, one walk, because they are the same walk.
///
/// **Nullability is stripped.** It is carried by `SimpleField.nullable` and by
/// nothing else, and it reaches the AST three different ways — `T?`, `?Model`,
/// `Struct?` — all three unwrapped here. Before this, the projected type kept
/// the `Nullable` wrapper, so editing `views: u32` to `views: u32?` moved *two*
/// projected values and the differ emitted a `ChangeFieldNullability` **and** a
/// spurious `ChangeFieldType`. The second classifies `Authored`, so the safest
/// edit in the language demanded a hand-written Rust transform for a type that
/// did not change (gate 1 finding 6, decision 6).
///
/// **The result is structured, not a `Debug` string.** `format!("{:?}", ty)`
/// could answer "same or not" and nothing else, which is all the *differ*
/// needed and not what the *classifier* needs: `u32 -> u64` maps every value
/// unchanged and there is nothing for a human to decide. This is the only place
/// the AST is read for the differ, which is why `SimpleType` can live in
/// `crates/migrations` with no parser dependency.
///
/// The match is **exhaustive on purpose** — no catch-all arm. A new
/// `FieldType` variant must be classified here rather than silently becoming
/// whatever the fallback said.
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
        // The rank IS the quantum order (#254): coarser -> finer is a widening,
        // finer -> coarser floors stored values.
        F::Timestamp(P::Seconds) => S::Timestamp(0),
        F::Timestamp(P::Millis) => S::Timestamp(1),
        F::Timestamp(P::Micros) => S::Timestamp(2),
        F::Json => S::Json,
        F::Decimal => S::Decimal,
        F::Enum(n) => S::Enum(n.clone()),
        F::StructType(n) => S::Struct(n.clone()),
        F::FixedArray(inner, n) => S::Array(Box::new(to_simple_type(inner)), *n),
        F::Relation(R::RequiredReference(m)) => S::Relation(m.clone()),
        // Filtered out by `is_storage_backed` before it reaches the differ; kept
        // so this projection is TOTAL rather than needing a catch-all that would
        // absorb a future variant silently.
        F::Relation(R::OneToMany(m)) | F::Relation(R::ManyToMany(m)) => S::Collection(m.clone()),
        // A component reference is virtual — it names a UI artifact, occupies no
        // column, and has no structure the differ can reason about. Kept opaque
        // rather than given a variant, which preserves exactly what it projected
        // to before #374 (an unstructured `Debug` string) and keeps any change to
        // one classified `Authored`.
        F::Component(c) => S::Opaque(format!("component {c:?}")),
    }
}

/// Project one AST field onto the differ's `SimpleField`, resolving its
/// transitive enum/struct dependencies against `schema`.
fn to_simple_field(
    schema: &forgedb_parser::Schema,
    f: &forgedb_parser::Field,
) -> forgedb_migrations::SimpleField {
    let mut seen = std::collections::HashSet::new();
    let mut depends_on = Vec::new();
    type_dependencies(schema, &f.field_type, &mut seen, &mut depends_on);
    forgedb_migrations::SimpleField {
        name: f.name.clone(),
        // For an enum/struct field this carries only the NAME
        // (`enum Status`), which is exactly why `depends_on` below has to
        // exist: the definition changing does not move this value.
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

/// Parse a `.forge` source and project it onto the migration differ's
/// `SimpleSchema`.  Only **storage-backed** fields are included (see
/// [`is_storage_backed`]).
///
/// The `enums` / `structs` projections (#438) are what let the differ see a
/// definition change at all: a field's type string names the enum/struct but
/// carries none of its contents, so two schemas differing only in `Status`'s
/// variant order project onto models that compare **equal**.
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
            // Declaration order, verbatim — it IS the stored discriminant map.
            variants: e.variants.clone(),
        })
        .collect();
    let structs = schema
        .structs
        .iter()
        .map(|s| forgedb_migrations::SimpleStruct {
            name: s.name.clone(),
            // Declaration order, verbatim — it IS the `#[repr(C)]` layout.
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

/// Print the offline data-migration next steps for a recorded hop that rewrites
/// data-at-rest (#74 Phase 4).  `has_authored` toggles the "author the body" step.
///
/// It stops teaching `forgedb migrate up`, which no longer exists (#335 §9,
/// #373). This text is how an operator learns the lifecycle, so a removed
/// command left in it is a runbook that dead-ends — every step names `--schema`
/// for the same reason.
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

    let mut changes = forgedb_migrations::SchemaDiffer::diff(&old_simple, &new_simple);
    resolve_schema_defaults(&new_schema, &mut changes);
    Ok(changes)
}

/// Fill each `AddField`'s `default_json` from the destination field's
/// `@default` directive (#374 step 4).
///
/// It runs **here, in the CLI**, and not in the differ, for the same reason
/// `depends_on` is populated here (#438): `crates/migrations` is a pure
/// comparison over its own value types and has no parser dependency, so it
/// cannot see a directive's typed parameters — `SimpleConstraint` carries them
/// stringified. The one definition of what a `@default` lowers to is
/// `forgedb_codegen::default_fill`, and this is one of its two call sites; the
/// other is the generated reopen backfill.
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
