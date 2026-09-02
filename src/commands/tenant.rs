use std::path::PathBuf;

use crate::error::{CliError, Result};

pub struct CreateOptions {
    pub name: String,
    pub root: PathBuf,
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
        let items: Vec<String> = tenants
            .iter()
            .map(|t| format!("{:?}", t))
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
