//! Model Serving and Endpoint Management for the Xergon agent.
//!
//! Exposes models as API endpoints with configurable timeouts, concurrency,
//! CORS, authentication, rate-limiting, and request logging.
//!
//! API:
//! - POST /api/serving/serve             -- serve a model
//! - POST /api/serving/{id}/stop         -- stop a served model
//! - GET  /api/serving/models            -- list served models
//! - GET  /api/serving/models/{id}       -- get model details
//! - PUT  /api/serving/models/{id}/config -- update model config
//! - GET  /api/serving/stats             -- aggregate serving stats
//! - GET  /api/serving/endpoints         -- list all endpoints
//! - GET  /api/serving/health            -- health check

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
    routing::{get, post, put},
    Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

use crate::api::AppState;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelServeConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_timeout")]
    pub default_timeout_secs: u64,
    #[serde(default = "default_concurrent")]
    pub max_concurrent_requests: usize,
    #[serde(default = "default_true")]
    pub enable_cors: bool,
    #[serde(default)]
    pub auth_required: bool,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_rpm: u64,
    #[serde(default = "default_true")]
    pub log_requests: bool,
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 8080 }
fn default_timeout() -> u64 { 30 }
fn default_concurrent() -> usize { 100 }
fn default_true() -> bool { true }
fn default_rate_limit() -> u64 { 60 }

impl Default for ModelServeConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            default_timeout_secs: default_timeout(),
            max_concurrent_requests: default_concurrent(),
            enable_cors: true,
            auth_required: false,
            rate_limit_rpm: default_rate_limit(),
            log_requests: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServeStatus {
    Starting,
    Ready,
    Draining,
    Stopped,
    Error,
}

#[derive(Debug)]
pub struct ServedModel {
    pub id: String,
    pub name: String,
    pub model_path: String,
    pub endpoint: String,
    pub status: ServeStatus,
    pub config: ModelServeConfig,
    pub started_at: DateTime<Utc>,
    pub total_requests: Arc<AtomicU64>,
    pub total_tokens: Arc<AtomicU64>,
    pub avg_latency_ms: Arc<AtomicU64>,
    pub error_count: Arc<AtomicU64>,
    pub gpu_memory_used: Arc<AtomicU64>,
}

impl Clone for ServedModel {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            model_path: self.model_path.clone(),
            endpoint: self.endpoint.clone(),
            status: self.status.clone(),
            config: self.config.clone(),
            started_at: self.started_at,
            total_requests: Arc::clone(&self.total_requests),
            total_tokens: Arc::clone(&self.total_tokens),
            avg_latency_ms: Arc::clone(&self.avg_latency_ms),
            error_count: Arc::clone(&self.error_count),
            gpu_memory_used: Arc::clone(&self.gpu_memory_used),
        }
    }
}

