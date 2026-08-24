use crate::project::{Answer, Asker, Question};
use crate::{error::CliError, project, templates, ui, Result};
use std::fs;
use std::path::Path;

pub struct InitOptions {
    /// The directory to scaffold. Frequently a path (`apps/api`), which is why
    /// the project id is its **last component** rather than the whole argument.
    pub project_name: String,
    /// `--project-name`: the project id, decoupled from the directory.
    ///
    /// The non-interactive twin of C12's prompt (#367). The two decisions a
    /// prompt fills fire on adopted repositories, so this does nothing for them
    /// on its own — but a scaffold whose directory name is already taken has
    /// exactly one thing it needs, and needing it in CI is why it is a flag.
    pub project_name_override: Option<String>,
    pub template: Option<String>,
    /// REMOVED (#335 §15). Carried only so `refuse_removed_flags` can name the
    /// replacement; setting it is always an error.
    pub rust: bool,
    /// REMOVED (#335 §15). Same as `rust`.
    pub api_only: bool,
    /// `--isolated` / `--no-isolated`.  `None` means "decide from what is
    /// above me", which is the whole reason this is three-valued rather than a
    /// plain flag: the useful default depends on the tree.
    pub isolated: Option<bool>,
}

/// `forgedb init`, with whatever asker this invocation is entitled to.
pub fn run(options: InitOptions) -> Result<()> {
    run_with(options, &*crate::ask::asker())
}

pub fn run_with(options: InitOptions, asker: &dyn Asker) -> Result<()> {
    // Before anything touches the filesystem: a removed flag is refused by name,
    // never absorbed. Placed first so the refusal cannot leave a half-scaffolded
    // directory behind.
    refuse_removed_flags(&options)?;

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
    let project_id = resolve_a_taken_name(project_id(&options)?, isolated, asker)?;

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

    // The deploy path is no longer gated on a Rust scaffold existing (#335 §15):
    // there is no Rust scaffold. It drives the CLI instead, so it is emitted for
    // every project.
    create_deploy_files(&options)?;

    ui::success("Done! Run the following to get started:");
    println!();
    println!("  cd {}", options.project_name);
    println!("  forgedb generate");
    println!("  forgedb build");
    println!();
    ui::info(
        "This project contains no Cargo.toml on purpose (#335): ForgeDB compiles the \
         generated Rust in its own build cache. `forgedb build` prints where the \
         artifacts landed; `forgedb build --print-artifact server` prints just the \
         server binary's path.",
    );

    Ok(())
}

/// Refuse `--rust` / `--api-only` by name (#335 §15).
///
/// Both selected between "scaffold a cargo package" and "do not", and `init` no
/// longer scaffolds one either way — so there is nothing left for them to
/// select. They stay in the parser (hidden) purely so this diagnostic can name
/// the replacement: dropping them from clap would produce "unexpected argument
/// '--rust' found", which tells the user nothing about where the Rust went.
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

    // Create main directories.
    //
    // No `src/` (#335 §15): that was the scaffolded cargo package's source dir,
    // and there is no scaffolded cargo package. `generated/` stays — it receives
    // the read-only mirror of `database.rs`/`api.rs` plus every non-Rust
    // artifact (types.ts, openapi.json, the SDKs, `go/`), all of which is text
    // the user commits.
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
fn resolve_a_taken_name(
    project_name: String,
    isolated: bool,
    asker: &dyn Asker,
) -> Result<String> {
    if !isolated {
        return Ok(project_name);
    }
    let Some(holder) = project::held_by(&project_name)? else {
        return Ok(project_name);
    };

    // C12's other half: at a terminal, offer a new name here — where the name is
    // being chosen — rather than refusing and leaving the user to pick one.
    //
    // `TakeOverClaim` is deliberately NOT offered and not honoured: `init`
    // claims nothing (a scaffold that reserved an id it may never generate would
    // leave a stale claim behind for every abandoned `init`), so there is
    // nothing here for a take-over to write into. The first `generate` in the
    // new tree reaches the take-over path with a real project behind it.
    let question = Question::Collision {
        id: project_name.clone(),
        root: Path::new(".").join(&project_name),
        held_by: holder.path.clone(),
        holder_exists: holder.exists,
        schema_hint: None,
    };
    if let Some(Answer::Name(chosen)) = asker.ask(&question)? {
        // The DIRECTORY keeps the name the user typed. A project id and a
        // directory are different things.
        ui::info(&format!("Naming this project {chosen:?}"));
        return Ok(chosen);
    }

    Err(CliError::Config(format!(
        "Project name {project_name:?} is already claimed by {}.\n\n\
         Two projects sharing an id would share one build cache, one lockfile \
         and one target directory.\n\n\
         Give this project a different id with `--project-name <NAME>` — the \
         directory keeps the name you typed — or pass --no-isolated to join \
         an enclosing project instead.",
        holder.path.display()
    )))
}

