use crate::error::CliError;
use forgedb_compaction::{CompactionConfig, MaintenanceApi};
use std::path::PathBuf;

/// Options for the DEPRECATED offline `compact` command (#105).  The fields are
/// still parsed from the CLI (so the flags resolve rather than erroring) but are
/// no longer acted on — the command mutates nothing and returns a deprecation
/// error.  `#[allow(dead_code)]` because the offline execution was removed.
#[allow(dead_code)]
pub struct CompactOptions {
    pub data_dir: PathBuf,
    pub model: Option<String>,
    pub all: bool,
    pub threshold: Option<f64>,
}

/// Deprecation message for the offline compaction path (#105).  The
/// tombstone-based `compact_model` is misaligned with the generated
/// superseding-version mutation surface (#66): superseded update-versions are
/// orphaned but NOT tombstoned (so nothing is reclaimed), and a delete's
/// tombstone is a NEW marker row — dropping it RESURRECTS the deleted record.
/// Computing the correct live-row set offline needs schema-aware id-column logic
/// the CLI does not carry, so this path is REMOVED: the supported path is the
/// in-process, keep-set-based `Database::compact()` (#92).
const DEPRECATED_COMPACT_MSG: &str = "\
Offline `forgedb compact` / `vacuum` is REMOVED (issue #105).  Its tombstone-based \
algorithm is unsafe on data written by the generated mutation surface \
(updates/deletes): it reclaims nothing from updates and can RESURRECT deleted rows.\n\n\
Use the supported path instead: generated databases compact automatically \
in-process under the single-writer lock (every COMPACTION_DEAD_THRESHOLD dead \
versions), or call `Database::compact()` from your running app.  That path is \
keep-set-based (it keeps exactly the live rows), so it never resurrects deletes.";

pub struct StatsOptions {
    pub data_dir: PathBuf,
    pub model: Option<String>,
    pub json: bool,
}

/// Options for the DEPRECATED offline `vacuum` command (#105).  See
/// [`CompactOptions`] — parsed but not acted on.
#[allow(dead_code)]
pub struct VacuumOptions {
    pub data_dir: PathBuf,
}

pub struct AnalyzeOptions {
    pub data_dir: PathBuf,
    pub json: bool,
}

/// Offline compaction is DEPRECATED and no longer executed (#105).
///
/// The tombstone-based offline algorithm is unsafe against the generated
/// mutation surface (#66) — it reclaims nothing from updates and can resurrect
/// deleted rows — so this command mutates nothing and returns an error (non-zero
/// exit) pointing to the supported in-process `Database::compact()` path (#92).
pub fn compact(_opts: CompactOptions) -> Result<(), CliError> {
    Err(CliError::Compaction(DEPRECATED_COMPACT_MSG.to_string()))
}

/// Show database statistics
pub fn stats(opts: StatsOptions) -> Result<(), CliError> {
    let config = CompactionConfig::default();
    let api = MaintenanceApi::new(&opts.data_dir, config);

    if let Some(model) = opts.model {
        // Show stats for specific model
        let stats = api.model_stats(&model).map_err(map_err)?;

        if opts.json {
            let json = serde_json::to_string_pretty(&stats).map_err(|e| map_err(e.to_string()))?;
            println!("{}", json);
        } else {
            print_model_stats(&stats);
        }
    } else {
        // Show database-wide stats
        let stats = api.stats().map_err(map_err)?;

        if opts.json {
            let json = serde_json::to_string_pretty(&stats).map_err(|e| map_err(e.to_string()))?;
            println!("{}", json);
        } else {
            print_database_stats(&stats);
        }
    }

    Ok(())
}

/// Vacuum is DEPRECATED and no longer executed (#105) — an alias for the removed
/// offline compaction path.  See [`compact`]; use in-process `Database::compact()`.
pub fn vacuum(_opts: VacuumOptions) -> Result<(), CliError> {
    Err(CliError::Compaction(DEPRECATED_COMPACT_MSG.to_string()))
}

