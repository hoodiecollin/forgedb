use forgedb_changefeed::durable::{DurableBroker, FsyncPolicy};
use forgedb_coordinator::client::CoordinatorClient;
use forgedb_coordinator::server::{CoordFsync, Coordinator};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

fn start(fsync: CoordFsync) -> (TempDir, std::path::PathBuf, Arc<Coordinator>) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_owned();
    let sock = tmp.path().join("coord.sock");
    let coord = Arc::new(Coordinator::open_with_fsync(&root, &sock, fsync).expect("open"));
    let run = Arc::clone(&coord);
    std::thread::spawn(move || {
        let _ = run.run();
    });
    for _ in 0..200 {
        if sock.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    (tmp, sock, coord)
}

fn commit_n(sock: &std::path::Path, n: u64) -> Vec<u64> {
    let client = CoordinatorClient::connect(sock).expect("connect");
    let mut lsns = Vec::new();
    let mut snapshot = 0u64;
    for i in 0..n {
        let key = format!("row:{i}").into_bytes();
        let (turn_id, _reserved) = client.request_turn(vec![key], snapshot).expect("turn");
        let lsn = client
            .committed(
                turn_id,
                vec![b"user".to_vec()],
                vec![i],
                vec![0],
                vec![vec![i as u8, 2, 3]],
            )
            .expect("commit");
        lsns.push(lsn);
        snapshot = lsn;
    }
    lsns
}

fn assert_log_recorded(root: &std::path::Path, n: u64) {
    let log = root.join("_coordinator_replication.log");
    let broker = DurableBroker::open(&log, FsyncPolicy::Never, 64).expect("reopen log");
    assert_eq!(broker.watermark(), n, "log watermark == number of commits");
    let events = broker.read_from(0, n as usize + 8).expect("read log");
    assert_eq!(events.len() as u64, n, "one record per commit");
    for (i, ev) in events.iter().enumerate() {
        assert_eq!(ev.offset, i as u64 + 1, "monotonic offset in commit order");
        assert_eq!(ev.model, "user");
        assert_eq!(ev.bytes[0], i as u8, "opaque bytes recorded verbatim");
    }
}

fn run_mode(fsync: CoordFsync) {
    let (tmp, sock, coord) = start(fsync);
    let lsns = commit_n(&sock, 5);
    assert_eq!(lsns, vec![1, 2, 3, 4, 5], "LSNs monotonic across commits");
    assert_log_recorded(tmp.path(), 5);
    coord.shutdown();
}

#[test]
fn commit_records_in_order_with_fsync_always() {
    run_mode(CoordFsync::Always);
}

#[test]
fn commit_records_in_order_with_fsync_never() {
    run_mode(CoordFsync::Never);
}

#[test]
fn commit_records_in_order_with_fsync_periodic() {
    run_mode(CoordFsync::Periodic(2));
}

#[test]
fn default_fsync_is_always() {
    assert_eq!(CoordFsync::default(), CoordFsync::Always);
}
