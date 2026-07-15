//! Schema-agnostic client for the Tier 3 coordinator Unix socket.
//!
//! The generated `CoordinatedDatabase` wraps this client; the client itself
//! knows nothing about models, fields, or schema — it only speaks the
//! coordinator wire protocol.

use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use crate::{ClientMsg, ServerMsg, decode_msg, encode_msg};

/// Connection timeout for coordinator socket operations.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// I/O timeout for individual read/write operations.
const IO_TIMEOUT: Duration = Duration::from_secs(35);

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
}

impl CoordinatorClient {
    /// Connect to a running coordinator at `socket_path`.
    pub fn connect(socket_path: &Path) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        Ok(CoordinatorClient {
            stream: Mutex::new(stream),
            last_known_lsn: Mutex::new(0),
        })
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
        let msg = ClientMsg::RequestTurn { write_set_keys, snapshot_lsn };
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
    fn send_recv<T>(&self, msg: &ClientMsg) -> io::Result<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let mut stream = self.stream.lock().unwrap();
        let frame = encode_msg(msg)?;
        stream.write_all(&frame)?;
        decode_msg(&mut *stream)
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
