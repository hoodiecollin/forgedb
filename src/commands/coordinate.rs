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
//! via `Database::connect(root, socket_path)`, which establishes the turn-channel
//! before opening the data dir lock-free (T3-5).

use std::path::PathBuf;

use std::time::Duration;

use forgedb_coordinator::DEFAULT_MAX_FRAME;
use forgedb_coordinator::server::{CoordConfig, CoordFsync, Coordinator, ServerError, TURN_TIMEOUT};

use crate::error::{CliError, Result};

pub struct CoordinateOptions {
    /// Data root directory.
    pub root: PathBuf,
    /// Unix socket path.  Default: `<root>/_coord.sock`.
    pub socket: Option<PathBuf>,
    /// Replication-log fsync policy (#156): `always` | `never` | `periodic`.
    /// `None` → env `FORGEDB_COORDINATOR_FSYNC` → default `always`.
    pub fsync: Option<String>,
    /// Commits per fsync for `periodic` mode.  `None` → env
    /// `FORGEDB_COORDINATOR_FSYNC_INTERVAL` → default 64.
    pub fsync_interval: Option<u64>,
    /// Turn-reclaim / read timeout, seconds (#144). `None` → env
    /// `FORGEDB_COORDINATOR_TURN_TIMEOUT` → default 30.
    pub turn_timeout_secs: Option<u64>,
    /// Max protocol frame, MiB (#145). `None` → env
    /// `FORGEDB_COORDINATOR_MAX_FRAME_MIB` → default 16.
    pub max_frame_mib: Option<u64>,
}

/// Resolve the coordinator fsync mode: CLI flag > env > default (`always`).
fn resolve_fsync(opts: &CoordinateOptions) -> Result<CoordFsync> {
    let mode = opts
        .fsync
        .clone()
        .or_else(|| std::env::var("FORGEDB_COORDINATOR_FSYNC").ok())
        .unwrap_or_else(|| "always".to_string())
        .to_lowercase();
    let interval = opts
        .fsync_interval
        .or_else(|| {
            std::env::var("FORGEDB_COORDINATOR_FSYNC_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(64);
    match mode.as_str() {
        "always" => Ok(CoordFsync::Always),
        "never" => Ok(CoordFsync::Never),
        "periodic" => Ok(CoordFsync::Periodic(interval.max(1))),
        other => Err(CliError::Other(format!(
            "invalid --fsync '{other}' — expected always | never | periodic"
        ))),
    }
}

/// Resolve the turn-reclaim timeout (#144): CLI flag > env > default (30s).
fn resolve_turn_timeout(opts: &CoordinateOptions) -> Duration {
    let secs = opts
        .turn_timeout_secs
        .or_else(|| {
            std::env::var("FORGEDB_COORDINATOR_TURN_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .map(|s: u64| s.max(1));
    match secs {
        Some(s) => Duration::from_secs(s),
        None => TURN_TIMEOUT,
    }
}

/// Resolve the max protocol frame (#145): CLI flag (MiB) > env (MiB) > default
/// (16 MiB).
fn resolve_max_frame(opts: &CoordinateOptions) -> usize {
    let mib = opts.max_frame_mib.or_else(|| {
        std::env::var("FORGEDB_COORDINATOR_MAX_FRAME_MIB")
            .ok()
            .and_then(|s| s.parse().ok())
    });
    match mib {
        Some(m) => (m.max(1) as usize).saturating_mul(1024 * 1024),
        None => DEFAULT_MAX_FRAME,
    }
}

pub fn run(opts: CoordinateOptions) -> Result<()> {
    let socket_path = opts
        .socket
        .clone()
        .unwrap_or_else(|| opts.root.join("_coord.sock"));
    let fsync = resolve_fsync(&opts)?;
    let turn_timeout = resolve_turn_timeout(&opts);
    let max_frame = resolve_max_frame(&opts);
    let config = CoordConfig {
        fsync,
        turn_timeout,
        max_frame,
    };

    eprintln!(
        "forgedb-coordinator: starting on root={} socket={} fsync={fsync:?} \
         turn_timeout={}s max_frame={}MiB",
        opts.root.display(),
        socket_path.display(),
        turn_timeout.as_secs(),
        max_frame / (1024 * 1024),
    );

    let coord = Coordinator::open_with_config(&opts.root, &socket_path, config).map_err(|e| match e {
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
