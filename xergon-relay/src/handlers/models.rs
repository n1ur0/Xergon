//! GET /v1/models — List available models from all healthy providers
//!
//! Returns an enriched format that the marketplace expects (ModelInfo[])
//! while keeping the response array-shaped for easy consumption.
//!
//! Pricing resolution order:
//!   1. Check model_pricing DB table for a matching provider+model entry
//!   2. Fall back to heuristic-based pricing (model ID string detection)

use axum::{
    extract::State,
    response::Json,
};
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
    pub price_per_input_token: f64,
    pub price_per_output_token: f64,
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
}

/// Try to resolve pricing from the DB for a given model.
/// Returns the per-token prices (price_in, price_out) if a DB entry exists.
fn resolve_pricing_from_db(
    model_id: &str,
    db: &crate::db::Db,
) -> Option<(f64, f64)> {
    // Query all pricing entries for this model (any provider).
    // Use the first match found. In future this could be smarter about
    // selecting the cheapest or most relevant provider.
    let entries = db.get_model_pricing(None, Some(model_id)).ok()?;

    if let Some(entry) = entries.first() {
        // Convert per-1K prices to per-token prices
        let price_in = entry.price_per_1k_prompt / 1000.0;
        let price_out = entry.price_per_1k_completion / 1000.0;
        info!(
            model_id = %model_id,
            provider_id = %entry.provider_id,
            price_per_1k_prompt = entry.price_per_1k_prompt,
            price_per_1k_completion = entry.price_per_1k_completion,
            "Pricing resolved from DB"
        );
        return Some((price_in, price_out));
    }

    None
}

/// Derive enriched metadata from a model ID using heuristic detection.
fn enrich_model(
    id: &str,
    name: &str,
    provider_count: usize,
    db: &crate::db::Db,
) -> EnrichedModel {
    let id_lower = id.to_lowercase();

    // Detect model family for sensible defaults
    let (tier, speed, context_window, tags, description) = if id_lower.contains("llama") {
        if id_lower.contains("70b") || id_lower.contains("72b") {
            ("pro", Some("balanced".to_string()), Some(8192),
             Some(vec!["Smart".into(), "Code".into()]),
             Some("General-purpose large language model".into()))
        } else {
            ("free", Some("fast".to_string()), Some(4096),
             Some(vec!["Fast".into(), "Free".into()]),
             Some("Fast and efficient language model".into()))
        }
    } else if id_lower.contains("qwen") {
        ("pro", Some("balanced".to_string()), Some(32768),
         Some(vec!["Smart".into(), "Code".into(), "Creative".into()]),
         Some("High-capability model with strong reasoning".into()))
    } else if id_lower.contains("mistral") || id_lower.contains("mixtral") {
        ("free", Some("fast".to_string()), Some(32768),
         Some(vec!["Fast".into(), "Code".into()]),
         Some("Efficient model with strong coding ability".into()))
    } else {
        ("free", Some("balanced".to_string()), Some(4096),
         Some(vec!["Free".into()]),
         Some("Community-hosted model".into()))
    };

    let free_tier = tier == "free";

    // Try DB pricing first, fall back to heuristic
    let (price_in, price_out, _pricing_source) =
        if let Some((db_in, db_out)) = resolve_pricing_from_db(id, db) {
            (db_in, db_out, "db")
        } else {
            // Heuristic: free tier is $0, pro tier uses default cost
            let (heuristic_in, heuristic_out) = if free_tier {
                (0.0, 0.0)
            } else {
                (0.000002, 0.000002) // $0.002/1K tokens
            };
            info!(
                model_id = %id,
                price_in = heuristic_in,
                price_out = heuristic_out,
                "Pricing resolved from heuristic (no DB entry)"
            );
            (heuristic_in, heuristic_out, "heuristic")
        };

    EnrichedModel {
        id: id.to_string(),
        name: name.to_string(),
        provider: format!("{} provider{}", provider_count, if provider_count != 1 { "s" } else { "" }),
        tier: tier.to_string(),
        price_per_input_token: price_in,
        price_per_output_token: price_out,
        available: true,
        description,
        context_window,
        speed,
        tags,
        free_tier: Some(free_tier),
    }
}

/// GET /v1/models handler
pub async fn list_models_handler(
    State(state): State<crate::proxy::AppState>,
) -> Json<Vec<EnrichedModel>> {
    info!("Listing available models");

    let models = collect_models(&state.provider_registry);
    let db = &state.db;

    let data: Vec<EnrichedModel> = models
        .into_iter()
        .map(|m| enrich_model(&m.id, &m.name, m.provider_count, db))
        .collect();

    Json(data)
}
