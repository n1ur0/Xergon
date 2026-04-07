//! GET /v1/providers — List providers (chain cache + health merge)
//!
//! Returns provider info by reading the cached chain state (from ChainCache)
//! and enriching with live health/latency data (from ProviderRegistry).
//!
//! Chain data provides the canonical list of registered providers.
//! In-memory data supplements with real-time health and latency metrics.
//!
//! If the chain cache is stale or never populated, a lazy refresh is
//! triggered in the background and stale/empty data is returned.

use axum::{extract::State, response::Json};
use serde::Serialize;
use tracing::{debug, info, warn};

use crate::proxy::AppState;

/// A single provider returned by the /v1/providers endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub provider_pk: String,
    pub endpoint: String,
    pub region: String,
    pub models: Vec<String>,
    pub pown_score: i32,
    pub last_heartbeat: i32,
    pub is_active: bool,
    pub value_nanoerg: u64,
    pub box_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_nanoerg_per_million_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    pub healthy: bool,
}

/// Response envelope for GET /v1/providers.
#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderInfo>,
}

/// GET /v1/providers handler
pub async fn list_providers_handler(State(state): State<AppState>) -> Json<ProvidersResponse> {
    info!("Listing providers (chain cache + health merge)");

    let chain_providers = match &state.chain_cache {
        Some(cache) => {
            // Try fresh cache first
            if let Some(providers) = cache.get_providers() {
                providers
            } else if cache.is_populated() {
                // Cache is stale but was populated before — return stale data
                debug!("Chain cache is stale, returning stale data");
                cache.get_providers_or_empty()
            } else {
                // Cache never populated — trigger a lazy scan in background
                warn!("Chain cache never populated, triggering lazy scan");
                trigger_lazy_scan(&state);
                cache.get_providers_or_empty()
            }
        }
        None => {
            // Chain scanning disabled — return empty (backward compat)
            debug!("Chain scanning disabled, no providers to list from chain");
            Vec::new()
        }
    };

    let mut providers: Vec<ProviderInfo> = Vec::new();

    for cp in chain_providers {
        // Look up live health data from the in-memory registry
        let mem_provider = state
            .provider_registry
            .providers
            .get(&cp.endpoint.trim_end_matches('/').to_string())
            .map(|r| r.value().clone());

        let is_active = mem_provider
            .as_ref()
            .map(|p| {
                // Consider active if heartbeat is recent (healthy or recently healthy)
                let elapsed = chrono::Utc::now().signed_duration_since(p.last_healthy_at);
                elapsed.num_minutes() < 10
            })
            .unwrap_or(false);

        let latency_ms = mem_provider
            .as_ref()
            .filter(|p| p.is_healthy)
            .map(|p| p.latency_ms);

        let healthy = mem_provider
            .as_ref()
            .map(|p| p.is_healthy)
            .unwrap_or(false);

        providers.push(ProviderInfo {
            provider_pk: cp.provider_pk.clone(),
            endpoint: cp.endpoint,
            region: cp.region,
            models: cp.models,
            pown_score: cp.pown_score,
            last_heartbeat: cp.last_heartbeat,
            is_active,
            value_nanoerg: cp.value_nanoerg,
            box_id: cp.box_id,
            pricing_nanoerg_per_million_tokens: cp.pricing_nanoerg_per_million_tokens,
            latency_ms,
            healthy,
        });
    }

    // Filter to only active providers
    providers.retain(|p| p.is_active);

    // Sort by pown_score descending
    providers.sort_by(|a, b| b.pown_score.cmp(&a.pown_score));

    Json(ProvidersResponse { providers })
}

/// Trigger a background lazy scan if the cache is stale.
/// This spawns a non-blocking task that scans the chain and updates the cache.
fn trigger_lazy_scan(state: &AppState) {
    if let (Some(scanner), Some(cache)) = (&state.chain_scanner, &state.chain_cache) {
        let scanner = scanner.clone();
        let cache = cache.clone();
        let registry = state.provider_registry.clone();
        let health_scorer = state.health_scorer.clone();
        tokio::spawn(async move {
            debug!("Lazy chain scan triggered");
            let providers = scanner.scan().await;
            cache.update(providers.clone());
            registry.sync_from_chain(&providers);
            // Bridge on-chain PoNW reputation into HealthScorer
            for cp in &providers {
                health_scorer.update_reputation_from_pown(&cp.provider_pk, cp.pown_score);
            }
            info!(count = providers.len(), "Lazy chain scan complete");
        });
    }
}
