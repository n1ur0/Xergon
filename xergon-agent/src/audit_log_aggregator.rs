//! Audit Log Aggregator — Centralized audit log management
//!
//! Provides centralized logging, querying, and aggregation of audit events:
//! - AuditEvent: structured audit event with metadata
//! - AuditQuery: flexible query builder for event filtering
//! - AggregationConfig: retention, export, and category configuration
//! - AuditLogAggregator: DashMap-backed event store with category indexes
//!
//! Features:
//! - Category-based indexing for fast lookups
//! - Time-range queries with pagination
//! - Aggregation by category and actor
//! - Export in JSON and CSV formats
//! - Automatic retention management with event pruning
//! - Statistics tracking
//!
//! REST endpoints:
//! - POST /v1/audit/log                        — Log an audit event
//! - GET  /v1/audit/events                     — Query audit events
//! - GET  /v1/audit/events/{id}                — Get a specific event
//! - GET  /v1/audit/stats                      — Get audit statistics
//! - GET  /v1/audit/export                     — Export events (JSON/CSV)
//! - GET  /v1/audit/aggregation/category       — Aggregate by category
//! - GET  /v1/audit/aggregation/actor/{id}     — Aggregate by actor

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// LogLevel
// ---------------------------------------------------------------------------

/// Severity level for audit events.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Critical,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Critical => write!(f, "critical"),
        }
    }
}

impl LogLevel {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "info" => Some(LogLevel::Info),
            "warn" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            "critical" => Some(LogLevel::Critical),
            _ => None,
        }
    }

    /// Numeric severity for comparison (higher = more severe).
    pub fn severity(&self) -> u8 {
        match self {
            LogLevel::Info => 0,
            LogLevel::Warn => 1,
            LogLevel::Error => 2,
            LogLevel::Critical => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// AuditEvent
// ---------------------------------------------------------------------------

/// A structured audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event identifier.
    pub event_id: String,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Severity level.
    pub level: LogLevel,
    /// Event category (e.g., "auth", "inference", "model").
    pub category: String,
    /// The action performed (e.g., "login", "deploy", "delete").
    pub action: String,
    /// The user or service performing the action.
    pub actor_id: String,
    /// The resource being acted upon.
    pub resource_id: String,
    /// Additional event details.
    pub details: HashMap<String, serde_json::Value>,
    /// The source service that generated this event.
    pub source_service: String,
    /// Correlation/request ID.
    pub request_id: Option<String>,
}

impl AuditEvent {
    /// Create a new audit event.
    pub fn new(
        level: LogLevel,
        category: &str,
        action: &str,
        actor_id: &str,
        resource_id: &str,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            level,
            category: category.to_string(),
            action: action.to_string(),
            actor_id: actor_id.to_string(),
            resource_id: resource_id.to_string(),
            details: HashMap::new(),
            source_service: String::new(),
            request_id: None,
        }
    }

    /// Set the source service.
    pub fn with_source(mut self, service: &str) -> Self {
        self.source_service = service.to_string();
        self
    }

    /// Set the request ID.
    pub fn with_request_id(mut self, request_id: &str) -> Self {
        self.request_id = Some(request_id.to_string());
        self
    }

    /// Add a detail.
    pub fn with_detail(mut self, key: &str, value: serde_json::Value) -> Self {
        self.details.insert(key.to_string(), value);
        self
    }

    /// Add multiple details.
    pub fn with_details(mut self, details: HashMap<String, serde_json::Value>) -> Self {
        self.details.extend(details);
        self
    }
}

// ---------------------------------------------------------------------------
// AuditQuery
// ---------------------------------------------------------------------------

