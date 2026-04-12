//! Provider SLA (Service Level Agreement) Tracking for the Xergon relay.
//!
//! Tracks per-provider SLA metrics (uptime %, latency p99, error rate) over
//! rolling windows, compares against configurable targets, and manages status
//! transitions (Compliant -> Warning -> Breached) with alert generation.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tracing::{debug, info, warn};

use chrono::{DateTime, Utc};

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Global SLA configuration with per-metric targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaConfig {
    /// Target uptime percentage (e.g. 99.9)
    pub target_uptime: f64,
    /// Maximum acceptable latency in milliseconds (p99)
    pub max_latency_ms: u64,
    /// Maximum acceptable error rate (e.g. 0.01 for 1%)
    pub max_error_rate: f64,
    /// Rolling window duration for SLA checks
    pub check_window_secs: u64,
    /// Alert threshold: SLA score below this percentage triggers Warning
    pub alert_threshold: f64,
}

impl Default for SlaConfig {
    fn default() -> Self {
        Self {
            target_uptime: 99.9,
            max_latency_ms: 500,
            max_error_rate: 0.01,
            check_window_secs: 3600, // 1 hour
            alert_threshold: 95.0,
        }
    }
}

/// Per-provider SLA config override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSlaConfigOverride {
    pub provider_id: String,
    pub target_uptime: Option<f64>,
    pub max_latency_ms: Option<u64>,
    pub max_error_rate: Option<f64>,
}

// ---------------------------------------------------------------------------
// Status and alert enums
// ---------------------------------------------------------------------------

/// SLA compliance status for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlaStatus {
    /// All metrics within targets
    Compliant,
    /// One or more metrics approaching breach threshold
    Warning,
    /// One or more metrics have breached SLA targets
    Breached,
    /// Not enough data to evaluate
    Unknown,
}

impl std::fmt::Display for SlaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlaStatus::Compliant => write!(f, "compliant"),
            SlaStatus::Warning => write!(f, "warning"),
            SlaStatus::Breached => write!(f, "breached"),
            SlaStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Types of SLA alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlaAlertType {
    UptimeWarning,
    LatencyWarning,
    ErrorRateWarning,
    SLABreach,
}

impl std::fmt::Display for SlaAlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlaAlertType::UptimeWarning => write!(f, "uptime_warning"),
            SlaAlertType::LatencyWarning => write!(f, "latency_warning"),
            SlaAlertType::ErrorRateWarning => write!(f, "error_rate_warning"),
            SlaAlertType::SLABreach => write!(f, "sla_breach"),
        }
    }
}

// ---------------------------------------------------------------------------
// Core data structures
// ---------------------------------------------------------------------------

/// A single data point in the rolling window for a provider.
#[derive(Debug, Clone)]
struct SlaDataPoint {
    timestamp: DateTime<Utc>,
    latency_ms: u64,
    success: bool,
}

/// Per-provider SLA state.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSla {
    pub provider_id: String,
    pub current_uptime: f64,
    pub current_latency_p99: u64,
    pub current_error_rate: f64,
    pub sla_status: SlaStatus,
    pub violations: u32,
    pub last_check: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    /// Number of data points in the current window
    pub sample_count: u64,
    /// Effective config (global defaults or per-provider override)
    pub effective_config: SlaConfig,
}

/// An SLA alert record.
#[derive(Debug, Clone, Serialize)]
pub struct SlaAlert {
    pub id: String,
    pub provider_id: String,
    pub alert_type: SlaAlertType,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub resolved: bool,
}

/// Historical SLA snapshot for trend analysis.
#[derive(Debug, Clone, Serialize)]
pub struct SlaHistoryRecord {
    pub provider_id: String,
    pub timestamp: DateTime<Utc>,
    pub uptime: f64,
    pub latency_p99: u64,
    pub error_rate: f64,
    pub status: SlaStatus,
    pub violations: u32,
}

// ---------------------------------------------------------------------------
// SLA Tracker
// ---------------------------------------------------------------------------

