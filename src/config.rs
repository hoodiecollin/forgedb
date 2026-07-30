/// Compile-time generator configuration loaded from `forgedb.toml`.
///
/// The generator consumes the `[generate]`, `[tenant]`, `[auth]`, `[runtime]`,
/// and `[storage]` tables; any other table present in a project's `forgedb.toml`
/// (e.g. `[project]`) is silently ignored, so this struct is forward-compatible
/// with the full config file emitted by `forgedb init`.
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

/// Generate-time **runtime-behavior** knobs (`[runtime]` table, epic #126).
///
/// Schema-blind (litmus: two apps with entirely different schemas could share
/// these verbatim with identical effect) — they select among behaviors the
/// generated/substrate code already contains and are baked into the emitted
/// `database.rs` at generate time (Tier A/B). **Never** read from a `.forge`
/// schema. Absent values reproduce today's output byte-for-byte, except
/// `replication` (default OFF — the #130 sanctioned exception).
#[derive(Debug, Deserialize, Default)]
pub struct RuntimeConfig {
    /// Attach the durable replication broker (#130, Tier A). Default `false`:
    /// an unused broker's second `F_FULLFSYNC` per write is pure waste. Turn on
    /// for apps consuming `/replicate` or running a browser read-replica.
    #[serde(default)]
    pub replication: bool,
    /// In-process changefeed broadcast capacity + durable-broker buffer (#135).
    /// Default 1024.
    pub changefeed_capacity: Option<usize>,
    /// Maximum `@on_delete(cascade)` recursion depth (#150). Default 64.
    pub max_cascade_depth: Option<u32>,
}

/// Generate-time **storage/durability** knobs (`[storage]` table, epic #126).
/// Schema-blind, baked at generate time (Tier A/B). Defaults are byte-identical
/// to today's output.
#[derive(Debug, Deserialize, Default)]
pub struct StorageConfig {
    /// WAL fsync policy (#129): `"always"` (default, crash-safe) or `"never"`
    /// (durability-weakening opt-in — a real data-loss window on power loss).
    pub fsync: Option<String>,
    /// Mutations per collection between WAL checkpoints (#131). Default 1000.
    pub wal_checkpoint_interval: Option<u64>,
    /// Emit the in-process auto-compaction trigger (#134, Tier A). Default
    /// `true`; when `false` the auto-trigger is omitted and reclaim is driven
    /// only by the explicit `Database::compact()`.
    pub compaction: Option<bool>,
    /// Dead row versions per model before auto-compaction fires (#133). Default
    /// 1000.
    pub compaction_threshold: Option<u64>,
}

/// Generate-time **transaction** knobs (`[transaction]` table, epic #126).
/// Schema-blind, baked at generate time (Tier B). Defaults are byte-identical
/// to today's output.
#[derive(Debug, Deserialize, Default)]
pub struct TransactionConfig {
    /// Default retry count for `transaction_optimistic` (#146). Default 3.
    /// Per-call control stays available via `transaction_retrying(retries, f)`.
    pub max_retries: Option<u32>,
}

/// Generate-time **server/transport** knobs (`[server]` table, epic #126).
/// Schema-blind, baked at generate time (Tier B). Defaults are byte-identical
/// to today's output. (The generated server's host/port and shutdown drain are
/// separate DEPLOY-environment knobs read from env at process start, not baked.)
#[derive(Debug, Deserialize, Default)]
pub struct ServerConfig {
    /// List-endpoint page size when the client omits `?limit` (#141). Default 50.
    pub page_default_limit: Option<usize>,
    /// Maximum list-endpoint page size; a larger `?limit` is clamped (#141).
    /// Default 1000.
    pub page_max_limit: Option<usize>,
    /// Emit the unauthenticated `/metrics` endpoint (#151). Default `true`; set
    /// `false` to omit the metrics handler + route entirely (Tier A).
    pub metrics: Option<bool>,
}

/// Generate-time **wasm read-replica** knobs (`[wasm]` table, epic #126).
/// Schema-blind, substituted into the static Worker bootstrap at generate time
/// (Tier B). Defaults are byte-identical to today's output.
#[derive(Debug, Deserialize, Default)]
pub struct WasmConfig {
    /// Browser read-replica auto-commit debounce, ms (#148). Default 250.
    pub commit_debounce_ms: Option<u64>,
    /// Browser read-replica auto-commit frame ceiling (#148). Default 100.
    pub commit_max_frames: Option<u64>,
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
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub transaction: TransactionConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub wasm: WasmConfig,
}

