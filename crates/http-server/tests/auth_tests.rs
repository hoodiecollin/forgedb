use forgedb_http_server::*;
use axum::body::Body;
use axum::http::Request;

#[test]
fn test_auth_context_roles() {
    let ctx = AuthContext::authenticated(
        "user1".to_string(),
        vec!["admin".to_string(), "user".to_string()],
    );

    assert!(ctx.is_authenticated);
    assert_eq!(ctx.user_id, Some("user1".to_string()));
    assert!(ctx.has_role("admin"));
    assert!(ctx.has_role("user"));
    assert!(!ctx.has_role("superadmin"));
    assert!(ctx.has_any_role(&["admin", "superadmin"]));
}

#[test]
fn test_unauthenticated_context() {
    let ctx = AuthContext::unauthenticated();

    assert!(!ctx.is_authenticated);
    assert_eq!(ctx.user_id, None);
    assert!(!ctx.has_role("any"));
}

#[test]
fn test_api_key_hook() {
    let hook = ApiKeyAuthHook::new(vec!["secret123".to_string()]);

    let req = Request::builder()
        .header("x-api-key", "secret123")
        .body(Body::empty())
        .unwrap();

    let result = hook.authenticate(&req);
    assert!(result.is_ok());

    let ctx = result.unwrap();
    assert!(ctx.is_authenticated);
}

#[test]
fn test_no_auth_hook() {
    let hook = NoAuthHook;
    let req = Request::builder().body(Body::empty()).unwrap();

    let result = hook.authenticate(&req);
    assert!(result.is_ok());

    let ctx = result.unwrap();
    assert!(!ctx.is_authenticated);
}
