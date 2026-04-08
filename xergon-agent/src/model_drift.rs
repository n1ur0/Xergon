//! Model Drift Detection and Monitoring
//!
//! Detects and monitors model drift using statistical tests:
//! - KL divergence for distribution drift
//! - PSI (Population Stability Index) for data drift
//! - Z-test for mean shift detection
//! - Chi-squared test for categorical drift
//!
//! Features:
//! - Configurable alert thresholds per severity
//! - Drift history tracking and trend analysis
//! - Baseline establishment and comparison
//!
//! REST endpoints:
//! - POST /v1/drift/baseline            — Establish a baseline
//! - POST /v1/drift/detect              — Detect drift against baseline
//! - GET  /v1/drift/baseline/:model_id  — Get baseline for a model
//! - GET  /v1/drift/reports/:model_id   — Get drift reports
//! - GET  /v1/drift/alerts              — Get active alerts
//! - GET  /v1/drift/thresholds          — Get current thresholds
//! - PUT  /v1/drift/thresholds          — Update thresholds

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Types of drift metrics to track.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DriftMetric {
    PredictionDrift,
    DataDrift,
    ConceptDrift,
    PerformanceDrift,
}

impl std::fmt::Display for DriftMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriftMetric::PredictionDrift => write!(f, "prediction_drift"),
            DriftMetric::DataDrift => write!(f, "data_drift"),
            DriftMetric::ConceptDrift => write!(f, "concept_drift"),
            DriftMetric::PerformanceDrift => write!(f, "performance_drift"),
        }
    }
}

/// Severity levels for drift alerts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DriftSeverity {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for DriftSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriftSeverity::None => write!(f, "none"),
            DriftSeverity::Low => write!(f, "low"),
            DriftSeverity::Medium => write!(f, "medium"),
            DriftSeverity::High => write!(f, "high"),
            DriftSeverity::Critical => write!(f, "critical"),
        }
    }
}

// ---------------------------------------------------------------------------
// DriftReport
// ---------------------------------------------------------------------------

/// A report of detected drift for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub model_id: String,
    pub metric_type: DriftMetric,
    pub severity: DriftSeverity,
    pub baseline_score: f64,
    pub current_score: f64,
    pub drift_magnitude: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub details: String,
}

// ---------------------------------------------------------------------------
// DriftBaseline
// ---------------------------------------------------------------------------

/// A statistical baseline for a model's performance/distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftBaseline {
    pub model_id: String,
    pub metric_type: DriftMetric,
    pub reference_data_hash: String,
    pub reference_mean: f64,
    pub reference_std: f64,
    pub reference_percentiles: HashMap<String, f64>,
    pub reference_histogram: Vec<f64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// DriftAlert
// ---------------------------------------------------------------------------

/// An alert triggered by drift detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftAlert {
    pub id: String,
    pub model_id: String,
    pub metric_type: DriftMetric,
    pub severity: DriftSeverity,
    pub message: String,
    pub drift_magnitude: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub acknowledged: bool,
}

// ---------------------------------------------------------------------------
// DriftThresholds
// ---------------------------------------------------------------------------

/// Configurable thresholds for drift severity levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftThresholds {
    /// KL divergence threshold for low severity.
    pub kl_divergence_low: f64,
    /// KL divergence threshold for medium severity.
    pub kl_divergence_medium: f64,
    /// KL divergence threshold for high severity.
    pub kl_divergence_high: f64,
    /// KL divergence threshold for critical severity.
    pub kl_divergence_critical: f64,
    /// PSI threshold for low severity.
    pub psi_low: f64,
    /// PSI threshold for medium severity.
    pub psi_medium: f64,
    /// PSI threshold for high severity.
    pub psi_high: f64,
    /// PSI threshold for critical severity.
    pub psi_critical: f64,
    /// Z-score threshold for significant mean shift.
    pub z_score_threshold: f64,
    /// Maximum number of reports to keep per model.
    pub max_reports_per_model: usize,
    /// Maximum number of alerts to keep.
    pub max_alerts: usize,
}

impl Default for DriftThresholds {
    fn default() -> Self {
        Self {
            kl_divergence_low: 0.1,
            kl_divergence_medium: 0.5,
            kl_divergence_high: 1.0,
            kl_divergence_critical: 2.0,
            psi_low: 0.1,
            psi_medium: 0.25,
            psi_high: 0.5,
            psi_critical: 1.0,
            z_score_threshold: 2.0,
            max_reports_per_model: 1000,
            max_alerts: 500,
        }
    }
}

// ---------------------------------------------------------------------------
// ModelDriftDetector
// ---------------------------------------------------------------------------

