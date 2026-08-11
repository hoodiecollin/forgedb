//! REST wire-format guard: boots the **real generated router** in-process and
//! asserts the exact response bytes of every read path.
//!
//! # Why this test exists in this form
//!
//! #229 replaced `json!({ "data": page, ... })` with a typed envelope serialized
//! straight to bytes, and the two record reads (`get`, and the `?projection=`
//! variants) with `Json(record)` instead of `Json(to_value(&record))`. That removes
//! an intermediate `serde_json::Value` clone of every string in the response — but
//! it also changes the response bytes, because `Value::Object` is a `BTreeMap`
//! (serde_json without `preserve_order`), so going through it *sorted the keys
//! alphabetically*. Serializing from the type emits declaration order instead.
//!
//! JSON object key order carries no meaning, no conforming client can depend on it,
//! and the generated WebSocket paths have always emitted records in declaration
//! order — so this made the surface consistent rather than breaking it. But it is
//! still a change nobody should be able to make *accidentally*, and #226/#228 both
//! rewrite how the list page is produced. Hence: freeze the bytes.
//!
//! The generator-side counterpart is `test_api_generation_list_envelope` in
//! `crates/codegen/tests/codegen_snapshots.rs`, which pins what is emitted. This
//! test pins what that emission puts on the socket. Both must move together.
//!
//! # What is pinned, precisely
//!
//! - Envelope key order: `data`, `total`, `limit`, `offset`.
//! - Record key order: schema declaration order, `id` first.
//! - That a `json` field is emitted as a sorted *object* — not a string, not
//!   restructured. NOTE this does not prove a borrowed passthrough would be caught:
//!   the fixture is built with `json!`, so its bytes are sorted before storage. See
//!   the comment above `PAGE_SCHEMA` for why that is not fixable from here.
//! - The error bodies, which deliberately stayed `json!` objects.
//!
//! # Two tests, two generated crates
//!
//! - `rest_read_paths_emit_the_frozen_wire_bytes` — #229's baseline over
//!   `SCHEMA` (`Post`/`Author`): envelope order, record order, projections, the
//!   error bodies, and the `?as_of=` arm.
//! - `list_page_emits_the_frozen_wire_bytes` — #226's guard over `PAGE_SCHEMA`,
//!   which carries the field classes and declaration orders that `SCHEMA` cannot
//!   express. See the block comment above `PAGE_SCHEMA` for why it is a second
//!   schema rather than more fields on `Post`.
//!
//! Each compiles its own generated crate, so this file is `#[ignore]`d out of the
//! fast hermetic default suite. Run it explicitly:
//!
//! ```bash
//! make api-wire-test         # or:
//! cargo test --test api_wire_test -- --ignored --nocapture
//! ```

mod common;

/// Every field class that reaches the wire: string (with escapes), optional,
/// integer, float, enum, json, required FK, optional FK — plus a projection, so
/// the projected read/list arms are covered too.
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

/// The driver: seeds two rows with **fixed** UUIDs (the bytes have to be
/// deterministic), then drives the generated router through `tower::oneshot` and
/// compares each response against a frozen literal.
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

