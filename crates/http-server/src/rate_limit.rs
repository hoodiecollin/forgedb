//! Rate limiting middleware using token bucket algorithm

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use dashmap::DashMap;
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed
    pub max_requests: usize,
    /// Time window in seconds
    pub window_secs: u64,
    /// Enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,      // 100 requests
            window_secs: 60,         // per minute
            enabled: true,
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
    pub fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Check if request is allowed for the given key (typically IP address)
    pub fn check_rate_limit(&self, key: &str) -> Result<(), Duration> {
        if !self.config.enabled {
            return Ok(());
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

    /// Get configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }
}

/// Extract client identifier from request (IP address)
fn get_client_id(req: &Request<Body>) -> String {
    // Try to get real IP from X-Forwarded-For header
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(ip) = forwarded_str.split(',').next() {
                return ip.trim().to_string();
            }
        }
    }

    // Try X-Real-IP header
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            return ip_str.to_string();
        }
    }

    // Fallback to connection IP (not available in Axum easily, use placeholder)
    "unknown".to_string()
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    limiter: Arc<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let client_id = get_client_id(&req);

    match limiter.check_rate_limit(&client_id) {
        Ok(()) => {
            // Request allowed
            next.run(req).await
        }
        Err(retry_after) => {
            // Rate limit exceeded
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