/// Drift detection and monitoring system.
pub struct ModelDriftDetector {
    /// Baselines per (model_id, metric_type).
    baselines: DashMap<String, DriftBaseline>,
    /// Report history per model_id.
    reports: DashMap<String, Vec<DriftReport>>,
    /// Active alerts.
    alerts: DashMap<String, Vec<DriftAlert>>,
    /// Configurable thresholds.
    thresholds: tokio::sync::RwLock<DriftThresholds>,
}

impl ModelDriftDetector {
    /// Create a new drift detector.
    pub fn new() -> Self {
        Self {
            baselines: DashMap::new(),
            reports: DashMap::new(),
            alerts: DashMap::new(),
            thresholds: tokio::sync::RwLock::new(DriftThresholds::default()),
        }
    }

    /// Create with custom thresholds.
    pub fn with_thresholds(thresholds: DriftThresholds) -> Self {
        Self {
            baselines: DashMap::new(),
            reports: DashMap::new(),
            alerts: DashMap::new(),
            thresholds: tokio::sync::RwLock::new(thresholds),
        }
    }

    fn baseline_key(model_id: &str, metric_type: &DriftMetric) -> String {
        format!("{}:{}", model_id, metric_type)
    }

    /// Establish a baseline for a model and metric type.
    pub fn establish_baseline(
        &self,
        model_id: impl Into<String>,
        metric_type: DriftMetric,
        reference_data: &[f64],
    ) -> DriftBaseline {
        let model_id = model_id.into();
        let key = Self::baseline_key(&model_id, &metric_type);

        let mean = statistical_mean(reference_data);
        let std_dev = statistical_std(reference_data, mean);
        let percentiles = compute_percentiles(reference_data);
        let histogram = build_histogram(reference_data, 20);
        let data_hash = simple_hash(&model_id, &metric_type, reference_data);

        let baseline = DriftBaseline {
            model_id: model_id.clone(),
            metric_type: metric_type.clone(),
            reference_data_hash: data_hash,
            reference_mean: mean,
            reference_std: std_dev,
            reference_percentiles: percentiles,
            reference_histogram: histogram,
            created_at: Utc::now(),
        };

        self.baselines.insert(key, baseline.clone());

        info!(
            model_id = %model_id,
            metric = %metric_type,
            mean = baseline.reference_mean,
            std = baseline.reference_std,
            "Drift baseline established"
        );

        baseline
    }

