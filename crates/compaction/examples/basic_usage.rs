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
        tombstone_threshold: 0.1, // Compact when 10% of records are deleted
        min_file_size: 1000,       // Only compact files larger than 1KB
        enabled: true,
    };
    
    println!("✓ Compaction configuration:");
    println!("  Tombstone threshold: {}%", config.tombstone_threshold * 100.0);
    println!("  Min file size: {} bytes", config.min_file_size);
    println!("  Enabled: {}\n", config.enabled);

    // Create maintenance API
    let api = MaintenanceApi::new(&data_dir, config);
    println!("✓ Maintenance API initialized\n");

    // Collect database statistics
    println!("--- Database Statistics ---");
    match api.stats() {
        Ok(stats) => {
            println!("Total size: {} bytes", stats.total_size);
            println!("Total tombstones: {}", stats.total_tombstones);
            println!("Number of models: {}", stats.models.len());
            println!("\nModels:");
            for model_stat in &stats.models {
                println!("  - {}", model_stat.model_name);
                println!("    Size: {} bytes", model_stat.size);
                println!("    Tombstones: {}", model_stat.tombstone_count);
                println!("    Last modified: {:?}", model_stat.last_modified);
            }
        }
        Err(e) => println!("Error collecting stats: {}", e),
    }
    println!();

    // Collect statistics for a specific model
    println!("--- Model Statistics: User ---");
    match api.model_stats("User") {
        Ok(model_stats) => {
            println!("Model: {}", model_stats.model_name);
            println!("Size: {} bytes", model_stats.size);
            println!("Tombstones: {}", model_stats.tombstone_count);
            if let Some(ratio) = model_stats.fragmentation_ratio {
                println!("Fragmentation: {:.2}%", ratio * 100.0);
            }
        }
        Err(e) => println!("Error collecting model stats: {}", e),
    }
    println!();

    // Check which models need compaction
    println!("--- Checking Compaction Needs ---");
    match api.stats() {
        Ok(stats) => {
            for model_stat in &stats.models {
                if let Some(ratio) = model_stat.fragmentation_ratio {
                    let needs_compaction = ratio > config.tombstone_threshold;
                    println!(
                        "{}: fragmentation {:.2}% - {}",
                        model_stat.model_name,
                        ratio * 100.0,
                        if needs_compaction {
                            "NEEDS COMPACTION"
                        } else {
                            "OK"
                        }
                    );
                }
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
                println!("    Records compacted: {}", result.records_compacted);
                println!("    Tombstones removed: {}", result.tombstones_removed);
                println!("    Space reclaimed: {} bytes", result.space_reclaimed);
                println!("    Duration: {:?}", result.duration);
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
            println!("  Total size: {} bytes", stats.total_size);
            println!("  Total tombstones: {}", stats.total_tombstones);
            println!("  Models analyzed: {}", stats.models.len());
        }
        Err(e) => println!("Error during analyze: {}", e),
    }

    println!("\n✓ Example completed successfully!");
    
    Ok(())
}
