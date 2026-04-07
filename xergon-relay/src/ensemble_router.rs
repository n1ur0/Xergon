//! Multi-model ensemble routing module for the Xergon Network relay.
//!
//! Handles request fan-out to multiple model providers, response aggregation,
//! confidence scoring, and fallback merge strategies.  An *ensemble group*
//! defines a set of model IDs and a strategy for combining their outputs.
//! Requests are fanned out concurrently and aggregated according to the
//! selected strategy.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{get, post, put},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Strategy for aggregating responses from multiple models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    /// Pick the most common response text.
    MajorityVote,
    /// Weighted average of confidence scores; response with highest wins.
    WeightedAverage,
    /// Return the first response whose confidence exceeds the threshold.
    FirstConfident,
    /// Select the response with the highest confidence score.
    ConfidenceScored,
}

impl Default for AggregationStrategy {
    fn default() -> Self {
        Self::ConfidenceScored
    }
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Global configuration for the ensemble router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleConfig {
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum number of models a single fan-out may target.
    pub max_fanout: usize,
    /// Default aggregation strategy.
    pub aggregation_strategy: AggregationStrategy,
    /// Whether fallback to backup models is enabled.
    pub fallback_enabled: bool,
    /// Confidence threshold for `FirstConfident` strategy (0.0 - 1.0).
    pub confidence_threshold: f64,
    /// Per-model weights (model_id -> weight).  Used by `WeightedAverage`.
    pub weights: HashMap<String, f64>,
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            max_fanout: 8,
            aggregation_strategy: AggregationStrategy::ConfidenceScored,
            fallback_enabled: true,
            confidence_threshold: 0.7,
            weights: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A named group of models that are queried together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleGroup {
    pub id: String,
    pub name: String,
    /// Ordered list of model IDs in this group.
    pub model_ids: Vec<String>,
    /// Aggregation strategy override (uses global config default if `None`).
    pub strategy: Option<AggregationStrategy>,
    /// Per-model weights for this group.
    pub weights: HashMap<String, f64>,
    /// Whether this group is active.
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// Inbound request that gets fanned out to multiple models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanOutRequest {
    /// Optional caller-supplied request ID.  Auto-generated if absent.
    pub request_id: Option<String>,
    /// The prompt / user message.
    pub prompt: String,
    /// Target model IDs (must be a subset of the group's models).
    pub model_ids: Vec<String>,
    /// Inference parameters.
    pub parameters: FanOutParameters,
    /// Per-request timeout override (ms).
    pub timeout_ms: Option<u64>,
    /// Aggregation strategy override.
    pub strategy: Option<AggregationStrategy>,
}

/// Inference parameters forwarded to each model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanOutParameters {
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stop: Option<Vec<String>>,
}

fn default_temperature() -> f64 {
    0.7
}

fn default_max_tokens() -> u32 {
    1024
}

impl Default for FanOutParameters {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            top_p: None,
            stop: None,
        }
    }
}

/// Response from a single model within the ensemble.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    pub model_id: String,
    /// Generated text.
    pub response: String,
    /// Confidence score in [0.0, 1.0].
    pub confidence_score: f64,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Token count of the response.
    pub token_count: u32,
    /// Error message if the model failed.
    pub error: Option<String>,
}

/// Final aggregated result returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResponse {
    pub request_id: String,
    pub final_text: String,
    pub confidence: f64,
    pub individual_responses: Vec<ModelResponse>,
    pub aggregation_method: AggregationStrategy,
    pub total_latency_ms: u64,
    pub total_tokens: u32,
    pub fallback_used: bool,
    pub created_at: DateTime<Utc>,
}

/// Lightweight statistics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleStats {
    pub total_requests: u64,
    pub total_fallbacks: u64,
    pub active_groups: usize,
    pub history_size: usize,
    pub avg_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// Create / Update DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub model_ids: Vec<String>,
    pub strategy: Option<AggregationStrategy>,
    pub weights: Option<HashMap<String, f64>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub model_ids: Option<Vec<String>>,
    pub strategy: Option<AggregationStrategy>,
    pub weights: Option<HashMap<String, f64>>,
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// EnsembleRouter core
// ---------------------------------------------------------------------------

/// Central ensemble routing engine.
///
/// Owns the group registry, response history ring-buffer, and atomic
/// counters for statistics.
pub struct EnsembleRouter {
    /// Ensemble groups keyed by ID.
    groups: DashMap<String, EnsembleGroup>,
    /// Recent aggregated responses (bounded).
    history: DashMap<String, AggregatedResponse>,
    /// Maximum history entries kept.
    max_history: usize,
    /// Monotonic request counter.
    total_requests: AtomicU64,
    /// Monotonic fallback counter.
    total_fallbacks: AtomicU64,
    /// Cumulative latency (ms) for computing averages.
    cumulative_latency_ms: AtomicU64,
    /// Runtime-adjustable configuration.
    config: RwLock<EnsembleConfig>,
}

