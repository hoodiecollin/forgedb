//! ForgeDB Validation
//!
//! The diagnostic vocabulary and reusable predicates that ForgeDB's schema
//! validation is built from, plus HTTP-status helpers.
//!
//! # Overview
//!
//! This crate provides:
//!
//! - **Diagnostic types** - [`ValidationError`] (message + [`Position`] + suggestion),
//!   the shared currency of positioned schema diagnostics.
//! - **Naming predicates** - [`is_snake_case`] / [`is_pascal_case`] and the
//!   [`validate_field_name`] / [`validate_model_name`] convenience checks.
//! - **HTTP validation** - Validate HTTP endpoint definitions and status codes.
//!
//! The *schema walk itself* — the single `validate_schema(&Schema)` that applies
//! these to a parsed AST — lives in `forgedb-parser`, which owns the AST (see the
//! architecture note below for why it cannot live here).
//!
//! # Architecture
//!
//! The validation system is organized into several modules:
//!
//! - **Core validation** - Basic validation types and error reporting
//! - **HTTP validation** - Validates HTTP-specific constructs (endpoints, status codes)
//! - **Status code mapping** - Maps database operations to appropriate HTTP status codes
//!
//! ## Validation Pipeline
//!
//! 1. **Schema Parsing** - Parse schema into AST (handled by forgedb-parser)
//! 2. **Schema Validation** - Validate schema structure and semantics
//! 3. **Constraint Validation** - Check constraint parameters and applicability
//! 4. **HTTP Validation** - Validate HTTP endpoints and status codes (if applicable)
//!
//! > **Where the schema walk lives (epic #173 — DONE).** The single
//! > positioned schema-validation authority is
//! > [`forgedb_parser::validate_schema`](../forgedb_parser/validate/fn.validate_schema.html),
//! > **not** a function in this crate. It has to live in `forgedb-parser`: it
//! > needs the `Schema` AST, and `forgedb-parser` already depends on
//! > `forgedb-validation` (for `Position`/`ValidationError`), so a reverse
//! > dependency would be a cycle. The three former implementations (the parser's
//! > built-in checks, the CLI's inline `src/commands/validate.rs` loops, and the
//! > `validate_*` helpers here) are consolidated into that one function; the CLI
//! > and the LSP both consume it.
//! >
//! > This crate is the **diagnostic vocabulary + reusable predicates** that
//! > `validate_schema` builds on: [`ValidationError`], [`Position`],
//! > [`validate_field_name`], [`validate_model_name`], [`is_snake_case`],
//! > [`is_pascal_case`]. The `HttpValidator`/`StatusCodeMapper` surface is a
//! > separate HTTP-status concern.
//!
//! # Examples
//!
//! ## Schema Validation
//!
//! ```rust
//! use forgedb_validation::{ValidationError, Position};
//!
//! // Create a validation error with position
//! let error = ValidationError::new("Field 'email' must have @email constraint")
//!     .with_position(Position::new(5, 12))
//!     .with_suggestion("Add @email constraint: email: string @email");
//!
//! println!("Error at {}:{}: {}",
//!     error.position.unwrap().line,
//!     error.position.unwrap().column,
//!     error.message
//! );
//! ```
//!
//! ## HTTP Validation
//!
//! ```rust
//! use forgedb_validation::HttpValidator;
//!
//! // HttpValidator is a unit struct — call methods directly (no constructor)
//! let result = HttpValidator::validate_email("user@example.com");
//! assert!(result.is_ok());
//!
//! let result = HttpValidator::validate_length("name", "hi", 1, 10);
//! assert!(result.is_ok());
//! ```
//!
//! ## Status Code Mapping
//!
//! ```rust
//! use forgedb_validation::StatusCodeMapper;
//!
//! // StatusCodeMapper is a unit struct — call methods as associated functions
//! let code = StatusCodeMapper::for_validation_error("not_found");
//! assert_eq!(code, 404);
//!
//! assert!(StatusCodeMapper::is_success(200));
//! assert!(StatusCodeMapper::is_client_error(404));
//! assert!(StatusCodeMapper::is_server_error(500));
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`ValidationError`] - Validation error with position and suggestion
//! - [`Position`] - Line and column position in source code
//! - [`HttpValidator`] - HTTP-specific validation
//! - [`StatusCodeMapper`] - Maps operations to HTTP status codes
//!
//! ## Key Methods
//!
//! - `ValidationError::new()` - Create a validation error
//! - `ValidationError::with_position()` - Add position information
//! - `ValidationError::with_suggestion()` - Add a suggestion for fixing the error
//! - `HttpValidator::validate_endpoint_path()` - Validate HTTP endpoint path
//! - `StatusCodeMapper::get_status_code()` - Get appropriate status code
//!
//! # Validation Rules
//!
//! ## Schema Validation
//!
//! - **Unique names**: Model and struct names must be unique
//! - **Field types**: Field types must be valid ForgeDB types
//! - **Primary keys**: Each model must have exactly one primary key
//! - **Relations**: Relation targets must reference existing models
//!
//! ## Constraint Validation
//!
//! - **@unique**: Can be applied to any field type
//! - **@email**: Only for string fields
//! - **@min/@max**: Only for numeric fields
//! - **@length**: Only for string fields
//! - **@pattern**: Only for string fields (regex validation)
//!
//! ## HTTP Validation
//!
//! - **Endpoint paths**: Must start with '/', valid parameter syntax
//! - **Status codes**: Must be valid HTTP status codes (200-599)
//! - **HTTP methods**: Must be valid (GET, POST, PUT, DELETE, PATCH)
//!
//! # Error Reporting
//!
//! Validation errors include:
//! - **Message**: Clear description of the error
//! - **Position**: Line and column where error occurred
//! - **Suggestion**: Helpful suggestion for fixing the error
//!
//! Example error output:
//! ```text
//! Error at line 5, column 12: Field 'email' must have @email constraint
//! Suggestion: Add @email constraint: email: string @email
//! ```
//!
//! # Related Crates
//!
//! - [`forgedb-parser`](../forgedb_parser) - Parses schemas before validation
//!
//! # See Also
//!
//! - [SPRINT2_VALIDATION.md](../../archive/sprint-summaries/SPRINT2_VALIDATION.md) - Schema validation
//! - [SPRINT9_SUMMARY.md](../../archive/sprint-summaries/SPRINT9_SUMMARY.md) - HTTP validation

