use crate::{error::CliError, Result};
use std::path::PathBuf;
use std::process::Command;

pub struct LspOptions {
    pub server_path: Option<PathBuf>,
    pub args: Vec<String>,
}

const SERVER_BIN: &str = "forgedb-lsp";

pub fn run(options: LspOptions) -> Result<()> {
    let server = resolve_server(options.server_path)?;

    let mut cmd = Command::new(&server);
    cmd.args(&options.args);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        Err(CliError::Other(format!(
            "failed to launch '{}': {}",
            server.display(),
            err
        )))
    }

    #[cfg(not(unix))]
    {
        let status = cmd
            .status()
            .map_err(|e| CliError::Other(format!("failed to launch '{}': {}", server.display(), e)))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn resolve_server(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Ok(p);
        }
        return Err(CliError::Other(format!(
            "--server-path '{}' does not exist",
            p.display()
        )));
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let sibling = dir.join(exe_name(SERVER_BIN));
        if sibling.is_file() {
            return Ok(sibling);
        }
    }

    if let Some(found) = find_on_path(SERVER_BIN) {
        return Ok(found);
    }

    Err(CliError::Other(format!(
        "the ForgeDB language server ('{bin}') was not found next to `forgedb` or on PATH.\n\
         It ships alongside the `forgedb` CLI — reinstall from a release archive, build it with \
         `cargo build --features lsp --bin forgedb-lsp`, or pass `--server-path <path>`.",
        bin = exe_name(SERVER_BIN),
    )))
}

fn exe_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn find_on_path(bin: &str) -> Option<PathBuf> {
    let name = exe_name(bin);
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}