impl EnsembleRouter {
    /// Create a new router with default configuration.
    pub fn new() -> Self {
        Self {
            groups: DashMap::new(),
            history: DashMap::new(),
            max_history: 1000,
            total_requests: AtomicU64::new(0),
            total_fallbacks: AtomicU64::new(0),
            cumulative_latency_ms: AtomicU64::new(0),
            config: RwLock::new(EnsembleConfig::default()),
        }
    }

    /// Create a new router with the given configuration.
    pub fn with_config(config: EnsembleConfig) -> Self {
        Self {
            config: RwLock::new(config),
            ..Self::new()
        }
    }

    // ----- Group CRUD -----

    /// Create a new ensemble group.  Returns the created group.
    pub fn create_group(&self, req: CreateGroupRequest) -> Result<EnsembleGroup, String> {
        if req.model_ids.is_empty() {
            return Err("model_ids must not be empty".into());
        }
        if req.model_ids.len() > 64 {
            return Err("model_ids exceeds maximum of 64".into());
        }

        let cfg = self.config_blocking();
        if req.model_ids.len() > cfg.max_fanout {
            return Err(format!(
                "model_ids ({}) exceeds max_fanout ({})",
                req.model_ids.len(),
                cfg.max_fanout
            ));
        }

        let group = EnsembleGroup {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            model_ids: req.model_ids,
            strategy: req.strategy,
            weights: req.weights.unwrap_or_default(),
            enabled: req.enabled.unwrap_or(true),
            created_at: Utc::now(),
        };

        let id = group.id.clone();
        self.groups.insert(id.clone(), group.clone());
        Ok(group)
    }

    /// List all ensemble groups.
    pub fn list_groups(&self) -> Vec<EnsembleGroup> {
        self.groups.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a single ensemble group by ID.
    pub fn get_group(&self, id: &str) -> Option<EnsembleGroup> {
        self.groups.get(id).map(|r| r.value().clone())
    }

    /// Update an existing ensemble group (partial update).
    pub fn update_group(&self, id: &str, req: UpdateGroupRequest) -> Result<EnsembleGroup, String> {
        let mut entry = self
            .groups
            .get_mut(id)
            .ok_or_else(|| format!("group '{}' not found", id))?;

        if let Some(name) = req.name {
            entry.name = name;
        }
        if let Some(model_ids) = req.model_ids {
            if model_ids.is_empty() {
                return Err("model_ids must not be empty".into());
            }
            entry.model_ids = model_ids;
        }
        if let Some(strategy) = req.strategy {
            entry.strategy = Some(strategy);
        }
        if let Some(weights) = req.weights {
            entry.weights = weights;
        }
        if let Some(enabled) = req.enabled {
            entry.enabled = enabled;
        }

        Ok(entry.value().clone())
    }

    /// Delete an ensemble group.  Returns `true` if it existed.
    pub fn delete_group(&self, id: &str) -> bool {
        self.groups.remove(id).is_some()
    }

    // ----- Fan-out & Aggregation -----

    /// Fan out a request to the given models, collect responses, and
    /// aggregate them using the specified (or default) strategy.
    ///
    /// In production this would dispatch concurrent HTTP requests to each
    /// model provider.  For the relay layer the fan-out is simulated
    /// (each model returns a deterministic response based on its ID and
    /// the prompt).
    pub async fn fan_out_request(&self, req: FanOutRequest) -> Result<AggregatedResponse, String> {
        let cfg = self.config.read().await;
        let timeout = req.timeout_ms.unwrap_or(cfg.timeout_ms);
        let strategy = req
            .strategy
            .as_ref()
            .unwrap_or(&cfg.aggregation_strategy)
            .clone();
        let threshold = cfg.confidence_threshold;
        let model_weights: HashMap<String, f64> = cfg.weights.clone();
        drop(cfg);

        if req.model_ids.is_empty() {
            return Err("model_ids must not be empty".into());
        }

        let request_id = req
            .request_id
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // Fan out concurrently (simulated).
        let mut handles = Vec::new();
        for model_id in &req.model_ids {
            let mid = model_id.clone();
            let prompt = req.prompt.clone();
            let params = req.parameters.clone();
            handles.push(tokio::spawn(async move {
                let start = std::time::Instant::now();
                // Simulate model inference with a small fake delay.
                let simulated_latency = simulate_model_latency(&mid);
                tokio::time::sleep(std::time::Duration::from_millis(simulated_latency)).await;
                let elapsed = start.elapsed().as_millis() as u64;

                let (response, confidence, tokens, error) =
                    simulate_model_response(&mid, &prompt, &params);

                ModelResponse {
                    model_id: mid,
                    response,
                    confidence_score: confidence,
                    latency_ms: elapsed,
                    token_count: tokens,
                    error,
                }
            }));
        }

        let mut responses: Vec<ModelResponse> = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(r) => responses.push(r),
                Err(e) => responses.push(ModelResponse {
                    model_id: "unknown".into(),
                    response: String::new(),
                    confidence_score: 0.0,
                    latency_ms: 0,
                    token_count: 0,
                    error: Some(format!("join error: {}", e)),
                }),
            }
        }

