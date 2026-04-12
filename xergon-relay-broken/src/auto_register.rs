//! Provider auto-registration service.
//!
//! Automatically registers a provider with the relay and maintains the
//! registration with periodic heartbeats. Handles:
//! - Initial registration on startup
//! - Periodic heartbeat to keep registration alive
//! - Re-registration on health changes
//! - Model list updates when local models change
//! - Exponential backoff on failure
//!
//! Admin endpoints:
//! - GET  /admin/auto-register/status — current auto-registration status
//! - POST /admin/auto-register/trigger — force immediate registration
//! - GET  /admin/auto-register/config — get current config
//! - PATCH /admin/auto-register/config — update config

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for provider auto-registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRegistrationConfig {
    /// Enable/disable auto-registration (default: false).
    pub enabled: bool,

    /// How often to check registration status and send heartbeats (seconds).
    pub check_interval_secs: u64,

    /// The relay URL to register with.
    pub relay_url: String,

    /// Auth token for registration API.
    pub auth_token: String,

    /// Provider information sent during registration.
    #[serde(flatten)]
    pub provider_info: ProviderRegistrationInfo,
}

impl Default for AutoRegistrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval_secs: 30,
            relay_url: String::new(),
            auth_token: String::new(),
            provider_info: ProviderRegistrationInfo::default(),
        }
    }
}

/// Information about this provider for registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistrationInfo {
    /// This provider's endpoint URL.
    pub endpoint: String,

    /// Models served by this provider.
    pub models: Vec<String>,

    /// Provider region (e.g. "us-east", "eu-west").
    pub region: String,

    /// GPU hardware description (e.g. "NVIDIA A100 80GB").
    pub gpu: String,

    /// Maximum concurrent requests this provider can handle.
    pub max_concurrent: u32,

    /// Pricing per model in nanoERG per million tokens.
    pub pricing: HashMap<String, u64>,
}

impl Default for ProviderRegistrationInfo {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            models: Vec::new(),
            region: "unknown".to_string(),
            gpu: String::new(),
            max_concurrent: 10,
            pricing: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Registration status
// ---------------------------------------------------------------------------

/// Runtime status of the auto-registration service.
#[derive(Debug, Clone, Serialize)]
pub struct RegistrationStatus {
    /// Whether auto-registration is enabled.
    pub enabled: bool,

    /// Whether the provider is currently registered with the relay.
    pub registered: bool,

    /// Timestamp of last successful registration/heartbeat (ISO 8601).
    pub last_success_at: Option<String>,

    /// Timestamp of last registration attempt (ISO 8601).
    pub last_attempt_at: Option<String>,

    /// Number of consecutive failures.
    pub consecutive_failures: u64,

    /// Current backoff interval in seconds.
    pub backoff_secs: u64,

    /// Total number of registration attempts.
    pub total_attempts: u64,

    /// Total number of successful registrations.
    pub total_successes: u64,

    /// Last error message (if any).
    pub last_error: Option<String>,

    /// Current models registered with the relay.
    pub registered_models: Vec<String>,
}

// ---------------------------------------------------------------------------
// Auto-registration service
// ---------------------------------------------------------------------------

/// The core auto-registration service.
pub struct AutoRegistrationService {
    /// Runtime configuration (can be updated via PATCH).
    config: RwLock<AutoRegistrationConfig>,

    /// Whether currently registered.
    registered: AtomicBool,

    /// Last successful registration time.
    last_success_at: RwLock<Option<Instant>>,

    /// Last attempt time.
    last_attempt_at: RwLock<Option<Instant>>,

    /// Consecutive failure count.
    consecutive_failures: AtomicU64,

    /// Current backoff multiplier (starts at 1, doubles on failure, resets on success).
    backoff_multiplier: AtomicU64,

    /// Base backoff interval in seconds.
    base_backoff_secs: u64,

    /// Maximum backoff in seconds (cap).
    max_backoff_secs: u64,

    /// Total attempts.
    total_attempts: AtomicU64,

    /// Total successes.
    total_successes: AtomicU64,

    /// Last error message.
    last_error: RwLock<Option<String>>,

    /// Current models registered with relay (for change detection).
    registered_models: RwLock<Vec<String>>,

    /// HTTP client for registration requests.
    http_client: reqwest::Client,