/// Analyze database (collect statistics)
pub fn analyze(opts: AnalyzeOptions) -> Result<(), CliError> {
    let config = CompactionConfig::default();
    let api = MaintenanceApi::new(&opts.data_dir, config.clone());

    let stats = api.analyze().map_err(map_err)?;

    if opts.json {
        let json = serde_json::to_string_pretty(&stats).map_err(|e| map_err(e.to_string()))?;
        println!("{}", json);
    } else {
        print_database_stats(&stats);

        // Show recommendations
        println!("\nRecommendations:");
        let models_needing_compact = stats.models_needing_compaction(&config);
        if models_needing_compact.is_empty() {
            println!("  ✓ No models need compaction");
        } else {
            println!(
                "  ⚠  {} model(s) exceed dead space threshold:",
                models_needing_compact.len()
            );
            for model in models_needing_compact {
                println!(
                    "    - {} ({:.1}% dead space)",
                    model.name,
                    model.dead_space_ratio * 100.0
                );
            }
            println!(
                "\n  Compact in-process: generated databases reclaim this space \
                 automatically under the single-writer lock, or call \
                 `Database::compact()` from your app (offline `forgedb compact` is \
                 deprecated — see #105)."
            );
        }
    }

    Ok(())
}

fn print_database_stats(stats: &forgedb_compaction::DatabaseStats) {
    println!("\nDatabase Statistics");
    println!("===================");
    println!(
        "Collected at: {}",
        stats.collected_at.format("%Y-%m-%d %H:%M:%S")
    );
    println!();
    println!("Storage:");
    println!(
        "  Total disk usage:  {}",
        format_bytes(stats.total_disk_bytes)
    );
    println!("  Used space:        {}", format_bytes(stats.used_bytes));
    println!("  Dead space:        {}", format_bytes(stats.dead_bytes));
    println!(
        "  Dead space ratio:  {:.1}%",
        stats.dead_space_ratio * 100.0
    );
    println!();
    println!("Models: {}", stats.models.len());
    println!();

    if !stats.models.is_empty() {
        println!(
            "{:<20} {:>12} {:>12} {:>12} {:>10}",
            "Model", "Total", "Used", "Dead", "Dead %"
        );
        println!("{}", "=".repeat(70));

        for model in &stats.models {
            println!(
                "{:<20} {:>12} {:>12} {:>12} {:>9.1}%",
                model.name,
                format_bytes(model.total_disk_bytes),
                format_bytes(model.used_bytes),
                format_bytes(model.dead_bytes),
                model.dead_space_ratio * 100.0
            );
        }
    }
}

fn print_model_stats(stats: &forgedb_compaction::ModelStats) {
    println!("\nModel: {}", stats.name);
    println!("===================");
    println!();
    println!("Rows:");
    println!("  Total:   {}", stats.total_rows);
    println!("  Active:  {}", stats.active_rows);
    println!("  Deleted: {}", stats.deleted_rows);
    println!();
    println!("Storage:");
    println!(
        "  Total disk usage:  {}",
        format_bytes(stats.total_disk_bytes)
    );
    println!("  Used space:        {}", format_bytes(stats.used_bytes));
    println!("  Dead space:        {}", format_bytes(stats.dead_bytes));
    println!(
        "  Dead space ratio:  {:.1}%",
        stats.dead_space_ratio * 100.0
    );

    if let Some(last_compact) = stats.last_compaction {
        println!();
        println!(
            "Last compaction: {}",
            last_compact.format("%Y-%m-%d %H:%M:%S")
        );
    }

    if !stats.columns.is_empty() {
        println!();
        println!("Columns:");
        println!(
            "{:<20} {:>10} {:>12} {:>12} {:>10}",
            "Name", "Type", "Total", "Dead", "Dead %"
        );
        println!("{}", "=".repeat(68));

        for col in &stats.columns {
            let col_type = match col.column_type {
                forgedb_compaction::ColumnType::Fixed => "Fixed",
                forgedb_compaction::ColumnType::Variable => "Variable",
            };
            println!(
                "{:<20} {:>10} {:>12} {:>12} {:>9.1}%",
                col.name,
                col_type,
                format_bytes(col.total_bytes),
                format_bytes(col.dead_bytes),
                col.dead_space_ratio * 100.0
            );
        }
    }

    if !stats.index_sizes.is_empty() {
        println!();
        println!("Indexes:");
        for (name, size) in &stats.index_sizes {
            println!("  {}: {}", name, format_bytes(*size));
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn map_err(e: String) -> CliError {
    CliError::Compaction(e)
}


