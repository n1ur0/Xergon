//! Semantic Cache for the Xergon relay.
//!
//! Provides an in-memory semantic similarity cache that uses trigram Jaccard
//! similarity to match semantically similar queries. When a new request is
//! sufficiently similar to a previously cached response, the cached response
//! is returned instead of forwarding to the provider.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashSet};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the semantic cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticCacheConfig {
    /// Enable/disable semantic caching (default: true).
    pub enabled: bool,
    /// Maximum number of entries (default: 5000).
    pub max_entries: usize,
    /// TTL for cached entries in seconds (default: 300 = 5 min).
    pub default_ttl_secs: u64,
    /// Minimum Jaccard similarity threshold (0.0 - 1.0, default: 0.75).
    pub similarity_threshold: f64,
    /// Maximum length of query text to consider (default: 2048).
    pub max_query_length: usize,
    /// Cleanup interval in seconds (default: 60).
    pub cleanup_interval_secs: u64,
}

impl Default for SemanticCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 5_000,
            default_ttl_secs: 300,
            similarity_threshold: 0.75,
            max_query_length: 2048,
            cleanup_interval_secs: 60,
        }
    }
}

// ---------------------------------------------------------------------------
// Cache entry
// ---------------------------------------------------------------------------

/// A single semantic cache entry.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticCacheEntry {
    /// SHA-256 hash of the normalized query.
    pub query_hash: String,
    /// The original (un-normalized) query.
    pub original_query: String,
    /// The normalized query used for similarity matching.
    pub normalized_query: String,
    /// Trigram set extracted from the normalized query.
    #[serde(skip)]
    pub trigrams: HashSet<String>,
    /// The cached response body (JSON string).
    pub response_body: serde_json::Value,
    /// HTTP status code of the cached response.
    pub status_code: u16,
    /// Model that generated this response.
    pub model: String,
    /// ISO 8601 timestamp when this entry was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// TTL in seconds.
    pub ttl_secs: u64,
    /// Number of times this entry was used as a cache hit.
    pub hit_count: u64,
}

impl SemanticCacheEntry {
    /// Check if this entry has expired.
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now();
        now.signed_duration_since(self.created_at).num_seconds() > self.ttl_secs as i64
    }
}

// ---------------------------------------------------------------------------
// Cache lookup result
// ---------------------------------------------------------------------------

/// Result of a semantic cache lookup.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticCacheLookup {
    /// Whether a cache hit was found.
    pub hit: bool,
    /// The cached response body (only if hit).
    pub response_body: Option<serde_json::Value>,
    /// The HTTP status code (only if hit).
    pub status_code: Option<u16>,
    /// The similarity score of the best match (0.0 - 1.0).
    pub similarity: f64,
    /// The matched entry's query hash (for debugging).
    pub matched_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Cache statistics
// ---------------------------------------------------------------------------

/// Statistics for the semantic cache.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticCacheStats {
    pub total_entries: usize,
    pub total_hits: u64,
    pub total_misses: u64,
    pub total_invalidations: u64,
    pub hit_rate: f64,
    pub similarity_threshold: f64,
    pub max_entries: usize,
    pub config: SemanticCacheConfig,
}

// ---------------------------------------------------------------------------
// The semantic cache
// ---------------------------------------------------------------------------

/// In-memory semantic cache backed by a DashMap.
pub struct SemanticCache {
    entries: DashMap<String, SemanticCacheEntry>,
    config: SemanticCacheConfig,
    hits: AtomicU64,
    misses: AtomicU64,
    invalidations: AtomicI64,
}

impl SemanticCache {
    /// Create a new semantic cache with default configuration.
    pub fn new() -> Self {
        Self::with_config(SemanticCacheConfig::default())
    }

