# ForgeDB HTTP Server

Production-ready HTTP server infrastructure for ForgeDB REST APIs. Built on [Axum](https://github.com/tokio-rs/axum) for type-safe, high-performance HTTP handling.

## Features

### 🔧 Server Configuration
- Flexible host/port configuration
- Unix socket support
- Built-in CORS support
- Structured logging and tracing

### 🔒 Security
- **TLS/HTTPS Support** - Production-ready SSL/TLS with rustls
- **Authentication** - Pluggable auth system (JWT, API keys, custom)
- **Rate Limiting** - Token bucket algorithm for request throttling
- **Input Validation** - Type-safe request handling

### ⚡ Performance
- **Response Caching** - TTL-based HTTP response caching
- **Connection Management** - Efficient async I/O with Tokio
- **Middleware Pipeline** - Composable request/response processing

### 📊 Observability
- **Health Checks** - Liveness and readiness probes for Kubernetes
- **Prometheus Metrics** - Request latency, error rates, cache stats
- **Structured Logging** - JSON and text output with tracing

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
forgedb-http-server = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

### Basic Server

```rust
use forgedb_http_server::*;

#[tokio::main]
async fn main() {
    // Initialize logging
    Server::init_tracing();

    // Create a simple router
    let app = Router::new()
        .route("/", get(|| async { "Hello, ForgeDB!" }))
        .route("/health", get(|| async { "OK" }));

    // Start server
    Server::new()
        .serve(app)
        .await
        .expect("Server failed");
}
```

Server runs on `http://0.0.0.0:3000` by default.

## Configuration

### ServerConfig

Customize server behavior with `ServerConfig`:

```rust
use forgedb_http_server::*;

let config = ServerConfig {
    host: "127.0.0.1".to_string(),
    port: 8080,
    enable_cors: true,
    enable_tracing: true,
};

let server = Server::with_config(config);
```

**Default values:**
- `host`: `"0.0.0.0"`
- `port`: `3000`
- `enable_cors`: `true`
- `enable_tracing`: `true`

### Unix Socket Support

Run on Unix domain socket instead of TCP:

```rust
Server::new()
    .with_socket("/tmp/forgedb.sock")
    .serve(app)
    .await?;
```

## TLS/HTTPS

### Basic HTTPS Setup

```rust
use forgedb_http_server::*;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Secure!" }));

    let tls_config = TlsConfig::new(
        "/path/to/cert.pem",
        "/path/to/key.pem",
    );

    let addr = "0.0.0.0:443".parse::<SocketAddr>().unwrap();

    serve_https(app, addr, tls_config)
        .await
        .expect("HTTPS server failed");
}
```

### Generating Self-Signed Certificates (Development Only)

For development and testing:

```rust
use forgedb_http_server::tls::generate_self_signed_cert;
use std::path::Path;

#[tokio::main]
async fn main() {
    generate_self_signed_cert(
        Path::new("./cert.pem"),
        Path::new("./key.pem"),
        "localhost"
    ).await.expect("Failed to generate cert");
}
```

⚠️ **Security Warning**: Self-signed certificates are for **development only**. Use proper CA-signed certificates in production.

### Production TLS Best Practices

1. **Use CA-signed certificates** from Let's Encrypt or commercial CA
2. **Secure private keys** with appropriate file permissions (e.g., `chmod 600`)
3. **Rotate certificates** before expiration
4. **Use strong cipher suites** (handled automatically by rustls)
5. **Enable HTTP/2** (automatic with axum-server)

## Authentication

### Built-in Auth Hooks

The server provides pluggable authentication via the `AuthHook` trait:

#### API Key Authentication

```rust
use forgedb_http_server::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let api_keys = vec!["secret-key-1".to_string(), "secret-key-2".to_string()];
    let auth_hook = Arc::new(ApiKeyAuthHook::new(api_keys));

    let app = Router::new()
        .route("/api/protected", get(protected_handler))
        .layer(axum::middleware::from_fn(move |req, next| {
            let hook = auth_hook.clone();
            auth_middleware(hook, req, next)
        }));

    Server::new().serve(app).await.unwrap();
}

async fn protected_handler() -> &'static str {
    "Protected resource"
}
```

Client requests must include the header: `X-API-Key: secret-key-1`

#### JWT Authentication

```rust
use forgedb_http_server::*;
use std::sync::Arc;

let jwt_hook = Arc::new(JwtAuthHook::new("your-secret-key".to_string()));

let app = Router::new()
    .route("/api/user", get(user_handler))
    .layer(axum::middleware::from_fn(move |req, next| {
        let hook = jwt_hook.clone();
        auth_middleware(hook, req, next)
    }));
```

Client requests use: `Authorization: Bearer <token>`

### Requiring Authentication

Enforce authentication on specific routes:

```rust
use forgedb_http_server::*;

let protected = Router::new()
    .route("/admin", get(admin_handler))
    .layer(axum::middleware::from_fn(require_auth_middleware));

let app = Router::new()
    .route("/public", get(public_handler))
    .nest("/", protected);
```

### Role-Based Access Control

Restrict routes by user role:

```rust
use forgedb_http_server::*;

let admin_routes = Router::new()
    .route("/admin/users", get(list_users))
    .route("/admin/settings", get(get_settings))
    .layer(axum::middleware::from_fn(
        require_role_middleware("admin".to_string())
    ));
```

### Custom Authentication

Implement the `AuthHook` trait for custom auth logic:

```rust
use forgedb_http_server::*;
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;

struct CustomAuthHook {
    // Your auth state
}

impl AuthHook for CustomAuthHook {
    fn authenticate(&self, req: &Request<Body>) -> Result<AuthContext, Response> {
        // Extract credentials from request
        // Validate against your auth system
        // Return AuthContext or error Response
        
        Ok(AuthContext::authenticated(
            "user_id".to_string(),
            vec!["user".to_string()],
        ))
    }
}
```

## Rate Limiting

Protect your API from abuse with token bucket rate limiting:

### Basic Rate Limiting

```rust
use forgedb_http_server::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let rate_limit_config = RateLimitConfig {
        max_requests: 100,      // 100 requests
        window_secs: 60,        // per 60 seconds (1 minute)
        enabled: true,
    };

    let limiter = Arc::new(RateLimiter::new(rate_limit_config));

    let app = Router::new()
        .route("/api/data", get(data_handler))
        .layer(axum::middleware::from_fn(move |req, next| {
            let lim = limiter.clone();
            rate_limit_middleware(lim, req, next)
        }));

    Server::new().serve(app).await.unwrap();
}
```

### Configuration Options

```rust
use forgedb_http_server::*;

// Strict limit: 10 requests per minute
let strict_config = RateLimitConfig {
    max_requests: 10,
    window_secs: 60,
    enabled: true,
};

// Generous limit: 1000 requests per hour
let generous_config = RateLimitConfig {
    max_requests: 1000,
    window_secs: 3600,
    enabled: true,
};

// Disable rate limiting
let disabled_config = RateLimitConfig {
    enabled: false,
    ..Default::default()
};
```

### Rate Limit Response

When rate limit is exceeded, clients receive:

```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Too many requests. Please try again later.",
    "retry_after_seconds": 42
  }
}
```

HTTP Status: `429 Too Many Requests`  
Header: `Retry-After: 42`

## Response Caching

Improve performance with intelligent HTTP response caching:

### Basic Caching

```rust
use forgedb_http_server::*;
use std::sync::Arc;

let cache_config = CacheConfig {
    enabled: true,
    default_ttl_secs: 300,  // 5 minutes
    max_entries: 1000,
};

let cache = Arc::new(ResponseCache::new(cache_config));

// Use in handlers
async fn cached_handler(
    State(cache): State<Arc<ResponseCache>>,
) -> impl IntoResponse {
    let key = CacheKey::new("GET", "/api/users", "");
    
    // Try cache first
    if let Some(entry) = cache.get(&key) {
        return (
            StatusCode::from_u16(entry.status).unwrap(),
            entry.data
        ).into_response();
    }
    
    // Generate response
    let data = b"Fresh data".to_vec();
    
    // Cache it
    cache.set(key, data.clone(), "application/json".to_string(), 200);
    
    (StatusCode::OK, data).into_response()
}
```

### Cache Configuration

```rust
use forgedb_http_server::*;
use std::time::Duration;

let config = CacheConfig {
    enabled: true,
    default_ttl_secs: 600,   // 10 minutes default
    max_entries: 5000,        // Cache up to 5000 responses
};

let cache = ResponseCache::new(config);

// Custom TTL for specific entries
cache.set_with_ttl(
    key,
    data,
    "application/json".to_string(),
    200,
    Duration::from_secs(3600)  // 1 hour
);
```

### Cache Invalidation

```rust
// Invalidate specific entry
cache.invalidate(&cache_key);

// Clear entire cache
cache.clear();

// Get cache statistics
let stats = cache.stats();
println!("Active entries: {}", stats.active_entries);
println!("Expired entries: {}", stats.expired_entries);
```

## Health Checks

Kubernetes-compatible health check endpoints:

### Setup Health Checks

```rust
use forgedb_http_server::*;

#[tokio::main]
async fn main() {
    // Initialize health system
    init_health_check();

    let app = Router::new()
        .merge(health_router())  // Adds /health, /health/live, /health/ready
        .route("/api/data", get(data_handler));

    Server::new().serve(app).await.unwrap();
}
```

### Health Endpoints

#### `/health` - Detailed Health Status

Returns comprehensive health information:

```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "timestamp": 1699564800,
  "checks": [
    {
      "name": "database",
      "status": "healthy",
      "message": "Database connection active",
      "response_time_ms": 1
    }
  ]
}
```

Status codes:
- `200 OK` - Healthy or Degraded
- `503 Service Unavailable` - Unhealthy

#### `/health/live` - Liveness Probe

Simple endpoint for Kubernetes liveness checks:

```
GET /health/live
200 OK
```

Always returns `200 OK` if server is running.

#### `/health/ready` - Readiness Probe

Indicates if the server can handle traffic:

```json
{
  "status": "ready"
}
```

Use for Kubernetes readiness probes.

### Custom Health Checks

Implement `HealthChecker` trait:

```rust
use forgedb_http_server::*;

struct CustomHealthCheck;

impl HealthChecker for CustomHealthCheck {
    fn name(&self) -> &str {
        "my_service"
    }

    fn check(&self) -> ComponentHealth {
        // Perform health check logic
        ComponentHealth {
            name: self.name().to_string(),
            status: HealthStatus::Healthy,
            message: Some("Service operational".to_string()),
            response_time_ms: Some(5),
        }
    }
}
```

## Prometheus Metrics

Export metrics for Prometheus monitoring:

### Setup Metrics

```rust
use forgedb_http_server::*;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .merge(metrics_router())  // Adds /metrics endpoint
        .route("/api/data", get(data_handler));

    Server::new().serve(app).await.unwrap();
}
```

### Available Metrics

Metrics are automatically exported at `/metrics` in Prometheus format:

```
# HTTP Requests
forgedb_http_requests_total{method="GET",path="/api/users",status="200"} 1523

# Request Duration
forgedb_http_request_duration_seconds_bucket{method="GET",path="/api/users",le="0.005"} 1420
forgedb_http_request_duration_seconds_sum{method="GET",path="/api/users"} 2.341

# Database Operations
forgedb_db_operations_total{operation="select",model="users"} 850
forgedb_db_operation_duration_seconds_sum{operation="select",model="users"} 1.234

# Active Connections
forgedb_active_connections{type="http"} 42

# Cache Operations
forgedb_cache_operations_total{operation="hit"} 3421
forgedb_cache_operations_total{operation="miss"} 892

# Errors
forgedb_errors_total{type="database",code="CONNECTION_FAILED"} 3
```

### Recording Metrics

Record custom metrics in your handlers:

```rust
use forgedb_http_server::*;
use std::time::Instant;

async fn my_handler() -> impl IntoResponse {
    let start = Instant::now();
    
    // Your handler logic
    let result = process_request().await;
    
    // Record metrics
    record_http_request(
        "GET",
        "/api/users",
        200,
        start.elapsed().as_secs_f64()
    );
    
    record_db_operation(
        "select",
        "users",
        0.0023  // duration in seconds
    );
    
    result
}

async fn process_request() -> impl IntoResponse {
    "OK"
}
```

### Prometheus Configuration

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'forgedb'
    static_configs:
      - targets: ['localhost:3000']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

## Middleware

The server uses a composable middleware pipeline for request/response processing.

### Applying Middleware

Middleware can be applied at different levels:

```rust
use forgedb_http_server::*;
use tower_http::timeout::TimeoutLayer;
use std::time::Duration;

// Global middleware (applies to all routes)
let app = Router::new()
    .route("/api/data", get(data_handler))
    .layer(TimeoutLayer::new(Duration::from_secs(30)));

// Route-specific middleware
let protected = Router::new()
    .route("/admin", get(admin_handler))
    .layer(axum::middleware::from_fn(require_auth_middleware));

let app = Router::new()
    .route("/public", get(public_handler))
    .merge(protected);
```

### Built-in Middleware

#### CORS (Enabled by default)

```rust
// CORS is automatically enabled when using Server::new()
// To customize:
use tower_http::cors::{CorsLayer, Any};

let cors = CorsLayer::new()
    .allow_origin("https://example.com".parse::<HeaderValue>().unwrap())
    .allow_methods([Method::GET, Method::POST])
    .allow_headers(Any);

let app = Router::new()
    .route("/api/data", get(handler))
    .layer(cors);
```

#### Tracing (Enabled by default)

```rust
use tower_http::trace::TraceLayer;

let app = Router::new()
    .route("/", get(handler))
    .layer(TraceLayer::new_for_http());
```

#### Compression

```rust
use tower_http::compression::CompressionLayer;

let app = Router::new()
    .route("/", get(handler))
    .layer(CompressionLayer::new());
```

#### Request/Response Size Limits

```rust
use tower_http::limit::RequestBodyLimitLayer;

let app = Router::new()
    .route("/upload", post(upload_handler))
    .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)); // 10 MB
```

### Middleware Order

Middleware is applied in reverse order (inside-out):

```rust
let app = Router::new()
    .route("/", get(handler))
    .layer(middleware_3)  // Runs third
    .layer(middleware_2)  // Runs second
    .layer(middleware_1); // Runs first
```

### Custom Middleware

Create custom middleware using `axum::middleware::from_fn`:

```rust
use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};

async fn my_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    // Before processing request
    println!("Request: {} {}", req.method(), req.uri());
    
    // Process request
    let response = next.run(req).await;
    
    // After processing request
    println!("Response status: {}", response.status());
    
    response
}

// Apply it
let app = Router::new()
    .route("/", get(handler))
    .layer(axum::middleware::from_fn(my_middleware));
```

## Complete Example

Putting it all together:

```rust
use forgedb_http_server::*;
use std::sync::Arc;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Initialize logging and health checks
    Server::init_tracing();
    init_health_check();

    // Setup authentication
    let api_keys = vec!["secret-key".to_string()];
    let auth_hook = Arc::new(ApiKeyAuthHook::new(api_keys));

    // Setup rate limiting
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig {
        max_requests: 100,
        window_secs: 60,
        enabled: true,
    }));

    // Setup caching
    let cache = Arc::new(ResponseCache::new(CacheConfig {
        enabled: true,
        default_ttl_secs: 300,
        max_entries: 1000,
    }));

    // Build router
    let app = Router::new()
        // Public routes
        .route("/", get(|| async { "ForgeDB API" }))
        .merge(health_router())
        .merge(metrics_router())
        
        // Protected API routes
        .route("/api/data", get(get_data))
        .route("/api/data", post(create_data))
        .layer(axum::middleware::from_fn(move |req, next| {
            let hook = auth_hook.clone();
            auth_middleware(hook, req, next)
        }))
        .layer(axum::middleware::from_fn(move |req, next| {
            let lim = rate_limiter.clone();
            rate_limit_middleware(lim, req, next)
        }))
        .with_state(cache);

    // Configure TLS
    let tls_config = TlsConfig::new("./cert.pem", "./key.pem");
    let addr = "0.0.0.0:443".parse::<SocketAddr>().unwrap();

    // Start HTTPS server
    tracing::info!("Starting ForgeDB HTTP server...");
    serve_https(app, addr, tls_config)
        .await
        .expect("Server failed");
}

async fn get_data() -> &'static str {
    "Data retrieved"
}

async fn create_data() -> &'static str {
    "Data created"
}
```

## Security Best Practices

### 1. Authentication
- ✅ Always use authentication on production APIs
- ✅ Use strong API keys (minimum 32 characters, cryptographically random)
- ✅ Rotate keys regularly
- ✅ Never commit secrets to version control

### 2. TLS/HTTPS
- ✅ **Always use HTTPS in production**
- ✅ Use CA-signed certificates (Let's Encrypt is free)
- ✅ Keep private keys secure (file permissions: `600`)
- ✅ Monitor certificate expiration
- ❌ Never use self-signed certificates in production

### 3. Rate Limiting
- ✅ Enable rate limiting on all public endpoints
- ✅ Set conservative limits initially (can adjust based on usage)
- ✅ Use different limits for authenticated vs. anonymous users
- ✅ Log rate limit violations for security monitoring

### 4. Input Validation
- ✅ Use Axum's type-safe extractors (`Json<T>`, `Query<T>`)
- ✅ Implement request size limits
- ✅ Validate all input fields
- ✅ Sanitize user input before database operations

### 5. Error Handling
- ✅ Don't leak sensitive information in error messages
- ✅ Use structured error responses
- ✅ Log errors with appropriate severity levels
- ✅ Monitor error rates with metrics

### 6. Headers
- ✅ Set security headers (HSTS, CSP, X-Frame-Options)
- ✅ Configure CORS appropriately (avoid `allow_origin(Any)` in production)
- ✅ Remove server version headers

Example security headers:

```rust
use axum::{
    middleware::Next,
    extract::Request,
    response::Response,
};

async fn security_headers_middleware(
    req: Request,
    next: Next,
) -> Response {
    let mut response = next.run(req).await;
    
    let headers = response.headers_mut();
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("Strict-Transport-Security", 
        "max-age=31536000; includeSubDomains".parse().unwrap());
    
    response
}
```

## Testing

Run the test suite:

```bash
# Run all tests
cargo test -p forgedb-http-server

# Run specific test file
cargo test -p forgedb-http-server --test server_tests

# Run with output
cargo test -p forgedb-http-server -- --nocapture

# Run doc tests
cargo test -p forgedb-http-server --doc
```

### Test Coverage

The crate includes comprehensive tests for:
- ✅ Server configuration
- ✅ Authentication hooks (JWT, API key, custom)
- ✅ Rate limiting (token bucket algorithm)
- ✅ Response caching (TTL, invalidation)
- ✅ Health checks (liveness, readiness)
- ✅ Metrics collection
- ✅ TLS configuration
- ✅ Error handling

## Architecture

```
forgedb-http-server/
├── src/
│   ├── lib.rs           # Public API exports
│   ├── server.rs        # Core server implementation
│   ├── auth.rs          # Authentication middleware
│   ├── rate_limit.rs    # Rate limiting middleware
│   ├── cache.rs         # Response caching
│   ├── tls.rs           # TLS/HTTPS support
│   ├── health.rs        # Health check endpoints
│   ├── metrics.rs       # Prometheus metrics
│   └── error.rs         # Error types
├── tests/
│   ├── server_tests.rs
│   ├── auth_tests.rs
│   ├── rate_limit_tests.rs
│   ├── cache_tests.rs
│   ├── tls_tests.rs
│   ├── health_tests.rs
│   └── metrics_tests.rs
└── Cargo.toml
```

## Dependencies

Core dependencies:
- **axum** (0.8) - Web framework
- **axum-server** (0.6) - TLS support
- **tokio** (1.x) - Async runtime
- **tower** (0.4) - Middleware framework
- **tower-http** (0.5) - HTTP middleware
- **prometheus** (0.13) - Metrics collection
- **rustls** (0.23) - TLS implementation
- **tracing** (0.1) - Structured logging

## Performance Considerations

### Caching Strategy
- Cache GET requests only (not POST/PUT/DELETE)
- Set appropriate TTLs based on data volatility
- Monitor cache hit rates via metrics
- Consider cache size limits based on available memory

### Rate Limiting
- Token bucket provides smooth rate limiting
- Per-client tracking prevents one client affecting others
- Automatic token refill for burst handling

### Connection Management
- Tokio provides efficient async I/O
- Connection pooling handled by underlying runtime
- Consider using connection limits in production

## Troubleshooting

### Server Won't Start

```
Error: Address already in use
```

**Solution**: Change port or stop the process using that port:
```bash
lsof -i :3000
kill <PID>
```

### TLS Certificate Errors

```
Error: Failed to load TLS certificates
```

**Solution**: Verify certificate paths and permissions:
```bash
ls -l /path/to/cert.pem
ls -l /path/to/key.pem
openssl x509 -in cert.pem -text -noout  # Verify cert
```

### High Memory Usage

If caching is consuming too much memory:
- Reduce `max_entries` in `CacheConfig`
- Decrease `default_ttl_secs` to expire entries faster
- Disable caching for large responses

### Rate Limit Not Working

Ensure middleware is applied correctly:
```rust
// ✅ Correct - middleware applied to routes
let app = Router::new()
    .route("/api/data", get(handler))
    .layer(/* middleware */);

// ❌ Wrong - middleware after routes
let app = Router::new()
    .layer(/* middleware */)
    .route("/api/data", get(handler));
```

## License

Part of the ForgeDB project.
