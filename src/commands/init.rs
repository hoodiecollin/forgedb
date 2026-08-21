use crate::{error::CliError, project, templates, ui, Result};
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub project_name: String,
    pub template: Option<String>,
    pub rust: bool,
    pub api_only: bool,
    /// `--isolated` / `--no-isolated`.  `None` means "decide from what is
    /// above me", which is the whole reason this is three-valued rather than a
    /// plain flag: the useful default depends on the tree.
    pub isolated: Option<bool>,
}

pub fn run(options: InitOptions) -> Result<()> {
    ui::header("✨", &format!("Creating project: {}", options.project_name));

    // Check if project directory already exists
    let project_path = Path::new(&options.project_name);
    if project_path.exists() {
        return Err(CliError::ProjectExists(options.project_name.clone()));
    }

    // Scaffolding *inside* an existing project is the normal case, not an error
    // (#333 §6/§7) — a new schema under an existing project is exactly what
    // adding an app looks like. So `init` asks nothing and simply reports what
    // it found, then records the answer explicitly.
    let isolated = resolve_isolated(&options)?;
    let project_id = derive_project_id(&options.project_name);
    refuse_a_taken_name(&project_id, isolated)?;

    // Create project directory structure
    create_project_structure(&options)?;

    // Create schema file based on template
    create_schema_file(&options)?;

    // Create config file
    create_config_file(&options, &project_id, isolated)?;

    // Create .gitignore
    create_gitignore(&options)?;

    // Create README
    create_readme(&options)?;

    // Create Rust files if needed
    if options.rust || !options.api_only {
        create_rust_files(&options)?;
        // The blessed container deploy path (Phase 5) targets the generated
        // Rust server, so it rides along with the Rust scaffold.
        create_deploy_files(&options)?;
    }

    ui::success("Done! Run the following to get started:");
    println!();
    println!("  cd {}", options.project_name);
    println!("  forgedb generate rust");
    println!("  forgedb build");
    println!();

    Ok(())
}

fn create_project_structure(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);

    // Create main directories
    fs::create_dir_all(project_path)?;
    fs::create_dir_all(project_path.join("src"))?;
    fs::create_dir_all(project_path.join("generated"))?;
    fs::create_dir_all(project_path.join("data/db"))?;
    fs::create_dir_all(project_path.join("data/wal"))?;

    ui::success("Created project directory structure");
    Ok(())
}

fn create_schema_file(options: &InitOptions) -> Result<()> {
    let schema_content = match options.template.as_deref() {
        Some("blog") => templates::blog_schema(),
        Some("ecommerce") => templates::ecommerce_schema(),
        Some("todo") => templates::todo_schema(),
        Some("blank") | None => templates::blank_schema(),
        Some(t) => {
            ui::warning(&format!("Unknown template '{}', using blank", t));
            templates::blank_schema()
        }
    };

    let schema_path = Path::new(&options.project_name).join("schema.forge");
    fs::write(schema_path, schema_content)?;

    ui::step("📄", "Created schema.forge");
    Ok(())
}

/// Decide whether the new project stands alone, and say why.
///
/// The flags are the complete contract; the report is the only thing that would
/// otherwise need a prompt, and it is one-way. Nothing here blocks on a TTY.
fn resolve_isolated(options: &InitOptions) -> Result<bool> {
    if let Some(explicit) = options.isolated {
        return Ok(explicit);
    }

    // Walk from where the project will be created — the new directory does not
    // exist yet, so start at its parent. `forgedb init apps/api` must see what is
    // above `apps/`, not only what is above the CWD.
    let parent = Path::new(&options.project_name).parent().unwrap_or(Path::new(""));
    let from = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let chain = project::Chain::walk(from)?;
    match chain.project_root() {
        Some(_) => {
            let id = project::identify(&chain)?;
            ui::info(&format!(
                "Joining the enclosing project {:?} (rooted at {}). \
                 Pass --isolated to stand alone instead.",
                id.name,
                id.root.display()
            ));
            Ok(false)
        }
        None => Ok(true),
    }
}

