//! Cross-provider inference orchestration.
//!
//! Routes inference requests across multiple providers for large model serving,
//! chain-of-thought reasoning steps, and distributed compute.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Strategy for distributing inference work across providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationStrategy {
    PipelineParallel,
    ChainOfThought,
    LoadBalanced,
    CostOptimized,
}

impl Default for OrchestrationStrategy {
    fn default() -> Self {
        Self::LoadBalanced
    }
}

/// Lifecycle status of an inference session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Pending,
    Routing,
    InProgress,
    Aggregating,
    Completed,
    Failed,
    Cancelled,
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::Pending
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single inference session that may span multiple providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceSession {
    pub id: String,
    pub model: String,
    pub strategy: OrchestrationStrategy,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub providers: Vec<String>,
    pub partial_results: HashMap<String, String>,
    pub error: Option<String>,
}

/// One step in a chain-of-thought reasoning pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoTStep {
    pub step_number: u32,
    pub provider_id: String,
    pub prompt: String,
    pub response: Option<String>,
    pub latency_ms: Option<u64>,
    pub tokens_in: Option<u32>,
    pub tokens_out: Option<u32>,
}

/// Describes how a model shard is routed to a specific provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardRoute {
    pub shard_index: u32,
    pub total_shards: u32,
    pub provider_id: String,
    pub model: String,
    pub layer_range: String,
}

/// Describes a provider's advertised capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapability {
    pub provider_id: String,
    pub models: Vec<String>,
    pub vram_mb: u64,
    pub latency_p50_ms: u64,
    pub latency_p99_ms: u64,
    pub queue_depth: u64,
    pub max_batch_size: u32,
    pub region: String,
}

/// Contribution of a single provider to an aggregated result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderContribution {
    pub provider_id: String,
    pub partial_responses: Vec<String>,
    pub total_latency_ms: u64,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

/// Final result after aggregating partial provider responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResult {
    pub session_id: String,
    pub model: String,
    pub final_response: String,
    pub total_latency_ms: u64,
    pub provider_contributions: HashMap<String, ProviderContribution>,
    pub tokens_total: u32,
}

/// Point-in-time snapshot of orchestration metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationMetricsSnapshot {
    pub active_sessions: u64,
    pub total_completed: u64,
    pub total_failed: u64,
    pub avg_latency_ms: u64,
    pub failover_count: u64,
}

// ---------------------------------------------------------------------------
// Internal metrics accumulator
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct OrchestrationMetrics {
    active_sessions: AtomicU64,
    total_completed: AtomicU64,
    total_failed: AtomicU64,
    avg_latency_ms: AtomicU64,
    failover_count: AtomicU64,
}