        // Aggregate.
        let aggregated = self
            .aggregate_responses(
                request_id.clone(),
                responses,
                strategy.clone(),
                threshold,
                &model_weights,
            )
            .await;

        // Record stats.
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.cumulative_latency_ms
            .fetch_add(aggregated.total_latency_ms, Ordering::Relaxed);
        if aggregated.fallback_used {
            self.total_fallbacks.fetch_add(1, Ordering::Relaxed);
        }

        // Store in history (bounded).
        let rid = aggregated.request_id.clone();
        self.history.insert(rid.clone(), aggregated.clone());
        if self.history.len() > self.max_history {
            // Evict oldest entries (DashMap iteration order is insertion order for recent entries).
            if let Some(oldest) = self.history.iter().next() {
                self.history.remove(oldest.key());
            }
        }

        Ok(aggregated)
    }

    /// Aggregate a set of model responses according to the chosen strategy.
    pub async fn aggregate_responses(
        &self,
        request_id: String,
        responses: Vec<ModelResponse>,
        strategy: AggregationStrategy,
        threshold: f64,
        weights: &HashMap<String, f64>,
    ) -> AggregatedResponse {
        let total_latency_ms: u64 = responses.iter().map(|r| r.latency_ms).sum();
        let total_tokens: u32 = responses.iter().map(|r| r.token_count).sum();

        // Separate successful and failed responses.
        let successful: Vec<&ModelResponse> = responses.iter().filter(|r| r.error.is_none()).collect();
        let failed: Vec<&ModelResponse> = responses.iter().filter(|r| r.error.is_some()).collect();

        let (final_text, confidence, fallback_used) = if successful.is_empty() {
            // All failed -- try fallback merge.
            let fallback = self.fallback_merge(&responses).await;
            (fallback.text, fallback.confidence, true)
        } else {
            match strategy {
                AggregationStrategy::MajorityVote => {
                    majority_vote(&successful, &responses)
                }
                AggregationStrategy::WeightedAverage => {
                    weighted_average(&successful, weights, &responses)
                }
                AggregationStrategy::FirstConfident => {
                    first_confident(&successful, threshold, &responses)
                }
                AggregationStrategy::ConfidenceScored => {
                    confidence_scored(&successful, &responses)
                }
            }
        };

        AggregatedResponse {
            request_id,
            final_text,
            confidence,
            individual_responses: responses,
            aggregation_method: strategy,
            total_latency_ms,
            total_tokens,
            fallback_used,
            created_at: Utc::now(),
        }
    }

    /// Fallback merge: if all primary models failed, try to return whatever
    /// partial / error information is available.  If absolutely nothing is
    /// usable, returns a synthetic fallback message.
    pub async fn fallback_merge(&self, responses: &[ModelResponse]) -> FallbackResult {
        // Try to find any response that has *some* text, even with an error.
        for r in responses {
            if !r.response.is_empty() {
                return FallbackResult {
                    text: r.response.clone(),
                    confidence: r.confidence_score * 0.5, // Penalise fallback confidence.
                };
            }
        }
        // Everything truly failed.
        FallbackResult {
            text: "All models in the ensemble failed to produce a response.".into(),
            confidence: 0.0,
        }
    }

    // ----- History & Stats -----

    /// Return the most recent aggregated responses.
    pub fn get_history(&self, limit: usize) -> Vec<AggregatedResponse> {
        self.history
            .iter()
            .take(limit)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Return a snapshot of ensemble statistics.
    pub fn get_stats(&self) -> EnsembleStats {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let total_fallbacks = self.total_fallbacks.load(Ordering::Relaxed);
        let cumulative = self.cumulative_latency_ms.load(Ordering::Relaxed);
        let avg_latency = if total_requests > 0 {
            cumulative as f64 / total_requests as f64
        } else {
            0.0
        };

        EnsembleStats {
            total_requests,
            total_fallbacks,
            active_groups: self.groups.len(),
            history_size: self.history.len(),
            avg_latency_ms: avg_latency,
        }
    }

    /// Read the current configuration (blocking for non-async contexts).
    fn config_blocking(&self) -> EnsembleConfig {
        self.config.blocking_read().clone()
    }
}

