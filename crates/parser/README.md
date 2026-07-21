# ForgeDB Parser

Core schema language parser for ForgeDB.

## Overview

The `forgedb-parser` crate is the schema language parser for ForgeDB. It parses `.forge` schema files into an Abstract Syntax Tree (AST) that can be used for code generation, validation, and analysis. The parser implements a complete lexer and recursive descent parser for the ForgeDB schema language.

## Features

- **Complete schema parsing** - Models, fields, types, and directives
- **Comprehensive type system** - Primitives, relations, structs, and arrays
- **Rich constraints** - Validation directives like `@min`, `@max`, `@email`
- **Relation support** - One-to-many, many-to-many, required and optional references
- **Component integration** - TSX/JSX component references and API routes
- **Composite indexes** - Multi-field index definitions
- **Detailed error messages** - Parse errors with line and column information
- **Validation** - Model and field name validation

## Schema Language

### Syntax Overview

ForgeDB schemas define models with fields. Each model represents a database table or collection.

```forge
User {
  id: +uuid
  email: &string
  name: string
}
```

### Model Definitions

Models are defined with a name (PascalCase) followed by a block containing fields:

```forge
ModelName {
  field1: type
  field2: type
}
```

### Field Types and Modifiers

#### Primitive Types

| Type | Description | Example |
|------|-------------|---------|
| `u32` | 32-bit unsigned integer | `count: u32` |
| `u64` | 64-bit unsigned integer | `id: u64` |
| `i32` | 32-bit signed integer | `temperature: i32` |
| `i64` | 64-bit signed integer | `balance: i64` |
| `f64` | 64-bit floating point | `price: f64` |
| `bool` | Boolean value | `active: bool` |
| `string` | UTF-8 text | `name: string` |
| `uuid` | UUID identifier | `id: uuid` |
| `timestamp` | Unix timestamp | `created_at: timestamp` |

#### Fixed-Size Types

```forge
Person {
  code: char(10)              // Fixed-size character array
  matrix: [i32; 9]            // Fixed-size array
  point: Point                // Struct reference
  optional_data: ?Point       // Optional struct
}
```

#### Field Modifiers

Field modifiers are symbols that appear before the type:

- `+` - Auto-generate: Automatically generate value on creation
- `&` - Unique: Value must be unique across all records
- `^` - Indexed: Create an index for fast lookups

```forge
User {
  id: +uuid           // Auto-generated UUID
  email: &string      // Unique email
  username: ^&string  // Indexed and unique
}
```

**Auto-generate support:**
- `u32`, `u64` - Auto-incrementing integers
- `uuid` - Random UUID generation
- `timestamp` - Current timestamp

### Directives

Directives are validation and configuration rules applied with the `@` symbol:

#### Validation Directives

```forge
User {
  email: string @email
  age: u32 @min(0) @max(150)
  password: string @length(8, 128)
  website: string @url
  name: string @length(1, 100)
}
```

Common **enforced** directives (a violation rejects the write with HTTP 422):
- `@email` - Email format validation (`string`)
- `@url` - URL format validation (`string`)
- `@min(n)` - Minimum value — **numeric fields only**
- `@max(n)` - Maximum value — **numeric fields only** (not a string-length check; use `@length`)
- `@length(min, max)` - String length range (`string`)
- `@pattern("…")` / `@regex("…")` - Regex match (`string`)

Uniqueness is the `&` modifier, not a directive. (`@min`/`@max` on a string,
or any unrecognized directive such as `@private`/`@unique`, parses but is a
no-op — it enforces nothing.)

#### Computed Fields

```forge
User {
  first_name: string
  last_name: string
  full_name: string @computed
}
```

#### Fulltext Search

```forge
Post {
  title: string @fulltext
  content: string @fulltext
}
```

#### Materialized Views

```forge
User {
  post_count: u32 @materialized
}
```

#### Soft Delete

```forge
User {
  id: +uuid
  name: string

  @soft_delete
}
```

### Relations

ForgeDB supports several types of relations between models:

#### One-to-Many Relations

Use `[ModelName]` to define a collection:

```forge
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
  author: *User    // Required reference back to User
}
```

#### Required References

Use `*ModelName` for a required foreign key:

```forge
Post {
  id: +uuid
  author: *User    // Must reference a User
}
```

#### Optional References

Use `?ModelName` for an optional foreign key:

```forge
Post {
  id: +uuid
  reviewer: ?User  // May or may not reference a User
}
```

#### Many-to-Many Relations

Define bidirectional collections without foreign keys:

