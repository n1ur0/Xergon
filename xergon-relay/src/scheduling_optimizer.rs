//! Request Scheduling Optimizer.
//!
//! Optimizes request routing decisions based on cost, latency, provider health,
//! and fairness constraints.
//!
//! Endpoints:
//!   POST /api/schedule               -- schedule a request
//!   GET  /api/schedule/strategy      -- get current strategy
//!   PUT  /api/schedule/strategy      -- set strategy
//!   GET  /api/schedule/metrics       -- get scheduling metrics
//!   POST /api/schedule/provider-health -- update provider health
//!   POST /api/schedule/reset         -- reset metrics

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Scheduling strategy enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SchedulingStrategy {
    LowestCost,
    LowestLatency,
    Balanced,
    RoundRobin,
    ProviderAffinity,
}

/// Scheduling constraints for a request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulingConstraints {
    #[serde(default)]
    pub max_cost_per_request: Option<f64>,
    #[serde(default)]
    pub max_latency_ms: Option<u64>,
    #[serde(default)]
    pub preferred_providers: Vec<String>,
    #[serde(default)]
    pub excluded_providers: Vec<String>,
    #[serde(default = "default_priority_level")]
    pub priority_level: String,
}

fn default_priority_level() -> String {
    "normal".to_string()
}

/// Per-provider scoring result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderScore {
    pub provider_id: String,
    pub cost_score: f64,
    pub latency_score: f64,
    pub health_score: f64,
    pub fairness_score: f64,
    pub composite_score: f64,
}

/// Result of a scheduling decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingDecision {
    pub request_id: String,
    pub selected_provider: String,
    pub strategy_used: SchedulingStrategy,
    pub scores: Vec<ProviderScore>,
    pub estimated_cost: f64,
    pub estimated_latency_ms: u64,
    pub reason: String,
}

/// Aggregated scheduling metrics snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct SchedulingMetricsSnapshot {
    pub total_decisions: u64,
    pub avg_cost_savings_pct: f64,
    pub avg_latency_ms: f64,
    pub strategy_distribution: HashMap<String, u64>,
    pub fair_share_violations: u64,
}

