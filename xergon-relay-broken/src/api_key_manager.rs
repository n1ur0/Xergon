//! API Key Management for the Xergon relay.
//!
//! Provides CRUD operations for API keys with scopes, tiers, expiry,
//! rotation, and usage tracking. Keys are formatted as `xergon-{32 hex chars}`.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info};

use crate::rate_limit_tiers::RateLimitTier;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A managed API key.
#[derive(Debug, Clone, Serialize)]
pub struct ApiKey {
    pub id: String,
    pub key: String,
    pub name: String,
    pub prefix: String,
    pub tier: RateLimitTier,
    pub scopes: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub active: bool,
    pub metadata: HashMap<String, String>,
    pub request_count: u64,
}

/// Request body for creating a new API key.
#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    #[serde(default)]
    pub tier: Option<RateLimitTier>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Request body for updating an API key.
#[derive(Debug, Deserialize)]
pub struct UpdateKeyRequest {
    pub name: Option<String>,
    pub tier: Option<RateLimitTier>,
    pub scopes: Option<Vec<String>>,
    pub expires_at: Option<Option<chrono::DateTime<chrono::Utc>>>,
    pub active: Option<bool>,
    pub metadata: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// API key manager
// ---------------------------------------------------------------------------

/// Manages API key lifecycle: creation, validation, rotation, expiry.
#[derive(Clone)]
pub struct ApiKeyManager {
    /// Maps full key string -> ApiKey for fast authentication lookups.
    keys_by_value: Arc<DashMap<String, ApiKey>>,
    /// Maps key ID -> ApiKey for admin CRUD.
    keys_by_id: Arc<DashMap<String, ApiKey>>,
    /// Maps key prefix -> ApiKey for listing by prefix.
    keys_by_prefix: Arc<DashMap<String, ApiKey>>,
}

/// Valid API key scopes.
pub const VALID_SCOPES: &[&str] = &[
    "chat",
    "embeddings",
    "images",
    "audio",
    "files",
    "admin",
];

impl ApiKeyManager {
    pub fn new() -> Self {
        Self {
            keys_by_value: Arc::new(DashMap::new()),
            keys_by_id: Arc::new(DashMap::new()),
            keys_by_prefix: Arc::new(DashMap::new()),
        }
    }

    /// Generate a new API key string: `xergon-{32 random hex chars}`.
    fn generate_key() -> String {
        let random_bytes: [u8; 16] = rand::random();
        format!("xergon-{}", hex::encode(random_bytes))
    }

    /// Extract the prefix from a key string (first 12 chars of the key).
    fn extract_prefix(key: &str) -> String {
        key.chars().take(12).collect()
    }

    /// Create a new API key.
    pub fn create(&self, req: CreateKeyRequest) -> Result<ApiKey, String> {
        // Validate scopes
        for scope in &req.scopes {
            if !VALID_SCOPES.contains(&scope.as_str()) {
                return Err(format!("Invalid scope: {}", scope));
            }
        }

        let key_str = Self::generate_key();
        let prefix = Self::extract_prefix(&key_str);
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let api_key = ApiKey {
            id: id.clone(),
            key: key_str.clone(),
            name: req.name,
            prefix: prefix.clone(),
            tier: req.tier.unwrap_or(RateLimitTier::Free),
            scopes: req.scopes,
            created_at: now,
            last_used: None,
            expires_at: req.expires_at,
            active: true,
            metadata: req.metadata,
            request_count: 0,
        };

        // Store in all indexes
        self.keys_by_value.insert(key_str.clone(), api_key.clone());
        self.keys_by_id.insert(id.clone(), api_key.clone());
        self.keys_by_prefix.insert(prefix, api_key.clone());

        // Also set the tier in the tier manager for rate limiting
        info!(
            key_id = %id,
            key_prefix = %api_key.prefix,
            tier = ?api_key.tier,
            scopes = ?api_key.scopes,
            "API key created"
        );

        Ok(api_key)
    }

