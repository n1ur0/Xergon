//! Adaptive Router module.
//!
//! Selects the best provider for each request using configurable strategies,
//! health scores, geo-proximity, sticky sessions, and fallback chains.

use crate::geo_router::GeoRouter;
use crate::health_score::HealthScorer;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Routing Strategy
// ---------------------------------------------------------------------------

/// Strategy for selecting which provider to route a request to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Route to the provider with the highest health score.
    HealthScore,
    /// Route to the provider with the lowest p50 latency.
    LowestLatency,
    /// Simple round-robin across providers.
    RoundRobin,
    /// Weighted random selection based on health scores.
    WeightedRandom,
    /// Route to the provider with fewest active connections.
    LeastConnections,
    /// Balance cost and performance.
    CostOptimized,
}

impl std::fmt::Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::HealthScore => write!(f, "health_score"),
            RoutingStrategy::LowestLatency => write!(f, "lowest_latency"),
            RoutingStrategy::RoundRobin => write!(f, "round_robin"),
            RoutingStrategy::WeightedRandom => write!(f, "weighted_random"),
            RoutingStrategy::LeastConnections => write!(f, "least_connections"),
            RoutingStrategy::CostOptimized => write!(f, "cost_optimized"),
        }
    }
}

// ---------------------------------------------------------------------------
// Routing Configuration
// ---------------------------------------------------------------------------

/// Configuration for the adaptive router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries_per_provider: u32,
    pub fallback_enabled: bool,
    pub geo_routing_enabled: bool,
    pub sticky_sessions: bool,
    pub sticky_ttl_secs: u64,
    pub circuit_breaker_threshold: f64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            strategy: RoutingStrategy::HealthScore,
            max_retries_per_provider: 2,
            fallback_enabled: true,
            geo_routing_enabled: true,
            sticky_sessions: true,
            sticky_ttl_secs: 300,
            circuit_breaker_threshold: 0.1,
        }
    }
}

// ---------------------------------------------------------------------------
// Routing Request
// ---------------------------------------------------------------------------

/// Input to the routing decision — describes what the request needs.
#[derive(Debug, Clone, Default)]
pub struct RoutingRequest {
    pub model: String,
    pub user_region: Option<String>,
    pub user_pk: Option<String>,
    pub priority: u8,
    pub estimated_tokens: Option<u32>,
    pub requires_gpu: bool,
    /// Maximum cost budget in nanoERG for this request.
    /// When set, cost-optimized routing filters out providers exceeding this budget.
    pub max_cost_nanoerg: Option<u64>,
}

impl RoutingRequest {
    /// Create a minimal routing request with just a model name.
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            user_region: None,
            user_pk: None,
            priority: 0,
            estimated_tokens: None,
            requires_gpu: false,
            max_cost_nanoerg: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Routing Decision
// ---------------------------------------------------------------------------

/// The output of a routing decision — which provider to use and why.
#[derive(Debug, Clone, Serialize)]
pub struct RoutingDecision {
    pub provider_pk: String,
    pub provider_endpoint: String,
    pub strategy_used: RoutingStrategy,
    pub health_score: f64,
    pub region: String,
    pub estimated_latency_ms: u64,
    pub fallback_providers: Vec<String>,
}

// ---------------------------------------------------------------------------
// Routing Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("No providers available for model '{model}'")]
    NoProviders { model: String },

    #[error("No healthy providers (all below circuit breaker threshold)")]
    AllUnhealthy,

    #[error("No providers fit the cost budget (max {max_cost_nanoerg} nanoERG) for model '{model}'")]
    BudgetExceeded { model: String, max_cost_nanoerg: u64 },
}

// ---------------------------------------------------------------------------
// Sticky Session Entry
// ---------------------------------------------------------------------------

struct StickyEntry {
    provider_pk: String,
    provider_endpoint: String,
    created_at: Instant,
}

// ---------------------------------------------------------------------------
// Routing Stats
// ---------------------------------------------------------------------------

