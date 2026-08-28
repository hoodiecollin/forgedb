mod common;

const SCHEMA: &str = r#"
enum Status { Draft, Published, Archived }

Post {
  @projection(card: title, status)
  id: +uuid
  title: &string
  body: string
  summary: string?
  views: ^u32
  rating: f64
  status: ^Status
  meta: json
  author: *Author
  editor: ?Author
}

Author {
  id: +uuid
  name: ^string
  posts: [Post]
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make api-wire-test`)"]
fn rest_read_paths_emit_the_frozen_wire_bytes() {
    let (out, proj) = common::generate_compile_run("wiredriver", SCHEMA, DRIVER);
    common::assert_driver_ok(&out, &proj, "driver reported a wire-format mismatch");
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
const POST_A: &str = "22222222-2222-2222-2222-222222222222";
const POST_B: &str = "33333333-3333-3333-3333-333333333333";
const MISSING: &str = "44444444-4444-4444-4444-444444444444";

static mut FAILURES: u32 = 0;

fn check(uri: &str, got: (u16, String), want_status: u16, want_body: &str) {
    let (status, body) = got;
    if status == want_status && body == want_body {
        println!("  ok   {status} {uri}");
        return;
    }
    println!("  FAIL {uri}");
    println!("    want {want_status}  {want_body}");
    println!("    got  {status}  {body}");
    unsafe { FAILURES += 1 }
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

fn uuid(s: &str) -> Uuid {
    s.parse().expect("parse uuid")
}

#[tokio::main]
async fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut db = Database::open_at(dir.clone());

    db.create_author(Author {
        id: uuid(AUTHOR_ID),
        name: "Ada".to_string(),
        posts: (),
    })
    .expect("create author");

    // Row A exercises the escape-sensitive string, a null optional, and a json
    // object whose OWN keys are out of declaration order.
    db.create_post(Post {
        id: uuid(POST_A),
        title: "quote \" backslash \\ tab \t unicode ✓".to_string(),
        body: String::new(),
        summary: None,
        views: 0,
        rating: 1.0,
        status: Status::Draft,
        meta: serde_json::json!({ "z": 1, "a": 2 }),
        author: uuid(AUTHOR_ID),
        editor: None,
    })
    .expect("create post a");

    // Row B exercises the populated optional, a set optional FK, and a float that
    // only the JSON number formatter renders this way.
    db.create_post(Post {
        id: uuid(POST_B),
        title: "second".to_string(),
        body: "b".to_string(),
        summary: Some("here".to_string()),
        views: 7,
        rating: 1e300,
        status: Status::Archived,
        meta: serde_json::Value::Null,
        author: uuid(AUTHOR_ID),
        editor: Some(uuid(AUTHOR_ID)),
    })
    .expect("create post b");

    db.commit().expect("commit");

    let db = Arc::new(RwLock::new(db));
    let router = || api::create_router(db.clone());

    // Records are emitted in SCHEMA DECLARATION ORDER (id first), not the
    // alphabetical order the intermediate `serde_json::Value` used to impose.
    // `meta`'s own keys stay sorted — that payload is a `Value` either way.
    let row_a = concat!(
        r#"{"id":"22222222-2222-2222-2222-222222222222","#,
        r#""title":"quote \" backslash \\ tab \t unicode ✓","#,
        r#""body":"","summary":null,"views":0,"rating":1.0,"status":"Draft","#,
        r#""meta":{"a":2,"z":1},"#,
        r#""author":"11111111-1111-1111-1111-111111111111","editor":null}"#
    );
    let row_b = concat!(
        r#"{"id":"33333333-3333-3333-3333-333333333333","#,
        r#""title":"second","body":"b","summary":"here","views":7,"rating":1e+300,"#,
        r#""status":"Archived","meta":null,"#,
        r#""author":"11111111-1111-1111-1111-111111111111","#,
        r#""editor":"11111111-1111-1111-1111-111111111111"}"#
    );

    // Envelope key order is `data`, `total`, `limit`, `offset` — declaration
    // order of the generated `__ListEnvelope`, not alphabetical.
    let uri = "/api/post?sort=views";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{row_a},{row_b}],"total":2,"limit":50,"offset":0}}"#),
    );

    let uri = "/api/post?sort=views&limit=1&offset=1";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{row_b}],"total":2,"limit":1,"offset":1}}"#),
    );

    let uri = "/api/post?views=999";
    check(
        uri,
        call(router(), uri).await,
        200,
        r#"{"data":[],"total":0,"limit":50,"offset":0}"#,
    );

    // The projection list arm builds a typed `Vec` and borrows it into the SAME
    // envelope — same key order, projected columns in declaration order.
    let uri = "/api/post?sort=views&projection=card";
    check(
        uri,
        call(router(), uri).await,
        200,
        concat!(
            r#"{"data":[{"id":"22222222-2222-2222-2222-222222222222","#,
            r#""title":"quote \" backslash \\ tab \t unicode ✓","status":"Draft"},"#,
            r#"{"id":"33333333-3333-3333-3333-333333333333","#,
            r#""title":"second","status":"Archived"}],"#,
            r#""total":2,"limit":50,"offset":0}"#
        ),
    );

    // Point reads: the record alone, same declaration order as inside the page.
    let uri = &format!("/api/post/{POST_A}");
    check(uri, call(router(), uri).await, 200, row_a);

    let uri = &format!("/api/post/{POST_B}");
    check(uri, call(router(), uri).await, 200, row_b);

    let uri = &format!("/api/post/{POST_A}?projection=card");
    check(
        uri,
        call(router(), uri).await,
        200,
        concat!(
            r#"{"id":"22222222-2222-2222-2222-222222222222","#,
            r#""title":"quote \" backslash \\ tab \t unicode ✓","status":"Draft"}"#
        ),
    );

    // Error bodies deliberately stayed ad-hoc `json!` objects (#229 non-goal).
    let uri = &format!("/api/post/{MISSING}");
    check(uri, call(router(), uri).await, 404, r#"{"error":"not found"}"#);

    let uri = "/api/post/not-a-uuid";
    check(uri, call(router(), uri).await, 400, r#"{"error":"invalid id"}"#);

    let uri = "/api/post?projection=nope";
    check(
        uri,
        call(router(), uri).await,
        400,
        r#"{"error":"unknown projection"}"#,
    );

    let uri = "/api/post/not-a-uuid?projection=card";
    check(uri, call(router(), uri).await, 400, r#"{"error":"invalid id"}"#);

    let uri = "/api/post?as_of=abc";
    check(
        uri,
        call(router(), uri).await,
        400,
        r#"{"error":"as_of must be a non-negative integer watermark"}"#,
    );

    // The `?as_of=` snapshot path is a separate branch of the same handler and
    // must reach the same envelope.
    let uri = "/api/post?as_of=2&sort=views";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{row_a},{row_b}],"total":2,"limit":50,"offset":0}}"#),
    );

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} wire-format mismatch(es)");
        std::process::exit(1);
    }
    println!("all REST wire-format checks passed");
}
"##;

const PAGE_SCHEMA: &str = r#"
enum Tier { Free, Pro, Enterprise }

struct Dims {
  w: u32
  h: u32
}

Widget {
  id: +uuid
  label: string?
  price: decimal
  tier: ^Tier
  made_at: timestamp(ms)
  checksum: bytes(4)
  scores: [i32; 3]
  dims: Dims
  owner: *Maker
  parts: [Part]
  payload: json
  serial: ^u32
}

Maker {
  id: +uuid
  name: string
  widgets: [Widget]
}

Part {
  id: +uuid
  name: string
  widget: *Widget
}

Note {
  body: string
  id: +uuid
  weight: ^u32
}

Churn {
  id: +uuid
  note: string
  seq: ^u32
}

// #281 (S9). `Item.vendor` is the ONE field class whose `borrowed` flag genuinely
// flips between the two page construction sites: it is non-scan (a relation is never
// filterable, so it is not in the scan set) AND inline-string, so the read path moves
// between the owned and the borrowed arm of `field_read_stmt`. Nothing else in this
// schema has that shape — `Widget.owner` is a uuid-keyed FK.
//
// A frozen literal here, in addition to `page_identity_test`'s site-vs-site grid,
// because the two catch different failures: the grid catches a change that moves ONE
// site, a literal catches one that moves BOTH.
Vendor {
  id: string(4!)
  name: string
  items: [Item]
}

Item {
  id: +uuid
  sku: string
  vendor: *Vendor
}

// #281 (S11). `limit`/`offset`/`sort`/`order` are not lexer keywords, so a model may
// legally declare them as fields — and for THIS model `?limit=1` really is a filter.
// It is the single case where the positive predicate ("does any key name a filterable
// field of this model?") and a negative reserved-key exclusion list visibly differ:
// under the negative form the fast path fires and returns the unfiltered page.
//
// None of them is indexed, deliberately. An indexed field resolves through the
// pushdown arm before any per-row work, which would produce the right answer on its
// own and mask a broken predicate entirely.
Gauge {
  id: +uuid
  limit: u32
  offset: u32
  sort: string
  order: string
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make api-wire-test`)"]
fn list_page_emits_the_frozen_wire_bytes() {
    let (out, proj) = common::generate_compile_run("wirepage", PAGE_SCHEMA, PAGE_DRIVER);
    common::assert_driver_ok(&out, &proj, "driver reported a list-page wire mismatch");
}