    /// Create a new semantic cache with custom configuration.
    pub fn with_config(config: SemanticCacheConfig) -> Self {
        info!(
            max_entries = config.max_entries,
            similarity_threshold = config.similarity_threshold,
            ttl_secs = config.default_ttl_secs,
            "Semantic cache initialized"
        );
        Self {
            entries: DashMap::with_capacity(config.max_entries),
            config,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            invalidations: AtomicI64::new(0),
        }
    }

    /// Check if semantic caching is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Look up a query in the semantic cache.
    ///
    /// Normalizes the query, extracts trigrams, and finds the best match
    /// above the similarity threshold.
    pub fn lookup(&self, query: &str, model: &str) -> SemanticCacheLookup {
        if !self.config.enabled {
            return SemanticCacheLookup {
                hit: false,
                response_body: None,
                status_code: None,
                similarity: 0.0,
                matched_hash: None,
            };
        }

        let normalized = normalize_query(query);
        let query_trigrams = extract_trigrams(&normalized);

        let mut best_similarity: f64 = 0.0;
        let mut best_entry: Option<SemanticCacheEntry> = None;

        for entry in self.entries.iter() {
            // Skip expired entries
            if entry.is_expired() {
                continue;
            }
            // Must match model
            if entry.model != model {
                continue;
            }

            let sim = jaccard_similarity(&query_trigrams, &entry.trigrams);
            if sim > best_similarity {
                best_similarity = sim;
                best_entry = Some(entry.value().clone());
            }
        }

        if let Some(mut entry) = best_entry {
            if best_similarity >= self.config.similarity_threshold {
                // Update hit count atomically via re-insert
                entry.hit_count += 1;
                self.hits.fetch_add(1, Ordering::Relaxed);
                if let Some(mut e) = self.entries.get_mut(&entry.query_hash) {
                    e.hit_count = entry.hit_count;
                }

                debug!(
                    similarity = best_similarity,
                    hash = %entry.query_hash,
                    hit_count = entry.hit_count,
                    "Semantic cache hit"
                );

                return SemanticCacheLookup {
                    hit: true,
                    response_body: Some(entry.response_body),
                    status_code: Some(entry.status_code),
                    similarity: best_similarity,
                    matched_hash: Some(entry.query_hash),
                };
            }
        }

        self.misses.fetch_add(1, Ordering::Relaxed);
        SemanticCacheLookup {
            hit: false,
            response_body: None,
            status_code: None,
            similarity: best_similarity,
            matched_hash: None,
        }
    }

    /// Store a response in the semantic cache.
    pub fn store(
        &self,
        query: &str,
        model: &str,
        response_body: serde_json::Value,
        status_code: u16,
    ) -> Option<String> {
        if !self.config.enabled {
            return None;
        }

        let normalized = normalize_query(query);
        let trigrams = extract_trigrams(&normalized);
        let hash = query_hash(&normalized);

        // Evict oldest entries if at capacity
        if self.entries.len() >= self.config.max_entries {
            self.evict_oldest(1);
        }

        let entry = SemanticCacheEntry {
            query_hash: hash.clone(),
            original_query: query.to_string(),
            normalized_query: normalized,
            trigrams,
            response_body,
            status_code,
            model: model.to_string(),
            created_at: chrono::Utc::now(),
            ttl_secs: self.config.default_ttl_secs,
            hit_count: 0,
        };

        self.entries.insert(hash.clone(), entry);
        debug!(hash = %hash, model = %model, "Semantic cache entry stored");
        Some(hash)
    }

    /// Store a response with a custom TTL.
    pub fn store_with_ttl(
        &self,
        query: &str,
        model: &str,
        response_body: serde_json::Value,
        status_code: u16,
        ttl_secs: u64,
    ) -> Option<String> {
        if !self.config.enabled {
            return None;
        }

        let normalized = normalize_query(query);
        let trigrams = extract_trigrams(&normalized);
        let hash = query_hash(&normalized);

        if self.entries.len() >= self.config.max_entries {
            self.evict_oldest(1);
        }

        let entry = SemanticCacheEntry {
            query_hash: hash.clone(),
            original_query: query.to_string(),
            normalized_query: normalized,
            trigrams,
            response_body,
            status_code,
            model: model.to_string(),
            created_at: chrono::Utc::now(),
            ttl_secs,
            hit_count: 0,
        };

        self.entries.insert(hash.clone(), entry);
        debug!(hash = %hash, model = %model, ttl_secs, "Semantic cache entry stored with custom TTL");
        Some(hash)
    }

