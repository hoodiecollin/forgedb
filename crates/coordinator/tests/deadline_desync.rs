#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use forgedb_coordinator::client::CoordinatorClient;
use forgedb_coordinator::{ClientMsg, ServerMsg, decode_msg, encode_msg};

fn sock(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("coord.sock")
}

fn spawn_listener(
    path: &Path,
    delay_for: impl Fn(usize) -> Duration + Send + 'static,
    reply_for: impl Fn(usize) -> ServerMsg + Send + 'static,
) -> (thread::JoinHandle<()>, mpsc::Receiver<ClientMsg>) {
    let listener = UnixListener::bind(path).expect("bind fake coordinator");
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut n = 0usize;
        while let Ok((mut stream, _)) = listener.accept() {
            while let Ok(msg) = decode_msg::<ClientMsg, _>(&mut stream) {
                let stop = matches!(msg, ClientMsg::Disconnect);
                if tx.send(msg).is_err() {
                    return;
                }
                if stop {
                    break;
                }
                thread::sleep(delay_for(n));
                let frame = encode_msg(&reply_for(n)).expect("encode reply");
                if stream.write_all(&frame).is_err() {
                    break;
                }
                n += 1;
            }
        }
    });
    (handle, rx)
}

fn grant(turn_id: u64) -> ServerMsg {
    ServerMsg::Grant { turn_id, reserved_lsn: turn_id * 10 }
}

#[test]
fn timed_out_request_poisons_the_connection() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let (_h, _rx) = spawn_listener(&path, |_| Duration::from_millis(400), |_| grant(1));

    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(100)).unwrap();
    assert!(!client.is_poisoned(), "a fresh client must not be poisoned");

    let out = client.request_turn(vec![b"k".to_vec()], 0);
    assert!(out.is_err(), "the 100ms deadline must fire before the 400ms reply");
    assert!(
        client.is_poisoned(),
        "a timed-out round-trip leaves a reply in flight, so the connection is \
         unusable and must be marked so"
    );
}

#[test]
fn poisoned_client_never_reads_the_stranded_grant() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let (_h, rx) = spawn_listener(
        &path,
        |n| if n == 0 { Duration::from_millis(400) } else { Duration::ZERO },
        |n| grant(n as u64 + 1),
    );

    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(100)).unwrap();
    assert!(client.request_turn(vec![b"a".to_vec()], 0).is_err());

    thread::sleep(Duration::from_millis(500));

    let second = client.request_turn(vec![b"b".to_vec()], 0);
    assert!(
        second.is_err(),
        "the second request must refuse rather than consume the stranded Grant; \
         got {second:?}"
    );

    let sent: Vec<ClientMsg> = rx.try_iter().collect();
    assert_eq!(
        sent.len(),
        1,
        "a poisoned client must refuse BEFORE writing; the fake coordinator saw {sent:?}"
    );
}

#[test]
fn reconnect_clears_the_poison_and_discards_the_stranded_reply() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let (_h, _rx) = spawn_listener(
        &path,
        |n| if n == 0 { Duration::from_millis(400) } else { Duration::ZERO },
        |n| grant(n as u64 + 1),
    );

    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(150)).unwrap();
    assert!(client.request_turn(vec![b"a".to_vec()], 0).is_err());
    assert!(client.is_poisoned());
    thread::sleep(Duration::from_millis(400));

    client.reconnect().expect("reconnect to a live coordinator");
    assert!(!client.is_poisoned(), "reconnect must clear the flag");

    let (turn_id, lsn) = client
        .request_turn(vec![b"b".to_vec()], 0)
        .expect("a reconnected client must work");
    assert_eq!((turn_id, lsn), (2, 20), "must be this request's reply, not the stale (1, 10)");
}

#[test]
fn stranded_ack_is_discarded_with_the_socket() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let (_h, _rx) = spawn_listener(
        &path,
        |n| if n == 0 { Duration::from_millis(400) } else { Duration::ZERO },
        |_| ServerMsg::Ack { lsn: 7 },
    );

    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(150)).unwrap();
    assert!(
        client.committed(1, vec![], vec![], vec![], vec![]).is_err(),
        "the 150ms deadline must fire before the 400ms Ack"
    );
    thread::sleep(Duration::from_millis(400));

    client.reconnect().expect("reconnect");
    let lsn = client
        .committed(2, vec![], vec![], vec![], vec![])
        .expect("a reconnected client must be able to commit");
    assert_eq!(lsn, 7);
    assert_eq!(client.last_known_lsn(), 7);
}

