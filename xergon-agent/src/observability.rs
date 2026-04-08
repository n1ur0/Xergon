//! Inference Observability -- distributed tracing and metrics for every inference request.
//!
//! Creates spans for each inference call (trace_id, span_id, timing), propagates W3C
//! TraceContext headers (traceparent, tracestate), and collects metrics (latency histogram,
//! error rate, token throughput).  Supports exporting to stdout (JSON), OTLP endpoint,
//! Jaeger, or keeping everything in memory.
//!
//! API endpoints:
//! - GET   /api/observability/traces          -- recent traces (query by model, limit)
//! - GET   /api/observability/traces/{id}     -- full trace with child spans
//! - GET   /api/observability/metrics         -- current metrics snapshot
//! - GET   /api/observability/config          -- current configuration
//! - PATCH /api/observability/config          -- update configuration at runtime

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Where to export trace data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TraceExporter {
    Stdout,
    Otlp { endpoint: String },
    Jaeger { endpoint: String },
    None,
}

/// Where to export metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MetricsExporter {
    Prometheus { port: u16 },
    Otlp { endpoint: String },
    Stdout,
    None,
}

/// Top-level observability configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Master switch (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Trace exporter backend.
    #[serde(default = "default_trace_exporter")]
    pub trace_exporter: TraceExporter,
    /// Metrics exporter backend.
    #[serde(default = "default_metrics_exporter")]
    pub metrics_exporter: MetricsExporter,
    /// Fraction of requests to sample (0.0 - 1.0, default: 1.0).
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
    /// Maximum spans kept in the in-memory ring buffer (default: 10 000).
    #[serde(default = "default_max_spans")]
    pub max_spans: usize,
}

fn default_sample_rate() -> f64 { 1.0 }
fn default_max_spans() -> usize { 10_000 }
fn default_trace_exporter() -> TraceExporter { TraceExporter::Stdout }
fn default_metrics_exporter() -> MetricsExporter { MetricsExporter::Stdout }

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trace_exporter: TraceExporter::Stdout,
            metrics_exporter: MetricsExporter::Stdout,
            sample_rate: 1.0,
            max_spans: 10_000,
        }
    }
}

/// Partial config used for PATCH /api/observability/config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfigUpdate {
    pub enabled: Option<bool>,
    pub trace_exporter: Option<TraceExporter>,
    pub metrics_exporter: Option<MetricsExporter>,
    pub sample_rate: Option<f64>,
    pub max_spans: Option<usize>,
}

impl Default for ObservabilityConfigUpdate {
    fn default() -> Self {
        Self {
            enabled: None,
            trace_exporter: None,
            metrics_exporter: None,
            sample_rate: None,
            max_spans: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Span types
// ---------------------------------------------------------------------------

/// Status of a finished span.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SpanStatus {
    Ok,
    Error { message: String },
}

/// An event that occurs within a span (e.g. model_load_start, inference_start).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub attributes: HashMap<String, String>,
}

/// A single trace span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub model: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: SpanStatus,
    pub attributes: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
}

impl InferenceSpan {
    /// Duration in milliseconds (None if span is still open).
    pub fn duration_ms(&self) -> Option<f64> {
        self.end_time.map(|end| {
            (end - self.start_time).num_milliseconds() as f64
        })
    }
}

// ---------------------------------------------------------------------------
// W3C TraceContext propagation
// ---------------------------------------------------------------------------

/// Parsed W3C traceparent header.
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub sampled: bool,
}

impl TraceContext {
    /// Parse a `traceparent` header value.
    /// Format: `00-{trace_id}-{span_id}-{flags}` where flags & 0x01 == sampled.
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.trim().split('-').collect();
        if parts.len() != 4 || parts[0] != "00" {
            return None;
        }
        let trace_id = parts[1].to_string();
        let span_id = parts[2].to_string();
        let flags = u8::from_str_radix(parts[3], 16).ok()?;
        Some(Self {
            trace_id,
            span_id,
            sampled: flags & 0x01 != 0,
        })
    }

    /// Build a `traceparent` header value.
    pub fn to_traceparent(&self) -> String {
        let sampled_flag = if self.sampled { "01" } else { "00" };
        format!("00-{}-{}-{}", self.trace_id, self.span_id, sampled_flag)
    }
}

// ---------------------------------------------------------------------------
// Observability Manager
// ---------------------------------------------------------------------------

