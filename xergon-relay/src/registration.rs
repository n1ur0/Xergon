//! Provider registration and discovery protocol
//!
//! Allows xergon-agent instances to dynamically register with the relay
//! instead of requiring static `known_endpoints` configuration.
//!
//! Protocol:
//!   1. Agent sends POST /v1/providers/register with provider info + auth token
//!   2. Relay validates token, adds provider to registry, returns provider session
//!   3. Agent sends POST /v1/providers/heartbeat periodically (every 30-60s)
//!   4. Relay expires providers that miss heartbeats (TTL: 3x heartbeat interval)
//!   5. Agent sends DELETE /v1/providers/register on graceful shutdown
//!
//! Auth: Shared secret via X-Provider-Token header.
//!   The relay config has `providers.registration_token`.
//!   Each agent config has `relay.token` matching it.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use crate::proxy::AppState;

/// A registered provider with metadata and TTL
#[derive(Debug, Clone, Serialize)]
pub struct RegisteredProvider {
    /// Unique provider ID (e.g., "Xergon_LT")
    pub provider_id: String,
    /// Provider display name
    pub provider_name: String,
    /// Region (e.g., "us-east")
    pub region: String,
    /// The base URL of this provider's xergon-agent API (e.g., "http://1.2.3.4:9099")
    pub endpoint: String,
    /// Ergo address for PoNW identity
    pub ergo_address: String,
    /// Models this provider serves (e.g., ["llama-3.1-70b"])
    pub models: Vec<String>,
    /// Last heartbeat timestamp
    pub last_heartbeat: chrono::DateTime<Utc>,
    /// Registration timestamp
    pub registered_at: chrono::DateTime<Utc>,
    /// TTL in seconds — provider is expired if now > last_heartbeat + ttl
    pub ttl_secs: u64,
}

/// Input for per-model pricing during registration or updates.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelPricingInput {
    /// Price per 1K prompt tokens (None = keep default)
    pub prompt_price_per_1k: Option<f64>,
    /// Price per 1K completion tokens (None = keep default)
    pub completion_price_per_1k: Option<f64>,
}

/// Request body for provider registration
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub provider_id: String,
    pub provider_name: String,
    pub region: String,
    /// The base URL where this agent's API is reachable (e.g., "http://1.2.3.4:9099")
    pub endpoint: String,
    pub ergo_address: String,
    /// Models this provider can serve
    #[serde(default)]
    pub models: Vec<String>,
    /// Requested TTL in seconds for heartbeat expiration (default: 180)
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
    /// Optional per-model pricing map keyed by model_id
    #[serde(default)]
    pub pricing: HashMap<String, ModelPricingInput>,
}

fn default_ttl() -> u64 { 180 }

/// Request body for heartbeat
#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    /// Updated models list (if model changed)
    #[serde(default)]
    pub models: Vec<String>,
}

/// Response for registration
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub status: String,
    pub provider_id: String,
    pub heartbeat_interval_secs: u64,
    pub ttl_secs: u64,
    pub message: String,
}

/// Response for heartbeat
#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub status: String,
    pub next_heartbeat_before: String,
}

/// Response for deregistration
#[derive(Debug, Serialize)]
pub struct DeregisterResponse {
    pub status: String,
    pub message: String,
}

/// Response for listing providers
#[derive(Debug, Serialize)]
pub struct ProviderDirectoryResponse {
    pub providers: Vec<RegisteredProvider>,
    pub total: usize,
    pub healthy: usize,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct RegistrationError {
    pub error: String,
    pub code: u16,
}

/// The provider directory — holds all registered providers
pub struct ProviderDirectory {
    providers: DashMap<String, RegisteredProvider>,
    registration_token: String,
    default_ttl_secs: u64,
}

impl ProviderDirectory {
    pub fn new(registration_token: String) -> Self {
        Self {
            providers: DashMap::new(),
            default_ttl_secs: 180, // 3 minutes default TTL
            registration_token,
        }
    }