impl ForgeConfig {
    /// Resolve the generate-time runtime-behavior knobs (#126) into the codegen
    /// `GenConfig` baked into `database.rs`. Absent values fall back to
    /// `GenConfig::DEFAULT` (today's output, byte-identical). Returns a config
    /// error if a knob has an out-of-range value (e.g. an unknown `fsync` mode).
    pub fn gen_config(&self) -> Result<forgedb_codegen::GenConfig> {
        let d = forgedb_codegen::GenConfig::DEFAULT;
        let fsync = match self.storage.fsync.as_deref() {
            None => d.fsync,
            Some("always") => forgedb_codegen::FsyncMode::Always,
            Some("never") => forgedb_codegen::FsyncMode::Never,
            Some(other) => {
                return Err(CliError::Config(format!(
                    "invalid [storage].fsync = \"{other}\" — expected \"always\" or \"never\""
                )));
            }
        };
        Ok(forgedb_codegen::GenConfig {
            replication: self.runtime.replication,
            fsync,
            wal_checkpoint_interval: self
                .storage
                .wal_checkpoint_interval
                .unwrap_or(d.wal_checkpoint_interval),
            compaction: self.storage.compaction.unwrap_or(d.compaction),
            compaction_threshold: self
                .storage
                .compaction_threshold
                .unwrap_or(d.compaction_threshold),
            changefeed_capacity: self
                .runtime
                .changefeed_capacity
                .unwrap_or(d.changefeed_capacity),
            max_cascade_depth: self.runtime.max_cascade_depth.unwrap_or(d.max_cascade_depth),
            txn_max_retries: self.transaction.max_retries.unwrap_or(d.txn_max_retries),
            page_default_limit: self
                .server
                .page_default_limit
                .unwrap_or(d.page_default_limit),
            page_max_limit: self.server.page_max_limit.unwrap_or(d.page_max_limit),
            metrics: self.server.metrics.unwrap_or(d.metrics),
            wasm_commit_debounce_ms: self
                .wasm
                .commit_debounce_ms
                .unwrap_or(d.wasm_commit_debounce_ms),
            wasm_commit_max_frames: self
                .wasm
                .commit_max_frames
                .unwrap_or(d.wasm_commit_max_frames),
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_codegen::{FsyncMode, GenConfig};

    #[test]
    fn empty_config_maps_to_default_gen_config() {
        // No [runtime]/[storage] tables → GenConfig::DEFAULT (byte-identical
        // output, replication OFF).
        let cfg: ForgeConfig = toml::from_str("[project]\nname = \"x\"\n").unwrap();
        assert_eq!(cfg.gen_config().unwrap(), GenConfig::DEFAULT);
        assert!(!cfg.runtime.replication);
    }

    #[test]
    fn runtime_and_storage_knobs_map_through() {
        let src = r#"
[runtime]
replication = true
changefeed_capacity = 256
max_cascade_depth = 16

[storage]
fsync = "never"
wal_checkpoint_interval = 500
compaction = false
compaction_threshold = 250

[transaction]
max_retries = 7

[server]
page_default_limit = 25
page_max_limit = 500
metrics = false

[wasm]
commit_debounce_ms = 500
commit_max_frames = 40
"#;
        let cfg: ForgeConfig = toml::from_str(src).unwrap();
        let gc = cfg.gen_config().unwrap();
        assert_eq!(
            gc,
            GenConfig {
                replication: true,
                fsync: FsyncMode::Never,
                wal_checkpoint_interval: 500,
                compaction: false,
                compaction_threshold: 250,
                changefeed_capacity: 256,
                max_cascade_depth: 16,
                txn_max_retries: 7,
                page_default_limit: 25,
                page_max_limit: 500,
                metrics: false,
                wasm_commit_debounce_ms: 500,
                wasm_commit_max_frames: 40,
            }
        );
    }

    #[test]
    fn partial_storage_keeps_other_defaults() {
        // Only fsync set → every other knob stays at its default.
        let cfg: ForgeConfig = toml::from_str("[storage]\nfsync = \"always\"\n").unwrap();
        let gc = cfg.gen_config().unwrap();
        assert_eq!(gc, GenConfig::DEFAULT);
    }

    #[test]
    fn invalid_fsync_is_a_config_error() {
        let cfg: ForgeConfig = toml::from_str("[storage]\nfsync = \"sometimes\"\n").unwrap();
        let err = cfg.gen_config().unwrap_err();
        assert!(matches!(err, CliError::Config(_)), "unknown fsync mode → config error");
    }
}
