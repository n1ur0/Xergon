//! Usage Analytics Aggregation for the Xergon relay.
//!
//! Collects per-request usage data (tokens, latency, model, provider),
//! aggregates into daily summaries, and provides per-model, per-key,
//! and per-tier breakdowns via admin endpoints.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tracing::{debug, info};

use crate::rate_limit_tiers::RateLimitTier;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single usage record from a proxy request.
#[derive(Debug, Clone, Serialize)]
pub struct UsageRecord {
    pub api_key: String,
    pub model: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub latency_ms: u64,
    pub provider_id: String,
    pub tier: RateLimitTier,
    pub error: bool,
}

/// Daily aggregated usage summary.
#[derive(Debug, Clone, Serialize)]
pub struct DailyUsageSummary {
    pub date: String,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub unique_users: u32,
    pub top_models: Vec<(String, u64)>,
    pub avg_latency_ms: f64,
    pub errors: u32,
}

/// Per-model usage breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub total_requests: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    pub avg_latency_ms: f64,
    pub errors: u32,
}

/// Per-key usage breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct KeyUsage {
    pub api_key_prefix: String,
    pub total_requests: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    pub avg_latency_ms: f64,
    pub errors: u32,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for usage analytics.
#[derive(Debug, Clone)]
pub struct UsageAnalyticsConfig {
    /// How many days of data to retain (default 30).
    pub retention_days: usize,
}

impl Default for UsageAnalyticsConfig {
    fn default() -> Self {
        Self { retention_days: 30 }
    }
}

// ---------------------------------------------------------------------------
// Usage analytics engine
// ---------------------------------------------------------------------------

/// Collects and aggregates usage data.
#[derive(Clone)]
pub struct UsageAnalytics {
    /// All individual usage records.
    records: Arc<StdMutex<Vec<UsageRecord>>>,
    config: UsageAnalyticsConfig,
}

impl UsageAnalytics {
    pub fn new() -> Self {
        Self::with_config(UsageAnalyticsConfig::default())
    }

    pub fn with_config(config: UsageAnalyticsConfig) -> Self {
        Self {
            records: Arc::new(StdMutex::new(Vec::new())),
            config,
        }
    }

    /// Record a single usage event.
    pub fn record(&self, record: UsageRecord) {
        let mut records = self.records.lock().unwrap();
        records.push(record);
    }

    /// Record usage from request parameters (convenience method).
    pub fn record_usage(
        &self,
        api_key: &str,
        model: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        latency_ms: u64,
        provider_id: &str,
        tier: RateLimitTier,
        error: bool,
    ) {
        let key_prefix = api_key.chars().take(12).collect();
        self.record(UsageRecord {
            api_key: key_prefix,
            model: model.to_string(),
            timestamp: chrono::Utc::now(),
            prompt_tokens,
            completion_tokens,
            latency_ms,
            provider_id: provider_id.to_string(),
            tier,
            error,
        });
    }

    /// Get aggregated usage for a date range.
    pub fn usage(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
        model_filter: Option<&str>,
        key_filter: Option<&str>,
    ) -> UsageAggregation {
        let records = self.records.lock().unwrap();
        let filtered: Vec<&UsageRecord> = records
            .iter()
            .filter(|r| {
                if r.timestamp < from || r.timestamp > to {
                    return false;
                }
                if let Some(model) = model_filter {
                    if r.model != model {
                        return false;
                    }
                }
                if let Some(key) = key_filter {
                    if r.api_key != key {
                        return false;
                    }
                }
                true
            })
            .collect();

        let total_requests = filtered.len() as u64;
        let total_prompt_tokens: u64 = filtered.iter().map(|r| r.prompt_tokens as u64).sum();
        let total_completion_tokens: u64 =
            filtered.iter().map(|r| r.completion_tokens as u64).sum();
        let total_tokens = total_prompt_tokens + total_completion_tokens;
        let total_errors: u64 = filtered.iter().filter(|r| r.error).count() as u64;
        let avg_latency_ms = if filtered.is_empty() {
            0.0
        } else {
            filtered.iter().map(|r| r.latency_ms as f64).sum::<f64>()
                / filtered.len() as f64
        };

        UsageAggregation {
            total_requests,
            total_prompt_tokens,
            total_completion_tokens,
            total_tokens,
            total_errors,
            avg_latency_ms,
        }
    }