    /// Validate the provider token from request headers
    fn validate_token(&self, headers: &HeaderMap) -> Result<(), RegistrationError> {
        let token = headers
            .get("x-provider-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if token.is_empty() {
            return Err(RegistrationError {
                error: "Missing X-Provider-Token header".into(),
                code: 401,
            });
        }

        // Constant-time comparison to prevent timing attacks
        if !constant_time_eq(token.as_bytes(), self.registration_token.as_bytes()) {
            return Err(RegistrationError {
                error: "Invalid provider token".into(),
                code: 403,
            });
        }

        Ok(())
    }

    /// Register a new provider or update an existing one
    pub fn register(
        &self,
        req: RegisterRequest,
    ) -> Result<RegisterResponse, RegistrationError> {
        let ttl = if req.ttl_secs > 0 { req.ttl_secs } else { self.default_ttl_secs };

        // Clamp TTL to reasonable bounds: 30s minimum, 600s maximum
        let ttl = ttl.clamp(30, 600);
        let heartbeat_interval = ttl / 3; // Heartbeat at 1/3 of TTL

        let now = Utc::now();

        let provider = RegisteredProvider {
            provider_id: req.provider_id.clone(),
            provider_name: req.provider_name,
            region: req.region,
            endpoint: req.endpoint.clone(),
            ergo_address: req.ergo_address,
            models: req.models,
            last_heartbeat: now,
            registered_at: now,
            ttl_secs: ttl,
        };

        let is_new = !self.providers.contains_key(&req.provider_id);
        self.providers.insert(req.provider_id.clone(), provider);

        info!(
            provider_id = %req.provider_id,
            endpoint = %req.endpoint,
            is_new,
            ttl_secs = ttl,
            "Provider registered"
        );

        Ok(RegisterResponse {
            status: if is_new { "registered" } else { "updated" }.into(),
            provider_id: req.provider_id,
            heartbeat_interval_secs: heartbeat_interval,
            ttl_secs: ttl,
            message: format!(
                "Registered. Send heartbeat every {}s. TTL: {}s.",
                heartbeat_interval, ttl
            ),
        })
    }