/// Aggregate routing statistics.
#[derive(Debug, Clone, Serialize)]
pub struct RoutingStats {
    pub strategy: RoutingStrategy,
    pub total_decisions: u64,
    pub total_fallbacks: u64,
    pub total_sticky_hits: u64,
    pub total_sticky_misses: u64,
    pub total_geo_routed: u64,
    pub providers_used: usize,
    pub avg_health_score: f64,
}

// ---------------------------------------------------------------------------
// Provider Info (for routing selection)
// ---------------------------------------------------------------------------

/// Lightweight provider info needed for routing decisions.
/// Passed in from the provider registry for each eligible provider.
#[derive(Debug, Clone)]
pub struct ProviderRoutingInfo {
    pub provider_pk: String,
    pub endpoint: String,
    pub active_requests: u32,
    /// Per-model pricing: model_id -> nanoERG per 1M tokens.
    /// Populated from on-chain R6 register via sync_from_chain().
    pub model_pricing: HashMap<String, u64>,
    /// Provider region (from on-chain R9 or /xergon/status)
    pub region: Option<String>,
}

// ---------------------------------------------------------------------------
// AdaptiveRouter
// ---------------------------------------------------------------------------

/// Adaptive router that selects the best provider for each request.
///
/// Combines health scoring, geo-routing, sticky sessions, and multiple
/// routing strategies into a unified routing decision.
pub struct AdaptiveRouter {
    health_scorer: Arc<HealthScorer>,
    geo_router: Arc<GeoRouter>,
    config: RwLock<RoutingConfig>,
    /// Round-robin counter
    rr_counter: AtomicU64,
    /// Sticky sessions: user_pk -> StickyEntry
    sticky_sessions: DashMap<String, StickyEntry>,
    /// Per-provider active connection counts
    active_connections: DashMap<String, AtomicU32>,
    /// Stats
    total_decisions: AtomicU64,
    total_fallbacks: AtomicU64,
    total_sticky_hits: AtomicU64,
    total_sticky_misses: AtomicU64,
    total_geo_routed: AtomicU64,
}

impl AdaptiveRouter {
    /// Create a new adaptive router.
    pub fn new(
        health_scorer: Arc<HealthScorer>,
        geo_router: Arc<GeoRouter>,
        config: RoutingConfig,
    ) -> Self {
        Self {
            health_scorer,
            geo_router,
            config: RwLock::new(config),
            rr_counter: AtomicU64::new(0),
            sticky_sessions: DashMap::new(),
            active_connections: DashMap::new(),
            total_decisions: AtomicU64::new(0),
            total_fallbacks: AtomicU64::new(0),
            total_sticky_hits: AtomicU64::new(0),
            total_sticky_misses: AtomicU64::new(0),
            total_geo_routed: AtomicU64::new(0),
        }
    }

