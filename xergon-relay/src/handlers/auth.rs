//! GET /v1/auth/status — Auth configuration status endpoint
//!
//! Returns the current authentication configuration so CLI tools
//! can determine what auth the relay expects.

use axum::{extract::State, response::IntoResponse, Json};
use serde::Serialize;

/// Response for the /v1/auth/status endpoint.
#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub auth_enabled: bool,
    pub require_staking_box: bool,
    pub max_age_secs: i64,
    pub timestamp: i64,
}

/// GET /v1/auth/status handler
pub async fn auth_status_handler(
    State(state): State<crate::proxy::AppState>,
) -> impl IntoResponse {
    let (auth_enabled, require_staking_box, max_age_secs) =
        if let Some(verifier) = &state.auth_verifier {
            (true, verifier.requires_staking_box(), verifier.max_age_secs())
        } else {
            // Auth is disabled or not configured
            (
                state.config.auth.enabled,
                state.config.auth.require_staking_box,
                state.config.auth.max_age_secs,
            )
        };

    let response = AuthStatusResponse {
        auth_enabled,
        require_staking_box,
        max_age_secs,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    Json(response)
}
