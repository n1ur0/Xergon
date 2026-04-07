//! Audit Logging for the Xergon relay.
//!
//! Provides an in-memory ring buffer of audit events with querying and
//! aggregation capabilities. All admin/privileged actions are logged.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single audit log entry.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub action: String,
    pub actor: String,
    pub resource_type: String,
    pub resource_id: String,
    pub details: serde_json::Value,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

/// Ring buffer configuration.
pub struct AuditConfig {
    pub max_events: usize,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self { max_events: 10_000 }
    }
}

// ---------------------------------------------------------------------------
// Audit logger
// ---------------------------------------------------------------------------

/// In-memory audit log backed by a ring buffer.
#[derive(Clone)]
pub struct AuditLogger {
    events: Arc<StdMutex<VecDeque<AuditEvent>>>,
    max_events: usize,
}

impl AuditLogger {
    pub fn new() -> Self {
        Self::with_config(AuditConfig::default())
    }

    pub fn with_config(config: AuditConfig) -> Self {
        Self {
            events: Arc::new(StdMutex::new(VecDeque::with_capacity(config.max_events))),
            max_events: config.max_events,
        }
    }

    /// Record an audit event.
    pub fn log(
        &self,
        action: &str,
        actor: &str,
        resource_type: &str,
        resource_id: &str,
        details: serde_json::Value,
        ip: Option<String>,
        user_agent: Option<String>,
    ) {
        let event = AuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            action: action.to_string(),
            actor: actor.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            details,
            ip,
            user_agent,
        };

        debug!(
            id = %event.id,
            action = %event.action,
            actor = %event.actor,
            "Audit event logged"
        );

        let mut events = self.events.lock().unwrap();
        events.push_back(event);
        while events.len() > self.max_events {
            events.pop_front();
        }
    }

    /// Query audit logs with optional filters.
    pub fn query(&self, params: &AuditQueryParams) -> Vec<AuditEvent> {
        let events = self.events.lock().unwrap();
        let mut results: Vec<&AuditEvent> = events
            .iter()
            .filter(|e| {
                // Filter by action
                if let Some(ref action_filter) = params.action {
                    if e.action != *action_filter {
                        return false;
                    }
                }
                // Filter by actor
                if let Some(ref actor_filter) = params.actor {
                    if e.actor != *actor_filter {
                        return false;
                    }
                }
                // Filter by resource_type
                if let Some(ref rt_filter) = params.resource_type {
                    if e.resource_type != *rt_filter {
                        return false;
                    }
                }
                // Filter by since timestamp
                if let Some(since) = params.since {
                    if e.timestamp < since {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Most recent first
        results.reverse();
        results.truncate(params.limit);
        results.into_iter().cloned().collect()
    }

    /// Get statistics: action counts and top actors.
    pub fn stats(&self) -> AuditStats {
        let events = self.events.lock().unwrap();

        let mut action_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let mut actor_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();

        for event in events.iter() {
            *action_counts.entry(event.action.clone()).or_insert(0) += 1;
            *actor_counts.entry(event.actor.clone()).or_insert(0) += 1;
        }

        // Top actors sorted by count descending
        let mut top_actors: Vec<(String, u64)> = actor_counts.into_iter().collect();
        top_actors.sort_by(|a, b| b.1.cmp(&a.1));

        let total_events = events.len();
        AuditStats {
            total_events,
            action_counts,
            top_actors,
        }
    }

    /// Current number of events in the buffer.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

/// Query parameters for audit log filtering.
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AuditQueryParams {
    pub action: Option<String>,
    pub actor: Option<String>,
    pub resource_type: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "audit_default_limit")]
    pub limit: usize,
}

fn audit_default_limit() -> usize {
    100
}

// ---------------------------------------------------------------------------
// Audit categories
// ---------------------------------------------------------------------------

/// Category of an audit event for filtering and organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditCategory {
    /// General request audit.
    Request,
    /// Authentication and authorization events.
    Auth,
    /// Compliance and regulatory events.
    Compliance,
    /// Unclassified / legacy events.
    General,
}

impl std::fmt::Display for AuditCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditCategory::Request => write!(f, "request"),
            AuditCategory::Auth => write!(f, "auth"),
            AuditCategory::Compliance => write!(f, "compliance"),
            AuditCategory::General => write!(f, "general"),
        }
    }
}

// ---------------------------------------------------------------------------
// Typed audit entries
// ---------------------------------------------------------------------------

/// Typed audit entry for request-level logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestAuditEntry {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub category: AuditCategory,
    pub method: String,
    pub path: String,
    pub status_code: u16,
    pub request_id: String,
    pub model: Option<String>,
    pub provider_id: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub duration_ms: u64,
    pub tokens_in: Option<u32>,
    pub tokens_out: Option<u32>,
    pub error: Option<String>,
}

/// Typed audit entry for authentication events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthAuditEntry {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub category: AuditCategory,
    pub action: String,
    pub actor: String,
    pub success: bool,
    pub api_key_id: Option<String>,
    pub ip: Option<String>,
    pub reason: Option<String>,
    pub details: serde_json::Value,
}

