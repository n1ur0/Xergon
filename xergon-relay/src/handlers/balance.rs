//! GET /v1/balance/:user_pk — Check user staking balance
//!
//! Returns the user's on-chain ERG balance from staking boxes,
//! along with metadata about the number of staking boxes found
//! and whether the balance is sufficient for inference requests.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use crate::balance::BalanceResponse;
use crate::proxy::AppState;

/// GET /v1/balance/:user_pk
///
/// Returns the user's staking balance from on-chain Staking Boxes.
pub async fn balance_handler(
    Path(user_pk): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<BalanceResponse>, impl IntoResponse> {
    let balance_config = &state.config.balance;

    // If balance checking is disabled or no checker, return a placeholder
    let checker = match &state.balance_checker {
        Some(c) => c,
        None => {
            return Ok(Json(BalanceResponse {
                user_pk,
                balance_nanoerg: 0,
                balance_erg: 0.0,
                staking_boxes_count: 0,
                sufficient: true, // Allow when not configured
                min_balance_nanoerg: balance_config.min_balance_nanoerg,
            }));
        }
    };

    info!(user_pk = %user_pk, "Checking balance for user");

    match checker.get_balance(&user_pk).await {
        Ok((balance_nanoerg, box_count)) => {
            let balance_erg = balance_nanoerg as f64 / 1_000_000_000.0;
            let sufficient = balance_nanoerg >= balance_config.min_balance_nanoerg;

            Ok(Json(BalanceResponse {
                user_pk,
                balance_nanoerg,
                balance_erg,
                staking_boxes_count: box_count,
                sufficient,
                min_balance_nanoerg: balance_config.min_balance_nanoerg,
            }))
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to check balance");
            let body = serde_json::json!({
                "error": {
                    "message": format!("Failed to check balance: {}", e),
                    "type": "balance_check_error",
                    "code": 500
                }
            });
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::body::Body::from(serde_json::to_string(&body).unwrap()),
            ))
        }
    }
}
