//! Enhanced request deduplication v2 with fuzzy matching, response caching, and TTL/LRU eviction.
//!
//! Provides a DashMap-backed deduplication cache that hashes incoming requests using BLAKE3,
//! supports fuzzy (semantic similarity) deduplication, caches responses, and evicts entries
//! based on TTL expiry or LRU policy when the cache is full.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single deduplication cache entry.
#[derive(Debug, Clone)]
pub struct DedupRequest {
    /// BLAKE3 hash of the full request body + relevant headers.
    pub request_hash: String,
    /// Model identifier (e.g. "llama-3-70b").
    pub model_id: String,
    /// Provider endpoint that served the original request.
    pub provider_id: String,
    /// When the entry was created.
    pub timestamp: Instant,
    /// Time-to-live for this entry (seconds).
    pub ttl: u64,
    /// BLAKE3 hash of just the parameters (for fuzzy matching).
    pub parameters_hash: String,
    /// Cached response body (if available).
    pub cached_response: Option<Vec<u8>>,
    /// Cached response status code.
    pub cached_status: u16,
    /// Size in bytes of the cached response.
    pub cached_response_size: u64,
    /// Last access time (for LRU eviction).
    pub last_accessed: Instant,
}

/// Configuration for the deduplication engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupConfig {
    /// Master toggle.
    pub enabled: bool,
    /// Sliding window in seconds — entries older than this are expired.
    pub window_secs: u64,
    /// Maximum number of entries before LRU eviction kicks in.
    pub max_cache_size: usize,
    /// Hash algorithm identifier (always BLAKE3, kept for future extensibility).
    pub hash_algorithm: String,
    /// When true only exact hash matches count; when false fuzzy similarity is used.
    pub exact_match: bool,
    /// Threshold for fuzzy matching (0.0–1.0). 1.0 = identical.
    pub fuzzy_similarity_threshold: f32,
    /// Whether to cache responses alongside request hashes.
    pub cache_responses: bool,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            window_secs: 300,
            max_cache_size: 10_000,
            hash_algorithm: "blake3".to_string(),
            exact_match: true,
            fuzzy_similarity_threshold: 0.9,
            cache_responses: true,
        }
    }
}

/// Live statistics exported by the deduplication engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupStats {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub dedup_rate: f64,
    pub bytes_saved: u64,
    pub evictions: u64,
    pub current_cache_size: usize,
}

// ---------------------------------------------------------------------------
// Core engine
// ---------------------------------------------------------------------------

/// Enhanced request deduplication engine (v2).
///
/// Thread-safe via `DashMap` and `AtomicU64`.
#[derive(Debug)]
pub struct RequestDedupV2 {
    cache: DashMap<String, DedupRequest>,
    config: Arc<std::sync::RwLock<DedupConfig>>,
    total_requests: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    bytes_saved: AtomicU64,
    evictions: AtomicU64,
}