/// Typed audit entry for compliance events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAuditEntry {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub category: AuditCategory,
    pub event_type: String,
    pub actor: String,
    pub resource_type: String,
    pub resource_id: String,
    pub outcome: String,
    pub details: serde_json::Value,
    pub retention_days: Option<u32>,
}

// ---------------------------------------------------------------------------
// Expanded audit buffers with typed methods
// ---------------------------------------------------------------------------

/// Ring buffer for typed request audit entries.
#[derive(Clone)]
pub struct RequestAuditBuffer {
    entries: Arc<StdMutex<VecDeque<RequestAuditEntry>>>,
    max_entries: usize,
}

impl RequestAuditBuffer {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(StdMutex::new(VecDeque::with_capacity(max_entries))),
            max_entries,
        }
    }

    pub fn push(&self, entry: RequestAuditEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.push_back(entry);
        while entries.len() > self.max_entries {
            entries.pop_front();
        }
    }

    pub fn query(&self, params: &RequestAuditQueryParams) -> Vec<RequestAuditEntry> {
        let entries = self.entries.lock().unwrap();
        let mut results: Vec<&RequestAuditEntry> = entries
            .iter()
            .filter(|e| {
                if let Some(ref path) = params.path {
                    if !e.path.contains(path) { return false; }
                }
                if let Some(ref method) = params.method {
                    if e.method != *method { return false; }
                }
                if let Some(min_status) = params.min_status {
                    if e.status_code < min_status { return false; }
                }
                if let Some(since) = params.since {
                    if e.timestamp < since { return false; }
                }
                if let Some(ref model) = params.model {
                    if e.model.as_ref() != Some(model) { return false; }
                }
                true
            })
            .collect();
        results.reverse();
        results.truncate(params.limit);
        results.into_iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RequestAuditQueryParams {
    pub path: Option<String>,
    pub method: Option<String>,
    pub min_status: Option<u16>,
    pub model: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "audit_default_limit")]
    pub limit: usize,
}

/// Ring buffer for typed auth audit entries.
#[derive(Clone)]
pub struct AuthAuditBuffer {
    entries: Arc<StdMutex<VecDeque<AuthAuditEntry>>>,
    max_events: usize,
}

impl AuthAuditBuffer {
    pub fn new(max_events: usize) -> Self {
        Self {
            entries: Arc::new(StdMutex::new(VecDeque::with_capacity(max_events))),
            max_events,
        }
    }

    pub fn push(&self, entry: AuthAuditEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.push_back(entry);
        while entries.len() > self.max_events {
            entries.pop_front();
        }
    }

    pub fn query(&self, params: &AuthAuditQueryParams) -> Vec<AuthAuditEntry> {
        let entries = self.entries.lock().unwrap();
        let mut results: Vec<&AuthAuditEntry> = entries
            .iter()
            .filter(|e| {
                if let Some(ref actor) = params.actor {
                    if e.actor != *actor { return false; }
                }
                if let Some(ref action) = params.action {
                    if e.action != *action { return false; }
                }
                if let Some(success) = params.success {
                    if e.success != success { return false; }
                }
                if let Some(since) = params.since {
                    if e.timestamp < since { return false; }
                }
                true
            })
            .collect();
        results.reverse();
        results.truncate(params.limit);
        results.into_iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AuthAuditQueryParams {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub success: Option<bool>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "audit_default_limit")]
    pub limit: usize,
}

/// Ring buffer for typed compliance audit entries.
#[derive(Clone)]
pub struct ComplianceAuditBuffer {
    entries: Arc<StdMutex<VecDeque<ComplianceAuditEntry>>>,
    max_entries: usize,
}

impl ComplianceAuditBuffer {
    pub fn new(max_events: usize) -> Self {
        Self {
            entries: Arc::new(StdMutex::new(VecDeque::with_capacity(max_events))),
            max_entries: max_events,
        }
    }

    pub fn push(&self, entry: ComplianceAuditEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.push_back(entry);
        while entries.len() > self.max_entries {
            entries.pop_front();
        }
    }

    pub fn query(&self, params: &ComplianceAuditQueryParams) -> Vec<ComplianceAuditEntry> {
        let entries = self.entries.lock().unwrap();
        let mut results: Vec<&ComplianceAuditEntry> = entries
            .iter()
            .filter(|e| {
                if let Some(ref event_type) = params.event_type {
                    if e.event_type != *event_type { return false; }
                }
                if let Some(ref actor) = params.actor {
                    if e.actor != *actor { return false; }
                }
                if let Some(since) = params.since {
                    if e.timestamp < since { return false; }
                }
                true
            })
            .collect();
        results.reverse();
        results.truncate(params.limit);
        results.into_iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ComplianceAuditQueryParams {
    pub event_type: Option<String>,
    pub actor: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "audit_default_limit")]
    pub limit: usize,
}