/// Tracks SLA metrics for all providers over rolling windows.
///
/// Internally uses `DashMap` for concurrent access to provider states,
/// and `StdMutex` for the alerts and history collections (accessed less frequently).
#[derive(Clone)]
pub struct SlaTracker {
    /// Global SLA configuration
    config: Arc<StdMutex<SlaConfig>>,
    /// Per-provider overrides
    provider_configs: Arc<DashMap<String, SlaConfig>>,
    /// Per-provider SLA state
    providers: Arc<DashMap<String, ProviderSla>>,
    /// Rolling window data points per provider
    data_points: Arc<DashMap<String, StdMutex<VecDeque<SlaDataPoint>>>>,
    /// Generated alerts
    alerts: Arc<StdMutex<VecDeque<SlaAlert>>>,
    /// Historical SLA snapshots
    history: Arc<StdMutex<VecDeque<SlaHistoryRecord>>>,
    /// Webhook event callback (fire-and-forget)
    webhook_manager: Option<crate::webhook::WebhookManager>,
}

impl SlaTracker {
    /// Create a new SLA tracker with default configuration.
    pub fn new() -> Self {
        Self::with_config(SlaConfig::default())
    }

    /// Create a new SLA tracker with the given global configuration.
    pub fn with_config(config: SlaConfig) -> Self {
        Self {
            config: Arc::new(StdMutex::new(config)),
            provider_configs: Arc::new(DashMap::new()),
            providers: Arc::new(DashMap::new()),
            data_points: Arc::new(DashMap::new()),
            alerts: Arc::new(StdMutex::new(VecDeque::with_capacity(1000))),
            history: Arc::new(StdMutex::new(VecDeque::with_capacity(100_000))),
            webhook_manager: None,
        }
    }

    /// Set the webhook manager for firing events on SLA breach.
    pub fn set_webhook_manager(&mut self, manager: crate::webhook::WebhookManager) {
        self.webhook_manager = Some(manager);
    }

    // ---- Configuration ----

    /// Get the current global SLA configuration.
    pub fn get_config(&self) -> SlaConfig {
        self.config.lock().unwrap().clone()
    }

    /// Update the global SLA configuration.
    pub fn update_config(&self, new_config: SlaConfig) {
        let mut config = self.config.lock().unwrap();
        *config = new_config;
        info!(
            uptime = config.target_uptime,
            latency_ms = config.max_latency_ms,
            error_rate = config.max_error_rate,
            "Global SLA config updated"
        );
    }

    /// Set a per-provider SLA config override.
    pub fn set_provider_config(&self, override_cfg: ProviderSlaConfigOverride) {
        let global = self.config.lock().unwrap().clone();
        let mut provider_config = global;
        if let Some(uptime) = override_cfg.target_uptime {
            provider_config.target_uptime = uptime;
        }
        if let Some(latency) = override_cfg.max_latency_ms {
            provider_config.max_latency_ms = latency;
        }
        if let Some(error_rate) = override_cfg.max_error_rate {
            provider_config.max_error_rate = error_rate;
        }
        self.provider_configs
            .insert(override_cfg.provider_id.clone(), provider_config);
        info!(
            provider_id = %override_cfg.provider_id,
            "Per-provider SLA config updated"
        );
    }

    /// Get per-provider config override (if any).
    pub fn get_provider_config(&self, provider_id: &str) -> Option<SlaConfig> {
        self.provider_configs.get(provider_id).map(|r| r.value().clone())
    }

    /// Remove per-provider config override, reverting to global defaults.
    pub fn remove_provider_config(&self, provider_id: &str) -> bool {
        let removed = self.provider_configs.remove(provider_id).is_some();
        if removed {
            info!(provider_id = %provider_id, "Per-provider SLA config removed, reverting to global");
        }
        removed
    }

    /// Get the effective config for a provider (override or global).
    fn effective_config(&self, provider_id: &str) -> SlaConfig {
        self.provider_configs
            .get(provider_id)
            .map(|r| r.value().clone())
            .unwrap_or_else(|| self.config.lock().unwrap().clone())
    }

    // ---- Data ingestion ----

