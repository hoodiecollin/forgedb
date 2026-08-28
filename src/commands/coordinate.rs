use std::path::PathBuf;

use std::time::Duration;

use forgedb_coordinator::DEFAULT_MAX_FRAME;
use forgedb_coordinator::server::{CoordConfig, CoordFsync, Coordinator, ServerError, TURN_TIMEOUT};

use crate::error::{CliError, Result};

pub struct CoordinateOptions {
    pub root: PathBuf,
    pub socket: Option<PathBuf>,
    pub fsync: Option<String>,
    pub fsync_interval: Option<u64>,
    pub turn_timeout_secs: Option<u64>,
    pub max_frame_mib: Option<u64>,
}

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
