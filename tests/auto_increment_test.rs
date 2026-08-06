//! Integer auto-increment behaviour (#187), proved by **running** generated code.
//!
//! # Why this test exists in this form
//!
//! The counter is not a string in the emitted source — it is a value handed out
//! over time, across a compaction, across a reopen, and across threads. Every
//! property #187 promises is a property of what the generated code *does*:
//!
//! - **Monotonic and unique**, never contiguous. A rolled-back transaction burns
//!   its number, deliberately.
//! - **Restart-safe.** The counter is in-memory, seeded by the reopen scan and
//!   floored by a high-water mark in `Manifest.auto_sequences`.
//! - **Reuse-free across compaction** — the one case a pure rescan cannot survive,
//!   because compaction physically drops the rows the rescan derives the max from.
//!
//! A codegen snapshot compares emitted *strings* and can say none of that. The
//! companion guards in `crates/codegen/tests/codegen_snapshots.rs` pin that the
//! machinery is emitted at all; this file pins that it is correct.
//!
//! # The scenario that motivates the persistence
//!
//! `compact → drop → reopen → create` (`compaction_does_not_re_issue` below) is the
//! whole reason a number is written to disk. It also fails **silently** in the most
//! plausible wrong implementation: `compact()` does `*self = Self::new_at_no_rehydrate(..)`,
//! which zeroes the counter *and* rewrites the manifest from inside the constructor.
//! Miss either the max-merge or the save/reinstall and this reads as working until
//! the second run after a compaction — so the assertion is on the **value**, never
//! on "no error".
//!
//! It compiles a generated crate, so it is `#[ignore]`d out of the fast hermetic
//! default suite. Run it explicitly:
//!
//! ```bash
//! make auto-increment-test   # or:
//! cargo test --test auto_increment_test -- --ignored --nocapture
//! ```

mod common;

/// `Ticket` is the plain integer identity. `Invoice` carries a *non-identity*
/// `&+u64` — the shape the conflict-visible rule (#187 decision 6) forces `&` onto,
/// and the one whose counter must be seeded by a column the ungated reopen scan
/// does not otherwise decode. `Small` is `+u32`, for the overflow guard.
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

