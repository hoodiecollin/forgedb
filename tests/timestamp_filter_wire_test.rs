//! **The REST half of the #389 quantum contract, through a booted router.**
//!
//! A `timestamp` field declares a quantum and the write path floors a written value to
//! it. Both read paths have to floor identically or a value that was accepted on write
//! matches nothing on read — silently, as an empty page rather than an error.
//!
//! # Why this is not covered by the codegen guards
//!
//! `crates/codegen/tests/codegen_snapshots.rs` asserts that a `floor_to_micros` is
//! emitted on each path. That is a claim about *text*. The REST list path derives the
//! same parameter **twice**, in two generators, and the two have to agree with each
//! other and with the write gate:
//!
//! 1. `__rows_by_<field>` — the index pushdown, from `RustGenerator::index_value_expr`,
//!    which resolves candidate ROWS (#160 C).
//! 2. `<model>_scan_matches` / `<model>_event_matches` — the residual predicate, from
//!    `ApiGenerator::generate_filter_check`, which re-checks every candidate.
//!
//! A string assertion passes if both contain *a* flooring. It passes just as happily if
//! they floor to different quanta, or if one floors the parameter and the other floors
//! nothing because the field reached it by a different route. Then the pushdown finds the
//! row and the predicate throws it away — an empty page again, for a new reason, with
//! both guards green. Only running the request can separate those.
//!
//! It is also the compile test for this shape. `generate all` emits `database.rs` AND
//! `api.rs`, and the snapshot suite compiles neither.
//!
//! # What each case isolates
//!
//! | Field | Declared | Isolates |
//! |---|---|---|
//! | `t_at` | `^timestamp` | pushdown + predicate, agreeing |
//! | `p_at` | `timestamp(s)`, NOT indexed | the predicate alone — there is no pushdown to mask it, and the quantum is a second, not a millisecond |
//! | `u_at` | `^timestamp(us)` | the control: quantum 1, no flooring emitted anywhere |
//! | `stamp` | `*Stamped`, identity `timestamp(ms)` | an FK column that is a coarse timestamp |
//!
//! The final case is the one that keeps the rest honest: a *different* millisecond
//! bucket must return nothing. Flooring that over-matched — collapsing every value to a
//! shared key — would satisfy every positive assertion above.
//!
//! ```bash
//! cargo test --test timestamp_filter_wire_test -- --ignored --nocapture
//! ```

mod common;

/// One indexed coarse timestamp, one UNindexed coarse timestamp at a different quantum,
/// a `timestamp(us)` control, and an FK whose target identity is itself a coarse
/// timestamp.
const SCHEMA: &str = r#"
Stamped {
  id: timestamp(ms)
  label: string
  kitchens: [Kitchen]
}

Kitchen {
  id: +uuid
  name: string
  t_at: ^timestamp
  p_at: timestamp(s)
  u_at: ^timestamp(us)
  stamp: *Stamped
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored"]
fn a_sub_quantum_filter_param_finds_the_row_it_wrote() {
    let (out, proj) = common::generate_compile_run("tsfilter", SCHEMA, DRIVER);
    common::assert_driver_ok(
        &out,
        &proj,
        "a timestamp filter parameter did not agree with the value that was written",
    );
}

const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use axum::body::Body;
use axum::http::Request;
use forgedb_types::{Timestamp, Uuid};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

static mut FAILURES: u32 = 0;

/// Every value below is MISALIGNED to its field's quantum on purpose. An aligned one
/// passes whether or not the read path floors, which is exactly the state #389 left the
/// round-trip guard in.
const T_AT: i64 = 1_234_567_890_123;      // ^timestamp      — quantum 1_000
const P_AT: i64 = 1_234_567_000_000 + 42; // timestamp(s)    — quantum 1_000_000
const U_AT: i64 = 1_234_567_890_987;      // ^timestamp(us)  — quantum 1, the control
const STAMP_ID: i64 = 1_555_000_111_222;  // identity        — quantum 1_000

/// The envelope's `total` — the match count before pagination. Sliced rather than parsed
/// so a malformed body reports as itself.
fn total_of(body: &str) -> i64 {
    let after = body.split(",\"total\":").nth(1).unwrap_or_else(|| {
        panic!("response is not a list envelope: {body}");
    });
    let end = after.find(',').unwrap_or(after.len());
    after[..end].parse().expect("total is a number")
}

async fn call(router: axum::Router, uri: &str) -> (u16, String) {
    let resp = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("router call");
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, String::from_utf8(bytes.to_vec()).expect("utf8 body"))
}

