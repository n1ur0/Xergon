//! # Event Bus
//!
//! Pub/sub event system for the Xergon marketplace.
//! Provides event publishing, subscription, activity feeds, aggregation, and statistics.
//!
//! REST endpoints are nested under `/v1/events`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ================================================================
// XergonEventType
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum XergonEventType {
    ProviderRegistered,
    ProviderDeregistered,
    ProviderOnline,
    ProviderOffline,
    ModelListed,
    ModelUpdated,
    ModelDelisted,
    InferenceCompleted,
    PaymentReceived,
    StakeDeposited,
    StakeWithdrawn,
    GovernanceProposalCreated,
    VoteCast,
    AttestationSubmitted,
    ProofVerified,
    FraudDetected,
    DeploymentCompleted,
    AlertTriggered,
}

impl std::fmt::Display for XergonEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProviderRegistered => write!(f, "ProviderRegistered"),
            Self::ProviderDeregistered => write!(f, "ProviderDeregistered"),
            Self::ProviderOnline => write!(f, "ProviderOnline"),
            Self::ProviderOffline => write!(f, "ProviderOffline"),
            Self::ModelListed => write!(f, "ModelListed"),
            Self::ModelUpdated => write!(f, "ModelUpdated"),
            Self::ModelDelisted => write!(f, "ModelDelisted"),
            Self::InferenceCompleted => write!(f, "InferenceCompleted"),
            Self::PaymentReceived => write!(f, "PaymentReceived"),
            Self::StakeDeposited => write!(f, "StakeDeposited"),
            Self::StakeWithdrawn => write!(f, "StakeWithdrawn"),
            Self::GovernanceProposalCreated => write!(f, "GovernanceProposalCreated"),
            Self::VoteCast => write!(f, "VoteCast"),
            Self::AttestationSubmitted => write!(f, "AttestationSubmitted"),
            Self::ProofVerified => write!(f, "ProofVerified"),
            Self::FraudDetected => write!(f, "FraudDetected"),
            Self::DeploymentCompleted => write!(f, "DeploymentCompleted"),
            Self::AlertTriggered => write!(f, "AlertTriggered"),
        }
    }
}

// ================================================================
// EventPriority
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for EventPriority {
    fn default() -> Self {
        Self::Normal
    }
}

// ================================================================
// XergonEvent
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct XergonEvent {
    pub id: String,
    pub event_type: XergonEventType,
    pub source: String,
    pub payload: serde_json::Value,
    pub timestamp: i64,
    pub priority: EventPriority,
}

// ================================================================
// EventSubscription
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventSubscription {
    pub id: String,
    pub subscriber_id: String,
    pub event_types: Vec<XergonEventType>,
    pub filter: serde_json::Value,
    pub created_at: i64,
    pub active: bool,
}

// ================================================================
// ActivityFeedItem
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActivityFeedItem {
    pub id: String,
    pub event_type: XergonEventType,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub summary: String,
    pub details: serde_json::Value,
    pub timestamp: i64,
}

// ================================================================
// EventAggregation
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventAggregation {
    pub event_type: String,
    pub count: u64,
    pub first_seen: i64,
    pub last_seen: i64,
    pub unique_sources: u64,
}

// ================================================================
// EventStats
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventStats {
    pub total_events: u64,
    pub events_by_type: HashMap<String, u64>,
    pub total_subscriptions: u64,
    pub active_subscriptions: u64,
    pub events_per_minute: f64,
}

// ================================================================
// Acknowledgment
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventAck {
    pub subscriber_id: String,
    pub event_id: String,
    pub acknowledged: bool,
    pub acked_at: i64,
}

// ================================================================
// EventBus (core state)
// ================================================================