    /// Get daily summaries for the last N days.
    pub fn daily_summaries(&self, days: usize) -> Vec<DailyUsageSummary> {
        let records = self.records.lock().unwrap();
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);

        // Group by date
        let mut by_date: HashMap<String, Vec<&UsageRecord>> = HashMap::new();
        for record in records.iter() {
            if record.timestamp < cutoff {
                continue;
            }
            let date = record.timestamp.format("%Y-%m-%d").to_string();
            by_date.entry(date).or_default().push(record);
        }

        let mut summaries: Vec<DailyUsageSummary> = by_date
            .into_iter()
            .map(|(date, recs)| {
                let total_requests = recs.len() as u64;
                let total_tokens: u64 = recs
                    .iter()
                    .map(|r| (r.prompt_tokens + r.completion_tokens) as u64)
                    .sum();
                let unique_users = recs.iter().map(|r| &r.api_key).collect::<std::collections::HashSet<_>>().len() as u32;
                let errors = recs.iter().filter(|r| r.error).count() as u32;
                let avg_latency = if recs.is_empty() {
                    0.0
                } else {
                    recs.iter().map(|r| r.latency_ms as f64).sum::<f64>() / recs.len() as f64
                };

                // Top models
                let mut model_counts: HashMap<String, u64> = HashMap::new();
                for r in &recs {
                    *model_counts.entry(r.model.clone()).or_insert(0) += 1;
                }
                let mut top_models: Vec<(String, u64)> = model_counts.into_iter().collect();
                top_models.sort_by(|a, b| b.1.cmp(&a.1));
                top_models.truncate(10);

                DailyUsageSummary {
                    date,
                    total_requests,
                    total_tokens,
                    unique_users,
                    top_models,
                    avg_latency_ms: avg_latency,
                    errors,
                }
            })
            .collect();

        summaries.sort_by(|a, b| b.date.cmp(&a.date));
        summaries
    }

    /// Get per-model breakdown.
    pub fn model_breakdown(&self) -> Vec<ModelUsage> {
        let records = self.records.lock().unwrap();

        let mut by_model: HashMap<&str, Vec<&UsageRecord>> = HashMap::new();
        for record in records.iter() {
            by_model.entry(&record.model).or_default().push(record);
        }

        let mut breakdown: Vec<ModelUsage> = by_model
            .into_iter()
            .map(|(model, recs)| {
                let total_requests = recs.len() as u64;
                let total_prompt_tokens: u64 =
                    recs.iter().map(|r| r.prompt_tokens as u64).sum();
                let total_completion_tokens: u64 =
                    recs.iter().map(|r| r.completion_tokens as u64).sum();
                let total_tokens = total_prompt_tokens + total_completion_tokens;
                let errors = recs.iter().filter(|r| r.error).count() as u32;
                let avg_latency = if recs.is_empty() {
                    0.0
                } else {
                    recs.iter().map(|r| r.latency_ms as f64).sum::<f64>() / recs.len() as f64
                };

                ModelUsage {
                    model: model.to_string(),
                    total_requests,
                    total_prompt_tokens,
                    total_completion_tokens,
                    total_tokens,
                    avg_latency_ms: avg_latency,
                    errors,
                }
            })
            .collect();

        breakdown.sort_by(|a, b| b.total_requests.cmp(&a.total_requests));
        breakdown
    }

    /// Get top API key consumers.
    pub fn top_users(&self, limit: usize) -> Vec<KeyUsage> {
        let records = self.records.lock().unwrap();

        let mut by_key: HashMap<&str, Vec<&UsageRecord>> = HashMap::new();
        for record in records.iter() {
            by_key.entry(&record.api_key).or_default().push(record);
        }

        let mut users: Vec<KeyUsage> = by_key
            .into_iter()
            .map(|(api_key, recs)| {
                let total_requests = recs.len() as u64;
                let total_prompt_tokens: u64 =
                    recs.iter().map(|r| r.prompt_tokens as u64).sum();
                let total_completion_tokens: u64 =
                    recs.iter().map(|r| r.completion_tokens as u64).sum();
                let total_tokens = total_prompt_tokens + total_completion_tokens;
                let errors = recs.iter().filter(|r| r.error).count() as u32;
                let avg_latency = if recs.is_empty() {
                    0.0
                } else {
                    recs.iter().map(|r| r.latency_ms as f64).sum::<f64>() / recs.len() as f64
                };

                KeyUsage {
                    api_key_prefix: api_key.to_string(),
                    total_requests,
                    total_prompt_tokens,
                    total_completion_tokens,
                    total_tokens,
                    avg_latency_ms: avg_latency,
                    errors,
                }
            })
            .collect();

        users.sort_by(|a, b| b.total_requests.cmp(&a.total_requests));
        users.truncate(limit);
        users
    }

    /// Prune records older than retention period.
    pub fn prune(&self) {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::days(self.config.retention_days as i64);
        let mut records = self.records.lock().unwrap();
        let before = records.len();
        records.retain(|r| r.timestamp > cutoff);
        let removed = before - records.len();
        if removed > 0 {
            debug!(removed, "Pruned old usage analytics records");
        }
    }

    /// Current record count.
    pub fn len(&self) -> usize {
        self.records.lock().unwrap().len()
    }
}

