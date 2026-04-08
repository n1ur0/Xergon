#![allow(dead_code)]
//! Health Monitor V2 — Deep health monitoring with dependency tracking,
//! consecutive failure detection, recovery detection, and check history.
//!
//! REST endpoints:
//! - GET  /v1/health/v2             — aggregate health status
//! - GET  /v1/health/v2/checks      — all individual checks
//! - GET  /v1/health/v2/dependencies — dependency health
//! - POST /v1/health/v2/check/{name} — trigger a specific check
//! - GET  /v1/health/v2/history      — check history
//! - GET  /v1/health/v2/config       — current configuration

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums & Structs
// ---------------------------------------------------------------------------

/// Overall health status of a component or check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// A single health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub check_id: String,
    pub name: String,
    pub component: String,
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub last_checked: DateTime<Utc>,
    pub details: String,
    pub error: Option<String>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
}

/// Dependency health tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyHealth {
    pub name: String,
    pub url: String,
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub timeout_ms: u64,
    pub interval_secs: u64,
    pub last_checked: DateTime<Utc>,
    pub consecutive_failures: u32,
}

/// Configuration for health monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub check_interval: u64,
    pub timeout_ms: u64,
    pub consecutive_failures_threshold: u32,
    pub recovery_threshold: u32,
    pub history_limit: usize,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: 30,
            timeout_ms: 5000,
            consecutive_failures_threshold: 3,
            recovery_threshold: 2,
            history_limit: 1000,
        }
    }
}

/// A history entry for a health check.
#[derive(Debug, Clone, Serialize)]
pub struct HealthHistoryEntry {
    pub check_name: String,
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub timestamp: DateTime<Utc>,
    pub error: Option<String>,
}

/// Request body for registering a dependency.
#[derive(Debug, Deserialize)]
pub struct RegisterDependencyRequest {
    pub name: String,
    pub url: String,
    #[serde(default = "default_dep_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_dep_interval")]
    pub interval_secs: u64,
}

fn default_dep_timeout() -> u64 {
    5000
}

fn default_dep_interval() -> u64 {
    30
}

/// Request body for updating config.
#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub check_interval: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub consecutive_failures_threshold: Option<u32>,
    pub recovery_threshold: Option<u32>,
}

// ---------------------------------------------------------------------------
// HealthMonitorV2
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HealthMonitorV2 {
    /// check name -> HealthCheck
    checks: Arc<DashMap<String, HealthCheck>>,
    /// dependency name -> DependencyHealth
    dependencies: Arc<DashMap<String, DependencyHealth>>,
    /// Check history ring buffer
    history: Arc<DashMap<String, VecDeque<HealthHistoryEntry>>>,
    /// Configuration
    config: Arc<std::sync::RwLock<HealthConfig>>,
    /// Global check counter for generating IDs
    check_counter: Arc<AtomicU64>,
    /// Forced status override (if set, overrides aggregate)
    forced_status: Arc<std::sync::RwLock<Option<HealthStatus>>>,
}

impl Default for HealthMonitorV2 {
    fn default() -> Self {
        Self::new(HealthConfig::default())
    }
}

