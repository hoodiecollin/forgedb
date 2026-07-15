//! `forgedb coordinate <root>` — start the Tier 3 MVCC commit coordinator.
//!
//! Acquires an exclusive lock on the data directory, opens/creates the
//! replication log, seeds the sequencer from the watermark, then listens on a
//! Unix socket for multi-process writer connections.
//!
//! ## Usage
//!
//! ```text
//! forgedb coordinate ./data
//! forgedb coordinate ./data --socket ./data/_coord.sock
//! ```
//!
//! The socket path defaults to `<root>/_coord.sock`.  Generated writers connect
//! via `Database::connect_coordinator(socket_path)`.

use std::path::PathBuf;

use forgedb_coordinator::server::{Coordinator, ServerError};

use crate::error::{CliError, Result};

pub struct CoordinateOptions {
    /// Data root directory.
    pub root: PathBuf,
    /// Unix socket path.  Default: `<root>/_coord.sock`.
    pub socket: Option<PathBuf>,
}

pub fn run(opts: CoordinateOptions) -> Result<()> {
    let socket_path = opts
        .socket
        .unwrap_or_else(|| opts.root.join("_coord.sock"));

    eprintln!(
        "forgedb-coordinator: starting on root={} socket={}",
        opts.root.display(),
        socket_path.display()
    );

    let coord = Coordinator::open(&opts.root, &socket_path).map_err(|e| match e {
        ServerError::DirAlreadyLocked => CliError::Other(format!(
            "another coordinator is already running on {}",
            opts.root.display()
        )),
        ServerError::Io(io_err) => CliError::Io(io_err),
        ServerError::Shutdown => CliError::Other("coordinator shut down unexpectedly".into()),
    })?;

    eprintln!(
        "forgedb-coordinator: ready — listening on {}",
        socket_path.display()
    );

    // Install Ctrl-C handler to shut down cleanly.
    let coord_arc = std::sync::Arc::new(coord);
    let coord_clone = std::sync::Arc::clone(&coord_arc);
    ctrlc::set_handler(move || {
        eprintln!("\nforgedb-coordinator: shutting down…");
        coord_clone.shutdown();
    })
    .ok();

    coord_arc.run().map_err(|e| CliError::Other(format!("coordinator run error: {e}")))?;

    eprintln!("forgedb-coordinator: stopped.");
    Ok(())
}
