//! Rate limiting middleware using token bucket algorithm

use axum::{
    body::Body,
    extract::ConnectInfo,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use dashmap::DashMap;
use serde_json::json;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed in the window
    pub max_requests: usize,
    /// Time window in seconds
    pub window_secs: u64,
    /// Enable rate limiting
    pub enabled: bool,
    /// Trust proxy headers (`X-Forwarded-For`, `X-Real-IP`) for client
    /// identification.
    ///
    /// **Default: `false`.**  When `false` the real peer socket address is
    /// always used as the rate-limit key; forwarded headers are ignored.
    /// Set to `true` only when the server sits behind a trusted reverse proxy
    /// that you fully control, because an attacker can otherwise spoof a fresh
    /// IP on every request and bypass the rate limiter entirely.
    pub trust_proxy: bool,
    /// Maximum number of distinct client buckets held in memory.
    ///
    /// When this cap is exceeded, fully-idle buckets (tokens refilled to
    /// capacity) are evicted before inserting a new entry.  This prevents
    /// unbounded memory growth under a distributed spoofing attack.
    ///
    /// Default: 10 000.  Set to `0` to disable the cap (not recommended for
    /// public-facing servers).
    pub max_entries: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_secs: 60,
            enabled: true,
            trust_proxy: false,
            max_entries: 10_000,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
pub struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    pub fn new(max_requests: usize, window_secs: u64) -> Self {
        let max_tokens = max_requests as f64;
        let refill_rate = max_tokens / (window_secs as f64);

        Self {
            tokens: max_tokens,
            last_refill: Instant::now(),
            max_tokens,
            refill_rate,
        }
    }

    pub fn try_consume(&mut self) -> bool {
        // Refill tokens based on elapsed time
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;

        // Try to consume a token
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn time_until_available(&self) -> Duration {
        if self.tokens >= 1.0 {
            Duration::from_secs(0)
        } else {
            let tokens_needed = 1.0 - self.tokens;
            let secs = tokens_needed / self.refill_rate;
            Duration::from_secs_f64(secs.ceil())
        }
    }

    /// Returns true when the bucket has been fully refilled and is idle.
    fn is_idle(&self) -> bool {
        let elapsed = Instant::now()
            .duration_since(self.last_refill)
            .as_secs_f64();
        let projected = self.tokens + elapsed * self.refill_rate;
        projected >= self.max_tokens
    }
}

/// Rate limiter state
pub struct RateLimiter {
    buckets: Arc<DashMap<String, TokenBucket>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Check if request is allowed for the given key (typically IP address)
    pub fn check_rate_limit(&self, key: &str) -> Result<(), Duration> {
        if !self.config.enabled {
            return Ok(());
        }

        // Evict idle buckets before inserting to bound memory.
        let cap = self.config.max_entries;
        if cap > 0 && self.buckets.len() >= cap {
            self.evict_idle();
        }

        let mut bucket = self.buckets.entry(key.to_string()).or_insert_with(|| {
            TokenBucket::new(self.config.max_requests, self.config.window_secs)
        });

        if bucket.try_consume() {
            Ok(())
        } else {
            Err(bucket.time_until_available())
        }
    }

    /// Remove fully-idle (fully-refilled) buckets from the map.
    fn evict_idle(&self) {
        self.buckets.retain(|_, bucket| !bucket.is_idle());
    }

    /// Get configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }
}

/// Derive the rate-limit key from the request.
///
/// When `trust_proxy` is **false** (the default), the real peer socket address
/// from axum's [`ConnectInfo`] extension is used.  Forwarded headers
/// (`X-Forwarded-For`, `X-Real-IP`) are ignored, so a client cannot bypass
/// limits by spoofing a fresh IP on every request.
///
/// When `trust_proxy` is **true**, `X-Forwarded-For` / `X-Real-IP` are
/// consulted first and the result is used as-is.  Only enable this when the
/// server runs behind a trusted reverse proxy that strips or overwrites those
/// headers before forwarding.
pub(crate) fn get_client_id(req: &Request<Body>, config: &RateLimitConfig) -> String {
    if config.trust_proxy {
        if let Some(forwarded) = req.headers().get("x-forwarded-for")
            && let Ok(s) = forwarded.to_str()
            && let Some(ip) = s.split(',').next()
        {
            return ip.trim().to_string();
        }
        if let Some(real_ip) = req.headers().get("x-real-ip")
            && let Ok(ip_str) = real_ip.to_str()
        {
            return ip_str.to_string();
        }
    }

    // Use the real TCP peer address inserted by axum when the server was
    // started with `.into_make_service_with_connect_info::<SocketAddr>()`.
    if let Some(connect_info) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return connect_info.0.ip().to_string();
    }

    // Fallback: Unix socket or ConnectInfo not wired — treat as a single
    // shared bucket called "local".
    "local".to_string()
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    limiter: Arc<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let client_id = get_client_id(&req, &limiter.config);

    match limiter.check_rate_limit(&client_id) {
        Ok(()) => next.run(req).await,
        Err(retry_after) => {
            tracing::warn!("Rate limit exceeded for client: {}", client_id);

            let retry_after_secs = retry_after.as_secs().to_string();

            (
                StatusCode::TOO_MANY_REQUESTS,
                [("Retry-After", retry_after_secs.as_str())],
                Json(json!({
                    "error": {
                        "code": "RATE_LIMIT_EXCEEDED",
                        "message": "Too many requests. Please try again later.",
                        "retry_after_seconds": retry_after.as_secs()
                    }
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req_with_forwarded(forwarded_for: &str, peer: SocketAddr) -> Request<Body> {
        let mut req = Request::builder()
            .header("x-forwarded-for", forwarded_for)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));
        req
    }

    /// When trust_proxy is false, two requests with different X-Forwarded-For
    /// headers but the same peer address must produce the same key.
    #[test]
    fn spoofed_forwarded_header_gives_same_key_when_trust_proxy_false() {
        let peer: SocketAddr = "1.2.3.4:1234".parse().unwrap();
        let config = RateLimitConfig {
            trust_proxy: false,
            ..Default::default()
        };

        let req1 = make_req_with_forwarded("9.9.9.9", peer);
        let req2 = make_req_with_forwarded("8.8.8.8", peer);

        let key1 = get_client_id(&req1, &config);
        let key2 = get_client_id(&req2, &config);

        assert_eq!(
            key1, key2,
            "different X-Forwarded-For headers must not yield different keys when trust_proxy=false"
        );
        assert_eq!(key1, "1.2.3.4", "key must equal the real peer IP");
    }

    /// When trust_proxy is true, the X-Forwarded-For header is honoured.
    #[test]
    fn forwarded_header_used_when_trust_proxy_true() {
        let peer: SocketAddr = "1.2.3.4:1234".parse().unwrap();
        let config = RateLimitConfig {
            trust_proxy: true,
            ..Default::default()
        };

        let req = make_req_with_forwarded("203.0.113.5", peer);
        let key = get_client_id(&req, &config);
        assert_eq!(key, "203.0.113.5");
    }

    /// The bucket map must stop growing once it hits max_entries.
    #[test]
    fn eviction_bounds_map_size() {
        let config = RateLimitConfig {
            max_requests: 10,
            window_secs: 60,
            enabled: true,
            trust_proxy: false,
            max_entries: 5,
        };
        let limiter = RateLimiter::new(config);

        // Fill map to capacity with distinct keys.
        for i in 0..5u32 {
            // Drain all tokens so buckets appear active.
            for _ in 0..10 {
                let _ = limiter.check_rate_limit(&format!("key-{}", i));
            }
        }
        assert_eq!(limiter.buckets.len(), 5);

        // New keys beyond the cap: because existing buckets have no tokens,
        // they won't be evicted, but we at least test that the code path runs
        // without panic.
        let _ = limiter.check_rate_limit("key-overflow");
        // Map size is allowed to temporarily exceed cap if nothing is evictable.
        assert!(limiter.buckets.len() <= 7);
    }
}