// ---------------------------------------------------------------------------
// Request / Response types for HTTP handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ScheduleRequest {
    pub request_id: String,
    pub providers: Vec<ProviderCostLatency>,
    #[serde(default)]
    pub constraints: SchedulingConstraints,
    #[serde(default)]
    pub strategy: Option<SchedulingStrategy>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderCostLatency {
    pub provider_id: String,
    pub cost: f64,
    pub latency_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHealthRequest {
    pub provider_id: String,
    pub health_score: f64,
}

#[derive(Debug, Deserialize)]
pub struct SetStrategyRequest {
    pub constraints: SchedulingConstraints,
}

// ---------------------------------------------------------------------------
// SchedulingOptimizer
// ---------------------------------------------------------------------------

/// Core scheduling optimizer engine.
pub struct SchedulingOptimizer {
    provider_scores: DashMap<String, ProviderScore>,
    request_counts: DashMap<String, AtomicU64>,
    strategy: std::sync::RwLock<SchedulingConstraints>,
    current_strategy_name: std::sync::RwLock<SchedulingStrategy>,
    total_decisions: AtomicU64,
    total_cost_savings: AtomicU64, // stored as basis points * 100 (e.g., 15.5% = 1550)
    total_latency: AtomicU64,
    fair_violations: AtomicU64,
    strategy_counts: DashMap<String, AtomicU64>,
    round_robin_index: AtomicU64,
}

impl SchedulingOptimizer {
    /// Create a new optimizer with default balanced strategy.
    pub fn new(constraints: SchedulingConstraints) -> Self {
        Self {
            provider_scores: DashMap::new(),
            request_counts: DashMap::new(),
            strategy: std::sync::RwLock::new(constraints),
            current_strategy_name: std::sync::RwLock::new(SchedulingStrategy::Balanced),
            total_decisions: AtomicU64::new(0),
            total_cost_savings: AtomicU64::new(0),
            total_latency: AtomicU64::new(0),
            fair_violations: AtomicU64::new(0),
            strategy_counts: DashMap::new(),
            round_robin_index: AtomicU64::new(0),
        }
    }

    /// Create with default constraints.
    pub fn default() -> Self {
        Self::new(SchedulingConstraints::default())
    }

    /// Schedule a request to the best provider.
    ///
    /// `providers_with_costs` is a list of (provider_id, cost, latency_ms).
    pub fn schedule(
        &self,
        request_id: &str,
        providers_with_costs: Vec<(String, f64, u64)>,
    ) -> SchedulingDecision {
        if providers_with_costs.is_empty() {
            return SchedulingDecision {
                request_id: request_id.to_string(),
                selected_provider: String::new(),
                strategy_used: SchedulingStrategy::Balanced,
                scores: vec![],
                estimated_cost: 0.0,
                estimated_latency_ms: 0,
                reason: "No providers available".to_string(),
            };
        }

        let strategy_name = {
            let guard = self.current_strategy_name.read().unwrap();
            guard.clone()
        };
        let constraints = {
            let guard = self.strategy.read().unwrap();
            (*guard).clone()
        };

        // Filter excluded providers
        let eligible: Vec<(String, f64, u64)> = providers_with_costs
            .into_iter()
            .filter(|(pid, _, _)| !constraints.excluded_providers.contains(pid))
            .collect();

        if eligible.is_empty() {
            return SchedulingDecision {
                request_id: request_id.to_string(),
                selected_provider: String::new(),
                strategy_used: strategy_name.clone(),
                scores: vec![],
                estimated_cost: 0.0,
                estimated_latency_ms: 0,
                reason: "All providers excluded by constraints".to_string(),
            };
        }

        // Compute scores for each provider
        let mut scored: Vec<ProviderScore> = eligible
            .iter()
            .map(|(pid, cost, latency)| {
                let cost_score = self.compute_cost_score(*cost, &eligible);
                let latency_score = self.compute_latency_score(*latency, &eligible);
                let health_score = self.get_provider_health(pid);
                let fairness_score = self.compute_fairness_score(pid);
                let composite = self.compute_composite(
                    cost_score,
                    latency_score,
                    health_score,
                    fairness_score,
                    &strategy_name,
                );

                ProviderScore {
                    provider_id: pid.clone(),
                    cost_score,
                    latency_score,
                    health_score,
                    fairness_score,
                    composite_score: composite,
                }
            })
            .collect();

        // Sort by composite score (descending)
        scored.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply cost constraint filter
        if let Some(max_cost) = constraints.max_cost_per_request {
            scored.retain(|s| {
                eligible.iter().any(|(pid, cost, _)| *pid == s.provider_id && *cost <= max_cost)
            });
            if scored.is_empty() {
                return SchedulingDecision {
                    request_id: request_id.to_string(),
                    selected_provider: String::new(),
                    strategy_used: strategy_name.clone(),
                    scores: vec![],
                    estimated_cost: 0.0,
                    estimated_latency_ms: 0,
                    reason: "No providers within cost constraint".to_string(),
                };
            }
        }

        // Apply latency constraint filter
        if let Some(max_latency) = constraints.max_latency_ms {
            scored.retain(|s| {
                eligible.iter().any(|(pid, _, lat)| *pid == s.provider_id && *lat <= max_latency)
            });
            if scored.is_empty() {
                return SchedulingDecision {
                    request_id: request_id.to_string(),
                    selected_provider: String::new(),
                    strategy_used: strategy_name.clone(),
                    scores: vec![],
                    estimated_cost: 0.0,
                    estimated_latency_ms: 0,
                    reason: "No providers within latency constraint".to_string(),
                };
            }
        }

        // Check for preferred providers
        let selected = if !constraints.preferred_providers.is_empty() {
            scored
                .iter()
                .find(|s| constraints.preferred_providers.contains(&s.provider_id))
                .cloned()
                .unwrap_or_else(|| scored[0].clone())
        } else {
            scored[0].clone()
        };

        // Get cost and latency for the selected provider
        let (est_cost, est_latency) = eligible
            .iter()
            .find(|(pid, _, _)| *pid == selected.provider_id)
            .map(|(_, c, l)| (*c, *l))
            .unwrap_or((0.0, 0));

        // Compute cost savings (vs. most expensive provider)
        let max_cost = eligible.iter().map(|(_, c, _)| *c).fold(0.0_f64, f64::max);
        let savings_pct = if max_cost > 0.0 {
            ((max_cost - est_cost) / max_cost * 100.0).max(0.0)
        } else {
            0.0
        };

        // Record decision
        self.total_decisions.fetch_add(1, Ordering::Relaxed);
        let savings_bps = ((savings_pct * 10.0).ceil() as u64).max(0);
        self.total_cost_savings.fetch_add(savings_bps, Ordering::Relaxed);
        self.total_latency.fetch_add(est_latency, Ordering::Relaxed);

        // Record strategy distribution
        let strategy_key = format!("{:?}", strategy_name);
        if let Some(mut count) = self.strategy_counts.get_mut(&strategy_key) {
            count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.strategy_counts
                .insert(strategy_key.clone(), AtomicU64::new(1));
        }

        // Update request count for fairness tracking
        self.increment_request_count(&selected.provider_id);

        // Check fairness violation (one provider getting >60% of requests)
        let total = self.total_decisions.load(Ordering::Relaxed);
        if total > 10 {
            if let Some(count) = self.request_counts.get(&selected.provider_id) {
                let provider_count = count.load(Ordering::Relaxed);
                if provider_count as f64 / total as f64 > 0.6 {
                    self.fair_violations.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Round robin index update
        if strategy_name == SchedulingStrategy::RoundRobin {
            self.round_robin_index.fetch_add(1, Ordering::Relaxed);
        }

        SchedulingDecision {
            request_id: request_id.to_string(),
            selected_provider: selected.provider_id,
            strategy_used: strategy_name,
            scores: scored,
            estimated_cost: est_cost,
            estimated_latency_ms: est_latency,
            reason: "Best composite score".to_string(),
        }
    }

    /// Compute cost score (0-1, higher = cheaper).
    fn compute_cost_score(&self, cost: f64, all: &[(String, f64, u64)]) -> f64 {
        let max_cost = all.iter().map(|(_, c, _)| *c).fold(0.0_f64, f64::max);
        let min_cost = all.iter().map(|(_, c, _)| *c).fold(f64::MAX, f64::min);

        if (max_cost - min_cost).abs() < f64::EPSILON {
            return 1.0;
        }

        ((max_cost - cost) / (max_cost - min_cost)).clamp(0.0, 1.0)
    }

    /// Compute latency score (0-1, higher = faster).
    fn compute_latency_score(&self, latency: u64, all: &[(String, f64, u64)]) -> f64 {
        let max_lat = all.iter().map(|(_, _, l)| *l).max().unwrap_or(1).max(1);
        let min_lat = all.iter().map(|(_, _, l)| *l).min().unwrap_or(1).max(1);

        if max_lat == min_lat {
            return 1.0;
        }

        ((max_lat - latency) as f64 / (max_lat - min_lat) as f64).clamp(0.0, 1.0)
    }

    /// Get provider health score from cached data.
    fn get_provider_health(&self, provider_id: &str) -> f64 {
        self.provider_scores
            .get(provider_id)
            .map(|s| s.value().health_score)
            .unwrap_or(0.5) // default neutral health
    }

    /// Compute fairness score (0-1, higher = fewer recent requests = more deserving).
    fn compute_fairness_score(&self, provider_id: &str) -> f64 {
        let count = self
            .request_counts
            .get(provider_id)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0);

        // Score decreases as count increases; capped at 0.0
        (1.0 - (count as f64 / 1000.0)).max(0.0)
    }

    /// Compute composite score based on strategy.
    fn compute_composite(
        &self,
        cost: f64,
        latency: f64,
        health: f64,
        fairness: f64,
        strategy: &SchedulingStrategy,
    ) -> f64 {
        match strategy {
            SchedulingStrategy::LowestCost => {
                cost * 0.7 + latency * 0.1 + health * 0.1 + fairness * 0.1
            }
            SchedulingStrategy::LowestLatency => {
                cost * 0.1 + latency * 0.7 + health * 0.1 + fairness * 0.1
            }
            SchedulingStrategy::Balanced => {
                cost * 0.3 + latency * 0.3 + health * 0.25 + fairness * 0.15
            }
            SchedulingStrategy::RoundRobin => {
                fairness * 0.8 + health * 0.2
            }
            SchedulingStrategy::ProviderAffinity => {
                health * 0.5 + fairness * 0.3 + cost * 0.1 + latency * 0.1
            }
        }
    }

    /// Increment request count for a provider.
    fn increment_request_count(&self, provider_id: &str) {
        if let Some(mut count) = self.request_counts.get_mut(provider_id) {
            count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.request_counts
                .insert(provider_id.to_string(), AtomicU64::new(1));
        }
    }

    /// Update provider health score.
    pub fn update_provider_health(&self, provider_id: &str, health_score: f64) {
        let clamped = health_score.clamp(0.0, 1.0);

        if let Some(mut entry) = self.provider_scores.get_mut(provider_id) {
            entry.value_mut().health_score = clamped;
        } else {
            self.provider_scores.insert(
                provider_id.to_string(),
                ProviderScore {
                    provider_id: provider_id.to_string(),
                    cost_score: 0.5,
                    latency_score: 0.5,
                    health_score: clamped,
                    fairness_score: 0.5,
                    composite_score: 0.5,
                },
            );
        }
    }

    /// Update provider latency (stored in provider_scores).
    pub fn update_provider_latency(&self, provider_id: &str, latency_ms: u64) {
        let latency_score = (1.0 - (latency_ms as f64 / 10000.0)).clamp(0.0, 1.0);

        if let Some(mut entry) = self.provider_scores.get_mut(provider_id) {
            entry.value_mut().latency_score = latency_score;
        } else {
            self.provider_scores.insert(
                provider_id.to_string(),
                ProviderScore {
                    provider_id: provider_id.to_string(),
                    cost_score: 0.5,
                    latency_score,
                    health_score: 0.5,
                    fairness_score: 0.5,
                    composite_score: 0.5,
                },
            );
        }
    }

    /// Set scheduling constraints.
    pub fn set_strategy(&self, constraints: SchedulingConstraints) {
        let mut guard = self.strategy.write().unwrap();
        *guard = constraints;
    }

    /// Get current scheduling constraints.
    pub fn get_strategy(&self) -> SchedulingConstraints {
        let guard = self.strategy.read().unwrap();
        guard.clone()
    }

    /// Set the scheduling strategy algorithm.
    pub fn set_strategy_name(&self, strategy: SchedulingStrategy) {
        let mut guard = self.current_strategy_name.write().unwrap();
        *guard = strategy;
    }

    /// Get current scheduling strategy name.
    pub fn get_strategy_name(&self) -> SchedulingStrategy {
        let guard = self.current_strategy_name.read().unwrap();
        guard.clone()
    }

    /// Get aggregated metrics snapshot.
    pub fn get_metrics(&self) -> SchedulingMetricsSnapshot {
        let total_decisions = self.total_decisions.load(Ordering::Relaxed);
        let total_savings_bps = self.total_cost_savings.load(Ordering::Relaxed);
        let total_latency = self.total_latency.load(Ordering::Relaxed);

        let avg_savings = if total_decisions > 0 {
            total_savings_bps as f64 / (total_decisions as f64 * 10.0)
        } else {
            0.0
        };

        let avg_latency = if total_decisions > 0 {
            total_latency as f64 / total_decisions as f64
        } else {
            0.0
        };

        let mut strategy_distribution = HashMap::new();
        for entry in self.strategy_counts.iter() {
            strategy_distribution.insert(
                entry.key().clone(),
                entry.value().load(Ordering::Relaxed),
            );
        }

        SchedulingMetricsSnapshot {
            total_decisions,
            avg_cost_savings_pct: avg_savings,
            avg_latency_ms: avg_latency,
            strategy_distribution,
            fair_share_violations: self.fair_violations.load(Ordering::Relaxed),
        }
    }

    /// Reset all metrics.
    pub fn reset_metrics(&self) {
        self.total_decisions.store(0, Ordering::Relaxed);
        self.total_cost_savings.store(0, Ordering::Relaxed);
        self.total_latency.store(0, Ordering::Relaxed);
        self.fair_violations.store(0, Ordering::Relaxed);
        self.strategy_counts.clear();
        self.request_counts.clear();
        self.round_robin_index.store(0, Ordering::Relaxed);
    }

    /// Get current round robin index (for testing).
    pub fn get_round_robin_index(&self) -> u64 {
        self.round_robin_index.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// POST /api/schedule
async fn schedule_handler(
    State(state): State<AppState>,
    Json(body): Json<ScheduleRequest>,
) -> impl IntoResponse {
    let optimizer = &state.scheduling_optimizer;

    // Override strategy if specified
    if let Some(ref strategy) = body.strategy {
        optimizer.set_strategy_name(strategy.clone());
    }

    let providers: Vec<(String, f64, u64)> = body
        .providers
        .iter()
        .map(|p| (p.provider_id.clone(), p.cost, p.latency_ms))
        .collect();

    // Apply constraints temporarily
    if !body.constraints.preferred_providers.is_empty()
        || !body.constraints.excluded_providers.is_empty()
        || body.constraints.max_cost_per_request.is_some()
        || body.constraints.max_latency_ms.is_some()
    {
        optimizer.set_strategy(body.constraints.clone());
    }

    let decision = optimizer.schedule(&body.request_id, providers);
    (StatusCode::OK, Json(decision))
}

/// GET /api/schedule/strategy
async fn get_strategy_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let optimizer = &state.scheduling_optimizer;
    Json(serde_json::json!({
        "strategy": format!("{:?}", optimizer.get_strategy_name()),
        "constraints": optimizer.get_strategy(),
    }))
}

/// PUT /api/schedule/strategy
async fn set_strategy_handler(
    State(state): State<AppState>,
    Json(body): Json<SetStrategyRequest>,
) -> impl IntoResponse {
    state.scheduling_optimizer.set_strategy(body.constraints);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "updated" })),
    )
}