```forge
Post {
  id: +uuid
  tags: [Tag]
}

Tag {
  id: +uuid
  posts: [Post]
}
```

### Composite Indexes

Define multi-field indexes at the model level:

```forge
User {
  id: +uuid
  first_name: string
  last_name: string
  city: string
  state: string

  @index(first_name, last_name)
  @index(city, state)
}
```

### Struct Definitions

Structs are fixed-size composite types that can only contain fixed-size fields:

```forge
struct Point {
  x: f64
  y: f64
}

struct Rectangle {
  top_left: Point
  bottom_right: Point
}

Model {
  location: Point
  bounds: ?Rectangle
}
```

### Component References

Reference UI components and API routes:

```forge
User {
  id: +uuid
  name: string
  posts: [Post]

  // TSX component with all relations
  profileCard: tsx://components/user/ProfileCard @relations(*)

  // JSX component with specific relations
  avatarView: jsx://components/user/AvatarView @relations(posts)

  // API route
  updateProfile: api://routes/user/update
}
```

Component protocols:
- `tsx://path` - TypeScript + JSX component
- `jsx://path` - JavaScript + JSX component
- `api://path` - API route handler

Relation inclusion:
- `@relations(*)` - Include all relations
- `@relations(field1, field2)` - Include specific relations
- No directive - No relations included

## Usage Examples

### Parsing a Schema File

```rust
use forgedb_parser::Parser;

// Parse a schema string
let input = r#"
User {
  id: +uuid
  email: &string
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  author: *User
}
"#;

let mut parser = Parser::new(input).unwrap();
let schema = parser.parse().unwrap();

// Access models
assert_eq!(schema.models.len(), 2);
assert_eq!(schema.models[0].name, "User");
assert_eq!(schema.models[1].name, "Post");
```

### Working with the AST

```rust
use forgedb_parser::Parser;

let input = r#"
User {
  id: +uuid
  email: &string @email
  age: u32 @min(0) @max(150)
}
"#;

let mut parser = Parser::new(input).unwrap();
let schema = parser.parse().unwrap();

// Find a model
let user = schema.find_model("User").unwrap();

// Iterate fields
for field in &user.fields {
    println!("Field: {} ({})", field.name, field.field_type.to_rust_type());
    
    // Check modifiers
    if field.auto_generate {
        println!("  Auto-generated");
    }
    if field.unique {
        println!("  Unique constraint");
    }
    if field.indexed {
        println!("  Indexed");
    }
    
    // Check constraints
    for constraint in &field.constraints {
        println!("  @{}", constraint.name);
        for param in &constraint.params {
            match param {
                ConstraintParam::Number(n) => println!("    - {}", n),
                ConstraintParam::String(s) => println!("    - {}", s),
            }
        }
    }
}
```

### Validating Relations

```rust
use forgedb_parser::Parser;

let input = r#"
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
  author: *User
}
"#;

let mut parser = Parser::new(input).unwrap();
let schema = parser.parse().unwrap();

// Validate all relations exist
assert!(schema.validate_relations().is_ok());

// Detect relation pairs
let relations = schema.detect_relations();
assert_eq!(relations.len(), 1);
assert_eq!(relations[0].parent_model, "User");
assert_eq!(relations[0].parent_field, "posts");
assert_eq!(relations[0].child_model, "Post");
assert_eq!(relations[0].child_field, "author");
```

### Detecting Many-to-Many Relations

```rust
use forgedb_parser::Parser;

let input = r#"
Post {
  id: +uuid
  tags: [Tag]
}

Tag {
  id: +uuid
  posts: [Post]
}
"#;

let mut parser = Parser::new(input).unwrap();
let schema = parser.parse().unwrap();

// Detect M:N relations
let m2m = schema.detect_many_to_many_relations();
assert_eq!(m2m.len(), 1);
assert_eq!(m2m[0].model1, "Post");
assert_eq!(m2m[0].field1, "tags");
assert_eq!(m2m[0].model2, "Tag");
assert_eq!(m2m[0].field2, "posts");
```

### Error Handling

```rust
use forgedb_parser::Parser;

// Invalid: duplicate field names
let input = r#"
User {
  id: +uuid
  email: string
  email: string
}
"#;

let mut parser = Parser::new(input).unwrap();
let result = parser.parse();

assert!(result.is_err());
let error = result.unwrap_err();
assert!(error.contains("Duplicate field name 'email'"));
```

### Disabling Validation

