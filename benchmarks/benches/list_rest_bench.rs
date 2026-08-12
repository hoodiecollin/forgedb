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
//! needs the generated router, whose deps sit behind `--features router` (**+78** packages,
//! 41 → 119 on the library's normal-deps graph). Adding S4's three client crates —
//! `hyper`, `hyper-util`, `http-body-util` — moved that number by ZERO, because `axum`
//! already pulls all three; the feature gained three direct deps and no new packages.
//! Re-derive rather than reason about it:
//!
//! ```bash
//! cargo tree --manifest-path benchmarks/Cargo.toml -e normal --prefix none \
//!   | awk 'NF{print $1}' | sort -u | wc -l          # and again with --features router
//! ```
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
use forgedb_query_params::{Sort, SortOrder};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

// The grid lives in `forgedb_benchmarks` (`LIST_*`, `list_grid`), not here — all five
// engine suites read the same constants, because a grid that disagrees does not fail, it
// silently compares a 10k page against a 100k one and reads as an engine difference.
//
// What each sweep is FOR:
//   - size sweep: **the load-bearing one.** ForgeDB's unfiltered line has a slope where
//     every SQL engine's is flat, because the read is O(rows) in the *table* rather than
//     O(rows) in the *page*. #281 removed the column-gather term and left the live-row
//     selection term, so the slope survives while its cause changed — which is exactly
//     why a single table size would hide the effect.
//   - limit sweep: at `limit=1000` over 1,000 rows the page IS the table, so a
//     page-bounded gather gathers everything — the regime where #281's win is zero by
//     construction.
use forgedb_benchmarks::{
    LIST_CORE_LIMIT as CORE_LIMIT, LIST_CORE_ROWS as CORE_ROWS, LIST_LIMITS as LIMITS,
    LIST_PROBE_VIEWS as PROBE_VIEWS, LIST_SIZES as SIZES,
};

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

/// The ForgeDB corpus. **Two transactions, users first**: a post's `author` FK validates
/// against **committed** rows, so a single grouped commit would fail.
///
/// Grouped commit (#170) rather than per-row inserts, which is what makes the 100k point
/// of the size sweep affordable — two fsync barriers for N rows instead of N.
fn populated_posts(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    let author = id_of(AUTHOR, 0);
    db.transaction(|tx| {
        tx.create_user(User {
            id: author,
            name: "bench".to_string(),
            email: "bench@example.com".to_string(),
            created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS),
            posts: (),
        })?;
        Ok(())
    })
    .expect("group-commit the author");
    db.transaction(|tx| {
        for i in 0..n {
            tx.create_post(Post {
                id: id_of(8, i),
                title: title_of(i),
                views: i as u64,
                published: i % 2 == 0,
                author,
                created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS + i as i64),
                tags: (),
            })?;
        }
        Ok(())
    })
    .expect("group-commit the posts");
    (db, dir)
}

// --- The three things `gen/api.rs` keeps private, mirrored ------------------------
//
// S1/S2 enter through the generated handler's own page scope, so the scan, the page
// gather, the pagination arithmetic and the serialization are all the generator's code.
// What the harness must supply is exactly what the emitter makes private: the filter
// predicate, the sort comparator, and the pushdown dispatch. Those three CAN drift, which
// is what BDD-1's byte-equality guard exists for.

