/// Default blank schema template
pub fn blank_schema() -> &'static str {
    r#"// SinkDB Schema
// Define your models below

User {
  id: +uuid
  email: ^&string @email
  created_at: +timestamp
}
"#
}

/// Blog template schema
pub fn blog_schema() -> &'static str {
    r#"// Blog Schema

User {
  id: +uuid
  username: ^&string
  email: ^&string @email
  password_hash: string
  created_at: +timestamp
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  slug: ^&string
  content: string
  published: bool
  published_at: timestamp?
  created_at: +timestamp
  updated_at: +timestamp
  author: *User
  tags: [Tag]
}

Tag {
  id: +uuid
  name: ^&string
  posts: [Post]
}
"#
}

/// E-commerce template schema
pub fn ecommerce_schema() -> &'static str {
    r#"// E-commerce Schema

User {
  id: +uuid
  email: ^&string @email
  name: string
  created_at: +timestamp
  orders: [Order]
}

Product {
  id: +uuid
  sku: ^&string
  name: string
  description: string
  price: f64 @min(0)
  stock: u32
  created_at: +timestamp
}

Order {
  id: +uuid
  customer: *User
  status: string
  total: f64
  created_at: +timestamp
  items: [OrderItem]
}

OrderItem {
  id: +uuid
  order: *Order
  product: *Product
  quantity: u32 @min(1)
  price: f64
}
"#
}

/// Todo app template schema
pub fn todo_schema() -> &'static str {
    r#"// Todo App Schema

User {
  id: +uuid
  email: ^&string @email
  name: string
  created_at: +timestamp
  todos: [Todo]
}

Todo {
  id: +uuid
  title: string
  description: string?
  completed: bool
  due_date: timestamp?
  created_at: +timestamp
  updated_at: +timestamp
  owner: *User
}
"#
}

/// Default sinkdb.toml configuration
pub fn default_config(project_name: &str) -> String {
    format!(
        r#"[project]
name = "{}"
version = "0.1.0"

[database]
path = "./data/db"
wal_path = "./data/wal"
page_size = 4096
compaction_threshold = 0.20

[api]
port = 3000
host = "127.0.0.1"
cors_origins = ["http://localhost:5173"]

[dev]
hot_reload = true
watch_paths = ["schema.sink", "src/"]
browser = true

[codegen]
rust_output = "./generated"
typescript_output = "./generated"
format_rust = true
format_typescript = false
"#,
        project_name
    )
}

/// Default .gitignore
pub fn default_gitignore() -> &'static str {
    r#"# SinkDB Generated Files
/generated/

# Database Files
/data/

# Rust
/target/
**/*.rs.bk
Cargo.lock

# TypeScript / Node
node_modules/
dist/
*.js
*.d.ts
!vite.config.js

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db
"#
}

/// Example main.rs for Rust backend
pub fn rust_main_template() -> &'static str {
    r#"use std::path::Path;

mod generated {
    include!("../generated/database.rs");
}

use generated::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting SinkDB application...");

    // Initialize database
    let db_path = Path::new("./data/db");
    let mut db = Database::new(db_path)?;

    println!("✓ Database initialized");
    println!("✓ Ready to use!");

    Ok(())
}
"#
}

/// README template
pub fn readme_template(project_name: &str) -> String {
    format!(
        r#"# {}

A SinkDB project.

## Getting Started

### Development

```bash
# Generate code from schema
cargo run

# Run the application
cargo run --example basic
```

### Schema

Edit `schema.sink` to define your data models. The code will be automatically generated.

### Generated Code

- `generated/database.rs` - Rust database implementation

## Learn More

- [SinkDB Documentation](https://github.com/yourusername/sinkdb)
- [Schema Language Guide](https://github.com/yourusername/sinkdb/blob/main/docs/schema.md)
"#,
        project_name
    )
}
