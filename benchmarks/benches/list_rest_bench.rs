//! #282 scenario 21 — the generated REST list endpoint, measured at four boundaries.
//!
//! **STEP 3 (calibration gate) — PASSED, with an amendment it produced.** This file
//! holds the calibration arms only; the other 82 cells are built on top of it. It exists
//! to answer one question first: **how big is the routing tax `R`?** Nothing under
//! `benchmarks/` had ever measured axum routing / extractor / envelope cost, so the whole
//! argument for measuring ForgeDB's S1 at the borrowed page view rested on `R` being
//! small relative to #226's win — an argument until this file made it a number.
//!
//! It is small: **7–8 µs, and O(1) in table size.** But getting there required a fifth
//! rung, because at the time of measuring, the obvious subtraction measured something
//! else entirely — and the rung is retained now for a *different* reason than the one
//! that introduced it. Both are below.
//!
//! # The boundaries — FIVE rungs, not four
//!
//! ```text
//!   S1   typed rows      the engine hands back its page, nothing serialized
//!   S2   JSON bytes      + serialize that page to a bare JSON array
//!   S3-0 router          + routing, extractors, envelope        <- GET /api/post
//!   S3   + predicate     + whatever the query string costs      <- GET /api/post?limit=50
//!   S4   socket          + HTTP parse, connection handling, TCP
//! ```
//!
//! ## Why the fifth rung exists, and why it stays
//!
//! **First measured at `d087574`, `S3 − S2` was NOT "routing + extractors + envelope".**
//! The generated `__post_scan_matches` short-circuited on `params.is_empty()`; `?limit=50`
//! makes the map non-empty **without naming a single filterable field**, so an *unfiltered*
//! request ran six `HashMap` lookups **per scanned row**. That per-row term — `P` — was
//! **97% of the subtraction at 10k rows**, so the accepted label was wrong by 34×. Isolating
//! it needed a rung whose URI satisfies the short-circuit, and no hand-written copy of the
//! generated predicate.
//!
//! **That finding is what filed #288**, which hoisted the predicate out of the per-row loop
//! behind `__post_is_unfiltered(params)`. So the rung's original justification is *gone* —
//! and the rung is kept anyway, because what it now measures is worth more than what it was
//! built for. Post-#288 and post-#281, over `Post`, unfiltered, in one paired run:
//!
//! ```text
//!            was (d087574)              is                       class
//!   R = S3-0 − S2   10.15 / 15.04 µs    8.03 ±0.27 / 7.03 ±0.90 µs   O(1)
//!   P = S3   − S3-0 49.67 / 501.55 µs   0.71 ±0.45 / 0.76 ±0.92 µs   O(1)
//!                   ~50 ns/row          0.08 ns/row at 10k
//! ```
//!
//! **#288 changed `P`'s complexity class, not merely its size** — 660× at the core grid
//! point, and at 10k rows `P` is now *inside its own confidence band*. `S3 − S2` is
//! 7.79 ±0.78 µs of which `P` is 9.8%, so **the accepted Gate 1 label is true again**.
//!
//! The rung therefore converts from an arithmetic necessity into a **standing O(1)
//! assertion on #288's hoist**: it is the only instrument in this repo that measures that
//! hoist's effect *at the request boundary* (the codegen guards prove the hoist is
//! **emitted**; this proves it is **effective**). A regression shows up here as `P`
//! reacquiring a slope in table size. Do not delete it to "simplify the ladder" now that
//! the two terms are no longer comparable in size — a guard is most worth keeping exactly
//! when it is reading zero.
//!
//! **`P` must never enter a cross-engine cell.** No SQL engine evaluates a per-row
//! predicate for a query with no `WHERE`, so pairing S3 against another engine's S2 would
//! malign ForgeDB for work that engine never did — the exact failure the fairness
//! contract exists to name. The cross-engine headline is **S2**; S3-0 and S3 are
//! ForgeDB-internal rungs. That holds even now that `P` rounds to nothing: the rule is
//! about what the term *is*, not how big it currently measures.
//!
//! # The S2 arm is the SHIPPED page method, and the retired one is kept beside it
//!
//! Post-#281 the handler's unfiltered, unsorted arm calls `__with_fast_page`, not
//! `__with_page`. `s2_json_fast` is therefore the arm that mirrors the shipped request and
//! `s2_json` is the **in-run control** — retained, not replaced, because the difference
//! between them *is* #281's win as seen from the request boundary (19.13 ±0.40 µs at 1k,
//! 180.99 ±2.00 µs at 10k) and a control measured in another run is not evidence here.
//!
//! **Computing `R` against the retired arm yields −173.96 µs at 10k — negative.** That is
//! not noise and not a broken harness: it is the step-3 gate's own escape clause firing.
//! A negative `R` on this shape means the bench arm is measuring a path the router no
//! longer takes. The fix is to repoint the arm, never to relax the gate.
//!
//! Each delta is an **in-run paired subtraction** — both halves in one Criterion run over
//! one dataset, registered adjacently so they are measured seconds apart rather than
//! minutes. A before/after across two runs on a machine whose state moved is not evidence
//! in this project.
//!
//! One resolved question, recorded so nobody re-opens it: **`Router::into_service` buys
//! nothing.** A long-lived service measured 128.96 vs 128.72 µs (1k) and 855.94 vs
//! 850.27 µs (10k) against the `oneshot`-on-a-clone shape — each inside the other's CI.
//! The per-request `Router::clone` is therefore **not** a component of `R`.
//!
//! # ForgeDB's S1 is a borrowed page view, and that is a disclosure, not a detail
//!
//! Post-#226 the shipped router serializes `&[<Model>PageRef]` borrowed straight out of
//! the scan buffers — it never materializes `Vec<Post>`. Measuring S1 as owned rows
//! would have been a **counterfactual arm**: it would publish ForgeDB ~40% worse than
//! the code that actually ships, and it would carry #226's phase-B win into `S3 − S2`
//! with the wrong sign. So S1 is the representation the handler's live arm hands its
//! terminal closure. The cross-engine headline is therefore **S2**, where every engine
//! writes byte-identical JSON.
//!
//! # Why this is a separate target from `list_page_bench`
//!
//! They answer different questions — that one is #226's phase-split attribution with
//! its historical control, this one is the cross-engine boundary ladder — and this one
//! needs the generated router, whose deps sit behind `--features router` (**+81** packages,
//! 41 → 122 on the library's normal-deps graph — measured, not the +78 the plan predicted;
//! the three extra are the S4 client crates).
//! `required-features` is a per-target gate, so co-locating them would make
//! `make bench-forgedb` compile axum. Do NOT factor shared helpers between the two
//! files: a shared module would drag the router deps into an unfeatured target and
//! reintroduce exactly the cost the feature exists to gate. Duplicating the
//! `populated_*` fixtures is the correct trade.
//!
//! ```bash
//! make bench-list
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use forgedb_benchmarks::forgedb_generated::{
    Database, Post, PostPageRef, PostScanRef, User,
};
use forgedb_benchmarks::forgedb_api as api;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

