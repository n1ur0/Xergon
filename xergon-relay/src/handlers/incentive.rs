//! Incentive system endpoints
//!
//! GET /v1/incentive/status — Show current rarity bonuses for all models
//! GET /v1/incentive/models — List models sorted by rarity (most rare first)

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use crate::proxy::AppState;
use crate::provider::{collect_models_by_rarity, model_rarity_from_registry};

/// Incentive system status
#[derive(Debug, Serialize)]
pub struct IncentiveStatus {
    pub rarity_bonus_enabled: bool,
    pub max_multiplier: f64,
    pub min_providers: usize,
    pub total_models: usize,
    pub rare_models: usize,
    pub models: Vec<serde_json::Value>,
}

/// GET /v1/incentive/status
///
/// Returns the current incentive configuration and rarity bonuses for all models.
pub async fn incentive_status_handler(
    State(state): State<AppState>,
) -> Response {
    let config = &state.config.incentive;
    let models = collect_models_by_rarity(
        &state.provider_registry,
        config.rarity_max_multiplier,
    );

    let rare_count = models.iter().filter(|m| m.is_rare).count();

    let model_details: Vec<serde_json::Value> = models
        .iter()
        .map(|m| {
            serde_json::json!({
                "model_id": m.model_id,
                "model_name": m.model_name,
                "provider_count": m.provider_count,
                "rarity_multiplier": m.rarity_multiplier,
                "is_rare": m.is_rare,
            })
        })
        .collect();

    let status = IncentiveStatus {
        rarity_bonus_enabled: config.rarity_bonus_enabled,
        max_multiplier: config.rarity_max_multiplier,
        min_providers: config.rarity_min_providers,
        total_models: models.len(),
        rare_models: rare_count,
        models: model_details,
    };

    Json(status).into_response()
}

/// GET /v1/incentive/models
///
/// Lists all models sorted by rarity (most rare first).
pub async fn incentive_models_handler(
    State(state): State<AppState>,
) -> Response {
    let config = &state.config.incentive;
    let models = collect_models_by_rarity(
        &state.provider_registry,
        config.rarity_max_multiplier,
    );

    Json(serde_json::json!({
        "rarity_bonus_enabled": config.rarity_bonus_enabled,
        "max_multiplier": config.rarity_max_multiplier,
        "models": models,
    }))
    .into_response()
}

/// GET /v1/incentive/models/:model
///
/// Get rarity info for a specific model.
pub async fn incentive_model_detail_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    let config = &state.config.incentive;
    let multiplier = model_rarity_from_registry(
        &state.provider_registry,
        &model,
        config.rarity_max_multiplier,
    );

    Json(serde_json::json!({
        "model": model,
        "rarity_multiplier": multiplier,
        "rarity_bonus_enabled": config.rarity_bonus_enabled,
        "is_rare": multiplier > 1.5,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::compute_rarity_multiplier;

    #[test]
    fn test_compute_rarity_multiplier_single_provider() {
        let mult = compute_rarity_multiplier(1, 10.0);
        assert!((mult - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_rarity_multiplier_many_providers() {
        let mult = compute_rarity_multiplier(10, 10.0);
        assert!((mult - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_rarity_multiplier_capped() {
        let mult = compute_rarity_multiplier(1, 10.0);
        assert!(mult <= 10.0);
        assert!(mult >= 1.0);
    }

    #[test]
    fn test_compute_rarity_multiplier_zero_providers() {
        let mult = compute_rarity_multiplier(0, 10.0);
        assert!((mult - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_rarity_multiplier_fractional() {
        let mult = compute_rarity_multiplier(3, 10.0);
        // 10.0 / 3.0 = 3.333...
        assert!((mult - 3.333).abs() < 0.01);
    }
}
