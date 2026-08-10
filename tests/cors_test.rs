//! Gate 3 for #140 — CORS on the generated HTTP routes, `Origin` on the generated
//! WebSocket routes.
//!
//! # Why these live at the wire level
//!
//! Three of the decisions in #140 cannot be checked by a snapshot, because they are
//! about what the *composed router* does rather than about what text is emitted:
//!
//! 1. **Omitting the layer is not the same as emitting an empty one.** A
//!    `CorsLayer` with no configured origins still *answers* preflight `OPTIONS`
//!    (200, no allow headers), whereas the generated routes — registered only with
//!    `get`/`post`/`put`/`delete` — return **405**. Emitting an empty layer would
//!    therefore change observable behavior for every existing deployment. Only a
//!    real request can tell the two apart.
//! 2. **The layer must be outermost, outside the auth guard.** Browsers send
//!    preflight `OPTIONS` with **no** `Authorization` header, so a CORS layer placed
//!    inside the tenant guard has its preflight rejected 401 and the browser reports
//!    an opaque CORS failure. A snapshot sees `.layer(cors)` either way; only a
//!    request through the guarded router sees 200-vs-401.
//! 3. **WebSocket handshakes are not covered by CORS at all.** Browsers do not
//!    preflight them and do not enforce CORS on them, so the server must check
//!    `Origin` itself. Shipping "wire origins" without this would read as done when
//!    it is half done.
//!
//! The generator-side counterparts (which functions exist, which methods and headers
//! are configured, that no `allow_credentials` appears) are
//! `test_api_generation_cors_*` in `crates/codegen/tests/codegen_snapshots.rs`. Both
//! must move together.
//!
//! Compiles a generated crate, so it is `#[ignore]`d out of the fast default suite:
//!
//! ```bash
//! make cors-test
//! ```

mod common;

