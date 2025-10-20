use forgedb_http_server::*;
use forgedb_http_server::rate_limit::TokenBucket;

#[test]
fn test_token_bucket_basic() {
    let mut bucket = TokenBucket::new(5, 60);

    // Should be able to consume 5 tokens
    for _ in 0..5 {
        assert!(bucket.try_consume());
    }

    // 6th request should fail
    assert!(!bucket.try_consume());
}

#[test]
fn test_token_bucket_refill() {
    let mut bucket = TokenBucket::new(10, 1); // 10 tokens per second

    // Consume all tokens
    for _ in 0..10 {
        assert!(bucket.try_consume());
    }

    // Should fail immediately
    assert!(!bucket.try_consume());

    // Wait a bit for refill (in real scenario)
    // Can't easily test time-based refill in unit test
}

#[test]
fn test_rate_limiter() {
    let config = RateLimitConfig {
        max_requests: 3,
        window_secs: 60,
        enabled: true,
    };
    let limiter = RateLimiter::new(config);

    // First 3 requests should succeed
    for _ in 0..3 {
        assert!(limiter.check_rate_limit("test-client").is_ok());
    }

    // 4th request should fail
    assert!(limiter.check_rate_limit("test-client").is_err());
}

#[test]
fn test_rate_limiter_disabled() {
    let config = RateLimitConfig {
        max_requests: 1,
        window_secs: 60,
        enabled: false,
    };
    let limiter = RateLimiter::new(config);

    // All requests should succeed when disabled
    for _ in 0..100 {
        assert!(limiter.check_rate_limit("test-client").is_ok());
    }
}

#[test]
fn test_rate_limiter_per_client() {
    let config = RateLimitConfig {
        max_requests: 2,
        window_secs: 60,
        enabled: true,
    };
    let limiter = RateLimiter::new(config);

    // Client 1: 2 requests OK
    assert!(limiter.check_rate_limit("client1").is_ok());
    assert!(limiter.check_rate_limit("client1").is_ok());
    assert!(limiter.check_rate_limit("client1").is_err());

    // Client 2: still has 2 requests available
    assert!(limiter.check_rate_limit("client2").is_ok());
    assert!(limiter.check_rate_limit("client2").is_ok());
    assert!(limiter.check_rate_limit("client2").is_err());
}
