use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::{ClientMsg, ServerMsg, decode_msg, encode_msg};

pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(35);

pub const POISONED_MSG: &str = "coordinator connection unusable after a failed request — reopen required";

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

pub struct CoordinatorClient {
    stream: Mutex<UnixStream>,
    last_known_lsn: Mutex<u64>,
    socket_path: PathBuf,
    io_timeout: Duration,
    poisoned: AtomicBool,
}

impl CoordinatorClient {
    pub fn connect(socket_path: &Path) -> io::Result<Self> {
        Self::connect_with_io_timeout(socket_path, DEFAULT_IO_TIMEOUT)
    }

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

    fn dial(socket_path: &Path, io_timeout: Duration) -> io::Result<UnixStream> {
        let stream = UnixStream::connect(socket_path)?;
        stream.set_read_timeout(Some(io_timeout))?;
        stream.set_write_timeout(Some(io_timeout))?;
        Ok(stream)
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
    }

    pub fn reconnect(&self) -> io::Result<()> {
        let mut stream = self.stream.lock().unwrap();
        let fresh = Self::dial(&self.socket_path, self.io_timeout)?;
        if let Ok(frame) = encode_msg(&ClientMsg::Disconnect) {
            let _ = stream.write_all(&frame);
        }
        *stream = fresh;
        self.poisoned.store(false, Ordering::Release);
        Ok(())
    }

    pub fn last_known_lsn(&self) -> u64 {
        *self.last_known_lsn.lock().unwrap()
    }

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
        if let Ok(mut s) = self.stream.lock() {
            if let Ok(frame) = encode_msg(&ClientMsg::Disconnect) {
                let _ = s.write_all(&frame);
            }
        }
    }
}
