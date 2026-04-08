//! Model Health Auto-Detection
//!
//! Periodically probes inference models to detect failures and auto-recover.
//! Tracks per-model health state including consecutive failures/successes,
//! average latency, and error rates.
//!
//! A model is marked Unhealthy after `failure_threshold` consecutive probe
//! failures, and recovered to Healthy after `recovery_threshold` consecutive
//! successes.
//!
//! API endpoints:
//! - GET  /api/models/health            -- all models health
//! - GET  /api/models/health/{model}    -- single model health
//! - POST /api/models/health/{model}/check -- trigger manual check

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Health status of a single model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Per-model health state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHealth {
    pub model_name: String,
    pub status: HealthStatus,
    pub last_check: String, // ISO 8601 timestamp
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
    pub last_error: Option<String>,
    /// Timestamp when the status last changed.
    pub status_since: String,
    /// Total number of health checks performed.
    pub total_checks: u64,
}

/// Result of a manual or scheduled health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub model: String,
    pub healthy: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
    pub status_before: HealthStatus,
    pub status_after: HealthStatus,
}

/// Summary of all models' health (returned by /api/models/health).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub total_models: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub unhealthy: usize,
    pub unknown: usize,
    pub models: Vec<ModelHealth>,
}

/// Configuration for the health monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMonitorConfig {
    /// Seconds between automatic health checks (default: 60).
    pub check_interval_secs: u64,
    /// Consecutive failures before marking Unhealthy (default: 3).
    pub failure_threshold: u32,
    /// Consecutive successes before marking Healthy (default: 5).
    pub recovery_threshold: u32,
    /// Probe timeout in seconds (default: 30).
    pub probe_timeout_secs: u64,
    /// Number of latency samples to keep for averaging (default: 10).
    pub latency_window: usize,
    /// Number of check results to keep for error rate calculation (default: 20).
    pub error_rate_window: usize,
}

impl Default for HealthMonitorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60,
            failure_threshold: 3,
            recovery_threshold: 5,
            probe_timeout_secs: 30,
            latency_window: 10,
            error_rate_window: 20,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal model health tracking
// ---------------------------------------------------------------------------

/// Internal mutable state for a single model's health.
struct InternalModelHealth {
    status: HealthStatus,
    last_check: Instant,
    consecutive_failures: u32,
    consecutive_successes: u32,
    last_error: Option<String>,
    status_since: Instant,
    total_checks: u64,
    /// Recent latency samples (ms).
    latency_samples: VecDeque<f64>,
    /// Recent check results (true = success, false = failure).
    check_results: VecDeque<bool>,
}

impl InternalModelHealth {
    fn new() -> Self {
        Self {
            status: HealthStatus::Unknown,
            last_check: Instant::now(),
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_error: None,
            status_since: Instant::now(),
            total_checks: 0,
            latency_samples: VecDeque::new(),
            check_results: VecDeque::new(),
        }
    }

    /// Compute average latency from samples.
    fn avg_latency_ms(&self) -> f64 {
        if self.latency_samples.is_empty() {
            return 0.0;
        }
        self.latency_samples.iter().sum::<f64>() / self.latency_samples.len() as f64
    }

    /// Compute error rate from recent check results.
    fn error_rate(&self) -> f64 {
        if self.check_results.is_empty() {
            return 0.0;
        }
        let failures = self.check_results.iter().filter(|&&ok| !ok).count();
        failures as f64 / self.check_results.len() as f64
    }

    /// Record a check result and update state.
    fn record_check(
        &mut self,
        success: bool,
        latency_ms: f64,
        error: Option<String>,
        config: &HealthMonitorConfig,
    ) -> HealthStatus {
        self.last_check = Instant::now();
        self.total_checks += 1;

        // Update latency window
        self.latency_samples.push_back(latency_ms);
        if self.latency_samples.len() > config.latency_window {
            self.latency_samples.pop_front();
        }

        // Update check results window
        self.check_results.push_back(success);
        if self.check_results.len() > config.error_rate_window {
            self.check_results.pop_front();
        }

        if success {
            self.consecutive_successes += 1;
            self.consecutive_failures = 0;
            self.last_error = None;

            // Recovery check
            if self.status == HealthStatus::Unhealthy
                && self.consecutive_successes >= config.recovery_threshold
            {
                self.status = HealthStatus::Healthy;
                self.status_since = Instant::now();
                info!(
                    model = self.model_name(),
                    "Model recovered to Healthy status"
                );
            } else if self.status == HealthStatus::Degraded {
                // Degraded models recover to Healthy after 2 consecutive successes
                if self.consecutive_successes >= 2 {
                    self.status = HealthStatus::Healthy;
                    self.status_since = Instant::now();
                }
            }
        } else {
            self.consecutive_failures += 1;
            self.consecutive_successes = 0;
            self.last_error = error;

            // Failure check
            if self.status == HealthStatus::Healthy
                && self.consecutive_failures >= config.failure_threshold
            {
                self.status = HealthStatus::Unhealthy;
                self.status_since = Instant::now();
                warn!(
                    model = self.model_name(),
                    failures = self.consecutive_failures,
                    "Model marked Unhealthy"
                );
            } else if self.status == HealthStatus::Healthy && self.consecutive_failures == 1 {
                // First failure: mark Degraded
                self.status = HealthStatus::Degraded;
                self.status_since = Instant::now();
            }
        }

        self.status
    }

