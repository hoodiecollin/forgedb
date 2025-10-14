//! ForgeDB HTTP Server
//!
//! Production-ready HTTP server infrastructure for REST API generation.
//! Built on Axum for type-safe, high-performance HTTP handling.
//!
//! ## Features
//!
//! - **Error Handling**: Comprehensive error types with structured responses
//! - **Observability**: Prometheus metrics, structured logging, health checks
//! - **Performance**: Rate limiting, response caching, connection management
//! - **Security**: TLS/SSL support, auth hooks, input validation
//!
//! ## Example
//!
//! ```rust,no_run
//! use forgedb_http_server::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new()
//!         .route("/", get(|| async { "Hello, World!" }));
//!
//!     Server::new()
//!         .serve(router)
//!         .await
//!         .expect("Server failed");
//! }
//! ```

mod error;
mod server;
pub mod auth;
pub mod cache;
pub mod health;
pub mod metrics;
pub mod rate_limit;
pub mod tls;

// Core exports
pub use error::{ApiError, ErrorDetail, ErrorResponse};
pub use server::{Server, ServerConfig};

// Observability
pub use health::{health_router, init_health_check, HealthChecker, HealthResponse, HealthStatus};
pub use metrics::{metrics_router, record_db_operation, record_error, record_http_request};

// Security & Performance
pub use auth::{
    auth_middleware, require_auth_middleware, require_role_middleware, ApiKeyAuthHook, AuthContext,
    AuthHook, JwtAuthHook, NoAuthHook,
};
pub use cache::{CacheConfig, CacheKey, ResponseCache};
pub use rate_limit::{rate_limit_middleware, RateLimitConfig, RateLimiter};
pub use tls::{serve_https, TlsConfig, TlsServer};

// Re-export commonly used types
pub use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Router,
};
pub use serde::{Deserialize, Serialize};