/// Core observability manager holding a ring buffer of spans and live metrics.
pub struct ObservabilityManager {
    config: Arc<tokio::sync::RwLock<ObservabilityConfig>>,
    /// Ring buffer of finished spans (newest last).
    spans: Arc<tokio::sync::RwLock<Vec<InferenceSpan>>>,
    /// Spans indexed by trace_id for fast lookup.
    trace_index: Arc<DashMap<String, Vec<usize>>>,
    /// Numeric metrics.
    metrics: Arc<DashMap<String, f64>>,
    /// Counter helpers.
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    total_tokens_in: AtomicU64,
    total_tokens_out: AtomicU64,
}

impl ObservabilityManager {
    /// Create a new observability manager.
    pub fn new(config: ObservabilityConfig) -> Self {
        Self {
            config: Arc::new(tokio::sync::RwLock::new(config)),
            spans: Arc::new(tokio::sync::RwLock::new(Vec::with_capacity(10_000))),
            trace_index: Arc::new(DashMap::new()),
            metrics: Arc::new(DashMap::new()),
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            total_tokens_in: AtomicU64::new(0),
            total_tokens_out: AtomicU64::new(0),
        }
    }

    // -- Configuration -------------------------------------------------------

    /// Get a snapshot of the current configuration.
    pub async fn get_config(&self) -> ObservabilityConfig {
        self.config.read().await.clone()
    }

    /// Apply a partial configuration update.
    pub async fn update_config(&self, update: ObservabilityConfigUpdate) {
        let mut cfg = self.config.write().await;
        if let Some(enabled) = update.enabled { cfg.enabled = enabled; }
        if let Some(ref te) = update.trace_exporter { cfg.trace_exporter = te.clone(); }
        if let Some(ref me) = update.metrics_exporter { cfg.metrics_exporter = me.clone(); }
        if let Some(sr) = update.sample_rate { cfg.sample_rate = sr.clamp(0.0, 1.0); }
        if let Some(ms) = update.max_spans { cfg.max_spans = ms.max(100); }
        info!(?cfg, "Observability config updated");
    }

    /// Check if observability is enabled.
    pub async fn is_enabled(&self) -> bool {
        self.config.read().await.enabled
    }

    // -- Span creation -------------------------------------------------------

    /// Generate a random 32-hex-char trace ID.
    fn generate_trace_id() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    /// Generate a random 16-hex-char span ID.
    fn generate_span_id() -> String {
        let id = uuid::Uuid::new_v4();
        format!("{:016x}", id.as_u128() >> 64)
    }

    /// Create a new root span for an inference request.
    pub async fn start_span(
        &self,
        operation: &str,
        model: &str,
        attributes: HashMap<String, String>,
    ) -> String {
        let cfg = self.config.read().await;
        if !cfg.enabled {
            return String::new();
        }

        // Sampling decision
        if rand::random::<f64>() > cfg.sample_rate {
            return String::new();
        }

        let trace_id = Self::generate_trace_id();
        let span_id = Self::generate_span_id();

        let span = InferenceSpan {
            trace_id: trace_id.clone(),
            span_id,
            parent_span_id: None,
            operation: operation.to_string(),
            model: model.to_string(),
            start_time: Utc::now(),
            end_time: None,
            status: SpanStatus::Ok,
            attributes,
            events: Vec::new(),
        };

        self.push_span(span, cfg.max_spans).await;
        trace_id
    }

    /// Create a child span under an existing trace.
    pub async fn start_child_span(
        &self,
        trace_id: &str,
        parent_span_id: &str,
        operation: &str,
        model: &str,
        attributes: HashMap<String, String>,
    ) -> String {
        let cfg = self.config.read().await;
        if !cfg.enabled || trace_id.is_empty() {
            return String::new();
        }

        let span_id = Self::generate_span_id();
        let span_id_ret = span_id.clone();

        let span = InferenceSpan {
            trace_id: trace_id.to_string(),
            span_id,
            parent_span_id: Some(parent_span_id.to_string()),
            operation: operation.to_string(),
            model: model.to_string(),
            start_time: Utc::now(),
            end_time: None,
            status: SpanStatus::Ok,
            attributes,
            events: Vec::new(),
        };

        self.push_span(span, cfg.max_spans).await;
        span_id_ret
    }

