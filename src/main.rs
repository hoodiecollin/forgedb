use clap::{Parser, Subcommand};

// The binary links the LIBRARY rather than re-declaring the same modules, so
// each is compiled once. Re-declaring them made every `pub` item that only the
// library's consumers use — the tests, and #335's callers — read as dead code in
// the binary, which is a warning that can only be silenced by an `#[allow]` that
// then outlives its reason and hides a real hole.
use forgedb::{cache, commands, error, naming, project, targets, ui};

use error::Result;

#[derive(Parser)]
#[command(name = "forgedb")]
#[command(author, version, about = "ForgeDB - Type-safe database from schemas", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Path to forgedb.toml config file
    #[arg(short, long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new ForgeDB project
    Init {
        /// Project name
        project_name: String,

        /// Use a template (blog, ecommerce, todo, blank)
        #[arg(short, long)]
        template: Option<String>,

        /// REMOVED (#335). Kept in the parser ONLY so it can be refused by
        /// name: `init` no longer scaffolds a cargo package at all, so a flag
        /// that selected one has nothing left to select. Deleting it outright
        /// would give clap's generic "unexpected argument", which names no
        /// replacement — and a flag that reads as applied and is not is the
        /// exact failure #335 deletes everywhere else. `init::refuse_removed_flags`
        /// owns the diagnostic.
        #[arg(long, hide = true)]
        rust: bool,

        /// REMOVED (#335) — see `--rust` above. Narrow what is generated with
        /// `[generate].targets` in `forgedb.toml` instead.
        #[arg(long, hide = true)]
        api_only: bool,

        /// Stand alone: the schemas here form their own project, even inside an
        /// existing one.  Mutually exclusive with `--no-isolated`; omit both to
        /// join an enclosing project when there is one.
        #[arg(long, group = "grouping")]
        isolated: bool,

        /// Join the enclosing project, sharing its build cache and lockfile.
        #[arg(long, group = "grouping")]
        no_isolated: bool,
    },

    /// Generate code from schema
    Generate {
        /// Target: a runtime (`python`, `node`, `bun`, `browser`) paired with a
        /// `--sdk`/`--runtime`/`--replica` mode, or a standalone artifact
        /// (`all`, `rust`, `api`, `openapi`, `stubs`, `ffi`, `transform`).
        #[arg(default_value = "all")]
        target: String,

        /// Mode (runtime targets): network REST client
        #[arg(long, group = "gen_mode")]
        sdk: bool,

        /// Mode (runtime targets): in-process native FFI binding
        #[arg(long, group = "gen_mode")]
        runtime: bool,

        /// Mode (runtime targets): in-process read-replica follower
        #[arg(long, group = "gen_mode")]
        replica: bool,

        /// Verify nothing needs regeneration (CI mode)
        #[arg(long)]
        check: bool,

        /// Output directory
        #[arg(short, long)]
        output: Option<String>,

        /// Schema file path
        #[arg(short, long)]
        schema: Option<String>,

        /// Force regeneration even if up-to-date
        #[arg(short, long)]
        force: bool,

        /// Origin format version for the `transform` target (#74)
        #[arg(long)]
        from: Option<u32>,

        /// Destination format version for the `transform` target (#74)
        #[arg(long)]
        to: Option<u32>,
    },

    /// Validate schema and check implementations
    Validate {
        /// Fail on unimplemented computed/views
        #[arg(long)]
        strict: bool,

        /// Only validate schema syntax
        #[arg(long)]
        schema_only: bool,

        /// Check computed field implementations (not yet enforced — see task #46)
        #[arg(long)]
        implementations: bool,

        /// Check UI component files (tsx://, jsx://, api:// references)
        #[arg(long)]
        components: bool,

        /// Schema file path
        #[arg(short, long)]
        schema: Option<String>,
    },

    /// Build production-ready artifacts
    Build {
        /// Build with optimizations (default)
        #[arg(long, default_value = "true")]
        release: bool,

        /// REMOVED (#335). Kept only so passing it errors with the replacement
        /// named, rather than clap's `unexpected argument`: what gets built is
        /// decided by `[generate].targets`, and a cargo `--target` is
        /// invocation-wide, so `--target wasm` never meant "also build the
        /// browser replica".
        #[arg(short, long, hide = true)]
        target: Option<String>,

        /// Output directory
        #[arg(short, long)]
        output: Option<String>,

        /// Schema file path
        #[arg(long)]
        schema: Option<String>,

        /// Skip API server build
        #[arg(long)]
        no_api: bool,

        /// Print the cargo invocations and compile nothing
        #[arg(long)]
        plan: bool,

        /// Write the machine-readable artifact report to PATH (`-` = stdout)
        #[arg(long, value_name = "PATH")]
        report: Option<String>,

        /// Print exactly one absolute artifact path for KIND (core, server, ffi, napi, pyo3, wasm)
        #[arg(long, value_name = "KIND")]
        print_artifact: Option<String>,
    },

    /// Watch schema file and auto-regenerate on changes
    Dev {
        /// Schema file to watch
        ///
        /// No clap default: the schema is FOUND by `project::find_schema` and the
        /// output is RESOLVED against it, exactly as `generate` does. A literal
        /// `"schema.forge"` / `"generated"` default here is what let `dev` bypass
        /// the config walk entirely (#364).
        #[arg(short, long)]
        schema: Option<String>,

        /// Output directory for generated code
        #[arg(short, long)]
        output: Option<String>,

        /// Debounce delay in milliseconds
        #[arg(short, long, default_value = "200")]
        debounce: u64,

        /// Clear terminal on each regeneration
        #[arg(long, default_value = "true")]
        clear: bool,
    },

    /// Database migration commands
    #[command(subcommand)]
    Migrate(MigrateCommands),

    /// Database compaction and maintenance commands
    #[command(subcommand)]
    Compact(CompactCommands),

    /// Snapshot backup & restore for a database directory
    #[command(subcommand)]
    Backup(BackupCommands),

    /// Manage physical, dir-per-tenant data directories (#59 multi-tenancy)
    #[command(subcommand)]
    Tenant(TenantCommands),

    /// Run the ForgeDB language server over stdio (used by editor extensions).
    ///
    /// Thin launcher for the sibling `forgedb-lsp` binary shipped alongside the
    /// CLI — keeps the async LSP stack out of the main binary (epic #173).
    Lsp {
        /// Explicit path to the `forgedb-lsp` server binary (otherwise resolved
        /// next to `forgedb`, then on PATH)
        #[arg(long)]
        server_path: Option<std::path::PathBuf>,

        /// Extra arguments forwarded to the language server
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Start the Tier 3 MVCC commit coordinator (multi-process writers, #84)
    Coordinate {
        /// Data root directory to coordinate.
        root: std::path::PathBuf,

        /// Unix socket path.  Default: <root>/_coord.sock
        #[arg(short, long)]
        socket: Option<std::path::PathBuf>,

        /// Replication-log fsync policy (#156): `always` (default, max durability),
        /// `never` (rely on the OS; a crash rewinds replication only — no committed
        /// client data lost), or `periodic` (fsync once per `--fsync-interval`
        /// commits). Overridable via `FORGEDB_COORDINATOR_FSYNC`.
        #[arg(long)]
        fsync: Option<String>,

        /// Commits per fsync when `--fsync periodic` (default 64). Overridable via
        /// `FORGEDB_COORDINATOR_FSYNC_INTERVAL`.
        #[arg(long)]
        fsync_interval: Option<u64>,

        /// Seconds a granted turn may stay un-committed before it is reclaimed
        /// (#144, default 30). Governs multi-process write fairness vs
        /// head-of-line blocking. Overridable via `FORGEDB_COORDINATOR_TURN_TIMEOUT`.
        #[arg(long)]
        turn_timeout: Option<u64>,

        /// Max protocol frame in MiB (#145, default 16) — bounds per-turn
        /// write-set size vs memory. Overridable via
        /// `FORGEDB_COORDINATOR_MAX_FRAME_MIB`.
        #[arg(long)]
        max_frame_mib: Option<u64>,
    },
}

#[derive(Subcommand)]
enum MigrateCommands {
    /// Create a new migration
    Create {
        /// Description of the migration
        description: String,

        /// Auto-detect schema changes
        #[arg(short, long)]
        auto: bool,

        /// The app this migration belongs to (REQUIRED — #335 §9).
        ///
        /// Every path derives from it: `migrations/` is read beside this file,
        /// and the build cache is keyed by it. Nothing is discovered from the
        /// working directory any more.
        #[arg(short, long)]
        schema: std::path::PathBuf,
    },

    /// Show migration status
    Status {
        /// The app whose lineage to report (REQUIRED — #335 §9).
        #[arg(short, long)]
        schema: std::path::PathBuf,
    },

    /// Generate + compile the offline transformer bin for a version range (#74).
    /// The one operator artifact that migrates data-at-rest from --from to --to.
    Build {
        /// The app to build the transformer for (REQUIRED — #335 §9).
        ///
        /// It selects the app, and nothing else: the transformer itself is
        /// generated from the committed per-version schemas under the app's
        /// `migrations/`, so a drifted current schema cannot change a
        /// historical hop.
        #[arg(short, long)]
        schema: std::path::PathBuf,

        /// Origin (current on-disk) format version
        #[arg(long)]
        from: u32,

        /// Destination format version
        #[arg(long)]
        to: u32,

        /// REMOVED (#335): the transformer is built as a member of ForgeDB's
        /// own cache workspace. Kept parseable only so that passing it is an
        /// error naming the replacement instead of clap's "unexpected
        /// argument", which names none.
        #[arg(short, long, hide = true)]
        output: Option<std::path::PathBuf>,
    },

    /// Run a built transformer over a src→dest data dir (app must be stopped).
    Run {
        /// The app whose transformer to run (REQUIRED — #335 §9).
        #[arg(short, long)]
        schema: std::path::PathBuf,

        /// Origin format version — REQUIRED, because the transformer is a
        /// range-stamped cache member (`transform-<from>-<to>`) and cannot be
        /// named without its range.
        #[arg(long)]
        from: u32,

        /// Destination format version
        #[arg(long)]
        to: u32,

        /// Source data directory (at the origin format version)
        #[arg(long)]
        src: std::path::PathBuf,

        /// Destination directory to materialize (must not exist / be empty)
        #[arg(long)]
        dest: std::path::PathBuf,

        /// REMOVED (#335): a directory cannot name a range-stamped member.
        /// Kept parseable only so that passing it errors with the replacement.
        #[arg(long, hide = true)]
        bin_dir: Option<std::path::PathBuf>,
    },

    /// Migrate a data dir across a ForgeDB **byte-format generation** (#254).
    ///
    /// Orthogonal to the schema-version transformer: `migrate build`/`run`
    /// replay the app's own `migrations/` lineage, this replays ForgeDB's
    /// engine generations.  An engine bump changes no `.forge`, so it produces
    /// no lineage hop and the transformer would run nothing.  The app must be
    /// STOPPED.
    Engine {
        /// Source data directory (at the old engine generation)
        #[arg(long)]
        src: std::path::PathBuf,

        /// Destination directory to materialize (must not exist / be empty)
        #[arg(long)]
        dest: std::path::PathBuf,

        /// The app (REQUIRED — #335 §9).  Unlike the transformer's, this
        /// schema is not location-only: an engine bump changes no `.forge`, so
        /// the SAME schema is baked on both sides of the hop.
        #[arg(short, long)]
        schema: std::path::PathBuf,

        /// REMOVED (#335): the hop crate is built as a member of ForgeDB's own
        /// cache workspace. Its old default, `migrations/engine`, is exactly
        /// the path that reproduces #328 — on the mandatory upgrade command.
        #[arg(short, long, hide = true)]
        output: Option<std::path::PathBuf>,
    },

    /// REMOVED (#335) — was: build the transformer + run it in one command.
    ///
    /// A tombstone rather than a deleted subcommand, so that an operator
    /// following a runbook is told what to run instead of getting clap's
    /// "unrecognized subcommand". Restoration of the per-tenant sweep is #373.
    #[command(hide = true)]
    Up {
        /// Every flag `migrate up` used to take, swallowed so the tombstone —
        /// and not clap — is what answers.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum TenantCommands {
    /// Create a new tenant's data directory under the tenant root
    Create {
        /// Tenant name (becomes the directory name under the root)
        name: String,
        /// Tenant root directory (default: from forgedb.toml `[tenant].root`, else ./data)
        #[arg(short, long)]
        root: Option<String>,
    },

    /// List existing tenant data directories under the tenant root
    List {
        /// Tenant root directory (default: from forgedb.toml `[tenant].root`, else ./data)
        #[arg(short, long)]
        root: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Remove a tenant's data directory (destructive)
    Drop {
        /// Tenant name to remove
        name: String,
        /// Tenant root directory (default: from forgedb.toml `[tenant].root`, else ./data)
        #[arg(short, long)]
        root: Option<String>,
        /// Skip the confirmation guard
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum CompactCommands {
    /// DEPRECATED (see #105): offline compaction is removed because it is unsafe
    /// against the generated mutation surface (it can resurrect deleted rows).
    /// Use in-process `Database::compact()` instead (auto-invoked). This command
    /// mutates nothing and exits non-zero.
    Run {
        /// Data directory (default: ./data)
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        /// Compact specific model only
        #[arg(short, long)]
        model: Option<String>,

        /// Compact all models (ignore threshold)
        #[arg(short, long)]
        all: bool,

        /// Dead space threshold (0.0-1.0)
        #[arg(short, long)]
        threshold: Option<f64>,
    },

    /// Show database statistics
    Stats {
        /// Data directory (default: ./data)
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        /// Show stats for specific model only
        #[arg(short, long)]
        model: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// DEPRECATED (see #105): alias for the removed offline compaction path.
    /// Use in-process `Database::compact()`. Mutates nothing; exits non-zero.
    Vacuum {
        /// Data directory (default: ./data)
        #[arg(short, long, default_value = "data")]
        data_dir: String,
    },

    /// Analyze database and show recommendations
    Analyze {
        /// Data directory (default: ./data)
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum BackupCommands {
    /// Create a lock-free snapshot archive of a database directory (full, or
    /// an incremental byte-tail delta with `--incremental`)
    Create {
        /// Data directory (default: ./data)
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        /// Output archive path
        #[arg(short, long, default_value = "backup.forgebk")]
        output: String,

        /// Produce an incremental delta against a base archive instead of a
        /// full snapshot. Refused if compaction bumped the epoch since the base.
        #[arg(long)]
        incremental: bool,

        /// Base archive the incremental deltas from (required with
        /// `--incremental`; may itself be an earlier incremental in the chain)
        #[arg(long)]
        base: Option<String>,
    },

    /// Restore a snapshot archive (or a base + incremental chain) into a data
    /// directory. Pass the base first, then each incremental in order.
    Restore {
        /// Archive path(s) to restore. One full archive, or a base followed by
        /// its incrementals in `sequence` order.
        #[arg(required = true, num_args = 1..)]
        archive: Vec<String>,

        /// Destination data directory
        #[arg(short, long, default_value = "data")]
        output: String,

        /// Replace an existing non-empty destination directory
        #[arg(long)]
        overwrite: bool,
    },

    /// List the contents of a snapshot archive without restoring it
    List {
        /// Archive path to inspect
        archive: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

/// Reserve an app's container and report where it is — the BEFORE half of the
/// old `place_in_cache` (#335 §3).
///
/// C7: **every** generate and build prints the cache path it wrote to. While the
/// build happened in a directory the user chose, a silent path was survivable;
/// once it happens in a hashed directory they have never seen, the thing that
/// makes the generator identity self-evident — opening `database.rs` and seeing
/// your own models — is gone unless the path is named.
///
/// Split from [`sync_after_emission`] because rendering the workspace root here
/// would list the packages of the PREVIOUS run: an app's very first `generate`
/// would write a root naming none of its own packages, and `cargo build -p <its
/// core>` would fail with `did not match any packages`.
fn reserve_in_cache(
    project: &project::ProjectId,
    schema: &std::path::Path,
    symbol_naming: naming::SymbolNaming,
) -> Result<cache::Reserved> {
    let reserved = cache::reserve(&project.name, &project.root, schema, symbol_naming)?;
    ui::info(&format!("Build cache: {}", reserved.container.display()));
    Ok(reserved)
}

/// Re-derive the workspace root from what is now on disk — the AFTER half —
/// pruning the packages this app's config no longer declares (#335 §3).
///
/// `declared` is what `[generate].targets` asks for, expressed as package kinds,
/// and **not** what this invocation selected: `forgedb generate rust` under
/// `targets = ["all"]` must leave `napi/` alone. `None` disables the prune
/// entirely, for an invocation that emitted nothing — `--check` compares a
/// scratch directory and touches nothing on disk, so it must delete nothing
/// either.
///
/// The de-list-then-delete order lives in [`cache::prune`], not here: a caller
/// cannot get it wrong because a caller cannot express the other order.
fn sync_after_emission(
    reserved: &cache::Reserved,
    declared: Option<&[naming::PackageKind]>,
) -> Result<cache::Synced> {
    let doomed = match declared {
        Some(declared) => cache::prunable(
            &reserved.container,
            declared,
            naming::PruneOwner::GenerateBuild,
        )?,
        None => Vec::new(),
    };

    let pruned = cache::prune(&reserved.project, &reserved.container, &doomed)?;
    report_sync(
        &reserved.project,
        pruned.synced.lock_dropped,
        &pruned.synced.orphans,
    );
    // Reported at `info`, not `detail`: this is a deletion ForgeDB performed in a
    // directory the user never opens, and the same reasoning as C9's lockfile
    // drop applies — a silent one is indistinguishable from a package that was
    // never emitted.
    for dir in &pruned.removed {
        ui::info(&format!(
            "Pruned (no longer in [generate].targets): {}",
            dir.display()
        ));
    }
    Ok(pruned.synced)
}

/// The app one `migrate` invocation acts on (#335 §9).
///
/// `--schema` is required by the parser on every `migrate` arm, so there is no
/// discovery step and no default to fall back to — a defaulted schema path is
/// the same defect as a CWD-relative `migrations/`, wearing a different name.
/// The global `--config` rides along so `migrate` governs exactly the way
/// `generate` does.
fn migrate_app(
    schema: std::path::PathBuf,
    explicit_config: Option<&str>,
) -> commands::migrate::AppRef {
    commands::migrate::AppRef {
        schema,
        config: explicit_config.map(str::to_string),
    }
}

fn report_sync(project: &std::path::Path, lock_dropped: bool, orphans: &[std::path::PathBuf]) {
    // C9: a CLI upgrade invalidates the project's dependency resolution, and the
    // drop happens in a directory the user never opens — so it is reported.
    if lock_dropped {
        ui::info(&format!(
            "CLI version changed — dropped {}/Cargo.lock so dependencies re-resolve",
            project.display()
        ));
    }
    for orphan in orphans {
        ui::detail(&format!(
            "Orphaned member (its schema is gone): {}",
            orphan.display()
        ));
    }
}

fn run(cli: Cli) -> Result<()> {
    // Wire the global -v/-q flags into the UI output level: --quiet suppresses
    // everything but errors, --verbose unlocks detail lines.
    ui::set_verbosity(cli.verbose, cli.quiet);

    // Config is no longer loaded once, up front, from the CWD (#333): which
    // config governs an app depends on where that app's *schema* lives, so the
    // schema-taking arms below resolve their schema first and walk up from it.
    // `--config` still overrides the knobs outright.
    let explicit_config = cli.config.as_deref();

    match cli.command {
        Commands::Init {
            project_name,
            template,
            rust,
            api_only,
            isolated,
            no_isolated,
        } => commands::init::run(commands::init::InitOptions {
            project_name,
            template,
            rust,
            api_only,
            // clap's ArgGroup guarantees at most one is set, so this is a
            // three-valued answer, not a precedence rule.
            isolated: match (isolated, no_isolated) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
        }),

        Commands::Generate {
            target,
            sdk,
            runtime,
            replica,
            check,
            output,
            schema,
            force,
            from,
            to,
        } => {
            // The schema names itself; no config participates (#333 §10).
            let schema_path = project::find_schema(schema.as_deref())?;
            let governing = project::govern(explicit_config, project::schema_dir(&schema_path))?;
            // Resolved (and claimed) here rather than lazily: a project id is a
            // precondition of generating, so a collision must be refused before
            // any bytes are written, not after.
            let project = governing.identify_reported()?;
            // Reserve BEFORE emission (it needs the path); re-derive the
            // workspace root AFTER, once this app's packages exist (#335 §3).
            let reserved = reserve_in_cache(&project, &schema_path, governing.symbol_naming())?;
            let forge_config = governing.config();
            // Precedence: CLI flag > config > built-in default.  A config's
            // relative `output` is resolved against the SCHEMA's directory, so a
            // root config's `output = "generated"` is a per-app pattern rather
            // than one shared directory every app interleaves into.
            let resolved_output = Some(governing.output(output.as_deref()));
            let resolved_schema = Some(schema_path.display().to_string());
            // The `--sdk`/`--runtime`/`--replica` flags are a clap ArgGroup, so at
            // most one is set — collapse them into the mode axis (#122).
            let mode = if sdk {
                Some(commands::generate::GenerateMode::Sdk)
            } else if runtime {
                Some(commands::generate::GenerateMode::Runtime)
            } else if replica {
                Some(commands::generate::GenerateMode::Replica)
            } else {
                None
            };
            // Generate-time runtime-behavior config (epic #126): resolve the
            // schema-blind [runtime]/[storage] knobs into the codegen GenConfig
            // baked into database.rs. An invalid knob value is a config error.
            let gen_config = forge_config.gen_config()?;
            // Canonical internal names, never the raw user spellings (#335 §12,
            // decision 10). The warnings are the deprecated pre-#122 values; a
            // deprecated value that behaves identically and says nothing is how
            // the config and CLI vocabularies drifted apart to begin with.
            let (config_targets, target_warnings) = forge_config.resolved_targets()?;
            for warning in &target_warnings {
                ui::warning(warning);
            }
            // What the package prune judges (#335 §3 rule 3): the DECLARED set,
            // never the target THIS invocation selected — `generate rust` under
            // `targets = ["all"]` must leave `napi/` alone. Read here rather
            // than inside `generate::run`, where `config_targets` is consumed
            // only by the `"all"` arm: a prune wired off the value that function
            // receives would see `None` for every single-target invocation,
            // which is exactly the set of invocations that can narrow anything.
            //
            // `--check` writes nothing (it compares a scratch dir and removes
            // it), so it must delete nothing: no prune.
            let declared = (!check).then(|| targets::declared_packages(&config_targets));
            commands::generate::run(commands::generate::GenerateOptions {
                target,
                mode,
                check,
                output: resolved_output,
                schema: resolved_schema,
                config_targets: Some(config_targets),
                gen_config,
                force,
                from,
                to,
                cache_container: Some(reserved.container.clone()),
                in_tree: governing.rust_package(),
            })?;
            sync_after_emission(&reserved, declared.as_deref())?;
            Ok(())
        }

        Commands::Validate {
            strict,
            schema_only,
            implementations,
            components,
            schema,
        } => {
            let resolved_schema = Some(
                project::find_schema(schema.as_deref())?
                    .display()
                    .to_string(),
            );
            commands::validate::run(commands::validate::ValidateOptions {
                strict,
                schema_only,
                implementations,
                components,
                schema: resolved_schema,
            })
        }

        Commands::Build {
            release,
            target,
            output,
            schema,
            no_api,
            plan,
            report,
            print_artifact,
        } => {
            // In the two machine-readable modes stdout belongs to the consumer —
            // a `$(…)` capturing one path, or a JSON document being piped — so
            // every human-facing line is silenced BEFORE the first one is
            // printed. `identify_reported` and `reserve_in_cache` below both
            // print, which is why this is the first statement in the arm rather
            // than something `build::run` could do for itself.
            //
            // Silenced rather than redirected: `ui` writes to stdout by design
            // (its warnings are part of `validate`'s report), and moving the
            // module wholesale to stderr would change every command instead of
            // this one. `ui::error` already goes to stderr, so a failure is
            // still visible.
            if commands::build::stdout_is_machine_readable(
                print_artifact.as_deref(),
                report.as_deref(),
            ) {
                ui::set_verbosity(false, true);
            }
            // Resolved exactly as `Commands::Generate` does, from the same walk —
            // `build` must compile in the same place `generate` emitted into, and
            // must bake what `generate` would (#361, extended to identity by #333).
            let schema_path = project::find_schema(schema.as_deref())?;
            let governing = project::govern(explicit_config, project::schema_dir(&schema_path))?;
            let project = governing.identify_reported()?;
            let reserved = reserve_in_cache(&project, &schema_path, governing.symbol_naming())?;
            let forge_config = governing.config();
            let resolved_output = Some(governing.output(output.as_deref()));
            let resolved_schema = Some(schema_path.display().to_string());
            // `build` reaches every declared target (#335 §12). It used to pass
            // `config_targets: None`, which made every opt-in arm of
            // `generate_all` unreachable from `forgedb build` entirely.
            let (config_targets, target_warnings) = forge_config.resolved_targets()?;
            for warning in &target_warnings {
                ui::warning(warning);
            }
            // Same declared-set prune as `Commands::Generate` (#335 §3 rule 3);
            // `build` owns the same package kinds `generate` does.
            let declared = targets::declared_packages(&config_targets);
            let container = reserved.container.clone();
            let project_root = reserved.project.clone();
            let gen_config = forge_config.gen_config()?;
            // Unlike `Generate`, the AFTER half runs in the MIDDLE of the
            // command rather than after it: `build` emits the packages it is
            // about to compile, and the root manifest cargo reads is rendered
            // from a scan of what is on disk. Synced only after `build::run`
            // returned, an app's very first `forgedb build` would hand cargo the
            // previous run's member list — `error: package(s) … not found`, on a
            // cold cache only. Handed in as the same function `Generate` calls,
            // never a second derivation of it (`dev` does the same).
            let sync: commands::dev::SyncHook =
                Box::new(move || sync_after_emission(&reserved, Some(&declared)).map(|_| ()));
            commands::build::run(commands::build::BuildOptions {
                release,
                target,
                output: resolved_output,
                schema: resolved_schema,
                no_api,
                config_targets,
                // Same resolution as `Commands::Generate` above, from the same
                // loaded config — `build` must bake what `generate` would (#361).
                gen_config,
                cache_container: Some(container),
                in_tree: governing.rust_package(),
                cache_project: Some(project_root),
                plan_only: plan,
                report,
                print_artifact,
                sync: Some(sync),
            })?;
            Ok(())
        }

        Commands::Dev {
            schema,
            output,
            debounce,
            clear,
        } => {
            // Resolved by the SAME chain `Commands::Generate` runs, because a
            // watch-driven regeneration must be the same emission a hand-run
            // `forgedb generate` produces (#364). `dev` reached
            // `forgedb_watcher::auto_watch` directly before this, so it read no
            // config at all: every save rewrote `database.rs` with
            // `GenConfig::DEFAULT` and `schema_version = 1`.
            let schema_path = project::find_schema(schema.as_deref())?;
            let governing = project::govern(explicit_config, project::schema_dir(&schema_path))?;
            let project = governing.identify_reported()?;
            let reserved = reserve_in_cache(&project, &schema_path, governing.symbol_naming())?;
            let forge_config = governing.config();
            let resolved_output = governing.output(output.as_deref());
            let gen_config = forge_config.gen_config()?;
            let (config_targets, target_warnings) = forge_config.resolved_targets()?;
            for warning in &target_warnings {
                ui::warning(warning);
            }
            let declared = targets::declared_packages(&config_targets);
            let container = reserved.container.clone();
            // `dev` blocks in the watch loop until Ctrl+C, so there is no "after
            // `dev::run`" in which to re-derive the workspace root. The AFTER half
            // is handed in and runs after each regeneration instead — the same
            // function `Generate`/`Build` call, never a second derivation of it.
            let sync: commands::dev::SyncHook =
                Box::new(move || sync_after_emission(&reserved, Some(&declared)).map(|_| ()));
            commands::dev::run(commands::dev::DevOptions {
                generate: commands::dev::DevGenerate {
                    schema: schema_path.display().to_string(),
                    output: resolved_output,
                    config_targets,
                    gen_config,
                    cache_container: Some(container),
                    in_tree: governing.rust_package(),
                },
                debounce,
                clear,
                sync,
            })
        }

        // Every `migrate` arm takes ONE required `--schema` (#335 §9): it is
        // the app selector, and `migrate.rs` runs the same
        // `govern -> identify_reported -> reserve` chain the Generate arm runs
        // from it. Nothing here is resolved from the working directory.
        Commands::Migrate(migrate_cmd) => match migrate_cmd {
            MigrateCommands::Create {
                description,
                auto,
                schema,
            } => commands::migrate::create(commands::migrate::MigrateCreateOptions {
                app: migrate_app(schema, explicit_config),
                description,
                auto,
            }),
            MigrateCommands::Status { schema } => {
                commands::migrate::status(commands::migrate::MigrateStatusOptions {
                    app: migrate_app(schema, explicit_config),
                })
            }
            MigrateCommands::Build {
                schema,
                from,
                to,
                output,
            } => commands::migrate::build(commands::migrate::MigrateBuildOptions {
                app: migrate_app(schema, explicit_config),
                from,
                to,
                removed_output: output,
            }),
            MigrateCommands::Run {
                schema,
                from,
                to,
                src,
                dest,
                bin_dir,
            } => commands::migrate::run(commands::migrate::MigrateRunOptions {
                app: migrate_app(schema, explicit_config),
                from,
                to,
                src,
                dest,
                removed_bin_dir: bin_dir,
            }),
            MigrateCommands::Engine {
                src,
                dest,
                schema,
                output,
            } => commands::migrate::engine(commands::migrate::MigrateEngineOptions {
                app: migrate_app(schema, explicit_config),
                src,
                dest,
                removed_output: output,
            }),
            // Swallowed args and all: the tombstone answers, not clap.
            MigrateCommands::Up { args: _ } => commands::migrate::up(),
        },

        Commands::Lsp { server_path, args } => {
            commands::lsp::run(commands::lsp::LspOptions { server_path, args })
        }

        Commands::Coordinate {
            root,
            socket,
            fsync,
            fsync_interval,
            turn_timeout,
            max_frame_mib,
        } => commands::coordinate::run(commands::coordinate::CoordinateOptions {
            root,
            socket,
            fsync,
            fsync_interval,
            turn_timeout_secs: turn_timeout,
            max_frame_mib,
        }),

        Commands::Compact(compact_cmd) => match compact_cmd {
            CompactCommands::Run {
                data_dir,
                model,
                all,
                threshold,
            } => commands::compact::compact(commands::compact::CompactOptions {
                data_dir: data_dir.into(),
                model,
                all,
                threshold,
            }),
            CompactCommands::Stats {
                data_dir,
                model,
                json,
            } => commands::compact::stats(commands::compact::StatsOptions {
                data_dir: data_dir.into(),
                model,
                json,
            }),
            CompactCommands::Vacuum { data_dir } => {
                commands::compact::vacuum(commands::compact::VacuumOptions {
                    data_dir: data_dir.into(),
                })
            }
            CompactCommands::Analyze { data_dir, json } => {
                commands::compact::analyze(commands::compact::AnalyzeOptions {
                    data_dir: data_dir.into(),
                    json,
                })
            }
        },

        Commands::Backup(backup_cmd) => match backup_cmd {
            BackupCommands::Create {
                data_dir,
                output,
                incremental,
                base,
            } => commands::backup::create(commands::backup::CreateOptions {
                data_dir: data_dir.into(),
                output: output.into(),
                incremental,
                base: base.map(Into::into),
            }),
            BackupCommands::Restore {
                archive,
                output,
                overwrite,
            } => commands::backup::restore(commands::backup::RestoreOptions {
                archives: archive.into_iter().map(Into::into).collect(),
                output: output.into(),
                overwrite,
            }),
            BackupCommands::List { archive, json } => {
                commands::backup::list(commands::backup::ListOptions {
                    archive: archive.into(),
                    json,
                })
            }
        },

        Commands::Tenant(tenant_cmd) => {
            // No schema to start from, so the walk starts at the CWD — which
            // reproduces the old CWD-only lookup as a special case of it.
            let governing = project::govern_cwd(explicit_config)?;
            let forge_config = governing.config();
            // Root precedence: --root flag > [tenant].root in config > ./data.
            let default_root = forge_config.tenant.root();
            // C4: refuse a data root that RESOLVES inside the build cache. The
            // trap is not a bad decision, it is a relative default — nobody
            // writes "put the database in the build cache", they run something
            // whose working directory is the cache dir and `[tenant].root`'s
            // `"data"` does it for them. Checking the configured value catches
            // nothing, because the dangerous case is the one where nothing was
            // configured at all.
            let resolve = |root: Option<String>| -> Result<std::path::PathBuf> {
                let resolved = root
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| default_root.clone());
                cache::assert_not_in_cache(&resolved)?;
                Ok(resolved)
            };
            match tenant_cmd {
                TenantCommands::Create { name, root } => {
                    commands::tenant::create(commands::tenant::CreateOptions {
                        name,
                        root: resolve(root)?,
                        auth_env: forge_config.auth.env_exports(),
                    })
                }
                TenantCommands::List { root, json } => {
                    commands::tenant::list(commands::tenant::ListOptions {
                        root: resolve(root)?,
                        json,
                    })
                }
                TenantCommands::Drop { name, root, force } => {
                    commands::tenant::drop(commands::tenant::DropOptions {
                        name,
                        root: resolve(root)?,
                        force,
                    })
                }
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(e.exit_code());
    }
}
