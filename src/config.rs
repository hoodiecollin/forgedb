/// Compile-time generator configuration loaded from `forgedb.toml`.
///
/// **Unknown tables and unknown keys are errors** (#333).  Every table this
/// struct declares must be one `forgedb init` emits, and vice versa — the
/// scaffold has to parse against its own CLI.  Silently ignoring a key was the
/// worse failure even for the version skew it was meant to tolerate: a knob the
/// CLI does not know reads as applied and is not, so `fsync = "never"` quietly
/// stays `always` and the wrong bytes get generated with no signal.
///
/// Which config governs a schema is decided by [`crate::project`], not here:
/// the nearest `forgedb.toml` at or above the schema's directory, or an explicit
/// `--config` path, which is an outright override.  This module only parses.
///
/// Precedence (highest → lowest):
/// 1. Explicit CLI flag (`--output`, `--schema`, …)
/// 2. Value from the governing config file (`[generate]` table)
/// 3. Built-in default (`./generated`, all targets)
use serde::Deserialize;
use toml::Spanned;

use crate::error::{CliError, Result};

/// Project identity and grouping (`[project]` table in `forgedb.toml`) — #333.
///
/// A `forgedb.toml` does **not** declare an app; a `.forge` schema does.  This
/// table says only what an enclosing config cannot: whether the schemas beneath
/// it form a project of their own.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// Explicit project id.  Only meaningful at the project root — declaring it
    /// at a nested, non-isolated config is a positioned error, because it reads
    /// as authoritative and is not.  Spanned so that error can name its line.
    pub name: Option<Spanned<String>>,
    /// Inert metadata.  Deliberately has **no** effect on generation or on
    /// identity: identity is `name` + root path, and giving a version a role
    /// would create a second axis on which two roots could collide.
    pub version: Option<String>,
    /// Whether the schemas beneath this config form their own project.
    ///
    /// **Absent means `true`, inverting `bool`'s own default on purpose.** Read
    /// naturally, an absent `isolated` would be "not isolated" — grouped — so a
    /// monorepo root config added for an unrelated reason would silently absorb
    /// every app beneath it into one project and one lockfile.  A config that
    /// never heard of grouping is not in a group; grouping stays opt-in, and
    /// `forgedb init` always writes the field explicitly.
    #[serde(default = "isolated_default")]
    pub isolated: bool,
}

fn isolated_default() -> bool {
    true
}

/// Hand-written, **not** derived.
///
/// `ForgeConfig::project` carries `#[serde(default)]`, so a config with no
/// `[project]` table at all is built from this impl rather than from
/// `isolated_default` — and a derived `Default` would give `isolated = false`
/// there, quietly regrouping every app whose config predates the field. The two
/// defaults have to agree, and only one of them is reachable from the attribute.
impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            name: None,
            version: None,
            isolated: isolated_default(),
        }
    }
}

impl ProjectConfig {
    /// The declared project name, if any.
    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().map(|n| n.as_ref().as_str())
    }
}

/// Generator-specific configuration (`[generate]` table in `forgedb.toml`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateConfig {
    /// **Removed in #333.**  Declared only so the key stays *recognized and
    /// rejected*: under `deny_unknown_fields` it would otherwise report
    /// `unknown field 'schema'`, which is true and useless.  A config governs
    /// every schema beneath it, so naming one is a category error — and it
    /// admitted a real loop (read config A to find the schema, then discover
    /// config B is that schema's nearest ancestor).  See [`removal_error`].
    pub schema: Option<Spanned<toml::Value>>,
    /// Output directory for generated files (default: `./generated`).
    ///
    /// Relative paths resolve against **the schema's** directory, not the
    /// config's: under a root config governing several apps, config-relative
    /// would send every app's `output = "generated"` to the same directory and
    /// silently interleave their generated code.  Schema-relative makes this a
    /// per-app pattern instead.
    pub output: Option<String>,
    /// Which targets to enable.  When absent all targets are enabled.
    /// Valid values: `rust`, `typescript`, `api`, `stubs`.
    pub targets: Option<Vec<String>>,
}

