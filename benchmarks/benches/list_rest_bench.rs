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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Unfiltered,
    FilteredUnindexed,
    FilteredIndexed,
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

fn keep_all(params: &HashMap<String, String>) -> bool {
    !["id", "title", "views", "published", "created_at"]
        .iter()
        .any(|f| params.contains_key(*f))
}

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

fn row_selection(db: &Database, params: &HashMap<String, String>) -> Option<Vec<usize>> {
    match params.get("views") {
        Some(v) => db.post.__rows_by_views(v),
        None => None,
    }
}

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

fn s1_rows_fast(db: &Database, offset: usize, limit: usize) -> (usize, u64) {
    db.post.__with_fast_page(offset, limit, |total: usize, page: &[PostPageRef<'_>]| {
        (total, page_fold(page))
    })
}

fn s2_json_fast(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.post.__with_fast_page(
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

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

fn envelope_total(body: &str) -> usize {
    let key = r#""total":"#;
    let at = body.find(key).expect("envelope carries `total`") + key.len();
    let rest = &body[at..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().expect("numeric total")
}

fn envelope_data(body: &str) -> &str {
    let head = r#"{"data":"#;
    let tail = r#","total":"#;
    let start = body.find(head).expect("envelope starts with `data`") + head.len();
    let end = body.find(tail).expect("envelope carries `total` after `data`");
    &body[start..end]
}

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

    let page_len = total.saturating_sub(offset).min(limit);
    if page_len > 0 {
        assert_ne!(
            fold, 0,
            "{}: the S1 fold returned 0 over {page_len} rows -- if the fold is dead code \
             the S1 arm is not measuring the page view's construction",
            shape.label()
        );
    }

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

    assert!(
        row_selection(&guard, &Shape::Unfiltered.params()).is_none(),
        "BDD-4: the unfiltered shape resolved an index selection; #281's fast path sits \
         ABOVE the pushdown binding precisely because a request naming no filterable \
         field resolves no index"
    );
}

fn register_unfiltered(g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>, fx: &Fixture, limit: usize) {
    let shape = Shape::Unfiltered;
    let label = format!("{}/rows={}/limit={limit}", shape.label(), fx.rows);

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

    let uri = format!("/api/post?limit={limit}");
    g.bench_with_input(BenchmarkId::new("s3_router", &label), &uri, |b, u| {
        b.iter(|| fx.get(u));
    });
    if limit == CORE_LIMIT {
        g.bench_with_input(
            BenchmarkId::new("s3_router_noparams", &label),
            "/api/post",
            |b, u| b.iter(|| fx.get(u)),
        );
    }
}

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

        if shape == Shape::Unfiltered {
            g.bench_function(BenchmarkId::new("s1_rows_fast", &label), |b| {
                let guard = fx.rt.block_on(fx.state.read());
                b.iter(|| s1_rows_fast(&guard, 0, CORE_LIMIT));
            });
            g.bench_function(BenchmarkId::new("s2_json_fast", &label), |b| {
                let guard = fx.rt.block_on(fx.state.read());
                b.iter(|| s2_json_fast(&guard, 0, CORE_LIMIT));
            });
            g.bench_with_input(
                BenchmarkId::new("s3_router_noparams", &label),
                "/api/post",
                |b, u| b.iter(|| fx.get(u)),
            );
        }

        let uri = shape.uri(0, CORE_LIMIT);
        g.bench_with_input(BenchmarkId::new("s3_router", &label), &uri, |b, u| {
            b.iter(|| fx.get(u));
        });
    }
    g.finish();
}

fn bench_size_sweep(c: &mut Criterion) {
    let mut g = c.benchmark_group("forgedb/list_unfiltered");
    for rows in SIZES {
        let fx = fixture(rows);
        verify_shape(&fx, Shape::Unfiltered, 0, CORE_LIMIT);
        register_unfiltered(&mut g, &fx, CORE_LIMIT);
    }
    g.finish();
}

fn bench_limit_sweep(c: &mut Criterion) {
    let fx = fixture(CORE_ROWS);
    let mut g = c.benchmark_group("forgedb/list_unfiltered_limits");
    for limit in LIMITS.into_iter().filter(|l| *l != CORE_LIMIT) {
        verify_shape(&fx, Shape::Unfiltered, 0, limit);
        register_unfiltered(&mut g, &fx, limit);
    }
    g.finish();
}

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
                .header(hyper::header::HOST, "localhost")
                .body(http_body_util::Empty::<hyper::body::Bytes>::new())
                .expect("build request");
            self.sender.ready().await.expect("connection ready");
            let resp = self.sender.send_request(req).await.expect("socket request");
            assert_eq!(resp.status().as_u16(), 200, "S4 status for {uri}");
            resp.into_body().collect().await.expect("collect").to_bytes()
        })
    }
}

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

    let accepted = arm.accepts.load(Ordering::Relaxed);
    assert_eq!(
        accepted, 1,
        "BDD-6: S4 must reuse ONE keep-alive connection -- {accepted} were accepted, so \
         connection setup is being billed to every request and every S4 number is inflated"
    );
}

criterion_group!(benches, bench_core, bench_size_sweep, bench_limit_sweep, bench_socket);
criterion_main!(benches);
