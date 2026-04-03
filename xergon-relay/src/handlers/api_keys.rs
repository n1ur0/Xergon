//! API key management endpoints.
//!
//! POST   /v1/user/api-keys      — create a new API key
//! GET    /v1/user/api-keys      — list all API keys for the authenticated user
//! DELETE /v1/user/api-keys/:id  — revoke an API key

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;
use uuid::Uuid;

use crate::auth::{extract_claims, AppError};
use crate::db::ApiKeyInfo;
use crate::proxy::AppState;

// ── Request / Response types ──

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub id: String,
    pub key: String,
    pub prefix: String,
    pub name: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct RevokeApiKeyResponse {
    pub message: String,
}

// ── Handlers ──

/// POST /v1/user/api-keys — Generate a new API key.
pub async fn create_api_key_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, AppError> {
    let claims = extract_claims(&headers, &state.config.auth.jwt_secret)?;

    if body.name.trim().is_empty() {
        return Err(AppError::Validation(
            "API key name must not be empty".into(),
        ));
    }

    // Check API key limit (max 10 active keys per user)
    let active_count = state
        .db
        .count_active_api_keys(&claims.sub)
        .map_err(|e| AppError::Internal(format!("Failed to count API keys: {}", e)))?;
    if active_count >= 10 {
        return Err(AppError::Validation(
            "Maximum API keys reached (10)".into(),
        ));
    }

    // Generate key: "xrg_" + 32 random hex chars
    let raw_key = format!("xrg_{}", Uuid::new_v4().simple());
    let prefix = raw_key[..8].to_string();

    // Hash the key with SHA-256
    let key_hash = hash_api_key(&raw_key);

    // Calculate expiry
    let expires_at = body.expires_in_days.map(|days| {
        (Utc::now() + chrono::Duration::days(days as i64)).to_rfc3339()
    });

    let row = state
        .db
        .create_api_key(&claims.sub, &key_hash, &prefix, body.name.trim(), expires_at.as_deref())
        .map_err(|e| AppError::Internal(format!("Failed to create API key: {}", e)))?;

    info!(
        user_id = %claims.sub,
        key_prefix = %prefix,
        "API key created"
    );

    Ok(Json(CreateApiKeyResponse {
        id: row.id,
        key: raw_key, // Full key — only shown once!
        prefix: row.key_prefix,
        name: row.name,
        expires_at: row.expires_at,
        created_at: row.created_at,
    }))
}

/// GET /v1/user/api-keys — List all API keys for the authenticated user.
pub async fn list_api_keys_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ApiKeyInfo>>, AppError> {
    let claims = extract_claims(&headers, &state.config.auth.jwt_secret)?;

    let keys = state
        .db
        .list_api_keys(&claims.sub)
        .map_err(|e| AppError::Internal(format!("Failed to list API keys: {}", e)))?;

    Ok(Json(keys))
}

/// DELETE /v1/user/api-keys/:id — Revoke an API key.
pub async fn revoke_api_key_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key_id): Path<String>,
) -> Result<Json<RevokeApiKeyResponse>, AppError> {
    let claims = extract_claims(&headers, &state.config.auth.jwt_secret)?;

    state
        .db
        .revoke_api_key(&claims.sub, &key_id)
        .map_err(|e| AppError::Validation(format!("{}", e)))?;

    info!(
        user_id = %claims.sub,
        key_id = %key_id,
        "API key revoked"
    );

    Ok(Json(RevokeApiKeyResponse {
        message: "API key revoked".into(),
    }))
}

// ── Helpers ──

/// Hash an API key with SHA-256 for storage.
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}
