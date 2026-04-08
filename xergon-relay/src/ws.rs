//! WebSocket endpoints for xergon-relay.
//!
//! Exposes two WebSocket endpoints:
//!
//! 1. GET /ws/status — real-time provider status updates
//!    - Sends the current provider list on connection
//!    - Broadcasts status changes (online/offline/heartbeat) in real-time
//!    - Broadcasts provider list snapshots when providers are added/removed
//!
//! 2. GET /v1/chat/ws — WebSocket chat transport
//!    - Client sends: { model, messages, stream }
//!    - Server streams back SSE-formatted chunks over WebSocket text frames
//!    - Close frame on completion
//!    - Enables persistent connections instead of per-request HTTP

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::SinkExt;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;
use tracing::{debug, error, info, warn};

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

// ===========================================================================
// WebSocket Chat Transport — GET /v1/chat/ws
// ===========================================================================

/// Tracks active WebSocket chat connections.
pub struct WsChatConnectionCounter(Arc<AtomicUsize>);

impl WsChatConnectionCounter {
    pub fn new() -> Self {
        Self(Arc::new(AtomicUsize::new(0)))
    }

    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    fn increment(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    fn decrement(&self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Clone for WsChatConnectionCounter {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Incoming chat request over WebSocket.
#[derive(Debug, Deserialize)]
pub struct WsChatRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
}

/// GET /v1/chat/ws — WebSocket upgrade handler for chat transport.
///
/// The client sends JSON text frames with chat requests.
/// The server responds with SSE-formatted text frames and closes on completion.
pub async fn ws_chat_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let max = state.config.ws_chat.max_connections;
    let current = state.ws_chat_connections.count();
    if current >= max {
        warn!(current, max, "WebSocket chat connections limit reached");
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    info!("WebSocket chat upgrade request for /v1/chat/ws");
    ws.on_upgrade(move |socket| handle_chat_socket(socket, state))
}

/// Handle an upgraded WebSocket chat connection.
///
/// This is a persistent connection: the client can send multiple chat requests
/// over the same WebSocket. Each request gets its own SSE stream response.
async fn handle_chat_socket(socket: WebSocket, state: AppState) {
    state.ws_chat_connections.increment();
    let (mut sender, mut receiver) = socket.split();

    info!("WebSocket chat client connected (total: {})", state.ws_chat_connections.count());

    loop {
        // Wait for a text frame from the client
        match receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                let request_id = uuid::Uuid::new_v4().to_string();

                // Parse the chat request
                let chat_req: WsChatRequest = match serde_json::from_str(&text) {
                    Ok(req) => req,
                    Err(e) => {
                        let error_json = serde_json::json!({
                            "error": {
                                "message": format!("Invalid JSON: {}", e),
                                "type": "validation_error",
                                "code": 400
                            }
                        });
                        let _ = sender
                            .send(Message::Text(error_json.to_string().into()))
                            .await;
                        continue;
                    }
                };

                let model = chat_req.model.clone();

                // Build the request body as serde_json::Value for proxy forwarding
                let body = serde_json::json!({
                    "model": chat_req.model,
                    "messages": chat_req.messages,
                    "stream": chat_req.stream,
                    "max_tokens": chat_req.max_tokens,
                    "temperature": chat_req.temperature,
                    "top_p": chat_req.top_p,
                });

                info!(
                    request_id = %request_id,
                    model = %model,
                    stream = chat_req.stream,
                    "Processing WebSocket chat request"
                );

                // Check for deduplication
                let messages = chat_req.messages.clone();
                let dedup_hash =
                    crate::dedup::RequestDedup::hash_request(&model, &messages);

                match state.request_dedup.register(dedup_hash) {
                    crate::dedup::DedupResult::FirstRequest {
                        response_writer,
                        done_signal,
                        byte_size_tracker: _,
                    } => {
                        // We are the first request — proxy to provider
                        let empty_headers = axum::http::HeaderMap::new();
                        let client_ip = "ws-client".to_string();

                        // Try pooled WS connection first for eligible providers
                        let pooled_result = if state.ws_pool.is_enabled() {
                            // Get the best provider endpoint for this model
                            let selected_endpoint = state
                                .provider_registry
                                .ranked_providers_for_model(Some(&model))
                                .first()
                                .map(|p| p.endpoint.clone());

                            if let Some(endpoint) = selected_endpoint {
                                try_pooled_ws_proxy(&state, &endpoint, &body).await
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        match pooled_result {
                            Some(pooled_response) => {
                                // Successfully proxied via pooled WS connection
                                info!(
                                    request_id = %request_id,
                                    model = %model,
                                    "Routed chat request via pooled WS connection"
                                );

                                // Forward the pooled response to the client
                                for line in pooled_response.split('\n') {
                                    let line = line.trim();
                                    if line.is_empty() {
                                        continue;
                                    }
                                    {
                                        let mut writer =
                                            response_writer.lock().await;
                                        writer.push(format!("{}\n", line));
                                    }
                                    if sender
                                        .send(Message::Text(
                                            format!("{}\n", line).into(),
                                        ))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }

                                state.demand.record(&model, 1);
                            }
                            None => {
                                // Pool unavailable or failed — fall back to HTTP proxy
                                debug!(
                                    request_id = %request_id,
                                    model = %model,
                                    "Pooled WS unavailable, falling back to HTTP proxy"
                                );

                                match crate::proxy::proxy_chat_completion(
                                    &state,
                                    body,
                                    &empty_headers,
                                    &request_id,
                                    &client_ip,
                                )
                                .await
                                {
                                    Ok(proxy_result) => {
                                        // For streaming responses, we need to consume the body
                                        // and forward each chunk as a WebSocket text frame.
                                        if chat_req.stream {
                                            // Stream the response body chunks over WebSocket
                                            let response_body = proxy_result.response.into_body();

                                            // Collect all body bytes and forward as SSE lines
                                            let full_body = axum::body::to_bytes(response_body, 10_000_000)
                                                .await
                                                .unwrap_or_default();
                                            let body_str = String::from_utf8_lossy(&full_body);

                                            for line in body_str.split('\n') {
                                                let line = line.trim();
                                                if line.is_empty() {
                                                    continue;
                                                }
                                                // Store in dedup buffer
                                                {
                                                    let mut writer =
                                                        response_writer.lock().await;
                                                    writer.push(format!("{}\n", line));
                                                }
                                                // Send as text frame
                                                if sender
                                                    .send(Message::Text(
                                                        format!("{}\n", line).into(),
                                                    ))
                                                    .await
                                                    .is_err()
                                                {
                                                    break;
                                                }
                                            }
                                            // Send [DONE] marker
                                            let done_line = "data: [DONE]\n";
                                            {
                                                let mut writer = response_writer.lock().await;
                                                writer.push(done_line.to_string());
                                            }
                                            let _ = sender
                                                .send(Message::Text(done_line.into()))
                                                .await;
                                        } else {
                                            // Non-streaming: collect full response and send as JSON
                                            let body_bytes =
                                                axum::body::to_bytes(proxy_result.response.into_body(), 10_000_000)
                                                    .await
                                                    .unwrap_or_default();
                                            let response_str =
                                                String::from_utf8_lossy(&body_bytes).to_string();

                                            {
                                                let mut writer = response_writer.lock().await;
                                                writer.push(response_str.clone());
                                            }

                                            if sender
                                                .send(Message::Text(response_str.into()))
                                                .await
                                                .is_err()
                                            {
                                                break;
                                            }
                                        }

                                        // Record demand
                                        state.demand.record(&model, 1);
                                    }
                                    Err(e) => {
                                        let error_json = serde_json::json!({
                                            "error": {
                                                "message": e.to_string(),
                                                "type": "relay_error",
                                                "code": "error"
                                            }
                                        });
                                        let error_str = error_json.to_string();
                                        {
                                            let mut writer = response_writer.lock().await;
                                            writer.push(error_str.clone());
                                        }
                                        let _ = sender
                                            .send(Message::Text(error_str.into()))
                                            .await;
                                    }
                                } // end match proxy_chat_completion
                            } // end None => { ... } (HTTP fallback)
                        } // end match pooled_result

                        // Signal completion and clean up dedup entry
                        let _ = done_signal.send(true);
                        state.request_dedup.complete(dedup_hash);
                    }
                    crate::dedup::DedupResult::Duplicate {
                        mut wait_rx,
                        response_reader,
                    } => {
                        // Piggyback on the in-flight request
                        debug!(
                            request_id = %request_id,
                            model = %model,
                            "Piggybacking on deduplicated request"
                        );

                        // Wait for the original to complete
                        let _ = wait_rx.changed().await;

                        // Read and forward all collected chunks
                        let chunks = response_reader.lock().await;
                        for chunk in chunks.iter() {
                            if sender.send(Message::Text(chunk.clone().into())).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            }
            Some(Ok(Message::Ping(data))) => {
                if sender.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Some(Ok(Message::Pong(_))) => {
                // Ignore unsolicited pongs
            }
            Some(Ok(Message::Close(_))) | None => {
                debug!("WebSocket chat client sent close or disconnected");
                break;
            }
            Some(Ok(Message::Binary(_))) => {
                // Ignore binary messages
                let error_json = serde_json::json!({
                    "error": {
                        "message": "Binary messages not supported. Send JSON text frames.",
                        "type": "validation_error",
                        "code": 400
                    }
                });
                let _ = sender
                    .send(Message::Text(error_json.to_string().into()))
                    .await;
            }
            Some(Err(e)) => {
                error!(error = %e, "WebSocket chat client error");
                break;
            }
        }
    }

    state.ws_chat_connections.decrement();
    info!(
        "WebSocket chat client disconnected (total: {})",
        state.ws_chat_connections.count()
    );
}

// ---------------------------------------------------------------------------
// Pool-aware proxy helper for WS chat
// ---------------------------------------------------------------------------

/// Attempt to proxy a chat request via a pooled WS connection.
///
/// Returns `Some(response_json)` if a pooled connection was used successfully.
/// Returns `None` if the pool is disabled, no connection was available, or the
/// pooled connection failed — in which case the caller should fall back to the
/// standard HTTP proxy path.
async fn try_pooled_ws_proxy(
    state: &AppState,
    endpoint: &str,
    request_body: &serde_json::Value,
) -> Option<String> {
    if !state.ws_pool.is_enabled() {
        return None;
    }

    let mut conn = state.ws_pool.get_connection(endpoint).await?;
    let request_json = serde_json::to_string(request_body).ok()?;

    // Send the request over the pooled WebSocket
    if conn
        .sender
        .send(TungsteniteMessage::Text(request_json.into()))
        .await
        .is_err()
    {
        warn!(endpoint = %endpoint, "Failed to send over pooled WS, discarding connection");
        return None;
    }

    // Read response frames until we get a complete response
    let mut response_parts = Vec::new();
    let mut done = false;
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(state.config.relay.provider_timeout_secs);

    while !done {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            warn!(endpoint = %endpoint, "Pooled WS response timed out");
            // Connection is still valid, return it
            state.ws_pool.return_connection(conn).await;
            return None;
        }

        match tokio::time::timeout(remaining, conn.receiver.next()).await {
            Ok(Some(Ok(msg))) => match msg {
                TungsteniteMessage::Text(text) => {
                    let text_str = &*text;
                    if text_str.contains("[DONE]") || text_str.contains("\"done\":true") {
                        done = true;
                    }
                    response_parts.push(text_str.to_string());
                }
                TungsteniteMessage::Ping(data) => {
                    let _ = conn
                        .sender
                        .send(TungsteniteMessage::Pong(data))
                        .await;
                }
                TungsteniteMessage::Close(_) => {
                    debug!(endpoint = %endpoint, "Pooled WS connection closed by provider");
                    return None;
                }
                _ => {}
            },
            Ok(Some(Err(e))) => {
                warn!(endpoint = %endpoint, error = %e, "Pooled WS read error");
                return None;
            }
            Ok(None) => {
                debug!(endpoint = %endpoint, "Pooled WS stream ended");
                return None;
            }
            Err(_) => {
                warn!(endpoint = %endpoint, "Pooled WS response read timed out");
                state.ws_pool.return_connection(conn).await;
                return None;
            }
        }
    }

    // Return the connection to the pool for reuse
    state.ws_pool.return_connection(conn).await;

    if response_parts.is_empty() {
        return None;
    }

    // Combine all SSE lines into the response
    let full_response = response_parts.join("\n");
    Some(full_response)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_chat_request_deserialize() {
        let json = r#"{"model":"gpt-4","messages":[{"role":"user","content":"hello"}],"stream":true}"#;
        let req: WsChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "gpt-4");
        assert_eq!(req.messages.len(), 1);
        assert!(req.stream);
    }

    #[test]
    fn test_ws_chat_request_defaults() {
        let json = r#"{"model":"gpt-4","messages":[]}"#;
        let req: WsChatRequest = serde_json::from_str(json).unwrap();
        assert!(!req.stream);
        assert!(req.max_tokens.is_none());
        assert!(req.temperature.is_none());
    }

    #[test]
    fn test_ws_chat_request_invalid_json() {
        let json = r#"not json at all"#;
        let result: Result<WsChatRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_ws_chat_connection_counter() {
        let counter = WsChatConnectionCounter::new();
        assert_eq!(counter.count(), 0);
        counter.increment();
        assert_eq!(counter.count(), 1);
        counter.increment();
        assert_eq!(counter.count(), 2);
        counter.decrement();
        assert_eq!(counter.count(), 1);
    }

    #[test]
    fn test_ws_chat_connection_counter_clone() {
        let counter = WsChatConnectionCounter::new();
        let clone = counter.clone();
        counter.increment();
        assert_eq!(clone.count(), 1);
        clone.decrement();
        assert_eq!(counter.count(), 0);
    }

    #[test]
    fn test_ws_chat_request_with_optional_params() {
        let json = r#"{"model":"gpt-4","messages":[{"role":"user","content":"hi"}],"stream":true,"max_tokens":100,"temperature":0.7,"top_p":0.9}"#;
        let req: WsChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.max_tokens, Some(100));
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.top_p, Some(0.9));
    }
}
