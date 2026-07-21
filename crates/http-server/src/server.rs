//! HTTP server setup and configuration

use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host address (default: 0.0.0.0)
    pub host: String,
    /// Port (default: 3000)
    pub port: u16,
    /// Enable CORS (default: true)
    pub enable_cors: bool,
    /// Enable tracing/logging (default: true)
    pub enable_tracing: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 3000,
            enable_cors: true,
            enable_tracing: true,
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

        // Add CORS if enabled
        if self.config.enable_cors {
            let cors = CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any);
            router = router.layer(cors);
        }

        // Add tracing if enabled
        if self.config.enable_tracing {
            router = router.layer(TraceLayer::new_for_http());
        }

        router
    }

    /// Start the server with the given router
    pub async fn serve(self, router: Router) -> Result<(), Box<dyn std::error::Error>> {
        // Apply middleware
        let app = self.apply_middleware(router);

        if let Some(socket_path) = &self.socket_path {
            // Unix socket mode
            use tokio::net::UnixListener;

            // Remove old socket if exists
            if socket_path.exists() {
                std::fs::remove_file(socket_path)?;
            }

            tracing::info!("Server listening on Unix socket: {}", socket_path.display());

            let listener = UnixListener::bind(socket_path)?;
            axum::serve(listener, app).await?;
        } else {
            // TCP mode
            let addr = format!("{}:{}", self.config.host, self.config.port).parse::<SocketAddr>()?;

            tracing::info!("Server listening on http://{}", addr);

            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app).await?;
        }

        Ok(())
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
