//! MVCC Tier 3 coordinator for ForgeDB (#84).
//!
//! Schema-agnostic control plane: owns the conflict map (via `forgedb-txn`),
//! the LSN sequence, the opaque `_replication.log` (via `forgedb-changefeed`),
//! and grants serialized exclusive commit turns over a Unix domain socket.
//!
//! ## Architecture
//!
//! One `forgedb coordinate <root>` process per data directory.  Generated
//! writers connect over a Unix socket and follow a three-message turn protocol:
//!
//! 1. **`RequestTurn`** — client sends its write-set keys + read-snapshot LSN.
//!    The coordinator does a conflict check (`CommitSequencer::try_commit`), then
//!    either **`Grant { turn_id, reserved_lsn }`** (exclusive turn) or
//!    **`Nack { conflict_key }`** (retry required).
//!
//! 2. *Data-plane write* (by the client, after Grant) — the client writes to
//!    the shared column files + WAL on its own, then sends `Committed`.
//!
//! 3. **`Committed`** — client announces durability, hands opaque row bytes to
//!    the coordinator, which appends them to `_replication.log` and replies with
//!    **`Ack { lsn }`**.  The turn is released; the next queued client may proceed.
//!
//! ## Identity red line
//!
//! The coordinator NEVER writes a column, NEVER decodes `opaque_row_bytes`, and
//! NEVER interprets a model name as anything other than an opaque routing tag.
//! Every `unsafe` block is forbidden by clippy (`#![forbid(unsafe_code)]`).

#![forbid(unsafe_code)]

pub mod client;
pub mod server;

use serde::{Deserialize, Serialize};

// ── Wire protocol ─────────────────────────────────────────────────────────────

/// Messages FROM client TO coordinator.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    /// Request a commit turn.
    ///
    /// The coordinator conflict-checks `write_set_keys` against its in-memory
    /// conflict map (`CommitSequencer`), then either grants an exclusive turn or
    /// nacks.  No shared column state is touched here — that is the data plane,
    /// solely the client's responsibility.
    RequestTurn {
        /// Opaque conflict keys (row-id bytes + unique-claim bytes).  The
        /// coordinator compares them for equality only — never decodes them.
        write_set_keys: Vec<Vec<u8>>,
        /// The LSN of the read snapshot at which this transaction started.
        /// Any key committed strictly after this LSN is a conflict.
        snapshot_lsn: u64,
        /// The client's own I/O deadline, in milliseconds (#274).
        ///
        /// The coordinator clamps its grant wait to fit inside this, so a `Busy`
        /// reply always reaches the client before it stops reading.  Without it
        /// neither side could see the other's deadline and the operator was
        /// silently responsible for keeping two numbers — `--turn-timeout` and a
        /// constant compiled into the client — in a relationship nothing checked.
        ///
        /// `0` (the `serde` default, i.e. a **pre-#274 client**, which omits the
        /// field entirely) means "unknown — assume the legacy 35s".  Deliberately
        /// an additive field rather than a handshake message: the protocol is
        /// internally-tagged JSON with no version field (#277), so an unknown
        /// *variant* breaks whichever peer ships second, while an unknown *field*
        /// is ignored in both directions.
        #[serde(default)]
        client_deadline_ms: u64,
    },
    /// Announce that the data-plane column + WAL write is now durable.
    ///
    /// The coordinator appends the opaque payload to `_replication.log` and
    /// releases the outstanding turn.
    Committed {
        /// Must match the `turn_id` from the preceding `Grant`.
        turn_id: u64,
        /// Opaque model tags — forwarded verbatim to the broker, never decoded.
        model_tags: Vec<Vec<u8>>,
        /// Physical row indices, one per `(model_tag, opaque_row_bytes)` entry.
        row_indices: Vec<u64>,
        /// [`ChangeKind`] as a byte: 0=Inserted, 1=Updated, 2=Deleted, 3=Linked.
        change_kinds: Vec<u8>,
        /// Opaque committed row bytes — forwarded verbatim to the log.
        opaque_row_bytes: Vec<Vec<u8>>,
    },
    /// Client is done and closing the connection cleanly.
    Disconnect,
}

/// Messages FROM coordinator TO client.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    /// Exclusive commit turn granted.  The client may now perform the
    /// data-plane (column + WAL) write and subsequently send `Committed`.
    Grant {
        /// Opaque turn ID — must be echoed back in `Committed`.
        turn_id: u64,
        /// The LSN reserved for this commit (monotonically increasing).
        reserved_lsn: u64,
    },
    /// Write-set conflict detected; no turn was granted.
    ///
    /// The client must discard its staged writes, re-snapshot, and retry the
    /// full transaction from scratch.
    Nack {
        /// The first conflicting opaque key (for logging/debugging; the client
        /// MUST NOT interpret it).
        conflict_key: Vec<u8>,
    },
    /// The committed payload was recorded to the log; the turn is released.
    Ack {
        /// The LSN assigned to this commit in `_replication.log`.
        lsn: u64,
    },
    /// The coordinator is busy with another outstanding turn.
    ///
    /// The client should back off briefly and retry `RequestTurn`.
    Busy,
    /// A protocol-level error; the connection will be closed.
    Error { message: String },
}

