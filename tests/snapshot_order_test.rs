//! **#457** — a snapshot read (`?as_of=` / `all_at`) must return rows in the same
//! order every process, and it must be the order the live list returns.
//!
//! `all_at` gathered its rows by iterating `id_versions`, a `std::collections::
//! HashMap` whose `RandomState` seed is drawn per process. So the row order of a
//! point-in-time read was a different random permutation in every process, while
//! the live list — which sorts its row indices — was stable. Paging a snapshot
//! across a restart or a second replica therefore skipped some rows and repeated
//! others, with every page individually well-formed and `total` correct.
//!
//! Two properties, and **neither alone is the guard**:
//!
//! 1. **The order is exactly ascending physical row order** — asserted against a
//!    hand-computed literal, not against another generated helper. `all_at` and
//!    `export_live_indices` now both sort, so comparing them would go green if a
//!    change removed both sorts together. The literal below cannot move with the
//!    code.
//! 2. **Every process agrees** — the compiled driver is run repeatedly as separate
//!    processes over one data dir. Within a single process the buggy order was
//!    *stable*, so a one-shot assertion could only ever catch it by luck of the
//!    seed; re-running is what exercises the actual failure mechanism. The first
//!    process seeds and the rest reopen, so each gets a fresh seed over identical
//!    bytes.
//!
//! The fixture is **churned on purpose**. Two rows are updated (superseding-version
//! append moves their live row to the tail) and one is deleted. That makes ascending
//! physical row order differ from insertion order, so the literal discriminates
//! between "sorted by row" and "happened to come back in the order I inserted".
//! On a churn-free table the two coincide and the test would prove nothing — the
//! driver asserts the churn took effect rather than assuming it.
//!
//! It compiles and RUNS a generated crate, so it is `#[ignore]`d out of the fast
//! hermetic default suite:
//!
//! ```bash
//! cargo test --test snapshot_order_test -- --ignored --nocapture
//! ```

mod common;

use std::process::Command;

const SCHEMA: &str = r#"
Ledger {
  id: +uuid
  seq: i64
  note: string
  @projection(brief: seq)
}
"#;

/// How many separate processes read the same seeded directory.
///
/// The buggy gather is stable *within* a process, so this count is the whole
/// experiment. Six independent seeds miss a 13-row misordering with probability
/// far below any flake threshold; one process would have caught it only when the
/// seed happened to permute.
const RUNS: usize = 6;

const DRIVER: &str = r##"mod database;
use database::*;
use forgedb_types::Uuid;

/// The seq values `all_at` must return, in order, after the churn below.
///
/// Hand-computed from physical row placement, NOT read back from another generated
/// helper — see the module docs. Inserts take rows 0..13 in seq order. Updating
/// seq 3 and seq 9 appends their new versions at rows 14 and 15, so both move to
/// the **tail**. Deleting seq 6 appends a tombstone at row 16, so it is absent.
/// Ascending row order is therefore:
///
///   rows 0 1 3 4 6 7 9 10 11 12 13 14 15
///   seq  1 2 4 5 7 8 10 11 12 13 14  3  9
///
/// Two things this literal pins that a length or a set could not: the updated rows
/// are LAST rather than in seq position, and the deleted one is gone rather than
/// resurrected at its old row.
const EXPECTED: &[i64] = &[1, 2, 4, 5, 7, 8, 10, 11, 12, 13, 14, 3, 9];

static mut FAILURES: u32 = 0;

fn check(label: &str, cond: bool, detail: String) {
    if cond {
        println!("ok    {label}");
    } else {
        eprintln!("FAIL  {label}: {detail}");
        unsafe { FAILURES += 1 };
    }
}