```rust
use forgedb_parser::Parser;

// Create parser without validation
let input = "User { InvalidFieldName: string }";
let mut parser = Parser::new_with_validation(input, false).unwrap();
let schema = parser.parse().unwrap();
```

## AST Structure

### Core Types

#### `Schema`

The root of the AST, containing all models and structs:

```rust
pub struct Schema {
    pub structs: Vec<Struct>,
    pub models: Vec<Model>,
}
```

Methods:
- `find_model(&self, name: &str) -> Option<&Model>` - Find a model by name
- `find_struct(&self, name: &str) -> Option<&Struct>` - Find a struct by name
- `validate_relations(&self) -> Result<(), String>` - Validate all relations
- `validate_struct_references(&self) -> Result<(), String>` - Validate struct references
- `detect_relations(&self) -> Vec<RelationPair>` - Find 1:N relationships
- `detect_many_to_many_relations(&self) -> Vec<ManyToManyRelation>` - Find M:N relationships

#### `Model`

Represents a database model:

```rust
pub struct Model {
    pub name: String,
    pub fields: Vec<Field>,
    pub composite_indexes: Vec<CompositeIndex>,
    pub soft_delete: bool,
}
```

#### `Struct`

Represents a fixed-size composite type:

```rust
pub struct Struct {
    pub name: String,
    pub fields: Vec<Field>,
}
```

Methods:
- `calculate_size(struct_def: &Struct, schema: &Schema) -> usize` - Calculate total size with padding
- `calculate_alignment(struct_def: &Struct, schema: &Schema) -> usize` - Calculate alignment requirement

#### `Field`

Represents a model or struct field:

```rust
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub auto_generate: bool,
    pub unique: bool,
    pub indexed: bool,
    pub constraints: Vec<Constraint>,
    pub index_type: IndexType,
    pub is_computed: bool,
    pub fulltext_indexed: bool,
    pub is_materialized: bool,
}
```

Methods:
- `has_constraint(&self, name: &str) -> bool` - Check for a constraint
- `get_constraint(&self, name: &str) -> Option<&Constraint>` - Get a constraint
- `is_nullable(&self) -> bool` - Check if field is nullable

### Type System

#### `FieldType`

Enum representing all possible field types:

```rust
pub enum FieldType {
    // Primitives
    U32, U64, I32, I64, F64, Bool, String, Uuid, Timestamp,
    
    // Fixed-size types
    Char(usize),
    FixedArray(Box<FieldType>, usize),
    StructType(String),
    OptionalStructType(String),
    
    // Relations
    Relation(RelationType),
    
    // Components
    Component(ComponentReference),
}
```

Methods:
- `to_rust_type(&self) -> String` - Convert to Rust type string
- `is_auto_incrementable(&self) -> bool` - Check if can auto-increment
- `is_auto_generatable(&self) -> bool` - Check if can auto-generate
- `is_relation(&self) -> bool` - Check if relation type
- `is_fixed_size(&self) -> bool` - Check if fixed-size
- `struct_name(&self) -> Option<&str>` - Get struct name if struct type
- `size_in_bytes(&self, schema: &Schema) -> usize` - Get size for fixed types
- `alignment(&self, schema: &Schema) -> usize` - Get alignment requirement
- `supports_range_queries(&self) -> bool` - Check if ordered type
- `default_index_type(&self) -> IndexType` - Get default index type

#### `RelationType`

Enum for relation types:

```rust
pub enum RelationType {
    OneToMany(String),         // [Model]
    RequiredReference(String), // *Model
    OptionalReference(String), // ?Model
    ManyToMany(String),        // Detected bidirectional
}
```

Methods:
- `target_model(&self) -> &str` - Get target model name
- `is_one_to_many(&self) -> bool` - Check if one-to-many
- `is_reference(&self) -> bool` - Check if foreign key reference
- `is_many_to_many(&self) -> bool` - Check if many-to-many

#### `IndexType`

Index type for fields:

```rust
pub enum IndexType {
    Hash,  // For exact matches
    BTree, // For range queries
}
```

### Constraints

#### `Constraint`

Represents a validation directive:

```rust
pub struct Constraint {
    pub name: String,
    pub params: Vec<ConstraintParam>,
}
```

Methods:
- `new(name: String) -> Self` - Create constraint without parameters
- `with_param(self, param: ConstraintParam) -> Self` - Add a parameter

#### `ConstraintParam`

Constraint parameter value:

```rust
pub enum ConstraintParam {
    Number(i64),
    String(String),
}
```

### Component Integration

#### `ComponentReference`

Reference to a UI component or API route:

