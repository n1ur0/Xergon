//! Circuit Breaker module for per-provider fault tolerance.
//!
//! Provides a standalone circuit breaker that tracks provider health via
//! success/failure records. Supports three states: Closed, Open, HalfOpen.
//! Tracks metrics for open/close events and rejected requests.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Circuit State
// ---------------------------------------------------------------------------

/// Circuit breaker state for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    /// Normal operation — requests flow through.
    Closed,
    /// Provider is failing — requests are rejected.
    Open,
    /// Probing recovery — limited requests allowed to test recovery.
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "Closed"),
            CircuitState::Open => write!(f, "Open"),
            CircuitState::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the circuit breaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Consecutive failures before opening the circuit (default: 5).
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    /// Consecutive successes in half-open before closing (default: 3).
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,
    /// Seconds to wait before transitioning Open -> HalfOpen (default: 30).
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Max concurrent probe requests in HalfOpen state (default: 3).
    #[serde(default = "default_half_open_max_calls")]
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
            timeout_secs: default_timeout_secs(),
            half_open_max_calls: default_half_open_max_calls(),
        }
    }
}

fn default_failure_threshold() -> u32 {
    5
}
fn default_success_threshold() -> u32 {
    3
}
fn default_timeout_secs() -> u64 {
    30
}
fn default_half_open_max_calls() -> u32 {
    3
}

// ---------------------------------------------------------------------------
// Per-Provider State
// ---------------------------------------------------------------------------

/// Internal state tracked per provider.
struct ProviderCircuitState {
    /// Current circuit state.
    state: CircuitState,
    /// Consecutive failures.
    consecutive_failures: u32,
    /// Consecutive successes in HalfOpen.
    half_open_successes: u32,
    /// When the circuit was opened.
    opened_at: Option<Instant>,
    /// Current probe count in HalfOpen.
    active_probes: u32,
}