    /// Record a request outcome for a provider.
    pub fn record(&self, provider_id: &str, latency_ms: u64, success: bool) {
        let now = Utc::now();

        // Ensure data_points entry exists
        self.data_points
            .entry(provider_id.to_string())
            .or_insert_with(|| StdMutex::new(VecDeque::with_capacity(10_000)));

        if let Some(points) = self.data_points.get(provider_id) {
            if let Ok(mut deque) = points.lock() {
                deque.push_back(SlaDataPoint {
                    timestamp: now,
                    latency_ms,
                    success,
                });
            }
        }
    }

    // ---- SLA evaluation ----

    /// Run SLA evaluation for all tracked providers.
    pub fn evaluate_all(&self) {
        let provider_ids: Vec<String> = self.data_points.iter().map(|r| r.key().clone()).collect();
        for pid in provider_ids {
            self.evaluate_provider(&pid);
        }
    }

    /// Run SLA evaluation for a specific provider.
    pub fn evaluate_provider(&self, provider_id: &str) {
        let config = self.effective_config(provider_id);
        let window = Duration::from_secs(config.check_window_secs);
        let now = Utc::now();
        let cutoff = now - chrono::Duration::from_std(window).unwrap_or_else(|_| chrono::Duration::hours(1));

        // Gather data points within the window
        let points: Vec<SlaDataPoint> = self
            .data_points
            .get(provider_id)
            .map(|entry| {
                let mut deque = entry.lock().unwrap();
                // Prune old points
                while deque.front().map_or(false, |p| p.timestamp < cutoff) {
                    deque.pop_front();
                }
                deque.iter().cloned().collect()
            })
            .unwrap_or_default();

        if points.is_empty() {
            // Not enough data — mark as Unknown
            self.providers.insert(
                provider_id.to_string(),
                ProviderSla {
                    provider_id: provider_id.to_string(),
                    current_uptime: 0.0,
                    current_latency_p99: 0,
                    current_error_rate: 0.0,
                    sla_status: SlaStatus::Unknown,
                    violations: 0,
                    last_check: now,
                    window_start: cutoff,
                    sample_count: 0,
                    effective_config: config,
                },
            );
            return;
        }

        let sample_count = points.len() as u64;
        let success_count = points.iter().filter(|p| p.success).count() as u64;
        let error_count = sample_count - success_count;

        // Uptime = success rate as percentage
        let uptime = (success_count as f64 / sample_count as f64) * 100.0;

        // Error rate = errors / total
        let error_rate = error_count as f64 / sample_count as f64;

        // Latency p99: sort and pick the 99th percentile
        let mut latencies: Vec<u64> = points.iter().map(|p| p.latency_ms).collect();
        latencies.sort_unstable();
        let p99_index = ((sample_count as f64 * 0.99).ceil() as usize).saturating_sub(1);
        let latency_p99 = latencies.get(p99_index).copied().unwrap_or(0);

        // Determine status
        let mut is_warning = false;
        let mut is_breach = false;

        // Check uptime
        if uptime < config.target_uptime {
            let uptime_gap = config.target_uptime - uptime;
            // Warning if within 2% of target, breach if further
            if uptime_gap <= 2.0 {
                is_warning = true;
            } else {
                is_breach = true;
            }
        }

        // Check latency
        if latency_p99 > config.max_latency_ms {
            let latency_ratio = latency_p99 as f64 / config.max_latency_ms as f64;
            if latency_ratio <= 2.0 {
                is_warning = true;
            } else {
                is_breach = true;
            }
        }

        // Check error rate
        if error_rate > config.max_error_rate {
            let error_ratio = error_rate / config.max_error_rate;
            if error_ratio <= 2.0 {
                is_warning = true;
            } else {
                is_breach = true;
            }
        }

        let new_status = if is_breach {
            SlaStatus::Breached
        } else if is_warning {
            SlaStatus::Warning
        } else {
            SlaStatus::Compliant
        };

        // Get previous state
        let prev_status = self
            .providers
            .get(provider_id)
            .map(|r| r.value().sla_status)
            .unwrap_or(SlaStatus::Unknown);

        let mut violations = self
            .providers
            .get(provider_id)
            .map(|r| r.value().violations)
            .unwrap_or(0);

        // Generate alerts on status transitions
        if new_status != prev_status {
            match (prev_status, new_status) {
                (_, SlaStatus::Breached) => {
                    violations += 1;
                    self.create_alert(
                        provider_id,
                        SlaAlertType::SLABreach,
                        format!(
                            "Provider {} SLA BREACHED: uptime={:.2}% latency_p99={}ms error_rate={:.2}%",
                            provider_id, uptime, latency_p99, error_rate * 100.0
                        ),
                    );
                    self.fire_webhook(
                        provider_id,
                        "sla.breach",
                        serde_json::json!({
                            "provider_id": provider_id,
                            "uptime": uptime,
                            "latency_p99": latency_p99,
                            "error_rate": error_rate,
                            "violations": violations,
                        }),
                    );
                }
                (_, SlaStatus::Warning) => {
                    // Determine which metric triggered the warning
                    if uptime < config.target_uptime {
                        self.create_alert(
                            provider_id,
                            SlaAlertType::UptimeWarning,
                            format!(
                                "Provider {} uptime WARNING: {:.2}% (target: {}%)",
                                provider_id, uptime, config.target_uptime
                            ),
                        );
                    }
                    if latency_p99 > config.max_latency_ms {
                        self.create_alert(
                            provider_id,
                            SlaAlertType::LatencyWarning,
                            format!(
                                "Provider {} latency WARNING: p99={}ms (max: {}ms)",
                                provider_id, latency_p99, config.max_latency_ms
                            ),
                        );
                    }
                    if error_rate > config.max_error_rate {
                        self.create_alert(
                            provider_id,
                            SlaAlertType::ErrorRateWarning,
                            format!(
                                "Provider {} error rate WARNING: {:.2}% (max: {:.2}%)",
                                provider_id, error_rate * 100.0, config.max_error_rate * 100.0
                            ),
                        );
                    }
                    self.fire_webhook(
                        provider_id,
                        "sla.warning",
                        serde_json::json!({
                            "provider_id": provider_id,
                            "uptime": uptime,
                            "latency_p99": latency_p99,
                            "error_rate": error_rate,
                        }),
                    );
                }
                (SlaStatus::Warning | SlaStatus::Breached, SlaStatus::Compliant) => {
                    // Resolve any open alerts for this provider
                    self.resolve_alerts(provider_id);
                    self.fire_webhook(
                        provider_id,
                        "sla.recovered",
                        serde_json::json!({
                            "provider_id": provider_id,
                            "uptime": uptime,
                            "latency_p99": latency_p99,
                            "error_rate": error_rate,
                        }),
                    );
                }
                _ => {}
            }
        }

        // Update provider SLA state
        self.providers.insert(
            provider_id.to_string(),
            ProviderSla {
                provider_id: provider_id.to_string(),
                current_uptime: uptime,
                current_latency_p99: latency_p99,
                current_error_rate: error_rate,
                sla_status: new_status,
                violations,
                last_check: now,
                window_start: cutoff,
                sample_count,
                effective_config: config,
            },
        );

        // Record history snapshot
        self.record_history(SlaHistoryRecord {
            provider_id: provider_id.to_string(),
            timestamp: now,
            uptime,
            latency_p99,
            error_rate,
            status: new_status,
            violations,
        });
    }

