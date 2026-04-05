//! GET /v1/leaderboard — Public provider leaderboard
//!
//! Returns per-provider stats aggregated from the in-memory usage store,
//! live health data from the provider registry, and on-chain metadata
//! from the ChainCache (when available).
//!
//! This is a PUBLIC endpoint — no authentication required.

use axum::{extract::State, response::Json};
use serde::Serialize;
use tracing::info;

use crate::proxy::AppState;

/// Enriched leaderboard entry returned to clients.
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardEntry {
    pub provider_id: String,
    pub endpoint: String,
    pub online: bool,
    pub latency_ms: u64,
    pub total_requests: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    /// On-chain PoNW reputation score (from ChainCache)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pown_score: Option<i32>,
    /// Provider region (from ChainCache)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

/// GET /v1/leaderboard
pub async fn leaderboard_handler(State(state): State<AppState>) -> Json<Vec<LeaderboardEntry>> {
    info!("Leaderboard requested");

    // Aggregate usage stats from in-memory store per provider
    let mut provider_stats: std::collections::HashMap<String, (u64, u64, u64, u64)> =
        std::collections::HashMap::new();

    for entry in state.usage_store.iter() {
        let record = entry.value();
        let stats = provider_stats
            .entry(record.provider.clone())
            .or_insert((0, 0, 0, 0));
        stats.0 += 1; // requests
        stats.1 += record.tokens_in as u64;
        stats.2 += record.tokens_out as u64;
        stats.3 += (record.tokens_in + record.tokens_out) as u64;
    }

    // Build a lookup for chain metadata (provider_pk, pown_score, region) keyed by endpoint
    let chain_meta: std::collections::HashMap<String, (String, i32, String)> = state
        .chain_cache
        .as_ref()
        .and_then(|cache| cache.get_providers())
        .map(|providers| {
            providers
                .into_iter()
                .map(|cp| {
                    let ep = cp.endpoint.trim_end_matches('/').to_string();
                    (ep, (cp.provider_pk, cp.pown_score, cp.region))
                })
                .collect()
        })
        .unwrap_or_default();

    // Build entries from all known providers (from registry — these are the ones we have health data for)
    let mut entries: Vec<LeaderboardEntry> = state
        .provider_registry
        .providers
        .iter()
        .map(|r| {
            let provider = r.value();
            let ep_normalized = provider.endpoint.trim_end_matches('/').to_string();
            let (reqs, pt, ct, tt) = provider_stats
                .get(&provider.endpoint)
                .copied()
                .unwrap_or((0, 0, 0, 0));

            // Enrich with chain metadata if available
            let (provider_id, pown_score, region) = chain_meta
                .get(&ep_normalized)
                .cloned()
                .unwrap_or_else(|| {
                    (
                        provider.endpoint.clone(),
                        0,
                        String::new(),
                    )
                });

            LeaderboardEntry {
                provider_id,
                endpoint: provider.endpoint.clone(),
                online: provider.is_healthy,
                latency_ms: provider.latency_ms,
                total_requests: reqs,
                total_prompt_tokens: pt,
                total_completion_tokens: ct,
                total_tokens: tt,
                pown_score: if chain_meta.contains_key(&ep_normalized) {
                    Some(pown_score)
                } else {
                    None
                },
                region: if chain_meta.contains_key(&ep_normalized) && !region.is_empty() {
                    Some(region)
                } else {
                    None
                },
            }
        })
        .collect();

    // Sort by total_tokens descending
    entries.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    Json(entries)
}