impl OrchestrationMetrics {
    fn snapshot(&self) -> OrchestrationMetricsSnapshot {
        OrchestrationMetricsSnapshot {
            active_sessions: self.active_sessions.load(Ordering::Relaxed),
            total_completed: self.total_completed.load(Ordering::Relaxed),
            total_failed: self.total_failed.load(Ordering::Relaxed),
            avg_latency_ms: self.avg_latency_ms.load(Ordering::Relaxed),
            failover_count: self.failover_count.load(Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Central coordinator for cross-provider inference.
#[derive(Debug)]
pub struct CrossProviderOrchestrator {
    sessions: DashMap<String, InferenceSession>,
    providers: DashMap<String, ProviderCapability>,
    cot_routes: DashMap<String, Vec<CoTStep>>,
    shard_routes: DashMap<String, Vec<ShardRoute>>,
    metrics: OrchestrationMetrics,
}

impl Default for CrossProviderOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossProviderOrchestrator {
    /// Create a new empty orchestrator.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            providers: DashMap::new(),
            cot_routes: DashMap::new(),
            shard_routes: DashMap::new(),
            metrics: OrchestrationMetrics::default(),
        }
    }

    // -----------------------------------------------------------------------
    // Session management
    // -----------------------------------------------------------------------

    /// Create a new inference session.
    pub fn create_session(
        &self,
        model: String,
        strategy: OrchestrationStrategy,
        preferred_providers: Vec<String>,
    ) -> InferenceSession {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        // Resolve which providers are actually available
        let resolved: Vec<String> = preferred_providers
            .iter()
            .filter(|p| self.providers.contains_key(*p))
            .cloned()
            .collect();

        let session = InferenceSession {
            id: id.clone(),
            model,
            strategy,
            status: SessionStatus::Pending,
            created_at: now,
            updated_at: now,
            providers: resolved,
            partial_results: HashMap::new(),
            error: None,
        };

        self.sessions.insert(id.clone(), session.clone());
        self.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);
        session
    }

    /// Retrieve a session by ID.
    pub fn get_session(&self, id: &str) -> Option<InferenceSession> {
        self.sessions.get(id).map(|r| r.value().clone())
    }

    /// Cancel a session. Returns `true` if the session existed and was not
    /// already terminal.
    pub fn cancel_session(&self, id: &str) -> bool {
        if let Some(mut session) = self.sessions.get_mut(id) {
            match session.status {
                SessionStatus::Completed
                | SessionStatus::Failed
                | SessionStatus::Cancelled => false,
                _ => {
                    session.status = SessionStatus::Cancelled;
                    session.updated_at = Utc::now();
                    self.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
                    true
                }
            }
        } else {
            false
        }
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<InferenceSession> {
        self.sessions.iter().map(|r| r.value().clone()).collect()
    }

    // -----------------------------------------------------------------------
    // Chain-of-thought routing
    // -----------------------------------------------------------------------

    /// Route a single CoT step to the best provider.
    pub fn route_cot_step(
        &self,
        session_id: &str,
        step_number: u32,
        prompt: String,
        preferred_provider: Option<String>,
    ) -> CoTStep {
        // Pick provider
        let provider_id = preferred_provider
            .filter(|p| self.providers.contains_key(p))
            .or_else(|| {
                self.select_provider(
                    &OrchestrationStrategy::ChainOfThought,
                    "",
                    &[],
                )
            })
            .unwrap_or_else(|| "fallback".to_string());

        let step = CoTStep {
            step_number,
            provider_id: provider_id.clone(),
            prompt,
            response: None,
            latency_ms: None,
            tokens_in: None,
            tokens_out: None,
        };

        self.cot_routes
            .entry(session_id.to_string())
            .or_default()
            .push(step.clone());

        // Transition session to InProgress if possible
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if session.status == SessionStatus::Pending || session.status == SessionStatus::Routing {
                session.status = SessionStatus::InProgress;
                session.updated_at = Utc::now();
            }
        }

        step
    }

    /// Record a provider's response for a CoT step.
    pub fn add_cot_response(
        &self,
        session_id: &str,
        step_number: u32,
        provider_id: &str,
        response: String,
        latency_ms: u64,
        tokens_in: u32,
        tokens_out: u32,
    ) {
        if let Some(mut steps) = self.cot_routes.get_mut(session_id) {
            if let Some(step) = steps.iter_mut().find(|s| s.step_number == step_number) {
                step.response = Some(response.clone());
                step.latency_ms = Some(latency_ms);
                step.tokens_in = Some(tokens_in);
                step.tokens_out = Some(tokens_out);
            }
        }

        // Store partial result keyed by provider+step
        let key = format!("cot:{}:{}", provider_id, step_number);
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.partial_results.insert(key, response);
            session.updated_at = Utc::now();
        }
    }

    // -----------------------------------------------------------------------
    // Shard routing
    // -----------------------------------------------------------------------

    /// Route a model shard to a specific provider.
    pub fn route_shard(
        &self,
        session_id: &str,
        shard_index: u32,
        total_shards: u32,
        model: String,
        layer_range: String,
        preferred_provider: Option<String>,
    ) -> ShardRoute {
        let provider_id = preferred_provider
            .filter(|p| self.providers.contains_key(p))
            .or_else(|| {
                self.select_provider(
                    &OrchestrationStrategy::PipelineParallel,
                    &model,
                    &[],
                )
            })
            .unwrap_or_else(|| "fallback".to_string());

        let route = ShardRoute {
            shard_index,
            total_shards,
            provider_id: provider_id.clone(),
            model: model.clone(),
            layer_range,
        };

        self.shard_routes
            .entry(session_id.to_string())
            .or_default()
            .push(route.clone());

        if let Some(mut session) = self.sessions.get_mut(session_id) {
            if session.status == SessionStatus::Pending || session.status == SessionStatus::Routing {
                session.status = SessionStatus::InProgress;
                session.updated_at = Utc::now();
            }
        }

        route
    }

    // -----------------------------------------------------------------------
    // Provider management
    // -----------------------------------------------------------------------

    /// Register (or update) a provider's capabilities.
    pub fn register_provider(&self, cap: ProviderCapability) -> ProviderCapability {
        self.providers
            .insert(cap.provider_id.clone(), cap.clone());
        cap
    }

