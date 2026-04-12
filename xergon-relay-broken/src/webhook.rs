//! Webhook Event Delivery for the Xergon relay.
//!
//! Manages webhook registration, event delivery with HMAC-SHA256 signatures,
//! exponential backoff retries, and dead-letter storage for failed deliveries.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Configuration for a registered webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub secret: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A single webhook delivery attempt record.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub webhook_id: String,
    pub event: String,
    pub payload: serde_json::Value,
    pub status: DeliveryStatus,
    pub attempts: u32,
    pub last_response: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Delivery status for a webhook event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Pending,
    Success,
    Failed,
    Retrying,
}

/// A dead-letter entry for a delivery that failed all retry attempts.
#[derive(Debug, Clone, Serialize)]
pub struct DeadLetterEntry {
    pub delivery_id: String,
    pub webhook_id: String,
    pub event: String,
    pub payload: serde_json::Value,
    pub attempts: u32,
    pub last_error: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub dead_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_DELIVERIES: usize = 1000;
const MAX_DEAD_LETTERS: usize = 500;
const MAX_RETRY_ATTEMPTS: u32 = 5;

// ---------------------------------------------------------------------------
// Webhook manager
// ---------------------------------------------------------------------------

/// Manages webhook registrations and event delivery.
///
/// Internally uses `Arc<StdMutex<...>>` so spawned async delivery tasks
/// can update the delivery log without holding a reference to the manager.
#[derive(Clone)]
pub struct WebhookManager {
    webhooks: Arc<DashMap<String, WebhookConfig>>,
    deliveries: Arc<StdMutex<VecDeque<WebhookDelivery>>>,
    dead_letters: Arc<StdMutex<VecDeque<DeadLetterEntry>>>,
    http_client: reqwest::Client,
}

impl WebhookManager {
    pub fn new() -> Self {
        Self {
            webhooks: Arc::new(DashMap::new()),
            deliveries: Arc::new(StdMutex::new(VecDeque::with_capacity(MAX_DELIVERIES))),
            dead_letters: Arc::new(StdMutex::new(VecDeque::with_capacity(MAX_DEAD_LETTERS))),
            http_client: reqwest::Client::new(),
        }
    }

    /// Register a new webhook.
    pub fn register(&self, url: String, events: Vec<String>, secret: String) -> WebhookConfig {
        let id = uuid::Uuid::new_v4().to_string();
        let config = WebhookConfig {
            id: id.clone(),
            url,
            events,
            secret,
            active: true,
            created_at: chrono::Utc::now(),
        };
        self.webhooks.insert(id.clone(), config.clone());
        info!(webhook_id = %id, "Webhook registered");
        config
    }

    /// List all registered webhooks.
    pub fn list_webhooks(&self) -> Vec<WebhookConfig> {
        self.webhooks.iter().map(|r| r.value().clone()).collect()
    }

    /// Remove a webhook by ID.
    pub fn remove(&self, id: &str) -> bool {
        let removed = self.webhooks.remove(id).is_some();
        if removed {
            info!(webhook_id = %id, "Webhook removed");
        }
        removed
    }

    /// Get a webhook by ID.
    pub fn get(&self, id: &str) -> Option<WebhookConfig> {
        self.webhooks.get(id).map(|r| r.value().clone())
    }

