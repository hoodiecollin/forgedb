use crate::error::CliError;
use sinkdb_compaction::{CompactionConfig, MaintenanceApi};
use std::path::PathBuf;

pub struct CompactOptions {
    pub data_dir: PathBuf,
    pub model: Option<String>,
    pub all: bool,
    pub threshold: Option<f64>,
}

pub struct StatsOptions {
    pub data_dir: PathBuf,
    pub model: Option<String>,
    pub json: bool,
}

pub struct VacuumOptions {
    pub data_dir: PathBuf,
}

pub struct AnalyzeOptions {
    pub data_dir: PathBuf,
    pub json: bool,
}

/// Compact database or specific model
pub fn compact(opts: CompactOptions) -> Result<(), CliError> {
    let mut config = CompactionConfig::default();

    if let Some(threshold) = opts.threshold {
        if threshold < 0.0 || threshold > 1.0 {
            return Err(CliError::Compaction(
                "Threshold must be between 0.0 and 1.0".to_string(),
            ));
        }
        config.dead_space_threshold = threshold;
    }

    let api = MaintenanceApi::new(&opts.data_dir, config);

    let results = if let Some(model) = opts.model {
        // Compact specific model
        let result = api.compact_model(&model).map_err(map_err)?;
        vec![result]
    } else if opts.all {
        // Compact all models
        api.compact_all().map_err(map_err)?
    } else {
        // Compact only models that need it
        api.compact_needed().map_err(map_err)?
    };

    if results.is_empty() {
        println!("✓ No models need compaction");
        return Ok(());
    }

    println!("\n✓ Compaction completed\n");
    println!(
        "{:<20} {:>15} {:>15} {:>10} {:>10}",
        "Model", "Before", "Reclaimed", "Percent", "Time"
    );
    println!("{}", "=".repeat(75));

    for result in results {
        if result.success {
            println!(
                "{:<20} {:>15} {:>15} {:>9.1}% {:>9}ms",
                result.model_name,
                format_bytes(result.bytes_before),
                format_bytes(result.bytes_reclaimed),
                result.reclaim_percentage(),
                result.duration_ms
            );
        } else {
            eprintln!(
                "{:<20} Error: {}",
                result.model_name,
                result
                    .error
                    .as_ref()
                    .unwrap_or(&"Unknown error".to_string())
            );
        }
    }

    Ok(())
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

/// Vacuum database (compact all models)
pub fn vacuum(opts: VacuumOptions) -> Result<(), CliError> {
    let config = CompactionConfig::default();
    let api = MaintenanceApi::new(&opts.data_dir, config);

    println!("Running vacuum on database...");

    let results = api.vacuum().map_err(map_err)?;

    if results.is_empty() {
        println!("✓ No models to vacuum");
        return Ok(());
    }

    let total_reclaimed: u64 = results.iter().map(|r| r.bytes_reclaimed).sum();
    let success_count = results.iter().filter(|r| r.success).count();

    println!("\n✓ Vacuum completed\n");
    println!("{} model(s) processed", results.len());
    println!("{} successful", success_count);
    println!("Total space reclaimed: {}", format_bytes(total_reclaimed));

    Ok(())
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
            println!("\n  Run 'sinkdb compact' to reclaim space");
        }
    }

    Ok(())
}

fn print_database_stats(stats: &sinkdb_compaction::DatabaseStats) {
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

fn print_model_stats(stats: &sinkdb_compaction::ModelStats) {
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
                sinkdb_compaction::ColumnType::Fixed => "Fixed",
                sinkdb_compaction::ColumnType::Variable => "Variable",
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

fn format_bytes(bytes: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(1536), "1.50 KB");
    }
}
