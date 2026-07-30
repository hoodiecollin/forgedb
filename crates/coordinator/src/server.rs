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

use crate::{ClientMsg, ServerMsg, decode_msg_with_limit, encode_msg};

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

/// When the coordinator fsyncs its durable replication log at commit (#156,
/// Option C). Configurable per deployment (`forgedb coordinate --fsync`);
/// default [`CoordFsync::Always`], which preserves the pre-#156 durability of
/// the replication log.
///
/// The coordinator's `_coordinator_replication.log` is a **resumable, secondary**
/// artifact: a coordinated client already fsync'd its own columns + WAL before
/// reporting `Committed`, and followers re-request from their watermark on a
/// coordinator crash — so trading this log's fsync for throughput never loses
/// committed client data, only rewinds replication. That is why `Never`/`Periodic`
/// are safe to offer (guardrail G7 still applies: they are explicit opt-ins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoordFsync {
    /// Fsync the replication log on every commit (max durability; the default).
    #[default]
    Always,
    /// Never fsync in the commit path — rely on the OS to flush the log. A
    /// coordinator crash rewinds the replication tail (no client data lost).
    Never,
    /// Fsync once per N commits (group commit): amortizes the barrier under load.
    Periodic(u64),
}

/// Coordinator tunables resolved at process start (#144/#145, epic #126). Not a
/// per-request or hot-reload knob — bound once when the coordinator opens. All
/// three fields are schema-blind (a coordinator interprets no schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordConfig {
    /// Replication-log fsync policy (#156).
    pub fsync: CoordFsync,
    /// How long a granted turn may remain un-committed before the coordinator
    /// reclaims it (#144). Governs multi-process write fairness vs head-of-line
    /// blocking. Default [`TURN_TIMEOUT`] (30s).
    pub turn_timeout: Duration,
    /// Maximum protocol frame size the coordinator will decode (#145) — bounds
    /// per-turn write-set size vs memory. Default [`DEFAULT_MAX_FRAME`] (16 MiB).
    pub max_frame: usize,
}

impl Default for CoordConfig {
    fn default() -> Self {
        Self {
            fsync: CoordFsync::default(),
            turn_timeout: TURN_TIMEOUT,
            max_frame: crate::DEFAULT_MAX_FRAME,
        }
    }
}

// ── Broker state (its own lock, off the turn critical section — #156 Option A) ──

/// The durable replication broker plus its fsync policy, behind a **separate**
/// mutex from [`CoordState`] (#156 Option A). The broker append + fsync barrier
/// runs here, OUTSIDE the turn/condvar critical section, so a committing client's
/// disk barrier no longer blocks other writers from being granted a turn. Broker
/// appends stay in commit order because a `Committed` handler holds this lock
/// across the turn release (see `handle_connection`), and only one turn is ever
/// outstanding.
struct BrokerState {
    broker: DurableBroker,
    fsync: CoordFsync,
    commits_since_flush: u64,
}

impl BrokerState {
    /// Append one commit's opaque records (in commit order) and fsync per policy.
    /// Runs under the broker mutex, never the coord/turn mutex.
    fn record_commit(
        &mut self,
        model_tags: &[Vec<u8>],
        row_indices: &[u64],
        change_kinds: &[u8],
        opaque_row_bytes: &[Vec<u8>],
    ) {
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
        // The broker is opened with `FsyncPolicy::Never` (record() does not fsync),
        // so the coordinator drives the barrier here per its configured mode —
        // one barrier per commit (or per N commits), never one per record.
        let should_flush = match self.fsync {
            CoordFsync::Always => true,
            CoordFsync::Never => false,
            CoordFsync::Periodic(k) => {
                self.commits_since_flush += 1;
                if self.commits_since_flush >= k.max(1) {
                    self.commits_since_flush = 0;
                    true
                } else {
                    false
                }
            }
        };
        if should_flush && let Err(e) = self.broker.flush() {
            log::warn!("coordinator: broker flush error: {e}");
        }
    }
}