// ---------------------------------------------------------------------------
// #226 — the list page's frozen bytes
// ---------------------------------------------------------------------------
//
// #226 replaces the list handler's page materialization (`page_ids` →
// `filter_map(get)` → `Vec<Model>` → serde) with a buffered gather serialized
// through a generated `<Model>PageRef<'a>`. Its success contract is **byte
// identity**, so this is a guard written *before* the change and expected GREEN
// against today's handler — a RED scenario here is a bug in the scenario.
//
// It is a second test rather than more requests on `SCHEMA` above, because three
// of the nine scenarios need shapes `Post`/`Author` cannot express, and rewriting
// `SCHEMA` would rewrite #229's frozen baseline at the same time:
//
//   - **the wide model** (`Widget`): a nullable `string`, a `decimal`, an enum, a
//     `timestamp`, a `bytes(N)`, a `[T; N]`, an inline `struct`, a required FK and
//     a virtual `[Model]`. `WidgetScanRef` holds only
//     `{id, label, price, tier, made_at, checksum, serial}` — `scores`, `dims`,
//     `owner`, `parts` and `payload` are outside `scan_field_set`, and `serial` is
//     declared *after* all of them. So building `PageRef` by appending the excluded
//     fields to the scan set emits `…checksum,serial,scores,dims,owner,payload`
//     where `Widget` emits `…checksum,scores,dims,owner,parts,payload,serial`:
//     semantically equal, byte-different, invisible to any `Value`-comparing
//     assertion (gotcha 1).
//   - **the identity-late model** (`Note`): `id` is declared *second*. `NoteScanRef`
//     is `{id, body, weight}` (identity-first) while `Note` is `{body, id, weight}`,
//     so the wire must NOT be identity-first (gotcha 1, sharpest form).
//
//   Two coverage limits worth knowing before editing these scenarios. A
//   `?sort=…&order=desc` request is NOT automatically a gotcha-3 (`__slot`) case:
//   under a position-instead-of-slot mutation, `Churn`'s desc sort stayed GREEN
//   because `seq = 1000 - i` makes descending order coincide with physical row
//   order — only the ASCENDING sorts caught it. The discriminating shape needs
//   sorted order != physical order != reverse-physical order. And gotcha 3 is
//   detectable only on `Widget`: every field of `Note` and `Churn` is filterable,
//   so their `PageRef` field set equals their `ScanRef`'s, their page-only gather
//   reads no columns, and a slot mis-map is invisible for them.
//   - **the churned model** (`Churn`): 3 live rows scattered across 503 physical
//     ones, which puts the scan's `note` column below
//     `SPARSE_OFFSETS_SPAN_FACTOR` density and sends `gather_buffered` down
//     `gather_sparse` (`crates/storage-native/src/lib.rs:1448`). The driver asserts
//     that density predicate — but note it MIRRORS the factor as a hardcoded 128, so
//     it tracks the *corpus*, not the substrate: raising
//     `SPARSE_OFFSETS_SPAN_FACTOR` would silently stop exercising the sparse arm
//     while the assertion still passed. Also, both gather arms are byte-equivalent in
//     both directions, so this is a correctness guard on the sparse *implementation*
//     and a performance guard on the branch *decision* — it is not a byte guard on
//     which arm ran.
//
// `Widget.payload` is WRITTEN as `json!({"z": 1, "a": {"y": 2, "b": 3}})` and must
// come out sorted, because `get(id)` round-trips it through `serde_json::Value`
// (a `BTreeMap` without `preserve_order`).
//
// **Read this before trusting the scenario: it does NOT guard gotcha 2's literal
// fear, and it cannot.** `json!` builds a `Value` — i.e. a `BTreeMap` — so the
// source order above is discarded *before* the value is ever stored. The bytes on
// disk are therefore ALREADY SORTED, and a borrowed `&str`/`RawValue` passthrough
// would emit those same sorted bytes and pass every check in this file. Storing
// genuinely unsorted json bytes is not expressible through the typed create path,
// which is why no scenario here does it.
//
// What the scenario DOES guard (verified RED by mutation): the grosser sibling
// failures — json emitted as a *string*, or otherwise restructured. That is worth
// keeping. The `!body.contains("\"payload\":{\"z\":1")` half is vacuous against a
// read-side change and is retained only as a cheap regression tripwire in case the
// write path ever starts preserving source order.
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
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make api-wire-test`)"]
fn list_page_emits_the_frozen_wire_bytes() {
    let (out, proj) = common::generate_compile_run("wirepage", PAGE_SCHEMA, PAGE_DRIVER);
    common::assert_driver_ok(&out, &proj, "driver reported a list-page wire mismatch");
}

/// The driver. Seeds fixed UUIDs and fixed field values (the bytes have to be
/// deterministic), churns one `Churn` row 500 times to scatter the live set, then
/// drives the generated router through `tower::oneshot`.
///
/// Three assertions are NOT about response bytes and exist so a scenario cannot
/// silently stop exercising what it names: the sparse-density predicate, and that
/// the pushdown resolver returns `Some` for the two `?serial=` requests.
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

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} list-page wire mismatch(es)");
        std::process::exit(1);
    }
    println!("all list-page wire-format checks passed");
}
"##;
