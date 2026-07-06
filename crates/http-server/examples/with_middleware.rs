//! Intermediate example for forgedb-http-server
//!
//! This example demonstrates composing authentication and rate-limiting
//! middleware around a small set of public and protected routes.

use forgedb_http_server::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// Simple shared state for the example
#[derive(Clone)]
struct AppState {
    counter: Arc<AtomicU64>,
}

// Protected handler: reads shared state.
async fn counter_handler(State(state): State<AppState>) -> impl IntoResponse {
    let count = state.counter.fetch_add(1, Ordering::SeqCst);
    Json(serde_json::json!({ "count": count + 1 }))
}

// Protected handler: echoes a path parameter.
async fn data_handler(Path(id): Path<String>) -> impl IntoResponse {
    Json(serde_json::json!({
        "id": id,
        "data": "example data",
    }))
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
        counter: Arc::new(AtomicU64::new(0)),
    };

    // Create rate limiter (100 requests per minute)
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig {
        max_requests: 100,
        window_secs: 60,
        enabled: true,
    }));
    println!("✓ Rate limiter configured (100 req/min)");

    // Create API key auth hook
    let auth_hook: Arc<dyn AuthHook> = Arc::new(ApiKeyAuthHook::new(vec![
        "secret-key-1".to_string(),
        "secret-key-2".to_string(),
    ]));
    println!("✓ API key authentication configured");

    // Protected routes carry AppState; `.with_state` resolves the state type
    // so the sub-router can be merged into the stateless top-level router.
    let protected = Router::new()
        .route("/api/counter", get(counter_handler))
        .route("/api/data/{id}", get(data_handler))
        .layer(middleware::from_fn(require_auth_middleware))
        .with_state(state);

    // Build the application router.
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
        // Protected endpoints (require an API key)
        .merge(protected)
        // Health and metrics endpoints
        .merge(health_router())
        .merge(metrics_router())
        // Rate limiting applied to every route (adapts the stateful middleware).
        .layer(middleware::from_fn({
            let rate_limiter = rate_limiter.clone();
            move |req, next| {
                let rate_limiter = rate_limiter.clone();
                async move { rate_limit_middleware(rate_limiter, req, next).await }
            }
        }))
        // Authentication context populated for every route.
        .layer(middleware::from_fn(move |req, next| {
            let auth_hook = auth_hook.clone();
            async move { auth_middleware(auth_hook, req, next).await }
        }));

    println!("\n✓ Routes configured:");
    println!("  GET  /                   - Public root");
    println!("  GET  /public/status      - Public status");
    println!("  GET  /api/counter        - Protected counter (requires auth)");
    println!("  GET  /api/data/{{id}}       - Protected data (requires auth)");
    println!("  GET  /health             - Health check");
    println!("  GET  /metrics            - Prometheus metrics");

    // Configure the server
    let config = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 3000,
        enable_cors: true,
        cors_allow_any: true, // dev-only: allow all origins for local testing
        enable_tracing: true,
        ..Default::default()
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
