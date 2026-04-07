//! A/B and multivariate experiment framework.
//!
//! Provides:
//! - Experiment lifecycle management (draft, running, paused, completed, archived)
//! - Traffic splitting by variant weight
//! - Sticky variant assignment (consistent hashing per user)
//! - Metric recording and aggregation
//! - Statistical significance calculation
//! - REST API endpoints for experiment management

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The status of an experiment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    /// Experiment is being drafted.
    Draft,
    /// Experiment is actively running and assigning variants.
    Running,
    /// Experiment is temporarily paused.
    Paused,
    /// Experiment has finished.
    Completed,
    /// Experiment has been archived.
    Archived,
}

/// A single variant in an experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    /// Unique variant identifier.
    pub id: String,
    /// Human-readable variant name.
    pub name: String,
    /// Traffic weight (0.0-1.0). Weights across all variants should sum to 1.0.
    pub weight: f64,
    /// Whether this is the control group.
    pub is_control: bool,
    /// Configuration for this variant (JSON object or key-value pairs).
    pub config: HashMap<String, serde_json::Value>,
    /// Collected metrics for this variant.
    pub metrics: HashMap<String, f64>,
}

/// A metric tracked for an experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentMetric {
    /// Metric name (e.g., "conversion_rate", "revenue_per_user").
    pub name: String,
    /// The target value we're optimizing for.
    pub target_value: f64,
    /// The current observed value.
    pub current_value: f64,
    /// Statistical significance (0.0-1.0, 1.0 = 100% significant).
    pub statistical_significance: f64,
    /// The winning variant ID, if any.
    pub winner: Option<String>,
}

/// An A/B or multivariate experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    /// Unique experiment identifier.
    pub id: String,
    /// Human-readable experiment name.
    pub name: String,
    /// Description of the experiment.
    pub description: String,
    /// Current status.
    pub status: ExperimentStatus,
    /// The variants in this experiment.
    pub variants: Vec<Variant>,
    /// Fraction of traffic allocated to this experiment (0.0-1.0).
    pub traffic_allocation: f64,
    /// Start time (Unix epoch seconds).
    pub start_time: Option<u64>,
    /// End time (Unix epoch seconds).
    pub end_time: Option<u64>,
    /// Metrics being tracked.
    pub metrics: Vec<ExperimentMetric>,
    /// Total number of users assigned.
    pub total_assignments: u64,
    /// Creation timestamp.
    pub created_at: u64,
    /// Last update timestamp.
    pub updated_at: u64,
}

/// Result of assigning a user to a variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentResult {
    /// The experiment ID.
    pub experiment_id: String,
    /// The variant the user was assigned to.
    pub variant_id: String,
    /// The variant name.
    pub variant_name: String,
    /// Whether this was a new assignment or cached.
    pub is_cached: bool,
}

/// Aggregated results for an experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResults {
    /// The experiment ID.
    pub experiment_id: String,
    /// The experiment name.
    pub experiment_name: String,
    /// Current status.
    pub status: ExperimentStatus,
    /// Per-variant metrics.
    pub variant_results: Vec<VariantResult>,
    /// Overall metrics.
    pub metrics: Vec<ExperimentMetric>,
    /// Total assignments.
    pub total_assignments: u64,
}

/// Results for a single variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantResult {
    /// Variant ID.
    pub variant_id: String,
    /// Variant name.
    pub variant_name: String,
    /// Whether this is the control.
    pub is_control: bool,
    /// Number of users assigned to this variant.
    pub assignments: u64,
    /// Collected metrics.
    pub metrics: HashMap<String, f64>,
}

/// Statistics about the experiment framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentStats {
    /// Total experiments.
    pub total_experiments: usize,
    /// Experiments by status.
    pub by_status: HashMap<String, usize>,
    /// Total variant assignments across all experiments.
    pub total_assignments: u64,
    /// Cache size (sticky session entries).
    pub cache_size: usize,
}

// ---------------------------------------------------------------------------
// ExperimentFramework
// ---------------------------------------------------------------------------

