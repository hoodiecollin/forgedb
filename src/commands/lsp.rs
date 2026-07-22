use crate::{error::CliError, Result};
use std::path::PathBuf;
use std::process::Command;

/// `forgedb lsp` — run the ForgeDB language server over stdio.
///
/// The main `forgedb` CLI deliberately stays free of the async LSP stack
/// (`tokio` / `tower-lsp`): the language server lives in a **sibling
/// `forgedb-lsp` binary** that ships alongside `forgedb` on every distribution
/// channel (epic #173 WS4). This subcommand is a thin, synchronous launcher —
/// it resolves that binary and hands the process off to it, so `forgedb lsp`
/// and invoking `forgedb-lsp` directly are equivalent. Editor extensions may
/// use either; routing through `forgedb lsp` keeps binary resolution in one
/// place (here).
pub struct LspOptions {
    /// Explicit path to the `forgedb-lsp` binary (from `--server-path`). When
    /// unset, the server is resolved next to the running `forgedb`, then on PATH.
    pub server_path: Option<PathBuf>,
    /// Extra arguments forwarded verbatim to `forgedb-lsp`.
    pub args: Vec<String>,
}

const SERVER_BIN: &str = "forgedb-lsp";

pub fn run(options: LspOptions) -> Result<()> {
    let server = resolve_server(options.server_path)?;

    let mut cmd = Command::new(&server);
    cmd.args(&options.args);

    // Hand this process over to the language server. On Unix, `exec` *replaces*
    // the process image so no wrapper sits in the stdio pipe between the editor
    // and the server. On other platforms, spawn it and forward the exit code.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // `exec` only returns if the handoff itself fails.
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

/// Resolve the `forgedb-lsp` binary: explicit `--server-path` → sibling of the
/// running `forgedb` (the layout every release archive ships) → `forgedb-lsp`
/// on `PATH`. Returns an actionable error otherwise.
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

/// Platform-correct executable file name (`forgedb-lsp` / `forgedb-lsp.exe`).
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