impl Default for ProviderCircuitState {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            half_open_successes: 0,
            opened_at: None,
            active_probes: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

/// Per-provider circuit breaker with metrics.
///
/// Tracks open/close events and rejected request counts.
pub struct CircuitBreaker {
    /// Per-provider circuit state, keyed by provider identifier (endpoint or pk).
    providers: DashMap<String, ProviderCircuitState>,
    /// Configuration.
    config: CircuitBreakerConfig,
    /// Total number of circuit-open events.
    open_events: AtomicU64,
    /// Total number of circuit-close events.
    close_events: AtomicU64,
    /// Total requests rejected due to open circuit.
    rejected_requests: AtomicU64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given config.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            providers: DashMap::new(),
            config,
            open_events: AtomicU64::new(0),
            close_events: AtomicU64::new(0),
            rejected_requests: AtomicU64::new(0),
        }
    }

    /// Record a successful request for a provider.
    ///
    /// - In Closed: resets failure count.
    /// - In HalfOpen: increments success count; closes circuit if threshold reached.
    pub fn record_success(&self, provider_pk: &str) {
        let mut state = self.providers.entry(provider_pk.to_string()).or_default();

        match state.state {
            CircuitState::Closed => {
                state.consecutive_failures = 0;
            }
            CircuitState::HalfOpen => {
                state.half_open_successes += 1;
                state.active_probes = state.active_probes.saturating_sub(1);

                if state.half_open_successes >= self.config.success_threshold {
                    state.state = CircuitState::Closed;
                    state.consecutive_failures = 0;
                    state.half_open_successes = 0;
                    state.opened_at = None;
                    self.close_events.fetch_add(1, Ordering::Relaxed);
                    info!(
                        provider = %provider_pk,
                        "Circuit breaker CLOSED (recovery confirmed)"
                    );
                }
            }
            CircuitState::Open => {
                // Should not happen (requests rejected in Open), but handle gracefully.
                state.consecutive_failures = 0;
            }
        }
    }

    /// Record a failed request for a provider.
    ///
    /// - In Closed: increments failure count; opens circuit if threshold reached.
    /// - In HalfOpen: re-opens the circuit.
    pub fn record_failure(&self, provider_pk: &str) {
        let mut state = self.providers.entry(provider_pk.to_string()).or_default();

        match state.state {
            CircuitState::Closed => {
                state.consecutive_failures += 1;
                if state.consecutive_failures >= self.config.failure_threshold {
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                    self.open_events.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        provider = %provider_pk,
                        failures = state.consecutive_failures,
                        "Circuit breaker OPENED"
                    );
                }
            }
            CircuitState::HalfOpen => {
                state.state = CircuitState::Open;
                state.opened_at = Some(Instant::now());
                state.half_open_successes = 0;
                state.active_probes = 0;
                self.open_events.fetch_add(1, Ordering::Relaxed);
                warn!(
                    provider = %provider_pk,
                    "Circuit breaker re-OPENED (probe failed)"
                );
            }
            CircuitState::Open => {
                // Already open, increment failures
                state.consecutive_failures += 1;
            }
        }
    }

    /// Check whether a provider is available for requests.
    ///
    /// Returns true if requests should be allowed. May transition
    /// Open -> HalfOpen if the recovery timeout has elapsed.
    pub fn is_available(&self, provider_pk: &str) -> bool {
        let mut state = self.providers.entry(provider_pk.to_string()).or_default();

        match state.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery timeout has elapsed
                if let Some(opened_at) = state.opened_at {
                    let timeout = Duration::from_secs(self.config.timeout_secs);
                    if opened_at.elapsed() >= timeout {
                        state.state = CircuitState::HalfOpen;
                        state.half_open_successes = 0;
                        state.active_probes = 0;
                        debug!(
                            provider = %provider_pk,
                            "Circuit breaker transitioned to HalfOpen"
                        );
                        return true;
                    }
                }
                self.rejected_requests.fetch_add(1, Ordering::Relaxed);
                false
            }
            CircuitState::HalfOpen => {
                // Allow requests up to the max probe count
                if state.active_probes < self.config.half_open_max_calls {
                    state.active_probes += 1;
                    true
                } else {
                    self.rejected_requests.fetch_add(1, Ordering::Relaxed);
                    false
                }
            }
        }
    }

    /// Get the current circuit state for a provider.
    pub fn get_state(&self, provider_pk: &str) -> CircuitState {
        self.providers
            .get(provider_pk)
            .map(|s| s.state)
            .unwrap_or(CircuitState::Closed)
    }

    /// Get all circuit breaker states.
    pub fn get_all_states(&self) -> Vec<(String, CircuitState, u32)> {
        self.providers
            .iter()
            .map(|entry| {
                let s = entry.value();
                (entry.key().clone(), s.state, s.consecutive_failures)
            })
            .collect()
    }

    /// Force-reset a provider's circuit breaker to Closed.
    pub fn reset(&self, provider_pk: &str) {
        if let Some(mut state) = self.providers.get_mut(provider_pk) {
            if state.state != CircuitState::Closed {
                info!(
                    provider = %provider_pk,
                    "Circuit breaker manually reset to Closed"
                );
            }
            state.state = CircuitState::Closed;
            state.consecutive_failures = 0;
            state.half_open_successes = 0;
            state.opened_at = None;
            state.active_probes = 0;
        }
    }

    /// Get aggregate metrics.
    pub fn metrics(&self) -> CircuitBreakerMetrics {
        CircuitBreakerMetrics {
            total_providers: self.providers.len(),
            open_circuits: self
                .providers
                .iter()
                .filter(|e| e.value().state == CircuitState::Open)
                .count(),
            half_open_circuits: self
                .providers
                .iter()
                .filter(|e| e.value().state == CircuitState::HalfOpen)
                .count(),
            open_events: self.open_events.load(Ordering::Relaxed),
            close_events: self.close_events.load(Ordering::Relaxed),
            rejected_requests: self.rejected_requests.load(Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Aggregate circuit breaker metrics.
#[derive(Debug, Clone, Serialize)]
pub struct CircuitBreakerMetrics {
    /// Total number of tracked providers.
    pub total_providers: usize,
    /// Number of providers with Open circuits.
    pub open_circuits: usize,
    /// Number of providers with HalfOpen circuits.
    pub half_open_circuits: usize,
    /// Total circuit-open events since startup.
    pub open_events: u64,
    /// Total circuit-close events since startup.
    pub close_events: u64,
    /// Total requests rejected due to open circuits.
    pub rejected_requests: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_secs: 1,
            half_open_max_calls: 2,
        }
    }

    #[test]
    fn test_default_config() {
        let cfg = CircuitBreakerConfig::default();
        assert_eq!(cfg.failure_threshold, 5);
        assert_eq!(cfg.success_threshold, 3);
        assert_eq!(cfg.timeout_secs, 30);
        assert_eq!(cfg.half_open_max_calls, 3);
    }

    #[test]
    fn test_starts_closed() {
        let cb = CircuitBreaker::new(make_config());
        assert_eq!(cb.get_state("provider1"), CircuitState::Closed);
        assert!(cb.is_available("provider1"));
    }

    #[test]
    fn test_closed_to_open_after_failures() {
        let cb = CircuitBreaker::new(make_config());

        cb.record_failure("p1");
        assert_eq!(cb.get_state("p1"), CircuitState::Closed);

        cb.record_failure("p1");
        assert_eq!(cb.get_state("p1"), CircuitState::Closed);

        cb.record_failure("p1");
        assert_eq!(cb.get_state("p1"), CircuitState::Open);
    }

    #[test]
    fn test_open_rejects_requests() {
        let cb = CircuitBreaker::new(make_config());
        // Trip the circuit
        for _ in 0..3 {
            cb.record_failure("p1");
        }
        assert_eq!(cb.get_state("p1"), CircuitState::Open);
        assert!(!cb.is_available("p1"));
    }

    #[test]
    fn test_success_resets_closed_failures() {
        let cb = CircuitBreaker::new(make_config());

        cb.record_failure("p1");
        cb.record_failure("p1");
        cb.record_success("p1");
        cb.record_failure("p1");

        // Should still be closed (failure count was reset by success)
        assert_eq!(cb.get_state("p1"), CircuitState::Closed);
    }

    #[test]
    fn test_open_to_half_open_after_timeout() {
        let cb = CircuitBreaker::new(make_config());
        for _ in 0..3 {
            cb.record_failure("p1");
        }
        assert_eq!(cb.get_state("p1"), CircuitState::Open);

        // Manually set opened_at to the past to simulate timeout
        {
            let mut state = cb.providers.get_mut("p1").unwrap();
            state.opened_at = Some(Instant::now() - Duration::from_secs(2));
        }

        // Next is_available call should transition to HalfOpen
        assert!(cb.is_available("p1"));
        assert_eq!(cb.get_state("p1"), CircuitState::HalfOpen);
    }

    #[test]
    fn test_half_open_success_closes_circuit() {
        let cb = CircuitBreaker::new(make_config());
        for _ in 0..3 {
            cb.record_failure("p1");
        }
        // Force HalfOpen
        {
            let mut state = cb.providers.get_mut("p1").unwrap();
            state.state = CircuitState::HalfOpen;
            state.opened_at = None;
        }

        cb.record_success("p1");
        assert_eq!(cb.get_state("p1"), CircuitState::HalfOpen);

        cb.record_success("p1");
        assert_eq!(cb.get_state("p1"), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_failure_reopens_circuit() {
        let cb = CircuitBreaker::new(make_config());
        for _ in 0..3 {
            cb.record_failure("p1");
        }
        // Force HalfOpen
        {
            let mut state = cb.providers.get_mut("p1").unwrap();
            state.state = CircuitState::HalfOpen;
            state.opened_at = None;
        }

        cb.record_failure("p1");
        assert_eq!(cb.get_state("p1"), CircuitState::Open);
    }

    #[test]
    fn test_metrics_tracking() {
        let cb = CircuitBreaker::new(make_config());

        // Trip one circuit
        for _ in 0..3 {
            cb.record_failure("p1");
        }
        // Try to use it (should be rejected)
        cb.is_available("p1");
        cb.is_available("p1");

        let m = cb.metrics();
        assert_eq!(m.total_providers, 1);
        assert_eq!(m.open_circuits, 1);
        assert_eq!(m.open_events, 1);
        assert_eq!(m.rejected_requests, 2);
    }

    #[test]
    fn test_get_all_states() {
        let cb = CircuitBreaker::new(make_config());

        for _ in 0..3 {
            cb.record_failure("p1");
        }
        // p2 stays closed
        cb.record_failure("p2");

        let states = cb.get_all_states();
        assert_eq!(states.len(), 2);
    }

    #[test]
    fn test_reset() {
        let cb = CircuitBreaker::new(make_config());
        for _ in 0..3 {
            cb.record_failure("p1");
        }
        assert_eq!(cb.get_state("p1"), CircuitState::Open);

        cb.reset("p1");
        assert_eq!(cb.get_state("p1"), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_max_calls() {
        let cb = CircuitBreaker::new(make_config());
        for _ in 0..3 {
            cb.record_failure("p1");
        }
        // Force HalfOpen
        {
            let mut state = cb.providers.get_mut("p1").unwrap();
            state.state = CircuitState::HalfOpen;
            state.opened_at = None;
        }

        // Max 2 calls in HalfOpen
        assert!(cb.is_available("p1"));
        assert!(cb.is_available("p1"));
        assert!(!cb.is_available("p1")); // 3rd should be rejected
    }

    #[test]
    fn test_circuit_state_display() {
        assert_eq!(format!("{}", CircuitState::Closed), "Closed");
        assert_eq!(format!("{}", CircuitState::Open), "Open");
        assert_eq!(format!("{}", CircuitState::HalfOpen), "HalfOpen");
    }

    #[test]
    fn test_circuit_state_serialize() {
        assert_eq!(
            serde_json::to_string(&CircuitState::Closed).unwrap(),
            "\"closed\""
        );
        assert_eq!(
            serde_json::to_string(&CircuitState::Open).unwrap(),
            "\"open\""
        );
        assert_eq!(
            serde_json::to_string(&CircuitState::HalfOpen).unwrap(),
            "\"half_open\""
        );
    }
}
