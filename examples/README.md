# SinkDB Comprehensive Example

This directory contains a single, comprehensive example application that demonstrates **all major SinkDB features** in one realistic, production-ready application.

## 📚 The Blog Platform Example

A full-featured blog platform that showcases every capability of SinkDB from Sprints 1-13.

### What It Includes

**Models:**
- **User** - User accounts with profile settings and social features
- **Category** - Hierarchical post organization
- **Tag** - Flexible post categorization
- **Post** - Rich blog posts with SEO metadata and engagement tracking
- **Comment** - Threaded comment system with moderation

**Features Demonstrated:**

#### Sprint 1: MVP - Core Functionality
- ✅ Schema parsing and code generation
- ✅ Auto-increment IDs with `+` symbol
- ✅ Unique constraints with `&` symbol
- ✅ Basic CRUD operations

#### Sprint 2: Persistence & Types
- ✅ Memory-mapped columnar storage
- ✅ All data types: `u32`, `u64`, `i32`, `i64`, `f64`, `bool`, `uuid`, `timestamp`, `string`
- ✅ Schema validation with helpful errors

#### Sprint 3: Indexing & Queries
- ✅ Hash indexes (`^`) for O(1) lookups
- ✅ Unique indexes (`^&`)
- ✅ `find_by_X` methods for indexed fields
- ✅ `list`, `update`, `delete` operations
- ✅ Tombstone filtering for soft deletes

#### Sprint 4: Relations (One-to-Many)
- ✅ OneToMany relations with `[Model]` syntax
- ✅ Foreign keys with `*Model` (required) and `?Model` (optional)
- ✅ Automatic FK validation
- ✅ Relation traversal methods (`user.posts`, `post.author`)

#### Sprint 5: Advanced Features
- ✅ Composite indexes: `@index(field1, field2)`
- ✅ Range queries: `find_by_X_range`, `_gt`, `_gte`, `_lt`, `_lte`
- ✅ B-tree indexes for ordered types
- ✅ Constraints & validation: `@min`, `@max`, `@pattern`
- ✅ CLI tooling and project scaffolding

#### Sprint 6: Many-to-Many Relations
- ✅ Bidirectional many-to-many detection
- ✅ Automatic junction table generation
- ✅ `add_relation`, `remove_relation`, `has_relation` methods
- ✅ Efficient querying of related entities

#### Sprint 7: WAL & Durability
- ✅ Write-ahead logging for crash safety
- ✅ ACID transaction support
- ✅ Automatic crash recovery on restart
- ✅ Configurable fsync policies

#### Sprint 8: Inline Structs & Fixed Arrays
- ✅ Inline struct definitions (e.g., `ProfileSettings`, `SeoMetadata`)
- ✅ Fixed-size arrays: `[type; N]`
- ✅ Nested struct support
- ✅ Zero-copy field access

#### Sprint 9: REST API Generation
- ✅ Auto-generated CRUD endpoints for all models
- ✅ GET, POST, PUT, DELETE operations
- ✅ Query parameters for filtering
- ✅ Axum-based HTTP server

#### Sprint 10: TypeScript SDK
- ✅ Type-safe TypeScript client generation
- ✅ Model types and interfaces
- ✅ API client methods matching Rust backend

#### Sprint 11: Directives & Validation
- ✅ Field-level validation rules
- ✅ Schema-level constraints
- ✅ Runtime validation with helpful error messages

#### Sprint 12: Computed Fields
- ✅ Client-side computed fields (default, zero overhead)
- ✅ Server-side plugins (WASM/Lua/Python) when needed
- ✅ Lazy evaluation and caching

#### Sprint 13: OpenAPI & Documentation
- ✅ OpenAPI 3.0 specification generation
- ✅ Markdown API documentation
- ✅ Swagger UI integration ready

## 🚀 Running the Example

```bash
# Run the comprehensive blog platform example
cargo run --example blog_platform
```

This will:
1. Parse the blog platform schema
2. Generate type-safe Rust database code
3. Generate REST API endpoints
4. Generate TypeScript SDK
5. Generate OpenAPI documentation
6. Write all files to `generated/blog_platform/`

## 📂 Generated Output Structure

After running the example, you'll find:

```
generated/blog_platform/
├── database/              # Rust database implementation
│   ├── database.rs        # Main Database struct
│   ├── user.rs            # User model and storage
│   ├── category.rs        # Category model and storage
│   ├── tag.rs             # Tag model and storage
│   ├── post.rs            # Post model and storage
│   ├── comment.rs         # Comment model and storage
│   └── junctions.rs       # Many-to-many junction tables
├── api/                   # REST API implementation
│   ├── router.rs          # Axum router setup
│   ├── user_handlers.rs   # User endpoints
│   ├── post_handlers.rs   # Post endpoints
│   └── ...                # Other model handlers
├── typescript/            # TypeScript SDK
│   ├── types.ts           # Model type definitions
│   ├── client.ts          # API client
│   └── index.ts           # Main export
└── docs/                  # Documentation
    ├── openapi.json       # OpenAPI 3.0 spec
    └── API.md             # Markdown documentation
```

