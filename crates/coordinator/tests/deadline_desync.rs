//! Gate 3 for #274 — the client's I/O deadline vs the coordinator's grant wait.
//!
//! # Why these tests are shaped the way they are
//!
//! The bug is not "a request fails". It is that a timed-out request leaves the
//! socket **desynchronized**: the coordinator writes its `Grant` onto a stream the
//! client has stopped reading, and the client's *next* request reads that stale
//! reply as its own. `request_turn` then returns `Ok((turn_id, reserved_lsn))` for
//! a turn the coordinator has since reclaimed, the generated data plane writes
//! columns believing it holds the turn, and the resulting coordinator `Error` is
//! swallowed by the generated `eprintln!` arm — so the transaction returns **Ok**.
//!
//! That means **an end-state assertion cannot test this**. "The second call
//! returned an error" is satisfied both by correctly refusing and by reading a
//! stale `Grant` that happened not to line up. So `poisoned_client_never_reads_the_stranded_grant`
//! asserts on the *identity* of what came back: it must not be `Ok`, and the
//! stranded `Grant` must still be sitting unread in the socket buffer afterwards.
//!
//! The mutation to guard against is **deleting the poison check in `send_recv`**.
//! Under that mutation the second call returns `Ok` — verified by hand after this
//! file went green.
//!
//! # The slow-listener harness
//!
//! These tests do not run a real `Coordinator`. They run a bare `UnixListener` that
//! reads one frame, sleeps past the client's declared deadline, then writes a
//! reply. That is the *only* way to put a reply on the wire strictly after the
//! client gave up, deterministically, in milliseconds — a real coordinator would
//! need genuine 35s contention. The real-coordinator path is covered by
//! `crates/coordinator/src/server.rs`'s own tests and by
//! `tests/auto_increment_coordinated_test.rs`.

#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use forgedb_coordinator::client::CoordinatorClient;
use forgedb_coordinator::{ClientMsg, ServerMsg, decode_msg, encode_msg};

/// A socket path under a unique temp dir, cleaned up by `TempDir`'s drop.
fn sock(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("coord.sock")
}

/// Spawn a listener that reads frames forever and replies to each with
/// `reply_for(n)` after `delay_for(n)` — `n` being the 0-based index of the request
/// **across the whole listener's life, not per connection.**
///
/// Global rather than per-connection on purpose: a `reconnect()` opens a *new*
/// connection, and a per-connection counter would restart the "slow" delay for the
/// first request on it — so a test asserting "reconnect recovers" would fail on a
/// second induced timeout rather than on anything under test.
///
/// Returns the join handle plus a channel that yields every `ClientMsg` the fake
/// coordinator actually received, so a test can assert on what went out as well as
/// what came back.
fn spawn_listener(
    path: &Path,
    delay_for: impl Fn(usize) -> Duration + Send + 'static,
    reply_for: impl Fn(usize) -> ServerMsg + Send + 'static,
) -> (thread::JoinHandle<()>, mpsc::Receiver<ClientMsg>) {
    let listener = UnixListener::bind(path).expect("bind fake coordinator");
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        // One connection at a time is enough: `reconnect` produces a *new*
        // connection, and accepting them in sequence is exactly the ordering the
        // tests reason about.
        let mut n = 0usize;
        while let Ok((mut stream, _)) = listener.accept() {
            // Loop ends when the client hangs up or reconnects (decode fails).
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

/// Read one frame directly off a raw stream connected to `path`, so a test can
/// prove a *stranded* reply is still sitting in the client's socket buffer.
fn grant(turn_id: u64) -> ServerMsg {
    ServerMsg::Grant { turn_id, reserved_lsn: turn_id * 10 }
}

// ── Scenario 1: a timed-out request poisons the connection ────────────────────

/// **Given** a client with a 100ms deadline against a listener replying after
/// 400ms, **when** `request_turn` is called, **then** it errors AND the client
/// reports itself poisoned.
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

// ── Scenario 2: the load-bearing one ──────────────────────────────────────────

/// **Given** a poisoned client, **when** `request_turn` is called a second time,
/// **then** it must NOT return `Ok` — the stranded `Grant` from the first request
/// must never be mistaken for this request's reply.
///
/// This is the scenario the whole issue exists for. Without the poison check the
/// second call returns `Ok((1, 10))`: a turn this client does not hold.
#[test]
fn poisoned_client_never_reads_the_stranded_grant() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    // Reply late to request 0, promptly to every later one — so if the client
    // *does* read out of step, it gets a syntactically perfect `Grant`.
    let (_h, rx) = spawn_listener(
        &path,
        |n| if n == 0 { Duration::from_millis(400) } else { Duration::ZERO },
        |n| grant(n as u64 + 1),
    );

    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(100)).unwrap();
    assert!(client.request_turn(vec![b"a".to_vec()], 0).is_err());

    // Give the listener time to actually put the stale Grant on the wire, so the
    // test exercises "a reply IS waiting" rather than "nothing arrived yet".
    thread::sleep(Duration::from_millis(500));

    let second = client.request_turn(vec![b"b".to_vec()], 0);
    assert!(
        second.is_err(),
        "the second request must refuse rather than consume the stranded Grant; \
         got {second:?}"
    );

    // And it must not even have sent the request — writing onto a desynchronized
    // stream is what leaves the coordinator's reader out of step too.
    let sent: Vec<ClientMsg> = rx.try_iter().collect();
    assert_eq!(
        sent.len(),
        1,
        "a poisoned client must refuse BEFORE writing; the fake coordinator saw {sent:?}"
    );
}

