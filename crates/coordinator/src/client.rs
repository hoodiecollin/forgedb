//! Schema-agnostic client for the Tier 3 coordinator Unix socket.
//!
//! The generated `CoordinatedDatabase` wraps this client; the client itself
//! knows nothing about models, fields, or schema — it only speaks the
//! coordinator wire protocol.

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::{ClientMsg, ServerMsg, decode_msg, encode_msg};

/// Default I/O timeout for individual read/write operations.
///
/// This value is **declared to the coordinator** on every `RequestTurn`
/// (`client_deadline_ms`), which clamps its grant wait to fit inside it (#274).
/// Before #274 the coordinator could not see it, so raising `--turn-timeout` past
/// this value silently desynchronized the connection.
pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(35);

/// The message carried by the [`ClientError::Io`] a poisoned client returns.
///
/// Stable and public so a caller can distinguish "this connection is unusable,
/// call `reconnect`" from an ordinary I/O failure without a new `ClientError`
/// variant — which would be a breaking change to a non-`non_exhaustive` public
/// enum, and would invalidate the `forgedb-coordinator = "0.2"` scaffold pin in
/// five generated `Cargo.toml`s for no behavioral gain.
pub const POISONED_MSG: &str = "coordinator connection unusable after a failed request — reopen required";

/// Error type for coordinator client operations.
#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Conflict { conflict_key: Vec<u8> },
    Busy,
    Protocol(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Io(e) => write!(f, "coordinator I/O: {e}"),
            ClientError::Conflict { .. } => write!(f, "write-set conflict — retry required"),
            ClientError::Busy => write!(f, "coordinator busy — retry later"),
            ClientError::Protocol(s) => write!(f, "coordinator protocol error: {s}"),
        }
    }
}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> Self {
        ClientError::Io(e)
    }
}

/// Schema-agnostic client for one coordinator connection.
///
/// Thread-safe: the inner stream is protected by a `Mutex`.  All messages are
/// synchronous (blocking I/O); the coordinator protocol is strictly sequential
/// — one request, one reply — so no interleaving can occur.
pub struct CoordinatorClient {
    stream: Mutex<UnixStream>,
    /// The LSN of the last `Ack` this client observed.  Updated after each
    /// successful `Committed` round-trip so the next `RequestTurn` carries a
    /// fresh snapshot LSN.
    last_known_lsn: Mutex<u64>,
    /// The socket path, retained so [`reconnect`](Self::reconnect) can re-dial
    /// without the caller having to hold it.
    socket_path: PathBuf,
    /// This client's own I/O deadline — declared to the coordinator on every
    /// `RequestTurn` so it can clamp its grant wait to fit (#274).
    io_timeout: Duration,
    /// Set when a request fails, rendering the connection unusable (#274).
    ///
    /// A timeout leaves the coordinator's reply **in flight**: it will still be
    /// written onto this socket, where the next request would read it as its own
    /// answer.  A stale `Grant` read that way is the worst case — the client would
    /// perform its data-plane write believing it holds a turn the coordinator has
    /// already reclaimed, and the resulting `Error` on `Committed` is not fatal to
    /// the generated caller.  So the connection fails closed until
    /// [`reconnect`](Self::reconnect) replaces the stream.
    poisoned: AtomicBool,
}

impl CoordinatorClient {
    /// Connect to a running coordinator at `socket_path` with the default
    /// [`DEFAULT_IO_TIMEOUT`].
    pub fn connect(socket_path: &Path) -> io::Result<Self> {
        Self::connect_with_io_timeout(socket_path, DEFAULT_IO_TIMEOUT)
    }

    /// Connect with an explicit I/O deadline.
    ///
    /// The value is declared to the coordinator on every `RequestTurn`, which
    /// clamps its grant wait to `min(turn_timeout, io_timeout - margin)` so a
    /// `Busy` reply always beats this deadline (#274).  Lowering it therefore
    /// makes the client give up sooner *and* makes the coordinator answer sooner —
    /// the two stay coupled, which is the whole point.
    pub fn connect_with_io_timeout(socket_path: &Path, io_timeout: Duration) -> io::Result<Self> {
        let stream = Self::dial(socket_path, io_timeout)?;
        Ok(CoordinatorClient {
            stream: Mutex::new(stream),
            last_known_lsn: Mutex::new(0),
            socket_path: socket_path.to_owned(),
            io_timeout,
            poisoned: AtomicBool::new(false),
        })
    }

