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

fn keep_all(params: &HashMap<String, String>) -> bool {
    !["id", "title", "views", "published", "created_at"]
        .iter()
        .any(|f| params.contains_key(*f))
}

fn scan_matches(r: &PostScanRef<'_>, params: &HashMap<String, String>) -> bool {
    if let Some(v) = params.get("views") {
        match v.parse::<u64>() {
            Ok(w) if r.views == w => {}
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

fn row_selection(db: &Database, params: &HashMap<String, String>) -> Option<Vec<usize>> {
    match params.get("views") {
        Some(v) => db.post.__rows_by_views(v),
        None => None,
    }
}

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
            query: "&sort=views&order=desc".to_string(),
        },
    ]
}

fn mirrored_envelope(db: &Database, shape: &Shape) -> String {
    let params: HashMap<String, String> =
        shape.params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let sel = row_selection(db, &params);
    let keep_everything = keep_all(&params);

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
