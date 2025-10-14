// Sprint 5: Project Scaffolding Example
// This example demonstrates the scaffolding functionality for creating new SinkDB projects

use sinkdb::scaffold::{ScaffoldConfig, Scaffolder};
use std::path::PathBuf;
use std::fs;

fn main() {
    println!("=== Sprint 5: Project Scaffolding Example ===\n");

    // Create a test project in a temporary location
    let temp_dir = std::env::temp_dir().join("sinkdb_scaffold_demo");

    // Clean up if it exists from a previous run
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).ok();
    }

    println!("Creating new SinkDB project: scaffold_demo");
    println!("Location: {:?}\n", temp_dir);

    // Configure the scaffolder
    let config = ScaffoldConfig {
        project_name: "scaffold_demo".to_string(),
        project_path: temp_dir.clone(),
    };

    let scaffolder = Scaffolder::new(config);

    // Scaffold the project
    match scaffolder.scaffold() {
        Ok(_) => {
            println!("✓ Project scaffolded successfully!\n");

            // Show the generated structure
            println!("Generated project structure:");
            print_directory_tree(&temp_dir, 0);

            println!("\n--- Generated Files ---\n");

            // Show some of the generated files
            println!("=== schema.sink ===");
            if let Ok(content) = fs::read_to_string(temp_dir.join("schema.sink")) {
                println!("{}", content);
            }

            println!("\n=== sinkdb.toml ===");
            if let Ok(content) = fs::read_to_string(temp_dir.join("sinkdb.toml")) {
                println!("{}", content);
            }

            println!("\n=== .gitignore (first 10 lines) ===");
            if let Ok(content) = fs::read_to_string(temp_dir.join(".gitignore")) {
                for (i, line) in content.lines().enumerate() {
                    if i >= 10 { break; }
                    println!("{}", line);
                }
            }

            println!("\n✓ All files generated successfully!");
            println!("\nNext steps for a real project:");
            println!("  1. cd scaffold_demo");
            println!("  2. Edit schema.sink to define your models");
            println!("  3. Run 'sinkdb generate' to generate database code");
            println!("  4. Build and run your application");

            // Clean up the demo project
            println!("\n(Cleaning up demo project...)");
            fs::remove_dir_all(&temp_dir).ok();
        }
        Err(e) => {
            eprintln!("Error scaffolding project: {}", e);
            std::process::exit(1);
        }
    }
}

fn print_directory_tree(path: &PathBuf, depth: usize) {
    let indent = "  ".repeat(depth);

    if let Ok(entries) = fs::read_dir(path) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            if entry.path().is_dir() {
                println!("{}📁 {}/", indent, file_name_str);
                print_directory_tree(&entry.path(), depth + 1);
            } else {
                println!("{}📄 {}", indent, file_name_str);
            }
        }
    }
}