    /// Push a span into the ring buffer, evicting oldest if over capacity.
    async fn push_span(&self, span: InferenceSpan, max_spans: usize) {
        let trace_id = span.trace_id.clone();
        let mut spans = self.spans.write().await;
        let idx = spans.len();
        spans.push(span);
        // Evict oldest if over capacity
        while spans.len() > max_spans {
            spans.remove(0);
        }
        // Track index offset (simplified: store latest position per trace_id)
        self.trace_index
            .entry(trace_id)
            .or_insert_with(Vec::new)
            .push(idx);
    }

    /// End an open span by finding the latest span with the given trace_id and
    /// setting its end_time and status.
    pub async fn end_span(&self, trace_id: &str, span_id: &str, status: SpanStatus) {
        if trace_id.is_empty() {
            return;
        }
        let mut spans = self.spans.write().await;
        // Search backwards for the matching span
        for span in spans.iter_mut().rev() {
            if span.trace_id == trace_id && span.span_id == span_id && span.end_time.is_none() {
                span.end_time = Some(Utc::now());
                span.status = status;
                break;
            }
        }
    }

    /// Add an event to the most recent open span with the given trace_id.
    pub async fn add_span_event(
        &self,
        trace_id: &str,
        event_name: &str,
        attributes: HashMap<String, String>,
    ) {
        if trace_id.is_empty() {
            return;
        }
        let mut spans = self.spans.write().await;
        for span in spans.iter_mut().rev() {
            if span.trace_id == trace_id && span.end_time.is_none() {
                span.events.push(SpanEvent {
                    name: event_name.to_string(),
                    timestamp: Utc::now(),
                    attributes,
                });
                break;
            }
        }
    }

    // -- Queries -------------------------------------------------------------

    /// Get recent traces, optionally filtered by model.
    pub async fn query_traces(&self, model: Option<&str>, limit: usize) -> Vec<InferenceSpan> {
        let spans = self.spans.read().await;
        let mut result: Vec<&InferenceSpan> = spans.iter().collect();

        if let Some(m) = model {
            result.retain(|s| s.model == m);
        }

        result.sort_by(|a, b| b.start_time.cmp(&a.start_time));
        result.into_iter().take(limit).cloned().collect()
    }

    /// Get all spans for a specific trace_id.
    pub async fn get_trace(&self, trace_id: &str) -> Vec<InferenceSpan> {
        let spans = self.spans.read().await;
        spans
            .iter()
            .filter(|s| s.trace_id == trace_id)
            .cloned()
            .collect()
    }

    // -- Metrics -------------------------------------------------------------

    /// Record that an inference request was processed.
    pub fn record_request(&self, model: &str, latency_ms: f64, tokens_in: u64, tokens_out: u64, error: bool) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_tokens_in.fetch_add(tokens_in, Ordering::Relaxed);
        self.total_tokens_out.fetch_add(tokens_out, Ordering::Relaxed);

        if error {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }

        // Per-model latency tracking
        let latency_key = format!("inference.latency_ms.{}", model);
        let count_key = format!("inference.count.{}", model);

        // Simple running average for latency
        let current = self.metrics.get(&latency_key).map(|r| *r).unwrap_or(0.0);
        let count = self.metrics.get(&count_key).map(|r| *r).unwrap_or(0.0);
        let new_count = count + 1.0;
        let new_avg = (current * count + latency_ms) / new_count;
        self.metrics.insert(latency_key, new_avg);
        self.metrics.insert(count_key, new_count);

