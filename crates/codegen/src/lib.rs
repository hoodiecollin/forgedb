pub mod config;
pub mod default_fill;
pub mod escape;
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

pub use api::ApiGenerator;
pub use config::{FsyncMode, GenConfig};
pub use default_fill::{FillValue, default_fill, fill_from_param};
pub use escape::{python_host, python_types, typescript_host, typescript_types};
pub use core_pkg::CorePackage;
pub use server_pkg::ServerPackage;
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
pub use transform::{
    EscapeBridge, HopPlan, ModelOp, TransformCrate, TransformGenerator, TransformPlan, VersionSchema,
};
pub use typescript::TypeScriptGenerator;
pub use wasm::WasmGenerator;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodegenError {
    #[error("Template error: {0}")]
    Template(String),

    #[error("Invalid schema: {0}")]
    InvalidSchema(String),

    #[error("Generation failed: {0}")]
    GenerationFailed(String),
}

pub type Result<T> = std::result::Result<T, CodegenError>;

#[derive(Debug, Clone)]
pub struct GeneratedCode {
    pub code: String,

    pub description: String,
}

impl GeneratedCode {
    pub fn line_count(&self) -> usize {
        self.code.lines().count()
    }
}