/// The built-in default is a **stated value**, not an absence (#335 §12).
///
/// This is what makes "required" mean the right thing.  With `targets` written
/// here rather than left `None`:
///
/// * **no `forgedb.toml` at all** — the fallback config — declares `["all"]`,
///   so a project that never wrote a config keeps working exactly as before;
/// * **a config file with no `[generate]` table** takes this same default,
///   because it has declared nothing about generation;
/// * **a config file WITH a `[generate]` table but no `targets` key** leaves the
///   field `None` (serde's default for an `Option` field) and is **refused**.
///
/// That last case is the one worth refusing: a half-declared table where the
/// most consequential key is left to be guessed.  The distinction costs one
/// hand-written `Default` and removes the "absent means everything" reading that
/// #335 §12 identifies as the same class of defect as #333's
/// `ProjectConfig::default()`.
impl Default for GenerateConfig {
    fn default() -> Self {
        Self {
            schema: None,
            output: None,
            targets: Some(vec![crate::targets::DEFAULT_TARGETS.to_string()]),
        }
    }
}

/// Multi-tenancy configuration (`[tenant]` table).  Physical, dir-per-tenant
/// isolation (#59): each tenant's data lives under `<root>/<tenant>/` and one
/// `forgedb serve` process serves exactly one tenant.  This table declares the
/// root the `forgedb tenant` CLI manages and generated processes open under.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
    /// JWKS re-fetch interval in seconds for key rotation (#81, default 300).
    pub jwks_refresh_secs: Option<u64>,
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
        // Emit the FULL algorithm allowlist (#147): the substrate accepts a
        // `Vec<Algorithm>`, so the config must not collapse it to one. Comma-join
        // into FORGEDB_JWT_ALGS, which the scaffold parses back into the vec.
        if let Some(algs) = self.algorithms.as_ref().filter(|a| !a.is_empty()) {
            out.push(("FORGEDB_JWT_ALGS".to_string(), algs.join(",")));
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
#[serde(deny_unknown_fields)]
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
    /// Durable replication-log retention: prune the broker log to the last N
    /// offsets in `maintain()` (#137). Default 0 = keep the full log. Only takes
    /// effect when `replication` is on.
    pub replication_log_retention: Option<u64>,
}

/// Generate-time **storage/durability** knobs (`[storage]` table, epic #126).
/// Schema-blind, baked at generate time (Tier A/B). Defaults are byte-identical
/// to today's output.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct WasmConfig {
    /// Browser read-replica auto-commit debounce, ms (#148). Default 250.
    pub commit_debounce_ms: Option<u64>,
    /// Browser read-replica auto-commit frame ceiling (#148). Default 100.
    pub commit_max_frames: Option<u64>,
}

/// Top-level config struct.  An unknown top-level table is an error (#333) —
/// a misspelled `[projekt]` would otherwise be ignored, identity would fall
/// through to manifest detection or the path hash, and two projects could
/// silently merge through the very mechanism meant to be a compatibility
/// feature.
///
/// Every table `forgedb init` emits must appear here, or the scaffold fails
/// against its own CLI; `scaffolded_config_parses_strictly` is that guard.
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
        // #335 §10. Decided from the app's DECLARED target set, never from what a
        // single invocation emitted — otherwise `generate rust` and `generate all`
        // would bake different `database.rs` for the same project. Resolution
        // failures fall back to ON, which is today's behavior: a spurious utoipa
        // dependency is inert, while a missing derive is a compile error in
        // `server` that the orphan rule makes unfixable downstream.
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

/// The file name a config is always stored under.
pub const CONFIG_FILE: &str = "forgedb.toml";