/// C12: report a taken project id here, where the name is being chosen, rather
/// than at the first `generate` — by which point the user has a scaffolded tree
/// whose name they now have to change.
///
/// Only meaningful for a project root: a config that joins an enclosing project
/// declares no name of its own, so it cannot collide.
fn refuse_a_taken_name(project_name: &str, isolated: bool) -> Result<()> {
    if !isolated {
        return Ok(());
    }
    if let Some(held_by) = project::held_by(project_name)? {
        return Err(CliError::Config(format!(
            "Project name {project_name:?} is already claimed by {}.\n\n\
             Two projects sharing an id would share one build cache, one lockfile \
             and one target directory. Pick a different name, or pass \
             --no-isolated to join an enclosing project instead.",
            held_by.display()
        )));
    }
    Ok(())
}

/// The project id for a scaffold: the **last component** of the path, not the
/// path.
///
/// `forgedb init ./apps/api` should name the project `api`. Writing the whole
/// argument was harmless while `[project].name` was ignored; since #333 the name
/// is used verbatim as a directory under `~/.forgedb`, so a path there is either
/// rejected or escapes the cache.
fn derive_project_id(project_name: &str) -> String {
    Path::new(project_name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| project_name.to_string())
}

fn create_config_file(options: &InitOptions, project_id: &str, isolated: bool) -> Result<()> {
    let config_content = templates::default_config(project_id, isolated);
    let config_path = Path::new(&options.project_name).join("forgedb.toml");
    fs::write(config_path, config_content)?;

    ui::step("⚙️", "Created forgedb.toml");
    Ok(())
}

fn create_gitignore(options: &InitOptions) -> Result<()> {
    let gitignore_path = Path::new(&options.project_name).join(".gitignore");
    fs::write(gitignore_path, templates::default_gitignore())?;

    ui::step("📝", "Created .gitignore");
    Ok(())
}

fn create_readme(options: &InitOptions) -> Result<()> {
    let readme_content = templates::readme_template(&options.project_name);
    let readme_path = Path::new(&options.project_name).join("README.md");
    fs::write(readme_path, readme_content)?;

    ui::step("📖", "Created README.md");
    Ok(())
}

