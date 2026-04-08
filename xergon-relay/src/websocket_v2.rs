#![allow(dead_code)]
//! Enhanced WebSocket V2 — Channels, presence, typing indicators, message history.
//!
//! REST endpoints:
//! - GET    /v1/ws/channels              — list all channels
//! - POST   /v1/ws/channels              — create a channel
//! - DELETE /v1/ws/channels/{id}         — delete a channel
//! - GET    /v1/ws/channels/{id}/presence — channel presence
//! - GET    /v1/ws/channels/{id}/history  — channel message history
//! - POST   /v1/ws/channels/{id}/publish — publish a message

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Unique message type inside a V2 channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsMessageType {
    Subscribe,
    Unsubscribe,
    Broadcast,
    Direct,
    Presence,
    Typing,
    Heartbeat,
    Error,
}

/// A single message exchanged in a V2 channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessage {
    pub msg_id: String,
    pub channel_id: String,
    pub sender_id: String,
    pub msg_type: WsMessageType,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// Presence status for a user in a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    Online,
    Away,
    Busy,
}

/// Presence info tracked per (user, channel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInfo {
    pub user_id: String,
    pub channel_id: String,
    pub status: PresenceStatus,
    pub last_seen: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

/// A single channel with subscriber tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsChannel {
    pub channel_id: String,
    pub name: String,
    pub subscribers: HashSet<String>,
    pub created_at: DateTime<Utc>,
    pub max_subscribers: usize,
    pub persistent: bool,
}

// ---------------------------------------------------------------------------
// CreateChannel request
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    #[serde(default = "default_max_subscribers")]
    pub max_subscribers: usize,
    #[serde(default)]
    pub persistent: bool,
}

fn default_max_subscribers() -> usize {
    1000
}

/// Publish request body.
#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    pub sender_id: String,
    pub msg_type: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Presence update request.
