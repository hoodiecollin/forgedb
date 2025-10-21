//! ForgeDB Parser
//!
//! Parser and lexer for ForgeDB's schema definition language.
//!
//! # Overview
//!
//! This crate provides parsing capabilities for ForgeDB's schema language, converting
//! schema files (.fdb) into an Abstract Syntax Tree (AST) that can be used for code
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
//! - **Constraints** - Validation rules (@unique, @email, @min, etc.)
//! - **Relations** - Model relationships (hasMany, belongsTo)
//! - **Components** - Reusable field groups with protocols
//!
//! # Examples
//!
//! ## Parsing a Schema File
//!
//! ```rust,no_run
//! use forgedb_parser::Parser;
//!
//! let source = r#"
//!     model User {
//!         +id: uuid
//!         email: string @unique @email
//!         age: i32 @min(18)
//!         createdAt: timestamp
//!     }
//! "#;
//!
//! let mut parser = Parser::new(source);
//! match parser.parse() {
//!     Ok(schema) => {
//!         println!("Parsed {} models", schema.models.len());
//!         for model in &schema.models {
//!             println!("Model: {}", model.name);
//!         }
//!     }
//!     Err(errors) => {
//!         for error in errors {
//!             eprintln!("Parse error: {}", error);
//!         }
//!     }
//! }
//! ```
//!
//! ## Working with the AST
//!
//! ```rust,no_run
//! use forgedb_parser::{Parser, Model, FieldType};
//!
//! let source = r#"
//!     model Post {
//!         +id: uuid
//!         title: string
//!         content: string
//!     }
//! "#;
//!
//! let mut parser = Parser::new(source);
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
//! let source = "model User { +id: uuid }";
//! let mut lexer = Lexer::new(source);
//!
//! let tokens: Vec<Token> = lexer.collect();
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
//! - [`Constraint`] - Validation constraint (@unique, @email, etc.)
//! - [`RelationType`] - Relationship type (hasMany, belongsTo)
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
//! - **Integers**: `i32`, `i64`
//! - **Floating Point**: `f64`
//! - **Boolean**: `bool`
//! - **String**: `string`
//! - **UUID**: `uuid`
//! - **Timestamp**: `timestamp`
//!
//! ## Field Modifiers
//!
//! - `+field` - Primary key
//! - `field?` - Optional (nullable)
//!
//! ## Constraints
//!
//! - `@unique` - Unique constraint
//! - `@email` - Email validation
//! - `@min(value)` - Minimum value
//! - `@max(value)` - Maximum value
//! - `@length(min, max)` - String length constraint
//!
//! ## Relations
//!
//! - `hasMany(Model)` - One-to-many relationship
//! - `belongsTo(Model)` - Many-to-one relationship
//! - `hasOne(Model)` - One-to-one relationship
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
    ComponentProtocol, ComponentReference, Constraint, ConstraintParam, Field, FieldType, Model,
    RelationType, Schema, Struct,
};
pub use lexer::{Lexer, Token};
pub use parser::Parser;
