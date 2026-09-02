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

const RUNS: usize = 6;

const DRIVER: &str = r##"mod database;
use database::*;
use forgedb_types::Uuid;

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

    let physical = db.ledger.row_count();
    check(
        "the fixture is churned: more physical rows than live ones",
        physical == 17 && got.len() == 13,
        format!("want 17 physical / 13 live; got {physical} physical / {} live", got.len()),
    );

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

    assert_eq!(
        orders.len(),
        1,
        "{RUNS} processes reported {} distinct row orders:\n{orders:#?}",
        orders.len()
    );

    let _ = std::fs::remove_dir_all(&proj);
}

fn order_line(stdout: &str) -> String {
    stdout
        .lines()
        .find(|l| l.starts_with("snapshot order ok:"))
        .unwrap_or_else(|| panic!("driver printed no order line:\n{stdout}"))
        .to_string()
}
