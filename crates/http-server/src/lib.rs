//! ForgeDB HTTP Server
//!
//! Production-ready HTTP server infrastructure for REST API generation.
//! Built on Axum for type-safe, high-performance HTTP handling.
//!
//! # Overview
//!
//! This crate provides a complete HTTP server framework for ForgeDB, offering:
//!
//! - **Type-safe routing** - Built on Axum's type-safe routing system
//! - **Error handling** - Comprehensive error types with structured responses
//! - **Observability** - Prometheus metrics, structured logging, health checks
//! - **Performance** - Rate limiting, response caching, connection management
//! - **Security** - TLS/SSL support, authentication hooks, input validation
//!
//! # Architecture
//!
//! The server is organized into several modules:
//!
//! - **Server Core** - Basic server configuration and startup
//! - **Error Handling** - Structured error types and API responses
//! - **Authentication** - JWT, API key, and custom auth strategies
//! - **Rate Limiting** - Token bucket rate limiting per client
//! - **Caching** - Response caching with TTL
//! - **Health Checks** - Readiness and liveness probes
//! - **Metrics** - Prometheus metrics collection
//! - **TLS** - HTTPS support with certificate management
//!
//! ## Request Flow
//!
//! ```text
//! Client Request
//!     ↓
//! Rate Limiter (check quota)
//!     ↓
//! Authentication (verify credentials)
//!     ↓
//! Router (match endpoint)
//!     ↓
//! Handler (process request)
//!     ↓
//! Cache (optional caching)
//!     ↓
//! Response
//! ```
//!
//! # Examples
//!
//! ## Basic Server
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
//!
//! ## With Authentication
//!
//! ```rust,no_run
//! use forgedb_http_server::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let auth = JwtAuthHook::new("secret_key");
//!     
//!     let router = Router::new()
//!         .route("/api/users", get(list_users))
//!         .layer(require_auth_middleware(auth));
//!
//!     Server::new()
//!         .bind("0.0.0.0:3000")
//!         .serve(router)
//!         .await
//!         .expect("Server failed");
//! }
//!
//! async fn list_users(auth: AuthContext) -> Json<Vec<String>> {
//!     // Access user info from auth context
//!     Json(vec!["user1".to_string()])
//! }
//! ```
//!
//! ## With Rate Limiting
//!
//! ```rust,no_run
//! use forgedb_http_server::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let rate_limit_config = RateLimitConfig {
//!         requests_per_second: 10.0,
//!         burst_size: 20,
//!     };
//!     
//!     let router = Router::new()
//!         .route("/api/data", get(get_data))
//!         .layer(rate_limit_middleware(rate_limit_config));
//!
//!     Server::new().serve(router).await.unwrap();
//! }
//!
//! async fn get_data() -> &'static str {
//!     "Data response"
//! }
//! ```
//!
//! ## With Metrics and Health Checks
//!
//! ```rust,no_run
//! use forgedb_http_server::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Initialize health checker
//!     init_health_check();
//!     
//!     let router = Router::new()
//!         .route("/api/data", get(get_data))
//!         .merge(health_router())
//!         .merge(metrics_router());
//!
//!     Server::new()
//!         .bind("0.0.0.0:3000")
//!         .serve(router)
//!         .await
//!         .unwrap();
//! }
//!
//! async fn get_data() -> &'static str {
//!     "Data response"
//! }
//! ```
//!
//! ## HTTPS Server
//!
//! ```rust,no_run
//! use forgedb_http_server::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let tls_config = TlsConfig {
//!         cert_path: "./cert.pem".into(),
//!         key_path: "./key.pem".into(),
//!     };
//!     
//!     let router = Router::new()
//!         .route("/", get(|| async { "Secure Hello!" }));
//!
//!     serve_https(router, "0.0.0.0:443", tls_config)
//!         .await
//!         .unwrap();
//! }
//! ```
//!
//! # Public API
//!
//! ## Core Types
//!
//! - [`Server`] - HTTP server builder and configuration
//! - [`ServerConfig`] - Server configuration options
//! - [`Router`] - Re-exported Axum router
//!
//! ## Error Handling
//!
//! - [`ApiError`] - Standard API error type
//! - [`ErrorResponse`] - JSON error response format
//! - [`ErrorDetail`] - Detailed error information
//!
//! ## Authentication
//!
//! - [`AuthContext`] - Authenticated user context
//! - [`AuthHook`] - Trait for custom authentication strategies
//! - [`JwtAuthHook`] - JWT-based authentication
//! - [`ApiKeyAuthHook`] - API key authentication
//! - [`NoAuthHook`] - No authentication (development)
//!
//! ## Performance
//!
//! - [`RateLimiter`] - Token bucket rate limiter
//! - [`RateLimitConfig`] - Rate limiting configuration
//! - [`ResponseCache`] - Response caching layer
//! - [`CacheConfig`] - Cache configuration
//!
//! ## Observability
//!
//! - [`HealthChecker`] - Health check status tracking
//! - [`HealthStatus`] - Health check status enum
//! - Metrics functions: `record_http_request()`, `record_db_operation()`, `record_error()`
//!
//! ## TLS/SSL
//!
//! - [`TlsServer`] - HTTPS server
//! - [`TlsConfig`] - TLS certificate configuration
//! - `serve_https()` - Serve HTTPS with TLS
//!
//! # Error Handling
//!
//! All API errors are returned in a consistent JSON format:
//!
//! ```json
//! {
//!   "error": "ValidationError",
//!   "message": "Invalid email format",
//!   "details": {
//!     "field": "email",
//!     "value": "invalid"
//!   },
//!   "timestamp": "2024-01-15T10:30:00Z"
//! }
//! ```
//!
//! # Metrics
//!
//! The server exposes Prometheus metrics at `/metrics`:
//!
//! - `http_requests_total` - Total HTTP requests
//! - `http_request_duration_seconds` - Request duration histogram
//! - `db_operations_total` - Database operation counter
//! - `errors_total` - Error counter by type
//!
//! # Related Crates
//!
//! - [`forgedb-crud-api`](../forgedb_crud_api) - CRUD handlers using this server
//! - [`forgedb-query-params`](../forgedb_query_params) - Query parameter parsing
//! - [`forgedb-validation`](../forgedb_validation) - Input validation
//!
//! # See Also
//!
//! - [README](./README.md) for detailed documentation
//! - [Axum documentation](https://docs.rs/axum) for router details
//! - [SPRINT9_HTTP_SERVER.md](../../archive/sprint-summaries/SPRINT9_HTTP_SERVER.md) - HTTP server implementation

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
