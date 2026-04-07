//! Provider registry, health polling, and smart routing
//!
//! The relay maintains a live view of all known xergon-agent instances.
//! Each provider is periodically polled via GET /xergon/status.
//! When a request arrives, the router selects the best provider based on:
//!   1. Latency (measured during health polls)
//!   2. PoNW score (from provider's /xergon/status)
//!   3. Current load (requests in-flight)

use chrono::Utc;
use dashmap::DashMap;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::chain::ChainProvider;
use crate::config::RelayConfig;
use crate::demand::DemandTracker;

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

/// Circuit breaker state for a provider.
///
/// - **Closed**: Normal operation — requests flow through.
/// - **Open**: Provider is failing — requests are skipped (routed elsewhere).
/// - **HalfOpen**: Probing recovery — a limited number of requests are allowed
///   through to test if the provider has recovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
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

/// A sticky session entry: maps a session key to a provider endpoint.
#[derive(Debug, Clone)]
struct StickySession {
    endpoint: String,
    created_at: Instant,
}

impl StickySession {
    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// Status returned by a xergon-agent at /xergon/status
#[allow(dead_code)] // TODO: fields will be used for provider dashboard display
#[derive(Debug, Clone, Deserialize)]
pub struct XergonAgentStatus {
    pub provider: Option<AgentProviderInfo>,
    pub pown_status: Option<AgentPownInfo>,
    #[serde(default)]
    pub pown_health: Option<AgentHealthInfo>,
}

#[allow(dead_code)] // TODO: fields will be used for provider dashboard display
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProviderInfo {
    pub id: String,
    pub name: String,
    pub region: String,
}

#[allow(dead_code)] // TODO: fields will be used for provider dashboard display
#[derive(Debug, Clone, Deserialize)]
pub struct AgentPownInfo {
    pub work_points: u64,
    pub ai_enabled: bool,
    pub ai_model: String,
    pub ai_total_requests: u64,
    pub ai_total_tokens: u64,
    pub node_id: String,
}

#[allow(dead_code)] // TODO: fields will be used for health monitoring dashboard
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealthInfo {
    pub is_synced: bool,
    pub node_height: u32,
    pub peer_count: usize,
    pub timestamp: i64,
}

/// A provider that the relay knows about
#[derive(Debug)]
pub struct Provider {
    /// The base URL of this provider (e.g. "http://1.2.3.4:9099")
    pub endpoint: String,
    /// Latest status from /xergon/status
    pub status: Option<XergonAgentStatus>,
    /// Round-trip latency measured during last health poll (ms)
    pub latency_ms: u64,
    /// Number of requests currently being proxied to this provider
    pub active_requests: Arc<std::sync::atomic::AtomicU32>,
    /// Whether this provider is considered healthy
    pub is_healthy: bool,
    /// Last successful health poll timestamp
    pub last_healthy_at: chrono::DateTime<Utc>,
    /// Consecutive health check failures
    pub consecutive_failures: u32,
    /// When the last health check (success or failure) was performed
    pub last_health_check: Instant,
    /// When the last *successful* health check was performed
    pub last_successful_check: Instant,
    /// Whether this provider was discovered from chain state (vs static config)
    pub from_chain: bool,
    /// Per-model pricing: model_id -> nanoERG per 1M tokens (populated from chain sync)
    pub model_pricing: HashMap<String, u64>,
    /// Models this provider serves (populated from chain sync)
    pub served_models: Vec<String>,
    /// On-chain PoNW reputation score (0-1000, from R7 register)
    pub pown_score: i32,
    /// Provider region (from on-chain R9 register or /xergon/status)
    pub region: Option<String>,
    // -- Circuit Breaker fields --
    /// Current circuit breaker state
    pub circuit_state: CircuitState,
    /// When the circuit was opened (used to determine when to transition to HalfOpen)
    pub circuit_opened_at: Option<Instant>,
    /// Number of probe requests currently allowed/running in HalfOpen state
    pub half_open_probes: Arc<AtomicU32>,
    // -- Latency tracking for Health Score V2 --
    /// Recent latency samples from actual proxy requests (not just health polls)
    pub latency_samples: Vec<Duration>,
    /// When the last proxied request was made to this provider
    pub last_request_at: Instant,
    /// Total proxied requests to this provider
    pub total_requests: u64,
    /// Total failed proxied requests to this provider
    pub failed_requests: u64,
    // -- Admin state fields --
    /// Whether this provider has been administratively suspended (not routed to)
    pub suspended: std::sync::atomic::AtomicBool,
    /// Whether this provider is draining (finish in-flight, reject new)
    pub draining: std::sync::atomic::AtomicBool,
}

impl Clone for Provider {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            status: self.status.clone(),
            latency_ms: self.latency_ms,
            active_requests: Arc::clone(&self.active_requests),
            is_healthy: self.is_healthy,
            last_healthy_at: self.last_healthy_at,
            consecutive_failures: self.consecutive_failures,
            last_health_check: self.last_health_check,
            last_successful_check: self.last_successful_check,
            from_chain: self.from_chain,
            model_pricing: self.model_pricing.clone(),
            served_models: self.served_models.clone(),
            pown_score: self.pown_score,
            region: self.region.clone(),
            circuit_state: self.circuit_state,
            circuit_opened_at: self.circuit_opened_at,
            half_open_probes: Arc::clone(&self.half_open_probes),
            latency_samples: self.latency_samples.clone(),
            last_request_at: self.last_request_at,
            total_requests: self.total_requests,
            failed_requests: self.failed_requests,
            suspended: std::sync::atomic::AtomicBool::new(self.suspended.load(Ordering::Relaxed)),
            draining: std::sync::atomic::AtomicBool::new(self.draining.load(Ordering::Relaxed)),
        }
    }
}

impl Provider {
    /// Record a latency sample from a proxied request.
    /// Maintains a bounded sliding window of samples (configurable max).
    pub fn record_latency(&mut self, duration: Duration, max_samples: usize) {
        self.latency_samples.push(duration);
        // Keep only the most recent samples
        if self.latency_samples.len() > max_samples {
            let excess = self.latency_samples.len() - max_samples;
            self.latency_samples.drain(..excess);
        }
        self.last_request_at = Instant::now();
    }

    /// Compute average latency from samples. Returns None if no samples.
    pub fn avg_latency(&self) -> Option<Duration> {
        if self.latency_samples.is_empty() {
            return None;
        }
        let sum: Duration = self.latency_samples.iter().copied().sum();
        Some(sum / self.latency_samples.len() as u32)
    }

    /// Compute p95 latency from samples. Returns None if fewer than 2 samples.
    pub fn p95_latency(&self) -> Option<Duration> {
        if self.latency_samples.len() < 2 {
            return None;
        }
        let mut sorted = self.latency_samples.clone();
        sorted.sort();
        let idx = ((0.95 * (sorted.len() - 1) as f64).round() as usize).min(sorted.len() - 1);
        Some(sorted[idx])
    }

    /// Compute request success rate. Returns 1.0 if no requests have been made.
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 1.0;
        }
        let successful = self.total_requests.saturating_sub(self.failed_requests);
        successful as f64 / self.total_requests as f64
    }
}

/// Multi-dimensional health score for provider routing (v2).
///
/// Combines latency, success rate, staleness (exponential decay), and PoNW
/// into a single 0.0-1.0 overall score. Each sub-score is independently
/// computed and combined via configurable weights.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthScoreV2 {
    /// Combined overall score (0.0 = worst, 1.0 = best)
    pub overall_score: f64,
    /// Latency component (sigmoid of avg_latency)
    pub latency_score: f64,
    /// Success rate component
    pub success_score: f64,
    /// Staleness component (exponential decay since last heartbeat)
    pub staleness_score: f64,
    /// PoNW component
    pub ponw_score: f64,
}

