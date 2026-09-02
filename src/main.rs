use clap::{Parser, Subcommand};

use forgedb::{ask, cache, commands, error, naming, project, targets, ui};

use error::Result;

#[derive(Parser)]
#[command(name = "forgedb")]
#[command(author, version, about = "ForgeDB - Type-safe database from schemas", long_about = None)]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(short, long, global = true)]
    quiet: bool,

    #[arg(short, long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(value_name = "DIR")]
        project_name: String,

        #[arg(short, long)]
        template: Option<String>,

        #[arg(long, hide = true)]
        rust: bool,

        #[arg(long, hide = true)]
        api_only: bool,

        #[arg(long, group = "grouping")]
        isolated: bool,

        #[arg(long, group = "grouping")]
        no_isolated: bool,
    },

    Generate {
        #[arg(default_value = "all")]
        target: String,

        #[arg(long, group = "gen_mode")]
        sdk: bool,

        #[arg(long, group = "gen_mode")]
        runtime: bool,

        #[arg(long, group = "gen_mode")]
        replica: bool,

        #[arg(long)]
        check: bool,

        #[arg(short, long)]
        output: Option<String>,

        #[arg(short, long)]
        schema: Option<String>,

        #[arg(short, long)]
        force: bool,

        #[arg(long)]
        from: Option<u32>,

        #[arg(long)]
        to: Option<u32>,
    },

    Validate {
        #[arg(long)]
        strict: bool,

        #[arg(long)]
        schema_only: bool,

        #[arg(long)]
        implementations: bool,

        #[arg(long)]
        components: bool,

        #[arg(short, long)]
        schema: Option<String>,
    },

    Build {
        #[arg(long, default_value = "true")]
        release: bool,

        #[arg(short, long, hide = true)]
        target: Option<String>,

        #[arg(short, long)]
        output: Option<String>,

        #[arg(long)]
        schema: Option<String>,

        #[arg(long)]
        no_api: bool,

        #[arg(long)]
        plan: bool,

        #[arg(long, value_name = "PATH")]
        report: Option<String>,

        #[arg(long, value_name = "KIND")]
        print_artifact: Option<String>,
    },

    Dev {
        #[arg(short, long)]
        schema: Option<String>,

        #[arg(short, long)]
        output: Option<String>,

        #[arg(short, long, default_value = "200")]
        debounce: u64,

        #[arg(long, default_value = "true")]
        clear: bool,
    },

    #[command(subcommand)]
    Migrate(MigrateCommands),

    #[command(subcommand)]
    Compact(CompactCommands),

    #[command(subcommand)]
    Backup(BackupCommands),

    #[command(subcommand)]
    Tenant(TenantCommands),

    Project {
        #[command(subcommand)]
        command: ProjectCommands,

        #[arg(short, long, global = true)]
        schema: Option<String>,
    },

    Lsp {
        #[arg(long)]
        server_path: Option<std::path::PathBuf>,

        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    Coordinate {
        root: std::path::PathBuf,

        #[arg(short, long)]
        socket: Option<std::path::PathBuf>,

        #[arg(long)]
        fsync: Option<String>,

        #[arg(long)]
        fsync_interval: Option<u64>,

        #[arg(long)]
        turn_timeout: Option<u64>,

        #[arg(long)]
        max_frame_mib: Option<u64>,
    },
}

#[derive(Subcommand)]
enum ProjectCommands {
    Show,
}

#[derive(Subcommand)]
enum MigrateCommands {
    Create {
        description: String,

        #[arg(short, long, hide = true)]
        auto: bool,

        #[arg(long)]
        no_auto: bool,

        #[arg(short, long)]
        schema: std::path::PathBuf,
    },

    Status {
        #[arg(short, long)]
        schema: std::path::PathBuf,
    },

    Build {
        #[arg(short, long)]
        schema: std::path::PathBuf,

        #[arg(long)]
        from: u32,

        #[arg(long)]
        to: u32,

        #[arg(short, long, hide = true)]
        output: Option<std::path::PathBuf>,
    },

    Run {
        #[arg(short, long)]
        schema: std::path::PathBuf,

        #[arg(long)]
        from: u32,

        #[arg(long)]
        to: u32,

        #[arg(long)]
        src: std::path::PathBuf,

        #[arg(long)]
        dest: std::path::PathBuf,

        #[arg(long, hide = true)]
        bin_dir: Option<std::path::PathBuf>,
    },

    Engine {
        #[arg(long)]
        src: std::path::PathBuf,

        #[arg(long)]
        dest: std::path::PathBuf,

        #[arg(short, long)]
        schema: std::path::PathBuf,

        #[arg(short, long, hide = true)]
        output: Option<std::path::PathBuf>,
    },

