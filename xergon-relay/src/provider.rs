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
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::chain::ChainProvider;
use crate::config::RelayConfig;
use crate::demand::DemandTracker;

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
#[derive(Debug, Clone)]
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
}

/// Consecutive failures before a provider is considered "degraded" (deprioritized, not removed)
const DEGRADED_THRESHOLD: u32 = 3;

/// Consecutive failures before a provider is removed from the registry
const REMOVAL_THRESHOLD: u32 = 10;

/// The provider registry
pub struct ProviderRegistry {
    /// All known providers keyed by endpoint URL
    pub(crate) providers: DashMap<String, Provider>,
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
    pub fn ranked_providers(&self) -> Vec<Provider> {
        let mut providers: Vec<Provider> = self
            .providers
            .iter()
            .filter(|p| {
                // Include healthy providers and degraded providers (below removal threshold)
                p.is_healthy || p.consecutive_failures < REMOVAL_THRESHOLD
            })
            .map(|r| r.value().clone())
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
    pub fn ranked_providers_for_model(&self, model_id: Option<&str>) -> Vec<Provider> {
        let normalized_model = model_id.map(|m| m.to_lowercase().replace(' ', "-"));

        let mut providers: Vec<Provider> = self
            .providers
            .iter()
            .filter(|p| {
                // Include healthy providers and degraded providers (below removal threshold)
                p.is_healthy || p.consecutive_failures < REMOVAL_THRESHOLD
            })
            .filter(|p| {
                // If a model is requested, only include providers that serve it
                if let Some(ref model) = normalized_model {
                    p.served_models
                        .iter()
                        .any(|sm| sm.to_lowercase().replace(' ', "-") == *model)
                        || p.status.as_ref().and_then(|s| s.pown_status.as_ref())
                            .map(|pown| pown.ai_model.to_lowercase().replace(' ', "-") == *model)
                            .unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|r| r.value().clone())
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
    /// Higher score = preferred. Components:
    ///   - PoNW work_points: 0-100 normalized (weight: 40%)
    ///   - Latency: inverse (lower latency = higher score) (weight: 35%)
    ///   - Load: inverse of active_requests (weight: 25%)
    ///
    /// When `model_id` is provided, also incorporates pricing (30% weight):
    ///   - Price: inverse of nanoERG cost (free providers score highest)
    fn routing_score(&self, provider: &Provider, model_id: Option<&str>) -> f64 {
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

    /// Increment active requests for a provider, return guard that decrements on drop
    pub fn acquire_provider(&self, endpoint: &str) -> Option<ProviderRequestGuard> {
        self.providers.get(endpoint).map(|provider| {
            provider
                .active_requests
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ProviderRequestGuard {
                endpoint: endpoint.to_string(),
                active: provider.active_requests.clone(),
            }
        })
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
            },
        );
    }

    /// Remove a single provider by endpoint URL.
    /// Only removes providers that were discovered from chain state,
    /// unless `force` is true (which also removes static bootstrap providers).
    /// Returns true if the provider was actually removed.
    #[allow(dead_code)] // Public API for future admin handler use
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
            // Update provider metadata from chain (pricing, served models)
            if let Some(mut provider) = self.providers.get_mut(&cp.endpoint.trim_end_matches('/').to_string()) {
                provider.model_pricing = cp.model_pricing.clone();
                provider.served_models = cp.models.clone();
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
}

impl Drop for ProviderRequestGuard {
    fn drop(&mut self) {
        self.active
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
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
        assert!(
            score_healthy > score_degraded * 2.0,
            "Healthy score ({}) should be much higher than degraded score ({})",
            score_healthy,
            score_degraded
        );
    }
}