/// DashMap-backed experiment framework supporting CRUD, assignment, and metrics.
#[derive(Debug, Clone)]
pub struct ExperimentFramework {
    /// Experiments keyed by ID.
    experiments: Arc<DashMap<String, Experiment>>,
    /// Sticky assignment cache: (experiment_id, user_id) -> variant_id.
    assignment_cache: Arc<DashMap<(String, String), String>>,
    /// Counter for generating experiment IDs.
    experiment_counter: Arc<AtomicU64>,
}

impl ExperimentFramework {
    /// Create a new experiment framework.
    pub fn new() -> Self {
        Self {
            experiments: Arc::new(DashMap::new()),
            assignment_cache: Arc::new(DashMap::new()),
            experiment_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    fn now_epoch() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn next_experiment_id(&self) -> String {
        format!("exp-{}", self.experiment_counter.fetch_add(1, Ordering::SeqCst))
    }

    /// Simple deterministic hash for consistent user-to-variant assignment.
    fn hash_pair(experiment_id: &str, user_id: &str) -> u64 {
        let mut hash: u64 = 5381;
        for byte in format!("{}:{}", experiment_id, user_id).bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        hash
    }

    /// Create a new experiment.
    pub fn create_experiment(
        &self,
        name: String,
        description: String,
        variants: Vec<Variant>,
        traffic_allocation: f64,
        metrics: Vec<String>,
    ) -> Result<Experiment, String> {
        if variants.is_empty() {
            return Err("At least one variant is required".to_string());
        }

        let total_weight: f64 = variants.iter().map(|v| v.weight).sum();
        if (total_weight - 1.0).abs() > 0.01 {
            return Err(format!(
                "Variant weights must sum to 1.0, got {}",
                total_weight
            ));
        }

        let has_control = variants.iter().any(|v| v.is_control);
        if !has_control {
            return Err("At least one variant must be marked as control".to_string());
        }

        let id = self.next_experiment_id();
        let now = Self::now_epoch();

        let experiment_metrics: Vec<ExperimentMetric> = metrics
            .into_iter()
            .map(|name| ExperimentMetric {
                name,
                target_value: 0.0,
                current_value: 0.0,
                statistical_significance: 0.0,
                winner: None,
            })
            .collect();

        let experiment = Experiment {
            id: id.clone(),
            name,
            description,
            status: ExperimentStatus::Draft,
            variants,
            traffic_allocation: traffic_allocation.clamp(0.0, 1.0),
            start_time: None,
            end_time: None,
            metrics: experiment_metrics,
            total_assignments: 0,
            created_at: now,
            updated_at: now,
        };

        self.experiments.insert(id, experiment.clone());
        Ok(experiment)
    }

    /// Start an experiment (changes status to Running).
    pub fn start_experiment(&self, experiment_id: &str) -> Result<Experiment, String> {
        let mut exp = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| format!("Experiment '{}' not found", experiment_id))?;

        if exp.status != ExperimentStatus::Draft && exp.status != ExperimentStatus::Paused {
            return Err(format!(
                "Cannot start experiment in {:?} status",
                exp.status
            ));
        }

        if exp.status == ExperimentStatus::Draft {
            exp.start_time = Some(Self::now_epoch());
        }
        exp.status = ExperimentStatus::Running;
        exp.updated_at = Self::now_epoch();

        Ok(exp.clone())
    }

    /// Pause a running experiment.
    pub fn pause_experiment(&self, experiment_id: &str) -> Result<Experiment, String> {
        let mut exp = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| format!("Experiment '{}' not found", experiment_id))?;

        if exp.status != ExperimentStatus::Running {
            return Err(format!(
                "Cannot pause experiment in {:?} status",
                exp.status
            ));
        }

        exp.status = ExperimentStatus::Paused;
        exp.updated_at = Self::now_epoch();

        Ok(exp.clone())
    }

    /// Complete an experiment.
    pub fn complete_experiment(&self, experiment_id: &str) -> Result<Experiment, String> {
        let mut exp = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| format!("Experiment '{}' not found", experiment_id))?;