    // ---- Queries ----

    /// Get SLA status for all tracked providers.
    pub fn get_all_status(&self) -> Vec<ProviderSla> {
        self.providers.iter().map(|r| r.value().clone()).collect()
    }

    /// Get SLA status for a specific provider.
    pub fn get_provider_status(&self, provider_id: &str) -> Option<ProviderSla> {
        self.providers.get(provider_id).map(|r| r.value().clone())
    }

    /// Get all alerts, optionally filtered by resolved status.
    pub fn get_alerts(&self, resolved: Option<bool>, limit: usize) -> Vec<SlaAlert> {
        let alerts = self.alerts.lock().unwrap();
        alerts
            .iter()
            .rev()
            .filter(|a| resolved.map_or(true, |r| a.resolved == r))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get historical SLA records for a provider.
    pub fn get_history(&self, provider_id: Option<&str>, days: usize) -> Vec<SlaHistoryRecord> {
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);
        let history = self.history.lock().unwrap();
        history
            .iter()
            .filter(|h| {
                h.timestamp >= cutoff
                    && provider_id.map_or(true, |pid| h.provider_id == pid)
            })
            .cloned()
            .collect()
    }

    // ---- Internal helpers ----

    fn create_alert(&self, provider_id: &str, alert_type: SlaAlertType, message: String) {
        let alert = SlaAlert {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: provider_id.to_string(),
            alert_type,
            message,
            created_at: Utc::now(),
            resolved: false,
        };
        warn!(
            provider_id = %provider_id,
            alert_type = %alert.alert_type,
            "SLA alert generated"
        );
        let mut alerts = self.alerts.lock().unwrap();
        alerts.push_back(alert);
        // Cap at 1000 alerts
        while alerts.len() > 1000 {
            alerts.pop_front();
        }
    }

