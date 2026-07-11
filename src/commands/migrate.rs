use crate::{error::CliError, ui, Result};
use colored::Colorize;
use forgedb_migrations::{MigrationExecutor, MigrationGenerator, MigrationTracker, SchemaChange};
use std::path::PathBuf;

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

pub struct MigrateUpOptions {
    pub steps: Option<usize>,
}

pub struct MigrateDownOptions {
    pub steps: Option<usize>,
}

/// Create a new migration
pub fn create(opts: MigrateCreateOptions) -> Result<()> {
    let migrations_dir = PathBuf::from("migrations");

    if opts.auto {
        // Auto-detect changes by diffing the current schema against the recorded
        // snapshot.  v1 Phase 4 (#92) scope: **additive changes only** are
        // auto-applied (new models, new nullable fields — existing rows are
        // backfilled on reopen by generated code).  Any breaking change is refused
        // here with the dump→regenerate→reload guidance (no data-transform engine
        // ships in v1).
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
            save_schema_snapshot(&migrations_dir, &new_src)?;
            ui::success("No schema changes detected (snapshot up to date)");
            return Ok(());
        }

        // Split into breaking vs additive.  A breaking change means we STOP — v1
        // has no data-transform engine, so the answer is the reload path.
        let breaking: Vec<_> = changes.iter().filter(|c| c.is_breaking()).collect();
        if !breaking.is_empty() {
            ui::warning("\n⚠️  Breaking schema changes detected — auto-migration refused");
            println!("\nBreaking changes:");
            for change in &breaking {
                println!("  • {}", change.description().red());
            }
            print_reload_guidance();
            return Err(CliError::Migration(
                "breaking schema changes require the dump → regenerate → reload path \
                 (see guidance above); v1 ships no data-transform migration engine"
                    .to_string(),
            ));
        }

        // Purely additive — safe to record.
        ui::info(&format!("Detected {} additive change(s)", changes.len()));
        for change in &changes {
            println!("  • {}", change.description());
        }

        let migration =
            MigrationGenerator::generate(migrations_dir, opts.description, changes.clone())
                .map_err(map_err)?;
        save_schema_snapshot(&PathBuf::from("migrations"), &new_src)?;
        ui::success(&format!("Created migration: {}", migration.filename()));

        println!("\n{}", MigrationGenerator::generate_report(&migration));
        ui::info(
            "Additive change: run `forgedb generate` and restart your app — existing \
             rows are backfilled with defaults on reopen (no data step needed).",
        );
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

/// Run pending migrations
pub fn up(opts: MigrateUpOptions) -> Result<()> {
    let migrations_dir = PathBuf::from("migrations");
    let data_dir = PathBuf::from("data");

    // Load all migrations
    let all_migrations =
        MigrationGenerator::load_all_migrations(&migrations_dir).map_err(map_err)?;

    if all_migrations.is_empty() {
        ui::info("No migrations found");
        return Ok(());
    }

    // Load tracker
    let mut tracker = MigrationTracker::new(&migrations_dir).map_err(map_err)?;

    // Get pending migrations
    let migration_ids: Vec<_> = all_migrations.iter().map(|m| m.id.clone()).collect();
    let pending = tracker.pending_migrations(&migration_ids);

    if pending.is_empty() {
        ui::success("All migrations are up to date");
        return Ok(());
    }

    // Determine how many to run
    let to_run = if let Some(steps) = opts.steps {
        pending.iter().take(steps).cloned().collect::<Vec<_>>()
    } else {
        pending
    };

    ui::info(&format!("Running {} migration(s)...", to_run.len()));

    for migration_id in to_run {
        let migration = all_migrations
            .iter()
            .find(|m| m.id == migration_id)
            .ok_or_else(|| anyhow::anyhow!("Migration {} not found", migration_id))?;

        println!("\n{} {}", "→".cyan(), migration.description.bold());

        // Check for breaking changes
        if migration.has_breaking_changes() {
            ui::warning("This migration contains breaking changes!");

            // In interactive mode, we'd ask for confirmation here
            println!("Breaking changes:");
            for change in migration.breaking_changes() {
                println!("  • {}", change.description().yellow());
            }
        }

        // Execute migration
        MigrationExecutor::execute_up(migration, &data_dir).map_err(map_err)?;

        // Mark as applied
        tracker
            .mark_applied(migration.id.clone(), migration.checksum.clone())
            .map_err(map_err)?;

        ui::success(&format!("Applied migration {}", migration.id));
    }

    ui::success("\n✓ All migrations completed successfully");

    Ok(())
}

/// Rollback migrations
pub fn down(opts: MigrateDownOptions) -> Result<()> {
    let migrations_dir = PathBuf::from("migrations");
    let data_dir = PathBuf::from("data");

    // Load tracker
    let mut tracker = MigrationTracker::new(&migrations_dir).map_err(map_err)?;

    let applied = tracker.applied_migrations();

    if applied.is_empty() {
        ui::info("No migrations to roll back");
        return Ok(());
    }

    // Determine how many to roll back
    let steps = opts.steps.unwrap_or(1);

    ui::warning(&format!("Rolling back {} migration(s)...", steps));

    // Load all migrations
    let all_migrations =
        MigrationGenerator::load_all_migrations(&migrations_dir).map_err(map_err)?;

    for _ in 0..steps {
        let last_record = tracker.last_migration();

        if last_record.is_none() {
            ui::info("No more migrations to roll back");
            break;
        }

        let record = last_record.unwrap();
        let migration = all_migrations
            .iter()
            .find(|m| m.id == record.migration_id)
            .ok_or_else(|| anyhow::anyhow!("Migration {} not found", record.migration_id))?;

        println!("\n{} {}", "←".yellow(), migration.description.bold());

        // Execute rollback
        MigrationExecutor::execute_down(migration, &data_dir).map_err(map_err)?;

        // Mark as rolled back
        tracker.mark_rolled_back().map_err(map_err)?;

        ui::success(&format!("Rolled back migration {}", migration.id));
    }

    ui::success("\n✓ Rollback completed successfully");

    Ok(())
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

/// Print the v1 answer for breaking schema changes: dump → regenerate → reload.
fn print_reload_guidance() {
    println!("\n{}", "The v1 path for breaking changes (no data-transform engine ships in v1):".bold());
    println!("  1. Export data with the CURRENT binary (before changing the schema).");
    println!("  2. Edit the schema and run `forgedb generate` to regenerate the code.");
    println!("  3. Re-import the exported data into the freshly-generated database.");
    println!("  See docs/MIGRATIONS.md for the worked reload procedure.");
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