// ---------------------------------------------------------------------------
// Audit export
// ---------------------------------------------------------------------------

/// Export format for all audit entries.
#[derive(Debug, Serialize)]
pub struct AuditExport {
    pub exported_at: chrono::DateTime<chrono::Utc>,
    pub general_events: Vec<AuditEvent>,
    pub request_events: Vec<RequestAuditEntry>,
    pub auth_events: Vec<AuthAuditEntry>,
    pub compliance_events: Vec<ComplianceAuditEntry>,
    pub total_entries: usize,
}

impl AuditLogger {
    /// Export all audit entries from all buffers.
    pub fn export(
        &self,
        request_buffer: &RequestAuditBuffer,
        auth_buffer: &AuthAuditBuffer,
        compliance_buffer: &ComplianceAuditBuffer,
    ) -> AuditExport {
        let general_events: Vec<AuditEvent> = {
            let events = self.events.lock().unwrap();
            events.iter().cloned().collect()
        };
        let request_events: Vec<RequestAuditEntry> = {
            let entries = request_buffer.entries.lock().unwrap();
            entries.iter().cloned().collect()
        };
        let auth_events: Vec<AuthAuditEntry> = {
            let entries = auth_buffer.entries.lock().unwrap();
            entries.iter().cloned().collect()
        };
        let compliance_events: Vec<ComplianceAuditEntry> = {
            let entries = compliance_buffer.entries.lock().unwrap();
            entries.iter().cloned().collect()
        };

        let total_entries =
            general_events.len() + request_events.len() + auth_events.len() + compliance_events.len();

        AuditExport {
            exported_at: chrono::Utc::now(),
            general_events,
            request_events,
            auth_events,
            compliance_events,
            total_entries,
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregated audit statistics
// ---------------------------------------------------------------------------

/// Aggregated audit statistics.
#[derive(Debug, Serialize)]
pub struct AuditStats {
    pub total_events: usize,
    pub action_counts: std::collections::HashMap<String, u64>,
    pub top_actors: Vec<(String, u64)>,
}

// ---------------------------------------------------------------------------
// Axum admin handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use axum::routing::get;

use crate::proxy::AppState;

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

/// GET /admin/audit/logs -- query audit logs
pub async fn audit_logs_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<AuditQueryParams>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let events = state.audit_logger.query(&params);
    admin_ok(serde_json::json!({
        "events": events,
        "total": events.len(),
        "query": params,
    }))
}

/// GET /admin/audit/stats -- audit statistics
pub async fn audit_stats_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let stats = state.audit_logger.stats();
    admin_ok(serde_json::json!(stats))
}

/// GET /admin/audit/requests -- query request audit log
pub async fn audit_requests_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<RequestAuditQueryParams>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }
    let events = state.request_audit_buffer.query(&params);
    admin_ok(serde_json::json!({
        "events": events,
        "total": events.len(),
        "query": params,
    }))
}

/// GET /admin/audit/auth -- query auth audit log
pub async fn audit_auth_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<AuthAuditQueryParams>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }
    let events = state.auth_audit_buffer.query(&params);
    admin_ok(serde_json::json!({
        "events": events,
        "total": events.len(),
        "query": params,
    }))
}

/// GET /admin/audit/compliance -- query compliance audit log
pub async fn audit_compliance_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ComplianceAuditQueryParams>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }
    let events = state.compliance_audit_buffer.query(&params);
    admin_ok(serde_json::json!({
        "events": events,
        "total": events.len(),
        "query": params,
    }))
}

/// GET /admin/audit/export -- export all audit entries
pub async fn audit_export_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }
    let export = state.audit_logger.export(
        &state.request_audit_buffer,
        &state.auth_audit_buffer,
        &state.compliance_audit_buffer,
    );
    admin_ok(serde_json::json!(export))
}

/// GET /admin/audit/categories -- list available audit categories
pub async fn audit_categories_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }
    let categories = vec![
        AuditCategory::Request,
        AuditCategory::Auth,
        AuditCategory::Compliance,
        AuditCategory::General,
    ];
    let cat_strings: Vec<String> = categories.iter().map(|c| c.to_string()).collect();
    admin_ok(serde_json::json!({
        "categories": cat_strings,
        "counts": {
            "general": state.audit_logger.len(),
            "requests": state.request_audit_buffer.len(),
            "auth": state.auth_audit_buffer.len(),
            "compliance": state.compliance_audit_buffer.len(),
        },
    }))
}

/// Build the audit admin router.
pub fn build_audit_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/audit/logs", get(audit_logs_handler))
        .route("/admin/audit/stats", get(audit_stats_handler))
        .route("/admin/audit/requests", get(audit_requests_handler))
        .route("/admin/audit/auth", get(audit_auth_handler))
        .route("/admin/audit/compliance", get(audit_compliance_handler))
        .route("/admin/audit/export", get(audit_export_handler))
        .route("/admin/audit/categories", get(audit_categories_handler))
        .with_state(state)
}
