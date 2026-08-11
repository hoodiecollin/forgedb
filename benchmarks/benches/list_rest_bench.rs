//! #282 scenario 21 — the generated REST list endpoint, measured at four boundaries.
//!
//! **STEP 3 (calibration gate) — PASSED, with an amendment it produced.** This file
//! holds the calibration arms only; the other 82 cells are built on top of it. It exists
//! to answer one question first: **how big is the routing tax `R`?** Nothing under
//! `benchmarks/` had ever measured axum routing / extractor / envelope cost, so the whole
//! argument for measuring ForgeDB's S1 at the borrowed page view rested on `R` being
//! small relative to #226's win — an argument until this file made it a number.
//!
//! It is small: **10–15 µs, and O(1) in table size.** But getting there required a fifth
//! rung, because the obvious subtraction measures something else entirely.
//!
//! # The boundaries — FIVE rungs, not four
//!
//! ```text
//!   S1   typed rows      the engine hands back its page, nothing serialized
//!   S2   JSON bytes      + serialize that page to a bare JSON array
//!   S3-0 router          + routing, extractors, envelope        <- GET /api/post
//!   S3   + predicate     + the per-row scan predicate           <- GET /api/post?limit=50
//!   S4   socket          + HTTP parse, connection handling, TCP
//! ```
//!
//! **`S3 − S2` is NOT "routing + extractors + envelope", and that was this file's first
//! finding.** The generated `__post_scan_matches` short-circuits on `params.is_empty()`;
//! `?limit=50` makes the map non-empty without naming a single filterable field, so an
//! *unfiltered* request runs six `HashMap` lookups per scanned row. Measured:
//!
//! ```text
//!   R = S3-0 − S2     10.15 µs (1k rows)    15.04 µs (10k rows)    O(1)
//!   P = S3   − S3-0   49.67 µs (1k rows)   501.55 µs (10k rows)    ~50 ns/row
//! ```
//!
//! At 10k rows `S3 − S2` is **97% P and 3% R**. Per-row cost is 49.7 ns and 50.2 ns
//! across a 10× table — identical, which is what proves it is the six lookups rather
//! than anything else in the request.
//!
//! **P must never enter a cross-engine cell.** No SQL engine evaluates a per-row
//! predicate for a query with no `WHERE`, so pairing S3 against another engine's S2 would
//! malign ForgeDB for work that engine never did — the exact failure the fairness
//! contract exists to name. The cross-engine headline is **S2**; S3-0 and S3 are
//! ForgeDB-internal rungs.
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
//! needs the generated router, whose deps sit behind `--features router` (+78 packages).
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
    // same 50 rows (50 IS the default), but the generated `__post_scan_matches`
    // short-circuits on `params.is_empty()`, which only the first URI satisfies. The
    // second therefore runs six `params.get(..)` HashMap lookups PER SCANNED ROW.
    //
    // This arm is what makes `R` measurable: without it, `S3 - S2` conflates the routing
    // tax with a per-row engine cost 33x its size, and the published number for "routing
    // + extractors + envelope" would be wrong by that factor. Measured on this file:
    // `R = S3-0 - S2` is 10-15 us and O(1), while `P = S3 - S3-0` is ~50 ns/row.
    // Do not remove it to "simplify the ladder"; it is the only in-run instrument that
    // separates the two, and it needs no hand-written copy of the generated predicate.
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