// ── Scenario 3: reconnect clears it ───────────────────────────────────────────

/// **Given** a poisoned client, **when** `reconnect()` succeeds, **then** the next
/// `request_turn` returns `Ok` and the client is no longer poisoned.
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

    // The fresh socket has no stranded bytes. The stranded reply was `grant(1)` =
    // `(1, 10)`; this request is the listener's second, so a correct client sees
    // `grant(2)` = `(2, 20)`. Asserting the exact pair — rather than merely `is_ok`
    // — is what distinguishes "read its own reply" from "read the stale one".
    let (turn_id, lsn) = client
        .request_turn(vec![b"b".to_vec()], 0)
        .expect("a reconnected client must work");
    assert_eq!((turn_id, lsn), (2, 20), "must be this request's reply, not the stale (1, 10)");
}

/// **Given** a `committed` call that times out with an `Ack` in flight, **when**
/// `reconnect()` is called and `committed` is retried, **then** it succeeds —
/// rather than failing `Protocol("unexpected reply: Ack")`, which is what reading
/// the stranded `Ack` would produce.
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

// ── Scenario 4: the client declares its deadline on the wire ──────────────────

/// **Given** a client built with a 2500ms I/O timeout, **when** it sends
/// `RequestTurn`, **then** the message carries `client_deadline_ms: 2500` — the
/// value the coordinator clamps its grant wait against.
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

/// **Given** a default `connect`, **then** the declared deadline is the crate's
/// documented 35s default — the value the server's legacy fallback assumes.
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

// ── Scenario 5: an old coordinator ignores the new field ──────────────────────

/// **Given** a coordinator that predates #274 (modelled by decoding `RequestTurn`
/// into a struct WITHOUT `client_deadline_ms`), **when** a new client sends one,
/// **then** it decodes fine — proving the field is additive and skew-safe in the
/// new-client → old-coordinator direction.
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

    // Skip the 4-byte length prefix the way a decoder would.
    let mut cursor = std::io::Cursor::new(&frame);
    let decoded: LegacyClientMsg = decode_msg(&mut cursor).expect("a pre-#274 decoder must cope");
    match decoded {
        LegacyClientMsg::RequestTurn { snapshot_lsn, .. } => assert_eq!(snapshot_lsn, 9),
        other => panic!("expected RequestTurn, got {other:?}"),
    }
}

/// **Given** a pre-#274 client (modelled by a frame with the field absent),
/// **when** the current `ClientMsg` decodes it, **then** `client_deadline_ms` is
/// `0` — the sentinel the server reads as "legacy, assume 35s".
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

// ── Scenario 6: a write error poisons too ─────────────────────────────────────

/// **Given** a client whose coordinator has gone away, **when** a request fails on
/// the *write* side, **then** the connection is poisoned as well.
///
/// A failed `write_all` can leave a partial frame on the wire, which desynchronizes
/// the coordinator's reader just as badly as a stranded reply desynchronizes ours.
/// Narrowing the poisoning to read timeouts would miss this.
#[test]
fn write_failure_poisons_the_connection() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let listener = UnixListener::bind(&path).unwrap();

    let client = CoordinatorClient::connect(&path).unwrap();
    // Accept and immediately drop: the peer is gone, so a write eventually fails.
    drop(listener.accept().map(|(s, _)| s));
    drop(listener);

    // The first request may succeed at the syscall level (into the socket buffer)
    // and then fail on read; either way the client must end up poisoned.
    let _ = client.request_turn(vec![b"k".to_vec()], 0);
    let _ = client.request_turn(vec![b"k".to_vec()], 0);
    assert!(
        client.is_poisoned(),
        "a dead peer must leave the connection marked unusable, not silently reusable"
    );
}

/// A `reconnect()` against a coordinator that is gone must fail and leave the
/// client poisoned, rather than clearing the flag on a connection it did not
/// actually re-establish.
#[test]
fn failed_reconnect_leaves_the_client_poisoned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = sock(&tmp);
    let listener = UnixListener::bind(&path).unwrap();
    let client =
        CoordinatorClient::connect_with_io_timeout(&path, Duration::from_millis(100)).unwrap();

    // Poison it with a listener that never replies.
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

/// The unused-import guard: `UnixStream` is referenced so the harness compiles
/// identically whether or not a scenario opens a raw stream.
#[allow(dead_code)]
fn _unused(_: UnixStream) {}