    /// Registration token for the relay (from onboarding).
    registration_token: RwLock<Option<String>>,
}

impl AutoRegistrationService {
    /// Create a new auto-registration service.
    pub fn new(config: AutoRegistrationConfig) -> Self {
        Self {
            config: RwLock::new(config),
            registered: AtomicBool::new(false),
            last_success_at: RwLock::new(None),
            last_attempt_at: RwLock::new(None),
            consecutive_failures: AtomicU64::new(0),
            backoff_multiplier: AtomicU64::new(1),
            base_backoff_secs: 2,
            max_backoff_secs: 300,
            total_attempts: AtomicU64::new(0),
            total_successes: AtomicU64::new(0),
            last_error: RwLock::new(None),
            registered_models: RwLock::new(Vec::new()),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            registration_token: RwLock::new(None),
        }
    }

    /// Whether auto-registration is enabled.
    pub async fn is_enabled(&self) -> bool {
        self.config.read().await.enabled
    }

    /// Get the current configuration.
    pub async fn get_config(&self) -> AutoRegistrationConfig {
        self.config.read().await.clone()
    }

    /// Update the configuration.
    pub async fn update_config(&self, new_config: AutoRegistrationConfig) {
        info!(
            enabled = new_config.enabled,
            relay_url = %new_config.relay_url,
            "Auto-registration config updated"
        );
        *self.config.write().await = new_config;
    }

    /// Get current registration status.
    pub async fn status(&self) -> RegistrationStatus {
        let config = self.config.read().await;
        let backoff = self.current_backoff_secs();
        let last_success = self.last_success_at.read().await.map(|i| {
            Utc::now()
                .checked_sub_signed(chrono::Duration::from_std(i.elapsed()).unwrap_or_default())
                .unwrap_or_else(Utc::now)
                .to_rfc3339()
        });
        let last_attempt = self.last_attempt_at.read().await.map(|i| {
            Utc::now()
                .checked_sub_signed(chrono::Duration::from_std(i.elapsed()).unwrap_or_default())
                .unwrap_or_else(Utc::now)
                .to_rfc3339()
        });
        let models = self.registered_models.read().await.clone();

        RegistrationStatus {
            enabled: config.enabled,
            registered: self.registered.load(Ordering::Relaxed),
            last_success_at: last_success,
            last_attempt_at: last_attempt,
            consecutive_failures: self.consecutive_failures.load(Ordering::Relaxed),
            backoff_secs: backoff,
            total_attempts: self.total_attempts.load(Ordering::Relaxed),
            total_successes: self.total_successes.load(Ordering::Relaxed),
            last_error: self.last_error.read().await.clone(),
            registered_models: models,
        }
    }

    /// Calculate current backoff in seconds.
    fn current_backoff_secs(&self) -> u64 {
        let mult = self.backoff_multiplier.load(Ordering::Relaxed);
        let backoff = self.base_backoff_secs * mult;
        backoff.min(self.max_backoff_secs)
    }

    /// Record a successful registration.
    async fn record_success(&self, models: Vec<String>) {
        self.registered.store(true, Ordering::Relaxed);
        *self.last_success_at.write().await = Some(Instant::now());
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.backoff_multiplier.store(1, Ordering::Relaxed);
        self.total_successes.fetch_add(1, Ordering::Relaxed);
        *self.last_error.write().await = None;
        *self.registered_models.write().await = models;
    }

    /// Record a failed registration attempt.
    async fn record_failure(&self, error_msg: &str) {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        let current_mult = self.backoff_multiplier.load(Ordering::Relaxed);
        self.backoff_multiplier
            .store((current_mult * 2).min(64), Ordering::Relaxed); // cap at 128x
        *self.last_error.write().await = Some(error_msg.to_string());
    }

    /// Perform one registration/heartbeat cycle.
    ///
    /// - If not registered, send a registration request.
    /// - If registered, send a heartbeat.
    /// - If models have changed, send an update.
    async fn tick(&self) {
        let config = self.config.read().await;
        if !config.enabled {
            return;
        }

        let relay_url = config.relay_url.clone();
        let auth_token = config.auth_token.clone();
        let provider_info = config.provider_info.clone();
        let is_registered = self.registered.load(Ordering::Relaxed);

        // Check if models have changed
        let current_models = provider_info.models.clone();
        let registered_models = self.registered_models.read().await.clone();
        let models_changed = current_models != registered_models;

        drop(config);

        *self.last_attempt_at.write().await = Some(Instant::now());
        self.total_attempts.fetch_add(1, Ordering::Relaxed);

        let result = if !is_registered || models_changed {
            // Full registration (or re-registration with updated models)
            debug!(
                was_registered = is_registered,
                models_changed = models_changed,
                "Performing provider registration"
            );
            self.send_registration(&relay_url, &auth_token, &provider_info)
                .await
        } else {
            // Heartbeat
            debug!("Sending registration heartbeat");
            self.send_heartbeat(&relay_url, &auth_token, &provider_info.endpoint)
                .await
        };

        match result {
            Ok(()) => {
                let models = if !is_registered || models_changed {
                    current_models.clone()
                } else {
                    registered_models.clone()
                };
                info!(
                    registered = !is_registered || models_changed,
                    "Provider registration successful"
                );
                self.record_success(models).await;
            }
            Err(e) => {
                let backoff = self.current_backoff_secs();
                warn!(
                    error = %e,
                    consecutive_failures = self.consecutive_failures.load(Ordering::Relaxed),
                    backoff_secs = backoff,
                    "Provider registration failed"
                );
                self.record_failure(&e.to_string()).await;
            }
        }
    }

