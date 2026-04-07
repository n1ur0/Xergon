//! Request Coalescing — Batch similar requests for efficiency
//!
//! Coalesces requests that share the same model, prompt hash, and parameter
//! hash within a configurable time window. Uses BLAKE3 for prompt hashing
//! and tracks deduplication savings in tokens and latency.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use blake3::hash;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default maximum age for a coalesce key in seconds
const DEFAULT_MAX_AGE_SECS: u64 = 30;

/// Maximum number of pending requests per coalesce group
const MAX_PENDING_PER_KEY: usize = 100;

// ---------------------------------------------------------------------------
// CoalesceKey
// ---------------------------------------------------------------------------

/// Composite key for identifying coalesceable requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoalesceKey {
    /// Model identifier (e.g., "llama-3.1-70b")
    pub model_id: String,
    /// BLAKE3 hash of the prompt content
    pub prompt_hash: [u8; 32],
    /// BLAKE3 hash of the request parameters (temperature, max_tokens, etc.)
    pub parameters_hash: [u8; 32],
    /// Maximum age in seconds for this coalesce group
    pub max_age_secs: u64,
}

impl CoalesceKey {
    /// Create a coalesce key from raw prompt and parameters.
    pub fn new(model_id: &str, prompt: &str, parameters: &str, max_age_secs: Option<u64>) -> Self {
        let prompt_hash = hash(prompt.as_bytes());
        let parameters_hash = hash(parameters.as_bytes());

        CoalesceKey {
            model_id: model_id.to_string(),
            prompt_hash: *prompt_hash.as_bytes(),
            parameters_hash: *parameters_hash.as_bytes(),
            max_age_secs: max_age_secs.unwrap_or(DEFAULT_MAX_AGE_SECS),
        }
    }

    /// Create a coalesce key from pre-computed hashes.
    pub fn from_hashes(
        model_id: String,
        prompt_hash: [u8; 32],
        parameters_hash: [u8; 32],
        max_age_secs: Option<u64>,
    ) -> Self {
        CoalesceKey {
            model_id,
            prompt_hash,
            parameters_hash,
            max_age_secs: max_age_secs.unwrap_or(DEFAULT_MAX_AGE_SECS),
        }
    }

    /// Convert the key to a string identifier for DashMap lookup.
    pub fn to_string_key(&self) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(64 + self.model_id.len());
        write!(s, "{}:", self.model_id).unwrap();
        for b in &self.prompt_hash[..8] {
            write!(s, "{:02x}", b).unwrap();
        }
        s.push(':');
        for b in &self.parameters_hash[..8] {
            write!(s, "{:02x}", b).unwrap();
        }
        s
    }
}

impl std::hash::Hash for CoalesceKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.model_id.hash(state);
        self.prompt_hash.hash(state);
        self.parameters_hash.hash(state);
    }
}

impl PartialEq for CoalesceKey {
    fn eq(&self, other: &Self) -> bool {
        self.model_id == other.model_id
            && self.prompt_hash == other.prompt_hash
            && self.parameters_hash == other.parameters_hash
    }
}

impl Eq for CoalesceKey {}

// ---------------------------------------------------------------------------
// CoalesceRequest
// ---------------------------------------------------------------------------

/// A request submitted for coalescing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoalesceRequest {
    /// Unique request identifier
    pub request_id: String,
    /// Coalesce grouping key
    pub coalesce_key: CoalesceKey,
    /// Raw request payload (JSON)
    pub payload: serde_json::Value,
    /// Submission timestamp
    pub timestamp: DateTime<Utc>,
    /// List of subscriber request IDs that will receive the response
    pub subscribers: Vec<String>,
    /// Request priority (0 = highest)
    pub priority: u8,
}

// ---------------------------------------------------------------------------
// CoalesceConfig
// ---------------------------------------------------------------------------

/// Configuration for the request coalescer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoalesceConfig {
    /// Maximum batch size for coalesced requests
    pub max_batch_size: usize,
    /// Maximum wait time in milliseconds before flushing a batch
    pub max_wait_ms: u64,
    /// Set of model IDs for which coalescing is enabled (empty = all)
    pub enabled_models: HashSet<String>,
    /// Similarity threshold for fuzzy matching (0.0 - 1.0)
    pub similarity_threshold: f64,
    /// Whether coalescing is enabled
    pub enabled: bool,
    /// Default max age for coalesce keys in seconds
    pub default_max_age_secs: u64,
}

