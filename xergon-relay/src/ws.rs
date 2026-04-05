//! WebSocket endpoint for real-time provider status updates.
//!
//! Exposes GET /ws/status which:
//! - Sends the current provider list on connection
//! - Broadcasts status changes (online/offline/heartbeat) in real-time
//! - Broadcasts provider list snapshots when providers are added/removed

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::SinkExt;
use futures_util::StreamExt;
use serde::Serialize;
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// WebSocket message types
// ---------------------------------------------------------------------------

/// A single provider's status for WebSocket messages.
#[derive(Debug, Clone, Serialize)]
pub struct WsProviderStatus {
    pub provider_id: String,
    pub status: String,
    pub latency_ms: u64,
    pub models: Vec<String>,
    pub region: String,
}

/// Outgoing WebSocket message envelope.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Individual provider status change.
    #[serde(rename = "provider_status")]
    ProviderStatus {
        provider_id: String,
        status: String,
        timestamp: i64,
    },
    /// Full provider list snapshot.
    #[serde(rename = "provider_list")]
    ProviderList {
        providers: Vec<WsProviderStatus>,
        total: usize,
    },
}

// ---------------------------------------------------------------------------
// Broadcast channel for WebSocket clients
// ---------------------------------------------------------------------------

/// A message sent to all WebSocket subscribers.
#[derive(Debug, Clone)]
pub enum WsBroadcast {
    ProviderStatus {
        provider_id: String,
        status: String,
    },
    ProviderListSnapshot,
}

/// Manages WebSocket client subscriptions via a broadcast channel.
/// Clonable and shared via AppState.
#[derive(Clone)]
pub struct WsBroadcaster {
    sender: tokio::sync::broadcast::Sender<WsBroadcast>,
}

impl WsBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender }
    }

    /// Notify all WebSocket clients of a provider status change.
    pub fn notify_provider_status(&self, provider_id: String, status: String) {
        let msg = WsBroadcast::ProviderStatus { provider_id, status };
        match self.sender.send(msg) {
            Ok(n) => debug!(subscribers = n, "WS broadcast: provider_status"),
            Err(_) => {} // no subscribers
        }
    }

    /// Notify all WebSocket clients that the provider list changed.
    pub fn notify_provider_list(&self) {
        match self.sender.send(WsBroadcast::ProviderListSnapshot) {
            Ok(n) => debug!(subscribers = n, "WS broadcast: provider_list"),
            Err(_) => {}
        }
    }

    /// Subscribe to WebSocket broadcasts.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<WsBroadcast> {
        self.sender.subscribe()
    }

    /// Number of active WebSocket subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

// ---------------------------------------------------------------------------
// WebSocket upgrade handler
// ---------------------------------------------------------------------------

/// GET /ws/status — WebSocket upgrade handler.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("WebSocket upgrade request for /ws/status");
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle an upgraded WebSocket connection.
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial provider list snapshot
    let initial_snapshot = build_provider_list(&state);
    let json = serde_json::to_string(&initial_snapshot).unwrap_or_else(|_| "{}".to_string());
    if sender.send(Message::Text(json.into())).await.is_err() {
        debug!("WS client disconnected before initial snapshot");
        return;
    }

    // Subscribe to event broadcaster (for health transitions) and WS broadcaster
    let mut event_rx = state.event_broadcaster.subscribe();
    let mut ws_rx = state.ws_broadcaster.subscribe();

    info!("WebSocket client connected, sent initial provider list");

    loop {
        tokio::select! {
            // Incoming messages from client (ping/pong/close)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Ignore unsolicited pongs
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("WS client sent close or disconnected");
                        break;
                    }
                    Some(Ok(Message::Text(_))) => {
                        // Ignore text messages from client
                    }
                    Some(Ok(Message::Binary(_))) => {
                        // Ignore binary messages from client
                    }
                    Some(Err(e)) => {
                        warn!(error = %e, "WS client error");
                        break;
                    }
                }
            }

            // SSE event -> convert to WS message
            event = event_rx.recv() => {
                match event {
                    Ok(relay_event) => {
                        if let Some(ws_msg) = relay_event_to_ws_message(&relay_event) {
                            let json = serde_json::to_string(&ws_msg).unwrap_or_else(|_| "{}".to_string());
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        debug!(lagged = n, "WS subscriber lagged on events, sending fresh snapshot");
                        let snapshot = build_provider_list(&state);
                        let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string());
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // WS broadcast (provider list snapshot requested)
            broadcast = ws_rx.recv() => {
                match broadcast {
                    Ok(WsBroadcast::ProviderStatus { provider_id, status }) => {
                        let msg = WsMessage::ProviderStatus {
                            provider_id,
                            status,
                            timestamp: chrono::Utc::now().timestamp(),
                        };
                        let json = serde_json::to_string(&msg).unwrap_or_else(|_| "{}".to_string());
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(WsBroadcast::ProviderListSnapshot) => {
                        let snapshot = build_provider_list(&state);
                        let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string());
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let snapshot = build_provider_list(&state);
                        let json = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string());
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a provider list snapshot from the current registry state.
fn build_provider_list(state: &AppState) -> WsMessage {
    let providers: Vec<WsProviderStatus> = state
        .provider_registry
        .providers
        .iter()
        .map(|entry| {
            let p = entry.value();
            let (id, region) = p
                .status
                .as_ref()
                .and_then(|s| s.provider.as_ref())
                .map(|info| (info.id.clone(), info.region.clone()))
                .unwrap_or_else(|| (p.endpoint.clone(), String::new()));

            WsProviderStatus {
                provider_id: id,
                status: if p.is_healthy {
                    "online".to_string()
                } else {
                    "offline".to_string()
                },
                latency_ms: p.latency_ms,
                models: p.served_models.clone(),
                region,
            }
        })
        .collect();

    let total = providers.len();
    WsMessage::ProviderList { providers, total }
}

/// Convert an SSE RelayEvent into an optional WsMessage.
fn relay_event_to_ws_message(event: &crate::events::RelayEvent) -> Option<WsMessage> {
    match event {
        crate::events::RelayEvent::ProviderOnline { provider_id, .. } => Some(
            WsMessage::ProviderStatus {
                provider_id: provider_id.clone(),
                status: "online".to_string(),
                timestamp: chrono::Utc::now().timestamp(),
            },
        ),
        crate::events::RelayEvent::ProviderOffline { provider_id, .. } => Some(
            WsMessage::ProviderStatus {
                provider_id: provider_id.clone(),
                status: "offline".to_string(),
                timestamp: chrono::Utc::now().timestamp(),
            },
        ),
        // For other event types, we let the caller send a full snapshot instead
        _ => None,
    }
}
