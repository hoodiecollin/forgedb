//! Multi-process integer auto-increment under the Tier-3 coordinator (#187/#84).
//!
//! # Why this test exists at all
//!
//! It is the only thing that demonstrates **decision 6** of RFC #187 — the rule
//! that an integer auto must be the identity or carry `&unique`.
//!
//! The counter is per-process. Two coordinated writers open the same data dir
//! lock-free and each derive their own, so they *can* allocate the same number.
//! Nothing prevents that, and the design deliberately does not try to. What makes
//! it safe is that the collision is **detected**: the opaque write-set carries an
//! id key for an identity and a unique-claim key for `&unique`, so the coordinator
//! equality-compares, `Nack`s the loser, and the retry re-refreshes past the
//! winner's value and allocates again.
//!
//! An index (`^`) claims nothing in the write-set. That is the whole reason `^`
//! alone is refused by validation, and the reason this file's assertion is
//! "every value distinct across processes" rather than "it compiles".
//!
//! # Shape
//!
//! One driver binary plays three roles, selected by `argv[3]`:
//!
//! - no role  → **parent**: spawns `forgedb coordinate`, re-execs itself twice as
//!   writers, collects their ids, asserts global distinctness.
//! - `writer` → opens via `Database::connect` and commits N rows through
//!   `transaction_coordinated`, printing each id.
//!
//! Re-execing the same binary is what makes this a genuine multi-*process* proof;
//! two threads would exercise Tier 2, which `auto_increment_test` already covers.
//!
//! Compiles a generated crate and spawns processes, so it is `#[ignore]`d out of
//! the fast default suite:
//!
//! ```bash
//! make auto-increment-test   # runs this and the single-process probe
//! ```

#![cfg(unix)]

mod common;

const SCHEMA: &str = r#"
Ticket {
  id: +u64
  title: string
}
"#;

#[test]
#[ignore = "compiles a generated crate and spawns processes; run with --ignored"]
fn coordinated_writers_never_duplicate_an_allocation() {
    let (out, proj) = common::generate_compile_run("coordseq", SCHEMA, DRIVER);
    common::assert_driver_ok(&out, &proj, "coordinated writers duplicated an allocation");
}

const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use std::io::BufRead;
use std::path::PathBuf;

const PER_WRITER: u64 = 40;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("data dir"));
    let forgedb = PathBuf::from(std::env::args().nth(2).expect("forgedb binary"));
    let role = std::env::args().nth(3);

    match role.as_deref() {
        Some("writer") => writer(dir),
        _ => parent(dir, forgedb),
    }
}

/// Commit `PER_WRITER` rows through the coordinator, printing each allocated id on
/// its own line for the parent to collect.
fn writer(dir: PathBuf) {
    let socket = dir.join("_coord.sock");
    let db = Database::connect(dir, socket).expect("connect to coordinator");
    for i in 0..PER_WRITER {
        // Generous retries: a `Nack` is the EXPECTED outcome of a collision here,
        // not a failure. Exhausting them would mean the retry never converges.
        let id = db
            .transaction_coordinated(256, |tx| {
                tx.create_ticket(Ticket { id: 0, title: format!("row-{i}") })
            })
            .expect("coordinated commit");
        println!("{id}");
    }
}

fn parent(dir: PathBuf, forgedb: PathBuf) {
    std::fs::create_dir_all(&dir).unwrap();

    let mut coord = std::process::Command::new(&forgedb)
        .arg("coordinate")
        .arg(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn forgedb coordinate");

    // Wait for the socket rather than sleeping a guessed interval.
    let socket = dir.join("_coord.sock");
    let mut waited = 0;
    while !socket.exists() && waited < 200 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 1;
    }
    if !socket.exists() {
        let _ = coord.kill();
        eprintln!("coordinator never created {}", socket.display());
        std::process::exit(1);
    }

    let me = std::env::current_exe().unwrap();
    let writers: Vec<_> = (0..2)
        .map(|_| {
            std::process::Command::new(&me)
                .arg(&dir)
                .arg(&forgedb)
                .arg("writer")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("spawn writer")
        })
        .collect();

    let mut ids: Vec<u64> = Vec::new();
    let mut failed = false;
    for w in writers {
        let out = w.wait_with_output().expect("writer exit");
        if !out.status.success() {
            eprintln!(
                "writer failed:\n{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            failed = true;
        }
        for line in out.stdout.lines() {
            if let Ok(n) = line.unwrap().trim().parse::<u64>() {
                ids.push(n);
            }
        }
    }

    let _ = coord.kill();
    let _ = coord.wait();

    if failed {
        std::process::exit(1);
    }

    let issued = ids.len();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();

    let expected = (2 * PER_WRITER) as usize;
    let mut bad = 0;

    if issued != expected {
        eprintln!("FAIL every coordinated create committed: got {issued}, want {expected}");
        bad += 1;
    }
    // THE assertion. A duplicate here means two processes both committed the same
    // number and the coordinator did not see the collision — which is exactly what
    // decision 6's identity-or-`&unique` rule exists to make impossible.
    if sorted.len() != issued {
        eprintln!(
            "FAIL every allocation is distinct ACROSS PROCESSES: {} unique of {issued}",
            sorted.len()
        );
        bad += 1;
    }
    // Gaps are allowed and expected (a `Nack`ed attempt burns its number), so the
    // set is deliberately NOT asserted to be contiguous — only distinct.
    if sorted.iter().any(|&n| n == 0) {
        eprintln!("FAIL no row was given the allocate sentinel");
        bad += 1;
    }

    if bad > 0 {
        std::process::exit(1);
    }
    println!("{issued} coordinated allocations, all distinct (max {})", sorted.last().unwrap());
}
"##;
