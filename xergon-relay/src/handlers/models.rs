//! GET /v1/models — List available models from all healthy providers
//!
//! Returns an enriched format that the marketplace expects (ModelInfo[]).
//!
//! When chain scanning is enabled, models are sourced from on-chain
//! provider metadata (ChainCache). Otherwise, models are collected from
//! the live provider registry (/xergon/status responses).

use axum::{extract::State, response::Json};
use serde::Serialize;
use tracing::info;

use crate::provider::collect_models;

/// Enriched model info matching the marketplace's ModelInfo interface.
#[derive(Debug, Serialize)]
pub struct EnrichedModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub tier: String,
    /// USD price per input token (kept for backward compat)
    pub price_per_input_token: f64,
    /// USD price per output token (kept for backward compat)
    pub price_per_output_token: f64,
    /// nanoERG per 1K input tokens (real chain-sourced pricing)
    pub price_per_input_token_nanoerg: u64,
    /// nanoERG per 1K output tokens (real chain-sourced pricing)
    pub price_per_output_token_nanoerg: u64,
    /// nanoERG price from the cheapest provider for this model
    pub min_provider_price_nanoerg: u64,
    /// Effective nanoERG price per 1K tokens after demand multiplier
    pub effective_price_nanoerg: u64,
    /// Number of providers serving this model
    pub provider_count: usize,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_tier: Option<bool>,
    /// Current ERG/USD rate from oracle (None if oracle not configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erg_usd_rate: Option<f64>,
}

/// Derive enriched metadata from a model ID using heuristic detection.
fn enrich_model(
    id: &str,
    name: &str,
    provider_count: usize,
    min_price_nanoerg: u64,
    demand_multiplier: f64,
    erg_usd_rate: Option<f64>,
) -> EnrichedModel {
    let id_lower = id.to_lowercase();

    // Detect model family for sensible defaults
    let (tier, speed, context_window, tags, description) = if id_lower.contains("llama") {
        if id_lower.contains("70b") || id_lower.contains("72b") {
            (
                "pro",
                Some("balanced".to_string()),
                Some(8192),
                Some(vec!["Smart".into(), "Code".into()]),
                Some("General-purpose large language model".into()),
            )
        } else {
            (
                "free",
                Some("fast".to_string()),
                Some(4096),
                Some(vec!["Fast".into(), "Free".into()]),
                Some("Fast and efficient language model".into()),
            )
        }
    } else if id_lower.contains("qwen") {
        (
            "pro",
            Some("balanced".to_string()),
            Some(32768),
            Some(vec!["Smart".into(), "Code".into(), "Creative".into()]),
            Some("High-capability model with strong reasoning".into()),
        )
    } else if id_lower.contains("mistral") || id_lower.contains("mixtral") {
        (
            "free",
            Some("fast".to_string()),
            Some(32768),
            Some(vec!["Fast".into(), "Code".into()]),
            Some("Efficient model with strong coding ability".into()),
        )
    } else {
        (
            "free",
            Some("balanced".to_string()),
            Some(4096),
            Some(vec!["Free".into()]),
            Some("Community-hosted model".into()),
        )
    };

    // Override tier based on actual pricing: if min_price > 0, it's not free
    let is_free = min_price_nanoerg == 0;
    let tier = if is_free { "free" } else { tier };
    let free_tier = Some(is_free);

    // nanoERG pricing: min_price is per 1M tokens, convert to per 1K tokens
    let price_per_1k_input = min_price_nanoerg / 1000;
    let price_per_1k_output = min_price_nanoerg / 1000; // same rate for now

    // Effective price after demand multiplier (round to u64)
    let effective_price = (price_per_1k_input as f64 * demand_multiplier).round() as u64;

    // Legacy USD pricing (kept for backward compat, derived from nanoERG)
    // Use oracle rate when available, otherwise fall back to hardcoded 0.50
    let erg_to_usd = erg_usd_rate.unwrap_or(0.50);
    let (price_in, price_out) = if is_free {
        (0.0, 0.0)
    } else {
        let usd_per_1k = (min_price_nanoerg as f64) * erg_to_usd / 1_000_000_000.0 / 1000.0;
        (usd_per_1k, usd_per_1k)
    };

    EnrichedModel {
        id: id.to_string(),
        name: name.to_string(),
        provider: format!(
            "{} provider{}",
            provider_count,
            if provider_count != 1 { "s" } else { "" }
        ),
        tier: tier.to_string(),
        price_per_input_token: price_in,
        price_per_output_token: price_out,
        price_per_input_token_nanoerg: price_per_1k_input,
        price_per_output_token_nanoerg: price_per_1k_output,
        min_provider_price_nanoerg: min_price_nanoerg,
        effective_price_nanoerg: effective_price,
        provider_count,
        available: true,
        description,
        context_window,
        speed,
        tags,
        free_tier,
        erg_usd_rate,
    }
}

/// Collect models from chain cache when available.
/// Returns a Vec of (model_id, model_name, provider_count, min_price_nanoerg_per_1m_tokens) tuples.
fn collect_models_from_chain(
    state: &crate::proxy::AppState,
) -> Option<Vec<(String, String, usize, u64)>> {
    let cache = state.chain_cache.as_ref()?;
    let providers = cache.get_providers()?;
    if providers.is_empty() {
        return None;
    }

    // Track per-model: (provider_count, min_price)
    let mut model_data: std::collections::HashMap<String, (usize, u64)> =
        std::collections::HashMap::new();

    for cp in &providers {
        for model in &cp.models {
            let normalized = model.to_lowercase();
            let price = cp
                .model_pricing
                .get(&normalized)
                .copied()
                .unwrap_or(0);
            let entry = model_data.entry(normalized).or_insert((0, u64::MAX));
            entry.0 += 1;
            if price < entry.1 {
                entry.1 = price;
            }
        }
    }

    // Fix up u64::MAX for models with no pricing data (they're free)
    for (_, data) in model_data.iter_mut() {
        if data.1 == u64::MAX {
            data.1 = 0;
        }
    }

    // Only use chain data if we found models
    if model_data.is_empty() {
        return None;
    }

    Some(
        model_data
            .into_iter()
            .map(|(id, (count, min_price))| (id.clone(), id.clone(), count, min_price))
            .collect(),
    )
}

/// GET /v1/models handler
pub async fn list_models_handler(
    State(state): State<crate::proxy::AppState>,
) -> Json<Vec<EnrichedModel>> {
    info!("Listing available models");

    // Read the current oracle rate
    let oracle_rate = state.erg_usd_rate.read().ok().and_then(|g| *g);

    // Prefer chain-sourced models when available
    let data: Vec<EnrichedModel> =
        if let Some(chain_models) = collect_models_from_chain(&state) {
            chain_models
                .into_iter()
                .map(|(id, name, count, min_price)| {
                    let demand_mult = state.demand.demand_multiplier(&id);
                    enrich_model(&id, &name, count, min_price, demand_mult, oracle_rate)
                })
                .collect()
        } else {
            // Fallback: collect from live provider registry (no pricing data available)
            let models = collect_models(&state.provider_registry);
            models
                .into_iter()
                .map(|m| {
                    let demand_mult = state.demand.demand_multiplier(&m.id);
                    enrich_model(&m.id, &m.name, m.provider_count, 0, demand_mult, oracle_rate)
                })
                .collect()
        };

    Json(data)
}