const PAGE_DRIVER: &str = r##"mod database;
use database::*;

mod api;

use axum::body::Body;
use axum::http::Request;
use forgedb_types::{Timestamp, Uuid};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const MAKER: &str = "11111111-1111-1111-1111-111111111111";
const WIDGET_A: &str = "22222222-2222-2222-2222-222222222222";
const WIDGET_B: &str = "33333333-3333-3333-3333-333333333333";
const PART: &str = "44444444-4444-4444-4444-444444444444";
const NOTE_A: &str = "55555555-5555-5555-5555-555555555555";
const NOTE_B: &str = "66666666-6666-6666-6666-666666666666";
const CHURN_0: &str = "77777777-0000-0000-0000-000000000000";
const CHURN_1: &str = "77777777-1111-1111-1111-111111111111";
const CHURN_2: &str = "77777777-2222-2222-2222-222222222222";
const ITEM_A: &str = "88888888-0000-0000-0000-000000000000";
const ITEM_B: &str = "88888888-1111-1111-1111-111111111111";
const GAUGE_A: &str = "99999999-0000-0000-0000-000000000000";
const GAUGE_B: &str = "99999999-1111-1111-1111-111111111111";

/// Superseding versions appended to `CHURN_2`. Each `update` appends a physical
/// row, so the live set ends up `{0, 1, 2 + CHURN_UPDATES}` — a span of
/// `3 + CHURN_UPDATES` over 3 rows, which is below
/// `SPARSE_OFFSETS_SPAN_FACTOR` (128) density with room to spare. `sparse_ok`
/// below asserts that rather than trusting this constant.
const CHURN_UPDATES: usize = 500;

