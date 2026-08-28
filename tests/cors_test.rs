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

fn authenticator() -> Arc<forgedb_auth::Authenticator> {
    let keys = forgedb_auth::KeySource::from_jwks_json(r#"{"keys":[]}"#).expect("empty jwks");
    Arc::new(forgedb_auth::Authenticator::new(
        forgedb_auth::AuthConfig::default(),
        keys,
        "tenant-a",
    ))
}

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

    let guarded = |origins: Vec<&str>| {
        api::create_router_with_auth_and_options(db.clone(), authenticator(), opts(origins))
    };
    check(
        "auth + configured: preflight is answered without a token",
        call(guarded(vec![APP]), preflight(APP, "POST")).await,
        200,
        Some(APP),
    );
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
    ws_check(
        "configured: WS with no Origin at all is accepted",
        Some(vec![APP]),
        None,
        101,
    )
    .await;

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