#[test]
fn request_turn_declares_the_clients_own_deadline() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let (_h, rx) = spawn_listener(&path, |_| Duration::ZERO, |_| grant(1));

    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(2500)).unwrap();
    client.request_turn(vec![b"k".to_vec()], 3).unwrap();

    match rx.recv_timeout(Duration::from_secs(5)).expect("coordinator saw the request") {
        ClientMsg::RequestTurn { client_deadline_ms, snapshot_lsn, .. } => {
            assert_eq!(client_deadline_ms, 2500);
            assert_eq!(snapshot_lsn, 3);
        }
        other => panic!("expected RequestTurn, got {other:?}"),
    }
}

#[test]
fn default_connect_declares_the_documented_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let (_h, rx) = spawn_listener(&path, |_| Duration::ZERO, |_| grant(1));

    let client = CoordinatorClient::connect(&path).unwrap();
    client.request_turn(vec![], 0).unwrap();

    match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        ClientMsg::RequestTurn { client_deadline_ms, .. } => {
            assert_eq!(
                client_deadline_ms, 35_000,
                "the default must equal the legacy 35s the server's `0 =>` arm assumes; \
                 if this changes, `LEGACY_CLIENT_DEADLINE` in server.rs must be revisited"
            );
        }
        other => panic!("expected RequestTurn, got {other:?}"),
    }
}

#[test]
fn old_coordinator_ignores_the_new_field() {
    #[derive(serde::Deserialize, Debug)]
    #[serde(tag = "type")]
    enum LegacyClientMsg {
        RequestTurn {
            #[allow(dead_code)]
            write_set_keys: Vec<Vec<u8>>,
            snapshot_lsn: u64,
        },
        Committed {},
        Disconnect,
    }

    let frame = encode_msg(&ClientMsg::RequestTurn {
        write_set_keys: vec![b"k".to_vec()],
        snapshot_lsn: 9,
        client_deadline_ms: 1234,
    })
    .unwrap();

    let mut cursor = std::io::Cursor::new(&frame);
    let decoded: LegacyClientMsg = decode_msg(&mut cursor).expect("a pre-#274 decoder must cope");
    match decoded {
        LegacyClientMsg::RequestTurn { snapshot_lsn, .. } => assert_eq!(snapshot_lsn, 9),
        other => panic!("expected RequestTurn, got {other:?}"),
    }
}

#[test]
fn old_client_frame_decodes_to_the_legacy_sentinel() {
    let json = br#"{"type":"RequestTurn","write_set_keys":[],"snapshot_lsn":4}"#;
    let mut framed = (json.len() as u32).to_be_bytes().to_vec();
    framed.extend_from_slice(json);

    let mut cursor = std::io::Cursor::new(framed);
    let decoded: ClientMsg = decode_msg(&mut cursor).expect("serde(default) must fill the field");
    match decoded {
        ClientMsg::RequestTurn { client_deadline_ms, snapshot_lsn, .. } => {
            assert_eq!(client_deadline_ms, 0, "absent must mean the legacy sentinel");
            assert_eq!(snapshot_lsn, 4);
        }
        other => panic!("expected RequestTurn, got {other:?}"),
    }
}

#[test]
fn write_failure_poisons_the_connection() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let listener = UnixListener::bind(&path).unwrap();

    let client = CoordinatorClient::connect(&path).unwrap();
    drop(listener.accept().map(|(s, _)| s));
    drop(listener);

    let _ = client.request_turn(vec![b"k".to_vec()], 0);
    let _ = client.request_turn(vec![b"k".to_vec()], 0);
    assert!(
        client.is_poisoned(),
        "a dead peer must leave the connection marked unusable, not silently reusable"
    );
}

#[test]
fn failed_reconnect_leaves_the_client_poisoned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let listener = UnixListener::bind(&path).unwrap();
    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(100)).unwrap();

    let _ = client.request_turn(vec![b"k".to_vec()], 0);
    assert!(client.is_poisoned());

    drop(listener);
    std::fs::remove_file(&path).ok();

    assert!(client.reconnect().is_err(), "reconnect to a removed socket must fail");
    assert!(
        client.is_poisoned(),
        "a failed reconnect must NOT clear the flag — the stream is still the old, \
         desynchronized one"
    );
}

#[allow(dead_code)]
fn _unused(_: UnixStream) {}
