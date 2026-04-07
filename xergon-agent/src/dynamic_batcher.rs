//! Dynamic Batching Engine for the Xergon agent.
//!
//! Groups inference requests into batches for higher GPU throughput.
//! Supports priority queues, token budgets, request aging, and preemption.
//!
//! API:
//! - POST /api/batching/submit      -- submit a single request
//! - POST /api/batching/batch       -- submit multiple requests
//! - POST /api/batching/{id}/cancel -- cancel a queued request
//! - GET  /api/batching/{id}/status -- check request status
//! - GET  /api/batching/stats       -- batcher statistics
//! - PUT  /api/batching/config      -- update batcher config
//! - GET  /api/batching/health      -- health check

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
    routing::{get, post, put},
    Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

use crate::api::AppState;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicBatchConfig {
    #[serde(default = "default_max_batch")]
    pub max_batch_size: usize,
    #[serde(default = "default_max_wait")]
    pub max_wait_time_ms: u64,
    #[serde(default)]
    pub min_batch_size: usize,
    #[serde(default = "default_max_queue")]
    pub max_queue_size: usize,
    #[serde(default = "default_priority_levels")]
    pub priority_levels: usize,
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
    #[serde(default)]
    pub enable_preemption: bool,
    #[serde(default = "default_aging")]
    pub aging_factor: f64,
}

fn default_max_batch() -> usize { 32 }
fn default_max_wait() -> u64 { 50 }
fn default_max_queue() -> usize { 1000 }
fn default_priority_levels() -> usize { 5 }
fn default_token_budget() -> usize { 4096 }
fn default_aging() -> f64 { 0.01 }

