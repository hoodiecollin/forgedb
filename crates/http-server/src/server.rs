//! HTTP server setup and configuration

use axum::{
    http::StatusCode,
    middleware::{self, Next},
    response::IntoResponse,
    Router,
};
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host address (default: 0.0.0.0)
    pub host: String,
    /// Port (default: 3000)
    pub port: u16,
    /// Enable CORS middleware (default: false — no CORS headers are emitted)
    ///
    /// Setting this to `true` alone is not sufficient to allow cross-origin
    /// requests; you must also configure either [`cors_allow_any`] or
    /// [`cors_origins`].
    pub enable_cors: bool,
    /// Allow **all** origins, methods and headers (`*`).
    ///
    /// **This is dangerous in production.**  Only enable when you explicitly
    /// know you need open CORS — for example, during local development.
    /// Requires `enable_cors: true` to take effect.
    pub cors_allow_any: bool,
    /// Allowlist of permitted `Origin` header values (e.g. `["https://example.com"]`).
    ///
    /// Used when `enable_cors: true` and `cors_allow_any: false`.
    /// An empty list means no origins are allowed (no `Access-Control-Allow-Origin`
    /// header is sent).
    pub cors_origins: Vec<String>,
    /// Enable tracing/logging (default: true)
    pub enable_tracing: bool,
    /// Maximum allowed request body size in bytes.
    ///
    /// Requests with a larger body receive a `413 Payload Too Large` response.
    /// Default: 1 MiB (1 048 576 bytes).
    pub request_body_limit_bytes: usize,
    /// Per-request timeout in seconds.
    ///
    /// Requests that take longer than this receive a `408 Request Timeout`
    /// response.  Set to `0` to disable (not recommended for public servers).
    /// Default: 30 seconds.
    pub request_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            enable_cors: false,
            cors_allow_any: false,
            cors_origins: vec![],
            enable_tracing: true,
            request_body_limit_bytes: 1024 * 1024, // 1 MiB
            request_timeout_secs: 30,
        }
    }
}

/// HTTP server
pub struct Server {
    config: ServerConfig,
    socket_path: Option<std::path::PathBuf>,
}

impl Server {
    /// Create a new server with default configuration
    pub fn new() -> Self {
        Self {
            config: ServerConfig::default(),
            socket_path: None,
        }
    }

    /// Create a server with custom configuration
    pub fn with_config(config: ServerConfig) -> Self {
        Self {
            config,
            socket_path: None,
        }
    }

    /// Set Unix socket path
    pub fn with_socket(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.socket_path = Some(path.into());
        self
    }

    /// Initialize tracing/logging
    pub fn init_tracing() {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "forgedb=debug,tower_http=debug".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    /// Apply middleware to router
    pub fn apply_middleware(&self, router: Router) -> Router {
        let mut router = router;

        // --- Body size limit -------------------------------------------------
        // Applied first so oversized requests are rejected before any handler
        // runs, keeping memory usage bounded.
        if self.config.request_body_limit_bytes > 0 {
            router =
                router.layer(RequestBodyLimitLayer::new(self.config.request_body_limit_bytes));
        }

        // --- Request timeout -------------------------------------------------
        // Uses axum middleware::from_fn so the error type stays Infallible and
        // no HandleErrorLayer is needed.
        if self.config.request_timeout_secs > 0 {
            let timeout = Duration::from_secs(self.config.request_timeout_secs);
            router = router.layer(middleware::from_fn(
                move |req: axum::extract::Request, next: Next| async move {
                    match tokio::time::timeout(timeout, next.run(req)).await {
                        Ok(resp) => resp,
                        Err(_elapsed) => {
                            (StatusCode::REQUEST_TIMEOUT, "Request timed out").into_response()
                        }
                    }
                },
            ));
        }

        // --- CORS ------------------------------------------------------------
        // Any/Any/Any is only applied when the caller explicitly opts in with
        // cors_allow_any.  A specific allowlist is used when cors_origins is
        // non-empty.  An empty cors_origins without cors_allow_any sends no
        // CORS headers (safe default).
        if self.config.enable_cors {
            if self.config.cors_allow_any {
                let cors = CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any);
                router = router.layer(cors);
            } else if !self.config.cors_origins.is_empty() {
                use axum::http::HeaderValue;
                use tower_http::cors::AllowOrigin;
                let origins: Vec<HeaderValue> = self
                    .config
                    .cors_origins
                    .iter()
                    .filter_map(|o| HeaderValue::from_str(o).ok())
                    .collect();
                let cors = CorsLayer::new()
                    .allow_origin(AllowOrigin::list(origins))
                    .allow_methods(Any)
                    .allow_headers(Any);
                router = router.layer(cors);
            }
        }

        // --- Tracing ---------------------------------------------------------
        if self.config.enable_tracing {
            router = router.layer(TraceLayer::new_for_http());
        }

        router
    }

    /// Start the server with the given router
    pub async fn serve(self, router: Router) -> Result<(), Box<dyn std::error::Error>> {
        let app = self.apply_middleware(router);

        if let Some(socket_path) = &self.socket_path {
            use tokio::net::UnixListener;

            if socket_path.exists() {
                std::fs::remove_file(socket_path)?;
            }

            tracing::info!("Server listening on Unix socket: {}", socket_path.display());

            let listener = UnixListener::bind(socket_path)?;
            // Unix sockets don't have a TCP peer address; skip ConnectInfo.
            axum::serve(listener, app).await?;
        } else {
            let addr =
                format!("{}:{}", self.config.host, self.config.port).parse::<SocketAddr>()?;

            tracing::info!("Server listening on http://{}", addr);

            let listener = tokio::net::TcpListener::bind(addr).await?;
            // Inject ConnectInfo<SocketAddr> so the rate limiter can use the
            // real peer address instead of relying on spoofable proxy headers.
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }

        Ok(())
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