/// Helper for fallback merge.
struct FallbackResult {
    text: String,
    confidence: f64,
}

impl Default for EnsembleRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Aggregation strategy implementations
// ---------------------------------------------------------------------------

/// **Majority vote**: count occurrences of each response text and pick the
/// most common one.  Ties broken by highest average confidence.
fn majority_vote(
    successful: &[&ModelResponse],
    _all_responses: &[ModelResponse],
) -> (String, f64, bool) {
    let mut freq: HashMap<&str, (usize, f64)> = HashMap::new();
    for r in successful {
        let entry = freq.entry(&r.response).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += r.confidence_score;
    }

    if let Some((&text, (count, conf_sum))) = freq.iter().max_by(|a, b| {
        match a.1 .0.cmp(&b.1 .0) {
            std::cmp::Ordering::Equal => a.1 .1.partial_cmp(&b.1 .1).unwrap_or(std::cmp::Ordering::Equal),
            other => other,
        }
    }) {
        let avg_conf = conf_sum / *count as f64;
        return (text.to_string(), avg_conf, false);
    }

    // Degenerate: no successful responses after all.
    (
        "No consensus could be reached.".into(),
        0.0,
        false,
    )
}

/// **Weighted average**: multiply each model's confidence by its weight,
/// pick the response with the highest weighted score.
fn weighted_average(
    successful: &[&ModelResponse],
    weights: &HashMap<String, f64>,
    all_responses: &[ModelResponse],
) -> (String, f64, bool) {
    let mut best: Option<(&ModelResponse, f64)> = None;

    for r in successful {
        let weight = weights.get(&r.model_id).copied().unwrap_or(1.0);
        let weighted = r.confidence_score * weight;
        if best
            .as_ref()
            .map_or(true, |(_, prev)| weighted > *prev)
        {
            best = Some((r, weighted));
        }
    }

    if let Some((r, weighted)) = best {
        return (r.response.clone(), weighted, false);
    }

    (
        "No weighted response available.".into(),
        0.0,
        false,
    )
}

/// **First confident**: return the first response whose confidence >= threshold.
fn first_confident(
    successful: &[&ModelResponse],
    threshold: f64,
    all_responses: &[ModelResponse],
) -> (String, f64, bool) {
    for r in successful {
        if r.confidence_score >= threshold {
            return (r.response.clone(), r.confidence_score, false);
        }
    }

    // No response met the threshold -- fall back to best available.
    if let Some(best) = successful
        .iter()
        .max_by(|a, b| a.confidence_score.partial_cmp(&b.confidence_score).unwrap_or(std::cmp::Ordering::Equal))
    {
        return (best.response.clone(), best.confidence_score, true);
    }

    (
        "No confident response available.".into(),
        0.0,
        true,
    )
}

/// **Confidence scored**: simply pick the response with the highest confidence.
fn confidence_scored(
    successful: &[&ModelResponse],
    _all_responses: &[ModelResponse],
) -> (String, f64, bool) {
    if let Some(best) = successful
        .iter()
        .max_by(|a, b| a.confidence_score.partial_cmp(&b.confidence_score).unwrap_or(std::cmp::Ordering::Equal))
    {
        return (best.response.clone(), best.confidence_score, false);
    }

    (
        "No scored response available.".into(),
        0.0,
        false,
    )
}

// ---------------------------------------------------------------------------
// Simulation helpers (stand-in for real provider calls)
// ---------------------------------------------------------------------------

/// Deterministic "latency" based on model ID length.
fn simulate_model_latency(model_id: &str) -> u64 {
    // 1-10 ms simulated latency
    (model_id.len() % 10 + 1) as u64
}

/// Deterministic response based on model ID and prompt.
fn simulate_model_response(
    model_id: &str,
    prompt: &str,
    params: &FanOutParameters,
) -> (String, f64, u32, Option<String>) {
    // Simulate that different models produce similar-but-not-identical text.
    let hash = simple_hash(&(model_id, prompt));
    let confidence = 0.5 + (hash % 500) as f64 / 1000.0; // 0.5 .. 1.0
    let tokens = (prompt.len() / 4 + hash % 50) as u32;

    let response = if hash % 5 == 0 {
        // ~20 % chance of simulated error
        return (
            String::new(),
            0.0,
            0,
            Some(format!("simulated timeout for model {}", model_id)),
        );
    } else {
        format!(
            "[{}] Response to \"{}\" (temp={:.2}, max_tokens={})",
            model_id,
            &prompt[..prompt.len().min(40)],
            params.temperature,
            params.max_tokens,
        )
    };

    (response, confidence, tokens, None)
}