/// GET /api/schedule/metrics
async fn metrics_handler(
    State(state): State<AppState>,
) -> Json<SchedulingMetricsSnapshot> {
    Json(state.scheduling_optimizer.get_metrics())
}

/// POST /api/schedule/provider-health
async fn provider_health_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateHealthRequest>,
) -> impl IntoResponse {
    state
        .scheduling_optimizer
        .update_provider_health(&body.provider_id, body.health_score);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "provider_id": body.provider_id,
            "health_score": body.health_score,
        })),
    )
}

/// POST /api/schedule/reset
async fn reset_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.scheduling_optimizer.reset_metrics();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "metrics reset" })),
    )
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the scheduling optimizer router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/schedule", post(schedule_handler))
        .route("/api/schedule/strategy", get(get_strategy_handler))
        .route("/api/schedule/strategy", put(set_strategy_handler))
        .route("/api/schedule/metrics", get(metrics_handler))
        .route("/api/schedule/provider-health", post(provider_health_handler))
        .route("/api/schedule/reset", post(reset_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_optimizer() -> SchedulingOptimizer {
        SchedulingOptimizer::default()
    }

    fn sample_providers() -> Vec<(String, f64, u64)> {
        vec![
            ("prov-a".to_string(), 0.01, 100),
            ("prov-b".to_string(), 0.005, 200),
            ("prov-c".to_string(), 0.02, 50),
        ]
    }

    #[test]
    fn test_schedule_selects_provider() {
        let opt = make_optimizer();
        let decision = opt.schedule("req-1", sample_providers());
        assert!(!decision.selected_provider.is_empty());
        assert!(!decision.reason.is_empty());
        assert_eq!(decision.request_id, "req-1");
    }

    #[test]
    fn test_schedule_empty_providers() {
        let opt = make_optimizer();
        let decision = opt.schedule("req-1", vec![]);
        assert!(decision.selected_provider.is_empty());
        assert_eq!(decision.reason, "No providers available");
    }

    #[test]
    fn test_schedule_lowest_cost_strategy() {
        let opt = make_optimizer();
        opt.set_strategy_name(SchedulingStrategy::LowestCost);
        let decision = opt.schedule("req-1", sample_providers());
        // prov-b is cheapest (0.005)
        assert_eq!(decision.selected_provider, "prov-b");
    }

    #[test]
    fn test_schedule_lowest_latency_strategy() {
        let opt = make_optimizer();
        opt.set_strategy_name(SchedulingStrategy::LowestLatency);
        let decision = opt.schedule("req-1", sample_providers());
        // prov-c has lowest latency (50ms)
        assert_eq!(decision.selected_provider, "prov-c");
    }

    #[test]
    fn test_schedule_balanced_strategy() {
        let opt = make_optimizer();
        opt.set_strategy_name(SchedulingStrategy::Balanced);
        let decision = opt.schedule("req-1", sample_providers());
        assert!(!decision.selected_provider.is_empty());
        assert_eq!(decision.strategy_used, SchedulingStrategy::Balanced);
    }

    #[test]
    fn test_schedule_round_robin_distributes() {
        let opt = make_optimizer();
        opt.set_strategy_name(SchedulingStrategy::RoundRobin);
        let providers = vec![
            ("prov-a".to_string(), 0.01, 100),
            ("prov-b".to_string(), 0.01, 100),
        ];

        let mut selections: HashMap<String, u32> = HashMap::new();
        for i in 0..10 {
            let d = opt.schedule(&format!("req-{}", i), providers.clone());
            *selections.entry(d.selected_provider.clone()).or_insert(0) += 1;
        }

        // Both providers should get some requests
        assert!(selections.contains_key("prov-a"));
        assert!(selections.contains_key("prov-b"));
    }

    #[test]
    fn test_schedule_with_excluded_providers() {
        let opt = make_optimizer();
        let constraints = SchedulingConstraints {
            excluded_providers: vec!["prov-a".to_string()],
            ..Default::default()
        };
        opt.set_strategy(constraints);

        let decision = opt.schedule("req-1", sample_providers());
        assert_ne!(decision.selected_provider, "prov-a");
    }

    #[test]
    fn test_schedule_all_excluded() {
        let opt = make_optimizer();
        let constraints = SchedulingConstraints {
            excluded_providers: vec![
                "prov-a".to_string(),
                "prov-b".to_string(),
                "prov-c".to_string(),
            ],
            ..Default::default()
        };
        opt.set_strategy(constraints);

        let decision = opt.schedule("req-1", sample_providers());
        assert!(decision.selected_provider.is_empty());
        assert!(decision.reason.contains("excluded"));
    }

    #[test]
    fn test_schedule_with_preferred_provider() {
        let opt = make_optimizer();
        let constraints = SchedulingConstraints {
            preferred_providers: vec!["prov-c".to_string()],
            ..Default::default()
        };
        opt.set_strategy(constraints);

        let decision = opt.schedule("req-1", sample_providers());
        assert_eq!(decision.selected_provider, "prov-c");
    }

    #[test]
    fn test_schedule_with_cost_constraint() {
        let opt = make_optimizer();
        let constraints = SchedulingConstraints {
            max_cost_per_request: Some(0.01),
            ..Default::default()
        };
        opt.set_strategy(constraints);

        let decision = opt.schedule("req-1", sample_providers());
        // prov-c costs 0.02, should be excluded
        assert_ne!(decision.selected_provider, "prov-c");
    }

    #[test]
    fn test_schedule_with_latency_constraint() {
        let opt = make_optimizer();
        let constraints = SchedulingConstraints {
            max_latency_ms: Some(100),
            ..Default::default()
        };
        opt.set_strategy(constraints);

        let decision = opt.schedule("req-1", sample_providers());
        // prov-b has 200ms, should be excluded
        assert_ne!(decision.selected_provider, "prov-b");
    }

    #[test]
    fn test_schedule_with_impossible_constraints() {
        let opt = make_optimizer();
        let constraints = SchedulingConstraints {
            max_cost_per_request: Some(0.001),
            max_latency_ms: Some(10),
            ..Default::default()
        };
        opt.set_strategy(constraints);

        let decision = opt.schedule("req-1", sample_providers());
        assert!(decision.selected_provider.is_empty());
    }

    #[test]
    fn test_scores_all_populated() {
        let opt = make_optimizer();
        let decision = opt.schedule("req-1", sample_providers());
        assert_eq!(decision.scores.len(), 3);
        for score in &decision.scores {
            assert!(score.cost_score >= 0.0 && score.cost_score <= 1.0);
            assert!(score.latency_score >= 0.0 && score.latency_score <= 1.0);
            assert!(score.health_score >= 0.0 && score.health_score <= 1.0);
            assert!(score.fairness_score >= 0.0 && score.fairness_score <= 1.0);
            assert!(score.composite_score >= 0.0 && score.composite_score <= 1.0);
        }
    }

    #[test]
    fn test_update_provider_health() {
        let opt = make_optimizer();
        opt.update_provider_health("prov-a", 0.95);
        assert_eq!(opt.get_provider_health("prov-a"), 0.95);
    }

    #[test]
    fn test_update_provider_health_clamped() {
        let opt = make_optimizer();
        opt.update_provider_health("prov-a", 1.5);
        assert_eq!(opt.get_provider_health("prov-a"), 1.0);

        opt.update_provider_health("prov-a", -0.5);
        assert_eq!(opt.get_provider_health("prov-a"), 0.0);
    }

    #[test]
    fn test_update_provider_latency() {
        let opt = make_optimizer();
        opt.update_provider_latency("prov-a", 50);
        let score = opt.provider_scores.get("prov-a").unwrap();
        assert!(score.value().latency_score > 0.0);
    }

    #[test]
    fn test_set_and_get_strategy() {
        let opt = make_optimizer();
        let constraints = SchedulingConstraints {
            max_cost_per_request: Some(0.05),
            priority_level: "high".to_string(),
            ..Default::default()
        };
        opt.set_strategy(constraints.clone());
        let retrieved = opt.get_strategy();
        assert_eq!(retrieved.max_cost_per_request, Some(0.05));
        assert_eq!(retrieved.priority_level, "high");
    }

    #[test]
    fn test_set_and_get_strategy_name() {
        let opt = make_optimizer();
        opt.set_strategy_name(SchedulingStrategy::LowestCost);
        assert_eq!(opt.get_strategy_name(), SchedulingStrategy::LowestCost);
    }

    #[test]
    fn test_metrics_initial() {
        let opt = make_optimizer();
        let metrics = opt.get_metrics();
        assert_eq!(metrics.total_decisions, 0);
        assert_eq!(metrics.avg_cost_savings_pct, 0.0);
        assert_eq!(metrics.avg_latency_ms, 0.0);
        assert_eq!(metrics.fair_share_violations, 0);
        assert!(metrics.strategy_distribution.is_empty());
    }

    #[test]
    fn test_metrics_after_scheduling() {
        let opt = make_optimizer();
        opt.schedule("req-1", sample_providers());
        opt.schedule("req-2", sample_providers());
        let metrics = opt.get_metrics();
        assert_eq!(metrics.total_decisions, 2);
        assert!(metrics.avg_latency_ms > 0.0);
        assert!(!metrics.strategy_distribution.is_empty());
    }

    #[test]
    fn test_reset_metrics() {
        let opt = make_optimizer();
        opt.schedule("req-1", sample_providers());
        opt.reset_metrics();
        let metrics = opt.get_metrics();
        assert_eq!(metrics.total_decisions, 0);
        assert_eq!(metrics.fair_share_violations, 0);
    }

    #[test]
    fn test_scheduling_strategy_serialization() {
        let strategy = SchedulingStrategy::LowestCost;
        let json = serde_json::to_string(&strategy).unwrap();
        assert_eq!(json, "\"LowestCost\"");

        let deserialized: SchedulingStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(strategy, deserialized);
    }

    #[test]
    fn test_scheduling_constraints_default() {
        let constraints = SchedulingConstraints::default();
        assert!(constraints.max_cost_per_request.is_none());
        assert!(constraints.max_latency_ms.is_none());
        assert!(constraints.preferred_providers.is_empty());
        assert!(constraints.excluded_providers.is_empty());
        assert_eq!(constraints.priority_level, "normal");
    }

    #[test]
    fn test_provider_affinity_strategy() {
        let opt = make_optimizer();
        opt.set_strategy_name(SchedulingStrategy::ProviderAffinity);

        // Update health so prov-a is healthiest
        opt.update_provider_health("prov-a", 1.0);
        opt.update_provider_health("prov-b", 0.3);
        opt.update_provider_health("prov-c", 0.3);

        let decision = opt.schedule("req-1", sample_providers());
        // Provider affinity heavily weights health
        assert_eq!(decision.selected_provider, "prov-a");
    }

    #[test]
    fn test_estimated_cost_and_latency_in_decision() {
        let opt = make_optimizer();
        let decision = opt.schedule("req-1", sample_providers());
        let selected = decision.selected_provider.clone();
        let providers_map: HashMap<String, (f64, u64)> = sample_providers()
            .into_iter()
            .map(|(name, cost, lat)| (name, (cost, lat)))
            .collect();

        if let Some((cost, lat)) = providers_map.get(&selected) {
            assert!((decision.estimated_cost - *cost).abs() < f64::EPSILON);
            assert_eq!(decision.estimated_latency_ms, *lat);
        }
    }

    #[test]
    fn test_single_provider_always_selected() {
        let opt = make_optimizer();
        let providers = vec![("only-prov".to_string(), 0.01, 100)];
        let decision = opt.schedule("req-1", providers);
        assert_eq!(decision.selected_provider, "only-prov");
    }

    #[test]
    fn test_equal_costs_equal_latencies() {
        let opt = make_optimizer();
        let providers = vec![
            ("prov-a".to_string(), 0.01, 100),
            ("prov-b".to_string(), 0.01, 100),
            ("prov-c".to_string(), 0.01, 100),
        ];
        let decision = opt.schedule("req-1", providers);
        assert!(!decision.selected_provider.is_empty());
        // With equal scores, should still pick one
        assert!(decision.scores.len() == 3);
    }

    #[test]
    fn test_default_constraints_in_schedule_request() {
        let req = ScheduleRequest {
            request_id: "test".to_string(),
            providers: vec![],
            constraints: SchedulingConstraints::default(),
            strategy: None,
        };
        assert!(req.constraints.preferred_providers.is_empty());
        assert_eq!(req.constraints.priority_level, "normal");
    }
}
