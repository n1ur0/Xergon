//! Server-Sent Events (SSE) endpoint for real-time provider and model status updates.
//!
//! Exposes GET /v1/events which streams events as:
//! ```text
//! event: provider_online
//! data: {"provider_id":"...","endpoint":"...","models":[...],"region":"..."}
//! ```
//!
//! Clients can filter with query params: `?types=provider_online,provider_offline`

use axum::body::Body;
use axum::extract::{Query, State};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};

use crate::chain::ChainProvider;
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

/// Event types emitted by the relay for real-time status updates.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RelayEvent {
    #[serde(rename_all = "snake_case")]
    ProviderOnline {
        provider_id: String,
        endpoint: String,
        models: Vec<String>,
        region: String,
    },
    #[serde(rename_all = "snake_case")]
    ProviderOffline {
        provider_id: String,
        reason: String,
    },
    #[serde(rename_all = "snake_case")]
    ModelAvailable {
        model_id: String,
        provider_id: String,
        price_per_input_nanoerg: u64,
    },
    #[serde(rename_all = "snake_case")]
    ModelUnavailable {
        model_id: String,
        last_provider_id: String,
    },
    #[serde(rename_all = "snake_case")]
    PriceChange {
        model_id: String,
        provider_id: String,
        old_price: u64,
        new_price: u64,
    },
    #[serde(rename_all = "snake_case")]
    NodeStatusChange {
        healthy: bool,
        message: String,
    },
}

impl RelayEvent {
    /// Returns the SSE `event:` field value for this variant.
    pub fn event_type(&self) -> &'static str {
        match self {
            RelayEvent::ProviderOnline { .. } => "provider_online",
            RelayEvent::ProviderOffline { .. } => "provider_offline",
            RelayEvent::ModelAvailable { .. } => "model_available",
            RelayEvent::ModelUnavailable { .. } => "model_unavailable",
            RelayEvent::PriceChange { .. } => "price_change",
            RelayEvent::NodeStatusChange { .. } => "node_status_change",
        }
    }
}

// ---------------------------------------------------------------------------
// Event broadcaster (wraps a tokio broadcast channel)
// ---------------------------------------------------------------------------

/// Wraps a `tokio::sync::broadcast` channel for publishing [`RelayEvent`]s
/// that are consumed by SSE subscribers.
#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<RelayEvent>,
}