pub mod http;
pub mod status;

use std::collections::HashSet;

pub use http::{HttpValidationError, HttpValidator};
pub use status::StatusCodeMapper;

/// Represents a position in the source code
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Position { line, column }
    }
}

/// Validation error with position information
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub message: String,
    pub position: Option<Position>,
    pub suggestion: Option<String>,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        ValidationError {
            message: message.into(),
            position: None,
            suggestion: None,
        }
    }

    pub fn with_position(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pos) = self.position {
            write!(
                f,
                "Error at line {}, column {}: {}",
                pos.line, pos.column, self.message
            )?;
        } else {
            write!(f, "Error: {}", self.message)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n  Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}

impl std::error::Error for ValidationError {}

pub type ValidationResult<T> = Result<T, ValidationError>;

/// Check if a string is in snake_case format
pub fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with lowercase letter or underscore
    if !s.chars().next().unwrap().is_lowercase() && !s.starts_with('_') {
        return false;
    }

    // Can only contain lowercase letters, digits, and underscores
    s.chars()
        .all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Check if a string is in PascalCase format
pub fn is_pascal_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with uppercase letter
    if !s.chars().next().unwrap().is_uppercase() {
        return false;
    }

    // Can only contain letters and digits, no underscores
    s.chars().all(|c| c.is_alphanumeric()) && !s.contains('_')
}

/// Convert a string to snake_case (for suggestions)
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];

        if c.is_uppercase() {
            // Add underscore before uppercase if:
            // - Not the first character
            // - Previous char was lowercase or digit
            // - OR next char is lowercase (e.g., "HTTPServer" -> "http_server")
            if i > 0 {
                let prev = chars[i - 1];
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();

                if prev.is_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_uppercase() && next_is_lower)
                {
                    result.push('_');
                }
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }

    result
}

/// Convert a string to PascalCase (for suggestions)
pub fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Validate field name follows snake_case convention
pub fn validate_field_name(name: &str, position: Option<Position>) -> ValidationResult<()> {
    if !is_snake_case(name) {
        let suggestion = to_snake_case(name);
        let error = ValidationError::new(format!("Field name '{}' must be in snake_case", name))
            .with_suggestion(format!("Consider using '{}'", suggestion));

        if let Some(pos) = position {
            return Err(error.with_position(pos));
        }
        return Err(error);
    }
    Ok(())
}

/// Validate model name follows PascalCase convention
pub fn validate_model_name(name: &str, position: Option<Position>) -> ValidationResult<()> {
    if !is_pascal_case(name) {
        let suggestion = to_pascal_case(name);
        let error = ValidationError::new(format!("Model name '{}' must be in PascalCase", name))
            .with_suggestion(format!("Consider using '{}'", suggestion));

        if let Some(pos) = position {
            return Err(error.with_position(pos));
        }
        return Err(error);
    }
    Ok(())
}

/// Check for duplicate field names in a collection
pub fn check_duplicate_fields(fields: &[(String, Option<Position>)]) -> ValidationResult<()> {
    let mut seen = HashSet::new();

    for (name, position) in fields {
        if seen.contains(name) {
            let error = ValidationError::new(format!("Duplicate field name '{}'", name));
            if let Some(pos) = position {
                return Err(error.with_position(*pos));
            }
            return Err(error);
        }
        seen.insert(name.clone());
    }

    Ok(())
}

/// Check for duplicate model names in a collection
pub fn check_duplicate_models(models: &[(String, Option<Position>)]) -> ValidationResult<()> {
    let mut seen = HashSet::new();

    for (name, position) in models {
        if seen.contains(name) {
            let error = ValidationError::new(format!("Duplicate model name '{}'", name));
            if let Some(pos) = position {
                return Err(error.with_position(*pos));
            }
            return Err(error);
        }
        seen.insert(name.clone());
    }

    Ok(())
}