    fn resolve_alerts(&self, provider_id: &str) {
        let mut alerts = self.alerts.lock().unwrap();
        for alert in alerts.iter_mut() {
            if alert.provider_id == provider_id && !alert.resolved {
                alert.resolved = true;
            }
        }
        info!(provider_id = %provider_id, "SLA alerts resolved");
    }

    fn record_history(&self, record: SlaHistoryRecord) {
        let mut history = self.history.lock().unwrap();
        history.push_back(record);
        // Cap at 100k records
        while history.len() > 100_000 {
            history.pop_front();
        }
    }

    fn fire_webhook(&self, provider_id: &str, event_type: &str, payload: serde_json::Value) {
        if let Some(ref manager) = self.webhook_manager {
            debug!(
                provider_id = %provider_id,
                event = event_type,
                "Firing SLA webhook event"
            );
            manager.emit(event_type, payload);
        }
    }

    /// Prune old data points and history entries.
    pub fn prune(&self) {
        let config = self.config.lock().unwrap();
        let cutoff = Utc::now()
            - chrono::Duration::seconds((config.check_window_secs * 2) as i64);

        // Prune data points
        for entry in self.data_points.iter() {
            let mut deque = entry.lock().unwrap();
            while deque.front().map_or(false, |p| p.timestamp < cutoff) {
                deque.pop_front();
            }
        }

        // Prune history (keep last 90 days)
        let history_cutoff = Utc::now() - chrono::Duration::days(90);
        let mut history = self.history.lock().unwrap();
        while history.front().map_or(false, |h| h.timestamp < history_cutoff) {
            history.pop_front();
        }

        debug!("SLA data pruned");
    }
}

// ---------------------------------------------------------------------------
// Axum admin handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};

use crate::proxy::AppState;

#[derive(Debug, Deserialize)]
pub struct SlaAlertsQuery {
    pub resolved: Option<bool>,
    #[serde(default = "sla_default_limit")]
    pub limit: usize,
}

fn sla_default_limit() -> usize {
    100
}

#[derive(Debug, Deserialize)]
pub struct SlaHistoryQuery {
    pub provider: Option<String>,
    #[serde(default = "sla_default_days")]
    pub days: usize,
}

fn sla_default_days() -> usize {
    30
}

