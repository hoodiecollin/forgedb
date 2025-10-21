//! Intermediate example for forgedb-compaction
//!
//! This example demonstrates setting up background compaction
//! that runs automatically on a schedule.

use forgedb_compaction::*;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<(), String> {
    println!("=== ForgeDB Compaction - Background Compaction ===\n");

    // Create a temporary data directory
    let data_dir = PathBuf::from("/tmp/forgedb_background_compaction_example");
    
    // Clean up and create fresh directory
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    
    // Create test model directories
    for model in &["User", "Post", "Comment", "Like"] {
        std::fs::create_dir_all(data_dir.join(model)).map_err(|e| e.to_string())?;
        std::fs::write(data_dir.join(format!("{}/data.bin", model)), vec![0u8; 20000])
            .map_err(|e| e.to_string())?;
        std::fs::write(data_dir.join(format!("{}/tombstones.bin", model)), vec![0u8; 300])
            .map_err(|e| e.to_string())?;
    }
    
    println!("✓ Created test database with 4 models\n");

    // Configure compaction
    let config = CompactionConfig {
        tombstone_threshold: 0.05, // Compact when 5% of records are deleted
        min_file_size: 5000,        // Only compact files larger than 5KB
        enabled: true,
    };
    
    println!("✓ Compaction configuration:");
    println!("  Tombstone threshold: {}%", config.tombstone_threshold * 100.0);
    println!("  Min file size: {} bytes", config.min_file_size);
    println!("  Enabled: {}\n", config.enabled);

    // Create background compactor
    let schedule = CompactionSchedule {
        interval: Duration::from_secs(10), // Run every 10 seconds
        max_duration: Duration::from_secs(60), // Max 60 seconds per run
    };

    println!("✓ Compaction schedule:");
    println!("  Interval: {:?}", schedule.interval);
    println!("  Max duration: {:?}\n", schedule.max_duration);

    let compactor = BackgroundCompactor::new(&data_dir, config, schedule);
    println!("✓ Background compactor created\n");

    // Start the background compactor
    println!("--- Starting Background Compactor ---");
    println!("The compactor will run every {:?}", schedule.interval);
    println!("Simulating 30 seconds of operation...\n");

    compactor.start();
    println!("✓ Background compactor started");

    // Simulate some work while compactor runs in background
    for i in 1..=3 {
        println!("\n[Tick {}] Main application running...", i);
        
        // Check compaction status
        if let Some(last_run) = compactor.last_run() {
            println!("  Last compaction: {:?}", last_run.elapsed());
        } else {
            println!("  No compactions run yet");
        }

        // Get compaction stats
        match compactor.stats() {
            Ok(stats) => {
                println!("  Database stats:");
                println!("    Total size: {} bytes", stats.total_size);
                println!("    Total tombstones: {}", stats.total_tombstones);
                println!("    Models: {}", stats.models.len());
            }
            Err(e) => println!("  Error getting stats: {}", e),
        }

        // Sleep to simulate work
        std::thread::sleep(Duration::from_secs(10));
    }

    // Stop the background compactor
    println!("\n--- Stopping Background Compactor ---");
    compactor.stop();
    println!("✓ Background compactor stopped");

    // Get final statistics
    println!("\n--- Final Statistics ---");
    match compactor.stats() {
        Ok(stats) => {
            println!("Total size: {} bytes", stats.total_size);
            println!("Total tombstones: {}", stats.total_tombstones);
            println!("Models: {}", stats.models.len());
            
            for model in &stats.models {
                println!("\n  {}:", model.model_name);
                println!("    Size: {} bytes", model.size);
                println!("    Tombstones: {}", model.tombstone_count);
                if let Some(ratio) = model.fragmentation_ratio {
                    println!("    Fragmentation: {:.2}%", ratio * 100.0);
                }
            }
        }
        Err(e) => println!("Error getting final stats: {}", e),
    }

    println!("\n✓ Example completed successfully!");
    println!("Note: In production, the background compactor would continue running");
    println!("      until explicitly stopped or the application exits.");

    Ok(())
}
