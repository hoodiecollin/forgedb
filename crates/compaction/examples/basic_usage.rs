//! Basic usage example for forgedb-compaction
//!
//! This example demonstrates collecting database statistics
//! and performing compaction operations.

use forgedb_compaction::*;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    println!("=== ForgeDB Compaction - Basic Usage ===\n");

    // Create a temporary data directory
    let data_dir = PathBuf::from("/tmp/forgedb_compaction_example");
    
    // Clean up and create fresh directory
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    
    // Create some fake model directories to simulate a database
    std::fs::create_dir_all(data_dir.join("User")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(data_dir.join("Post")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(data_dir.join("Comment")).map_err(|e| e.to_string())?;
    
    // Write some fake data files
    std::fs::write(data_dir.join("User/data.bin"), vec![0u8; 10000])
        .map_err(|e| e.to_string())?;
    std::fs::write(data_dir.join("User/tombstones.bin"), vec![0u8; 100])
        .map_err(|e| e.to_string())?;
    std::fs::write(data_dir.join("Post/data.bin"), vec![0u8; 50000])
        .map_err(|e| e.to_string())?;
    std::fs::write(data_dir.join("Post/tombstones.bin"), vec![0u8; 500])
        .map_err(|e| e.to_string())?;
    
    println!("✓ Created test database at {:?}\n", data_dir);

    // Create compaction configuration
    let config = CompactionConfig {
        dead_space_threshold: 0.1, // Compact when 10% of space is dead
        auto_compact: true,
        check_interval_secs: 300,
        max_compaction_time_secs: 600,
    };
    
    println!("✓ Compaction configuration:");
    println!("  Dead space threshold: {}%", config.dead_space_threshold * 100.0);
    println!("  Auto compact: {}", config.auto_compact);
    println!("  Check interval: {} seconds\n", config.check_interval_secs);

    // Create maintenance API
    let api = MaintenanceApi::new(&data_dir, config.clone());
    println!("✓ Maintenance API initialized\n");

    // Collect database statistics
    println!("--- Database Statistics ---");
    match api.stats() {
        Ok(stats) => {
            println!("Total disk bytes: {} bytes", stats.total_disk_bytes);
            println!("Dead bytes: {} bytes", stats.dead_bytes);
            println!("Dead space ratio: {:.2}%", stats.dead_space_ratio * 100.0);
            println!("Number of models: {}", stats.models.len());
            println!("\nModels:");
            for model_stat in &stats.models {
                println!("  - {}", model_stat.name);
                println!("    Disk bytes: {} bytes", model_stat.total_disk_bytes);
                println!("    Deleted rows: {}", model_stat.deleted_rows);
                println!("    Last compaction: {:?}", model_stat.last_compaction);
            }
        }
        Err(e) => println!("Error collecting stats: {}", e),
    }
    println!();

    // Collect statistics for a specific model
    println!("--- Model Statistics: User ---");
    match api.model_stats("User") {
        Ok(model_stats) => {
            println!("Model: {}", model_stats.name);
            println!("Total disk bytes: {} bytes", model_stats.total_disk_bytes);
            println!("Active rows: {}", model_stats.active_rows);
            println!("Deleted rows: {}", model_stats.deleted_rows);
            println!("Dead space ratio: {:.2}%", model_stats.dead_space_ratio * 100.0);
        }
        Err(e) => println!("Error collecting model stats: {}", e),
    }
    println!();

    // Check which models need compaction
    println!("--- Checking Compaction Needs ---");
    match api.stats() {
        Ok(stats) => {
            for model_stat in &stats.models {
                let needs_compaction = model_stat.needs_compaction(&config);
                println!(
                    "{}: dead space {:.2}% - {}",
                    model_stat.name,
                    model_stat.dead_space_ratio * 100.0,
                    if needs_compaction {
                        "NEEDS COMPACTION"
                    } else {
                        "OK"
                    }
                );
            }
        }
        Err(e) => println!("Error: {}", e),
    }
    println!();

    // Perform vacuum operation (compact all models)
    println!("--- Vacuum Operation ---");
    match api.vacuum() {
        Ok(results) => {
            println!("Compacted {} models:", results.len());
            for result in results {
                println!("  - {}", result.model_name);
                println!("    Bytes before: {}", result.bytes_before);
                println!("    Bytes after: {}", result.bytes_after);
                println!("    Space reclaimed: {} bytes ({:.1}%)", 
                    result.bytes_reclaimed, result.reclaim_percentage());
                println!("    Duration: {} ms", result.duration_ms);
            }
        }
        Err(e) => println!("Error during vacuum: {}", e),
    }
    println!();

    // Analyze operation (just collect statistics)
    println!("--- Analyze Operation ---");
    match api.analyze() {
        Ok(stats) => {
            println!("Analysis complete:");
            println!("  Total disk bytes: {} bytes", stats.total_disk_bytes);
            println!("  Dead bytes: {} bytes", stats.dead_bytes);
            println!("  Models analyzed: {}", stats.models.len());
        }
        Err(e) => println!("Error during analyze: {}", e),
    }

    println!("\n✓ Example completed successfully!");
    
    Ok(())
}