    /// Open one stream with both timeouts applied.  Shared by `connect` and
    /// `reconnect` so a replacement socket can never silently revert to blocking
    /// forever.
    fn dial(socket_path: &Path, io_timeout: Duration) -> io::Result<UnixStream> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(io_timeout))?;
        stream.set_write_timeout(Some(io_timeout))?;
        Ok(stream)
    }

    /// Whether this connection has been rendered unusable by a failed request.
    ///
    /// A poisoned client refuses every request until [`reconnect`](Self::reconnect)
    /// succeeds.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
    }

    /// Replace the socket, discarding any reply stranded on the old one, and clear
    /// the poison flag.
    ///
    /// Takes `&self` so it works through the `Arc` the generated
    /// `CoordinatedDatabase` holds — recovery policy stays in generated code,
    /// beside the `Busy` budget and retry limit that already live there, rather
    /// than being decided by this substrate.
    ///
    /// On failure the flag is **left set**: the stream is still the old,
    /// desynchronized one.
    pub fn reconnect(&self) -> io::Result<()> {
        let mut stream = self.stream.lock().unwrap();
        let fresh = Self::dial(&self.socket_path, self.io_timeout)?;
        // Best-effort `Disconnect` on the way out: the coordinator would notice
        // eventually (EOF, or `reclaim_timed_out`), but telling it now releases any
        // turn still held in this client's name — which is exactly the ghost turn
        // that would otherwise make the next request `Busy` for `turn_timeout`.
        if let Ok(frame) = encode_msg(&ClientMsg::Disconnect) {
            let _ = stream.write_all(&frame);
        }
        *stream = fresh;
        self.poisoned.store(false, Ordering::Release);
        Ok(())
    }

    /// The highest LSN this client has seen committed.  Use as `snapshot_lsn`
    /// for the next `RequestTurn`.
    pub fn last_known_lsn(&self) -> u64 {
        *self.last_known_lsn.lock().unwrap()
    }

    /// Request a commit turn for the given write-set keys + snapshot LSN.
    ///
    /// Returns `Ok(Grant { turn_id, reserved_lsn })` on success, or one of the
    /// error variants on conflict/busy/IO failure.
    pub fn request_turn(
        &self,
        write_set_keys: Vec<Vec<u8>>,
        snapshot_lsn: u64,
    ) -> Result<(u64, u64), ClientError> {
        let msg = ClientMsg::RequestTurn {
            write_set_keys,
            snapshot_lsn,
            client_deadline_ms: self.io_timeout.as_millis().min(u64::MAX as u128) as u64,
        };
        let reply: ServerMsg = self.send_recv(&msg)?;
        match reply {
            ServerMsg::Grant { turn_id, reserved_lsn } => Ok((turn_id, reserved_lsn)),
            ServerMsg::Nack { conflict_key } => Err(ClientError::Conflict { conflict_key }),
            ServerMsg::Busy => Err(ClientError::Busy),
            ServerMsg::Error { message } => Err(ClientError::Protocol(message)),
            other => Err(ClientError::Protocol(format!("unexpected reply: {other:?}"))),
        }
    }

    /// Announce that the data-plane write is durable and hand the opaque payload
    /// to the coordinator for the replication log.
    ///
    /// Returns the committed LSN on success.
    pub fn committed(
        &self,
        turn_id: u64,
        model_tags: Vec<Vec<u8>>,
        row_indices: Vec<u64>,
        change_kinds: Vec<u8>,
        opaque_row_bytes: Vec<Vec<u8>>,
    ) -> Result<u64, ClientError> {
        let msg = ClientMsg::Committed {
            turn_id,
            model_tags,
            row_indices,
            change_kinds,
            opaque_row_bytes,
        };
        let reply: ServerMsg = self.send_recv(&msg)?;
        match reply {
            ServerMsg::Ack { lsn } => {
                *self.last_known_lsn.lock().unwrap() = lsn;
                Ok(lsn)
            }
            ServerMsg::Error { message } => Err(ClientError::Protocol(message)),
            other => Err(ClientError::Protocol(format!("unexpected reply: {other:?}"))),
        }
    }

    /// Send `msg` and receive one reply.  Serialized under the stream mutex.
    ///
    /// **Fails closed** (#274): the poison flag is checked while holding the stream
    /// mutex — checking it outside would let two threads race between observing
    /// "not poisoned" and acquiring the stream — and is set on *any* error, not just
    /// a read timeout.  A failed `write_all` can leave a partial frame on the wire,
    /// which desynchronizes the coordinator's reader just as badly as a stranded
    /// reply desynchronizes ours.
    fn send_recv<T>(&self, msg: &ClientMsg) -> io::Result<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let mut stream = self.stream.lock().unwrap();
        if self.poisoned.load(Ordering::Acquire) {
            return Err(io::Error::other(POISONED_MSG));
        }
        let result = (|| {
            let frame = encode_msg(msg)?;
            stream.write_all(&frame)?;
            decode_msg(&mut *stream)
        })();
        if result.is_err() {
            self.poisoned.store(true, Ordering::Release);
        }
        result
    }
}

impl Drop for CoordinatorClient {
    fn drop(&mut self) {
        // Best-effort disconnect notification.
        if let Ok(mut s) = self.stream.lock() {
            if let Ok(frame) = encode_msg(&ClientMsg::Disconnect) {
                let _ = s.write_all(&frame);
            }
        }
    }
}
