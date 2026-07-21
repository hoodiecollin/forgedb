//! ForgeDB Parser
//!
//! Parser and lexer for ForgeDB's schema definition language.
//!
//! # Overview
//!
//! This crate provides parsing capabilities for ForgeDB's schema language, converting
//! schema files (.forge) into an Abstract Syntax Tree (AST) that can be used for code
//! generation, validation, and other tooling.
//!
//! # Architecture
//!
//! The parser is implemented as a traditional two-stage compiler frontend:
//!
//! 1. **Lexer** - Tokenizes input source code into a stream of tokens
//! 2. **Parser** - Converts token stream into an Abstract Syntax Tree (AST)
//!
//! ## Supported Constructs
//!
//! - **Models** - Database table definitions with fields and constraints
//! - **Structs** - Composite data types
//! - **Fields** - Typed fields with constraints
//! - **Constraints** - Directive validation rules (@email, @min, @pattern, etc.)
//! - **Relations** - Model relationships (`[Model]`, `*Model`, `?Model`)
//! - **Components** - Reusable field groups with protocols
//!
//! # Examples
//!
//! ## Parsing a Schema File
//!
//! ```rust
//! use forgedb_parser::Parser;
//!
//! let source = r#"
//!     User {
//!         id: +uuid
//!         email: &string @email
//!         age: i32 @min(18)
//!     }
//! "#;
//!
//! // Parser::new returns Result<Parser, String>
//! let mut parser = Parser::new(source).expect("parse error");
//! match parser.parse() {
//!     Ok(schema) => {
//!         println!("Parsed {} models", schema.models.len());
//!         for model in &schema.models {
//!             println!("Model: {}", model.name);
//!         }
//!     }
//!     Err(error) => {
//!         eprintln!("Parse error: {}", error);
//!     }
//! }
//! ```
//!
//! ## Working with the AST
//!
//! ```rust
//! use forgedb_parser::{Parser, Model, FieldType};
//!
//! let source = r#"
//!     Post {
//!         id: +uuid
//!         title: string
//!         content: string
//!     }
//! "#;
//!
//! let mut parser = Parser::new(source).expect("parse error");
//! let schema = parser.parse().unwrap();
//!
//! for model in &schema.models {
//!     println!("Model: {}", model.name);
//!     for field in &model.fields {
//!         println!("  Field: {} : {:?}", field.name, field.field_type);
//!     }
//! }
//! ```
//!
//! ## Tokenization
//!
//! ```rust
//! use forgedb_parser::{Lexer, Token};
//!
//! let source = "User { id: +uuid }";
//! let mut lexer = Lexer::new(source);
//!
//! // Lexer::tokenize() collects all tokens into a Vec
//! let tokens: Vec<Token> = lexer.tokenize().expect("lex error");
//! println!("Tokenized into {} tokens", tokens.len());
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`Parser`] - Main parser interface
//! - [`Lexer`] - Tokenizer for schema source code
//! - [`Schema`] - Root AST node containing all definitions
//!
//! ## AST Nodes
//!
//! - [`Model`] - Database table definition
//! - [`Struct`] - Composite data type
//! - [`Field`] - Field definition with type and constraints
//! - [`FieldType`] - Field data type (i32, string, uuid, etc.)
//! - [`Constraint`] - Directive constraint (@email, @min, @pattern, etc.)
//! - [`RelationType`] - Relationship type (OneToMany, RequiredReference, OptionalReference, ManyToMany)
//!
//! ## Component System
//!
//! - [`ComponentProtocol`] - Interface definition for components
//! - [`ComponentReference`] - Reference to a component in a model
//!
//! # Schema Language Features
//!
//! ## Field Types
//!
//! - **Integers**: `u32`, `u64`, `i32`, `i64`
//! - **Floating Point**: `f64`
//! - **Decimal**: `decimal` (exact fixed-point)
//! - **Boolean**: `bool`
//! - **String**: `string`, `char(N)` (fixed-size)
//! - **JSON**: `json`
//! - **UUID**: `uuid`
//! - **Timestamp**: `timestamp`
//! - **Enum**: a bare reference to a top-level `enum Name { ... }`
//!
//! ## Field Modifiers (prefix, before the type)
//!
//! - `+field` - Auto-generate (`u32`/`u64`/`uuid`/`timestamp` only)
//! - `&field` - Unique
//! - `^field` - Index
//! - `?field` / `field?` - Optional (nullable)
//!
//! ## Constraints (directives)
//!
//! - `@email` - Email validation
//! - `@min(value)` / `@max(value)` - Numeric bounds
//! - `@length(n)` / `@length(min, max)` - String length
//! - `@pattern("…")` / `@regex("…")` - Regex validation (enforced)
//! - `@index(a, b)` - Composite index (model level)
//! - `@on_delete(restrict|cascade|set_null)` - FK on-delete policy (enforced)
//!
//! ## Relations
//!
//! - `[Model]` - One-to-many
//! - `*Model` - Required reference (belongs-to)
//! - `?Model` - Optional reference
//! - `[Model]` on both sides - Many-to-many
//!
//! # Error Handling
//!
//! The parser returns detailed error messages with line and column information:
//!
//! ```text
//! Parse error at line 5, column 12: Expected '}', found 'field'
//! ```
//!
//! # Related Crates
//!
//! - [`forgedb-validation`](../forgedb_validation) - Schema validation
//! - [`forgedb-types`](../forgedb_types) - Runtime type definitions
//! - [`forgedb`](../../) - Main ForgeDB CLI using this parser
//!
//! # See Also
//!
//! - [ForgeDB Schema Language Guide](../../docs/schema-language.md)
//! - [SPRINT1_SUMMARY.md](../../archive/sprint-summaries/SPRINT1_SUMMARY.md) - Parser implementation

pub mod ast;
pub mod lexer;
pub mod parser;

// Re-export main types for convenience
pub use ast::{
    ComponentProtocol, ComponentReference, Constraint, ConstraintParam, EnumDef, Field, FieldType,
    Model, Projection, RelationType, Schema, Struct,
};
pub use lexer::{Lexer, Token};
pub use parser::Parser;