/// Insert 14 rows, update two of them, delete one. Only the first process does
/// this; the rest reopen the bytes it left.
fn seed(db: &mut Database) {
    let mut ids = Vec::new();
    for seq in 1..=14i64 {
        let id = db
            .create_ledger(Ledger { id: Uuid::nil(), seq, note: format!("n{seq}") })
            .expect("insert");
        ids.push((seq, id));
    }
    for seq in [3i64, 9] {
        let (_, id) = ids.iter().find(|(s, _)| *s == seq).copied().expect("id");
        let updated = db
            .update_ledger(id, Ledger { id, seq, note: format!("n{seq}-v2") })
            .expect("update");
        assert!(updated, "update of seq {seq} found no row");
    }
    let (_, gone) = ids.iter().find(|(s, _)| *s == 6).copied().expect("id");
    assert!(db.delete_ledger(gone).expect("delete"), "delete of seq 6 found no row");
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir"));
    let mut db = Database::open_at(dir);

    if db.ledger.row_count() == 0 {
        seed(&mut db);
        println!("seeded");
    }

    let snap = db.ledger.snapshot();
    let got: Vec<i64> = db.ledger.all_at(&snap).into_iter().map(|r| r.seq).collect();

    check(
        "all_at returns ascending physical row order",
        got == EXPECTED,
        format!("want {EXPECTED:?}\n           got  {got:?}"),
    );

    // Anti-vacuity, and it has to be a RUNTIME property. Comparing EXPECTED to a
    // written-out insertion order would compare two constants and could never
    // fail whatever the code did. What actually makes this fixture discriminating
    // is that superseding-version append happened: 14 inserts + 2 updates + 1
    // delete = 17 physical rows behind 13 live ones. If append ever became
    // in-place mutation, physical order would collapse back to insertion order,
    // the literal would stop telling "sorted by row" from "returned as inserted",
    // and this is the only assertion that would notice.
    let physical = db.ledger.row_count();
    check(
        "the fixture is churned: more physical rows than live ones",
        physical == 17 && got.len() == 13,
        format!("want 17 physical / 13 live; got {physical} physical / {} live", got.len()),
    );

    // The #113 projection resolves its snapshot rows through a SECOND gather
    // (`__proj_live_rows_at`) that had the identical defect. Same order or the
    // two views of one snapshot disagree.
    let proj: Vec<i64> = db.ledger.all_brief_at(&snap).into_iter().map(|r| r.seq).collect();
    check(
        "the projection's snapshot scan agrees with all_at",
        proj == got,
        format!("all_at {got:?}\n           brief  {proj:?}"),
    );

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} snapshot-order failure(s)");
        std::process::exit(1);
    }
    println!("snapshot order ok: {got:?}");
}
"##;

#[test]
#[ignore = "compiles a generated crate; run with --ignored"]
fn a_snapshot_read_returns_the_same_row_order_in_every_process() {
    let (out, proj) = common::generate_compile_run("snapshot-order", SCHEMA, DRIVER);
    let first = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "seeding run failed:\n{first}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        first.contains("seeded"),
        "the first run must be the one that seeds — otherwise every later run \
         reads an empty table and the order assertion is vacuous:\n{first}"
    );

    // The seeding run is one process, i.e. one `RandomState` seed. Re-run the
    // SAME binary over the SAME bytes: each is a fresh seed, and the buggy gather
    // permuted per process while staying stable within one.
    let bin = proj.join("target/debug/snapshot-order");
    let data = proj.join("data");
    let mut orders = std::collections::BTreeSet::new();
    orders.insert(order_line(&first));

    for run in 2..=RUNS {
        let out = Command::new(&bin).arg(&data).output().expect("re-run driver");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "run {run} of {RUNS} failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !stdout.contains("seeded"),
            "run {run} re-seeded — it should have reopened the first run's rows"
        );
        orders.insert(order_line(&stdout));
    }

    // Every run asserted the literal itself, so this can only fail if a run
    // reported an order the literal check somehow passed. Kept because the count
    // is the property the issue is about, and it names the disagreement.
    assert_eq!(
        orders.len(),
        1,
        "{RUNS} processes reported {} distinct row orders:\n{orders:#?}",
        orders.len()
    );

    let _ = std::fs::remove_dir_all(&proj);
}

/// The driver's final line, which carries the order it observed.
fn order_line(stdout: &str) -> String {
    stdout
        .lines()
        .find(|l| l.starts_with("snapshot order ok:"))
        .unwrap_or_else(|| panic!("driver printed no order line:\n{stdout}"))
        .to_string()
}
