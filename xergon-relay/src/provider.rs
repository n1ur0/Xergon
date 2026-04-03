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
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

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
}

/// The provider registry
pub struct ProviderRegistry {
    /// All known providers keyed by endpoint URL
    pub(crate) providers: DashMap<String, Provider>,
    http_client: Client,
    config: Arc<crate::config::RelayConfig>,
}

impl ProviderRegistry {
    pub fn new(config: Arc<crate::config::RelayConfig>) -> Self {
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

        // Seed with known endpoints
        for endpoint in &registry.config.providers.known_endpoints {
            registry.providers.insert(endpoint.clone(), Provider {
                endpoint: endpoint.clone(),
                status: None,
                latency_ms: 0,
                active_requests: Arc::new(std::sync::atomic::AtomicU32::new(0)),
                is_healthy: false,
                last_healthy_at: Utc::now(),
                consecutive_failures: 0,
            });
        }

        registry
    }

    /// Add a new provider endpoint
    pub fn add_provider(&self, endpoint: String) {
        self.providers.entry(endpoint.clone()).or_insert_with(|| Provider {
            endpoint,
            status: None,
            latency_ms: 0,
            active_requests: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            is_healthy: false,
            last_healthy_at: Utc::now(),
            consecutive_failures: 0,
        });
    }

    #[allow(dead_code)] // TODO: will be used for provider removal on health check failure
    pub fn remove_provider(&self, endpoint: &str) {
        self.providers.remove(endpoint);
    }

    /// Poll a single provider's health
    pub(crate) async fn poll_provider(&self, endpoint: &str) {
        let url = format!("{}/xergon/status", endpoint.trim_end_matches('/'));
        let start = std::time::Instant::now();

        let result = self.http_client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        let latency_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<XergonAgentStatus>().await {
                    Ok(status) => {
                        let ai_enabled = status.pown_status
                            .as_ref()
                            .map(|p| p.ai_enabled)
                            .unwrap_or(false);

                        if let Some(mut provider) = self.providers.get_mut(endpoint) {
                            provider.status = Some(status);
                            provider.latency_ms = latency_ms;
                            provider.is_healthy = ai_enabled; // Must have AI enabled
                            provider.last_healthy_at = Utc::now();
                            provider.consecutive_failures = 0;
                        }
                        debug!(endpoint, latency_ms, ai_enabled, "Provider health check OK");
                    }
                    Err(e) => {
                        warn!(endpoint, error = %e, "Failed to parse provider status");
                        self.mark_unhealthy(endpoint);
                    }
                }
            }
            Ok(resp) => {
                warn!(endpoint, status = %resp.status(), "Provider returned error status");
                self.mark_unhealthy(endpoint);
            }
            Err(e) => {
                warn!(endpoint, error = %e, "Provider health check failed");
                self.mark_unhealthy(endpoint);
            }
        }
    }

    fn mark_unhealthy(&self, endpoint: &str) {
        if let Some(mut provider) = self.providers.get_mut(endpoint) {
            provider.is_healthy = false;
            provider.consecutive_failures += 1;
        }
    }

    /// Number of currently healthy providers
    pub fn healthy_provider_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|p| p.is_healthy)
            .count()
    }

    #[allow(dead_code)] // TODO: will be used for admin provider listing endpoint
    pub fn all_providers(&self) -> Vec<Provider> {
        self.providers.iter().map(|r| r.value().clone()).collect()
    }

    /// Get healthy providers sorted by routing score (best first)
    pub fn ranked_providers(&self) -> Vec<Provider> {
        let mut providers: Vec<Provider> = self.providers
            .iter()
            .filter(|p| p.is_healthy)
            .map(|r| r.value().clone())
            .collect();

        // Sort by routing score: higher is better
        providers.sort_by(|a, b| {
            let score_a = self.routing_score(a);
            let score_b = self.routing_score(b);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        providers
    }

    /// Calculate routing score for a provider.
    ///
    /// Higher score = preferred. Components:
    ///   - PoNW work_points: 0-100 normalized (weight: 40%)
    ///   - Latency: inverse (lower latency = higher score) (weight: 35%)
    ///   - Load: inverse of active_requests (weight: 25%)
    fn routing_score(&self, provider: &Provider) -> f64 {
        // PoNW score (0-100, from work_points)
        let pown_score = provider.status
            .as_ref()
            .and_then(|s| s.pown_status.as_ref())
            .map(|p| (p.work_points as f64 / 100.0).min(1.0))
            .unwrap_or(0.0);

        // Latency score (inverse, 0=terrible, 1=instant)
        // Use a sigmoid-like curve: score = 1 / (1 + latency/500)
        let latency_score = 1.0 / (1.0 + (provider.latency_ms as f64) / 500.0);

        // Load score (inverse of active requests, 0=busy, 1=idle)
        let active = provider.active_requests.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let load_score = 1.0 / (1.0 + active / 5.0);

        // Weighted combination
        0.40 * pown_score + 0.35 * latency_score + 0.25 * load_score
    }

    /// Select the best provider for a request, excluding already-tried endpoints.
    pub fn select_provider(&self, exclude: &[String]) -> Option<Provider> {
        self.ranked_providers()
            .into_iter()
            .find(|p| !exclude.contains(&p.endpoint))
    }

    /// Increment active requests for a provider, return guard that decrements on drop
    pub fn acquire_provider(&self, endpoint: &str) -> Option<ProviderRequestGuard> {
        self.providers.get(endpoint).map(|provider| {
            provider.active_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ProviderRequestGuard {
                endpoint: endpoint.to_string(),
                active: provider.active_requests.clone(),
            }
        })
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
        self.active.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
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

/// Collect unique models across all healthy providers
pub fn collect_models(registry: &ProviderRegistry) -> Vec<ModelInfo> {
    let mut models: std::collections::HashMap<String, (String, usize)> = std::collections::HashMap::new();

    for provider in registry.ranked_providers() {
        if let Some(ref status) = provider.status {
            if let Some(ref pown) = status.pown_status {
                if pown.ai_enabled && !pown.ai_model.is_empty() {
                    let model_id = pown.ai_model.to_lowercase().replace(' ', "-");
                    let entry = models.entry(model_id.clone()).or_insert_with(|| {
                        (pown.ai_model.clone(), 0)
                    });
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