    /// Emit an event to all matching active webhooks.
    ///
    /// Spawns a delivery task per webhook with exponential backoff retries.
    pub fn emit(&self, event_type: &str, payload: serde_json::Value) {
        let matching: Vec<WebhookConfig> = self
            .webhooks
            .iter()
            .filter(|r| {
                let wh = r.value();
                wh.active && (wh.events.is_empty() || wh.events.iter().any(|e| e == event_type))
            })
            .map(|r| r.value().clone())
            .collect();

        if matching.is_empty() {
            return;
        }

        debug!(
            event = event_type,
            webhooks = matching.len(),
            "Dispatching webhook event"
        );

        for wh in matching {
            let delivery_id = uuid::Uuid::new_v4().to_string();
            let delivery = WebhookDelivery {
                id: delivery_id.clone(),
                webhook_id: wh.id.clone(),
                event: event_type.to_string(),
                payload: payload.clone(),
                status: DeliveryStatus::Pending,
                attempts: 0,
                last_response: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            // Store delivery record
            {
                let mut deliveries = self.deliveries.lock().unwrap();
                deliveries.push_back(delivery.clone());
                while deliveries.len() > MAX_DELIVERIES {
                    deliveries.pop_front();
                }
            }

            // Clone Arc handles for the spawned task
            let client = self.http_client.clone();
            let dl = delivery;
            let secret = wh.secret.clone();
            let webhook_id = wh.id.clone();
            let webhook_url = wh.url.clone();
            let deliveries_arc = self.deliveries.clone();
            let dead_letters_arc = self.dead_letters.clone();

            tokio::spawn(async move {
                let mut current_delivery = dl;
                let mut last_error = String::new();

                for attempt in 0..MAX_RETRY_ATTEMPTS {
                    current_delivery.attempts = attempt + 1;

                    if attempt > 0 {
                        // Exponential backoff: 1s, 2s, 4s, 8s, 16s
                        let delay_secs = 1u64 << attempt;
                        debug!(
                            delivery_id = %current_delivery.id,
                            attempt = attempt + 1,
                            delay_secs,
                            "Retrying webhook delivery"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                    }

                    // Compute HMAC-SHA256 signature
                    let payload_str =
                        serde_json::to_string(&current_delivery.payload).unwrap_or_default();
                    let signature = compute_hmac_sha256(&secret, &payload_str);

                    match client
                        .post(&webhook_url)
                        .header("Content-Type", "application/json")
                        .header("X-Xergon-Signature", format!("sha256={}", signature))
                        .header("X-Xergon-Event", &current_delivery.event)
                        .header("X-Xergon-Delivery", &current_delivery.id)
                        .body(payload_str)
                        .timeout(std::time::Duration::from_secs(30))
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            current_delivery.status = DeliveryStatus::Success;
                            current_delivery.last_response =
                                Some(format!("HTTP {}", resp.status()));
                            current_delivery.updated_at = chrono::Utc::now();
                            info!(
                                delivery_id = %current_delivery.id,
                                webhook_id = %webhook_id,
                                attempts = current_delivery.attempts,
                                "Webhook delivery succeeded"
                            );
                            break;
                        }
                        Ok(resp) => {
                            last_error = format!("HTTP {}", resp.status());
                            current_delivery.last_response = Some(last_error.clone());
                            current_delivery.status = if attempt + 1 < MAX_RETRY_ATTEMPTS {
                                DeliveryStatus::Retrying
                            } else {
                                DeliveryStatus::Failed
                            };
                            current_delivery.updated_at = chrono::Utc::now();
                            warn!(
                                delivery_id = %current_delivery.id,
                                webhook_id = %webhook_id,
                                status = %last_error,
                                attempt = attempt + 1,
                                "Webhook delivery failed"
                            );
                        }
                        Err(e) => {
                            last_error = e.to_string();
                            current_delivery.last_response = Some(last_error.clone());
                            current_delivery.status = if attempt + 1 < MAX_RETRY_ATTEMPTS {
                                DeliveryStatus::Retrying
                            } else {
                                DeliveryStatus::Failed
                            };
                            current_delivery.updated_at = chrono::Utc::now();
                            warn!(
                                delivery_id = %current_delivery.id,
                                webhook_id = %webhook_id,
                                error = %e,
                                attempt = attempt + 1,
                                "Webhook delivery error"
                            );
                        }
                    }
                }

                // Update delivery record
                {
                    let mut deliveries = deliveries_arc.lock().unwrap();
                    if let Some(existing) = deliveries
                        .iter_mut()
                        .find(|d| d.id == current_delivery.id)
                    {
                        *existing = current_delivery.clone();
                    }
                }

                // Move to dead letter if all retries exhausted
                if current_delivery.status == DeliveryStatus::Failed {
                    warn!(
                        delivery_id = %current_delivery.id,
                        webhook_id = %webhook_id,
                        "Webhook delivery exhausted retries, moving to dead letter"
                    );
                    let entry = DeadLetterEntry {
                        delivery_id: current_delivery.id.clone(),
                        webhook_id: webhook_id.clone(),
                        event: current_delivery.event.clone(),
                        payload: current_delivery.payload.clone(),
                        attempts: current_delivery.attempts,
                        last_error: last_error,
                        created_at: current_delivery.created_at,
                        dead_at: chrono::Utc::now(),
                    };
                    let mut dl = dead_letters_arc.lock().unwrap();
                    dl.push_back(entry);
                    while dl.len() > MAX_DEAD_LETTERS {
                        dl.pop_front();
                    }
                }
            });
        }
    }