    #[command(hide = true)]
    Up {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum TenantCommands {
    Create {
        name: String,
        #[arg(short, long)]
        root: Option<String>,
    },

    List {
        #[arg(short, long)]
        root: Option<String>,
        #[arg(long)]
        json: bool,
    },

    Drop {
        name: String,
        #[arg(short, long)]
        root: Option<String>,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum CompactCommands {
    Run {
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        #[arg(short, long)]
        model: Option<String>,

        #[arg(short, long)]
        all: bool,

        #[arg(short, long)]
        threshold: Option<f64>,
    },

    Stats {
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        #[arg(short, long)]
        model: Option<String>,

        #[arg(long)]
        json: bool,
    },

    Vacuum {
        #[arg(short, long, default_value = "data")]
        data_dir: String,
    },

    Analyze {
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum BackupCommands {
    Create {
        #[arg(short, long, default_value = "data")]
        data_dir: String,

        #[arg(short, long, default_value = "backup.forgebk")]
        output: String,

        #[arg(long)]
        incremental: bool,

        #[arg(long)]
        base: Option<String>,
    },

    Restore {
        #[arg(required = true, num_args = 1..)]
        archive: Vec<String>,

        #[arg(short, long, default_value = "data")]
        output: String,

        #[arg(long)]
        overwrite: bool,
    },

    List {
        archive: String,

        #[arg(long)]
        json: bool,
    },
}

fn reserve_in_cache(
    project: &project::ProjectId,
    schema: &std::path::Path,
    symbol_naming: naming::SymbolNaming,
) -> Result<cache::Reserved> {
    let reserved = cache::reserve(&project.name, &project.root, schema, symbol_naming)?;
    ui::info(&format!("Build cache: {}", reserved.container.display()));
    Ok(reserved)
}

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
    for dir in &pruned.removed {
        ui::info(&format!(
            "Pruned (no longer in [generate].targets): {}",
            dir.display()
        ));
    }
    Ok(pruned.synced)
}

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
    ui::set_verbosity(cli.verbose, cli.quiet);

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
            let schema_path = project::find_schema(schema.as_deref())?;
            let governing = project::govern_for_schema(explicit_config, &schema_path)?;
            let project = governing.identify_reported()?;
            let reserved = reserve_in_cache(&project, &schema_path, governing.symbol_naming())?;
            let forge_config = governing.config();
            let resolved_output = Some(governing.output(output.as_deref()));
            let resolved_schema = Some(schema_path.display().to_string());
            let mode = if sdk {
                Some(commands::generate::GenerateMode::Sdk)
            } else if runtime {
                Some(commands::generate::GenerateMode::Runtime)
            } else if replica {
                Some(commands::generate::GenerateMode::Replica)
            } else {
                None
            };
            let gen_config = forge_config.gen_config()?;
            let (config_targets, target_warnings) = forge_config.resolved_targets()?;
            for warning in &target_warnings {
                ui::warning(warning);
            }
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
            if commands::build::stdout_is_machine_readable(
                print_artifact.as_deref(),
                report.as_deref(),
            ) {
                ui::set_verbosity(false, true);
                ask::forbid();
            }
            let schema_path = project::find_schema(schema.as_deref())?;
            let governing = project::govern_for_schema(explicit_config, &schema_path)?;
            let project = governing.identify_reported()?;
            let reserved = reserve_in_cache(&project, &schema_path, governing.symbol_naming())?;
            let forge_config = governing.config();
            let resolved_output = Some(governing.output(output.as_deref()));
            let resolved_schema = Some(schema_path.display().to_string());
            let (config_targets, target_warnings) = forge_config.resolved_targets()?;
            for warning in &target_warnings {
                ui::warning(warning);
            }
            let declared = targets::declared_packages(&config_targets);
            let container = reserved.container.clone();
            let project_root = reserved.project.clone();
            let gen_config = forge_config.gen_config()?;
            let sync: commands::dev::SyncHook =
                Box::new(move || sync_after_emission(&reserved, Some(&declared)).map(|_| ()));
            commands::build::run(commands::build::BuildOptions {
                release,
                target,
                output: resolved_output,
                schema: resolved_schema,
                no_api,
                config_targets,
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
            let schema_path = project::find_schema(schema.as_deref())?;
            let governing = project::govern_for_schema(explicit_config, &schema_path)?;
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

        Commands::Migrate(migrate_cmd) => match migrate_cmd {
            MigrateCommands::Create {
                description,
                auto,
                no_auto,
                schema,
            } => commands::migrate::create(commands::migrate::MigrateCreateOptions {
                app: migrate_app(schema, explicit_config),
                description,
                removed_auto: auto,
                no_auto,
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
            MigrateCommands::Up { args: _ } => commands::migrate::up(),
        },

        Commands::Project { command, schema } => {
            commands::project::run(commands::project::ProjectOptions {
                command: match command {
                    ProjectCommands::Show => commands::project::ProjectCommand::Show,
                },
                schema,
            })
        }

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
            let governing = project::govern_cwd(explicit_config)?;
            let forge_config = governing.config();
            let default_root = forge_config.tenant.root();
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