#[tokio::main]
async fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir"));
    let mut db = Database::open_at(dir);

    // The parent is created with a misaligned identity; it is STORED at the floored one.
    let stamp_id = db
        .create_stamped(Stamped {
            id: Timestamp::from_micros(STAMP_ID),
            label: "s".into(),
            kitchens: (),
        })
        .expect("create stamped");

    db.create_kitchen(Kitchen {
        id: Uuid::nil(),
        name: "k".into(),
        t_at: Timestamp::from_micros(T_AT),
        p_at: Timestamp::from_micros(P_AT),
        u_at: Timestamp::from_micros(U_AT),
        // The RAW instant, not `stamp_id`: a reference is resolved at the field's
        // declared precision, so the same value the parent was created with must
        // resolve to that parent.
        stamp: Timestamp::from_micros(STAMP_ID),
    })
    .expect("create kitchen");

    let state = Arc::new(RwLock::new(db));

    let rfc = |us: i64| Timestamp::from_micros(us).to_rfc3339();

    // (label, uri, expected `total`)
    let cases: Vec<(&str, String, i64)> = vec![
        // Indexed: the pushdown resolves the row AND the predicate keeps it. If only
        // one of the two floors, this is 0.
        (
            "?t_at= a sub-millisecond instant (pushdown + predicate)",
            format!("/api/kitchen?t_at={}", rfc(T_AT)),
            1,
        ),
        // UNindexed, and a second-quantum field: no pushdown exists, so this is the
        // predicate on its own — and it must floor to 1_000_000, not to a millisecond.
        (
            "?p_at= a sub-second instant on an UNindexed field (predicate alone)",
            format!("/api/kitchen?p_at={}", rfc(P_AT)),
            1,
        ),
        // The control. Quantum 1 means no flooring is emitted at all, on either path.
        (
            "?u_at= an exact microsecond (control, no flooring emitted)",
            format!("/api/kitchen?u_at={}", rfc(U_AT)),
            1,
        ),
        // The negative that keeps the three above honest: flooring must map a value
        // into its OWN bucket, not into a shared one. A neighbouring millisecond
        // matches nothing.
        (
            "?t_at= the NEXT millisecond bucket matches nothing",
            format!("/api/kitchen?t_at={}", rfc(T_AT + 1_000)),
            0,
        ),
    ];

    for (label, uri, want) in cases {
        let (status, body) = call(api::create_router(state.clone()), &uri).await;
        if status != 200 {
            println!("FAIL  {label}: HTTP {status}\n      {body}");
            unsafe { FAILURES += 1 };
            continue;
        }
        let got = total_of(&body);
        if got == want {
            println!("ok    {label}");
        } else {
            println!("FAIL  {label}: total {got}, wanted {want}\n      uri {uri}");
            unsafe { FAILURES += 1 };
        }
    }

    // The FK resolved through to the floored parent, which is what let the create above
    // succeed at all — assert it explicitly rather than leaving it implied by `.expect`.
    {
        let db = state.read().await;
        let rows = db.kitchen.find_by_stamp(Timestamp::from_micros(STAMP_ID));
        match rows.first() {
            Some(r) if r.stamp == stamp_id => println!("ok    FK stored and probed at the floored identity"),
            Some(r) => {
                println!("FAIL  FK stored {} , parent is at {}", r.stamp.as_micros(), stamp_id.as_micros());
                unsafe { FAILURES += 1 };
            }
            None => {
                println!("FAIL  find_by_stamp did not resolve the raw instant to its parent");
                unsafe { FAILURES += 1 };
            }
        }
    }

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} timestamp filter failure(s)");
        std::process::exit(1);
    }
    println!("all timestamp filter checks passed");
}

"##;