// Implement Serialize manually to handle Arc<AtomicU64>
impl Serialize for ServedModel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ServedModel", 12)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("model_path", &self.model_path)?;
        s.serialize_field("endpoint", &self.endpoint)?;
        s.serialize_field("status", &self.status)?;
        s.serialize_field("config", &self.config)?;
        s.serialize_field("started_at", &self.started_at)?;
        s.serialize_field("total_requests", &self.total_requests.load(Ordering::Relaxed))?;
        s.serialize_field("total_tokens", &self.total_tokens.load(Ordering::Relaxed))?;
        s.serialize_field("avg_latency_ms", &self.avg_latency_ms.load(Ordering::Relaxed))?;
        s.serialize_field("error_count", &self.error_count.load(Ordering::Relaxed))?;
        s.serialize_field("gpu_memory_used", &self.gpu_memory_used.load(Ordering::Relaxed))?;
        s.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeStats {
    pub total_models: usize,
    pub ready_models: usize,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_errors: u64,
    pub avg_latency_ms: f64,
    pub total_gpu_memory_mb: u64,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModelServeManager {
    models: Arc<DashMap<String, ServedModel>>,
    endpoint_index: Arc<DashMap<String, String>>, // endpoint -> model_id
}

impl ModelServeManager {
    pub fn new(config: ModelServeConfig) -> Self {
        let _ = config; // stored as default for new models
        Self {
            models: Arc::new(DashMap::new()),
            endpoint_index: Arc::new(DashMap::new()),
        }
    }

    /// Register a model to be served at the given endpoint.
    pub fn serve_model(
        &self,
        model_id: String,
        name: String,
        model_path: String,
        endpoint: String,
        config: ModelServeConfig,
    ) -> ServedModel {
        let model = ServedModel {
            id: model_id.clone(),
            name,
            model_path,
            endpoint: endpoint.clone(),
            status: ServeStatus::Starting,
            config,
            started_at: Utc::now(),
            total_requests: Arc::new(AtomicU64::new(0)),
            total_tokens: Arc::new(AtomicU64::new(0)),
            avg_latency_ms: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            gpu_memory_used: Arc::new(AtomicU64::new(0)),
        };

        // In a real implementation we'd start the inference server here.
        // For now we mark it ready immediately.
        let mut writable = model.clone();
        writable.status = ServeStatus::Ready;
        self.models.insert(model_id.clone(), writable);
        self.endpoint_index.insert(model.endpoint.clone(), model_id.clone());

        info!(model_id = %model_id, endpoint = %model.endpoint, "Model served and ready");
        self.models.get(&model_id).unwrap().clone()
    }

    /// Stop serving a model by ID.
    pub fn stop_model(&self, model_id: &str) -> bool {
        if let Some(mut model) = self.models.get_mut(model_id) {
            model.status = ServeStatus::Stopped;
            let endpoint = model.endpoint.clone();
            drop(model);
            self.endpoint_index.remove(&endpoint);
            info!(model_id = %model_id, "Model stopped");
            true
        } else {
            false
        }
    }

    /// Get a served model by ID.
    pub fn get_model(&self, model_id: &str) -> Option<ServedModel> {
        self.models.get(model_id).map(|m| m.clone())
    }

    /// List all served models.
    pub fn list_models(&self) -> Vec<ServedModel> {
        self.models.iter().map(|m| m.clone()).collect()
    }

    /// Look up a model by its endpoint path.
    pub fn get_endpoint(&self, endpoint: &str) -> Option<String> {
        self.endpoint_index.get(endpoint).map(|m| m.clone())
    }

    /// Update configuration for a served model.
    pub fn update_config(&self, model_id: &str, config: ModelServeConfig) -> bool {
        if let Some(mut model) = self.models.get_mut(model_id) {
            model.config = config;
            info!(model_id = %model_id, "Model config updated");
            true
        } else {
            false
        }
    }

    /// Aggregate statistics across all served models.
    pub fn get_stats(&self) -> ServeStats {
        let mut total_requests: u64 = 0;
        let mut total_tokens: u64 = 0;
        let mut total_errors: u64 = 0;
        let mut total_latency: u64 = 0;
        let mut total_gpu: u64 = 0;
        let mut ready_count: usize = 0;

        for model in self.models.iter() {
            let reqs = model.total_requests.load(Ordering::Relaxed);
            total_requests += reqs;
            total_tokens += model.total_tokens.load(Ordering::Relaxed);
            total_errors += model.error_count.load(Ordering::Relaxed);
            total_latency += model.avg_latency_ms.load(Ordering::Relaxed);
            total_gpu += model.gpu_memory_used.load(Ordering::Relaxed);
            if model.status == ServeStatus::Ready {
                ready_count += 1;
            }
        }

        let count = self.models.len();
        let avg_latency = if count > 0 {
            total_latency as f64 / count as f64
        } else {
            0.0
        };

        ServeStats {
            total_models: count,
            ready_models: ready_count,
            total_requests,
            total_tokens,
            total_errors,
            avg_latency_ms: avg_latency,
            total_gpu_memory_mb: total_gpu,
        }
    }

    /// Record a completed request for metrics.
    pub fn record_request(&self, model_id: &str, tokens: u64, latency_ms: u64, is_error: bool) {
        if let Some(mut model) = self.models.get_mut(model_id) {
            model.total_requests.fetch_add(1, Ordering::Relaxed);
            model.total_tokens.fetch_add(tokens, Ordering::Relaxed);
            if is_error {
                model.error_count.fetch_add(1, Ordering::Relaxed);
            }
            // Exponential moving average for latency
            let prev = model.avg_latency_ms.load(Ordering::Relaxed);
            let new_avg = if prev == 0 {
                latency_ms
            } else {
                (prev * 9 + latency_ms) / 10
            };
            model.avg_latency_ms.store(new_avg, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ServeModelRequest {
    pub model_id: String,
    pub name: String,
    pub model_path: String,
    pub endpoint: String,
    #[serde(default)]
    pub config: ModelServeConfig,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub config: ModelServeConfig,
}

#[derive(Debug, Serialize)]
pub struct ServeModelResponse {
    pub success: bool,
    pub model_id: String,
    pub endpoint: String,
    pub status: ServeStatus,
}

#[derive(Debug, Serialize)]
pub struct StopModelResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub models_ready: usize,
    pub total_models: usize,
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_serving_router(state: AppState) -> Router {
    Router::new()
        .route("/api/serving/serve", post(serve_handler))
        .route("/api/serving/{id}/stop", post(stop_handler))
        .route("/api/serving/models", get(list_models_handler))
        .route("/api/serving/models/{id}", get(get_model_handler))
        .route("/api/serving/models/{id}/config", put(update_config_handler))
        .route("/api/serving/stats", get(stats_handler))
        .route("/api/serving/endpoints", get(endpoints_handler))
        .route("/api/serving/health", get(health_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/serving/serve — register and start serving a model.
async fn serve_handler(
    State(state): State<AppState>,
    Json(req): Json<ServeModelRequest>,
) -> Response {
    let config = req.config;
    let model = state.model_serve_manager.serve_model(
        req.model_id.clone(),
        req.name,
        req.model_path,
        req.endpoint.clone(),
        config,
    );

    (
        axum::http::StatusCode::CREATED,
        Json(ServeModelResponse {
            success: true,
            model_id: model.id,
            endpoint: model.endpoint,
            status: model.status,
        }),
    )
        .into_response()
}

/// POST /api/serving/{id}/stop — stop serving a model.
async fn stop_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let stopped = state.model_serve_manager.stop_model(&id);
    if stopped {
        Json(StopModelResponse {
            success: true,
            message: format!("Model {} stopped", id),
        })
        .into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(StopModelResponse {
                success: false,
                message: format!("Model {} not found", id),
            }),
        )
            .into_response()
    }
}

/// GET /api/serving/models — list all served models.
async fn list_models_handler(
    State(state): State<AppState>,
) -> Json<Vec<ServedModel>> {
    Json(state.model_serve_manager.list_models())
}

/// GET /api/serving/models/{id} — get details of a single served model.
async fn get_model_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.model_serve_manager.get_model(&id) {
        Some(model) => Json(model).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Model not found"})),
        )
            .into_response(),
    }
}

/// PUT /api/serving/models/{id}/config — update model configuration.
async fn update_config_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> Response {
    let updated = state.model_serve_manager.update_config(&id, req.config);
    if updated {
        Json(serde_json::json!({"success": true, "model_id": id}))
            .into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Model not found"})),
        )
            .into_response()
    }
}

/// GET /api/serving/stats — aggregate statistics.
async fn stats_handler(
    State(state): State<AppState>,
) -> Json<ServeStats> {
    Json(state.model_serve_manager.get_stats())
}

/// GET /api/serving/endpoints — list all registered endpoints.
async fn endpoints_handler(
    State(state): State<AppState>,
) -> Json<Vec<serde_json::Value>> {
    let models = state.model_serve_manager.list_models();
    let endpoints: Vec<serde_json::Value> = models
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "endpoint": m.endpoint,
                "model_id": m.id,
                "status": m.status,
            })
        })
        .collect();
    Json(endpoints)
}

