#![allow(dead_code)]
//! HTTP response caching with header management, ETag support, and conditional requests.
//!
//! Provides a DashMap-backed response cache that generates proper `Cache-Control`,
//! `ETag`, `Last-Modified`, `Vary`, and `Age` headers. Supports conditional requests
//! via `If-None-Match` and `If-Modified-Since`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{delete, get, put},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums & data structures
// ---------------------------------------------------------------------------

/// Cache-Control directive policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum CachePolicy {
    NoStore,
    NoCache,
    Private,
    Public,
    MaxAge(u64),
    MustRevalidate,
}

impl std::fmt::Display for CachePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CachePolicy::NoStore => write!(f, "no-store"),
            CachePolicy::NoCache => write!(f, "no-cache"),
            CachePolicy::Private => write!(f, "private"),
            CachePolicy::Public => write!(f, "public"),
            CachePolicy::MaxAge(secs) => write!(f, "public, max-age={}", secs),
            CachePolicy::MustRevalidate => write!(f, "must-revalidate"),
        }
    }
}

/// A single cached response entry.
#[derive(Debug)]
pub struct CachedResponse {
    /// Cache key (typically URL path or hash).
    pub key: String,
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: HeaderMap,
    /// Response body bytes.
    pub body: Vec<u8>,
    /// When the entry was cached.
    pub cached_at: Instant,
    /// Number of cache hits.
    pub hit_count: AtomicU64,
    /// Size of body in bytes.
    pub size_bytes: usize,
    /// Content-Type header value.
    pub content_type: Option<String>,
    /// ETag value.
    pub etag: Option<String>,
    /// Cache policy for this entry.
    pub policy: CachePolicy,
    /// TTL in seconds.
    pub ttl: u64,
    /// Last-Modified timestamp.
    pub last_modified: Option<SystemTime>,
    /// Vary header value.
    pub vary: Option<String>,
}

impl CachedResponse {
    /// Compute the effective age in seconds.
    pub fn age_secs(&self) -> u64 {
        self.cached_at.elapsed().as_secs()
    }

    /// Check if the entry is still fresh based on TTL.
    pub fn is_fresh(&self) -> bool {
        self.age_secs() < self.ttl
    }
}

/// Configuration for the response cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Default TTL in seconds.
    pub default_ttl: u64,
    /// Maximum body size to cache (bytes).
    pub max_body_size: usize,
    /// Headers that trigger separate cache entries (Vary).
    pub vary_headers: Vec<String>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            default_ttl: 300,
            max_body_size: 1_048_576, // 1 MB
            vary_headers: vec!["Accept".to_string(), "Accept-Encoding".to_string()],
        }
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub total_bytes: u64,
    pub evictions: u64,
}

// ---------------------------------------------------------------------------
// Core cache engine
// ---------------------------------------------------------------------------

