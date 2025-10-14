use crate::{ui, CliError, Result};
use colored::Colorize;
use sinkdb_migrations::{MigrationExecutor, MigrationGenerator, MigrationTracker, SchemaChange};
use std::path::PathBuf;

// Helper to convert String errors to CliError
fn map_err(e: String) -> CliError {
    CliError::Migration(e)
}

pub struct MigrateCreateOptions {
    pub description: String,
    pub auto: bool,
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
        // Auto-detect changes by comparing current schema to last known state
        ui::info("Auto-detecting schema changes...");

        let changes = detect_schema_changes()?;

        if changes.is_empty() {
            ui::success("No schema changes detected");
            return Ok(());
        }

        ui::info(&format!("Detected {} change(s)", changes.len()));
        for change in &changes {
            println!("  • {}", change.description());
        }

        let migration = MigrationGenerator::generate(migrations_dir, opts.description, changes)
            .map_err(map_err)?;
        ui::success(&format!("Created migration: {}", migration.filename()));

        if migration.has_breaking_changes() {
            ui::warning("\n⚠️  This migration contains BREAKING CHANGES");
            println!("\nBreaking changes:");
            for change in migration.breaking_changes() {
                println!("  • {}", change.description().red());
            }
        }

        println!("\n{}", MigrationGenerator::generate_report(&migration));
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

/// Detect schema changes by comparing schemas
fn detect_schema_changes() -> Result<Vec<SchemaChange>> {
    // For this to work properly, we'd need to:
    // 1. Parse the current schema.sink file
    // 2. Load the last schema snapshot (stored when last migration was created)
    // 3. Diff them using SchemaDiffer

    // For now, return a placeholder
    // In a real implementation, this would:
    // - Read schema.sink
    // - Parse it into SimpleSchema
    // - Load .last_schema.json
    // - Call SchemaDiffer::diff()

    ui::warning("Auto-detection requires schema parsing (not yet implemented in CLI)");
    ui::info("For now, you can create a manual migration and edit it");

    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_commands_compile() {
        // Just ensure the code compiles
        assert!(true);
    }
}
