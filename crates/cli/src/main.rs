use clap::{Parser, Subcommand};
use sinkdb_cli::{commands, Result};

#[derive(Parser)]
#[command(name = "sinkdb")]
#[command(author, version, about = "SinkDB - Type-safe database from schemas", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Path to sinkdb.toml config file
    #[arg(short, long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new SinkDB project
    Init {
        /// Project name
        project_name: String,

        /// Use a template (blog, ecommerce, todo, blank)
        #[arg(short, long)]
        template: Option<String>,

        /// Include Rust backend
        #[arg(long)]
        rust: bool,

        /// Include TypeScript frontend
        #[arg(long, default_value = "true")]
        typescript: bool,

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

        /// Check computed field implementations
        #[arg(long)]
        implementations: bool,

        /// Check UI component files
        #[arg(long)]
        components: bool,
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

        /// Skip API server build
        #[arg(long)]
        no_api: bool,

        /// Skip database build
        #[arg(long)]
        no_db: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging/verbosity based on flags
    let _verbose = cli.verbose;
    let _quiet = cli.quiet;

    match cli.command {
        Commands::Init {
            project_name,
            template,
            rust,
            typescript,
            api_only,
        } => {
            commands::init::run(commands::init::InitOptions {
                project_name,
                template,
                rust,
                typescript,
                api_only,
            })
        }

        Commands::Generate {
            target,
            check,
            output,
            force,
        } => {
            commands::generate::run(commands::generate::GenerateOptions {
                target,
                check,
                output,
                force,
            })
        }

        Commands::Validate {
            strict,
            schema_only,
            implementations,
            components,
        } => {
            commands::validate::run(commands::validate::ValidateOptions {
                strict,
                schema_only,
                implementations,
                components,
            })
        }

        Commands::Build {
            release,
            target,
            output,
            no_api,
            no_db,
        } => {
            commands::build::run(commands::build::BuildOptions {
                release,
                target,
                output,
                no_api,
                no_db,
            })
        }
    }
}
