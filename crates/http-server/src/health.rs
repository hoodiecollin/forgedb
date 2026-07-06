//! Health check endpoints for monitoring and load balancers

use axum::{http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub version: String,
    pub uptime_seconds: u64,
    pub timestamp: u64,
    pub checks: Vec<ComponentHealth>,
}

/// Individual component health
#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub response_time_ms: Option<u64>,
}

/// Health checker trait for custom health checks
pub trait HealthChecker: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self) -> ComponentHealth;
}

/// Basic database health checker
pub struct DatabaseHealthChecker;

impl HealthChecker for DatabaseHealthChecker {
    fn name(&self) -> &str {
        "database"
    }

    fn check(&self) -> ComponentHealth {
        // TODO: Implement actual database ping
        // For now, assume healthy
        ComponentHealth {
            name: self.name().to_string(),
            status: HealthStatus::Healthy,
            message: Some("Database connection active".to_string()),
            response_time_ms: Some(1),
        }
    }
}

/// Server start time recorded via [`init_health_check`].
///
/// Using `OnceLock<Instant>` avoids `static mut` and all associated `unsafe`
/// blocks; the value is written once (on first call to `init_health_check`) and
/// is readable from any thread without synchronisation.
static START_TIME: OnceLock<Instant> = OnceLock::new();

/// Initialize health check system.
///
/// Should be called once at server startup.  Subsequent calls are silently
/// ignored (the `OnceLock` value is never overwritten).
pub fn init_health_check() {
    let _ = START_TIME.set(Instant::now());
}

/// Get server uptime in seconds.
fn get_uptime_seconds() -> u64 {
    START_TIME
        .get()
        .map(|start| {
            // `Instant::elapsed` is infallible; use it directly.
            start.elapsed().as_secs()
        })
        .unwrap_or(0)
}


/// Liveness probe - always returns 200 OK if server is running
pub async fn liveness_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Readiness probe - checks if server can handle requests
pub async fn readiness_handler() -> impl IntoResponse {
    // Check if components are ready
    let db_checker = DatabaseHealthChecker;
    let db_health = db_checker.check();

    if db_health.status == HealthStatus::Healthy {
        (StatusCode::OK, Json(serde_json::json!({ "status": "ready" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "not_ready" })),
        )
    }
}

/// Detailed health check with component status
pub async fn health_handler() -> impl IntoResponse {
    let db_checker = DatabaseHealthChecker;

    let checks = vec![db_checker.check()];

    // Determine overall status
    let overall_status = if checks.iter().all(|c| c.status == HealthStatus::Healthy) {
        HealthStatus::Healthy
    } else if checks.iter().any(|c| c.status == HealthStatus::Unhealthy) {
        HealthStatus::Unhealthy
    } else {
        HealthStatus::Degraded
    };

    let response = HealthResponse {
        status: overall_status.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: get_uptime_seconds(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        checks,
    };

    let status_code = match overall_status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK, // Still available
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(response))
}

/// Create health check router
pub fn health_router() -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/health/live", get(liveness_handler))
        .route("/health/ready", get(readiness_handler))
}
