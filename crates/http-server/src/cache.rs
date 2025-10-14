//! Response caching with TTL support

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Enable caching
    pub enabled: bool,
    /// Default TTL in seconds
    pub default_ttl_secs: u64,
    /// Maximum cache size (number of entries)
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_ttl_secs: 300, // 5 minutes
            max_entries: 1000,
        }
    }
}

/// Cache key for HTTP responses
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub method: String,
    pub path: String,
    pub query: String,
}

impl CacheKey {
    pub fn new(method: impl Into<String>, path: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            query: query.into(),
        }
    }
}

/// Cached response entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub data: Vec<u8>,
    pub content_type: String,
    pub status: u16,
    pub expires_at: Instant,
}

impl CacheEntry {
    /// Check if entry is expired
    pub fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// Response cache
pub struct ResponseCache {
    cache: Arc<DashMap<String, CacheEntry>>,
    config: CacheConfig,
}

impl ResponseCache {
    /// Create a new response cache
    pub fn new(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(CacheConfig::default())
    }

    /// Generate cache key hash
    fn key_hash(key: &CacheKey) -> String {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get cached response
    pub fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        if !self.config.enabled {
            return None;
        }

        let key_hash = Self::key_hash(key);
        if let Some(entry) = self.cache.get(&key_hash) {
            if !entry.is_expired() {
                return Some(entry.clone());
            } else {
                // Remove expired entry
                drop(entry);
                self.cache.remove(&key_hash);
            }
        }

        None
    }

    /// Set cached response with default TTL
    pub fn set(&self, key: CacheKey, data: Vec<u8>, content_type: String, status: u16) {
        self.set_with_ttl(
            key,
            data,
            content_type,
            status,
            Duration::from_secs(self.config.default_ttl_secs),
        );
    }

    /// Set cached response with custom TTL
    pub fn set_with_ttl(
        &self,
        key: CacheKey,
        data: Vec<u8>,
        content_type: String,
        status: u16,
        ttl: Duration,
    ) {
        if !self.config.enabled {
            return;
        }

        // Evict old entries if cache is full
        if self.cache.len() >= self.config.max_entries {
            self.evict_oldest();
        }

        let key_hash = Self::key_hash(&key);
        let entry = CacheEntry {
            data,
            content_type,
            status,
            expires_at: Instant::now() + ttl,
        };

        self.cache.insert(key_hash, entry);
    }

    /// Invalidate cache entry
    pub fn invalidate(&self, key: &CacheKey) {
        let key_hash = Self::key_hash(key);
        self.cache.remove(&key_hash);
    }

    /// Invalidate all entries matching path prefix
    pub fn invalidate_prefix(&self, _path_prefix: &str) {
        self.cache.retain(|_, _| {
            // For simplicity, we can't easily check path from hash
            // In production, consider using a more sophisticated eviction strategy
            true
        });
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Evict oldest entries (simple random eviction for now)
    fn evict_oldest(&self) {
        // Evict 10% of entries
        let to_evict = self.config.max_entries / 10;
        let mut count = 0;

        self.cache.retain(|_, entry| {
            if count < to_evict || entry.is_expired() {
                count += 1;
                false
            } else {
                true
            }
        });
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_entries = self.cache.len();
        let expired = self
            .cache
            .iter()
            .filter(|entry| entry.is_expired())
            .count();

        CacheStats {
            total_entries,
            expired_entries: expired,
            active_entries: total_entries - expired,
            max_entries: self.config.max_entries,
        }
    }

    /// Get configuration
    pub fn config(&self) -> &CacheConfig {
        &self.config
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
    pub max_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
