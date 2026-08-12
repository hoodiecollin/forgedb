//! Multi-process proof for #260: a **bare** integer auto — neither the model's
//! identity nor `&unique` — is conflict-visible through its own write-set class.
//!
//! # Why this is a separate file from `auto_increment_coordinated_test`
//!
//! That file pins #187 decision 6: an integer auto that *is* the identity is safe
//! because its row key (`b"r"`) already enters the write-set. This file pins the
//! shape decision 6 **refused** — `seq: +u64` beside a `+uuid` identity — which is
//! safe only because #260 adds a third class:
//!
//! ```text
//! b"s" ++ model ++ field ++ value
//! ```
//!
//! Same mechanism, different key. Keeping them apart means a regression in one
//! cannot be masked by the other still passing.
//!
//! # The two scenarios, and why the second one exists
//!
//! **Phase 1 — distinctness.** Two processes allocate concurrently; every committed
//! `seq` must be globally distinct. This is the correctness claim.
//!
//! **Phase 2 — convergence.** A claim key makes a collision *detected*, but detection
//! alone does not make the retry *terminate*. `CoordinatorClient::last_known_lsn`
//! advances only on a client's own `Ack`, so a `Nack` does not trip the peer-refresh
//! gate, and a naive retry merely re-runs the closure — walking the counter forward
//! one value per attempt. A writer 60 values behind would need 60 retries.
//!
//! So phase 2 gives the lagging writer a retry budget of **3** against a **60**-value
//! gap. Brute force cannot pass it; only fast-forwarding off the `Nack`ed key can.
//! The assertion is deliberately on the retry *budget* rather than on the committed
//! value, because a generous budget converges either way and would prove nothing.
//!
//! Compiles a generated crate and spawns processes, so it is `#[ignore]`d out of the
//! fast default suite:
//!
//! ```bash
//! make auto-increment-test
//! ```

#![cfg(unix)]

mod common;

/// The shape #187 refused: an integer auto that is neither the identity nor unique.
/// If this schema stops parsing, the #260 parser relaxation has regressed.
const SCHEMA: &str = r#"
Ticket {
  id: +uuid
  seq: +u64
  title: string
}
"#;

#[test]
#[ignore = "compiles a generated crate and spawns processes; run with --ignored"]
fn bare_integer_auto_is_conflict_visible_across_processes() {
    let (out, proj) = common::generate_compile_run("seqclaim", SCHEMA, DRIVER);
    common::assert_driver_ok(&out, &proj, "bare integer auto duplicated across processes");
}

const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use forgedb_types::Uuid;
use std::path::{Path, PathBuf};

const PER_WRITER: u64 = 40;
const BULK: u64 = 60;
/// Deliberately far below `BULK`: a writer that walks its counter one value per
/// attempt cannot close a 60-value gap in 3 tries. Only a fast-forward can.
const LAG_RETRIES: u32 = 3;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("data dir"));
    let forgedb = PathBuf::from(std::env::args().nth(2).expect("forgedb binary"));

    match std::env::args().nth(3).as_deref() {
        Some("writer") => writer(dir),
        Some("bulk") => bulk(dir),
        Some("lag") => lag(dir),
        _ => parent(dir, forgedb),
    }
}

fn connect(dir: &Path) -> CoordinatedDatabase {
    Database::connect(dir.to_path_buf(), dir.join("_coord.sock")).expect("connect to coordinator")
}

/// Phase 1 writer: commit `PER_WRITER` rows, racing a peer.
fn writer(dir: PathBuf) {
    let db = connect(&dir);
    for i in 0..PER_WRITER {
        // A `Nack` is the EXPECTED outcome of a collision, not a failure, so the
        // budget here is generous — phase 1 is about distinctness, not convergence.
        db.transaction_coordinated(256, |tx| {
            tx.create_ticket(Ticket { id: Uuid::nil(), seq: 0, title: format!("row-{i}") })
        })
        .expect("coordinated commit");
    }
}

/// Phase 2 bulk writer: run the counter far ahead of the lagging writer.
fn bulk(dir: PathBuf) {
    let db = connect(&dir);
    for i in 0..BULK {
        db.transaction_coordinated(256, |tx| {
            tx.create_ticket(Ticket { id: Uuid::nil(), seq: 0, title: format!("bulk-{i}") })
        })
        .expect("bulk commit");
    }
    std::fs::write(dir.join("bulk_done"), b"1").unwrap();
}

/// Phase 2 lagging writer: connect FIRST (so the counter is seeded at 0), wait for
/// the bulk writer to run far ahead, then commit one row on a tight budget.
fn lag(dir: PathBuf) {
    let db = connect(&dir);
    std::fs::write(dir.join("lag_ready"), b"1").unwrap();

    let done = dir.join("bulk_done");
    let mut waited = 0;
    while !done.exists() && waited < 600 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    if !done.exists() {
        eprintln!("bulk writer never finished");
        std::process::exit(1);
    }

    // THE phase-2 assertion. Our counter is ~60 behind; the first attempt collides
    // with a committed value. Passing within LAG_RETRIES requires jumping past the
    // conflicting value, not incrementing toward it.
    db.transaction_coordinated(LAG_RETRIES, |tx| {
        tx.create_ticket(Ticket { id: Uuid::nil(), seq: 0, title: "lagger".to_string() })
    })
    .expect("a Nacked writer must fast-forward past the winner, not walk to it");
}

