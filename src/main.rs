use clap::{Parser, Subcommand};

mod commands;
mod config;
mod error;
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
    },

    /// Generate code from schema
    Generate {
        /// Generation target (all, rust, typescript, api, openapi, stubs)
        #[arg(default_value = "all")]
        target: String,

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

    /// Start ForgeDB server (Rust API + Bun Runtime + nginx)
    Serve {
        /// Host address
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Rust API Unix socket path
        #[arg(long)]
        rust_socket: Option<String>,

        /// Bun runtime Unix socket path
        #[arg(long)]
        bun_socket: Option<String>,

        /// Skip nginx (expose services directly)
        #[arg(long)]
        no_nginx: bool,

        /// Skip Bun runtime server
        #[arg(long)]
        no_bun: bool,
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
    },

    /// Show migration status
    Status,

    /// Run pending migrations
    Up {
        /// Number of migrations to run (default: all)
        #[arg(short, long)]
        steps: Option<usize>,
    },

    /// Rollback migrations
    Down {
        /// Number of migrations to rollback (default: 1)
        #[arg(short, long)]
        steps: Option<usize>,
    },
}

#[derive(Subcommand)]
enum CompactCommands {
    /// Compact database to reclaim dead space
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

    /// Vacuum database (compact all models)
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

fn run(cli: Cli) -> Result<()> {
    // Set up logging/verbosity based on flags
    let _verbose = cli.verbose;
    let _quiet = cli.quiet;

    // Load generator config (--config path or auto-discover forgedb.toml).
    // Commands that need config-derived defaults pull from this.
    let forge_config = config::load_config(cli.config.as_deref())?;

    match cli.command {
        Commands::Init {
            project_name,
            template,
            rust,
            api_only,
        } => commands::init::run(commands::init::InitOptions {
            project_name,
            template,
            rust,
            api_only,
        }),

        Commands::Generate {
            target,
            check,
            output,
            schema,
            force,
        } => {
            // Precedence: CLI flag > config > built-in default
            let resolved_output = output.or_else(|| forge_config.generate.output.clone());
            let resolved_schema = schema.or_else(|| forge_config.generate.schema.clone());
            commands::generate::run(commands::generate::GenerateOptions {
                target,
                check,
                output: resolved_output,
                schema: resolved_schema,
                config_targets: forge_config.generate.targets.clone(),
                force,
            })
        }

        Commands::Validate {
            strict,
            schema_only,
            implementations,
            components,
            schema,
        } => {
            let resolved_schema = schema.or_else(|| forge_config.generate.schema.clone());
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
            let resolved_output = output.or_else(|| forge_config.generate.output.clone());
            let resolved_schema = schema.or_else(|| forge_config.generate.schema.clone());
            commands::build::run(commands::build::BuildOptions {
                release,
                target,
                output: resolved_output,
                schema: resolved_schema,
                no_api,
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
            MigrateCommands::Create { description, auto } => {
                commands::migrate::create(commands::migrate::MigrateCreateOptions {
                    description,
                    auto,
                })
            }
            MigrateCommands::Status => {
                commands::migrate::status(commands::migrate::MigrateStatusOptions)
            }
            MigrateCommands::Up { steps } => {
                commands::migrate::up(commands::migrate::MigrateUpOptions { steps })
            }
            MigrateCommands::Down { steps } => {
                commands::migrate::down(commands::migrate::MigrateDownOptions { steps })
            }
        },

        Commands::Serve {
            host,
            port,
            rust_socket,
            bun_socket,
            no_nginx,
            no_bun,
        } => commands::serve::run(commands::serve::ServeOptions {
            host,
            port,
            rust_socket: rust_socket.map(std::path::PathBuf::from),
            bun_socket: bun_socket.map(std::path::PathBuf::from),
            no_nginx,
            no_bun,
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
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(e.exit_code());
    }
}
