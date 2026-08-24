//! ForgeDB Code Generator
//!
//! Generates database implementations, APIs, and type definitions from ForgeDB schemas.
//!
//! # Overview
//!
//! This crate provides code generation capabilities for ForgeDB schemas, converting
//! parsed AST into various output formats:
//!
//! - **Rust**: Database implementation with columnar storage, CRUD operations, and query API
//! - **TypeScript**: Type definitions and SDK client code
//! - **API**: REST API server implementation
//! - **OpenAPI**: API specification documents
//! - **Stubs**: Component and computed field implementation stubs
//!
//! # Architecture
//!
//! All generators follow a common pattern:
//! 1. Accept a `&Schema` (parsed AST from forgedb-parser)
//! 2. Accept generator-specific options
//! 3. Return `Result<GeneratedCode>` with the generated code as a string
//!
//! The caller is responsible for writing the generated code to files. This makes
//! the generators easier to test and more flexible.
//!
//! # Examples
//!
//! ```rust,no_run
//! use forgedb_codegen::RustGenerator;
//! use forgedb_parser::Parser;
//!
//! let schema_source = r#"
//!     model User {
//!         +id: uuid
//!         email: string @unique @email
//!         created_at: timestamp
//!     }
//! "#;
//!
//! let mut parser = Parser::new(schema_source).unwrap();
//! let schema = parser.parse().unwrap();
//!
//! let result = RustGenerator::generate(&schema)?;
//! println!("Generated {} lines of code", result.code.lines().count());
//! // Write to file yourself:
//! // std::fs::write("database.rs", result.code)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod config;
pub mod default_fill;
pub mod core_pkg;
pub mod server_pkg;
pub mod go_sdk;
pub mod python_sdk;
pub mod rust;
pub mod rust_sdk;
pub mod typescript;
pub mod api;
pub mod openapi;
pub mod stubs;
pub mod engine;
pub mod transform;
pub mod wasm;
pub mod ffi;
pub mod pyo3;
pub mod napi;
pub mod go;

// Re-export generators for convenience
pub use api::ApiGenerator;
pub use config::{FsyncMode, GenConfig};
pub use default_fill::{FillValue, default_fill, fill_from_param};
pub use core_pkg::CorePackage;
pub use server_pkg::{ServerLayout, ServerPackage};
pub use ffi::FfiGenerator;
pub use go::GoGenerator;
pub use go_sdk::GoSdkGenerator;
pub use python_sdk::PythonSdkGenerator;
pub use napi::NapiGenerator;
pub use openapi::OpenApiGenerator;
pub use pyo3::PyO3Generator;
pub use rust::RustGenerator;
pub use rust_sdk::RustSdkGenerator;
pub use stubs::StubGenerator;
pub use engine::{EngineHopPlan, EngineMigrationGenerator};
pub use transform::{HopPlan, ModelOp, TransformCrate, TransformGenerator, TransformPlan, VersionSchema};
pub use typescript::TypeScriptGenerator;
pub use wasm::WasmGenerator;

use thiserror::Error;

/// Errors that can occur during code generation
#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("Template error: {0}")]
    Template(String),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),

    #[error("Generation failed: {0}")]
    GenerationFailed(String),
}

/// Result type for code generation operations
pub type Result<T> = std::result::Result<T, CodegenError>;

/// Generated code result
#[derive(Debug, Clone)]
pub struct GeneratedCode {
    /// The generated code as a string
    pub code: String,

    /// Brief description of what was generated
    pub description: String,
}

impl GeneratedCode {
    /// Count the number of lines in the generated code
    pub fn line_count(&self) -> usize {
        self.code.lines().count()
    }
}
