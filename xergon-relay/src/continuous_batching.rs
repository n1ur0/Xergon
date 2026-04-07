use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// PaddingStrategy
// ---------------------------------------------------------------------------

/// Determines how sequences within a batch are padded to equal length.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum PaddingStrategy {
    /// Pad every sequence to the length of the longest one in the batch.
    #[default]
    PadToLongest,
    /// Round the longest sequence length up to the next power of two.
    PadToPowerOfTwo,
    /// Round the longest sequence length up to the next multiple of *N*.
    PadToMultipleOf(u32),
}

// ---------------------------------------------------------------------------
// BatchPriority
// ---------------------------------------------------------------------------

/// Request priority.  Lower discriminant → higher priority.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy)]
pub enum BatchPriority {
    RealTime = 0,
    High = 1,
    Normal = 2,
    Low = 3,
    Background = 4,
}

impl Default for BatchPriority {
    fn default() -> Self {
        BatchPriority::Normal
    }
}

impl BatchPriority {
    /// Returns the string key used for the priority queue in the DashMap.
    pub fn queue_key(&self) -> String {
        format!("{:?}", self)
    }

    /// Return all priority levels from highest to lowest.
    pub fn all_priorities() -> &'static [BatchPriority] {
        &[
            BatchPriority::RealTime,
            BatchPriority::High,
            BatchPriority::Normal,
            BatchPriority::Low,
            BatchPriority::Background,
        ]
    }
}

// ---------------------------------------------------------------------------
// BatchStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a request or batch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum BatchStatus {
    Queued,
    Batching,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

// ---------------------------------------------------------------------------
// BatchConfig
// ---------------------------------------------------------------------------

/// Tunable knobs for the continuous batching engine.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchConfig {
    /// Maximum number of requests in a single batch.
    pub max_batch_size: u32,
    /// Maximum milliseconds to wait before emitting a batch (even if partially filled).
    pub max_wait_ms: u64,
    /// Maximum combined token budget for a single batch.
    pub max_tokens_per_batch: u32,
    /// How to pad sequences inside a batch.
    pub padding_strategy: PaddingStrategy,
    /// When true, higher-priority requests are served first.
    pub priority_enabled: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 32,
            max_wait_ms: 50,
            max_tokens_per_batch: 4096,
            padding_strategy: PaddingStrategy::PadToLongest,
            priority_enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// BatchRequest
// ---------------------------------------------------------------------------

/// A single inference request sitting in (or formerly in) the batching queue.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchRequest {
    pub id: String,
    pub prompt: String,
    pub model: String,
    pub priority: BatchPriority,
    pub tokens_estimate: u32,
    pub submitted_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
    pub status: BatchStatus,
}

impl BatchRequest {
    /// Convenience constructor that generates a UUID and stamps `submitted_at`.
    pub fn new(prompt: impl Into<String>, model: impl Into<String>, tokens_estimate: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            prompt: prompt.into(),
            model: model.into(),
            priority: BatchPriority::default(),
            tokens_estimate,
            submitted_at: Utc::now(),
            metadata: HashMap::new(),
            status: BatchStatus::Queued,
        }
    }

    /// Builder-style setter for priority.
    pub fn with_priority(mut self, priority: BatchPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Builder-style setter for metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// InferenceBatch
// ---------------------------------------------------------------------------

/// A collection of requests grouped together for a single GPU forward pass.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InferenceBatch {
    pub id: String,
    pub requests: Vec<BatchRequest>,
    pub model: String,
    pub total_tokens: u32,
    pub created_at: DateTime<Utc>,
    pub processing_started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: BatchStatus,
}

impl InferenceBatch {
    /// Constructor — callers typically set `model` / `total_tokens` themselves.
    pub fn new(requests: Vec<BatchRequest>) -> Self {
        let total_tokens: u32 = requests.iter().map(|r| r.tokens_estimate).sum();
        let model = requests
            .first()
            .map(|r| r.model.clone())
            .unwrap_or_default();
        Self {
            id: Uuid::new_v4().to_string(),
            requests,
            model,
            total_tokens,
            created_at: Utc::now(),
            processing_started_at: None,
            completed_at: None,
            status: BatchStatus::Batching,
        }
    }
}

// ---------------------------------------------------------------------------
// BatchingMetricsSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time metrics gathered from the engine.
#[derive(Serialize, Deserialize)]
pub struct BatchingMetricsSnapshot {
    pub total_requests: u64,
    pub total_batches: u64,
    pub avg_batch_size: f64,
    pub avg_wait_ms: f64,
    pub avg_processing_ms: f64,
    pub queue_depth: u64,
    pub throughput_tokens_per_sec: f64,
    pub total_tokens_processed: u64,
}