/// The four query shapes. Each hits a distinct generated path; collapsing them would hide
/// which path a change moved.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// The default request. No filter, no sort — the shape #281 targets and the only one
    /// the two sweeps use.
    Unfiltered,
    /// `published = true`. Scan + `keep` predicate; no index helps any engine.
    FilteredUnindexed,
    /// `views = N` exact. Exercises #160 index pushdown (`__sel = Some(..)`). The
    /// generated REST filter is equality-only (#284), so every SQL mirror is `WHERE views
    /// = N` — which selects ~0–1 rows in this corpus, making this a measurement of the
    /// pushdown path at O(1) rather than of a 50-row filtered page. Stated, not hidden.
    FilteredIndexed,
    /// `ORDER BY views DESC`. Exercises the scan sort. `views` is indexed for ForgeDB
    /// (`^views`) and for SQLite only — see BDD-10 and fairness contract 1.
    Sorted,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Shape::Unfiltered => "unfiltered",
            Shape::FilteredUnindexed => "filtered_unindexed",
            Shape::FilteredIndexed => "filtered_indexed",
            Shape::Sorted => "sorted",
        }
    }

    /// The query map the router would have parsed, minus the pagination keys — #288's
    /// `__post_is_unfiltered` answers on filterable-field names only, so `limit`/`offset`
    /// never belong in here.
    fn params(self) -> HashMap<String, String> {
        let mut p = HashMap::new();
        match self {
            Shape::Unfiltered | Shape::Sorted => {}
            Shape::FilteredUnindexed => {
                p.insert("published".to_string(), "true".to_string());
            }
            Shape::FilteredIndexed => {
                p.insert("views".to_string(), PROBE_VIEWS.to_string());
            }
        }
        p
    }

    fn sort(self) -> Option<Sort> {
        match self {
            Shape::Sorted => Some(Sort::new("views", SortOrder::Desc)),
            _ => None,
        }
    }

    /// The URI the router serves for this shape, mirroring `params()` + `sort()`.
    fn uri(self, offset: usize, limit: usize) -> String {
        let mut q = format!("limit={limit}&offset={offset}");
        for (k, v) in self.params() {
            q.push_str(&format!("&{k}={v}"));
        }
        if self == Shape::Sorted {
            q.push_str("&sort=views&order=desc");
        }
        format!("/api/post?{q}")
    }
}

/// Mirrors `__post_is_unfiltered` (#288): does NO query key name a filterable field?
///
/// Positive, exactly as the generator emits it. A negative exclusion list would need
/// maintaining and would be wrong for a model that legally declares a field named `limit`.
fn keep_all(params: &HashMap<String, String>) -> bool {
    !["id", "title", "views", "published", "created_at"]
        .iter()
        .any(|f| params.contains_key(*f))
}

/// Mirrors `__post_scan_matches`: equality on each present filterable key.
fn scan_matches(r: &PostScanRef<'_>, params: &HashMap<String, String>) -> bool {
    if params.is_empty() {
        return true;
    }
    if let Some(want) = params.get("views") {
        match want.parse::<u64>() {
            Ok(w) if r.views == w => {}
            _ => return false,
        }
    }
    if let Some(want) = params.get("published") {
        match want.parse::<bool>() {
            Ok(w) if r.published == w => {}
            _ => return false,
        }
    }
    if let Some(want) = params.get("title") {
        if r.title != want.as_str() {
            return false;
        }
    }
    true
}

/// Mirrors `__post_scan_sort`.
///
/// **`sort_by` then `reverse()`, not a descending comparator.** The generator emits
/// exactly that, and it is not the same thing: `sort_by` is stable, so reversing afterwards
/// reverses ties too. Mirroring the *semantics* instead of the *code* here would make
/// BDD-1's byte-equality guard fail on a tie — and `Post.views` is unique in this corpus,
/// so it would fail only on some other schema, later, for a reason nobody would connect
/// back to this line.
fn scan_sort(rows: &mut Vec<PostScanRef<'_>>, sort: &Option<Sort>) {
    let Some(sort) = sort.as_ref() else { return };
    match sort.field.as_str() {
        "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
        "title" => rows.sort_by(|a, b| a.title.cmp(&b.title)),
        "views" => rows.sort_by(|a, b| a.views.cmp(&b.views)),
        "published" => rows.sort_by(|a, b| a.published.cmp(&b.published)),
        "created_at" => rows.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        _ => return,
    }
    if sort.is_descending() {
        rows.reverse();
    }
}

/// Mirrors the #160 pushdown chain: `views` is the one indexed filterable field of `Post`.
fn row_selection(db: &Database, params: &HashMap<String, String>) -> Option<Vec<usize>> {
    match params.get("views") {
        Some(v) => db.post.__rows_by_views(v),
        None => None,
    }
}