/// Query parameters for filtering audit events.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuditQuery {
    /// Filter events after this timestamp (ISO 8601).
    pub start_time: Option<DateTime<Utc>>,
    /// Filter events before this timestamp (ISO 8601).
    pub end_time: Option<DateTime<Utc>>,
    /// Filter by log level.
    pub level: Option<String>,
    /// Filter by category.
    pub category: Option<String>,
    /// Filter by actor ID.
    pub actor_id: Option<String>,
    /// Filter by resource ID.
    pub resource_id: Option<String>,
    /// Maximum number of results (default: 100).
    pub limit: Option<usize>,
    /// Offset for pagination (default: 0).
    pub offset: Option<usize>,
}

// ---------------------------------------------------------------------------
// AggregationConfig
// ---------------------------------------------------------------------------

/// Configuration for the audit log aggregator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationConfig {
    /// Number of days to retain events before pruning.
    pub retention_days: u64,
    /// Maximum number of events to store (ring buffer behavior).
    pub max_events: u64,
    /// Whether to auto-export events.
    pub auto_export: bool,
    /// Categories to track (empty = all).
    pub categories: Vec<String>,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        Self {
            retention_days: 90,
            max_events: 1_000_000,
            auto_export: false,
            categories: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// AuditStats
// ---------------------------------------------------------------------------

/// Aggregate statistics for audit events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditStats {
    /// Total number of events.
    pub total_events: u64,
    /// Events by log level.
    pub by_level: HashMap<String, u64>,
    /// Events by category.
    pub by_category: HashMap<String, u64>,
    /// Events by action.
    pub by_action: HashMap<String, u64>,
    /// Events by source service.
    pub by_source: HashMap<String, u64>,
    /// Total unique actors.
    pub unique_actors: usize,
    /// Total unique resources.
    pub unique_resources: usize,
    /// Oldest event timestamp.
    pub oldest_event: Option<DateTime<Utc>>,
    /// Newest event timestamp.
    pub newest_event: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// CategoryAggregation
// ---------------------------------------------------------------------------

/// Aggregation result for a single category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryAggregation {
    pub category: String,
    pub count: u64,
    pub by_level: HashMap<String, u64>,
    pub by_action: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// ActorAggregation
// ---------------------------------------------------------------------------

/// Aggregation result for a single actor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorAggregation {
    pub actor_id: String,
    pub total_events: u64,
    pub by_category: HashMap<String, u64>,
    pub by_level: HashMap<String, u64>,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// AuditLogAggregator
// ---------------------------------------------------------------------------

/// Centralized audit log manager with DashMap-backed event storage and indexes.
#[derive(Debug, Clone)]
pub struct AuditLogAggregator {
    /// All events keyed by event_id.
    events: Arc<DashMap<String, AuditEvent>>,
    /// Category index: category -> Vec<event_id>.
    category_index: Arc<DashMap<String, Vec<String>>>,
    /// Actor index: actor_id -> Vec<event_id>.
    actor_index: Arc<DashMap<String, Vec<String>>>,
    /// Aggregation configuration.
    config: Arc<std::sync::RwLock<AggregationConfig>>,
    /// Total events counter.
    total_events: Arc<AtomicU64>,
}

impl AuditLogAggregator {
    /// Create a new audit log aggregator with default configuration.
    pub fn new() -> Self {
        Self::with_config(AggregationConfig::default())
    }

    /// Create a new audit log aggregator with custom configuration.
    pub fn with_config(config: AggregationConfig) -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            category_index: Arc::new(DashMap::new()),
            actor_index: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(config)),
            total_events: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Log an audit event.
    pub fn log_event(&self, event: AuditEvent) -> Result<AuditEvent, String> {
        let config = self
            .config
            .read()
            .map_err(|e| format!("Config lock error: {}", e))?;

        // Check max events limit
        if self.events.len() as u64 >= config.max_events {
            // Remove oldest events (approximate: remove from the front of iterators)
            let to_remove = 100; // Batch removal
            let mut removed = 0;
            let mut keys_to_remove = Vec::with_capacity(to_remove);
            for entry in self.events.iter() {
                if removed >= to_remove {
                    break;
                }
                keys_to_remove.push(entry.key().clone());
                removed += 1;
            }
            for key in keys_to_remove {
                if let Some((_, old_event)) = self.events.remove(&key) {
                    // Clean up indexes
                    if let Some(mut cat_list) =
                        self.category_index.get_mut(&old_event.category)
                    {
                        cat_list.retain(|id| id != &key);
                    }
                    if let Some(mut actor_list) = self.actor_index.get_mut(&old_event.actor_id)
                    {
                        actor_list.retain(|id| id != &key);
                    }
                }
            }
        }

        // Check category filter
        if !config.categories.is_empty() && !config.categories.contains(&event.category) {
            return Ok(event); // Silently drop events not in configured categories
        }

        let event_id = event.event_id.clone();
        let category = event.category.clone();
        let actor_id = event.actor_id.clone();

        // Store the event
        self.events.insert(event_id.clone(), event.clone());

        // Update category index
        self.category_index
            .entry(category)
            .or_default()
            .value_mut()
            .push(event_id.clone());

        // Update actor index
        self.actor_index
            .entry(actor_id)
            .or_default()
            .value_mut()
            .push(event_id.clone());

        self.total_events.fetch_add(1, Ordering::Relaxed);

        Ok(event)
    }

    /// Query audit events with filters.
    pub fn query_events(&self, query: &AuditQuery) -> Vec<AuditEvent> {
        let limit = query.limit.unwrap_or(100).min(10000);
        let offset = query.offset.unwrap_or(0);
        let level_filter: Option<LogLevel> = query
            .level
            .as_ref()
            .and_then(|l| LogLevel::from_str(l));

        let mut results: Vec<AuditEvent> = self
            .events
            .iter()
            .filter(|entry| {
                let event = entry.value();

                // Time range filter
                if let Some(ref start) = query.start_time {
                    if event.timestamp < *start {
                        return false;
                    }
                }
                if let Some(ref end) = query.end_time {
                    if event.timestamp > *end {
                        return false;
                    }
                }

                // Level filter
                if let Some(ref level) = level_filter {
                    if event.level != *level {
                        return false;
                    }
                }

                // Category filter
                if let Some(ref category) = query.category {
                    if event.category != *category {
                        return false;
                    }
                }

                // Actor filter
                if let Some(ref actor_id) = query.actor_id {
                    if event.actor_id != *actor_id {
                        return false;
                    }
                }

                // Resource filter
                if let Some(ref resource_id) = query.resource_id {
                    if event.resource_id != *resource_id {
                        return false;
                    }
                }

                true
            })
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by timestamp descending (newest first)
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply pagination
        results
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect()
    }

    /// Get a specific event by ID.
    pub fn get_event(&self, event_id: &str) -> Option<AuditEvent> {
        self.events.get(event_id).map(|e| e.clone())
    }

    /// Get aggregate statistics.
    pub fn get_stats(&self) -> AuditStats {
        let mut stats = AuditStats::default();
        let mut actors: HashSet<String> = HashSet::new();
        let mut resources: HashSet<String> = HashSet::new();

        for entry in self.events.iter() {
            let event = entry.value();

            *stats
                .by_level
                .entry(event.level.to_string())
                .or_insert(0) += 1;
            *stats
                .by_category
                .entry(event.category.clone())
                .or_insert(0) += 1;
            *stats
                .by_action
                .entry(event.action.clone())
                .or_insert(0) += 1;
            *stats
                .by_source
                .entry(event.source_service.clone())
                .or_insert(0) += 1;

            actors.insert(event.actor_id.clone());
            resources.insert(event.resource_id.clone());

            match &stats.oldest_event {
                None => stats.oldest_event = Some(event.timestamp),
                Some(oldest) if event.timestamp < *oldest => {
                    stats.oldest_event = Some(event.timestamp);
                }
                _ => {}
            }

            match &stats.newest_event {
                None => stats.newest_event = Some(event.timestamp),
                Some(newest) if event.timestamp > *newest => {
                    stats.newest_event = Some(event.timestamp);
                }
                _ => {}
            }
        }

        stats.total_events = self.total_events.load(Ordering::Relaxed);
        stats.unique_actors = actors.len();
        stats.unique_resources = resources.len();

        stats
    }

    /// Export events in JSON format.
    pub fn export_json(&self, query: &AuditQuery) -> Result<String, String> {
        let events = self.query_events(query);
        serde_json::to_string_pretty(&events)
            .map_err(|e| format!("JSON serialization error: {}", e))
    }

    /// Export events in CSV format.
    pub fn export_csv(&self, query: &AuditQuery) -> Result<String, String> {
        let events = self.query_events(query);
        let mut csv = String::from(
            "event_id,timestamp,level,category,action,actor_id,resource_id,source_service,request_id\n",
        );

        for event in &events {
            let request_id = event.request_id.as_deref().unwrap_or("");
            let escaped_resource = event.resource_id.replace(',', ";");
            let escaped_action = event.action.replace(',', ";");
            let escaped_actor = event.actor_id.replace(',', ";");
            let escaped_category = event.category.replace(',', ";");
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                event.event_id,
                event.timestamp.to_rfc3339(),
                event.level,
                escaped_category,
                escaped_action,
                escaped_actor,
                escaped_resource,
                event.source_service,
                request_id,
            ));
        }

        Ok(csv)
    }

    /// Aggregate events by category.
    pub fn aggregate_by_category(&self) -> Vec<CategoryAggregation> {
        let mut category_data: HashMap<String, CategoryAggregation> = HashMap::new();

        for entry in self.events.iter() {
            let event = entry.value();

            let agg = category_data
                .entry(event.category.clone())
                .or_insert_with(|| CategoryAggregation {
                    category: event.category.clone(),
                    count: 0,
                    by_level: HashMap::new(),
                    by_action: HashMap::new(),
                });

            agg.count += 1;
            *agg.by_level
                .entry(event.level.to_string())
                .or_insert(0) += 1;
            *agg.by_action
                .entry(event.action.clone())
                .or_insert(0) += 1;
        }

        let mut results: Vec<CategoryAggregation> = category_data.into_values().collect();
        results.sort_by(|a, b| b.count.cmp(&a.count));
        results
    }

    /// Aggregate events by a specific actor.
    pub fn aggregate_by_actor(&self, actor_id: &str) -> ActorAggregation {
        let mut agg = ActorAggregation {
            actor_id: actor_id.to_string(),
            total_events: 0,
            by_category: HashMap::new(),
            by_level: HashMap::new(),
            first_seen: None,
            last_seen: None,
        };

        for entry in self.events.iter() {
            let event = entry.value();
            if event.actor_id != actor_id {
                continue;
            }

            agg.total_events += 1;
            *agg.by_category
                .entry(event.category.clone())
                .or_insert(0) += 1;
            *agg.by_level
                .entry(event.level.to_string())
                .or_insert(0) += 1;

            match &agg.first_seen {
                None => agg.first_seen = Some(event.timestamp),
                Some(first) if event.timestamp < *first => {
                    agg.first_seen = Some(event.timestamp);
                }
                _ => {}
            }

            match &agg.last_seen {
                None => agg.last_seen = Some(event.timestamp),
                Some(last) if event.timestamp > *last => {
                    agg.last_seen = Some(event.timestamp);
                }
                _ => {}
            }
        }

        agg
    }

    /// Prune events older than the configured retention period.
    pub fn prune_old_events(&self) -> Result<usize, String> {
        let config = self
            .config
            .read()
            .map_err(|e| format!("Config lock error: {}", e))?;
        let cutoff = Utc::now() - chrono::Duration::days(config.retention_days as i64);

        let mut pruned = 0;
        let mut keys_to_remove: Vec<String> = Vec::new();

        for entry in self.events.iter() {
            if entry.value().timestamp < cutoff {
                keys_to_remove.push(entry.key().clone());
            }
        }

        for key in keys_to_remove {
            if let Some((_, old_event)) = self.events.remove(&key) {
                // Clean up indexes
                if let Some(mut cat_list) = self.category_index.get_mut(&old_event.category) {
                    cat_list.retain(|id| id != &key);
                }
                if let Some(mut actor_list) = self.actor_index.get_mut(&old_event.actor_id) {
                    actor_list.retain(|id| id != &key);
                }
                pruned += 1;
            }
        }

        Ok(pruned)
    }

    /// Update the aggregation configuration.
    pub fn update_config(&self, new_config: AggregationConfig) -> Result<(), String> {
        let mut config = self
            .config
            .write()
            .map_err(|e| format!("Config lock error: {}", e))?;
        *config = new_config;
        Ok(())
    }

    /// Get the current configuration.
    pub fn get_config(&self) -> Result<AggregationConfig, String> {
        self.config
            .read()
            .map(|c| c.clone())
            .map_err(|e| format!("Config lock error: {}", e))
    }

    /// Get total event count.
    pub fn event_count(&self) -> u64 {
        self.total_events.load(Ordering::Relaxed)
    }
}