    fn model_name(&self) -> &str {
        // This is a placeholder; the real name is stored in the DashMap key
        "unknown"
    }
}

// ---------------------------------------------------------------------------
// ModelHealthMonitor
// ---------------------------------------------------------------------------

/// Monitors the health of loaded inference models via periodic probes.
pub struct ModelHealthMonitor {
    models: DashMap<String, RwLock<InternalModelHealth>>,
    config: HealthMonitorConfig,
    http_client: Client,
    /// Backend URL for probing (e.g. http://127.0.0.1:11434).
    backend_url: String,
    /// Background task cancellation flag.
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl ModelHealthMonitor {
    /// Create a new health monitor.
    ///
    /// `backend_url` is the base URL of the inference backend (e.g. Ollama).
    pub fn new(backend_url: &str, config: HealthMonitorConfig) -> Self {
        Self {
            models: DashMap::new(),
            config,
            http_client: Client::new(),
            backend_url: backend_url.trim_end_matches('/').to_string(),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Register a model for health monitoring.
    pub fn register_model(&self, model_name: &str) {
        self.models
            .entry(model_name.to_string())
            .or_insert_with(|| RwLock::new(InternalModelHealth::new()));
        info!(model = model_name, "Model registered for health monitoring");
    }

    /// Unregister a model from health monitoring.
    pub fn unregister_model(&self, model_name: &str) {
        if self.models.remove(model_name).is_some() {
            info!(model = model_name, "Model unregistered from health monitoring");
        }
    }

    /// Run a health check on a single model.
    ///
    /// Sends a minimal inference request to the backend and measures latency.
    pub async fn check_model(&self, model_name: &str) -> HealthCheckResult {
        let health = self.models
            .entry(model_name.to_string())
            .or_insert_with(|| RwLock::new(InternalModelHealth::new()));

        let status_before = {
            let h = health.read().await;
            h.status
        };

        let start = Instant::now();
        let probe_url = format!("{}/v1/chat/completions", self.backend_url);

        let probe_body = serde_json::json!({
            "model": model_name,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1,
            "stream": false,
        });

        let result = self
            .http_client
            .post(&probe_url)
            .timeout(Duration::from_secs(self.config.probe_timeout_secs))
            .header("Content-Type", "application/json")
            .json(&probe_body)
            .send()
            .await;

        let (success, latency_ms, error) = match result {
            Ok(resp) if resp.status().is_success() => {
                let latency = start.elapsed().as_millis() as u64;
                (true, latency, None)
            }
            Ok(resp) => {
                let latency = start.elapsed().as_millis() as u64;
                let status = resp.status();
                let error_msg = format!("Backend returned HTTP {}", status);
                (false, latency, Some(error_msg))
            }
            Err(e) => {
                let latency = start.elapsed().as_millis() as u64;
                (false, latency, Some(format!("Probe failed: {}", e)))
            }
        };

        let status_after = {
            let mut h = health.write().await;
            h.record_check(success, latency_ms as f64, error.clone(), &self.config)
        };

        HealthCheckResult {
            model: model_name.to_string(),
            healthy: success,
            latency_ms,
            error,
            status_before,
            status_after,
        }
    }

    /// Get health info for a single model.
    pub async fn get_health(&self, model_name: &str) -> Option<ModelHealth> {
        let entry = self.models.get(model_name)?;
        let h = entry.value().read().await;
        Some(self.to_public_health(model_name, &h))
    }

    /// Get health summary for all monitored models.
    pub async fn get_all_health(&self) -> HealthSummary {
        let mut models = Vec::new();
        let mut healthy = 0usize;
        let mut degraded = 0usize;
        let mut unhealthy = 0usize;
        let mut unknown = 0usize;

        for entry in self.models.iter() {
            let model_name = entry.key().clone();
            let h = entry.value().read().await;
            match h.status {
                HealthStatus::Healthy => healthy += 1,
                HealthStatus::Degraded => degraded += 1,
                HealthStatus::Unhealthy => unhealthy += 1,
                HealthStatus::Unknown => unknown += 1,
            }
            models.push(self.to_public_health(&model_name, &h));
        }

        HealthSummary {
            total_models: models.len(),
            healthy,
            degraded,
            unhealthy,
            unknown,
            models,
        }
    }

    /// Convert internal state to public API type.
    fn to_public_health(&self, model_name: &str, h: &InternalModelHealth) -> ModelHealth {
        ModelHealth {
            model_name: model_name.to_string(),
            status: h.status,
            last_check: chrono::Utc::now().timestamp_millis().to_string(),
            consecutive_failures: h.consecutive_failures,
            consecutive_successes: h.consecutive_successes,
            avg_latency_ms: h.avg_latency_ms(),
            error_rate: h.error_rate(),
            last_error: h.last_error.clone(),
            status_since: chrono::Utc::now().timestamp_millis().to_string(),
            total_checks: h.total_checks,
        }
    }

    /// Start the background health check loop.
    ///
    /// Spawns a tokio task that periodically checks all registered models.
    /// Call `stop()` to terminate.
    pub fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::Relaxed) {
            warn!("Health monitor is already running");
            return;
        }
        info!(
            interval_secs = self.config.check_interval_secs,
            "Starting model health monitor"
        );

        let monitor = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                if !monitor.running.load(Ordering::Relaxed) {
                    break;
                }

                // Collect model names to check
                let model_names: Vec<String> = {
                    monitor
                        .models
                        .iter()
                        .map(|entry| entry.key().clone())
                        .collect()
                };

                // Check each model
                for model_name in &model_names {
                    let result = monitor.check_model(model_name).await;
                    if !result.healthy {
                        warn!(
                            model = %model_name,
                            error = ?result.error,
                            "Health check failed"
                        );
                    }
                }

                // Sleep until next check
                tokio::time::sleep(Duration::from_secs(monitor.config.check_interval_secs)).await;
            }
            info!("Model health monitor stopped");
        });
    }

    /// Stop the background health check loop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        info!("Model health monitor stop requested");
    }

    /// Check if the background loop is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get the list of registered model names.
    pub fn registered_models(&self) -> Vec<String> {
        self.models.iter().map(|e| e.key().clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> HealthMonitorConfig {
        HealthMonitorConfig {
            check_interval_secs: 60,
            failure_threshold: 3,
            recovery_threshold: 5,
            probe_timeout_secs: 5,
            latency_window: 5,
            error_rate_window: 10,
        }
    }

    #[tokio::test]
    async fn test_register_and_get_health() {
        let monitor = ModelHealthMonitor::new("http://localhost:11434", test_config());
        monitor.register_model("test-model");
        let health = monitor.get_health("test-model").await.unwrap();
        assert_eq!(health.status, HealthStatus::Unknown);
        assert_eq!(health.total_checks, 0);
    }

    #[tokio::test]
    async fn test_get_all_health_empty() {
        let monitor = ModelHealthMonitor::new("http://localhost:11434", test_config());
        let summary = monitor.get_all_health().await;
        assert_eq!(summary.total_models, 0);
    }

    #[tokio::test]
    async fn test_internal_health_state_transitions() {
        let config = HealthMonitorConfig {
            failure_threshold: 2,
            recovery_threshold: 2,
            ..test_config()
        };
        let mut state = InternalModelHealth::new();

        // First failure -> Degraded
        let s = state.record_check(false, 100.0, Some("err".into()), &config);
        assert_eq!(s, HealthStatus::Degraded);

        // Second failure -> Unhealthy (threshold=2)
        let s = state.record_check(false, 100.0, Some("err".into()), &config);
        assert_eq!(s, HealthStatus::Unhealthy);

        // First success -> still Unhealthy (need 2)
        let s = state.record_check(true, 50.0, None, &config);
        assert_eq!(s, HealthStatus::Unhealthy);

        // Second success -> Healthy (recovery_threshold=2)
        let s = state.record_check(true, 50.0, None, &config);
        assert_eq!(s, HealthStatus::Healthy);
    }

    #[test]
    fn test_avg_latency_and_error_rate() {
        let config = test_config();
        let mut state = InternalModelHealth::new();

        state.record_check(true, 100.0, None, &config);
        state.record_check(true, 200.0, None, &config);
        state.record_check(false, 300.0, Some("err".into()), &config);

        assert!((state.avg_latency_ms() - 200.0).abs() < 0.01);
        assert!((state.error_rate() - (1.0 / 3.0)).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_unregister_model() {
        let monitor = ModelHealthMonitor::new("http://localhost:11434", test_config());
        monitor.register_model("temp-model");
        assert!(monitor.get_health("temp-model").await.is_some());
        monitor.unregister_model("temp-model");
        assert!(monitor.get_health("temp-model").await.is_none());
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(HealthStatus::Unknown.to_string(), "unknown");
    }
}