/// The core grid point: `READ_POSTS` in every one of the five engine suites.
const CORE_ROWS: usize = 10_000;

/// `PAGE_DEFAULT_LIMIT` from the generated handler — what a bare `GET /api/post` sends.
const CORE_LIMIT: usize = 50;

const AUTHOR: u8 = 7;
const BASE_SECS: i64 = 1_700_000_000;

fn id_of(tag: u8, i: usize) -> uuid::Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = tag;
    bytes[8..16].copy_from_slice(&(i as u64).to_be_bytes());
    uuid::Uuid::from_bytes(bytes)
}

fn title_of(i: usize) -> String {
    let mut s = String::with_capacity(64);
    s.push('t');
    while s.len() < 64 {
        s.push((b'a' + ((i + s.len()) % 26) as u8) as char);
    }
    s
}

/// The ForgeDB corpus. Two transactions, users first: a post's `author` FK validates
/// against **committed** rows, so a single grouped commit would fail.
fn populated_posts(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    let author = id_of(AUTHOR, 0);
    db.user
        .insert(User {
            id: author,
            name: "bench".to_string(),
            email: "bench@example.com".to_string(),
            created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS),
            posts: (),
        })
        .expect("insert user");
    for i in 0..n {
        db.post
            .insert(Post {
                id: id_of(8, i),
                title: title_of(i),
                views: i as u64,
                published: i % 2 == 0,
                author,
                created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS + i as i64),
                tags: (),
            })
            .expect("insert post");
    }
    (db, dir)
}

