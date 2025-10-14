//! Test TypeScript SDK Generation (Sprint 10)
//!
//! This example demonstrates generating a TypeScript SDK from a schema

use sinkdb::{Parser, TypeScriptGenerator};

fn main() {
    println!("🚀 Sprint 10: TypeScript SDK Generation Test\n");

    // Sample schema with multiple models and relations
    let schema_content = r#"
User {
  id: +uuid
  email: ^&string @email
  username: ^string
  age: u32
  created_at: +timestamp
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  content: string
  author: *User
  published: bool
  created_at: +timestamp
  tags: [Tag]
}

Tag {
  id: +uuid
  name: ^&string
  posts: [Post]
}
"#;

    println!("📄 Parsing schema...");
    let mut parser = Parser::new(schema_content).expect("Failed to create parser");
    let schema = parser.parse().expect("Failed to parse schema");

    println!("✅ Parsed {} models\n", schema.models.len());

    println!("🔨 Generating TypeScript SDK...");
    let files = TypeScriptGenerator::generate(&schema);

    println!("✅ Generated {} files:\n", files.len());

    for file in &files {
        let lines = file.content.lines().count();
        println!("  📝 {} ({} lines)", file.path, lines);
    }

    println!("\n📦 Package structure:");
    println!("  - types.ts: Type definitions for all models");
    println!("  - UserApi.ts: API client for User");
    println!("  - PostApi.ts: API client for Post");
    println!("  - TagApi.ts: API client for Tag");
    println!("  - index.ts: Main SDK entry point");
    println!("  - package.json: NPM package configuration");
    println!("  - tsconfig.json: TypeScript configuration");
    println!("  - tsup.config.ts: Bundler configuration");
    println!("  - README.md: SDK documentation");

    // Show sample of generated types
    println!("\n📋 Sample generated types:");
    if let Some(types_file) = files.iter().find(|f| f.path.ends_with("types.ts")) {
        let lines: Vec<&str> = types_file.content.lines().take(30).collect();
        for line in lines {
            println!("  {}", line);
        }
        println!("  ...");
    }

    // Show sample of generated API client
    println!("\n📋 Sample generated API client (UserApi):");
    if let Some(user_api) = files.iter().find(|f| f.path.ends_with("UserApi.ts")) {
        let lines: Vec<&str> = user_api.content.lines().take(35).collect();
        for line in lines {
            println!("  {}", line);
        }
        println!("  ...");
    }

    println!("\n✨ Sprint 10 complete! TypeScript SDK ready to use.");
    println!("\nUsage example:");
    println!("  import {{ SinkDBClient }} from '@sinkdb/client';");
    println!("  const client = new SinkDBClient('http://localhost:3000');");
    println!("  const users = await client.user.list();");
    println!("  const user = await client.user.get(users.data[0].id);");
}
