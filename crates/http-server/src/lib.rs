//! SinkDB HTTP Server
//!
//! Provides HTTP server infrastructure for REST API generation.
//! Built on Axum for type-safe, high-performance HTTP handling.

mod error;
mod server;

pub use error::{ApiError, ErrorResponse};
pub use server::{Server, ServerConfig};

// Re-export commonly used types
pub use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
pub use serde::{Deserialize, Serialize};