/// Mirrors `SPARSE_OFFSETS_SPAN_FACTOR` in `crates/storage-native/src/lib.rs`.
/// If the substrate constant moves, this assertion is what says so.
const SPARSE_SPAN_FACTOR: usize = 128;

static mut FAILURES: u32 = 0;

fn check(uri: &str, got: (u16, String), want_status: u16, want_body: &str) {
    let (status, body) = got;
    if status == want_status && body == want_body {
        println!("  ok   {status} {uri}");
        return;
    }
    println!("  FAIL {uri}");
    println!("    want {want_status}  {want_body}");
    println!("    got  {status}  {body}");
    unsafe { FAILURES += 1 }
}

/// A non-byte assertion: that a scenario is still exercising the branch it names.
fn require(what: &str, cond: bool, detail: &str) {
    if cond {
        println!("  ok   {what}");
        return;
    }
    println!("  FAIL {what}");
    println!("    {detail}");
    unsafe { FAILURES += 1 }
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

fn uuid(s: &str) -> Uuid {
    s.parse().expect("parse uuid")
}

fn dec(s: &str) -> rust_decimal::Decimal {
    s.parse().expect("parse decimal")
}

fn ts(s: &str) -> Timestamp {
    Timestamp::from_rfc3339(s).expect("parse timestamp")
}

#[tokio::main]
async fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut db = Database::open_at(dir.clone());

    db.create_maker(Maker {
        id: uuid(MAKER),
        name: "Ada".to_string(),
        widgets: (),
    })
    .expect("create maker");

    // Row A: the null optional, a scale-bearing decimal, the first enum variant,
    // a positive instant, high/low bytes, a negative array element, and a json
    // object whose OWN keys (and nested keys) are out of sorted order.
    db.create_widget(Widget {
        id: uuid(WIDGET_A),
        label: None,
        price: dec("10.50"),
        tier: Tier::Free,
        made_at: ts("2026-01-02T03:04:05.678Z"),
        checksum: [0, 1, 254, 255],
        scores: [-1, 0, 7],
        dims: Dims { w: 3, h: 4 },
        owner: uuid(MAKER),
        parts: (),
        payload: serde_json::json!({ "z": 1, "a": { "y": 2, "b": 3 } }),
        serial: 1,
    })
    .expect("create widget a");

    // Row B: the populated optional carrying escape-sensitive text, a negative
    // sub-unit decimal, the last enum variant, a pre-2000 instant, and a null json.
    db.create_widget(Widget {
        id: uuid(WIDGET_B),
        label: Some("quote \" tab \t ✓".to_string()),
        price: dec("-0.001"),
        tier: Tier::Enterprise,
        made_at: ts("1999-12-31T23:59:59.999Z"),
        checksum: [9, 9, 9, 9],
        scores: [2, 3, 4],
        dims: Dims { w: 0, h: 65535 },
        owner: uuid(MAKER),
        parts: (),
        payload: serde_json::Value::Null,
        serial: 2,
    })
    .expect("create widget b");

    // Makes `Widget.parts` a populated one-to-many rather than a vacuous one.
    db.create_part(Part {
        id: uuid(PART),
        name: "bolt".to_string(),
        widget: uuid(WIDGET_A),
    })
    .expect("create part");

    db.create_note(Note {
        body: "first".to_string(),
        id: uuid(NOTE_A),
        weight: 1,
    })
    .expect("create note a");
    db.create_note(Note {
        body: "second".to_string(),
        id: uuid(NOTE_B),
        weight: 2,
    })
    .expect("create note b");

    for (id, note, seq) in [
        (CHURN_0, "c-zero", 10u32),
        (CHURN_1, "c-one", 20),
        (CHURN_2, "c-two", 30),
    ] {
        db.create_churn(Churn {
            id: uuid(id),
            note: note.to_string(),
            seq,
        })
        .expect("create churn");
    }
    // #281 S9. One vendor, two items: `Item.vendor` is the non-scan inline-string FK
    // whose `borrowed` flag flips between the two page construction sites.
    db.create_vendor(Vendor {
        id: forgedb_types::InlineStr::try_from("acme").expect("vendor key is 4 chars"),
        name: "Acme".to_string(),
        items: (),
    })
    .expect("create vendor");
    for (id, sku) in [(ITEM_A, "sku-a"), (ITEM_B, "sku-b")] {
        db.create_item(Item {
            id: uuid(id),
            sku: sku.to_string(),
            vendor: forgedb_types::InlineStr::try_from("acme").expect("vendor key"),
        })
        .expect("create item");
    }

    // #281 S11. TWO gauges, and the `>= 2` is load-bearing: with a single row the
    // negative-form predicate would take the fast path and return that same row with
    // `total: 1` — byte-identical to the filtered answer, so the scenario would stay
    // green under its own discriminating mutation. A second, non-matching row makes
    // the mutation change both `total` and the `data` array.
    for (id, lim) in [(GAUGE_A, 1u32), (GAUGE_B, 7)] {
        db.create_gauge(Gauge {
            id: uuid(id),
            limit: lim,
            offset: 0,
            sort: "none".to_string(),
            order: "asc".to_string(),
        })
        .expect("create gauge");
    }

    // Same values every time: the churn is about *physical row placement*, so the
    // wire bytes must not move with it.
    for _ in 0..CHURN_UPDATES {
        db.update_churn(
            uuid(CHURN_2),
            Churn {
                id: uuid(CHURN_2),
                note: "c-two".to_string(),
                seq: 30,
            },
        )
        .expect("update churn");
    }

    db.commit().expect("commit");

    // ---- branch-reached assertions, taken before `db` is shared ----
    //
    // Scenario 6 claims the page's rows are scattered enough for
    // `VariableColumn::gather_buffered` to take `gather_sparse`. That is a density
    // predicate over the physical row indices, so assert the predicate itself —
    // a corpus that quietly stopped being sparse would leave the scenario passing
    // while exercising the dense path.
    let churn_live = db.churn.export_live_indices();
    let churn_span = churn_live.iter().max().unwrap() + 1 - churn_live.iter().min().unwrap();
    require(
        "churn selection is below gather_sparse density",
        churn_span > churn_live.len() * SPARSE_SPAN_FACTOR,
        &format!(
            "live rows {churn_live:?} span {churn_span} over {} rows; need span > {}",
            churn_live.len(),
            churn_live.len() * SPARSE_SPAN_FACTOR
        ),
    );

    // Scenario 8 claims `?serial=` takes the #160 pushdown arm (`sel = Some(..)`).
    // `__rows_by_serial` is exactly what the generated handler calls to build it.
    require(
        "?serial=1 resolves through the index pushdown",
        db.widget.__rows_by_serial("1").is_some(),
        "expected Some(candidate rows) — a None falls through to the full scan",
    );
    // Scenario 4's pushdown half: a parseable value with no matches yields an
    // EMPTY candidate set, so the gather runs on `&[]` (gotcha 7).
    require(
        "?serial=999 resolves to an empty pushdown selection",
        db.widget.__rows_by_serial("999") == Some(Vec::new()),
        "expected Some([]) — the empty-selection gather is what this covers",
    );

    // `as_of` is an opaque row-count watermark; at the live row count the snapshot
    // read must return exactly what the live read does (scenario 9).
    let widget_rows = db.widget.row_count();

    let db = Arc::new(RwLock::new(db));
    let router = || api::create_router(db.clone());

    // Declaration order, NOT identity-first and NOT scan-set order: `scores`,
    // `dims`, `owner`, `parts` and `payload` all sit between `checksum` and
    // `serial`, and `parts` (a virtual `[Model]`, `()` in the record) is `null`.
    let widget_a = concat!(
        r#"{"id":"22222222-2222-2222-2222-222222222222","label":null,"price":"10.50","#,
        r#""tier":"Free","made_at":"2026-01-02T03:04:05.678000Z","#,
        r#""checksum":[0,1,254,255],"scores":[-1,0,7],"dims":{"w":3,"h":4},"#,
        r#""owner":"11111111-1111-1111-1111-111111111111","parts":null,"#,
        r#""payload":{"a":{"b":3,"y":2},"z":1},"serial":1}"#
    );
    let widget_b = concat!(
        r#"{"id":"33333333-3333-3333-3333-333333333333","#,
        r#""label":"quote \" tab \t ✓","price":"-0.001","#,
        r#""tier":"Enterprise","made_at":"1999-12-31T23:59:59.999000Z","#,
        r#""checksum":[9,9,9,9],"scores":[2,3,4],"dims":{"w":0,"h":65535},"#,
        r#""owner":"11111111-1111-1111-1111-111111111111","parts":null,"#,
        r#""payload":null,"serial":2}"#
    );
    // `id` is declared SECOND in `Note`, and the wire follows the declaration.
    let note_a = r#"{"body":"first","id":"55555555-5555-5555-5555-555555555555","weight":1}"#;
    let note_b = r#"{"body":"second","id":"66666666-6666-6666-6666-666666666666","weight":2}"#;
    let churn_0 = r#"{"id":"77777777-0000-0000-0000-000000000000","note":"c-zero","seq":10}"#;
    let churn_1 = r#"{"id":"77777777-1111-1111-1111-111111111111","note":"c-one","seq":20}"#;
    let churn_2 = r#"{"id":"77777777-2222-2222-2222-222222222222","note":"c-two","seq":30}"#;

    // --- Scenario 1: the wide model, every field class, key order included ---
    let uri = "/api/widget?sort=serial";
    let live_page = format!(r#"{{"data":[{widget_a},{widget_b}],"total":2,"limit":50,"offset":0}}"#);
    check(uri, call(router(), uri).await, 200, &live_page);

    // --- Scenario 2: identity field declared SECOND — no identity-first reorder ---
    let uri = "/api/note?sort=weight";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{note_a},{note_b}],"total":2,"limit":50,"offset":0}}"#),
    );

    // --- Scenario 3: a json object stored with non-sorted keys ---
    // Today's path is `read the stored text` → `from_str` → `Value` (BTreeMap) →
    // re-serialize, which SORTS. `sort=made_at` is a non-pushdown sort, so this is
    // a different arm from scenario 1's.
    let uri = "/api/widget?sort=made_at";
    let (status, body) = call(router(), uri).await;
    check(
        uri,
        (status, body.clone()),
        200,
        &format!(r#"{{"data":[{widget_b},{widget_a}],"total":2,"limit":50,"offset":0}}"#),
    );
    require(
        "json is emitted as a sorted object, not a string or a restructured value",
        body.contains(r#""payload":{"a":{"b":3,"y":2},"z":1}"#)
            && !body.contains(r#""payload":{"z":1"#),
        &format!("expected a sorted json object for `payload`; got: {body}"),
    );

    // --- #389: a timestamp filter param is floored to the field's quantum ---
    //
    // `made_at` is `timestamp(ms)` and is NOT indexed, so `?made_at=` takes the
    // predicate path alone (`__widget_scan_matches`) with no index pushdown. That
    // isolates the REST half of #389 from the index-key half, which `index_test`
    // covers — the two are separate fixes and the REST list path needs both, since
    // the pushdown selects candidates and the predicate then re-checks them.
    //
    // The param carries MICROSECOND precision (`…678999Z`); row A is stored at
    // `…678Z`. `Timestamp`'s `PartialEq` is over the raw i64 micros and does not
    // self-correct, so before #389 this compared 1767323045678999 against
    // 1767323045678000 and returned an empty page. A whole-millisecond param would
    // pass either way and prove nothing — that is the point of the extra digits.
    let uri = "/api/widget?made_at=2026-01-02T03:04:05.678999Z";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{widget_a}],"total":1,"limit":50,"offset":0}}"#),
    );

    // The same instant spelled exactly, to the millisecond. Flooring is idempotent,
    // so both spellings must select the same row; if only one passes, the param and
    // the stored value have been reconciled in the wrong direction.
    let uri = "/api/widget?made_at=2026-01-02T03:04:05.678Z";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{widget_a}],"total":1,"limit":50,"offset":0}}"#),
    );

    // --- Scenario 4: a filter that eliminates every row ---
    // Pushdown half: `Some([])` → the gather runs on an empty selection.
    let uri = "/api/widget?serial=999";
    check(
        uri,
        call(router(), uri).await,
        200,
        r#"{"data":[],"total":0,"limit":50,"offset":0}"#,
    );
    // Full-scan half: `sel = None`, every row decoded and rejected by `keep`.
    let uri = "/api/widget?label=nope";
    check(
        uri,
        call(router(), uri).await,
        200,
        r#"{"data":[],"total":0,"limit":50,"offset":0}"#,
    );

    // --- Scenario 5: offset beyond total — empty page, TRUE total ---
    let uri = "/api/widget?sort=serial&offset=5";
    check(
        uri,
        call(router(), uri).await,
        200,
        r#"{"data":[],"total":2,"limit":50,"offset":5}"#,
    );

    // --- Scenario 6: rows scattered across a churned table (gather_sparse) ---
    let uri = "/api/churn?sort=seq";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(
            r#"{{"data":[{churn_0},{churn_1},{churn_2}],"total":3,"limit":50,"offset":0}}"#
        ),
    );
    // Pushdown over the same churned table: one candidate, far from row 0.
    let uri = "/api/churn?seq=30";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{churn_2}],"total":1,"limit":50,"offset":0}}"#),
    );

    // --- Scenario 7: descending sort — row ORDER unchanged ---
    // The refs are built by buffer slot, then filtered, then reordered; after that
    // a ref's position says nothing about its physical row. A `__slot`-to-row
    // mis-mapping returns real rows in the wrong order, which no assertion on
    // `total` catches — only the exact body does.
    let uri = "/api/widget?sort=serial&order=desc";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{widget_b},{widget_a}],"total":2,"limit":50,"offset":0}}"#),
    );
    // Sort then TRUNCATE: the surviving row is the one the sort moved to the front,
    // so a mis-mapping shows up as the wrong single row rather than a wrong order.
    let uri = "/api/widget?sort=serial&order=desc&limit=1";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{widget_b}],"total":2,"limit":1,"offset":0}}"#),
    );
    // The hardest shape: descending sort + a mid-page slice over the SPARSE
    // selection, where slot order, physical order and sorted order all differ.
    let uri = "/api/churn?sort=seq&order=desc&limit=2&offset=1";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{churn_1},{churn_0}],"total":3,"limit":2,"offset":1}}"#),
    );

    // --- Scenario 8: the #160 index-pushdown arm (`sel = Some(..)`) ---
    let uri = "/api/widget?serial=1";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{widget_a}],"total":1,"limit":50,"offset":0}}"#),
    );
    // The enum-keyed index is a second pushdown-eligible column.
    let uri = "/api/widget?tier=Enterprise";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{widget_b}],"total":1,"limit":50,"offset":0}}"#),
    );

    // --- Scenario 9: `?as_of=` — the snapshot branch is a DIFFERENT arm ---
    // Asserted against `live_page` itself, so "unchanged" means unchanged from the
    // live read, not merely unchanged from a literal copied beside it.
    let uri = &format!("/api/widget?as_of={widget_rows}&sort=serial");
    check(uri, call(router(), uri).await, 200, &live_page);

    // --- W1 (#281): the UNFILTERED, UNSORTED page — the fast path ---
    //
    // Every URI above carries `?sort=` or a field filter, so before this block the
    // fast path was invisible to the byte guard: it could have been emitted, never
    // taken, and every assertion here would still pass. These are the requests that
    // reach it.
    //
    // Byte-identical to the sorted reads above, and that equality is the assertion
    // rather than a fresh literal: `widget_a` is created before `widget_b` and
    // `note_a` before `note_b`, so on this fixture physical order coincides with
    // serial/weight order. That is convenient and it is also the caveat — these
    // three URIs pin the BYTES and the WINDOW, not the ordering. Ordering is pinned
    // where it can actually be discriminated: `tests/list_scan_test.rs` recomputes
    // the physical order of a churned corpus independently, and
    // `tests/page_identity_test.rs` compares the two construction sites over a
    // 1,000-row corpus whose updates and deletes genuinely scramble it.
    let uri = "/api/widget";
    check(uri, call(router(), uri).await, 200, &live_page);
    let uri = "/api/note";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{note_a},{note_b}],"total":2,"limit":50,"offset":0}}"#),
    );
    let uri = "/api/churn";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{churn_0},{churn_1},{churn_2}],"total":3,"limit":50,"offset":0}}"#),
    );

    // The fast path's own pagination arithmetic. `offset` past the end and `limit`
    // shorter than the page are where the clamping lives, and neither is expressible
    // through any sorted URI above.
    let uri = "/api/widget?limit=1";
    check(
        uri,
        call(router(), uri).await,
        200,
        // `total` is the live-row count, NOT the page length — the envelope's
        // pagination fields describe the window, `total` describes the set it is a
        // window onto. A fast path that returned `__page.len()` here would look
        // right on every unpaginated URI above.
        &format!(r#"{{"data":[{widget_a}],"total":2,"limit":1,"offset":0}}"#),
    );
    let uri = "/api/widget?offset=5";
    check(
        uri,
        call(router(), uri).await,
        200,
        r#"{"data":[],"total":2,"limit":50,"offset":5}"#,
    );
    let uri = "/api/churn?limit=2&offset=1";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{churn_1},{churn_2}],"total":3,"limit":2,"offset":1}}"#),
    );

    // `?projection=` on a model that declares NO `@projection`. `#proj_list_block` is
    // empty for such a model, so `projection` is an ordinary unknown key naming no
    // filterable field: the predicate holds and the fast path is taken. Asserted
    // against the bare URI's own answer, so "unchanged" means unchanged from the
    // request it must equal. If a future change makes `projection` a predicate term,
    // this silently starts returning the scan path's envelope with no compile error.
    let uri = "/api/widget?projection=card";
    check(uri, call(router(), uri).await, 200, &live_page);

    // #281 S9: the non-scan, inline-string FK. `Item.vendor` is decoded through a
    // different arm of `field_read_stmt` on the fast path than on the scan path, so
    // a frozen literal is what says the two arms produce the same bytes.
    let item_a = format!(r#"{{"id":"{ITEM_A}","sku":"sku-a","vendor":"acme"}}"#);
    let item_b = format!(r#"{{"id":"{ITEM_B}","sku":"sku-b","vendor":"acme"}}"#);
    let uri = "/api/item";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{item_a},{item_b}],"total":2,"limit":50,"offset":0}}"#),
    );

    // --- W2 (#281): `?as_of=` is a DIFFERENT arm, and the watermark must cut below
    // the create count or the scenario cannot discriminate ---
    //
    // Scenario 9 above asserts the snapshot page EQUALS the live page, which is the
    // right assertion there and is exactly why it cannot catch a fast-path branch
    // hoisted above `match __as_of`: both sides would be the live page. `Churn` does
    // not fix that by itself — it is churned with IDENTICAL values by construction,
    // so at any watermark at or after the three creates the snapshot read is
    // byte-identical to the live read. `as_of=2` cuts below the third create, so the
    // two pages differ in ROW COUNT, and a hoist returns 3 rows where 2 are expected.
    let uri = "/api/churn?as_of=2";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{churn_0},{churn_1}],"total":2,"limit":50,"offset":0}}"#),
    );

    // --- W3 (#281 S11): a model that legally declares `limit`/`offset`/`sort`/`order`
    // as FIELDS — the only scenario separating the two predicate forms ---
    //
    // Every other scenario in this file passes under both the positive predicate
    // ("does any key name a filterable field of this model?") and a negative
    // reserved-key exclusion list. Here they differ: `?limit=1` names a real field of
    // `Gauge`, so the filter must apply. Under the negative form the name is excluded,
    // the fast path fires, and BOTH gauges come back.
    //
    // `?limit=1` is simultaneously a filter (set 1) and the page limit (set 3), and
    // both behaviours are asserted — the row is filtered to `limit == 1` AND the
    // envelope's `limit` is 1. Asserting only one documents half of what it proves.
    let gauge_a =
        format!(r#"{{"id":"{GAUGE_A}","limit":1,"offset":0,"sort":"none","order":"asc"}}"#);
    let uri = "/api/gauge?limit=1";
    check(
        uri,
        call(router(), uri).await,
        200,
        &format!(r#"{{"data":[{gauge_a}],"total":1,"limit":1,"offset":0}}"#),
    );

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} list-page wire mismatch(es)");
        std::process::exit(1);
    }
    println!("all list-page wire-format checks passed");
}
"##;
