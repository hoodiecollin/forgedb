//! Authentication and authorization hooks

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

/// Authentication context extracted from request
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// User ID (if authenticated)
    pub user_id: Option<String>,
    /// User roles/permissions
    pub roles: Vec<String>,
    /// Is authenticated
    pub is_authenticated: bool,
    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl AuthContext {
    /// Create an unauthenticated context
    pub fn unauthenticated() -> Self {
        Self {
            user_id: None,
            roles: vec![],
            is_authenticated: false,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Create an authenticated context
    pub fn authenticated(user_id: String, roles: Vec<String>) -> Self {
        Self {
            user_id: Some(user_id),
            roles,
            is_authenticated: true,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Check if user has any of the specified roles
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        roles.iter().any(|role| self.has_role(role))
    }
}

/// Authentication hook trait
///
/// Implement this trait to provide custom authentication logic
pub trait AuthHook: Send + Sync {
    /// Authenticate a request and return auth context
    ///
    /// # Arguments
    /// * `req` - The HTTP request
    ///
    /// # Returns
    /// * `Ok(AuthContext)` - Successfully extracted auth context
    /// * `Err(Response)` - Authentication failed with error response
    fn authenticate(&self, req: &Request<Body>) -> Result<AuthContext, Response>;
}

/// JWT authentication hook (example implementation)
pub struct JwtAuthHook {
    _secret: String,
}

impl JwtAuthHook {
    pub fn new(secret: String) -> Self {
        Self { _secret: secret }
    }
}

impl AuthHook for JwtAuthHook {
    fn authenticate(&self, req: &Request<Body>) -> Result<AuthContext, Response> {
        // Extract Authorization header
        let auth_header = req
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok());

        if let Some(auth_str) = auth_header {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                // TODO: Implement actual JWT validation
                // For now, this is a placeholder

                // Example: parse token and extract user info
                if token == "valid_token" {
                    return Ok(AuthContext::authenticated(
                        "user123".to_string(),
                        vec!["user".to_string()],
                    ));
                }
            }
        }

        // No valid authentication found
        Ok(AuthContext::unauthenticated())
    }
}

/// API key authentication hook (example implementation)
pub struct ApiKeyAuthHook {
    valid_keys: Vec<String>,
}

impl ApiKeyAuthHook {
    pub fn new(valid_keys: Vec<String>) -> Self {
        Self { valid_keys }
    }
}

impl AuthHook for ApiKeyAuthHook {
    fn authenticate(&self, req: &Request<Body>) -> Result<AuthContext, Response> {
        // Extract API key from header
        let api_key = req
            .headers()
            .get("x-api-key")
            .and_then(|h| h.to_str().ok());

        if let Some(key) = api_key {
            if self.valid_keys.contains(&key.to_string()) {
                return Ok(AuthContext::authenticated(
                    key.to_string(),
                    vec!["api_client".to_string()],
                ));
            }
        }

        Ok(AuthContext::unauthenticated())
    }
}

/// No-op authentication hook (allows all requests)
pub struct NoAuthHook;

impl AuthHook for NoAuthHook {
    fn authenticate(&self, _req: &Request<Body>) -> Result<AuthContext, Response> {
        Ok(AuthContext::unauthenticated())
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    auth_hook: Arc<dyn AuthHook>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    match auth_hook.authenticate(&req) {
        Ok(auth_context) => {
            // Store auth context in request extensions for downstream handlers
            req.extensions_mut().insert(auth_context);
            next.run(req).await
        }
        Err(error_response) => error_response,
    }
}

/// Require authentication middleware
///
/// Returns 401 if request is not authenticated
pub async fn require_auth_middleware(
    req: Request<Body>,
    next: Next,
) -> Response {
    // Extract auth context from extensions
    let auth_context = req.extensions().get::<AuthContext>().cloned();

    match auth_context {
        Some(ctx) if ctx.is_authenticated => {
            // User is authenticated, proceed
            next.run(req).await
        }
        _ => {
            // Not authenticated
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": {
                        "code": "UNAUTHORIZED",
                        "message": "Authentication required"
                    }
                })),
            )
                .into_response()
        }
    }
}

/// Require specific role middleware
pub fn require_role_middleware(required_role: String) -> impl Fn(Request<Body>, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone {
    move |req: Request<Body>, next: Next| {
        let role = required_role.clone();
        Box::pin(async move {
            let auth_context = req.extensions().get::<AuthContext>().cloned();

            match auth_context {
                Some(ctx) if ctx.has_role(&role) => {
                    // User has required role
                    next.run(req).await
                }
                Some(_) => {
                    // Authenticated but missing role
                    (
                        StatusCode::FORBIDDEN,
                        Json(json!({
                            "error": {
                                "code": "FORBIDDEN",
                                "message": format!("Role '{}' required", role)
                            }
                        })),
                    )
                        .into_response()
                }
                None => {
                    // Not authenticated
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({
                            "error": {
                                "code": "UNAUTHORIZED",
                                "message": "Authentication required"
                            }
                        })),
                    )
                        .into_response()
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
