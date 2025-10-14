// Sprint 13: OpenAPI & Documentation Example
// Demonstrates generating OpenAPI 3.0 specs and markdown documentation from schemas

use sinkdb::{Parser, OpenApiGenerator};
use std::fs;
use std::path::Path;

fn main() {
    println!("Sprint 13: OpenAPI & Documentation");
    println!("===================================\n");

    // Example schema with multiple models and various field types
    let schema_text = r#"
User {
  id: +uuid
  email: ^&string
  username: &string
  created_at: timestamp
  posts: [Post]
  comments: [Comment]
}

Post {
  id: +uuid
  title: &string
  content: string
  author: *User
  published: bool
  created_at: timestamp
  view_count: i64
  tags: [string; 10]
  comments: [Comment]
}

Comment {
  id: +uuid
  post: *Post
  author: *User
  content: string
  created_at: timestamp
  likes: i32
}
"#;

    println!("📄 Parsing schema...");
    let mut parser = match Parser::new(schema_text) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Parser creation error: {}", e);
            return;
        }
    };

    let schema = match parser.parse() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Parse error: {:?}", e);
            return;
        }
    };
    println!("✓ Schema parsed successfully\n");

    println!("📋 Schema contains:");
    for model in &schema.models {
        println!("  - {} ({} fields)", model.name, model.fields.len());
    }
    println!();

    println!("🔧 Generating OpenAPI documentation...");
    let generated_files = OpenApiGenerator::generate(&schema);
    println!("✓ Generated {} files\n", generated_files.len());

    // Debug: show what files were generated
    println!("Generated file paths:");
    for file in &generated_files {
        println!("  - {}", file.path);
    }
    println!();

    // Create output directory
    let output_dir = "generated/openapi";
    fs::create_dir_all(output_dir).expect("Failed to create output directory");
    println!("📁 Output directory: {}", output_dir);

    // Write generated files
    for file in &generated_files {
        let file_path = Path::new(output_dir).join(&file.path);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .expect(&format!("Failed to create parent directory for: {}", file.path));
        }

        fs::write(&file_path, &file.content)
            .expect(&format!("Failed to write file: {}", file.path));

        println!("  ✓ {} ({} bytes)", file.path, file.content.len());
    }
    println!();

    // Validate OpenAPI JSON
    println!("🔍 Validating OpenAPI spec...");
    let openapi_file = generated_files.iter()
        .find(|f| f.path.ends_with("openapi.json"))
        .expect("OpenAPI spec not found");

    match serde_json::from_str::<serde_json::Value>(&openapi_file.content) {
        Ok(spec) => {
            println!("✓ OpenAPI spec is valid JSON");

            // Check key components
            if let Some(obj) = spec.as_object() {
                println!("  - OpenAPI version: {}", obj.get("openapi").unwrap());

                if let Some(info) = obj.get("info") {
                    println!("  - API title: {}", info["title"]);
                    println!("  - API version: {}", info["version"]);
                }

                if let Some(paths) = obj.get("paths").and_then(|p| p.as_object()) {
                    println!("  - Endpoints: {}", paths.len());
                }

                if let Some(schemas) = obj.get("components")
                    .and_then(|c| c.get("schemas"))
                    .and_then(|s| s.as_object()) {
                    println!("  - Schemas: {}", schemas.len());
                    println!("    Schema names:");
                    for name in schemas.keys() {
                        println!("      • {}", name);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Invalid JSON: {}", e);
            return;
        }
    }
    println!();

    // Display sample of markdown documentation
    println!("📖 Markdown documentation preview:");
    println!("───────────────────────────────────");
    let markdown_file = generated_files.iter()
        .find(|f| f.path.ends_with("API.md"))
        .expect("Markdown docs not found");

    let preview_lines: Vec<&str> = markdown_file.content.lines().take(30).collect();
    for line in preview_lines {
        println!("{}", line);
    }

    if markdown_file.content.lines().count() > 30 {
        println!("\n... ({} more lines)", markdown_file.content.lines().count() - 30);
    }
    println!("───────────────────────────────────");
    println!();

    println!("✅ OpenAPI generation complete!");
    println!("\nYou can now:");
    println!("  1. Import generated/openapi/openapi.json into Swagger Editor");
    println!("  2. View generated/openapi/API.md for documentation");
    println!("  3. Use the spec with API testing tools like Postman");
}
