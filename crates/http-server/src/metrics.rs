//! Prometheus metrics collection and export

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, Encoder,
    GaugeVec, HistogramVec, TextEncoder,
};

lazy_static! {
    /// HTTP request counter by method, path, and status
    pub static ref HTTP_REQUESTS: CounterVec = register_counter_vec!(
        "forgedb_http_requests_total",
        "Total number of HTTP requests",
        &["method", "path", "status"]
    )
    .unwrap();

    /// HTTP request duration histogram
    pub static ref HTTP_DURATION: HistogramVec = register_histogram_vec!(
        "forgedb_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "path"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
    )
    .unwrap();

    /// Database operation counter
    pub static ref DB_OPERATIONS: CounterVec = register_counter_vec!(
        "forgedb_db_operations_total",
        "Total number of database operations",
        &["operation", "model"]
    )
    .unwrap();

    /// Database operation duration histogram
    pub static ref DB_DURATION: HistogramVec = register_histogram_vec!(
        "forgedb_db_operation_duration_seconds",
        "Database operation duration in seconds",
        &["operation", "model"],
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]
    )
    .unwrap();

    /// Active connections gauge
    pub static ref ACTIVE_CONNECTIONS: GaugeVec = register_gauge_vec!(
        "forgedb_active_connections",
        "Number of active connections",
        &["type"]
    )
    .unwrap();

    /// Cache hit/miss counter
    pub static ref CACHE_OPERATIONS: CounterVec = register_counter_vec!(
        "forgedb_cache_operations_total",
        "Cache operations (hits/misses)",
        &["operation"]
    )
    .unwrap();

    /// Error counter by type
    pub static ref ERRORS: CounterVec = register_counter_vec!(
        "forgedb_errors_total",
        "Total number of errors",
        &["type", "code"]
    )
    .unwrap();
}

/// Metrics endpoint handler
pub async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        tracing::error!("Failed to encode metrics: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to encode metrics",
        )
            .into_response();
    }

    let output = String::from_utf8(buffer).unwrap_or_else(|e| {
        tracing::error!("Failed to convert metrics to string: {}", e);
        String::from("Failed to convert metrics")
    });

    (StatusCode::OK, output).into_response()
}

/// Create metrics router
pub fn metrics_router() -> Router {
    Router::new().route("/metrics", get(metrics_handler))
}

/// Record an HTTP request
pub fn record_http_request(method: &str, path: &str, status: u16, duration_secs: f64) {
    HTTP_REQUESTS
        .with_label_values(&[method, path, &status.to_string()])
        .inc();

    HTTP_DURATION
        .with_label_values(&[method, path])
        .observe(duration_secs);
}

/// Record a database operation
pub fn record_db_operation(operation: &str, model: &str, duration_secs: f64) {
    DB_OPERATIONS
        .with_label_values(&[operation, model])
        .inc();

    DB_DURATION
        .with_label_values(&[operation, model])
        .observe(duration_secs);
}

/// Record an error
pub fn record_error(error_type: &str, error_code: &str) {
    ERRORS.with_label_values(&[error_type, error_code]).inc();
}

/// Increment active connections
pub fn increment_connections(conn_type: &str) {
    ACTIVE_CONNECTIONS.with_label_values(&[conn_type]).inc();
}

/// Decrement active connections
pub fn decrement_connections(conn_type: &str) {
    ACTIVE_CONNECTIONS.with_label_values(&[conn_type]).dec();
}

/// Record cache operation
pub fn record_cache_operation(operation: &str) {
    CACHE_OPERATIONS.with_label_values(&[operation]).inc();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_http_request() {
        record_http_request("GET", "/api/users", 200, 0.05);
        // Metrics should be recorded without panicking
    }

    #[test]
    fn test_record_db_operation() {
        record_db_operation("select", "User", 0.001);
        // Metrics should be recorded without panicking
    }

    #[test]
    fn test_record_error() {
        record_error("validation", "BAD_REQUEST");
        // Metrics should be recorded without panicking
    }
}