fn spawn_coordinator(forgedb: &Path, dir: &Path) -> std::process::Child {
    std::fs::create_dir_all(dir).unwrap();
    let coord = std::process::Command::new(forgedb)
        .arg("coordinate")
        .arg(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn forgedb coordinate");

    let socket = dir.join("_coord.sock");
    let mut waited = 0;
    while !socket.exists() && waited < 200 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    if !socket.exists() {
        eprintln!("coordinator never created {}", socket.display());
        std::process::exit(1);
    }
    coord
}

fn run(me: &Path, dir: &Path, forgedb: &Path, role: &str) -> std::process::Child {
    std::process::Command::new(me)
        .arg(dir)
        .arg(forgedb)
        .arg(role)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn role")
}

fn wait_ok(child: std::process::Child, role: &str) -> bool {
    let out = child.wait_with_output().expect("role exit");
    if !out.status.success() {
        eprintln!(
            "{role} failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return false;
    }
    true
}

/// Read every committed `seq` straight off disk, standalone. Stronger than
/// collecting values the writers print: it proves what actually landed.
fn committed_seqs(dir: &Path) -> Vec<u64> {
    let db = Database::open_at(dir.to_path_buf());
    db.ticket.all().iter().map(|t| t.seq).collect()
}

fn parent(root: PathBuf, forgedb: PathBuf) {
    let me = std::env::current_exe().unwrap();
    let mut bad = 0;

    // ---- Phase 1: concurrent allocation must never duplicate -----------------
    let p1 = root.join("p1");
    let mut coord = spawn_coordinator(&forgedb, &p1);
    let writers: Vec<_> = (0..2).map(|_| run(&me, &p1, &forgedb, "writer")).collect();
    let mut ok = true;
    for w in writers {
        ok &= wait_ok(w, "writer");
    }
    let _ = coord.kill();
    let _ = coord.wait();
    if !ok {
        std::process::exit(1);
    }

    let seqs = committed_seqs(&p1);
    let issued = seqs.len();
    let mut sorted = seqs.clone();
    sorted.sort();
    sorted.dedup();

    let expected = (2 * PER_WRITER) as usize;
    if issued != expected {
        eprintln!("FAIL every create committed: got {issued}, want {expected}");
        bad += 1;
    }
    // THE phase-1 assertion: without the b"s" claim key both processes commit the
    // same number and the coordinator never sees the collision.
    if sorted.len() != issued {
        eprintln!(
            "FAIL every bare-auto allocation is distinct ACROSS PROCESSES: {} unique of {issued}",
            sorted.len()
        );
        bad += 1;
    }
    if sorted.iter().any(|&n| n == 0) {
        eprintln!("FAIL no row kept the allocate sentinel");
        bad += 1;
    }

    // ---- Phase 2: a Nacked writer converges in ~1 retry, not ~BULK -----------
    let p2 = root.join("p2");
    let mut coord = spawn_coordinator(&forgedb, &p2);

    // The lagging writer must connect BEFORE any row exists, so its counter starts
    // at 0 and the bulk writer's commits are invisible to it.
    let lagger = run(&me, &p2, &forgedb, "lag");
    let ready = p2.join("lag_ready");
    let mut waited = 0;
    while !ready.exists() && waited < 600 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    if !ready.exists() {
        let _ = coord.kill();
        eprintln!("FAIL lagging writer never connected");
        std::process::exit(1);
    }

    let bulker = run(&me, &p2, &forgedb, "bulk");
    let bulk_ok = wait_ok(bulker, "bulk");
    let lag_ok = wait_ok(lagger, "lag");
    let _ = coord.kill();
    let _ = coord.wait();

    if !bulk_ok {
        bad += 1;
    }
    if !lag_ok {
        eprintln!(
            "FAIL a Nacked writer fast-forwards: it had {LAG_RETRIES} retries to close a \
             {BULK}-value gap, which only works by jumping past the conflicting value"
        );
        bad += 1;
    }

    let p2_seqs = committed_seqs(&p2);
    let mut p2_sorted = p2_seqs.clone();
    p2_sorted.sort();
    p2_sorted.dedup();
    if p2_sorted.len() != p2_seqs.len() {
        eprintln!("FAIL phase 2 committed a duplicate seq");
        bad += 1;
    }

    if bad > 0 {
        std::process::exit(1);
    }
    println!(
        "phase 1: {issued} distinct across 2 processes (max {}); \
         phase 2: lagger closed a {BULK}-value gap within {LAG_RETRIES} retries",
        sorted.last().unwrap()
    );
}
"##;
