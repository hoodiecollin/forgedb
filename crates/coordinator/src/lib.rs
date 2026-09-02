#![forbid(unsafe_code)]

pub mod client;
pub mod server;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    RequestTurn {
        write_set_keys: Vec<Vec<u8>>,
        snapshot_lsn: u64,
        #[serde(default)]
        client_deadline_ms: u64,
    },
    Committed {
        turn_id: u64,
        model_tags: Vec<Vec<u8>>,
        row_indices: Vec<u64>,
        change_kinds: Vec<u8>,
        opaque_row_bytes: Vec<Vec<u8>>,
    },
    Disconnect,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Grant {
        turn_id: u64,
        reserved_lsn: u64,
    },
    Nack {
        conflict_key: Vec<u8>,
    },
    Ack {
        lsn: u64,
    },
    Busy,
    Error { message: String },
}

use std::io::{self, Read};

pub const DEFAULT_MAX_FRAME: usize = 16 * 1024 * 1024;

pub fn encode_msg<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let payload = serde_json::to_vec(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = payload.len() as u32;
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_msg<T: for<'de> Deserialize<'de>, R: Read>(reader: &mut R) -> io::Result<T> {
    decode_msg_with_limit(reader, DEFAULT_MAX_FRAME)
}

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
        let msg = ServerMsg::Busy;
        let encoded = encode_msg(&msg).unwrap();
        let payload_len = encoded.len() - 4;

        let ok: ServerMsg = decode_msg_with_limit(&mut encoded.as_slice(), payload_len).unwrap();
        assert!(matches!(ok, ServerMsg::Busy));

        let err = decode_msg_with_limit::<ServerMsg, _>(&mut encoded.as_slice(), payload_len - 1)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        assert_eq!(DEFAULT_MAX_FRAME, 16 * 1024 * 1024);
    }

    #[test]
    fn coord_config_default_reproduces_prior_behavior() {
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