impl Default for AuditLogAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LogEventRequest {
    pub level: String,
    pub category: String,
    pub action: String,
    pub actor_id: String,
    pub resource_id: String,
    #[serde(default)]
    pub details: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub source_service: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub category: Option<String>,
    pub actor_id: Option<String>,
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// POST /v1/audit/log — Log an audit event.
pub async fn log_event_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<LogEventRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let level = match LogLevel::from_str(&req.level) {
        Some(l) => l,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid log level: {}", req.level)})),
            );
        }
    };

    let mut event = AuditEvent::new(level, &req.category, &req.action, &req.actor_id, &req.resource_id);
    event.details = req.details;
    if let Some(service) = req.source_service {
        event.source_service = service;
    }
    event.request_id = req.request_id;

    match state.audit_aggregator.log_event(event) {
        Ok(logged) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "event_id": logged.event_id,
                "timestamp": logged.timestamp.to_rfc3339(),
                "level": logged.level.to_string(),
                "category": logged.category,
                "action": logged.action,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// GET /v1/audit/events — Query audit events.
pub async fn query_events_handler(
    State(state): State<crate::api::AppState>,
    Query(query): Query<AuditQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let events = state.audit_aggregator.query_events(&query);
    let total = state.audit_aggregator.event_count();

    let response: Vec<serde_json::Value> = events
        .iter()
        .map(|e| serde_json::json!(e))
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "events": response,
            "count": response.len(),
            "total": total,
            "offset": query.offset.unwrap_or(0),
        })),
    )
}

