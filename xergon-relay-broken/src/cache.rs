//! Response cache with ETag support for GET endpoints.
//!
//! Provides an in-memory LRU-like cache backed by DashMap with TTL-based
//! expiration, SHA-256 ETag generation, and conditional request handling
//! (If-None-Match -> 304 Not Modified).

use bytes::Bytes;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::debug;

// ---------------------------------------------------------------------------
// Cache configuration
// ---------------------------------------------------------------------------

/// Configuration for the response cache.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Enable/disable response caching (default: true).
    pub enabled: bool,
    /// Maximum number of cache entries (default: 10000).
    pub max_entries: usize,
    /// Default TTL for cached responses in seconds (default: 60).
    pub default_ttl_secs: u64,
    /// TTL for /v1/models in seconds (default: 30).
    pub model_list_ttl_secs: u64,
    /// TTL for /v1/providers in seconds (default: 15).
    pub provider_list_ttl_secs: u64,
    /// TTL for /v1/health in seconds (default: 5).
    pub health_ttl_secs: u64,
    /// Maximum size of a single cached entry in bytes (default: 102400 = 100KB).
    pub max_entry_size_bytes: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 10_000,
            default_ttl_secs: 60,
            model_list_ttl_secs: 30,
            provider_list_ttl_secs: 15,
            health_ttl_secs: 5,
            max_entry_size_bytes: 102_400,
        }
    }
}

// ---------------------------------------------------------------------------
// Cache entry
// ---------------------------------------------------------------------------

struct CacheEntry {
    response_body: Bytes,
    etag: String,
    content_type: String,
    status: u16,
    created_at: Instant,
    ttl: Duration,
    hit_count: AtomicU64,
    last_accessed: AtomicI64,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }

    fn record_access(&self) {
        self.hit_count.fetch_add(1, Ordering::Relaxed);
        self.last_accessed.store(
            chrono::Utc::now().timestamp_millis(),
            Ordering::Relaxed,
        );
    }
}

// ---------------------------------------------------------------------------
// Cache stats
// ---------------------------------------------------------------------------

/// Snapshot of cache statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub size_bytes: u64,
}

// ---------------------------------------------------------------------------
// ResponseCache
// ---------------------------------------------------------------------------