// ---------------------------------------------------------------------------
// ContinuousBatchingEngine
// ---------------------------------------------------------------------------

/// The core continuous batching engine.
///
/// Requests are submitted via [`submit`](Self::submit) or
/// [`submit_batch`](Self::submit_batch) and are placed into per-priority
/// queues.  [`form_batch`](Self::form_batch) drains queues in priority order
/// until the configured limits are reached.
pub struct ContinuousBatchingEngine {
    config: RwLock<BatchConfig>,
    /// Per-priority queues.  Key is the Debug-rendered name of the variant.
    queues: DashMap<String, VecDeque<BatchRequest>>,
    /// Currently active (in-flight) batches.
    active_batches: DashMap<String, InferenceBatch>,
    completed_batches: AtomicU64,
    total_requests: AtomicU64,
    total_tokens_processed: AtomicU64,
    total_wait_ms: AtomicU64,
    total_processing_ms: AtomicU64,
}

impl ContinuousBatchingEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: BatchConfig) -> Self {
        let mut queues = DashMap::new();
        for p in BatchPriority::all_priorities() {
            queues.insert(p.queue_key(), VecDeque::new());
        }
        Self {
            config: RwLock::new(config),
            queues,
            active_batches: DashMap::new(),
            completed_batches: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            total_tokens_processed: AtomicU64::new(0),
            total_wait_ms: AtomicU64::new(0),
            total_processing_ms: AtomicU64::new(0),
        }
    }

    // -----------------------------------------------------------------------
    // Submission
    // -----------------------------------------------------------------------

    /// Submit a single request.
    ///
    /// Returns the (now `Queued`) request on success, or an error string.
    pub fn submit(&self, mut req: BatchRequest) -> Result<BatchRequest, String> {
        let cfg = self.read_config()?;
        if req.tokens_estimate == 0 {
            return Err("tokens_estimate must be > 0".into());
        }
        if req.tokens_estimate > cfg.max_tokens_per_batch {
            return Err(format!(
                "tokens_estimate {} exceeds max_tokens_per_batch {}",
                req.tokens_estimate, cfg.max_tokens_per_batch
            ));
        }
        req.status = BatchStatus::Queued;
        req.submitted_at = Utc::now();
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let key = req.priority.queue_key();
        if let Some(mut queue) = self.queues.get_mut(&key) {
            queue.push_back(req.clone());
        }
        Ok(req)
    }

    /// Submit many requests at once.
    ///
    /// Returns `(accepted, rejected)` counts.
    pub fn submit_batch(&self, reqs: Vec<BatchRequest>) -> (u32, u32) {
        let mut accepted: u32 = 0;
        let mut rejected: u32 = 0;
        for req in reqs {
            match self.submit(req) {
                Ok(_) => accepted += 1,
                Err(_) => rejected += 1,
            }
        }
        (accepted, rejected)
    }

    // -----------------------------------------------------------------------
    // Cancellation / lookup
    // -----------------------------------------------------------------------

    /// Cancel a queued request by its id.  Returns `true` if it was found
    /// and removed from the queue.
    pub fn cancel(&self, request_id: &str) -> bool {
        for mut queue in self.queues.iter_mut() {
            let len = queue.len();
            for i in 0..len {
                if queue[i].id == request_id && queue[i].status == BatchStatus::Queued {
                    let mut req = queue.remove(i).expect("index in bounds");
                    req.status = BatchStatus::Cancelled;
                    return true;
                }
            }
        }
        false
    }

    /// Look up a request by id across all queues and active batches.
    pub fn get_request(&self, request_id: &str) -> Option<BatchRequest> {
        // Check queues first
        for queue in self.queues.iter() {
            for req in queue.iter() {
                if req.id == request_id {
                    return Some(req.clone());
                }
            }
        }
        // Check active batches
        for batch in self.active_batches.iter() {
            for req in &batch.requests {
                if req.id == request_id {
                    return Some(req.clone());
                }
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Batch formation
    // -----------------------------------------------------------------------

    /// Drain queues in priority order and form a batch up to the configured
    /// limits.  Returns `None` when all queues are empty.
    pub fn form_batch(&self) -> Option<InferenceBatch> {
        let cfg = self.read_config().ok()?;
        let mut collected: Vec<BatchRequest> = Vec::new();
        let mut token_budget = cfg.max_tokens_per_batch;

        let priorities: Vec<BatchPriority> = if cfg.priority_enabled {
            BatchPriority::all_priorities().to_vec()
        } else {
            // When priority is disabled, treat everything as Normal.
            vec![BatchPriority::Normal]
        };

        for priority in &priorities {
            if collected.len() as u32 >= cfg.max_batch_size {
                break;
            }
            let key = priority.queue_key();
            if let Some(mut queue) = self.queues.get_mut(&key) {
                while let Some(mut req) = queue.pop_front() {
                    if collected.len() as u32 >= cfg.max_batch_size {
                        // Put it back at the front
                        queue.push_front(req);
                        break;
                    }
                    if req.tokens_estimate > token_budget {
                        // This single request exceeds remaining budget — skip
                        // but do NOT put it back to avoid infinite loops; instead
                        // we park it in a temporary overflow.
                        // For simplicity we drop it back to the queue front.
                        queue.push_front(req);
                        break;
                    }
                    token_budget -= req.tokens_estimate;
                    req.status = BatchStatus::Batching;
                    collected.push(req);
                }
            }
        }

        if collected.is_empty() {
            return None;
        }

        let batch = InferenceBatch::new(collected);
        self.active_batches
            .insert(batch.id.clone(), batch.clone());
        Some(batch)
    }

    // -----------------------------------------------------------------------
    // Batch completion
    // -----------------------------------------------------------------------

    /// Mark an active batch as completed, recording timing metrics.
    pub fn complete_batch(&self, batch_id: &str) -> Result<InferenceBatch, String> {
        let mut batch = self
            .active_batches
            .get_mut(batch_id)
            .ok_or_else(|| format!("batch {} not found in active batches", batch_id))?;

        let now = Utc::now();
        batch.processing_started_at = Some(now);
        batch.status = BatchStatus::Completed;
        batch.completed_at = Some(now);

        // Record metrics
        self.completed_batches.fetch_add(1, Ordering::Relaxed);
        self.total_tokens_processed
            .fetch_add(batch.total_tokens as u64, Ordering::Relaxed);

        let batch_clone = batch.clone();
        drop(batch);
        self.active_batches.remove(batch_id);
        Ok(batch_clone)
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// List all currently active (in-flight) batches.
    pub fn get_active_batches(&self) -> Vec<InferenceBatch> {
        self.active_batches.iter().map(|r| r.value().clone()).collect()
    }

    /// Number of requests in the queue for a given priority.
    pub fn get_queue_depth(&self, priority: &BatchPriority) -> usize {
        self.queues
            .get(&priority.queue_key())
            .map(|q| q.len())
            .unwrap_or(0)
    }

    /// Total number of requests across all priority queues.
    pub fn get_total_queue_depth(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }

    /// Snapshot of current engine metrics.
    pub fn get_metrics(&self) -> BatchingMetricsSnapshot {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let total_batches = self.completed_batches.load(Ordering::Relaxed);
        let total_tokens = self.total_tokens_processed.load(Ordering::Relaxed);
        let total_wait = self.total_wait_ms.load(Ordering::Relaxed);
        let total_proc = self.total_processing_ms.load(Ordering::Relaxed);
        let queue_depth = self.get_total_queue_depth() as u64;

        let avg_batch_size = if total_batches > 0 {
            (total_requests as f64) / (total_batches as f64)
        } else {
            0.0
        };
        let avg_wait_ms = if total_requests > 0 {
            (total_wait as f64) / (total_requests as f64)
        } else {
            0.0
        };
        let avg_processing_ms = if total_batches > 0 {
            (total_proc as f64) / (total_batches as f64)
        } else {
            0.0
        };

        // Throughput: tokens / seconds since epoch (rough proxy).
        let now_ts = Utc::now().timestamp_millis() as f64 / 1000.0;
        let throughput_tokens_per_sec = if now_ts > 0.0 {
            total_tokens as f64 / now_ts
        } else {
            0.0
        };

        BatchingMetricsSnapshot {
            total_requests,
            total_batches,
            avg_batch_size,
            avg_wait_ms,
            avg_processing_ms,
            queue_depth,
            throughput_tokens_per_sec,
            total_tokens_processed: total_tokens,
        }
    }

    /// Atomically replace the engine configuration.
    pub fn update_config(&self, config: BatchConfig) -> BatchConfig {
        let mut guard = self.config.write().expect("config lock poisoned");
        let old = guard.clone();
        *guard = config;
        old
    }

    /// Read a clone of the current configuration.
    pub fn get_config(&self) -> BatchConfig {
        self.read_config().unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn read_config(&self) -> Result<BatchConfig, String> {
        self.config
            .read()
            .map(|g| g.clone())
            .map_err(|_| "config lock poisoned".into())
    }
}

// ---------------------------------------------------------------------------
// Padding helper (used externally or in tests)
// ---------------------------------------------------------------------------

/// Compute the padded length for a batch based on the configured strategy.
pub fn compute_padded_length(strategy: &PaddingStrategy, max_seq_len: u32) -> u32 {
    match strategy {
        PaddingStrategy::PadToLongest => max_seq_len.max(1),
        PaddingStrategy::PadToPowerOfTwo => {
            if max_seq_len <= 1 {
                return 1;
            }
            let bits = 32u32 - max_seq_len.leading_zeros();
            // next power of two ≥ max_seq_len
            if max_seq_len.is_power_of_two() {
                max_seq_len
            } else {
                1u32 << bits
            }
        }
        PaddingStrategy::PadToMultipleOf(n) => {
            let n = (*n).max(1);
            ((max_seq_len as f64 / n as f64).ceil() as u32).max(1) * n
        }
    }
}

// ---------------------------------------------------------------------------
// Axum router
// ---------------------------------------------------------------------------

/// Build the batching API router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/batching/requests", post(submit_request))
        .route("/api/batching/requests/batch", post(submit_requests_batch))
        .route("/api/batching/requests/:id", delete(cancel_request))
        .route("/api/batching/requests/:id", get(get_request))
        .route("/api/batching/form", post(form_batch))
        .route("/api/batching/batches/:id/complete", post(complete_batch))
        .route("/api/batching/batches", get(get_active_batches))
        .route("/api/batching/queue", get(get_queue))
        .route("/api/batching/metrics", get(get_metrics))
        .route("/api/batching/config", put(update_config))
        .route("/api/batching/config", get(get_config))
        .with_state(state)
}

// --- handlers ---

async fn submit_request(
    State(_state): State<AppState>,
    Json(req): Json<BatchRequest>,
) -> Result<(StatusCode, Json<BatchRequest>), (StatusCode, Json<serde_json::Value>)> {
    // For handler purposes we use a placeholder engine; the real integration
    // would store the engine inside AppState.  Here we validate the request
    // shape and return it stamped.
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    match engine.submit(req) {
        Ok(stamped) => Ok((StatusCode::OK, Json(stamped))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )),
    }
}

async fn submit_requests_batch(
    State(_state): State<AppState>,
    Json(reqs): Json<Vec<BatchRequest>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    let (accepted, rejected) = engine.submit_batch(reqs);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "accepted": accepted, "rejected": rejected })),
    )
}

