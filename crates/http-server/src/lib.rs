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
//! - **Performance** - Rate limiting, connection management
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
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     // ApiKeyAuthHook checks the `X-API-Key` header against the allowlist.
//!     let auth_hook: Arc<dyn AuthHook> = Arc::new(ApiKeyAuthHook::new(vec![
//!         "secret-api-key".to_string(),
//!     ]));
//!
//!     // Protected sub-router: require_auth_middleware rejects unauthenticated
//!     // requests with 401.  auth_middleware populates the AuthContext extension
//!     // used by require_auth_middleware.
//!     let protected = Router::new()
//!         .route("/api/users", get(list_users))
//!         .layer(middleware::from_fn(require_auth_middleware));
//!
//!     let app = Router::new()
//!         .merge(protected)
//!         .layer(middleware::from_fn(move |req, next| {
//!             let auth_hook = auth_hook.clone();
//!             async move { auth_middleware(auth_hook, req, next).await }
//!         }));
//!
//!     Server::new().serve(app).await.expect("Server failed");
//! }
//!
//! async fn list_users() -> Json<serde_json::Value> {
//!     Json(serde_json::json!(["user1", "user2"]))
//! }
//! ```
//!
//! ## With Rate Limiting
//!
//! ```rust,no_run
//! use forgedb_http_server::*;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let limiter = Arc::new(RateLimiter::new(RateLimitConfig {
//!         max_requests: 100,
//!         window_secs: 60,
//!         enabled: true,
//!         trust_proxy: false,
//!         max_entries: 10_000,
//!     }));
//!
//!     let app = Router::new()
//!         .route("/api/data", get(get_data))
//!         .layer(middleware::from_fn(move |req, next| {
//!             let limiter = limiter.clone();
//!             async move { rate_limit_middleware(limiter, req, next).await }
//!         }));
//!
//!     Server::new().serve(app).await.unwrap();
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
//!     // Record the start time for uptime reporting.
//!     init_health_check();
//!
//!     let router = Router::new()
//!         .route("/api/data", get(get_data))
//!         .merge(health_router())
//!         .merge(metrics_router());
//!
//!     Server::new().serve(router).await.unwrap();
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
//! use std::net::SocketAddr;
//!
//! #[tokio::main]
//! async fn main() {
//!     let tls_config = TlsConfig::new("./cert.pem", "./key.pem");
//!
//!     let router = Router::new()
//!         .route("/", get(|| async { "Secure Hello!" }));
//!
//!     let addr: SocketAddr = "0.0.0.0:443".parse().unwrap();
//!     serve_https(router, addr, tls_config)
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
    AuthHook, NoAuthHook,
};
/// Development-only static-token bearer stub.  Gated behind `feature = "dev-auth"`.
#[cfg(feature = "dev-auth")]
pub use auth::JwtAuthHook;
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