    /// Send a registration request to the relay.
    async fn send_registration(
        &self,
        relay_url: &str,
        auth_token: &str,
        info: &ProviderRegistrationInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/v1/providers/onboard", relay_url.trim_end_matches('/'));

        let mut body = serde_json::json!({
            "endpoint": info.endpoint,
            "region": info.region,
            "gpu": info.gpu,
            "max_concurrent": info.max_concurrent,
        });

        if !auth_token.is_empty() {
            body["auth_token"] = serde_json::json!(auth_token);
        }

        if !info.models.is_empty() {
            body["models"] = serde_json::json!(info.models);
        }

        if !info.pricing.is_empty() {
            body["pricing"] = serde_json::json!(info.pricing);
        }

        let resp = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            // Try to extract registration token from response
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(token) = json.get("registration_token").and_then(|t| t.as_str()) {
                    *self.registration_token.write().await = Some(token.to_string());
                }
            }
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Registration failed ({}): {}", status, body).into())
        }
    }

    /// Send a heartbeat to keep registration alive.
    async fn send_heartbeat(
        &self,
        relay_url: &str,
        auth_token: &str,
        endpoint: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/v1/providers/onboard/{}",
            relay_url.trim_end_matches('/'),
            endpoint
        );

        let mut req = self.http_client.get(&url);
        if !auth_token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", auth_token));
        }

        let resp = req.send().await?;

        if resp.status().is_success() {
            Ok(())
        } else if resp.status().as_u16() == 404 {
            // Registration expired — need to re-register
            self.registered.store(false, Ordering::Relaxed);
            warn!("Registration expired (404), will re-register");
            Err("Registration expired".into())
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Heartbeat failed ({}): {}", status, body).into())
        }
    }

    /// Force an immediate registration attempt.
    pub async fn force_register(&self) -> Result<(), String> {
        let config = self.config.read().await;
        if !config.enabled {
            return Err("Auto-registration is disabled".to_string());
        }
        let relay_url = config.relay_url.clone();
        let auth_token = config.auth_token.clone();
        let provider_info = config.provider_info.clone();
        drop(config);

        *self.last_attempt_at.write().await = Some(Instant::now());
        self.total_attempts.fetch_add(1, Ordering::Relaxed);

        match self
            .send_registration(&relay_url, &auth_token, &provider_info)
            .await
        {
            Ok(()) => {
                info!("Forced registration successful");
                self.record_success(provider_info.models).await;
                Ok(())
            }
            Err(e) => {
                warn!(error = %e, "Forced registration failed");
                self.record_failure(&e.to_string()).await;
                Err(e.to_string())
            }
        }
    }

    /// Start the background registration loop.
    ///
    /// Returns a `JoinHandle` for the background task.
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // Initial registration attempt
            self.tick().await;

            loop {
                let config = self.config.read().await;
                if !config.enabled {
                    drop(config);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
                let check_interval = config.check_interval_secs;
                drop(config);

                // Apply exponential backoff to check interval
                let backoff = self.current_backoff_secs();
                let sleep_duration = if self.consecutive_failures.load(Ordering::Relaxed) > 0 {
                    // On failure, sleep for backoff duration (capped at check_interval)
                    Duration::from_secs(backoff.min(check_interval))
                } else {
                    Duration::from_secs(check_interval)
                };

                tokio::time::sleep(sleep_duration).await;
                self.tick().await;
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Admin API handlers
// ---------------------------------------------------------------------------

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
    (
        status,
        Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

/// GET /admin/auto-register/status
async fn auto_register_status_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match &state.auto_register {
        Some(service) => {
            let status = service.status().await;
            (StatusCode::OK, Json(status)).into_response()
        }
        None => admin_error("Auto-registration service not initialized", StatusCode::NOT_FOUND),
    }
}

/// POST /admin/auto-register/trigger
async fn auto_register_trigger_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match &state.auto_register {
        Some(service) => match service.force_register().await {
            Ok(()) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "registered",
                    "message": "Provider successfully registered with relay",
                })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "failed",
                    "error": e,
                })),
            )
                .into_response(),
        },
        None => admin_error("Auto-registration service not initialized", StatusCode::NOT_FOUND),
    }
}