async fn cancel_request(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    let cancelled = engine.cancel(&id);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "cancelled": cancelled })),
    )
}

async fn get_request(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<BatchRequest>), (StatusCode, Json<serde_json::Value>)> {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    match engine.get_request(&id) {
        Some(req) => Ok((StatusCode::OK, Json(req))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "request not found" })),
        )),
    }
}

async fn form_batch(
    State(_state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    match engine.form_batch() {
        Some(batch) => (StatusCode::OK, Json(serde_json::json!(batch))),
        None => (
            StatusCode::NO_CONTENT,
            Json(serde_json::json!({ "message": "no requests to batch" })),
        ),
    }
}

async fn complete_batch(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<InferenceBatch>), (StatusCode, Json<serde_json::Value>)> {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    match engine.complete_batch(&id) {
        Ok(batch) => Ok((StatusCode::OK, Json(batch))),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e })),
        )),
    }
}

async fn get_active_batches(
    State(_state): State<AppState>,
) -> (StatusCode, Json<Vec<InferenceBatch>>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    (StatusCode::OK, Json(engine.get_active_batches()))
}

async fn get_queue(
    State(_state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    let mut map = serde_json::Map::new();
    for p in BatchPriority::all_priorities() {
        map.insert(
            p.queue_key(),
            serde_json::json!(engine.get_queue_depth(p)),
        );
    }
    (StatusCode::OK, Json(serde_json::Value::Object(map)))
}

async fn get_metrics(
    State(_state): State<AppState>,
) -> (StatusCode, Json<BatchingMetricsSnapshot>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    (StatusCode::OK, Json(engine.get_metrics()))
}

async fn update_config(
    State(_state): State<AppState>,
    Json(cfg): Json<BatchConfig>,
) -> (StatusCode, Json<BatchConfig>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    engine.update_config(cfg.clone());
    (StatusCode::OK, Json(cfg))
}

async fn get_config(
    State(_state): State<AppState>,
) -> (StatusCode, Json<BatchConfig>) {
    let engine = ContinuousBatchingEngine::new(BatchConfig::default());
    (StatusCode::OK, Json(engine.get_config()))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // -- helpers --

    fn default_engine() -> ContinuousBatchingEngine {
        ContinuousBatchingEngine::new(BatchConfig::default())
    }

    fn make_request(tokens: u32) -> BatchRequest {
        BatchRequest::new("hello world", "test-model", tokens)
    }

    fn make_request_with_priority(tokens: u32, priority: BatchPriority) -> BatchRequest {
        make_request(tokens).with_priority(priority)
    }

    // ======================================================================
    // PaddingStrategy tests
    // ======================================================================

    #[test]
    fn test_padding_strategy_default_is_pad_to_longest() {
        let s: PaddingStrategy = Default::default();
        assert_eq!(s, PaddingStrategy::PadToLongest);
    }

    #[test]
    fn test_compute_padded_length_pad_to_longest() {
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToLongest, 10), 10);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToLongest, 0), 1);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToLongest, 1), 1);
    }

    #[test]
    fn test_compute_padded_length_pad_to_power_of_two() {
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToPowerOfTwo, 1), 1);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToPowerOfTwo, 5), 8);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToPowerOfTwo, 8), 8);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToPowerOfTwo, 9), 16);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToPowerOfTwo, 0), 1);
    }

    #[test]
    fn test_compute_padded_length_pad_to_multiple_of() {
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToMultipleOf(8), 10), 16);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToMultipleOf(8), 8), 8);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToMultipleOf(8), 1), 8);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToMultipleOf(32), 33), 64);
        assert_eq!(compute_padded_length(&PaddingStrategy::PadToMultipleOf(1), 10), 10);
    }

    // ======================================================================
    // BatchPriority tests
    // ======================================================================

    #[test]
    fn test_batch_priority_ordering() {
        assert!(BatchPriority::RealTime < BatchPriority::High);
        assert!(BatchPriority::High < BatchPriority::Normal);
        assert!(BatchPriority::Normal < BatchPriority::Low);
        assert!(BatchPriority::Low < BatchPriority::Background);
    }

    #[test]
    fn test_batch_priority_queue_key() {
        assert_eq!(BatchPriority::RealTime.queue_key(), "RealTime");
        assert_eq!(BatchPriority::Background.queue_key(), "Background");
    }

    #[test]
    fn test_batch_priority_default_is_normal() {
        assert_eq!(BatchPriority::default(), BatchPriority::Normal);
    }

    #[test]
    fn test_batch_priority_all_priorities_sorted() {
        let all = BatchPriority::all_priorities();
        for w in all.windows(2) {
            assert!(w[0] < w[1]);
        }
    }

    // ======================================================================
    // BatchConfig tests
    // ======================================================================

    #[test]
    fn test_batch_config_defaults() {
        let cfg = BatchConfig::default();
        assert_eq!(cfg.max_batch_size, 32);
        assert_eq!(cfg.max_wait_ms, 50);
        assert_eq!(cfg.max_tokens_per_batch, 4096);
        assert_eq!(cfg.padding_strategy, PaddingStrategy::PadToLongest);
        assert!(cfg.priority_enabled);
    }

    #[test]
    fn test_batch_config_serialization_roundtrip() {
        let cfg = BatchConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: BatchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_batch_size, cfg.max_batch_size);
    }

    // ======================================================================
    // BatchRequest tests
    // ======================================================================

    #[test]
    fn test_batch_request_new_generates_id() {
        let req = make_request(100);
        assert!(!req.id.is_empty());
        assert_eq!(req.status, BatchStatus::Queued);
        assert_eq!(req.tokens_estimate, 100);
        assert_eq!(req.model, "test-model");
    }

    #[test]
    fn test_batch_request_with_priority() {
        let req = make_request(50).with_priority(BatchPriority::RealTime);
        assert_eq!(req.priority, BatchPriority::RealTime);
    }

    #[test]
    fn test_batch_request_with_metadata() {
        let req = make_request(50).with_metadata("user_id", "42");
        assert_eq!(req.metadata.get("user_id").unwrap(), "42");
    }

    #[test]
    fn test_batch_request_serialization() {
        let req = make_request(100);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("hello world"));
        let _decoded: BatchRequest = serde_json::from_str(&json).unwrap();
    }

    // ======================================================================
    // ContinuousBatchingEngine — submit
    // ======================================================================

    #[test]
    fn test_submit_single_request() {
        let engine = default_engine();
        let req = make_request(100);
        let result = engine.submit(req).unwrap();
        assert_eq!(result.status, BatchStatus::Queued);
        assert_eq!(engine.get_total_queue_depth(), 1);
    }

    #[test]
    fn test_submit_rejects_zero_tokens() {
        let engine = default_engine();
        let req = BatchRequest::new("hi", "m", 0);
        let err = engine.submit(req).unwrap_err();
        assert!(err.contains("tokens_estimate must be > 0"));
        assert_eq!(engine.get_total_queue_depth(), 0);
    }

    #[test]
    fn test_submit_rejects_exceeds_max_tokens() {
        let engine = default_engine(); // max_tokens_per_batch = 4096
        let req = make_request(5000);
        let err = engine.submit(req).unwrap_err();
        assert!(err.contains("exceeds max_tokens_per_batch"));
    }

    #[test]
    fn test_submit_batch_mixed() {
        let engine = default_engine();
        let reqs = vec![
            make_request(100),
            BatchRequest::new("hi", "m", 0),  // will be rejected
            make_request(200),
        ];
        let (accepted, rejected) = engine.submit_batch(reqs);
        assert_eq!(accepted, 2);
        assert_eq!(rejected, 1);
        assert_eq!(engine.get_total_queue_depth(), 2);
    }

    // ======================================================================
    // ContinuousBatchingEngine — cancel
    // ======================================================================

    #[test]
    fn test_cancel_existing_request() {
        let engine = default_engine();
        let req = engine.submit(make_request(100)).unwrap();
        assert_eq!(engine.get_total_queue_depth(), 1);
        let cancelled = engine.cancel(&req.id);
        assert!(cancelled);
        assert_eq!(engine.get_total_queue_depth(), 0);
    }

    #[test]
    fn test_cancel_nonexistent_request() {
        let engine = default_engine();
        let cancelled = engine.cancel("no-such-id");
        assert!(!cancelled);
    }

    // ======================================================================
    // ContinuousBatchingEngine — get_request
    // ======================================================================

    #[test]
    fn test_get_request_from_queue() {
        let engine = default_engine();
        let req = engine.submit(make_request(100)).unwrap();
        let found = engine.get_request(&req.id).unwrap();
        assert_eq!(found.id, req.id);
    }

    #[test]
    fn test_get_request_not_found() {
        let engine = default_engine();
        assert!(engine.get_request("nope").is_none());
    }

    // ======================================================================
    // ContinuousBatchingEngine — form_batch
    // ======================================================================

    #[test]
    fn test_form_batch_basic() {
        let engine = default_engine();
        engine.submit(make_request(100)).unwrap();
        engine.submit(make_request(200)).unwrap();
        let batch = engine.form_batch().unwrap();
        assert_eq!(batch.requests.len(), 2);
        assert_eq!(batch.total_tokens, 300);
        assert_eq!(batch.status, BatchStatus::Batching);
        assert_eq!(engine.get_total_queue_depth(), 0);
    }

    #[test]
    fn test_form_batch_empty_returns_none() {
        let engine = default_engine();
        assert!(engine.form_batch().is_none());
    }

    #[test]
    fn test_form_batch_respects_max_batch_size() {
        let cfg = BatchConfig {
            max_batch_size: 3,
            ..Default::default()
        };
        let engine = ContinuousBatchingEngine::new(cfg);
        for _ in 0..6 {
            engine.submit(make_request(10)).unwrap();
        }
        let batch = engine.form_batch().unwrap();
        assert_eq!(batch.requests.len(), 3);
        assert_eq!(engine.get_total_queue_depth(), 3);
    }

    #[test]
    fn test_form_batch_respects_max_tokens() {
        let cfg = BatchConfig {
            max_tokens_per_batch: 100,
            ..Default::default()
        };
        let engine = ContinuousBatchingEngine::new(cfg);
        engine.submit(make_request(60)).unwrap();
        engine.submit(make_request(60)).unwrap();
        // First request (60) fits; second (60) would push to 120 > 100.
        let batch = engine.form_batch().unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert_eq!(batch.total_tokens, 60);
        assert_eq!(engine.get_total_queue_depth(), 1);
    }

    // ======================================================================
    // ContinuousBatchingEngine — priority ordering
    // ======================================================================

    #[test]
    fn test_form_batch_priority_ordering() {
        let engine = default_engine();
        // Submit in reverse priority order
        engine
            .submit(make_request_with_priority(50, BatchPriority::Background))
            .unwrap();
        engine
            .submit(make_request_with_priority(50, BatchPriority::Low))
            .unwrap();
        engine
            .submit(make_request_with_priority(50, BatchPriority::RealTime))
            .unwrap();

        let batch = engine.form_batch().unwrap();
        // With default max_batch_size=32, all 3 should fit.
        assert_eq!(batch.requests.len(), 3);
        // First should be RealTime
        assert_eq!(batch.requests[0].priority, BatchPriority::RealTime);
    }

    #[test]
    fn test_form_batch_priority_disabled_treats_all_normal() {
        let cfg = BatchConfig {
            priority_enabled: false,
            ..Default::default()
        };
        let engine = ContinuousBatchingEngine::new(cfg);
        engine
            .submit(make_request_with_priority(50, BatchPriority::RealTime))
            .unwrap();
        engine
            .submit(make_request_with_priority(50, BatchPriority::Background))
            .unwrap();
        // Both should be batched
        let batch = engine.form_batch().unwrap();
        assert_eq!(batch.requests.len(), 2);
    }

    // ======================================================================
    // ContinuousBatchingEngine — complete_batch
    // ======================================================================

    #[test]
    fn test_complete_batch() {
        let engine = default_engine();
        engine.submit(make_request(100)).unwrap();
        let batch = engine.form_batch().unwrap();
        assert_eq!(engine.get_active_batches().len(), 1);
        let completed = engine.complete_batch(&batch.id).unwrap();
        assert_eq!(completed.status, BatchStatus::Completed);
        assert!(completed.completed_at.is_some());
        assert!(completed.processing_started_at.is_some());
        assert_eq!(engine.get_active_batches().len(), 0);
        assert_eq!(engine.completed_batches.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_complete_nonexistent_batch() {
        let engine = default_engine();
        let err = engine.complete_batch("nope").unwrap_err();
        assert!(err.contains("not found"));
    }

    // ======================================================================
    // ContinuousBatchingEngine — metrics
    // ======================================================================

    #[test]
    fn test_metrics_initial_state() {
        let engine = default_engine();
        let m = engine.get_metrics();
        assert_eq!(m.total_requests, 0);
        assert_eq!(m.total_batches, 0);
        assert_eq!(m.queue_depth, 0);
    }

    #[test]
    fn test_metrics_after_submit_and_complete() {
        let engine = default_engine();
        engine.submit(make_request(100)).unwrap();
        engine.submit(make_request(200)).unwrap();
        let batch = engine.form_batch().unwrap();
        engine.complete_batch(&batch.id).unwrap();
        let m = engine.get_metrics();
        assert_eq!(m.total_requests, 2);
        assert_eq!(m.total_batches, 1);
        assert_eq!(m.total_tokens_processed, 300);
        assert!(m.avg_batch_size > 0.0);
    }

    #[test]
    fn test_metrics_queue_depth() {
        let engine = default_engine();
        engine.submit(make_request(10)).unwrap();
        engine.submit(make_request(10)).unwrap();
        engine.submit(make_request(10)).unwrap();
        let m = engine.get_metrics();
        assert_eq!(m.queue_depth, 3);
    }

    // ======================================================================
    // ContinuousBatchingEngine — config
    // ======================================================================

    #[test]
    fn test_get_config() {
        let engine = default_engine();
        let cfg = engine.get_config();
        assert_eq!(cfg.max_batch_size, 32);
    }

    #[test]
    fn test_update_config() {
        let engine = default_engine();
        let new_cfg = BatchConfig {
            max_batch_size: 64,
            max_wait_ms: 100,
            ..Default::default()
        };
        let old = engine.update_config(new_cfg.clone());
        assert_eq!(old.max_batch_size, 32);
        let current = engine.get_config();
        assert_eq!(current.max_batch_size, 64);
        assert_eq!(current.max_wait_ms, 100);
    }

    // ======================================================================
    // ContinuousBatchingEngine — queue depth
    // ======================================================================

    #[test]
    fn test_get_queue_depth_per_priority() {
        let engine = default_engine();
        engine
            .submit(make_request_with_priority(50, BatchPriority::RealTime))
            .unwrap();
        engine
            .submit(make_request_with_priority(50, BatchPriority::RealTime))
            .unwrap();
        engine
            .submit(make_request_with_priority(50, BatchPriority::Low))
            .unwrap();
        assert_eq!(engine.get_queue_depth(&BatchPriority::RealTime), 2);
        assert_eq!(engine.get_queue_depth(&BatchPriority::Low), 1);
        assert_eq!(engine.get_queue_depth(&BatchPriority::High), 0);
        assert_eq!(engine.get_total_queue_depth(), 3);
    }

    // ======================================================================
    // InferenceBatch tests
    // ======================================================================

    #[test]
    fn test_inference_batch_new() {
        let reqs = vec![make_request(100), make_request(200)];
        let batch = InferenceBatch::new(reqs);
        assert!(!batch.id.is_empty());
        assert_eq!(batch.total_tokens, 300);
        assert_eq!(batch.model, "test-model");
        assert_eq!(batch.status, BatchStatus::Batching);
        assert!(batch.completed_at.is_none());
    }

    #[test]
    fn test_inference_batch_serialization() {
        let batch = InferenceBatch::new(vec![make_request(50)]);
        let json = serde_json::to_string(&batch).unwrap();
        assert!(json.contains("test-model"));
        let _decoded: InferenceBatch = serde_json::from_str(&json).unwrap();
    }

    // ======================================================================
    // Concurrency smoke test
    // ======================================================================

    #[test]
    fn test_concurrent_submits() {
        let engine = Arc::new(default_engine());
        let mut handles = Vec::new();
        for _ in 0..10 {
            let eng = Arc::clone(&engine);
            handles.push(thread::spawn(move || {
                eng.submit(make_request(50)).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(engine.get_total_queue_depth(), 10);
        assert_eq!(engine.total_requests.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn test_concurrent_submit_and_cancel() {
        let engine = Arc::new(default_engine());
        // Submit 5 requests and collect their ids
        let mut ids: Vec<String> = Vec::new();
        for _ in 0..5 {
            let req = engine.submit(make_request(50)).unwrap();
            ids.push(req.id);
        }
        // Cancel from another thread
        let eng = Arc::clone(&engine);
        let cancel_ids = ids.clone();
        let handle = thread::spawn(move || {
            for id in cancel_ids {
                eng.cancel(&id);
            }
        });
        handle.join().unwrap();
        assert_eq!(engine.get_total_queue_depth(), 0);
    }

    // ======================================================================
    // BatchingMetricsSnapshot serialization
    // ======================================================================

    #[test]
    fn test_metrics_snapshot_serialization() {
        let snap = BatchingMetricsSnapshot {
            total_requests: 100,
            total_batches: 10,
            avg_batch_size: 10.0,
            avg_wait_ms: 25.0,
            avg_processing_ms: 100.0,
            queue_depth: 5,
            throughput_tokens_per_sec: 42.5,
            total_tokens_processed: 1000,
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("total_requests"));
        let _decoded: BatchingMetricsSnapshot = serde_json::from_str(&json).unwrap();
    }

    // ======================================================================
    // Edge cases
    // ======================================================================

    #[test]
    fn test_form_batch_single_request_at_token_limit() {
        let cfg = BatchConfig {
            max_tokens_per_batch: 100,
            ..Default::default()
        };
        let engine = ContinuousBatchingEngine::new(cfg);
        engine.submit(make_request(100)).unwrap();
        engine.submit(make_request(1)).unwrap();
        let batch = engine.form_batch().unwrap();
        // The 100-token request exactly fills the budget.
        assert_eq!(batch.requests.len(), 1);
        assert_eq!(batch.total_tokens, 100);
        assert_eq!(engine.get_total_queue_depth(), 1);
    }

    #[test]
    fn test_multiple_form_and_complete_cycles() {
        let engine = default_engine();
        // Round 1
        engine.submit(make_request(50)).unwrap();
        let b1 = engine.form_batch().unwrap();
        engine.complete_batch(&b1.id).unwrap();
        // Round 2
        engine.submit(make_request(75)).unwrap();
        engine.submit(make_request(25)).unwrap();
        let b2 = engine.form_batch().unwrap();
        engine.complete_batch(&b2.id).unwrap();

        assert_eq!(engine.completed_batches.load(Ordering::Relaxed), 2);
        assert_eq!(
            engine.total_tokens_processed.load(Ordering::Relaxed),
            150
        );
    }

    #[test]
    fn test_batch_status_equality() {
        assert_eq!(BatchStatus::Queued, BatchStatus::Queued);
        assert_ne!(BatchStatus::Queued, BatchStatus::Completed);
    }

    #[test]
    fn test_padding_strategy_clone_and_equality() {
        let a = PaddingStrategy::PadToPowerOfTwo;
        let b = a.clone();
        assert_eq!(a, b);
        let c = PaddingStrategy::PadToMultipleOf(16);
        assert_ne!(a, c);
    }
}