    /// Select the best provider for a request.
    ///
    /// Takes a list of available providers and returns the best routing decision.
    pub fn select_provider(
        &self,
        request: &RoutingRequest,
        available_providers: &[ProviderRoutingInfo],
    ) -> Result<RoutingDecision, RoutingError> {
        if available_providers.is_empty() {
            return Err(RoutingError::NoProviders {
                model: request.model.clone(),
            });
        }

        let config = self.config.read().unwrap();
        let strategy = config.strategy;

        // Check sticky session first
        if config.sticky_sessions {
            if let Some(ref user_pk) = request.user_pk {
                if let Some(entry) = self.check_sticky_session(user_pk, config.sticky_ttl_secs) {
                    // Verify the sticky provider is still available
                    if available_providers.iter().any(|p| p.provider_pk == entry.provider_pk) {
                        self.total_sticky_hits.fetch_add(1, Ordering::Relaxed);
                        self.total_decisions.fetch_add(1, Ordering::Relaxed);

                        let score = self.health_scorer.get_score(&entry.provider_pk);
                        let health_score = score.as_ref().map(|s| s.overall_score).unwrap_or(0.5);

                        let region = self
                            .geo_router
                            .get_provider_region(&entry.provider_pk)
                            .unwrap_or_default();

                        let provider_pk = entry.provider_pk.clone();
                        let provider_endpoint = entry.provider_endpoint.clone();

                        let estimated_latency = self.estimate_latency_for_provider(
                            &provider_pk,
                            request.user_region.as_deref(),
                        );

                        return Ok(RoutingDecision {
                            provider_pk,
                            provider_endpoint,
                            strategy_used: strategy,
                            health_score,
                            region,
                            estimated_latency_ms: estimated_latency,
                            fallback_providers: vec![],
                        });
                    }
                }
                self.total_sticky_misses.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Filter providers by circuit breaker threshold
        let eligible: Vec<&ProviderRoutingInfo> = available_providers
            .iter()
            .filter(|p| {
                let score = self.health_scorer.get_score(&p.provider_pk);
                match score {
                    Some(s) => s.overall_score >= config.circuit_breaker_threshold,
                    None => true, // No score yet — allow through (new provider)
                }
            })
            .collect();

        if eligible.is_empty() {
            return Err(RoutingError::AllUnhealthy);
        }

        // Apply routing strategy
        let selected = self.apply_strategy(strategy, request, &eligible, &config);

        self.total_decisions.fetch_add(1, Ordering::Relaxed);

        // Set sticky session
        if config.sticky_sessions {
            if let Some(ref user_pk) = request.user_pk {
                self.set_sticky_session(
                    user_pk,
                    &selected.provider_pk,
                    &selected.provider_endpoint,
                );
            }
        }

        Ok(selected)
    }

    /// Select a provider with a fallback chain.
    pub fn select_with_fallbacks(
        &self,
        request: &RoutingRequest,
        available_providers: &[ProviderRoutingInfo],
        max_fallbacks: usize,
    ) -> Vec<RoutingDecision> {
        let mut decisions = Vec::new();
        let mut tried_pks: Vec<String> = Vec::new();

        // First selection
        let remaining: Vec<&ProviderRoutingInfo> = available_providers.iter().collect();
        match self.select_provider_internal(request, &remaining) {
            Ok(decision) => {
                tried_pks.push(decision.provider_pk.clone());
                decisions.push(decision);
            }
            Err(_) => return decisions,
        }

        // Generate fallbacks
        for _ in 0..max_fallbacks {
            let remaining: Vec<&ProviderRoutingInfo> = available_providers
                .iter()
                .filter(|p| !tried_pks.contains(&p.provider_pk))
                .collect();

            if remaining.is_empty() {
                break;
            }

            match self.select_provider_internal(request, &remaining) {
                Ok(decision) => {
                    tried_pks.push(decision.provider_pk.clone());
                    self.total_fallbacks.fetch_add(1, Ordering::Relaxed);
                    decisions.push(decision);
                }
                Err(_) => break,
            }
        }

        // Set fallback list on first decision
        if decisions.len() > 1 {
            let fallbacks: Vec<String> = decisions[1..]
                .iter()
                .map(|d| d.provider_pk.clone())
                .collect();
            decisions[0].fallback_providers = fallbacks;
        }

        decisions
    }

    /// Record routing outcome (for learning / feedback loop).
    pub fn record_outcome(&self, provider_pk: &str, latency_ms: u64, success: bool) {
        if success {
            self.health_scorer.record_success(provider_pk, latency_ms);
        } else {
            self.health_scorer.record_failure(provider_pk, "proxy_failure");
        }
    }

    /// Update the routing strategy.
    pub fn set_strategy(&self, strategy: RoutingStrategy) {
        let mut config = self.config.write().unwrap();
        info!(old_strategy = %config.strategy, new_strategy = %strategy, "Routing strategy changed");
        config.strategy = strategy;
    }

    /// Get the current routing strategy.
    pub fn get_strategy(&self) -> RoutingStrategy {
        self.config.read().unwrap().strategy
    }

    /// Get routing statistics.
    pub fn get_stats(&self) -> RoutingStats {
        let config = self.config.read().unwrap();
        let all_scores = self.health_scorer.get_all_scores();
        let avg_health = if all_scores.is_empty() {
            0.0
        } else {
            all_scores.iter().map(|s| s.overall_score).sum::<f64>() / all_scores.len() as f64
        };

        RoutingStats {
            strategy: config.strategy,
            total_decisions: self.total_decisions.load(Ordering::Relaxed),
            total_fallbacks: self.total_fallbacks.load(Ordering::Relaxed),
            total_sticky_hits: self.total_sticky_hits.load(Ordering::Relaxed),
            total_sticky_misses: self.total_sticky_misses.load(Ordering::Relaxed),
            total_geo_routed: self.total_geo_routed.load(Ordering::Relaxed),
            providers_used: self.active_connections.len(),
            avg_health_score: avg_health,
        }
    }

    /// Increment active connections for a provider.
    pub fn acquire_connection(&self, provider_pk: &str) {
        let counter = self
            .active_connections
            .entry(provider_pk.to_string())
            .or_insert_with(|| AtomicU32::new(0));
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connections for a provider.
    pub fn release_connection(&self, provider_pk: &str) {
        if let Some(counter) = self.active_connections.get(provider_pk) {
            counter.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Get active connection count for a provider.
    pub fn get_active_connections(&self, provider_pk: &str) -> u32 {
        self.active_connections
            .get(provider_pk)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Access the health scorer.
    pub fn health_scorer(&self) -> &Arc<HealthScorer> {
        &self.health_scorer
    }

    /// Access the geo router.
    pub fn geo_router(&self) -> &Arc<GeoRouter> {
        &self.geo_router
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn select_provider_internal(
        &self,
        request: &RoutingRequest,
        available: &[&ProviderRoutingInfo],
    ) -> Result<RoutingDecision, RoutingError> {
        if available.is_empty() {
            return Err(RoutingError::NoProviders {
                model: request.model.clone(),
            });
        }

        let config = self.config.read().unwrap();
        let selected = self.apply_strategy(config.strategy, request, available, &config);
        Ok(selected)
    }

    fn apply_strategy(
        &self,
        strategy: RoutingStrategy,
        request: &RoutingRequest,
        providers: &[&ProviderRoutingInfo],
        config: &RoutingConfig,
    ) -> RoutingDecision {
        match strategy {
            RoutingStrategy::HealthScore => self.select_by_health_score(providers, request, config),
            RoutingStrategy::LowestLatency => self.select_by_lowest_latency(providers, request),
            RoutingStrategy::RoundRobin => self.select_round_robin(providers, request),
            RoutingStrategy::WeightedRandom => self.select_weighted_random(providers, request, config),
            RoutingStrategy::LeastConnections => self.select_least_connections(providers, request),
            RoutingStrategy::CostOptimized => self.select_cost_optimized(providers, request, config),
        }
    }

    fn select_cost_optimized(
        &self,
        providers: &[&ProviderRoutingInfo],
        request: &RoutingRequest,
        config: &RoutingConfig,
    ) -> RoutingDecision {
        let estimated_tokens = request.estimated_tokens.unwrap_or(1000) as f64;

        // Step 1: Check if any providers have pricing data for the requested model.
        let has_pricing: Vec<&&ProviderRoutingInfo> = providers
            .iter()
            .filter(|p| p.model_pricing.contains_key(&request.model))
            .collect();

        if has_pricing.is_empty() {
            // No pricing data available for any provider — fall back to health-only scoring
            debug!(
                model = %request.model,
                "No pricing data available for model, falling back to health score"
            );
            return self.select_by_health_score(providers, request, config);
        }

        // Step 2: Compute cost-per-request for each provider with pricing
        let mut scored: Vec<(f64, &&ProviderRoutingInfo)> = has_pricing
            .iter()
            .map(|p| {
                let price_per_1m = p.model_pricing.get(&request.model).copied().unwrap_or(0) as f64;
                let cost_estimate = price_per_1m * estimated_tokens / 1_000_000.0;

                // Filter by budget if set
                if let Some(max_cost) = request.max_cost_nanoerg {
                    if cost_estimate > max_cost as f64 {
                        return (f64::NEG_INFINITY, *p);
                    }
                }

                // Health score component
                let health = self
                    .health_scorer
                    .get_score(&p.provider_pk)
                    .map(|s| s.overall_score)
                    .unwrap_or(0.5);

                // Cost score: inverse sigmoid — cheaper is better
                // Normalize against ~0.1 ERG (100_000_000 nanoERG) so free = 1.0
                let cost_score = 1.0 / (1.0 + cost_estimate / 100_000_000.0);

                // Geo bonus if enabled
                let geo_bonus = if config.geo_routing_enabled {
                    self.geo_bonus(&p.provider_pk, request.user_region.as_deref())
                } else {
                    0.0
                };

                // Combined: 60% health + 40% cost score + geo bonus
                let combined = 0.6 * health + 0.4 * cost_score + geo_bonus;

                (combined, *p)
            })
            .collect();

        // Step 3: Remove providers that exceeded budget (scored as NEG_INFINITY)
        let before_len = scored.len();
        scored.retain(|(score, _)| *score > f64::NEG_INFINITY);

        if scored.is_empty() && before_len > 0 {
            // All providers exceeded the budget — still pick the cheapest one
            // rather than failing, but log a warning
            warn!(
                model = %request.model,
                max_cost = request.max_cost_nanoerg.unwrap_or(0),
                "All providers exceed cost budget, selecting cheapest available"
            );
            // Re-score without budget filtering
            let mut fallback_scored: Vec<(f64, &&ProviderRoutingInfo)> = has_pricing
                .iter()
                .map(|p| {
                    let price_per_1m = p.model_pricing.get(&request.model).copied().unwrap_or(0) as f64;
                    let cost_estimate = price_per_1m * estimated_tokens / 1_000_000.0;
                    let health = self
                        .health_scorer
                        .get_score(&p.provider_pk)
                        .map(|s| s.overall_score)
                        .unwrap_or(0.5);
                    let cost_score = 1.0 / (1.0 + cost_estimate / 100_000_000.0);
                    let combined = 0.6 * health + 0.4 * cost_score;
                    (combined, *p)
                })
                .collect();
            fallback_scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            if let Some((_, best)) = fallback_scored.first() {
                return self.build_decision(best, request, RoutingStrategy::CostOptimized);
            }
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let best = *scored.first().unwrap().1;
        self.build_decision(best, request, RoutingStrategy::CostOptimized)
    }

    fn select_by_health_score(
        &self,
        providers: &[&ProviderRoutingInfo],
        request: &RoutingRequest,
        config: &RoutingConfig,
    ) -> RoutingDecision {
        let mut scored: Vec<(f64, &&ProviderRoutingInfo)> = providers
            .iter()
            .map(|p| {
                let score = self.health_scorer.get_score(&p.provider_pk);
                let health = score.as_ref().map(|s| s.overall_score).unwrap_or(0.5);

                // Apply geo bonus if enabled
                let geo_bonus = if config.geo_routing_enabled {
                    self.geo_bonus(&p.provider_pk, request.user_region.as_deref())
                } else {
                    0.0
                };

                (health + geo_bonus, p)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let best = *scored.first().unwrap().1;
        self.build_decision(best, request, RoutingStrategy::HealthScore)
    }

    fn select_by_lowest_latency(
        &self,
        providers: &[&ProviderRoutingInfo],
        request: &RoutingRequest,
    ) -> RoutingDecision {
        let mut lat_ordered: Vec<(u64, &&ProviderRoutingInfo)> = providers
            .iter()
            .map(|p| {
                let lat = self.estimate_latency_for_provider(
                    &p.provider_pk,
                    request.user_region.as_deref(),
                );
                (lat, p)
            })
            .collect();

        lat_ordered.sort_by_key(|(lat, _)| *lat);

        let best = *lat_ordered.first().unwrap().1;
        self.build_decision(best, request, RoutingStrategy::LowestLatency)
    }

    fn select_round_robin(
        &self,
        providers: &[&ProviderRoutingInfo],
        request: &RoutingRequest,
    ) -> RoutingDecision {
        let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) as usize % providers.len();
        self.build_decision(providers[idx], request, RoutingStrategy::RoundRobin)
    }

    fn select_weighted_random(
        &self,
        providers: &[&ProviderRoutingInfo],
        request: &RoutingRequest,
        config: &RoutingConfig,
    ) -> RoutingDecision {
        use rand::RngExt;

        let weights: Vec<f64> = providers
            .iter()
            .map(|p| {
                let score = self.health_scorer.get_score(&p.provider_pk);
                let health = score.as_ref().map(|s| s.overall_score).unwrap_or(0.5);
                let geo_bonus = if config.geo_routing_enabled {
                    self.geo_bonus(&p.provider_pk, request.user_region.as_deref())
                } else {
                    0.0
                };
                (health + geo_bonus).max(0.01) // ensure non-zero weight
            })
            .collect();

        let total_weight: f64 = weights.iter().sum();

        let mut rng = rand::rng();
        let mut rand_val = rng.random_range(0.0..total_weight);
        let mut selected_idx = 0;

        for (i, w) in weights.iter().enumerate() {
            rand_val -= w;
            if rand_val <= 0.0 {
                selected_idx = i;
                break;
            }
        }

        self.build_decision(providers[selected_idx], request, RoutingStrategy::WeightedRandom)
    }

    fn select_least_connections(
        &self,
        providers: &[&ProviderRoutingInfo],
        request: &RoutingRequest,
    ) -> RoutingDecision {
        let mut conn_ordered: Vec<(u32, &&ProviderRoutingInfo)> = providers
            .iter()
            .map(|p| {
                let conns = self.get_active_connections(&p.provider_pk);
                (conns, p)
            })
            .collect();

        conn_ordered.sort_by_key(|(c, _)| *c);

        let best = *conn_ordered.first().unwrap().1;
        self.build_decision(best, request, RoutingStrategy::LeastConnections)
    }

    fn build_decision(
        &self,
        provider: &ProviderRoutingInfo,
        request: &RoutingRequest,
        strategy: RoutingStrategy,
    ) -> RoutingDecision {
        let score = self.health_scorer.get_score(&provider.provider_pk);
        let health_score = score.as_ref().map(|s| s.overall_score).unwrap_or(0.5);

        let region = self
            .geo_router
            .get_provider_region(&provider.provider_pk)
            .unwrap_or_default();

        let estimated_latency = self.estimate_latency_for_provider(
            &provider.provider_pk,
            request.user_region.as_deref(),
        );

        RoutingDecision {
            provider_pk: provider.provider_pk.clone(),
            provider_endpoint: provider.endpoint.clone(),
            strategy_used: strategy,
            health_score,
            region: region.clone(),
            estimated_latency_ms: estimated_latency,
            fallback_providers: vec![],
        }
    }

    fn geo_bonus(&self, provider_pk: &str, user_region: Option<&str>) -> f64 {
        let provider_region = match self.geo_router.get_provider_region(provider_pk) {
            Some(r) => r,
            None => return 0.0,
        };

        let user = match user_region {
            Some(r) => r,
            None => return 0.0,
        };

        if provider_region == user {
            self.total_geo_routed.fetch_add(1, Ordering::Relaxed);
            0.1 // 10% bonus for same region
        } else {
            0.0
        }
    }

    fn estimate_latency_for_provider(
        &self,
        provider_pk: &str,
        user_region: Option<&str>,
    ) -> u64 {
        let provider_region = self.geo_router.get_provider_region(provider_pk);
        match (user_region, provider_region) {
            (Some(ur), Some(pr)) => self.geo_router.estimate_latency(&ur, &pr),
            _ => 100, // default 100ms when regions unknown
        }
    }

    fn check_sticky_session(&self, user_pk: &str, ttl_secs: u64) -> Option<StickyEntry> {
        let entry = self.sticky_sessions.get(user_pk)?;
        if entry.created_at.elapsed() > Duration::from_secs(ttl_secs) {
            drop(entry);
            self.sticky_sessions.remove(user_pk);
            return None;
        }
        Some(StickyEntry {
            provider_pk: entry.provider_pk.clone(),
            provider_endpoint: entry.provider_endpoint.clone(),
            created_at: entry.created_at,
        })
    }

    fn set_sticky_session(&self, user_pk: &str, provider_pk: &str, endpoint: &str) {
        self.sticky_sessions.insert(
            user_pk.to_string(),
            StickyEntry {
                provider_pk: provider_pk.to_string(),
                provider_endpoint: endpoint.to_string(),
                created_at: Instant::now(),
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health_score::HealthScoringConfig;

    fn make_router() -> AdaptiveRouter {
        let scorer = Arc::new(HealthScorer::new(HealthScoringConfig {
            min_samples: 3,
            ..Default::default()
        }));
        let geo = Arc::new(GeoRouter::new());
        let config = RoutingConfig::default();
        AdaptiveRouter::new(scorer, geo, config)
    }

    fn make_providers() -> Vec<ProviderRoutingInfo> {
        vec![
            ProviderRoutingInfo {
                provider_pk: "provider-a".into(),
                endpoint: "http://a:9099".into(),
                active_requests: 0,
                model_pricing: HashMap::new(),
                region: None,
            },
            ProviderRoutingInfo {
                provider_pk: "provider-b".into(),
                endpoint: "http://b:9099".into(),
                active_requests: 0,
                model_pricing: HashMap::new(),
                region: None,
            },
            ProviderRoutingInfo {
                provider_pk: "provider-c".into(),
                endpoint: "http://c:9099".into(),
                active_requests: 0,
                model_pricing: HashMap::new(),
                region: None,
            },
        ]
    }

    #[test]
    fn test_adaptive_router_health_score_strategy() {
        let router = make_router();
        let providers = make_providers();

        // Give provider-a good scores
        for _ in 0..10 {
            router.health_scorer().record_success("provider-a", 50);
        }
        // Give provider-b mediocre scores
        for _ in 0..7 {
            router.health_scorer().record_success("provider-b", 500);
        }
        for _ in 0..3 {
            router.health_scorer().record_failure("provider-b", "timeout");
        }

        let request = RoutingRequest::new("llama-3.1-8b");
        let decision = router.select_provider(&request, &providers).unwrap();
        assert_eq!(decision.provider_pk, "provider-a");
        assert_eq!(decision.strategy_used, RoutingStrategy::HealthScore);
    }

    #[test]
    fn test_adaptive_router_lowest_latency_strategy() {
        let router = make_router();
        let providers = make_providers();

        router.set_strategy(RoutingStrategy::LowestLatency);

        let request = RoutingRequest::new("llama-3.1-8b");
        let decision = router.select_provider(&request, &providers).unwrap();
        assert_eq!(decision.strategy_used, RoutingStrategy::LowestLatency);
    }

    #[test]
    fn test_adaptive_router_with_fallbacks() {
        let router = make_router();
        let providers = make_providers();

        let request = RoutingRequest::new("llama-3.1-8b");
        let fallbacks = router.select_with_fallbacks(&request, &providers, 2);
        assert_eq!(fallbacks.len(), 3); // 1 primary + 2 fallbacks
        assert_eq!(fallbacks[0].fallback_providers.len(), 2);
    }

    #[test]
    fn test_adaptive_router_with_sticky_sessions() {
        let router = make_router();
        let providers = make_providers();

        // First request for user-1
        let request = RoutingRequest {
            model: "llama-3.1-8b".into(),
            user_pk: Some("user-1".into()),
            ..Default::default()
        };
        let first = router.select_provider(&request, &providers).unwrap();

        // Second request for same user should get same provider
        let second = router.select_provider(&request, &providers).unwrap();
        assert_eq!(first.provider_pk, second.provider_pk);
    }

    #[test]
    fn test_routing_strategy_switching() {
        let router = make_router();
        assert_eq!(router.get_strategy(), RoutingStrategy::HealthScore);

        router.set_strategy(RoutingStrategy::RoundRobin);
        assert_eq!(router.get_strategy(), RoutingStrategy::RoundRobin);

        router.set_strategy(RoutingStrategy::WeightedRandom);
        assert_eq!(router.get_strategy(), RoutingStrategy::WeightedRandom);
    }

    #[test]
    fn test_circuit_breaker_threshold_skip() {
        let scorer = Arc::new(HealthScorer::new(HealthScoringConfig {
            min_samples: 3,
            ..Default::default()
        }));
        let geo = Arc::new(GeoRouter::new());
        let config = RoutingConfig {
            circuit_breaker_threshold: 0.5, // skip providers below 0.5
            ..Default::default()
        };
        let router = AdaptiveRouter::new(scorer, geo, config);

        // Make provider-a unhealthy
        for _ in 0..3 {
            router.health_scorer().record_success("provider-a", 50);
        }
        for _ in 0..7 {
            router.health_scorer().record_failure("provider-a", "error");
        }

        // Make provider-b healthy
        for _ in 0..10 {
            router.health_scorer().record_success("provider-b", 100);
        }

        let providers = make_providers();
        let request = RoutingRequest::new("llama-3.1-8b");
        let decision = router.select_provider(&request, &providers).unwrap();

        // Should skip provider-a and pick provider-b
        assert_eq!(decision.provider_pk, "provider-b");
    }

    #[test]
    fn test_no_providers_error() {
        let router = make_router();
        let request = RoutingRequest::new("llama-3.1-8b");
        let result = router.select_provider(&request, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            RoutingError::NoProviders { model } => assert_eq!(model, "llama-3.1-8b"),
            RoutingError::AllUnhealthy => panic!("expected NoProviders, got AllUnhealthy"),
            RoutingError::BudgetExceeded { .. } => panic!("expected NoProviders, got BudgetExceeded"),
        }
    }

    #[test]
    fn test_record_outcome() {
        let router = make_router();
        router.record_outcome("provider-a", 100, true);
        router.record_outcome("provider-a", 200, false);

        let stats = router.health_scorer().get_stats();
        assert_eq!(stats.total_requests_recorded, 2);
    }

    #[test]
    fn test_get_routing_stats() {
        let router = make_router();
        let stats = router.get_stats();
        assert_eq!(stats.strategy, RoutingStrategy::HealthScore);
        assert_eq!(stats.total_decisions, 0);
    }

    #[test]
    fn test_round_robin_strategy() {
        let router = make_router();
        router.set_strategy(RoutingStrategy::RoundRobin);

        let providers = make_providers();
        let request = RoutingRequest::new("llama-3.1-8b");

        // Should cycle through providers
        let mut seen = std::collections::HashSet::new();
        for _ in 0..providers.len() {
            let decision = router.select_provider(&request, &providers).unwrap();
            seen.insert(decision.provider_pk.clone());
        }

        assert_eq!(seen.len(), providers.len());
    }

    #[test]
    fn test_least_connections_strategy() {
        let router = make_router();
        router.set_strategy(RoutingStrategy::LeastConnections);

        let providers = make_providers();

        // Give provider-a and provider-b some connections
        router.acquire_connection("provider-a");
        router.acquire_connection("provider-a");
        router.acquire_connection("provider-b");

        let request = RoutingRequest::new("llama-3.1-8b");
        let decision = router.select_provider(&request, &providers).unwrap();

        // Should pick provider-c (0 connections)
        assert_eq!(decision.provider_pk, "provider-c");
    }
}
