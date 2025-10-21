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
        dead_space_threshold: 0.05, // Compact when 5% of space is dead
        auto_compact: true,
        check_interval_secs: 10,   // Check every 10 seconds
        max_compaction_time_secs: 60, // Max 60 seconds per compaction
    };
    
    println!("✓ Compaction configuration:");
    println!("  Dead space threshold: {}%", config.dead_space_threshold * 100.0);
    println!("  Check interval: {} seconds", config.check_interval_secs);
    println!("  Max compaction time: {} seconds\n", config.max_compaction_time_secs);

    let compactor = BackgroundCompactor::new(&data_dir, config.clone());
    println!("✓ Background compactor created\n");

    // Start the background compactor
    println!("--- Starting Background Compactor ---");
    println!("The compactor will run every {} seconds", config.check_interval_secs);
    println!("Simulating 30 seconds of operation...\n");

    compactor.start();
    println!("✓ Background compactor started");

    // Simulate some work while compactor runs in background
    for i in 1..=3 {
        println!("\n[Tick {}] Main application running...", i);
        
        // Check compaction status
        let status = compactor.status();
        println!("  Compaction status: {:?}", status);
        
        // Get last results
        let last_results = compactor.last_results();
        if !last_results.is_empty() {
            println!("  Last compaction results:");
            for result in &last_results {
                if result.success {
                    println!("    - {}: reclaimed {} bytes", 
                        result.model_name, result.bytes_reclaimed);
                }
            }
        }

        // Sleep to simulate work
        std::thread::sleep(Duration::from_secs(10));
    }

    // Stop the background compactor
    println!("\n--- Stopping Background Compactor ---");
    compactor.stop();
    println!("✓ Background compactor stopped");

    // Get final results
    println!("\n--- Final Statistics ---");
    let final_results = compactor.last_results();
    println!("Total compactions performed: {}", final_results.len());
    
    for result in &final_results {
        if result.success {
            println!("\n  {}:", result.model_name);
            println!("    Bytes before: {}", result.bytes_before);
            println!("    Bytes after: {}", result.bytes_after);
            println!("    Reclaimed: {} bytes ({:.1}%)",
                result.bytes_reclaimed, result.reclaim_percentage());
            println!("    Duration: {} ms", result.duration_ms);
        }
    }

    println!("\n✓ Example completed successfully!");
    println!("Note: In production, the background compactor would continue running");
    println!("      until explicitly stopped or the application exits.");

    Ok(())
}
