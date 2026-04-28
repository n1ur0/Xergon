//! A/B Testing v2 — Multivariate testing with canary deployments.
//!
//! Extends the basic A/B testing with:
//! - Multiple variants (not just A/B)
//! - Configurable traffic splitting
//! - Statistical significance evaluation
//! - Canary deployment support
//! - Leader promotion and rollback
//!
//! API:
//! - POST /api/abv2/tests              -- create a test
//! - POST /api/abv2/tests/{id}/start   -- start a test
//! - POST /api/abv2/tests/{id}/pause   -- pause a test
//! - POST /api/abv2/tests/{id}/stop    -- stop a test
//! - GET  /api/abv2/tests              -- list tests
//! - GET  /api/abv2/tests/{id}         -- get test details
//! - POST /api/abv2/tests/{id}/evaluate -- evaluate results
//! - POST /api/abv2/tests/{id}/promote -- promote leader variant
//! - POST /api/abv2/tests/{id}/rollback -- rollback to control

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use crate::api::AppState;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestV2Config {
    pub name: String,
    pub variants: Vec<VariantConfig>,
    pub traffic_split: Vec<f64>,
    pub metric: TestMetric,
    #[serde(default = "default_confidence")]
    pub confidence_level: f64,
    #[serde(default = "default_sample_size")]
    pub min_sample_size: usize,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,
    #[serde(default)]
    pub enable_canary: bool,
    #[serde(default = "default_canary_pct")]
    pub canary_percentage: f64,
}

