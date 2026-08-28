use serde::Deserialize;
use toml::Spanned;

use crate::error::{CliError, Result};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub id: Option<Spanned<String>>,
    pub version: Option<String>,
    #[serde(default = "isolated_default")]
    pub isolated: bool,
    #[serde(default)]
    pub symbol_naming: crate::naming::SymbolNaming,
}

fn isolated_default() -> bool {
    true
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            id: None,
            version: None,
            isolated: isolated_default(),
            symbol_naming: crate::naming::SymbolNaming::default(),
        }
    }
}

impl ProjectConfig {
    pub fn id(&self) -> Option<&str> {
        self.id.as_ref().map(|n| n.as_ref().as_str())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateConfig {
    pub schema: Option<Spanned<toml::Value>>,
    pub output: Option<String>,
    pub targets: Option<Vec<String>>,
}

impl Default for GenerateConfig {
    fn default() -> Self {
        Self {
            schema: None,
            output: None,
            targets: Some(vec![crate::targets::DEFAULT_TARGETS.to_string()]),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TenantConfig {
    pub root: Option<String>,
}

impl TenantConfig {
    pub fn root(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(self.root.as_deref().unwrap_or("data"))
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default)]
    pub enabled: bool,
    pub issuer: Option<String>,
    pub audience: Option<String>,
    pub tenant_claim: Option<String>,
    pub jwks_url: Option<String>,
    pub jwks_refresh_secs: Option<u64>,
    pub public_key_path: Option<String>,
    pub algorithms: Option<Vec<String>>,
    pub leeway_secs: Option<u64>,
}

impl AuthConfig {
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
        if let Some(secs) = self.jwks_refresh_secs {
            out.push(("FORGEDB_JWKS_REFRESH_SECS".to_string(), secs.to_string()));
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
        if let Some(algs) = self.algorithms.as_ref().filter(|a| !a.is_empty()) {
            out.push(("FORGEDB_JWT_ALGS".to_string(), algs.join(",")));
        }
        if let Some(leeway) = self.leeway_secs {
            out.push(("FORGEDB_JWT_LEEWAY".to_string(), leeway.to_string()));
        }
        out
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub replication: bool,
    pub changefeed_capacity: Option<usize>,
    pub max_cascade_depth: Option<u32>,
    pub replication_log_retention: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    pub fsync: Option<String>,
    pub wal_checkpoint_interval: Option<u64>,
    pub compaction: Option<bool>,
    pub compaction_threshold: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TransactionConfig {
    pub max_retries: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub page_default_limit: Option<usize>,
    pub page_max_limit: Option<usize>,
    pub metrics: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WasmConfig {
    pub commit_debounce_ms: Option<u64>,
    pub commit_max_frames: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PlacementConfig {
    pub rust_package: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ToolchainConfig {
    pub bun: Option<InterpreterConfig>,
    pub node: Option<InterpreterConfig>,
    pub python: Option<InterpreterConfig>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct InterpreterConfig {
    pub path: Option<String>,
    pub min_version: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ForgeConfig {
    #[serde(default)]
    pub project: ProjectConfig,
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
    #[serde(default)]
    pub placement: PlacementConfig,
    #[serde(default)]
    pub toolchain: ToolchainConfig,
}

impl ForgeConfig {
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
        let web = self
            .resolved_targets()
            .map(|(targets, _)| targets.iter().any(|t| t == "api"))
            .unwrap_or(true);

        Ok(forgedb_codegen::GenConfig {
            replication: self.runtime.replication,
            web,
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
            replication_log_retention: self
                .runtime
                .replication_log_retention
                .unwrap_or(d.replication_log_retention),
        })
    }
}

pub const CONFIG_FILE: &str = "forgedb.toml";

pub fn line_col(content: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(content.len());
    let before = &content[..clamped];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

pub fn key_position(content: &str, value_offset: usize) -> (usize, usize) {
    let (line, _) = line_col(content, value_offset);
    let indent = content
        .lines()
        .nth(line.saturating_sub(1))
        .map_or(0, |l| l.chars().take_while(|c| c.is_whitespace()).count());
    (line, indent + 1)
}

pub fn parse_config(content: &str, path: &std::path::Path) -> Result<ForgeConfig> {
    let config: ForgeConfig = toml::from_str(content).map_err(|e| {
        CliError::ConfigDiagnostic(format!("{}:\n{}", path.display(), e))
    })?;

    if let Some(removed) = &config.generate.schema {
        return Err(removal_error(content, path, removed.span().start));
    }

    if config.generate.targets.is_none() {
        return Err(missing_targets_error(content, path));
    }

    Ok(config)
}

impl ForgeConfig {
    pub fn resolved_targets(&self) -> Result<(Vec<String>, Vec<String>)> {
        let declared = self.generate.targets.as_deref().unwrap_or(&[]);
        crate::targets::resolve_all(declared)
    }
}

fn missing_targets_error(content: &str, path: &std::path::Path) -> CliError {
    let (line, column) = content
        .find("[generate]")
        .map(|offset| key_position(content, offset))
        .unwrap_or((1, 1));

    CliError::ConfigDiagnostic(format!(
        "{}:{}:{}: `[generate].targets` is required.\n\n\
         An absent value used to mean \"every target\", which is the opposite of \
         what an absent list normally means — and it leaves the set of packages \
         to build undefined.\n\n\
         To keep exactly today's behavior, write:\n\n\
         \x20   [generate]\n\
         \x20   targets = [\"all\"]\n\n\
         Legal values:\n{}",
        path.display(),
        line,
        column,
        crate::targets::legal_list()
    ))
}

fn removal_error(content: &str, path: &std::path::Path, offset: usize) -> CliError {
    let (line, column) = key_position(content, offset);
    CliError::ConfigDiagnostic(format!(
        "{}:{}:{}: `[generate].schema` was removed.\n\n\
         A config governs every schema beneath it, so it cannot name one: the \
         same file may be the nearest ancestor of several apps. It also admitted \
         a loop — the config that named a schema need not be the config that \
         governs it.\n\n\
         The schema is named by the invocation or found beside you: pass \
         `--schema <path>`, or leave it out and keep the schema next to this file.",
        path.display(),
        line,
        column
    ))
}

pub fn load_config_file(path: &std::path::Path) -> Result<ForgeConfig> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Config(format!("Cannot read config file '{}': {}", path.display(), e))
    })?;
    parse_config(&content, path)
}

pub fn write_checked(path: &std::path::Path, content: &str) -> Result<()> {
    parse_config(content, path)?;
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(dir)?;
    let temp = dir.join(format!(
        ".{}.forgedb-tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("config")
    ));
    std::fs::write(&temp, content)?;
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            Err(e.into())
        }
    }
}

pub fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_codegen::{FsyncMode, GenConfig};

    #[test]
    fn empty_config_maps_to_default_gen_config() {
        let cfg: ForgeConfig = toml::from_str("[project]\nid = \"x\"\n").unwrap();
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
replication_log_retention = 4096

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
                replication_log_retention: 4096,
                web: true,
            }
        );
    }

    #[test]
    fn partial_storage_keeps_other_defaults() {
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
