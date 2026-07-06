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

/// Top-level config struct.  Unknown top-level TOML tables are ignored.
#[derive(Debug, Deserialize, Default)]
pub struct ForgeConfig {
    #[serde(default)]
    pub generate: GenerateConfig,
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
