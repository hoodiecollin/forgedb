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
    // and forgedb-query-params for list-endpoint filter/sort/paginate (#90 — the
    // query string is parsed by this schema-agnostic substrate; all field-aware
    // filtering/sorting is generated per-model).
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
forgedb-storage = "0.1.5"
forgedb-types = "0.2"
forgedb-changefeed = "0.1"
forgedb-wal = "0.2"
forgedb-auth = "0.1"
forgedb-query-params = "0.1"
forgedb-compaction = "0.1"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
utoipa = {{ version = "5", features = ["uuid"] }}
utoipa-axum = "0.2"
axum = {{ version = "0.8", features = ["ws"] }}
tokio = {{ version = "1", features = ["full"] }}
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
            println!("🔐 JWT tenant guard enabled (tenant = {:?})", tenant);
            api::create_router_with_auth(db, std::sync::Arc::new(auth))
        }
        None => api::create_router(db),
    };

    let addr = format!("{host}:{port}");
    println!("🚀 ForgeDB serving tenant {:?} from '{}' on http://{}", tenant, data_root, addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind listener");
    axum::serve(listener, router).await.expect("serve");
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