impl HealthScoreV2 {
    /// Compute health score v2 for a provider.
    ///
    /// Latency score uses a sigmoid: 1 / (1 + e^((latency_ms - 500) / 200))
    ///   - 50ms  -> ~0.94
    ///   - 200ms -> ~0.77
    ///   - 500ms -> ~0.50
    ///   - 1000ms -> ~0.08
    ///
    /// Success score: linear mapping
    ///   - 100% -> 1.0, 90% -> 0.8, 50% -> 0.0, <50% -> 0.0
    ///
    /// Staleness score: exponential decay since last successful check
    ///   - score = e^(-elapsed_minutes / decay_minutes)
    ///   - At 0 min -> 1.0, at decay_minutes -> 0.37, at 3*decay -> 0.05
    ///
    /// PoNW score: work_points normalized to 0-1
    ///
    /// Overall: weighted sum (default: 0.4*latency + 0.3*success + 0.2*staleness + 0.1*ponw)
    pub fn compute(
        provider: &Provider,
        latency_weight: f64,
        success_weight: f64,
        staleness_weight: f64,
        ponw_weight: f64,
        staleness_decay_minutes: f64,
    ) -> Self {
        // -- Latency score: sigmoid function --
        // Use p95 latency if available, fall back to avg, fall back to health poll latency_ms
        let latency_ms = provider
            .p95_latency()
            .map(|d| d.as_millis() as f64)
            .or_else(|| provider.avg_latency().map(|d| d.as_millis() as f64))
            .unwrap_or(provider.latency_ms as f64);

        // Sigmoid: 1 / (1 + e^((x - midpoint) / steepness))
        // midpoint=500ms, steepness=200
        //   50ms  -> ~0.94
        //   200ms -> ~0.77
        //   500ms -> ~0.50
        //   1000ms -> ~0.08
        //   2000ms -> ~0.00
        let latency_score = 1.0 / (1.0 + ((latency_ms - 500.0) / 200.0).exp());

        // -- Success score: linear mapping --
        // 100% -> 1.0, 90% -> 0.8, 50% -> 0.0
        let raw_success = provider.success_rate();
        let success_score = if raw_success >= 1.0 {
            1.0
        } else if raw_success >= 0.5 {
            (raw_success - 0.5) * 2.0 // maps [0.5, 1.0] -> [0.0, 1.0]
        } else {
            0.0
        };

        // -- Staleness score: exponential decay since last successful health check --
        let elapsed_secs = provider.last_successful_check.elapsed().as_secs_f64();
        let elapsed_minutes = elapsed_secs / 60.0;
        let staleness_score = if staleness_decay_minutes > 0.0 {
            (-elapsed_minutes / staleness_decay_minutes).exp()
        } else {
            1.0
        };

        // -- PoNW score: work_points normalized to 0-1 --
        let ponw_score = provider
            .status
            .as_ref()
            .and_then(|s| s.pown_status.as_ref())
            .map(|p| (p.work_points as f64 / 100.0).min(1.0))
            .unwrap_or(0.0);

        // -- Overall: weighted combination --
        let total_weight = latency_weight + success_weight + staleness_weight + ponw_weight;
        let overall_score = if total_weight > 0.0 {
            (latency_weight * latency_score
                + success_weight * success_score
                + staleness_weight * staleness_score
                + ponw_weight * ponw_score)
                / total_weight
        } else {
            0.0
        };

        Self {
            overall_score,
            latency_score,
            success_score,
            staleness_score,
            ponw_score,
        }
    }

    /// Compute health score with default weights.
    pub fn compute_default(provider: &Provider) -> Self {
        Self::compute(provider, 0.4, 0.3, 0.2, 0.1, 5.0)
    }
}

/// Consecutive failures before a provider is considered "degraded" (deprioritized, not removed)
const DEGRADED_THRESHOLD: u32 = 3;

/// Consecutive failures before a provider is removed from the registry
const REMOVAL_THRESHOLD: u32 = 10;

/// The provider registry
pub struct ProviderRegistry {
    /// All known providers keyed by endpoint URL
    pub(crate) providers: DashMap<String, Provider>,
    /// Sticky session map: session_key -> (endpoint, created_at)
    session_map: DashMap<String, StickySession>,
    http_client: Client,
    config: Arc<RelayConfig>,
}

impl ProviderRegistry {
    pub fn new(config: Arc<RelayConfig>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client for provider registry");

        let registry = Self {
            providers: DashMap::<String, Provider>::new(),
            session_map: DashMap::new(),
            http_client,
            config,
        };

        // Seed with known endpoints from static config (bootstrap fallback)
        for endpoint in &registry.config.providers.known_endpoints {
            registry.providers.insert(
                endpoint.clone(),
                Provider {
                    endpoint: endpoint.clone(),
                    status: None,
                    latency_ms: 0,
                    active_requests: Arc::new(std::sync::atomic::AtomicU32::new(0)),
                    is_healthy: false,
                    last_healthy_at: Utc::now(),
                    consecutive_failures: 0,
                    last_health_check: Instant::now(),
                    last_successful_check: Instant::now(),
                    from_chain: false,
                    model_pricing: HashMap::new(),
                    served_models: Vec::new(),
                    pown_score: 0,
                    region: None,
                    circuit_state: CircuitState::Closed,
                    circuit_opened_at: None,
                    half_open_probes: Arc::new(AtomicU32::new(0)),
                    latency_samples: Vec::new(),
                    last_request_at: Instant::now(),
                    total_requests: 0,
                    failed_requests: 0,
                    suspended: std::sync::atomic::AtomicBool::new(false),
                    draining: std::sync::atomic::AtomicBool::new(false),
                },
            );
        }

        registry
    }

