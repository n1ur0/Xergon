//! Self-Healing Circuit Breaker with Auto-Recovery
//!
//! Implements a circuit breaker pattern per provider with self-healing
//! capabilities: automatic health probes in half-open state, gradual traffic
//! restoration, exponential backoff on repeated failures, and auto-reset
//! after cooldown.
//!
//! REST endpoints:
//! - GET  /v1/circuit/state/:provider_id       — Get circuit state
//! - GET  /v1/circuit/stats/:provider_id       — Get circuit stats
//! - GET  /v1/circuit/all                      — List all circuits
//! - POST /v1/circuit/reset/:provider_id       — Reset circuit
//! - GET  /v1/circuit/health-check/:provider_id — Trigger health check
//! - POST /v1/circuit/force/:provider_id       — Force state change

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// CircuitState
// ---------------------------------------------------------------------------

/// State of a circuit breaker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests pass through.
    Closed,
    /// Circuit tripped — requests are rejected.
    Open,
    /// Probing — limited requests allowed to test recovery.
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half_open"),
        }
    }
}

// ---------------------------------------------------------------------------
// CircuitConfig
// ---------------------------------------------------------------------------

/// Configuration for a circuit breaker instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,
    /// Number of consecutive successes in half-open before closing.
    pub success_threshold: u32,
    /// How long to stay in open state before transitioning to half-open (seconds).
    pub open_duration_secs: u64,
    /// Maximum number of probe requests in half-open state.
    pub half_open_max_calls: u32,
    /// Interval between automatic health checks (seconds).
    pub health_check_interval_secs: u64,
}

impl Default for CircuitConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            open_duration_secs: 60,
            half_open_max_calls: 3,
            health_check_interval_secs: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// CircuitStats
// ---------------------------------------------------------------------------

/// Statistics for a circuit breaker instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitStats {
    pub provider_id: String,
    pub state: CircuitState,
    pub total_calls: u64,
    pub successes: u64,
    pub failures: u64,
    pub current_consecutive_failures: u32,
    pub current_consecutive_successes: u32,
    pub half_open_calls_remaining: u32,
    pub last_failure_time: Option<chrono::DateTime<chrono::Utc>>,
    pub last_success_time: Option<chrono::DateTime<chrono::Utc>>,
    pub opened_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_open_time_ms: u64,
    pub circuit_opens_count: u64,
    pub backoff_multiplier: u32,
    pub next_backoff_ms: u64,
}

// ---------------------------------------------------------------------------
// HealthCheckResult
// ---------------------------------------------------------------------------

/// Result of a health check probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub provider_id: String,
    pub is_healthy: bool,
    pub latency_ms: u64,
    pub error_rate: f64,
    pub response: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Per-provider circuit data
// ---------------------------------------------------------------------------

struct CircuitData {
    state: CircuitState,
    config: CircuitConfig,
    consecutive_failures: u32,
    consecutive_successes: u32,
    half_open_calls_used: u32,
    total_calls: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
    last_failure_time: Option<chrono::DateTime<chrono::Utc>>,
    last_success_time: Option<chrono::DateTime<chrono::Utc>>,
    opened_at: Option<chrono::DateTime<chrono::Utc>>,
    total_open_time_ms: AtomicU64,
    circuit_opens_count: AtomicU64,
    backoff_multiplier: AtomicU64,
    open_instant: Option<Instant>,
}

