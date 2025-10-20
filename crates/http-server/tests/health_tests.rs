use forgedb_http_server::*;
use forgedb_http_server::health::{liveness_handler, health_handler, DatabaseHealthChecker};

#[test]
fn test_database_health_checker() {
    let checker = DatabaseHealthChecker;
    let health = checker.check();
    assert_eq!(health.name, "database");
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[test]
fn test_health_status_serialization() {
    let status = HealthStatus::Healthy;
    let json = serde_json::to_string(&status).unwrap();
    assert_eq!(json, "\"healthy\"");
}

#[tokio::test]
async fn test_liveness_handler() {
    let response = liveness_handler().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_handler() {
    init_health_check();
    let response = health_handler().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
}