/// GET /admin/auto-register/config
async fn auto_register_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match &state.auto_register {
        Some(service) => {
            let config = service.get_config().await;
            (StatusCode::OK, Json(config)).into_response()
        }
        None => admin_error("Auto-registration service not initialized", StatusCode::NOT_FOUND),
    }
}

/// PATCH /admin/auto-register/config
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AutoRegisterConfigUpdate {
    enabled: Option<bool>,
    check_interval_secs: Option<u64>,
    relay_url: Option<String>,
    auth_token: Option<String>,
    endpoint: Option<String>,
    models: Option<Vec<String>>,
    region: Option<String>,
    gpu: Option<String>,
    max_concurrent: Option<u32>,
    pricing: Option<HashMap<String, u64>>,
}

async fn auto_register_patch_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AutoRegisterConfigUpdate>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match &state.auto_register {
        Some(service) => {
            let mut config = service.get_config().await;

            if let Some(enabled) = body.enabled {
                config.enabled = enabled;
            }
            if let Some(interval) = body.check_interval_secs {
                config.check_interval_secs = interval;
            }
            if let Some(url) = body.relay_url {
                config.relay_url = url;
            }
            if let Some(token) = body.auth_token {
                config.auth_token = token;
            }
            if let Some(endpoint) = body.endpoint {
                config.provider_info.endpoint = endpoint;
            }
            if let Some(models) = body.models {
                config.provider_info.models = models;
            }
            if let Some(region) = body.region {
                config.provider_info.region = region;
            }
            if let Some(gpu) = body.gpu {
                config.provider_info.gpu = gpu;
            }
            if let Some(max_concurrent) = body.max_concurrent {
                config.provider_info.max_concurrent = max_concurrent;
            }
            if let Some(pricing) = body.pricing {
                config.provider_info.pricing = pricing;
            }

            service.update_config(config.clone()).await;

            info!(
                enabled = config.enabled,
                relay_url = %config.relay_url,
                "Auto-registration config patched via admin API"
            );

            (StatusCode::OK, Json(config)).into_response()
        }
        None => admin_error("Auto-registration service not initialized", StatusCode::NOT_FOUND),
    }
}

/// Build the auto-registration admin router.
pub fn build_auto_register_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/auto-register/status",
            get(auto_register_status_handler),
        )
        .route(
            "/admin/auto-register/trigger",
            post(auto_register_trigger_handler),
        )
        .route(
            "/admin/auto-register/config",
            get(auto_register_config_handler),
        )
        .route(
            "/admin/auto-register/config",
            patch(auto_register_patch_config_handler),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AutoRegistrationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.check_interval_secs, 30);
        assert_eq!(config.provider_info.region, "unknown");
    }

    #[test]
    fn test_backoff_calculation() {
        let service = AutoRegistrationService::new(AutoRegistrationConfig::default());
        assert_eq!(service.current_backoff_secs(), 2); // base

        service.consecutive_failures.store(1, Ordering::Relaxed);
        service.backoff_multiplier.store(2, Ordering::Relaxed);
        assert_eq!(service.current_backoff_secs(), 4);

        service.backoff_multiplier.store(64, Ordering::Relaxed);
        assert_eq!(service.current_backoff_secs(), 128); // 2 * 64

        service.backoff_multiplier.store(256, Ordering::Relaxed);
        assert_eq!(service.current_backoff_secs(), 300); // capped
    }

    #[tokio::test]
    async fn test_registration_status_when_disabled() {
        let service = Arc::new(AutoRegistrationService::new(AutoRegistrationConfig::default()));
        let status = service.status().await;
        assert!(!status.enabled);
        assert!(!status.registered);
        assert_eq!(status.total_attempts, 0);
    }

    #[tokio::test]
    async fn test_config_update() {
        let service = Arc::new(AutoRegistrationService::new(AutoRegistrationConfig::default()));
        assert!(!service.is_enabled().await);

        let mut new_config = AutoRegistrationConfig::default();
        new_config.enabled = true;
        new_config.relay_url = "http://relay:8080".into();
        service.update_config(new_config).await;

        assert!(service.is_enabled().await);
        let config = service.get_config().await;
        assert_eq!(config.relay_url, "http://relay:8080");
    }
}
