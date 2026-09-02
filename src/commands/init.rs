use crate::{error::CliError, project, templates, ui, Result};
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub project_name: String,
    pub template: Option<String>,
    pub rust: bool,
    pub api_only: bool,
    pub isolated: Option<bool>,
}

pub fn run(options: InitOptions) -> Result<()> {
    refuse_removed_flags(&options)?;

    ui::header("✨", &format!("Creating project: {}", options.project_name));

    let project_path = Path::new(&options.project_name);
    if project_path.exists() {
        return Err(CliError::ProjectExists(options.project_name.clone()));
    }

    let isolated = resolve_isolated(&options)?;

    let project_id = project::mint_id(project_path);

    create_project_structure(&options)?;

    create_schema_file(&options)?;

    create_config_file(&options, &project_id, isolated)?;

    create_gitignore(&options)?;

    create_readme(&options)?;

    create_deploy_files(&options)?;

    ui::success("Done! Run the following to get started:");
    ui::blank();
    ui::line(&format!("  cd {}", options.project_name));
    ui::line("  forgedb generate");
    ui::line("  forgedb build");
    ui::blank();
    ui::info(
        "This project contains no Cargo.toml on purpose (#335): ForgeDB compiles the \
         generated Rust in its own build cache. `forgedb build` prints where the \
         artifacts landed; `forgedb build --print-artifact server` prints just the \
         server binary's path.",
    );

    Ok(())
}

fn refuse_removed_flags(options: &InitOptions) -> Result<()> {
    if options.rust {
        return Err(CliError::ConfigDiagnostic(
            "`--rust` was removed in #335.\n\n\
             `forgedb init` no longer scaffolds a cargo package: the generated Rust \
             (core, server, and the runtime bindings) is built in ForgeDB's own cache \
             under $FORGEDB_HOME, not in your repository. There is no longer a \
             \"without Rust\" project to opt out of.\n\n\
             Choose which generators run with `targets` under `[generate]` in \
             forgedb.toml — `targets = [\"all\"]` is what the scaffold writes — then \
             run `forgedb generate` and `forgedb build`."
                .to_string(),
        ));
    }
    if options.api_only {
        return Err(CliError::ConfigDiagnostic(
            "`--api-only` was removed in #335.\n\n\
             It suppressed the scaffolded cargo package, and `forgedb init` no longer \
             scaffolds one — the generated Rust is built in ForgeDB's own cache under \
             $FORGEDB_HOME.\n\n\
             To narrow what is generated, set `targets` under `[generate]` in \
             forgedb.toml — e.g. `targets = [\"api\", \"openapi\"]` — then run \
             `forgedb generate`."
                .to_string(),
        ));
    }
    Ok(())
}

fn create_project_structure(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);

    fs::create_dir_all(project_path)?;
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