fn default_confidence() -> f64 { 0.95 }
fn default_sample_size() -> usize { 1000 }
fn default_max_duration() -> u64 { 604800 } // 7 days
fn default_canary_pct() -> f64 { 0.05 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantConfig {
    pub id: String,
    pub name: String,
    pub model_id: String,
    pub weight: f64,
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TestMetric {
    LatencyP50,
    LatencyP95,
    LatencyP99,
    ErrorRate,
    Throughput,
    UserSatisfaction,
    TokenEfficiency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Draft,
    Running,
    Paused,
    Completed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantStats {
    pub requests: u64,
    pub errors: u64,
    pub total_latency_ms: u64,
    pub p50_latency: f64,
    pub p95_latency: f64,
    pub p99_latency: f64,
    pub tokens_generated: u64,
    pub avg_score: f64,
    #[serde(skip)]
    latency_samples: Vec<u64>,
}

impl Default for VariantStats {
    fn default() -> Self {
        Self {
            requests: 0,
            errors: 0,
            total_latency_ms: 0,
            p50_latency: 0.0,
            p95_latency: 0.0,
            p99_latency: 0.0,
            tokens_generated: 0,
            avg_score: 0.0,
            latency_samples: Vec::new(),
        }
    }
}

impl VariantStats {
    fn compute_percentiles(&mut self) {
        if self.latency_samples.is_empty() {
            return;
        }
        let mut sorted = self.latency_samples.clone();
        sorted.sort_unstable();
        let len = sorted.len();
        let p50_idx = (len * 50 / 100).min(len - 1);
        let p95_idx = (len * 95 / 100).min(len - 1);
        let p99_idx = (len * 99 / 100).min(len - 1);
        self.p50_latency = sorted[p50_idx] as f64;
        self.p95_latency = sorted[p95_idx] as f64;
        self.p99_latency = sorted[p99_idx] as f64;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestV2 {
    pub id: String,
    pub config: ABTestV2Config,
    pub status: TestStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub variant_stats: HashMap<String, VariantStats>,
    pub current_leader: Option<String>,
    pub statistical_significance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEvaluation {
    pub test_id: String,
    pub leader: String,
    pub statistical_significance: f64,
    pub is_significant: bool,
    pub variant_results: HashMap<String, VariantResult>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantResult {
    pub variant_id: String,
    pub requests: u64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub tokens_generated: u64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryDeployment {
    pub id: String,
    pub control_model_id: String,
    pub canary_model_id: String,
    pub percentage: f64,
    pub status: TestStatus,
    pub started_at: DateTime<Utc>,
    pub total_control_requests: u64,
    pub total_canary_requests: u64,
    pub control_errors: u64,
    pub canary_errors: u64,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ABTestV2Manager {
    tests: Arc<DashMap<String, ABTestV2>>,
    canaries: Arc<DashMap<String, CanaryDeployment>>,
    test_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl ABTestV2Manager {
    pub fn new() -> Self {
        Self {
            tests: Arc::new(DashMap::new()),
            canaries: Arc::new(DashMap::new()),
            test_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Create a new A/B test.
    pub fn create_test(&self, config: ABTestV2Config) -> Result<ABTestV2, String> {
        // Validate traffic split
        let split_sum: f64 = config.traffic_split.iter().sum();
        if (split_sum - 1.0).abs() > 0.01 {
            return Err(format!(
                "Traffic split must sum to 1.0, got {}",
                split_sum
            ));
        }
        if config.variants.is_empty() {
            return Err("At least one variant required".to_string());
        }
        if config.variants.len() != config.traffic_split.len() {
            return Err("Variants and traffic_split must have same length".to_string());
        }

        let id = format!("abv2-{}", self.test_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let variant_stats: HashMap<String, VariantStats> = config
            .variants
            .iter()
            .map(|v| (v.id.clone(), VariantStats::default()))
            .collect();

        let test = ABTestV2 {
            id: id.clone(),
            config,
            status: TestStatus::Draft,
            started_at: None,
            ended_at: None,
            variant_stats,
            current_leader: None,
            statistical_significance: None,
        };

        self.tests.insert(id.clone(), test.clone());
        info!(test_id = %id, "A/B test v2 created");
        Ok(test)
    }

    /// Start a test.
    pub fn start_test(&self, test_id: &str) -> bool {
        if let Some(mut test) = self.tests.get_mut(test_id) {
            if test.status != TestStatus::Draft && test.status != TestStatus::Paused {
                return false;
            }
            test.status = TestStatus::Running;
            if test.started_at.is_none() {
                test.started_at = Some(Utc::now());
            }
            info!(test_id = %test_id, "A/B test v2 started");
            true
        } else {
            false
        }
    }

    /// Pause a running test.
    pub fn pause_test(&self, test_id: &str) -> bool {
        if let Some(mut test) = self.tests.get_mut(test_id) {
            if test.status != TestStatus::Running {
                return false;
            }
            test.status = TestStatus::Paused;
            info!(test_id = %test_id, "A/B test v2 paused");
            true
        } else {
            false
        }
    }

    /// Stop a test.
    pub fn stop_test(&self, test_id: &str) -> bool {
        if let Some(mut test) = self.tests.get_mut(test_id) {
            if test.status == TestStatus::Completed || test.status == TestStatus::RolledBack {
                return false;
            }
            test.status = TestStatus::Completed;
            test.ended_at = Some(Utc::now());
            info!(test_id = %test_id, "A/B test v2 stopped");
            true
        } else {
            false
        }
    }

    /// Get a test by ID.
    pub fn get_test(&self, test_id: &str) -> Option<ABTestV2> {
        self.tests.get(test_id).map(|t| t.clone())
    }

    /// List all tests.
    pub fn list_tests(&self) -> Vec<ABTestV2> {
        self.tests.iter().map(|t| t.clone()).collect()
    }

    /// Route a request to a variant based on traffic split.
    pub fn route_request(&self, test_id: &str) -> Option<String> {
        let test = self.tests.get(test_id)?;
        if test.status != TestStatus::Running {
            return None;
        }

        let mut rng = rand::thread_rng();
        let r: f64 = rng.gen();
        let mut cumulative = 0.0;

        for (i, weight) in test.config.traffic_split.iter().enumerate() {
            cumulative += weight;
            if r <= cumulative {
                return Some(test.config.variants[i].id.clone());
            }
        }

        // Fallback to last variant
        test.config.variants.last().map(|v| v.id.clone())
    }

    /// Record a metric for a specific variant.
    pub fn record_metric(
        &self,
        test_id: &str,
        variant_id: &str,
        latency_ms: u64,
        tokens: u64,
        is_error: bool,
        score: f64,
    ) {
        if let Some(mut test) = self.tests.get_mut(test_id) {
            if let Some(stats) = test.variant_stats.get_mut(variant_id) {
                stats.requests += 1;
                stats.total_latency_ms += latency_ms;
                stats.tokens_generated += tokens;
                if is_error {
                    stats.errors += 1;
                }
                // Update average score
                let n = stats.requests as f64;
                stats.avg_score = (stats.avg_score * (n - 1.0) + score) / n;
                stats.latency_samples.push(latency_ms);
                // Keep last 10000 samples for percentile computation
                if stats.latency_samples.len() > 10000 {
                    stats.latency_samples.drain(..1000);
                }
                stats.compute_percentiles();
            }
        }
    }

    /// Evaluate test results and determine a leader.
    pub fn evaluate_results(&self, test_id: &str) -> Result<TestEvaluation, String> {
        let test = self.tests.get(test_id).ok_or("Test not found")?;

        let mut best_variant = String::new();
        let mut best_score = f64::NEG_INFINITY;
        let mut results = HashMap::new();

        for (variant_id, stats) in &test.variant_stats {
            let error_rate = if stats.requests > 0 {
                stats.errors as f64 / stats.requests as f64
            } else {
                1.0
            };
            let avg_latency = if stats.requests > 0 {
                stats.total_latency_ms as f64 / stats.requests as f64
            } else {
                f64::MAX
            };

            // Compute score based on the configured metric
            let score = match test.config.metric {
                TestMetric::LatencyP50 | TestMetric::LatencyP95 | TestMetric::LatencyP99 => {
                    -avg_latency // lower latency is better
                }
                TestMetric::ErrorRate => -error_rate,
                TestMetric::Throughput => stats.requests as f64,
                TestMetric::UserSatisfaction => stats.avg_score,
                TestMetric::TokenEfficiency => {
                    if stats.tokens_generated > 0 {
                        stats.requests as f64 / stats.tokens_generated as f64
                    } else {
                        0.0
                    }
                }
            };

            if score > best_score {
                best_score = score;
                best_variant = variant_id.clone();
            }

            results.insert(
                variant_id.clone(),
                VariantResult {
                    variant_id: variant_id.clone(),
                    requests: stats.requests,
                    error_rate,
                    avg_latency_ms: avg_latency,
                    p95_latency_ms: stats.p95_latency,
                    tokens_generated: stats.tokens_generated,
                    score,
                },
            );
        }

        // Compute statistical significance (simplified z-test approximation)
        let significance = self.compute_significance(&test.variant_stats);
        let is_significant = significance.unwrap_or(0.0) >= test.config.confidence_level;

        // Update test
        if let Some(mut t) = self.tests.get_mut(test_id) {
            t.current_leader = Some(best_variant.clone());
            t.statistical_significance = significance;
        }

        let recommendation = if is_significant {
            format!("Variant {} is statistically significant leader", best_variant)
        } else {
            "Not yet statistically significant — continue testing".to_string()
        };

        Ok(TestEvaluation {
            test_id: test_id.to_string(),
            leader: best_variant,
            statistical_significance: significance.unwrap_or(0.0),
            is_significant,
            variant_results: results,
            recommendation,
        })
    }

    /// Simplified statistical significance using a two-proportion z-test approximation.
    fn compute_significance(
        &self,
        variant_stats: &HashMap<String, VariantStats>,
    ) -> Option<f64> {
        let variants: Vec<_> = variant_stats.iter().collect();
        if variants.len() < 2 {
            return None;
        }

        // Compare top two variants by requests
        let mut sorted = variants.clone();
        sorted.sort_by(|a, b| b.1.requests.cmp(&a.1.requests));
        let (a_stats, b_stats) = (&sorted[0].1, &sorted[1].1);

        let min_samples = a_stats.requests.min(b_stats.requests) as f64;
        if min_samples < 100.0 {
            return Some(0.0);
        }

        let p1 = a_stats.avg_score;
        let p2 = b_stats.avg_score;
        let n1 = a_stats.requests as f64;
        let n2 = b_stats.requests as f64;

        let pooled = (p1 * n1 + p2 * n2) / (n1 + n2);
        if pooled == 0.0 {
            return Some(0.0);
        }

        let se = ((pooled * (1.0 - pooled)) * (1.0 / n1 + 1.0 / n2)).sqrt();
        if se == 0.0 {
            return Some(1.0);
        }

        let z = (p1 - p2).abs() / se;
        // Convert z-score to confidence (simplified)
        let confidence = 1.0 - 2.0 * (-z * std::f64::consts::FRAC_1_SQRT_2).exp(); // approximation
        Some(confidence.min(1.0))
    }

    /// Promote the current leader variant.
    pub fn promote_leader(&self, test_id: &str) -> Result<String, String> {
        let evaluation = self.evaluate_results(test_id)?;
        self.stop_test(test_id);
        info!(test_id = %test_id, leader = %evaluation.leader, "Leader promoted");
        Ok(evaluation.leader)
    }

    /// Rollback the test to the control (first variant).
    pub fn rollback(&self, test_id: &str) -> Result<String, String> {
        if let Some(mut test) = self.tests.get_mut(test_id) {
            let control = test
                .config
                .variants
                .first()
                .map(|v| v.id.clone())
                .ok_or("No variants")?;
            test.status = TestStatus::RolledBack;
            test.ended_at = Some(Utc::now());
            test.current_leader = Some(control.clone());
            info!(test_id = %test_id, control = %control, "Test rolled back");
            Ok(control)
        } else {
            Err("Test not found".to_string())
        }
    }

    /// Create a canary deployment.
    pub fn create_canary(
        &self,
        control_model_id: String,
        canary_model_id: String,
        percentage: f64,
    ) -> CanaryDeployment {
        let id = format!("canary-{}", self.test_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let canary = CanaryDeployment {
            id: id.clone(),
            control_model_id,
            canary_model_id,
            percentage,
            status: TestStatus::Running,
            started_at: Utc::now(),
            total_control_requests: 0,
            total_canary_requests: 0,
            control_errors: 0,
            canary_errors: 0,
        };
        self.canaries.insert(id.clone(), canary.clone());
        info!(canary_id = %id, "Canary deployment created");
        canary
    }

    /// List canary deployments.
    pub fn list_canaries(&self) -> Vec<CanaryDeployment> {
        self.canaries.iter().map(|c| c.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// Request / Response types for API
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTestRequest {
    pub name: String,
    pub variants: Vec<VariantConfig>,
    pub traffic_split: Vec<f64>,
    #[serde(default = "default_test_metric")]
    pub metric: TestMetric,
    pub confidence_level: Option<f64>,
    pub min_sample_size: Option<usize>,
    pub max_duration_secs: Option<u64>,
    pub enable_canary: Option<bool>,
    pub canary_percentage: Option<f64>,
}

fn default_test_metric() -> TestMetric { TestMetric::LatencyP95 }

#[derive(Debug, Deserialize)]
pub struct RecordMetricRequest {
    pub variant_id: String,
    pub latency_ms: u64,
    pub tokens: u64,
    pub is_error: bool,
    pub score: f64,
}

#[derive(Debug, Serialize)]
pub struct CreateTestResponse {
    pub success: bool,
    pub test_id: String,
    pub status: TestStatus,
}

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub success: bool,
    pub evaluation: TestEvaluation,
}

#[derive(Debug, Serialize)]
pub struct PromoteResponse {
    pub success: bool,
    pub leader: String,
}

#[derive(Debug, Serialize)]
pub struct RollbackResponse {
    pub success: bool,
    pub control: String,
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_abv2_router(state: AppState) -> Router {
    Router::new()
        .route("/api/abv2/tests", post(create_test_handler).get(list_tests_handler))
        .route("/api/abv2/tests/{id}", get(get_test_handler))
        .route("/api/abv2/tests/{id}/start", post(start_test_handler))
        .route("/api/abv2/tests/{id}/pause", post(pause_test_handler))
        .route("/api/abv2/tests/{id}/stop", post(stop_test_handler))
        .route("/api/abv2/tests/{id}/evaluate", post(evaluate_test_handler))
        .route("/api/abv2/tests/{id}/promote", post(promote_test_handler))
        .route("/api/abv2/tests/{id}/rollback", post(rollback_test_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/abv2/tests — create a new A/B test.
async fn create_test_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateTestRequest>,
) -> Response {
    let config = ABTestV2Config {
        name: req.name,
        variants: req.variants,
        traffic_split: req.traffic_split,
        metric: req.metric,
        confidence_level: req.confidence_level.unwrap_or(default_confidence()),
        min_sample_size: req.min_sample_size.unwrap_or(default_sample_size()),
        max_duration_secs: req.max_duration_secs.unwrap_or(default_max_duration()),
        enable_canary: req.enable_canary.unwrap_or(false),
        canary_percentage: req.canary_percentage.unwrap_or(default_canary_pct()),
    };

    match state.ab_testing_v2.create_test(config) {
        Ok(test) => (
            axum::http::StatusCode::CREATED,
            Json(CreateTestResponse {
                success: true,
                test_id: test.id,
                status: test.status,
            }),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// GET /api/abv2/tests — list all tests.
async fn list_tests_handler(
    State(state): State<AppState>,
) -> Json<Vec<ABTestV2>> {
    Json(state.ab_testing_v2.list_tests())
}

/// GET /api/abv2/tests/{id} — get test details.
async fn get_test_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing_v2.get_test(&id) {
        Some(test) => Json(test).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Test not found"})),
        )
            .into_response(),
    }
}

/// POST /api/abv2/tests/{id}/start — start a test.
async fn start_test_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let started = state.ab_testing_v2.start_test(&id);
    Json(ActionResponse {
        success: started,
        message: if started {
            format!("Test {} started", id)
        } else {
            format!("Test {} could not be started (not in Draft/Paused state)", id)
        },
    })
    .into_response()
}

/// POST /api/abv2/tests/{id}/pause — pause a running test.
async fn pause_test_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let paused = state.ab_testing_v2.pause_test(&id);
    Json(ActionResponse {
        success: paused,
        message: if paused {
            format!("Test {} paused", id)
        } else {
            format!("Test {} could not be paused", id)
        },
    })
    .into_response()
}

/// POST /api/abv2/tests/{id}/stop — stop a test.
async fn stop_test_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let stopped = state.ab_testing_v2.stop_test(&id);
    Json(ActionResponse {
        success: stopped,
        message: if stopped {
            format!("Test {} stopped", id)
        } else {
            format!("Test {} could not be stopped", id)
        },
    })
    .into_response()
}

/// POST /api/abv2/tests/{id}/evaluate — evaluate test results.
async fn evaluate_test_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing_v2.evaluate_results(&id) {
        Ok(evaluation) => Json(EvaluateResponse { success: true, evaluation }).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// POST /api/abv2/tests/{id}/promote — promote the leader variant.
async fn promote_test_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing_v2.promote_leader(&id) {
        Ok(leader) => (
            axum::http::StatusCode::OK,
            Json(PromoteResponse { success: true, leader }),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// POST /api/abv2/tests/{id}/rollback — rollback to control variant.
async fn rollback_test_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing_v2.rollback(&id) {
        Ok(control) => (
            axum::http::StatusCode::OK,
            Json(RollbackResponse { success: true, control }),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_config() -> ABTestV2Config {
        ABTestV2Config {
            name: "Test Experiment".to_string(),
            variants: vec![
                VariantConfig {
                    id: "control".to_string(),
                    name: "Control".to_string(),
                    model_id: "model-a".to_string(),
                    weight: 0.5,
                    parameters: HashMap::new(),
                },
                VariantConfig {
                    id: "treatment".to_string(),
                    name: "Treatment".to_string(),
                    model_id: "model-b".to_string(),
                    weight: 0.5,
                    parameters: HashMap::new(),
                },
            ],
            traffic_split: vec![0.5, 0.5],
            metric: TestMetric::LatencyP95,
            confidence_level: 0.95,
            min_sample_size: 100,
            max_duration_secs: 3600,
            enable_canary: false,
            canary_percentage: 0.05,
        }
    }

    #[test]
    fn test_create_test() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        assert_eq!(test.status, TestStatus::Draft);
        assert!(test.id.starts_with("abv2-"));
    }

    #[test]
    fn test_create_test_invalid_split() {
        let manager = ABTestV2Manager::new();
        let mut config = make_test_config();
        config.traffic_split = vec![0.3, 0.3]; // sums to 0.6
        assert!(manager.create_test(config).is_err());
    }

    #[test]
    fn test_start_pause_stop() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();

        assert!(manager.start_test(&test.id));
        let t = manager.get_test(&test.id).unwrap();
        assert_eq!(t.status, TestStatus::Running);

        assert!(manager.pause_test(&test.id));
        let t = manager.get_test(&test.id).unwrap();
        assert_eq!(t.status, TestStatus::Paused);

        assert!(manager.start_test(&test.id)); // resume
        assert!(manager.stop_test(&test.id));
        let t = manager.get_test(&test.id).unwrap();
        assert_eq!(t.status, TestStatus::Completed);
    }

    #[test]
    fn test_route_request() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);

        // Route many requests and check both variants get traffic
        let mut variant_counts: HashMap<String, usize> = HashMap::new();
        for _ in 0..1000 {
            if let Some(variant) = manager.route_request(&test.id) {
                *variant_counts.entry(variant).or_insert(0) += 1;
            }
        }

        assert!(variant_counts.contains_key("control"));
        assert!(variant_counts.contains_key("treatment"));
        // Both should get reasonable traffic (between 30% and 70% for 1000 samples)
        let ctrl_pct = *variant_counts.get("control").unwrap() as f64 / 1000.0;
        assert!(ctrl_pct > 0.3 && ctrl_pct < 0.7);
    }

    #[test]
    fn test_record_metrics_and_evaluate() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);

        // Record metrics for control (higher latency)
        for _ in 0..200 {
            manager.record_metric(&test.id, "control", 100, 50, false, 0.7);
        }
        // Record metrics for treatment (lower latency = better for LatencyP95)
        for _ in 0..200 {
            manager.record_metric(&test.id, "treatment", 50, 80, false, 0.9);
        }

        let eval = manager.evaluate_results(&test.id).unwrap();
        assert_eq!(eval.leader, "treatment"); // lower latency wins
    }

    #[test]
    fn test_promote_leader() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);

        for _ in 0..200 {
            manager.record_metric(&test.id, "control", 100, 50, false, 0.7);
            manager.record_metric(&test.id, "treatment", 50, 80, false, 0.9);
        }

        let leader = manager.promote_leader(&test.id).unwrap();
        assert_eq!(leader, "treatment");

        let t = manager.get_test(&test.id).unwrap();
        assert_eq!(t.status, TestStatus::Completed);
    }

    #[test]
    fn test_rollback() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);

        let control = manager.rollback(&test.id).unwrap();
        assert_eq!(control, "control");

        let t = manager.get_test(&test.id).unwrap();
        assert_eq!(t.status, TestStatus::RolledBack);
    }

    #[test]
    fn test_canary_deployment() {
        let manager = ABTestV2Manager::new();
        let canary = manager.create_canary("model-a".into(), "model-b".into(), 0.05);
        assert!(canary.id.starts_with("canary-"));
        assert_eq!(canary.percentage, 0.05);

        let canaries = manager.list_canaries();
        assert_eq!(canaries.len(), 1);
    }

    #[test]
    fn test_list_tests() {
        let manager = ABTestV2Manager::new();
        manager.create_test(make_test_config()).unwrap();
        manager.create_test(make_test_config()).unwrap();
        assert_eq!(manager.list_tests().len(), 2);
    }

    #[test]
    fn test_create_test_empty_variants() {
        let manager = ABTestV2Manager::new();
        let mut config = make_test_config();
        config.variants = vec![];
        config.traffic_split = vec![];
        assert!(manager.create_test(config).is_err());
    }

    #[test]
    fn test_create_test_variant_split_mismatch() {
        let manager = ABTestV2Manager::new();
        let mut config = make_test_config();
        config.traffic_split = vec![0.5, 0.3, 0.2]; // 3 splits, 2 variants
        assert!(manager.create_test(config).is_err());
    }

    #[test]
    fn test_start_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        assert!(!manager.start_test("does-not-exist"));
    }

    #[test]
    fn test_pause_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        assert!(!manager.pause_test("does-not-exist"));
    }

    #[test]
    fn test_stop_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        assert!(!manager.stop_test("does-not-exist"));
    }

    #[test]
    fn test_get_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        assert!(manager.get_test("ghost").is_none());
    }

    #[test]
    fn test_stop_already_completed_test() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);
        assert!(manager.stop_test(&test.id));
        // Cannot stop again
        assert!(!manager.stop_test(&test.id));
    }

    #[test]
    fn test_stop_already_rolledback_test() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);
        assert!(manager.rollback(&test.id).is_ok());
        // Cannot stop a rolled-back test
        assert!(!manager.stop_test(&test.id));
    }

    #[test]
    fn test_start_completed_test_fails() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);
        manager.stop_test(&test.id);
        // Cannot start a completed test
        assert!(!manager.start_test(&test.id));
    }

    #[test]
    fn test_pause_draft_test_fails() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        // Cannot pause a draft test (only running)
        assert!(!manager.pause_test(&test.id));
    }

    #[test]
    fn test_route_request_draft_test() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        // Cannot route requests to a draft test
        assert!(manager.route_request(&test.id).is_none());
    }

    #[test]
    fn test_route_request_paused_test() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);
        manager.pause_test(&test.id);
        // Cannot route requests to a paused test
        assert!(manager.route_request(&test.id).is_none());
    }

    #[test]
    fn test_evaluate_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        assert!(manager.evaluate_results("ghost").is_err());
    }

    #[test]
    fn test_promote_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        assert!(manager.promote_leader("ghost").is_err());
    }

    #[test]
    fn test_rollback_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        assert!(manager.rollback("ghost").is_err());
    }

    #[test]
    fn test_record_metric_nonexistent_test() {
        let manager = ABTestV2Manager::new();
        // Should not panic when recording for missing test/variant
        manager.record_metric("ghost", "variant", 100, 50, false, 0.8);
    }

    #[test]
    fn test_record_metric_nonexistent_variant() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);
        // Should not panic when recording for missing variant
        manager.record_metric(&test.id, "ghost_variant", 100, 50, false, 0.8);
    }

    #[test]
    fn test_three_variant_test() {
        let manager = ABTestV2Manager::new();
        let config = ABTestV2Config {
            name: "Three Way Test".to_string(),
            variants: vec![
                VariantConfig {
                    id: "v1".to_string(),
                    name: "V1".to_string(),
                    model_id: "model-a".to_string(),
                    weight: 0.33,
                    parameters: HashMap::new(),
                },
                VariantConfig {
                    id: "v2".to_string(),
                    name: "V2".to_string(),
                    model_id: "model-b".to_string(),
                    weight: 0.33,
                    parameters: HashMap::new(),
                },
                VariantConfig {
                    id: "v3".to_string(),
                    name: "V3".to_string(),
                    model_id: "model-c".to_string(),
                    weight: 0.34,
                    parameters: HashMap::new(),
                },
            ],
            traffic_split: vec![0.33, 0.33, 0.34],
            metric: TestMetric::Throughput,
            confidence_level: 0.95,
            min_sample_size: 100,
            max_duration_secs: 3600,
            enable_canary: false,
            canary_percentage: 0.05,
        };

        let test = manager.create_test(config).unwrap();
        assert_eq!(test.variant_stats.len(), 3);
        manager.start_test(&test.id);

        // Record metrics for all three
        for _ in 0..100 {
            manager.record_metric(&test.id, "v1", 100, 50, false, 0.7);
            manager.record_metric(&test.id, "v2", 80, 60, false, 0.8);
            manager.record_metric(&test.id, "v3", 60, 70, false, 0.9);
        }

        let eval = manager.evaluate_results(&test.id).unwrap();
        // Throughput metric: more requests = better. All have 100 requests, so tie-break
        // by other factors. Check all variant_results are present
        assert!(eval.variant_results.contains_key("v1"));
        assert!(eval.variant_results.contains_key("v2"));
        assert!(eval.variant_results.contains_key("v3"));
    }

    #[test]
    fn test_evaluate_with_error_rates() {
        let manager = ABTestV2Manager::new();
        let mut config = make_test_config();
        config.metric = TestMetric::ErrorRate;
        let test = manager.create_test(config).unwrap();
        manager.start_test(&test.id);

        // Control has high error rate, treatment has low
        for _ in 0..200 {
            manager.record_metric(&test.id, "control", 50, 10, true, 0.5); // error
            manager.record_metric(&test.id, "treatment", 50, 10, false, 0.9); // no error
        }

        let eval = manager.evaluate_results(&test.id).unwrap();
        // Treatment should win (lower error rate)
        assert_eq!(eval.leader, "treatment");
        let control_result = &eval.variant_results["control"];
        assert!(control_result.error_rate > 0.9); // all errors
        let treatment_result = &eval.variant_results["treatment"];
        assert!(treatment_result.error_rate < 0.01); // no errors
    }

    #[test]
    fn test_latency_percentiles_computed() {
        let manager = ABTestV2Manager::new();
        let test = manager.create_test(make_test_config()).unwrap();
        manager.start_test(&test.id);

        // Record 200 metrics with varying latencies
        for i in 0..200 {
            let latency = (i % 100) as u64; // 0 to 99
            manager.record_metric(&test.id, "control", latency, 10, false, 0.7);
        }

        let t = manager.get_test(&test.id).unwrap();
        let stats = &t.variant_stats["control"];
        assert_eq!(stats.requests, 200);
        assert!(stats.p50_latency > 0.0);
        assert!(stats.p95_latency >= stats.p50_latency);
        assert!(stats.p99_latency >= stats.p95_latency);
    }

    #[test]
    fn test_canary_list_empty() {
        let manager = ABTestV2Manager::new();
        assert!(manager.list_canaries().is_empty());
    }

    #[test]
    fn test_multiple_canary_deployments() {
        let manager = ABTestV2Manager::new();
        manager.create_canary("model-a".into(), "model-b".into(), 0.05);
        manager.create_canary("model-a".into(), "model-c".into(), 0.10);
        assert_eq!(manager.list_canaries().len(), 2);
    }
}
