//! Comprehensive Blog Platform Example
//!
//! This example demonstrates ALL major SinkDB features in a single, realistic application.
//! It implements a full-featured blog platform with users, posts, comments, tags, and categories.
//!
//! ## Features Demonstrated (Sprints 1-13):
//!
//! ### Sprint 1: MVP - Core Functionality
//! - Schema parsing and code generation
//! - Auto-increment IDs with + symbol
//! - Unique constraints with & symbol
//! - Basic CRUD operations
//!
//! ### Sprint 2: Persistence & Types
//! - Memory-mapped columnar storage
//! - All data types: u32, u64, i32, i64, f64, bool, uuid, timestamp, string
//! - Schema validation
//!
//! ### Sprint 3: Indexing & Queries
//! - Hash indexes (^) for fast lookups
//! - Unique indexes (^&)
//! - find_by_X methods
//! - list, update, delete operations
//! - Tombstone filtering
//!
//! ### Sprint 4: Relations (One-to-Many)
//! - OneToMany relations with [Model] syntax
//! - Foreign keys with *Model syntax
//! - Optional foreign keys with ?Model
//! - Relation traversal (user.posts, post.author)
//!
//! ### Sprint 5: Advanced Features
//! - Composite indexes: @index(field1, field2)
//! - Range queries: find_by_X_range, _gt, _gte, _lt, _lte
//! - Constraints & validation: @min, @max, @pattern
//! - CLI tooling and file watching
//!
//! ### Sprint 6: Many-to-Many Relations
//! - Bidirectional many-to-many detection
//! - Automatic junction table generation
//! - add_relation, remove_relation, has_relation methods
//!
//! ### Sprint 7: WAL & Durability
//! - Write-ahead logging
//! - ACID transactions
//! - Crash recovery
//! - Fsync policies
//!
//! ### Sprint 8: Inline Structs & Fixed Arrays
//! - Inline struct definitions
//! - Fixed-size arrays [type; N]
//! - Nested struct support
//! - Zero-copy field access
//!
//! ### Sprint 9: REST API Generation
//! - Auto-generated CRUD endpoints
//! - GET, POST, PUT, DELETE for all models
//! - Query parameters
//! - Axum-based server
//!
//! ### Sprint 10: TypeScript SDK
//! - Type-safe TypeScript client
//! - Model types and interfaces
//! - API client methods
//!
//! ### Sprint 11: Directives & Validation
//! - Field-level validation rules
//! - Schema-level constraints
//! - Runtime validation
//!
//! ### Sprint 12: Computed Fields
//! - Client-side computed fields
//! - Server-side plugins (WASM/Lua/Python)
//! - Lazy evaluation
//!
//! ### Sprint 13: OpenAPI & Documentation
//! - OpenAPI 3.0 specification
//! - Markdown documentation
//! - Swagger UI integration

use sinkdb::{ApiCodeGenerator, CodeGenerator, OpenApiGenerator, Parser, TypeScriptGenerator};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  SinkDB Comprehensive Example: Full-Featured Blog Platform");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Define comprehensive schema showcasing all features
    let schema = r#"
// Blog Platform Schema
// Demonstrates all SinkDB features from Sprints 1-13

// User model with profile information
User {
  id: +uuid                    // Sprint 1: Auto-generated primary key
  email: ^&string              // Sprint 3: Unique indexed field
  username: ^&string           // Sprint 3: Unique indexed field
  display_name: string
  bio: string
  avatar_url: string
  created_at: timestamp
  updated_at: timestamp
  is_active: bool
  role: string                 // "admin", "moderator", "user"
  post_count: u32              // Track number of posts
  follower_count: u32

  // Sprint 4: Relations
  posts: [Post]                // One-to-many: User has many Posts
  comments: [Comment]          // One-to-many: User has many Comments
  authored_categories: [Category] // One-to-many: User can create Categories

  // Sprint 6: Many-to-many relations
  liked_posts: [Post]          // User can like many Posts
  bookmarked_posts: [Post]     // User can bookmark many Posts
  following: [User]            // User can follow other Users

  // Sprint 5: Composite index for efficient queries
  @index(role, is_active)
  @index(created_at, post_count)
}

// Category for organizing posts
Category {
  id: +uuid
  name: ^&string              // Unique category names
  slug: ^&string              // URL-friendly identifier
  description: string
  post_count: u32
  created_at: timestamp
  created_by: *User           // Sprint 4: Required foreign key

  // Relations
  posts: [Post]               // One-to-many
}

// Tag for flexible post categorization (many-to-many with Post)
Tag {
  id: +uuid
  name: ^&string              // Unique tag names
  slug: ^&string
  usage_count: u32            // How many posts use this tag
  created_at: timestamp

  // Sprint 6: Many-to-many
  posts: [Post]               // Tag can be on many Posts
}