fn resolve_isolated(options: &InitOptions) -> Result<bool> {
    if let Some(explicit) = options.isolated {
        return Ok(explicit);
    }

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

fn dir_slug(project_name: &str) -> String {
    Path::new(project_name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| project_name.to_string())
}

fn create_deploy_files(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);
    let bin = dir_slug(&options.project_name);
    let forgedb_version = env!("CARGO_PKG_VERSION");

    let dockerfile = format!(
        r#"# syntax=docker/dockerfile:1
# ForgeDB generated-server image (#335 §15).
#
# ForgeDB owns the build. This image copies your SCHEMA — not generated code, and
# not a cargo package, because ForgeDB no longer scaffolds one — installs the
# pinned CLI, and lets it generate and compile into its own build cache. The
# server binary's path is never guessed here: it is the one `forgedb build`
# reports.
#
#   docker build -t {bin} .

FROM rust:1-slim AS builder

# Name the build cache explicitly. Leaving it at $HOME/.forgedb works, but in a
# container HOME is an accident of the base image, and every artifact path
# `forgedb build` reports lives under this directory.
ENV FORGEDB_HOME=/forgedb

# Pinned to the CLI that scaffolded this project. Bump it deliberately.
RUN cargo install forgedb --version {forgedb_version} --locked

WORKDIR /build
# The whole project, minus what .dockerignore drops. `generated/` is dropped on
# purpose: the builder regenerates it from schema.forge, so a stale committed
# copy can never reach the image.
COPY . ./

RUN forgedb generate

# Resolve the artifact; do not construct its path. Package names carry a per-app
# hash and change when the schema file is renamed, so the Dockerfile names the
# stable KIND (`server`) instead. One invocation emits both the human build and
# the machine-readable inventory; `--print-artifact` puts the single path on
# stdout and everything else on stderr.
RUN mkdir -p /out \
 && cp "$(forgedb build --release --report /out/artifacts.json --print-artifact server)" /out/server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 forgedb
WORKDIR /app
COPY --from=builder /out/server /usr/local/bin/forgedb-server
# The build inventory, for anything downstream that wants more than one path.
COPY --from=builder /out/artifacts.json /app/artifacts.json

# Config comes from the environment (12-factor). Data lives on a mounted volume —
# never baked into the image.
#
# FORGEDB_DATA is ABSOLUTE, and outside the builder's FORGEDB_HOME, on purpose:
# the generated server refuses to open a database inside the ForgeDB build cache
# (#335 C4), and the built-in data root is RELATIVE (`data`), which is what makes
# the cache reachable by accident rather than by mistake. FORGEDB_HOME does not
# exist in this stage at all.
ENV FORGEDB_HOST=0.0.0.0 \
    FORGEDB_PORT=3000 \
    FORGEDB_DATA=/data \
    RUST_LOG=info
RUN mkdir -p /data && chown forgedb:forgedb /data
VOLUME ["/data"]
USER forgedb
EXPOSE 3000

# Liveness against the generated /health endpoint.
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 \
    CMD curl -fsS http://localhost:3000/health || exit 1

CMD ["forgedb-server"]
"#
    );
    fs::write(project_path.join("Dockerfile"), dockerfile)?;

    let dockerignore = "\
# Regenerated inside the image from schema.forge — a committed copy must never
# win over a fresh generate (#335).
generated/

# Never ship data or build output into a build context.
data/
target/
.git/
node_modules/
**/*.rs.bk

# The deploy files themselves are not build inputs.
Dockerfile
.dockerignore
docker-compose.yml
";
    fs::write(project_path.join(".dockerignore"), dockerignore)?;

    let compose = format!(
        r#"# ForgeDB generated-server compose file.
#   docker compose up --build
#
# The image generates and builds from schema.forge itself (#335) — there is no
# `forgedb generate` step to run first, and no cargo package in this directory.
services:
  {bin}:
    build: .
    ports:
      - "3000:3000"
    environment:
      FORGEDB_HOST: 0.0.0.0
      FORGEDB_PORT: "3000"
      # Absolute, and not inside a ForgeDB build cache — the server refuses the
      # latter (#335 C4).
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

    create_systemd_files(options)?;

    Ok(())
}

fn create_systemd_files(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);
    let bin = project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(options.project_name.as_str());
    let deploy_dir = project_path.join("deploy");
    fs::create_dir_all(&deploy_dir)?;

    let service = format!(
        r#"# systemd unit for the {bin} ForgeDB generated server (#115 on-host deploy).
#
# Install:
#   forgedb generate
#   sudo install -Dm755 \
#       "$(forgedb build --release --print-artifact server)" /usr/local/bin/{bin}
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
# The generated server drains in-flight requests on SIGTERM.
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

    let readme = format!(
        r#"# On-host deployment ({bin})

The symmetric on-host (non-container) path to the `Dockerfile` (#115). The
generated app is a single self-contained binary configured entirely from the
environment — an ideal systemd citizen.

## systemd (Linux — the scaffolded path)

```bash
forgedb generate
# ForgeDB compiles the generated Rust in its own cache and REPORTS where the
# binary landed (#335); there is no cargo package in this directory to build,
# and the path is never one you construct by hand.
sudo install -Dm755 \
    "$(forgedb build --release --print-artifact server)" /usr/local/bin/{bin}
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
the graceful-shutdown path in the generated server).

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