#[derive(Debug, Deserialize)]
pub struct UpdatePresenceRequest {
    pub user_id: String,
    pub status: PresenceStatus,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Subscribe request.
#[derive(Debug, Deserialize)]
pub struct SubscribeRequest {
    pub user_id: String,
}

/// Unsubscribe request.
#[derive(Debug, Deserialize)]
pub struct UnsubscribeRequest {
    pub user_id: String,
}

// ---------------------------------------------------------------------------
// History entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub msg_id: String,
    pub sender_id: String,
    pub msg_type: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ChannelInfo response
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ChannelInfoResponse {
    pub channel_id: String,
    pub name: String,
    pub subscriber_count: usize,
    pub max_subscribers: usize,
    pub persistent: bool,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// WebSocketV2 core struct
// ---------------------------------------------------------------------------

/// Per-channel message history ring buffer size.
const HISTORY_CAP: usize = 500;

#[derive(Debug, Clone)]
pub struct WebSocketV2 {
    /// channel_id -> WsChannel
    channels: Arc<DashMap<String, WsChannel>>,
    /// (channel_id, user_id) -> PresenceInfo
    presence: Arc<DashMap<(String, String), PresenceInfo>>,
    /// channel_id -> message history
    history: Arc<DashMap<String, Vec<WsMessage>>>,
}

impl Default for WebSocketV2 {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketV2 {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
            presence: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
        }
    }

    // -- Channel management ------------------------------------------------

    /// Create a new channel. Returns the channel ID.
    pub fn create_channel(
        &self,
        name: String,
        max_subscribers: usize,
        persistent: bool,
    ) -> String {
        let channel_id = uuid::Uuid::new_v4().to_string();
        let channel = WsChannel {
            channel_id: channel_id.clone(),
            name,
            subscribers: HashSet::new(),
            created_at: Utc::now(),
            max_subscribers,
            persistent,
        };
        self.channels.insert(channel_id.clone(), channel);
        info!(channel_id = %channel_id, "V2 channel created");
        channel_id
    }

    /// Delete a channel and all associated presence / history.
    pub fn delete_channel(&self, channel_id: &str) -> bool {
        let removed = self.channels.remove(channel_id).is_some();
        if removed {
            // Clean up presence for this channel
            let keys_to_remove: Vec<(String, String)> = self
                .presence
                .iter()
                .filter(|e| e.key().0 == channel_id)
                .map(|e| e.key().clone())
                .collect();
            for key in keys_to_remove {
                self.presence.remove(&key);
            }
            self.history.remove(channel_id);
            info!(channel_id = %channel_id, "V2 channel deleted");
        }
        removed
    }

    /// Get info about a channel.
    pub fn get_channel_info(&self, channel_id: &str) -> Option<ChannelInfoResponse> {
        self.channels.get(channel_id).map(|ch| ChannelInfoResponse {
            channel_id: ch.channel_id.clone(),
            name: ch.name.clone(),
            subscriber_count: ch.subscribers.len(),
            max_subscribers: ch.max_subscribers,
            persistent: ch.persistent,
            created_at: ch.created_at,
        })
    }

    /// List all channels.
    pub fn list_channels(&self) -> Vec<ChannelInfoResponse> {
        self.channels
            .iter()
            .map(|ch| ChannelInfoResponse {
                channel_id: ch.channel_id.clone(),
                name: ch.name.clone(),
                subscriber_count: ch.subscribers.len(),
                max_subscribers: ch.max_subscribers,
                persistent: ch.persistent,
                created_at: ch.created_at,
            })
            .collect()
    }

    // -- Subscription -------------------------------------------------------

    /// Subscribe a user to a channel.
    pub fn subscribe(&self, channel_id: &str, user_id: &str) -> Result<(), String> {
        let mut ch = self
            .channels
            .get_mut(channel_id)
            .ok_or_else(|| format!("channel {channel_id} not found"))?;
        if ch.subscribers.len() >= ch.max_subscribers {
            return Err("channel is full".into());
        }
        ch.subscribers.insert(user_id.to_string());
        Ok(())
    }

    /// Unsubscribe a user from a channel.
    pub fn unsubscribe(&self, channel_id: &str, user_id: &str) -> bool {
        if let Some(mut ch) = self.channels.get_mut(channel_id) {
            let removed = ch.subscribers.remove(user_id);
            if removed {
                // Also remove presence
                self.presence.remove(&(channel_id.to_string(), user_id.to_string()));
            }
            return removed;
        }
        false
    }

    // -- Messaging ----------------------------------------------------------

    /// Publish a message to a channel (broadcast to all subscribers).
    pub fn publish(
        &self,
        channel_id: &str,
        sender_id: &str,
        msg_type: WsMessageType,
        payload: serde_json::Value,
    ) -> Result<WsMessage, String> {
        let ch = self
            .channels
            .get(channel_id)
            .ok_or_else(|| format!("channel {channel_id} not found"))?;
        if !ch.subscribers.contains(sender_id) {
            return Err("sender is not subscribed".into());
        }
        let msg = WsMessage {
            msg_id: uuid::Uuid::new_v4().to_string(),
            channel_id: channel_id.to_string(),
            sender_id: sender_id.to_string(),
            msg_type,
            payload,
            timestamp: Utc::now(),
        };
        // Append to history
        if let Some(mut hist) = self.history.get_mut(channel_id) {
            hist.push(msg.clone());
            // Keep a bounded ring buffer
            if hist.len() > HISTORY_CAP {
                let drain = hist.len() - HISTORY_CAP;
                hist.drain(..drain);
            }
        } else {
            self.history
                .insert(channel_id.to_string(), vec![msg.clone()]);
        }
        Ok(msg)
    }

    /// Send a direct message between two users in a channel.
    pub fn send_direct(
        &self,
        channel_id: &str,
        sender_id: &str,
        _recipient_id: &str,
        payload: serde_json::Value,
    ) -> Result<WsMessage, String> {
        self.publish(channel_id, sender_id, WsMessageType::Direct, payload)
    }

    // -- Presence -----------------------------------------------------------

    /// Update or set presence for a user in a channel.
    pub fn update_presence(
        &self,
        channel_id: &str,
        user_id: &str,
        status: PresenceStatus,
        metadata: HashMap<String, String>,
    ) -> Result<(), String> {
        // Ensure the user is subscribed
        let ch = self
            .channels
            .get(channel_id)
            .ok_or_else(|| format!("channel {channel_id} not found"))?;
        if !ch.subscribers.contains(user_id) {
            return Err("user is not subscribed to this channel".into());
        }
        let key = (channel_id.to_string(), user_id.to_string());
        self.presence.insert(
            key,
            PresenceInfo {
                user_id: user_id.to_string(),
                channel_id: channel_id.to_string(),
                status,
                last_seen: Utc::now(),
                metadata,
            },
        );
        Ok(())
    }

    /// Get presence for all users in a channel.
    pub fn get_presence(&self, channel_id: &str) -> Vec<PresenceInfo> {
        self.presence
            .iter()
            .filter(|e| e.key().0 == channel_id)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Get presence for a specific user in a channel.
    pub fn get_user_presence(&self, channel_id: &str, user_id: &str) -> Option<PresenceInfo> {
        self.presence
            .get(&(channel_id.to_string(), user_id.to_string()))
            .map(|v| v.clone())
    }

    // -- History ------------------------------------------------------------

    /// Get message history for a channel (bounded).
    pub fn get_history(&self, channel_id: &str, limit: usize) -> Vec<HistoryEntry> {
        self.history
            .get(channel_id)
            .map(|hist| {
                let start = if hist.len() > limit { hist.len() - limit } else { 0 };
                hist[start..]
                    .iter()
                    .map(|m| HistoryEntry {
                        msg_id: m.msg_id.clone(),
                        sender_id: m.sender_id.clone(),
                        msg_type: format!("{:?}", m.msg_type).to_lowercase(),
                        payload: m.payload.clone(),
                        timestamp: m.timestamp,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_ws_v2_router() -> Router<AppState> {
    Router::new()
        .route("/v1/ws/channels", get(list_channels_handler))
        .route("/v1/ws/channels", post(create_channel_handler))
        .route(
            "/v1/ws/channels/{id}",
            delete(delete_channel_handler),
        )
        .route(
            "/v1/ws/channels/{id}/presence",
            get(get_presence_handler),
        )
        .route(
            "/v1/ws/channels/{id}/history",
            get(get_history_handler),
        )
        .route(
            "/v1/ws/channels/{id}/publish",
            post(publish_handler),
        )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/ws/channels
async fn list_channels_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let channels = state.websocket_v2.list_channels();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "channels": channels })),
    )
}

/// POST /v1/ws/channels
async fn create_channel_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateChannelRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let channel_id = state
        .websocket_v2
        .create_channel(body.name, body.max_subscribers, body.persistent);
    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "channel_id": channel_id })),
    )
}