        if exp.status != ExperimentStatus::Running && exp.status != ExperimentStatus::Paused {
            return Err(format!(
                "Cannot complete experiment in {:?} status",
                exp.status
            ));
        }

        exp.status = ExperimentStatus::Completed;
        exp.end_time = Some(Self::now_epoch());
        exp.updated_at = Self::now_epoch();

        Ok(exp.clone())
    }

    /// Assign a user to a variant.
    ///
    /// Uses sticky sessions: once assigned, the same user always gets the same variant.
    /// If the experiment is not running, returns an error.
    pub fn assign_variant(
        &self,
        experiment_id: &str,
        user_id: &str,
    ) -> Result<AssignmentResult, String> {
        let exp = self
            .experiments
            .get(experiment_id)
            .ok_or_else(|| format!("Experiment '{}' not found", experiment_id))?;

        if exp.status != ExperimentStatus::Running {
            return Err(format!(
                "Experiment '{}' is not running (status: {:?})",
                experiment_id, exp.status
            ));
        }

        // Check cache first (sticky session)
        let cache_key = (experiment_id.to_string(), user_id.to_string());
        if let Some(cached_variant_id) = self.assignment_cache.get(&cache_key) {
            let variant = exp
                .variants
                .iter()
                .find(|v| v.id == *cached_variant_id)
                .unwrap();
            return Ok(AssignmentResult {
                experiment_id: experiment_id.to_string(),
                variant_id: variant.id.clone(),
                variant_name: variant.name.clone(),
                is_cached: true,
            });
        }

        // Check traffic allocation using hash
        let hash = Self::hash_pair(experiment_id, user_id);
        let hash_frac = (hash as f64) / (u64::MAX as f64);

        if hash_frac > exp.traffic_allocation {
            // User is not in the experiment traffic allocation
            let control = exp.variants.iter().find(|v| v.is_control).unwrap();
            return Ok(AssignmentResult {
                experiment_id: experiment_id.to_string(),
                variant_id: control.id.clone(),
                variant_name: control.name.clone(),
                is_cached: false,
            });
        }

        // Assign to variant based on weight distribution
        let variant_hash = Self::hash_pair(experiment_id, &format!("variant:{}", user_id));
        let variant_frac = (variant_hash as f64) / (u64::MAX as f64);

        let mut cumulative = 0.0;
        let selected_variant = exp
            .variants
            .iter()
            .find(|v| {
                cumulative += v.weight;
                variant_frac <= cumulative
            })
            .or_else(|| exp.variants.last())
            .unwrap();

        // Cache the assignment
        self.assignment_cache
            .insert(cache_key.clone(), selected_variant.id.clone());

        // Clone what we need before dropping the immutable borrow
        let selected_variant_id = selected_variant.id.clone();
        let selected_variant_name = selected_variant.name.clone();

        // Update assignment count
        drop(exp);
        if let Some(mut exp) = self.experiments.get_mut(experiment_id) {
            exp.total_assignments += 1;
            // Also update variant-level metrics
            if let Some(variant) = exp.variants.iter_mut().find(|v| v.id == selected_variant_id) {
                *variant.metrics.entry("assignments".to_string()).or_insert(0.0) += 1.0;
            }
        }

        Ok(AssignmentResult {
            experiment_id: experiment_id.to_string(),
            variant_id: selected_variant_id,
            variant_name: selected_variant_name,
            is_cached: false,
        })
    }

    /// Record a metric value for a variant in an experiment.
    pub fn record_metric(
        &self,
        experiment_id: &str,
        variant_id: &str,
        metric_name: &str,
        value: f64,
    ) -> Result<(), String> {
        let mut exp = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| format!("Experiment '{}' not found", experiment_id))?;

        let variant = exp
            .variants
            .iter_mut()
            .find(|v| v.id == variant_id)
            .ok_or_else(|| format!("Variant '{}' not found", variant_id))?;

        // Update the variant's metric (sum)
        *variant.metrics.entry(metric_name.to_string()).or_insert(0.0) += value;

        // Also update experiment-level metric
        if let Some(em) = exp.metrics.iter_mut().find(|m| m.name == metric_name) {
            em.current_value += value;
        }

        Ok(())
    }

    /// Get aggregated results for an experiment.
    pub fn get_results(&self, experiment_id: &str) -> Result<ExperimentResults, String> {
        let exp = self
            .experiments
            .get(experiment_id)
            .ok_or_else(|| format!("Experiment '{}' not found", experiment_id))?;

        let variant_results: Vec<VariantResult> = exp
            .variants
            .iter()
            .map(|v| VariantResult {
                variant_id: v.id.clone(),
                variant_name: v.name.clone(),
                is_control: v.is_control,
                assignments: v.metrics.get("assignments").copied().unwrap_or(0.0) as u64,
                metrics: v.metrics.clone(),
            })
            .collect();

        Ok(ExperimentResults {
            experiment_id: exp.id.clone(),
            experiment_name: exp.name.clone(),
            status: exp.status.clone(),
            variant_results,
            metrics: exp.metrics.clone(),
            total_assignments: exp.total_assignments,
        })
    }

    /// Get an experiment by ID.
    pub fn get_experiment(&self, experiment_id: &str) -> Option<Experiment> {
        self.experiments.get(experiment_id).map(|e| e.clone())
    }

    /// List all experiments, optionally filtered by status.
    pub fn list_experiments(&self, status: Option<&ExperimentStatus>) -> Vec<Experiment> {
        self.experiments
            .iter()
            .map(|e| e.value().clone())
            .filter(|e| status.is_none() || &e.status == status.unwrap())
            .collect()
    }

    /// Calculate statistical significance between variants for a metric.
    ///
    /// Uses a simplified two-proportion z-test for conversion-style metrics.
    /// Returns significance value 0.0-1.0.
    pub fn calculate_significance(
        &self,
        experiment_id: &str,
        metric_name: &str,
    ) -> Result<f64, String> {
        let exp = self
            .experiments
            .get(experiment_id)
            .ok_or_else(|| format!("Experiment '{}' not found", experiment_id))?;

        let mut values: Vec<(String, f64, f64)> = Vec::new(); // (variant_id, total, metric_sum)

        for variant in &exp.variants {
            let total = variant.metrics.get("assignments").copied().unwrap_or(0.0);
            let metric_sum = variant.metrics.get(metric_name).copied().unwrap_or(0.0);
            values.push((variant.id.clone(), total, metric_sum));
        }

        if values.len() < 2 {
            return Err("Need at least 2 variants for significance calculation".to_string());
        }

        // Two-proportion z-test between control and first non-control variant
        let control = values.iter().find(|(id, _, _)| {
            exp.variants
                .iter()
                .any(|v| v.id == *id && v.is_control)
        });

        let treatment = values.iter().find(|(id, _, _)| {
            exp.variants
                .iter()
                .any(|v| v.id == *id && !v.is_control)
        });

        match (control, treatment) {
            (Some((_, n1, x1)), Some((_, n2, x2))) => {
                if *n1 < 1.0 || *n2 < 1.0 {
                    return Ok(0.0); // Not enough data
                }

                let p1 = x1 / n1;
                let p2 = x2 / n2;
                let p_pooled = (x1 + x2) / (n1 + n2);

                if p_pooled == 0.0 || p_pooled == 1.0 {
                    return Ok(0.0);
                }

                let se = (p_pooled * (1.0 - p_pooled) * (1.0 / n1 + 1.0 / n2)).sqrt();
                if se == 0.0 {
                    return Ok(0.0);
                }

                let z = (p2 - p1).abs() / se;
                // Approximate normal CDF for |z|
                // Using a simple approximation: significance ≈ 1 - 2 * exp(-z^2 / 2) * (1/sqrt(2*pi)) * (1/z)
                let significance = Self::approximate_normal_cdf(z);

                Ok(significance)
            }
            _ => Err("Need both control and treatment variants".to_string()),
        }
    }

    /// Approximate the cumulative distribution function of the standard normal distribution.
    fn approximate_normal_cdf(z: f64) -> f64 {
        if z <= 0.0 {
            return 0.0;
        }
        // Approximation: P(Z <= z) using Abramowitz and Stegun
        let t = 1.0 / (1.0 + 0.2316419 * z);
        let d = 0.3989422804014327; // 1/sqrt(2*pi)
        let p =
            d * (-z * z / 2.0).exp()
                * t
                * (0.319381530
                    + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
        // Return two-tailed significance
        (1.0 - p) * 2.0
    }

    /// Get statistics about the framework.
    pub fn get_stats(&self) -> ExperimentStats {
        let total_experiments = self.experiments.len();
        let mut by_status: HashMap<String, usize> = HashMap::new();
        let mut total_assignments: u64 = 0;

        for exp in self.experiments.iter() {
            let status_str = format!("{:?}", exp.value().status).to_lowercase();
            *by_status.entry(status_str).or_insert(0) += 1;
            total_assignments += exp.value().total_assignments;
        }

        ExperimentStats {
            total_experiments,
            by_status,
            total_assignments,
            cache_size: self.assignment_cache.len(),
        }
    }

    /// Clear the assignment cache.
    pub fn clear_cache(&self) {
        self.assignment_cache.clear();
    }
}