```rust
pub struct ComponentReference {
    pub protocol: ComponentProtocol,
    pub path: String,
    pub relations: RelationInclusion,
}
```

#### `ComponentProtocol`

```rust
pub enum ComponentProtocol {
    Tsx,  // tsx://
    Jsx,  // jsx://
    Api,  // api://
}
```

#### `RelationInclusion`

```rust
pub enum RelationInclusion {
    None,
    All,
    Specific(Vec<String>),
}
```

### Composite Indexes

#### `CompositeIndex`

Multi-field index definition:

```rust
pub struct CompositeIndex {
    pub fields: Vec<String>,
}
```

### Relation Detection

#### `RelationPair`

One-to-many relationship:

```rust
pub struct RelationPair {
    pub parent_model: String,
    pub parent_field: String,
    pub child_model: String,
    pub child_field: String,
    pub is_required: bool,
}
```

#### `ManyToManyRelation`

Many-to-many relationship:

```rust
pub struct ManyToManyRelation {
    pub model1: String,
    pub field1: String,
    pub model2: String,
    pub field2: String,
}
```

### Traversing the AST

```rust
use forgedb_parser::Parser;

let mut parser = Parser::new(input).unwrap();
let schema = parser.parse().unwrap();

// Iterate all models
for model in &schema.models {
    println!("Model: {}", model.name);
    
    // Iterate all fields
    for field in &model.fields {
        println!("  Field: {} : {:?}", field.name, field.field_type);
        
        // Check for relations
        if let FieldType::Relation(rel_type) = &field.field_type {
            println!("    Relation to: {}", rel_type.target_model());
        }
    }
    
    // Check composite indexes
    for index in &model.composite_indexes {
        println!("  Composite index: {:?}", index.fields);
    }
}

// Iterate all structs
for struct_def in &schema.structs {
    println!("Struct: {}", struct_def.name);
    for field in &struct_def.fields {
        println!("  Field: {} : {:?}", field.name, field.field_type);
    }
}
```

## API Reference

### `Parser` Type

The main parser type for parsing schema strings:

```rust
pub struct Parser { /* ... */ }
```

#### Constructor Methods

- `Parser::new(input: &str) -> Result<Self, String>`
  - Create a new parser with validation enabled
  - Returns error if lexing fails

- `Parser::new_with_validation(input: &str, use_validation: bool) -> Result<Self, String>`
  - Create a parser with optional validation
  - Set `use_validation` to `false` to disable name validation

#### Parsing Methods

- `parser.parse(&mut self) -> Result<Schema, String>`
  - Parse the input and return the AST
  - Returns error if parsing fails with detailed error message

### `Lexer` Type

The lexer tokenizes schema input:

```rust
pub struct Lexer { /* ... */ }
```

#### Constructor

- `Lexer::new(input: &str) -> Self`
  - Create a new lexer for the input string

#### Methods

- `lexer.next_token(&mut self) -> Result<Token, String>`
  - Get the next token
  - Returns error for unexpected characters

- `lexer.next_token_with_pos(&mut self) -> Result<TokenWithPos, String>`
  - Get next token with position information

- `lexer.tokenize(&mut self) -> Result<Vec<Token>, String>`
  - Tokenize entire input
  - Returns all tokens including EOF

- `lexer.tokenize_with_pos(&mut self) -> Result<Vec<TokenWithPos>, String>`
  - Tokenize with position information for error reporting

### `Token` Type

Token types recognized by the lexer:

```rust
pub enum Token {
    // Identifiers and literals
    Ident(String),
    Number(i64),
    
    // Types
    TypeU32, TypeU64, TypeI32, TypeI64, TypeF64,
    TypeBool, TypeString, TypeUuid, TypeTimestamp, TypeChar,
    
    // Keywords
    KwStruct,
    
    // Symbols
    Plus, Ampersand, Caret, Colon,
    LBrace, RBrace, LBracket, RBracket,
    Asterisk, Question, At,
    LParen, RParen, Comma, Semicolon, Slash,
    
    // Control
    Newline, Eof,
}
```

## Error Handling

### Parse Errors

The parser provides detailed error messages with context:

```rust
use forgedb_parser::Parser;

let input = r#"
User {
  id: +string  // Invalid: string cannot be auto-generated
}
"#;

let mut parser = Parser::new(input).unwrap();
match parser.parse() {
    Ok(schema) => println!("Parsed successfully"),
    Err(error) => {
        eprintln!("Parse error: {}", error);
        // Error: "Auto-generate symbol '+' cannot be used with type 'string'"
    }
}
```