fn verify_admin_key(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected_key = &state.config.admin.api_key;
    let provided = headers
        .get("x-admin-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided.is_empty() || provided != expected_key {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn admin_error(msg: &str, status: StatusCode) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn admin_ok(value: serde_json::Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

/// GET /admin/sla/status -- all providers SLA status
pub async fn sla_status_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    // Run evaluation before returning status
    state.sla_tracker.evaluate_all();

    let all_status = state.sla_tracker.get_all_status();
    let compliant = all_status.iter().filter(|s| s.sla_status == SlaStatus::Compliant).count();
    let warning = all_status.iter().filter(|s| s.sla_status == SlaStatus::Warning).count();
    let breached = all_status.iter().filter(|s| s.sla_status == SlaStatus::Breached).count();
    let unknown = all_status.iter().filter(|s| s.sla_status == SlaStatus::Unknown).count();

    admin_ok(serde_json::json!({
        "providers": all_status,
        "total": all_status.len(),
        "summary": {
            "compliant": compliant,
            "warning": warning,
            "breached": breached,
            "unknown": unknown,
        },
    }))
}

/// GET /admin/sla/providers/{id} -- detailed SLA for one provider
pub async fn sla_provider_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    // Evaluate this provider
    state.sla_tracker.evaluate_provider(&id);

    match state.sla_tracker.get_provider_status(&id) {
        Some(sla) => admin_ok(serde_json::json!({
            "provider": sla,
        })),
        None => admin_error("Provider not found in SLA tracker", StatusCode::NOT_FOUND),
    }
}

/// GET /admin/sla/alerts -- list SLA alerts
pub async fn sla_alerts_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SlaAlertsQuery>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let alerts = state.sla_tracker.get_alerts(params.resolved, params.limit);
    admin_ok(serde_json::json!({
        "alerts": alerts,
        "total": alerts.len(),
    }))
}

/// PATCH /admin/sla/config -- update global SLA config
pub async fn sla_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SlaConfig>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let old = state.sla_tracker.get_config();
    state.sla_tracker.update_config(body.clone());
    info!(
        old_uptime = old.target_uptime,
        new_uptime = body.target_uptime,
        "Global SLA config updated via admin API"
    );

    admin_ok(serde_json::json!({
        "status": "updated",
        "config": body,
    }))
}

/// POST /admin/sla/providers/{id}/config -- set per-provider SLA config
pub async fn sla_provider_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ProviderSlaConfigOverride>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let mut override_cfg = body;
    override_cfg.provider_id = id.clone();
    state.sla_tracker.set_provider_config(override_cfg.clone());

    admin_ok(serde_json::json!({
        "status": "updated",
        "provider_id": id,
        "config": override_cfg,
    }))
}

/// GET /admin/sla/history?provider=&days=30 -- historical SLA data
pub async fn sla_history_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SlaHistoryQuery>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let records = state
        .sla_tracker
        .get_history(params.provider.as_deref(), params.days);

    admin_ok(serde_json::json!({
        "records": records,
        "total": records.len(),
        "days": params.days,
        "provider": params.provider,
    }))
}