impl Default for ExperimentFramework {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};

/// Build the experiment framework router.
pub fn build_experiment_router(state: crate::api::AppState) -> Router {
    Router::new()
        .route("/v1/experiments", post(create_experiment_handler))
        .route("/v1/experiments", get(list_experiments_handler))
        .route("/v1/experiments/{id}", get(get_experiment_handler))
        .route("/v1/experiments/{id}/status", put(update_experiment_status_handler))
        .route("/v1/experiments/{id}/assign", post(assign_variant_handler))
        .route("/v1/experiments/{id}/metric", post(record_metric_handler))
        .route("/v1/experiments/{id}/results", get(get_results_handler))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct CreateExperimentRequest {
    name: String,
    description: String,
    variants: Vec<Variant>,
    #[serde(default = "default_traffic_allocation")]
    traffic_allocation: f64,
    #[serde(default)]
    metrics: Vec<String>,
}

fn default_traffic_allocation() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
struct UpdateStatusRequest {
    status: ExperimentStatus,
}

#[derive(Debug, Deserialize)]
struct AssignVariantRequest {
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct RecordMetricRequest {
    variant_id: String,
    metric_name: String,
    value: f64,
}

async fn create_experiment_handler(
    State(state): State<crate::api::AppState>,
    axum::Json(req): axum::Json<CreateExperimentRequest>,
) -> impl IntoResponse {
    match state.experiments.create_experiment(
        req.name,
        req.description,
        req.variants,
        req.traffic_allocation,
        req.metrics,
    ) {
        Ok(exp) => (axum::http::StatusCode::CREATED, axum::Json(exp)).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn list_experiments_handler(
    State(state): State<crate::api::AppState>,
) -> impl IntoResponse {
    axum::Json(state.experiments.list_experiments(None))
}

async fn get_experiment_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.experiments.get_experiment(&id) {
        Some(exp) => axum::Json(exp).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": "Experiment not found"})),
        )
            .into_response(),
    }
}

async fn update_experiment_status_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
    axum::Json(req): axum::Json<UpdateStatusRequest>,
) -> impl IntoResponse {
    let result = match req.status {
        ExperimentStatus::Running => state.experiments.start_experiment(&id),
        ExperimentStatus::Paused => state.experiments.pause_experiment(&id),
        ExperimentStatus::Completed => state.experiments.complete_experiment(&id),
        _ => Err(format!("Cannot transition to {:?} status directly", req.status)),
    };

    match result {
        Ok(exp) => axum::Json(exp).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn assign_variant_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
    axum::Json(req): axum::Json<AssignVariantRequest>,
) -> impl IntoResponse {
    match state.experiments.assign_variant(&id, &req.user_id) {
        Ok(result) => axum::Json(result).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn record_metric_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
    axum::Json(req): axum::Json<RecordMetricRequest>,
) -> impl IntoResponse {
    match state
        .experiments
        .record_metric(&id, &req.variant_id, &req.metric_name, req.value)
    {
        Ok(()) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({"recorded": true})),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn get_results_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.experiments.get_results(&id) {
        Ok(results) => axum::Json(results).into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": e})),
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

    fn create_framework() -> ExperimentFramework {
        ExperimentFramework::new()
    }

    fn default_variants() -> Vec<Variant> {
        vec![
            Variant {
                id: "control".into(),
                name: "Control".into(),
                weight: 0.5,
                is_control: true,
                config: HashMap::new(),
                metrics: HashMap::new(),
            },
            Variant {
                id: "treatment".into(),
                name: "Treatment".into(),
                weight: 0.5,
                is_control: false,
                config: HashMap::new(),
                metrics: HashMap::new(),
            },
        ]
    }

    #[test]
    fn test_create_experiment() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Test Experiment".into(),
                "A test".into(),
                default_variants(),
                1.0,
                vec!["conversion".into()],
            )
            .unwrap();

        assert_eq!(exp.status, ExperimentStatus::Draft);
        assert_eq!(exp.variants.len(), 2);
        assert_eq!(exp.metrics.len(), 1);
    }

    #[test]
    fn test_create_experiment_no_variants_fails() {
        let fw = create_framework();
        assert!(fw
            .create_experiment(
                "Bad".into(),
                "No variants".into(),
                vec![],
                1.0,
                vec![],
            )
            .is_err());
    }

    #[test]
    fn test_create_experiment_bad_weights_fails() {
        let fw = create_framework();
        let mut variants = default_variants();
        variants[0].weight = 0.9;
        variants[1].weight = 0.2; // total 1.1

        assert!(fw
            .create_experiment("Bad".into(), "".into(), variants, 1.0, vec![])
            .is_err());
    }

    #[test]
    fn test_create_experiment_no_control_fails() {
        let fw = create_framework();
        let mut variants = default_variants();
        variants[0].is_control = false;
        variants[1].is_control = false;

        assert!(fw
            .create_experiment("Bad".into(), "".into(), variants, 1.0, vec![])
            .is_err());
    }

    #[test]
    fn test_experiment_lifecycle() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Lifecycle".into(),
                "".into(),
                default_variants(),
                1.0,
                vec![],
            )
            .unwrap();

        // Draft -> Running
        let running = fw.start_experiment(&exp.id).unwrap();
        assert_eq!(running.status, ExperimentStatus::Running);
        assert!(running.start_time.is_some());

        // Running -> Paused
        let paused = fw.pause_experiment(&exp.id).unwrap();
        assert_eq!(paused.status, ExperimentStatus::Paused);

        // Paused -> Running
        let running2 = fw.start_experiment(&exp.id).unwrap();
        assert_eq!(running2.status, ExperimentStatus::Running);

        // Running -> Completed
        let completed = fw.complete_experiment(&exp.id).unwrap();
        assert_eq!(completed.status, ExperimentStatus::Completed);
        assert!(completed.end_time.is_some());
    }

    #[test]
    fn test_cannot_start_completed_experiment() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Done".into(),
                "".into(),
                default_variants(),
                1.0,
                vec![],
            )
            .unwrap();

        fw.start_experiment(&exp.id).unwrap();
        fw.complete_experiment(&exp.id).unwrap();
        assert!(fw.start_experiment(&exp.id).is_err());
    }

    #[test]
    fn test_assign_variant_sticky() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Sticky".into(),
                "".into(),
                default_variants(),
                1.0,
                vec![],
            )
            .unwrap();

        fw.start_experiment(&exp.id).unwrap();

        let a1 = fw.assign_variant(&exp.id, "user1").unwrap();
        let a2 = fw.assign_variant(&exp.id, "user1").unwrap();

        assert_eq!(a1.variant_id, a2.variant_id);
        assert!(!a1.is_cached);
        assert!(a2.is_cached);
    }

    #[test]
    fn test_record_metric() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Metrics".into(),
                "".into(),
                default_variants(),
                1.0,
                vec!["revenue".into()],
            )
            .unwrap();

        fw.start_experiment(&exp.id).unwrap();
        fw.assign_variant(&exp.id, "u1").unwrap();

        fw.record_metric(&exp.id, "control", "revenue", 10.0).unwrap();
        fw.record_metric(&exp.id, "control", "revenue", 5.0).unwrap();

        let results = fw.get_results(&exp.id).unwrap();
        let control = results.variant_results.iter().find(|v| v.is_control).unwrap();
        assert_eq!(control.metrics.get("revenue"), Some(&15.0));
    }

    #[test]
    fn test_traffic_allocation() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Partial".into(),
                "".into(),
                default_variants(),
                0.5,
                vec![],
            )
            .unwrap();

        fw.start_experiment(&exp.id).unwrap();

        // With 50% traffic allocation, approximately 50% should get treatment
        let mut treatment_count = 0;
        let total = 1000;
        for i in 0..total {
            let result = fw.assign_variant(&exp.id, &format!("user-{}", i)).unwrap();
            if result.variant_id == "treatment" {
                treatment_count += 1;
            }
        }

        // Should be roughly 25% (50% allocation * 50% treatment weight)
        // Allow wide tolerance for hash distribution
        assert!(
            treatment_count > 100 && treatment_count < 400,
            "Expected ~25% treatment assignments, got {}%",
            treatment_count * 100 / total
        );
    }

    #[test]
    fn test_list_experiments() {
        let fw = create_framework();
        fw.create_experiment(
            "A".into(),
            "".into(),
            default_variants(),
            1.0,
            vec![],
        )
        .unwrap();
        fw.create_experiment(
            "B".into(),
            "".into(),
            default_variants(),
            1.0,
            vec![],
        )
        .unwrap();

        assert_eq!(fw.list_experiments(None).len(), 2);
        assert_eq!(
            fw.list_experiments(Some(&ExperimentStatus::Draft))
                .len(),
            2
        );
    }

    #[test]
    fn test_get_stats() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Stats".into(),
                "".into(),
                default_variants(),
                1.0,
                vec![],
            )
            .unwrap();
        fw.start_experiment(&exp.id).unwrap();
        fw.assign_variant(&exp.id, "u1").unwrap();

        let stats = fw.get_stats();
        assert_eq!(stats.total_experiments, 1);
        assert_eq!(stats.total_assignments, 1);
        assert_eq!(stats.cache_size, 1);
    }

    #[test]
    fn test_significance_calculation() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Sig".into(),
                "".into(),
                default_variants(),
            1.0,
                vec!["conversions".into()],
            )
        .unwrap();

        fw.start_experiment(&exp.id).unwrap();

        // Record some metrics
        fw.record_metric(&exp.id, "control", "conversions", 50.0).unwrap();
        fw.record_metric(&exp.id, "treatment", "conversions", 80.0).unwrap();

        // Add assignments
        for _ in 0..100 {
            let r = fw.assign_variant(&exp.id, &format!("cu-{}", uuid::Uuid::new_v4())).unwrap();
        }

        let sig = fw.calculate_significance(&exp.id, "conversions");
        assert!(sig.is_ok());
    }

    #[test]
    fn test_clear_cache() {
        let fw = create_framework();
        let exp = fw
            .create_experiment(
                "Cache".into(),
                "".into(),
                default_variants(),
                1.0,
                vec![],
            )
            .unwrap();
        fw.start_experiment(&exp.id).unwrap();
        fw.assign_variant(&exp.id, "u1").unwrap();

        assert_eq!(fw.get_stats().cache_size, 1);
        fw.clear_cache();
        assert_eq!(fw.get_stats().cache_size, 0);
    }
}
