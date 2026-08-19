use clap::{Parser, Subcommand};

mod cache;
mod commands;
mod config;
mod diagnostics;
mod error;
mod project;
mod templates;
mod ui;

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

        /// Include Rust backend
        #[arg(long)]
        rust: bool,

        /// Generate API only
        #[arg(long)]
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

        /// Build target (native, wasm, both)
        #[arg(short, long, default_value = "native")]
        target: String,

        /// Output directory
        #[arg(short, long)]
        output: Option<String>,

        /// Schema file path
        #[arg(long)]
        schema: Option<String>,

        /// Skip API server build
        #[arg(long)]
        no_api: bool,
    },

    /// Watch schema file and auto-regenerate on changes
    Dev {
        /// Schema file to watch
        #[arg(short, long, default_value = "schema.forge")]
        schema: String,

        /// Output directory for generated code
        #[arg(short, long, default_value = "generated")]
        output: String,

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

        /// Schema file to diff against the recorded snapshot (for --auto)
        #[arg(short, long)]
        schema: Option<std::path::PathBuf>,
    },

    /// Show migration status
    Status,

    /// Generate + compile the offline transformer bin for a version range (#74).
    /// The one operator artifact that migrates data-at-rest from --from to --to.
    Build {
        /// Origin (current on-disk) format version
        #[arg(long)]
        from: u32,

        /// Destination format version
        #[arg(long)]
        to: u32,

        /// Where to emit + build the transformer (default: migrations/transform)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },

    /// Run a built transformer over a src→dest data dir (app must be stopped).
    Run {
        /// Source data directory (at the origin format version)
        #[arg(long)]
        src: std::path::PathBuf,

        /// Destination directory to materialize (must not exist / be empty)
        #[arg(long)]
        dest: std::path::PathBuf,

        /// The transformer crate dir (default: migrations/transform)
        #[arg(long)]
        bin_dir: Option<std::path::PathBuf>,
    },

    /// Migrate a data dir across a ForgeDB **byte-format generation** (#254).
    ///
    /// Orthogonal to the schema-version transformer: `migrate up` replays the
    /// app's own `migrations/` lineage, this replays ForgeDB's engine
    /// generations.  An engine bump changes no `.forge`, so it produces no
    /// lineage hop and `migrate up` would run nothing.  The app must be STOPPED.
    Engine {
        /// Source data directory (at the old engine generation)
        #[arg(long)]
        src: std::path::PathBuf,

        /// Destination directory to materialize (must not exist / be empty)
        #[arg(long)]
        dest: std::path::PathBuf,

        /// Schema file (default: auto-discovered).  The SAME schema is baked on
        /// both sides — an engine bump changes no `.forge`.
        #[arg(long)]
        schema: Option<std::path::PathBuf>,

        /// Where to emit + build the hop crate (default: migrations/engine)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },

    /// One-CLI migration: build the transformer + run it over a data dir (or every
    /// tenant dir under --tenant-root).  The app must be STOPPED.  #74 Phase 4.
    Up {
        /// Origin format version (default: detected from the source manifests)
        #[arg(long)]
        from: Option<u32>,

        /// Destination format version (default: the lineage's current version)
        #[arg(long)]
        to: Option<u32>,

        /// Source data directory (single-dir mode)
        #[arg(long)]
        src: Option<std::path::PathBuf>,

        /// Destination directory to materialize (single-dir mode)
        #[arg(long)]
        dest: Option<std::path::PathBuf>,

        /// Transformer crate dir (default: migrations/transform)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,

        /// Migrate every data dir directly under this root (per-tenant sweep)
        #[arg(long)]
        tenant_root: Option<std::path::PathBuf>,

        /// Per-tenant destination suffix (default: -migrated-v<to>)
        #[arg(long)]
        dest_suffix: Option<String>,
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

/// The directory a schema's governing config is walked up from.
///
/// A bare `schema.forge` has no parent component, and walking from `""` would
/// resolve against the filesystem root rather than the CWD — which is the
/// difference between "the config beside my schema" and "any config on the
/// machine".
fn schema_dir(schema: &std::path::Path) -> &std::path::Path {
    match schema.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
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
            let governing = project::govern(explicit_config, schema_dir(&schema_path))?;
            // Resolved (and claimed) here rather than lazily: a project id is a
            // precondition of generating, so a collision must be refused before
            // any bytes are written, not after.
            let _project = governing.identify_reported()?;
            let forge_config = governing.config();
            // Precedence: CLI flag > config > built-in default.  A config's
            // relative `output` is resolved against the SCHEMA's directory, so a
            // root config's `output = "generated"` is a per-app pattern rather
            // than one shared directory every app interleaves into.
            let resolved_output = output.or_else(|| {
                forge_config
                    .generate
                    .output
                    .as_deref()
                    .map(|o| governing.resolve_path(o).display().to_string())
            });
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
            commands::generate::run(commands::generate::GenerateOptions {
                target,
                mode,
                check,
                output: resolved_output,
                schema: resolved_schema,
                config_targets: forge_config.generate.targets.clone(),
                gen_config,
                force,
                from,
                to,
            })
        }

        Commands::Validate {
            strict,
            schema_only,
            implementations,
            components,
            schema,
        } => {
            let resolved_schema = Some(project::find_schema(schema.as_deref())?.display().to_string());
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
        } => {
            // Resolved exactly as `Commands::Generate` does, from the same walk —
            // `build` must compile in the same place `generate` emitted into, and
            // must bake what `generate` would (#361, extended to identity by #333).
            let schema_path = project::find_schema(schema.as_deref())?;
            let governing = project::govern(explicit_config, schema_dir(&schema_path))?;
            let _project = governing.identify_reported()?;
            let forge_config = governing.config();
            let resolved_output = output.or_else(|| {
                forge_config
                    .generate
                    .output
                    .as_deref()
                    .map(|o| governing.resolve_path(o).display().to_string())
            });
            let resolved_schema = Some(schema_path.display().to_string());
            commands::build::run(commands::build::BuildOptions {
                release,
                target,
                output: resolved_output,
                schema: resolved_schema,
                no_api,
                // Same resolution as `Commands::Generate` above, from the same
                // loaded config — `build` must bake what `generate` would (#361).
                gen_config: forge_config.gen_config()?,
            })
        }

        Commands::Dev {
            schema,
            output,
            debounce,
            clear,
        } => commands::dev::run(commands::dev::DevOptions {
            schema,
            output,
            debounce,
            clear,
        }),

        Commands::Migrate(migrate_cmd) => match migrate_cmd {
            MigrateCommands::Create { description, auto, schema } => {
                commands::migrate::create(commands::migrate::MigrateCreateOptions {
                    description,
                    schema,
                    auto,
                })
            }
            MigrateCommands::Status => {
                commands::migrate::status(commands::migrate::MigrateStatusOptions)
            }
            MigrateCommands::Build { from, to, output } => {
                commands::migrate::build(commands::migrate::MigrateBuildOptions {
                    from,
                    to,
                    output,
                })
            }
            MigrateCommands::Run { src, dest, bin_dir } => {
                commands::migrate::run(commands::migrate::MigrateRunOptions {
                    src,
                    dest,
                    bin_dir,
                })
            }
            MigrateCommands::Engine {
                src,
                dest,
                schema,
                output,
            } => commands::migrate::engine(commands::migrate::MigrateEngineOptions {
                src,
                dest,
                schema,
                output,
            }),
            MigrateCommands::Up {
                from,
                to,
                src,
                dest,
                output,
                tenant_root,
                dest_suffix,
            } => commands::migrate::up(commands::migrate::MigrateUpOptions {
                from,
                to,
                src,
                dest,
                output,
                tenant_root,
                dest_suffix,
            }),
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
            let resolve = |root: Option<String>| {
                root.map(std::path::PathBuf::from)
                    .unwrap_or_else(|| default_root.clone())
            };
            match tenant_cmd {
                TenantCommands::Create { name, root } => {
                    commands::tenant::create(commands::tenant::CreateOptions {
                        name,
                        root: resolve(root),
                        auth_env: forge_config.auth.env_exports(),
                    })
                }
                TenantCommands::List { root, json } => {
                    commands::tenant::list(commands::tenant::ListOptions {
                        root: resolve(root),
                        json,
                    })
                }
                TenantCommands::Drop { name, root, force } => {
                    commands::tenant::drop(commands::tenant::DropOptions {
                        name,
                        root: resolve(root),
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