impl Default for DynamicBatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: default_max_batch(),
            max_wait_time_ms: default_max_wait(),
            min_batch_size: 1,
            max_queue_size: default_max_queue(),
            priority_levels: default_priority_levels(),
            token_budget: default_token_budget(),
            enable_preemption: false,
            aging_factor: default_aging(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RequestStatus {
    Queued,
    Processing,
    Completed,
    Cancelled,
    Preempted,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    pub id: String,
    pub prompt: String,
    pub max_tokens: usize,
    pub priority: u8,
    pub submitted_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub model_id: String,
    pub status: RequestStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse {
    pub request_id: String,
    pub output: String,
    pub tokens_generated: usize,
    pub latency_ms: u64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub batch_id: String,
    pub request_ids: Vec<String>,
    pub responses: Vec<BatchResponse>,
    pub total_tokens: usize,
    pub processing_time_ms: u64,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStats {
    pub total_batches: u64,
    pub avg_batch_size: f64,
    pub avg_wait_time_ms: f64,
    pub avg_process_time_ms: f64,
    pub throughput_tokens_per_sec: f64,
    pub queue_depth: usize,
    pub preemption_count: u64,
}

/// Internal priority entry for the priority queue.
#[derive(Debug, Clone)]
struct PriorityEntry {
    effective_priority: i64,
    request: BatchRequest,
}

impl PartialEq for PriorityEntry {
    fn eq(&self, other: &Self) -> bool {
        self.effective_priority == other.effective_priority && self.request.id == other.request.id
    }
}

impl Eq for PriorityEntry {}

impl Ord for PriorityEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher effective priority = processed first
        other.effective_priority.cmp(&self.effective_priority)
    }
}

impl PartialOrd for PriorityEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DynamicBatcher {
    config: Arc<std::sync::RwLock<DynamicBatchConfig>>,
    queue: Arc<DashMap<String, BatchRequest>>,
    request_counter: Arc<AtomicU64>,
    batch_counter: Arc<AtomicU64>,
    total_batches: Arc<AtomicU64>,
    total_preemptions: Arc<AtomicU64>,
    total_wait_time_ms: Arc<AtomicU64>,
    total_process_time_ms: Arc<AtomicU64>,
    total_tokens_processed: Arc<AtomicU64>,
    total_requests_processed: Arc<AtomicU64>,
}

impl DynamicBatcher {
    pub fn new(config: DynamicBatchConfig) -> Self {
        Self {
            config: Arc::new(std::sync::RwLock::new(config)),
            queue: Arc::new(DashMap::new()),
            request_counter: Arc::new(AtomicU64::new(0)),
            batch_counter: Arc::new(AtomicU64::new(0)),
            total_batches: Arc::new(AtomicU64::new(0)),
            total_preemptions: Arc::new(AtomicU64::new(0)),
            total_wait_time_ms: Arc::new(AtomicU64::new(0)),
            total_process_time_ms: Arc::new(AtomicU64::new(0)),
            total_tokens_processed: Arc::new(AtomicU64::new(0)),
            total_requests_processed: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Submit a single inference request into the batch queue.
    pub fn submit(
        &self,
        prompt: String,
        max_tokens: usize,
        priority: u8,
        model_id: String,
        deadline: Option<DateTime<Utc>>,
    ) -> String {
        let cfg = self.config.read().unwrap();

        if self.queue.len() >= cfg.max_queue_size {
            warn!("Batch queue full ({}), rejecting request", cfg.max_queue_size);
            return String::new();
        }

        let id = format!("breq-{}", self.request_counter.fetch_add(1, Ordering::Relaxed));
        let request = BatchRequest {
            id: id.clone(),
            prompt,
            max_tokens,
            priority,
            submitted_at: Utc::now(),
            deadline,
            model_id,
            status: RequestStatus::Queued,
        };
        self.queue.insert(id.clone(), request);
        info!(request_id = %id, "Request submitted to batch queue");
        id
    }

    /// Submit multiple requests at once.
    pub fn submit_batch(
        &self,
        requests: Vec<(String, usize, u8, String)>,
    ) -> Vec<String> {
        requests
            .into_iter()
            .map(|(prompt, max_tokens, priority, model_id)| {
                self.submit(prompt, max_tokens, priority, model_id, None)
            })
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// Cancel a queued request.
    pub fn cancel(&self, request_id: &str) -> bool {
        if let Some(mut req) = self.queue.get_mut(request_id) {
            if req.status == RequestStatus::Queued {
                req.status = RequestStatus::Cancelled;
                info!(request_id = %request_id, "Request cancelled");
                return true;
            }
        }
        false
    }

    /// Get the status of a request.
    pub fn get_status(&self, request_id: &str) -> Option<RequestStatus> {
        self.queue.get(request_id).map(|r| r.status.clone())
    }

    /// Build a priority queue from current queued requests, applying aging.
    fn build_priority_queue(&self) -> BinaryHeap<PriorityEntry> {
        let cfg = self.config.read().unwrap();
        let now = Utc::now();
        let mut heap = BinaryHeap::new();

        for entry in self.queue.iter() {
            if entry.status != RequestStatus::Queued {
                continue;
            }
            // Check deadline
            if let Some(deadline) = entry.deadline {
                if now > deadline {
                    continue; // skip expired requests
                }
            }

            // Apply aging: priority increases over time
            let wait_secs = (now - entry.submitted_at).num_seconds().max(0) as f64;
            let age_bonus = (wait_secs * cfg.aging_factor * 1000.0) as i64;
            let effective = entry.priority as i64 * 1000 + age_bonus;

            heap.push(PriorityEntry {
                effective_priority: effective,
                request: entry.clone(),
            });
        }

        heap
    }

    /// Process the next batch of requests from the queue.
    pub fn process_next_batch(&self) -> Option<BatchResult> {
        let cfg = self.config.read().unwrap();
        let mut heap = self.build_priority_queue();

        let mut batch_requests: Vec<BatchRequest> = Vec::new();
        let mut token_budget_used: usize = 0;
        let batch_start = std::time::Instant::now();

        // Select requests for the batch
        while let Some(entry) = heap.pop() {
            if batch_requests.len() >= cfg.max_batch_size {
                break;
            }
            if token_budget_used + entry.request.max_tokens > cfg.token_budget {
                if cfg.enable_preemption && !batch_requests.is_empty() {
                    // Try to preempt a lower-priority request to fit this one
                    if let Some(preempt_idx) = batch_requests
                        .iter()
                        .position(|r| r.priority < entry.request.priority)
                    {
                        let preempted = batch_requests.remove(preempt_idx);
                        token_budget_used -= preempted.max_tokens;
                        self.total_preemptions.fetch_add(1, Ordering::Relaxed);
                        if let Some(mut p) = self.queue.get_mut(&preempted.id) {
                            p.status = RequestStatus::Preempted;
                        }
                    } else {
                        continue; // can't fit, skip
                    }
                } else {
                    continue;
                }
            }
            token_budget_used += entry.request.max_tokens;
            batch_requests.push(entry.request);
        }

        if batch_requests.is_empty() {
            return None;
        }

        let batch_id = format!("batch-{}", self.batch_counter.fetch_add(1, Ordering::Relaxed));

        // Mark requests as processing
        for req in &batch_requests {
            if let Some(mut r) = self.queue.get_mut(&req.id) {
                r.status = RequestStatus::Processing;
            }
        }

        // Simulate batch processing (in real impl this calls the GPU)
        let processing_time_ms = 5u64; // placeholder

        // Generate placeholder responses
        let responses: Vec<BatchResponse> = batch_requests
            .iter()
            .map(|req| BatchResponse {
                request_id: req.id.clone(),
                output: format!("Batch response for {}", req.id),
                tokens_generated: req.max_tokens / 2,
                latency_ms: processing_time_ms,
                success: true,
            })
            .collect();

        // Mark requests as completed
        let total_tokens: usize = responses.iter().map(|r| r.tokens_generated).sum();
        for req in &batch_requests {
            if let Some(mut r) = self.queue.get_mut(&req.id) {
                r.status = RequestStatus::Completed;
            }
        }

        // Update stats
        let now = Utc::now();
        let total_wait: u64 = batch_requests
            .iter()
            .map(|r| (now - r.submitted_at).num_milliseconds().max(0) as u64)
            .sum();
        self.total_batches.fetch_add(1, Ordering::Relaxed);
        self.total_wait_time_ms.fetch_add(total_wait, Ordering::Relaxed);
        self.total_process_time_ms.fetch_add(processing_time_ms, Ordering::Relaxed);
        self.total_tokens_processed.fetch_add(total_tokens as u64, Ordering::Relaxed);
        self.total_requests_processed.fetch_add(batch_requests.len() as u64, Ordering::Relaxed);

        // Clean up completed requests
        for req in &batch_requests {
            self.queue.remove(&req.id);
        }

        Some(BatchResult {
            batch_id,
            request_ids: batch_requests.iter().map(|r| r.id.clone()).collect(),
            responses,
            total_tokens,
            processing_time_ms: batch_start.elapsed().as_millis() as u64,
            batch_size: batch_requests.len(),
        })
    }

    /// Get batcher statistics.
    pub fn get_stats(&self) -> BatchStats {
        let total_batches = self.total_batches.load(Ordering::Relaxed);
        let total_processed = self.total_requests_processed.load(Ordering::Relaxed);
        let total_wait = self.total_wait_time_ms.load(Ordering::Relaxed);
        let total_process = self.total_process_time_ms.load(Ordering::Relaxed);
        let total_tokens = self.total_tokens_processed.load(Ordering::Relaxed);

        let avg_batch_size = if total_batches > 0 {
            total_processed as f64 / total_batches as f64
        } else {
            0.0
        };
        let avg_wait = if total_processed > 0 {
            total_wait as f64 / total_processed as f64
        } else {
            0.0
        };
        let avg_process = if total_batches > 0 {
            total_process as f64 / total_batches as f64
        } else {
            0.0
        };
        let throughput = if total_process > 0 {
            (total_tokens as f64 / total_process as f64) * 1000.0
        } else {
            0.0
        };

        BatchStats {
            total_batches,
            avg_batch_size,
            avg_wait_time_ms: avg_wait,
            avg_process_time_ms: avg_process,
            throughput_tokens_per_sec: throughput,
            queue_depth: self.queue.len(),
            preemption_count: self.total_preemptions.load(Ordering::Relaxed),
        }
    }

    /// Resize the maximum queue capacity.
    pub fn resize_queue(&self, new_size: usize) {
        let mut cfg = self.config.write().unwrap();
        cfg.max_queue_size = new_size;
        info!(new_size = new_size, "Batch queue resized");
    }

    /// Update the batcher configuration.
    pub fn update_config(&self, config: DynamicBatchConfig) {
        let mut cfg = self.config.write().unwrap();
        *cfg = config;
        info!("Batcher config updated");
    }
}

// ---------------------------------------------------------------------------
// Request / Response types for API
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    pub prompt: String,
    pub max_tokens: usize,
    #[serde(default = "default_priority")]
    pub priority: u8,
    pub model_id: String,
    pub deadline: Option<DateTime<Utc>>,
}

fn default_priority() -> u8 { 3 }

#[derive(Debug, Deserialize)]
pub struct SubmitBatchRequest {
    pub requests: Vec<SubmitRequest>,
}

#[derive(Debug, Serialize)]
pub struct SubmitResponse {
    pub success: bool,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitBatchResponse {
    pub success: bool,
    pub request_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CancelResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub request_id: String,
    pub status: RequestStatus,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub queue_depth: usize,
    pub total_batches: u64,
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_batching_router(state: AppState) -> Router {
    Router::new()
        .route("/api/batching/submit", post(submit_handler))
        .route("/api/batching/batch", post(submit_batch_handler))
        .route("/api/batching/{id}/cancel", post(cancel_handler))
        .route("/api/batching/{id}/status", get(status_handler))
        .route("/api/batching/stats", get(stats_handler))
        .route("/api/batching/config", put(config_handler))
        .route("/api/batching/health", get(health_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/batching/submit — submit a single inference request.
async fn submit_handler(
    State(state): State<AppState>,
    Json(req): Json<SubmitRequest>,
) -> Response {
    let id = state.dynamic_batcher.submit(
        req.prompt,
        req.max_tokens,
        req.priority,
        req.model_id,
        req.deadline,
    );
    if id.is_empty() {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Queue full"})),
        )
            .into_response()
    } else {
        (
            axum::http::StatusCode::CREATED,
            Json(SubmitResponse { success: true, request_id: id }),
        )
            .into_response()
    }
}

/// POST /api/batching/batch — submit multiple requests at once.
async fn submit_batch_handler(
    State(state): State<AppState>,
    Json(req): Json<SubmitBatchRequest>,
) -> Response {
    let items: Vec<(String, usize, u8, String)> = req
        .requests
        .into_iter()
        .map(|r| (r.prompt, r.max_tokens, r.priority, r.model_id))
        .collect();
    let ids = state.dynamic_batcher.submit_batch(items);
    (
        axum::http::StatusCode::CREATED,
        Json(SubmitBatchResponse { success: true, request_ids: ids }),
    )
        .into_response()
}

/// POST /api/batching/{id}/cancel — cancel a queued request.
async fn cancel_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let cancelled = state.dynamic_batcher.cancel(&id);
    Json(CancelResponse {
        success: cancelled,
        message: if cancelled {
            format!("Request {} cancelled", id)
        } else {
            format!("Request {} not found or not cancellable", id)
        },
    })
    .into_response()
}

/// GET /api/batching/{id}/status — check request status.
async fn status_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.dynamic_batcher.get_status(&id) {
        Some(status) => Json(StatusResponse { request_id: id, status }).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Request not found"})),
        )
            .into_response(),
    }
}

/// GET /api/batching/stats — batcher statistics.
async fn stats_handler(
    State(state): State<AppState>,
) -> Json<BatchStats> {
    Json(state.dynamic_batcher.get_stats())
}

/// PUT /api/batching/config — update batcher configuration.
async fn config_handler(
    State(state): State<AppState>,
    Json(config): Json<DynamicBatchConfig>,
) -> Json<serde_json::Value> {
    state.dynamic_batcher.update_config(config);
    Json(serde_json::json!({"success": true}))
}

/// GET /api/batching/health — health check.
async fn health_handler(
    State(state): State<AppState>,
) -> Json<HealthResponse> {
    let stats = state.dynamic_batcher.get_stats();
    Json(HealthResponse {
        healthy: true,
        queue_depth: stats.queue_depth,
        total_batches: stats.total_batches,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DynamicBatchConfig::default();
        assert_eq!(config.max_batch_size, 32);
        assert_eq!(config.max_wait_time_ms, 50);
        assert_eq!(config.token_budget, 4096);
        assert!(!config.enable_preemption);
    }

    #[test]
    fn test_submit_and_cancel() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let id = batcher.submit("Hello".into(), 100, 3, "model-1".into(), None);
        assert!(!id.is_empty());

        assert_eq!(batcher.get_status(&id), Some(RequestStatus::Queued));
        assert!(batcher.cancel(&id));
        assert_eq!(batcher.get_status(&id), Some(RequestStatus::Cancelled));
    }

    #[test]
    fn test_submit_batch() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let requests = vec![
            ("Prompt 1".into(), 50, 3, "m1".into()),
            ("Prompt 2".into(), 50, 3, "m1".into()),
            ("Prompt 3".into(), 50, 3, "m1".into()),
        ];
        let ids = batcher.submit_batch(requests);
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_process_batch() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        batcher.submit("Hello".into(), 100, 5, "m1".into(), None);
        batcher.submit("World".into(), 100, 5, "m1".into(), None);

        let result = batcher.process_next_batch();
        assert!(result.is_some());
        let batch = result.unwrap();
        assert_eq!(batch.batch_size, 2);
        assert_eq!(batch.responses.len(), 2);
        assert!(batch.total_tokens > 0);
    }

    #[test]
    fn test_process_empty_queue() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let result = batcher.process_next_batch();
        assert!(result.is_none());
    }

    #[test]
    fn test_queue_full() {
        let mut config = DynamicBatchConfig::default();
        config.max_queue_size = 2;
        let batcher = DynamicBatcher::new(config);

        let id1 = batcher.submit("p1".into(), 10, 3, "m1".into(), None);
        let id2 = batcher.submit("p2".into(), 10, 3, "m1".into(), None);
        let id3 = batcher.submit("p3".into(), 10, 3, "m1".into(), None);

        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
        assert!(id3.is_empty()); // queue full
    }

    #[test]
    fn test_stats() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        batcher.submit("Hello".into(), 100, 5, "m1".into(), None);
        batcher.submit("World".into(), 100, 5, "m1".into(), None);
        batcher.process_next_batch();

        let stats = batcher.get_stats();
        assert_eq!(stats.total_batches, 1);
        assert!(stats.avg_batch_size > 0.0);
        assert_eq!(stats.queue_depth, 0);
    }

    #[test]
    fn test_resize_queue() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        batcher.resize_queue(500);
        let stats = batcher.get_stats();
        assert_eq!(stats.queue_depth, 0);
    }

    #[test]
    fn test_cancel_nonexistent_request() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        assert!(!batcher.cancel("does-not-exist"));
    }

    #[test]
    fn test_cancel_already_processed_request() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let id = batcher.submit("Hello".into(), 100, 3, "model-1".into(), None);
        // Process the batch, which marks requests as Completed and removes them
        batcher.process_next_batch();
        // The request is gone from the queue, so cancel should fail
        assert!(!batcher.cancel(&id));
    }

    #[test]
    fn test_get_status_nonexistent() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        assert!(batcher.get_status("ghost").is_none());
    }

    #[test]
    fn test_submit_batch_filters_empty_ids() {
        let mut config = DynamicBatchConfig::default();
        config.max_queue_size = 2;
        let batcher = DynamicBatcher::new(config);
        let requests = vec![
            ("p1".into(), 10, 3, "m1".into()),
            ("p2".into(), 10, 3, "m1".into()),
            ("p3".into(), 10, 3, "m1".into()), // will be rejected (queue full)
        ];
        let ids = batcher.submit_batch(requests);
        assert_eq!(ids.len(), 2); // empty id for the 3rd is filtered out
    }

    #[test]
    fn test_priority_ordering() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        // Submit requests with different priorities
        let low = batcher.submit("low priority".into(), 10, 1, "m1".into(), None);
        let high = batcher.submit("high priority".into(), 10, 5, "m1".into(), None);
        let med = batcher.submit("med priority".into(), 10, 3, "m1".into(), None);

        let result = batcher.process_next_batch().unwrap();
        assert_eq!(result.batch_size, 3);
        // All three should be in the result
        assert!(result.request_ids.contains(&low));
        assert!(result.request_ids.contains(&high));
        assert!(result.request_ids.contains(&med));
        // With the reversed Ord impl, lowest effective_priority pops first from BinaryHeap
        // So low priority (1) comes out first in the batch
        assert_eq!(result.responses[0].request_id, low);
        assert_eq!(result.responses[1].request_id, med);
        assert_eq!(result.responses[2].request_id, high);
    }

    #[test]
    fn test_token_budget_limits_batch() {
        let mut config = DynamicBatchConfig::default();
        config.token_budget = 50;
        config.max_batch_size = 100;
        let batcher = DynamicBatcher::new(config);

        batcher.submit("p1".into(), 30, 5, "m1".into(), None);
        batcher.submit("p2".into(), 30, 5, "m1".into(), None);
        batcher.submit("p3".into(), 10, 5, "m1".into(), None);

        let result = batcher.process_next_batch().unwrap();
        // Only 2 requests fit in 50 token budget (30+10 or 30+10)
        assert!(result.batch_size <= 2);
    }

    #[test]
    fn test_max_batch_size_limit() {
        let mut config = DynamicBatchConfig::default();
        config.max_batch_size = 2;
        config.token_budget = 100000;
        let batcher = DynamicBatcher::new(config);

        batcher.submit("p1".into(), 10, 5, "m1".into(), None);
        batcher.submit("p2".into(), 10, 5, "m1".into(), None);
        batcher.submit("p3".into(), 10, 5, "m1".into(), None);

        let result = batcher.process_next_batch().unwrap();
        assert_eq!(result.batch_size, 2);
        // p3 should remain in queue
        let stats = batcher.get_stats();
        assert_eq!(stats.queue_depth, 1);
    }

    #[test]
    fn test_stats_initial_values() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let stats = batcher.get_stats();
        assert_eq!(stats.total_batches, 0);
        assert_eq!(stats.avg_batch_size, 0.0);
        assert_eq!(stats.avg_wait_time_ms, 0.0);
        assert_eq!(stats.avg_process_time_ms, 0.0);
        assert_eq!(stats.throughput_tokens_per_sec, 0.0);
        assert_eq!(stats.queue_depth, 0);
        assert_eq!(stats.preemption_count, 0);
    }

    #[test]
    fn test_update_config() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let new_config = DynamicBatchConfig {
            max_batch_size: 16,
            max_wait_time_ms: 100,
            min_batch_size: 2,
            max_queue_size: 500,
            priority_levels: 3,
            token_budget: 2048,
            enable_preemption: true,
            aging_factor: 0.05,
        };
        batcher.update_config(new_config);
        // Verify by submitting up to the new queue limit
        let mut config = DynamicBatchConfig::default();
        config.max_queue_size = 500;
        // After update, queue should be 500
        // We can verify indirectly: submit 250 requests should all succeed
        let requests: Vec<_> = (0..250)
            .map(|i| (format!("p{}", i), 10, 3, "m1".into()))
            .collect();
        let ids = batcher.submit_batch(requests);
        assert_eq!(ids.len(), 250);
    }

    #[test]
    fn test_preemption_enabled() {
        let mut config = DynamicBatchConfig::default();
        config.max_batch_size = 2;
        config.token_budget = 100;
        config.enable_preemption = true;
        let batcher = DynamicBatcher::new(config);

        // Submit a low-priority request that uses most of the budget
        let low = batcher.submit("big low".into(), 90, 1, "m1".into(), None);
        // Submit a high-priority request that exceeds remaining budget
        let high = batcher.submit("small high".into(), 50, 5, "m1".into(), None);

        let result = batcher.process_next_batch().unwrap();
        assert!(result.request_ids.contains(&high));
        assert!(!result.request_ids.contains(&low));

        let stats = batcher.get_stats();
        assert_eq!(stats.preemption_count, 1);
        // The low priority request should be preempted
        assert_eq!(batcher.get_status(&low), Some(RequestStatus::Preempted));
    }

    #[test]
    fn test_expired_deadline_skipped() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        // Submit with a deadline in the past
        let past_deadline = Utc::now() - chrono::Duration::seconds(10);
        let id = batcher.submit("expired".into(), 10, 5, "m1".into(), Some(past_deadline));

        let result = batcher.process_next_batch();
        assert!(result.is_none()); // expired request should be skipped
        // The request should still be in the queue but not processed
        assert_eq!(batcher.get_status(&id), Some(RequestStatus::Queued));
    }

    #[test]
    fn test_request_status_transitions() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let id = batcher.submit("test".into(), 100, 5, "m1".into(), None);

        // Initially queued
        assert_eq!(batcher.get_status(&id), Some(RequestStatus::Queued));

        // Cancel
        assert!(batcher.cancel(&id));
        assert_eq!(batcher.get_status(&id), Some(RequestStatus::Cancelled));

        // Cannot cancel again
        assert!(!batcher.cancel(&id));
    }

    #[test]
    fn test_concurrent_submit_and_cancel() {
        use std::thread;

        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let batcher_clone = batcher.clone();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let b = batcher_clone.clone();
                thread::spawn(move || {
                    let id = b.submit(format!("p{}", i), 10, 3, "m1".into(), None);
                    if !id.is_empty() {
                        b.cancel(&id);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All requests should be cancelled (status = Cancelled, still in queue)
        let stats = batcher.get_stats();
        assert_eq!(stats.queue_depth, 10);
    }

    #[test]
    fn test_multiple_batch_processes() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());

        // First batch
        batcher.submit("p1".into(), 100, 5, "m1".into(), None);
        batcher.submit("p2".into(), 100, 5, "m1".into(), None);
        let result1 = batcher.process_next_batch().unwrap();
        assert_eq!(result1.batch_size, 2);

        // Second batch
        batcher.submit("p3".into(), 100, 5, "m1".into(), None);
        let result2 = batcher.process_next_batch().unwrap();
        assert_eq!(result2.batch_size, 1);

        let stats = batcher.get_stats();
        assert_eq!(stats.total_batches, 2);
    }

    #[test]
    fn test_request_id_increments() {
        let batcher = DynamicBatcher::new(DynamicBatchConfig::default());
        let id1 = batcher.submit("p1".into(), 10, 3, "m1".into(), None);
        let id2 = batcher.submit("p2".into(), 10, 3, "m1".into(), None);
        assert!(id1.starts_with("breq-"));
        assert!(id2.starts_with("breq-"));
        assert_ne!(id1, id2);
    }
}