/// DELETE /v1/ws/channels/{id}
async fn delete_channel_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let deleted = state.websocket_v2.delete_channel(&id);
    if deleted {
        (StatusCode::OK, Json(serde_json::json!({ "deleted": true })))
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "channel not found" })),
        )
    }
}

/// GET /v1/ws/channels/{id}/presence
async fn get_presence_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let presence = state.websocket_v2.get_presence(&id);
    (StatusCode::OK, Json(serde_json::json!({ "presence": presence })))
}

/// GET /v1/ws/channels/{id}/history
async fn get_history_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let history = state.websocket_v2.get_history(&id, 100);
    (StatusCode::OK, Json(serde_json::json!({ "history": history })))
}

/// POST /v1/ws/channels/{id}/publish
async fn publish_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PublishRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let msg_type = match body.msg_type.as_deref() {
        Some("broadcast") | Some("typing") | Some("heartbeat") | None => WsMessageType::Broadcast,
        Some("direct") => WsMessageType::Direct,
        Some("presence") => WsMessageType::Presence,
        Some(other) => {
            warn!(msg_type = other, "Unknown message type, defaulting to broadcast");
            WsMessageType::Broadcast
        }
    };

    match state.websocket_v2.publish(&id, &body.sender_id, msg_type, body.payload) {
        Ok(msg) => (StatusCode::OK, Json(serde_json::json!(msg))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> WebSocketV2 {
        WebSocketV2::new()
    }

    #[test]
    fn test_create_channel() {
        let ws = setup();
        let id = ws.create_channel("general".into(), 100, false);
        let info = ws.get_channel_info(&id).unwrap();
        assert_eq!(info.name, "general");
        assert_eq!(info.subscriber_count, 0);
        assert_eq!(info.max_subscribers, 100);
        assert!(!info.persistent);
    }

    #[test]
    fn test_delete_channel() {
        let ws = setup();
        let id = ws.create_channel("temp".into(), 10, false);
        assert!(ws.delete_channel(&id));
        assert!(ws.get_channel_info(&id).is_none());
    }

    #[test]
    fn test_delete_nonexistent_channel() {
        let ws = setup();
        assert!(!ws.delete_channel("nope"));
    }

    #[test]
    fn test_subscribe_unsubscribe() {
        let ws = setup();
        let id = ws.create_channel("chat".into(), 10, false);
        assert!(ws.subscribe(&id, "alice").is_ok());
        let info = ws.get_channel_info(&id).unwrap();
        assert_eq!(info.subscriber_count, 1);
        assert!(ws.unsubscribe(&id, "alice"));
        let info = ws.get_channel_info(&id).unwrap();
        assert_eq!(info.subscriber_count, 0);
    }

    #[test]
    fn test_subscribe_nonexistent_channel() {
        let ws = setup();
        assert!(ws.subscribe("nope", "alice").is_err());
    }

    #[test]
    fn test_subscribe_full_channel() {
        let ws = setup();
        let id = ws.create_channel("tiny".into(), 2, false);
        ws.subscribe(&id, "a").unwrap();
        ws.subscribe(&id, "b").unwrap();
        assert!(ws.subscribe(&id, "c").is_err());
    }

    #[test]
    fn test_publish_message() {
        let ws = setup();
        let id = ws.create_channel("chat".into(), 10, false);
        ws.subscribe(&id, "alice").unwrap();
        let msg = ws
            .publish(&id, "alice", WsMessageType::Broadcast, serde_json::json!({"text": "hi"}))
            .unwrap();
        assert_eq!(msg.sender_id, "alice");
        assert_eq!(msg.msg_type, WsMessageType::Broadcast);
        assert!(!msg.msg_id.is_empty());
    }

    #[test]
    fn test_publish_non_subscriber_rejected() {
        let ws = setup();
        let id = ws.create_channel("chat".into(), 10, false);
        assert!(ws
            .publish(&id, "eve", WsMessageType::Broadcast, serde_json::json!({}))
            .is_err());
    }

    #[test]
    fn test_presence_tracking() {
        let ws = setup();
        let id = ws.create_channel("room".into(), 10, false);
        ws.subscribe(&id, "bob").unwrap();
        ws.update_presence(&id, "bob", PresenceStatus::Online, HashMap::new())
            .unwrap();
        let presence = ws.get_presence(&id);
        assert_eq!(presence.len(), 1);
        assert_eq!(presence[0].status, PresenceStatus::Online);
    }

    #[test]
    fn test_message_history() {
        let ws = setup();
        let id = ws.create_channel("logs".into(), 10, false);
        ws.subscribe(&id, "bot").unwrap();
        ws.publish(&id, "bot", WsMessageType::Broadcast, serde_json::json!({"n": 1}))
            .unwrap();
        ws.publish(&id, "bot", WsMessageType::Broadcast, serde_json::json!({"n": 2}))
            .unwrap();
        let history = ws.get_history(&id, 100);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_history_bounded() {
        let ws = setup();
        let id = ws.create_channel("big".into(), 10, false);
        ws.subscribe(&id, "spam").unwrap();
        for i in 0..600u32 {
            ws.publish(
                &id,
                "spam",
                WsMessageType::Broadcast,
                serde_json::json!({"i": i}),
            )
            .unwrap();
        }
        let history = ws.get_history(&id, 1000);
        assert_eq!(history.len(), 500); // capped at HISTORY_CAP
    }

    #[test]
    fn test_list_channels() {
        let ws = setup();
        ws.create_channel("a".into(), 10, false);
        ws.create_channel("b".into(), 10, true);
        let channels = ws.list_channels();
        assert_eq!(channels.len(), 2);
    }
}
