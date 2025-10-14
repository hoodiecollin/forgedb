// Sprint 5: File Watcher Example
//
// This example demonstrates the file watching and auto-regeneration functionality.
// It watches a schema file for changes and automatically regenerates code when
// the schema is modified.
//
// Usage:
//   cargo run --example sprint5_watcher
//
// Then in another terminal, modify the test_watch_schema.sink file to see
// auto-regeneration in action.

use sinkdb_watcher::{auto_watch, RegenerateResult};
use std::fs;
use std::path::Path;

fn main() {
    println!("=== Sprint 5: File Watcher Demo ===\n");

    // Setup test schema file
    let schema_path = "test_watch_schema.sink";
    let output_dir = "generated_watch";

    // Create initial schema if it doesn't exist
    if !Path::new(schema_path).exists() {
        let initial_schema = r#"User {
  id: +uuid
  email: ^&string @email
  username: ^string
  created_at: +timestamp
}

Post {
  id: +uuid
  title: string
  content: string
  author: *User
  created_at: +timestamp
}
"#;
        fs::write(schema_path, initial_schema).expect("Failed to create test schema");
        println!("✓ Created test schema at {}", schema_path);
    }

    println!("\nStarting file watcher...");
    println!("Try modifying {} to see auto-regeneration!\n", schema_path);
    println!("Example changes you can make:");
    println!("  - Add a new field to User model");
    println!("  - Add a new model");
    println!("  - Change field types or constraints");
    println!("\nPress Ctrl+C to stop watching.\n");
    println!("{}", "=".repeat(60));

    // Start watching with callback for status display
    let result = auto_watch(
        schema_path,
        output_dir,
        200, // 200ms debounce
        Some(Box::new(|result: &RegenerateResult| {
            display_result(result);
        })),
    );

    if let Err(e) = result {
        eprintln!("\n✗ Watcher error: {}", e);
        std::process::exit(1);
    }
}

fn display_result(result: &RegenerateResult) {
    println!("\n{}", "-".repeat(60));

    if result.success {
        println!("✓ SUCCESS");
        println!("  {}", result.message);

        if let Some(ref path) = result.output_path {
            println!("  Output: {}", path.display());
        }
    } else {
        println!("✗ FAILED");
        println!("  {}", result.message);

        // Display helpful hints for common errors
        if result.message.contains("not found") {
            println!("\n  Hint: Make sure the schema file exists");
        } else if result.message.contains("Parse error") {
            println!("\n  Hint: Check your schema syntax");
            println!("        Valid example: User {{ id: +uuid, email: string }}");
        }
    }

    println!("{}", "-".repeat(60));
}
