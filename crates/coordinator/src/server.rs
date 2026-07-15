//! Coordinator server — Unix-socket listener that serializes multi-process
//! commit turns for Tier 3 MVCC (#84).
//!
//! ## What the server owns (schema-agnostic control plane)
//!
//! - The **#89 single-writer `DirLock`** on the data directory, held on behalf
//!   of all coordinated clients (Tier 3 mode-switch, spec T3-5): an exclusive
//!   advisory `fs2` lock on `<root>/.forgedb.lock` — the *exact same file*
//!   `forgedb_storage::DirLock::acquire` locks. Holding it means (a) a second
//!   coordinator is refused, and (b) a *standalone* writer (which self-acquires
//!   the `DirLock` in `open_at`) is mutually excluded — so "coordinated" and
//!   "standalone" modes can never both run. Coordinated clients therefore open
//!   **lock-free** (`_lock: None`); their write mutual-exclusion comes from the
//!   serialized turn-grant, not the file lock. This is pure filesystem interop
//!   on an opaque path — the coordinator gains NO `forgedb-storage*` dependency
//!   (T3-8): it never opens a column, it only advisory-locks a known path.
//! - A [`CommitSequencer`] seeded from the broker watermark, so LSNs continue
//!   monotonically across restarts.
//! - A [`DurableBroker`] for `_coordinator_replication.log` — the cross-process
//!   durable log that remote read-replica followers resume from.
//! - The pending-turn slot (at most one outstanding `Grant` at a time).
//!
//! ## What the server does NOT own (data plane — stays in the writer process)
//!
//! - Column files (`FixedColumn`, `VariableColumn`, `Tombstones`).
//! - The per-model WAL.
//! - Any record field or schema knowledge.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use fs2::FileExt;
use forgedb_changefeed::ChangeKind;
use forgedb_changefeed::durable::{DurableBroker, FsyncPolicy};
use forgedb_txn::{CommitOutcome, CommitSequencer, Lsn, WriteSet};
use thiserror::Error;

use crate::{ClientMsg, ServerMsg, decode_msg, encode_msg};

/// How long a granted turn may remain un-committed before the coordinator
/// reclaims it.  A client that crashes or hangs holding a turn loses it after
/// this deadline, un-wedging all other writers.
pub const TURN_TIMEOUT: Duration = Duration::from_secs(30);

/// The data-directory single-writer lock filename.
///
/// **Load-bearing cross-crate contract:** this MUST byte-match the filename
/// `forgedb_storage::DirLock::acquire` locks (`storage-native/src/dir_lock.rs`,
/// `<root>/.forgedb.lock`). The coordinator advisory-locks the *same* file so a
/// standalone writer and a coordinator are mutually exclusive (spec T3-5). The
/// constant is duplicated rather than imported because the coordinator must not
/// depend on `forgedb-storage*` (T3-8) — a filesystem contract, the same class
/// of coupling as the on-disk column format the backup crate reads. A parity
/// test in each crate pins the string so a rename cannot silently desync.
pub const DIR_LOCK_FILENAME: &str = ".forgedb.lock";

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error(
        "data directory is already locked (another coordinator, or a standalone \
         ForgeDB writer, already holds <root>/.forgedb.lock)"
    )]
    DirAlreadyLocked,
    #[error("coordinator is shutting down")]
    Shutdown,
}

pub type Result<T> = std::result::Result<T, ServerError>;

// ── Pending-turn state ────────────────────────────────────────────────────────

struct PendingTurn {
    turn_id: u64,
    reserved_lsn: u64,
    granted_at: Instant,
}

// ── Shared coordinator state (behind Mutex + Condvar) ────────────────────────

struct CoordState {
    seq: CommitSequencer,
    broker: DurableBroker,
    next_turn_id: u64,
    /// At most one outstanding granted turn.
    pending_turn: Option<PendingTurn>,
    /// Whether the server is shutting down.
    shutdown: bool,
}