    /// Validate an API key and return its info if valid.
    pub fn validate(&self, key: &str) -> Option<ApiKey> {
        let entry = self.keys_by_value.get(key)?;
        let api_key = entry.value().clone();

        // Check if active
        if !api_key.active {
            return None;
        }

        // Check expiry
        if let Some(expires_at) = api_key.expires_at {
            if chrono::Utc::now() > expires_at {
                return None;
            }
        }

        // Update last_used and request_count
        if let Some(mut entry) = self.keys_by_value.get_mut(key) {
            entry.last_used = Some(chrono::Utc::now());
            entry.request_count += 1;
        }
        if let Some(mut entry) = self.keys_by_id.get_mut(&api_key.id) {
            entry.last_used = Some(chrono::Utc::now());
            entry.request_count += 1;
        }

        Some(api_key)
    }

    /// Check if a key has a specific scope.
    pub fn has_scope(key: &ApiKey, scope: &str) -> bool {
        key.scopes.is_empty() || key.scopes.iter().any(|s| s == scope)
    }

    /// Get an API key by ID (admin).
    pub fn get_by_id(&self, id: &str) -> Option<ApiKey> {
        self.keys_by_id.get(id).map(|r| {
            let mut k = r.value().clone();
            k.key = "***".to_string(); // Never expose full key
            k
        })
    }

    /// List all API keys (sanitized, no full key values).
    pub fn list_keys(&self) -> Vec<ApiKey> {
        self.keys_by_id
            .iter()
            .map(|r| {
                let mut k = r.value().clone();
                k.key = "***".to_string();
                k
            })
            .collect()
    }

    /// Update an API key by ID.
    pub fn update(&self, id: &str, req: UpdateKeyRequest) -> Result<ApiKey, String> {
        let mut entry = self
            .keys_by_id
            .get_mut(id)
            .ok_or_else(|| "Key not found".to_string())?;

        // Validate new scopes if provided
        if let Some(ref scopes) = req.scopes {
            for scope in scopes {
                if !VALID_SCOPES.contains(&scope.as_str()) {
                    return Err(format!("Invalid scope: {}", scope));
                }
            }
        }

        let old_tier = entry.tier;

        if let Some(ref name) = req.name {
            entry.name = name.clone();
        }
        if let Some(tier) = req.tier {
            entry.tier = tier;
        }
        if let Some(ref scopes) = req.scopes {
            entry.scopes = scopes.clone();
        }
        if let Some(expires_at) = req.expires_at {
            entry.expires_at = expires_at;
        }
        if let Some(active) = req.active {
            entry.active = active;
        }
        if let Some(ref metadata) = req.metadata {
            entry.metadata = metadata.clone();
        }

        info!(
            key_id = %id,
            old_tier = ?old_tier,
            new_tier = ?entry.tier,
            "API key updated"
        );

        let mut result = entry.clone();
        result.key = "***".to_string();
        Ok(result)
    }

    /// Delete an API key by ID.
    pub fn delete(&self, id: &str) -> bool {
        if let Some((_, key)) = self.keys_by_id.remove(id) {
            // Clean up other indexes
            self.keys_by_value.remove(&key.key);
            self.keys_by_prefix.remove(&key.prefix);
            info!(key_id = %id, "API key deleted");
            true
        } else {
            false
        }
    }

    /// Rotate an API key: generate new key, invalidate old.
    pub fn rotate(&self, id: &str) -> Result<ApiKey, String> {
        // Get the existing key
        let old_key = self
            .keys_by_id
            .get(id)
            .ok_or_else(|| "Key not found".to_string())?
            .clone();

        // Remove old key from value and prefix indexes
        self.keys_by_value.remove(&old_key.key);
        self.keys_by_prefix.remove(&old_key.prefix);

        // Generate new key
        let new_key_str = Self::generate_key();
        let new_prefix = Self::extract_prefix(&new_key_str);

        // Update the stored key
        let mut entry = self.keys_by_id.get_mut(id).unwrap();
        entry.key = new_key_str.clone();
        entry.prefix = new_prefix.clone();

        // Re-insert into value and prefix indexes
        self.keys_by_value.insert(new_key_str.clone(), entry.clone());
        self.keys_by_prefix.insert(new_prefix.clone(), entry.clone());

        info!(
            key_id = %id,
            old_prefix = %old_key.prefix,
            new_prefix = %new_prefix,
            "API key rotated"
        );

        let mut result = entry.clone();
        result.key = "***".to_string();
        Ok(result)
    }

    /// Get the tier for a given key value (for rate limiting integration).
    pub fn get_tier_for_key(&self, key: &str) -> Option<RateLimitTier> {
        self.keys_by_value.get(key).map(|r| r.value().tier)
    }