/// GET /v1/audit/events/{id} — Get a specific event.
pub async fn get_event_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.audit_aggregator.get_event(&id) {
        Some(event) => (StatusCode::OK, Json(serde_json::json!(event))),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Event {} not found", id)})),
        ),
    }
}

/// GET /v1/audit/stats — Get audit statistics.
pub async fn get_stats_handler(
    State(state): State<crate::api::AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let stats = state.audit_aggregator.get_stats();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "total_events": stats.total_events,
            "by_level": stats.by_level,
            "by_category": stats.by_category,
            "by_action": stats.by_action,
            "by_source": stats.by_source,
            "unique_actors": stats.unique_actors,
            "unique_resources": stats.unique_resources,
            "oldest_event": stats.oldest_event.map(|t| t.to_rfc3339()),
            "newest_event": stats.newest_event.map(|t| t.to_rfc3339()),
        })),
    )
}

/// GET /v1/audit/export — Export events.
pub async fn export_events_handler(
    State(state): State<crate::api::AppState>,
    Query(query): Query<ExportQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let audit_query = AuditQuery {
        start_time: query.start_time,
        end_time: query.end_time,
        category: query.category,
        actor_id: query.actor_id,
        limit: query.limit,
        ..Default::default()
    };

    let format = query.format.as_deref().unwrap_or("json");

    match format {
        "json" => match state.audit_aggregator.export_json(&audit_query) {
            Ok(json) => (StatusCode::OK, Json(serde_json::json!({"format": "json", "data": serde_json::from_str::<serde_json::Value>(&json).unwrap_or(serde_json::json!([]))}))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
        },
        "csv" => match state.audit_aggregator.export_csv(&audit_query) {
            Ok(csv) => (StatusCode::OK, Json(serde_json::json!({"format": "csv", "data": csv}))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
        },
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Unsupported export format: {}", format)})),
        ),
    }
}

/// GET /v1/audit/aggregation/category — Aggregate events by category.
pub async fn aggregate_category_handler(
    State(state): State<crate::api::AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let aggregations = state.audit_aggregator.aggregate_by_category();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "aggregations": aggregations,
            "count": aggregations.len(),
        })),
    )
}

