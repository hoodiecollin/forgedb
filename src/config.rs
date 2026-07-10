/// Compile-time generator configuration loaded from `forgedb.toml`.
///
/// Only the `[generate]` table is consumed here; other tables present in a
/// project's `forgedb.toml` (e.g. `[project]`, `[database]`, `[api]`) are
/// silently ignored, so this struct is forward-compatible with the full config
/// file emitted by `forgedb init`.
///
/// Precedence (highest → lowest):
/// 1. Explicit CLI flag (`--output`, `--schema`, …)
/// 2. Value from the loaded config file (`[generate]` table)
/// 3. Built-in default (`./generated`, `schema.forge`, all targets)
use serde::Deserialize;
use std::path::Path;

use crate::error::{CliError, Result};

/// Generator-specific configuration (`[generate]` table in `forgedb.toml`).
#[derive(Debug, Deserialize, Default)]
pub struct GenerateConfig {
    /// Schema file path (default: `schema.forge`).
    pub schema: Option<String>,
    /// Output directory for generated files (default: `./generated`).
    pub output: Option<String>,
    /// Which targets to enable.  When absent all targets are enabled.
    /// Valid values: `rust`, `typescript`, `api`, `stubs`.
    pub targets: Option<Vec<String>>,
}

/// Multi-tenancy configuration (`[tenant]` table).  Physical, dir-per-tenant
/// isolation (#59): each tenant's data lives under `<root>/<tenant>/` and one
/// `forgedb serve` process serves exactly one tenant.  This table declares the
/// root the `forgedb tenant` CLI manages and generated processes open under.
#[derive(Debug, Deserialize, Default)]
pub struct TenantConfig {
    /// Root directory under which each tenant's data dir lives (default `./data`).
    pub root: Option<String>,
}

impl TenantConfig {
    /// The tenant root, defaulting to `./data`.
    pub fn root(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(self.root.as_deref().unwrap_or("data"))
    }
}

/// Authentication configuration (`[auth]` table) for the generated server's
/// verify-only JWT tenant guard (#59).  Consumed at runtime by the generated
/// process (via env, see the scaffold `main.rs`); this table is the declarative
/// source `forgedb serve` translates into that env.  **Never** read from a
/// `.forge` schema — auth is deployment config, the schema is data shape.
#[derive(Debug, Deserialize, Default)]
pub struct AuthConfig {
    /// Turn the JWT tenant guard on for the generated server.
    #[serde(default)]
    pub enabled: bool,
    /// Expected `iss` (issuer) claim.
    pub issuer: Option<String>,
    /// Expected `aud` (audience) claim.
    pub audience: Option<String>,
    /// Name of the claim carrying the tenant identity (default `tenant`).
    pub tenant_claim: Option<String>,
    /// JWKS endpoint URL (`.well-known/jwks.json`) to fetch verification keys.
    pub jwks_url: Option<String>,
    /// Path to a static PEM public key (alternative to `jwks_url`).
    pub public_key_path: Option<String>,
    /// Allowed signature algorithms (default `["RS256"]`).  Asymmetric only.
    pub algorithms: Option<Vec<String>>,
    /// Clock-skew leeway in seconds (default 60).
    pub leeway_secs: Option<u64>,
}

impl AuthConfig {
    /// The `FORGEDB_*` environment exports a generated process reads to build its
    /// verify-only JWT tenant guard, derived from this `[auth]` table.  Empty
    /// when auth is disabled.  This is the bridge between the declarative config
    /// and the process-per-tenant runtime (which is env-driven); `forgedb tenant
    /// create` prints these so an operator can copy the exact incantation.
    pub fn env_exports(&self) -> Vec<(String, String)> {
        if !self.enabled {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(path) = &self.public_key_path {
            out.push(("FORGEDB_JWT_PUBKEY".to_string(), path.clone()));
        }
        if let Some(url) = &self.jwks_url {
            out.push(("FORGEDB_JWKS_URL".to_string(), url.clone()));
        }
        if let Some(iss) = &self.issuer {
            out.push(("FORGEDB_JWT_ISSUER".to_string(), iss.clone()));
        }
        if let Some(aud) = &self.audience {
            out.push(("FORGEDB_JWT_AUDIENCE".to_string(), aud.clone()));
        }
        if let Some(claim) = &self.tenant_claim {
            out.push(("FORGEDB_TENANT_CLAIM".to_string(), claim.clone()));
        }
        if let Some(first) = self.algorithms.as_ref().and_then(|a| a.first()) {
            out.push(("FORGEDB_JWT_ALG".to_string(), first.clone()));
        }
        if let Some(leeway) = self.leeway_secs {
            out.push(("FORGEDB_JWT_LEEWAY".to_string(), leeway.to_string()));
        }
        out
    }
}

/// Top-level config struct.  Unknown top-level TOML tables are ignored.
#[derive(Debug, Deserialize, Default)]
pub struct ForgeConfig {
    #[serde(default)]
    pub generate: GenerateConfig,
    #[serde(default)]
    pub tenant: TenantConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

/// Load generator configuration.
///
/// * `path = Some(p)` — load from the explicit path; error if the file is
///   missing or contains invalid TOML.
/// * `path = None` — look for `forgedb.toml` in the current working directory;
///   return the default config silently if the file does not exist.
pub fn load_config(path: Option<&str>) -> Result<ForgeConfig> {
    match path {
        Some(explicit_path) => {
            let content = std::fs::read_to_string(explicit_path).map_err(|e| {
                CliError::Config(format!(
                    "Cannot read config file '{}': {}",
                    explicit_path, e
                ))
            })?;
            toml::from_str(&content).map_err(|e| {
                CliError::Config(format!(
                    "Invalid config file '{}': {}",
                    explicit_path, e
                ))
            })
        }
        None => {
            if Path::new("forgedb.toml").exists() {
                let content = std::fs::read_to_string("forgedb.toml")
                    .map_err(|e| CliError::Config(format!("Cannot read forgedb.toml: {}", e)))?;
                toml::from_str(&content)
                    .map_err(|e| CliError::Config(format!("Invalid forgedb.toml: {}", e)))
            } else {
                Ok(ForgeConfig::default())
            }
        }
    }
}
