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
//! - That a `json` field's *own* keys are still normalized (sorted) — that payload
//!   is a `serde_json::Value` either way, so #229 did not touch it.
//! - The error bodies, which deliberately stayed `json!` objects.
//!
//! It compiles a generated crate, so it is `#[ignore]`d out of the fast hermetic
//! default suite. Run it explicitly:
//!
//! ```bash
//! make api-wire-test         # or:
//! cargo test --test api_wire_test -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

/// Repo root — `CARGO_MANIFEST_DIR` is the crate this test compiles under.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Path dep line for a workspace substrate crate.
fn dep(name: &str, crate_dir: &str) -> String {
    let path = repo_root().join("crates").join(crate_dir);
    format!("{name} = {{ path = {:?} }}\n", path.to_string_lossy())
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

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
    let proj = std::env::temp_dir().join(format!("forgedb-apiwire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();

    write(&proj.join("schema.forge"), SCHEMA);
    let forgedb = env!("CARGO_BIN_EXE_forgedb");
    let gen_status = Command::new(forgedb)
        .args(["generate", "all", "--output", "src", "--schema", "schema.forge"])
        .current_dir(&proj)
        .status()
        .expect("run forgedb generate");
    assert!(gen_status.success(), "forgedb generate all failed");

    let mut cargo_toml = String::from(
        "[package]\nname = \"wiredriver\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n",
    );
    for (n, d) in [
        ("forgedb-storage", "storage"),
        ("forgedb-types", "types"),
        ("forgedb-changefeed", "changefeed"),
        ("forgedb-wal", "wal"),
        ("forgedb-auth", "auth"),
        ("forgedb-query-params", "query-params"),
        ("forgedb-compaction", "compaction"),
        ("forgedb-txn", "txn"),
        ("forgedb-coordinator", "coordinator"),
    ] {
        cargo_toml.push_str(&dep(n, d));
    }
    cargo_toml.push_str("serde = { version = \"1\", features = [\"derive\"] }\n");
    cargo_toml.push_str("serde_json = \"1\"\n");
    cargo_toml.push_str("regex = \"1\"\n");
    cargo_toml.push_str("rust_decimal = { version = \"1\", features = [\"serde-with-str\"] }\n");
    cargo_toml.push_str("utoipa = { version = \"5\", features = [\"uuid\"] }\n");
    cargo_toml.push_str("utoipa-axum = \"0.2\"\n");
    cargo_toml.push_str("axum = { version = \"0.8\", features = [\"ws\"] }\n");
    cargo_toml.push_str("tokio = { version = \"1\", features = [\"full\"] }\n");
    cargo_toml.push_str("tower = { version = \"0.5\", features = [\"util\"] }\n");
    cargo_toml.push_str("tower-http = { version = \"0.6\", features = [\"trace\"] }\n");
    cargo_toml.push_str("\n[workspace]\n");
    write(&proj.join("Cargo.toml"), &cargo_toml);

    // `generate all` writes database.rs / api.rs into src/; the driver is the
    // crate root that mounts them, exactly as the `forgedb init` scaffold does.
    write(&proj.join("src/main.rs"), DRIVER);

    let target = proj.join("target");
    let build = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(&proj)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("run cargo build");
    assert!(build.success(), "driver failed to compile");

    let out = Command::new(target.join("debug/wiredriver"))
        .arg(proj.join("data"))
        .output()
        .expect("run driver");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    println!("{stdout}");
    assert!(
        out.status.success(),
        "driver reported a wire-format mismatch:\n{stdout}\n{stderr}"
    );

    let _ = std::fs::remove_dir_all(&proj);
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