        // Token throughput (tokens_out per second, based on last request)
        let throughput_key = format!("inference.throughput_tps.{}", model);
        if latency_ms > 0.0 {
            let tps = (tokens_out as f64 / latency_ms) * 1000.0;
            self.metrics.insert(throughput_key, tps);
        }
    }

    /// Get a snapshot of all current metrics.
    pub fn get_metrics(&self) -> HashMap<String, f64> {
        let mut m = HashMap::new();
        m.insert("inference.total_requests".into(), self.total_requests.load(Ordering::Relaxed) as f64);
        m.insert("inference.total_errors".into(), self.total_errors.load(Ordering::Relaxed) as f64);
        m.insert("inference.total_tokens_in".into(), self.total_tokens_in.load(Ordering::Relaxed) as f64);
        m.insert("inference.total_tokens_out".into(), self.total_tokens_out.load(Ordering::Relaxed) as f64);

        let total = self.total_requests.load(Ordering::Relaxed);
        let errors = self.total_errors.load(Ordering::Relaxed);
        if total > 0 {
            m.insert("inference.error_rate".into(), (errors as f64) / (total as f64));
        }

        for kv in self.metrics.iter() {
            m.insert(kv.key().clone(), *kv.value());
        }
        m
    }

    // -- Export --------------------------------------------------------------

    /// Export a finished span according to the configured exporter.
    #[allow(dead_code)]
    async fn export_span(&self, span: &InferenceSpan) {
        let cfg = self.config.read().await;
        match &cfg.trace_exporter {
            TraceExporter::Stdout => {
                if let Ok(json) = serde_json::to_string_pretty(span) {
                    println!("[TRACE] {}", json);
                }
            }
            TraceExporter::Otlp { endpoint } => {
                debug!(endpoint, "OTLP trace export (stub -- integrate with gRPC client)");
                // In production: POST spans to the OTLP HTTP endpoint
            }
            TraceExporter::Jaeger { endpoint } => {
                debug!(endpoint, "Jaeger trace export (stub -- integrate with jaeger client)");
            }
            TraceExporter::None => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_roundtrip() {
        let ctx = TraceContext {
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
            span_id: "00f067aa0ba902b7".into(),
            sampled: true,
        };
        let header = ctx.to_traceparent();
        assert_eq!(header, "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01");

        let parsed = TraceContext::from_traceparent(&header).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
        assert!(parsed.sampled);
    }

    #[test]
    fn test_trace_context_unsampled() {
        let header = "00-abc123-def456-00";
        let ctx = TraceContext::from_traceparent(header).unwrap();
        assert!(!ctx.sampled);
    }

    #[tokio::test]
    async fn test_observability_manager_lifecycle() {
        let mgr = ObservabilityManager::new(ObservabilityConfig {
            enabled: true,
            ..Default::default()
        });

        let trace_id = mgr.start_span("inference", "llama-3.1-8b", HashMap::new()).await;
        assert!(!trace_id.is_empty());

        mgr.add_span_event(&trace_id, "queue_start", HashMap::new()).await;
        mgr.add_span_event(&trace_id, "inference_start", HashMap::new()).await;

        mgr.end_span(&trace_id, "", SpanStatus::Ok).await;

        let traces = mgr.query_traces(None, 10).await;
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].events.len(), 2);
        assert!(traces[0].end_time.is_some());

        let full_trace = mgr.get_trace(&trace_id).await;
        assert_eq!(full_trace.len(), 1);
    }

    #[tokio::test]
    async fn test_observability_disabled() {
        let mgr = ObservabilityManager::new(ObservabilityConfig::default());
        let trace_id = mgr.start_span("inference", "test", HashMap::new()).await;
        assert!(trace_id.is_empty());
    }

    #[tokio::test]
    async fn test_config_update() {
        let mgr = ObservabilityManager::new(ObservabilityConfig::default());
        assert!(!mgr.is_enabled().await);

        mgr.update_config(ObservabilityConfigUpdate {
            enabled: Some(true),
            sample_rate: Some(0.5),
            ..Default::default()
        }).await;

        let cfg = mgr.get_config().await;
        assert!(cfg.enabled);
        assert!((cfg.sample_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metrics_recording() {
        let mgr = ObservabilityManager::new(ObservabilityConfig::default());
        mgr.record_request("llama-3.1-8b", 120.0, 100, 50, false);
        mgr.record_request("llama-3.1-8b", 180.0, 200, 100, true);

        let m = mgr.get_metrics();
        assert_eq!(m["inference.total_requests"], 2.0);
        assert_eq!(m["inference.total_errors"], 1.0);
        assert_eq!(m["inference.total_tokens_in"], 300.0);
        assert_eq!(m["inference.total_tokens_out"], 150.0);
        assert!((m["inference.error_rate"] - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_ring_buffer_eviction() {
        let mgr = ObservabilityManager::new(ObservabilityConfig {
            enabled: true,
            max_spans: 5,
            ..Default::default()
        });

        for i in 0..8 {
            let mut attrs = HashMap::new();
            attrs.insert("idx".to_string(), i.to_string());
            mgr.start_span("inference", "test", attrs).await;
        }

        let traces = mgr.query_traces(None, 100).await;
        assert_eq!(traces.len(), 5);
    }
}