/// 1-based line and column of a byte offset in `content`.
///
/// `toml` reports spans as byte offsets; every positioned diagnostic in this
/// crate goes through here so they all count lines the same way.
pub fn line_col(content: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(content.len());
    let before = &content[..clamped];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// The position of the **key** on the line a value's span points into.
///
/// `Spanned` reports where the *value* starts, so `name = "api"` positions at
/// column 8 — past the thing the reader has to change. Every diagnostic here is
/// about a key, so they all point at the start of its line instead.
pub fn key_position(content: &str, value_offset: usize) -> (usize, usize) {
    let (line, _) = line_col(content, value_offset);
    let indent = content
        .lines()
        .nth(line.saturating_sub(1))
        .map_or(0, |l| l.chars().take_while(|c| c.is_whitespace()).count());
    (line, indent + 1)
}

/// Parse a `forgedb.toml` from its text, applying the diagnostics that
/// `deny_unknown_fields` alone cannot produce.
///
/// `path` is used only for the message; nothing is read from disk here, which is
/// what lets the strict-parsing tests run without fixtures.
pub fn parse_config(content: &str, path: &std::path::Path) -> Result<ForgeConfig> {
    let config: ForgeConfig = toml::from_str(content).map_err(|e| {
        // The library already emits `TOML parse error at line L, column C`, a
        // caret snippet, the offending key AND the expected set.  Wrapping that
        // in `Configuration error:` would only bury it, so it is rendered
        // verbatim under the file it came from.
        CliError::ConfigDiagnostic(format!("{}:\n{}", path.display(), e))
    })?;

    if let Some(removed) = &config.generate.schema {
        return Err(removal_error(content, path, removed.span().start));
    }

    // `[generate].targets` is REQUIRED in a config file (#335 §12, decision 2).
    //
    // It used to be optional, where ABSENT MEANT EVERY TARGET — the inverse of
    // what an empty list normally reads as, and the same class of defect as
    // #333's `ProjectConfig::default()`.  #335 defines the package prune against
    // the *declared* set, so an absent value would collapse that set to whatever
    // a single invocation happened to select: `forgedb generate rust` on a
    // default project would prune away the server, the bindings and the replica.
    //
    // A project with NO config file at all is a different case and is not an
    // error — it takes the built-in `["all"]`, which is a value like any other
    // (see `crate::targets::DEFAULT_TARGETS`).  What is refused is a config file
    // that declares some of `[generate]` and leaves this key to be guessed.
    if config.generate.targets.is_none() {
        return Err(missing_targets_error(content, path));
    }

    Ok(config)
}

impl ForgeConfig {
    /// The declared target set, as canonical internal names, plus any
    /// deprecation warnings the caller should print.
    ///
    /// The one door onto `[generate].targets` — both `generate` and `build` come
    /// through here, so neither can see the raw user spellings and neither can
    /// invent its own reading of an absent value. `build` used to pass
    /// `config_targets: None` outright (#335 §12), which made every opt-in arm
    /// of `generate_all` unreachable from `forgedb build`.
    pub fn resolved_targets(&self) -> Result<(Vec<String>, Vec<String>)> {
        let declared = self.generate.targets.as_deref().unwrap_or(&[]);
        crate::targets::resolve_all(declared)
    }
}

/// The diagnostic for an absent `[generate].targets` (#335 §12).
///
/// Anchored on the `[generate]` table header when there is one, so the caret
/// lands where the key belongs rather than at the top of the file.
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

/// The bespoke diagnostic for the removed `[generate].schema` key (#333 §10).
///
/// `deny_unknown_fields` would report `unknown field 'schema'` — true, and
/// useless.  A removal the user cannot act on is a worse break than the removal
/// itself, so the key stays recognized and this names what replaced it.
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

/// Read and parse a config file from an explicit path.
///
/// Every config read in the CLI comes through here or [`parse_config`], and both
/// are reached only from [`crate::project`].  That is a greppable property, and
/// it is the invariant #361 established: two loaders means two identities, and a
/// single invocation served by two of them mis-keys the build cache silently.
pub fn load_config_file(path: &std::path::Path) -> Result<ForgeConfig> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Config(format!("Cannot read config file '{}': {}", path.display(), e))
    })?;
    parse_config(&content, path)
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
                // No `[generate]` table in this fixture, so the built-in
                // `["all"]` applies and `all` expands to include `api` (#335 §12).
                web: true,
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