impl Default for CoalesceConfig {
    fn default() -> Self {
        CoalesceConfig {
            max_batch_size: 10,
            max_wait_ms: 100,
            enabled_models: HashSet::new(),
            similarity_threshold: 0.95,
            enabled: true,
            default_max_age_secs: DEFAULT_MAX_AGE_SECS,
        }
    }
}

// ---------------------------------------------------------------------------
// CoalesceResult
// ---------------------------------------------------------------------------

/// Result of a coalesced request batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoalesceResult {
    /// Unique coalesce group identifier
    pub coalesce_id: String,
    /// Primary request that was executed
    pub primary_request_id: String,
    /// Number of requests merged into this batch
    pub merged_count: usize,
    /// The response content
    pub response: serde_json::Value,
    /// Estimated tokens saved by deduplication
    pub saved_tokens: u64,
    /// Total latency in milliseconds
    pub latency_ms: u64,
}

// ---------------------------------------------------------------------------
// Coalescer Stats
// ---------------------------------------------------------------------------

/// Coalescer performance statistics.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CoalesceStats {
    /// Total requests submitted for coalescing
    pub total_submitted: AtomicU64,
    /// Total requests that were coalesced (deduped)
    pub total_coalesced: AtomicU64,
    /// Total requests executed as primary
    pub total_executed: AtomicU64,
    /// Total tokens saved by deduplication
    pub total_saved_tokens: AtomicU64,
    /// Total batches flushed
    pub total_batches: AtomicU64,
    /// Current pending request count
    pub pending_count: AtomicU64,
}

impl CoalesceStats {
    fn new() -> Self {
        Self::default()
    }