/// **S2** — the shipped request minus routing. One page-scope call whose terminal
/// closure serializes the **bare array**, exactly as `list_page_bench`'s
/// `full_buffered` does. The router adds the envelope; that difference is `R`.
fn s2_json(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.post.__with_page(
        None,
        |_: &PostScanRef<'_>| true,
        |_: &mut Vec<PostScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

/// **S2, the shipped form.** `__with_fast_page` is what the generated handler's
/// unfiltered, unsorted arm calls post-#281, and #288's hoisted `__post_is_unfiltered`
/// is what routes `?limit=50` into it. Same terminal closure, same bare array, same
/// `<Model>PageRef` representation the fairness contract names — the difference from
/// `s2_json` is entirely *which generated method* produced the view, which is why the
/// retired arm is kept beside it as the in-run control rather than deleted.
fn s2_json_fast(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.post.__with_fast_page(
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

fn bench_calibration(c: &mut Criterion) {
    for rows in [1_000usize, 10_000usize] {
        bench_calibration_at(c, rows);
    }
}

fn bench_calibration_at(c: &mut Criterion, core_rows: usize) {
    let (db, _dir) = populated_posts(core_rows);
    let _ = CORE_ROWS;

    // The runtime and the router are built ONCE, outside every loop. Constructing
    // either inside `b.iter` costs ~100x a request and would swallow `R` entirely --
    // the single most likely way this gate produces a wrong number.
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let state = Arc::new(RwLock::new(db));
    let router = api::create_router(state.clone());

    let uri = format!("/api/post?limit={CORE_LIMIT}");
    let label = format!("rows={core_rows}/limit={CORE_LIMIT}");

    // Sanity, outside the timed loops: the two arms must be describing the same page.
    {
        let guard = rt.block_on(state.read());
        let (total, body) = s2_json(&guard, 0, CORE_LIMIT);
        assert_eq!(total, core_rows, "S2 total");
        assert!(body.starts_with('['), "S2 must serialize the BARE array");

        // The retired arm and the shipped arm must describe the SAME page, byte for
        // byte. This is what licenses reporting their difference as the cost of the
        // method rather than as a difference in what was measured -- and it is the
        // only assertion here that would catch `__with_fast_page` returning a
        // differently-ordered or differently-bounded window than `__with_page`.
        let (fast_total, fast_body) = s2_json_fast(&guard, 0, CORE_LIMIT);
        assert_eq!(fast_total, total, "S2 fast/retired total");
        assert_eq!(fast_body, body, "S2 fast/retired page bytes");
    }
    let routed = rt.block_on(async {
        let resp = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(&uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router call");
        assert_eq!(resp.status().as_u16(), 200, "S3 status");
        axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body")
    });
    let routed = String::from_utf8(routed.to_vec()).expect("utf8");
    assert!(
        routed.starts_with(r#"{"data":["#),
        "S3 must be the enveloped form, got: {}",
        &routed[..routed.len().min(60)]
    );

    let mut g = c.benchmark_group("forgedb/list_unfiltered");

    g.bench_with_input(BenchmarkId::new("s2_json", &label), &CORE_LIMIT, |b, &l| {
        let guard = rt.block_on(state.read());
        b.iter(|| s2_json(&guard, 0, l));
    });

    g.bench_with_input(
        BenchmarkId::new("s2_json_fast", &label),
        &CORE_LIMIT,
        |b, &l| {
            let guard = rt.block_on(state.read());
            b.iter(|| s2_json_fast(&guard, 0, l));
        },
    );

    g.bench_with_input(BenchmarkId::new("s3_router", &label), &uri, |b, u| {
        b.iter(|| {
            rt.block_on(async {
                let resp = router
                    .clone()
                    .oneshot(
                        axum::http::Request::builder()
                            .uri(u.as_str())
                            .body(axum::body::Body::empty())
                            .unwrap(),
                    )
                    .await
                    .expect("router call");
                axum::body::to_bytes(resp.into_body(), usize::MAX)
                    .await
                    .expect("read body")
            })
        });
    });

    // **S3-0** -- the SAME logical request with no query string at all, and a permanent
    // rung rather than a probe. `GET /api/post` and `GET /api/post?limit=50` return the
    // same 50 rows (50 IS the default), and post-#288 both take the hoisted unfiltered
    // path: `__post_is_unfiltered` answers on `contains_key` over the model's filterable
    // fields, and `limit` is not one of them.
    //
    // At `d087574` that was NOT true -- `__post_scan_matches` short-circuited on
    // `params.is_empty()`, which only the first URI satisfies, so the second ran six
    // `params.get(..)` lookups PER SCANNED ROW. This arm is what caught that (`P` was 97%
    // of `S3 - S2` at 10k rows, making the published "routing + extractors + envelope"
    // figure wrong by 34x) and it is what filed #288.
    //
    // Keep it. `P` is now 0.71/0.76 us -- O(1), and inside its own band at 10k -- so this
    // arm no longer separates two comparable terms. What it does instead is assert, at the
    // request boundary, that #288's hoist is still effective: a regression reappears as `P`
    // reacquiring a slope in table size. The codegen guards prove the hoist is emitted;
    // only this proves it works. A guard reading zero is not a guard worth deleting.
    // It also needs no hand-written copy of the generated predicate, which is why it is
    // an extra URI rather than an extra closure.
    g.bench_with_input(
        BenchmarkId::new("s3_router_noparams", &label),
        "/api/post",
        |b, u| {
            b.iter(|| {
                rt.block_on(async {
                    let resp = router
                        .clone()
                        .oneshot(
                            axum::http::Request::builder()
                                .uri(u)
                                .body(axum::body::Body::empty())
                                .unwrap(),
                        )
                        .await
                        .expect("router call");
                    axum::body::to_bytes(resp.into_body(), usize::MAX)
                        .await
                        .expect("read body")
                })
            });
        },
    );

    g.finish();
}

criterion_group!(benches, bench_calibration);
criterion_main!(benches);