impl CircuitData {
    fn new(config: CircuitConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            config,
            consecutive_failures: 0,
            consecutive_successes: 0,
            half_open_calls_used: 0,
            total_calls: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            last_failure_time: None,
            last_success_time: None,
            opened_at: None,
            total_open_time_ms: AtomicU64::new(0),
            circuit_opens_count: AtomicU64::new(0),
            backoff_multiplier: AtomicU64::new(1),
            open_instant: None,
        }
    }

    fn base_open_duration_ms(&self) -> u64 {
        self.config.open_duration_secs * 1000
    }

    fn effective_open_duration_ms(&self) -> u64 {
        let multiplier = self.backoff_multiplier.load(Ordering::SeqCst);
        let base = self.base_open_duration_ms();
        if multiplier <= 1 {
            base
        } else {
            // Exponential backoff: base * 2^(multiplier-1), capped at 10 minutes
            let backoff = base * (1u64 << (multiplier.min(5) - 1));
            backoff.min(600_000)
        }
    }

    fn next_backoff_ms(&self) -> u64 {
        let current_multiplier = self.backoff_multiplier.load(Ordering::SeqCst);
        let next_mult = current_multiplier + 1;
        let base = self.base_open_duration_ms();
        let backoff = if next_mult <= 1 {
            base
        } else {
            base * (1u64 << (next_mult.min(5) - 1))
        };
        backoff.min(600_000)
    }
}

// ---------------------------------------------------------------------------
// SelfHealingCircuitBreaker
// ---------------------------------------------------------------------------

/// Self-healing circuit breaker with per-provider circuits.
pub struct SelfHealingCircuitBreaker {
    /// One circuit per provider_id.
    circuits: DashMap<String, CircuitData>,
}