/// Simple deterministic hash for simulation purposes.
fn simple_hash(data: &(&str, &str)) -> usize {
    let combined = format!("{}:{}", data.0, data.1);
    combined.bytes().fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize))
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// POST /api/ensemble/groups
async fn create_group_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateGroupRequest>,
) -> (StatusCode, Json<EnsembleGroup>) {
    let router = state.ensemble_router.clone();
    match router.create_group(req) {
        Ok(group) => (StatusCode::CREATED, Json(group)),
        Err(ref msg) => (StatusCode::BAD_REQUEST, Json(error_group(msg))),
    }
}

/// GET /api/ensemble/groups
async fn list_groups_handler(
    State(state): State<AppState>,
) -> Json<Vec<EnsembleGroup>> {
    Json(state.ensemble_router.list_groups())
}

/// GET /api/ensemble/groups/:id
async fn get_group_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.ensemble_router.get_group(&id) {
        Some(group) => (
            StatusCode::OK,
            Json(serde_json::to_value(group).unwrap()),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "group not found"})),
        ),
    }
}

/// PUT /api/ensemble/groups/:id
async fn update_group_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateGroupRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.ensemble_router.update_group(&id, req) {
        Ok(group) => (
            StatusCode::OK,
            Json(serde_json::to_value(group).unwrap()),
        ),
        Err(msg) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        ),
    }
}

/// DELETE /api/ensemble/groups/:id
async fn delete_group_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    if state.ensemble_router.delete_group(&id) {
        (
            StatusCode::NO_CONTENT,
            Json(serde_json::json!({"deleted": true})),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "group not found"})),
        )
    }
}

/// POST /api/ensemble/route
async fn route_handler(
    State(state): State<AppState>,
    Json(req): Json<FanOutRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.ensemble_router.fan_out_request(req).await {
        Ok(resp) => (
            StatusCode::OK,
            Json(serde_json::to_value(resp).unwrap()),
        ),
        Err(msg) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        ),
    }
}

/// GET /api/ensemble/history
async fn history_handler(
    State(state): State<AppState>,
) -> Json<Vec<AggregatedResponse>> {
    Json(state.ensemble_router.get_history(100))
}