    /// Invalidate all cache entries whose original query starts with the given prefix.
    pub fn invalidate_prefix(&self, prefix: &str) -> usize {
        let prefix_lower = prefix.to_lowercase();
        let mut count = 0;
        self.entries.retain(|_, entry| {
            if entry.original_query.to_lowercase().starts_with(&prefix_lower) {
                count += 1;
                false
            } else {
                true
            }
        });
        self.invalidations.fetch_add(count as i64, Ordering::Relaxed);
        debug!(prefix = %prefix, count, "Semantic cache prefix invalidation");
        count
    }

    /// Invalidate a specific entry by hash.
    pub fn invalidate(&self, hash: &str) -> bool {
        let removed = self.entries.remove(hash).is_some();
        if removed {
            self.invalidations.fetch_add(1, Ordering::Relaxed);
        }
        removed
    }

    /// Clear all entries from the cache.
    pub fn clear(&self) {
        let count = self.entries.len();
        self.entries.clear();
        self.invalidations.fetch_add(count as i64, Ordering::Relaxed);
        info!(count, "Semantic cache cleared");
    }

    /// Remove expired entries. Returns the number of entries removed.
    pub fn cleanup(&self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, entry| !entry.is_expired());
        let removed = before - self.entries.len();
        if removed > 0 {
            debug!(removed, "Semantic cache cleanup completed");
        }
        removed
    }

    /// Get cache statistics.
    pub fn stats(&self) -> SemanticCacheStats {
        let total_hits = self.hits.load(Ordering::Relaxed);
        let total_misses = self.misses.load(Ordering::Relaxed);
        let total_requests = total_hits + total_misses;
        let hit_rate = if total_requests > 0 {
            total_hits as f64 / total_requests as f64
        } else {
            0.0
        };

        SemanticCacheStats {
            total_entries: self.entries.len(),
            total_hits,
            total_misses,
            total_invalidations: self.invalidations.load(Ordering::Relaxed) as u64,
            hit_rate,
            similarity_threshold: self.config.similarity_threshold,
            max_entries: self.config.max_entries,
            config: self.config.clone(),
        }
    }

    /// Get all (non-expired) entries, most recent first, up to a limit.
    pub fn entries(&self, limit: usize) -> Vec<SemanticCacheEntry> {
        let mut all: Vec<SemanticCacheEntry> = self
            .entries
            .iter()
            .filter(|e| !e.is_expired())
            .map(|e| e.value().clone())
            .collect();
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.truncate(limit);
        all
    }

    /// Get entries filtered by model.
    pub fn entries_by_model(&self, model: &str, limit: usize) -> Vec<SemanticCacheEntry> {
        let mut all: Vec<SemanticCacheEntry> = self
            .entries
            .iter()
            .filter(|e| !e.is_expired() && e.model == model)
            .map(|e| e.value().clone())
            .collect();
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.truncate(limit);
        all
    }

    // -- internal helpers --------------------------------------------------

    /// Evict the oldest N entries by creation time.
    fn evict_oldest(&self, count: usize) {
        let mut by_age: Vec<(String, chrono::DateTime<chrono::Utc>)> = self
            .entries
            .iter()
            .map(|e| (e.key().clone(), e.value().created_at))
            .collect();
        by_age.sort_by_key(|&(_, ts)| ts);
        for (hash, _) in by_age.into_iter().take(count) {
            self.entries.remove(&hash);
        }
    }
}

