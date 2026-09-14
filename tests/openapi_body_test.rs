mod common;

const SCHEMA: &str = r#"
Library {
  id: +uuid
  name: string
  books: [Book]
  hero: tsx://components/LibraryHero @relations(books)
}

Book {
  id: +uuid
  title: string
  shelf: *Library
}
"#;

#[test]
#[ignore = "compiles a generated crate; run with --ignored"]
fn the_documented_create_and_replace_bodies_are_accepted_by_the_generated_handler() {
    let (out, proj) = common::generate_compile_run("openapibody", SCHEMA, DRIVER);
    common::assert_driver_ok(&out, &proj, "driver reported a write-body mismatch");
}

const DRIVER: &str = r##"mod database;
use database::*;

mod api;

use axum::body::Body;
use axum::http::{Method, Request};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

const LIBRARY_ID: &str = "11111111-1111-1111-1111-111111111111";

static mut FAILURES: u32 = 0;

fn check(label: &str, got: (u16, String), want_status: u16, want_body: Option<&str>) {
    let (status, body) = got;
    let body_ok = want_body.is_none_or(|w| w == body);
    if status == want_status && body_ok {
        println!("  ok   {status} {label}");
        return;
    }
    println!("  FAIL {label}");
    println!("    want {want_status}  {}", want_body.unwrap_or("<any>"));
    println!("    got  {status}  {body}");
    unsafe { FAILURES += 1 }
}

async fn send(router: axum::Router, method: Method, uri: &str, body: Option<&str>) -> (u16, String) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty))
        .unwrap();
    let resp = router.oneshot(req).await.expect("router call");
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, String::from_utf8(bytes.to_vec()).expect("utf8 body"))
}

#[tokio::main]
async fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let _ = std::fs::remove_dir_all(&dir);
    let db = Database::open_at(dir.clone());
    let db = Arc::new(RwLock::new(db));
    let router = || api::create_router(db.clone());

    let documented = format!(r#"{{"id":"{LIBRARY_ID}","name":"Main"}}"#);

    check(
        "POST exactly the body openapi.json documents",
        send(router(), Method::POST, "/api/library", Some(&documented)).await,
        201,
        Some(&format!(r#"{{"id":"{LIBRARY_ID}"}}"#)),
    );
    check(
        "PUT exactly the body openapi.json documents",
        send(router(), Method::PUT, &format!("/api/library/{LIBRARY_ID}"), Some(&documented)).await,
        200,
        Some(&format!(r#"{{"id":"{LIBRARY_ID}"}}"#)),
    );
    check(
        "GET returns the pre-fix row shape byte for byte",
        send(router(), Method::GET, &format!("/api/library/{LIBRARY_ID}"), None).await,
        200,
        Some(&format!(r#"{{"id":"{LIBRARY_ID}","name":"Main","books":null,"hero":""}}"#)),
    );

    let with_null_relation = format!(r#"{{"id":"{LIBRARY_ID}","name":"Main","books":null}}"#);
    check(
        "PUT with the explicit null the SDKs send for a virtual relation",
        send(router(), Method::PUT, &format!("/api/library/{LIBRARY_ID}"), Some(&with_null_relation)).await,
        200,
        None,
    );

    let with_null_component = format!(r#"{{"id":"{LIBRARY_ID}","name":"Main","hero":null}}"#);
    check(
        "PUT with a null component is still refused (the residual, #287)",
        send(router(), Method::PUT, &format!("/api/library/{LIBRARY_ID}"), Some(&with_null_component)).await,
        422,
        Some(r#"{"error":"invalid payload"}"#),
    );

    let with_list_relation = format!(r#"{{"id":"{LIBRARY_ID}","name":"Main","books":[]}}"#);
    check(
        "PUT with a list for a virtual relation is still refused (not this issue's question)",
        send(router(), Method::PUT, &format!("/api/library/{LIBRARY_ID}"), Some(&with_list_relation)).await,
        422,
        Some(r#"{"error":"invalid payload"}"#),
    );

    let failures = unsafe { FAILURES };
    if failures > 0 {
        eprintln!("{failures} write-body mismatch(es)");
        std::process::exit(1);
    }
    println!("all write-body checks passed");
}
"##;