    /// Process a heartbeat from a provider
    pub fn heartbeat(
        &self,
        provider_id: &str,
        req: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, RegistrationError> {
        let mut provider = self.providers.get_mut(provider_id).ok_or_else(|| {
            warn!(provider_id, "Heartbeat from unknown provider");
            RegistrationError {
                error: "Provider not registered".into(),
                code: 404,
            }
        })?;

        provider.last_heartbeat = Utc::now();

        // Update models if provided
        if !req.models.is_empty() {
            provider.models = req.models;
        }

        let next_before = (provider.last_heartbeat
            + chrono::Duration::seconds(provider.ttl_secs as i64))
            .to_rfc3339();

        Ok(HeartbeatResponse {
            status: "ok".into(),
            next_heartbeat_before: next_before,
        })
    }

    /// Deregister a provider
    pub fn deregister(&self, provider_id: &str) -> Result<DeregisterResponse, RegistrationError> {
        let removed = self.providers.remove(provider_id).is_some();

        if removed {
            info!(provider_id, "Provider deregistered");
            Ok(DeregisterResponse {
                status: "deregistered".into(),
                message: format!("Provider {} removed", provider_id),
            })
        } else {
            Err(RegistrationError {
                error: "Provider not found".into(),
                code: 404,
            })
        }
    }

    /// Get all providers, optionally filtering to non-expired ones
    pub fn list_providers(&self, healthy_only: bool) -> ProviderDirectoryResponse {
        let now = Utc::now();
        let providers: Vec<RegisteredProvider> = self
            .providers
            .iter()
            .filter_map(|r| {
                let p = r.value();
                if healthy_only {
                    let expires_at = p.last_heartbeat
                        + chrono::Duration::seconds(p.ttl_secs as i64);
                    if now > expires_at {
                        return None;
                    }
                }
                Some(p.clone())
            })
            .collect();

        let healthy = providers.len();
        let total = self.providers.len();

        ProviderDirectoryResponse {
            providers,
            total,
            healthy,
        }
    }

    /// Get provider endpoint URLs for all non-expired providers.
    /// Used by ProviderRegistry to sync registered providers.
    pub fn active_endpoints(&self) -> Vec<String> {
        let now = Utc::now();
        self.providers
            .iter()
            .filter_map(|r| {
                let p = r.value();
                let expires_at = p.last_heartbeat
                    + chrono::Duration::seconds(p.ttl_secs as i64);
                if now <= expires_at {
                    Some(p.endpoint.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Expire providers that haven't sent a heartbeat within their TTL
    pub fn expire_stale(&self) -> Vec<String> {
        let now = Utc::now();
        let mut expired = Vec::new();

        for entry in self.providers.iter() {
            let p = entry.value();
            let expires_at = p.last_heartbeat
                + chrono::Duration::seconds(p.ttl_secs as i64);
            if now > expires_at {
                expired.push(p.provider_id.clone());
            }
        }

        for id in &expired {
            if let Some((_, p)) = self.providers.remove(id) {
                warn!(
                    provider_id = %p.provider_id,
                    endpoint = %p.endpoint,
                    "Provider expired (missed heartbeat)"
                );
            }
        }

        expired
    }

    /// Spawn a background task that expires stale providers periodically
    pub fn spawn_expiry_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            let interval = Duration::from_secs(60); // Check every 60s
            loop {
                tokio::time::sleep(interval).await;
                let expired = self.expire_stale();
                if !expired.is_empty() {
                    info!(count = expired.len(), "Expired stale providers");
                }
            }
        });
    }
}

/// Build the registration routes
pub fn build_router() -> Router<AppState> {
    Router::new()
        .route("/v1/providers/register", post(register_handler))
        .route("/v1/providers/heartbeat", post(heartbeat_handler))
        .route("/v1/providers/register", delete(deregister_handler))
        .route("/v1/providers/directory", get(directory_handler))
        // Pricing management endpoints
        .route("/v1/providers/pricing", put(update_pricing_handler))
        .route("/v1/providers/pricing/{provider_id}", get(get_pricing_handler))
}

// ── Handlers ──

async fn register_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    let directory = &state.provider_directory;

    // Validate auth token
    if let Err(e) = directory.validate_token(&headers) {
        return (
            StatusCode::from_u16(e.code).unwrap_or(StatusCode::UNAUTHORIZED),
            Json(serde_json::json!({ "error": e.error })),
        );
    }

    // Validate endpoint format
    if req.endpoint.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "endpoint is required" })),
        );
    }

    // Validate provider_id
    if req.provider_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "provider_id is required" })),
        );
    }

    // Persist provider model pricing if provided
    let pricing_count = req.pricing.len();
    if pricing_count > 0 {
        for (model_id, pricing_input) in &req.pricing {
            let prompt_price = pricing_input.prompt_price_per_1k.unwrap_or(0.0);
            let completion_price = pricing_input.completion_price_per_1k.unwrap_or(0.002);
            if let Err(e) = state.db.upsert_model_pricing(
                &req.provider_id,
                model_id,
                prompt_price,
                completion_price,
            ) {
                warn!(
                    provider_id = %req.provider_id,
                    model_id = %model_id,
                    error = %e,
                    "Failed to persist model pricing during registration"
                );
            }
        }
        info!(
            provider_id = %req.provider_id,
            pricing_count,
            "Provider pricing persisted during registration"
        );
    }

    match directory.register(req) {
        Ok(resp) => (StatusCode::OK, Json(serde_json::to_value(resp).unwrap())),
        Err(e) => (
            StatusCode::from_u16(e.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(serde_json::json!({ "error": e.error })),
        ),
    }
}