// ---------------------------------------------------------------------------
// Query normalization
// ---------------------------------------------------------------------------

/// Normalize a query string for similarity matching.
///
/// - Lowercase
/// - Trim whitespace
/// - Collapse internal whitespace
/// - Remove punctuation (except spaces)
/// - Truncate to max_query_length
pub fn normalize_query(query: &str) -> String {
    let max_len = 2048;
    let trimmed: String = query
        .chars()
        .take(max_len)
        .collect::<String>()
        .to_lowercase()
        .trim()
        .to_string();

    // Remove punctuation (keep alphanumeric and spaces)
    let cleaned: String = trimmed
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();

    // Collapse multiple whitespace into single space
    let mut result = String::with_capacity(cleaned.len());
    let mut prev_space = false;
    for c in cleaned.chars() {
        if c.is_whitespace() {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }

    result.trim().to_string()
}

// ---------------------------------------------------------------------------
// Trigram extraction
// ---------------------------------------------------------------------------

/// Extract 3-character substrings (trigrams) from a string.
pub fn extract_trigrams(text: &str) -> HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut trigrams = HashSet::with_capacity(chars.len().saturating_sub(2));
    if chars.len() < 3 {
        // For very short strings, use the whole string as a single "trigram"
        if !chars.is_empty() {
            trigrams.insert(chars.iter().collect());
        }
        return trigrams;
    }
    for window in chars.windows(3) {
        trigrams.insert(window.iter().collect());
    }
    trigrams
}

// ---------------------------------------------------------------------------
// Jaccard similarity
// ---------------------------------------------------------------------------

/// Compute Jaccard similarity between two sets of trigrams.
///
/// Jaccard(A, B) = |A ∩ B| / |A ∪ B|
pub fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

// ---------------------------------------------------------------------------
// Query hashing
// ---------------------------------------------------------------------------

/// Compute SHA-256 hash of a normalized query for use as a cache key.
fn query_hash(normalized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Axum admin handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use axum::routing::get;

use crate::proxy::AppState;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SemanticCacheQueryParams {
    pub model: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// GET /admin/semantic-cache/stats
pub async fn semantic_cache_stats_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_sc_admin_key(&state, &headers) {
        return admin_sc_error("Invalid or missing admin key", status);
    }
    let stats = state.semantic_cache.stats();
    admin_sc_ok(serde_json::json!(stats))
}

/// GET /admin/semantic-cache/entries
pub async fn semantic_cache_entries_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SemanticCacheQueryParams>,
) -> Response {
    if let Err(status) = verify_sc_admin_key(&state, &headers) {
        return admin_sc_error("Invalid or missing admin key", status);
    }

    let entries = if let Some(ref model) = params.model {
        state.semantic_cache.entries_by_model(model, params.limit)
    } else {
        state.semantic_cache.entries(params.limit)
    };

    admin_sc_ok(serde_json::json!({
        "entries": entries,
        "total": entries.len(),
        "query": params,
    }))
}

/// POST /admin/semantic-cache/cleanup
pub async fn semantic_cache_cleanup_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_sc_admin_key(&state, &headers) {
        return admin_sc_error("Invalid or missing admin key", status);
    }
    let removed = state.semantic_cache.cleanup();
    admin_sc_ok(serde_json::json!({
        "removed": removed,
        "message": format!("Removed {} expired entries", removed),
    }))
}

/// DELETE /admin/semantic-cache/clear
pub async fn semantic_cache_clear_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_sc_admin_key(&state, &headers) {
        return admin_sc_error("Invalid or missing admin key", status);
    }
    state.semantic_cache.clear();
    admin_sc_ok(serde_json::json!({ "message": "Semantic cache cleared" }))
}

