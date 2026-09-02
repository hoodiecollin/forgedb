use std::path::{Path, PathBuf};

use forgedb_codegen::{CorePackage, GenConfig};

use crate::{error::Result, ui};

pub(super) fn emit(
    dir: &Path,
    core_pkg: &str,
    config: &GenConfig,
    core_lib: &str,
) -> Result<Vec<PathBuf>> {
    let files = CorePackage::files(core_pkg, config, core_lib);
    let written = super::write_core_package(dir, &files)?;

    ui::success(&format!(
        "In-tree Rust package: {} ({} files)",
        dir.display(),
        written.len()
    ));
    ui::info("  Add this one line to the [dependencies] of whichever crate should use it,");
    ui::info("  with `path` re-based against THAT crate's directory:");
    ui::info(&format!("    {}", CorePackage::dep_line(core_pkg, dir)));

    Ok(written)
}

pub(super) fn guard(dir: &Path) -> Result<()> {
    if let Some(home) = crate::cache::home_containing(dir)? {
        return Err(crate::error::CliError::Config(format!(
            "[placement].rust_package = {} resolves inside the ForgeDB build cache ({}).\n\
             The build cache is derived state and may be deleted at any time, so it must \
             never hold committed source — and an in-tree package is committed source: a \
             path dependency naming a missing directory fails the workspace load for every \
             crate in it.\n\
             Point `[placement].rust_package` at a directory in your own repository, or \
             remove the key to opt out.",
            dir.display(),
            home.display()
        )));
    }
    Ok(())
}

pub(super) fn check(
    dir: &Path,
    core_pkg: &str,
    config: &GenConfig,
    core_lib: &str,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut missing = Vec::new();
    let mut stale = Vec::new();

    for (rel, body) in CorePackage::files(core_pkg, config, core_lib) {
        let path = dir.join(rel);
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing == body => {}
            Ok(_) => stale.push(path),
            Err(_) => missing.push(path),
        }
    }

    Ok((missing, stale))
}