    /// Unregister a provider. Returns `true` if the provider existed.
    pub fn unregister_provider(&self, provider_id: &str) -> bool {
        self.providers.remove(provider_id).is_some()
    }

    /// List all registered providers.
    pub fn list_providers(&self) -> Vec<ProviderCapability> {
        self.providers.iter().map(|r| r.value().clone()).collect()
    }

    // -----------------------------------------------------------------------
    // Active routes
    // -----------------------------------------------------------------------

    /// Get all active shard routes keyed by session ID.
    pub fn get_active_routes(&self) -> HashMap<String, Vec<ShardRoute>> {
        self.shard_routes
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }

    // -----------------------------------------------------------------------
    // Aggregation
    // -----------------------------------------------------------------------

    /// Aggregate partial results from all providers for a session.
    pub fn aggregate_results(&self, session_id: &str) -> Result<AggregatedResult, String> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        if session.partial_results.is_empty() {
            return Err("No partial results to aggregate".to_string());
        }

        let mut provider_contributions: HashMap<String, ProviderContribution> = HashMap::new();
        let mut all_parts: Vec<String> = Vec::new();
        let mut total_latency_ms: u64 = 0;
        let mut tokens_total: u32 = 0;

        // Collect CoT steps for latency / token info
        let cot_steps = self
            .cot_routes
            .get(session_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();

        for (key, response) in &session.partial_results {
            // key format: "cot:<provider>:<step>" or "shard:<provider>:<shard>"
            let parts: Vec<&str> = key.splitn(3, ':').collect();
            let provider_id = parts.get(1).unwrap_or(&"unknown");

            all_parts.push(response.clone());

            let contribution = provider_contributions
                .entry(provider_id.to_string())
                .or_insert_with(|| ProviderContribution {
                    provider_id: provider_id.to_string(),
                    partial_responses: Vec::new(),
                    total_latency_ms: 0,
                    tokens_in: 0,
                    tokens_out: 0,
                });

            contribution.partial_responses.push(response.clone());

            // Find matching CoT step for latency / token details
            if parts.get(0).copied() == Some("cot") {
                if let Ok(step_num) = parts.get(2).unwrap_or(&"0").parse::<u32>() {
                    if let Some(step) = cot_steps.iter().find(|s| s.step_number == step_num) {
                        total_latency_ms += step.latency_ms.unwrap_or(0);
                        contribution.total_latency_ms += step.latency_ms.unwrap_or(0);
                        contribution.tokens_in += step.tokens_in.unwrap_or(0);
                        contribution.tokens_out += step.tokens_out.unwrap_or(0);
                        tokens_total += step.tokens_in.unwrap_or(0) + step.tokens_out.unwrap_or(0);
                    }
                }
            }
        }

        let final_response = all_parts.join("\n\n---\n\n");

        // Transition session to Completed
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.status = SessionStatus::Completed;
            session.updated_at = Utc::now();
        }
        self.metrics.active_sessions.fetch_sub(1, Ordering::Relaxed);
        self.metrics.total_completed.fetch_add(1, Ordering::Relaxed);

        Ok(AggregatedResult {
            session_id: session_id.to_string(),
            model: session.model.clone(),
            final_response,
            total_latency_ms,
            provider_contributions,
            tokens_total,
        })
    }

    // -----------------------------------------------------------------------
    // Provider selection
    // -----------------------------------------------------------------------

    /// Select the best provider given a strategy, model, and exclusion list.
    ///
    /// Selection criteria:
    /// - **LoadBalanced**: pick the provider with the smallest queue depth.
    /// - **CostOptimized**: pick the provider with the best latency P99.
    /// - **PipelineParallel** / **ChainOfThought**: pick the provider with
    ///   the most available VRAM.
    /// - Falls back to the first provider that matches the model.
    pub fn select_provider(
        &self,
        strategy: &OrchestrationStrategy,
        model: &str,
        exclude: &[String],
    ) -> Option<String> {
        let candidates: Vec<ProviderCapability> = self
            .providers
            .iter()
            .filter(|r| {
                let cap = r.value();
                !exclude.contains(&cap.provider_id)
                    && (model.is_empty() || cap.models.iter().any(|m| m == model))
            })
            .map(|r| r.value().clone())
            .collect();

        if candidates.is_empty() {
            return None;
        }

        match strategy {
            OrchestrationStrategy::LoadBalanced => candidates
                .iter()
                .min_by_key(|c| c.queue_depth)
                .map(|c| c.provider_id.clone()),

            OrchestrationStrategy::CostOptimized => candidates
                .iter()
                .min_by_key(|c| c.latency_p99_ms)
                .map(|c| c.provider_id.clone()),

            OrchestrationStrategy::PipelineParallel
            | OrchestrationStrategy::ChainOfThought => candidates
                .iter()
                .max_by_key(|c| c.vram_mb)
                .map(|c| c.provider_id.clone()),
        }
    }

    // -----------------------------------------------------------------------
    // Metrics
    // -----------------------------------------------------------------------

    /// Return a point-in-time snapshot of all metrics.
    pub fn get_metrics(&self) -> OrchestrationMetricsSnapshot {
        self.metrics.snapshot()
    }
}