    /// Poll a single provider's health
    pub(crate) async fn poll_provider(&self, endpoint: &str) {
        let url = format!("{}/xergon/status", endpoint.trim_end_matches('/'));
        let start = std::time::Instant::now();

        let result = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        let latency_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<XergonAgentStatus>().await {
                    Ok(status) => {
                        let ai_enabled = status
                            .pown_status
                            .as_ref()
                            .map(|p| p.ai_enabled)
                            .unwrap_or(false);

                        if let Some(mut provider) = self.providers.get_mut(endpoint) {
                            provider.status = Some(status);
                            provider.latency_ms = latency_ms;
                            provider.is_healthy = ai_enabled; // Must have AI enabled
                            provider.last_healthy_at = Utc::now();
                            provider.consecutive_failures = 0;
                            provider.last_health_check = Instant::now();
                            provider.last_successful_check = Instant::now();
                            // Reset circuit breaker on successful health check
                            provider.circuit_state = CircuitState::Closed;
                            provider.circuit_opened_at = None;
                            provider.half_open_probes.store(0, Ordering::Relaxed);
                        }
                        debug!(endpoint, latency_ms, ai_enabled, "Provider health check OK");
                    }
                    Err(e) => {
                        warn!(endpoint, error = %e, "Failed to parse provider status");
                        self.mark_unhealthy_or_degrade(endpoint);
                    }
                }
            }
            Ok(resp) => {
                warn!(endpoint, status = %resp.status(), "Provider returned error status");
                self.mark_unhealthy_or_degrade(endpoint);
            }
            Err(e) => {
                warn!(endpoint, error = %e, "Provider health check failed");
                self.mark_unhealthy_or_degrade(endpoint);
            }
        }
    }

    /// Check if a provider is in degraded state (consecutive_failures >= DEGRADED_THRESHOLD)
    pub fn is_degraded(&self, endpoint: &str) -> bool {
        self.providers
            .get(endpoint)
            .map(|p| p.consecutive_failures >= DEGRADED_THRESHOLD)
            .unwrap_or(false)
    }

    /// Number of currently degraded providers (failed >= DEGRADED_THRESHOLD but < REMOVAL_THRESHOLD)
    pub fn degraded_provider_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|p| {
                p.consecutive_failures >= DEGRADED_THRESHOLD
                    && p.consecutive_failures < REMOVAL_THRESHOLD
            })
            .count()
    }

    /// Mark a provider unhealthy and apply SLA deprioritization logic.
    ///
    /// - consecutive_failures < DEGRADED_THRESHOLD: just mark unhealthy, keep in routing pool
    /// - consecutive_failures >= DEGRADED_THRESHOLD: mark as degraded (heavily deprioritized)
    /// - consecutive_failures >= REMOVAL_THRESHOLD: remove from registry entirely
    pub(crate) fn mark_unhealthy_or_degrade(&self, endpoint: &str) {
        if let Some(mut provider) = self.providers.get_mut(endpoint) {
            provider.is_healthy = false;
            provider.consecutive_failures += 1;
            provider.last_health_check = Instant::now();

            let failures = provider.consecutive_failures;

            // Circuit breaker: trip if consecutive failures >= threshold
            let failure_threshold = self.config.relay.circuit_failure_threshold;
            if provider.circuit_state == CircuitState::Closed && failures >= failure_threshold {
                provider.circuit_state = CircuitState::Open;
                provider.circuit_opened_at = Some(Instant::now());
                warn!(
                    endpoint,
                    consecutive_failures = failures,
                    threshold = failure_threshold,
                    "Circuit breaker OPENED for provider"
                );
            }

            if failures >= REMOVAL_THRESHOLD {
                warn!(
                    endpoint,
                    consecutive_failures = failures,
                    threshold = REMOVAL_THRESHOLD,
                    "Provider exceeded removal threshold, removing from registry"
                );
                drop(provider);
                self.providers.remove(endpoint);
            } else if failures == DEGRADED_THRESHOLD {
                warn!(
                    endpoint,
                    consecutive_failures = failures,
                    "Provider marked as degraded (heavily deprioritized)"
                );
            }
        }
    }

    /// Number of currently healthy providers
    pub fn healthy_provider_count(&self) -> usize {
        self.providers.iter().filter(|p| p.is_healthy).count()
    }

    /// Get healthy providers sorted by routing score (best first).
    /// Includes degraded providers (heavily deprioritized but not excluded).
    /// Skips providers with Open circuit state (unless recovery timeout has elapsed,
    /// in which case they transition to HalfOpen and are included).
    pub fn ranked_providers(&self) -> Vec<Provider> {
        let recovery_timeout = Duration::from_secs(self.config.relay.circuit_recovery_timeout_secs);
        let half_open_max = self.config.relay.circuit_half_open_max_probes;

        // First pass: transition Open circuits to HalfOpen if recovery timeout elapsed
        {
            let mut endpoints_to_transition: Vec<String> = Vec::new();
            for entry in self.providers.iter() {
                let p = entry.value();
                if p.circuit_state == CircuitState::Open {
                    if let Some(opened_at) = p.circuit_opened_at {
                        if opened_at.elapsed() >= recovery_timeout {
                            endpoints_to_transition.push(p.endpoint.clone());
                        }
                    }
                }
            }
            for ep in endpoints_to_transition {
                if let Some(mut prov) = self.providers.get_mut(&ep) {
                    prov.circuit_state = CircuitState::HalfOpen;
                    prov.half_open_probes.store(0, Ordering::Relaxed);
                    debug!(
                        endpoint = %ep,
                        "Circuit breaker transitioned to HalfOpen"
                    );
                }
            }
        }

        // Second pass: collect providers, filtering by circuit state
        let mut providers: Vec<Provider> = self
            .providers
            .iter()
            .filter_map(|r| {
                let p = r.value();

                // Skip administratively suspended or draining providers
                if p.suspended.load(Ordering::Relaxed) {
                    return None;
                }
                if p.draining.load(Ordering::Relaxed) {
                    return None;
                }

                // Include healthy providers and degraded providers (below removal threshold)
                if !p.is_healthy && p.consecutive_failures >= REMOVAL_THRESHOLD {
                    return None;
                }

                // Circuit breaker filter
                match p.circuit_state {
                    CircuitState::Open => {
                        // Still in Open state (recovery timeout not elapsed), skip
                        None
                    }
                    CircuitState::HalfOpen => {
                        // Only allow if we have probe capacity
                        if p.half_open_probes.load(Ordering::Relaxed) >= half_open_max {
                            None
                        } else {
                            Some(p.clone())
                        }
                    }
                    CircuitState::Closed => Some(p.clone()),
                }
            })
            .collect();

        providers.sort_by(|a, b| {
            let score_a = self.routing_score(a, None);
            let score_b = self.routing_score(b, None);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        providers
    }

    /// Get healthy providers sorted by routing score for a specific model.
    /// When `model_id` is Some, filters to providers that serve that model
    /// and uses price-aware scoring.
    /// Skips providers with Open circuit state.
    pub fn ranked_providers_for_model(&self, model_id: Option<&str>) -> Vec<Provider> {
        let normalized_model = model_id.map(|m| m.to_lowercase().replace(' ', "-"));
        let recovery_timeout = Duration::from_secs(self.config.relay.circuit_recovery_timeout_secs);
        let half_open_max = self.config.relay.circuit_half_open_max_probes;

        // First pass: transition Open circuits to HalfOpen if recovery timeout elapsed
        {
            let mut endpoints_to_transition: Vec<String> = Vec::new();
            for entry in self.providers.iter() {
                let p = entry.value();
                if p.circuit_state == CircuitState::Open {
                    if let Some(opened_at) = p.circuit_opened_at {
                        if opened_at.elapsed() >= recovery_timeout {
                            endpoints_to_transition.push(p.endpoint.clone());
                        }
                    }
                }
            }
            for ep in endpoints_to_transition {
                if let Some(mut prov) = self.providers.get_mut(&ep) {
                    prov.circuit_state = CircuitState::HalfOpen;
                    prov.half_open_probes.store(0, Ordering::Relaxed);
                    debug!(
                        endpoint = %ep,
                        "Circuit breaker transitioned to HalfOpen"
                    );
                }
            }
        }

        // Second pass: collect providers, filtering by circuit state and model
        let mut providers: Vec<Provider> = self
            .providers
            .iter()
            .filter_map(|r| {
                let p = r.value();

                // Skip administratively suspended or draining providers
                if p.suspended.load(Ordering::Relaxed) {
                    return None;
                }
                if p.draining.load(Ordering::Relaxed) {
                    return None;
                }

                // Include healthy providers and degraded providers (below removal threshold)
                if !p.is_healthy && p.consecutive_failures >= REMOVAL_THRESHOLD {
                    return None;
                }

                // Circuit breaker filter
                match p.circuit_state {
                    CircuitState::Open => {
                        // Still in Open state (recovery timeout not elapsed), skip
                        return None;
                    }
                    CircuitState::HalfOpen => {
                        if p.half_open_probes.load(Ordering::Relaxed) >= half_open_max {
                            return None;
                        }
                    }
                    CircuitState::Closed => {}
                }

                // If a model is requested, only include providers that serve it
                if let Some(ref model) = normalized_model {
                    let serves_model = p.served_models
                        .iter()
                        .any(|sm| sm.to_lowercase().replace(' ', "-") == *model)
                        || p.status.as_ref().and_then(|s| s.pown_status.as_ref())
                            .map(|pown| pown.ai_model.to_lowercase().replace(' ', "-") == *model)
                            .unwrap_or(false);
                    if !serves_model {
                        return None;
                    }
                }

                Some(p.clone())
            })
            .collect();

        // Sort by routing score: higher is better
        let model_ref = normalized_model.as_deref();
        providers.sort_by(|a, b| {
            let score_a = self.routing_score(a, model_ref);
            let score_b = self.routing_score(b, model_ref);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        providers
    }

    /// Calculate routing score for a provider.
    ///
    /// When `health_v2` is enabled (default), uses the multi-dimensional v2
    /// scoring model with latency sigmoid, success rate, exponential staleness
    /// decay, and PoNW. Falls back to v1 when no latency samples exist or
    /// health_v2 is disabled.
    ///
    /// V2 Higher score = preferred. Components (configurable weights):
    ///   - Latency: sigmoid of p95/avg latency (default weight: 40%)
    ///   - Success rate: linear mapping 50-100% -> 0-1 (default weight: 30%)
    ///   - Staleness: exponential decay since last heartbeat (default weight: 20%)
    ///   - PoNW: work_points / 100 (default weight: 10%)
    ///
    /// V1 fallback: PoNW (40%) + latency inverse (35%) + load inverse (25%)
    ///   plus price factor (30%) and degradation/recency penalties.
    fn routing_score(&self, provider: &Provider, model_id: Option<&str>) -> f64 {
        let v2_config = &self.config.health_v2;

        // Use v2 scoring if enabled AND provider has latency data or health poll data
        if v2_config.enabled {
            let has_latency_data = !provider.latency_samples.is_empty() || provider.latency_ms > 0;

            if has_latency_data {
                let v2 = HealthScoreV2::compute(
                    provider,
                    v2_config.latency_weight,
                    v2_config.success_weight,
                    v2_config.staleness_weight,
                    v2_config.ponw_weight,
                    v2_config.staleness_decay_minutes,
                );

                // Apply degradation penalty (same as v1)
                let degradation_factor = if provider.consecutive_failures >= DEGRADED_THRESHOLD {
                    let excess = (provider.consecutive_failures - DEGRADED_THRESHOLD) as f64;
                    let range = (REMOVAL_THRESHOLD - DEGRADED_THRESHOLD) as f64;
                    let base = 0.30;
                    let min_factor = 0.05;
                    base - (base - min_factor) * (excess / range)
                } else {
                    1.0
                };

                // Apply price factor for model-specific routing
                let price_factor = if let Some(model) = model_id {
                    let price = provider.model_pricing.get(model).copied().unwrap_or(0);
                    1.0 / (1.0 + (price as f64) / 100_000_000.0)
                } else {
                    1.0
                };

                // Load factor: still penalize busy providers
                let active = provider
                    .active_requests
                    .load(std::sync::atomic::Ordering::Relaxed) as f64;
                let load_factor = 1.0 / (1.0 + active / 10.0);

                return v2.overall_score * degradation_factor * price_factor * load_factor;
            }
        }

        // --- V1 fallback: legacy scoring ---
        self.routing_score_v1(provider, model_id)
    }

    /// Legacy v1 routing score (original implementation).
    fn routing_score_v1(&self, provider: &Provider, model_id: Option<&str>) -> f64 {
        // PoNW score (0-100, from work_points)
        let pown_score = provider
            .status
            .as_ref()
            .and_then(|s| s.pown_status.as_ref())
            .map(|p| (p.work_points as f64 / 100.0).min(1.0))
            .unwrap_or(0.0);

        // Latency score (inverse, 0=terrible, 1=instant)
        // Use a sigmoid-like curve: score = 1 / (1 + latency/500)
        let latency_score = 1.0 / (1.0 + (provider.latency_ms as f64) / 500.0);

        // Load score (inverse of active requests, 0=busy, 1=idle)
        let active = provider
            .active_requests
            .load(std::sync::atomic::Ordering::Relaxed) as f64;
        let load_score = 1.0 / (1.0 + active / 5.0);

        // Quality score: weighted combination of PoNW, latency, load
        let quality_score = 0.40 * pown_score + 0.35 * latency_score + 0.25 * load_score;

        // Price factor: cheaper = higher score
        // Normalize against ~0.1 ERG (100_000_000 nanoERG) so free = 1.0, expensive = lower
        let price_score = if let Some(model) = model_id {
            let price = provider.model_pricing.get(model).copied().unwrap_or(0);
            1.0 / (1.0 + (price as f64) / 100_000_000.0)
        } else {
            1.0 // no model specified, neutral
        };

        // Combined: quality (70%) + price (30%)
        let base_score = 0.70 * quality_score + 0.30 * price_score;

        // Degradation penalty: providers with consecutive failures get heavily deprioritized
        // - At DEGRADED_THRESHOLD (3 failures): multiply by 0.3
        // - Gradually decrease toward 0.05 as failures approach REMOVAL_THRESHOLD (10)
        let degradation_factor = if provider.consecutive_failures >= DEGRADED_THRESHOLD {
            let excess = (provider.consecutive_failures - DEGRADED_THRESHOLD) as f64;
            let range = (REMOVAL_THRESHOLD - DEGRADED_THRESHOLD) as f64;
            // Linear decay from 0.3 down to 0.05 as failures go from DEGRADED to REMOVAL
            let base = 0.30;
            let min_factor = 0.05;
            base - (base - min_factor) * (excess / range)
        } else {
            1.0
        };

        // Recency penalty: providers not checked recently get slightly lower scores
        // If last health check was > 2x poll interval ago, penalize by up to 0.2
        let poll_interval_secs = self.config.relay.health_poll_interval_secs as f64;
        let recency_factor = if poll_interval_secs > 0.0 {
            let elapsed_secs = provider.last_health_check.elapsed().as_secs_f64();
            let staleness_ratio = elapsed_secs / (2.0 * poll_interval_secs);
            if staleness_ratio > 1.0 {
                1.0 - 0.2 * ((staleness_ratio - 1.0).min(4.0) / 4.0) // decay to 0.8 over 8x poll interval
            } else {
                1.0
            }
        } else {
            1.0
        };

        base_score * degradation_factor * recency_factor
    }

    /// Select the best provider for a request, excluding already-tried endpoints.
    pub fn select_provider(&self, exclude: &[String]) -> Option<Provider> {
        self.ranked_providers()
            .into_iter()
            .find(|p| !exclude.contains(&p.endpoint))
    }

    /// Select the best provider for a specific model, excluding already-tried endpoints.
    ///
    /// Filters providers to those that serve the requested model, then ranks
    /// by price-aware routing score. Falls back to non-model-specific selection
    /// if no providers serve the exact model.
    pub fn select_provider_for_model(
        &self,
        model_id: &str,
        exclude: &[String],
        demand: &DemandTracker,
    ) -> Option<Provider> {
        let normalized_model = model_id.to_lowercase().replace(' ', "-");

        // Try model-aware selection first
        let model_providers = self.ranked_providers_for_model(Some(model_id));
        if let Some(provider) = model_providers.into_iter().find(|p| !exclude.contains(&p.endpoint)) {
            debug!(
                model = %normalized_model,
                endpoint = %provider.endpoint,
                price = provider.model_pricing.get(&normalized_model).copied().unwrap_or(0),
                demand = demand.demand_multiplier(&normalized_model),
                "Selected provider via price-aware routing"
            );
            return Some(provider);
        }

        // Fallback: no providers serve this model specifically; use generic selection
        self.ranked_providers()
            .into_iter()
            .find(|p| !exclude.contains(&p.endpoint))
    }

    /// Increment active requests for a provider, return guard that decrements on drop.
    /// If the provider is in HalfOpen state, also increments the probe counter.
    pub fn acquire_provider(&self, endpoint: &str) -> Option<ProviderRequestGuard> {
        self.providers.get(endpoint).map(|provider| {
            provider
                .active_requests
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            // Track HalfOpen probes
            let is_half_open = provider.circuit_state == CircuitState::HalfOpen;
            if is_half_open {
                provider.half_open_probes.fetch_add(1, Ordering::Relaxed);
            }

            ProviderRequestGuard {
                endpoint: endpoint.to_string(),
                active: provider.active_requests.clone(),
                half_open_probes: if is_half_open {
                    Some(provider.half_open_probes.clone())
                } else {
                    None
                },
            }
        })
    }

    /// Record a successful request to a provider.
    /// Resets the circuit breaker to Closed state and increments request counters.
    pub fn record_success(&self, endpoint: &str) {
        if let Some(mut provider) = self.providers.get_mut(endpoint) {
            // Reset circuit breaker
            if provider.circuit_state != CircuitState::Closed {
                info!(
                    endpoint,
                    previous_state = %provider.circuit_state,
                    "Circuit breaker CLOSED after successful request"
                );
            }
            provider.circuit_state = CircuitState::Closed;
            provider.circuit_opened_at = None;
            provider.consecutive_failures = 0;
            provider.half_open_probes.store(0, Ordering::Relaxed);
            // Track request stats for health v2
            provider.total_requests += 1;
        }
    }

    /// Record a failed request to a provider.
    /// Increments failure count and may trip the circuit breaker.
    pub fn record_failure(&self, endpoint: &str) {
        if let Some(mut provider) = self.providers.get_mut(endpoint) {
            provider.consecutive_failures += 1;
            provider.total_requests += 1;
            provider.failed_requests += 1;
            let failures = provider.consecutive_failures;
            let threshold = self.config.relay.circuit_failure_threshold;

            match provider.circuit_state {
                CircuitState::Closed => {
                    if failures >= threshold {
                        provider.circuit_state = CircuitState::Open;
                        provider.circuit_opened_at = Some(Instant::now());
                        warn!(
                            endpoint,
                            consecutive_failures = failures,
                            threshold,
                            "Circuit breaker OPENED due to request failures"
                        );
                    }
                }
                CircuitState::HalfOpen => {
                    // A failed probe means the provider is still unhealthy, re-open
                    provider.circuit_state = CircuitState::Open;
                    provider.circuit_opened_at = Some(Instant::now());
                    provider.half_open_probes.store(0, Ordering::Relaxed);
                    warn!(
                        endpoint,
                        "HalfOpen probe failed, circuit breaker re-OPENED"
                    );
                }
                CircuitState::Open => {
                    // Already open, just increment failures
                }
            }
        }
    }

    /// Record a latency sample for a proxied request to a provider.
    /// Used by the health v2 scoring model.
    pub fn record_request_latency(&self, endpoint: &str, duration: Duration) {
        let max_samples = self.config.health_v2.max_latency_samples;
        if let Some(mut provider) = self.providers.get_mut(endpoint) {
            provider.record_latency(duration, max_samples);
        }
    }

    /// Compute the health score v2 for a specific provider.
    /// Returns None if the provider is not found.
    pub fn health_score_v2(&self, endpoint: &str) -> Option<HealthScoreV2> {
        let provider = self.providers.get(endpoint)?;
        let v2_config = &self.config.health_v2;
        Some(HealthScoreV2::compute(
            provider.value(),
            v2_config.latency_weight,
            v2_config.success_weight,
            v2_config.staleness_weight,
            v2_config.ponw_weight,
            v2_config.staleness_decay_minutes,
        ))
    }

    /// Derive a session key from request headers.
    /// Uses X-Session-Id header if present, falls back to the client IP.
    pub fn derive_session_key(headers: &axum::http::HeaderMap, client_ip: &str) -> String {
        headers
            .get("x-session-id")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .map(|s| format!("session:{}", s))
            .unwrap_or_else(|| format!("ip:{}", client_ip))
    }

    /// Look up a sticky provider for a given session key and model.
    /// Returns None if no sticky session exists, or if it's expired, or if the
    /// provider is unhealthy / circuit-open.
    pub fn get_sticky_provider(&self, session_key: &str) -> Option<Provider> {
        let ttl = Duration::from_secs(self.config.relay.sticky_session_ttl_secs);

        let session = self.session_map.get(session_key)?;
        if session.is_expired(ttl) {
            drop(session);
            self.session_map.remove(session_key);
            return None;
        }

        let endpoint = session.endpoint.clone();
        drop(session);

        let provider = self.providers.get(&endpoint)?;
        let p = provider.value();

        // Don't use sticky if provider is unhealthy or circuit is open
        if !p.is_healthy || p.circuit_state == CircuitState::Open {
            return None;
        }

        Some(p.clone())
    }

    /// Set a sticky session mapping a session key to a provider endpoint.
    pub fn set_sticky_session(&self, session_key: &str, endpoint: &str) {
        self.session_map.insert(
            session_key.to_string(),
            StickySession {
                endpoint: endpoint.to_string(),
                created_at: Instant::now(),
            },
        );
    }

    /// Get the current number of active sticky sessions.
    #[allow(dead_code)]
    pub fn sticky_session_count(&self) -> usize {
        self.session_map.len()
    }

    /// Prune expired sticky sessions. Called periodically to prevent unbounded growth.
    #[allow(dead_code)]
    pub fn prune_expired_sessions(&self) {
        let ttl = Duration::from_secs(self.config.relay.sticky_session_ttl_secs);
        self.session_map
            .retain(|_, session| !session.is_expired(ttl));
    }

    /// Add a single provider by endpoint URL.
    /// No-op if the endpoint already exists in the registry.
    pub fn add_provider(&self, endpoint: String, from_chain: bool) {
        let ep = endpoint.trim_end_matches('/').to_string();
        if self.providers.contains_key(&ep) {
            debug!(endpoint = %ep, "Provider already in registry, skipping add");
            return;
        }
        info!(endpoint = %ep, from_chain, "Adding provider to registry");
        self.providers.insert(
            ep.clone(),
            Provider {
                endpoint: ep,
                status: None,
                latency_ms: 0,
                active_requests: Arc::new(std::sync::atomic::AtomicU32::new(0)),
                is_healthy: false,
                last_healthy_at: Utc::now(),
                consecutive_failures: 0,
                last_health_check: Instant::now(),
                last_successful_check: Instant::now(),
                from_chain,
                model_pricing: HashMap::new(),
                served_models: Vec::new(),
                pown_score: 0,
                region: None,
                circuit_state: CircuitState::Closed,
                circuit_opened_at: None,
                half_open_probes: Arc::new(AtomicU32::new(0)),
                latency_samples: Vec::new(),
                last_request_at: Instant::now(),
                total_requests: 0,
                failed_requests: 0,
                suspended: std::sync::atomic::AtomicBool::new(false),
                draining: std::sync::atomic::AtomicBool::new(false),
            },
        );
    }

    /// Remove a single provider by endpoint URL.
    /// Only removes providers that were discovered from chain state,
    /// unless `force` is true (which also removes static bootstrap providers).
    /// Returns true if the provider was actually removed.
    pub fn remove_provider(&self, endpoint: &str, force: bool) -> bool {
        let ep = endpoint.trim_end_matches('/');
        if let Some(provider) = self.providers.get(ep) {
            if !provider.from_chain && !force {
                debug!(endpoint = %ep, "Not removing static bootstrap provider");
                return false;
            }
        }
        if self.providers.remove(ep).is_some() {
            info!(endpoint = %ep, "Removed provider from registry");
            true
        } else {
            false
        }
    }

    /// Reconcile the in-memory provider registry with on-chain state.
    ///
    /// - Adds new providers found on-chain (not yet in registry)
    /// - Removes chain-discovered providers that are no longer on-chain,
    ///   but only if they've been unhealthy for > 5 minutes
    /// - Static bootstrap providers (from config known_endpoints) are never removed
    pub fn sync_from_chain(&self, chain_providers: &[ChainProvider]) {
        let chain_endpoints: std::collections::HashSet<&str> = chain_providers
            .iter()
            .map(|cp| cp.endpoint.trim_end_matches('/'))
            .collect();

        // 1. Add new providers discovered on-chain and populate metadata
        for cp in chain_providers {
            self.add_provider(cp.endpoint.clone(), true);
            // Update provider metadata from chain (pricing, served models, reputation, region)
            if let Some(mut provider) = self.providers.get_mut(&cp.endpoint.trim_end_matches('/').to_string()) {
                provider.model_pricing = cp.model_pricing.clone();
                provider.served_models = cp.models.clone();
                provider.pown_score = cp.pown_score;
                if !cp.region.is_empty() && cp.region != "unknown" {
                    provider.region = Some(cp.region.clone());
                }
            }
        }

        // 2. Remove chain-discovered providers no longer on-chain
        //    (only if unhealthy for > 5 minutes to avoid flapping)
        let stale_threshold = Utc::now() - chrono::Duration::minutes(5);
        let mut to_remove: Vec<String> = Vec::new();

        for entry in self.providers.iter() {
            let provider = entry.value();
            // Only consider chain-discovered providers for removal
            if !provider.from_chain {
                continue;
            }
            let ep = provider.endpoint.trim_end_matches('/');
            if !chain_endpoints.contains(ep) {
                // Check if this provider has been unhealthy long enough
                if !provider.is_healthy && provider.last_healthy_at < stale_threshold {
                    to_remove.push(provider.endpoint.clone());
                } else {
                    debug!(
                        endpoint = %provider.endpoint,
                        "Chain provider missing from latest scan, but still within grace period"
                    );
                }
            }
        }

        for ep in to_remove {
            self.providers.remove(&ep);
            info!(endpoint = %ep, "Removed stale chain-discovered provider");
        }

        info!(
            total = self.providers.len(),
            chain_discovered = chain_providers.len(),
            "Chain sync complete"
        );
    }
}

/// RAII guard that decrements active_requests when dropped
pub struct ProviderRequestGuard {
    #[allow(dead_code)] // TODO: could be used for logging/debugging
    endpoint: String,
    active: Arc<std::sync::atomic::AtomicU32>,
    /// If the provider was in HalfOpen state when acquired, this tracks the probe counter
    half_open_probes: Option<Arc<AtomicU32>>,
}

impl Drop for ProviderRequestGuard {
    fn drop(&mut self) {
        self.active
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        // Decrement half-open probe counter when the request completes
        if let Some(ref probes) = self.half_open_probes {
            probes.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Models that providers might serve
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub provider_count: usize,
}

/// Rarity information for a model
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelRarity {
    pub model_id: String,
    pub model_name: String,
    pub provider_count: usize,
    pub rarity_multiplier: f64,
    pub is_rare: bool,
}

/// Collect unique models across all healthy providers
pub fn collect_models(registry: &ProviderRegistry) -> Vec<ModelInfo> {
    let mut models: std::collections::HashMap<String, (String, usize)> =
        std::collections::HashMap::new();

    for provider in registry.ranked_providers() {
        if let Some(ref status) = provider.status {
            if let Some(ref pown) = status.pown_status {
                if pown.ai_enabled && !pown.ai_model.is_empty() {
                    let model_id = pown.ai_model.to_lowercase().replace(' ', "-");
                    let entry = models
                        .entry(model_id.clone())
                        .or_insert_with(|| (pown.ai_model.clone(), 0));
                    entry.1 += 1;
                }
            }
        }
    }

    models
        .into_iter()
        .map(|(id, (name, count))| ModelInfo {
            id,
            name,
            available: count > 0,
            provider_count: count,
        })
        .collect()
}

/// Compute the rarity multiplier for a model based on provider availability.
///
/// Formula: rarity_multiplier = max_multiplier / provider_count
/// Capped at [1.0, max_multiplier].
///
/// A model served by 1 provider with max_multiplier=10.0 gets 10x.
/// A model served by 10 providers gets 1x (no bonus).
pub fn compute_rarity_multiplier(
    provider_count: usize,
    max_multiplier: f64,
) -> f64 {
    if provider_count == 0 {
        return 1.0;
    }
    let raw = max_multiplier / provider_count as f64;
    raw.clamp(1.0, max_multiplier)
}

/// Compute the rarity score for a specific model from the provider registry.
///
/// Returns the rarity multiplier. Returns 1.0 if the model is not found
/// or if rarity bonus is disabled.
pub fn model_rarity_from_registry(
    registry: &ProviderRegistry,
    model: &str,
    max_multiplier: f64,
) -> f64 {
    let normalized_model = model.to_lowercase().replace(' ', "-");

    let mut count: usize = 0;
    for provider in registry.ranked_providers() {
        if let Some(ref status) = provider.status {
            if let Some(ref pown) = status.pown_status {
                if pown.ai_enabled {
                    let pown_model = pown.ai_model.to_lowercase().replace(' ', "-");
                    if pown_model == normalized_model {
                        count += 1;
                    }
                }
            }
        }
    }

    compute_rarity_multiplier(count, max_multiplier)
}

/// Collect all models sorted by rarity (most rare first).
pub fn collect_models_by_rarity(
    registry: &ProviderRegistry,
    max_multiplier: f64,
) -> Vec<ModelRarity> {
    let models = collect_models(registry);
    let mut rarities: Vec<ModelRarity> = models
        .into_iter()
        .map(|m| {
            let mult = compute_rarity_multiplier(m.provider_count, max_multiplier);
            ModelRarity {
                model_id: m.id,
                model_name: m.name,
                provider_count: m.provider_count,
                rarity_multiplier: mult,
                is_rare: mult > 1.5,
            }
        })
        .collect();

    // Sort: highest rarity first
    rarities.sort_by(|a, b| {
        b.rarity_multiplier
            .partial_cmp(&a.rarity_multiplier)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    rarities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthConfig, BalanceConfig, ChainConfig, ProviderSettings, RelaySettings};

    fn test_config(endpoints: Vec<String>) -> Arc<RelayConfig> {
        Arc::new(RelayConfig {
            relay: RelaySettings {
                listen_addr: "0.0.0.0:8080".into(),
                cors_origins: "*".into(),
                health_poll_interval_secs: 30,
                provider_timeout_secs: 30,
                max_fallback_attempts: 3,
                circuit_failure_threshold: 5,
                circuit_recovery_timeout_secs: 30,
                circuit_half_open_max_probes: 2,
                sticky_session_ttl_secs: 1800,
                onboarding_auth_token: None,
            },
            providers: ProviderSettings {
                known_endpoints: endpoints,
            },
            chain: ChainConfig::default(),
            balance: BalanceConfig::default(),
            auth: AuthConfig::default(),
            incentive: crate::config::IncentiveConfig::default(),
            bridge: crate::config::BridgeConfig::default(),
            rate_limit: crate::config::RateLimitConfig::default(),
            oracle: crate::config::OracleConfig::default(),
            free_tier: crate::config::FreeTierConfig::default(),
            events: crate::config::EventsConfig::default(),
            gossip: crate::config::GossipConfig::default(),
            health_v2: crate::config::HealthV2Config::default(),
            ws_chat: crate::config::WsChatConfig::default(),
            dedup: crate::config::DedupConfig::default(),
            cache: crate::config::CacheConfig::default(),
            circuit_breaker: crate::circuit_breaker::CircuitBreakerConfig::default(),
            load_shed: crate::load_shed::LoadShedConfig::default(),
            degradation: crate::degradation::DegradationConfig::default(),
            coalesce: crate::coalesce::CoalesceConfig::default(),
            stream_buffer: crate::stream_buffer::StreamBufferConfig::default(),
            adaptive_routing: crate::config::AdaptiveRoutingConfig::default(),
            telemetry: crate::config::TelemetryConfig::default(),
            admin: crate::config::AdminConfig::default(),
            auto_register: crate::auto_register::AutoRegistrationConfig::default(),
            cache_sync: crate::cache_sync::CacheSyncConfig::default(),
            multi_region: crate::multi_region::RegionConfig::default(),
            ws_pool: crate::config::WsPoolConfig::default(),
        })
    }

    #[test]
    fn test_add_provider_new() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        assert_eq!(registry.providers.len(), 0);

        registry.add_provider("http://1.2.3.4:9099".into(), true);
        assert_eq!(registry.providers.len(), 1);

        let p = registry.providers.get("http://1.2.3.4:9099").unwrap();
        assert!(p.from_chain);
    }

    #[test]
    fn test_add_provider_idempotent() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        registry.add_provider("http://1.2.3.4:9099".into(), true);
        registry.add_provider("http://1.2.3.4:9099".into(), true);
        assert_eq!(registry.providers.len(), 1);
    }

    #[test]
    fn test_add_provider_normalizes_trailing_slash() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        registry.add_provider("http://1.2.3.4:9099/".into(), true);
        assert!(registry.providers.contains_key("http://1.2.3.4:9099"));
    }

    #[test]
    fn test_remove_provider_chain_discovered() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        registry.add_provider("http://1.2.3.4:9099".into(), true);
        assert!(registry.remove_provider("http://1.2.3.4:9099", false));
        assert_eq!(registry.providers.len(), 0);
    }

    #[test]
    fn test_remove_provider_static_not_forced() {
        let registry = ProviderRegistry::new(test_config(vec![
            "http://1.2.3.4:9099".into(),
        ]));
        // Static providers have from_chain = false
        assert!(!registry.remove_provider("http://1.2.3.4:9099", false));
        assert_eq!(registry.providers.len(), 1);
    }

    #[test]
    fn test_remove_provider_static_forced() {
        let registry = ProviderRegistry::new(test_config(vec![
            "http://1.2.3.4:9099".into(),
        ]));
        assert!(registry.remove_provider("http://1.2.3.4:9099", true));
        assert_eq!(registry.providers.len(), 0);
    }

    #[test]
    fn test_sync_from_chain_adds_new() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        let chain_providers = vec![ChainProvider {
            box_id: "box1".into(),
            provider_pk: "02".to_string() + &"00".repeat(32),
            endpoint: "http://1.2.3.4:9099".into(),
            models: vec!["llama-3".into()],
            model_pricing: std::collections::HashMap::new(),
            pown_score: 50,
            last_heartbeat: 100,
            region: "us-east".into(),
            pricing_nanoerg_per_million_tokens: None,
            value_nanoerg: 1_000_000_000,
        }];
        registry.sync_from_chain(&chain_providers);
        assert_eq!(registry.providers.len(), 1);
        assert!(registry
            .providers
            .get("http://1.2.3.4:9099")
            .unwrap()
            .from_chain);
    }

    #[test]
    fn test_sync_from_chain_keeps_static_providers() {
        let registry = ProviderRegistry::new(test_config(vec![
            "http://static:9099".into(),
        ]));
        let chain_providers: Vec<ChainProvider> = vec![];
        registry.sync_from_chain(&chain_providers);
        // Static provider should remain
        assert_eq!(registry.providers.len(), 1);
        assert!(registry.providers.contains_key("http://static:9099"));
    }

    #[test]
    fn test_sync_from_chain_removes_stale_chain_provider() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        // Add a chain-discovered provider that's unhealthy and old
        registry.add_provider("http://old:9099".into(), true);
        {
            let mut p = registry.providers.get_mut("http://old:9099").unwrap();
            p.is_healthy = false;
            p.last_healthy_at = Utc::now() - chrono::Duration::minutes(10);
        }
        // Sync with empty chain state - stale provider should be removed
        registry.sync_from_chain(&[]);
        assert_eq!(registry.providers.len(), 0);
    }

    #[test]
    fn test_sync_from_chain_keeps_recently_healthy_chain_provider() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        // Add a chain-discovered provider that's unhealthy but only recently
        registry.add_provider("http://recent:9099".into(), true);
        {
            let mut p = registry.providers.get_mut("http://recent:9099").unwrap();
            p.is_healthy = false;
            p.last_healthy_at = Utc::now() - chrono::Duration::minutes(2);
        }
        // Sync with empty chain state - recent provider should stay (grace period)
        registry.sync_from_chain(&[]);
        assert_eq!(registry.providers.len(), 1);
    }

    #[test]
    fn test_sync_from_chain_populates_pricing_and_models() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        let mut pricing = HashMap::new();
        pricing.insert("llama-3".to_string(), 50_000u64);
        let chain_providers = vec![ChainProvider {
            box_id: "box1".into(),
            provider_pk: "02".to_string() + &"00".repeat(32),
            endpoint: "http://1.2.3.4:9099".into(),
            models: vec!["llama-3".into(), "mistral-7b".into()],
            model_pricing: pricing,
            pown_score: 50,
            last_heartbeat: 100,
            region: "us-east".into(),
            pricing_nanoerg_per_million_tokens: None,
            value_nanoerg: 1_000_000_000,
        }];
        registry.sync_from_chain(&chain_providers);
        let provider = registry.providers.get("http://1.2.3.4:9099").unwrap();
        assert_eq!(provider.served_models, vec!["llama-3", "mistral-7b"]);
        assert_eq!(provider.model_pricing.get("llama-3").copied(), Some(50_000u64));
    }

    #[test]
    fn test_price_aware_routing_favors_cheaper() {
        let registry = ProviderRegistry::new(test_config(vec![]));

        // Add two providers with different pricing for the same model
        for (ep, price) in [
            ("http://cheap:9099", 10_000u64),
            ("http://expensive:9099", 100_000_000u64),
        ] {
            let mut pricing = HashMap::new();
            pricing.insert("llama-3".to_string(), price);
            let chain_providers = vec![ChainProvider {
                box_id: format!("box-{}", ep),
                provider_pk: "02".to_string() + &"00".repeat(32),
                endpoint: ep.into(),
                models: vec!["llama-3".into()],
                model_pricing: pricing,
                pown_score: 50,
                last_heartbeat: 100,
                region: "us-east".into(),
                pricing_nanoerg_per_million_tokens: None,
                value_nanoerg: 1_000_000_000,
            }];
            registry.sync_from_chain(&chain_providers);
        }

        // Make both providers healthy
        for ep in ["http://cheap:9099", "http://expensive:9099"] {
            let mut p = registry.providers.get_mut(ep).unwrap();
            p.is_healthy = true;
        }

        // Price-aware selection should prefer the cheaper provider
        let demand = DemandTracker::new(300);
        let selected = registry.select_provider_for_model("llama-3", &[], &demand);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().endpoint, "http://cheap:9099");
    }

    #[test]
    fn test_select_provider_for_model_falls_back() {
        let registry = ProviderRegistry::new(test_config(vec![]));

        // Add a provider that doesn't serve the requested model
        let chain_providers = vec![ChainProvider {
            box_id: "box1".into(),
            provider_pk: "02".to_string() + &"00".repeat(32),
            endpoint: "http://generic:9099".into(),
            models: vec!["mistral-7b".into()],
            model_pricing: HashMap::new(),
            pown_score: 50,
            last_heartbeat: 100,
            region: "us-east".into(),
            pricing_nanoerg_per_million_tokens: None,
            value_nanoerg: 1_000_000_000,
        }];
        registry.sync_from_chain(&chain_providers);
        registry
            .providers
            .get_mut("http://generic:9099")
            .unwrap()
            .is_healthy = true;

        // Request a model no one serves — should fall back to generic selection
        let demand = DemandTracker::new(300);
        let selected = registry.select_provider_for_model("nonexistent-model", &[], &demand);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().endpoint, "http://generic:9099");
    }

    #[test]
    fn test_degraded_provider_still_in_routing_pool() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        registry.add_provider("http://p1:9099".into(), true);
        {
            let mut p = registry.providers.get_mut("http://p1:9099").unwrap();
            p.is_healthy = true;
            p.status = Some(XergonAgentStatus {
                provider: None,
                pown_status: None,
                pown_health: None,
            });
        }

        // Initially healthy
        assert_eq!(registry.ranked_providers().len(), 1);

        // Mark as degraded (3 consecutive failures)
        for _ in 0..DEGRADED_THRESHOLD {
            registry.mark_unhealthy_or_degrade("http://p1:9099");
        }
        assert!(registry.is_degraded("http://p1:9099"));
        // Should still be in routing pool (degraded, not removed)
        assert_eq!(registry.ranked_providers().len(), 1);
    }

    #[test]
    fn test_provider_removed_at_removal_threshold() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        registry.add_provider("http://p1:9099".into(), true);

        // Mark failures up to REMOVAL_THRESHOLD
        for _ in 0..REMOVAL_THRESHOLD {
            registry.mark_unhealthy_or_degrade("http://p1:9099");
        }
        // Should be removed
        assert_eq!(registry.providers.len(), 0);
        assert_eq!(registry.ranked_providers().len(), 0);
    }

    #[test]
    fn test_degraded_provider_count() {
        let registry = ProviderRegistry::new(test_config(vec![]));
        registry.add_provider("http://p1:9099".into(), true);
        registry.add_provider("http://p2:9099".into(), true);

        // No degraded providers initially
        assert_eq!(registry.degraded_provider_count(), 0);

        // Degrade p1
        for _ in 0..DEGRADED_THRESHOLD {
            registry.mark_unhealthy_or_degrade("http://p1:9099");
        }
        assert_eq!(registry.degraded_provider_count(), 1);
    }

    #[test]
    fn test_degradation_penalty_in_routing_score() {
        let registry = ProviderRegistry::new(test_config(vec![]));

        // Add two identical providers
        for ep in ["http://healthy:9099", "http://degraded:9099"] {
            registry.add_provider(ep.into(), true);
            let mut p = registry.providers.get_mut(ep).unwrap();
            p.is_healthy = true;
            p.latency_ms = 100;
            p.status = Some(XergonAgentStatus {
                provider: None,
                pown_status: None,
                pown_health: None,
            });
        }

        // Degrade one provider
        for _ in 0..DEGRADED_THRESHOLD {
            registry.mark_unhealthy_or_degrade("http://degraded:9099");
        }

        let providers = registry.ranked_providers();
        assert_eq!(providers.len(), 2);
        // Healthy provider should be ranked first
        assert_eq!(providers[0].endpoint, "http://healthy:9099");
        // Degraded provider should be ranked second
        assert_eq!(providers[1].endpoint, "http://degraded:9099");

        // Verify the score difference is significant
        let score_healthy = registry.routing_score(&providers[0], None);
        let score_degraded = registry.routing_score(&providers[1], None);
        assert!(score_healthy > score_degraded * 2.0,
            "Healthy score ({}) should be much higher than degraded score ({})",
            score_healthy,
            score_degraded
        );
    }

    // ---- Health Score V2 tests ----

    #[test]
    fn test_health_score_v2_latency_sigmoid() {
        // Create a provider with no latency samples but known latency_ms
        let mut provider = Provider {
            endpoint: "http://test:9099".into(),
            status: None,
            latency_ms: 50, // fast
            active_requests: Arc::new(AtomicU32::new(0)),
            is_healthy: true,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
            last_health_check: Instant::now(),
            last_successful_check: Instant::now(),
            from_chain: false,
            model_pricing: HashMap::new(),
            served_models: Vec::new(),
            pown_score: 0,
            region: None,
            circuit_state: CircuitState::Closed,
            circuit_opened_at: None,
            half_open_probes: Arc::new(AtomicU32::new(0)),
            latency_samples: Vec::new(),
            last_request_at: Instant::now(),
            total_requests: 0,
            failed_requests: 0,
            suspended: std::sync::atomic::AtomicBool::new(false),
            draining: std::sync::atomic::AtomicBool::new(false),
        };

        // Fast provider (50ms health poll) should have high latency score
        let v2 = HealthScoreV2::compute_default(&provider);
        assert!(
            v2.latency_score > 0.9,
            "50ms should give latency_score > 0.9, got {}",
            v2.latency_score
        );

        // Slow provider (1000ms)
        provider.latency_ms = 1000;
        let v2 = HealthScoreV2::compute_default(&provider);
        assert!(
            v2.latency_score < 0.1,
            "1000ms should give latency_score < 0.1, got {}",
            v2.latency_score
        );

        // Medium provider (500ms) should be ~0.5
        provider.latency_ms = 500;
        let v2 = HealthScoreV2::compute_default(&provider);
        assert!(
            (v2.latency_score - 0.5).abs() < 0.05,
            "500ms should give latency_score ~0.5, got {}",
            v2.latency_score
        );
    }

    #[test]
    fn test_health_score_v2_p95_latency_preferred_over_health_poll() {
        let mut provider = Provider {
            endpoint: "http://test:9099".into(),
            status: None,
            latency_ms: 50, // health poll says fast
            active_requests: Arc::new(AtomicU32::new(0)),
            is_healthy: true,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
            last_health_check: Instant::now(),
            last_successful_check: Instant::now(),
            from_chain: false,
            model_pricing: HashMap::new(),
            served_models: Vec::new(),
            pown_score: 0,
            region: None,
            circuit_state: CircuitState::Closed,
            circuit_opened_at: None,
            half_open_probes: Arc::new(AtomicU32::new(0)),
            latency_samples: Vec::new(),
            last_request_at: Instant::now(),
            total_requests: 0,
            failed_requests: 0,
            suspended: std::sync::atomic::AtomicBool::new(false),
            draining: std::sync::atomic::AtomicBool::new(false),
        };

        // No samples: should use latency_ms
        let v2_no_samples = HealthScoreV2::compute_default(&provider);
        assert!(v2_no_samples.latency_score > 0.9);

        // Add slow actual request samples
        for _ in 0..10 {
            provider.record_latency(Duration::from_millis(800), 100);
        }

        // With samples, p95 should dominate (800ms is slow)
        let v2_with_samples = HealthScoreV2::compute_default(&provider);
        assert!(
            v2_with_samples.latency_score < v2_no_samples.latency_score,
            "With slow request samples, latency score should be worse than health poll suggests"
        );
    }

    #[test]
    fn test_health_score_v2_success_rate() {
        let provider = Provider {
            endpoint: "http://test:9099".into(),
            status: None,
            latency_ms: 100,
            active_requests: Arc::new(AtomicU32::new(0)),
            is_healthy: true,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
            last_health_check: Instant::now(),
            last_successful_check: Instant::now(),
            from_chain: false,
            model_pricing: HashMap::new(),
            served_models: Vec::new(),
            pown_score: 0,
            region: None,
            circuit_state: CircuitState::Closed,
            circuit_opened_at: None,
            half_open_probes: Arc::new(AtomicU32::new(0)),
            latency_samples: Vec::new(),
            last_request_at: Instant::now(),
            total_requests: 0,
            failed_requests: 0,
            suspended: std::sync::atomic::AtomicBool::new(false),
            draining: std::sync::atomic::AtomicBool::new(false),
        };

        // No requests: success rate = 1.0
        assert_eq!(provider.success_rate(), 1.0);
        let v2 = HealthScoreV2::compute_default(&provider);
        assert_eq!(v2.success_score, 1.0);
    }

    #[test]
    fn test_health_score_v2_staleness_decay() {
        let mut provider = Provider {
            endpoint: "http://test:9099".into(),
            status: None,
            latency_ms: 100,
            active_requests: Arc::new(AtomicU32::new(0)),
            is_healthy: true,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
            last_health_check: Instant::now(),
            last_successful_check: Instant::now(),
            from_chain: false,
            model_pricing: HashMap::new(),
            served_models: Vec::new(),
            pown_score: 0,
            region: None,
            circuit_state: CircuitState::Closed,
            circuit_opened_at: None,
            half_open_probes: Arc::new(AtomicU32::new(0)),
            latency_samples: Vec::new(),
            last_request_at: Instant::now(),
            total_requests: 0,
            failed_requests: 0,
            suspended: std::sync::atomic::AtomicBool::new(false),
            draining: std::sync::atomic::AtomicBool::new(false),
        };

        // Fresh: staleness should be ~1.0
        let v2_fresh = HealthScoreV2::compute_default(&provider);
        assert!(
            v2_fresh.staleness_score > 0.99,
            "Fresh provider staleness should be ~1.0, got {}",
            v2_fresh.staleness_score
        );

        // Simulate staleness by manipulating last_successful_check
        // We can't actually backdate Instant, but we can test the formula
        // by using a different decay_minutes parameter
        let v2_no_decay = HealthScoreV2::compute(&provider, 0.4, 0.3, 0.2, 0.1, 0.001);
        // With very short decay, even a tiny elapsed time decays
        assert!(
            v2_no_decay.staleness_score < 1.0,
            "With tiny decay constant, staleness should be < 1.0"
        );
    }

    #[test]
    fn test_health_score_v2_overall_in_range() {
        let provider = Provider {
            endpoint: "http://test:9099".into(),
            status: Some(XergonAgentStatus {
                provider: None,
                pown_status: Some(AgentPownInfo {
                    work_points: 75,
                    ai_enabled: true,
                    ai_model: "llama-3".into(),
                    ai_total_requests: 1000,
                    ai_total_tokens: 500_000,
                    node_id: "node1".into(),
                }),
                pown_health: None,
            }),
            latency_ms: 100,
            active_requests: Arc::new(AtomicU32::new(0)),
            is_healthy: true,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
            last_health_check: Instant::now(),
            last_successful_check: Instant::now(),
            from_chain: false,
            model_pricing: HashMap::new(),
            served_models: Vec::new(),
            pown_score: 0,
            region: None,
            circuit_state: CircuitState::Closed,
            circuit_opened_at: None,
            half_open_probes: Arc::new(AtomicU32::new(0)),
            latency_samples: Vec::new(),
            last_request_at: Instant::now(),
            total_requests: 0,
            failed_requests: 0,
            suspended: std::sync::atomic::AtomicBool::new(false),
            draining: std::sync::atomic::AtomicBool::new(false),
        };

        let v2 = HealthScoreV2::compute_default(&provider);
        assert!(
            (0.0..=1.0).contains(&v2.overall_score),
            "Overall score should be in [0, 1], got {}",
            v2.overall_score
        );
        assert!(
            (0.0..=1.0).contains(&v2.latency_score),
            "Latency score should be in [0, 1], got {}",
            v2.latency_score
        );
        assert!(
            (0.0..=1.0).contains(&v2.success_score),
            "Success score should be in [0, 1], got {}",
            v2.success_score
        );
        assert!(
            (0.0..=1.0).contains(&v2.staleness_score),
            "Staleness score should be in [0, 1], got {}",
            v2.staleness_score
        );
        assert!(
            (0.0..=1.0).contains(&v2.ponw_score),
            "PoNW score should be in [0, 1], got {}",
            v2.ponw_score
        );
        // With 75 work_points, ponw should be 0.75
        assert!(
            (v2.ponw_score - 0.75).abs() < 0.01,
            "PoNW should be ~0.75, got {}",
            v2.ponw_score
        );
    }

    #[test]
    fn test_health_score_v2_routing_prefers_low_latency() {
        let registry = ProviderRegistry::new(test_config(vec![]));

        // Add two providers with different latencies
        for (ep, latency) in [
            ("http://fast:9099", 50u64),
            ("http://slow:9099", 1000u64),
        ] {
            registry.add_provider(ep.into(), true);
            let mut p = registry.providers.get_mut(ep).unwrap();
            p.is_healthy = true;
            p.latency_ms = latency;
            p.status = Some(XergonAgentStatus {
                provider: None,
                pown_status: None,
                pown_health: None,
            });
        }

        let providers = registry.ranked_providers();
        assert_eq!(providers.len(), 2);
        // Fast provider should rank first
        assert_eq!(providers[0].endpoint, "http://fast:9099");

        let score_fast = registry.routing_score(&providers[0], None);
        let score_slow = registry.routing_score(&providers[1], None);
        assert!(
            score_fast > score_slow,
            "Fast provider ({}) should score higher than slow ({})",
            score_fast,
            score_slow
        );
    }

    #[test]
    fn test_health_score_v2_fallback_to_v1_when_disabled() {
        // Create a config with v2 disabled
        let mut config = test_config(vec![]).as_ref().clone();
        config.health_v2.enabled = false;
        let config = Arc::new(config);

        let registry = ProviderRegistry::new(config);

        // Add a provider
        registry.add_provider("http://p1:9099".into(), true);
        {
            let mut p = registry.providers.get_mut("http://p1:9099").unwrap();
            p.is_healthy = true;
            p.latency_ms = 100;
            p.status = Some(XergonAgentStatus {
                provider: None,
                pown_status: None,
                pown_health: None,
            });
        }

        // Should still produce a valid score (v1 fallback)
        let providers = registry.ranked_providers();
        assert_eq!(providers.len(), 1);
        let score = registry.routing_score(&providers[0], None);
        assert!(
            score > 0.0,
            "V1 fallback should produce a positive score, got {}",
            score
        );
    }

    #[test]
    fn test_provider_latency_tracking() {
        let mut provider = Provider {
            endpoint: "http://test:9099".into(),
            status: None,
            latency_ms: 0,
            active_requests: Arc::new(AtomicU32::new(0)),
            is_healthy: true,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
            last_health_check: Instant::now(),
            last_successful_check: Instant::now(),
            from_chain: false,
            model_pricing: HashMap::new(),
            served_models: Vec::new(),
            pown_score: 0,
            region: None,
            circuit_state: CircuitState::Closed,
            circuit_opened_at: None,
            half_open_probes: Arc::new(AtomicU32::new(0)),
            latency_samples: Vec::new(),
            last_request_at: Instant::now(),
            total_requests: 0,
            failed_requests: 0,
            suspended: std::sync::atomic::AtomicBool::new(false),
            draining: std::sync::atomic::AtomicBool::new(false),
        };

        // No samples initially
        assert!(provider.avg_latency().is_none());
        assert!(provider.p95_latency().is_none());

        // Record some latencies
        for ms in [50u64, 100, 150, 200, 250] {
            provider.record_latency(Duration::from_millis(ms), 100);
        }

        // Check avg
        let avg = provider.avg_latency().unwrap();
        assert_eq!(avg.as_millis(), 150); // (50+100+150+200+250)/5

        // Check p95 (95th percentile of 5 values: index = round(0.95*4) = 4 -> 250ms)
        let p95 = provider.p95_latency().unwrap();
        assert_eq!(p95.as_millis(), 250);

        // Test bounded window: add more samples beyond max
        for ms in [10u64; 200].iter() {
            provider.record_latency(Duration::from_millis(*ms), 100);
        }

        // Should only keep last 100 samples
        assert_eq!(provider.latency_samples.len(), 100);
    }

    #[test]
    fn test_provider_success_rate() {
        let provider = Provider {
            endpoint: "http://test:9099".into(),
            status: None,
            latency_ms: 0,
            active_requests: Arc::new(AtomicU32::new(0)),
            is_healthy: true,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
            last_health_check: Instant::now(),
            last_successful_check: Instant::now(),
            from_chain: false,
            model_pricing: HashMap::new(),
            served_models: Vec::new(),
            pown_score: 0,
            region: None,
            circuit_state: CircuitState::Closed,
            circuit_opened_at: None,
            half_open_probes: Arc::new(AtomicU32::new(0)),
            latency_samples: Vec::new(),
            last_request_at: Instant::now(),
            total_requests: 10,
            failed_requests: 1,
            suspended: std::sync::atomic::AtomicBool::new(false),
            draining: std::sync::atomic::AtomicBool::new(false),
        };

        assert!((provider.success_rate() - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_health_v2_config_defaults() {
        let config = crate::config::HealthV2Config::default();
        assert!(config.enabled);
        assert!((config.latency_weight - 0.4).abs() < 0.001);
        assert!((config.success_weight - 0.3).abs() < 0.001);
        assert!((config.staleness_weight - 0.2).abs() < 0.001);
        assert!((config.ponw_weight - 0.1).abs() < 0.001);
        assert!((config.staleness_decay_minutes - 5.0).abs() < 0.001);
        assert_eq!(config.max_latency_samples, 100);
    }
}
