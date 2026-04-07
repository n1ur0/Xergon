//! GET /v1/models/:model_id -- Model detail handler
//!
//! Returns detailed info about a specific model including all providers
//! that serve it, with pricing, latency, and health data.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Serialize;
use tracing::info;

use crate::proxy::AppState;

/// Response for GET /v1/models/:model_id
#[derive(Debug, Serialize)]
pub struct ModelDetailResponse {
    pub model_id: String,
    pub providers: Vec<ModelProviderInfo>,
    pub available_providers: usize,
    pub cheapest_price_nanoerg_per_million_tokens: u64,
    pub max_context_length: u32,
    pub avg_latency_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ModelProviderInfo {
    pub provider_pk: String,
    pub provider_endpoint: String,
    pub pricing_nanoerg_per_million_tokens: u64,
    pub context_length: u32,
    pub is_available: bool,
    pub latency_ms: Option<u64>,
    pub healthy: bool,
}

/// GET /v1/models/:model_id handler
pub async fn get_model_detail_handler(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> impl IntoResponse {
    info!(model_id = %model_id, "Getting model detail");

    let model_lower = model_id.to_lowercase();

    // Get all provider entries from model registry for this model
    let registry_providers = state.model_registry.get_providers_for_model(&model_lower);

    if registry_providers.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Model '{}' not found", model_id)
            })),
        )
            .into_response();
    }

    // Enrich each provider entry with live health/latency data from ProviderRegistry
    let mut enriched_providers: Vec<ModelProviderInfo> = Vec::new();
    let mut latency_sum: f64 = 0.0;
    let mut latency_count: usize = 0;

    for rp in &registry_providers {
        // Look up live provider data
        let live_data = state
            .provider_registry
            .providers
            .get(&rp.provider_endpoint);

        let (latency_ms, healthy) = match live_data {
            Some(p) => {
                let lat = if p.is_healthy {
                    Some(p.latency_ms)
                } else {
                    None
                };
                (lat, p.is_healthy)
            }
            None => (None, false),
        };

        if let Some(lat) = latency_ms {
            latency_sum += lat as f64;
            latency_count += 1;
        }

        enriched_providers.push(ModelProviderInfo {
            provider_pk: rp.provider_pk.clone(),
            provider_endpoint: rp.provider_endpoint.clone(),
            pricing_nanoerg_per_million_tokens: rp.pricing_nanoerg_per_million_tokens,
            context_length: rp.context_length,
            is_available: rp.is_available,
            latency_ms,
            healthy,
        });
    }

    let available_count = enriched_providers.iter().filter(|p| p.is_available).count();
    let cheapest = enriched_providers
        .iter()
        .map(|p| p.pricing_nanoerg_per_million_tokens)
        .min()
        .unwrap_or(0);
    let max_ctx = enriched_providers
        .iter()
        .map(|p| p.context_length)
        .max()
        .unwrap_or(0);
    let avg_latency = if latency_count > 0 {
        Some(latency_sum / latency_count as f64)
    } else {
        None
    };

    Json(ModelDetailResponse {
        model_id: model_lower,
        providers: enriched_providers,
        available_providers: available_count,
        cheapest_price_nanoerg_per_million_tokens: cheapest,
        max_context_length: max_ctx,
        avg_latency_ms: avg_latency,
    })
    .into_response()
}