/// The checksum that keeps a page's view construction from being dead code (BDD-2).
///
/// Touches **every** field of the page view, including `author`, which `PostScanRef` does
/// not carry — so this fold is what forces the second gather the page view exists to do.
/// A fold over a subset would let the optimizer skip a column and the S1 number would
/// describe less work than the shipped handler does.
fn page_fold(page: &[PostPageRef<'_>]) -> u64 {
    let mut acc = 0u64;
    for r in page {
        acc ^= r.id.as_u128() as u64;
        acc = acc.wrapping_add(r.title.len() as u64);
        acc ^= r.views;
        acc = acc.wrapping_add(r.published as u64);
        acc ^= r.author.as_u128() as u64;
        acc ^= r.created_at.as_micros() as u64;
    }
    acc
}

/// **S1** — the engine hands back its page; nothing is serialized.
///
/// The boundary is the **borrowed page view** `&[PostPageRef]`, which is the
/// representation the generated handler's live arm hands its terminal closure. Measuring
/// owned `Vec<Post>` here would be a counterfactual arm: it would publish ForgeDB ~40%
/// worse than the code that ships, and would carry #226's phase-B win into `S3 − S2` with
/// the wrong sign.
fn s1_rows(db: &Database, shape: Shape, offset: usize, limit: usize) -> (usize, u64) {
    let params = shape.params();
    let sort = shape.sort();
    let all = keep_all(&params);
    db.post.__with_page(
        row_selection(db, &params),
        |r: &PostScanRef<'_>| all || scan_matches(r, &params),
        |scan: &mut Vec<PostScanRef<'_>>| scan_sort(scan, &sort),
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| (total, page_fold(page)),
    )
}

/// **S2** — the shipped request minus routing: S1 plus `serde_json` over the same page.
///
/// The timed arm serializes the **bare array**; the router adds the envelope, and that
/// difference is part of `R`. Keeping the envelope out of S2 is what makes `S3 − S2` the
/// accepted label rather than a subtraction with the envelope on both sides.
fn s2_json(db: &Database, shape: Shape, offset: usize, limit: usize) -> (usize, String) {
    let params = shape.params();
    let sort = shape.sort();
    let all = keep_all(&params);
    db.post.__with_page(
        row_selection(db, &params),
        |r: &PostScanRef<'_>| all || scan_matches(r, &params),
        |scan: &mut Vec<PostScanRef<'_>>| scan_sort(scan, &sort),
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

/// **S1, the shipped form** for the unfiltered shape (#281). See `s2_json_fast`.
fn s1_rows_fast(db: &Database, offset: usize, limit: usize) -> (usize, u64) {
    db.post.__with_fast_page(offset, limit, |total: usize, page: &[PostPageRef<'_>]| {
        (total, page_fold(page))
    })
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

// --- The harness ------------------------------------------------------------------
//
// The runtime and the router are built ONCE per corpus, outside every loop.
// Constructing either inside `b.iter` costs ~100x a request and would swallow `R`
// entirely -- the single most likely way this file produces a wrong number.

struct Fixture {
    rt: tokio::runtime::Runtime,
    state: Arc<RwLock<Database>>,
    router: axum::Router,
    rows: usize,
    _dir: tempfile::TempDir,
}

fn fixture(rows: usize) -> Fixture {
    let (db, dir) = populated_posts(rows);
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let state = Arc::new(RwLock::new(db));
    let router = api::create_router(state.clone());
    Fixture { rt, state, router, rows, _dir: dir }
}

impl Fixture {
    fn get(&self, uri: &str) -> (u16, String) {
        self.rt.block_on(async {
            let resp = self
                .router
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(uri)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .expect("router call");
            let status = resp.status().as_u16();
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .expect("read body");
            (status, String::from_utf8(bytes.to_vec()).expect("utf8"))
        })
    }
}

/// `"total":N` out of the generated envelope, without pulling in a JSON parser for one
/// field. Used to cross-check the harness's mirrored predicate against the router's.
fn envelope_total(body: &str) -> usize {
    let key = r#""total":"#;
    let at = body.find(key).expect("envelope carries `total`") + key.len();
    let rest = &body[at..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().expect("numeric total")
}

/// The `data` array of the generated envelope, verbatim.
///
/// The emitter writes `{"data":[…],"total":N,"limit":L,"offset":O}` in that fixed field
/// order (`__ListEnvelope`), so the array is exactly what sits between the two markers.
/// Extracting the slice rather than re-serializing a parsed value is deliberate: a
/// round-trip through `serde_json::Value` would normalise formatting and could hide the
/// very byte difference this is looking for.
fn envelope_data(body: &str) -> &str {
    let head = r#"{"data":"#;
    let tail = r#","total":"#;
    let start = body.find(head).expect("envelope starts with `data`") + head.len();
    let end = body.find(tail).expect("envelope carries `total` after `data`");
    &body[start..end]
}

/// **BDD-2 and the mirrored-logic cross-check**, run outside every timed loop.
///
/// The generated predicate, comparator and pushdown dispatch are private, so the harness
/// reimplements them and they CAN drift. `total` is the live-row count *after* filtering,
/// so comparing the harness's total against the router's own envelope catches a drifted
/// predicate or a drifted pushdown on the spot -- in-run, per shape, per point, for the
/// cost of one untimed request.
///
/// **`total` alone is not enough, and a mutation proved it rather than a review.**
/// Inverting the mirrored predicate (`r.published == w` -> `!=`) selects the *other* half
/// of a 50/50 corpus: a different row set with an identical **count**, so a `total`
/// comparison stayed green through it. `total` answers "how many", never "which". So the
/// page **bytes** are compared too -- the harness's bare array against the `data` array
/// lifted out of the router's envelope -- which is what makes a drifted *comparator*
/// visible as well, since sorting changes which rows the page holds and not how many.
///
/// This is the in-run half of BDD-1. The `tests/` form still owes the full-envelope
/// comparison; this one is free, runs on every shape at every point, and needs no
/// generated crate compile.
fn verify_shape(fx: &Fixture, shape: Shape, offset: usize, limit: usize) {
    let guard = fx.rt.block_on(fx.state.read());
    let (total, fold) = s1_rows(&guard, shape, offset, limit);
    let (json_total, body) = s2_json(&guard, shape, offset, limit);

    assert_eq!(total, json_total, "{}: S1/S2 disagree on total", shape.label());
    assert!(
        body.starts_with('['),
        "{}: S2 must serialize the BARE array, not the envelope",
        shape.label()
    );

    // BDD-2: the S1 fold must be live. An empty page folds to 0 legitimately, so the
    // assertion is conditional on the page being non-empty -- an unconditional one would
    // fail on `offset` past the end and get "fixed" by weakening it.
    let page_len = total.saturating_sub(offset).min(limit);
    if page_len > 0 {
        assert_ne!(
            fold, 0,
            "{}: the S1 fold returned 0 over {page_len} rows -- if the fold is dead code \
             the S1 arm is not measuring the page view's construction",
            shape.label()
        );
    }

    // The router must agree with the mirrored logic on how many rows match.
    drop(guard);
    let uri = shape.uri(offset, limit);
    let (status, routed) = fx.get(&uri);
    assert_eq!(status, 200, "{}: {uri} -> {status}", shape.label());
    assert_eq!(
        envelope_total(&routed),
        total,
        "{}: the harness's mirrored filter disagrees with the router's at {uri}. \
         The predicate, the pushdown dispatch or both have drifted from `gen/api.rs`.",
        shape.label()
    );
    assert_eq!(
        envelope_data(&routed),
        body.as_str(),
        "{}: the harness's page BYTES differ from the router's `data` at {uri}. \
         The counts matched, so this is a predicate selecting a different row set of the \
         same size, or a drifted sort comparator -- neither of which `total` can see.",
        shape.label()
    );
}

/// **BDD-4** — the filtered-indexed shape actually pushes down, and is honest about how
/// few rows that selects.
fn verify_pushdown(fx: &Fixture) {
    let guard = fx.rt.block_on(fx.state.read());
    let params = Shape::FilteredIndexed.params();
    let sel = row_selection(&guard, &params);
    let sel = sel.expect(
        "BDD-4: `views=N` must resolve through `__rows_by_views` -- a `None` here means \
         the shape is measuring a full scan and the pushdown path is untested",
    );
    assert!(
        sel.len() <= 1,
        "BDD-4: `views` is unique in this corpus, so an equality probe should select \
         0-1 rows, got {}. This shape measures the pushdown path at O(1), NOT a 50-row \
         filtered page -- and that is what gets stated in the writeup.",
        sel.len()
    );

    // The unfiltered shape must NOT push down: it names no filterable field, so there is
    // no index to resolve. This is the other half of the claim -- without it, "pushdown
    // happens" could be true unconditionally and the shape would prove nothing.
    assert!(
        row_selection(&guard, &Shape::Unfiltered.params()).is_none(),
        "BDD-4: the unfiltered shape resolved an index selection; #281's fast path sits \
         ABOVE the pushdown binding precisely because a request naming no filterable \
         field resolves no index"
    );
}

/// Register the unfiltered pair at one `(rows, limit)` point: both boundaries, and for
/// each, both the shipped `__with_fast_page` and the retained `__with_page` control.
///
/// Registration order is the pairing: each arm sits next to the one it is subtracted
/// from, so the two halves of a delta are measured seconds apart rather than minutes.
fn register_unfiltered(g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>, fx: &Fixture, limit: usize) {
    let label = format!("rows={}/limit={limit}", fx.rows);
    let shape = Shape::Unfiltered;

    g.bench_with_input(BenchmarkId::new("s1_rows", &label), &limit, |b, &l| {
        let guard = fx.rt.block_on(fx.state.read());
        b.iter(|| s1_rows(&guard, shape, 0, l));
    });
    g.bench_with_input(BenchmarkId::new("s1_rows_fast", &label), &limit, |b, &l| {
        let guard = fx.rt.block_on(fx.state.read());
        b.iter(|| s1_rows_fast(&guard, 0, l));
    });
    g.bench_with_input(BenchmarkId::new("s2_json", &label), &limit, |b, &l| {
        let guard = fx.rt.block_on(fx.state.read());
        b.iter(|| s2_json(&guard, shape, 0, l));
    });
    g.bench_with_input(BenchmarkId::new("s2_json_fast", &label), &limit, |b, &l| {
        let guard = fx.rt.block_on(fx.state.read());
        b.iter(|| s2_json_fast(&guard, 0, l));
    });

    // S3 -- the real router. `noparams` is the fifth rung; see the module docs.
    let uri = format!("/api/post?limit={limit}");
    g.bench_with_input(BenchmarkId::new("s3_router", &label), &uri, |b, u| {
        b.iter(|| fx.get(u));
    });
    if limit == CORE_LIMIT {
        // `GET /api/post` sends `PAGE_DEFAULT_LIMIT`, so the rung is only comparable to
        // `s3_router` at that limit. Registering it elsewhere would subtract two
        // different requests.
        g.bench_with_input(
            BenchmarkId::new("s3_router_noparams", &label),
            "/api/post",
            |b, u| b.iter(|| fx.get(u)),
        );
    }
}

/// The core grid: four shapes x two boundaries, at `rows=10_000, limit=50`.
fn bench_core(c: &mut Criterion) {
    let fx = fixture(CORE_ROWS);
    verify_pushdown(&fx);

    let mut g = c.benchmark_group("forgedb/list_core");
    for shape in [
        Shape::Unfiltered,
        Shape::FilteredUnindexed,
        Shape::FilteredIndexed,
        Shape::Sorted,
    ] {
        verify_shape(&fx, shape, 0, CORE_LIMIT);
        let label = format!("{}/rows={}/limit={CORE_LIMIT}", shape.label(), fx.rows);

        g.bench_with_input(BenchmarkId::new("s1_rows", &label), &shape, |b, &s| {
            let guard = fx.rt.block_on(fx.state.read());
            b.iter(|| s1_rows(&guard, s, 0, CORE_LIMIT));
        });
        g.bench_with_input(BenchmarkId::new("s2_json", &label), &shape, |b, &s| {
            let guard = fx.rt.block_on(fx.state.read());
            b.iter(|| s2_json(&guard, s, 0, CORE_LIMIT));
        });
        let uri = shape.uri(0, CORE_LIMIT);
        g.bench_with_input(BenchmarkId::new("s3_router", &label), &uri, |b, u| {
            b.iter(|| fx.get(u));
        });
    }
    g.finish();
}

/// The size sweep: unfiltered, `limit=50`, `rows` across `SIZES`. One corpus per size,
/// each built with grouped commit so the 100k point is affordable.
fn bench_size_sweep(c: &mut Criterion) {
    let mut g = c.benchmark_group("forgedb/list_unfiltered");
    for rows in SIZES {
        let fx = fixture(rows);
        verify_shape(&fx, Shape::Unfiltered, 0, CORE_LIMIT);
        register_unfiltered(&mut g, &fx, CORE_LIMIT);
    }
    g.finish();
}

/// The limit sweep: unfiltered, `rows=10_000`, `limit` across `LIMITS`. The `limit=50`
/// point is the size sweep's and is not repeated.
fn bench_limit_sweep(c: &mut Criterion) {
    let fx = fixture(CORE_ROWS);
    let mut g = c.benchmark_group("forgedb/list_unfiltered_limits");
    for limit in LIMITS.into_iter().filter(|l| *l != CORE_LIMIT) {
        verify_shape(&fx, Shape::Unfiltered, 0, limit);
        register_unfiltered(&mut g, &fx, limit);
    }
    g.finish();
}

/// **S4** — the same request over a real TCP socket.
///
/// One keep-alive connection, established **outside** the timing loop and reused. That is
/// a methodology decision, not a convenience: connection setup is a per-*client* cost, not
/// a per-*request* one, and billing it to every request would inflate S4 systematically.
/// If setup cost is itself interesting it gets its own arm rather than contaminating this.
///
/// **BDD-6 is executable rather than asserted in prose**, via axum's own
/// `ListenerExt::tap_io`: the server counts accepts, and after the timed arms have run the
/// count must still be 1. Without that, "we reuse the connection" is a claim about code
/// nobody re-reads, and a regression to connect-per-request would show up only as an
/// unexplained S4 that everyone assumes is just TCP being slow.
struct SocketArm {
    sender: hyper::client::conn::http1::SendRequest<http_body_util::Empty<hyper::body::Bytes>>,
    accepts: Arc<std::sync::atomic::AtomicUsize>,
}

fn bind_socket(fx: &Fixture) -> SocketArm {
    use axum::serve::ListenerExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fx.rt.block_on(async {
        let accepts = Arc::new(AtomicUsize::new(0));
        let counter = accepts.clone();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind an ephemeral port");
        let addr = listener.local_addr().expect("local addr");
        let listener = listener.tap_io(move |_io| {
            counter.fetch_add(1, Ordering::Relaxed);
        });

        let router = fx.router.clone();
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("serve");
        });

        // The ONE connection. Handshaking here rather than inside `b.iter` is the whole
        // point of the arm.
        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to the bench server");
        let (sender, conn) = hyper::client::conn::http1::handshake(
            hyper_util::rt::TokioIo::new(stream),
        )
        .await
        .expect("http1 handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });

        SocketArm { sender, accepts }
    })
}

impl SocketArm {
    fn get(&mut self, rt: &tokio::runtime::Runtime, uri: &str) -> hyper::body::Bytes {
        use http_body_util::BodyExt;
        rt.block_on(async {
            let req = hyper::Request::builder()
                .uri(uri)
                // Origin-form request, so HTTP/1.1 needs Host set explicitly.
                .header(hyper::header::HOST, "localhost")
                .body(http_body_util::Empty::<hyper::body::Bytes>::new())
                .expect("build request");
            let resp = self.sender.send_request(req).await.expect("socket request");
            assert_eq!(resp.status().as_u16(), 200, "S4 status for {uri}");
            resp.into_body().collect().await.expect("collect").to_bytes()
        })
    }
}

/// S4 at the core point, mirroring S3's four shapes so `S4 - S3` is an in-run paired
/// subtraction per shape rather than one aggregate number.
fn bench_socket(c: &mut Criterion) {
    use std::sync::atomic::Ordering;

    let fx = fixture(CORE_ROWS);
    let mut arm = bind_socket(&fx);

    let mut g = c.benchmark_group("forgedb/list_socket");
    for shape in [
        Shape::Unfiltered,
        Shape::FilteredUnindexed,
        Shape::FilteredIndexed,
        Shape::Sorted,
    ] {
        let uri = shape.uri(0, CORE_LIMIT);
        let label = format!("{}/rows={}/limit={CORE_LIMIT}", shape.label(), fx.rows);

        // Untimed: the socket and the in-process router must return the same bytes. This
        // is what makes `S4 - S3` a transport measurement rather than a comparison of two
        // different responses.
        let over_socket = arm.get(&fx.rt, &uri);
        let (status, in_process) = fx.get(&uri);
        assert_eq!(status, 200, "{}: oneshot status", shape.label());
        assert_eq!(
            std::str::from_utf8(&over_socket).expect("utf8"),
            in_process.as_str(),
            "{}: the socket and the oneshot router disagree at {uri}",
            shape.label()
        );

        g.bench_with_input(BenchmarkId::new("s4_socket", &label), &uri, |b, u| {
            b.iter(|| arm.get(&fx.rt, u));
        });
    }
    g.finish();

    // BDD-6. Every timed iteration above went through the connection opened in
    // `bind_socket`; a connect-per-request regression lands here.
    let accepted = arm.accepts.load(Ordering::Relaxed);
    assert_eq!(
        accepted, 1,
        "BDD-6: S4 must reuse ONE keep-alive connection -- {accepted} were accepted, so \
         connection setup is being billed to every request and every S4 number is inflated"
    );
}

criterion_group!(benches, bench_core, bench_size_sweep, bench_limit_sweep, bench_socket);
criterion_main!(benches);
