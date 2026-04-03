//! Admin API endpoints — user management, provider listing, platform stats.
//!
//! All endpoints require an X-Admin-Token header matching the XERGON_ADMIN_TOKEN
//! environment variable. If the env var is not set, admin API returns 503.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::db::UserSummary;
use crate::proxy::AppState;

// ── Request / Response types ──

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub tier: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListUsersResponse {
    pub users: Vec<UserSummary>,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTierBody {
    pub tier: String,
}

#[derive(Debug, Deserialize)]
pub struct AdjustCreditsBody {
    pub amount: f64,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct AdminUserResponse {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub tier: String,
    pub credits: f64,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AdminProviderInfo {
    pub provider_id: String,
    pub name: String,
    pub endpoint: String,
    pub region: String,
    pub models: Vec<String>,
    pub status: String,
    pub last_heartbeat: chrono::DateTime<Utc>,
    pub ergo_address: String,
}

#[derive(Debug, Serialize)]
pub struct AdminProvidersResponse {
    pub providers: Vec<AdminProviderInfo>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct AdminErrorResponse {
    pub error: String,
}

// ── Admin token auth ──

/// Check admin token from request headers against XERGON_ADMIN_TOKEN env var.
/// Returns Ok(()) on success, or an error response on failure.
fn check_admin_token(headers: &HeaderMap) -> Result<(), (StatusCode, Json<AdminErrorResponse>)> {
    let expected = match std::env::var("XERGON_ADMIN_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(AdminErrorResponse {
                    error: "Admin API is disabled (XERGON_ADMIN_TOKEN not set)".into(),
                }),
            ));
        }
    };

    let provided = headers
        .get("x-admin-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provided.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(AdminErrorResponse {
                error: "Missing X-Admin-Token header".into(),
            }),
        ));
    }

    // Constant-time comparison to prevent timing attacks
    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        warn!("Admin auth failed: invalid token");
        return Err((
            StatusCode::FORBIDDEN,
            Json(AdminErrorResponse {
                error: "Invalid admin token".into(),
            }),
        ));
    }

    Ok(())
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // Pad shorter string with zeros to match the longer string's length
    let max_len = a.len().max(b.len());
    let a_padded: Vec<u8> = a.iter().copied().chain(std::iter::repeat(0)).take(max_len).collect();
    let b_padded: Vec<u8> = b.iter().copied().chain(std::iter::repeat(0)).take(max_len).collect();
    let mut result: u8 = 0;
    for (x, y) in a_padded.iter().zip(b_padded.iter()) {
        result |= x ^ y;
    }
    result == 0
}

// ── Handlers ──

/// GET /v1/admin/users — list all users with pagination and optional tier filter
pub async fn list_users_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListUsersQuery>,
) -> impl IntoResponse {
    if let Err(e) = check_admin_token(&headers) {
        return e.into_response();
    }

    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).clamp(0, 10_000);
    let tier_filter = query.tier.as_deref().filter(|t| !t.is_empty());

    match state.db.list_users(limit, offset, tier_filter) {
        Ok((users, total)) => (
            StatusCode::OK,
            Json(ListUsersResponse { users, total }),
        ).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Admin list_users failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminErrorResponse {
                    error: "Internal server error".into(),
                }),
            )
                .into_response()
        }
    }
}

/// PUT /v1/admin/users/:id/tier — change a user's tier
pub async fn update_user_tier_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(body): Json<UpdateTierBody>,
) -> impl IntoResponse {
    if let Err(e) = check_admin_token(&headers) {
        return e.into_response();
    }

    // Validate tier value
    let tier = body.tier.to_lowercase();
    if tier != "free" && tier != "pro" {
        return (
            StatusCode::BAD_REQUEST,
            Json(AdminErrorResponse {
                error: "Invalid tier. Must be 'free' or 'pro'.".into(),
            }),
        )
            .into_response();
    }

    match state.db.update_user_tier_admin(&user_id, &tier) {
        Ok(user) => {
            let credits = state.db.get_credit_balance(&user.id).unwrap_or(0.0);
            info!(user_id = %user_id, new_tier = %tier, "Admin updated user tier");
            (
                StatusCode::OK,
                Json(AdminUserResponse {
                    id: user.id,
                    email: user.email,
                    name: user.name,
                    tier: user.tier,
                    credits,
                    created_at: user.created_at,
                }),
            )
                .into_response()
        }
        Err(_e) => (
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse {
                error: "User not found or update failed".into(),
            }),
        )
            .into_response(),
    }
}

/// PUT /v1/admin/users/:id/credits — adjust user credits (add or deduct)
pub async fn adjust_user_credits_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(body): Json<AdjustCreditsBody>,
) -> impl IntoResponse {
    if let Err(e) = check_admin_token(&headers) {
        return e.into_response();
    }

    if body.reason.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(AdminErrorResponse {
                error: "Reason is required for credit adjustments".into(),
            }),
        )
            .into_response();
    }

    if body.amount == 0.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(AdminErrorResponse {
                error: "Amount must be non-zero".into(),
            }),
        )
            .into_response();
    }

    match state.db.admin_adjust_credits(&user_id, body.amount, &body.reason) {
        Ok(user) => {
            let credits = state.db.get_credit_balance(&user.id).unwrap_or(0.0);
            info!(
                user_id = %user_id,
                amount = body.amount,
                reason = %body.reason,
                "Admin adjusted user credits"
            );
            (
                StatusCode::OK,
                Json(AdminUserResponse {
                    id: user.id,
                    email: user.email,
                    name: user.name,
                    tier: user.tier,
                    credits,
                    created_at: user.created_at,
                }),
            )
                .into_response()
        }
        Err(_e) => (
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse {
                error: "User not found or adjustment failed".into(),
            }),
        )
            .into_response(),
    }
}

/// GET /v1/admin/providers — list all registered providers from the provider directory
pub async fn list_providers_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = check_admin_token(&headers) {
        return e.into_response();
    }

    let directory = &state.provider_directory;
    let now = Utc::now();

    // List all providers (including expired ones, for admin visibility)
    let all_providers: Vec<AdminProviderInfo> = directory
        .list_providers(false)
        .providers
        .into_iter()
        .map(|p| {
            let expires_at = p.last_heartbeat
                + chrono::Duration::seconds(p.ttl_secs as i64);
            let status = if now <= expires_at {
                "healthy".to_string()
            } else {
                "unhealthy".to_string()
            };
            AdminProviderInfo {
                provider_id: p.provider_id,
                name: p.provider_name,
                endpoint: p.endpoint,
                region: p.region,
                models: p.models,
                status,
                last_heartbeat: p.last_heartbeat,
                ergo_address: p.ergo_address,
            }
        })
        .collect();

    let total = all_providers.len();

    (
        StatusCode::OK,
        Json(AdminProvidersResponse {
            providers: all_providers,
            total,
        }),
    )
        .into_response()
}

/// GET /v1/admin/stats — platform statistics
pub async fn get_stats_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = check_admin_token(&headers) {
        return e.into_response();
    }

    match state.db.get_platform_stats() {
        Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Admin get_platform_stats failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminErrorResponse {
                    error: "Internal server error".into(),
                }),
            )
                .into_response()
        }
    }
}
