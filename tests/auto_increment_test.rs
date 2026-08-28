mod common;

const SCHEMA: &str = r#"
Ticket {
  id: +u64
  title: string
}

Invoice {
  id: +uuid
  number: &+u64
  total: f64
}

Small {
  id: +u32
  name: string
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make auto-increment-test`)"]
fn integer_autos_allocate_monotonically_and_survive_restart() {
    let (out, proj) = common::generate_compile_run("seqdriver", SCHEMA, DRIVER);
    common::assert_driver_ok(&out, &proj, "driver reported an auto-increment defect");
}

const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use forgedb_types::Uuid;
use std::sync::Arc;

static mut FAILURES: u32 = 0;

fn check(what: &str, ok: bool, detail: String) {
    if ok {
        println!("  ok   {what}");
    } else {
        println!("  FAIL {what}\n       {detail}");
        unsafe { FAILURES += 1 }
    }
}

fn eq<T: std::fmt::Debug + PartialEq>(what: &str, got: T, want: T) {
    let ok = got == want;
    check(what, ok, format!("got {got:?}, want {want:?}"));
}

fn ticket(title: &str) -> Ticket {
    Ticket { id: 0, title: title.to_string() }
}

fn manifest_floor(root: &std::path::Path) -> u64 {
    let bytes = match std::fs::read(root.join("ticket/manifest.json")) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    let m: serde_json::Value = serde_json::from_slice(&bytes).expect("parse manifest");
    m["auto_sequences"]["id"].as_u64().unwrap_or(0)
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir"));

    {
        let mut db = Database::open_at(dir.join("basic"));
        let a = db.create_ticket(ticket("a")).unwrap();
        let b = db.create_ticket(ticket("b")).unwrap();
        let c = db.create_ticket(ticket("c")).unwrap();
        eq("first allocation is 1, not 0", a, 1u64);
        eq("allocation is monotonic", (b, c), (2u64, 3u64));
        check("no row was given the sentinel", a != 0 && b != 0 && c != 0, format!("{a} {b} {c}"));
    }

    {
        let root = dir.join("reopen");
        {
            let mut db = Database::open_at(root.clone());
            for i in 0..5 { db.create_ticket(ticket(&format!("t{i}"))).unwrap(); }
            db.commit().unwrap();
        } // dropped — the in-memory counter is gone
        let mut db = Database::open_at(root);
        eq("reopen resumes past the highest existing value", db.create_ticket(ticket("next")).unwrap(), 6u64);
    }

    {
        let root = dir.join("delete");
        {
            let mut db = Database::open_at(root.clone());
            for i in 0..3 { db.create_ticket(ticket(&format!("t{i}"))).unwrap(); }
            db.delete_ticket(3).unwrap();
            db.commit().unwrap();
        }
        let mut db = Database::open_at(root);
        eq("a deleted row's number is not recycled after restart", db.create_ticket(ticket("next")).unwrap(), 4u64);
    }

    {
        let root = dir.join("compact");
        let highest;
        {
            let mut db = Database::open_at(root.clone());
            for i in 0..10 { db.create_ticket(ticket(&format!("t{i}"))).unwrap(); }
            for id in 2..=10 { db.delete_ticket(id).unwrap(); }
            db.compact();
            highest = 10u64;
            eq("compaction does not regress the live counter", db.create_ticket(ticket("post")).unwrap(), highest + 1);
            db.commit().unwrap();
        }

        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("ticket/manifest.json")).expect("read manifest"),
        )
        .expect("parse manifest");
        let persisted = manifest["auto_sequences"]["id"].as_u64().unwrap_or(0);
        check(
            "the floor covers the values compaction destroyed",
            persisted >= highest,
            format!("manifest holds {persisted}, but ids up to {highest} were reclaimed"),
        );

        let mut db = Database::open_at(root);
        eq("compaction + restart does not re-issue", db.create_ticket(ticket("after")).unwrap(), highest + 2);
    }

    #[cfg(unix)]
    {
        let root = dir.join("prefloor");
        {
            let mut db = Database::open_at(root.clone());
            for i in 0..5 { db.create_ticket(ticket(&format!("seed{i}"))).unwrap(); }
            db.commit().unwrap();
        }

        let mut db = Database::open_at(root.clone());
        eq("reopen persists the scanned maximum as the floor", manifest_floor(&root), 5u64);
        let mut highest = 0u64;
        let mut doomed = Vec::new();
        for i in 0..10 {
            let id = db.create_ticket(ticket(&format!("doomed{i}"))).unwrap();
            highest = highest.max(id);
            doomed.push(id);
        }
        for id in &doomed { db.delete_ticket(*id).unwrap(); }
        db.commit().unwrap();

        let manifest = root.join("ticket/manifest.json");
        std::fs::remove_file(&manifest).unwrap();
        std::fs::create_dir(&manifest).unwrap();

        db.compact();
        db.commit().unwrap();
        drop(db);

        let mut db = Database::open_at(root);
        eq(
            "compaction that cannot persist the floor first re-issues nothing",
            db.create_ticket(ticket("after")).unwrap(),
            highest + 1,
        );
    }

    {
        let mut db = Database::open_at(dir.join("explicit"));
        db.create_ticket(ticket("a")).unwrap();
        let seeded = db.create_ticket(Ticket { id: 500, title: "seeded".into() }).unwrap();
        eq("an explicit value is honoured verbatim", seeded, 500u64);
        eq("and the counter jumps past it", db.create_ticket(ticket("next")).unwrap(), 501u64);
    }

    {
        let mut db = Database::open_at(dir.join("rollback"));
        let first = db.create_ticket(ticket("a")).unwrap();
        let _ = db.transaction(|tx| {
            tx.create_ticket(ticket("doomed"))?;
            Err::<(), _>(TxError::Conflict)
        });
        eq("a rolled-back allocation is burned, not reused", db.create_ticket(ticket("b")).unwrap(), first + 2);
    }

    {
        let root = dir.join("nonidentity");
        let mk = |n: u64| Invoice { id: Uuid::nil(), number: n, total: 1.0 };
        {
            let mut db = Database::open_at(root.clone());
            db.create_invoice(mk(0)).unwrap();
            db.create_invoice(mk(0)).unwrap();
            db.commit().unwrap();
        }
        let mut db = Database::open_at(root);
        db.create_invoice(mk(0)).unwrap();
        let all = db.invoice.all();
        let mut nums: Vec<u64> = all.iter().map(|i| i.number).collect();
        nums.sort();
        eq("a non-identity auto allocates and survives reopen", nums, vec![1u64, 2, 3]);
    }

    {
        const THREADS: u64 = 8;
        const EACH: u64 = 25;
        let shared = Arc::new(Database::open_at(dir.join("tier2")).shared());
        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let db = Arc::clone(&shared);
                std::thread::spawn(move || {
                    let mut mine = Vec::new();
                    for i in 0..EACH {
                        let id = db
                            .transaction_concurrent(64, |tx| tx.create_ticket(ticket(&format!("t{t}-{i}"))))
                            .unwrap();
                        mine.push(id);
                    }
                    mine
                })
            })
            .collect();
        let mut all: Vec<u64> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        let issued = all.len();
        all.sort();
        all.dedup();
        eq("every concurrent allocation is distinct", all.len(), issued);
        eq("and every create succeeded", issued, (THREADS * EACH) as usize);
    }

    {
        let mut db = Database::open_at(dir.join("overflow"));
        db.create_small(Small { id: u32::MAX, name: "last".into() }).unwrap();
        let before = db.small.all().len();
        let err = db.create_small(Small { id: 0, name: "overflow".into() });
        check(
            "an exhausted u32 sequence is an error, not a wrap",
            matches!(err, Err(ValidationError::SequenceExhausted { .. })),
            format!("{err:?}"),
        );
        eq("and no row was written", db.small.all().len(), before);
    }

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("\n{failures} auto-increment check(s) failed");
        std::process::exit(1);
    }
    println!("\nall auto-increment checks passed");
}
"##;