/// GET /api/ensemble/stats
async fn stats_handler(
    State(state): State<AppState>,
) -> Json<EnsembleStats> {
    Json(state.ensemble_router.get_stats())
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn error_group(msg: &str) -> EnsembleGroup {
    EnsembleGroup {
        id: String::new(),
        name: msg.to_string(),
        model_ids: Vec::new(),
        strategy: None,
        weights: HashMap::new(),
        enabled: false,
        created_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the axum router for ensemble routing endpoints.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/ensemble/groups",
            post(create_group_handler).get(list_groups_handler),
        )
        .route(
            "/api/ensemble/groups/{id}",
            get(get_group_handler)
                .put(update_group_handler)
                .delete(delete_group_handler),
        )
        .route("/api/ensemble/route", post(route_handler))
        .route("/api/ensemble/history", get(history_handler))
        .route("/api/ensemble/stats", get(stats_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // ----- Group CRUD tests -----

    #[test]
    fn test_create_group_success() {
        let router = EnsembleRouter::new();
        let group = router
            .create_group(CreateGroupRequest {
                name: "test-group".into(),
                model_ids: vec!["model-a".into(), "model-b".into()],
                strategy: None,
                weights: None,
                enabled: None,
            })
            .unwrap();

        assert!(!group.id.is_empty());
        assert_eq!(group.name, "test-group");
        assert_eq!(group.model_ids.len(), 2);
        assert!(group.enabled);
        assert!(group.strategy.is_none());
    }

    #[test]
    fn test_create_group_empty_models_rejected() {
        let router = EnsembleRouter::new();
        let result = router.create_group(CreateGroupRequest {
            name: "bad".into(),
            model_ids: vec![],
            strategy: None,
            weights: None,
            enabled: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_create_group_exceeds_max_fanout() {
        let router = EnsembleRouter::with_config(EnsembleConfig {
            max_fanout: 2,
            ..Default::default()
        });
        let result = router.create_group(CreateGroupRequest {
            name: "too-big".into(),
            model_ids: vec!["a".into(), "b".into(), "c".into()],
            strategy: None,
            weights: None,
            enabled: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_create_group_with_strategy_and_weights() {
        let router = EnsembleRouter::new();
        let mut weights = HashMap::new();
        weights.insert("m1".into(), 2.0);
        weights.insert("m2".into(), 1.0);

        let group = router
            .create_group(CreateGroupRequest {
                name: "weighted".into(),
                model_ids: vec!["m1".into(), "m2".into()],
                strategy: Some(AggregationStrategy::WeightedAverage),
                weights: Some(weights.clone()),
                enabled: Some(false),
            })
            .unwrap();

        assert_eq!(group.strategy, Some(AggregationStrategy::WeightedAverage));
        assert_eq!(group.weights, weights);
        assert!(!group.enabled);
    }

    #[test]
    fn test_list_groups() {
        let router = EnsembleRouter::new();
        assert!(router.list_groups().is_empty());

        router
            .create_group(CreateGroupRequest {
                name: "g1".into(),
                model_ids: vec!["m1".into()],
                strategy: None,
                weights: None,
                enabled: None,
            })
            .unwrap();
        router
            .create_group(CreateGroupRequest {
                name: "g2".into(),
                model_ids: vec!["m2".into()],
                strategy: None,
                weights: None,
                enabled: None,
            })
            .unwrap();

        assert_eq!(router.list_groups().len(), 2);
    }

    #[test]
    fn test_get_group_found() {
        let router = EnsembleRouter::new();
        let created = router
            .create_group(CreateGroupRequest {
                name: "find-me".into(),
                model_ids: vec!["m1".into()],
                strategy: None,
                weights: None,
                enabled: None,
            })
            .unwrap();

        let found = router.get_group(&created.id).unwrap();
        assert_eq!(found.name, "find-me");
    }

    #[test]
    fn test_get_group_not_found() {
        let router = EnsembleRouter::new();
        assert!(router.get_group("nonexistent").is_none());
    }

    #[test]
    fn test_update_group() {
        let router = EnsembleRouter::new();
        let created = router
            .create_group(CreateGroupRequest {
                name: "original".into(),
                model_ids: vec!["m1".into()],
                strategy: None,
                weights: None,
                enabled: None,
            })
            .unwrap();

        let updated = router
            .update_group(
                &created.id,
                UpdateGroupRequest {
                    name: Some("renamed".into()),
                    model_ids: Some(vec!["m1".into(), "m2".into()]),
                    strategy: Some(AggregationStrategy::MajorityVote),
                    weights: None,
                    enabled: Some(false),
                },
            )
            .unwrap();

        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.model_ids.len(), 2);
        assert_eq!(updated.strategy, Some(AggregationStrategy::MajorityVote));
        assert!(!updated.enabled);
    }

    #[test]
    fn test_update_group_not_found() {
        let router = EnsembleRouter::new();
        let result = router.update_group(
            "nope",
            UpdateGroupRequest {
                name: Some("x".into()),
                model_ids: None,
                strategy: None,
                weights: None,
                enabled: None,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_group() {
        let router = EnsembleRouter::new();
        let created = router
            .create_group(CreateGroupRequest {
                name: "doomed".into(),
                model_ids: vec!["m1".into()],
                strategy: None,
                weights: None,
                enabled: None,
            })
            .unwrap();

        assert!(router.delete_group(&created.id));
        assert!(!router.delete_group(&created.id)); // already gone
        assert!(router.get_group(&created.id).is_none());
    }

    // ----- Aggregation strategy tests -----

    use std::sync::Arc;

    #[tokio::test]
    async fn test_aggregate_majority_vote() {
        let router = EnsembleRouter::new();

        let responses = vec![
            ModelResponse {
                model_id: "m1".into(),
                response: "answer-A".into(),
                confidence_score: 0.9,
                latency_ms: 10,
                token_count: 50,
                error: None,
            },
            ModelResponse {
                model_id: "m2".into(),
                response: "answer-A".into(),
                confidence_score: 0.8,
                latency_ms: 12,
                token_count: 55,
                error: None,
            },
            ModelResponse {
                model_id: "m3".into(),
                response: "answer-B".into(),
                confidence_score: 0.95,
                latency_ms: 8,
                token_count: 40,
                error: None,
            },
        ];

        let result = router
            .aggregate_responses(
                "req-1".into(),
                responses,
                AggregationStrategy::MajorityVote,
                0.7,
                &HashMap::new(),
            )
            .await;

        assert_eq!(result.final_text, "answer-A");
        assert!(!result.fallback_used);
    }

    #[tokio::test]
    async fn test_aggregate_weighted_average() {
        let router = EnsembleRouter::new();

        let mut weights = HashMap::new();
        weights.insert("m1".into(), 1.0);
        weights.insert("m2".into(), 10.0); // m2 heavily weighted

        let responses = vec![
            ModelResponse {
                model_id: "m1".into(),
                response: "light".into(),
                confidence_score: 0.9,
                latency_ms: 10,
                token_count: 20,
                error: None,
            },
            ModelResponse {
                model_id: "m2".into(),
                response: "heavy".into(),
                confidence_score: 0.5,
                latency_ms: 15,
                token_count: 30,
                error: None,
            },
        ];

        let result = router
            .aggregate_responses(
                "req-2".into(),
                responses,
                AggregationStrategy::WeightedAverage,
                0.7,
                &weights,
            )
            .await;

        // m2: 0.5 * 10 = 5.0,  m1: 0.9 * 1 = 0.9 -> m2 wins
        assert_eq!(result.final_text, "heavy");
        assert!(!result.fallback_used);
    }

    #[tokio::test]
    async fn test_aggregate_first_confident() {
        let router = EnsembleRouter::new();

        let responses = vec![
            ModelResponse {
                model_id: "m1".into(),
                response: "low-conf".into(),
                confidence_score: 0.3,
                latency_ms: 5,
                token_count: 10,
                error: None,
            },
            ModelResponse {
                model_id: "m2".into(),
                response: "high-conf".into(),
                confidence_score: 0.95,
                latency_ms: 20,
                token_count: 40,
                error: None,
            },
            ModelResponse {
                model_id: "m3".into(),
                response: "mid-conf".into(),
                confidence_score: 0.75,
                latency_ms: 12,
                token_count: 25,
                error: None,
            },
        ];

        let result = router
            .aggregate_responses(
                "req-3".into(),
                responses,
                AggregationStrategy::FirstConfident,
                0.7,
                &HashMap::new(),
            )
            .await;

        // m1 is first but below threshold, m3 is next above threshold.
        assert_eq!(result.final_text, "mid-conf");
        assert!(!result.fallback_used);
    }

    #[tokio::test]
    async fn test_aggregate_confidence_scored() {
        let router = EnsembleRouter::new();

        let responses = vec![
            ModelResponse {
                model_id: "m1".into(),
                response: "good".into(),
                confidence_score: 0.6,
                latency_ms: 10,
                token_count: 20,
                error: None,
            },
            ModelResponse {
                model_id: "m2".into(),
                response: "best".into(),
                confidence_score: 0.99,
                latency_ms: 50,
                token_count: 100,
                error: None,
            },
        ];

        let result = router
            .aggregate_responses(
                "req-4".into(),
                responses,
                AggregationStrategy::ConfidenceScored,
                0.0,
                &HashMap::new(),
            )
            .await;

        assert_eq!(result.final_text, "best");
        assert!(!result.fallback_used);
    }

    // ----- Fallback tests -----

    #[tokio::test]
    async fn test_aggregate_all_fail_triggers_fallback() {
        let router = EnsembleRouter::new();

        let responses = vec![
            ModelResponse {
                model_id: "m1".into(),
                response: String::new(),
                confidence_score: 0.0,
                latency_ms: 30,
                token_count: 0,
                error: Some("timeout".into()),
            },
            ModelResponse {
                model_id: "m2".into(),
                response: String::new(),
                confidence_score: 0.0,
                latency_ms: 30,
                token_count: 0,
                error: Some("error".into()),
            },
        ];

        let result = router
            .aggregate_responses(
                "req-fail".into(),
                responses,
                AggregationStrategy::ConfidenceScored,
                0.7,
                &HashMap::new(),
            )
            .await;

        assert!(result.fallback_used);
        assert_eq!(
            result.final_text,
            "All models in the ensemble failed to produce a response."
        );
    }

    #[tokio::test]
    async fn test_fallback_merge_with_partial_response() {
        let router = EnsembleRouter::new();

        let responses = vec![
            ModelResponse {
                model_id: "m1".into(),
                response: "partial result".into(),
                confidence_score: 0.3,
                latency_ms: 20,
                token_count: 10,
                error: Some("truncated".into()), // has error but also has text
            },
            ModelResponse {
                model_id: "m2".into(),
                response: String::new(),
                confidence_score: 0.0,
                latency_ms: 30,
                token_count: 0,
                error: Some("timeout".into()),
            },
        ];

        let result = router
            .aggregate_responses(
                "req-partial".into(),
                responses,
                AggregationStrategy::ConfidenceScored,
                0.7,
                &HashMap::new(),
            )
            .await;

        assert!(result.fallback_used);
        assert_eq!(result.final_text, "partial result");
    }

    // ----- Fan-out test -----

    #[tokio::test]
    async fn test_fan_out_request() {
        let router = EnsembleRouter::new();

        let result = router
            .fan_out_request(FanOutRequest {
                request_id: Some("fan-1".into()),
                prompt: "Hello world".into(),
                model_ids: vec!["model-a".into(), "model-b".into(), "model-c".into()],
                parameters: FanOutParameters::default(),
                timeout_ms: None,
                strategy: Some(AggregationStrategy::ConfidenceScored),
            })
            .await
            .unwrap();

        assert_eq!(result.request_id, "fan-1");
        assert!(!result.final_text.is_empty());
        assert!(result.confidence >= 0.0);
        assert_eq!(result.individual_responses.len(), 3);
        assert_eq!(result.aggregation_method, AggregationStrategy::ConfidenceScored);
        assert!(!result.fallback_used);
    }

    #[tokio::test]
    async fn test_fan_out_empty_models_rejected() {
        let router = EnsembleRouter::new();

        let result = router
            .fan_out_request(FanOutRequest {
                request_id: None,
                prompt: "test".into(),
                model_ids: vec![],
                parameters: FanOutParameters::default(),
                timeout_ms: None,
                strategy: None,
            })
            .await;

        assert!(result.is_err());
    }

    // ----- Stats tests -----

    #[tokio::test]
    async fn test_stats_initial() {
        let router = EnsembleRouter::new();
        let stats = router.get_stats();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.total_fallbacks, 0);
        assert_eq!(stats.active_groups, 0);
        assert_eq!(stats.history_size, 0);
        assert_eq!(stats.avg_latency_ms, 0.0);
    }

    #[tokio::test]
    async fn test_stats_after_fanout() {
        let router = EnsembleRouter::new();

        router
            .fan_out_request(FanOutRequest {
                request_id: Some("stat-test".into()),
                prompt: "ping".into(),
                model_ids: vec!["m1".into()],
                parameters: FanOutParameters::default(),
                timeout_ms: None,
                strategy: None,
            })
            .await
            .unwrap();

        let stats = router.get_stats();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.history_size, 1);
        assert!(stats.avg_latency_ms > 0.0);
    }

    // ----- History tests -----

    #[tokio::test]
    async fn test_history_limit() {
        let router = EnsembleRouter::new();

        for i in 0..5 {
            router
                .fan_out_request(FanOutRequest {
                    request_id: Some(format!("hist-{}", i)),
                    prompt: format!("prompt-{}", i),
                    model_ids: vec!["m1".into()],
                    parameters: FanOutParameters::default(),
                    timeout_ms: None,
                    strategy: None,
                })
                .await
                .unwrap();
        }

        let history = router.get_history(3);
        assert_eq!(history.len(), 3);
    }

    // ----- Concurrent access -----

    #[test]
    fn test_concurrent_group_creation() {
        let router = Arc::new(EnsembleRouter::new());
        let mut handles = Vec::new();

        for i in 0..10 {
            let r = router.clone();
            handles.push(thread::spawn(move || {
                r.create_group(CreateGroupRequest {
                    name: format!("concurrent-{}", i),
                    model_ids: vec![format!("m{}", i)],
                    strategy: None,
                    weights: None,
                    enabled: None,
                })
                .unwrap()
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(router.list_groups().len(), 10);
    }

    // ----- Serialization tests -----

    #[test]
    fn test_aggregation_strategy_serialization() {
        let s = AggregationStrategy::FirstConfident;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"first_confident\"");

        let deserialized: AggregationStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, AggregationStrategy::FirstConfident);
    }

    #[test]
    fn test_ensemble_config_serialization() {
        let cfg = EnsembleConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: EnsembleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.timeout_ms, cfg.timeout_ms);
        assert_eq!(deserialized.max_fanout, cfg.max_fanout);
    }

    #[test]
    fn test_fan_out_request_serialization() {
        let req = FanOutRequest {
            request_id: Some("test-req".into()),
            prompt: "What is 2+2?".into(),
            model_ids: vec!["gpt-4".into(), "claude-3".into()],
            parameters: FanOutParameters::default(),
            timeout_ms: Some(5000),
            strategy: Some(AggregationStrategy::MajorityVote),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: FanOutRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.request_id, req.request_id);
        assert_eq!(deserialized.model_ids.len(), 2);
        assert_eq!(deserialized.strategy, req.strategy);
    }

    #[test]
    fn test_aggregated_response_serialization() {
        let resp = AggregatedResponse {
            request_id: "agg-1".into(),
            final_text: "42".into(),
            confidence: 0.95,
            individual_responses: vec![ModelResponse {
                model_id: "m1".into(),
                response: "42".into(),
                confidence_score: 0.95,
                latency_ms: 10,
                token_count: 5,
                error: None,
            }],
            aggregation_method: AggregationStrategy::ConfidenceScored,
            total_latency_ms: 10,
            total_tokens: 5,
            fallback_used: false,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: AggregatedResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.request_id, "agg-1");
        assert_eq!(deserialized.confidence, 0.95);
        assert!(!deserialized.fallback_used);
    }

    #[test]
    fn test_default_fan_out_parameters() {
        let params = FanOutParameters::default();
        assert!((params.temperature - 0.7).abs() < f64::EPSILON);
        assert_eq!(params.max_tokens, 1024);
        assert!(params.top_p.is_none());
        assert!(params.stop.is_none());
    }
}