impl HealthMonitorV2 {
    pub fn new(config: HealthConfig) -> Self {
        Self {
            checks: Arc::new(DashMap::new()),
            dependencies: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(config)),
            check_counter: Arc::new(AtomicU64::new(0)),
            forced_status: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    // -- Configuration ------------------------------------------------------

    /// Get the current configuration.
    pub fn get_config(&self) -> HealthConfig {
        self.config.read().unwrap().clone()
    }

    /// Update the configuration.
    pub fn update_config(&self, update: UpdateConfigRequest) {
        let mut cfg = self.config.write().unwrap();
        if let Some(v) = update.check_interval {
            cfg.check_interval = v;
        }
        if let Some(v) = update.timeout_ms {
            cfg.timeout_ms = v;
        }
        if let Some(v) = update.consecutive_failures_threshold {
            cfg.consecutive_failures_threshold = v;
        }
        if let Some(v) = update.recovery_threshold {
            cfg.recovery_threshold = v;
        }
    }

    // -- Forced status ------------------------------------------------------

    /// Force the monitor into a healthy state.
    pub fn force_healthy(&self) {
        *self.forced_status.write().unwrap() = Some(HealthStatus::Healthy);
        info!("HealthMonitorV2 forced to Healthy");
    }

    /// Force the monitor into an unhealthy state.
    pub fn force_unhealthy(&self) {
        *self.forced_status.write().unwrap() = Some(HealthStatus::Unhealthy);
        warn!("HealthMonitorV2 forced to Unhealthy");
    }

    /// Clear any forced status override.
    pub fn clear_forced_status(&self) {
        *self.forced_status.write().unwrap() = None;
    }

    // -- Health checks ------------------------------------------------------

    /// Run a check and record the result. Returns the updated HealthCheck.
    pub fn run_check(
        &self,
        name: &str,
        component: &str,
        healthy: bool,
        latency_ms: u64,
        details: &str,
        error: Option<&str>,
    ) -> HealthCheck {
        let cfg = self.get_config();
        let check_id = format!("chk-{}", self.check_counter.fetch_add(1, Ordering::Relaxed));
        let now = Utc::now();
        let status = if healthy {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        };

        // Get or create the check entry
        let entry = self
            .checks
            .entry(name.to_string())
            .or_insert_with(|| HealthCheck {
                check_id: check_id.clone(),
                name: name.to_string(),
                component: component.to_string(),
                status: HealthStatus::Unknown,
                latency_ms: 0,
                last_checked: now,
                details: String::new(),
                error: None,
                consecutive_failures: 0,
                consecutive_successes: 0,
            });

        let mut check = entry.value().clone();
        check.check_id = check_id;
        check.last_checked = now;
        check.latency_ms = latency_ms;
        check.details = details.to_string();
        check.error = error.map(|s| s.to_string());

        if healthy {
            check.consecutive_successes += 1;
            check.consecutive_failures = 0;
            // Check for recovery
            if check.status == HealthStatus::Unhealthy
                && check.consecutive_successes >= cfg.recovery_threshold
            {
                check.status = HealthStatus::Healthy;
                info!(name = %name, "Check recovered to Healthy");
            } else if check.status == HealthStatus::Unknown {
                check.status = HealthStatus::Healthy;
            }
        } else {
            check.consecutive_failures += 1;
            check.consecutive_successes = 0;
            check.status = HealthStatus::Unhealthy;
            if check.consecutive_failures >= cfg.consecutive_failures_threshold {
                warn!(
                    name = %name,
                    failures = check.consecutive_failures,
                    "Check exceeded consecutive failure threshold"
                );
            }
        }

        // Update the check
        *self.checks.get_mut(name).unwrap() = check.clone();

        // Append to history
        self.append_history(name, &status, latency_ms, now, error);

        check
    }

    /// Get a specific check by name.
    pub fn get_check(&self, name: &str) -> Option<HealthCheck> {
        self.checks.get(name).map(|v| v.clone())
    }

    /// Get all checks.
    pub fn get_all_checks(&self) -> Vec<HealthCheck> {
        self.checks.iter().map(|v| v.value().clone()).collect()
    }

    // -- Dependency management ----------------------------------------------

    /// Register a dependency for health tracking.
    pub fn register_dependency(&self, name: &str, url: &str, timeout_ms: u64, interval_secs: u64) {
        let dep = DependencyHealth {
            name: name.to_string(),
            url: url.to_string(),
            status: HealthStatus::Unknown,
            latency_ms: 0,
            timeout_ms,
            interval_secs,
            last_checked: Utc::now(),
            consecutive_failures: 0,
        };
        self.dependencies.insert(name.to_string(), dep);
        info!(name = %name, url = %url, "Dependency registered");
    }

    /// Update a dependency's health status.
    pub fn update_dependency_status(
        &self,
        name: &str,
        healthy: bool,
        latency_ms: u64,
    ) -> bool {
        if let Some(mut dep) = self.dependencies.get_mut(name) {
            dep.last_checked = Utc::now();
            dep.latency_ms = latency_ms;
            if healthy {
                dep.status = HealthStatus::Healthy;
                dep.consecutive_failures = 0;
            } else {
                dep.consecutive_failures += 1;
                dep.status = HealthStatus::Unhealthy;
            }
            return true;
        }
        false
    }

    /// Get health info for all dependencies.
    pub fn get_dependency_health(&self) -> Vec<DependencyHealth> {
        self.dependencies.iter().map(|v| v.value().clone()).collect()
    }

    /// Get health info for a specific dependency.
    pub fn get_dependency(&self, name: &str) -> Option<DependencyHealth> {
        self.dependencies.get(name).map(|v| v.value().clone())
    }

    // -- Aggregate status ---------------------------------------------------

    /// Get the aggregate health status.
    pub fn get_status(&self) -> HealthStatus {
        // Check forced override first
        if let Some(ref forced) = *self.forced_status.read().unwrap() {
            return forced.clone();
        }

        let checks = self.get_all_checks();
        let deps = self.get_dependency_health();

        let all: Vec<&HealthStatus> = checks.iter().map(|c| &c.status).collect();
        let dep_statuses: Vec<&HealthStatus> = deps.iter().map(|d| &d.status).collect();
        let mut combined: Vec<&HealthStatus> = Vec::with_capacity(all.len() + dep_statuses.len());
        for s in all.iter().chain(dep_statuses.iter()) {
            combined.push(*s);
        }

        if combined.is_empty() {
            return HealthStatus::Unknown;
        }

        let unhealthy_count = combined.iter().filter(|s| matches!(s, HealthStatus::Unhealthy)).count();
        let degraded_count = combined.iter().filter(|s| matches!(s, HealthStatus::Degraded)).count();

        if unhealthy_count > 0 {
            HealthStatus::Unhealthy
        } else if degraded_count > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    // -- History ------------------------------------------------------------

    /// Get history entries for a specific check.
    pub fn get_history(&self, name: &str, limit: usize) -> Vec<HealthHistoryEntry> {
        self.history
            .get(name)
            .map(|h| {
                h.iter()
                    .rev()
                    .take(limit)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all history entries across all checks.
    pub fn get_all_history(&self, limit: usize) -> Vec<HealthHistoryEntry> {
        let mut all: Vec<HealthHistoryEntry> = Vec::new();
        for entry in self.history.iter() {
            for item in entry.value().iter() {
                all.push(item.clone());
            }
        }
        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all.truncate(limit);
        all
    }

    // -- Internal -----------------------------------------------------------

    fn append_history(
        &self,
        name: &str,
        status: &HealthStatus,
        latency_ms: u64,
        timestamp: DateTime<Utc>,
        error: Option<&str>,
    ) {
        let cfg = self.get_config();
        let entry = HealthHistoryEntry {
            check_name: name.to_string(),
            status: status.clone(),
            latency_ms,
            timestamp,
            error: error.map(|s| s.to_string()),
        };

        let mut hist = self
            .history
            .entry(name.to_string())
            .or_insert_with(VecDeque::new);
        hist.push_back(entry);
        while hist.len() > cfg.history_limit {
            hist.pop_front();
        }
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_health_v2_router() -> Router<AppState> {
    Router::new()
        .route("/v1/health/v2", get(health_v2_status_handler))
        .route("/v1/health/v2/checks", get(health_v2_checks_handler))
        .route(
            "/v1/health/v2/dependencies",
            get(health_v2_dependencies_handler),
        )
        .route(
            "/v1/health/v2/check/{name}",
            post(health_v2_run_check_handler),
        )
        .route("/v1/health/v2/history", get(health_v2_history_handler))
        .route(
            "/v1/health/v2/config",
            get(health_v2_config_handler),
        )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/health/v2
async fn health_v2_status_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let status = state.health_monitor_v2.get_status();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": status,
            "timestamp": Utc::now(),
        })),
    )
}

/// GET /v1/health/v2/checks
async fn health_v2_checks_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let checks = state.health_monitor_v2.get_all_checks();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "checks": checks })),
    )
}