## 💡 Example Usage

Here's how you would use the generated code:

### Creating Data

```rust
use generated::blog_platform::Database;

let mut db = Database::new();

// Create a user with profile settings
let profile = ProfileSettings {
    email_notifications: true,
    newsletter: true,
    theme: "dark".to_string(),
    language: "en".to_string(),
    posts_per_page: 20,
};

let user = db.user.insert(
    "alice@example.com".to_string(),
    "alice".to_string(),
    "Alice Smith".to_string(),
    Some("Tech blogger".to_string()),
    None,
    Utc::now(),
    Utc::now(),
    true,
    "user".to_string(),
    0,
    0,
    profile,
)?;
```

### Querying Data

```rust
// Find by indexed field (O(1) hash lookup)
let users = db.user.find_by_email("alice@example.com".to_string());

// Range query on indexed field (O(log n))
let popular_posts = db.post.find_by_view_count_gt(1000);

// Composite index query
let published_posts = db.post.find_by_status_and_created_at(
    "published".to_string(),
    start_date..end_date,
);
```

### Relations

```rust
// One-to-many: Get all posts by a user
let alice_posts = db.post.find_by_author(user.id);

// Many-to-many: Add tags to post
db.post_tags.add_relation(post.id, tag.id)?;

// Many-to-many: Get all tags for a post
let post_tags = db.post_tags.get_post_tags(post.id);

// Self-referential: Nested comments
let reply = db.comment.insert(
    "Great point!".to_string(),
    user.id,
    post.id,
    Some(parent_comment.id), // Parent comment
    Utc::now(),
    Utc::now(),
    0,
    false,
    false,
    false,
)?;
```

### REST API

```rust
use sinkdb_http_server::Server;
use generated::blog_platform::api::create_router;

#[tokio::main]
async fn main() {
    let db = Database::new();
    let app = create_router(db);

    Server::new()
        .port(3000)
        .serve(app)
        .await
        .unwrap();
}

// API available at:
// GET    http://localhost:3000/api/users
// POST   http://localhost:3000/api/posts
// etc...
```

### TypeScript Client

```typescript
import { BlogPlatformClient, User, Post } from './generated/typescript';

const client = new BlogPlatformClient('http://localhost:3000');

// Type-safe API calls
const users = await client.users.list();
const user = await client.users.get(userId);
const post = await client.posts.create({
  title: "My Post",
  content: "...",
  // TypeScript ensures all required fields are present
});
```

## 🎯 Learning Path

1. **Read the schema** in `examples/blog_platform.rs` - It's heavily commented
2. **Run the example** to generate code
3. **Explore generated files** in `generated/blog_platform/`
4. **Check the OpenAPI spec** - Import into Swagger UI
5. **Modify the schema** and regenerate to see changes
6. **Build your own** application following this pattern

## 🔍 Key Concepts Demonstrated

### Type Safety
Every field, relation, and constraint is checked at compile time. Invalid schemas won't compile.

### Performance
- **O(1)** lookups for indexed fields (hash maps)
- **O(log n)** range queries (B-trees)
- **Zero-copy** access for fixed-size types
- **Columnar storage** for efficient memory usage

### Ergonomics
- **Single schema** defines everything
- **Auto-generated** code, no boilerplate
- **Type inference** throughout
- **Helpful errors** at validation time

### Full-Stack
From one schema file, you get:
- Database implementation
- REST API
- TypeScript SDK
- API documentation
- All perfectly synchronized

## 📖 Next Steps

- Check the main [README.md](../README.md) for project overview
- Review [SPRINT_PLAN.md](../SPRINT_PLAN.md) for feature roadmap
- See [archive/sprint-summaries/](../archive/sprint-summaries/) for implementation details
- Start building your own schemas!

## 🤝 Contributing

This example serves as both documentation and integration test. When adding new features:

1. Update the schema to demonstrate the feature
2. Add comments explaining the feature
3. Update this README with the new capability
4. Ensure the example still runs successfully

## ❓ Common Questions

**Q: Can I use this as a template for my project?**
A: Absolutely! Copy the schema, modify it for your needs, and regenerate.

**Q: What if I only need some features?**
A: The schema is modular. Use only the features you need (e.g., skip many-to-many relations).

**Q: How do I add custom logic?**
A: Generated code is meant to be extended. Add your business logic in separate files that import the generated types.

**Q: Can I modify the generated code?**
A: Not recommended. Regeneration will overwrite changes. Instead, wrap generated code with your custom logic.

## 📝 License

Same as SinkDB project (TBD)