// ---------------------------------------------------------------------------
// REST request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub model: String,
    #[serde(default)]
    pub strategy: OrchestrationStrategy,
    #[serde(default)]
    pub preferred_providers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CoTRouteRequest {
    pub session_id: String,
    pub step_number: u32,
    pub prompt: String,
    pub preferred_provider: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShardRouteRequest {
    pub session_id: String,
    pub shard_index: u32,
    pub total_shards: u32,
    pub model: String,
    pub layer_range: String,
    pub preferred_provider: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AggregateRequest {
    pub session_id: String,
}

// Re-use InferenceSession and ProviderCapability directly as response types.

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// POST /api/orchestration/session
async fn create_session_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<InferenceSession>) {
    let session = state.cross_provider_orchestrator.create_session(
        req.model,
        req.strategy,
        req.preferred_providers,
    );
    (StatusCode::CREATED, Json(session))
}

/// GET /api/orchestration/session/:id
async fn get_session_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<InferenceSession>>) {
    let session = state.cross_provider_orchestrator.get_session(&id);
    match session {
        Some(s) => (StatusCode::OK, Json(Some(s))),
        None => (StatusCode::NOT_FOUND, Json(None)),
    }
}

/// DELETE /api/orchestration/session/:id
async fn delete_session_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    if state.cross_provider_orchestrator.cancel_session(&id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

/// POST /api/orchestration/cot-route
async fn cot_route_handler(
    State(state): State<AppState>,
    Json(req): Json<CoTRouteRequest>,
) -> (StatusCode, Json<CoTStep>) {
    let step = state.cross_provider_orchestrator.route_cot_step(
        &req.session_id,
        req.step_number,
        req.prompt,
        req.preferred_provider,
    );
    (StatusCode::OK, Json(step))
}

/// POST /api/orchestration/shard-route
async fn shard_route_handler(
    State(state): State<AppState>,
    Json(req): Json<ShardRouteRequest>,
) -> (StatusCode, Json<ShardRoute>) {
    let route = state.cross_provider_orchestrator.route_shard(
        &req.session_id,
        req.shard_index,
        req.total_shards,
        req.model,
        req.layer_range,
        req.preferred_provider,
    );
    (StatusCode::OK, Json(route))
}

/// POST /api/orchestration/aggregate
async fn aggregate_handler(
    State(state): State<AppState>,
    Json(req): Json<AggregateRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.cross_provider_orchestrator.aggregate_results(&req.session_id) {
        Ok(result) => (StatusCode::OK, Json(serde_json::to_value(result).unwrap_or_default())),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

/// GET /api/orchestration/providers
async fn list_providers_handler(
    State(state): State<AppState>,
) -> Json<Vec<ProviderCapability>> {
    Json(state.cross_provider_orchestrator.list_providers())
}

/// GET /api/orchestration/routes
async fn active_routes_handler(
    State(state): State<AppState>,
) -> Json<HashMap<String, Vec<ShardRoute>>> {
    Json(state.cross_provider_orchestrator.get_active_routes())
}

/// GET /api/orchestration/metrics
async fn metrics_handler(
    State(state): State<AppState>,
) -> Json<OrchestrationMetricsSnapshot> {
    Json(state.cross_provider_orchestrator.get_metrics())
}

/// GET /api/orchestration/sessions
async fn list_sessions_handler(
    State(state): State<AppState>,
) -> Json<Vec<InferenceSession>> {
    Json(state.cross_provider_orchestrator.list_sessions())
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the axum router for cross-provider orchestration endpoints.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/orchestration/session", post(create_session_handler))
        .route(
            "/api/orchestration/session/{id}",
            get(get_session_handler),
        )
        .route(
            "/api/orchestration/session/{id}",
            delete(delete_session_handler),
        )
        .route("/api/orchestration/cot-route", post(cot_route_handler))
        .route(
            "/api/orchestration/shard-route",
            post(shard_route_handler),
        )
        .route(
            "/api/orchestration/aggregate",
            post(aggregate_handler),
        )
        .route(
            "/api/orchestration/providers",
            get(list_providers_handler),
        )
        .route(
            "/api/orchestration/routes",
            get(active_routes_handler),
        )
        .route("/api/orchestration/metrics", get(metrics_handler))
        .route(
            "/api/orchestration/sessions",
            get(list_sessions_handler),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    /// Helper to build a test provider capability.
    fn test_provider(id: &str, queue: u64, p99: u64, vram: u64) -> ProviderCapability {
        ProviderCapability {
            provider_id: id.to_string(),
            models: vec!["llama-70b".to_string(), "mixtral-8x7b".to_string()],
            vram_mb: vram,
            latency_p50_ms: 50,
            latency_p99_ms: p99,
            queue_depth: queue,
            max_batch_size: 32,
            region: "us-east-1".to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // test_new_orchestrator
    // -----------------------------------------------------------------------
    #[test]
    fn test_new_orchestrator() {
        let orch = CrossProviderOrchestrator::new();
        let metrics = orch.get_metrics();
        assert_eq!(metrics.active_sessions, 0);
        assert_eq!(metrics.total_completed, 0);
        assert_eq!(metrics.total_failed, 0);
        assert_eq!(metrics.avg_latency_ms, 0);
        assert_eq!(metrics.failover_count, 0);
        assert!(orch.list_sessions().is_empty());
        assert!(orch.list_providers().is_empty());
    }

    // -----------------------------------------------------------------------
    // test_create_session
    // -----------------------------------------------------------------------
    #[test]
    fn test_create_session() {
        let orch = CrossProviderOrchestrator::new();
        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::ChainOfThought,
            vec![],
        );
        assert!(!session.id.is_empty());
        assert_eq!(session.model, "llama-70b");
        assert_eq!(session.strategy, OrchestrationStrategy::ChainOfThought);
        assert_eq!(session.status, SessionStatus::Pending);
        assert!(session.providers.is_empty());
        assert!(session.error.is_none());

        let metrics = orch.get_metrics();
        assert_eq!(metrics.active_sessions, 1);
    }

    // -----------------------------------------------------------------------
    // test_get_session_not_found
    // -----------------------------------------------------------------------
    #[test]
    fn test_get_session_not_found() {
        let orch = CrossProviderOrchestrator::new();
        let result = orch.get_session("nonexistent");
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // test_cancel_session
    // -----------------------------------------------------------------------
    #[test]
    fn test_cancel_session() {
        let orch = CrossProviderOrchestrator::new();
        let session = orch.create_session(
            "mixtral-8x7b".to_string(),
            OrchestrationStrategy::PipelineParallel,
            vec![],
        );
        let id = session.id.clone();

        // Cancel should succeed
        assert!(orch.cancel_session(&id));
        let fetched = orch.get_session(&id).unwrap();
        assert_eq!(fetched.status, SessionStatus::Cancelled);

        // Second cancel should fail (already terminal)
        assert!(!orch.cancel_session(&id));

        // Cancel non-existent should fail
        assert!(!orch.cancel_session("nope"));

        let metrics = orch.get_metrics();
        assert_eq!(metrics.active_sessions, 0);
    }

    // -----------------------------------------------------------------------
    // test_list_sessions
    // -----------------------------------------------------------------------
    #[test]
    fn test_list_sessions() {
        let orch = CrossProviderOrchestrator::new();
        assert!(orch.list_sessions().is_empty());

        orch.create_session("m1".to_string(), OrchestrationStrategy::default(), vec![]);
        orch.create_session("m2".to_string(), OrchestrationStrategy::default(), vec![]);
        orch.create_session("m3".to_string(), OrchestrationStrategy::default(), vec![]);

        let sessions = orch.list_sessions();
        assert_eq!(sessions.len(), 3);
    }

    // -----------------------------------------------------------------------
    // test_route_cot_step
    // -----------------------------------------------------------------------
    #[test]
    fn test_route_cot_step() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("prov-a", 0, 100, 8192));

        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::ChainOfThought,
            vec!["prov-a".to_string()],
        );
        let sid = session.id.clone();

        let step = orch.route_cot_step(&sid, 1, "Think step 1".into(), None);
        assert_eq!(step.step_number, 1);
        assert_eq!(step.provider_id, "prov-a");
        assert_eq!(step.prompt, "Think step 1");
        assert!(step.response.is_none());

        // Session should now be InProgress
        let s = orch.get_session(&sid).unwrap();
        assert_eq!(s.status, SessionStatus::InProgress);
    }

    // -----------------------------------------------------------------------
    // test_add_cot_response
    // -----------------------------------------------------------------------
    #[test]
    fn test_add_cot_response() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("prov-b", 0, 80, 4096));

        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::ChainOfThought,
            vec!["prov-b".to_string()],
        );
        let sid = session.id.clone();

        orch.route_cot_step(&sid, 1, "Step 1".into(), Some("prov-b".into()));
        orch.add_cot_response(&sid, 1, "prov-b", "Result of step 1".into(), 120, 50, 30);

        // Verify partial result stored
        let s = orch.get_session(&sid).unwrap();
        assert_eq!(s.partial_results.len(), 1);
        assert_eq!(
            s.partial_results.get("cot:prov-b:1").unwrap(),
            "Result of step 1"
        );
    }

    // -----------------------------------------------------------------------
    // test_route_shard
    // -----------------------------------------------------------------------
    #[test]
    fn test_route_shard() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("gpu-1", 2, 60, 16384));

        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::PipelineParallel,
            vec!["gpu-1".to_string()],
        );
        let sid = session.id.clone();

        let route = orch.route_shard(
            &sid,
            0,
            4,
            "llama-70b".to_string(),
            "0-16".to_string(),
            Some("gpu-1".into()),
        );

        assert_eq!(route.shard_index, 0);
        assert_eq!(route.total_shards, 4);
        assert_eq!(route.provider_id, "gpu-1");
        assert_eq!(route.layer_range, "0-16");

        // Verify stored in active routes
        let routes = orch.get_active_routes();
        assert_eq!(routes.get(&sid).unwrap().len(), 1);
    }

    // -----------------------------------------------------------------------
    // test_register_provider
    // -----------------------------------------------------------------------
    #[test]
    fn test_register_provider() {
        let orch = CrossProviderOrchestrator::new();

        let cap = test_provider("prov-x", 5, 200, 8192);
        let returned = orch.register_provider(cap.clone());
        assert_eq!(returned.provider_id, "prov-x");

        let providers = orch.list_providers();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, "prov-x");

        // Re-register should update
        let updated = ProviderCapability {
            provider_id: "prov-x".to_string(),
            models: vec!["new-model".to_string()],
            vram_mb: 32768,
            latency_p50_ms: 10,
            latency_p99_ms: 30,
            queue_depth: 0,
            max_batch_size: 64,
            region: "eu-west-1".to_string(),
        };
        orch.register_provider(updated);
        let providers = orch.list_providers();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].region, "eu-west-1");
        assert_eq!(providers[0].vram_mb, 32768);
    }

    // -----------------------------------------------------------------------
    // test_unregister_provider
    // -----------------------------------------------------------------------
    #[test]
    fn test_unregister_provider() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("prov-y", 0, 50, 4096));

        assert!(orch.unregister_provider("prov-y"));
        assert!(!orch.unregister_provider("prov-y")); // already gone
        assert!(orch.list_providers().is_empty());
    }

    // -----------------------------------------------------------------------
    // test_select_provider_load_balanced
    // -----------------------------------------------------------------------
    #[test]
    fn test_select_provider_load_balanced() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("heavy", 100, 200, 8192));
        orch.register_provider(test_provider("light", 2, 150, 4096));
        orch.register_provider(test_provider("medium", 50, 100, 16384));

        let selected = orch.select_provider(
            &OrchestrationStrategy::LoadBalanced,
            "llama-70b",
            &[],
        );
        assert_eq!(selected, Some("light".to_string()));

        // With exclusion
        let selected = orch.select_provider(
            &OrchestrationStrategy::LoadBalanced,
            "llama-70b",
            &["light".to_string()],
        );
        assert_eq!(selected, Some("medium".to_string()));
    }

    // -----------------------------------------------------------------------
    // test_select_provider_cost_optimized
    // -----------------------------------------------------------------------
    #[test]
    fn test_select_provider_cost_optimized() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("fast", 10, 30, 4096));
        orch.register_provider(test_provider("slow", 10, 500, 8192));

        let selected = orch.select_provider(
            &OrchestrationStrategy::CostOptimized,
            "llama-70b",
            &[],
        );
        assert_eq!(selected, Some("fast".to_string()));
    }

    // -----------------------------------------------------------------------
    // test_select_provider_vram_for_pipeline
    // -----------------------------------------------------------------------
    #[test]
    fn test_select_provider_vram_for_pipeline() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("small-gpu", 0, 50, 2048));
        orch.register_provider(test_provider("big-gpu", 0, 100, 24576));

        let selected = orch.select_provider(
            &OrchestrationStrategy::PipelineParallel,
            "llama-70b",
            &[],
        );
        assert_eq!(selected, Some("big-gpu".to_string()));
    }

    // -----------------------------------------------------------------------
    // test_select_provider_model_filter
    // -----------------------------------------------------------------------
    #[test]
    fn test_select_provider_model_filter() {
        let orch = CrossProviderOrchestrator::new();

        // Provider with different model
        let mut cap = test_provider("other-model-prov", 0, 50, 8192);
        cap.models = vec!["gpt-4".to_string()];
        orch.register_provider(cap);

        // Provider with target model
        orch.register_provider(test_provider("llama-prov", 5, 100, 4096));

        let selected = orch.select_provider(
            &OrchestrationStrategy::LoadBalanced,
            "llama-70b",
            &[],
        );
        assert_eq!(selected, Some("llama-prov".to_string()));
    }

    // -----------------------------------------------------------------------
    // test_select_provider_no_candidates
    // -----------------------------------------------------------------------
    #[test]
    fn test_select_provider_no_candidates() {
        let orch = CrossProviderOrchestrator::new();
        let selected = orch.select_provider(
            &OrchestrationStrategy::LoadBalanced,
            "nonexistent-model",
            &[],
        );
        assert!(selected.is_none());
    }

    // -----------------------------------------------------------------------
    // test_aggregate_results
    // -----------------------------------------------------------------------
    #[test]
    fn test_aggregate_results() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("prov-a", 0, 80, 8192));

        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::ChainOfThought,
            vec!["prov-a".to_string()],
        );
        let sid = session.id.clone();

        // Add two CoT steps
        orch.route_cot_step(&sid, 1, "Step 1".into(), Some("prov-a".into()));
        orch.add_cot_response(&sid, 1, "prov-a", "Answer part 1".into(), 100, 20, 10);

        orch.route_cot_step(&sid, 2, "Step 2".into(), Some("prov-a".into()));
        orch.add_cot_response(&sid, 2, "prov-a", "Answer part 2".into(), 150, 25, 15);

        let result = orch.aggregate_results(&sid).unwrap();
        assert_eq!(result.session_id, sid);
        assert_eq!(result.model, "llama-70b");
        assert!(result.final_response.contains("Answer part 1"));
        assert!(result.final_response.contains("Answer part 2"));
        assert_eq!(result.total_latency_ms, 250); // 100 + 150
        assert_eq!(result.tokens_total, 70); // (20+10) + (25+15)
        assert!(result.provider_contributions.contains_key("prov-a"));

        let metrics = orch.get_metrics();
        assert_eq!(metrics.total_completed, 1);
        assert_eq!(metrics.active_sessions, 0);
    }

    // -----------------------------------------------------------------------
    // test_aggregate_results_empty_session
    // -----------------------------------------------------------------------
    #[test]
    fn test_aggregate_results_empty_session() {
        let orch = CrossProviderOrchestrator::new();
        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::LoadBalanced,
            vec![],
        );
        let sid = session.id.clone();

        let result = orch.aggregate_results(&sid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No partial results"));
    }

    // -----------------------------------------------------------------------
    // test_aggregate_results_nonexistent_session
    // -----------------------------------------------------------------------
    #[test]
    fn test_aggregate_results_nonexistent_session() {
        let orch = CrossProviderOrchestrator::new();
        let result = orch.aggregate_results("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // -----------------------------------------------------------------------
    // test_metrics_tracking
    // -----------------------------------------------------------------------
    #[test]
    fn test_metrics_tracking() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("prov-a", 0, 80, 8192));

        // Create 3 sessions
        let s1 = orch.create_session("m".into(), OrchestrationStrategy::default(), vec![]);
        let s2 = orch.create_session("m".into(), OrchestrationStrategy::default(), vec![]);
        let s3 = orch.create_session("m".into(), OrchestrationStrategy::default(), vec![]);

        assert_eq!(orch.get_metrics().active_sessions, 3);

        // Cancel one
        orch.cancel_session(&s1.id);
        assert_eq!(orch.get_metrics().active_sessions, 2);

        // Complete one via aggregation
        orch.route_cot_step(&s2.id, 1, "prompt".into(), Some("prov-a".into()));
        orch.add_cot_response(&s2.id, 1, "prov-a", "result".into(), 80, 10, 5);
        orch.aggregate_results(&s2.id).unwrap();

        assert_eq!(orch.get_metrics().active_sessions, 1);
        assert_eq!(orch.get_metrics().total_completed, 1);
    }

    // -----------------------------------------------------------------------
    // test_concurrent_sessions
    // -----------------------------------------------------------------------
    #[test]
    fn test_concurrent_sessions() {
        let orch = Arc::new(CrossProviderOrchestrator::new());
        orch.register_provider(test_provider("shared-prov", 0, 50, 8192));

        let mut handles = Vec::new();

        for i in 0..10 {
            let orch_clone = Arc::clone(&orch);
            let handle = thread::spawn(move || {
                let session = orch_clone.create_session(
                    "llama-70b".to_string(),
                    OrchestrationStrategy::LoadBalanced,
                    vec!["shared-prov".to_string()],
                );
                let sid = session.id.clone();

                orch_clone.route_cot_step(&sid, i, format!("Prompt {}", i), None);
                orch_clone.add_cot_response(
                    &sid,
                    i,
                    "shared-prov",
                    format!("Response {}", i),
                    100,
                    20,
                    10,
                );

                orch_clone.aggregate_results(&sid).unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let metrics = orch.get_metrics();
        assert_eq!(metrics.active_sessions, 0);
        assert_eq!(metrics.total_completed, 10);
        assert_eq!(metrics.total_failed, 0);
        assert_eq!(orch.list_sessions().len(), 10);
    }

    // -----------------------------------------------------------------------
    // test_preferred_provider_resolution
    // -----------------------------------------------------------------------
    #[test]
    fn test_preferred_provider_resolution() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("prov-real", 0, 80, 8192));

        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::default(),
            vec!["prov-real".to_string(), "prov-ghost".to_string()],
        );

        // Ghost provider should be filtered out
        assert_eq!(session.providers, vec!["prov-real".to_string()]);
    }

    // -----------------------------------------------------------------------
    // test_cot_route_with_no_providers
    // -----------------------------------------------------------------------
    #[test]
    fn test_cot_route_with_no_providers() {
        let orch = CrossProviderOrchestrator::new();
        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::ChainOfThought,
            vec![],
        );
        let sid = session.id.clone();

        // No providers registered; should fall back to "fallback"
        let step = orch.route_cot_step(&sid, 1, "prompt".into(), None);
        assert_eq!(step.provider_id, "fallback");
    }

    // -----------------------------------------------------------------------
    // test_shard_route_fallback
    // -----------------------------------------------------------------------
    #[test]
    fn test_shard_route_fallback() {
        let orch = CrossProviderOrchestrator::new();
        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::PipelineParallel,
            vec![],
        );
        let sid = session.id.clone();

        let route = orch.route_shard(
            &sid,
            0,
            2,
            "llama-70b".to_string(),
            "0-32".to_string(),
            None,
        );
        assert_eq!(route.provider_id, "fallback");
    }

    // -----------------------------------------------------------------------
    // test_multiple_shards_per_session
    // -----------------------------------------------------------------------
    #[test]
    fn test_multiple_shards_per_session() {
        let orch = CrossProviderOrchestrator::new();
        orch.register_provider(test_provider("gpu-1", 0, 60, 16384));
        orch.register_provider(test_provider("gpu-2", 0, 70, 16384));

        let session = orch.create_session(
            "llama-70b".to_string(),
            OrchestrationStrategy::PipelineParallel,
            vec!["gpu-1".to_string(), "gpu-2".to_string()],
        );
        let sid = session.id.clone();

        orch.route_shard(&sid, 0, 4, "llama-70b".into(), "0-16".into(), Some("gpu-1".into()));
        orch.route_shard(&sid, 1, 4, "llama-70b".into(), "17-32".into(), Some("gpu-2".into()));
        orch.route_shard(&sid, 2, 4, "llama-70b".into(), "33-48".into(), Some("gpu-1".into()));
        orch.route_shard(&sid, 3, 4, "llama-70b".into(), "49-64".into(), Some("gpu-2".into()));

        let routes = orch.get_active_routes();
        assert_eq!(routes.get(&sid).unwrap().len(), 4);

        // Verify shard assignments
        let session_routes = routes.get(&sid).unwrap();
        assert_eq!(session_routes[0].provider_id, "gpu-1");
        assert_eq!(session_routes[1].provider_id, "gpu-2");
        assert_eq!(session_routes[2].provider_id, "gpu-1");
        assert_eq!(session_routes[3].provider_id, "gpu-2");
    }
}
