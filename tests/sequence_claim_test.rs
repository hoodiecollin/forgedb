#![cfg(unix)]

mod common;

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

fn writer(dir: PathBuf) {
    let db = connect(&dir);
    for i in 0..PER_WRITER {
        db.transaction_coordinated(256, |tx| {
            tx.create_ticket(Ticket { id: Uuid::nil(), seq: 0, title: format!("row-{i}") })
        })
        .expect("coordinated commit");
    }
}

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

fn committed_seqs(dir: &Path) -> Vec<u64> {
    let db = Database::open_at(dir.to_path_buf());
    db.ticket.all().iter().map(|t| t.seq).collect()
}

fn parent(root: PathBuf, forgedb: PathBuf) {
    let me = std::env::current_exe().unwrap();
    let mut bad = 0;

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

    let p2 = root.join("p2");
    let mut coord = spawn_coordinator(&forgedb, &p2);

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
