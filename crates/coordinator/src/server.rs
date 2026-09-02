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

pub const TURN_TIMEOUT: Duration = Duration::from_secs(30);

pub const GRANT_REPLY_MARGIN: Duration = Duration::from_millis(500);

const LEGACY_CLIENT_DEADLINE: Duration = Duration::from_secs(35);

pub const DIR_LOCK_FILENAME: &str = ".forgedb.lock";

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

struct PendingTurn {
    turn_id: u64,
    reserved_lsn: u64,
    granted_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoordFsync {
    #[default]
    Always,
    Never,
    Periodic(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordConfig {
    pub fsync: CoordFsync,
    pub turn_timeout: Duration,
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

struct BrokerState {
    broker: DurableBroker,
    fsync: CoordFsync,
    commits_since_flush: u64,
}

impl BrokerState {
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

struct Shared {
    coord: (Mutex<CoordState>, Condvar),
    broker: Mutex<BrokerState>,
    max_frame: usize,
    turn_timeout: Duration,
}

struct CoordState {
    seq: CommitSequencer,
    next_turn_id: u64,
    pending_turn: Option<PendingTurn>,
    shutdown: bool,
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

        self.pending_turn = None;
        Ok(lsn)
    }
}

pub struct Coordinator {
    root: PathBuf,
    socket_path: PathBuf,
    state: Arc<Shared>,
    _lock_file: File,
}

impl std::fmt::Debug for Coordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coordinator")
            .field("root", &self.root)
            .field("socket_path", &self.socket_path)
            .finish()
    }
}

impl Coordinator {
    pub fn open(root: &Path, socket_path: &Path) -> Result<Self> {
        Self::open_with_config(root, socket_path, CoordConfig::default())
    }

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

    pub fn open_with_config(
        root: &Path,
        socket_path: &Path,
        config: CoordConfig,
    ) -> Result<Self> {
        let fsync = config.fsync;
        std::fs::create_dir_all(root)?;

        let lock_path = root.join(DIR_LOCK_FILENAME);
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;
        lock_file
            .try_lock_exclusive()
            .map_err(|_| ServerError::DirAlreadyLocked)?;

        let log_path = root.join("_coordinator_replication.log");
        let broker = DurableBroker::open(&log_path, FsyncPolicy::Never, 1024)
            .map_err(io::Error::other)?;

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

    pub fn run(&self) -> Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;
        log::info!("coordinator: listening on {}", self.socket_path.display());

        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let state = Arc::clone(&self.state);
                    std::thread::spawn(move || {
                        if let Err(e) = handle_connection(s, state) {
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

    pub fn shutdown(&self) {
        let (lock, cvar) = &self.state.coord;
        let mut s = lock.lock().unwrap();
        s.shutdown = true;
        cvar.notify_all();
        let _ = UnixStream::connect(&self.socket_path);
    }
}

fn is_eof(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::UnexpectedEof
        || e.kind() == io::ErrorKind::ConnectionReset
        || e.kind() == io::ErrorKind::BrokenPipe
}

fn handle_connection(
    mut stream: UnixStream,
    state: Arc<Shared>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(state.turn_timeout))?;

    loop {
        let msg: ClientMsg = match decode_msg_with_limit(&mut stream, state.max_frame) {
            Ok(m) => m,
            Err(e) if is_eof(&e) => break,
            Err(e) => return Err(e),
        };

        match msg {
            ClientMsg::RequestTurn { write_set_keys, snapshot_lsn, client_deadline_ms } => {
                let reply = request_turn_with_wait(
                    &state,
                    write_set_keys,
                    snapshot_lsn,
                    client_deadline_ms,
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
                let mut broker = state.broker.lock().unwrap();
                let (lock, cvar) = &state.coord;
                let reply = {
                    let mut s = lock.lock().unwrap();
                    match s.take_commit(turn_id) {
                        Ok(lsn) => {
                            drop(s);
                            cvar.notify_all();
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

fn effective_grant_wait(turn_timeout: Duration, client_deadline_ms: u64) -> Duration {
    let declared = match client_deadline_ms {
        0 => LEGACY_CLIENT_DEADLINE,
        ms => Duration::from_millis(ms),
    };
    turn_timeout.min(declared.saturating_sub(GRANT_REPLY_MARGIN))
}

fn request_turn_with_wait(
    state: &Arc<Shared>,
    write_set_keys: Vec<Vec<u8>>,
    snapshot_lsn: u64,
    client_deadline_ms: u64,
) -> ServerMsg {
    let (lock, cvar) = &state.coord;
    let deadline = Instant::now() + effective_grant_wait(state.turn_timeout, client_deadline_ms);

    let mut s = lock.lock().unwrap();
    loop {
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
            }
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn clamp_prefers_the_clients_deadline_when_shorter() {
        let w = effective_grant_wait(Duration::from_secs(60), 10_000);
        assert_eq!(w, Duration::from_millis(9_500), "10s declared − 500ms margin");
    }

    #[test]
    fn clamp_never_extends_past_turn_timeout() {
        let w = effective_grant_wait(Duration::from_secs(5), 60_000);
        assert_eq!(w, Duration::from_secs(5));
    }

    #[test]
    fn clamp_fixes_a_legacy_client_that_declares_nothing() {
        let w = effective_grant_wait(Duration::from_secs(300), 0);
        assert_eq!(
            w,
            Duration::from_millis(34_500),
            "a declaration-less client must be treated as the legacy 35s, not as \
             consenting to turn_timeout"
        );
    }

    #[test]
    fn clamp_saturates_instead_of_underflowing() {
        assert_eq!(effective_grant_wait(Duration::from_secs(30), 200), Duration::ZERO);
        assert_eq!(effective_grant_wait(Duration::from_secs(30), 500), Duration::ZERO);
    }

    #[test]
    fn legacy_assumption_matches_the_live_client_default() {
        assert_eq!(LEGACY_CLIENT_DEADLINE, crate::client::DEFAULT_IO_TIMEOUT);
    }

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

    #[test]
    fn coordinator_lock_excludes_standalone_writer() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_owned();
        let sock = tmp.path().join("coord.sock");
        let _coord = Coordinator::open(&root, &sock).expect("open coordinator");

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

        let reply = s.try_request_turn(vec![b"row:0".to_vec()], 0);
        let (turn_id, reserved_lsn) = match reply {
            ServerMsg::Grant { turn_id, reserved_lsn } => (turn_id, reserved_lsn),
            other => panic!("expected Grant, got {:?}", other),
        };
        assert_eq!(reserved_lsn, 1);

        let busy = s.try_request_turn(vec![b"row:1".to_vec()], 0);
        assert!(matches!(busy, ServerMsg::Busy));

        let lsn = s.take_commit(turn_id).expect("commit");
        assert_eq!(lsn, reserved_lsn);

        let reply2 = s.try_request_turn(vec![b"row:1".to_vec()], reserved_lsn);
        assert!(matches!(reply2, ServerMsg::Grant { .. }));
    }

    #[test]
    fn conflict_detection() {
        let tmp = TempDir::new().unwrap();
        let coord = make_coordinator(&tmp);
        let (lock, _) = &coord.state.coord;
        let mut s = lock.lock().unwrap();

        let r1 = s.try_request_turn(vec![b"row:0".to_vec()], 0);
        let turn_id = match r1 {
            ServerMsg::Grant { turn_id, .. } => turn_id,
            _ => panic!(),
        };
        let committed_lsn = s.take_commit(turn_id).expect("commit");

        let r2 = s.try_request_turn(vec![b"row:0".to_vec()], 0);
        assert!(
            matches!(r2, ServerMsg::Nack { .. }),
            "should conflict: committed_lsn={committed_lsn}"
        );

        let r3 = s.try_request_turn(vec![b"row:0".to_vec()], committed_lsn);
        assert!(matches!(r3, ServerMsg::Grant { .. }));
    }
}
