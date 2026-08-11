//! List-path selection guard (#228): the **ids and `total`** the generated list
//! endpoint returns, checked against an independently computed oracle, by running
//! the real generated router over a churned corpus.
//!
//! # Why this test exists in this form
//!
//! #228 turned the narrow scan into a *scope*: the filter, the sort, the count and
//! the pagination now all run inside `__with_scan`, on borrowed views over the
//! buffered columns, and only `(total, Vec<Id>)` escapes. Nothing about the
//! selection is supposed to change — but every step of it moved.
//!
//! A codegen snapshot cannot catch a regression here. It compares emitted
//! *strings*; ordering, tie behaviour, and which rows survive a filter are
//! properties of what the emitted code *does*. The companion
//! `tests/api_wire_test.rs` freezes the response bytes for a two-row corpus, which
//! pins the envelope but says nothing about selection over a realistic table.
//!
//! So this test computes the expected `(ids, total)` in the driver, from a plain
//! `Vec` mirroring the rows it inserted, and compares. The oracle is a genuine
//! second implementation — not a re-derivation from the same code — which is the
//! only way a "these must be identical" claim means anything.
//!
//! # What the oracle deliberately replicates
//!
//! Three behaviours that are easy to break and easy to get subtly wrong:
//!
//! 1. **Pre-sort order is physical row order.** The scan walks `id_to_row`'s values
//!    sorted ascending, and an update *appends* a new version (append-only storage,
//!    #66), so an updated row moves to the tail. With no `?sort=` that order IS the
//!    response order, and with a `?sort=` it decides ties.
//! 2. **Descending is `sort_by(..)` then `reverse()`**, not a reversed comparator.
//!    `sort_by` is stable, so reversing also reverses ties — a descending page is
//!    not the ascending page read backwards field-by-field. The oracle does the same
//!    two steps rather than sorting with a flipped comparator, because that is what
//!    the generated code does and the difference is observable.
//! 3. **A nullable sort key orders `None` first** (`Option`'s derived `Ord`), and
//!    the borrowed view sorts `Option<&str>` where the owned one sorted
//!    `Option<String>`. Same order, different type — pinned because #228 is exactly
//!    the change that swapped them.
//!
//! # Coverage
//!
//! The corpus is churned before any query runs: rows are updated (dead versions
//! inside the mapped span, live rows moved to the tail) and deleted (holes in the
//! gathered selection, plus index entries removed). Cases cover the full scan, each
//! index-pushdown arm, the pushdown-plus-residual-filter combination, the
//! unparseable-value fallback that must never miss a match, and pagination past the
//! end.
//!
//! # #288 — the hoisted unfiltered predicate
//!
//! #288 moved the "is there any filter at all?" question out of the per-row loop.
//! It is provably a pure evaluation-order change, so the whole case table above is
//! already equivalence coverage for it: several cases carry non-filter keys with no
//! filter (the arm that now short-circuits) and the rest take the fall-through.
//!
//! Three properties are structurally beyond the oracle, and are checked separately:
//!
//! - **H2** — `Gauge` declares a field literally named `limit`, so `?limit=3` is a
//!   real filter for that model. This is what forces the predicate to be positive
//!   ("does any key name a filterable field?") rather than a reserved-key exclusion
//!   list, which would short-circuit here and quietly return unfiltered rows.
//! - **H4** — `/api/post` and `/api/post?limit=50` must be byte-identical. Compared
//!   as bytes, not as `(ids, total)`, so an envelope divergence cannot slip through.
//! - **H5** — `Link` has zero filterable fields, so its predicate takes no checks
//!   and its parameter must be `_params`. That defect compiles and behaves
//!   correctly; it is visible only as a *warning* in the user's crate, so the test
//!   reads build diagnostics via `common::build_warnings`.
//!
//! It compiles a generated crate, so it is `#[ignore]`d out of the fast hermetic
//! default suite. Run it explicitly:
//!
//! ```bash
//! make list-scan-test        # or:
//! cargo test --test list_scan_test -- --ignored --nocapture
//! ```

mod common;

/// `title` is unique-indexed, `views` and `status` are indexed (so all three drive
/// the pushdown arm), `body` is not indexed (so it forces the full scan), and
/// `summary` is a nullable string (the sort key with `None`s in it).
const SCHEMA: &str = r#"
enum Status { Draft, Published, Archived }

Post {
  id: +uuid
  title: &string
  body: string
  summary: string?
  views: ^u32
  status: ^Status
  author: *Author
}

Author {
  id: +uuid
  name: string
  posts: [Post]
}