/// The project id for a scaffold: the **last component** of the path, not the
/// path.
///
/// `forgedb init ./apps/api` should name the project `api`. Writing the whole
/// argument was harmless while `[project].name` was ignored; since #333 the name
/// is used verbatim as a directory under `~/.forgedb`, so a path there is either
/// rejected or escapes the cache.
/// This scaffold's project id: `--project-name` when given, else the directory's
/// last path component.
///
/// The **directory keeps the name the user typed** either way. A project id and
/// a directory are different things, and conflating them is what made a taken id
/// an unfixable `init` — the user had to rename the directory to rename the
/// project.
fn project_id(options: &InitOptions) -> Result<String> {
    match options.project_name_override.as_deref() {
        Some(explicit) => {
            // Refused here rather than at the first `generate`: a project id is
            // used verbatim as a directory under ~/.forgedb, so a path there
            // escapes the cache rather than keys it.
            if explicit.is_empty()
                || explicit.contains('/')
                || explicit.contains('\\')
                || explicit == "."
                || explicit == ".."
            {
                return Err(CliError::Config(format!(
                    "Invalid --project-name {explicit:?}: a project id is used \
                     verbatim as a directory name under ~/.forgedb/projects/, so \
                     it cannot be empty, contain a path separator, or be `.` or \
                     `..`."
                )));
            }
            Ok(explicit.to_string())
        }
        None => Ok(derive_project_id(&options.project_name)),
    }
}

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

/// Emit the blessed container deploy path: a multi-stage `Dockerfile`, a
/// `.dockerignore`, and a `docker-compose.yml` (#335 §15).
///
/// **The image drives the CLI.** Before #335 the builder stage did
/// `COPY Cargo.toml`, `COPY src`, `COPY generated`, `RUN cargo build --release`
/// and then copied `target/release/<project>` — every one of those inputs is
/// gone, because `init` no longer scaffolds a cargo package and the generated
/// Rust is compiled in ForgeDB's own cache. So the builder installs the pinned
/// `forgedb`, copies the project source (schema + config, never `generated/`),
/// runs `forgedb generate` + `forgedb build`,
/// and copies out **the path ForgeDB reports** rather than a path this file
/// guessed.
///
/// It is still ops packaging: nothing here reads `schema.forge` at runtime.
fn create_deploy_files(options: &InitOptions) -> Result<()> {
    let project_path = Path::new(&options.project_name);
    // The image/service name is the project's final path component, never the
    // whole `project_name` — which may be a path (`forgedb init apps/api`), and
    // a `/` is illegal in both a docker tag and a compose service name.
    let bin = derive_project_id(&options.project_name);
    // Pin the CLI that scaffolded this project. A floating `cargo install
    // forgedb` would change the generated code between two builds of the same
    // commit, which is the reproducibility hole a Dockerfile exists to close.
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

    // The builder regenerates from the schema, so generated code, build output
    // and database files must not enter the context.
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
/// compose file), and relies on the generated server's graceful-shutdown path for
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
    // `TimeoutStopSec=30` pair with the generated server's graceful-shutdown drain.
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
