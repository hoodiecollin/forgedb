# Sprint 4: Relations (One-to-Many)

## Overview

Sprint 4 implements basic relationship support in ForgeDB, specifically one-to-many relationships. This allows models to reference each other through foreign keys with automatic indexing and type-safe code generation.

## Features Implemented

### ✅ Completed

1. **Relation Syntax Parsing**
   - `[Post]` - One-to-many: A user has many posts
   - `*User` - Required reference: A post must have an author
   - `?User` - Optional reference: A post may have a reviewer

2. **Foreign Key Generation**
   - Reference fields (`*User`, `?User`) generate FK columns (e.g., `author_id`)
   - OneToMany fields (`[Post]`) are virtual and don't consume storage

3. **Automatic Indexing**
   - All FK fields are automatically indexed for fast lookups
   - Generated `find_by_X_id()` methods for all FK fields

4. **Schema Validation**
   - Validates that referenced models exist
   - Detects and analyzes relation pairs

5. **Comprehensive Testing**
   - 85 tests passing (including Sprint 4 relation tests)
   - Tests for all relation types and code generation

### ✅ Sprint 4.1 - Additional Features

6. **Database Struct**
   - Central struct holding all model storages
   - Enables cross-model operations

7. **Foreign Key Validation**
   - Runtime validation that FKs reference existing records
   - Generated `insert_X()` methods with FK checks
   - Error on insert if referenced record doesn't exist

8. **Relation Traversal Methods**
   - `user_posts(user_id)` - Get all posts for a user
   - Automatic generation based on relation pairs
   - Uses underlying `find_by_X_id()` methods

9. **Reverse Lookup Methods**
   - `post_author(post_id)` - Get the author of a post
   - Type-safe navigation from child to parent
   - Returns `Option<Parent>` for safe access

## Schema Syntax

### One-to-Many Relationship

```
User {
  id: +uuid
  email: ^&string
  posts: [Post]      // One-to-many: virtual field
}

Post {
  id: +uuid
  title: string
  author: *User      // Required reference: generates author_id FK
}
```

### Required vs Optional References

```
Post {
  id: +uuid
  author: *User       // Required: author_id: uuid::Uuid
  reviewer: ?User     // Optional: reviewer_id: Option<uuid::Uuid>
}
```

## Generated Code (Sprint 4.1)

### Database Struct

```rust
pub struct Database {
    pub user: UserStorage,
    pub post: PostStorage,
}

impl Database {
    pub fn new() -> Self { ... }

    // FK-validated insert
    pub fn insert_post(&mut self, title: String, content: String, author_id: uuid::Uuid)
        -> Result<Post, String> {
        if self.user.get(author_id).is_none() {
            return Err("Foreign key validation failed: User does not exist".to_string());
        }
        self.post.insert(title, content, author_id)
    }

    // Relation traversal
    pub fn user_posts(&self, user_id: uuid::Uuid) -> Vec<Post> {
        self.post.find_by_user_id(user_id)
    }

    // Reverse lookup
    pub fn post_author(&self, post_id: uuid::Uuid) -> Option<User> {
        if let Some(child) = self.post.get(post_id) {
            return self.user.get(child.author_id);
        }
        None
    }
}
```

### Struct Generation

**User struct** (parent):
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: uuid::Uuid,
    pub email: String,
    // posts field is NOT generated (virtual)
}
```

**Post struct** (child):
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub id: uuid::Uuid,
    pub title: String,
    pub author_id: uuid::Uuid,  // FK field generated
}
```

### Storage Generation

```rust
pub struct PostStorage {
    records: Vec<Post>,
    next_id: u64,
    tombstones: Vec<bool>,
    author_id_index: std::collections::HashMap<uuid::Uuid, Vec<usize>>,  // Auto-indexed
}
```

### Method Generation

**Insert with FK:**
```rust
pub fn insert(&mut self, title: String, author_id: uuid::Uuid) -> Result<Post, String>
```

**Find by FK:**
```rust
pub fn find_by_author_id(&self, author_id: uuid::Uuid) -> Vec<Post>
```

## Example Usage

```rust
use forgedb::parser::Parser;
use forgedb::codegen::CodeGenerator;

let schema = r#"
User {
  id: +uuid
  email: ^&string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;

let mut parser = Parser::new(schema).unwrap();
let schema = parser.parse().unwrap();

// Validate relations
schema.validate_relations().unwrap();

// Detect relation pairs
let relations = schema.detect_relations();
// relations[0]: User.posts -> Post.author

// Generate code
let codegen = CodeGenerator::new();
let code = codegen.generate(&schema);
```

## Key Implementation Details

### 1. AST Extensions

**New types in `ast.rs`:**
```rust
pub enum FieldType {
    // ... existing types
    Relation(RelationType),
}

pub enum RelationType {
    OneToMany(String),          // [Post]
    RequiredReference(String),  // *User
    OptionalReference(String),  // ?User
}

pub struct RelationPair {
    pub parent_model: String,
    pub parent_field: String,
    pub child_model: String,
    pub child_field: String,
    pub is_required: bool,
}
```

### 2. Lexer Extensions

**New tokens in `lexer.rs`:**
```rust
Token::LBracket,  // [
Token::RBracket,  // ]
Token::Asterisk,  // *
Token::Question,  // ?
```

### 3. Parser Extensions

**Relation parsing in `parser.rs`:**
- `parse_type()` now handles `[ModelName]`, `*ModelName`, `?ModelName`
- `validate_relations()` checks that referenced models exist
- `detect_relations()` finds matching parent/child pairs

### 4. Codegen Extensions

**Helper methods:**
- `get_field_param_name()` - Maps `author` field to `author_id` parameter
- `get_field_param_type()` - Returns correct Rust type for FK fields

**Field handling:**
- OneToMany fields skipped in struct generation
- Reference fields generate `_id` suffix
- FK fields automatically added to indexes

## Testing

Run Sprint 4 tests:
```bash
# All tests
cargo test --lib

# Sprint 4 relation tests only
cargo test --lib test_generate_relation

# Run example
cargo run --example sprint4_relations
```

## Success Criteria

All Sprint 4 + 4.1 success criteria have been met:

- ✅ Can create posts linked to users (via FK)
- ✅ Can find posts by author_id (via generated index)
- ✅ Foreign keys are automatically indexed
- ✅ Schema validation prevents invalid references
- ✅ Can traverse user → posts (relation traversal methods)
- ✅ Can traverse post → user (reverse lookup methods)
- ✅ Foreign key validation works at runtime

## Next Steps (Future Sprints)

1. **Support Many-to-Many Relations** (Sprint 6)
   - Junction table generation
   - Bi-directional traversal methods

## Files Modified

- `src/ast.rs` - Added `RelationType` and `RelationPair`
- `src/lexer.rs` - Added relation tokens
- `src/parser.rs` - Added relation parsing and validation
- `src/codegen.rs` - Updated to handle FK field generation
- `examples/sprint4_relations.rs` - Sprint 4 example

## Statistics

### Sprint 4
- **Lines of code added**: ~400
- **Tests added**: 5 new relation-specific tests
- **Total tests passing**: 85
- **Example programs**: 1 (sprint4_relations)

### Sprint 4.1
- **Lines of code added**: ~200
- **Tests added**: 4 new Database/FK/relation tests
- **Total tests passing**: 89
- **Example programs**: 1 (sprint4_1_database)

---

**Sprint Status**: All features complete ✅✅
**Next Sprint**: Sprint 5 - CLI & Developer Experience