impl EventBroadcaster {
    /// Create a new broadcaster with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event. If no subscribers exist the event is silently dropped.
    pub fn publish(&self, event: RelayEvent) {
        match self.sender.send(event) {
            Ok(n) => debug!(subscribers = n, "Event published"),
            Err(broadcast::error::SendError(_)) => {
                // No subscribers — that's fine
            }
        }
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<RelayEvent> {
        self.sender.subscribe()
    }

    /// Number of currently active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

// ---------------------------------------------------------------------------
// SSE handler
// ---------------------------------------------------------------------------

/// Query parameters for `GET /v1/events`.
#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    /// Comma-separated list of event types to include (default: all).
    pub types: Option<String>,
}

/// `GET /v1/events` — SSE endpoint for real-time relay events.
///
/// Each subscriber gets its own lightweight forwarding task that reads from
/// the broadcast channel and writes SSE frames into an mpsc channel consumed
/// by the HTTP response body stream.
pub async fn events_handler(
    State(state): State<AppState>,
    Query(params): Query<EventsQuery>,
) -> Response {
    let max = state.config.events.max_subscribers;
    let current = state.event_broadcaster.subscriber_count();
    if current >= max {
        return Response::builder()
            .status(503)
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::json!({"error": "max_subscribers_reached", "current": current, "max": max}).to_string(),
            ))
            .unwrap();
    }

    let receiver = state.event_broadcaster.subscribe();
    let allowed_types = parse_type_filter(&params.types);

    info!(
        allowed_types = ?params.types,
        "New SSE subscriber connected"
    );

    // Bridge: broadcast -> mpsc.  The forwarding task dies when the client
    // disconnects (tx.send fails).
    let (tx, rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);

    tokio::spawn(async move {
        let mut receiver = receiver;
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    // Apply optional type filter
                    if let Some(ref allowed) = allowed_types {
                        let et = event.event_type();
                        if !allowed.iter().any(|a| a == et) {
                            continue;
                        }
                    }
                    let frame = format_sse_frame(&event);
                    if tx.send(frame).await.is_err() {
                        // Client disconnected
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(lagged = n, "SSE subscriber lagged, skipping events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
        debug!("SSE forwarding task ended");
    });

    // Convert mpsc receiver into a Stream of Result<Bytes, axum::Error>
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|frame| Ok::<bytes::Bytes, axum::Error>(frame));

    Response::builder()
        .status(200)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// Parse the optional `types` query param into a set of lowercase event type strings.
fn parse_type_filter(types: &Option<String>) -> Option<Vec<String>> {
    types.as_ref().map(|s| {
        s.split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    })
}

/// Format a [`RelayEvent`] as an SSE frame: `event: <type>\ndata: <json>\n\n`.
fn format_sse_frame(event: &RelayEvent) -> bytes::Bytes {
    let json = serde_json::to_string(event).unwrap_or_else(|e| {
        warn!(error = %e, "Failed to serialize SSE event");
        "{}".to_string()
    });
    bytes::Bytes::from(format!("event: {}\ndata: {}\n\n", event.event_type(), json))
}

// ---------------------------------------------------------------------------
// Diff helper — compare old vs new provider lists and publish events
// ---------------------------------------------------------------------------

/// Compare the previous and new chain provider snapshots, publishing
/// [`RelayEvent`]s for every detected change (online/offline providers,
/// added/removed models, price changes).
pub fn diff_and_publish_events(
    old_providers: &[ChainProvider],
    new_providers: &[ChainProvider],
    broadcaster: &EventBroadcaster,
) {
    let old_map: HashMap<&str, &ChainProvider> = old_providers
        .iter()
        .map(|p| (p.endpoint.as_str(), p))
        .collect();

    let new_map: HashMap<&str, &ChainProvider> = new_providers
        .iter()
        .map(|p| (p.endpoint.as_str(), p))
        .collect();

    // --- Providers that went offline ---
    for (endpoint, provider) in &old_map {
        if !new_map.contains_key(*endpoint) {
            broadcaster.publish(RelayEvent::ProviderOffline {
                provider_id: provider.box_id.clone(),
                reason: "removed_from_chain".to_string(),
            });
            for model in &provider.models {
                broadcaster.publish(RelayEvent::ModelUnavailable {
                    model_id: model.clone(),
                    last_provider_id: provider.box_id.clone(),
                });
            }
        }
    }

    // --- Providers that came online or changed ---
    for (endpoint, provider) in &new_map {
        if let Some(old_provider) = old_map.get(*endpoint) {
            // Provider persisted — check model and price changes
            let old_models: HashSet<&str> =
                old_provider.models.iter().map(|s| s.as_str()).collect();
            let new_models: HashSet<&str> =
                provider.models.iter().map(|s| s.as_str()).collect();

            // Newly-added models
            for model in &provider.models {
                if !old_models.contains(model.as_str()) {
                    let price = provider.model_pricing.get(model).copied().unwrap_or(0);
                    broadcaster.publish(RelayEvent::ModelAvailable {
                        model_id: model.clone(),
                        provider_id: provider.box_id.clone(),
                        price_per_input_nanoerg: price,
                    });
                }
            }

            // Removed models
            for model in &old_provider.models {
                if !new_models.contains(model.as_str()) {
                    broadcaster.publish(RelayEvent::ModelUnavailable {
                        model_id: model.clone(),
                        last_provider_id: provider.box_id.clone(),
                    });
                }
            }

            // Price changes for models that existed before and after
            for model in &provider.models {
                if old_models.contains(model.as_str()) {
                    let old_price = old_provider.model_pricing.get(model).copied().unwrap_or(0);
                    let new_price = provider.model_pricing.get(model).copied().unwrap_or(0);
                    if old_price != new_price {
                        broadcaster.publish(RelayEvent::PriceChange {
                            model_id: model.clone(),
                            provider_id: provider.box_id.clone(),
                            old_price,
                            new_price,
                        });
                    }
                }
            }
        } else {
            // Brand-new provider
            broadcaster.publish(RelayEvent::ProviderOnline {
                provider_id: provider.box_id.clone(),
                endpoint: provider.endpoint.clone(),
                models: provider.models.clone(),
                region: provider.region.clone(),
            });
            for model in &provider.models {
                let price = provider.model_pricing.get(model).copied().unwrap_or(0);
                broadcaster.publish(RelayEvent::ModelAvailable {
                    model_id: model.clone(),
                    provider_id: provider.box_id.clone(),
                    price_per_input_nanoerg: price,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Health-check event helpers
// ---------------------------------------------------------------------------

/// Publish [`ProviderOffline`] events for providers that transitioned from
/// healthy to unhealthy.
pub fn publish_health_offline_events(
    was_healthy: &HashMap<String, bool>,
    registry: &crate::provider::ProviderRegistry,
    broadcaster: &EventBroadcaster,
) {
    for entry in registry.providers.iter() {
        let provider = entry.value();
        let now_healthy = provider.is_healthy;
        let before = was_healthy.get(&provider.endpoint).copied().unwrap_or(false);
        if before && !now_healthy {
            broadcaster.publish(RelayEvent::ProviderOffline {
                provider_id: provider.endpoint.clone(),
                reason: "health_check_failed".to_string(),
            });
        }
    }
}

/// Publish [`ProviderOnline`] events for providers that transitioned from
/// unhealthy to healthy.
pub fn publish_health_online_events(
    was_healthy: &HashMap<String, bool>,
    registry: &crate::provider::ProviderRegistry,
    broadcaster: &EventBroadcaster,
) {
    for entry in registry.providers.iter() {
        let provider = entry.value();
        let now_healthy = provider.is_healthy;
        let before = was_healthy.get(&provider.endpoint).copied().unwrap_or(false);
        if !before && now_healthy {
            broadcaster.publish(RelayEvent::ProviderOnline {
                provider_id: provider.endpoint.clone(),
                endpoint: provider.endpoint.clone(),
                models: Vec::new(), // models are not tracked per-provider in registry
                region: String::new(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider(id: &str, endpoint: &str) -> ChainProvider {
        ChainProvider {
            box_id: format!("box-{}", id),
            provider_pk: "02".to_string() + &"00".repeat(32),
            endpoint: endpoint.to_string(),
            models: vec!["llama-3".to_string()],
            model_pricing: std::collections::HashMap::new(),
            pown_score: 50,
            last_heartbeat: 100,
            region: "us-east".to_string(),
            pricing_nanoerg_per_million_tokens: None,
            value_nanoerg: 1_000_000_000,
        }
    }

    #[test]
    fn test_event_type_strings() {
        assert_eq!(
            RelayEvent::ProviderOnline {
                provider_id: "x".into(),
                endpoint: "y".into(),
                models: vec![],
                region: "z".into(),
            }
            .event_type(),
            "provider_online"
        );
        assert_eq!(
            RelayEvent::ProviderOffline {
                provider_id: "x".into(),
                reason: "r".into(),
            }
            .event_type(),
            "provider_offline"
        );
        assert_eq!(
            RelayEvent::ModelAvailable {
                model_id: "m".into(),
                provider_id: "p".into(),
                price_per_input_nanoerg: 0,
            }
            .event_type(),
            "model_available"
        );
        assert_eq!(
            RelayEvent::ModelUnavailable {
                model_id: "m".into(),
                last_provider_id: "p".into(),
            }
            .event_type(),
            "model_unavailable"
        );
        assert_eq!(
            RelayEvent::PriceChange {
                model_id: "m".into(),
                provider_id: "p".into(),
                old_price: 1,
                new_price: 2,
            }
            .event_type(),
            "price_change"
        );
        assert_eq!(
            RelayEvent::NodeStatusChange {
                healthy: true,
                message: "ok".into(),
            }
            .event_type(),
            "node_status_change"
        );
    }

    #[test]
    fn test_parse_type_filter() {
        assert!(parse_type_filter(&None).is_none());
        let filter = parse_type_filter(&Some("provider_online,provider_offline".into())).unwrap();
        assert_eq!(filter, vec!["provider_online", "provider_offline"]);
    }

    #[test]
    fn test_diff_new_provider() {
        let broadcaster = EventBroadcaster::new(10);
        let mut rx = broadcaster.subscribe();

        let old: Vec<ChainProvider> = vec![];
        let new = vec![make_provider("p1", "http://p1.example.com:9099")];

        diff_and_publish_events(&old, &new, &broadcaster);

        // Should receive provider_online + model_available
        let ev1 = rx.try_recv().unwrap();
        assert!(matches!(ev1, RelayEvent::ProviderOnline { .. }));
        let ev2 = rx.try_recv().unwrap();
        assert!(matches!(ev2, RelayEvent::ModelAvailable { .. }));
    }

    #[test]
    fn test_diff_removed_provider() {
        let broadcaster = EventBroadcaster::new(10);
        let mut rx = broadcaster.subscribe();

        let old = vec![make_provider("p1", "http://p1.example.com:9099")];
        let new: Vec<ChainProvider> = vec![];

        diff_and_publish_events(&old, &new, &broadcaster);

        let ev1 = rx.try_recv().unwrap();
        assert!(matches!(ev1, RelayEvent::ProviderOffline { .. }));
        let ev2 = rx.try_recv().unwrap();
        assert!(matches!(ev2, RelayEvent::ModelUnavailable { .. }));
    }

    #[test]
    fn test_diff_price_change() {
        let broadcaster = EventBroadcaster::new(10);
        let mut rx = broadcaster.subscribe();

        let mut p_old = make_provider("p1", "http://p1.example.com:9099");
        p_old.model_pricing.insert("llama-3".into(), 100);

        let mut p_new = make_provider("p1", "http://p1.example.com:9099");
        p_new.model_pricing.insert("llama-3".into(), 200);

        diff_and_publish_events(&[p_old], &[p_new], &broadcaster);

        let ev = rx.try_recv().unwrap();
        assert!(matches!(ev, RelayEvent::PriceChange { old_price: 100, new_price: 200, .. }));
    }

    #[test]
    fn test_broadcaster_subscriber_count() {
        let b = EventBroadcaster::new(10);
        assert_eq!(b.subscriber_count(), 0);
        let _rx1 = b.subscribe();
        assert_eq!(b.subscriber_count(), 1);
        let _rx2 = b.subscribe();
        assert_eq!(b.subscriber_count(), 2);
    }

    #[test]
    fn test_sse_frame_format() {
        let event = RelayEvent::ProviderOnline {
            provider_id: "box-123".into(),
            endpoint: "http://example.com:9099".into(),
            models: vec!["llama-3".into()],
            region: "us-east".into(),
        };
        let frame = format_sse_frame(&event);
        let frame_slice: &[u8] = frame.as_ref();
        assert!(frame_slice.starts_with(b"event: provider_online\ndata: "));
        assert!(frame_slice.ends_with(b"\n\n"));
        assert!(frame_slice.windows(23).any(|w| w == b"\"provider_id\":\"box-123\""));
    }
}