/// Everything a connection handler shares: the turn state (coord mutex + condvar)
/// and the broker (its own mutex). Split so the broker fsync barrier is off the
/// turn critical section (#156 Option A).
struct Shared {
    coord: (Mutex<CoordState>, Condvar),
    broker: Mutex<BrokerState>,
    /// Max protocol frame the per-connection decoder accepts (#145). Read before
    /// locking `coord`, so it lives on `Shared` (not `CoordState`).
    max_frame: usize,
    /// Turn reclaim / read-timeout deadline (#144). Mirrored on `CoordState` for
    /// the reclaim path (a `CoordState` method); set once at open, so no drift.
    turn_timeout: Duration,
}

// ── Shared coordinator state (behind Mutex + Condvar) ────────────────────────

struct CoordState {
    seq: CommitSequencer,
    next_turn_id: u64,
    /// At most one outstanding granted turn.
    pending_turn: Option<PendingTurn>,
    /// Whether the server is shutting down.
    shutdown: bool,
    /// Turn reclaim deadline (#144); bound once at open.
    turn_timeout: Duration,
}

impl CoordState {
    fn new(seq: CommitSequencer, turn_timeout: Duration) -> Self {
        CoordState {
            seq,
            next_turn_id: 1,
            pending_turn: None,
            shutdown: false,
            turn_timeout,
        }
    }

