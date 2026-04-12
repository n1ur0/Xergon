//! Cost Estimation API.
//!
//! Provides endpoints for estimating request costs across providers.
//! Uses pricing data from the ModelRegistry to calculate costs.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::model_registry::ProviderEntry;
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Query parameters for single cost estimation.
#[derive(Debug, Deserialize)]
pub struct CostEstimateQuery {
    /// Model name (e.g., "llama-3.1-70b").
    pub model: String,
    /// Estimated number of tokens (default: 1000).
    pub tokens: Option<u32>,
    /// Routing strategy to use for provider selection (default: "cost_optimized").
    pub strategy: Option<String>,
}

/// Query parameters for batch cost estimation.
#[derive(Debug, Deserialize)]
pub struct BatchCostEstimateRequest {
    /// Array of cost estimation requests.
    pub requests: Vec<CostEstimateItem>,
}

/// A single cost estimation item in a batch request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CostEstimateItem {
    /// Model name.
    pub model: String,
    /// Estimated number of tokens (default: 1000).
    #[serde(default = "default_tokens")]
    pub tokens: u32,
    /// Routing strategy.
    #[serde(default = "default_strategy")]
    pub strategy: String,
}

fn default_tokens() -> u32 {
    1000
}
fn default_strategy() -> String {
    "cost_optimized".to_string()
}

/// Cost information for a single provider.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderCostInfo {
    /// Provider public key.
    pub provider_pk: String,
    /// Provider endpoint.
    pub endpoint: String,
    /// Price per 1M tokens in nanoERG.
    pub price_per_1m: u64,
    /// Estimated cost for the requested tokens in nanoERG.
    pub estimated_cost_nanoerg: u64,
    /// Whether this provider is currently available.
    pub is_available: bool,
}

/// Single cost estimation response.
#[derive(Debug, Clone, Serialize)]
pub struct CostEstimateResponse {
    /// Model name.
    pub model: String,
    /// Number of tokens used for estimation.
    pub tokens: u32,
    /// Estimated total cost in nanoERG.
    pub estimated_cost_nanoerg: u64,
    /// Estimated total cost in ERG (1 ERG = 1,000,000,000 nanoERG).
    pub estimated_cost_erg: f64,
    /// Routing strategy used.
    pub strategy: String,
    /// All providers considered for this model.
    pub providers_considered: Vec<ProviderCostInfo>,
    /// The cheapest provider (by estimated cost).
    pub cheapest_provider: Option<ProviderCostInfo>,
    /// The best-value provider (cost/health-weighted score).
    pub best_value_provider: Option<ProviderCostInfo>,
}

/// Batch cost estimation response.
#[derive(Debug, Clone, Serialize)]
pub struct BatchCostEstimateResponse {
    /// Array of individual cost estimates (may include error objects for models with no providers).
    pub estimates: Vec<serde_json::Value>,
}

/// Error response for cost estimation.
#[derive(Debug, Serialize)]
pub struct CostEstimateError {
    pub error: String,
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/cost/estimate?model=llama-3.1-70b&tokens=1000&strategy=cost_optimized
///
/// Returns cost estimation for a single model across all providers.
pub async fn cost_estimate_handler(
    State(state): State<AppState>,
    Query(params): Query<CostEstimateQuery>,
) -> impl IntoResponse {
    let tokens = params.tokens.unwrap_or(1000);
    let strategy = params.strategy.as_deref().unwrap_or("cost_optimized");

    let providers = state
        .model_registry
        .get_providers_for_model(&params.model);

    if providers.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!(CostEstimateError {
                error: format!("No providers found for model '{}'", params.model),
                model: Some(params.model.clone()),
            })),
        );
    }

    let response = build_cost_estimate(&params.model, tokens, strategy, &providers, &state);
    (StatusCode::OK, Json(serde_json::json!(response)))
}