async fn heartbeat_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let directory = &state.provider_directory;

    // Validate auth token
    if let Err(e) = directory.validate_token(&headers) {
        return (
            StatusCode::from_u16(e.code).unwrap_or(StatusCode::UNAUTHORIZED),
            Json(serde_json::json!({ "error": e.error })),
        );
    }

    // Extract provider_id from header (required)
    let provider_id = headers
        .get("x-provider-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provider_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Missing X-Provider-Id header" })),
        );
    }

    match directory.heartbeat(provider_id, req) {
        Ok(resp) => (StatusCode::OK, Json(serde_json::to_value(resp).unwrap())),
        Err(e) => (
            StatusCode::from_u16(e.code).unwrap_or(StatusCode::NOT_FOUND),
            Json(serde_json::json!({ "error": e.error })),
        ),
    }
}

async fn deregister_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let directory = &state.provider_directory;

    // Validate auth token
    if let Err(e) = directory.validate_token(&headers) {
        return (
            StatusCode::from_u16(e.code).unwrap_or(StatusCode::UNAUTHORIZED),
            Json(serde_json::json!({ "error": e.error })),
        );
    }

    let provider_id = headers
        .get("x-provider-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provider_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Missing X-Provider-Id header" })),
        );
    }

    match directory.deregister(provider_id) {
        Ok(resp) => (StatusCode::OK, Json(serde_json::to_value(resp).unwrap())),
        Err(e) => (
            StatusCode::from_u16(e.code).unwrap_or(StatusCode::NOT_FOUND),
            Json(serde_json::json!({ "error": e.error })),
        ),
    }
}

async fn directory_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let resp = state.provider_directory.list_providers(true);
    Json(serde_json::to_value(resp).unwrap_or_default())
}

// ── Pricing Handlers ──

/// Request body for PUT /v1/providers/pricing
#[derive(Debug, Deserialize)]
pub struct UpdatePricingRequest {
    pub model_id: String,
    pub prompt_price_per_1k: f64,
    pub completion_price_per_1k: f64,
}

/// PUT /v1/providers/pricing — Update model pricing for a provider.
/// Requires X-Provider-Token and X-Provider-Id headers.
async fn update_pricing_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpdatePricingRequest>,
) -> impl IntoResponse {
    let directory = &state.provider_directory;

    // Validate auth token
    if let Err(e) = directory.validate_token(&headers) {
        return (
            StatusCode::from_u16(e.code).unwrap_or(StatusCode::UNAUTHORIZED),
            Json(serde_json::json!({ "error": e.error })),
        );
    }

    // Extract provider_id from header
    let provider_id = headers
        .get("x-provider-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provider_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Missing X-Provider-Id header" })),
        );
    }

    if req.model_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "model_id is required" })),
        );
    }

    match state.db.upsert_model_pricing(
        provider_id,
        &req.model_id,
        req.prompt_price_per_1k,
        req.completion_price_per_1k,
    ) {
        Ok(pricing) => {
            info!(
                provider_id = %provider_id,
                model_id = %req.model_id,
                prompt_price = req.prompt_price_per_1k,
                completion_price = req.completion_price_per_1k,
                "Model pricing updated"
            );
            (
                StatusCode::OK,
                Json(serde_json::to_value(pricing).unwrap_or_default()),
            )
        }
        Err(e) => {
            error!(
                provider_id = %provider_id,
                model_id = %req.model_id,
                error = %e,
                "Failed to update model pricing"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to update pricing: {}", e) })),
            )
        }
    }
}

/// GET /v1/providers/pricing/:provider_id — Public endpoint to get a provider's model pricing.
async fn get_pricing_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_model_pricing(Some(&provider_id), None) {
        Ok(pricing) => {
            if pricing.is_empty() {
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "provider_id": provider_id,
                        "pricing": [],
                        "total": 0,
                    })),
                )
            } else {
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "provider_id": provider_id,
                        "pricing": pricing,
                        "total": pricing.len(),
                    })),
                )
            }
        }
        Err(e) => {
            error!(
                provider_id = %provider_id,
                error = %e,
                "Failed to get model pricing"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to get pricing: {}", e) })),
            )
        }
    }
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