### Error Categories

#### Lexer Errors

- Unexpected character: `"Unexpected character 'x' at line 5, column 10"`
- Invalid number format: Fails to parse numeric literal

#### Parser Errors

- Missing token: `"Expected '{', found 'identifier'"`
- Empty model: `"Model 'User' must have at least one field"`
- Empty schema: `"Schema must contain at least one model"`
- Invalid field type: `"Unknown type 'int32'"`

#### Validation Errors

- Duplicate field: `"Duplicate field name 'email' in model 'User'"`
- Duplicate model: `"Duplicate model name 'User'"`
- Invalid field name: `"Field name 'UserName' must be snake_case. Suggested: 'user_name'"`
- Invalid model name: `"Model name 'user_model' must be PascalCase. Suggested: 'UserModel'"`
- Auto-generate misuse: `"Auto-generate symbol '+' cannot be used with type 'string'"`
- Undefined reference: `"Model 'Post' field 'author' references undefined model 'User'"`
- Variable-length in struct: `"Struct 'Data' field 'text' contains variable-length type"`
- Composite index error: `"Field 'email' in @index directive not found in model 'User'"`

### Error Recovery

The parser does not currently support error recovery - it stops at the first error encountered. This ensures clean error messages without cascading errors.

### Position Information

Errors include line and column information when available:

```rust
use forgedb_parser::lexer::Lexer;

let mut lexer = Lexer::new("User { id: !");
match lexer.tokenize() {
    Ok(_) => println!("Success"),
    Err(error) => {
        println!("{}", error);
        // Output: "Unexpected character '!' at line 1, column 12"
    }
}
```

## Testing

### Running Parser Tests

```bash
# Run all parser tests
cargo test -p forgedb-parser

# Run with output
cargo test -p forgedb-parser -- --nocapture

# Run specific test
cargo test -p forgedb-parser test_parse_simple_model
```

### Integration Tests

The parser has extensive integration tests in `/tests/parser_tests.rs` covering:

- Basic model parsing
- All primitive types
- Field modifiers (auto-generate, unique, indexed)
- Relations (one-to-many, required reference, optional reference)
- Constraints and directives
- Composite indexes
- Struct definitions
- Component references
- Error cases and validation
- Name validation (PascalCase for models, snake_case for fields)

### Test Coverage

The parser tests cover:
- ✅ Simple and complex model definitions
- ✅ All primitive and fixed-size types
- ✅ Field modifiers and combinations
- ✅ All relation types
- ✅ Constraint parsing with parameters
- ✅ Composite index definitions
- ✅ Struct parsing and validation
- ✅ Component integration (TSX/JSX/API)
- ✅ Duplicate detection (models, fields)
- ✅ Name validation (PascalCase, snake_case)
- ✅ Invalid auto-generate usage
- ✅ Undefined reference detection
- ✅ Error message quality

### Example Test

```rust
use forgedb_parser::Parser;
use forgedb_parser::ast::*;

#[test]
fn test_parse_with_relations() {
    let input = r#"
User {
  id: +uuid
  posts: [Post]
}

Post {
  id: +uuid
  author: *User
}
"#;
    
    let mut parser = Parser::new(input).unwrap();
    let schema = parser.parse().unwrap();
    
    assert_eq!(schema.models.len(), 2);
    
    // Check one-to-many
    let user = &schema.models[0];
    let posts_field = &user.fields[1];
    match &posts_field.field_type {
        FieldType::Relation(RelationType::OneToMany(target)) => {
            assert_eq!(target, "Post");
        }
        _ => panic!("Expected OneToMany relation"),
    }
    
    // Check required reference
    let post = &schema.models[1];
    let author_field = &post.fields[1];
    match &author_field.field_type {
        FieldType::Relation(RelationType::RequiredReference(target)) => {
            assert_eq!(target, "User");
        }
        _ => panic!("Expected RequiredReference"),
    }
    
    // Validate relations
    assert!(schema.validate_relations().is_ok());
    
    // Detect relation pairs
    let relations = schema.detect_relations();
    assert_eq!(relations.len(), 1);
}
```

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
forgedb-parser = "0.1"
```

Or with path dependency for local development:

```toml
[dependencies]
forgedb-parser = { path = "../parser" }
```

## Dependencies

- `forgedb-validation` - Validation utilities for names and constraints

## Contributing

This crate is part of the ForgeDB project. For contribution guidelines, see the main repository.

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT License ([LICENSE-MIT](../../LICENSE-MIT))

at your option.