    /// Number of managed keys.
    pub fn len(&self) -> usize {
        self.keys_by_id.len()
    }
}

// ---------------------------------------------------------------------------
// Axum admin handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use axum::routing::{delete, get, patch, post};

use crate::proxy::AppState;

fn verify_admin_key(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected_key = &state.config.admin.api_key;
    let provided = headers
        .get("x-admin-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided.is_empty() || provided != expected_key {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn admin_error(msg: &str, status: StatusCode) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn admin_ok(value: serde_json::Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

/// POST /admin/keys -- create a new API key
pub async fn create_key_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateKeyRequest>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match state.api_key_manager.create(body) {
        Ok(key) => {
            // Log to audit
            state.audit_logger.log(
                "api_key.created",
                "admin",
                "api_key",
                &key.id,
                serde_json::json!({
                    "name": key.name,
                    "tier": format!("{}", key.tier),
                    "scopes": key.scopes,
                }),
                headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).map(String::from),
                headers.get("user-agent").and_then(|v| v.to_str().ok()).map(String::from),
            );

            // Set tier in tier manager
            state.tier_manager.set_tier(&key.key, key.tier);

            admin_ok(serde_json::json!({
                "id": key.id,
                "key": key.key,
                "name": key.name,
                "prefix": key.prefix,
                "tier": format!("{}", key.tier),
                "scopes": key.scopes,
                "created_at": key.created_at,
                "expires_at": key.expires_at,
            }))
        }
        Err(e) => admin_error(&e, StatusCode::BAD_REQUEST),
    }
}

/// GET /admin/keys -- list all API keys
pub async fn list_keys_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let keys = state.api_key_manager.list_keys();
    admin_ok(serde_json::json!({
        "keys": keys,
        "total": keys.len(),
    }))
}

/// GET /admin/keys/:id -- get key details
pub async fn get_key_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match state.api_key_manager.get_by_id(&id) {
        Some(key) => admin_ok(serde_json::json!(key)),
        None => admin_error("API key not found", StatusCode::NOT_FOUND),
    }
}

/// PATCH /admin/keys/:id -- update a key
pub async fn update_key_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateKeyRequest>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match state.api_key_manager.update(&id, body) {
        Ok(key) => {
            state.audit_logger.log(
                "api_key.updated",
                "admin",
                "api_key",
                &id,
                serde_json::json!({ "tier": format!("{}", key.tier) }),
                headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).map(String::from),
                headers.get("user-agent").and_then(|v| v.to_str().ok()).map(String::from),
            );

            admin_ok(serde_json::json!(key))
        }
        Err(e) => admin_error(&e, StatusCode::BAD_REQUEST),
    }
}

/// DELETE /admin/keys/:id -- delete a key
pub async fn delete_key_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    if state.api_key_manager.delete(&id) {
        state.audit_logger.log(
            "api_key.deleted",
            "admin",
            "api_key",
            &id,
            serde_json::json!({}),
            headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).map(String::from),
            headers.get("user-agent").and_then(|v| v.to_str().ok()).map(String::from),
        );

        admin_ok(serde_json::json!({
            "id": id,
            "status": "deleted",
        }))
    } else {
        admin_error("API key not found", StatusCode::NOT_FOUND)
    }
}

/// POST /admin/keys/:id/rotate -- rotate a key
pub async fn rotate_key_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match state.api_key_manager.rotate(&id) {
        Ok(key) => {
            state.audit_logger.log(
                "api_key.rotated",
                "admin",
                "api_key",
                &id,
                serde_json::json!({ "prefix": key.prefix }),
                headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).map(String::from),
                headers.get("user-agent").and_then(|v| v.to_str().ok()).map(String::from),
            );

            admin_ok(serde_json::json!(key))
        }
        Err(e) => admin_error(&e, StatusCode::BAD_REQUEST),
    }
}

/// Build the API key admin router.
pub fn build_api_key_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/keys", post(create_key_handler))
        .route("/admin/keys", get(list_keys_handler))
        .route("/admin/keys/{id}", get(get_key_handler))
        .route("/admin/keys/{id}", patch(update_key_handler))
        .route("/admin/keys/{id}", delete(delete_key_handler))
        .route("/admin/keys/{id}/rotate", post(rotate_key_handler))
        .with_state(state)
}
