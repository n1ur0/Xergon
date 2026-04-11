//! Routing management endpoints.
//!
//! GET  /v1/routing/stats    - AdaptiveRouter statistics
//! GET  /v1/routing/health   - All provider health scores
//! GET  /v1/routing/geo      - Geo routing info and latency matrix
//! PUT  /v1/routing/strategy - Switch routing strategy at runtime

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::adaptive_router::RoutingStrategy;
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// GET /v1/routing/stats
// ---------------------------------------------------------------------------

/// GET /v1/routing/stats - AdaptiveRouter statistics
pub async fn routing_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.adaptive_router.get_stats();
    let scorer_stats = state.health_scorer.get_stats();

    Json(serde_json::json!({
        "router": stats,
        "health_scorer": scorer_stats,
    }))
}

// ---------------------------------------------------------------------------
// GET /v1/routing/health
// ---------------------------------------------------------------------------

/// GET /v1/routing/health - All provider health scores
pub async fn routing_health_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let all_scores = state.health_scorer.get_all_scores();

    Json(serde_json::json!({
        "providers": all_scores,
        "total": all_scores.len(),
    }))
}

// ---------------------------------------------------------------------------
// GET /v1/routing/geo
// ---------------------------------------------------------------------------

/// GET /v1/routing/geo - Geo routing info and latency matrix
pub async fn routing_geo_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let regions = state.geo_router.get_all_provider_regions();
    let latency_matrix = state.geo_router.get_latency_matrix();

    Json(serde_json::json!({
        "providers": regions,
        "latency_matrix": latency_matrix,
        "total_providers": regions.len(),
    }))
}

// ---------------------------------------------------------------------------
// PUT /v1/routing/strategy
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SetStrategyRequest {
    pub strategy: String,
}

#[derive(Debug, Serialize)]
pub struct SetStrategyResponse {
    pub strategy: String,
    pub previous_strategy: String,
    pub message: String,
}

/// PUT /v1/routing/strategy - Switch routing strategy at runtime
pub async fn set_routing_strategy_handler(
    State(state): State<AppState>,
    Json(body): Json<SetStrategyRequest>,
) -> impl IntoResponse {
    let parsed: Result<RoutingStrategy, _> =
        serde_json::from_value(serde_json::json!(body.strategy));

    match parsed {
        Ok(strategy) => {
            let previous = state.adaptive_router.get_strategy();
            let previous_str = format!("{previous}");
            info!(
                old_strategy = %previous,
                new_strategy = %strategy,
                "Routing strategy changed via API"
            );
            state.adaptive_router.set_strategy(strategy);
            let new_str = format!("{strategy}");

            (
                StatusCode::OK,
                Json(serde_json::json!(SetStrategyResponse {
                    strategy: new_str,
                    previous_strategy: previous_str,
                    message: "Routing strategy updated".into(),
                })),
            )
        }
        Err(_) => {
            let valid = vec![
                "health_score",
                "lowest_latency",
                "round_robin",
                "weighted_random",
                "least_connections",
                "cost_optimized",
            ];
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid strategy '{}'. Valid strategies: {:?}", body.strategy, valid),
                    "valid_strategies": valid,
                })),
            )
        }
    }
}