    fn record_submit(&self) {
        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        self.pending_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_coalesce(&self) {
        self.total_coalesced.fetch_add(1, Ordering::Relaxed);
        self.pending_count.fetch_sub(1, Ordering::Relaxed);
    }

    fn record_execute(&self, saved_tokens: u64) {
        self.total_executed.fetch_add(1, Ordering::Relaxed);
        self.total_saved_tokens.fetch_add(saved_tokens, Ordering::Relaxed);
        self.pending_count.fetch_sub(1, Ordering::Relaxed);
    }

    fn record_batch(&self) {
        self.total_batches.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> CoalesceStatsSnapshot {
        CoalesceStatsSnapshot {
            total_submitted: self.total_submitted.load(Ordering::Relaxed),
            total_coalesced: self.total_coalesced.load(Ordering::Relaxed),
            total_executed: self.total_executed.load(Ordering::Relaxed),
            total_saved_tokens: self.total_saved_tokens.load(Ordering::Relaxed),
            total_batches: self.total_batches.load(Ordering::Relaxed),
            pending_count: self.pending_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CoalesceStatsSnapshot {
    pub total_submitted: u64,
    pub total_coalesced: u64,
    pub total_executed: u64,
    pub total_saved_tokens: u64,
    pub total_batches: u64,
    pub pending_count: u64,
}

// ---------------------------------------------------------------------------
// Request Coalescer
// ---------------------------------------------------------------------------

/// Request coalescer that batches similar requests together.
///
/// Uses DashMap for concurrent access to pending request groups.
/// Supports time-windowed batching and subscriber notification.
pub struct RequestCoalescer {
    /// Pending requests grouped by coalesce key string
    pending: DashMap<String, Vec<CoalesceRequest>>,
    /// Configuration
    config: Arc<std::sync::RwLock<CoalesceConfig>>,
    /// Performance statistics
    stats: CoalesceStats,
}

impl RequestCoalescer {
    /// Create a new request coalescer with default configuration.
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(CoalesceConfig::default())),
            stats: CoalesceStats::new(),
        }
    }

    /// Create a new request coalescer with custom configuration.
    pub fn with_config(config: CoalesceConfig) -> Self {
        Self {
            pending: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(config)),
            stats: CoalesceStats::new(),
        }
    }

    /// Submit a request for coalescing.
    ///
    /// If a matching group exists, the request is added as a subscriber.
    /// If the group is full, the request becomes a new primary.
    /// Returns the primary request ID and whether this request is a new primary.
    pub fn submit_request(&self, request: CoalesceRequest) -> (String, bool) {
        let config = self.config.read().unwrap();

        if !config.enabled {
            return (request.request_id.clone(), true);
        }

        // Check if model is in the enabled set
        if !config.enabled_models.is_empty()
            && !config.enabled_models.contains(&request.coalesce_key.model_id)
        {
            return (request.request_id.clone(), true);
        }

        self.stats.record_submit();

        let key_str = request.coalesce_key.to_string_key();

        // Check for existing pending group
        if let Some(mut group) = self.pending.get_mut(&key_str) {
            if group.len() < config.max_batch_size {
                let primary_id = group[0].request_id.clone();
                group[0].subscribers.push(request.request_id.clone());
                debug!(
                    request_id = %request.request_id,
                    primary_id = %primary_id,
                    group_size = group.len(),
                    "Request coalesced into existing group"
                );
                self.stats.record_coalesce();
                return (primary_id, false);
            }
        }

        // No matching group or group is full — create new primary
        let primary_id = request.request_id.clone();
        let new_request = CoalesceRequest {
            subscribers: vec![request.request_id.clone()],
            ..request
        };

        self.pending.insert(key_str.clone(), vec![new_request]);

        debug!(
            request_id = %primary_id,
            "Request registered as new coalesce primary"
        );

        (primary_id, true)
    }

    /// Try to coalesce a request with existing pending requests.
    ///
    /// Returns Some(primary_request_id) if coalescing succeeded,
    /// or None if no matching group was found.
    pub fn try_coalesce(&self, request: &CoalesceRequest) -> Option<String> {
        let config = self.config.read().unwrap();

        if !config.enabled {
            return None;
        }

        let key_str = request.coalesce_key.to_string_key();

        if let Some(mut group) = self.pending.get_mut(&key_str) {
            if group.len() < config.max_batch_size {
                let primary_id = group[0].request_id.clone();
                group[0].subscribers.push(request.request_id.clone());
                self.stats.record_submit();
                self.stats.record_coalesce();
                return Some(primary_id);
            }
        }

        None
    }

    /// Get all pending requests grouped by coalesce key.
    pub fn get_pending(&self) -> HashMap<String, Vec<CoalesceRequest>> {
        self.pending
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    /// Flush all pending requests older than the given duration.
    /// Returns the flushed groups.
    pub fn flush(&self, max_age_secs: Option<u64>) -> Vec<CoalesceResult> {
        let config = self.config.read().unwrap();
        let age = max_age_secs.unwrap_or(config.default_max_age_secs);
        let now = Utc::now();

        let mut results = Vec::new();
        let mut keys_to_remove: Vec<String> = Vec::new();

        for entry in self.pending.iter() {
            let key_str = entry.key();
            let group = entry.value();

            if group.is_empty() {
                keys_to_remove.push(key_str.clone());
                continue;
            }

            let oldest = group.iter().map(|r| r.timestamp).min().unwrap_or(now);
            let age_secs = (now - oldest).num_seconds().unsigned_abs();

            if age_secs >= age || group.len() >= config.max_batch_size {
                let primary = &group[0];
                let merged_count = primary.subscribers.len();
                let coalesce_id = uuid::Uuid::new_v4().to_string();

                let result = CoalesceResult {
                    coalesce_id,
                    primary_request_id: primary.request_id.clone(),
                    merged_count,
                    response: serde_json::json!({"status": "flushed"}),
                    saved_tokens: (merged_count.saturating_sub(1) as u64) * 100, // estimate
                    latency_ms: 0,
                };

                results.push(result);
                keys_to_remove.push(key_str.clone());

                self.stats.record_execute((merged_count.saturating_sub(1) as u64) * 100);
                self.stats.record_batch();
            }
        }

        for key in keys_to_remove {
            self.pending.remove(&key);
        }

        if !results.is_empty() {
            info!(flushed = results.len(), "Flushed coalesce groups");
        }

        results
    }

    /// Get coalescer statistics.
    pub fn get_stats(&self) -> CoalesceStatsSnapshot {
        self.stats.snapshot()
    }

    /// Update the coalescer configuration.
    pub fn update_config(&self, new_config: CoalesceConfig) {
        info!(
            max_batch_size = new_config.max_batch_size,
            max_wait_ms = new_config.max_wait_ms,
            enabled = new_config.enabled,
            "Coalesce config updated"
        );
        *self.config.write().unwrap() = new_config;
    }

    /// Get the current coalescer configuration.
    pub fn get_config(&self) -> CoalesceConfig {
        self.config.read().unwrap().clone()
    }

    /// Get the number of currently pending groups.
    pub fn pending_group_count(&self) -> usize {
        self.pending.len()
    }

    /// Get the total number of pending requests across all groups.
    pub fn pending_request_count(&self) -> usize {
        self.pending.iter().map(|e| e.value().len()).sum()
    }

    /// Clear all pending requests.
    pub fn clear(&self) {
        let count = self.pending.len();
        self.pending.clear();
        if count > 0 {
            info!(cleared = count, "Cleared all pending coalesce groups");
        }
    }

    /// Remove a specific request from any pending group.
    pub fn cancel_request(&self, request_id: &str) -> bool {
        let mut removed = false;
        let mut empty_keys = Vec::new();

        for mut entry in self.pending.iter_mut() {
            let group = entry.value_mut();
            if let Some(pos) = group.iter().position(|r| r.request_id == request_id) {
                group.remove(pos);
                removed = true;
                if group.is_empty() {
                    empty_keys.push(entry.key().clone());
                }
                break;
            }
            // Also check subscriber lists
            for req in group.iter_mut() {
                if let Some(sub_pos) = req.subscribers.iter().position(|s| s == request_id) {
                    req.subscribers.remove(sub_pos);
                    removed = true;
                    break;
                }
            }
        }

        for key in empty_keys {
            self.pending.remove(&key);
        }

        removed
    }
}

impl Default for RequestCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SubmitCoalesceRequest {
    pub model_id: String,
    pub prompt: String,
    pub parameters: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub priority: Option<u8>,
    pub max_age_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SubmitCoalesceResponse {
    pub request_id: String,
    pub primary_request_id: String,
    pub is_primary: bool,
    pub coalesce_key: String,
}

#[derive(Debug, Serialize)]
pub struct PendingResponse {
    pub groups: HashMap<String, Vec<CoalesceRequest>>,
    pub total_groups: usize,
    pub total_requests: usize,
}

#[derive(Debug, Serialize)]
pub struct FlushResponse {
    pub flushed: Vec<CoalesceResult>,
    pub total_flushed: usize,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCoalesceConfigRequest {
    pub max_batch_size: Option<usize>,
    pub max_wait_ms: Option<u64>,
    pub enabled: Option<bool>,
    pub similarity_threshold: Option<f64>,
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

/// POST /v1/coalesce/submit — Submit a request for coalescing
async fn submit_coalesce_handler(
    State(state): State<AppState>,
    Json(req): Json<SubmitCoalesceRequest>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    let parameters = req.parameters.unwrap_or_default();
    let payload = req.payload.unwrap_or(serde_json::json!({}));
    let priority = req.priority.unwrap_or(5);

    let coalesce_key = CoalesceKey::new(
        &req.model_id,
        &req.prompt,
        &parameters,
        req.max_age_secs,
    );

    let request = CoalesceRequest {
        request_id: request_id.clone(),
        coalesce_key: coalesce_key.clone(),
        payload,
        timestamp: Utc::now(),
        subscribers: Vec::new(),
        priority,
    };

    let (primary_id, is_primary) = state.request_coalescer.submit_request(request);

    let response = SubmitCoalesceResponse {
        request_id,
        primary_request_id: primary_id,
        is_primary,
        coalesce_key: coalesce_key.to_string_key(),
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/coalesce/pending — Get all pending coalesce groups
async fn get_pending_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let groups = state.request_coalescer.get_pending();
    let total_requests: usize = groups.values().map(|v| v.len()).sum();

    let response = PendingResponse {
        total_groups: groups.len(),
        total_requests,
        groups,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /v1/coalesce/flush — Flush all expired or full groups
async fn flush_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let flushed = state.request_coalescer.flush(None);
    let total = flushed.len();

    let response = FlushResponse {
        total_flushed: total,
        flushed,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/coalesce/stats — Get coalescer statistics
async fn get_stats_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let stats = state.request_coalescer.get_stats();
    (StatusCode::OK, Json(stats)).into_response()
}

/// GET /v1/coalesce/config — Get coalescer configuration
async fn get_config_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let config = state.request_coalescer.get_config();
    (StatusCode::OK, Json(config)).into_response()
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the request coalescing router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/coalesce/submit", post(submit_coalesce_handler))
        .route("/v1/coalesce/pending", get(get_pending_handler))
        .route("/v1/coalesce/flush", post(flush_handler))
        .route("/v1/coalesce/stats", get(get_stats_handler))
        .route("/v1/coalesce/config", get(get_config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_coalescer() -> RequestCoalescer {
        RequestCoalescer::new()
    }

    fn make_request(model: &str, prompt: &str, request_id: &str) -> CoalesceRequest {
        CoalesceRequest {
            request_id: request_id.to_string(),
            coalesce_key: CoalesceKey::new(model, prompt, "temp=0.7", None),
            payload: serde_json::json!({"model": model, "prompt": prompt}),
            timestamp: Utc::now(),
            subscribers: Vec::new(),
            priority: 5,
        }
    }

    #[test]
    fn test_coalesce_key_deterministic() {
        let key1 = CoalesceKey::new("model-a", "hello world", "temp=0.7", None);
        let key2 = CoalesceKey::new("model-a", "hello world", "temp=0.7", None);
        assert_eq!(key1.to_string_key(), key2.to_string_key());
    }

    #[test]
    fn test_coalesce_key_different_prompt() {
        let key1 = CoalesceKey::new("model-a", "hello", "temp=0.7", None);
        let key2 = CoalesceKey::new("model-a", "goodbye", "temp=0.7", None);
        assert_ne!(key1.to_string_key(), key2.to_string_key());
    }

    #[test]
    fn test_coalesce_key_different_params() {
        let key1 = CoalesceKey::new("model-a", "hello", "temp=0.7", None);
        let key2 = CoalesceKey::new("model-a", "hello", "temp=0.9", None);
        assert_ne!(key1.to_string_key(), key2.to_string_key());
    }

    #[test]
    fn test_submit_primary_request() {
        let coalescer = make_coalescer();
        let req = make_request("model-a", "hello", "req-1");
        let (primary_id, is_primary) = coalescer.submit_request(req);

        assert!(is_primary);
        assert_eq!(primary_id, "req-1");
        assert_eq!(coalescer.pending_group_count(), 1);
    }

    #[test]
    fn test_submit_coalesced_request() {
        let coalescer = make_coalescer();
        let req1 = make_request("model-a", "hello", "req-1");
        let req2 = make_request("model-a", "hello", "req-2");

        coalescer.submit_request(req1);
        let (primary_id, is_primary) = coalescer.submit_request(req2);

        assert!(!is_primary);
        assert_eq!(primary_id, "req-1");
        assert_eq!(coalescer.pending_group_count(), 1);
    }

    #[test]
    fn test_different_models_not_coalesced() {
        let coalescer = make_coalescer();
        let req1 = make_request("model-a", "hello", "req-1");
        let req2 = make_request("model-b", "hello", "req-2");

        coalescer.submit_request(req1);
        let (primary_id, is_primary) = coalescer.submit_request(req2);

        assert!(is_primary);
        assert_eq!(primary_id, "req-2");
        assert_eq!(coalescer.pending_group_count(), 2);
    }

    #[test]
    fn test_flush_expired_groups() {
        let coalescer = make_coalescer();
        let req = make_request("model-a", "hello", "req-1");
        coalescer.submit_request(req);

        // Flush with age 0 — should flush everything
        let results = coalescer.flush(Some(0));
        assert_eq!(results.len(), 1);
        assert_eq!(coalescer.pending_group_count(), 0);
    }

    #[test]
    fn test_stats_tracking() {
        let coalescer = make_coalescer();
        let req1 = make_request("model-a", "hello", "req-1");
        let req2 = make_request("model-a", "hello", "req-2");

        coalescer.submit_request(req1);
        coalescer.submit_request(req2);

        let stats = coalescer.get_stats();
        assert_eq!(stats.total_submitted, 2);
        assert_eq!(stats.total_coalesced, 1);
    }

    #[test]
    fn test_config_update() {
        let coalescer = make_coalescer();
        let mut new_config = CoalesceConfig::default();
        new_config.max_batch_size = 5;
        new_config.enabled = false;

        coalescer.update_config(new_config.clone());
        let retrieved = coalescer.get_config();

        assert_eq!(retrieved.max_batch_size, 5);
        assert!(!retrieved.enabled);
    }

    #[test]
    fn test_cancel_request() {
        let coalescer = make_coalescer();
        let req = make_request("model-a", "hello", "req-1");
        coalescer.submit_request(req);

        let removed = coalescer.cancel_request("req-1");
        assert!(removed);
        assert_eq!(coalescer.pending_group_count(), 0);
    }

    #[test]
    fn test_clear_all_pending() {
        let coalescer = make_coalescer();
        coalescer.submit_request(make_request("model-a", "hello", "req-1"));
        coalescer.submit_request(make_request("model-b", "world", "req-2"));

        assert_eq!(coalescer.pending_group_count(), 2);
        coalescer.clear();
        assert_eq!(coalescer.pending_group_count(), 0);
    }
}