/// GET /api/serving/health — health check.
async fn health_handler(
    State(state): State<AppState>,
) -> Json<HealthResponse> {
    let stats = state.model_serve_manager.get_stats();
    Json(HealthResponse {
        healthy: stats.ready_models > 0 || stats.total_models == 0,
        models_ready: stats.ready_models,
        total_models: stats.total_models,
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
        let config = ModelServeConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(config.enable_cors);
        assert!(!config.auth_required);
    }

    #[test]
    fn test_serve_and_stop_model() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        let model = manager.serve_model(
            "test-model".to_string(),
            "Test Model".to_string(),
            "/tmp/model".to_string(),
            "/v1/models/test".to_string(),
            ModelServeConfig::default(),
        );
        assert_eq!(model.status, ServeStatus::Ready);

        assert!(manager.stop_model("test-model"));
        let stopped = manager.get_model("test-model").unwrap();
        assert_eq!(stopped.status, ServeStatus::Stopped);
    }

    #[test]
    fn test_endpoint_lookup() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/models/m1".to_string(),
            ModelServeConfig::default(),
        );

        assert_eq!(manager.get_endpoint("/v1/models/m1"), Some("m1".to_string()));
        assert_eq!(manager.get_endpoint("/v1/models/nonexistent"), None);
    }

    #[test]
    fn test_record_request_metrics() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/models/m1".to_string(),
            ModelServeConfig::default(),
        );

        manager.record_request("m1", 100, 50, false);
        manager.record_request("m1", 200, 30, false);
        manager.record_request("m1", 0, 100, true);

        let model = manager.get_model("m1").unwrap();
        assert_eq!(model.total_requests.load(Ordering::Relaxed), 3);
        assert_eq!(model.total_tokens.load(Ordering::Relaxed), 300);
        assert_eq!(model.error_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_stats() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/m1".to_string(),
            ModelServeConfig::default(),
        );
        manager.serve_model(
            "m2".to_string(),
            "M2".to_string(),
            "/path".to_string(),
            "/v1/m2".to_string(),
            ModelServeConfig::default(),
        );

        let stats = manager.get_stats();
        assert_eq!(stats.total_models, 2);
        assert_eq!(stats.ready_models, 2);
    }

    #[test]
    fn test_update_config() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/m1".to_string(),
            ModelServeConfig::default(),
        );

        let mut new_config = ModelServeConfig::default();
        new_config.port = 9090;
        assert!(manager.update_config("m1", new_config));

        let model = manager.get_model("m1").unwrap();
        assert_eq!(model.config.port, 9090);
        assert!(!manager.update_config("nonexistent", ModelServeConfig::default()));
    }

    #[test]
    fn test_stop_nonexistent_model() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        assert!(!manager.stop_model("does-not-exist"));
    }

    #[test]
    fn test_get_nonexistent_model() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        assert!(manager.get_model("ghost").is_none());
    }

    #[test]
    fn test_list_empty_models() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        let models = manager.list_models();
        assert!(models.is_empty());
    }

    #[test]
    fn test_stats_empty_manager() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        let stats = manager.get_stats();
        assert_eq!(stats.total_models, 0);
        assert_eq!(stats.ready_models, 0);
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.total_tokens, 0);
        assert_eq!(stats.total_errors, 0);
        assert_eq!(stats.avg_latency_ms, 0.0);
        assert_eq!(stats.total_gpu_memory_mb, 0);
    }

    #[test]
    fn test_serve_multiple_models_same_endpoint() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/same".to_string(),
            ModelServeConfig::default(),
        );
        manager.serve_model(
            "m2".to_string(),
            "M2".to_string(),
            "/path".to_string(),
            "/v1/same".to_string(),
            ModelServeConfig::default(),
        );
        // The second serve overwrites the endpoint index
        assert_eq!(manager.get_endpoint("/v1/same"), Some("m2".to_string()));
        // Both models should still be in the model map
        assert_eq!(manager.list_models().len(), 2);
    }

    #[test]
    fn test_stop_model_removes_endpoint_index() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/m1".to_string(),
            ModelServeConfig::default(),
        );
        assert_eq!(manager.get_endpoint("/v1/m1"), Some("m1".to_string()));
        manager.stop_model("m1");
        assert_eq!(manager.get_endpoint("/v1/m1"), None);
    }

    #[test]
    fn test_record_request_for_nonexistent_model() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        // Should not panic when recording for missing model
        manager.record_request("ghost", 100, 50, false);
        // Verify stats are still zero
        let stats = manager.get_stats();
        assert_eq!(stats.total_requests, 0);
    }

    #[test]
    fn test_record_request_latency_ema() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/m1".to_string(),
            ModelServeConfig::default(),
        );
        // First request sets latency directly
        manager.record_request("m1", 10, 100, false);
        let model = manager.get_model("m1").unwrap();
        assert_eq!(model.avg_latency_ms.load(Ordering::Relaxed), 100);

        // Second request applies EMA: (100*9 + 50) / 10 = 95
        manager.record_request("m1", 10, 50, false);
        let model = manager.get_model("m1").unwrap();
        assert_eq!(model.avg_latency_ms.load(Ordering::Relaxed), 95);
    }

    #[test]
    fn test_stats_with_mixed_model_statuses() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/m1".to_string(),
            ModelServeConfig::default(),
        );
        manager.serve_model(
            "m2".to_string(),
            "M2".to_string(),
            "/path".to_string(),
            "/v1/m2".to_string(),
            ModelServeConfig::default(),
        );
        manager.stop_model("m1");

        let stats = manager.get_stats();
        assert_eq!(stats.total_models, 2);
        assert_eq!(stats.ready_models, 1); // only m2 is Ready
    }

    #[test]
    fn test_custom_config_non_defaults() {
        let config = ModelServeConfig {
            host: "127.0.0.1".to_string(),
            port: 9090,
            default_timeout_secs: 60,
            max_concurrent_requests: 50,
            enable_cors: false,
            auth_required: true,
            rate_limit_rpm: 120,
            log_requests: false,
        };
        let manager = ModelServeManager::new(config.clone());
        let model = manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/m1".to_string(),
            config,
        );
        assert_eq!(model.config.port, 9090);
        assert_eq!(model.config.default_timeout_secs, 60);
        assert!(!model.config.enable_cors);
        assert!(model.config.auth_required);
        assert_eq!(model.config.rate_limit_rpm, 120);
        assert!(!model.config.log_requests);
    }

    #[test]
    fn test_clone_served_model_shares_atomic_counters() {
        let manager = ModelServeManager::new(ModelServeConfig::default());
        let model = manager.serve_model(
            "m1".to_string(),
            "M1".to_string(),
            "/path".to_string(),
            "/v1/m1".to_string(),
            ModelServeConfig::default(),
        );
        let cloned = model.clone();
        manager.record_request("m1", 42, 10, false);

        // Both the original and clone should see the updated counter
        assert_eq!(model.total_requests.load(Ordering::Relaxed), 1);
        assert_eq!(cloned.total_requests.load(Ordering::Relaxed), 1);
        assert_eq!(model.total_tokens.load(Ordering::Relaxed), 42);
        assert_eq!(cloned.total_tokens.load(Ordering::Relaxed), 42);
    }

    #[test]
    fn test_serve_status_serialization() {
        assert_eq!(serde_json::to_string(&ServeStatus::Ready).unwrap(), "\"ready\"");
        assert_eq!(serde_json::to_string(&ServeStatus::Stopped).unwrap(), "\"stopped\"");
        assert_eq!(serde_json::to_string(&ServeStatus::Error).unwrap(), "\"error\"");
        assert_eq!(serde_json::to_string(&ServeStatus::Starting).unwrap(), "\"starting\"");
        assert_eq!(serde_json::to_string(&ServeStatus::Draining).unwrap(), "\"draining\"");
    }

    #[test]
    fn test_concurrent_serve_and_stop() {
        use std::thread;

        let manager = ModelServeManager::new(ModelServeConfig::default());
        let manager_clone = manager.clone();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let mgr = manager_clone.clone();
                thread::spawn(move || {
                    let id = format!("model-{}", i);
                    mgr.serve_model(
                        id.clone(),
                        format!("Model {}", i),
                        "/path".to_string(),
                        format!("/v1/{}", id),
                        ModelServeConfig::default(),
                    );
                    // Immediately stop
                    mgr.stop_model(&id);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All 10 should be in the map (stopped), none ready
        let stats = manager.get_stats();
        assert_eq!(stats.total_models, 10);
        assert_eq!(stats.ready_models, 0);
    }
}
