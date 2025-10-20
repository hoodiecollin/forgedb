use forgedb_http_server::*;
use std::time::Duration;

#[test]
fn test_cache_basic() {
    let cache = ResponseCache::default();
    let key = CacheKey::new("GET", "/api/users", "");

    // Initially empty
    assert!(cache.get(&key).is_none());

    // Set entry
    cache.set(key.clone(), b"test data".to_vec(), "application/json".to_string(), 200);

    // Should be cached
    let entry = cache.get(&key).unwrap();
    assert_eq!(entry.data, b"test data");
    assert_eq!(entry.status, 200);
}

#[test]
fn test_cache_expiration() {
    let cache = ResponseCache::default();
    let key = CacheKey::new("GET", "/api/test", "");

    // Set with very short TTL
    cache.set_with_ttl(
        key.clone(),
        b"test".to_vec(),
        "text/plain".to_string(),
        200,
        Duration::from_millis(1),
    );

    // Wait for expiration
    std::thread::sleep(Duration::from_millis(10));

    // Should be expired and removed
    assert!(cache.get(&key).is_none());
}

#[test]
fn test_cache_invalidate() {
    let cache = ResponseCache::default();
    let key = CacheKey::new("GET", "/api/users/1", "");

    cache.set(key.clone(), b"user data".to_vec(), "application/json".to_string(), 200);
    assert!(cache.get(&key).is_some());

    // Invalidate
    cache.invalidate(&key);
    assert!(cache.get(&key).is_none());
}

#[test]
fn test_cache_disabled() {
    let config = CacheConfig {
        enabled: false,
        ..Default::default()
    };
    let cache = ResponseCache::new(config);
    let key = CacheKey::new("GET", "/api/test", "");

    // Set shouldn't cache when disabled
    cache.set(key.clone(), b"test".to_vec(), "text/plain".to_string(), 200);
    assert!(cache.get(&key).is_none());
}

#[test]
fn test_cache_stats() {
    let cache = ResponseCache::default();

    for i in 0..5 {
        let key = CacheKey::new("GET", format!("/api/users/{}", i), "");
        cache.set(key, b"data".to_vec(), "application/json".to_string(), 200);
    }

    let stats = cache.stats();
    assert_eq!(stats.total_entries, 5);
    assert_eq!(stats.active_entries, 5);
}
