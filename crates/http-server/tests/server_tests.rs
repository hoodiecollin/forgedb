use forgedb_http_server::*;

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
    // Server created successfully
    assert!(true);

    let config = ServerConfig {
        port: 8080,
        ..Default::default()
    };
    let _server = Server::with_config(config);
    // Server with custom config created successfully
    assert!(true);
}

#[test]
fn test_middleware_application() {
    let server = Server::new();
    let router = Router::new();

    let _app = server.apply_middleware(router);
    // Just verify it compiles and applies without panic
    assert!(true);
}