/// Build the SLA admin router.
pub fn build_sla_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/sla/status", get(sla_status_handler))
        .route("/admin/sla/providers/{id}", get(sla_provider_handler))
        .route("/admin/sla/alerts", get(sla_alerts_handler))
        .route("/admin/sla/config", patch(sla_config_handler))
        .route("/admin/sla/providers/{id}/config", post(sla_provider_config_handler))
        .route("/admin/sla/history", get(sla_history_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sla_config_default() {
        let config = SlaConfig::default();
        assert!((config.target_uptime - 99.9).abs() < f64::EPSILON);
        assert_eq!(config.max_latency_ms, 500);
        assert!((config.max_error_rate - 0.01).abs() < f64::EPSILON);
        assert_eq!(config.check_window_secs, 3600);
        assert!((config.alert_threshold - 95.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sla_tracker_record_and_evaluate() {
        let tracker = SlaTracker::new();
        let provider_id = "test-provider-1";

        // Record some successful requests
        for _ in 0..100 {
            tracker.record(provider_id, 50, true);
        }

        tracker.evaluate_provider(provider_id);

        let status = tracker.get_provider_status(provider_id).unwrap();
        assert_eq!(status.sla_status, SlaStatus::Compliant);
        assert!(status.current_uptime > 99.0);
        assert!(status.current_latency_p99 <= 50);
        assert!(status.current_error_rate < 0.01);
    }

    #[test]
    fn test_sla_tracker_breach_detection() {
        let tracker = SlaTracker::new();
        let provider_id = "bad-provider";

        // Record many failures (high error rate)
        for _ in 0..100 {
            tracker.record(provider_id, 1000, false);
        }

        tracker.evaluate_provider(provider_id);

        let status = tracker.get_provider_status(provider_id).unwrap();
        assert_eq!(status.sla_status, SlaStatus::Breached);
        assert!(status.current_uptime < 1.0);
        assert!(status.current_error_rate > 0.5);
    }

    #[test]
    fn test_sla_alerts_generated() {
        let tracker = SlaTracker::new();
        let provider_id = "alert-provider";

        // Record all failures to trigger breach
        for _ in 0..50 {
            tracker.record(provider_id, 2000, false);
        }

        tracker.evaluate_provider(provider_id);

        let alerts = tracker.get_alerts(None, 100);
        assert!(!alerts.is_empty());
        assert!(alerts.iter().any(|a| a.alert_type == SlaAlertType::SLABreach));
        assert!(!alerts[0].resolved);
    }

    #[test]
    fn test_sla_alerts_resolved_on_recovery() {
        let tracker = SlaTracker::new();
        let provider_id = "recovering-provider";

        // Breach first
        for _ in 0..50 {
            tracker.record(provider_id, 2000, false);
        }
        tracker.evaluate_provider(provider_id);

        // Then recover with all successes
        for _ in 0..200 {
            tracker.record(provider_id, 30, true);
        }
        tracker.evaluate_provider(provider_id);

        let alerts = tracker.get_alerts(None, 100);
        // The breach alert should now be resolved
        let breach_alerts: Vec<_> = alerts
            .iter()
            .filter(|a| a.alert_type == SlaAlertType::SLABreach)
            .collect();
        assert!(!breach_alerts.is_empty());
        // All breach alerts should be resolved after recovery
        assert!(breach_alerts.iter().all(|a| a.resolved));
    }

    #[test]
    fn test_sla_history_recorded() {
        let tracker = SlaTracker::new();
        let provider_id = "history-provider";

        tracker.record(provider_id, 100, true);
        tracker.evaluate_provider(provider_id);

        let history = tracker.get_history(Some(provider_id), 30);
        assert!(!history.is_empty());
        assert_eq!(history[0].provider_id, provider_id);
    }

    #[test]
    fn test_sla_per_provider_config() {
        let tracker = SlaTracker::new();
        let provider_id = "strict-provider";

        // Set strict config: very low latency target
        tracker.set_provider_config(ProviderSlaConfigOverride {
            provider_id: provider_id.to_string(),
            target_uptime: None,
            max_latency_ms: Some(10), // Very strict
            max_error_rate: None,
        });

        // Record with 50ms latency (would pass global 500ms, fails 10ms)
        for _ in 0..100 {
            tracker.record(provider_id, 50, true);
        }

        tracker.evaluate_provider(provider_id);

        let status = tracker.get_provider_status(provider_id).unwrap();
        // Should be Warning or Breached because 50ms > 10ms target
        assert!(status.sla_status == SlaStatus::Warning || status.sla_status == SlaStatus::Breached);
    }

    #[test]
    fn test_sla_unknown_when_no_data() {
        let tracker = SlaTracker::new();
        let provider_id = "empty-provider";

        tracker.evaluate_provider(provider_id);

        let status = tracker.get_provider_status(provider_id).unwrap();
        assert_eq!(status.sla_status, SlaStatus::Unknown);
        assert_eq!(status.sample_count, 0);
    }

    #[test]
    fn test_sla_status_display() {
        assert_eq!(SlaStatus::Compliant.to_string(), "compliant");
        assert_eq!(SlaStatus::Warning.to_string(), "warning");
        assert_eq!(SlaStatus::Breached.to_string(), "breached");
        assert_eq!(SlaStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_sla_alert_type_display() {
        assert_eq!(SlaAlertType::UptimeWarning.to_string(), "uptime_warning");
        assert_eq!(SlaAlertType::LatencyWarning.to_string(), "latency_warning");
        assert_eq!(SlaAlertType::ErrorRateWarning.to_string(), "error_rate_warning");
        assert_eq!(SlaAlertType::SLABreach.to_string(), "sla_breach");
    }
}
