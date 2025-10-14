// Sprint 5: Project Scaffolding Example
// This example demonstrates the scaffolding functionality for creating new SinkDB projects

use sinkdb_cli::commands::init::{run, InitOptions};
use std::fs;

fn main() {
    println!("=== Sprint 5: Project Scaffolding Example ===\n");

    // Create a test project in a temporary location
    let project_name = "sinkdb_scaffold_demo";

    // Clean up if it exists from a previous run
    if std::path::Path::new(project_name).exists() {
        fs::remove_dir_all(project_name).ok();
    }

    println!("Creating new SinkDB project: {}", project_name);
    println!();

    // Initialize the project using the CLI command
    let options = InitOptions {
        project_name: project_name.to_string(),
        template: Some("blank".to_string()),
        rust: true,
        typescript: false,
        api_only: false,
    };

    // Execute the init command
    match run(options) {
        Ok(_) => {
            println!("\n--- Generated Files ---\n");

            // Show some of the generated files
            println!("=== schema.sink ===");
            if let Ok(content) = fs::read_to_string(format!("{}/schema.sink", project_name)) {
                println!("{}", content);
            }

            println!("\n=== sinkdb.toml ===");
            if let Ok(content) = fs::read_to_string(format!("{}/sinkdb.toml", project_name)) {
                println!("{}", content);
            }

            println!("\n=== .gitignore (first 10 lines) ===");
            if let Ok(content) = fs::read_to_string(format!("{}/.gitignore", project_name)) {
                for (i, line) in content.lines().enumerate() {
                    if i >= 10 { break; }
                    println!("{}", line);
                }
            }

            // Clean up the demo project
            println!("\n(Cleaning up demo project...)");
            fs::remove_dir_all(project_name).ok();
        }
        Err(e) => {
            eprintln!("Error scaffolding project: {}", e);
            std::process::exit(1);
        }
    }
}