    /// Deliver a test event to a specific webhook (synchronous, waits for result).
    pub async fn send_test(&self, webhook_id: &str) -> Result<WebhookDelivery, String> {
        let wh = self
            .get(webhook_id)
            .ok_or_else(|| "Webhook not found".to_string())?;

        let delivery_id = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "test": true,
            "message": "Test webhook event from Xergon Relay",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let payload_str = serde_json::to_string(&payload).unwrap_or_default();
        let signature = compute_hmac_sha256(&wh.secret, &payload_str);

        let resp = self
            .http_client
            .post(&wh.url)
            .header("Content-Type", "application/json")
            .header("X-Xergon-Signature", format!("sha256={}", signature))
            .header("X-Xergon-Event", "test")
            .header("X-Xergon-Delivery", &delivery_id)
            .body(payload_str)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status_str = format!("HTTP {}", resp.status());
        let status = if resp.status().is_success() {
            DeliveryStatus::Success
        } else {
            DeliveryStatus::Failed
        };

        let now = chrono::Utc::now();
        let delivery = WebhookDelivery {
            id: delivery_id,
            webhook_id: webhook_id.to_string(),
            event: "test".to_string(),
            payload,
            status,
            attempts: 1,
            last_response: Some(status_str),
            created_at: now,
            updated_at: now,
        };

        {
            let mut deliveries = self.deliveries.lock().unwrap();
            deliveries.push_back(delivery.clone());
            while deliveries.len() > MAX_DELIVERIES {
                deliveries.pop_front();
            }
        }

        Ok(delivery)
    }

    /// List recent deliveries (most recent first).
    pub fn list_deliveries(&self, limit: usize) -> Vec<WebhookDelivery> {
        let deliveries = self.deliveries.lock().unwrap();
        deliveries.iter().rev().take(limit).cloned().collect()
    }

    /// List dead letter entries (most recent first).
    pub fn list_dead_letters(&self, limit: usize) -> Vec<DeadLetterEntry> {
        let dl = self.dead_letters.lock().unwrap();
        dl.iter().rev().take(limit).cloned().collect()
    }

    /// Prune old dead letter entries beyond capacity.
    pub fn prune_dead_letters(&self) {
        let mut dl = self.dead_letters.lock().unwrap();
        while dl.len() > MAX_DEAD_LETTERS {
            dl.pop_front();
        }
    }
}

// ---------------------------------------------------------------------------
// HMAC-SHA256 signature computation
// ---------------------------------------------------------------------------

/// Compute HMAC-SHA256 signature for webhook payload verification.
pub fn compute_hmac_sha256(secret: &str, payload: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key is valid");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

// ---------------------------------------------------------------------------
// Axum admin handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use axum::routing::{delete, get, post};

use crate::proxy::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterWebhookRequest {
    pub url: String,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct ListDeliveriesQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

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

/// POST /admin/webhooks -- register a webhook
pub async fn register_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterWebhookRequest>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }
    if body.url.is_empty() {
        return admin_error("url is required", StatusCode::BAD_REQUEST);
    }

    let secret = if body.secret.is_empty() {
        hex::encode(rand::random::<[u8; 32]>())
    } else {
        body.secret.clone()
    };

    let wh = state.webhook_manager.register(body.url, body.events, secret);
    admin_ok(serde_json::json!({
        "id": wh.id,
        "url": wh.url,
        "events": wh.events,
        "active": wh.active,
        "created_at": wh.created_at,
    }))
}

/// GET /admin/webhooks -- list all webhooks
pub async fn list_webhooks_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let webhooks = state.webhook_manager.list_webhooks();
    let sanitized: Vec<serde_json::Value> = webhooks
        .iter()
        .map(|wh| {
            serde_json::json!({
                "id": wh.id,
                "url": wh.url,
                "events": wh.events,
                "active": wh.active,
                "created_at": wh.created_at,
                "secret": "***",
            })
        })
        .collect();

    admin_ok(serde_json::json!({
        "webhooks": sanitized,
        "total": sanitized.len(),
    }))
}

/// DELETE /admin/webhooks/:id -- remove a webhook
pub async fn delete_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    if state.webhook_manager.remove(&id) {
        admin_ok(serde_json::json!({
            "id": id,
            "status": "removed",
        }))
    } else {
        admin_error("Webhook not found", StatusCode::NOT_FOUND)
    }
}

/// GET /admin/webhooks/deliveries -- list recent deliveries
pub async fn list_deliveries_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ListDeliveriesQuery>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let deliveries = state.webhook_manager.list_deliveries(params.limit);
    admin_ok(serde_json::json!({
        "deliveries": deliveries,
        "total": deliveries.len(),
    }))
}

/// POST /admin/webhooks/:id/test -- send a test event
pub async fn test_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    match state.webhook_manager.send_test(&id).await {
        Ok(delivery) => admin_ok(serde_json::json!({
            "id": delivery.id,
            "webhook_id": delivery.webhook_id,
            "event": delivery.event,
            "status": delivery.status,
            "attempts": delivery.attempts,
            "last_response": delivery.last_response,
        })),
        Err(e) => admin_error(&e, StatusCode::BAD_REQUEST),
    }
}

