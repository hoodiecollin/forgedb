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
}

impl Server {
    /// Create a new server with default configuration
    pub fn new() -> Self {
        Self {
            config: ServerConfig::default(),
        }
    }

    /// Create a server with custom configuration
    pub fn with_config(config: ServerConfig) -> Self {
        Self { config }
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

        // Build address
        let addr = format!("{}:{}", self.config.host, self.config.port).parse::<SocketAddr>()?;

        tracing::info!("Server listening on http://{}", addr);

        // Create TCP listener
        let listener = tokio::net::TcpListener::bind(addr).await?;

        // Serve
        axum::serve(listener, app).await?;

        Ok(())
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json};
    use serde_json::json;

    async fn health_check() -> Json<serde_json::Value> {
        Json(json!({"status": "ok"}))
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 3000);
        assert!(config.enable_cors);
        assert!(config.enable_tracing);
    }

    #[test]
    fn test_server_creation() {
        let server = Server::new();
        assert_eq!(server.config.port, 3000);

        let config = ServerConfig {
            port: 8080,
            ..Default::default()
        };
        let server = Server::with_config(config);
        assert_eq!(server.config.port, 8080);
    }

    #[test]
    fn test_middleware_application() {
        let server = Server::new();
        let router = Router::new().route("/health", get(health_check));

        let _app = server.apply_middleware(router);
        // Just verify it compiles and applies without panic
        assert!(true);
    }

    #[tokio::test]
    async fn test_health_check_handler() {
        let response = health_check().await;
        assert_eq!(response.0["status"], "ok");
    }
}