// #288. `limit`/`offset`/`sort`/`order` are NOT lexer keywords — the keyword set is
// exactly `struct` and `enum` — so a model may legally declare fields with those
// names, and for THIS model `?limit=3` is a genuine filter. It is the case that
// decides the shape of the hoisted predicate: a negative exclusion list of reserved
// query keys would short-circuit here and silently return unfiltered rows.
//
// `limit` MUST NOT be indexed. An indexed field is resolved by the pushdown arm
// before the keep closure ever runs, so the pushdown would produce the right answer
// on its own and mask a broken predicate entirely — the guard would pass vacuously.
// Leaving it unindexed forces the filter to run as a per-row residual, which is the
// path #288 changes. (Verified: with `^u32` here, the exclusion-list mutation stays
// GREEN; without it, the mutation goes RED.)
Gauge {
  id: +u32
  limit: u32
  offset: u32
  sort: string
  order: string
}

// #288. `Link` has ZERO filterable fields: its identity is a required FK (a
// relation, not filterable) and `meta` is json (no total order). Its predicate is
// therefore body-less, and its parameter is unused — which is a WARNING in the
// user's crate, not an error, and `database.rs`'s blanket allow header does not
// (and should not) cover `api.rs`.
Doc {
  id: +uuid
  title: string
  links: [Link]
}