/// DELETE /admin/semantic-cache/invalidate
pub async fn semantic_cache_invalidate_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SemanticCacheInvalidateParams>,
) -> Response {
    if let Err(status) = verify_sc_admin_key(&state, &headers) {
        return admin_sc_error("Invalid or missing admin key", status);
    }
    let count = if let Some(ref hash) = params.hash {
        let removed = state.semantic_cache.invalidate(hash);
        admin_sc_ok(serde_json::json!({
            "removed": removed,
            "hash": hash,
        }))
    } else if let Some(ref prefix) = params.prefix {
        let count = state.semantic_cache.invalidate_prefix(prefix);
        admin_sc_ok(serde_json::json!({
            "removed": count,
            "prefix": prefix,
        }))
    } else {
        admin_sc_error("Must provide either 'hash' or 'prefix' parameter", StatusCode::BAD_REQUEST)
    };
    count
}

#[derive(Debug, Deserialize)]
pub struct SemanticCacheInvalidateParams {
    pub hash: Option<String>,
    pub prefix: Option<String>,
}

fn verify_sc_admin_key(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected_key = &state.config.admin.api_key;
    let provided = headers
        .get("x-admin-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided.is_empty() || provided != expected_key {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn admin_sc_error(msg: &str, status: StatusCode) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn admin_sc_ok(value: serde_json::Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

/// Build the semantic cache admin router.
pub fn build_semantic_cache_router() -> Router<AppState> {
    Router::new()
        .route("/admin/semantic-cache/stats", get(semantic_cache_stats_handler))
        .route("/admin/semantic-cache/entries", get(semantic_cache_entries_handler))
        .route("/admin/semantic-cache/cleanup", get(semantic_cache_cleanup_handler))
        .route("/admin/semantic-cache/clear", get(semantic_cache_clear_handler))
        .route("/admin/semantic-cache/invalidate", get(semantic_cache_invalidate_handler))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_query() {
        assert_eq!(normalize_query("Hello, World!"), "hello world");
        assert_eq!(normalize_query("  foo   bar  "), "foo bar");
        assert_eq!(normalize_query("What is AI?"), "what is ai");
    }

    #[test]
    fn test_extract_trigrams() {
        let t = extract_trigrams("hello");
        assert!(t.contains("hel"));
        assert!(t.contains("ell"));
        assert!(t.contains("llo"));
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn test_jaccard_similarity() {
        let a: HashSet<String> = ["abc", "bcd", "cde"].iter().map(|s| s.to_string()).collect();
        let b: HashSet<String> = ["abc", "bcd", "xyz"].iter().map(|s| s.to_string()).collect();
        let sim = jaccard_similarity(&a, &b);
        // intersection = {abc, bcd} = 2, union = {abc, bcd, cde, xyz} = 4
        assert!((sim - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_cache_store_and_lookup() {
        let cache = SemanticCache::with_config(SemanticCacheConfig {
            enabled: true,
            similarity_threshold: 0.5,
            max_entries: 100,
            default_ttl_secs: 300,
            ..Default::default()
        });

        let response = serde_json::json!({"text": "Hello!"});
        cache.store("What is AI?", "gpt-4", response.clone(), 200);

        // Exact match
        let result = cache.lookup("What is AI?", "gpt-4");
        assert!(result.hit);

        // Similar query
        let result = cache.lookup("What is artificial intelligence?", "gpt-4");
        assert!(result.hit);

        // Different model
        let result = cache.lookup("What is AI?", "claude-3");
        assert!(!result.hit);
    }

    #[test]
    fn test_cache_invalidate_prefix() {
        let cache = SemanticCache::new();
        cache.store("List all users", "gpt-4", serde_json::json!({"a": 1}), 200);
        cache.store("List all orders", "gpt-4", serde_json::json!({"a": 2}), 200);
        cache.store("Get user profile", "gpt-4", serde_json::json!({"a": 3}), 200);

        let removed = cache.invalidate_prefix("List");
        assert_eq!(removed, 2);
        assert_eq!(cache.entries(100).len(), 1);
    }
}
