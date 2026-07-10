//! `forgedb tenant` — manage physical, dir-per-tenant data directories (#59).
//!
//! Multi-tenancy is filesystem-enforced: each tenant's data lives under
//! `<root>/<tenant>/`, and one `forgedb serve` process serves exactly one
//! tenant (process-per-tenant). This command makes tenant existence explicit
//! and auditable — a peer to `migrate` / `compact` / `backup`. It only ever
//! creates, lists, or removes directories; it never reads a `.forge` schema.

use std::path::PathBuf;

use crate::error::{CliError, Result};

pub struct CreateOptions {
    pub name: String,
    pub root: PathBuf,
    /// `FORGEDB_*` auth exports derived from `[auth]` (empty when auth is off),
    /// printed alongside the serve incantation so the operator gets the full env.
    pub auth_env: Vec<(String, String)>,
}

pub struct ListOptions {
    pub root: PathBuf,
    pub json: bool,
}

pub struct DropOptions {
    pub name: String,
    pub root: PathBuf,
    pub force: bool,
}

/// Reject names that would escape the root or collide with path syntax. A tenant
/// name is a single directory segment — no separators, no `.`/`..`.
fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(std::path::MAIN_SEPARATOR);
    if ok {
        Ok(())
    } else {
        Err(CliError::Other(format!(
            "invalid tenant name '{name}': must be a single path segment (no separators, not '.'/'..')"
        )))
    }
}

/// Create a tenant's data directory. The generated server populates it lazily on
/// first `open_at`; this just reserves the (empty) directory so the tenant is
/// explicit and listable.
pub fn create(opts: CreateOptions) -> Result<()> {
    validate_name(&opts.name)?;
    let dir = opts.root.join(&opts.name);
    if dir.exists() {
        return Err(CliError::Other(format!(
            "tenant '{}' already exists at {}",
            opts.name,
            dir.display()
        )));
    }
    std::fs::create_dir_all(&dir)?;
    println!("✓ Created tenant '{}' at {}", opts.name, dir.display());

    // The process-per-tenant serve incantation: env selects the tenant + data
    // root; when `[auth]` is enabled the guard's env is appended.
    let mut env = format!(
        "FORGEDB_TENANT={} FORGEDB_DATA={}",
        opts.name,
        opts.root.display()
    );
    for (k, v) in &opts.auth_env {
        env.push_str(&format!(" {k}={v}"));
    }
    println!("  Serve it with:  {env} <your-generated-binary>");
    Ok(())
}

/// List tenant directories under the root (immediate subdirectories).
pub fn list(opts: ListOptions) -> Result<()> {
    let mut tenants: Vec<String> = Vec::new();
    if opts.root.exists() {
        for entry in std::fs::read_dir(&opts.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(name) = entry.file_name().to_str()
            {
                tenants.push(name.to_string());
            }
        }
    }
    tenants.sort();

    if opts.json {
        // Minimal, dependency-free JSON array of names.
        let items: Vec<String> = tenants
            .iter()
            .map(|t| format!("{:?}", t)) // quoted + escaped
            .collect();
        println!("[{}]", items.join(","));
    } else if tenants.is_empty() {
        println!("No tenants under {}", opts.root.display());
    } else {
        println!("Tenants under {}:", opts.root.display());
        for t in &tenants {
            println!("  {t}");
        }
    }
    Ok(())
}

/// Remove a tenant's data directory and everything under it (destructive).
pub fn drop(opts: DropOptions) -> Result<()> {
    validate_name(&opts.name)?;
    let dir = opts.root.join(&opts.name);
    if !dir.exists() {
        return Err(CliError::Other(format!(
            "tenant '{}' does not exist at {}",
            opts.name,
            dir.display()
        )));
    }
    if !opts.force {
        return Err(CliError::Other(format!(
            "refusing to drop tenant '{}' ({}) without --force — this deletes all its data",
            opts.name,
            dir.display()
        )));
    }
    std::fs::remove_dir_all(&dir)?;
    println!("✓ Dropped tenant '{}' ({})", opts.name, dir.display());
    Ok(())
}