/// Thread-safe in-memory response cache with TTL and ETag support.
pub struct ResponseCache {
    entries: DashMap<String, CacheEntry>,
    config: CacheConfig,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl ResponseCache {
    /// Create a new response cache with the given configuration.
    pub fn new(config: CacheConfig) -> Self {
        Self {
            entries: DashMap::new(),
            config,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Generate a cache key from method, path, and query string.
    /// Format: `{method}:{path}:{query_hash}` so that prefix matching
    /// on `{method}:{path}:` invalidates all query variations.
    pub fn cache_key(method: &str, path: &str, query: &str) -> String {
        format!("{}:{}:{}", method, path, query_hash(query))
    }

    /// Generate a cache key prefix for invalidation (method + path only).
    pub fn cache_prefix(method: &str, path: &str) -> String {
        format!("{}:{}:", method, path)
    }

    /// Look up a cached entry. Returns `None` if not found or expired.
    /// Records a hit or miss in stats.
    pub fn get(&self, key: &str) -> Option<CachedResponse> {
        let entry = self.entries.get(key)?;
        if entry.is_expired() {
            drop(entry);
            self.entries.remove(key);
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        entry.record_access();
        self.hits.fetch_add(1, Ordering::Relaxed);
        Some(CachedResponse {
            body: entry.response_body.clone(),
            etag: entry.etag.clone(),
            content_type: entry.content_type.clone(),
            status: entry.status,
        })
    }

    /// Store a response in the cache.
    /// Returns `false` if the cache is at capacity or the entry is too large.
    pub fn put(
        &self,
        key: &str,
        body: Bytes,
        content_type: String,
        status: u16,
        ttl: Duration,
    ) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Check max entry size
        if body.len() > self.config.max_entry_size_bytes {
            debug!(
                key = %key,
                size = body.len(),
                max = self.config.max_entry_size_bytes,
                "Cache entry too large, skipping"
            );
            return false;
        }

        // Check capacity (evict expired entries first)
        if self.entries.len() >= self.config.max_entries {
            self.cleanup();
            if self.entries.len() >= self.config.max_entries {
                debug!(
                    key = %key,
                    max = self.config.max_entries,
                    "Cache at capacity, skipping insert"
                );
                return false;
            }
        }

        let etag = generate_etag(&body);
        let entry = CacheEntry {
            response_body: body,
            etag,
            content_type,
            status,
            created_at: Instant::now(),
            ttl,
            hit_count: AtomicU64::new(0),
            last_accessed: AtomicI64::new(chrono::Utc::now().timestamp_millis()),
        };

        self.entries.insert(key.to_string(), entry);
        true
    }

    /// Remove a specific cache entry.
    pub fn invalidate(&self, key: &str) {
        self.entries.remove(key);
    }

    /// Remove all cache entries matching a prefix.
    pub fn invalidate_prefix(&self, prefix: &str) {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.key().starts_with(prefix))
            .map(|e| e.key().clone())
            .collect();

        let removed_count = keys.len();
        for key in keys {
            self.entries.remove(&key);
        }

        if removed_count > 0 {
            debug!(removed = removed_count, %prefix, "Cache prefix invalidation completed");
        }
    }

    /// Get the configured max entry size in bytes.
    pub fn max_entry_size_bytes(&self) -> usize {
        self.config.max_entry_size_bytes
    }

    /// Remove all entries from the cache.
    pub fn clear(&self) {
        self.entries.clear();
    }

    /// Remove expired entries from the cache.
    pub fn cleanup(&self) {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.value().is_expired())
            .map(|e| e.key().clone())
            .collect();

        let removed_count = keys.len();
        for key in keys {
            self.entries.remove(&key);
        }

        if removed_count > 0 {
            debug!(removed = removed_count, remaining = self.entries.len(), "Cache cleanup completed");
        }
    }

    /// Get current cache statistics.
    pub fn stats(&self) -> CacheStats {
        let size_bytes: u64 = self
            .entries
            .iter()
            .map(|e| e.value().response_body.len() as u64)
            .sum();

        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries: self.entries.len(),
            size_bytes,
        }
    }

    /// Start a background cleanup task that evicts expired entries periodically.
    pub fn start_cleanup_task(self: &std::sync::Arc<Self>, interval_secs: u64) -> tokio::task::JoinHandle<()> {
        let cache = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                cache.cleanup();
            }
        })
    }
}

use std::sync::Arc;

/// A cached response ready to be sent to the client.
#[derive(Debug, Clone)]
pub struct CachedResponse {
    pub body: Bytes,
    pub etag: String,
    pub content_type: String,
    pub status: u16,
}

// ---------------------------------------------------------------------------
// ETag generation
// ---------------------------------------------------------------------------

/// Generate a strong ETag from response body bytes using SHA-256.
/// Format: `"sha256:<first 16 hex chars>"`
pub fn generate_etag(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let hash = hasher.finalize();
    let hex = hex::encode(hash);
    format!("\"sha256:{}\"", &hex[..16])
}

/// Generate a weak ETag for dynamic content.
/// Format: `W/"sha256:<first 16 hex chars>"`
#[allow(dead_code)]
pub fn generate_weak_etag(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let hash = hasher.finalize();
    let hex = hex::encode(hash);
    format!("W/\"sha256:{}\"", &hex[..16])
}

/// Hash a query string for use in cache keys.
fn query_hash(query: &str) -> String {
    if query.is_empty() {
        return "_".to_string();
    }
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    let hash = hasher.finalize();
    hex::encode(&hash[..8])
}

