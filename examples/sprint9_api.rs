//! Sprint 9: REST API Generation Example
//!
//! This example demonstrates the API code generation capabilities.
//! It shows how to generate REST API code from a schema definition.

use sinkdb::{ApiCodeGenerator, Parser};

fn main() {
    println!("=== Sprint 9: REST API Generation ===\n");

    // Example schema with multiple models
    let schema_source = r#"
User {
    id: +uuid
    email: ^&string
    name: string
    age: u32
}

Post {
    id: +uuid
    title: string
    content: string
    author: *User
    published: bool
}
"#;

    // Parse the schema
    println!("1. Parsing schema...");
    let mut parser = match Parser::new(schema_source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("   ✗ Parser creation error: {}", e);
            return;
        }
    };

    let schema = match parser.parse() {
        Ok(schema) => {
            println!("   ✓ Schema parsed successfully");
            println!("   - {} model(s) found", schema.models.len());
            schema
        }
        Err(e) => {
            eprintln!("   ✗ Parse error: {}", e);
            return;
        }
    };

    // Generate API code
    println!("\n2. Generating API code...");
    let api_files = ApiCodeGenerator::generate(&schema);
    println!("   ✓ Generated {} file(s):", api_files.len());
    for file in &api_files {
        println!("     - {} ({} bytes)", file.path, file.content.len());
    }

    // Show sample generated code
    println!("\n3. Sample Generated Code:");
    println!("   ================================");

    // Show User types
    if let Some(user_types) = api_files.iter().find(|f| f.path.contains("user_types")) {
        println!("\n   File: {}", user_types.path);
        println!("   {}", "-".repeat(60));
        // Show first 30 lines
        for (i, line) in user_types.content.lines().take(30).enumerate() {
            println!("   {:3} | {}", i + 1, line);
        }
        if user_types.content.lines().count() > 30 {
            println!("   ... ({} more lines)", user_types.content.lines().count() - 30);
        }
    }

    // Show router
    if let Some(router) = api_files.iter().find(|f| f.path.contains("router")) {
        println!("\n   File: {}", router.path);
        println!("   {}", "-".repeat(60));
        for (i, line) in router.content.lines().enumerate() {
            println!("   {:3} | {}", i + 1, line);
        }
    }

    // Show API endpoints
    println!("\n4. Generated API Endpoints:");
    println!("   ================================");
    for model in &schema.models {
        let model_lower = model.name.to_lowercase();
        let plural = format!("{}s", model_lower);
        println!("\n   {}:", model.name);
        println!("   - GET    /api/{:<15} → list_{}()", plural, model_lower);
        println!("   - GET    /api/{}/:id{:<8} → get_{}()", plural, "", model_lower);
        println!("   - POST   /api/{:<15} → create_{}()", plural, model_lower);
        println!("   - PUT    /api/{}/:id{:<8} → update_{}()", plural, "", model_lower);
        println!("   - DELETE /api/{}/:id{:<8} → delete_{}()", plural, "", model_lower);
    }

    // Show integration instructions
    println!("\n5. Next Steps:");
    println!("   ================================");
    println!("   To use the generated API:");
    println!("   1. Write generated files to disk");
    println!("   2. Add dependencies to Cargo.toml:");
    println!("      - axum = \"0.7\"");
    println!("      - tokio = {{ version = \"1\", features = [\"full\"] }}");
    println!("      - sinkdb-http-server");
    println!("      - sinkdb-query-params");
    println!("   3. Import the generated router:");
    println!("      use generated::api::create_router;");
    println!("   4. Start the server:");
    println!("      let app = create_router();");
    println!("      Server::new().serve(app).await?;");

    println!("\n=== Sprint 9 Demo Complete ===");
}