pub struct EventBus {
    /// All published events, keyed by event ID.
    events: DashMap<String, XergonEvent>,
    /// Active subscriptions, keyed by subscription ID.
    subscriptions: DashMap<String, EventSubscription>,
    /// Events delivered to subscribers (subscriber_id -> list of event_ids).
    subscriber_events: DashMap<String, Vec<String>>,
    /// Acknowledged events (subscriber_id -> set of event_ids).
    acknowledged: DashMap<String, DashMap<String, bool>>,
    /// Activity feed items.
    activity_feed: DashMap<String, ActivityFeedItem>,
    /// Timestamp of the first event, for events-per-minute calculation.
    first_event_ts: std::sync::atomic::AtomicI64,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            events: DashMap::new(),
            subscriptions: DashMap::new(),
            subscriber_events: DashMap::new(),
            acknowledged: DashMap::new(),
            activity_feed: DashMap::new(),
            first_event_ts: std::sync::atomic::AtomicI64::new(0),
        }
    }

    /// Publish an event to the bus.
    pub fn publish(&self, event: XergonEvent) -> XergonEvent {
        let event_id = event.id.clone();
        let event_type = event.event_type.clone();
        let timestamp = event.timestamp;

        // Store first event timestamp for rate calculation
        let first_ts = self.first_event_ts.load(std::sync::atomic::Ordering::Relaxed);
        if first_ts == 0 || timestamp < first_ts {
            self.first_event_ts.store(timestamp, std::sync::atomic::Ordering::Relaxed);
        }

        self.events.insert(event_id.clone(), event.clone());

        // Build activity feed item
        let summary = format!(
            "{} from {}",
            event.event_type, event.source
        );
        let provider_id = event.payload.get("provider_id")
            .and_then(|v| v.as_str())
            .map(String::from);
        let model_id = event.payload.get("model_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let feed_item = ActivityFeedItem {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type.clone(),
            provider_id,
            model_id,
            summary,
            details: event.payload.clone(),
            timestamp,
        };
        self.activity_feed.insert(feed_item.id.clone(), feed_item);

        // Deliver to matching subscriptions
        for sub in self.subscriptions.iter_mut() {
            if !sub.value().active {
                continue;
            }
            // Check if subscription is interested in this event type
            if sub.value().event_types.contains(&event_type) || sub.value().event_types.is_empty() {
                let sub_id = sub.value().subscriber_id.clone();
                drop(sub);
                self.deliver_to_subscriber(&sub_id, &event_id);
            }
        }

        event
    }

    fn deliver_to_subscriber(&self, subscriber_id: &str, event_id: &str) {
        let mut entry = self.subscriber_events.entry(subscriber_id.to_string()).or_default();
        let event_id_owned = event_id.to_string();
        if !entry.value().contains(&event_id_owned) {
            entry.value_mut().push(event_id_owned);
        }
    }

    /// Subscribe to events.
    pub fn subscribe(
        &self,
        subscriber_id: String,
        event_types: Vec<XergonEventType>,
        filter: serde_json::Value,
    ) -> EventSubscription {
        let sub = EventSubscription {
            id: uuid::Uuid::new_v4().to_string(),
            subscriber_id,
            event_types,
            filter,
            created_at: Utc::now().timestamp_millis(),
            active: true,
        };
        let sub_id = sub.id.clone();
        self.subscriptions.insert(sub_id.clone(), sub.clone());
        sub
    }

    /// Unsubscribe from events.
    pub fn unsubscribe(&self, subscription_id: &str) -> bool {
        let removed = self.subscriptions.remove(subscription_id);
        removed.is_some()
    }

    /// Get a specific event by ID.
    pub fn get_event(&self, id: &str) -> Option<XergonEvent> {
        self.events.get(id).map(|r| r.value().clone())
    }

    /// Query events with optional filters.
    pub fn get_events(
        &self,
        event_type: Option<&str>,
        source: Option<&str>,
        from: Option<i64>,
        to: Option<i64>,
        limit: Option<usize>,
    ) -> Vec<XergonEvent> {
        let limit = limit.unwrap_or(100);
        let mut results: Vec<XergonEvent> = self
            .events
            .iter()
            .filter(|entry| {
                let ev = entry.value();
                if let Some(et) = event_type {
                    if ev.event_type.to_string() != et {
                        return false;
                    }
                }
                if let Some(src) = source {
                    if ev.source != src {
                        return false;
                    }
                }
                if let Some(f) = from {
                    if ev.timestamp < f {
                        return false;
                    }
                }
                if let Some(t) = to {
                    if ev.timestamp > t {
                        return false;
                    }
                }
                true
            })
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by timestamp descending
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(limit);
        results
    }

    /// Get activity feed items.
    pub fn get_activity_feed(
        &self,
        provider_id: Option<&str>,
        limit: Option<usize>,
    ) -> Vec<ActivityFeedItem> {
        let limit = limit.unwrap_or(50);
        let mut items: Vec<ActivityFeedItem> = self
            .activity_feed
            .iter()
            .filter(|entry| {
                if let Some(pid) = provider_id {
                    match &entry.value().provider_id {
                        Some(p) => p == pid,
                        None => false,
                    }
                } else {
                    true
                }
            })
            .map(|entry| entry.value().clone())
            .collect();

        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        items.truncate(limit);
        items
    }

    /// List subscriptions.
    pub fn get_subscriptions(&self, subscriber_id: Option<&str>) -> Vec<EventSubscription> {
        let mut subs: Vec<EventSubscription> = self
            .subscriptions
            .iter()
            .filter(|entry| {
                if let Some(sid) = subscriber_id {
                    entry.value().subscriber_id == sid
                } else {
                    true
                }
            })
            .map(|entry| entry.value().clone())
            .collect();
        subs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        subs
    }

    /// Get event aggregations.
    pub fn get_aggregations(&self, from: Option<i64>, to: Option<i64>) -> Vec<EventAggregation> {
        let mut agg_map: HashMap<String, EventAggregation> = HashMap::new();

        for entry in self.events.iter() {
            let ev = entry.value();
            if let Some(f) = from {
                if ev.timestamp < f {
                    continue;
                }
            }
            if let Some(t) = to {
                if ev.timestamp > t {
                    continue;
                }
            }

            let type_key = ev.event_type.to_string();
            let agg = agg_map.entry(type_key.clone()).or_insert(EventAggregation {
                event_type: type_key,
                count: 0,
                first_seen: ev.timestamp,
                last_seen: ev.timestamp,
                unique_sources: 0,
            });
            agg.count += 1;
            if ev.timestamp < agg.first_seen {
                agg.first_seen = ev.timestamp;
            }
            if ev.timestamp > agg.last_seen {
                agg.last_seen = ev.timestamp;
            }
        }

        // Calculate unique sources per type
        for entry in self.events.iter() {
            let ev = entry.value();
            if let Some(f) = from {
                if ev.timestamp < f {
                    continue;
                }
            }
            if let Some(t) = to {
                if ev.timestamp > t {
                    continue;
                }
            }
            let type_key = ev.event_type.to_string();
            if let Some(agg) = agg_map.get_mut(&type_key) {
                agg.unique_sources += 1;
            }
        }

        let mut result: Vec<EventAggregation> = agg_map.into_values().collect();
        result.sort_by(|a, b| b.count.cmp(&a.count));
        result
    }

    /// Get event bus statistics.
    pub fn get_stats(&self) -> EventStats {
        let total_events = self.events.len() as u64;
        let total_subscriptions = self.subscriptions.len() as u64;
        let active_subscriptions = self
            .subscriptions
            .iter()
            .filter(|s| s.value().active)
            .count() as u64;

        let mut events_by_type: HashMap<String, u64> = HashMap::new();
        for entry in self.events.iter() {
            let type_key = entry.value().event_type.to_string();
            *events_by_type.entry(type_key).or_insert(0) += 1;
        }

        // Calculate events per minute
        let first_ts = self.first_event_ts.load(std::sync::atomic::Ordering::Relaxed);
        let now = Utc::now().timestamp_millis();
        let elapsed_minutes = if first_ts > 0 {
            ((now - first_ts) as f64) / 60_000.0
        } else {
            0.0
        };
        let events_per_minute = if elapsed_minutes > 0.0 {
            total_events as f64 / elapsed_minutes
        } else {
            0.0
        };

        EventStats {
            total_events,
            events_by_type,
            total_subscriptions,
            active_subscriptions,
            events_per_minute,
        }
    }

    /// Get events for a specific subscriber.
    pub fn get_subscriber_events(&self, subscriber_id: &str, limit: Option<usize>) -> Vec<XergonEvent> {
        let limit = limit.unwrap_or(100);
        let event_ids = match self.subscriber_events.get(subscriber_id) {
            Some(ids) => ids.clone(),
            None => return vec![],
        };

        let mut events: Vec<XergonEvent> = event_ids
            .iter()
            .filter_map(|id| self.events.get(id).map(|r| r.value().clone()))
            .collect();

        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(limit);
        events
    }

    /// Acknowledge an event for a subscriber.
    pub fn ack_event(&self, subscriber_id: &str, event_id: &str) -> bool {
        let ack_map = self
            .acknowledged
            .entry(subscriber_id.to_string())
            .or_insert_with(|| DashMap::new());
        ack_map.insert(event_id.to_string(), true);
        true
    }

    /// Prune events older than a given timestamp.
    pub fn prune_events(&self, older_than: i64) -> u64 {
        let mut count = 0u64;
        let event_ids: Vec<String> = self
            .events
            .iter()
            .filter(|entry| entry.value().timestamp < older_than)
            .map(|entry| entry.key().clone())
            .collect();

        for id in &event_ids {
            if self.events.remove(id).is_some() {
                count += 1;
            }
        }

        // Also prune activity feed items
        let feed_ids: Vec<String> = self
            .activity_feed
            .iter()
            .filter(|entry| entry.value().timestamp < older_than)
            .map(|entry| entry.key().clone())
            .collect();

        for id in &feed_ids {
            self.activity_feed.remove(id);
        }

        count
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// AppState
// ================================================================

#[derive(Clone)]
pub struct AppState {
    pub event_bus: Arc<EventBus>,
}

// ================================================================
// Request / Response DTOs
// ================================================================

#[derive(Deserialize)]
pub struct PublishRequest {
    pub event_type: XergonEventType,
    pub source: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub priority: Option<EventPriority>,
}

#[derive(Serialize)]
pub struct PublishResponse {
    pub id: String,
    pub event_type: String,
    pub timestamp: i64,
}

#[derive(Deserialize)]
pub struct SubscribeRequest {
    pub subscriber_id: String,
    #[serde(default)]
    pub event_types: Vec<XergonEventType>,
    #[serde(default)]
    pub filter: serde_json::Value,
}

#[derive(Deserialize)]
pub struct EventQueryParams {
    pub event_type: Option<String>,
    pub source: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct FeedQueryParams {
    pub provider_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct SubscriptionQueryParams {
    pub subscriber_id: Option<String>,
}

#[derive(Deserialize)]
pub struct AggregationQueryParams {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Deserialize)]
pub struct SubscriberEventParams {
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct AckRequest {
    pub subscriber_id: String,
    pub event_id: String,
}

#[derive(Serialize)]
pub struct AckResponse {
    pub acknowledged: bool,
}

#[derive(Deserialize)]
pub struct PruneRequest {
    pub older_than: i64,
}

#[derive(Serialize)]
pub struct PruneResponse {
    pub pruned_count: u64,
}

#[derive(Serialize)]
pub struct UnsubscribeResponse {
    pub unsubscribed: bool,
}

// ================================================================
// REST Handlers
// ================================================================

/// POST /v1/events/publish
pub async fn publish_event(
    State(state): State<AppState>,
    Json(req): Json<PublishRequest>,
) -> Json<PublishResponse> {
    let event = XergonEvent {
        id: uuid::Uuid::new_v4().to_string(),
        event_type: req.event_type,
        source: req.source,
        payload: req.payload,
        timestamp: Utc::now().timestamp_millis(),
        priority: req.priority.unwrap_or_default(),
    };

    let event_type_str = event.event_type.to_string();
    let event_id = event.id.clone();
    let ts = event.timestamp;

    state.event_bus.publish(event);

    Json(PublishResponse {
        id: event_id,
        event_type: event_type_str,
        timestamp: ts,
    })
}

/// POST /v1/events/subscribe
pub async fn subscribe(
    State(state): State<AppState>,
    Json(req): Json<SubscribeRequest>,
) -> Json<EventSubscription> {
    let sub = state
        .event_bus
        .subscribe(req.subscriber_id, req.event_types, req.filter);
    Json(sub)
}

/// DELETE /v1/events/subscribe/:id
pub async fn unsubscribe(
    State(state): State<AppState>,
    Path(subscription_id): Path<String>,
) -> Json<UnsubscribeResponse> {
    let unsubscribed = state.event_bus.unsubscribe(&subscription_id);
    Json(UnsubscribeResponse { unsubscribed })
}

/// GET /v1/events/:id
pub async fn get_event(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.event_bus.get_event(&id) {
        Some(event) => Json(serde_json::to_value(event).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "Event not found"})),
    }
}

/// GET /v1/events
pub async fn query_events(
    State(state): State<AppState>,
    Query(params): Query<EventQueryParams>,
) -> Json<Vec<XergonEvent>> {
    let events = state.event_bus.get_events(
        params.event_type.as_deref(),
        params.source.as_deref(),
        params.from,
        params.to,
        params.limit,
    );
    Json(events)
}

/// GET /v1/events/feed
pub async fn activity_feed(
    State(state): State<AppState>,
    Query(params): Query<FeedQueryParams>,
) -> Json<Vec<ActivityFeedItem>> {
    let items = state.event_bus.get_activity_feed(
        params.provider_id.as_deref(),
        params.limit,
    );
    Json(items)
}

/// GET /v1/events/subscriptions
pub async fn list_subscriptions(
    State(state): State<AppState>,
    Query(params): Query<SubscriptionQueryParams>,
) -> Json<Vec<EventSubscription>> {
    let subs = state.event_bus.get_subscriptions(params.subscriber_id.as_deref());
    Json(subs)
}

/// GET /v1/events/aggregations
pub async fn get_aggregations(
    State(state): State<AppState>,
    Query(params): Query<AggregationQueryParams>,
) -> Json<Vec<EventAggregation>> {
    let aggs = state.event_bus.get_aggregations(params.from, params.to);
    Json(aggs)
}

/// GET /v1/events/stats
pub async fn get_stats(State(state): State<AppState>) -> Json<EventStats> {
    let stats = state.event_bus.get_stats();
    Json(stats)
}

/// GET /v1/events/subscriber/:id/events
pub async fn get_subscriber_events(
    State(state): State<AppState>,
    Path(subscriber_id): Path<String>,
    Query(params): Query<SubscriberEventParams>,
) -> Json<Vec<XergonEvent>> {
    let events = state.event_bus.get_subscriber_events(&subscriber_id, params.limit);
    Json(events)
}

/// POST /v1/events/ack
pub async fn ack_event(
    State(state): State<AppState>,
    Json(req): Json<AckRequest>,
) -> Json<AckResponse> {
    let acknowledged = state.event_bus.ack_event(&req.subscriber_id, &req.event_id);
    Json(AckResponse { acknowledged })
}

/// POST /v1/events/prune
pub async fn prune_events(
    State(state): State<AppState>,
    Json(req): Json<PruneRequest>,
) -> Json<PruneResponse> {
    let pruned_count = state.event_bus.prune_events(req.older_than);
    Json(PruneResponse { pruned_count })
}

// ================================================================
// Router
// ================================================================

pub fn event_bus_router() -> Router<AppState> {
    Router::new()
        .route("/publish", post(publish_event))
        .route("/subscribe", post(subscribe))
        .route("/subscribe/{id}", delete(unsubscribe))
        .route("/ack", post(ack_event))
        .route("/prune", post(prune_events))
        .route("/stats", get(get_stats))
        .route("/feed", get(activity_feed))
        .route("/subscriptions", get(list_subscriptions))
        .route("/aggregations", get(get_aggregations))
        .route("/subscriber/{id}/events", get(get_subscriber_events))
        .route("/{id}", get(get_event))
        .route("/", get(query_events))
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_bus() -> Arc<EventBus> {
        Arc::new(EventBus::new())
    }

    fn make_event(event_type: XergonEventType, source: &str) -> XergonEvent {
        XergonEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type,
            source: source.to_string(),
            payload: json!({"test": true}),
            timestamp: Utc::now().timestamp_millis(),
            priority: EventPriority::Normal,
        }
    }

    fn make_event_with_provider(
        event_type: XergonEventType,
        source: &str,
        provider_id: &str,
    ) -> XergonEvent {
        XergonEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type,
            source: source.to_string(),
            payload: json!({"provider_id": provider_id, "model_id": "model-1"}),
            timestamp: Utc::now().timestamp_millis(),
            priority: EventPriority::Normal,
        }
    }

    #[test]
    fn test_publish_event() {
        let bus = make_bus();
        let event = make_event(XergonEventType::ProviderRegistered, "test-source");
        let event_id = event.id.clone();

        let published = bus.publish(event);

        assert_eq!(published.id, event_id);
        assert_eq!(published.event_type, XergonEventType::ProviderRegistered);
        assert_eq!(published.source, "test-source");

        let retrieved = bus.get_event(&event_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, event_id);
    }

    #[test]
    fn test_subscribe_to_events() {
        let bus = make_bus();

        let sub = bus.subscribe(
            "sub-1".to_string(),
            vec![XergonEventType::ProviderRegistered],
            json!({}),
        );

        assert!(sub.active);
        assert_eq!(sub.subscriber_id, "sub-1");
        assert_eq!(sub.event_types.len(), 1);

        // Publish an event that matches
        let event = make_event(XergonEventType::ProviderRegistered, "src-1");
        bus.publish(event);

        let sub_events = bus.get_subscriber_events("sub-1", None);
        assert_eq!(sub_events.len(), 1);
    }

    #[test]
    fn test_unsubscribe() {
        let bus = make_bus();

        let sub = bus.subscribe(
            "sub-1".to_string(),
            vec![XergonEventType::ProviderRegistered],
            json!({}),
        );
        let sub_id = sub.id.clone();

        let removed = bus.unsubscribe(&sub_id);
        assert!(removed);

        let removed_again = bus.unsubscribe(&sub_id);
        assert!(!removed_again);
    }

    #[test]
    fn test_event_filtering_by_type() {
        let bus = make_bus();

        bus.subscribe(
            "sub-filter".to_string(),
            vec![XergonEventType::ProviderRegistered],
            json!({}),
        );

        // Publish matching event
        bus.publish(make_event(XergonEventType::ProviderRegistered, "src-1"));
        // Publish non-matching event
        bus.publish(make_event(XergonEventType::ModelListed, "src-2"));

        let events = bus.get_subscriber_events("sub-filter", None);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, XergonEventType::ProviderRegistered);
    }

    #[test]
    fn test_activity_feed() {
        let bus = make_bus();

        bus.publish(make_event_with_provider(
            XergonEventType::ProviderRegistered,
            "src-1",
            "provider-abc",
        ));
        bus.publish(make_event_with_provider(
            XergonEventType::ModelListed,
            "src-2",
            "provider-abc",
        ));
        bus.publish(make_event_with_provider(
            XergonEventType::InferenceCompleted,
            "src-3",
            "provider-xyz",
        ));

        // All feed items
        let all_feed = bus.get_activity_feed(None, None);
        assert_eq!(all_feed.len(), 3);

        // Filtered by provider
        let provider_feed = bus.get_activity_feed(Some("provider-abc"), None);
        assert_eq!(provider_feed.len(), 2);

        // Limited
        let limited_feed = bus.get_activity_feed(None, Some(1));
        assert_eq!(limited_feed.len(), 1);
    }

    #[test]
    fn test_event_aggregation() {
        let bus = make_bus();

        bus.publish(make_event(XergonEventType::ProviderRegistered, "src-1"));
        bus.publish(make_event(XergonEventType::ProviderRegistered, "src-2"));
        bus.publish(make_event(XergonEventType::ModelListed, "src-1"));

        let aggs = bus.get_aggregations(None, None);

        assert_eq!(aggs.len(), 2);
        let registered = aggs.iter().find(|a| a.event_type == "ProviderRegistered").unwrap();
        assert_eq!(registered.count, 2);
        let listed = aggs.iter().find(|a| a.event_type == "ModelListed").unwrap();
        assert_eq!(listed.count, 1);
    }

    #[test]
    fn test_stats_calculation() {
        let bus = make_bus();

        bus.publish(make_event(XergonEventType::ProviderRegistered, "src-1"));
        bus.publish(make_event(XergonEventType::ModelListed, "src-1"));

        let sub = bus.subscribe("sub-1".to_string(), vec![], json!({}));
        let _sub_id = sub.id;

        let stats = bus.get_stats();

        assert_eq!(stats.total_events, 2);
        assert_eq!(stats.total_subscriptions, 1);
        assert_eq!(stats.active_subscriptions, 1);
        assert_eq!(stats.events_by_type.get("ProviderRegistered"), Some(&1));
        assert_eq!(stats.events_by_type.get("ModelListed"), Some(&1));
        assert!(stats.events_per_minute >= 0.0);
    }

    #[test]
    fn test_subscriber_events() {
        let bus = make_bus();

        bus.subscribe(
            "sub-a".to_string(),
            vec![XergonEventType::ProviderRegistered, XergonEventType::ModelListed],
            json!({}),
        );
        bus.subscribe(
            "sub-b".to_string(),
            vec![XergonEventType::ModelListed],
            json!({}),
        );

        bus.publish(make_event(XergonEventType::ProviderRegistered, "src-1"));
        bus.publish(make_event(XergonEventType::ModelListed, "src-2"));

        let events_a = bus.get_subscriber_events("sub-a", None);
        assert_eq!(events_a.len(), 2);

        let events_b = bus.get_subscriber_events("sub-b", None);
        assert_eq!(events_b.len(), 1);
        assert_eq!(events_b[0].event_type, XergonEventType::ModelListed);
    }

    #[test]
    fn test_ack_event() {
        let bus = make_bus();

        let result = bus.ack_event("sub-1", "event-1");
        assert!(result);
    }

    #[test]
    fn test_prune_events() {
        let bus = make_bus();

        // Publish event with old timestamp
        let old_event = XergonEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: XergonEventType::ProviderRegistered,
            source: "old-src".to_string(),
            payload: json!({}),
            timestamp: 1000,
            priority: EventPriority::Normal,
        };
        let old_id = old_event.id.clone();
        bus.publish(old_event);

        // Publish event with recent timestamp
        let new_event = XergonEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: XergonEventType::ModelListed,
            source: "new-src".to_string(),
            payload: json!({}),
            timestamp: Utc::now().timestamp_millis(),
            priority: EventPriority::Normal,
        };
        let new_id = new_event.id.clone();
        bus.publish(new_event);

        // Prune events older than 2000ms
        let pruned = bus.prune_events(2000);
        assert_eq!(pruned, 1);

        // Old event should be gone
        assert!(bus.get_event(&old_id).is_none());
        // New event should still exist
        assert!(bus.get_event(&new_id).is_some());
    }

    #[test]
    fn test_concurrent_publishing() {
        let bus = make_bus();
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let bus_clone = bus.clone();
                std::thread::spawn(move || {
                    let event = make_event(XergonEventType::InferenceCompleted, &format!("src-{}", i));
                    bus_clone.publish(event);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let stats = bus.get_stats();
        assert_eq!(stats.total_events, 10);
    }

    #[test]
    fn test_event_priority_ordering() {
        assert!(EventPriority::Low < EventPriority::Normal);
        assert!(EventPriority::Normal < EventPriority::High);
        assert!(EventPriority::High < EventPriority::Critical);

        let mut priorities = vec![
            EventPriority::Critical,
            EventPriority::Low,
            EventPriority::Normal,
            EventPriority::High,
        ];
        priorities.sort();
        assert_eq!(
            priorities,
            vec![
                EventPriority::Low,
                EventPriority::Normal,
                EventPriority::High,
                EventPriority::Critical,
            ]
        );
    }
}