impl CoordState {
    fn new(seq: CommitSequencer, broker: DurableBroker) -> Self {
        CoordState {
            seq,
            broker,
            next_turn_id: 1,
            pending_turn: None,
            shutdown: false,
        }
    }

    /// Reclaim a granted turn if the timeout has elapsed (client crashed/hung).
    fn reclaim_timed_out(&mut self) {
        if let Some(ref p) = self.pending_turn {
            if p.granted_at.elapsed() > TURN_TIMEOUT {
                log::warn!(
                    "coordinator: reclaiming timed-out turn {} (>{:.1}s)",
                    p.turn_id,
                    TURN_TIMEOUT.as_secs_f64(),
                );
                self.pending_turn = None;
            }
        }
    }

    /// Conflict-check the write-set and, if clean and no turn is outstanding,
    /// grant an exclusive turn.  Returns the message to send to the client.
    fn try_request_turn(&mut self, keys: Vec<Vec<u8>>, snapshot_lsn: u64) -> ServerMsg {
        self.reclaim_timed_out();

        if self.pending_turn.is_some() {
            return ServerMsg::Busy;
        }

        let opaque_keys: Vec<forgedb_txn::OpaqueKey> = keys
            .into_iter()
            .map(|k| k.into_boxed_slice())
            .collect();
        let ws = WriteSet {
            keys: opaque_keys,
            snapshot_lsn: Lsn(snapshot_lsn),
        };

        match self.seq.try_commit(&ws) {
            CommitOutcome::Committed(lsn) => {
                let turn_id = self.next_turn_id;
                self.next_turn_id += 1;
                let reserved_lsn = lsn.as_u64();
                self.pending_turn = Some(PendingTurn {
                    turn_id,
                    reserved_lsn,
                    granted_at: Instant::now(),
                });
                ServerMsg::Grant { turn_id, reserved_lsn }
            }
            CommitOutcome::Conflict { key } => {
                ServerMsg::Nack { conflict_key: key.into_vec() }
            }
        }
    }

    /// Record the committed payload to `_coordinator_replication.log` and
    /// release the turn.  Returns the message to send to the client.
    fn commit(
        &mut self,
        turn_id: u64,
        model_tags: Vec<Vec<u8>>,
        row_indices: Vec<u64>,
        change_kinds: Vec<u8>,
        opaque_row_bytes: Vec<Vec<u8>>,
    ) -> ServerMsg {
        // Validate that this is the current outstanding turn.
        let lsn = match &self.pending_turn {
            Some(p) if p.turn_id == turn_id => p.reserved_lsn,
            Some(p) => {
                return ServerMsg::Error {
                    message: format!(
                        "turn {} is not current (current is {})",
                        turn_id, p.turn_id
                    ),
                };
            }
            None => {
                return ServerMsg::Error {
                    message: format!("turn {} was not granted (no pending turn)", turn_id),
                };
            }
        };

        // Append to the durable replication log — opaque bytes, never decoded.
        let n = model_tags.len().min(row_indices.len()).min(opaque_row_bytes.len());
        for i in 0..n {
            let model_name = String::from_utf8_lossy(&model_tags[i]).into_owned();
            let row_index = row_indices[i];
            let kind_byte = *change_kinds.get(i).unwrap_or(&0);
            let kind = ChangeKind::from_byte(kind_byte).unwrap_or(ChangeKind::Inserted);
            let bytes = opaque_row_bytes[i].clone();
            if let Err(e) = self.broker.record(&model_name, row_index, kind, bytes) {
                log::warn!("coordinator: broker record error: {e}");
            }
        }
        if let Err(e) = self.broker.flush() {
            log::warn!("coordinator: broker flush error: {e}");
        }

        // Release the turn.
        self.pending_turn = None;
        // Note: we intentionally do NOT call seq.gc() here.  The coordinator
        // never calls register_snapshot / release_snapshot on behalf of clients
        // (clients report snapshot_lsn in RequestTurn, but we don't track live
        // client snapshots), so live_snapshots is always empty.  Calling gc()
        // would clear the entire conflict map, destroying future conflict detection.
        // The conflict map grows monotonically with committed keys — bounded by the
        // number of unique rows and unique-key claims ever written, which is
        // acceptable at v1 application-database scale.

        ServerMsg::Ack { lsn }
    }
}