const SCHEMA: &str = r#"
Post {
  id: +uuid
  title: string
  views: u32
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored (see `make cors-test`)"]
fn cors_and_ws_origin_behavior() {
    let (out, proj) = common::generate_compile_run("corsdriver", SCHEMA, DRIVER);
    common::assert_driver_ok(&out, &proj, "driver reported a CORS/origin mismatch");
}

/// The driver drives each router variant through `tower::oneshot` and asserts on
/// status + the `access-control-allow-origin` header. No socket is needed: a
/// preflight is an ordinary request, and a rejected WS upgrade never upgrades.
const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use axum::body::Body;
use axum::http::Request;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const APP: &str = "https://app.example";
const EVIL: &str = "https://evil.example";

static mut FAILURES: u32 = 0;

fn check(what: &str, got: (u16, Option<String>), want_status: u16, want_acao: Option<&str>) {
    let (status, acao) = got;
    let want = want_acao.map(|s| s.to_string());
    if status == want_status && acao == want {
        println!("  ok   {what} -> {status} acao={acao:?}");
        return;
    }
    println!("  FAIL {what}");
    println!("    want status={want_status} acao={want:?}");
    println!("    got  status={status} acao={acao:?}");
    unsafe { FAILURES += 1 }
}

/// Send `req` and return its status plus `access-control-allow-origin`.
async fn call(router: axum::Router, req: Request<Body>) -> (u16, Option<String>) {
    let resp = router.oneshot(req).await.expect("router call");
    let status = resp.status().as_u16();
    let acao = resp
        .headers()
        .get("access-control-allow-origin")
        .map(|v| v.to_str().expect("header utf8").to_string());
    (status, acao)
}

fn preflight(origin: &str, method: &str) -> Request<Body> {
    Request::builder()
        .method("OPTIONS")
        .uri("/api/post")
        .header("origin", origin)
        .header("access-control-request-method", method)
        .body(Body::empty())
        .unwrap()
}

fn bare_options() -> Request<Body> {
    Request::builder()
        .method("OPTIONS")
        .uri("/api/post")
        .body(Body::empty())
        .unwrap()
}

/// An authenticator that can never verify anything: an empty JWKS. Exactly right
/// here — the point is that a preflight must be answered WITHOUT auth, and that a
/// rejected request still carries the CORS headers the browser needs in order to
/// let the page read the status.
fn authenticator() -> Arc<forgedb_auth::Authenticator> {
    let keys = forgedb_auth::KeySource::from_jwks_json(r#"{"keys":[]}"#).expect("empty jwks");
    Arc::new(forgedb_auth::Authenticator::new(
        forgedb_auth::AuthConfig::default(),
        keys,
        "tenant-a",
    ))
}

/// Boot the generated router on a real loopback listener, speak a WebSocket
/// handshake over raw TCP, and assert the response status.
///
/// A real server connection is required because axum's `WebSocketUpgrade` extractor
/// needs hyper's `OnUpgrade` extension; `tower::oneshot` has none, so it rejects
/// every handshake with 426 before the handler's origin check can run. Reading only
/// the status line keeps this dependency-free — 101 means the gate let it through,
/// 403 means the gate refused it.
async fn ws_check(
    what: &str,
    origins: Option<Vec<&str>>,
    origin: Option<&str>,
    want_status: u16,
) {
    use std::io::{Read, Write};

    let dir = std::env::temp_dir().join(format!("forgedb-corsws-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = Arc::new(RwLock::new(Database::open_at(dir)));
    let opts = api::HttpOptions {
        allowed_origins: origins.map(|v| v.into_iter().map(|s| s.to_string()).collect()),
    };
    let router = api::create_router_with_options(db, opts);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    // Own the origin before it crosses into the blocking task: a `&str` borrowed
    // from the caller cannot escape into `spawn_blocking`.
    let origin_line = match origin {
        Some(o) => format!("Origin: {o}\r\n"),
        None => String::new(),
    };
    let status = tokio::task::spawn_blocking(move || {
        let mut sock = std::net::TcpStream::connect(addr).expect("connect");
        let req = format!(
            "GET /subscribe/post HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Connection: Upgrade\r\n\
             Upgrade: websocket\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             {origin_line}\r\n"
        );
        sock.write_all(req.as_bytes()).expect("write handshake");
        sock.set_read_timeout(Some(std::time::Duration::from_secs(5))).ok();
        let mut buf = [0u8; 256];
        let n = sock.read(&mut buf).unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]).to_string();
        head.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok()).unwrap_or(0)
    })
    .await
    .expect("handshake task");

    server.abort();
    check(what, (status, None), want_status, None);
}

#[tokio::main]
async fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let _ = std::fs::remove_dir_all(&dir);
    let db = Arc::new(RwLock::new(Database::open_at(dir)));

    let none = || api::create_router(db.clone());
    let opts = |origins: Vec<&str>| api::HttpOptions {
        allowed_origins: Some(origins.into_iter().map(|s| s.to_string()).collect()),
    };
    let with = |origins: Vec<&str>| api::create_router_with_options(db.clone(), opts(origins));

    // ── The unconfigured default must be byte-identical to today ──────────────
    //
    // This is the regression guard for "omit the layer, do not emit an empty one".
    // It fails the moment anyone makes the layer unconditional.
    check(
        "default: OPTIONS on a data route",
        call(none(), bare_options()).await,
        405,
        None,
    );
    check(
        "default: preflight from any origin gets no CORS headers",
        call(none(), preflight(APP, "POST")).await,
        405,
        None,
    );

    // ── Configured: the allow-list decides ───────────────────────────────────
    check(
        "configured: preflight from an allowed origin",
        call(with(vec![APP]), preflight(APP, "POST")).await,
        200,
        Some(APP),
    );
    check(
        "configured: preflight from a disallowed origin",
        call(with(vec![APP]), preflight(EVIL, "POST")).await,
        200,
        None,
    );
    check(
        "configured: an actual GET carries the header",
        call(
            with(vec![APP]),
            Request::builder()
                .uri("/api/post")
                .header("origin", APP)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
        200,
        Some(APP),
    );
    check(
        "wildcard: any origin is allowed",
        call(with(vec!["*"]), preflight(EVIL, "POST")).await,
        200,
        Some("*"),
    );

    // ── Placement: outermost, outside the auth guard ──────────────────────────
    //
    // A browser preflight carries no Authorization header. If the CORS layer sat
    // inside the tenant guard this would be 401 and the browser would report a
    // CORS error with nothing actionable in it.
    let guarded = |origins: Vec<&str>| {
        api::create_router_with_auth_and_options(db.clone(), authenticator(), opts(origins))
    };
    check(
        "auth + configured: preflight is answered without a token",
        call(guarded(vec![APP]), preflight(APP, "POST")).await,
        200,
        Some(APP),
    );
    // And an unauthenticated real request is still rejected — but with the header,
    // so the page can read the 401 instead of seeing an opaque network failure.
    check(
        "auth + configured: unauthenticated GET is 401 WITH the CORS header",
        call(
            guarded(vec![APP]),
            Request::builder()
                .uri("/api/post")
                .header("origin", APP)
                .body(Body::empty())
                .unwrap(),
        )
        .await,
        401,
        Some(APP),
    );

    // ── WebSocket: checked by the handler, not by CORS ────────────────────────
    //
    // These cannot go through `tower::oneshot`: axum's `WebSocketUpgrade` extractor
    // requires hyper's `OnUpgrade` request extension, which only a real server
    // connection carries, so `oneshot` rejects every handshake 426 before the
    // handler body ever runs. So this half binds a real listener and speaks the
    // handshake over raw TCP — no ws client dependency needed, since only the
    // status line is under test.
    ws_check("default: WS from any origin is accepted (unchanged)", None, Some(EVIL), 101).await;
    ws_check(
        "configured: WS from a disallowed origin is refused before upgrade",
        Some(vec![APP]),
        Some(EVIL),
        403,
    )
    .await;
    ws_check(
        "configured: WS from an allowed origin upgrades",
        Some(vec![APP]),
        Some(APP),
        101,
    )
    .await;
    // Native `/replicate` followers, CLI tools and tests send no Origin. Rejecting
    // on absence would break them and buy nothing: an attacker who controls the
    // client controls the header. Origin checking defends the browser threat model
    // only, where the browser sets it and the page cannot forge it.
    ws_check(
        "configured: WS with no Origin at all is accepted",
        Some(vec![APP]),
        None,
        101,
    )
    .await;

    // ── Parsing ──────────────────────────────────────────────────────────────
    assert!(api::parse_origins("").expect("empty is not an error").is_none());
    assert_eq!(
        api::parse_origins(" https://a.example , https://b.example ").unwrap(),
        Some(vec!["https://a.example".to_string(), "https://b.example".to_string()]),
        "entries are trimmed"
    );
    assert!(
        api::parse_origins("https://a.example,*").is_err(),
        "`*` mixed with an explicit origin is ambiguous and must be refused, not \
         silently resolved one way or the other"
    );
    assert!(
        api::parse_origins("https://a.example,\u{7f}bad").is_err(),
        "an entry that is not a valid header value must be refused"
    );

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} CORS/origin mismatch(es)");
        std::process::exit(1);
    }
    println!("all CORS/origin checks passed");
}
"##;
