//! TLS/SSL support for HTTPS

use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// TLS configuration
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Path to certificate file (PEM format)
    pub cert_path: PathBuf,
    /// Path to private key file (PEM format)
    pub key_path: PathBuf,
    /// Enable TLS
    pub enabled: bool,
}

impl TlsConfig {
    /// Create a new TLS configuration
    pub fn new(cert_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        Self {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
            enabled: true,
        }
    }

    /// Disable TLS (HTTP only)
    pub fn disabled() -> Self {
        Self {
            cert_path: PathBuf::new(),
            key_path: PathBuf::new(),
            enabled: false,
        }
    }

    /// Validate TLS configuration
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if !self.cert_path.exists() {
            return Err(format!(
                "Certificate file not found: {}",
                self.cert_path.display()
            ));
        }

        if !self.key_path.exists() {
            return Err(format!(
                "Private key file not found: {}",
                self.key_path.display()
            ));
        }

        Ok(())
    }
}

/// Start HTTPS server with TLS
pub async fn serve_https(
    router: Router,
    addr: SocketAddr,
    tls_config: TlsConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !tls_config.enabled {
        return Err("TLS is not enabled".into());
    }

    // Validate configuration
    tls_config.validate()?;

    tracing::info!("Loading TLS certificate from: {}", tls_config.cert_path.display());
    tracing::info!("Loading TLS key from: {}", tls_config.key_path.display());

    // Create rustls config
    let rustls_config = RustlsConfig::from_pem_file(&tls_config.cert_path, &tls_config.key_path)
        .await
        .map_err(|e| format!("Failed to load TLS certificates: {}", e))?;

    tracing::info!("Starting HTTPS server on https://{}", addr);

    // Serve with TLS
    axum_server::bind_rustls(addr, rustls_config)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}

/// Generate self-signed certificate for development (requires openssl CLI)
///
/// This is for development only. Use proper certificates in production.
pub async fn generate_self_signed_cert(
    cert_path: &Path,
    key_path: &Path,
    common_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::process::Command;

    tracing::info!("Generating self-signed certificate...");

    // Generate private key
    let key_output = Command::new("openssl")
        .args(&[
            "genrsa",
            "-out",
            key_path.to_str().unwrap(),
            "2048",
        ])
        .output()
        .await?;

    if !key_output.status.success() {
        return Err(format!(
            "Failed to generate private key: {}",
            String::from_utf8_lossy(&key_output.stderr)
        )
        .into());
    }

    // Generate certificate
    let cert_output = Command::new("openssl")
        .args(&[
            "req",
            "-new",
            "-x509",
            "-key",
            key_path.to_str().unwrap(),
            "-out",
            cert_path.to_str().unwrap(),
            "-days",
            "365",
            "-subj",
            &format!("/CN={}", common_name),
        ])
        .output()
        .await?;

    if !cert_output.status.success() {
        return Err(format!(
            "Failed to generate certificate: {}",
            String::from_utf8_lossy(&cert_output.stderr)
        )
        .into());
    }

    tracing::info!("Self-signed certificate generated successfully");
    tracing::info!("  Certificate: {}", cert_path.display());
    tracing::info!("  Private key: {}", key_path.display());
    tracing::warn!("⚠️  Self-signed certificates are for DEVELOPMENT ONLY");
    tracing::warn!("⚠️  Use proper CA-signed certificates in production");

    Ok(())
}

/// TLS server wrapper
pub struct TlsServer {
    addr: SocketAddr,
    tls_config: TlsConfig,
}

impl TlsServer {
    /// Create a new TLS server
    pub fn new(addr: SocketAddr, tls_config: TlsConfig) -> Self {
        Self { addr, tls_config }
    }

    /// Serve the router with TLS
    pub async fn serve(self, router: Router) -> Result<(), Box<dyn std::error::Error>> {
        serve_https(router, self.addr, self.tls_config).await
    }
}