// ── Length-framed JSON codec ──────────────────────────────────────────────────

use std::io::{self, Read};

/// Default maximum frame payload length: 16 MiB (#145). Configurable per
/// coordinator via [`server::CoordConfig::max_frame`]; the client-side decode of
/// (small) server responses keeps this default.
pub const DEFAULT_MAX_FRAME: usize = 16 * 1024 * 1024;

/// Encode a message as a length-framed JSON frame: `[4-byte BE length][json bytes]`.
pub fn encode_msg<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let payload = serde_json::to_vec(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = payload.len() as u32;
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Decode one length-framed JSON message from a reader, rejecting frames larger
/// than [`DEFAULT_MAX_FRAME`]. Used by the client to decode (small) server
/// responses; the coordinator's per-connection decode uses
/// [`decode_msg_with_limit`] with its configured cap (#145).
pub fn decode_msg<T: for<'de> Deserialize<'de>, R: Read>(reader: &mut R) -> io::Result<T> {
    decode_msg_with_limit(reader, DEFAULT_MAX_FRAME)
}

/// Decode one length-framed JSON message, rejecting frames larger than
/// `max_frame` (#145) — the coordinator threads its configured
/// [`server::CoordConfig::max_frame`] here to bound per-turn write-set memory.
pub fn decode_msg_with_limit<T: for<'de> Deserialize<'de>, R: Read>(
    reader: &mut R,
    max_frame: usize,
) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_frame {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("coordinator frame too large: {len} bytes (max {max_frame})"),
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_msg_with_limit_rejects_oversized_frame() {
        // #145: a frame within the limit decodes; one just over is rejected.
        let msg = ServerMsg::Busy;
        let encoded = encode_msg(&msg).unwrap();
        let payload_len = encoded.len() - 4;

        // Exactly at the payload length: accepted.
        let ok: ServerMsg = decode_msg_with_limit(&mut encoded.as_slice(), payload_len).unwrap();
        assert!(matches!(ok, ServerMsg::Busy));

        // One byte under the payload length: rejected as too large.
        let err = decode_msg_with_limit::<ServerMsg, _>(&mut encoded.as_slice(), payload_len - 1)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        // The default cap is 16 MiB.
        assert_eq!(DEFAULT_MAX_FRAME, 16 * 1024 * 1024);
    }

    #[test]
    fn coord_config_default_reproduces_prior_behavior() {
        // #144/#145: the default config is the pre-#126 behavior.
        use crate::server::{CoordConfig, CoordFsync, TURN_TIMEOUT};
        let d = CoordConfig::default();
        assert_eq!(d.fsync, CoordFsync::Always);
        assert_eq!(d.turn_timeout, TURN_TIMEOUT);
        assert_eq!(d.turn_timeout.as_secs(), 30);
        assert_eq!(d.max_frame, DEFAULT_MAX_FRAME);
    }

    #[test]
    fn roundtrip_client_request_turn() {
        let msg = ClientMsg::RequestTurn {
            write_set_keys: vec![b"row:0".to_vec(), b"unique:email:a@b.com".to_vec()],
            snapshot_lsn: 42,
            client_deadline_ms: 35_000,
        };
        let encoded = encode_msg(&msg).unwrap();
        let decoded: ClientMsg = decode_msg(&mut encoded.as_slice()).unwrap();
        match decoded {
            ClientMsg::RequestTurn { write_set_keys, snapshot_lsn, client_deadline_ms } => {
                assert_eq!(write_set_keys.len(), 2);
                assert_eq!(snapshot_lsn, 42);
                assert_eq!(client_deadline_ms, 35_000);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn roundtrip_server_grant() {
        let msg = ServerMsg::Grant {
            turn_id: 7,
            reserved_lsn: 100,
        };
        let encoded = encode_msg(&msg).unwrap();
        let decoded: ServerMsg = decode_msg(&mut encoded.as_slice()).unwrap();
        match decoded {
            ServerMsg::Grant { turn_id, reserved_lsn } => {
                assert_eq!(turn_id, 7);
                assert_eq!(reserved_lsn, 100);
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn roundtrip_committed() {
        let msg = ClientMsg::Committed {
            turn_id: 3,
            model_tags: vec![b"user".to_vec()],
            row_indices: vec![0],
            change_kinds: vec![0],
            opaque_row_bytes: vec![vec![1, 2, 3]],
        };
        let encoded = encode_msg(&msg).unwrap();
        let decoded: ClientMsg = decode_msg(&mut encoded.as_slice()).unwrap();
        match decoded {
            ClientMsg::Committed { turn_id, model_tags, .. } => {
                assert_eq!(turn_id, 3);
                assert_eq!(model_tags[0], b"user");
            }
            _ => panic!("unexpected variant"),
        }
    }
}