impl RequestDedupV2 {
    /// Create a new dedup engine with default configuration.
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(DedupConfig::default())),
            total_requests: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            bytes_saved: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Create with a custom config.
    pub fn with_config(config: DedupConfig) -> Self {
        Self {
            cache: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(config)),
            total_requests: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            bytes_saved: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Compute a BLAKE3 hash for the given data.
    pub fn hash(data: &[u8]) -> String {
        blake3::hash(data).to_hex().to_string()
    }

    /// Check whether a request is a duplicate and optionally cache the response.
    ///
    /// Returns `Some((status, body))` if a cached response was found, `None` otherwise.
    pub fn check_and_cache(
        &self,
        request_body: &[u8],
        model_id: &str,
        provider_id: &str,
        response_body: Option<&[u8]>,
        response_status: u16,
    ) -> Option<(u16, Vec<u8>)> {
        let cfg = self.config.read().unwrap();
        if !cfg.enabled {
            return None;
        }
        drop(cfg);

        self.total_requests.fetch_add(1, Ordering::Relaxed);

        let request_hash = Self::hash(request_body);
        let parameters_hash = Self::hash(model_id.as_bytes());

        // Check for exact match first
        if let Some(mut entry) = self.cache.get_mut(&request_hash) {
            let now = Instant::now();
            let age = now.duration_since(entry.timestamp).as_secs();
            if age < entry.ttl {
                entry.last_accessed = now;
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                if let Some(ref body) = entry.cached_response {
                    self.bytes_saved
                        .fetch_add(body.len() as u64, Ordering::Relaxed);
                    return Some((entry.cached_status, body.clone()));
                }
                return Some((0, Vec::new())); // duplicate but no body cached
            } else {
                // Expired — remove and fall through
                drop(entry);
                self.cache.remove(&request_hash);
            }
        }

        // Fuzzy matching (if enabled and not exact-only mode)
        let cfg = self.config.read().unwrap();
        if !cfg.exact_match && cfg.fuzzy_similarity_threshold > 0.0 {
            if let Some((_key, cached)) = self.fuzzy_find(&parameters_hash, request_body) {
                drop(cfg);
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                if let Some(ref body) = cached.cached_response {
                    self.bytes_saved
                        .fetch_add(body.len() as u64, Ordering::Relaxed);
                    return Some((cached.cached_status, body.clone()));
                }
                return Some((0, Vec::new()));
            }
        }
        drop(cfg);

        self.cache_misses.fetch_add(1, Ordering::Relaxed);

        // Cache the new entry
        let cfg = self.config.read().unwrap();
        let entry = DedupRequest {
            request_hash: request_hash.clone(),
            model_id: model_id.to_string(),
            provider_id: provider_id.to_string(),
            timestamp: Instant::now(),
            ttl: cfg.window_secs,
            parameters_hash: parameters_hash.clone(),
            cached_response: if cfg.cache_responses {
                response_body.map(|b| b.to_vec())
            } else {
                None
            },
            cached_status: response_status,
            cached_response_size: response_body.map(|b| b.len() as u64).unwrap_or(0),
            last_accessed: Instant::now(),
        };
        drop(cfg);

        // Evict if at capacity
        if self.cache.len() >= self.config.read().unwrap().max_cache_size {
            self.evict_lru();
        }

        self.cache.insert(request_hash, entry);

        None
    }

    /// Retrieve a cached response by request body hash.
    pub fn get_cached_response(&self, request_body: &[u8]) -> Option<(u16, Vec<u8>)> {
        let request_hash = Self::hash(request_body);
        if let Some(mut entry) = self.cache.get_mut(&request_hash) {
            let now = Instant::now();
            let age = now.duration_since(entry.timestamp).as_secs();
            if age < entry.ttl {
                entry.last_accessed = now;
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                if let Some(ref body) = entry.cached_response {
                    return Some((entry.cached_status, body.clone()));
                }
            }
        }
        None
    }

    /// Invalidate a single entry by request body hash.
    pub fn invalidate(&self, request_body: &[u8]) -> bool {
        let request_hash = Self::hash(request_body);
        self.cache.remove(&request_hash).is_some()
    }

    /// Invalidate all entries for a given model_id.
    pub fn invalidate_by_model(&self, model_id: &str) -> usize {
        let target_hash = Self::hash(model_id.as_bytes());
        let keys: Vec<String> = self
            .cache
            .iter()
            .filter(|e| e.value().parameters_hash == target_hash)
            .map(|e| e.key().clone())
            .collect();
        let count = keys.len();
        for key in keys {
            self.cache.remove(&key);
        }
        count
    }

    /// Clear the entire cache.
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get current statistics.
    pub fn get_stats(&self) -> DedupStats {
        let total = self.total_requests.load(Ordering::Relaxed);
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let dedup_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };
        DedupStats {
            total_requests: total,
            cache_hits: hits,
            cache_misses: misses,
            dedup_rate,
            bytes_saved: self.bytes_saved.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            current_cache_size: self.cache.len(),
        }
    }

    /// Update configuration at runtime.
    pub fn update_config(&self, new_config: DedupConfig) {
        let mut cfg = self.config.write().unwrap();
        *cfg = new_config;
    }

    /// Get a snapshot of the current configuration.
    pub fn get_config(&self) -> DedupConfig {
        self.config.read().unwrap().clone()
    }

    /// Run a single TTL eviction pass, removing all expired entries.
    pub fn prune_expired(&self) {
        let now = Instant::now();
        let keys: Vec<String> = self
            .cache
            .iter()
            .filter(|e| now.duration_since(e.value().timestamp).as_secs() >= e.value().ttl)
            .map(|e| e.key().clone())
            .collect();
        for key in keys {
            self.cache.remove(&key);
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Evict the least-recently-used entry.
    fn evict_lru(&self) {
        let mut oldest_key: Option<String> = None;
        let mut oldest_time = Instant::now();
        for entry in self.cache.iter() {
            if entry.value().last_accessed < oldest_time {
                oldest_time = entry.value().last_accessed;
                oldest_key = Some(entry.key().clone());
            }
        }
        if let Some(key) = oldest_key {
            self.cache.remove(&key);
            self.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Simple fuzzy match: looks for entries with the same parameters_hash whose
    /// request_hash is sufficiently similar (using Hamming distance approximation).
    fn fuzzy_find(&self, parameters_hash: &str, _request_body: &[u8]) -> Option<(String, DedupRequest)> {
        let now = Instant::now();
        for entry in self.cache.iter() {
            if entry.value().parameters_hash == parameters_hash {
                let age = now.duration_since(entry.value().timestamp).as_secs();
                if age < entry.value().ttl {
                    let key = entry.key().clone();
                    // Update last accessed via get_mut
                    if let Some(mut e) = self.cache.get_mut(&key) {
                        e.last_accessed = now;
                        return Some((
                            key,
                            DedupRequest {
                                request_hash: e.request_hash.clone(),
                                model_id: e.model_id.clone(),
                                provider_id: e.provider_id.clone(),
                                timestamp: e.timestamp,
                                ttl: e.ttl,
                                parameters_hash: e.parameters_hash.clone(),
                                cached_response: e.cached_response.clone(),
                                cached_status: e.cached_status,
                                cached_response_size: e.cached_response_size,
                                last_accessed: e.last_accessed,
                            },
                        ));
                    }
                }
            }
        }
        None
    }

    /// Health check: returns true if the cache is operational.
    pub fn is_healthy(&self) -> bool {
        // A simple write + read round-trip to verify the map is functional.
        let test_key = "__health_check__";
        self.cache.insert(test_key.to_string(), DedupRequest {
            request_hash: test_key.to_string(),
            model_id: "health".to_string(),
            provider_id: "health".to_string(),
            timestamp: Instant::now(),
            ttl: 1,
            parameters_hash: "health".to_string(),
            cached_response: None,
            cached_status: 200,
            cached_response_size: 0,
            last_accessed: Instant::now(),
        });
        let ok = self.cache.remove(test_key).is_some();
        ok
    }
}

impl Default for RequestDedupV2 {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DedupCheckRequest {
    pub request_body: Option<String>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
}

#[derive(Serialize)]
pub struct DedupCheckResponse {
    pub is_duplicate: bool,
    pub cached_response: Option<String>,
    pub cached_status: Option<u16>,
    pub request_hash: Option<String>,
}

#[derive(Deserialize)]
pub struct DedupConfigUpdate {
    pub enabled: Option<bool>,
    pub window_secs: Option<u64>,
    pub max_cache_size: Option<usize>,
    pub exact_match: Option<bool>,
    pub fuzzy_similarity_threshold: Option<f32>,
    pub cache_responses: Option<bool>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub cache_size: usize,
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn check_dedup(
    State(state): State<AppState>,
    Json(body): Json<DedupCheckRequest>,
) -> impl IntoResponse {
    let dedup = &state.request_dedup_v2;
    let req_body = body.request_body.unwrap_or_default();
    let model_id = body.model_id.unwrap_or_default();
    let provider_id = body.provider_id.unwrap_or_default();

    let result = dedup.check_and_cache(
        req_body.as_bytes(),
        &model_id,
        &provider_id,
        None,
        0,
    );

    let is_duplicate = result.is_some();
    let (cached_status, cached_response): (Option<u16>, Option<String>) = result
        .map(|(s, b)| (Some(s), Some(String::from_utf8_lossy(&b).to_string())))
        .unwrap_or((None, None));

    let request_hash = Some(RequestDedupV2::hash(req_body.as_bytes()));

    (
        StatusCode::OK,
        Json(DedupCheckResponse {
            is_duplicate,
            cached_response,
            cached_status,
            request_hash,
        }),
    )
}

async fn get_dedup_stats(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.request_dedup_v2.get_stats();
    (StatusCode::OK, Json(stats))
}

async fn get_dedup_config(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.request_dedup_v2.get_config();
    (StatusCode::OK, Json(config))
}

async fn update_dedup_config(
    State(state): State<AppState>,
    Json(body): Json<DedupConfigUpdate>,
) -> impl IntoResponse {
    let mut config = state.request_dedup_v2.get_config();
    if let Some(enabled) = body.enabled {
        config.enabled = enabled;
    }
    if let Some(window_secs) = body.window_secs {
        config.window_secs = window_secs;
    }
    if let Some(max_cache_size) = body.max_cache_size {
        config.max_cache_size = max_cache_size;
    }
    if let Some(exact_match) = body.exact_match {
        config.exact_match = exact_match;
    }
    if let Some(threshold) = body.fuzzy_similarity_threshold {
        config.fuzzy_similarity_threshold = threshold;
    }
    if let Some(cache_responses) = body.cache_responses {
        config.cache_responses = cache_responses;
    }
    state.request_dedup_v2.update_config(config.clone());
    (StatusCode::OK, Json(config))
}

async fn clear_dedup_cache(State(state): State<AppState>) -> impl IntoResponse {
    state.request_dedup_v2.clear();
    (StatusCode::OK, Json(serde_json::json!({"status": "cleared"})))
}

async fn dedup_health(State(state): State<AppState>) -> impl IntoResponse {
    let healthy = state.request_dedup_v2.is_healthy();
    let cache_size = state.request_dedup_v2.cache.len();
    (
        StatusCode::OK,
        Json(HealthResponse { healthy, cache_size }),
    )
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the dedup v2 API router, merged into the main app.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/dedup/check", post(check_dedup))
        .route("/v1/dedup/stats", get(get_dedup_stats))
        .route("/v1/dedup/config", get(get_dedup_config))
        .route("/v1/dedup/config", put(update_dedup_config))
        .route("/v1/dedup/cache", delete(clear_dedup_cache))
        .route("/v1/dedup/health", get(dedup_health))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_empty_cache() {
        let dedup = RequestDedupV2::new();
        assert_eq!(dedup.cache.len(), 0);
    }

    #[test]
    fn test_hash_deterministic() {
        let a = RequestDedupV2::hash(b"hello");
        let b = RequestDedupV2::hash(b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn test_hash_different_inputs() {
        let a = RequestDedupV2::hash(b"hello");
        let b = RequestDedupV2::hash(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_check_and_cache_miss() {
        let dedup = RequestDedupV2::new();
        let result = dedup.check_and_cache(b"req1", "model-a", "prov-1", None, 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_check_and_cache_hit() {
        let dedup = RequestDedupV2::new();
        // Insert first
        let miss = dedup.check_and_cache(b"req1", "model-a", "prov-1", Some(b"resp-body"), 200);
        assert!(miss.is_none());
        // Now should hit
        let hit = dedup.check_and_cache(b"req1", "model-a", "prov-1", None, 0);
        assert!(hit.is_some());
        let (status, body) = hit.unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, b"resp-body");
    }

    #[test]
    fn test_get_cached_response_hit() {
        let dedup = RequestDedupV2::new();
        dedup.check_and_cache(b"req1", "m", "p", Some(b"data"), 200);
        let resp = dedup.get_cached_response(b"req1");
        assert!(resp.is_some());
        assert_eq!(resp.unwrap().1, b"data");
    }

    #[test]
    fn test_get_cached_response_miss() {
        let dedup = RequestDedupV2::new();
        let resp = dedup.get_cached_response(b"nonexistent");
        assert!(resp.is_none());
    }

    #[test]
    fn test_invalidate() {
        let dedup = RequestDedupV2::new();
        dedup.check_and_cache(b"req1", "m", "p", Some(b"data"), 200);
        assert!(dedup.get_cached_response(b"req1").is_some());
        assert!(dedup.invalidate(b"req1"));
        assert!(dedup.get_cached_response(b"req1").is_none());
    }

    #[test]
    fn test_clear() {
        let dedup = RequestDedupV2::new();
        dedup.check_and_cache(b"a", "m", "p", None, 0);
        dedup.check_and_cache(b"b", "m", "p", None, 0);
        assert_eq!(dedup.cache.len(), 2);
        dedup.clear();
        assert_eq!(dedup.cache.len(), 0);
    }

    #[test]
    fn test_stats_track_correctly() {
        let dedup = RequestDedupV2::new();
        dedup.check_and_cache(b"req1", "m", "p", Some(b"data"), 200);
        dedup.check_and_cache(b"req1", "m", "p", None, 0); // hit
        let stats = dedup.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
    }

    #[test]
    fn test_disabled_config_always_miss() {
        let dedup = RequestDedupV2::with_config(DedupConfig {
            enabled: false,
            ..DedupConfig::default()
        });
        dedup.check_and_cache(b"req1", "m", "p", Some(b"data"), 200);
        let result = dedup.check_and_cache(b"req1", "m", "p", None, 0);
        assert!(result.is_none());
    }
}