/// Build the webhook admin router.
pub fn build_webhook_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/webhooks", post(register_webhook_handler))
        .route("/admin/webhooks", get(list_webhooks_handler))
        .route("/admin/webhooks/{id}", delete(delete_webhook_handler))
        .route("/admin/webhooks/deliveries", get(list_deliveries_handler))
        .route("/admin/webhooks/{id}/test", post(test_webhook_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manager() {
        let mgr = WebhookManager::new();
        assert!(mgr.list_webhooks().is_empty());
        assert!(mgr.list_deliveries(10).is_empty());
        assert!(mgr.list_dead_letters(10).is_empty());
    }

    #[test]
    fn test_register_webhook() {
        let mgr = WebhookManager::new();
        let wh = mgr.register(
            "https://example.com/webhook".to_string(),
            vec!["model.request".to_string()],
            "secret123".to_string(),
        );
        assert!(!wh.id.is_empty());
        assert_eq!(wh.url, "https://example.com/webhook");
        assert!(wh.active);
        assert_eq!(wh.events.len(), 1);
    }

    #[test]
    fn test_list_webhooks() {
        let mgr = WebhookManager::new();
        assert!(mgr.list_webhooks().is_empty());
        mgr.register("https://a.com".to_string(), vec![], "s".to_string());
        mgr.register("https://b.com".to_string(), vec![], "s".to_string());
        assert_eq!(mgr.list_webhooks().len(), 2);
    }

    #[test]
    fn test_remove_webhook() {
        let mgr = WebhookManager::new();
        let wh = mgr.register("https://example.com".to_string(), vec![], "s".to_string());
        assert!(mgr.remove(&wh.id));
        assert!(mgr.list_webhooks().is_empty());
    }

    #[test]
    fn test_remove_nonexistent_webhook() {
        let mgr = WebhookManager::new();
        assert!(!mgr.remove("nonexistent"));
    }

    #[test]
    fn test_get_webhook() {
        let mgr = WebhookManager::new();
        assert!(mgr.get("nonexistent").is_none());
        let wh = mgr.register("https://example.com".to_string(), vec![], "secret".to_string());
        let fetched = mgr.get(&wh.id).unwrap();
        assert_eq!(fetched.id, wh.id);
        assert_eq!(fetched.url, wh.url);
    }

    #[test]
    fn test_list_deliveries_empty() {
        let mgr = WebhookManager::new();
        let deliveries = mgr.list_deliveries(100);
        assert!(deliveries.is_empty());
    }

    #[test]
    fn test_list_dead_letters_empty() {
        let mgr = WebhookManager::new();
        let dl = mgr.list_dead_letters(100);
        assert!(dl.is_empty());
    }

    #[test]
    fn test_compute_hmac_sha256_deterministic() {
        let sig1 = compute_hmac_sha256("secret", "{\"test\":true}");
        let sig2 = compute_hmac_sha256("secret", "{\"test\":true}");
        assert_eq!(sig1, sig2);
        assert!(!sig1.is_empty());
    }

    #[test]
    fn test_compute_hmac_sha256_different_secrets() {
        let sig1 = compute_hmac_sha256("secret1", "payload");
        let sig2 = compute_hmac_sha256("secret2", "payload");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_compute_hmac_sha256_different_payloads() {
        let sig1 = compute_hmac_sha256("secret", "payload1");
        let sig2 = compute_hmac_sha256("secret", "payload2");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_delivery_status_equality() {
        assert_eq!(DeliveryStatus::Pending, DeliveryStatus::Pending);
        assert_eq!(DeliveryStatus::Success, DeliveryStatus::Success);
        assert_eq!(DeliveryStatus::Failed, DeliveryStatus::Failed);
        assert_eq!(DeliveryStatus::Retrying, DeliveryStatus::Retrying);
        assert_ne!(DeliveryStatus::Pending, DeliveryStatus::Success);
    }

    #[test]
    fn test_register_webhook_with_empty_events() {
        let mgr = WebhookManager::new();
        let wh = mgr.register("https://example.com".to_string(), vec![], "secret".to_string());
        assert!(wh.events.is_empty());
        assert!(wh.active);
    }

    #[test]
    fn test_prune_dead_letters_noop() {
        let mgr = WebhookManager::new();
        // Should not panic on empty state
        mgr.prune_dead_letters();
        assert!(mgr.list_dead_letters(10).is_empty());
    }

    #[test]
    fn test_webhook_config_fields() {
        let mgr = WebhookManager::new();
        let wh = mgr.register(
            "https://example.com".to_string(),
            vec!["event.a".to_string(), "event.b".to_string()],
            "mysecret".to_string(),
        );
        assert_eq!(wh.events, vec!["event.a", "event.b"]);
        assert_eq!(wh.secret, "mysecret");
        assert_eq!(wh.url, "https://example.com");
    }
}