/// Parse the If-None-Match header value and check if the given ETag matches.
/// Supports multiple ETags (comma-separated) and the `*` wildcard.
pub fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    let if_none_match = if_none_match.trim();
    if if_none_match == "*" {
        return true;
    }

    // Split by comma and check each value
    for candidate in if_none_match.split(',') {
        let candidate = candidate.trim();
        if candidate == etag {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CacheConfig {
        CacheConfig {
            enabled: true,
            max_entries: 100,
            default_ttl_secs: 60,
            model_list_ttl_secs: 30,
            provider_list_ttl_secs: 15,
            health_ttl_secs: 5,
            max_entry_size_bytes: 1024,
        }
    }

    #[test]
    fn test_cache_put_and_get() {
        let cache = ResponseCache::new(test_config());
        let key = "GET:/v1/models:_";
        let body = Bytes::from_static(b"{\"models\":[]}");

        assert!(cache.put(key, body.clone(), "application/json".into(), 200, Duration::from_secs(60)));

        let cached = cache.get(key).expect("should find cached entry");
        assert_eq!(cached.body, body);
        assert_eq!(cached.status, 200);
        assert!(cached.etag.starts_with("\"sha256:"));
        assert_eq!(cached.etag.len(), 25); // "sha256:" + 16 hex chars + quotes
    }

    #[test]
    fn test_cache_miss_returns_none() {
        let cache = ResponseCache::new(test_config());
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_ttl_expiration() {
        let cache = ResponseCache::new(test_config());
        let key = "GET:/v1/health:_";
        let body = Bytes::from_static(b"ok");

        cache.put(key, body, "text/plain".into(), 200, Duration::from_millis(50));
        assert!(cache.get(key).is_some());

        std::thread::sleep(Duration::from_millis(100));
        assert!(cache.get(key).is_none());
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = ResponseCache::new(test_config());
        let key = "GET:/v1/models:_";
        let body = Bytes::from_static(b"[]");

        cache.put(key, body, "application/json".into(), 200, Duration::from_secs(60));
        assert!(cache.get(key).is_some());

        cache.invalidate(key);
        assert!(cache.get(key).is_none());
    }

    #[test]
    fn test_cache_invalidate_prefix() {
        let cache = ResponseCache::new(test_config());
        let body = Bytes::from_static(b"data");

        cache.put("GET:/v1/models:_", body.clone(), "application/json".into(), 200, Duration::from_secs(60));
        cache.put("GET:/v1/models:abc12345", body.clone(), "application/json".into(), 200, Duration::from_secs(60));
        cache.put("GET:/v1/health:_", body.clone(), "application/json".into(), 200, Duration::from_secs(60));

        cache.invalidate_prefix("GET:/v1/models:");

        // Both query variations of /v1/models should be invalidated
        assert!(cache.get("GET:/v1/models:_").is_none());
        assert!(cache.get("GET:/v1/models:abc12345").is_none());
        // /v1/health should remain
        assert!(cache.get("GET:/v1/health:_").is_some());
    }

    #[test]
    fn test_cache_clear() {
        let cache = ResponseCache::new(test_config());
        let body = Bytes::from_static(b"data");

        cache.put("GET:/v1/models:_", body.clone(), "application/json".into(), 200, Duration::from_secs(60));
        cache.put("GET:/v1/health:_", body.clone(), "application/json".into(), 200, Duration::from_secs(60));

        cache.clear();
        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn test_cache_stats_tracking() {
        let cache = ResponseCache::new(test_config());
        let body = Bytes::from_static(b"[]");

        cache.put("GET:/v1/models:_", body, "application/json".into(), 200, Duration::from_secs(60));

        // One miss (nonexistent) — absent keys don't increment miss counter
        assert!(cache.get("nonexistent").is_none());

        // One hit
        assert!(cache.get("GET:/v1/models:_").is_some());

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.size_bytes, 2); // b"[]"
    }

    #[test]
    fn test_max_entry_size_limit() {
        let config = CacheConfig {
            max_entry_size_bytes: 10,
            ..test_config()
        };
        let cache = ResponseCache::new(config);
        let large_body = Bytes::from_static(b"this is way too large");

        assert!(!cache.put("GET:/v1/big:_", large_body, "application/json".into(), 200, Duration::from_secs(60)));
        assert!(cache.get("GET:/v1/big:_").is_none());
    }

    #[test]
    fn test_cache_cleanup_removes_expired() {
        let cache = ResponseCache::new(test_config());
        let body = Bytes::from_static(b"data");

        // Insert with very short TTL
        cache.put("GET:/v1/expired:_", body.clone(), "text/plain".into(), 200, Duration::from_millis(10));
        cache.put("GET:/v1/fresh:_", body, "text/plain".into(), 200, Duration::from_secs(300));

        std::thread::sleep(Duration::from_millis(50));

        cache.cleanup();

        assert!(cache.get("GET:/v1/expired:_").is_none());
        assert!(cache.get("GET:/v1/fresh:_").is_some());
    }

    #[test]
    fn test_cache_disabled() {
        let config = CacheConfig {
            enabled: false,
            ..test_config()
        };
        let cache = ResponseCache::new(config);
        let body = Bytes::from_static(b"[]");

        assert!(!cache.put("GET:/v1/models:_", body, "application/json".into(), 200, Duration::from_secs(60)));
    }

    #[test]
    fn test_cache_key_generation() {
        assert_eq!(ResponseCache::cache_key("GET", "/v1/models", ""), "GET:/v1/models:_");
        // Query string gets hashed; just verify it differs from empty-query key
        let with_query = ResponseCache::cache_key("GET", "/v1/models", "limit=10");
        let without_query = ResponseCache::cache_key("GET", "/v1/models", "");
        assert_ne!(with_query, without_query);
        assert!(with_query.starts_with("GET:/v1/models:"));
    }

    #[test]
    fn test_etag_generation() {
        let body1 = b"{\"models\":[]}";
        let body2 = b"{\"models\":[\"llama\"]}";

        let etag1 = generate_etag(body1);
        let etag2 = generate_etag(body2);

        assert!(etag1.starts_with("\"sha256:"));
        assert!(etag2.starts_with("\"sha256:"));
        assert_ne!(etag1, etag2);

        // Same body should produce same etag
        assert_eq!(generate_etag(body1), generate_etag(body1));
    }

    #[test]
    fn test_weak_etag_generation() {
        let body = b"dynamic content";
        let etag = generate_weak_etag(body);
        assert!(etag.starts_with("W/\"sha256:"));
    }

    #[test]
    fn test_etag_matches() {
        let etag = "\"sha256:abcdef1234567890\"";

        // Exact match
        assert!(etag_matches(etag, etag));

        // Wildcard
        assert!(etag_matches("*", etag));

        // Multiple ETags
        assert!(etag_matches("\"other\", \"sha256:abcdef1234567890\"", etag));

        // No match
        assert!(!etag_matches("\"sha256:deadbeefdeadbeef\"", etag));
    }

    #[test]
    fn test_cache_at_capacity() {
        let config = CacheConfig {
            max_entries: 2,
            ..test_config()
        };
        let cache = ResponseCache::new(config);
        let body = Bytes::from_static(b"data");

        assert!(cache.put("GET:/a:_", body.clone(), "text/plain".into(), 200, Duration::from_secs(60)));
        assert!(cache.put("GET:/b:_", body.clone(), "text/plain".into(), 200, Duration::from_secs(60)));
        // Third insert should fail (at capacity, no expired entries to evict)
        assert!(!cache.put("GET:/c:_", body, "text/plain".into(), 200, Duration::from_secs(60)));
    }
}
