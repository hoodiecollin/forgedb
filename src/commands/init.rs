use crate::{error::CliError, templates, ui, Result};
use std::fs;
use std::path::Path;

pub struct InitOptions {
    pub project_name: String,
    pub template: Option<String>,
    pub rust: bool,
    pub api_only: bool,
}

pub fn run(options: InitOptions) -> Result<()> {
    ui::header("✨", &format!("Creating project: {}", options.project_name));

    // Check if project directory already exists
    let project_path = Path::new(&options.project_name);
    if project_path.exists() {
        return Err(CliError::ProjectExists(options.project_name.clone()));
    }

    // Create project directory structure
    create_project_structure(&options)?;

    // Create schema file based on template
    create_schema_file(&options)?;

    // Create config file
    create_config_file(&options)?;

    // Create .gitignore
    create_gitignore(&options)?;

    // Create README
    create_readme(&options)?;

    // Create Rust files if needed
    if options.rust || !options.api_only {
        create_rust_files(&options)?;
        // The blessed container deploy path (Phase 5 WS2) targets the generated
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

fn create_config_file(options: &InitOptions) -> Result<()> {
    let config_content = templates::default_config(&options.project_name);
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
    // for the request-logging layer on the generated router (Phase 5 WS1
    // observability).  The scaffold `main.rs` installs a `tracing-subscriber`
    // (env-filtered via `RUST_LOG`) so those spans are emitted.
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedb-storage = "0.1.5"
forgedb-types = "0.2"
forgedb-changefeed = "0.2"
forgedb-wal = "0.2"
forgedb-auth = "0.1"
forgedb-query-params = "0.1"
forgedb-compaction = "0.1"
forgedb-txn = "0.1"
forgedb-coordinator = "0.1"
regex = "1"
rust_decimal = {{ version = "1", features = ["serde-with-str"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
utoipa = {{ version = "5", features = ["uuid"] }}
utoipa-axum = "0.2"
axum = {{ version = "0.8", features = ["ws"] }}
tokio = {{ version = "1", features = ["full"] }}
tower-http = {{ version = "0.6", features = ["trace"] }}
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
    let main_rs = r#"#[path = "../generated/database.rs"]
mod database;
use database::*;

#[path = "../generated/api.rs"]
mod api;

// Deployment config comes from the environment — one binary, N tenant processes
// (12-factor). Multi-tenancy (#59) is physical: this process serves ONE tenant,
// opening its data dir; a front proxy routes each tenant's subdomain/host to its
// process. Nothing here reads schema.forge at runtime.
//
//   FORGEDB_TENANT       the tenant this process serves (selects <data>/<tenant>)
//   FORGEDB_DATA         tenant root dir (default: data)
//   FORGEDB_HOST         bind host (default: 127.0.0.1)
//   FORGEDB_PORT         bind port (default: 3000)
//
// Verify-only JWT tenant guard (enabled when FORGEDB_JWT_PUBKEY is set):
//   FORGEDB_JWT_PUBKEY   path to the IdP's PEM public key (verification key)
//   FORGEDB_JWT_ALG      signature algorithm (default: RS256; asymmetric only)
//   FORGEDB_JWT_ISSUER   expected `iss`
//   FORGEDB_JWT_AUDIENCE expected `aud`
//   FORGEDB_TENANT_CLAIM claim carrying the tenant id (default: tenant)
//   FORGEDB_JWT_LEEWAY   clock-skew leeway seconds (default: 60)
#[tokio::main]
async fn main() {
    // Structured logging (Phase 5 WS1): the router logs each request as a
    // `tracing` span via tower-http's TraceLayer; install a subscriber that
    // honors `RUST_LOG` (default `info`) so those spans are emitted.  Set
    // FORGEDB_LOG_FORMAT=json for machine-parseable JSON lines (log aggregators);
    // any other value (or unset) keeps the human-readable text format.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let json_logs = std::env::var("FORGEDB_LOG_FORMAT")
        .map(|f| f.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if json_logs {
        tracing_subscriber::fmt().json().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }

    let tenant = std::env::var("FORGEDB_TENANT").ok();
    let data_root = std::env::var("FORGEDB_DATA").unwrap_or_else(|_| "data".to_string());
    let host = std::env::var("FORGEDB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("FORGEDB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    // Per-tenant data dir: <data_root>/<tenant> when a tenant is set, else the
    // root itself (single-tenant / tenancy off).
    let data_dir = match &tenant {
        Some(t) => std::path::Path::new(&data_root).join(t),
        None => std::path::PathBuf::from(&data_root),
    };
    let db = std::sync::Arc::new(tokio::sync::RwLock::new(
        database::Database::open_at(data_dir),
    ));

    let router = match build_authenticator(tenant.as_deref()) {
        Some(auth) => {
            tracing::info!(tenant = ?tenant, "JWT tenant guard enabled");
            api::create_router_with_auth(db, std::sync::Arc::new(auth))
        }
        None => api::create_router(db),
    };

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind listener");
    tracing::info!(tenant = ?tenant, data_root = %data_root, %addr, "ForgeDB serving");
    // Graceful shutdown (Phase 5 WS2): drain in-flight requests on SIGINT/SIGTERM
    // so a container stop or `Ctrl-C` doesn't sever open connections mid-write.
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("serve");
}

/// Resolve on the first shutdown signal — `Ctrl-C` (SIGINT) or, on Unix, SIGTERM
/// (how Docker/Kubernetes ask a container to stop).  Returning from this future
/// tells `axum::serve` to stop accepting and drain (Phase 5 WS2).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received — draining connections");
}

/// Build the verify-only JWT authenticator from env, or `None` to run without a
/// tenant guard. Requires FORGEDB_JWT_PUBKEY; when set, FORGEDB_TENANT must name
/// the tenant this process serves (the value the token's tenant claim is
/// cross-checked against).
fn build_authenticator(tenant: Option<&str>) -> Option<forgedb_auth::Authenticator> {
    let pubkey_path = std::env::var("FORGEDB_JWT_PUBKEY").ok()?;
    let tenant = tenant.expect("FORGEDB_TENANT is required when the JWT guard is enabled");
    let pem = std::fs::read_to_string(&pubkey_path).expect("read FORGEDB_JWT_PUBKEY");
    let alg = std::env::var("FORGEDB_JWT_ALG")
        .ok()
        .and_then(|a| forgedb_auth::parse_algorithm(&a))
        .unwrap_or(forgedb_auth::Algorithm::RS256);
    let cfg = forgedb_auth::AuthConfig {
        algorithms: vec![alg],
        issuer: std::env::var("FORGEDB_JWT_ISSUER").ok(),
        audience: std::env::var("FORGEDB_JWT_AUDIENCE").ok(),
        tenant_claim: std::env::var("FORGEDB_TENANT_CLAIM").unwrap_or_else(|_| "tenant".to_string()),
        leeway_secs: std::env::var("FORGEDB_JWT_LEEWAY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60),
        required_claims: vec![],
    };
    let keys = forgedb_auth::KeySource::static_pem(None, pem, alg);
    Some(forgedb_auth::Authenticator::new(cfg, keys, tenant))
}
"#;

    let main_rs_path = Path::new(&options.project_name).join("src").join("main.rs");
    fs::write(main_rs_path, main_rs)?;

    ui::step("🦀", "Created Rust project files");
    ui::info("Run 'forgedb generate --rust' to generate the database code");
    Ok(())
}

/// Emit the blessed container deploy path (Phase 5 WS2): a multi-stage
/// `Dockerfile`, a `.dockerignore`, and a `docker-compose.yml`.  The image builds
/// the generated Rust server and runs it as a non-root user with `/data` on a
/// volume, `FORGEDB_HOST=0.0.0.0`, and a `HEALTHCHECK` against the generated
/// `/health` endpoint (Phase 5 WS1).  None of this reads `schema.forge` at
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
# ForgeDB generated-server image (Phase 5 WS2 deploy path).
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

# Liveness against the generated /health endpoint (Phase 5 WS1).
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
        r#"# ForgeDB generated-server compose file (Phase 5 WS2).
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
    Ok(())
}