Link {
  id: *Doc
  meta: json
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make list-scan-test`)"]
fn list_selection_matches_an_independent_oracle() {
    let (out, proj) = common::generate_compile_run("scandriver", SCHEMA, DRIVER);
    // Captured before `assert_driver_ok`, which removes the project directory.
    let warnings = common::build_warnings(&proj);
    common::assert_driver_ok(&out, &proj, "driver reported a list-selection mismatch");

    // #288 H5. The `Link` model has no filterable fields, so its hoisted predicate
    // takes no checks and its parameter must be named `_params`. Getting that wrong
    // compiles and behaves correctly — it is observable ONLY as a warning in the
    // user's crate, which is why this assertion reads build diagnostics rather than
    // any value the driver produced.
    assert!(
        !warnings.contains("_is_unfiltered"),
        "#288: the generated unfiltered-predicate helper warns in the user's crate \
         (a zero-filterable model's parameter must be `_params`):\n{warnings}"
    );
}

const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use axum::body::Body;
use axum::http::Request;
use forgedb_types::Uuid;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const AUTHOR_ID: &str = "11111111-1111-1111-1111-111111111111";

static mut FAILURES: u32 = 0;

/// The oracle's mirror of one live row. `status_ord` is the enum's DECLARATION
/// index, which is what the generated `Ord` derive compares — not the name.
#[derive(Clone, Debug)]
struct R {
    id: String,
    title: String,
    body: String,
    summary: Option<String>,
    views: u32,
    status_ord: u8,
    status_name: &'static str,
}

fn status_of(ord: u8) -> Status {
    match ord {
        0 => Status::Draft,
        1 => Status::Published,
        _ => Status::Archived,
    }
}

fn status_name(ord: u8) -> &'static str {
    match ord {
        0 => "Draft",
        1 => "Published",
        _ => "Archived",
    }
}

/// The live set in PHYSICAL ROW ORDER — the order the scan yields before any sort.
/// `upsert` models append-only storage: an updated row's new version lands at the
/// tail, so it moves to the end here too.
struct Live(Vec<R>);

impl Live {
    fn insert(&mut self, r: R) {
        self.0.push(r);
    }
    fn update(&mut self, id: &str, f: impl FnOnce(&mut R)) {
        let pos = self.0.iter().position(|r| r.id == id).expect("update: id live");
        let mut r = self.0.remove(pos);
        f(&mut r);
        self.0.push(r); // the new version is APPENDED — it becomes the last row
    }
    fn delete(&mut self, id: &str) {
        let pos = self.0.iter().position(|r| r.id == id).expect("delete: id live");
        self.0.remove(pos);
    }

    /// Recompute what `GET /api/post?<query>` must return: the page's ids in order,
    /// and `total` (the filtered count BEFORE pagination).
    fn expect(
        &self,
        filters: &[(&str, &str)],
        sort: Option<(&str, bool)>,
        limit: Option<usize>,
        offset: usize,
    ) -> (Vec<String>, usize) {
        // Closed-set filter: every named param must match, parsing the raw string
        // into the field's type first. A value that does not parse matches nothing
        // (it can never equal a stored value) — which is also why an unparseable
        // value on an INDEXED field must fall back to the full scan rather than
        // silently returning the empty index bucket.
        let mut rows: Vec<&R> = self
            .0
            .iter()
            .filter(|r| {
                filters.iter().all(|(k, v)| match *k {
                    "id" => r.id == *v,
                    "title" => r.title == *v,
                    "body" => r.body == *v,
                    "summary" => r.summary.as_deref() == Some(*v),
                    "views" => v.parse::<u32>().map(|n| n == r.views).unwrap_or(false),
                    "status" => r.status_name == *v,
                    other => panic!("oracle has no rule for param {other}"),
                })
            })
            .collect();

        // Stable sort ascending, THEN reverse for descending — the two steps the
        // generated helper performs, tie behaviour included.
        if let Some((field, desc)) = sort {
            match field {
                "id" => rows.sort_by(|a, b| a.id.cmp(&b.id)),
                "title" => rows.sort_by(|a, b| a.title.cmp(&b.title)),
                "body" => rows.sort_by(|a, b| a.body.cmp(&b.body)),
                "summary" => rows.sort_by(|a, b| a.summary.cmp(&b.summary)),
                "views" => rows.sort_by(|a, b| a.views.cmp(&b.views)),
                "status" => rows.sort_by(|a, b| a.status_ord.cmp(&b.status_ord)),
                // An unknown sort field returns early in the generated helper,
                // leaving physical order untouched.
                _ => {}
            }
            if desc {
                rows.reverse();
            }
        }

        let total = rows.len();
        let limit = limit.unwrap_or(50).clamp(1, 1000);
        let start = offset.min(total);
        let end = offset.saturating_add(limit).min(total);
        (rows[start..end].iter().map(|r| r.id.clone()).collect(), total)
    }
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

/// Pull `(ids, total)` back out of the response envelope.
fn observed(body: &str) -> (Vec<String>, usize) {
    let v: serde_json::Value = serde_json::from_str(body).expect("envelope is json");
    let ids = v["data"]
        .as_array()
        .expect("data array")
        .iter()
        .map(|r| r["id"].as_str().expect("id string").to_string())
        .collect();
    (ids, v["total"].as_u64().expect("total") as usize)
}

fn uuid(s: &str) -> Uuid {
    s.parse().expect("parse uuid")
}

/// Deterministic id for post `n` — the ids are compared as strings, so they have
/// to be stable across runs.
fn post_id(n: usize) -> String {
    format!("22222222-2222-2222-2222-{:012}", n)
}

#[tokio::main]
async fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut db = Database::open_at(dir.clone());
    let mut live = Live(Vec::new());

    db.create_author(Author {
        id: uuid(AUTHOR_ID),
        name: "Ada".to_string(),
        posts: (),
    })
    .expect("create author");

    // 12 posts. `views` repeats deliberately (so an indexed pushdown resolves
    // MULTIPLE candidate rows, not one), `summary` is absent on every third row
    // (so the nullable sort has `None`s to order), and `body` groups rows into
    // buckets a non-indexed filter can select.
    for n in 0..12usize {
        let id = post_id(n);
        let views = (n % 4) as u32 * 10;
        let status_ord = (n % 3) as u8;
        let summary = if n % 3 == 0 {
            None
        } else {
            Some(format!("summary-{n:02}"))
        };
        let body = format!("bucket-{}", n % 2);
        let title = format!("title-{n:02}");
        db.create_post(Post {
            id: uuid(&id),
            title: title.clone(),
            body: body.clone(),
            summary: summary.clone(),
            views,
            status: status_of(status_ord),
            author: uuid(AUTHOR_ID),
        })
        .expect("create post");
        live.insert(R {
            id,
            title,
            body,
            summary,
            views,
            status_ord,
            status_name: status_name(status_ord),
        });
    }

    // Churn. Updates leave DEAD VERSIONS inside the mapped data span and move the
    // live row to the tail; deletes punch HOLES in the gathered selection and
    // remove the ids from every secondary index (which is why the pushdown arm can
    // skip the tombstone read).
    for n in [1usize, 5, 9] {
        let id = post_id(n);
        let mut updated = live.0.iter().find(|r| r.id == id).expect("live").clone();
        updated.views += 100;
        updated.summary = Some(format!("revised-{n:02}"));
        updated.title = format!("title-{n:02}-v2");
        db.update_post(
            uuid(&id),
            Post {
                id: uuid(&id),
                title: updated.title.clone(),
                body: updated.body.clone(),
                summary: updated.summary.clone(),
                views: updated.views,
                status: status_of(updated.status_ord),
                author: uuid(AUTHOR_ID),
            },
        )
        .expect("update post");
        live.update(&id, |r| {
            r.views = updated.views;
            r.summary = updated.summary.clone();
            r.title = updated.title.clone();
        });
    }
    for n in [3usize, 7] {
        let id = post_id(n);
        db.delete_post(uuid(&id)).expect("delete post");
        live.delete(&id);
    }

    // #288 H2. Four gauges whose `limit` FIELD takes four distinct values. Exactly
    // one holds 3, so `?limit=3` against this model must return that single row —
    // and must NOT be read as "page size 3", which would return three rows.
    for n in 0..4u32 {
        db.create_gauge(Gauge {
            id: 0, // `0` is the allocate sentinel for a `+u32` identity
            limit: n,
            offset: n * 10,
            sort: format!("s-{n}"),
            order: format!("o-{n}"),
        })
        .expect("create gauge");
    }

    db.commit().expect("commit");

    let db = Arc::new(RwLock::new(db));
    let router = || api::create_router(db.clone());

    // (label, uri, filters, sort, limit, offset)
    type Case = (
        &'static str,
        String,
        Vec<(&'static str, &'static str)>,
        Option<(&'static str, bool)>,
        Option<usize>,
        usize,
    );

    let live_title_1 = live.0.iter().find(|r| r.id == post_id(1)).unwrap().title.clone();
    let title_1: &'static str = Box::leak(live_title_1.into_boxed_str());

    let cases: Vec<Case> = vec![
        // --- full scan, no filter -------------------------------------------------
        (
            "unsorted — response order IS physical row order, updated rows at the tail",
            "/api/post".to_string(),
            vec![],
            None,
            None,
            0,
        ),
        (
            "sort=views asc",
            "/api/post?sort=views".to_string(),
            vec![],
            Some(("views", false)),
            None,
            0,
        ),
        (
            "sort=views desc — stable sort then reverse, so ties reverse too",
            "/api/post?sort=views&order=desc".to_string(),
            vec![],
            Some(("views", true)),
            None,
            0,
        ),
        (
            "sort=summary asc — nullable key, None orders first",
            "/api/post?sort=summary".to_string(),
            vec![],
            Some(("summary", false)),
            None,
            0,
        ),
        (
            "sort=summary desc — None orders last",
            "/api/post?sort=summary&order=desc".to_string(),
            vec![],
            Some(("summary", true)),
            None,
            0,
        ),
        (
            "sort=status — enum orders by DECLARATION index, not variant name",
            "/api/post?sort=status".to_string(),
            vec![],
            Some(("status", false)),
            None,
            0,
        ),
        (
            "sort=title with a page window",
            "/api/post?sort=title&limit=3&offset=2".to_string(),
            vec![],
            Some(("title", false)),
            Some(3),
            2,
        ),
        (
            "unknown sort field falls through to physical order",
            "/api/post?sort=nosuchfield".to_string(),
            vec![],
            Some(("nosuchfield", false)),
            None,
            0,
        ),
        // --- index pushdown -------------------------------------------------------
        (
            "pushdown on the indexed enum, sorted",
            "/api/post?status=Published&sort=views".to_string(),
            vec![("status", "Published")],
            Some(("views", false)),
            None,
            0,
        ),
        (
            "pushdown on the indexed u32 — several candidate rows",
            "/api/post?views=20&sort=title".to_string(),
            vec![("views", "20")],
            Some(("title", false)),
            None,
            0,
        ),
        (
            "pushdown on the UNIQUE-indexed string — one candidate, an updated row",
            format!("/api/post?title={title_1}"),
            vec![("title", title_1)],
            None,
            None,
            0,
        ),
        (
            "pushdown + residual filter on a non-indexed field",
            "/api/post?status=Draft&body=bucket-0&sort=views".to_string(),
            vec![("status", "Draft"), ("body", "bucket-0")],
            Some(("views", false)),
            None,
            0,
        ),
        (
            "pushdown that hits no index bucket",
            "/api/post?views=999999".to_string(),
            vec![("views", "999999")],
            None,
            None,
            0,
        ),
        // --- fallbacks ------------------------------------------------------------
        (
            "unparseable value on an INDEXED field falls back to the full scan",
            "/api/post?views=not-a-number".to_string(),
            vec![("views", "not-a-number")],
            None,
            None,
            0,
        ),
        (
            "unparseable enum on an INDEXED field falls back to the full scan",
            "/api/post?status=Nonsense".to_string(),
            vec![("status", "Nonsense")],
            None,
            None,
            0,
        ),
        (
            "filter on a NON-indexed field — full scan, no pushdown available",
            "/api/post?body=bucket-1&sort=views".to_string(),
            vec![("body", "bucket-1")],
            Some(("views", false)),
            None,
            0,
        ),
        (
            "filter matching a row that was DELETED — must not resurrect it",
            format!("/api/post?title={}", "title-03"),
            vec![("title", "title-03")],
            None,
            None,
            0,
        ),
        (
            "filter on the PRE-UPDATE title — the dead version must not match",
            format!("/api/post?title={}", "title-01"),
            vec![("title", "title-01")],
            None,
            None,
            0,
        ),
        // --- pagination edges -----------------------------------------------------
        (
            "offset past the end — empty page, total unchanged",
            "/api/post?sort=views&limit=5&offset=100".to_string(),
            vec![],
            Some(("views", false)),
            Some(5),
            100,
        ),
        (
            "limit larger than the live set",
            "/api/post?sort=views&limit=500".to_string(),
            vec![],
            Some(("views", false)),
            Some(500),
            0,
        ),
    ];

    for (label, uri, filters, sort, limit, offset) in &cases {
        let (status, body) = call(router(), uri).await;
        if status != 200 {
            println!("  FAIL {label}\n    {uri} -> {status} {body}");
            unsafe { FAILURES += 1 }
            continue;
        }
        let got = observed(&body);
        let want = live.expect(filters, *sort, *limit, *offset);
        if got == want {
            println!("  ok   {label}  (total={}, page={})", want.1, want.0.len());
        } else {
            println!("  FAIL {label}");
            println!("    uri  {uri}");
            println!("    want total={} ids={:?}", want.1, want.0);
            println!("    got  total={} ids={:?}", got.1, got.0);
            unsafe { FAILURES += 1 }
        }
    }

    // ---- #288: the hoisted unfiltered predicate ----------------------------------
    //
    // The cases above already cover the hoist's equivalence for free — several carry
    // non-filter keys (`?sort=`, `?limit=`) with no filter, which is exactly the arm
    // that now short-circuits, and the filtered ones take the fall-through. The two
    // checks below cover what the oracle structurally cannot.

    // H4. `/api/post` and `/api/post?limit=50` differ ONLY in whether the params map
    // is empty (50 is the default page size), so they must be byte-identical. That
    // pair is the whole reason this issue exists: pre-hoist they returned the same
    // bytes for 1.63x/2.44x different cost. Comparing bytes rather than (ids, total)
    // also catches an envelope divergence the oracle would not see.
    {
        let (s_a, b_a) = call(router(), "/api/post").await;
        let (s_b, b_b) = call(router(), "/api/post?limit=50").await;
        if s_a == 200 && s_b == 200 && b_a == b_b {
            println!("  ok   #288 H4 — /api/post == /api/post?limit=50, byte for byte");
        } else {
            println!("  FAIL #288 H4 — statuses {s_a}/{s_b}; bodies differ");
            println!("    without  {b_a}");
            println!("    with     {b_b}");
            unsafe { FAILURES += 1 }
        }
    }

    // H2. The decisive case for the predicate's SHAPE. `Gauge` declares a field
    // literally named `limit`, so `?limit=3` names a filterable field of this model
    // and must filter to the one row holding 3. A predicate built as a negative
    // exclusion list of reserved query keys would short-circuit here and return all
    // four rows — behaviourally wrong, and wrong in the silent direction.
    {
        let (status, body) = call(router(), "/api/gauge?limit=3").await;
        // Parsed inline rather than via `observed`, which reads `id` as a string for
        // the uuid-keyed `Post`; `Gauge`'s identity is a `+u32` and serializes as a
        // JSON number.
        let v: serde_json::Value = serde_json::from_str(&body).expect("envelope is json");
        let rows = v["data"].as_array().map(|a| a.len()).unwrap_or(0);
        let total = v["total"].as_u64().unwrap_or(u64::MAX);
        let got_limit = v["data"][0]["limit"].as_u64();
        if status == 200 && rows == 1 && total == 1 && got_limit == Some(3) {
            println!("  ok   #288 H2 — ?limit=3 filters a model whose FIELD is `limit`");
        } else {
            println!("  FAIL #288 H2 — expected exactly 1 row (total=1, limit=3), got {rows} row(s) total={total} limit={got_limit:?}");
            println!("    {status} {body}");
            unsafe { FAILURES += 1 }
        }
    }

    // The whole point of the corpus is that it is churned; if the churn silently
    // stopped happening the cases above would still pass against a smaller table.
    assert_eq!(live.0.len(), 10, "corpus should be 12 created - 2 deleted");

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} list-selection mismatch(es)");
        std::process::exit(1);
    }
    println!("all {} list-selection checks passed", cases.len());
}
"##;