/// The persisted `Ticket.id` allocation floor, read straight off disk.
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

    // ---- 1. Allocation is monotonic from 1, and `0` means "allocate" ---------
    {
        let mut db = Database::open_at(dir.join("basic"));
        let a = db.create_ticket(ticket("a")).unwrap();
        let b = db.create_ticket(ticket("b")).unwrap();
        let c = db.create_ticket(ticket("c")).unwrap();
        eq("first allocation is 1, not 0", a, 1u64);
        eq("allocation is monotonic", (b, c), (2u64, 3u64));
        // `0` is the sentinel, so it can never be a value the store hands back.
        check("no row was given the sentinel", a != 0 && b != 0 && c != 0, format!("{a} {b} {c}"));
    }

    // ---- 2. Reopen does not re-issue ----------------------------------------
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

    // ---- 3. Deleting the top row does not free its number -------------------
    // The reopen id-scan is deliberately UNGATED by tombstones, so a deleted row
    // still bounds the counter. Without that, deleting the newest row and
    // restarting would re-issue its id to a different row — visible in the
    // replication log, in backups, and in any URL holding it.
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

    // ---- 4. compact → drop → reopen → create does not re-issue --------------
    // THE scenario the persisted high-water mark exists for. Compaction physically
    // drops dead rows, so the reopen rescan yields a LOWER max than was ever
    // issued; only the manifest floor prevents a second issuance.
    {
        let root = dir.join("compact");
        let highest;
        {
            let mut db = Database::open_at(root.clone());
            for i in 0..10 { db.create_ticket(ticket(&format!("t{i}"))).unwrap(); }
            // Delete all but the first, so compaction reclaims 9 rows and the
            // surviving max is 1 — far below the 10 actually handed out.
            for id in 2..=10 { db.delete_ticket(id).unwrap(); }
            db.compact();
            highest = 10u64;
            // Still correct in THIS process (the live counter never regressed).
            eq("compaction does not regress the live counter", db.create_ticket(ticket("post")).unwrap(), highest + 1);
            db.commit().unwrap();
        }

        // Assert the DURABLE contract directly, not only its observable effect.
        //
        // The floor's job is NOT to record every value ever issued — that would
        // mean rewriting the manifest on every allocation, the per-allocation
        // fsync the design exists to avoid. Live rows are covered by the reopen
        // scan for free. What the scan *cannot* recover is a value issued to a row
        // compaction physically destroyed, and that is exactly what the floor has
        // to cover.
        //
        // Here ids 2..=10 were deleted and reclaimed, so nothing on disk mentions
        // them; only the persisted number stands between a rescan and re-issuing
        // `10`. The reopen contract is `max(persisted, scanned)`, so this and the
        // value checks above are two halves of one claim.
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

        // Read BEFORE any reopen, deliberately. `new_at` writes the manifest too,
        // from a counter the reopen scan just seeded — so checking after a reopen
        // would measure that write instead, and would pass even if `compact()`
        // itself persisted nothing. The moment the floor has to be right is the
        // moment compaction ends, because a crash there is the case it exists for.

        // ...and it is still correct after a restart, which is the half that fails
        // silently when the counter is not carried across `compact()`'s reset.
        let mut db = Database::open_at(root);
        eq("compaction + restart does not re-issue", db.create_ticket(ticket("after")).unwrap(), highest + 2);
    }

    // ---- 4b. Compaction refuses to run if the floor cannot be persisted -----
    // Scenario 4 proves the floor is right once `compact()` has *returned*, which
    // a re-persist placed AFTER the destructive rewrite satisfies just as well.
    // This one pins the ordering, and it is the assertion scenario 4 cannot make.
    //
    // The floor reaches disk at open and at compaction, and nowhere between — so
    // between them it holds the counter as of process *open*. Every value allocated
    // since exists only in memory and in the rows themselves, and
    // `compact_model_keeping` is about to delete those rows. Persist only after
    // that rewrite and a crash in the window leaves a reopen scanning a LOWER
    // maximum and re-issuing the difference: exactly what the floor exists to
    // prevent, in the one case a rescan cannot recover from.
    //
    // Making that window observable needs the manifest write to fail while the
    // column rewrite still succeeds, so the two orderings diverge in the final
    // state. Replacing `manifest.json` with a *directory* does it precisely:
    // `save_to`'s temp-file rename cannot land on a directory, and the compactor
    // never touches the manifest at all (it rewrites `fixed/`, `variable/` and
    // `tombstones.bin`). So:
    //
    //   floor first → the write fails → compaction ABORTS → rows survive → the
    //     ungated reopen scan still sees 15 → the next id is 16.
    //   rewrite first → rows are destroyed → the re-persist then fails with
    //     nothing left to fall back on → the reopen scan sees 5 → 6..=15 are
    //     handed out a second time.
    //
    // Deliberately two processes: a single fresh one opens at floor 0, where the
    // gap is easy to misread as "nothing written yet." Opening onto existing rows
    // starts the floor at 5 while 15 have been issued, which is unambiguous.
    #[cfg(unix)]
    {
        let root = dir.join("prefloor");
        {
            let mut db = Database::open_at(root.clone());
            for i in 0..5 { db.create_ticket(ticket(&format!("seed{i}"))).unwrap(); }
            db.commit().unwrap();
        }

        let mut db = Database::open_at(root.clone());
        // Opening is what stamps the floor: the scan seeds the counter and the
        // constructor persists it. Nothing between now and compaction writes it
        // again, which is precisely why the window below exists.
        eq("reopen persists the scanned maximum as the floor", manifest_floor(&root), 5u64);
        let mut highest = 0u64;
        let mut doomed = Vec::new();
        for i in 0..10 {
            let id = db.create_ticket(ticket(&format!("doomed{i}"))).unwrap();
            highest = highest.max(id);
            doomed.push(id);
        }
        // Delete every row allocated this run so compaction reclaims all of them:
        // afterwards nothing on disk would mention 6..=15.
        for id in &doomed { db.delete_ticket(*id).unwrap(); }
        db.commit().unwrap();

        // Jam the manifest write, and leave it jammed across the reopen — the
        // point is that there is no persisted floor to rescue a lost scan.
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

    // ---- 5. An explicitly supplied value advances the counter ---------------
    // Required regardless of compaction: it is what stops a restored backup or an
    // imported dataset from colliding with live rows immediately.
    {
        let mut db = Database::open_at(dir.join("explicit"));
        db.create_ticket(ticket("a")).unwrap();
        let seeded = db.create_ticket(Ticket { id: 500, title: "seeded".into() }).unwrap();
        eq("an explicit value is honoured verbatim", seeded, 500u64);
        eq("and the counter jumps past it", db.create_ticket(ticket("next")).unwrap(), 501u64);
    }

    // ---- 6. A rolled-back transaction burns its number ----------------------
    // Pinned as INTENDED, not tolerated. Rewinding is unsafe under Tier 2 (the
    // prepare closure runs with no lock, so you cannot know you were the last
    // taker), and gaps are the same contract Postgres/MySQL offer.
    {
        let mut db = Database::open_at(dir.join("rollback"));
        let first = db.create_ticket(ticket("a")).unwrap();
        let _ = db.transaction(|tx| {
            tx.create_ticket(ticket("doomed"))?;
            Err::<(), _>(TxError::Conflict)
        });
        eq("a rolled-back allocation is burned, not reused", db.create_ticket(ticket("b")).unwrap(), first + 2);
    }

    // ---- 7. A non-identity `&+u64` allocates too ----------------------------
    // Its counter cannot be seeded by the ungated id-scan (which decodes only the
    // identity column), so this is the case that needs its own column read at
    // reopen — the Gate 2 correction to "folding the max in is free".
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

    // ---- 8. Tier 2: concurrent prepares never duplicate ---------------------
    // `transaction_concurrent` runs its prepare closure with NO write lock, so this
    // is the path where two threads can interleave inside allocation.
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

    // ---- 9. `+u32` refuses to wrap -----------------------------------------
    // Wrapping would re-issue 0 — which is also the allocate sentinel — and then
    // collide with every id already handed out.
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