    /// Reclaim a granted turn if the timeout has elapsed (client crashed/hung).
    fn reclaim_timed_out(&mut self) {
        if let Some(ref p) = self.pending_turn {
            if p.granted_at.elapsed() > self.turn_timeout {
                log::warn!(
                    "coordinator: reclaiming timed-out turn {} (>{:.1}s)",
                    p.turn_id,
                    self.turn_timeout.as_secs_f64(),
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
    /// Validate that `turn_id` is the current outstanding turn, release it (so a
    /// waiting writer can be granted immediately), and return its reserved LSN.
    /// The durable broker append + fsync barrier happens AFTER this, under the
    /// separate broker lock (#156 Option A) — never inside the turn critical
    /// section — so the barrier no longer blocks turn-granting.
    fn take_commit(&mut self, turn_id: u64) -> std::result::Result<u64, ServerMsg> {
        let lsn = match &self.pending_turn {
            Some(p) if p.turn_id == turn_id => p.reserved_lsn,
            Some(p) => {
                return Err(ServerMsg::Error {
                    message: format!("turn {} is not current (current is {})", turn_id, p.turn_id),
                });
            }
            None => {
                return Err(ServerMsg::Error {
                    message: format!("turn {} was not granted (no pending turn)", turn_id),
                });
            }
        };

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
        Ok(lsn)
    }
}

// ── Coordinator ───────────────────────────────────────────────────────────────

/// The Tier 3 coordinator server.
pub struct Coordinator {
    root: PathBuf,
    socket_path: PathBuf,
    state: Arc<Shared>,
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
        Self::open_with_config(root, socket_path, CoordConfig::default())
    }

    /// Like [`open`](Self::open) but with an explicit replication-log fsync policy
    /// (#156 Option C). `CoordFsync::Always` (the `open` default) preserves the
    /// pre-#156 durability; `Never`/`Periodic` trade replication-log durability
    /// for commit throughput (no committed client data is ever at risk — see
    /// [`CoordFsync`]).
    pub fn open_with_fsync(root: &Path, socket_path: &Path, fsync: CoordFsync) -> Result<Self> {
        Self::open_with_config(
            root,
            socket_path,
            CoordConfig {
                fsync,
                ..CoordConfig::default()
            },
        )
    }

    /// Like [`open`](Self::open) but with an explicit [`CoordConfig`] (#144/#145,
    /// epic #126): the replication-log fsync policy, the turn-reclaim timeout, and
    /// the max protocol frame. `CoordConfig::default()` reproduces the prior
    /// behavior (Always fsync, 30s turn timeout, 16 MiB frame cap).
    pub fn open_with_config(
        root: &Path,
        socket_path: &Path,
        config: CoordConfig,
    ) -> Result<Self> {
        let fsync = config.fsync;
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

        // Open the durable replication log with `FsyncPolicy::Never` so `record()`
        // does NOT fsync per event; the coordinator drives the barrier once per
        // commit (or per N, or never) via `CoordFsync` (#156). This also fixes a
        // latent inefficiency — the old `Always` broker fsync'd once per record
        // AND once per explicit flush (N+1 barriers per commit); now it is at most
        // one barrier per commit even in `Always` mode.
        let log_path = root.join("_coordinator_replication.log");
        let broker = DurableBroker::open(&log_path, FsyncPolicy::Never, 1024)
            .map_err(io::Error::other)?;

        // Seed the sequencer from the broker's current watermark so LSNs
        // continue monotonically across coordinator restarts.
        let watermark = broker.watermark();
        let seq = CommitSequencer::new(watermark);

        log::info!(
            "coordinator: opened root={} socket={} watermark={} fsync={:?}",
            root.display(),
            socket_path.display(),
            watermark,
            fsync,
        );

        Ok(Coordinator {
            root: root.to_owned(),
            socket_path: socket_path.to_owned(),
            state: Arc::new(Shared {
                coord: (
                    Mutex::new(CoordState::new(seq, config.turn_timeout)),
                    Condvar::new(),
                ),
                broker: Mutex::new(BrokerState {
                    broker,
                    fsync,
                    commits_since_flush: 0,
                }),
                max_frame: config.max_frame,
                turn_timeout: config.turn_timeout,
            }),
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
                    let (lock, _) = &self.state.coord;
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
        let (lock, cvar) = &self.state.coord;
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
    state: Arc<Shared>,
) -> io::Result<()> {
    // Set a read timeout so a hung client cannot block this thread forever (#144).
    stream.set_read_timeout(Some(state.turn_timeout))?;

    loop {
        let msg: ClientMsg = match decode_msg_with_limit(&mut stream, state.max_frame) {
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
                // #156 Option A: acquire the BROKER lock first (held across the
                // turn release), then briefly the coord lock to validate + release
                // the turn and notify waiters. Ordering: because the broker lock is
                // held from before the turn release until after this commit's fsync,
                // and only one turn is ever outstanding, broker appends stay in
                // commit order — while the NEXT writer's turn + client I/O overlap
                // this commit's replication barrier (which is now off the turn path).
                let mut broker = state.broker.lock().unwrap();
                let (lock, cvar) = &state.coord;
                let reply = {
                    let mut s = lock.lock().unwrap();
                    match s.take_commit(turn_id) {
                        Ok(lsn) => {
                            // Coord lock is dropped at the end of this block; wake
                            // waiters so a new turn can be granted DURING the fsync.
                            drop(s);
                            cvar.notify_all();
                            // Durable append + fsync (per policy) under the broker
                            // lock only — not the turn lock.
                            broker.record_commit(
                                &model_tags,
                                &row_indices,
                                &change_kinds,
                                &opaque_row_bytes,
                            );
                            ServerMsg::Ack { lsn }
                        }
                        Err(err) => err,
                    }
                };
                drop(broker);
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
    state: &Arc<Shared>,
    write_set_keys: Vec<Vec<u8>>,
    snapshot_lsn: u64,
) -> ServerMsg {
    let (lock, cvar) = &state.coord;
    let deadline = Instant::now() + state.turn_timeout;

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
        let (lock, _) = &coord.state.coord;
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

        // Commit the first turn (release + LSN; the broker append is off the turn
        // lock in the handler — #156 Option A — so `take_commit` only returns lsn).
        let lsn = s.take_commit(turn_id).expect("commit");
        assert_eq!(lsn, reserved_lsn);

        // Now another request succeeds (different key — no conflict).
        let reply2 = s.try_request_turn(vec![b"row:1".to_vec()], reserved_lsn);
        assert!(matches!(reply2, ServerMsg::Grant { .. }));
    }

    #[test]
    fn conflict_detection() {
        let tmp = TempDir::new().unwrap();
        let coord = make_coordinator(&tmp);
        let (lock, _) = &coord.state.coord;
        let mut s = lock.lock().unwrap();

        // Commit key "row:0" at snapshot_lsn=0.
        let r1 = s.try_request_turn(vec![b"row:0".to_vec()], 0);
        let turn_id = match r1 {
            ServerMsg::Grant { turn_id, .. } => turn_id,
            _ => panic!(),
        };
        let committed_lsn = s.take_commit(turn_id).expect("commit");

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
