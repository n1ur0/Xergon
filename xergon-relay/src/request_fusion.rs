use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock};
use uuid::Uuid;
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FusionStatus {
    Waiting,
    Executing,
    Completed,
    Expired,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionConfig {
    /// Minimum Jaccard similarity (0.0-1.0) to consider two requests fusible.
    pub similarity_threshold: f64,
    /// Maximum milliseconds a candidate will wait before being auto-fused alone.
    pub max_wait_ms: u64,
    /// Maximum number of prompts that can be batched into a single fusion.
    pub max_batch_size: u32,
    /// Time-to-live in seconds before a candidate or fusion is considered expired.
    pub ttl_secs: u64,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.6,
            max_wait_ms: 100,
            max_batch_size: 5,
            ttl_secs: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// Domain models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionCandidate {
    pub id: String,
    pub prompt: String,
    pub model: String,
    pub metadata: HashMap<String, String>,
    pub status: FusionStatus,
    pub fusion_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl FusionCandidate {
    pub fn new(prompt: String, model: String, metadata: HashMap<String, String>, ttl_secs: u64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            prompt,
            model,
            metadata,
            status: FusionStatus::Waiting,
            fusion_id: None,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(ttl_secs as i64),
        }
    }

    /// Returns true if this candidate has passed its expiration time.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedRequest {
    pub id: String,
    pub model: String,
    pub prompts: Vec<String>,
    pub candidate_ids: Vec<String>,
    pub status: FusionStatus,
    pub similarity_scores: Vec<f64>,
    pub result: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl FusedRequest {
    pub fn new(model: String, _ttl_secs: u64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            model,
            prompts: Vec::new(),
            candidate_ids: Vec::new(),
            status: FusionStatus::Waiting,
            similarity_scores: Vec::new(),
            result: None,
            created_at: now,
            completed_at: None,
        }
    }

    pub fn is_full(&self, max_batch_size: u32) -> bool {
        self.candidate_ids.len() as u32 >= max_batch_size
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionResult {
    pub request_id: String,
    pub fusion_id: String,
    pub result: String,
    pub was_fused: bool,
    pub tokens_saved: u32,
    pub latency_saved_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionMetricsSnapshot {
    pub total_requests: u64,
    pub total_fusions: u64,
    pub fusion_rate: f64,
    pub avg_batch_size: f64,
    pub total_tokens_saved: u64,
    pub total_latency_saved_ms: u64,
    pub active_fusions: u64,
    pub pending_requests: u64,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RequestFusionEngine {
    candidates: DashMap<String, FusionCandidate>,
    fusions: DashMap<String, FusedRequest>,
    config: RwLock<FusionConfig>,
    total_requests: AtomicU64,
    total_fusions: AtomicU64,
    total_tokens_saved: AtomicU64,
    total_latency_saved_ms: AtomicU64,
}

impl RequestFusionEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: FusionConfig) -> Self {
        Self {
            candidates: DashMap::new(),
            fusions: DashMap::new(),
            config: RwLock::new(config),
            total_requests: AtomicU64::new(0),
            total_fusions: AtomicU64::new(0),
            total_tokens_saved: AtomicU64::new(0),
            total_latency_saved_ms: AtomicU64::new(0),
        }
    }

    // -----------------------------------------------------------------------
    // Core logic
    // -----------------------------------------------------------------------

    /// Submit a new request. If a similar pending request exists above the
    /// similarity threshold the candidate is fused into the existing group.
    /// Otherwise a new standalone candidate is created.
    pub fn submit_request(
        &self,
        prompt: String,
        model: String,
        metadata: HashMap<String, String>,
    ) -> FusionCandidate {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let cfg = self.config.read().unwrap();
        let ttl_secs = cfg.ttl_secs;
        let threshold = cfg.similarity_threshold;
        let max_batch = cfg.max_batch_size;
        drop(cfg);

        let candidate = FusionCandidate::new(prompt.clone(), model.clone(), metadata, ttl_secs);
        let candidate_id = candidate.id.clone();

        // Try to find a similar pending fusion that still has room.
        let mut fused_into = None;
        let mut best_score = 0.0_f64;

        for entry in self.fusions.iter() {
            let fr = entry.value();
            if fr.status != FusionStatus::Waiting {
                continue;
            }
            if fr.model != model {
                continue;
            }
            if fr.is_full(max_batch) {
                continue;
            }
            // Check similarity against every prompt already in the fusion.
            for existing_prompt in &fr.prompts {
                let score = self.simulate_similarity(&prompt, existing_prompt);
                if score >= threshold && score > best_score {
                    best_score = score;
                    fused_into = Some(fr.id.clone());
                }
            }
        }

        // Also check standalone waiting candidates not yet in a fusion.
        if fused_into.is_none() {
            for entry in self.candidates.iter() {
                let c = entry.value();
                if c.status != FusionStatus::Waiting {
                    continue;
                }
                if c.fusion_id.is_some() {
                    continue;
                }
                if c.model != model {
                    continue;
                }
                let score = self.simulate_similarity(&prompt, &c.prompt);
                if score >= threshold && score > best_score {
                    best_score = score;
                    fused_into = None; // Will create a new fusion grouping the two.
                    // We'll create a new fusion with both candidates below.
                    break;
                }
            }
        }

        if let Some(fusion_id) = fused_into {
            // Attach to existing fusion.
            if let Some(mut fr) = self.fusions.get_mut(&fusion_id) {
                fr.prompts.push(prompt.clone());
                fr.candidate_ids.push(candidate_id.clone());
                fr.similarity_scores.push(best_score);
            }
            let mut cand = candidate;
            cand.fusion_id = Some(fusion_id.clone());
            self.candidates.insert(candidate_id.clone(), cand.clone());
            cand
        } else {
            // Check if there's a waiting candidate we can pair with to create a
            // new fusion.
            let mut partner_id: Option<String> = None;
            let mut partner_score = 0.0_f64;
            for entry in self.candidates.iter() {
                let c = entry.value();
                if c.status != FusionStatus::Waiting {
                    continue;
                }
                if c.fusion_id.is_some() {
                    continue;
                }
                if c.model != model {
                    continue;
                }
                let score = self.simulate_similarity(&prompt, &c.prompt);
                if score >= threshold && score > partner_score {
                    partner_score = score;
                    partner_id = Some(c.id.clone());
                }
            }

            if let Some(pid) = partner_id {
                // Create a new fusion with both candidates.
                let mut fr = FusedRequest::new(model.clone(), ttl_secs);
                fr.prompts.push(prompt.clone());
                fr.candidate_ids.push(candidate_id.clone());
                fr.similarity_scores.push(partner_score);
                fr.prompts.push(
                    self.candidates
                        .get(&pid)
                        .map(|c| c.prompt.clone())
                        .unwrap_or_default(),
                );
                fr.candidate_ids.push(pid.clone());
                fr.similarity_scores.push(partner_score);
                let fusion_id = fr.id.clone();
                self.fusions.insert(fusion_id.clone(), fr);

                // Update partner candidate.
                if let Some(mut pc) = self.candidates.get_mut(&pid) {
                    pc.fusion_id = Some(fusion_id.clone());
                }

                let mut cand = candidate;
                cand.fusion_id = Some(fusion_id);
                self.candidates.insert(candidate_id.clone(), cand.clone());
                self.total_fusions.fetch_add(1, Ordering::Relaxed);
                cand
            } else {
                // No match — standalone candidate.
                self.candidates.insert(candidate_id.clone(), candidate.clone());
                candidate
            }
        }
    }

    /// Check the status of a candidate by its request id.
    pub fn check_fusion(&self, request_id: &str) -> Option<FusionCandidate> {
        // Evict expired candidates before reading.
        if let Some(c) = self.candidates.get(request_id) {
            if c.is_expired() && c.status == FusionStatus::Waiting {
                drop(c);
                self.expire_candidate(request_id);
            }
        }
        self.candidates.get(request_id).map(|c| c.clone())
    }

    /// Retrieve the fusion result for a request.
    pub fn get_result(&self, request_id: &str) -> Option<FusionResult> {
        let candidate = self.candidates.get(request_id)?;
        let fusion_id = candidate.fusion_id.as_ref()?;
        let fr = self.fusions.get(fusion_id)?;

        let was_fused = fr.candidate_ids.len() > 1;
        let result = fr.result.clone().unwrap_or_default();
        let tokens_saved = if was_fused {
            // Rough heuristic: (N-1)/N of a single prompt's token estimate saved.
            let n = fr.candidate_ids.len() as u32;
            let est_tokens_per_prompt = 50u32; // simplistic
            (n - 1) * est_tokens_per_prompt
        } else {
            0
        };
        let latency_saved_ms = if was_fused {
            let n = fr.candidate_ids.len() as u64;
            (n - 1) * 200 // simplistic 200 ms per eliminated call
        } else {
            0
        };

        Some(FusionResult {
            request_id: request_id.to_string(),
            fusion_id: fusion_id.clone(),
            result,
            was_fused,
            tokens_saved,
            latency_saved_ms,
        })
    }

    /// Cancel a pending request. Returns true if it was actually cancelled.
    pub fn cancel_request(&self, request_id: &str) -> bool {
        let mut cancelled = false;
        if let Some(mut c) = self.candidates.get_mut(request_id) {
            if c.status == FusionStatus::Waiting {
                c.status = FusionStatus::Cancelled;
                cancelled = true;
            }
        }
        // If the candidate was part of a fusion, update the fusion too.
        if cancelled {
            if let Some(c) = self.candidates.get(request_id) {
                if let Some(ref fid) = c.fusion_id {
                    if let Some(mut fr) = self.fusions.get_mut(fid) {
                        // If all candidates are cancelled or completed, mark fusion completed.
                        let all_done = fr.candidate_ids.iter().all(|cid| {
                            self.candidates
                                .get(cid)
                                .map(|cc| {
                                    cc.status == FusionStatus::Cancelled
                                        || cc.status == FusionStatus::Completed
                                })
                                .unwrap_or(true)
                        });
                        if all_done {
                            fr.status = FusionStatus::Completed;
                            fr.completed_at = Some(Utc::now());
                        }
                    }
                }
            }
        }
        cancelled
    }

    /// List all currently active (non-terminal) fusions.
    pub fn list_active_fusions(&self) -> Vec<FusedRequest> {
        self.evict_expired();
        self.fusions
            .iter()
            .filter(|e| e.value().status == FusionStatus::Waiting || e.value().status == FusionStatus::Executing)
            .map(|e| e.value().clone())
            .collect()
    }

    /// List all pending (waiting) candidates.
    pub fn list_pending_requests(&self) -> Vec<FusionCandidate> {
        self.evict_expired();
        self.candidates
            .iter()
            .filter(|e| e.value().status == FusionStatus::Waiting)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Snapshot of current engine metrics.
    pub fn get_metrics(&self) -> FusionMetricsSnapshot {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let total_fusions = self.total_fusions.load(Ordering::Relaxed);
        let tokens_saved = self.total_tokens_saved.load(Ordering::Relaxed);
        let latency_saved = self.total_latency_saved_ms.load(Ordering::Relaxed);

        let active_fusions = self
            .fusions
            .iter()
            .filter(|e| {
                let s = &e.value().status;
                *s == FusionStatus::Waiting || *s == FusionStatus::Executing
            })
            .count() as u64;

        let pending_requests = self
            .candidates
            .iter()
            .filter(|e| e.value().status == FusionStatus::Waiting)
            .count() as u64;

        // Compute average batch size from completed fusions.
        let completed_count = self
            .fusions
            .iter()
            .filter(|e| e.value().status == FusionStatus::Completed)
            .count();

        let avg_batch_size = if completed_count > 0 {
            let sum: f64 = self
                .fusions
                .iter()
                .filter(|e| e.value().status == FusionStatus::Completed)
                .map(|e| e.value().candidate_ids.len() as f64)
                .sum::<f64>();
            sum / completed_count as f64
        } else {
            0.0
        };

        let fusion_rate = if total_requests > 0 {
            total_fusions as f64 / total_requests as f64
        } else {
            0.0
        };

        FusionMetricsSnapshot {
            total_requests,
            total_fusions,
            fusion_rate,
            avg_batch_size,
            total_tokens_saved: tokens_saved,
            total_latency_saved_ms: latency_saved,
            active_fusions,
            pending_requests,
        }
    }

    /// Compute Jaccard similarity between two strings based on word sets.
    /// J(A,B) = |A ∩ B| / |A ∪ B|
    pub fn simulate_similarity(&self, a: &str, b: &str) -> f64 {
        let set_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
        let set_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();

        if union == 0 {
            return 1.0; // Both empty strings are maximally similar.
        }
        intersection as f64 / union as f64
    }

    /// Replace the engine configuration atomically. Returns the old config.
    pub fn update_config(&self, config: FusionConfig) -> FusionConfig {
        let mut cfg = self.config.write().unwrap();
        let old = cfg.clone();
        *cfg = config;
        old
    }

    /// Return a copy of the current configuration.
    pub fn get_config(&self) -> FusionConfig {
        self.config.read().unwrap().clone()
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn expire_candidate(&self, request_id: &str) {
        if let Some(mut c) = self.candidates.get_mut(request_id) {
            c.status = FusionStatus::Expired;
        }
    }

    /// Sweep and expire stale entries.
    fn evict_expired(&self) {
        let expired_candidates: Vec<String> = self
            .candidates
            .iter()
            .filter(|e| e.value().is_expired() && e.value().status == FusionStatus::Waiting)
            .map(|e| e.key().clone())
            .collect();

        for id in expired_candidates {
            self.expire_candidate(&id);
        }

        let expired_fusions: Vec<String> = self
            .fusions
            .iter()
            .filter(|e| {
                let fr = e.value();
                (fr.status == FusionStatus::Waiting || fr.status == FusionStatus::Executing)
                    && fr.result.is_none()
                    && (Utc::now() - fr.created_at).num_seconds() > self.config.read().unwrap().ttl_secs as i64
            })
            .map(|e| e.key().clone())
            .collect();

        for fid in expired_fusions {
            if let Some(mut fr) = self.fusions.get_mut(&fid) {
                fr.status = FusionStatus::Expired;
                fr.completed_at = Some(Utc::now());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SubmitRequestPayload {
    pub prompt: String,
    pub model: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct SubmitResponse {
    pub request_id: String,
    pub fusion_id: Option<String>,
    pub status: FusionStatus,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub request_id: String,
    pub status: FusionStatus,
    pub fusion_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// POST /api/fusion/requests — submit a new request for potential fusion.
async fn submit_request_handler(
    State(state): State<AppState>,
    Json(payload): Json<SubmitRequestPayload>,
) -> (StatusCode, Json<SubmitResponse>) {
    let engine = &state.request_fusion;
    let candidate = engine.submit_request(payload.prompt, payload.model, payload.metadata);
    (
        StatusCode::OK,
        Json(SubmitResponse {
            request_id: candidate.id,
            fusion_id: candidate.fusion_id,
            status: candidate.status,
        }),
    )
}

/// GET /api/fusion/requests/:id — check fusion status of a request.
async fn check_fusion_handler(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = &state.request_fusion;
    match engine.check_fusion(&request_id) {
        Some(c) => (
            StatusCode::OK,
            Json(serde_json::to_value(StatusResponse {
                request_id: c.id,
                status: c.status,
                fusion_id: c.fusion_id,
            }).unwrap()),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::to_value(ErrorResponse {
                error: "Request not found".into(),
            }).unwrap()),
        ),
    }
}

/// GET /api/fusion/requests/:id/result — get the fusion result.
async fn get_result_handler(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = &state.request_fusion;
    match engine.get_result(&request_id) {
        Some(r) => (
            StatusCode::OK,
            Json(serde_json::to_value(r).unwrap()),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::to_value(ErrorResponse {
                error: "Result not available".into(),
            }).unwrap()),
        ),
    }
}

/// DELETE /api/fusion/requests/:id — cancel a pending request.
async fn cancel_request_handler(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let engine = &state.request_fusion;
    if engine.cancel_request(&request_id) {
        (
            StatusCode::OK,
            Json(serde_json::json!({ "cancelled": true })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::to_value(ErrorResponse {
                error: "Request not found or not cancellable".into(),
            }).unwrap()),
        )
    }
}

/// GET /api/fusion/active — list active fusions.
async fn list_active_fusions_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<Vec<FusedRequest>>) {
    let engine = &state.request_fusion;
    let fusions = engine.list_active_fusions();
    (StatusCode::OK, Json(fusions))
}

/// GET /api/fusion/pending — list pending candidates.
async fn list_pending_requests_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<Vec<FusionCandidate>>) {
    let engine = &state.request_fusion;
    let candidates = engine.list_pending_requests();
    (StatusCode::OK, Json(candidates))
}

/// GET /api/fusion/metrics — engine metrics.
async fn get_metrics_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<FusionMetricsSnapshot>) {
    let engine = &state.request_fusion;
    let metrics = engine.get_metrics();
    (StatusCode::OK, Json(metrics))
}

/// PUT /api/fusion/config — update engine configuration.
async fn update_config_handler(
    State(state): State<AppState>,
    Json(config): Json<FusionConfig>,
) -> (StatusCode, Json<FusionConfig>) {
    let engine = &state.request_fusion;
    let old = engine.update_config(config);
    (StatusCode::OK, Json(old))
}

/// GET /api/fusion/config — retrieve current engine configuration.
async fn get_config_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<FusionConfig>) {
    let engine = &state.request_fusion;
    let config = engine.get_config();
    (StatusCode::OK, Json(config))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the fusion sub-router attached to the shared application state.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/fusion/requests", post(submit_request_handler))
        .route("/api/fusion/requests/{id}", get(check_fusion_handler))
        .route("/api/fusion/requests/{id}/result", get(get_result_handler))
        .route("/api/fusion/requests/{id}", delete(cancel_request_handler))
        .route("/api/fusion/active", get(list_active_fusions_handler))
        .route("/api/fusion/pending", get(list_pending_requests_handler))
        .route("/api/fusion/metrics", get(get_metrics_handler))
        .route("/api/fusion/config", put(update_config_handler))
        .route("/api/fusion/config", get(get_config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_engine() -> RequestFusionEngine {
        RequestFusionEngine::new(FusionConfig::default())
    }

    // -- similarity tests ---------------------------------------------------

    #[test]
    fn test_similarity_identical_strings() {
        let engine = default_engine();
        let score = engine.simulate_similarity("hello world foo", "hello world foo");
        assert!((score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_similarity_disjoint_strings() {
        let engine = default_engine();
        let score = engine.simulate_similarity("alpha beta gamma", "delta epsilon zeta");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_similarity_partial_overlap() {
        let engine = default_engine();
        // "hello world" ∩ "hello there" = {hello}, union = {hello, world, there}
        let score = engine.simulate_similarity("hello world", "hello there");
        assert!((score - (1.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_similarity_empty_strings() {
        let engine = default_engine();
        let score = engine.simulate_similarity("", "");
        assert!((score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_similarity_one_empty() {
        let engine = default_engine();
        let score = engine.simulate_similarity("hello", "");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_similarity_case_sensitive() {
        let engine = default_engine();
        let score = engine.simulate_similarity("Hello", "hello");
        assert_eq!(score, 0.0);
    }

    // -- config tests -------------------------------------------------------

    #[test]
    fn test_default_config() {
        let cfg = FusionConfig::default();
        assert!((cfg.similarity_threshold - 0.6).abs() < 1e-9);
        assert_eq!(cfg.max_wait_ms, 100);
        assert_eq!(cfg.max_batch_size, 5);
        assert_eq!(cfg.ttl_secs, 30);
    }

    #[test]
    fn test_update_config_returns_old() {
        let engine = default_engine();
        let new_cfg = FusionConfig {
            similarity_threshold: 0.9,
            max_wait_ms: 500,
            max_batch_size: 10,
            ttl_secs: 60,
        };
        let old = engine.update_config(new_cfg.clone());
        assert_eq!(old.similarity_threshold, 0.6);
        assert_eq!(engine.get_config().similarity_threshold, 0.9);
    }

    #[test]
    fn test_get_config_reflects_update() {
        let engine = default_engine();
        assert_eq!(engine.get_config().max_batch_size, 5);
        engine.update_config(FusionConfig {
            max_batch_size: 20,
            ..Default::default()
        });
        assert_eq!(engine.get_config().max_batch_size, 20);
    }

    // -- submission and auto-fusion -----------------------------------------

    #[test]
    fn test_submit_single_request_no_fusion() {
        let engine = default_engine();
        let c = engine.submit_request(
            "explain quantum computing".into(),
            "gpt-4".into(),
            HashMap::new(),
        );
        assert_eq!(c.status, FusionStatus::Waiting);
        assert!(c.fusion_id.is_none());
    }

    #[test]
    fn test_two_similar_requests_auto_fuse() {
        let engine = default_engine();
        // These share enough words to exceed 0.6 threshold.
        let prompt1 = "explain quantum computing basics and principles";
        let prompt2 = "explain quantum computing basics and fundamentals";

        let c1 = engine.submit_request(prompt1.into(), "gpt-4".into(), HashMap::new());
        assert!(c1.fusion_id.is_none(), "First request should not be fused");

        let c2 = engine.submit_request(prompt2.into(), "gpt-4".into(), HashMap::new());
        assert!(
            c2.fusion_id.is_some(),
            "Second similar request should be fused"
        );
    }

    #[test]
    fn test_different_models_do_not_fuse() {
        let engine = default_engine();
        let prompt = "explain quantum computing basics and principles";

        let _c1 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());
        let c2 = engine.submit_request(prompt.into(), "claude-3".into(), HashMap::new());

        assert!(
            c2.fusion_id.is_none(),
            "Requests to different models should not fuse"
        );
    }

    #[test]
    fn test_no_fusion_below_threshold() {
        let engine = RequestFusionEngine::new(FusionConfig {
            similarity_threshold: 0.9,
            ..Default::default()
        });
        let prompt1 = "the quick brown fox jumps over the lazy dog";
        let prompt2 = "a completely different sentence about astronomy and stars";

        let _c1 = engine.submit_request(prompt1.into(), "gpt-4".into(), HashMap::new());
        let c2 = engine.submit_request(prompt2.into(), "gpt-4".into(), HashMap::new());

        assert!(
            c2.fusion_id.is_none(),
            "Below-threshold requests should not fuse"
        );
    }

    #[test]
    fn test_submit_increments_total_requests() {
        let engine = default_engine();
        engine.submit_request("prompt one".into(), "m".into(), HashMap::new());
        engine.submit_request("prompt two".into(), "m".into(), HashMap::new());
        let m = engine.get_metrics();
        assert_eq!(m.total_requests, 2);
    }

    // -- cancellation -------------------------------------------------------

    #[test]
    fn test_cancel_pending_request() {
        let engine = default_engine();
        let c = engine.submit_request("some prompt".into(), "m".into(), HashMap::new());
        let cancelled = engine.cancel_request(&c.id);
        assert!(cancelled);
        let fetched = engine.check_fusion(&c.id);
        assert_eq!(fetched.unwrap().status, FusionStatus::Cancelled);
    }

    #[test]
    fn test_cancel_nonexistent_request() {
        let engine = default_engine();
        let cancelled = engine.cancel_request("nonexistent-id");
        assert!(!cancelled);
    }

    #[test]
    fn test_cancel_already_completed_request() {
        let engine = default_engine();
        let c = engine.submit_request("prompt".into(), "m".into(), HashMap::new());
        // Manually mark as completed.
        if let Some(mut entry) = engine.candidates.get_mut(&c.id) {
            entry.status = FusionStatus::Completed;
        }
        let cancelled = engine.cancel_request(&c.id);
        assert!(!cancelled);
    }

    // -- result retrieval ---------------------------------------------------

    #[test]
    fn test_get_result_no_fusion_returns_none() {
        let engine = default_engine();
        let c = engine.submit_request("prompt".into(), "m".into(), HashMap::new());
        assert!(engine.get_result(&c.id).is_none());
    }

    #[test]
    fn test_get_result_nonexistent() {
        let engine = default_engine();
        assert!(engine.get_result("nope").is_none());
    }

    #[test]
    fn test_get_result_after_fusion() {
        let engine = default_engine();
        let prompt = "explain quantum computing basics and principles of quantum mechanics";

        let c1 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());
        let c2 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());

        // At least one should have a fusion_id now.
        let fusion_candidate = if c1.fusion_id.is_some() { &c1 } else { &c2 };
        let result = engine.get_result(&fusion_candidate.id);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.was_fused);
    }

    // -- metrics ------------------------------------------------------------

    #[test]
    fn test_initial_metrics() {
        let engine = default_engine();
        let m = engine.get_metrics();
        assert_eq!(m.total_requests, 0);
        assert_eq!(m.total_fusions, 0);
        assert!((m.fusion_rate - 0.0).abs() < 1e-9);
        assert_eq!(m.active_fusions, 0);
        assert_eq!(m.pending_requests, 0);
    }

    #[test]
    fn test_metrics_after_submissions() {
        let engine = default_engine();
        let prompt = "explain quantum computing basics and principles of quantum mechanics";
        engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());
        engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());

        let m = engine.get_metrics();
        assert_eq!(m.total_requests, 2);
        assert!(m.total_fusions >= 1);
        assert!(m.fusion_rate > 0.0);
    }

    #[test]
    fn test_pending_requests_count() {
        let engine = default_engine();
        engine.submit_request("first prompt here".into(), "m".into(), HashMap::new());
        engine.submit_request("second different prompt".into(), "m".into(), HashMap::new());
        let m = engine.get_metrics();
        assert_eq!(m.pending_requests, 2);
    }

    // -- active fusions & pending list --------------------------------------

    #[test]
    fn test_list_active_fusions_empty() {
        let engine = default_engine();
        let active = engine.list_active_fusions();
        assert!(active.is_empty());
    }

    #[test]
    fn test_list_pending_requests_returns_submitted() {
        let engine = default_engine();
        engine.submit_request("prompt alpha".into(), "m".into(), HashMap::new());
        engine.submit_request("prompt beta".into(), "m".into(), HashMap::new());
        let pending = engine.list_pending_requests();
        assert_eq!(pending.len(), 2);
    }

    // -- edge cases ---------------------------------------------------------

    #[test]
    fn test_max_batch_size_limits_fusion() {
        let engine = RequestFusionEngine::new(FusionConfig {
            max_batch_size: 2,
            similarity_threshold: 0.5,
            ..Default::default()
        });
        let prompt = "explain quantum computing basics and principles of quantum mechanics theory";

        let c1 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());
        let c2 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());
        let c3 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());

        // c1 + c2 should be fused. c3 might be fused or standalone depending on
        // the fusion reaching max_batch_size.
        let fused_count = [&c1, &c2, &c3]
            .iter()
            .filter(|c| c.fusion_id.is_some())
            .count();
        assert!(fused_count >= 2, "At least two candidates should be fused");
    }

    #[test]
    fn test_check_fusion_nonexistent() {
        let engine = default_engine();
        assert!(engine.check_fusion("does-not-exist").is_none());
    }

    #[test]
    fn test_candidate_fields_populated() {
        let engine = default_engine();
        let mut meta = HashMap::new();
        meta.insert("user_id".to_string(), "42".to_string());
        let c = engine.submit_request("hello".into(), "llama-3".into(), meta);
        assert!(!c.id.is_empty());
        assert_eq!(c.prompt, "hello");
        assert_eq!(c.model, "llama-3");
        assert_eq!(c.metadata.get("user_id").unwrap(), "42");
        assert_eq!(c.status, FusionStatus::Waiting);
    }

    #[test]
    fn test_metadata_preserved_after_fusion() {
        let engine = default_engine();
        let prompt = "explain quantum computing basics and principles of quantum mechanics";

        let mut meta = HashMap::new();
        meta.insert("source".to_string(), "test".to_string());

        let c1 = engine.submit_request(prompt.into(), "gpt-4".into(), meta);
        let fetched = engine.check_fusion(&c1.id).unwrap();
        assert_eq!(fetched.metadata.get("source").unwrap(), "test");
    }

    #[test]
    fn test_multiple_fusions_different_models() {
        let engine = default_engine();
        let prompt = "explain quantum computing basics and principles of quantum mechanics";

        let _c1 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());
        let _c2 = engine.submit_request(prompt.into(), "gpt-4".into(), HashMap::new());
        let _c3 = engine.submit_request(prompt.into(), "claude-3".into(), HashMap::new());
        let _c4 = engine.submit_request(prompt.into(), "claude-3".into(), HashMap::new());

        // We should have at least 2 fusions (one per model).
        let metrics = engine.get_metrics();
        assert!(metrics.total_fusions >= 2);
    }

    #[test]
    fn test_fusion_candidate_expires_based_on_ttl() {
        let engine = RequestFusionEngine::new(FusionConfig {
            ttl_secs: 0, // Immediate expiration
            ..Default::default()
        });
        let c = engine.submit_request("prompt".into(), "m".into(), HashMap::new());
        // The candidate was just created, but TTL is 0 so it should be expired
        // on the next check.
        // Give a tiny bit of time for the clock to advance.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let fetched = engine.check_fusion(&c.id);
        assert_eq!(fetched.unwrap().status, FusionStatus::Expired);
    }

    #[test]
    fn test_similarity_with_repeated_words() {
        let engine = default_engine();
        // Jaccard uses sets, so repeated words don't change the score.
        let score = engine.simulate_similarity("hello hello hello", "hello");
        assert!((score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_similarity_single_word_match() {
        let engine = default_engine();
        let score = engine.simulate_similarity("alpha beta", "beta gamma");
        // intersection = {beta}, union = {alpha, beta, gamma} => 1/3
        assert!((score - (1.0 / 3.0)).abs() < 1e-9);
    }
}
