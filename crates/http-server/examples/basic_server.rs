//! Basic usage example for forgedb-http-server
//!
//! This example demonstrates creating a simple HTTP server with basic routes
//! and health checks.

use forgedb_http_server::*;

#[tokio::main]
async fn main() {
    println!("=== ForgeDB HTTP Server - Basic Usage ===\n");

    // Initialize health check system
    init_health_check();
    println!("✓ Health check system initialized");

    // Create a simple router with basic endpoints
    let app = Router::new()
        // Root endpoint
        .route("/", get(|| async { "Welcome to ForgeDB!" }))
        // Echo endpoint that returns JSON
        .route(
            "/echo/{message}",
            get(|Path(message): Path<String>| async move {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                Json(serde_json::json!({
                    "message": message,
                    "timestamp": timestamp,
                }))
            }),
        )
        // Add health check endpoint
        .merge(health_router())
        // Add metrics endpoint
        .merge(metrics_router());

    println!("✓ Routes configured:");
    println!("  GET  /              - Root endpoint");
    println!("  GET  /echo/{{message}} - Echo message");
    println!("  GET  /health        - Health check");
    println!("  GET  /metrics       - Prometheus metrics");

    // Configure the server
    let config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 3000,
        enable_cors: true,
        enable_tracing: true,
    };

    println!("\n✓ Server configured:");
    println!("  Host: {}", config.host);
    println!("  Port: {}", config.port);
    println!("  CORS enabled: {}", config.enable_cors);
    println!("  Tracing enabled: {}", config.enable_tracing);

    // Create and start the server
    println!("\n✓ Starting server at http://{}:{}...", config.host, config.port);
    println!("\nTest endpoints:");
    println!("  curl http://localhost:3000/");
    println!("  curl http://localhost:3000/echo/hello");
    println!("  curl http://localhost:3000/health");
    println!("  curl http://localhost:3000/metrics");
    println!("\nPress Ctrl+C to stop the server\n");

    let server = Server::with_config(config);

    if let Err(e) = server.serve(app).await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}
