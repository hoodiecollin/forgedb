use crate::{error::CliError, ui, Result};
use sinkdb_watcher::{auto_watch, RegenerateResult};
use std::path::Path;

pub struct DevOptions {
    pub schema: String,
    pub output: String,
    pub debounce: u64,
    pub clear: bool,
}

pub fn run(options: DevOptions) -> Result<()> {
    // Verify schema file exists
    let schema_path = Path::new(&options.schema);
    if !schema_path.exists() {
        return Err(CliError::SchemaNotFound(format!(
            "Schema file not found: {}",
            options.schema
        )));
    }

    // Print initial header
    ui::header("👁️", "SinkDB Dev Mode");
    println!();
    ui::info(&format!("Watching: {}", options.schema));
    ui::info(&format!("Output:   {}", options.output));
    ui::info(&format!("Debounce: {}ms", options.debounce));
    println!();
    ui::info("Press Ctrl+C to stop watching");
    println!();
    println!("{}", "─".repeat(60));
    println!();

    // Start watching with callback
    let clear_terminal = options.clear;
    let callback = Box::new(move |result: &RegenerateResult| {
        // Clear terminal if requested
        if clear_terminal {
            clear_screen();
        }

        // Print result
        if result.success {
            ui::success(&result.message);
            if let Some(ref path) = result.output_path {
                ui::info(&format!("Generated: {}", path.display()));
            }
        } else {
            ui::error(&result.message);
        }

        println!();
        ui::info("Waiting for changes...");
        println!();
    });

    // Run watcher (blocks until Ctrl+C)
    auto_watch(
        &options.schema,
        &options.output,
        options.debounce,
        Some(callback),
    )
    .map_err(|e| CliError::Other(format!("Watcher error: {}", e)))?;

    Ok(())
}

/// Clear the terminal screen
fn clear_screen() {
    // ANSI escape codes for clearing screen
    print!("\x1B[2J\x1B[1;1H");
}
