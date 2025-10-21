//! Intermediate example for forgedb-http-server
//!
//! This example demonstrates using authentication, rate limiting,
//! and response caching middleware.

use forgedb_http_server::*;
use std::sync::Arc;

// Simple shared state for the example
#[derive(Clone)]
struct AppState {
    counter: Arc<std::sync::atomic::AtomicU64>,
}

#[tokio::main]
async fn main() {
    println!("=== ForgeDB HTTP Server - With Middleware ===\n");

    // Initialize tracing for debugging
    Server::init_tracing();
    println!("✓ Tracing initialized");

    // Initialize health check system
    init_health_check();
    println!("✓ Health check system initialized");

    // Create shared state
    let state = AppState {
        counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    // Create rate limiter (100 requests per minute)
    let rate_limiter = RateLimiter::new(RateLimitConfig {
        max_requests: 100,
        window: std::time::Duration::from_secs(60),
    });
    println!("✓ Rate limiter configured (100 req/min)");

    // Create response cache
    let cache = ResponseCache::new(CacheConfig {
        max_entries: 1000,
        ttl: std::time::Duration::from_secs(300), // 5 minutes
    });
    println!("✓ Response cache configured (5 min TTL)");

    // Create API key auth hook
    let auth_hook = ApiKeyAuthHook::new(vec![
        "secret-key-1".to_string(),
        "secret-key-2".to_string(),
    ]);
    println!("✓ API key authentication configured");

    // Create router with middleware
    let app = Router::new()
        // Public endpoints (no auth required)
        .route("/", get(|| async { "Welcome to ForgeDB API!" }))
        .route(
            "/public/status",
            get(|| async {
                Json(serde_json::json!({
                    "status": "ok",
                    "version": "0.1.0",
                }))
            }),
        )
        // Protected endpoints (requires API key)
        .route(
            "/api/counter",
            get({
                let state = state.clone();
                move |State(state): State<AppState>| async move {
                    let count = state
                        .counter
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Json(serde_json::json!({
                        "count": count + 1,
                    }))
                }
            })
            .layer(middleware::from_fn(require_auth_middleware)),
        )
        .route(
            "/api/data/:id",
            get(|Path(id): Path<String>| async move {
                // Simulate fetching data
                Json(serde_json::json!({
                    "id": id,
                    "data": "example data",
                    "cached": true,
                }))
            })
            .layer(middleware::from_fn(require_auth_middleware)),
        )
        // Add health and metrics endpoints
        .merge(health_router())
        .merge(metrics_router())
        // Apply rate limiting to all routes
        .layer(middleware::from_fn(rate_limit_middleware))
        // Apply authentication context
        .layer(middleware::from_fn(auth_middleware))
        // Add shared state
        .with_state(state);

    println!("\n✓ Routes configured:");
    println!("  GET  /                   - Public root");
    println!("  GET  /public/status      - Public status");
    println!("  GET  /api/counter        - Protected counter (requires auth)");
    println!("  GET  /api/data/:id       - Protected data (requires auth)");
    println!("  GET  /health             - Health check");
    println!("  GET  /metrics            - Prometheus metrics");

    // Configure the server
    let config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 3000,
        enable_cors: true,
        enable_tracing: true,
    };

    println!("\n✓ Server starting at http://{}:{}...", config.host, config.port);
    println!("\nTest endpoints:");
    println!("  # Public endpoint:");
    println!("  curl http://localhost:3000/public/status");
    println!();
    println!("  # Protected endpoints (with API key):");
    println!("  curl -H 'X-API-Key: secret-key-1' http://localhost:3000/api/counter");
    println!("  curl -H 'X-API-Key: secret-key-1' http://localhost:3000/api/data/123");
    println!();
    println!("  # Without API key (should fail):");
    println!("  curl http://localhost:3000/api/counter");
    println!("\nPress Ctrl+C to stop the server\n");

    let server = Server::with_config(config);

    if let Err(e) = server.serve(app).await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}