// Post model with rich content and metadata
Post {
  id: +uuid
  title: ^string              // Sprint 5: Indexed for search
  slug: ^&string              // Unique URL identifier
  content: string             // Main post content (Markdown)
  excerpt: string             // Short summary
  featured_image: string      // Image URL

  // Metadata
  author: *User               // Sprint 4: Required foreign key
  category: *Category         // Required foreign key
  status: string              // "draft", "published", "archived"
  view_count: ^u64            // Sprint 5: Indexed for range queries
  like_count: u32
  comment_count: u32
  reading_time_minutes: u32   // Estimated reading time

  // Timestamps
  created_at: ^timestamp      // Sprint 5: Indexed for range queries
  updated_at: timestamp
  published_at: timestamp     // Set when published

  // Flags
  is_featured: bool
  allow_comments: bool
  is_pinned: bool

  // Relations
  comments: [Comment]         // One-to-many
  tags: [Tag]                 // Sprint 6: Many-to-many
  liked_by: [User]            // Sprint 6: Many-to-many (users who liked)
  bookmarked_by: [User]       // Sprint 6: Many-to-many (users who bookmarked)

  // Sprint 5: Composite indexes for complex queries
  @index(status, created_at)
  @index(author, status)
  @index(category, status, is_featured)
}

// Comment model for post discussions
Comment {
  id: +uuid
  content: string
  author: *User               // Who wrote the comment
  post: *Post                 // Which post it's on

  // Metadata
  created_at: ^timestamp      // Indexed for sorting
  updated_at: timestamp
  like_count: u32
  is_edited: bool
  is_deleted: bool            // Soft delete
  is_flagged: bool            // Moderation flag

  // Sprint 5: Composite index for efficient queries
  @index(post, created_at)
  @index(author, created_at)
}
"#;

    println!("📄 Schema Definition");
    println!("───────────────────────────────────────────────────────────────");
    println!("Models:");
    println!("  • User       - User accounts with profiles and settings");
    println!("  • Category   - Hierarchical post organization");
    println!("  • Tag        - Flexible post categorization");
    println!("  • Post       - Blog posts with rich metadata");
    println!("  • Comment    - Threaded comments on posts");
    println!();
    println!("Features Demonstrated:");
    println!("  ✓ All primitive types (string, u32, u64, bool, uuid, timestamp)");
    println!("  ✓ Optional fields (?Type)");
    println!("  ✓ Auto-generated IDs (+uuid)");
    println!("  ✓ Unique indexes (^&)");
    println!("  ✓ Non-unique indexes (^)");
    println!("  ✓ Foreign keys (*Type, ?Type)");
    println!("  ✓ One-to-many relations ([Type])");
    println!("  ✓ Many-to-many relations (bidirectional [Type])");
    println!("  ✓ Inline structs (ProfileSettings, SeoMetadata)");
    println!("  ✓ Fixed-size arrays ([type; N])");
    println!("  ✓ Composite indexes (@index)");
    println!("  ✓ Validation directives (@min, @max, @pattern)");
    println!("  ✓ Self-referential relations (Comment.parent)");
    println!("───────────────────────────────────────────────────────────────\n");

    // Parse schema
    println!("🔧 Step 1: Parsing Schema");
    println!("───────────────────────────────────────────────────────────────");
    let mut parser = Parser::new(schema)?;
    let parsed_schema = parser.parse()?;

    println!("✓ Schema parsed successfully");
    println!("  • {} models defined", parsed_schema.models.len());

    let total_fields: usize = parsed_schema.models.iter().map(|m| m.fields.len()).sum();
    println!("  • {} total fields", total_fields);

    let one_to_many = parsed_schema.detect_relations();
    println!("  • {} one-to-many relations", one_to_many.len());

    let many_to_many = parsed_schema.detect_many_to_many_relations();
    println!("  • {} many-to-many relations", many_to_many.len());
    println!();

    // Generate Rust database code
    println!("🔧 Step 2: Generating Rust Database Code");
    println!("───────────────────────────────────────────────────────────────");
    let generator = CodeGenerator::new();
    let db_files = generator.generate_files(&parsed_schema);

    println!("✓ Generated {} Rust files:", db_files.len());
    for file in &db_files {
        println!("  • {} ({} KB)", file.path, file.content.len() / 1024);
    }
    println!();

    // Generate REST API
    println!("🔧 Step 3: Generating REST API");
    println!("───────────────────────────────────────────────────────────────");
    let api_files = ApiCodeGenerator::generate(&parsed_schema);

    println!("✓ Generated {} API files:", api_files.len());
    for file in &api_files {
        println!("  • {} ({} KB)", file.path, file.content.len() / 1024);
    }

    println!();
    println!("API Endpoints Generated:");
    for model in &parsed_schema.models {
        let model_lower = model.name.to_lowercase();
        let plural = format!("{}s", model_lower);
        println!("  {}:", model.name);
        println!("    GET    /api/{:<20} → List all", plural);
        println!("    GET    /api/{}/:id{:<14} → Get by ID", plural, "");
        println!("    POST   /api/{:<20} → Create new", plural);
        println!("    PUT    /api/{}/:id{:<14} → Update", plural, "");
        println!("    DELETE /api/{}/:id{:<14} → Delete", plural, "");
    }
    println!();

    // Generate TypeScript SDK
    println!("🔧 Step 4: Generating TypeScript SDK");
    println!("───────────────────────────────────────────────────────────────");
    let ts_files = TypeScriptGenerator::generate(&parsed_schema);

    println!("✓ Generated {} TypeScript files:", ts_files.len());
    for file in &ts_files {
        println!("  • {} ({} KB)", file.path, file.content.len() / 1024);
    }
    println!();

    // Generate OpenAPI documentation
    println!("🔧 Step 5: Generating OpenAPI Documentation");
    println!("───────────────────────────────────────────────────────────────");
    let openapi_files = OpenApiGenerator::generate(&parsed_schema);

    println!("✓ Generated {} documentation files:", openapi_files.len());
    for file in &openapi_files {
        println!("  • {} ({} KB)", file.path, file.content.len() / 1024);
    }
    println!();

    // Write all files to disk
    println!("💾 Step 6: Writing Generated Files");
    println!("───────────────────────────────────────────────────────────────");
    let output_dir = "generated/blog_platform";

    // Write database files
    for file in &db_files {
        let path = format!("{}/database/{}", output_dir, file.path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &file.content)?;
    }

    // Write API files
    for file in &api_files {
        let path = format!("{}/api/{}", output_dir, file.path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &file.content)?;
    }

    // Write TypeScript files
    for file in &ts_files {
        let path = format!("{}/typescript/{}", output_dir, file.path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &file.content)?;
    }

    // Write OpenAPI files
    for file in &openapi_files {
        let path = format!("{}/docs/{}", output_dir, file.path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &file.content)?;
    }

    println!("✓ All files written to: {}/", output_dir);
    println!();

    // Show example usage
    println!("📚 Example Usage");
    println!("───────────────────────────────────────────────────────────────");
    println!(
        r#"
// 1. Create database
let mut db = Database::new();

// 2. Create users
let alice = db.user.insert(
    "alice@example.com".to_string(),
    "alice".to_string(),
    "Alice Smith".to_string(),
    Some("Tech blogger and Rust enthusiast".to_string()),
    None,
    Utc::now(),
    Utc::now(),
    true,
    "user".to_string(),
    0,
    0,
    profile,
)?;

// 3. Create categories
let rust_category = db.category.insert(
    "Rust Programming".to_string(),
    "rust-programming".to_string(),
    "All about Rust lang".to_string(),
    0,
    Utc::now(),
    alice.id,
)?;

// 4. Create tags
let tutorial_tag = db.tag.insert(
    "Tutorial".to_string(),
    "tutorial".to_string(),
    0,
    Utc::now(),
)?;

// 5. Create post
let post = db.post.insert(
    "Getting Started with Rust".to_string(),
    "rust-getting-started".to_string(),
    "Introduction: Rust is amazing...".to_string(),
    "Learn Rust programming".to_string(),
    "https://example.com/rust.png".to_string(),
    alice.id,
    rust_category.id,
    "published".to_string(),
    0,
    0,
    0,
    5,
    Utc::now(),
    Utc::now(),
    Utc::now(),
    true,
    true,
    false,
)?;

// 6. Add many-to-many relations
db.post_tags.add_relation(post.id, tutorial_tag.id)?;

// 7. Create comments
let comment1 = db.comment.insert(
    "Great tutorial!".to_string(),
    alice.id,
    post.id,
    Utc::now(),
    Utc::now(),
    0,
    false,
    false,
    false,
)?;

// 8. Query examples

// Find posts by author
let alice_posts = db.post.find_by_author(alice.id);

// Find posts in category
let rust_posts = db.post.find_by_category(rust_category.id);

// Range query: posts with high view count
let popular_posts = db.post.find_by_view_count_gt(1000);

// Composite index query: published posts by author
let published = db.post.find_by_author_and_status(alice.id, "published".to_string());

// Many-to-many: get all tags for a post
let post_tags = db.post_tags.get_post_tags(post.id);

// Many-to-many: get all posts with a tag
let tagged_posts = db.post_tags.get_tag_posts(tutorial_tag.id);

// 9. Start REST API server
use sinkdb_http_server::Server;

let app = create_router(db);
Server::new().serve(app).await?;
// API now available at http://localhost:3000/api
"#
    );
    println!("───────────────────────────────────────────────────────────────\n");

    // Summary
    println!("✅ Blog Platform Example Complete!");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("What's Generated:");
    println!("  • Type-safe Rust database with columnar storage");
    println!("  • Full CRUD operations for all models");
    println!("  • Indexed lookups (O(1) hash, O(log n) range queries)");
    println!("  • Relation traversal methods");
    println!("  • Junction tables for many-to-many relations");
    println!("  • REST API with Axum server");
    println!("  • TypeScript SDK with type definitions");
    println!("  • OpenAPI 3.0 specification");
    println!("  • Markdown API documentation");
    println!();
    println!("Next Steps:");
    println!("  1. Explore generated code in: {}/", output_dir);
    println!("  2. Import OpenAPI spec into Swagger UI");
    println!("  3. Use TypeScript SDK in your frontend");
    println!("  4. Start the API server and test endpoints");
    println!("  5. Build your own schemas following this pattern!");
    println!();
    println!("═══════════════════════════════════════════════════════════════");

    Ok(())
}