// ── Coordinator ───────────────────────────────────────────────────────────────

/// The Tier 3 coordinator server.
pub struct Coordinator {
    root: PathBuf,
    socket_path: PathBuf,
    state: Arc<(Mutex<CoordState>, Condvar)>,
    _lock_file: File,
}

// Manual Debug impl to avoid requiring DurableBroker/CommitSequencer to implement Debug.
impl std::fmt::Debug for Coordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coordinator")
            .field("root", &self.root)
            .field("socket_path", &self.socket_path)
            .finish()
    }
}

impl Coordinator {
    /// Acquire the data-directory lock, open/create the replication log, seed
    /// the `CommitSequencer` from the watermark, and return a `Coordinator`
    /// ready to call [`run`](Coordinator::run).
    ///
    /// Returns `Err(DirAlreadyLocked)` if another coordinator process already
    /// holds the lock on this directory.
    pub fn open(root: &Path, socket_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;

        // Acquire the #89 single-writer lock (spec T3-5): the SAME
        // `<root>/.forgedb.lock` file `forgedb_storage::DirLock` locks. Holding
        // it excludes both a second coordinator AND a standalone writer, so the
        // two modes are mutually exclusive. Coordinated clients open lock-free.
        let lock_path = root.join(DIR_LOCK_FILENAME);
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        lock_file
            .try_lock_exclusive()
            .map_err(|_| ServerError::DirAlreadyLocked)?;

        // Open the durable replication log.
        let log_path = root.join("_coordinator_replication.log");
        let broker = DurableBroker::open(&log_path, FsyncPolicy::Always, 1024)
            .map_err(io::Error::other)?;

        // Seed the sequencer from the broker's current watermark so LSNs
        // continue monotonically across coordinator restarts.
        let watermark = broker.watermark();
        let seq = CommitSequencer::new(watermark);

        log::info!(
            "coordinator: opened root={} socket={} watermark={}",
            root.display(),
            socket_path.display(),
            watermark,
        );

        Ok(Coordinator {
            root: root.to_owned(),
            socket_path: socket_path.to_owned(),
            state: Arc::new((Mutex::new(CoordState::new(seq, broker)), Condvar::new())),
            _lock_file: lock_file,
        })
    }

    /// Run the coordinator: bind the Unix socket, accept connections, dispatch
    /// each to a handler thread.  Blocks until the process is killed or
    /// `shutdown()` is called.
    pub fn run(&self) -> Result<()> {
        // Remove a stale socket file if present.
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;
        log::info!("coordinator: listening on {}", self.socket_path.display());

        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let state = Arc::clone(&self.state);
                    std::thread::spawn(move || {
                        if let Err(e) = handle_connection(s, state) {
                            // EOF on disconnect is normal.
                            if !is_eof(&e) {
                                log::warn!("coordinator: connection error: {e}");
                            }
                        }
                    });
                }
                Err(e) => {
                    let (lock, _) = &*self.state;
                    if lock.lock().unwrap().shutdown {
                        break;
                    }
                    log::warn!("coordinator: accept error: {e}");
                }
            }
        }
        Ok(())
    }

    /// Signal the coordinator to stop accepting new connections.
    pub fn shutdown(&self) {
        let (lock, cvar) = &*self.state;
        let mut s = lock.lock().unwrap();
        s.shutdown = true;
        cvar.notify_all();
        // Wake the accept loop by connecting and immediately disconnecting.
        let _ = UnixStream::connect(&self.socket_path);
    }
}

fn is_eof(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::UnexpectedEof
        || e.kind() == io::ErrorKind::ConnectionReset
        || e.kind() == io::ErrorKind::BrokenPipe
}