    /// Detect drift for a model against its baseline.
    pub async fn detect_drift(
        &self,
        model_id: impl Into<String>,
        metric_type: DriftMetric,
        current_data: &[f64],
    ) -> Result<DriftReport, String> {
        let model_id = model_id.into();
        let key = Self::baseline_key(&model_id, &metric_type);

        let baseline = self.baselines.get(&key)
            .ok_or_else(|| format!("No baseline found for model {} metric {}", model_id, metric_type))?;

        let baseline_ref = baseline.value();
        let thresholds = self.thresholds.read().await;

        // Compute KL divergence
        let baseline_hist = normalize_histogram(&baseline_ref.reference_histogram);
        let current_hist = build_histogram(current_data, baseline_hist.len());
        let current_hist_norm = normalize_histogram(&current_hist);
        let baseline_hist_norm = normalize_histogram(&baseline_ref.reference_histogram);
        let kl_div = kl_divergence(&baseline_hist_norm, &current_hist_norm);

        // Compute PSI
        let psi = compute_psi(&baseline_hist_norm, &current_hist_norm);

        // Compute Z-test for mean shift
        let current_mean = statistical_mean(current_data);
        let current_std = statistical_std(current_data, current_mean);
        let z_score = if baseline_ref.reference_std > 0.0 && current_data.len() > 1 {
            let se = current_std / (current_data.len() as f64).sqrt();
            if se > 0.0 {
                ((current_mean - baseline_ref.reference_mean) / se).abs()
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Determine severity based on KL divergence and PSI
        let severity = determine_severity(kl_div, psi, &thresholds);

        // Compute overall drift magnitude (composite score)
        let drift_magnitude = 0.4 * kl_div + 0.4 * psi + 0.2 * (z_score / 5.0).min(1.0);

        let details = format!(
            "KL divergence: {:.6}, PSI: {:.6}, Z-score: {:.4}, current_mean: {:.4} (baseline: {:.4})",
            kl_div, psi, z_score, current_mean, baseline_ref.reference_mean
        );

        let report = DriftReport {
            model_id: model_id.clone(),
            metric_type: metric_type.clone(),
            severity: severity.clone(),
            baseline_score: baseline_ref.reference_mean,
            current_score: current_mean,
            drift_magnitude,
            timestamp: Utc::now(),
            details,
        };

        drop(baseline);
        drop(thresholds);

        // Store report
        self.reports.entry(model_id.clone())
            .or_default()
            .push(report.clone());

        // Trim old reports
        self.trim_reports(&model_id).await;

        // Generate alert if severity is Medium or higher
        if severity >= DriftSeverity::Medium {
            self.generate_alert(model_id.clone(), metric_type, &report).await;
        }

        debug!(
            model_id = %model_id,
            metric = %report.metric_type,
            severity = ?report.severity,
            magnitude = report.drift_magnitude,
            "Drift detection completed"
        );

        Ok(report)
    }

    /// Get the baseline for a model and metric type.
    pub fn get_baseline(&self, model_id: &str, metric_type: &DriftMetric) -> Option<DriftBaseline> {
        let key = Self::baseline_key(model_id, metric_type);
        self.baselines.get(&key).map(|b| b.value().clone())
    }

    /// Get all drift reports for a model.
    pub fn get_reports(&self, model_id: &str) -> Vec<DriftReport> {
        self.reports.get(model_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Get the latest report for a model.
    pub fn get_latest_report(&self, model_id: &str) -> Option<DriftReport> {
        self.reports.get(model_id)
            .and_then(|r| r.value().last().cloned())
    }

    /// Get all active (unacknowledged) alerts.
    pub fn get_alerts(&self) -> Vec<DriftAlert> {
        let mut all_alerts = Vec::new();
        for entry in self.alerts.iter() {
            for alert in entry.value().iter() {
                if !alert.acknowledged {
                    all_alerts.push(alert.clone());
                }
            }
        }
        // Sort by timestamp descending
        all_alerts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all_alerts
    }

    /// Get alerts for a specific model.
    pub fn get_alerts_for_model(&self, model_id: &str) -> Vec<DriftAlert> {
        self.alerts.get(model_id)
            .map(|a| a.value().clone())
            .unwrap_or_default()
    }

    /// Acknowledge an alert.
    pub fn acknowledge_alert(&self, model_id: &str, alert_id: &str) -> bool {
        if let Some(mut alerts) = self.alerts.get_mut(model_id) {
            for alert in alerts.value_mut().iter_mut() {
                if alert.id == alert_id {
                    alert.acknowledged = true;
                    return true;
                }
            }
        }
        false
    }

    /// Update drift thresholds.
    pub async fn update_thresholds(&self, thresholds: DriftThresholds) {
        let mut cfg = self.thresholds.write().await;
        *cfg = thresholds;
        info!("Drift thresholds updated");
    }

    /// Get current thresholds.
    pub async fn get_thresholds(&self) -> DriftThresholds {
        self.thresholds.read().await.clone()
    }

    /// Compare two models by their drift reports.
    pub fn compare_models(&self, model_a: &str, model_b: &str) -> ModelComparison {
        let reports_a = self.get_reports(model_a);
        let reports_b = self.get_reports(model_b);

        let latest_a = reports_a.last();
        let latest_b = reports_b.last();

        let avg_drift_a = if reports_a.is_empty() {
            0.0
        } else {
            reports_a.iter().map(|r| r.drift_magnitude).sum::<f64>() / reports_a.len() as f64
        };

        let avg_drift_b = if reports_b.is_empty() {
            0.0
        } else {
            reports_b.iter().map(|r| r.drift_magnitude).sum::<f64>() / reports_b.len() as f64
        };

        let trend_a = compute_trend(&reports_a);
        let trend_b = compute_trend(&reports_b);

        ModelComparison {
            model_a: model_a.to_string(),
            model_b: model_b.to_string(),
            latest_severity_a: latest_a.map(|r| r.severity.clone()),
            latest_severity_b: latest_b.map(|r| r.severity.clone()),
            avg_drift_a,
            avg_drift_b,
            trend_a,
            trend_b,
            more_stable: if avg_drift_a <= avg_drift_b {
                model_a.to_string()
            } else {
                model_b.to_string()
            },
        }
    }

    /// Get trend analysis for a model.
    pub fn get_trend(&self, model_id: &str) -> Option<TrendAnalysis> {
        let reports = self.get_reports(model_id);
        if reports.is_empty() {
            return None;
        }

        let trend = compute_trend(&reports);
        let recent_reports: Vec<_> = reports.iter().rev().take(10).collect();
        let recent_severities: Vec<_> = recent_reports.iter().map(|r| r.severity.clone()).collect();

        let most_frequent_severity = most_common_severity(&recent_severities);

        Some(TrendAnalysis {
            model_id: model_id.to_string(),
            trend_direction: trend,
            report_count: reports.len(),
            recent_severities,
            most_frequent_severity,
            last_report: reports.last().cloned(),
        })
    }

    async fn generate_alert(&self, model_id: String, metric_type: DriftMetric, report: &DriftReport) {
        let alert = DriftAlert {
            id: uuid::Uuid::new_v4().to_string(),
            model_id: model_id.clone(),
            metric_type,
            severity: report.severity.clone(),
            message: format!(
                "Drift detected for {}: severity={}, magnitude={:.4}",
                model_id, report.severity, report.drift_magnitude
            ),
            drift_magnitude: report.drift_magnitude,
            timestamp: Utc::now(),
            acknowledged: false,
        };

        self.alerts.entry(model_id.clone())
            .or_default()
            .push(alert.clone());

        // Trim old alerts
        let thresholds = self.thresholds.read().await;
        let max_alerts = thresholds.max_alerts;
        if let Some(mut entry) = self.alerts.get_mut(&model_id) {
            let len = entry.len();
            if len > max_alerts {
                entry.value_mut().drain(..len - max_alerts);
            }
        }

        warn!(
            model_id = %model_id,
            severity = ?report.severity,
            magnitude = report.drift_magnitude,
            "Drift alert generated"
        );
    }

    async fn trim_reports(&self, model_id: &str) {
        let thresholds = self.thresholds.read().await;
        let max_reports = thresholds.max_reports_per_model;
        if let Some(mut entry) = self.reports.get_mut(model_id) {
            let len = entry.len();
            if len > max_reports {
                entry.value_mut().drain(..len - max_reports);
            }
        }
    }

    /// Get total number of baselines.
    pub fn baseline_count(&self) -> usize {
        self.baselines.len()
    }

    /// Get total number of reports.
    pub fn report_count(&self) -> usize {
        self.reports.iter().map(|e| e.value().len()).sum()
    }
}

impl Default for ModelDriftDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Model comparison and trend types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelComparison {
    pub model_a: String,
    pub model_b: String,
    pub latest_severity_a: Option<DriftSeverity>,
    pub latest_severity_b: Option<DriftSeverity>,
    pub avg_drift_a: f64,
    pub avg_drift_b: f64,
    pub trend_a: TrendDirection,
    pub trend_b: TrendDirection,
    pub more_stable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrendDirection {
    Improving,
    Stable,
    Worsening,
    InsufficientData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub model_id: String,
    pub trend_direction: TrendDirection,
    pub report_count: usize,
    pub recent_severities: Vec<DriftSeverity>,
    pub most_frequent_severity: Option<DriftSeverity>,
    pub last_report: Option<DriftReport>,
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EstablishBaselineRequest {
    pub model_id: String,
    pub metric_type: String,
    pub reference_data: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct EstablishBaselineResponse {
    pub baseline: DriftBaseline,
}

#[derive(Debug, Deserialize)]
pub struct DetectDriftRequest {
    pub model_id: String,
    pub metric_type: String,
    pub current_data: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct DetectDriftResponse {
    pub report: DriftReport,
}

#[derive(Debug, Serialize)]
pub struct AlertsResponse {
    pub alerts: Vec<DriftAlert>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ReportsResponse {
    pub model_id: String,
    pub reports: Vec<DriftReport>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct TrendResponse {
    pub analysis: Option<TrendAnalysis>,
}

#[derive(Debug, Serialize)]
pub struct CompareResponse {
    pub comparison: ModelComparison,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn establish_baseline_handler(
    State(detector): State<Arc<ModelDriftDetector>>,
    Json(body): Json<EstablishBaselineRequest>,
) -> Result<Json<EstablishBaselineResponse>, StatusCode> {
    let metric_type: DriftMetric = parse_metric_type(&body.metric_type)
        .ok_or(StatusCode::BAD_REQUEST)?;

    if body.reference_data.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let baseline = detector.establish_baseline(&body.model_id, metric_type, &body.reference_data);
    Ok(Json(EstablishBaselineResponse { baseline }))
}

async fn detect_drift_handler(
    State(detector): State<Arc<ModelDriftDetector>>,
    Json(body): Json<DetectDriftRequest>,
) -> Result<Json<DetectDriftResponse>, StatusCode> {
    let metric_type: DriftMetric = parse_metric_type(&body.metric_type)
        .ok_or(StatusCode::BAD_REQUEST)?;

    if body.current_data.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let report = detector.detect_drift(&body.model_id, metric_type, &body.current_data)
        .await
        .map_err(|e| {
            warn!(error = %e, "Drift detection failed");
            StatusCode::NOT_FOUND
        })?;

    Ok(Json(DetectDriftResponse { report }))
}

async fn get_baseline_handler(
    State(detector): State<Arc<ModelDriftDetector>>,
    Path(model_id): Path<String>,
) -> Json<serde_json::Value> {
    let baselines: Vec<DriftBaseline> = [DriftMetric::PredictionDrift, DriftMetric::DataDrift,
        DriftMetric::ConceptDrift, DriftMetric::PerformanceDrift]
        .iter()
        .filter_map(|m| detector.get_baseline(&model_id, m))
        .collect();

    Json(serde_json::json!({
        "model_id": model_id,
        "baselines": baselines,
    }))
}

async fn get_reports_handler(
    State(detector): State<Arc<ModelDriftDetector>>,
    Path(model_id): Path<String>,
) -> Json<ReportsResponse> {
    let reports = detector.get_reports(&model_id);
    let total = reports.len();
    Json(ReportsResponse { model_id, reports, total })
}

async fn get_alerts_handler(
    State(detector): State<Arc<ModelDriftDetector>>,
) -> Json<AlertsResponse> {
    let alerts = detector.get_alerts();
    let total = alerts.len();
    Json(AlertsResponse { alerts, total })
}

async fn get_thresholds_handler(
    State(detector): State<Arc<ModelDriftDetector>>,
) -> Json<DriftThresholds> {
    Json(detector.get_thresholds().await)
}

async fn update_thresholds_handler(
    State(detector): State<Arc<ModelDriftDetector>>,
    Json(thresholds): Json<DriftThresholds>,
) -> Json<serde_json::Value> {
    detector.update_thresholds(thresholds).await;
    Json(serde_json::json!({ "updated": true }))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the model drift detection router.
pub fn build_drift_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{get, post};

    let detector = state.model_drift_detector.clone();

    axum::Router::new()
        .route("/v1/drift/baseline", post(establish_baseline_handler))
        .route("/v1/drift/detect", post(detect_drift_handler))
        .route("/v1/drift/baseline/{model_id}", get(get_baseline_handler))
        .route("/v1/drift/reports/{model_id}", get(get_reports_handler))
        .route("/v1/drift/alerts", get(get_alerts_handler))
        .route("/v1/drift/thresholds", get(get_thresholds_handler).put(update_thresholds_handler))
        .with_state(detector)
}

// ---------------------------------------------------------------------------
// Statistical functions
// ---------------------------------------------------------------------------

/// Compute the mean of a dataset.
pub fn statistical_mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    data.iter().sum::<f64>() / data.len() as f64
}

/// Compute the standard deviation of a dataset.
pub fn statistical_std(data: &[f64], mean: f64) -> f64 {
    if data.len() <= 1 {
        return 0.0;
    }
    let variance = data.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / (data.len() - 1) as f64;
    variance.sqrt()
}

/// Compute KL divergence between two probability distributions.
/// D_KL(P || Q) = sum(P(i) * ln(P(i) / Q(i)))
pub fn kl_divergence(p: &[f64], q: &[f64]) -> f64 {
    if p.len() != q.len() || p.is_empty() {
        return f64::INFINITY;
    }

    let eps = 1e-10;
    let mut kl = 0.0;

    for i in 0..p.len() {
        let p_val = p[i].max(eps);
        let q_val = q[i].max(eps);
        // Normalize to probability distributions
        let p_sum: f64 = p.iter().map(|x| x.max(eps)).sum();
        let q_sum: f64 = q.iter().map(|x| x.max(eps)).sum();
        let p_prob = p_val / p_sum;
        let q_prob = q_val / q_sum;

        if p_prob > eps && q_prob > eps {
            kl += p_prob * (p_prob.ln() - q_prob.ln());
        }
    }

    kl.max(0.0)
}

/// Compute Population Stability Index (PSI).
/// PSI = sum((Q_i - P_i) * ln(Q_i / P_i))
pub fn compute_psi(expected: &[f64], actual: &[f64]) -> f64 {
    if expected.len() != actual.len() || expected.is_empty() {
        return f64::INFINITY;
    }

    let eps = 1e-10;
    let e_sum: f64 = expected.iter().sum();
    let a_sum: f64 = actual.iter().sum();

    if e_sum <= eps || a_sum <= eps {
        return f64::INFINITY;
    }

    let mut psi = 0.0;
    for i in 0..expected.len() {
        let e_prob = (expected[i] / e_sum).max(eps);
        let a_prob = (actual[i] / a_sum).max(eps);

        let diff = a_prob - e_prob;
        let ratio = (a_prob / e_prob).max(eps);
        psi += diff * ratio.ln();
    }

    psi.abs()
}

/// Compute Z-test statistic for mean shift.
pub fn z_test(baseline_mean: f64, baseline_std: f64, baseline_n: usize,
              current_mean: f64, current_std: f64, current_n: usize) -> f64 {
    if baseline_n == 0 || current_n == 0 || baseline_std <= 0.0 {
        return 0.0;
    }

    let se = ((baseline_std.powi(2) / baseline_n as f64)
        + (current_std.powi(2) / current_n as f64)).sqrt();

    if se <= 0.0 {
        return 0.0;
    }

    ((current_mean - baseline_mean) / se).abs()
}

/// Chi-squared test for categorical drift.
/// Compares observed frequencies against expected frequencies.
pub fn chi_squared_test(expected: &[u64], observed: &[u64]) -> (f64, f64) {
    if expected.len() != observed.len() || expected.is_empty() {
        return (0.0, 1.0);
    }

    let mut chi_sq = 0.0;
    let mut total_expected = 0u64;
    let mut total_observed = 0u64;

    for i in 0..expected.len() {
        total_expected += expected[i];
        total_observed += observed[i];
    }

    if total_expected == 0 {
        return (0.0, 1.0);
    }

    for i in 0..expected.len() {
        let e = (expected[i] as f64 * total_observed as f64 / total_expected as f64).max(0.5);
        let o = observed[i] as f64;
        chi_sq += (o - e).powi(2) / e;
    }

    let df = (expected.len() - 1).max(1) as f64;
    // Approximate p-value using normal approximation for large df
    let z = ((chi_sq - df) / (2.0 * df).sqrt()).abs();
    let p_value = (1.0 - error_function(z / std::f64::consts::SQRT_2)) * 2.0;

    (chi_sq, p_value.min(1.0))
}

/// Approximate error function for p-value calculation.
fn error_function(x: f64) -> f64 {
    // Abramowitz and Stegun approximation
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x_abs = x.abs();
    let t = 1.0 / (1.0 + p * x_abs);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x_abs * x_abs).exp();
    sign * y
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn parse_metric_type(s: &str) -> Option<DriftMetric> {
    match s {
        "prediction_drift" => Some(DriftMetric::PredictionDrift),
        "data_drift" => Some(DriftMetric::DataDrift),
        "concept_drift" => Some(DriftMetric::ConceptDrift),
        "performance_drift" => Some(DriftMetric::PerformanceDrift),
        _ => None,
    }
}

fn compute_percentiles(data: &[f64]) -> HashMap<String, f64> {
    if data.is_empty() {
        return HashMap::new();
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut percentiles = HashMap::new();
    for (name, pct) in [("p25", 0.25), ("p50", 0.50), ("p75", 0.75), ("p90", 0.90), ("p99", 0.99)] {
        let idx = ((sorted.len() as f64) * pct) as usize;
        let val = sorted.get(idx.min(sorted.len() - 1)).copied().unwrap_or(0.0);
        percentiles.insert(name.to_string(), val);
    }
    percentiles
}

fn build_histogram(data: &[f64], bins: usize) -> Vec<f64> {
    if data.is_empty() || bins == 0 {
        return vec![0.0; bins];
    }

    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range <= 0.0 {
        let mut hist = vec![0.0; bins];
        if !hist.is_empty() {
            hist[0] = data.len() as f64;
        }
        return hist;
    }

    let bin_width = range / bins as f64;
    let mut histogram = vec![0.0; bins];

    for &val in data {
        let bin_idx = ((val - min) / bin_width) as usize;
        let idx = bin_idx.min(bins - 1);
        histogram[idx] += 1.0;
    }

    histogram
}

fn normalize_histogram(hist: &[f64]) -> Vec<f64> {
    let sum: f64 = hist.iter().sum();
    if sum <= 0.0 {
        return hist.to_vec();
    }
    hist.iter().map(|x| x / sum).collect()
}

fn simple_hash(model_id: &str, metric_type: &DriftMetric, data: &[f64]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    model_id.hash(&mut hasher);
    metric_type.to_string().hash(&mut hasher);
    data.len().hash(&mut hasher);
    if !data.is_empty() {
        data[0].to_bits().hash(&mut hasher);
        data[data.len() / 2].to_bits().hash(&mut hasher);
        data[data.len() - 1].to_bits().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn determine_severity(kl_div: f64, psi: f64, thresholds: &DriftThresholds) -> DriftSeverity {
    let kl_severity = if kl_div >= thresholds.kl_divergence_critical {
        4
    } else if kl_div >= thresholds.kl_divergence_high {
        3
    } else if kl_div >= thresholds.kl_divergence_medium {
        2
    } else if kl_div >= thresholds.kl_divergence_low {
        1
    } else {
        0
    };

    let psi_severity = if psi >= thresholds.psi_critical {
        4
    } else if psi >= thresholds.psi_high {
        3
    } else if psi >= thresholds.psi_medium {
        2
    } else if psi >= thresholds.psi_low {
        1
    } else {
        0
    };

    let max_severity = kl_severity.max(psi_severity);

    match max_severity {
        0 => DriftSeverity::None,
        1 => DriftSeverity::Low,
        2 => DriftSeverity::Medium,
        3 => DriftSeverity::High,
        _ => DriftSeverity::Critical,
    }
}

fn compute_trend(reports: &[DriftReport]) -> TrendDirection {
    if reports.len() < 3 {
        return TrendDirection::InsufficientData;
    }

    // Look at the last 10 reports (or fewer)
    let window: Vec<_> = reports.iter().rev().take(10).collect();
    if window.len() < 3 {
        return TrendDirection::InsufficientData;
    }

    // Simple linear regression on drift magnitudes
    let n = window.len() as f64;
    let sum_x: f64 = (0..window.len()).map(|i| i as f64).sum();
    let sum_y: f64 = window.iter().map(|r| r.drift_magnitude).sum();
    let sum_xy: f64 = window.iter().enumerate().map(|(i, r)| i as f64 * r.drift_magnitude).sum();
    let sum_x2: f64 = (0..window.len()).map(|i| (i as f64).powi(2)).sum();

    let denom = n * sum_x2 - sum_x * sum_x;
    if denom.abs() < 1e-10 {
        return TrendDirection::Stable;
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let avg_magnitude = sum_y / n;

    // Determine trend based on slope relative to average
    let relative_slope = if avg_magnitude > 0.0 {
        slope / avg_magnitude
    } else {
        0.0
    };

    if relative_slope > 0.05 {
        TrendDirection::Worsening
    } else if relative_slope < -0.05 {
        TrendDirection::Improving
    } else {
        TrendDirection::Stable
    }
}

fn most_common_severity(severities: &[DriftSeverity]) -> Option<DriftSeverity> {
    if severities.is_empty() {
        return None;
    }
    let mut counts: HashMap<DriftSeverity, usize> = HashMap::new();
    for s in severities {
        *counts.entry(s.clone()).or_insert(0) += 1;
    }
    counts.into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(s, _)| s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate normally-distributed-ish data for testing.
    fn generate_normal_data(n: usize, mean: f64, std: f64) -> Vec<f64> {
        use std::f64::consts::{PI, E};
        let mut data = Vec::with_capacity(n);
        for i in 0..n {
            // Box-Muller transform
            let u1 = (i as f64 + 0.5) / n as f64;
            let u2 = ((i * 7 + 3) % n) as f64 / n as f64;
            let z0 = ((-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos());
            data.push(mean + std * z0);
        }
        data
    }

    #[test]
    fn test_baseline_establishment() {
        let detector = ModelDriftDetector::new();
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let baseline = detector.establish_baseline("model-1", DriftMetric::PredictionDrift, &data);

        assert_eq!(baseline.model_id, "model-1");
        assert_eq!(baseline.metric_type, DriftMetric::PredictionDrift);
        assert!((baseline.reference_mean - 3.0).abs() < 0.01);
        assert!(baseline.reference_std > 0.0);
        assert!(!baseline.reference_data_hash.is_empty());
    }

    #[tokio::test]
    async fn test_drift_detection_no_drift() {
        let detector = ModelDriftDetector::new();
        let baseline_data = generate_normal_data(1000, 0.0, 1.0);
        detector.establish_baseline("model-1", DriftMetric::PredictionDrift, &baseline_data);

        // Same distribution -> no drift
        let current_data = generate_normal_data(1000, 0.0, 1.0);
        let report = detector.detect_drift("model-1", DriftMetric::PredictionDrift, &current_data)
            .await
            .unwrap();

        assert!(report.drift_magnitude < 0.5);
        assert!(report.severity <= DriftSeverity::Low);
    }

    #[tokio::test]
    async fn test_drift_detection_low_drift() {
        let detector = ModelDriftDetector::with_thresholds(DriftThresholds {
            kl_divergence_low: 0.001,
            kl_divergence_medium: 1.0,
            kl_divergence_high: 2.0,
            kl_divergence_critical: 5.0,
            psi_low: 0.001,
            psi_medium: 1.0,
            psi_high: 2.0,
            psi_critical: 5.0,
            z_score_threshold: 2.0,
            max_reports_per_model: 100,
            max_alerts: 50,
        });

        let baseline_data = generate_normal_data(1000, 0.0, 1.0);
        detector.establish_baseline("model-1", DriftMetric::DataDrift, &baseline_data);

        // Slightly shifted distribution
        let current_data = generate_normal_data(1000, 0.3, 1.0);
        let report = detector.detect_drift("model-1", DriftMetric::DataDrift, &current_data)
            .await
            .unwrap();

        assert!(report.drift_magnitude > 0.0);
        assert!(report.current_score != report.baseline_score);
    }

    #[tokio::test]
    async fn test_drift_detection_high_drift() {
        let detector = ModelDriftDetector::with_thresholds(DriftThresholds {
            kl_divergence_low: 0.01,
            kl_divergence_medium: 0.05,
            kl_divergence_high: 0.1,
            kl_divergence_critical: 0.5,
            psi_low: 0.01,
            psi_medium: 0.05,
            psi_high: 0.1,
            psi_critical: 0.5,
            z_score_threshold: 1.0,
            max_reports_per_model: 100,
            max_alerts: 50,
        });

        let baseline_data = generate_normal_data(1000, 0.0, 1.0);
        detector.establish_baseline("model-1", DriftMetric::PerformanceDrift, &baseline_data);

        // Heavily shifted distribution
        let current_data = generate_normal_data(1000, 5.0, 2.0);
        let report = detector.detect_drift("model-1", DriftMetric::PerformanceDrift, &current_data)
            .await
            .unwrap();

        assert!(report.drift_magnitude > 0.1);
        assert!(report.severity >= DriftSeverity::Low);
    }

    #[test]
    fn test_kl_divergence_identical_distributions() {
        let p = vec![0.25, 0.25, 0.25, 0.25];
        let q = vec![0.25, 0.25, 0.25, 0.25];
        let kl = kl_divergence(&p, &q);
        assert!(kl < 0.01); // Should be ~0 for identical distributions
    }

    #[test]
    fn test_kl_divergence_different_distributions() {
        let p = vec![0.9, 0.05, 0.03, 0.02];
        let q = vec![0.25, 0.25, 0.25, 0.25];
        let kl = kl_divergence(&p, &q);
        assert!(kl > 0.0); // Should be positive for different distributions
    }

    #[test]
    fn test_psi_identical_distributions() {
        let p = vec![100.0, 100.0, 100.0, 100.0];
        let q = vec![100.0, 100.0, 100.0, 100.0];
        let psi = compute_psi(&p, &q);
        assert!(psi < 0.01); // Should be ~0 for identical distributions
    }

    #[test]
    fn test_psi_different_distributions() {
        let p = vec![400.0, 30.0, 30.0, 40.0];
        let q = vec![100.0, 100.0, 100.0, 100.0];
        let psi = compute_psi(&p, &q);
        assert!(psi > 0.0);
    }

    #[tokio::test]
    async fn test_alert_generation() {
        let detector = ModelDriftDetector::with_thresholds(DriftThresholds {
            kl_divergence_low: 0.0, // Everything triggers
            kl_divergence_medium: 0.0,
            kl_divergence_high: 0.0,
            kl_divergence_critical: 0.0,
            psi_low: 0.0,
            psi_medium: 0.0,
            psi_high: 0.0,
            psi_critical: 0.0,
            z_score_threshold: 0.0,
            max_reports_per_model: 100,
            max_alerts: 50,
        });

        let baseline_data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        detector.establish_baseline("model-1", DriftMetric::PredictionDrift, &baseline_data);

        let current_data = vec![10.0, 20.0, 30.0, 40.0, 50.0]; // Very different
        let _report = detector.detect_drift("model-1", DriftMetric::PredictionDrift, &current_data)
            .await
            .unwrap();

        let alerts = detector.get_alerts();
        assert!(!alerts.is_empty());
    }

    #[tokio::test]
    async fn test_threshold_updates() {
        let detector = ModelDriftDetector::new();

        let new_thresholds = DriftThresholds {
            kl_divergence_low: 0.5,
            kl_divergence_medium: 1.5,
            kl_divergence_high: 3.0,
            kl_divergence_critical: 5.0,
            psi_low: 0.5,
            psi_medium: 1.5,
            psi_high: 3.0,
            psi_critical: 5.0,
            z_score_threshold: 3.0,
            max_reports_per_model: 500,
            max_alerts: 200,
        };

        detector.update_thresholds(new_thresholds.clone()).await;
        let retrieved = detector.get_thresholds().await;

        assert!((retrieved.kl_divergence_low - 0.5).abs() < 0.01);
        assert!((retrieved.psi_critical - 5.0).abs() < 0.01);
        assert_eq!(retrieved.max_reports_per_model, 500);
    }

    #[tokio::test]
    async fn test_report_history() {
        let detector = ModelDriftDetector::new();
        let baseline_data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        detector.establish_baseline("model-1", DriftMetric::PredictionDrift, &baseline_data);

        // Run multiple drift detections
        for i in 0..5 {
            let shift = (i as f64) * 0.5;
            let current_data: Vec<f64> = baseline_data.iter().map(|x| x + shift).collect();
            let _ = detector.detect_drift("model-1", DriftMetric::PredictionDrift, &current_data).await;
        }

        let reports = detector.get_reports("model-1");
        assert_eq!(reports.len(), 5);
        assert!(reports[4].timestamp >= reports[3].timestamp);
    }
}