/// Aggregated usage over a time range.
#[derive(Debug, Serialize)]
pub struct UsageAggregation {
    pub total_requests: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    pub total_errors: u64,
    pub avg_latency_ms: f64,
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

#[derive(Debug, Deserialize)]
pub struct UsageQueryParams {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub model: Option<String>,
    pub key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DailyQueryParams {
    #[serde(default = "analytics_default_days")]
    pub days: usize,
}

fn analytics_default_days() -> usize {
    30
}

#[derive(Debug, Deserialize)]
pub struct TopUsersQueryParams {
    #[serde(default = "analytics_default_limit")]
    pub limit: usize,
}

fn analytics_default_limit() -> usize {
    20
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

/// GET /admin/analytics/usage -- aggregated usage
pub async fn usage_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<UsageQueryParams>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let from = params.from.unwrap_or_else(|| {
        chrono::Utc::now() - chrono::Duration::days(30)
    });
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let agg = state.usage_analytics.usage(
        from,
        to,
        params.model.as_deref(),
        params.key.as_deref(),
    );

    admin_ok(serde_json::json!({
        "from": from.to_rfc3339(),
        "to": to.to_rfc3339(),
        "aggregation": agg,
    }))
}

/// GET /admin/analytics/daily -- daily summaries
pub async fn daily_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<DailyQueryParams>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let summaries = state.usage_analytics.daily_summaries(params.days);
    admin_ok(serde_json::json!({
        "days": params.days,
        "summaries": summaries,
    }))
}

/// GET /admin/analytics/models -- per-model breakdown
pub async fn models_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let breakdown = state.usage_analytics.model_breakdown();
    admin_ok(serde_json::json!({
        "models": breakdown,
        "total": breakdown.len(),
    }))
}

/// GET /admin/analytics/top-users -- top API key consumers
pub async fn top_users_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<TopUsersQueryParams>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let users = state.usage_analytics.top_users(params.limit);
    admin_ok(serde_json::json!({
        "users": users,
        "total": users.len(),
    }))
}

/// Build the usage analytics admin router.
pub fn build_analytics_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/analytics/usage", get(usage_handler))
        .route("/admin/analytics/daily", get(daily_handler))
        .route("/admin/analytics/models", get(models_handler))
        .route("/admin/analytics/top-users", get(top_users_handler))
        .with_state(state)
}