fn create_rust_files(options: &InitOptions) -> Result<()> {
    // Create Cargo.toml with all dependencies required by generated code.
    //
    // The generated `database.rs` needs: forgedb-storage, forgedb-types, serde,
    // utoipa (with uuid feature for ToSchema on Uuid/Timestamp fields), plus
    // forgedb-changefeed for the change-feed emits (#62 Direction A),
    // forgedb-wal for the durable write path (#89 — WAL commit + crash recovery),
    // and forgedb-compaction for in-process auto-compaction (#92 — schema-agnostic
    // dead-row reclaim keyed by dir name; the trigger + reindex are generated).
    //
    // The generated `api.rs` needs: axum (with the `ws` feature for the
    // change-feed subscription endpoints), utoipa-axum, tokio (full), serde_json,
    // forgedb-query-params for list-endpoint filter/sort/paginate (#90 — the
    // query string is parsed by this schema-agnostic substrate; all field-aware
    // filtering/sorting is generated per-model), and tower-http (trace feature)
    // for the request-logging layer on the generated router (Phase 5
    // observability).  The scaffold `main.rs` installs a `tracing-subscriber`
    // (env-filtered via `RUST_LOG`) so those spans are emitted.
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedb-storage = "0.3"
forgedb-types = "0.3"
forgedb-changefeed = "0.2"
forgedb-wal = "0.2"
forgedb-auth = {{ version = "0.2", features = ["jwks-http"] }}
forgedb-query-params = "0.1"
forgedb-compaction = "0.1"
forgedb-txn = "0.1"
forgedb-coordinator = "0.2"
regex = "1"
rust_decimal = {{ version = "1", features = ["serde-with-str"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
utoipa = {{ version = "5", features = ["uuid"] }}
utoipa-axum = "0.2"
axum = {{ version = "0.8", features = ["ws"] }}
tokio = {{ version = "1", features = ["full"] }}
tower-http = {{ version = "0.6", features = ["trace", "cors"] }}
tracing = "0.1"
tracing-subscriber = {{ version = "0.3", features = ["env-filter", "json"] }}
"#,
        options.project_name
    );

    let cargo_path = Path::new(&options.project_name).join("Cargo.toml");
    fs::write(cargo_path, cargo_toml)?;

    // Create src/main.rs — a real, env-driven, process-per-tenant server (#59).
    //
    // The generated files are `#[path]` modules (they carry inner `#![allow]` /
    // `//!` docs, illegal inside `include!`ed inline `mod { }`, E0753). `api.rs`
    // refers to the model types as `super::*`, so the crate root re-exports
    // `database::*`.
    // ONE definition of the generated server body, shared with the cache
    // `server/` package (#335 §1). Only the module preamble differs: this
    // scaffold reaches the generated files through `#[path]` modules, while the
    // cache package links `core` as a cargo dependency. A copy here would be
    // two emitters of one artifact — the exact drift #335 exists to end.
    let main_rs = forgedb_codegen::ServerPackage::main_rs(
        forgedb_codegen::ServerLayout::InTree,
    );

    let main_rs_path = Path::new(&options.project_name).join("src").join("main.rs");
    fs::write(main_rs_path, main_rs)?;

    ui::step("🦀", "Created Rust project files");
    ui::info("Run 'forgedb generate rust' to generate the database code");
    Ok(())
}

/// Emit the blessed container deploy path (Phase 5): a multi-stage
/// `Dockerfile`, a `.dockerignore`, and a `docker-compose.yml`.  The image builds
/// the generated Rust server and runs it as a non-root user with `/data` on a
/// volume, `FORGEDB_HOST=0.0.0.0`, and a `HEALTHCHECK` against the generated
/// `/health` endpoint (Phase 5).  None of this reads `schema.forge` at
/// runtime — it is ops packaging around the already-generated server binary.
fn create_deploy_files(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);
    let bin = &options.project_name;

    // Multi-stage: build with the full Rust toolchain, run on a slim base.
    // `forgedb generate` must have produced ./generated/{database,api}.rs into the
    // build context first (documented); committing Cargo.lock makes builds
    // reproducible (it is copied when present).
    let dockerfile = format!(
        r#"# syntax=docker/dockerfile:1
# ForgeDB generated-server image (Phase 5 deploy path).
#
# Build context expects the generated code present:
#   forgedb generate all --output ./generated
#   docker build -t {bin} .

FROM rust:1-slim AS builder
WORKDIR /build
# Manifests first for dependency-layer caching, then sources.
COPY Cargo.toml ./
COPY src ./src
COPY generated ./generated
RUN cargo build --release --locked || cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 forgedb
WORKDIR /app
COPY --from=builder /build/target/release/{bin} /usr/local/bin/forgedb-server

# Config comes from the environment (12-factor). Data lives on a mounted volume —
# never baked into the image.
ENV FORGEDB_HOST=0.0.0.0 \
    FORGEDB_PORT=3000 \
    FORGEDB_DATA=/data \
    RUST_LOG=info
RUN mkdir -p /data && chown forgedb:forgedb /data
VOLUME ["/data"]
USER forgedb
EXPOSE 3000

# Liveness against the generated /health endpoint (Phase 5).
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 \
    CMD curl -fsS http://localhost:3000/health || exit 1

CMD ["forgedb-server"]
"#
    );
    fs::write(project_path.join("Dockerfile"), dockerfile)?;

    let dockerignore = "\
target/
data/
.git/
node_modules/
**/*.rs.bk
";
    fs::write(project_path.join(".dockerignore"), dockerignore)?;

    let compose = format!(
        r#"# ForgeDB generated-server compose file (Phase 5).
#   docker compose up --build
services:
  {bin}:
    build: .
    ports:
      - "3000:3000"
    environment:
      FORGEDB_HOST: 0.0.0.0
      FORGEDB_PORT: "3000"
      FORGEDB_DATA: /data
      RUST_LOG: info
      # Machine-parseable JSON logs for a log aggregator (default is text):
      # FORGEDB_LOG_FORMAT: json
      # Multi-tenancy (#59): one process serves ONE tenant. Set FORGEDB_TENANT
      # to serve <FORGEDB_DATA>/<tenant>, and run one service per tenant behind a
      # host/subdomain proxy.
      # FORGEDB_TENANT: my-tenant
      # Verify-only JWT tenant guard — mount the IdP public key and set:
      # FORGEDB_JWT_PUBKEY: /keys/idp.pem
      # FORGEDB_JWT_ISSUER: https://issuer.example.com
      # FORGEDB_JWT_AUDIENCE: forgedb
    volumes:
      - forgedb-data:/data
    healthcheck:
      test: ["CMD-SHELL", "curl -fsS http://localhost:3000/health || exit 1"]
      interval: 10s
      timeout: 3s
      retries: 5

volumes:
  forgedb-data:
"#
    );
    fs::write(project_path.join("docker-compose.yml"), compose)?;

    ui::step("🐳", "Created Dockerfile, .dockerignore, docker-compose.yml");

    // The symmetric on-host (non-container) path (#115): a systemd unit template
    // + EnvironmentFile + a short install README, grouped under deploy/. Same
    // class of artifact as the Docker files above — inert ops packaging around
    // the already-generated binary; nothing reads schema.forge at runtime.
    create_systemd_files(options)?;

    Ok(())
}

/// Emit the on-host (non-container) deploy path (#115): a systemd unit template,
/// an `EnvironmentFile`, and an install README, under `deploy/`.  The unit runs
/// the generated binary as a non-root `DynamicUser` with a managed
/// `StateDirectory` (the on-host analogue of the container's non-root user +
/// `/data` VOLUME), reads config from the env file (12-factor, same knobs as the
/// compose file), and relies on the scaffold `main.rs` graceful-shutdown path for
/// a clean `systemctl stop`.  systemd goes under `deploy/` (not the project root
/// like the `Dockerfile`) because a unit has no build-context root requirement —
/// it is installed to `/etc/systemd/system/`.  Nothing here reads `schema.forge`.
fn create_systemd_files(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);
    // The service/binary name is the project's final path component (the operator
    // installs the binary as `<name>`), not the full project_name string — which
    // may be a path. Used for the emitted file NAMES + the in-unit references.
    let bin = project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(options.project_name.as_str());
    let deploy_dir = project_path.join("deploy");
    fs::create_dir_all(&deploy_dir)?;

    // The unit template. `Type=exec` (not `notify`) is the honest readiness model
    // — the binary does not `sd_notify`; proxy/LB readiness is `GET /ready`.
    // `DynamicUser=yes` + `StateDirectory=<name>` give a non-root, isolated,
    // persistent data dir (/var/lib/<name>) without a manual useradd/chown; the
    // env file below points FORGEDB_DATA at it.  `KillSignal=SIGTERM` +
    // `TimeoutStopSec=30` pair with the graceful-shutdown drain in main.rs.
    let service = format!(
        r#"# systemd unit for the {bin} ForgeDB generated server (#115 on-host deploy).
#
# Install:
#   cargo build --release
#   sudo install -Dm755 target/release/{bin} /usr/local/bin/{bin}
#   sudo install -Dm644 deploy/{bin}.env     /etc/{bin}/{bin}.env
#   sudo install -Dm644 deploy/{bin}.service /etc/systemd/system/{bin}.service
#   sudo systemctl daemon-reload
#   sudo systemctl enable --now {bin}
#
# One writer per data directory (v1 single-writer contract): do NOT run two units
# against the same FORGEDB_DATA. For multi-tenant scale-out, run one unit per
# tenant, each with its own FORGEDB_TENANT + StateDirectory (see deploy/README.md).
[Unit]
Description={bin} — ForgeDB generated server
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
ExecStart=/usr/local/bin/{bin}
EnvironmentFile=/etc/{bin}/{bin}.env

# Non-root without a manual useradd; StateDirectory is created + chowned to the
# transient user and persists across restarts. The env file sets
# FORGEDB_DATA=/var/lib/{bin} to match.
DynamicUser=yes
StateDirectory={bin}

Restart=on-failure
RestartSec=2
# main.rs drains in-flight requests on SIGTERM (graceful shutdown).
KillSignal=SIGTERM
TimeoutStopSec=30

# Hardening — the server needs only its state dir and a TCP socket.
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
RestrictAddressFamilies=AF_INET AF_INET6
RestrictNamespaces=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
"#
    );
    fs::write(deploy_dir.join(format!("{bin}.service")), service)?;

    // The EnvironmentFile — the on-host mirror of the compose `environment:` block.
    // Uncomment/edit to change a default. FORGEDB_DATA points at the unit's
    // StateDirectory so it works out of the box.
    let env_file = format!(
        r#"# EnvironmentFile for the {bin} systemd unit (#115). Installed to
# /etc/{bin}/{bin}.env. All config is 12-factor (no runtime config file).

# Bind on all interfaces so a reverse proxy in front can reach it.
FORGEDB_HOST=0.0.0.0
FORGEDB_PORT=3000

# Data directory — the systemd StateDirectory (/var/lib/{bin}), created + owned
# by the service user. This is the whole database; back it up with `forgedb
# backup create`.
FORGEDB_DATA=/var/lib/{bin}

# Log level (tracing env-filter). Uncomment for JSON lines to the journal:
RUST_LOG=info
# FORGEDB_LOG_FORMAT=json

# Multi-tenancy (#59): one process serves ONE tenant. Set FORGEDB_TENANT to serve
# <FORGEDB_DATA>/<tenant>, and run one unit per tenant behind a host/subdomain
# proxy.
# FORGEDB_TENANT=my-tenant

# Verify-only JWT tenant guard — set the IdP public key path to enable:
# FORGEDB_JWT_PUBKEY=/etc/{bin}/idp.pem
# FORGEDB_JWT_ISSUER=https://issuer.example.com
# FORGEDB_JWT_AUDIENCE={bin}
# FORGEDB_TENANT_CLAIM=tenant
"#
    );
    fs::write(deploy_dir.join(format!("{bin}.env")), env_file)?;

    // The install README — copy/enable/start + per-tenant + BYO-proxy pointers.
    let readme = format!(
        r#"# On-host deployment ({bin})

The symmetric on-host (non-container) path to the `Dockerfile` (#115). The
generated app is a single self-contained binary configured entirely from the
environment — an ideal systemd citizen.

## systemd (Linux — the scaffolded path)

```bash
cargo build --release
sudo install -Dm755 target/release/{bin} /usr/local/bin/{bin}
sudo install -Dm644 deploy/{bin}.env     /etc/{bin}/{bin}.env
sudo install -Dm644 deploy/{bin}.service /etc/systemd/system/{bin}.service
sudo systemctl daemon-reload
sudo systemctl enable --now {bin}

systemctl status {bin}
journalctl -u {bin} -f          # logs (add FORGEDB_LOG_FORMAT=json for JSON)
curl -fsS http://localhost:3000/health   # liveness
curl -fsS http://localhost:3000/ready    # readiness
```

Edit config in `/etc/{bin}/{bin}.env`, then `sudo systemctl restart {bin}`.

The unit runs as a non-root `DynamicUser` with a managed `StateDirectory`
(`/var/lib/{bin}`, the data dir) and drains in-flight requests on stop (SIGTERM →
the graceful-shutdown path in `main.rs`).

## One writer per data directory

The v1 contract is one writer per data dir (an advisory lock on open; a second
writer refuses to start, it does not corrupt). Do **not** point two units at the
same `FORGEDB_DATA`. To scale across tenants, run **one unit per tenant** — copy
`{bin}.service` to `{bin}@.service` (a systemd template), set
`FORGEDB_TENANT=%i` and `StateDirectory={bin}/%i` in it, and
`systemctl enable --now {bin}@acme`.

## Reverse proxy / TLS (bring your own)

Terminate TLS and route hosts/subdomains with nginx or Caddy in front of the
bound port. Forward the `Upgrade`/`Connection` headers so the change-feed /
live-query / replication **WebSocket** routes work, and forward `Authorization`
for the JWT guard.

```caddy
db.example.com {{
    reverse_proxy 127.0.0.1:3000
}}
```

```nginx
location / {{
    proxy_pass http://127.0.0.1:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Authorization $http_authorization;
}}
```

## Other init systems

systemd is scaffolded; the rest are a hand-portable copy of the same idea —
`exec` the binary as a non-root user with the env from `{bin}.env`, auto-restart,
stop with SIGTERM:

- **OpenRC** (Alpine/Gentoo): a `/etc/init.d/{bin}` `supervise-daemon` script +
  `/etc/conf.d/{bin}` for the env.
- **runit / s6** (Void/minimal): a `run` script `exec chpst -u {bin} <binary>`
  (runit) or `s6-setuidgid` (s6); restart is intrinsic.
- **launchd** (macOS): a `.plist` with `ProgramArguments`, `EnvironmentVariables`,
  and `KeepAlive`.
- **supervisord** (systemd-less/shared hosts): a `[program:{bin}]` block with
  `environment=`, `autorestart=true`, `stopsignal=TERM`.
- **Windows service**: wrap the console binary with WinSW/NSSM (SIGTERM semantics
  differ; graceful shutdown rides Ctrl-C on Windows).

`nohup`/`tmux`/`screen` are **not** deployment targets — no restart, no boot
persistence, no log management.

See `docs/DEPLOYMENT.md` for the full landscape and the container path.
"#
    );
    fs::write(deploy_dir.join("README.md"), readme)?;

    ui::step(
        "🐧",
        &format!("Created deploy/{bin}.service, deploy/{bin}.env, deploy/README.md"),
    );
    Ok(())
}