// ── Per-connection handler ─────────────────────────────────────────────────────

/// Handle one client connection: read `ClientMsg`s and reply with `ServerMsg`s.
///
/// The handler acquires the shared `(Mutex<CoordState>, Condvar)` ONLY at
/// critical points (request-turn, commit).  Blocking waits for the turn mutex
/// use `Condvar::wait_timeout` so a stuck client cannot deadlock others.
fn handle_connection(
    mut stream: UnixStream,
    state: Arc<(Mutex<CoordState>, Condvar)>,
) -> io::Result<()> {
    // Set a read timeout so a hung client cannot block this thread forever.
    stream.set_read_timeout(Some(TURN_TIMEOUT))?;

    loop {
        let msg: ClientMsg = match decode_msg(&mut stream) {
            Ok(m) => m,
            Err(e) if is_eof(&e) => break,
            Err(e) => return Err(e),
        };

        match msg {
            ClientMsg::RequestTurn { write_set_keys, snapshot_lsn } => {
                // Spin briefly (with back-off) waiting for the pending turn to
                // clear rather than immediately returning Busy to every client.
                let reply = request_turn_with_wait(
                    &state,
                    write_set_keys,
                    snapshot_lsn,
                );
                let frame = encode_msg(&reply)?;
                stream.write_all(&frame)?;
            }

            ClientMsg::Committed {
                turn_id,
                model_tags,
                row_indices,
                change_kinds,
                opaque_row_bytes,
            } => {
                let (lock, cvar) = &*state;
                let reply = {
                    let mut s = lock.lock().unwrap();
                    s.commit(turn_id, model_tags, row_indices, change_kinds, opaque_row_bytes)
                };
                // Notify waiting clients that the turn slot is now free.
                cvar.notify_all();
                let frame = encode_msg(&reply)?;
                stream.write_all(&frame)?;
            }

            ClientMsg::Disconnect => {
                break;
            }
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
    Ok(())
}

/// Attempt `RequestTurn` with bounded wait-on-condvar backpressure:
/// if the coordinator is busy, wait up to `TURN_TIMEOUT` for the current
/// turn to clear before returning `Busy`.
fn request_turn_with_wait(
    state: &Arc<(Mutex<CoordState>, Condvar)>,
    write_set_keys: Vec<Vec<u8>>,
    snapshot_lsn: u64,
) -> ServerMsg {
    let (lock, cvar) = state.as_ref();
    let deadline = Instant::now() + TURN_TIMEOUT;

    let mut s = lock.lock().unwrap();
    loop {
        // Check for timeout first.
        let reply = s.try_request_turn(write_set_keys.clone(), snapshot_lsn);
        match reply {
            ServerMsg::Busy => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return ServerMsg::Busy;
                }
                let (guard, timeout) = cvar.wait_timeout(s, remaining).unwrap();
                s = guard;
                if timeout.timed_out() {
                    return ServerMsg::Busy;
                }
                // Loop and try again.
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_coordinator(tmp: &TempDir) -> Coordinator {
        let root = tmp.path().to_owned();
        let sock = tmp.path().join("coord.sock");
        Coordinator::open(&root, &sock).expect("open coordinator")
    }

    #[test]
    fn double_lock_refused() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_owned();
        let sock1 = tmp.path().join("coord1.sock");
        let sock2 = tmp.path().join("coord2.sock");
        let _c1 = Coordinator::open(&root, &sock1).expect("first coordinator");
        let err = Coordinator::open(&root, &sock2).unwrap_err();
        assert!(matches!(err, ServerError::DirAlreadyLocked));
    }

    /// G2 (PM re-gate #84 guard) — the coordinator holds the SAME
    /// `<root>/.forgedb.lock` a standalone writer's `DirLock` would take, so a
    /// standalone writer is mutually excluded while a coordinator runs. We can't
    /// depend on `forgedb-storage*` (T3-8), so we replay exactly what
    /// `DirLock::acquire` does — an `fs2` exclusive advisory lock on that file —
    /// and assert it is refused (`WouldBlock`).
    #[test]
    fn coordinator_lock_excludes_standalone_writer() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_owned();
        let sock = tmp.path().join("coord.sock");
        let _coord = Coordinator::open(&root, &sock).expect("open coordinator");

        // Simulate `forgedb_storage::DirLock::acquire(root)` byte-for-byte.
        let standalone = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join(DIR_LOCK_FILENAME))
            .unwrap();
        let err = standalone.try_lock_exclusive().unwrap_err();
        assert_eq!(
            err.kind(),
            io::ErrorKind::WouldBlock,
            "a standalone writer must be refused while the coordinator holds the lock"
        );
    }

    /// G3 (PM re-gate #84 guard) — the coordinator's lock filename MUST byte-match
    /// the one `forgedb_storage::DirLock` uses (`storage-native/src/dir_lock.rs`:
    /// `<root>/.forgedb.lock`). The two are duplicated (not imported) because the
    /// coordinator must not depend on `forgedb-storage*` (T3-8); this test pins the
    /// string so a rename on either side is caught. The mirror assertion lives in
    /// `storage-native`'s `dir_lock` tests.
    #[test]
    fn lock_filename_matches_storage_dirlock() {
        assert_eq!(DIR_LOCK_FILENAME, ".forgedb.lock");
    }

    #[test]
    fn grant_and_commit_through_state() {
        let tmp = TempDir::new().unwrap();
        let coord = make_coordinator(&tmp);
        let (lock, _) = &*coord.state;
        let mut s = lock.lock().unwrap();

        // Empty write-set at snapshot_lsn=0 → should grant.
        let reply = s.try_request_turn(vec![b"row:0".to_vec()], 0);
        let (turn_id, reserved_lsn) = match reply {
            ServerMsg::Grant { turn_id, reserved_lsn } => (turn_id, reserved_lsn),
            other => panic!("expected Grant, got {:?}", other),
        };
        assert_eq!(reserved_lsn, 1); // first commit at Lsn(1)

        // While turn is held, another request is Busy.
        let busy = s.try_request_turn(vec![b"row:1".to_vec()], 0);
        assert!(matches!(busy, ServerMsg::Busy));

        // Commit the first turn.
        let ack = s.commit(
            turn_id,
            vec![b"user".to_vec()],
            vec![0],
            vec![0],
            vec![vec![1, 2, 3]],
        );
        assert!(matches!(ack, ServerMsg::Ack { lsn } if lsn == reserved_lsn));

        // Now another request succeeds (different key — no conflict).
        let reply2 = s.try_request_turn(vec![b"row:1".to_vec()], reserved_lsn);
        assert!(matches!(reply2, ServerMsg::Grant { .. }));
    }

    #[test]
    fn conflict_detection() {
        let tmp = TempDir::new().unwrap();
        let coord = make_coordinator(&tmp);
        let (lock, _) = &*coord.state;
        let mut s = lock.lock().unwrap();

        // Commit key "row:0" at snapshot_lsn=0.
        let r1 = s.try_request_turn(vec![b"row:0".to_vec()], 0);
        let turn_id = match r1 {
            ServerMsg::Grant { turn_id, .. } => turn_id,
            _ => panic!(),
        };
        let ServerMsg::Ack { lsn: committed_lsn } =
            s.commit(turn_id, vec![], vec![], vec![], vec![])
        else {
            panic!()
        };

        // Another transaction with snapshot_lsn=0 (predates the commit) on the same key
        // → conflict.
        let r2 = s.try_request_turn(vec![b"row:0".to_vec()], 0);
        assert!(
            matches!(r2, ServerMsg::Nack { .. }),
            "should conflict: committed_lsn={committed_lsn}"
        );

        // With snapshot_lsn=committed_lsn (after the commit) → no conflict.
        let r3 = s.try_request_turn(vec![b"row:0".to_vec()], committed_lsn);
        assert!(matches!(r3, ServerMsg::Grant { .. }));
    }
}