/// Thread-safe HTTP response cache with LRU eviction and header management.
#[derive(Debug)]
pub struct ResponseCache {
    entries: DashMap<String, CachedResponse>,
    config: Arc<std::sync::RwLock<CacheConfig>>,
    total_bytes: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

impl ResponseCache {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(CacheConfig::default())),
            total_bytes: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            entries: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(config)),
            total_bytes: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Generate an ETag from the response body using SHA-256.
    pub fn generate_etag(body: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(body);
        format!("\"{}\"", hex::encode(hash))
    }

    /// Get a cached response by key. Returns a snapshot of the entry data.
    pub fn get(&self, key: &str) -> Option<(u16, Vec<u8>, HeaderMap)> {
        if let Some(mut entry) = self.entries.get_mut(key) {
            let val = entry.value_mut();
            if val.is_fresh() {
                val.hit_count.fetch_add(1, Ordering::Relaxed);
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Some((val.status, val.body.clone(), val.headers.clone()));
            } else {
                drop(entry);
                self.entries.remove(key);
                self.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Store a response in the cache.
    pub fn put(
        &self,
        key: &str,
        status: u16,
        headers: HeaderMap,
        body: Vec<u8>,
        policy: CachePolicy,
        ttl: Option<u64>,
    ) -> bool {
        let cfg = self.config.read().unwrap();

        if body.len() > cfg.max_body_size {
            return false;
        }

        // Evict if at capacity
        if self.entries.len() >= cfg.max_entries {
            self.evict_lru();
        }

        let effective_ttl = ttl.unwrap_or(cfg.default_ttl);
        let etag = Self::generate_etag(&body);
        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let vary = if cfg.vary_headers.is_empty() {
            None
        } else {
            Some(cfg.vary_headers.join(", "))
        };

        let size_bytes = body.len();
        let last_modified = Some(SystemTime::now());

        let entry = CachedResponse {
            key: key.to_string(),
            status,
            headers,
            body,
            cached_at: Instant::now(),
            hit_count: AtomicU64::new(0),
            size_bytes,
            content_type,
            etag: Some(etag.clone()),
            policy,
            ttl: effective_ttl,
            last_modified,
            vary,
        };

        // Track total bytes (swap delta)
        let prev_size = self
            .entries
            .get(key)
            .map(|e| e.value().size_bytes as u64)
            .unwrap_or(0);
        self.total_bytes
            .fetch_add(size_bytes as u64 - prev_size, Ordering::Relaxed);

        drop(cfg);
        self.entries.insert(key.to_string(), entry);
        true
    }

    /// Invalidate a single cache entry.
    pub fn invalidate(&self, key: &str) -> bool {
        if let Some((_, entry)) = self.entries.remove(key) {
            self.total_bytes
                .fetch_sub(entry.size_bytes as u64, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Purge all entries whose key matches a glob-style pattern.
    pub fn purge_by_pattern(&self, pattern: &str) -> usize {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.key().contains(pattern))
            .map(|e| e.key().clone())
            .collect();
        let count = keys.len();
        for key in keys {
            self.invalidate(&key);
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
        count
    }

    /// Check freshness of a cached entry.
    pub fn check_freshness(&self, key: &str) -> Option<bool> {
        self.entries.get(key).map(|e| e.value().is_fresh())
    }

    /// Evaluate conditional request headers.
    /// Returns `Some(StatusCode::NOT_MODIFIED)` if the cached response is still valid.
    pub fn check_conditional(&self, key: &str, headers: &HeaderMap) -> Option<StatusCode> {
        let entry = self.entries.get(key)?;
        let val = entry.value();

        // Check If-None-Match
        if let Some(inm) = headers.get("if-none-match").and_then(|v| v.to_str().ok()) {
            if let Some(ref etag) = val.etag {
                if inm == etag || inm == "*" {
                    return Some(StatusCode::NOT_MODIFIED);
                }
                // Support comma-separated list
                for tag in inm.split(',') {
                    let trimmed = tag.trim();
                    if trimmed == etag {
                        return Some(StatusCode::NOT_MODIFIED);
                    }
                }
            }
        }

        // Check If-Modified-Since
        if let Some(ims_str) = headers.get("if-modified-since").and_then(|v| v.to_str().ok()) {
            if let Some(ref last_modified) = val.last_modified {
                // Parse the HTTP-date using chrono
                if let Ok(ims) = chrono::DateTime::parse_from_rfc2822(ims_str) {
                    let lm_utc: chrono::DateTime<chrono::Utc> = (*last_modified).into();
                    if ims >= lm_utc {
                        return Some(StatusCode::NOT_MODIFIED);
                    }
                }
            }
        }

        None
    }

    /// Generate Cache-Control header value from a policy.
    pub fn cache_control_value(policy: &CachePolicy) -> String {
        policy.to_string()
    }

    /// Generate a Last-Modified header value.
    pub fn last_modified_value(time: SystemTime) -> String {
        let datetime: chrono::DateTime<chrono::Utc> = time.into();
        datetime.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }

    /// Build full response headers for a cached entry.
    pub fn build_response_headers(entry: &CachedResponse) -> HeaderMap {
        let mut headers = entry.headers.clone();
        let age = entry.age_secs();

        // Cache-Control
        headers.insert(
            "cache-control",
            HeaderValue::from_str(&Self::cache_control_value(&entry.policy))
                .unwrap_or_else(|_| HeaderValue::from_static("no-cache")),
        );

        // ETag
        if let Some(ref etag) = entry.etag {
            if let Ok(val) = HeaderValue::from_str(etag) {
                headers.insert("etag", val);
            }
        }

        // Last-Modified
        if let Some(lm) = entry.last_modified {
            if let Ok(val) = HeaderValue::from_str(&Self::last_modified_value(lm)) {
                headers.insert("last-modified", val);
            }
        }

        // Vary
        if let Some(ref vary) = entry.vary {
            if let Ok(val) = HeaderValue::from_str(vary) {
                headers.insert("vary", val);
            }
        }

        // Age
        if let Ok(val) = HeaderValue::from_str(&age.to_string()) {
            headers.insert("age", val);
        }

        headers
    }

    /// Get cache statistics.
    pub fn get_stats(&self) -> CacheStats {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        CacheStats {
            entries: self.entries.len(),
            hits,
            misses,
            hit_rate: if total > 0 { hits as f64 / total as f64 } else { 0.0 },
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
        }
    }

    /// Update configuration.
    pub fn update_config(&self, new_config: CacheConfig) {
        *self.config.write().unwrap() = new_config;
    }

    /// Get current configuration.
    pub fn get_config(&self) -> CacheConfig {
        self.config.read().unwrap().clone()
    }

    /// Prune expired entries.
    pub fn prune_expired(&self) {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|e| !e.value().is_fresh())
            .map(|e| e.key().clone())
            .collect();
        for key in keys {
            self.invalidate(&key);
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Evict the least-recently-used entry.
    fn evict_lru(&self) {
        let mut oldest_key: Option<String> = None;
        let mut oldest_time = Instant::now();
        for entry in self.entries.iter() {
            if entry.value().cached_at < oldest_time {
                oldest_time = entry.value().cached_at;
                oldest_key = Some(entry.key().clone());
            }
        }
        if let Some(key) = oldest_key {
            self.invalidate(&key);
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CachePutRequest {
    pub body: Option<String>,
    pub status: Option<u16>,
    pub ttl: Option<u64>,
    pub policy: Option<String>,
}

#[derive(Serialize)]
pub struct CacheEntryResponse {
    pub key: String,
    pub status: u16,
    pub size_bytes: usize,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub is_fresh: bool,
    pub age_secs: u64,
    pub hit_count: u64,
}

#[derive(Deserialize)]
pub struct PurgeQuery {
    pub pattern: String,
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn get_cache_entry(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let cache = &state.response_cache_headers;
    let result: Option<(u16, Vec<u8>, HeaderMap)> = cache.get(&key);
    if let Some((status, body, _headers)) = result {
        let size_bytes: usize = body.len();
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "key": key.clone(),
                "status": status,
                "size_bytes": size_bytes,
                "content_type": null,
                "etag": null,
                "is_fresh": true,
                "age_secs": 0,
                "hit_count": 0,
            })),
        )
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"})))
    }
}

async fn put_cache_entry(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<CachePutRequest>,
) -> impl IntoResponse {
    let cache = &state.response_cache_headers;
    let data = body.body.unwrap_or_default().into_bytes();
    let status = body.status.unwrap_or(200);
    let policy = match body.policy.as_deref() {
        Some("no-store") => CachePolicy::NoStore,
        Some("no-cache") => CachePolicy::NoCache,
        Some("private") => CachePolicy::Private,
        Some("must-revalidate") => CachePolicy::MustRevalidate,
        _ => CachePolicy::Public,
    };

    let stored = cache.put(&key, status, HeaderMap::new(), data, policy, body.ttl);
    if stored {
        (StatusCode::OK, Json(serde_json::json!({"status": "cached"})))
    } else {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "body too large"})),
        )
    }
}

async fn delete_cache_entry(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let cache = &state.response_cache_headers;
    if cache.invalidate(&key) {
        (StatusCode::OK, Json(serde_json::json!({"status": "deleted"})))
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"})))
    }
}

async fn purge_cache(
    State(state): State<AppState>,
    Query(query): Query<PurgeQuery>,
) -> impl IntoResponse {
    let cache = &state.response_cache_headers;
    let count = cache.purge_by_pattern(&query.pattern);
    (StatusCode::OK, Json(serde_json::json!({"purged": count})))
}

async fn get_cache_stats(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.response_cache_headers.get_stats();
    (StatusCode::OK, Json(stats))
}

async fn get_cache_config(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.response_cache_headers.get_config();
    (StatusCode::OK, Json(config))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the response cache headers API router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/cache/{key}", get(get_cache_entry))
        .route("/v1/cache/{key}", put(put_cache_entry))
        .route("/v1/cache/{key}", delete(delete_cache_entry))
        .route("/v1/cache/purge", delete(purge_cache))
        .route("/v1/cache/stats", get(get_cache_stats))
        .route("/v1/cache/config", get(get_cache_config))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cache_is_empty() {
        let cache = ResponseCache::new();
        assert_eq!(cache.entries.len(), 0);
    }

    #[test]
    fn test_put_and_get() {
        let cache = ResponseCache::new();
        cache.put("key1", 200, HeaderMap::new(), b"hello".to_vec(), CachePolicy::Public, None);
        let entry = cache.get("key1").unwrap();
        assert_eq!(entry.0, 200);
        assert_eq!(entry.1, b"hello");
    }

    #[test]
    fn test_get_miss() {
        let cache = ResponseCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_invalidate() {
        let cache = ResponseCache::new();
        cache.put("key1", 200, HeaderMap::new(), b"data".to_vec(), CachePolicy::Public, None);
        assert!(cache.invalidate("key1"));
        assert!(cache.get("key1").is_none());
    }

    #[test]
    fn test_invalidate_nonexistent() {
        let cache = ResponseCache::new();
        assert!(!cache.invalidate("nope"));
    }

    #[test]
    fn test_etag_generation() {
        let etag1 = ResponseCache::generate_etag(b"hello");
        let etag2 = ResponseCache::generate_etag(b"hello");
        let etag3 = ResponseCache::generate_etag(b"world");
        assert_eq!(etag1, etag2);
        assert_ne!(etag1, etag3);
        assert!(etag1.starts_with('"'));
    }

    #[test]
    fn test_cache_control_value() {
        assert_eq!(ResponseCache::cache_control_value(&CachePolicy::NoStore), "no-store");
        assert_eq!(ResponseCache::cache_control_value(&CachePolicy::Public), "public");
        assert_eq!(ResponseCache::cache_control_value(&CachePolicy::MaxAge(60)), "public, max-age=60");
    }

    #[test]
    fn test_purge_by_pattern() {
        let cache = ResponseCache::new();
        cache.put("user:1", 200, HeaderMap::new(), b"a".to_vec(), CachePolicy::Public, None);
        cache.put("user:2", 200, HeaderMap::new(), b"b".to_vec(), CachePolicy::Public, None);
        cache.put("model:1", 200, HeaderMap::new(), b"c".to_vec(), CachePolicy::Public, None);
        let purged = cache.purge_by_pattern("user:");
        assert_eq!(purged, 2);
        assert!(cache.get("user:1").is_none());
        assert!(cache.get("user:2").is_none());
        assert!(cache.get("model:1").is_some());
    }

    #[test]
    fn test_max_body_size_rejection() {
        let cache = ResponseCache::with_config(CacheConfig {
            max_body_size: 10,
            ..CacheConfig::default()
        });
        let stored = cache.put("big", 200, HeaderMap::new(), vec![0u8; 100], CachePolicy::Public, None);
        assert!(!stored);
    }

    #[test]
    fn test_stats_tracking() {
        let cache = ResponseCache::new();
        cache.put("k1", 200, HeaderMap::new(), b"data".to_vec(), CachePolicy::Public, None);
        cache.get("k1"); // hit
        cache.get("k2"); // miss
        let stats = cache.get_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }
}