/// GET /v1/health/v2/dependencies
async fn health_v2_dependencies_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let deps = state.health_monitor_v2.get_dependency_health();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "dependencies": deps })),
    )
}

/// POST /v1/health/v2/check/{name}
async fn health_v2_run_check_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Run a synthetic check (component name = check name, healthy = true, 0ms latency)
    let check = state.health_monitor_v2.run_check(&name, &name, true, 0, "manual trigger", None);
    (
        StatusCode::OK,
        Json(serde_json::json!(check)),
    )
}

/// GET /v1/health/v2/history
async fn health_v2_history_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let history = state.health_monitor_v2.get_all_history(100);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "history": history })),
    )
}

/// GET /v1/health/v2/config
async fn health_v2_config_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let config = state.health_monitor_v2.get_config();
    (StatusCode::OK, Json(serde_json::json!(config)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> HealthMonitorV2 {
        HealthMonitorV2::new(HealthConfig::default())
    }

    #[test]
    fn test_run_check_healthy() {
        let hm = setup();
        let check = hm.run_check("db", "database", true, 5, "ok", None);
        assert_eq!(check.status, HealthStatus::Healthy);
        assert_eq!(check.consecutive_successes, 1);
        assert_eq!(check.consecutive_failures, 0);
    }

    #[test]
    fn test_run_check_unhealthy() {
        let hm = setup();
        let check = hm.run_check("db", "database", false, 5000, "timeout", Some("connection refused"));
        assert_eq!(check.status, HealthStatus::Unhealthy);
        assert_eq!(check.consecutive_failures, 1);
        assert!(check.error.is_some());
    }

    #[test]
    fn test_consecutive_failures() {
        let hm = setup();
        for _ in 0..5 {
            hm.run_check("api", "api_server", false, 1000, "down", Some("error"));
        }
        let check = hm.get_check("api").unwrap();
        assert_eq!(check.consecutive_failures, 5);
        assert_eq!(check.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_recovery_detection() {
        let hm = setup();
        // First fail 3 times
        for _ in 0..3 {
            hm.run_check("svc", "service", false, 1000, "down", None);
        }
        // Then succeed 2 times (recovery_threshold = 2)
        hm.run_check("svc", "service", true, 5, "ok", None);
        let check = hm.get_check("svc").unwrap();
        assert_eq!(check.status, HealthStatus::Unhealthy); // not yet recovered
        hm.run_check("svc", "service", true, 5, "ok", None);
        let check = hm.get_check("svc").unwrap();
        assert_eq!(check.status, HealthStatus::Healthy); // recovered!
    }

    #[test]
    fn test_get_all_checks() {
        let hm = setup();
        hm.run_check("a", "comp_a", true, 1, "ok", None);
        hm.run_check("b", "comp_b", false, 100, "fail", None);
        let checks = hm.get_all_checks();
        assert_eq!(checks.len(), 2);
    }

    #[test]
    fn test_register_and_get_dependency() {
        let hm = setup();
        hm.register_dependency("redis", "http://redis:6379", 3000, 15);
        let dep = hm.get_dependency("redis").unwrap();
        assert_eq!(dep.name, "redis");
        assert_eq!(dep.url, "http://redis:6379");
        assert_eq!(dep.status, HealthStatus::Unknown);
    }

    #[test]
    fn test_dependency_status_update() {
        let hm = setup();
        hm.register_dependency("pg", "http://pg:5432", 5000, 30);
        hm.update_dependency_status("pg", true, 12);
        let dep = hm.get_dependency("pg").unwrap();
        assert_eq!(dep.status, HealthStatus::Healthy);
        assert_eq!(dep.latency_ms, 12);
    }

    #[test]
    fn test_aggregate_status_healthy() {
        let hm = setup();
        hm.run_check("a", "a", true, 1, "ok", None);
        hm.run_check("b", "b", true, 2, "ok", None);
        assert_eq!(hm.get_status(), HealthStatus::Healthy);
    }

    #[test]
    fn test_aggregate_status_unhealthy() {
        let hm = setup();
        hm.run_check("a", "a", true, 1, "ok", None);
        hm.run_check("b", "b", false, 100, "fail", None);
        assert_eq!(hm.get_status(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_forced_status() {
        let hm = setup();
        hm.run_check("a", "a", true, 1, "ok", None);
        assert_eq!(hm.get_status(), HealthStatus::Healthy);
        hm.force_unhealthy();
        assert_eq!(hm.get_status(), HealthStatus::Unhealthy);
        hm.force_healthy();
        assert_eq!(hm.get_status(), HealthStatus::Healthy);
        hm.clear_forced_status();
        assert_eq!(hm.get_status(), HealthStatus::Healthy);
    }
}