/// GET /v1/audit/aggregation/actor/{id} — Aggregate events by actor.
pub async fn aggregate_actor_handler(
    State(state): State<crate::api::AppState>,
    Path(actor_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let aggregation = state.audit_aggregator.aggregate_by_actor(&actor_id);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "actor_id": aggregation.actor_id,
            "total_events": aggregation.total_events,
            "by_category": aggregation.by_category,
            "by_level": aggregation.by_level,
            "first_seen": aggregation.first_seen.map(|t| t.to_rfc3339()),
            "last_seen": aggregation.last_seen.map(|t| t.to_rfc3339()),
        })),
    )
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the audit log aggregator router.
pub fn build_audit_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::get;
    use axum::routing::post;

    axum::Router::new()
        .route("/v1/audit/log", post(log_event_handler))
        .route("/v1/audit/events", get(query_events_handler))
        .route("/v1/audit/events/{id}", get(get_event_handler))
        .route("/v1/audit/stats", get(get_stats_handler))
        .route("/v1/audit/export", get(export_events_handler))
        .route(
            "/v1/audit/aggregation/category",
            get(aggregate_category_handler),
        )
        .route(
            "/v1/audit/aggregation/actor/{id}",
            get(aggregate_actor_handler),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_aggregator() -> AuditLogAggregator {
        AuditLogAggregator::with_config(AggregationConfig {
            retention_days: 90,
            max_events: 10000,
            auto_export: false,
            categories: Vec::new(),
        })
    }

    fn create_test_event(category: &str, action: &str, actor: &str) -> AuditEvent {
        AuditEvent::new(LogLevel::Info, category, action, actor, "resource-1")
            .with_source("test-service")
            .with_request_id("req-123")
            .with_detail("key", serde_json::json!("value"))
    }

    #[test]
    fn test_log_event() {
        let agg = create_test_aggregator();
        let event = create_test_event("auth", "login", "user-1");
        let logged = agg.log_event(event).unwrap();
        assert_eq!(logged.category, "auth");
        assert_eq!(logged.action, "login");
        assert_eq!(agg.event_count(), 1);
    }

    #[test]
    fn test_get_event() {
        let agg = create_test_aggregator();
        let event = create_test_event("auth", "login", "user-1");
        let logged = agg.log_event(event).unwrap();
        let retrieved = agg.get_event(&logged.event_id).unwrap();
        assert_eq!(retrieved.event_id, logged.event_id);
        assert_eq!(retrieved.actor_id, "user-1");
    }

    #[test]
    fn test_get_nonexistent_event() {
        let agg = create_test_aggregator();
        assert!(agg.get_event("nonexistent").is_none());
    }

    #[test]
    fn test_query_events_by_category() {
        let agg = create_test_aggregator();
        agg.log_event(create_test_event("auth", "login", "user-1")).unwrap();
        agg.log_event(create_test_event("auth", "logout", "user-1")).unwrap();
        agg.log_event(create_test_event("inference", "predict", "user-2")).unwrap();

        let query = AuditQuery {
            category: Some("auth".to_string()),
            ..Default::default()
        };
        let results = agg.query_events(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_events_by_actor() {
        let agg = create_test_aggregator();
        agg.log_event(create_test_event("auth", "login", "user-1")).unwrap();
        agg.log_event(create_test_event("inference", "predict", "user-2")).unwrap();

        let query = AuditQuery {
            actor_id: Some("user-1".to_string()),
            ..Default::default()
        };
        let results = agg.query_events(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor_id, "user-1");
    }

    #[test]
    fn test_query_events_by_level() {
        let agg = create_test_aggregator();
        let mut event = create_test_event("auth", "login", "user-1");
        event.level = LogLevel::Error;
        agg.log_event(event).unwrap();

        let query = AuditQuery {
            level: Some("error".to_string()),
            ..Default::default()
        };
        let results = agg.query_events(&query);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_query_events_pagination() {
        let agg = create_test_aggregator();
        for i in 0..20 {
            agg.log_event(create_test_event("test", &format!("action-{}", i), "user-1"))
                .unwrap();
        }

        let query = AuditQuery {
            limit: Some(5),
            offset: Some(0),
            ..Default::default()
        };
        let results = agg.query_events(&query);
        assert_eq!(results.len(), 5);

        let query = AuditQuery {
            limit: Some(5),
            offset: Some(5),
            ..Default::default()
        };
        let results = agg.query_events(&query);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_aggregate_by_category() {
        let agg = create_test_aggregator();
        agg.log_event(create_test_event("auth", "login", "user-1")).unwrap();
        agg.log_event(create_test_event("auth", "logout", "user-2")).unwrap();
        agg.log_event(create_test_event("inference", "predict", "user-1")).unwrap();

        let aggregations = agg.aggregate_by_category();
        assert_eq!(aggregations.len(), 2);

        let auth_agg = aggregations.iter().find(|a| a.category == "auth").unwrap();
        assert_eq!(auth_agg.count, 2);
    }

    #[test]
    fn test_aggregate_by_actor() {
        let agg = create_test_aggregator();
        agg.log_event(create_test_event("auth", "login", "user-1")).unwrap();
        agg.log_event(create_test_event("inference", "predict", "user-1")).unwrap();
        agg.log_event(create_test_event("auth", "login", "user-2")).unwrap();

        let agg_result = agg.aggregate_by_actor("user-1");
        assert_eq!(agg_result.total_events, 2);
        assert_eq!(agg_result.by_category.get("auth").unwrap(), &1);
        assert_eq!(agg_result.by_category.get("inference").unwrap(), &1);
        assert!(agg_result.first_seen.is_some());
        assert!(agg_result.last_seen.is_some());
    }

    #[test]
    fn test_export_json() {
        let agg = create_test_aggregator();
        agg.log_event(create_test_event("auth", "login", "user-1")).unwrap();

        let query = AuditQuery::default();
        let json = agg.export_json(&query).unwrap();
        assert!(json.contains("auth"));
        assert!(json.contains("login"));
    }

    #[test]
    fn test_export_csv() {
        let agg = create_test_aggregator();
        agg.log_event(create_test_event("auth", "login", "user-1")).unwrap();

        let query = AuditQuery::default();
        let csv = agg.export_csv(&query).unwrap();
        assert!(csv.starts_with("event_id,timestamp,level,category"));
        assert!(csv.contains("auth"));
    }

    #[test]
    fn test_prune_old_events() {
        let agg = create_test_aggregator();

        // Create an event and manually backdate it
        let mut event = create_test_event("auth", "login", "user-1");
        event.timestamp = Utc::now() - chrono::Duration::days(200);
        agg.log_event(event).unwrap();

        // Create a recent event
        agg.log_event(create_test_event("auth", "login", "user-2")).unwrap();

        let pruned = agg.prune_old_events().unwrap();
        assert!(pruned >= 1);
        assert!(agg.event_count() >= 1);
    }
}