/// POST /v1/cost/estimate-batch
///
/// Returns cost estimation for multiple models at once.
pub async fn cost_estimate_batch_handler(
    State(state): State<AppState>,
    Json(body): Json<BatchCostEstimateRequest>,
) -> impl IntoResponse {
    let mut estimates = Vec::new();

    for item in &body.requests {
        let providers = state.model_registry.get_providers_for_model(&item.model);

        if providers.is_empty() {
            estimates.push(serde_json::json!(CostEstimateError {
                error: format!("No providers found for model '{}'", item.model),
                model: Some(item.model.clone()),
            }));
            continue;
        }

        let estimate =
            build_cost_estimate(&item.model, item.tokens, &item.strategy, &providers, &state);
        estimates.push(serde_json::json!(estimate));
    }

    (
        StatusCode::OK,
        Json(serde_json::json!(BatchCostEstimateResponse { estimates })),
    )
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a cost estimate from provider data.
fn build_cost_estimate(
    model: &str,
    tokens: u32,
    strategy: &str,
    providers: &[ProviderEntry],
    state: &AppState,
) -> CostEstimateResponse {
    let providers_considered: Vec<ProviderCostInfo> = providers
        .iter()
        .map(|p| {
            let cost = estimate_cost(p.pricing_nanoerg_per_million_tokens, tokens);
            ProviderCostInfo {
                provider_pk: p.provider_pk.clone(),
                endpoint: p.provider_endpoint.clone(),
                price_per_1m: p.pricing_nanoerg_per_million_tokens,
                estimated_cost_nanoerg: cost,
                is_available: p.is_available,
            }
        })
        .collect();

    // Find cheapest provider
    let cheapest_provider = providers_considered
        .iter()
        .filter(|p| p.is_available)
        .min_by_key(|p| p.estimated_cost_nanoerg)
        .cloned();

    // Find best-value provider (cost/health weighted score)
    let best_value_provider = find_best_value_provider(&providers_considered, state);

    // Calculate total estimated cost (from cheapest or first available)
    let total_cost = cheapest_provider
        .as_ref()
        .map(|p| p.estimated_cost_nanoerg)
        .unwrap_or(0);

    debug!(
        model = %model,
        tokens,
        providers = providers_considered.len(),
        estimated_cost_nanoerg = total_cost,
        strategy = %strategy,
        "Cost estimate computed"
    );

    CostEstimateResponse {
        model: model.to_string(),
        tokens,
        estimated_cost_nanoerg: total_cost,
        estimated_cost_erg: total_cost as f64 / 1_000_000_000.0,
        strategy: strategy.to_string(),
        providers_considered,
        cheapest_provider,
        best_value_provider,
    }
}

/// Calculate estimated cost in nanoERG.
fn estimate_cost(price_per_1m_tokens: u64, tokens: u32) -> u64 {
    if price_per_1m_tokens == 0 {
        return 0;
    }
    // cost = (price_per_1m * tokens) / 1_000_000
    (price_per_1m_tokens as u128 * tokens as u128 / 1_000_000) as u64
}

/// Find the best-value provider using health score + cost balance.
/// Uses the same 60/40 weighting as AdaptiveRouter::select_cost_optimized.
fn find_best_value_provider(
    providers: &[ProviderCostInfo],
    state: &AppState,
) -> Option<ProviderCostInfo> {
    let available: Vec<&ProviderCostInfo> = providers.iter().filter(|p| p.is_available).collect();
    if available.is_empty() {
        return None;
    }

    let mut best: Option<(&ProviderCostInfo, f64)> = None;

    for provider in &available {
        let health = state
            .health_scorer
            .get_score(&provider.provider_pk)
            .map(|s| s.overall_score)
            .unwrap_or(0.5);

        let cost_score = 1.0 / (1.0 + provider.estimated_cost_nanoerg as f64 / 100_000_000.0);
        let combined = 0.6 * health + 0.4 * cost_score;

        if best.is_none() || combined > best.unwrap().1 {
            best = Some((provider, combined));
        }
    }

    best.map(|(p, _)| (*p).clone())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider(pk: &str, price: u64, available: bool) -> ProviderEntry {
        ProviderEntry {
            provider_pk: pk.to_string(),
            provider_endpoint: format!("http://{}.example.com", pk),
            pricing_nanoerg_per_million_tokens: price,
            context_length: 4096,
            is_available: available,
        }
    }

    #[test]
    fn test_estimate_cost() {
        // 500 nanoERG per 1M tokens, 1000 tokens -> 0 nanoERG (rounded down)
        assert_eq!(estimate_cost(500, 1000), 0);
        // 1_000_000 nanoERG per 1M tokens, 1000 tokens -> 1_000 nanoERG
        assert_eq!(estimate_cost(1_000_000, 1000), 1_000);
        // 1_000_000 nanoERG per 1M tokens, 1_000_000 tokens -> 1_000_000 nanoERG
        assert_eq!(estimate_cost(1_000_000, 1_000_000), 1_000_000);
        // Free provider
        assert_eq!(estimate_cost(0, 1000), 0);
    }

    #[test]
    fn test_build_cost_estimate_no_state() {
        let providers = vec![
            make_provider("pk1", 1_000_000_000, true), // 1 ERG per 1M
            make_provider("pk2", 500_000_000, true),  // 0.5 ERG per 1M
            make_provider("pk3", 100_000_000, false), // 0.1 ERG per 1M, offline
        ];

        let providers_considered: Vec<ProviderCostInfo> = providers
            .iter()
            .map(|p| ProviderCostInfo {
                provider_pk: p.provider_pk.clone(),
                endpoint: p.provider_endpoint.clone(),
                price_per_1m: p.pricing_nanoerg_per_million_tokens,
                estimated_cost_nanoerg: estimate_cost(p.pricing_nanoerg_per_million_tokens, 1000),
                is_available: p.is_available,
            })
            .collect();

        // pk2 should be cheapest available (500_000 nanoERG for 1000 tokens)
        let cheapest = providers_considered
            .iter()
            .filter(|p| p.is_available)
            .min_by_key(|p| p.estimated_cost_nanoerg);
        assert_eq!(cheapest.unwrap().provider_pk, "pk2");
        assert_eq!(cheapest.unwrap().estimated_cost_nanoerg, 500_000);

        // Total should be 500_000 nanoERG -> 0.0005 ERG
        let total = cheapest.unwrap().estimated_cost_nanoerg;
        assert_eq!(total, 500_000);
        assert!((total as f64 / 1_000_000_000.0 - 5e-4).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cost_estimate_item_defaults() {
        let item = serde_json::from_value::<CostEstimateItem>(serde_json::json!({
            "model": "llama-3.1"
        }))
        .unwrap();
        assert_eq!(item.model, "llama-3.1");
        assert_eq!(item.tokens, 1000);
        assert_eq!(item.strategy, "cost_optimized");
    }
}
