mod common;

const SCHEMA: &str = r#"
User {
  id: +uuid
  name: string
  posts: [Post]
}

Post {
  id: +uuid
  title: string
  views: ^u64
  published: bool
  author: *User
  created_at: +timestamp
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make list-wire-test`)"]
fn the_bench_mirror_matches_the_router_byte_for_byte() {
    let (out, proj) = common::generate_compile_run("listwire", SCHEMA, DRIVER);
    common::assert_driver_ok(&out, &proj, "the mirrored page bytes diverged from the router");
}

const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use axum::body::Body;
use axum::http::Request;
use forgedb_query_params::{Sort, SortOrder};
use forgedb_types::{Timestamp, Uuid};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const ROWS: usize = 200;
const LIMIT: usize = 25;
/// A `views` value the corpus is guaranteed to contain exactly once.
const PROBE_VIEWS: u64 = 7;

static mut FAILURES: u32 = 0;

fn fail(what: &str, detail: String) {
    println!("  FAIL {what}");
    for line in detail.lines() {
        println!("    {line}");
    }
    unsafe { FAILURES += 1 }
}

fn ok(what: &str) {
    println!("  ok   {what}");
}

/// The envelope's `total` — the match count BEFORE pagination. Sliced out of the raw bytes
/// rather than parsed, for the same reason the comparison above is on bytes: a
/// `serde_json::Value` round-trip normalizes formatting and would hide a divergence.
fn total_of(body: &str) -> usize {
    let after = body.split(",\"total\":").nth(1).expect("envelope has a total");
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

// --- the mirror: what the bench's S1/S2 arms supply by hand ------------------
//
// Each of the three is a hand-written stand-in for something the generated handler
// derives from the query string. The three RED mutations that make this test worth its
// runtime all live here: admit a row the handler rejects, sort the other way, or resolve
// the wrong selection.

/// #288's hoisted predicate: does any parameter name a FILTERABLE field of this model?
/// Positive, not a reserved-key exclusion list — a model may legally declare a field named
/// `limit`, and an exclusion list would short-circuit and return the unfiltered page.
fn keep_all(params: &HashMap<String, String>) -> bool {
    !["id", "title", "views", "published", "created_at"]
        .iter()
        .any(|f| params.contains_key(*f))
}

/// The per-row residual filter. Mirrors the generated `__post_scan_matches`.
fn scan_matches(r: &PostScanRef<'_>, params: &HashMap<String, String>) -> bool {
    if let Some(v) = params.get("views") {
        match v.parse::<u64>() {
            Ok(w) if r.views == w => {}
            // An unparseable value must never MATCH and must never SKIP the row silently
            // for the wrong reason; the generated code falls through to no-match.
            _ => return false,
        }
    }
    if let Some(v) = params.get("published") {
        match v.parse::<bool>() {
            Ok(w) if r.published == w => {}
            _ => return false,
        }
    }
    true
}

/// Descending is `sort_by(..)` then `reverse()`, NOT a flipped comparator: `sort_by` is
/// stable, so reversing also reverses ties. A descending page is not the ascending page
/// read backwards, and the generated code does the two steps.
fn scan_sort(rows: &mut Vec<PostScanRef<'_>>, sort: &Option<Sort>) {
    let Some(s) = sort else { return };
    if s.field != "views" {
        return;
    }
    rows.sort_by(|a, b| a.views.cmp(&b.views));
    if matches!(s.order, SortOrder::Desc) {
        rows.reverse();
    }
}

/// The index pushdown. `views` is the only `^` field, so it is the only key that can
/// resolve a selection.
fn row_selection(db: &Database, params: &HashMap<String, String>) -> Option<Vec<usize>> {
    match params.get("views") {
        Some(v) => db.post.__rows_by_views(v),
        None => None,
    }
}

// --- the four shapes --------------------------------------------------------

struct Shape {
    label: &'static str,
    params: Vec<(&'static str, String)>,
    sort: Option<Sort>,
    query: String,
}

fn shapes() -> Vec<Shape> {
    vec![
        Shape { label: "unfiltered", params: vec![], sort: None, query: String::new() },
        Shape {
            label: "filtered_unindexed",
            params: vec![("published", "true".to_string())],
            sort: None,
            query: "&published=true".to_string(),
        },
        Shape {
            label: "filtered_indexed",
            params: vec![("views", PROBE_VIEWS.to_string())],
            sort: None,
            query: format!("&views={PROBE_VIEWS}"),
        },
        Shape {
            label: "sorted",
            params: vec![],
            sort: Some(Sort::new("views", SortOrder::Desc)),
            // `&sort=views&order=desc`, NOT `&sort=-views`. The leading-minus spelling is
            // NOT parsed by `forgedb-query-params` — it is silently ignored, and the
            // endpoint answers 200 with the UNSORTED page. #282's Gate 2 wrote `?sort=-views`
            // in BDD-1's own scenario text, and the first RED run of this test is what caught
            // it: the mirror sorted, the router did not, and the bytes diverged. A benchmark
            // arm carrying that spelling would have measured the unfiltered page under a
            // "sorted" label and never failed.
            query: "&sort=views&order=desc".to_string(),
        },
    ]
}

/// Build the envelope the way the bench's S2 arm does: the page bytes from the generated
/// page scope with the mirrored trio, wrapped in a locally reconstructed envelope.
fn mirrored_envelope(db: &Database, shape: &Shape) -> String {
    let params: HashMap<String, String> =
        shape.params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let sel = row_selection(db, &params);
    let keep_everything = keep_all(&params);

    // `total` comes FIRST in the terminal closure, and the call returns `R` directly
    // rather than a `Result` — annotate both, so a signature change is a compile error
    // here instead of a silently swapped pair of numbers in the envelope.
    let (total, data) = db.post.__with_page(
        sel,
        |r: &PostScanRef<'_>| keep_everything || scan_matches(r, &params),
        |rows: &mut Vec<PostScanRef<'_>>| scan_sort(rows, &shape.sort),
        0,
        LIMIT,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize page"))
        },
    );

    format!("{{\"data\":{data},\"total\":{total},\"limit\":{LIMIT},\"offset\":0}}")
}

#[tokio::main]
async fn main() {
    let dir = std::env::args().nth(1).expect("data dir as argv[1]");
    let mut db = Database::open_at(std::path::PathBuf::from(&dir));

    // Seed. `views` is `i % 40` so PROBE_VIEWS lands on a handful of rows and `sorted` has
    // ties to reverse; `published` alternates so the unindexed filter drops half.
    let author = Uuid::new_v4();
    db.transaction(|tx| {
        tx.create_user(User { id: author, name: "author".into(), posts: () })?;
        Ok(())
    })
    .expect("seed the author");
    db.transaction(|tx| {
        for i in 0..ROWS {
            tx.create_post(Post {
                id: Uuid::new_v4(),
                title: format!("post {i}"),
                views: (i % 40) as u64,
                published: i % 2 == 0,
                author,
                created_at: Timestamp::from_micros(
                    1_700_000_000_000_000 + i as i64,
                ),
            })?;
        }
        Ok(())
    })
    .expect("seed the posts");

    let state = Arc::new(RwLock::new(db));

    // BDD-1: for each shape, the mirror's bytes and the router's bytes are equal.
    for shape in shapes() {
        let uri = format!("/api/post?limit={LIMIT}&offset=0{}", shape.query);
        let mirrored = {
            let guard = state.read().await;
            mirrored_envelope(&guard, &shape)
        };
        let (status, routed) = call(api::create_router(state.clone()), &uri).await;

        if status != 200 {
            fail(shape.label, format!("router returned {status}\n{routed}"));
            continue;
        }
        if mirrored == routed {
            ok(shape.label);
        } else {
            fail(
                shape.label,
                format!("uri      {uri}\nmirrored {mirrored}\nrouted   {routed}"),
            );
        }
    }

    // BDD-5: the indexed shape's URI is spelled the way the endpoint actually PARSES it.
    // A `?views=` spelling the parser ignores would leave the "filtered_indexed" arm
    // measuring the unfiltered page under a filtered label — and BDD-1 above would stay
    // green, because the mirror would ignore it identically. So it is asserted as a
    // property of the RESULT: strictly fewer rows than unfiltered at the same limit.
    // Compared on `total`, NOT on the number of rows in the page. `published=true` matches
    // 100 of 200 rows, so at `limit=25` both pages hold exactly 25 and a row count cannot
    // tell a working filter from an ignored one. `total` is the count BEFORE pagination,
    // which is the quantity the filter actually moves.
    let (_, unfiltered) =
        call(api::create_router(state.clone()), &format!("/api/post?limit={LIMIT}&offset=0")).await;
    let all_total = total_of(&unfiltered);
    for shape in shapes().into_iter().filter(|s| !s.query.is_empty() && s.label != "sorted") {
        let uri = format!("/api/post?limit={LIMIT}&offset=0{}", shape.query);
        let (_, filtered) = call(api::create_router(state.clone()), &uri).await;
        let some_total = total_of(&filtered);
        if some_total < all_total && some_total > 0 {
            ok(&format!("BDD-5 {} narrows the set ({some_total} < {all_total})", shape.label));
        } else {
            fail(
                &format!("BDD-5 {}", shape.label),
                format!("uri {uri}\ntotal was {some_total}; unfiltered total was {all_total}"),
            );
        }
    }

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} check(s) failed");
        std::process::exit(1);
    }
    println!("all checks passed");
}
"##;
