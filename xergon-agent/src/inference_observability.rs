//! Inference Observability — Distributed Tracing for Inference Workloads
//!
//! Provides distributed tracing capabilities tailored for AI inference:
//! - TraceId / SpanId (UUID-based)
//! - Hierarchical spans with parent-child relationships
//! - Attribute recording (model_id, provider_id, token_count, etc.)
//! - Span events for lifecycle milestones
//! - Trace querying with filters (service, operation, status, time range, min_duration)
//! - Sampling-based trace collection
//! - Export for external analysis
//!
//! REST endpoints:
//! - GET  /v1/observability/traces              — List/query traces
//! - GET  /v1/observability/traces/{trace_id}   — Get single trace
//! - GET  /v1/observability/spans/{span_id}     — Get span by ID
//! - GET  /v1/observability/stats               — Observability statistics
//! - GET  /v1/observability/services            — Known services
//! - POST /v1/observability/export              — Export traces
//! - GET  /v1/observability/config              — Current config

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A unique trace identifier (UUID-based).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

impl TraceId {
    pub fn new() -> Self {
        TraceId(Uuid::new_v4().to_string())
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A unique span identifier (UUID-based).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub String);

impl SpanId {
    pub fn new() -> Self {
        SpanId(Uuid::new_v4().to_string())
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status of a span or trace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
}

/// An event recorded during a span's lifetime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub attributes: HashMap<String, serde_json::Value>,
}

/// A single span within a trace — represents a unit of work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: SpanId,
    pub parent_id: Option<SpanId>,
    pub operation_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<f64>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub status: SpanStatus,
    pub events: Vec<SpanEvent>,
    pub service_name: String,
}

impl Span {
    /// Create a new active span.
    pub fn new(operation_name: &str, service_name: &str, parent_id: Option<SpanId>) -> Self {
        Span {
            span_id: SpanId::new(),
            parent_id,
            operation_name: operation_name.to_string(),
            start_time: Utc::now(),
            end_time: None,
            duration_ms: None,
            attributes: HashMap::new(),
            status: SpanStatus::Ok,
            events: Vec::new(),
            service_name: service_name.to_string(),
        }
    }

    /// Mark the span as completed.
    pub fn finish(&mut self) {
        self.end_time = Some(Utc::now());
        if let Some(end) = self.end_time {
            self.duration_ms = Some((end - self.start_time).num_milliseconds() as f64);
        }
    }

    /// Record an event on this span.
    pub fn record_event(&mut self, name: &str, attributes: HashMap<String, serde_json::Value>) {
        self.events.push(SpanEvent {
            name: name.to_string(),
            timestamp: Utc::now(),
            attributes,
        });
    }

    /// Set a key-value attribute on this span.
    pub fn set_attribute(&mut self, key: &str, value: serde_json::Value) {
        self.attributes.insert(key.to_string(), value);
    }

    /// Mark the span as errored with an optional message.
    pub fn set_error(&mut self, message: &str) {
        self.status = SpanStatus::Error;
        self.attributes
            .insert("error.message".to_string(), serde_json::json!(message));
    }
}

/// A complete trace containing all spans for a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: TraceId,
    pub root_span: SpanId,
    pub spans: Vec<Span>,
    pub service_name: String,
    pub attributes: HashMap<String, serde_json::Value>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<f64>,
}

impl Trace {
    /// Create a new trace with a root span.
    pub fn new(service_name: &str) -> Self {
        Trace {
            trace_id: TraceId::new(),
            root_span: SpanId::new(),
            spans: Vec::new(),
            service_name: service_name.to_string(),
            attributes: HashMap::new(),
            start_time: Utc::now(),
            end_time: None,
            duration_ms: None,
        }
    }

    /// Compute overall duration from spans.
    pub fn compute_duration(&mut self) {
        if let Some(end) = self.end_time {
            self.duration_ms = Some((end - self.start_time).num_milliseconds() as f64);
        }
    }