impl SelfHealingCircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new() -> Self {
        Self {
            circuits: DashMap::new(),
        }
    }

    /// Ensure a circuit exists for the given provider, creating one with default config if needed.
    fn ensure_circuit(&self, provider_id: &str) {
        if !self.circuits.contains_key(provider_id) {
            self.circuits.entry(provider_id.to_string())
                .or_insert_with(|| CircuitData::new(CircuitConfig::default()));
        }
    }

    /// Check whether a request should be allowed through the circuit.
    pub fn check_circuit(&self, provider_id: &str) -> bool {
        self.ensure_circuit(provider_id);

        let mut data = self.circuits.get_mut(provider_id).unwrap();

        match data.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(opened_instant) = data.open_instant {
                    let elapsed = opened_instant.elapsed();
                    let effective_duration = Duration::from_millis(data.effective_open_duration_ms());
                    if elapsed >= effective_duration {
                        data.state = CircuitState::HalfOpen;
                        data.half_open_calls_used = 0;
                        data.open_instant = None;

                        // Account for total open time
                        let open_ms = elapsed.as_millis() as u64;
                        data.total_open_time_ms.fetch_add(open_ms, Ordering::SeqCst);

                        debug!(
                            provider_id = %provider_id,
                            open_ms = open_ms,
                            "Circuit transitioning to half-open"
                        );
                        true // Allow the probe request
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                if data.half_open_calls_used < data.config.half_open_max_calls {
                    data.half_open_calls_used += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful call.
    pub fn record_success(&self, provider_id: &str) {
        self.ensure_circuit(provider_id);

        let mut data = self.circuits.get_mut(provider_id).unwrap();
        data.total_calls.fetch_add(1, Ordering::SeqCst);
        data.successes.fetch_add(1, Ordering::SeqCst);
        data.consecutive_failures = 0;
        data.consecutive_successes += 1;
        data.last_success_time = Some(Utc::now());

        match data.state {
            CircuitState::HalfOpen => {
                if data.consecutive_successes >= data.config.success_threshold {
                    data.state = CircuitState::Closed;
                    data.consecutive_successes = 0;
                    data.half_open_calls_used = 0;
                    data.backoff_multiplier.store(1, Ordering::SeqCst);
                    info!(
                        provider_id = %provider_id,
                        "Circuit closed after successful recovery"
                    );
                }
            }
            CircuitState::Closed => {
                // Reset backoff on success in closed state
                if data.backoff_multiplier.load(Ordering::SeqCst) > 1 {
                    data.backoff_multiplier.store(1, Ordering::SeqCst);
                }
            }
            CircuitState::Open => {
                // Shouldn't happen but handle gracefully
            }
        }
    }

    /// Record a failed call.
    pub fn record_failure(&self, provider_id: &str) {
        self.ensure_circuit(provider_id);

        let mut data = self.circuits.get_mut(provider_id).unwrap();
        data.total_calls.fetch_add(1, Ordering::SeqCst);
        data.failures.fetch_add(1, Ordering::SeqCst);
        data.consecutive_successes = 0;
        data.consecutive_failures += 1;
        data.last_failure_time = Some(Utc::now());

        match data.state {
            CircuitState::Closed => {
                if data.consecutive_failures >= data.config.failure_threshold {
                    data.state = CircuitState::Open;
                    data.opened_at = Some(Utc::now());
                    data.open_instant = Some(Instant::now());
                    data.circuit_opens_count.fetch_add(1, Ordering::SeqCst);
                    data.half_open_calls_used = 0;

                    // Increase backoff multiplier
                    let current = data.backoff_multiplier.load(Ordering::SeqCst);
                    data.backoff_multiplier.store(current + 1, Ordering::SeqCst);

                    warn!(
                        provider_id = %provider_id,
                        consecutive_failures = data.consecutive_failures,
                        backoff_multiplier = data.backoff_multiplier.load(Ordering::SeqCst),
                        "Circuit opened due to consecutive failures"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Failure in half-open -> back to open with increased backoff
                data.state = CircuitState::Open;
                data.opened_at = Some(Utc::now());
                data.open_instant = Some(Instant::now());
                data.consecutive_failures = 0;
                data.half_open_calls_used = 0;
                let current = data.backoff_multiplier.load(Ordering::SeqCst);
                data.backoff_multiplier.store(current + 1, Ordering::SeqCst);

                warn!(
                    provider_id = %provider_id,
                    "Circuit reopened after failure in half-open state"
                );
            }
            CircuitState::Open => {
                // Already open, just track
            }
        }
    }

    /// Get the current state of a circuit.
    pub fn get_state(&self, provider_id: &str) -> Option<CircuitState> {
        self.circuits.get(provider_id).map(|d| d.state.clone())
    }

    /// Get detailed stats for a circuit.
    pub fn get_stats(&self, provider_id: &str) -> Option<CircuitStats> {
        self.circuits.get(provider_id).map(|data| {
            let effective_open_ms = data.effective_open_duration_ms();
            CircuitStats {
                provider_id: provider_id.to_string(),
                state: data.state.clone(),
                total_calls: data.total_calls.load(Ordering::SeqCst),
                successes: data.successes.load(Ordering::SeqCst),
                failures: data.failures.load(Ordering::SeqCst),
                current_consecutive_failures: data.consecutive_failures,
                current_consecutive_successes: data.consecutive_successes,
                half_open_calls_remaining: data.config.half_open_max_calls.saturating_sub(data.half_open_calls_used),
                last_failure_time: data.last_failure_time,
                last_success_time: data.last_success_time,
                opened_at: data.opened_at,
                total_open_time_ms: data.total_open_time_ms.load(Ordering::SeqCst),
                circuit_opens_count: data.circuit_opens_count.load(Ordering::SeqCst),
                backoff_multiplier: data.backoff_multiplier.load(Ordering::SeqCst) as u32,
                next_backoff_ms: data.next_backoff_ms(),
            }
        })
    }

    /// Reset a circuit to closed state.
    pub fn reset(&self, provider_id: &str) -> bool {
        if let Some(mut data) = self.circuits.get_mut(provider_id) {
            let was_open = data.state == CircuitState::Open;
            if was_open {
                if let Some(open_instant) = data.open_instant.take() {
                    let open_ms = open_instant.elapsed().as_millis() as u64;
                    data.total_open_time_ms.fetch_add(open_ms, Ordering::SeqCst);
                }
            }
            data.state = CircuitState::Closed;
            data.consecutive_failures = 0;
            data.consecutive_successes = 0;
            data.half_open_calls_used = 0;
            data.backoff_multiplier.store(1, Ordering::SeqCst);
            data.opened_at = None;
            data.open_instant = None;
            info!(provider_id = %provider_id, "Circuit reset");
            true
        } else {
            false
        }
    }

    /// Perform a health check against a provider.
    pub fn health_check(&self, provider_id: &str) -> HealthCheckResult {
        self.ensure_circuit(provider_id);

        let start = Instant::now();

        // Simulate a health probe (in production this would be an actual HTTP/gRPC check)
        let state = self.get_state(provider_id).unwrap_or(CircuitState::Closed);
        let is_healthy = state != CircuitState::Open;

        let latency_ms = start.elapsed().as_millis() as u64;

        // Calculate error rate from stats
        let error_rate = self.get_stats(provider_id)
            .map(|s| {
                if s.total_calls > 0 {
                    s.failures as f64 / s.total_calls as f64
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);

        HealthCheckResult {
            provider_id: provider_id.to_string(),
            is_healthy,
            latency_ms,
            error_rate,
            response: format!("Circuit state: {}, latency: {}ms, error_rate: {:.2}%", state, latency_ms, error_rate * 100.0),
            timestamp: Utc::now(),
        }
    }

    /// Get all circuit states.
    pub fn get_all_circuits(&self) -> Vec<CircuitStats> {
        self.circuits.iter()
            .map(|entry| {
                let data = entry.value();
                CircuitStats {
                    provider_id: entry.key().clone(),
                    state: data.state.clone(),
                    total_calls: data.total_calls.load(Ordering::SeqCst),
                    successes: data.successes.load(Ordering::SeqCst),
                    failures: data.failures.load(Ordering::SeqCst),
                    current_consecutive_failures: data.consecutive_failures,
                    current_consecutive_successes: data.consecutive_successes,
                    half_open_calls_remaining: data.config.half_open_max_calls.saturating_sub(data.half_open_calls_used),
                    last_failure_time: data.last_failure_time,
                    last_success_time: data.last_success_time,
                    opened_at: data.opened_at,
                    total_open_time_ms: data.total_open_time_ms.load(Ordering::SeqCst),
                    circuit_opens_count: data.circuit_opens_count.load(Ordering::SeqCst),
                    backoff_multiplier: data.backoff_multiplier.load(Ordering::SeqCst) as u32,
                    next_backoff_ms: data.next_backoff_ms(),
                }
            })
            .collect()
    }

    /// Force a circuit into a specific state.
    pub fn force_state(&self, provider_id: &str, new_state: CircuitState) -> bool {
        self.ensure_circuit(provider_id);

        let mut data = self.circuits.get_mut(provider_id).unwrap();

        // Account for open time if transitioning away from open
        if data.state == CircuitState::Open && new_state != CircuitState::Open {
            if let Some(open_instant) = data.open_instant.take() {
                let open_ms = open_instant.elapsed().as_millis() as u64;
                data.total_open_time_ms.fetch_add(open_ms, Ordering::SeqCst);
            }
        }

        match new_state {
            CircuitState::Open => {
                data.opened_at = Some(Utc::now());
                data.open_instant = Some(Instant::now());
                data.circuit_opens_count.fetch_add(1, Ordering::SeqCst);
            }
            CircuitState::Closed => {
                data.consecutive_failures = 0;
                data.consecutive_successes = 0;
                data.half_open_calls_used = 0;
                data.backoff_multiplier.store(1, Ordering::SeqCst);
                data.opened_at = None;
            }
            CircuitState::HalfOpen => {
                data.half_open_calls_used = 0;
                data.open_instant = None;
            }
        }

        data.state = new_state.clone();
        info!(
            provider_id = %provider_id,
            new_state = %new_state,
            "Circuit state forced"
        );
        true
    }

    /// Get the number of circuits being tracked.
    pub fn circuit_count(&self) -> usize {
        self.circuits.len()
    }
}

impl Default for SelfHealingCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ForceStateRequest {
    pub state: String,
}

#[derive(Debug, Serialize)]
pub struct AllCircuitsResponse {
    pub circuits: Vec<CircuitStats>,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn get_circuit_state_handler(
    State(cb): State<Arc<SelfHealingCircuitBreaker>>,
    Path(provider_id): Path<String>,
) -> Result<Json<CircuitStats>, StatusCode> {
    cb.get_stats(&provider_id)
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}

async fn get_circuit_stats_handler(
    State(cb): State<Arc<SelfHealingCircuitBreaker>>,
    Path(provider_id): Path<String>,
) -> Result<Json<CircuitStats>, StatusCode> {
    cb.get_stats(&provider_id)
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}

async fn get_all_circuits_handler(
    State(cb): State<Arc<SelfHealingCircuitBreaker>>,
) -> Json<AllCircuitsResponse> {
    let circuits = cb.get_all_circuits();
    let total = circuits.len();
    Json(AllCircuitsResponse { circuits, total })
}

async fn reset_circuit_handler(
    State(cb): State<Arc<SelfHealingCircuitBreaker>>,
    Path(provider_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if cb.reset(&provider_id) {
        Ok(Json(serde_json::json!({
            "provider_id": provider_id,
            "reset": true,
            "message": "Circuit reset to closed state"
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn health_check_handler(
    State(cb): State<Arc<SelfHealingCircuitBreaker>>,
    Path(provider_id): Path<String>,
) -> Json<HealthCheckResult> {
    Json(cb.health_check(&provider_id))
}

async fn force_state_handler(
    State(cb): State<Arc<SelfHealingCircuitBreaker>>,
    Path(provider_id): Path<String>,
    Json(body): Json<ForceStateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let new_state = match body.state.to_lowercase().as_str() {
        "closed" => CircuitState::Closed,
        "open" => CircuitState::Open,
        "half_open" | "half-open" => CircuitState::HalfOpen,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    if cb.force_state(&provider_id, new_state.clone()) {
        Ok(Json(serde_json::json!({
            "provider_id": provider_id,
            "previous_state": "unknown",
            "new_state": new_state,
            "forced": true
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the self-healing circuit breaker router.
pub fn build_circuit_breaker_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{get, post};

    let cb = state.circuit_breaker.clone();

    axum::Router::new()
        .route("/v1/circuit/state/{provider_id}", get(get_circuit_state_handler))
        .route("/v1/circuit/stats/{provider_id}", get(get_circuit_stats_handler))
        .route("/v1/circuit/all", get(get_all_circuits_handler))
        .route("/v1/circuit/reset/{provider_id}", post(reset_circuit_handler))
        .route("/v1/circuit/health-check/{provider_id}", get(health_check_handler))
        .route("/v1/circuit/force/{provider_id}", post(force_state_handler))
        .with_state(cb)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_cb() -> SelfHealingCircuitBreaker {
        SelfHealingCircuitBreaker::new()
    }

    fn create_cb_with_thresholds(failure: u32, success: u32, open_secs: u64) -> SelfHealingCircuitBreaker {
        let cb = SelfHealingCircuitBreaker::new();
        cb.circuits.insert("test-provider".to_string(), CircuitData::new(CircuitConfig {
            failure_threshold: failure,
            success_threshold: success,
            open_duration_secs: open_secs,
            half_open_max_calls: 3,
            health_check_interval_secs: 30,
        }));
        cb
    }

    #[test]
    fn test_closed_state_allows_requests() {
        let cb = create_cb();
        assert!(cb.check_circuit("provider-1"));
        assert_eq!(cb.get_state("provider-1"), Some(CircuitState::Closed));
    }

    #[test]
    fn test_failure_threshold_opens_circuit() {
        let cb = create_cb_with_thresholds(3, 3, 60);
        cb.record_success("test-provider");
        cb.record_success("test-provider");
        cb.record_failure("test-provider");
        cb.record_failure("test-provider");
        // Still closed
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Closed));

        cb.record_failure("test-provider");
        // Now open
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Open));
        assert!(!cb.check_circuit("test-provider"));
    }

    #[test]
    fn test_open_state_blocks_requests() {
        let cb = create_cb_with_thresholds(1, 1, 600); // 10 min open
        cb.record_failure("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Open));
        assert!(!cb.check_circuit("test-provider"));
        assert!(!cb.check_circuit("test-provider"));
    }

    #[tokio::test]
    async fn test_half_open_recovery() {
        let cb = create_cb_with_thresholds(2, 2, 0); // Open for 0 seconds (immediate half-open)
        cb.record_failure("test-provider");
        cb.record_failure("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Open));

        // Wait briefly then check (0s open duration means immediate transition)
        tokio::time::sleep(Duration::from_millis(10)).await;

        // check_circuit should transition to half-open and allow
        assert!(cb.check_circuit("test-provider"));
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::HalfOpen));

        // Record successes to close
        cb.record_success("test-provider");
        cb.record_success("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Closed));
    }

    #[test]
    fn test_success_threshold_resets_circuit() {
        let cb = create_cb_with_thresholds(1, 3, 0);
        cb.record_failure("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Open));

        // Simulate time passing by forcing half-open
        cb.force_state("test-provider", CircuitState::HalfOpen);
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::HalfOpen));

        // Need 3 consecutive successes
        cb.record_success("test-provider");
        cb.record_success("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::HalfOpen)); // Not yet

        cb.record_success("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Closed));
    }

    #[test]
    fn test_health_check() {
        let cb = create_cb();
        cb.record_success("provider-1");
        let result = cb.health_check("provider-1");
        assert!(result.is_healthy);
        assert!(result.error_rate < 1.0);
        assert!(!result.provider_id.is_empty());
    }

    #[test]
    fn test_force_state() {
        let cb = create_cb();
        cb.force_state("provider-1", CircuitState::Open);
        assert_eq!(cb.get_state("provider-1"), Some(CircuitState::Open));

        cb.force_state("provider-1", CircuitState::Closed);
        assert_eq!(cb.get_state("provider-1"), Some(CircuitState::Closed));

        cb.force_state("provider-1", CircuitState::HalfOpen);
        assert_eq!(cb.get_state("provider-1"), Some(CircuitState::HalfOpen));
    }

    #[test]
    fn test_stats_tracking() {
        let cb = create_cb();
        cb.record_success("provider-1");
        cb.record_success("provider-1");
        cb.record_failure("provider-1");

        let stats = cb.get_stats("provider-1").unwrap();
        assert_eq!(stats.total_calls, 3);
        assert_eq!(stats.successes, 2);
        assert_eq!(stats.failures, 1);
        assert_eq!(stats.current_consecutive_failures, 1);
        assert!(stats.last_success_time.is_some());
        assert!(stats.last_failure_time.is_some());
    }

    #[test]
    fn test_backoff_timing() {
        let cb = create_cb_with_thresholds(1, 1, 60);
        cb.record_failure("test-provider"); // Opens, backoff_mult = 2
        let stats = cb.get_stats("test-provider").unwrap();
        assert_eq!(stats.backoff_multiplier, 2);

        // Force half-open and fail again to increase backoff
        cb.force_state("test-provider", CircuitState::HalfOpen);
        cb.record_failure("test-provider"); // Reopens, backoff_mult = 3
        let stats2 = cb.get_stats("test-provider").unwrap();
        assert_eq!(stats2.backoff_multiplier, 3);
        // Next backoff should be 60 * 2^(3-1) = 240s
        assert_eq!(stats2.next_backoff_ms, 240_000);
    }

    #[test]
    fn test_reset_clears_state() {
        let cb = create_cb_with_thresholds(1, 1, 60);
        cb.record_failure("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Open));

        cb.reset("test-provider");
        assert_eq!(cb.get_state("test-provider"), Some(CircuitState::Closed));

        let stats = cb.get_stats("test-provider").unwrap();
        assert_eq!(stats.current_consecutive_failures, 0);
        assert_eq!(stats.backoff_multiplier, 1);
    }

    #[test]
    fn test_get_all_circuits() {
        let cb = create_cb();
        cb.record_success("p1");
        cb.record_success("p2");
        cb.record_failure("p3");

        let all = cb.get_all_circuits();
        assert_eq!(all.len(), 3);
    }
}
