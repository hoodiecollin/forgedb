use forgedb_http_server::*;
use std::path::PathBuf;

#[test]
fn test_tls_config_disabled() {
    let config = TlsConfig::disabled();
    assert!(!config.enabled);
    assert!(config.validate().is_ok());
}

#[test]
fn test_tls_config_validation() {
    let config = TlsConfig::new("/nonexistent/cert.pem", "/nonexistent/key.pem");
    assert!(config.validate().is_err());
}

#[test]
fn test_tls_config_new() {
    let config = TlsConfig::new("cert.pem", "key.pem");
    assert!(config.enabled);
    assert_eq!(config.cert_path, PathBuf::from("cert.pem"));
    assert_eq!(config.key_path, PathBuf::from("key.pem"));
}