    /// Get the overall status (Error if any span is errored).
    pub fn status(&self) -> &SpanStatus {
        for span in &self.spans {
            if span.status == SpanStatus::Error {
                return &SpanStatus::Error;
            }
        }
        &SpanStatus::Ok
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Attribute limits for spans and traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeLimits {
    pub max_attributes_per_span: usize,
    pub max_attribute_key_length: usize,
    pub max_attribute_value_length: usize,
    pub max_events_per_span: usize,
}

impl Default for AttributeLimits {
    fn default() -> Self {
        AttributeLimits {
            max_attributes_per_span: 128,
            max_attribute_key_length: 256,
            max_attribute_value_length: 4096,
            max_events_per_span: 64,
        }
    }
}

/// Configuration for the inference observability system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Sampling rate (0.0 to 1.0). 1.0 = record all traces.
    pub sampling_rate: f64,
    /// Maximum number of traces to retain in memory.
    pub max_traces: usize,
    /// Export interval in seconds.
    pub export_interval_secs: u64,
    /// Attribute size/count limits.
    pub attribute_limits: AttributeLimits,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        ObservabilityConfig {
            sampling_rate: 1.0,
            max_traces: 10000,
            export_interval_secs: 30,
            attribute_limits: AttributeLimits::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Observability statistics
// ---------------------------------------------------------------------------

/// Runtime statistics for the observability system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObservabilityStats {
    pub total_traces_created: u64,
    pub total_spans_created: u64,
    pub total_traces_exported: u64,
    pub total_traces_sampled_out: u64,
    pub active_traces: usize,
    pub completed_traces: usize,
    pub error_traces: usize,
    pub total_events_recorded: u64,
    pub services_seen: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// InferenceObservability — main manager
// ---------------------------------------------------------------------------

/// The central inference observability manager.
///
/// Stores completed traces in a DashMap for concurrent access.
/// Supports sampling, querying, and export.
pub struct InferenceObservability {
    /// Completed traces indexed by trace_id.
    traces: DashMap<String, Trace>,
    /// Index: service_name -> Vec<trace_id>
    service_index: DashMap<String, Vec<String>>,
    /// Index: operation_name -> Vec<trace_id>
    operation_index: DashMap<String, Vec<String>>,
    /// Index: span_id -> trace_id (for span lookup)
    span_trace_index: DashMap<String, String>,
    /// Configuration.
    config: Arc<tokio::sync::RwLock<ObservabilityConfig>>,
    /// Runtime statistics.
    #[allow(dead_code)]
    stats: ObservabilityStats,
    /// Atomic counters for lock-free updates.
    traces_created: AtomicU64,
    spans_created: AtomicU64,
    traces_exported: AtomicU64,
    traces_sampled_out: AtomicU64,
    events_recorded: AtomicU64,
}

impl InferenceObservability {
    /// Create a new observability manager with default config.
    pub fn new() -> Self {
        Self::with_config(ObservabilityConfig::default())
    }

    /// Create a new observability manager with the given config.
    pub fn with_config(config: ObservabilityConfig) -> Self {
        InferenceObservability {
            traces: DashMap::new(),
            service_index: DashMap::new(),
            operation_index: DashMap::new(),
            span_trace_index: DashMap::new(),
            config: Arc::new(tokio::sync::RwLock::new(config)),
            stats: ObservabilityStats::default(),
            traces_created: AtomicU64::new(0),
            spans_created: AtomicU64::new(0),
            traces_exported: AtomicU64::new(0),
            traces_sampled_out: AtomicU64::new(0),
            events_recorded: AtomicU64::new(0),
        }
    }

    /// Start a new trace. Returns (trace_id, root_span_id).
    /// Returns None if the trace is sampled out.
    pub async fn start_trace(
        &self,
        service_name: &str,
        operation_name: &str,
        attributes: HashMap<String, serde_json::Value>,
    ) -> Option<(TraceId, SpanId)> {
        let config = self.config.read().await;
        // Apply sampling
        if rand::random::<f64>() > config.sampling_rate {
            self.traces_sampled_out.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        drop(config);

        let trace_id = TraceId::new();
        let span_id = SpanId::new();

        let mut trace = Trace::new(service_name);
        trace.trace_id = trace_id.clone();
        trace.root_span = span_id.clone();
        trace.attributes = attributes;

        let mut root_span = Span::new(operation_name, service_name, None);
        root_span.span_id = span_id.clone();
        trace.spans.push(root_span);

        // Index
        let tid = trace_id.0.clone();
        let svc = service_name.to_string();
        let op = operation_name.to_string();

        self.service_index
            .entry(svc)
            .or_default()
            .push(tid.clone());
        self.operation_index
            .entry(op)
            .or_default()
            .push(tid.clone());
        self.span_trace_index
            .insert(span_id.0.clone(), tid.clone());

        self.traces.insert(tid, trace);
        self.traces_created.fetch_add(1, Ordering::Relaxed);
        self.spans_created.fetch_add(1, Ordering::Relaxed);

        Some((trace_id, span_id))
    }

    /// Start a new child span within an existing trace.
    pub async fn start_span(
        &self,
        trace_id: &TraceId,
        operation_name: &str,
        service_name: &str,
        parent_id: Option<&SpanId>,
    ) -> Option<SpanId> {
        let mut trace = self.traces.get_mut(&trace_id.0)?;
        let span_id = SpanId::new();
        let mut span = Span::new(operation_name, service_name, parent_id.cloned());
        span.span_id = span_id.clone();
        trace.spans.push(span);
        self.spans_created.fetch_add(1, Ordering::Relaxed);
        self.span_trace_index
            .insert(span_id.0.clone(), trace_id.0.clone());
        Some(span_id)
    }

    /// End a span by ID.
    pub async fn end_span(&self, trace_id: &TraceId, span_id: &SpanId) {
        if let Some(mut trace) = self.traces.get_mut(&trace_id.0) {
            for span in trace.spans.iter_mut() {
                if span.span_id == *span_id {
                    span.finish();
                    break;
                }
            }
            // If ending root span, finalize trace
            if trace.root_span == *span_id {
                trace.end_time = Some(Utc::now());
                trace.compute_duration();
            }
        }
    }

    /// End a trace (finalizes the root span and the trace itself).
    pub async fn end_trace(&self, trace_id: &TraceId) {
        if let Some(mut trace) = self.traces.get_mut(&trace_id.0) {
            for span in trace.spans.iter_mut() {
                if span.end_time.is_none() {
                    span.finish();
                }
            }
            trace.end_time = Some(Utc::now());
            trace.compute_duration();
        }
    }

    /// Record an event on a specific span.
    pub async fn record_event(
        &self,
        trace_id: &TraceId,
        span_id: &SpanId,
        event_name: &str,
        attributes: HashMap<String, serde_json::Value>,
    ) {
        if let Some(mut trace) = self.traces.get_mut(&trace_id.0) {
            for span in trace.spans.iter_mut() {
                if span.span_id == *span_id {
                    span.record_event(event_name, attributes);
                    self.events_recorded.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
    }

    /// Set an attribute on a specific span.
    pub async fn record_attribute(
        &self,
        trace_id: &TraceId,
        span_id: &SpanId,
        key: &str,
        value: serde_json::Value,
    ) {
        if let Some(mut trace) = self.traces.get_mut(&trace_id.0) {
            for span in trace.spans.iter_mut() {
                if span.span_id == *span_id {
                    span.set_attribute(key, value);
                    break;
                }
            }
        }
    }

    /// Mark a span as errored.
    pub async fn set_span_error(&self, trace_id: &TraceId, span_id: &SpanId, message: &str) {
        if let Some(mut trace) = self.traces.get_mut(&trace_id.0) {
            for span in trace.spans.iter_mut() {
                if span.span_id == *span_id {
                    span.set_error(message);
                    break;
                }
            }
        }
    }

    /// Get a trace by ID.
    pub fn get_trace(&self, trace_id: &TraceId) -> Option<Trace> {
        self.traces.get(&trace_id.0).map(|t| t.clone())
    }

    /// Get a span by ID (returns (span, trace_id)).
    pub fn get_span(&self, span_id: &SpanId) -> Option<(Span, TraceId)> {
        let trace_id_ref = self.span_trace_index.get(&span_id.0)?;
        let trace_id_str = trace_id_ref.value().clone();
        let trace = self.traces.get(&trace_id_str)?;
        for span in &trace.spans {
            if span.span_id == *span_id {
                return Some((span.clone(), TraceId(trace_id_str)));
            }
        }
        None
    }

    /// Query traces with optional filters.
    pub async fn query_traces(&self, query: TraceQuery) -> Vec<Trace> {
        let mut results: Vec<Trace> = Vec::new();

        for entry in self.traces.iter() {
            let trace = entry.value();
            // Filter by service
            if let Some(ref service) = query.service {
                if &trace.service_name != service {
                    continue;
                }
            }
            // Filter by operation (check root span)
            if let Some(ref operation) = query.operation {
                let root = trace.spans.iter().find(|s| s.span_id == trace.root_span);
                if let Some(root_span) = root {
                    if &root_span.operation_name != operation {
                        continue;
                    }
                }
            }
            // Filter by status
            if let Some(ref status) = query.status {
                let status_matches = match status.as_str() {
                    "ok" => *trace.status() == SpanStatus::Ok,
                    "error" => *trace.status() == SpanStatus::Error,
                    _ => false,
                };
                if !status_matches {
                    continue;
                }
            }
            // Filter by start time range
            if let Some(since) = query.since {
                if trace.start_time < since {
                    continue;
                }
            }
            if let Some(until) = query.until {
                if trace.start_time > until {
                    continue;
                }
            }
            // Filter by min duration
            if let Some(min_ms) = query.min_duration_ms {
                if let Some(dur) = trace.duration_ms {
                    if dur < min_ms {
                        continue;
                    }
                } else {
                    continue; // incomplete traces don't have duration
                }
            }
            results.push(trace.clone());
        }

        // Sort by start_time descending (most recent first)
        results.sort_by(|a, b| b.start_time.cmp(&a.start_time));

        // Apply limit
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        results
    }

    /// Get all known service names.
    pub fn get_services(&self) -> Vec<String> {
        self.service_index
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    /// Get observability statistics.
    pub fn get_stats(&self) -> ObservabilityStats {
        let mut stats = ObservabilityStats::default();
        stats.total_traces_created = self.traces_created.load(Ordering::Relaxed);
        stats.total_spans_created = self.spans_created.load(Ordering::Relaxed);
        stats.total_traces_exported = self.traces_exported.load(Ordering::Relaxed);
        stats.total_traces_sampled_out = self.traces_sampled_out.load(Ordering::Relaxed);
        stats.total_events_recorded = self.events_recorded.load(Ordering::Relaxed);
        stats.active_traces = self
            .traces
            .iter()
            .filter(|t| t.value().end_time.is_none())
            .count();
        stats.completed_traces = self
            .traces
            .iter()
            .filter(|t| t.value().end_time.is_some())
            .count();
        stats.error_traces = self
            .traces
            .iter()
            .filter(|t| t.value().status() == &SpanStatus::Error)
            .count();

        for entry in self.service_index.iter() {
            stats
                .services_seen
                .insert(entry.key().clone(), entry.value().len() as u64);
        }

        stats
    }

    /// Export traces matching a query.
    pub async fn export_traces(&self, query: TraceQuery) -> Vec<Trace> {
        let traces = self.query_traces(query).await;
        self.traces_exported.fetch_add(traces.len() as u64, Ordering::Relaxed);
        traces
    }

    /// Update the configuration.
    pub async fn update_config(&self, new_config: ObservabilityConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
    }

    /// Get current configuration.
    pub async fn get_config(&self) -> ObservabilityConfig {
        self.config.read().await.clone()
    }

    /// Get the number of stored traces.
    pub fn trace_count(&self) -> usize {
        self.traces.len()
    }

    /// Evict old traces if we exceed max_traces.
    pub async fn evict_old_traces(&self) {
        let config = self.config.read().await;
        if self.traces.len() <= config.max_traces {
            return;
        }
        drop(config);

        // Collect trace IDs sorted by start_time ascending (oldest first)
        let mut trace_times: Vec<(String, DateTime<Utc>)> = self
            .traces
            .iter()
            .map(|e| (e.key().clone(), e.value().start_time))
            .collect();
        trace_times.sort_by_key(|(_, t)| *t);

        let to_remove = self.traces.len() - self.config.read().await.max_traces;
        for (trace_id, _) in trace_times.into_iter().take(to_remove) {
            self.remove_trace_internal(&trace_id);
        }
    }

    /// Internal: remove a trace and clean up indexes.
    fn remove_trace_internal(&self, trace_id: &str) {
        if let Some((_, trace)) = self.traces.remove(trace_id) {
            for span in &trace.spans {
                self.span_trace_index.remove(&span.span_id.0);
            }
        }
    }

    /// Clear all traces.
    pub fn clear(&self) {
        self.traces.clear();
        self.service_index.clear();
        self.operation_index.clear();
        self.span_trace_index.clear();
    }
}

impl Default for InferenceObservability {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Query types
// ---------------------------------------------------------------------------

/// Query parameters for trace searching.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TraceQuery {
    pub service: Option<String>,
    pub operation: Option<String>,
    pub status: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub min_duration_ms: Option<f64>,
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// REST request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub service: Option<String>,
    pub operation: Option<String>,
    pub status: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub min_duration_ms: Option<f64>,
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// GET /v1/observability/traces
async fn list_traces_handler(
    State(obs): State<Arc<InferenceObservability>>,
    Query(query): Query<TraceQuery>,
) -> Json<serde_json::Value> {
    let traces = obs.query_traces(query).await;
    Json(serde_json::json!({
        "traces": traces,
        "count": traces.len(),
    }))
}

/// GET /v1/observability/traces/:trace_id
async fn get_trace_handler(
    State(obs): State<Arc<InferenceObservability>>,
    Path(trace_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let trace = obs
        .get_trace(&TraceId(trace_id))
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({ "trace": trace })))
}

/// GET /v1/observability/spans/:span_id
async fn get_span_handler(
    State(obs): State<Arc<InferenceObservability>>,
    Path(span_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (span, trace_id) = obs.get_span(&SpanId(span_id)).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "span": span,
        "trace_id": trace_id.0,
    })))
}

/// GET /v1/observability/stats
async fn get_stats_handler(
    State(obs): State<Arc<InferenceObservability>>,
) -> Json<serde_json::Value> {
    let stats = obs.get_stats();
    Json(serde_json::json!({ "stats": stats }))
}

/// GET /v1/observability/services
async fn get_services_handler(
    State(obs): State<Arc<InferenceObservability>>,
) -> Json<serde_json::Value> {
    let services = obs.get_services();
    Json(serde_json::json!({ "services": services }))
}

/// POST /v1/observability/export
async fn export_handler(
    State(obs): State<Arc<InferenceObservability>>,
    Json(body): Json<ExportRequest>,
) -> Json<serde_json::Value> {
    let since = body
        .since
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let until = body
        .until
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let query = TraceQuery {
        service: body.service,
        operation: body.operation,
        status: body.status,
        since,
        until,
        min_duration_ms: body.min_duration_ms,
        limit: body.limit,
    };

    let traces = obs.export_traces(query).await;
    Json(serde_json::json!({
        "exported": traces.len(),
        "traces": traces,
    }))
}

/// GET /v1/observability/config
async fn get_config_handler(
    State(obs): State<Arc<InferenceObservability>>,
) -> Json<serde_json::Value> {
    let config = obs.get_config().await;
    Json(serde_json::json!({ "config": config }))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the inference observability router.
pub fn build_inference_observability_router(
    state: crate::api::AppState,
) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/v1/observability/traces", get(list_traces_handler))
        .route(
            "/v1/observability/traces/{trace_id}",
            get(get_trace_handler),
        )
        .route(
            "/v1/observability/spans/{span_id}",
            get(get_span_handler),
        )
        .route("/v1/observability/stats", get(get_stats_handler))
        .route("/v1/observability/services", get(get_services_handler))
        .route("/v1/observability/export", post(export_handler))
        .route("/v1/observability/config", get(get_config_handler))
        .with_state(state.inference_observability.clone())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_obs() -> Arc<InferenceObservability> {
        Arc::new(InferenceObservability::new())
    }

    #[tokio::test]
    async fn test_start_and_end_trace() {
        let obs = make_obs();
        let (trace_id, span_id) = obs
            .start_trace("test-svc", "root-op", HashMap::new())
            .await
            .expect("trace should start");
        obs.end_span(&trace_id, &span_id).await;
        let trace = obs.get_trace(&trace_id).expect("trace should exist");
        assert!(trace.end_time.is_some());
        assert!(trace.duration_ms.is_some());
    }

    #[tokio::test]
    async fn test_start_child_span() {
        let obs = make_obs();
        let (trace_id, root_span_id) = obs
            .start_trace("svc", "root", HashMap::new())
            .await
            .unwrap();
        let child_id = obs
            .start_span(&trace_id, "child-op", "svc", Some(&root_span_id))
            .await
            .expect("child span should start");
        let trace = obs.get_trace(&trace_id).unwrap();
        assert_eq!(trace.spans.len(), 2);
        let child = trace.spans.iter().find(|s| s.span_id == child_id).unwrap();
        assert_eq!(child.parent_id, Some(root_span_id.clone()));
    }

    #[tokio::test]
    async fn test_record_event() {
        let obs = make_obs();
        let (trace_id, span_id) = obs
            .start_trace("svc", "op", HashMap::new())
            .await
            .unwrap();
        let mut attrs = HashMap::new();
        attrs.insert("key".to_string(), serde_json::json!("value"));
        obs.record_event(&trace_id, &span_id, "test-event", attrs).await;
        let trace = obs.get_trace(&trace_id).unwrap();
        assert_eq!(trace.spans[0].events.len(), 1);
        assert_eq!(trace.spans[0].events[0].name, "test-event");
    }

    #[tokio::test]
    async fn test_record_attribute() {
        let obs = make_obs();
        let (trace_id, span_id) = obs
            .start_trace("svc", "op", HashMap::new())
            .await
            .unwrap();
        obs.record_attribute(
            &trace_id,
            &span_id,
            "model_id",
            serde_json::json!("llama-3-70b"),
        )
        .await;
        let trace = obs.get_trace(&trace_id).unwrap();
        assert_eq!(
            trace.spans[0].attributes.get("model_id").unwrap(),
            &serde_json::json!("llama-3-70b")
        );
    }

    #[tokio::test]
    async fn test_span_error_status() {
        let obs = make_obs();
        let (trace_id, span_id) = obs
            .start_trace("svc", "op", HashMap::new())
            .await
            .unwrap();
        obs.set_span_error(&trace_id, &span_id, "timeout").await;
        let trace = obs.get_trace(&trace_id).unwrap();
        assert_eq!(trace.spans[0].status, SpanStatus::Error);
        assert_eq!(trace.status(), &SpanStatus::Error);
    }

    #[tokio::test]
    async fn test_get_span_by_id() {
        let obs = make_obs();
        let (trace_id, span_id) = obs
            .start_trace("svc", "op", HashMap::new())
            .await
            .unwrap();
        let (span, found_trace_id) = obs.get_span(&span_id).unwrap();
        assert_eq!(span.span_id, span_id);
        assert_eq!(found_trace_id, trace_id);
    }

    #[tokio::test]
    async fn test_query_by_service() {
        let obs = make_obs();
        let (t1, s1) = obs
            .start_trace("svc-a", "op", HashMap::new())
            .await
            .unwrap();
        let (t2, s2) = obs
            .start_trace("svc-b", "op", HashMap::new())
            .await
            .unwrap();
        obs.end_span(&t1, &s1).await;
        obs.end_span(&t2, &s2).await;

        let results = obs
            .query_traces(TraceQuery {
                service: Some("svc-a".to_string()),
                ..Default::default()
            })
            .await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].service_name, "svc-a");
    }

    #[tokio::test]
    async fn test_query_by_status() {
        let obs = make_obs();
        let (t1, s1) = obs
            .start_trace("svc", "ok-op", HashMap::new())
            .await
            .unwrap();
        let (t2, s2) = obs
            .start_trace("svc", "err-op", HashMap::new())
            .await
            .unwrap();
        obs.end_span(&t1, &s1).await;
        obs.set_span_error(&t2, &s2, "fail").await;
        obs.end_span(&t2, &s2).await;

        let results = obs
            .query_traces(TraceQuery {
                status: Some("error".to_string()),
                ..Default::default()
            })
            .await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_query_with_limit() {
        let obs = make_obs();
        for i in 0..5 {
            let (tid, sid) = obs
                .start_trace("svc", &format!("op-{}", i), HashMap::new())
                .await
                .unwrap();
            obs.end_span(&tid, &sid).await;
        }
        let results = obs
            .query_traces(TraceQuery {
                limit: Some(2),
                ..Default::default()
            })
            .await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_sampling() {
        let config = ObservabilityConfig {
            sampling_rate: 0.0, // sample nothing
            ..Default::default()
        };
        let obs = Arc::new(InferenceObservability::with_config(config));
        let result = obs
            .start_trace("svc", "op", HashMap::new())
            .await;
        assert!(result.is_none());
        let stats = obs.get_stats();
        assert_eq!(stats.total_traces_sampled_out, 1);
    }

    #[tokio::test]
    async fn test_export_traces() {
        let obs = make_obs();
        let (t1, s1) = obs
            .start_trace("svc", "op", HashMap::new())
            .await
            .unwrap();
        obs.end_span(&t1, &s1).await;

        let exported = obs
            .export_traces(TraceQuery {
                service: Some("svc".to_string()),
                ..Default::default()
            })
            .await;
        assert_eq!(exported.len(), 1);
        let stats = obs.get_stats();
        assert_eq!(stats.total_traces_exported, 1);
    }

    #[tokio::test]
    async fn test_get_services() {
        let obs = make_obs();
        obs.start_trace("svc-a", "op", HashMap::new()).await.unwrap();
        obs.start_trace("svc-b", "op", HashMap::new()).await.unwrap();
        let services = obs.get_services();
        assert!(services.contains(&"svc-a".to_string()));
        assert!(services.contains(&"svc-b".to_string()));
    }

    #[tokio::test]
    async fn test_end_trace_closes_all_spans() {
        let obs = make_obs();
        let (trace_id, root_id) = obs
            .start_trace("svc", "root", HashMap::new())
            .await
            .unwrap();
        obs.start_span(&trace_id, "child", "svc", Some(&root_id))
            .await
            .unwrap();
        obs.end_trace(&trace_id).await;
        let trace = obs.get_trace(&trace_id).unwrap();
        for span in &trace.spans {
            assert!(span.end_time.is_some());
        }
    }

    #[tokio::test]
    async fn test_config_update() {
        let obs = make_obs();
        let new_config = ObservabilityConfig {
            sampling_rate: 0.5,
            max_traces: 500,
            export_interval_secs: 60,
            attribute_limits: AttributeLimits {
                max_attributes_per_span: 64,
                max_attribute_key_length: 128,
                max_attribute_value_length: 2048,
                max_events_per_span: 32,
            },
        };
        obs.update_config(new_config.clone()).await;
        let read = obs.get_config().await;
        assert_eq!(read.sampling_rate, 0.5);
        assert_eq!(read.max_traces, 500);
    }

    #[tokio::test]
    async fn test_evict_old_traces() {
        let config = ObservabilityConfig {
            max_traces: 2,
            ..Default::default()
        };
        let obs = Arc::new(InferenceObservability::with_config(config));
        let mut tids = Vec::new();
        for _ in 0..5 {
            let (tid, sid) = obs
                .start_trace("svc", "op", HashMap::new())
                .await
                .unwrap();
            obs.end_span(&tid, &sid).await;
            tids.push(tid);
        }
        obs.evict_old_traces().await;
        assert_eq!(obs.trace_count(), 2);
    }
}
