//! Proxy layer — forwards inference requests to xergon-agent providers
//!
//! Features:
//! - SSE streaming passthrough with token counting
//! - Fallback chain on provider failure
#![allow(unused_doc_comments)]
//! - Timeout handling
//! - Credit pre-authorization with post-stream reconciliation

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use futures_core::Stream;
use pin_project_lite::pin_project;
use reqwest::Client;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

use crate::db::Db;
use crate::provider::ProviderRegistry;

/// Shared app state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<crate::config::RelayConfig>,
    pub provider_registry: Arc<ProviderRegistry>,
    pub rate_limiter: Arc<crate::rate_limit::RateLimiter>,
    pub http_client: Client,
    /// Simple in-memory usage store (keyed by request ID or IP for anon)
    pub usage_store: Arc<DashMap<String, UsageRecord>>,
    /// Database for users and credits
    pub db: Arc<Db>,
    /// Provider directory for dynamic registration
    pub provider_directory: Arc<crate::registration::ProviderDirectory>,
}

/// A single usage record
#[allow(dead_code)] // TODO: will be used for usage analytics endpoint
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub request_id: String,
    pub ip: String,
    pub model: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub provider: String,
    pub latency_ms: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_anonymous: bool,
}

/// Information needed to reconcile credits after a streaming response completes.
/// The handler deducts the estimated cost upfront, then the stream wrapper
/// reconciles the difference once actual token counts are known.
#[derive(Clone)]
pub struct ReconcileInfo {
    pub user_id: String,
    pub estimated_cost_usd: f64,
    pub cost_per_1k_tokens: f64,
    pub model: String,
    pub db: Arc<Db>,
    /// Set to true once reconciliation has fired (prevents double-reconcile).
    pub reconciled: Arc<AtomicBool>,
}

/// Result of a proxied request — includes the response and optional usage info
pub struct ProxyResult {
    pub response: Response,
    pub provider: String,
    pub latency_ms: u64,
    /// Actual token counts from the response (non-streaming) or final SSE chunk.
    /// Shared via Arc so streaming wrapper can write while handler reads later.
    pub usage: Option<Arc<TokenUsage>>,
    /// For streaming responses: carries pre-authorization info so the stream
    /// wrapper can reconcile credits once the client finishes consuming the stream.
    /// Stored here for observability; the actual reconciliation is handled inside
    /// the `ReconcileStreamInner` stream wrapper.
    #[allow(dead_code)]
    pub reconcile_info: Option<ReconcileInfo>,
}

/// Token usage extracted from a provider response.
/// Uses atomics so the streaming wrapper can write while the handler reads.
#[derive(Debug, Default)]
pub struct TokenUsage {
    pub prompt_tokens: AtomicU64,
    pub completion_tokens: AtomicU64,
    pub total_tokens: AtomicU64,
}

impl TokenUsage {
    /// Convenience read of all fields.
    pub fn snapshot(&self) -> TokenUsageSnapshot {
        TokenUsageSnapshot {
            prompt_tokens: self.prompt_tokens.load(Ordering::Relaxed),
            completion_tokens: self.completion_tokens.load(Ordering::Relaxed),
            total_tokens: self.total_tokens.load(Ordering::Relaxed),
        }
    }
}

/// Non-atomic snapshot of token usage (for cloning/returning).
#[derive(Debug, Clone, Default)]
pub struct TokenUsageSnapshot {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// Proxy a chat completions request to the best available provider.
///
/// Tries providers in ranked order. On failure, falls back to the next.
/// If stream=true, wraps the SSE stream with a token counter.
/// If `reconcile_info` is provided for streaming, the stream wrapper will
/// also reconcile credits after the stream completes.
pub async fn proxy_chat_completion(
    state: &AppState,
    request_body: serde_json::Value,
    headers: &HeaderMap,
    reconcile_info: Option<ReconcileInfo>,
) -> Result<ProxyResult, ProxyError> {
    let model = request_body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown")
        .to_string();

    let is_stream = request_body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let max_attempts = state.config.relay.max_fallback_attempts;
    let mut tried: Vec<String> = Vec::new();

    for attempt in 0..max_attempts {
        // Select next best provider (excluding already-tried)
        let provider = state
            .provider_registry
            .select_provider(&tried)
            .ok_or_else(|| {
                if tried.is_empty() {
                    ProxyError::NoProviders
                } else {
                    ProxyError::AllProvidersFailed {
                        attempts: tried.len(),
                    }
                }
            })?;

        tried.push(provider.endpoint.clone());

        // Acquire a slot on this provider
        let _guard = match state.provider_registry.acquire_provider(&provider.endpoint) {
            Some(guard) => guard,
            None => {
                warn!(
                    provider = %provider.endpoint,
                    "Provider disappeared from registry before acquire, skipping"
                );
                continue;
            }
        };

        let provider_url = format!(
            "{}/v1/chat/completions",
            provider.endpoint.trim_end_matches('/')
        );

        info!(
            attempt = attempt + 1,
            provider = %provider.endpoint,
            model = %model,
            stream = is_stream,
            "Proxying request to provider"
        );

        let start = std::time::Instant::now();

        // Build forwarded request
        let mut req_builder = state
            .http_client
            .post(&provider_url)
            .timeout(std::time::Duration::from_secs(
                state.config.relay.provider_timeout_secs,
            ));

        // Forward relevant headers
        for (name, value) in headers.iter() {
            let name_str = name.as_str().to_lowercase();
            // Skip hop-by-hop and auth headers
            if matches!(
                name_str.as_str(),
                "host" | "connection" | "transfer-encoding" | "authorization"
                    | "x-forwarded-for"
                    | "x-real-ip"
            ) {
                continue;
            }
            if let Ok(val) = HeaderValue::from_bytes(value.as_bytes()) {
                req_builder = req_builder.header(name.as_str(), val);
            }
        }

        req_builder = req_builder.json(&request_body);

        match req_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let latency_ms = start.elapsed().as_millis() as u64;

                info!(
                    provider = %provider.endpoint,
                    status = %resp.status(),
                    latency_ms,
                    "Provider responded successfully"
                );

                if is_stream {
                    // Stream SSE response with token counting (and optional reconciliation)
                    let stream = resp.bytes_stream();
                    let (tracking_body, usage) = match &reconcile_info {
                        Some(ri) => counting_stream_body_with_reconcile(stream, ri.clone()),
                        None => counting_stream_body(stream),
                    };
                    let response = build_streaming_response(tracking_body);

                    return Ok(ProxyResult {
                        response,
                        provider: provider.endpoint.clone(),
                        latency_ms,
                        usage: Some(usage),
                        reconcile_info,
                    });
                } else {
                    // Collect the full response and extract usage
                    match resp.bytes().await {
                        Ok(body) => {
                            let usage = extract_usage_from_json(&body).map(Arc::new);
                            let response = Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "application/json")
                                .header("X-Provider", &provider.endpoint)
                                .header("X-Latency-Ms", latency_ms.to_string())
                                .body(Body::from(body))
                                .unwrap();
                            return Ok(ProxyResult {
                                response,
                                provider: provider.endpoint.clone(),
                                latency_ms,
                                usage,
                                reconcile_info: None,
                            });
                        }
                        Err(e) => {
                            warn!(provider = %provider.endpoint, error = %e, "Failed to read provider response body");
                            continue; // Try next provider
                        }
                    }
                }
            }
            Ok(resp) => {
                let status = resp.status();
                warn!(
                    provider = %provider.endpoint,
                    status = %status,
                    "Provider returned error status"
                );
                // If it's a 4xx client error, don't retry — it's our fault
                if status.is_client_error() && status.as_u16() != 429 {
                    match resp.bytes().await {
                        Ok(body) => {
                            let response = Response::builder()
                                .status(status)
                                .header("Content-Type", "application/json")
                                .body(Body::from(body))
                                .unwrap();
                            return Ok(ProxyResult {
                                response,
                                provider: provider.endpoint.clone(),
                                latency_ms: 0,
                                usage: None,
                                reconcile_info: None,
                            });
                        }
                        Err(_) => continue,
                    }
                }
                continue; // Retry on 429 or 5xx
            }
            Err(e) => {
                warn!(
                    provider = %provider.endpoint,
                    error = %e,
                    "Provider request failed"
                );
                continue; // Try next provider
            }
        }
    }

    Err(ProxyError::AllProvidersFailed {
        attempts: tried.len(),
    })
}

/// Extract token usage from a non-streaming JSON response body
fn extract_usage_from_json(body: &[u8]) -> Option<TokenUsage> {
    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = json.get("usage")?;

    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage
        .get("completion_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let tu = TokenUsage::default();
    tu.prompt_tokens.store(prompt_tokens, Ordering::Relaxed);
    tu.completion_tokens.store(completion_tokens, Ordering::Relaxed);
    tu.total_tokens.store(total_tokens, Ordering::Relaxed);
    Some(tu)
}

/// Wrap an SSE byte stream with token counting.
///
/// Returns a Body for the response and an Arc<TokenUsage> that gets populated
/// as the stream is consumed. The caller can read the usage after the stream ends.
fn counting_stream_body(
    stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> (Body, Arc<TokenUsage>) {
    let usage = Arc::new(TokenUsage::default());
    let usage_clone = usage.clone();

    let counting = StreamTokenCounter {
        inner: Box::pin(stream),
        usage: usage_clone,
        has_usage_field: false,
    };

    (Body::from_stream(counting), usage)
}

/// Wrap an SSE byte stream with token counting AND post-stream credit reconciliation.
///
/// Like `counting_stream_body`, but when the stream completes (or errors),
/// it spawns a background task to reconcile the difference between the
/// pre-authorized estimated cost and the actual token-based cost.
///
/// Returns a Body for the response and an Arc<TokenUsage>.
fn counting_stream_body_with_reconcile(
    stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    reconcile_info: ReconcileInfo,
) -> (Body, Arc<TokenUsage>) {
    let usage = Arc::new(TokenUsage::default());
    let usage_clone = usage.clone();
    let reconcile_info_clone = reconcile_info.clone();

    let counting = ReconcileStreamInner {
        inner: Box::pin(stream),
        usage: usage_clone,
        reconcile_info: reconcile_info_clone,
        has_usage_field: false,
        fired: Arc::clone(&reconcile_info.reconciled),
    };

    (Body::from_stream(counting), usage)
}

pin_project! {
    struct StreamTokenCounter<S> {
        #[pin]
        inner: S,
        usage: Arc<TokenUsage>,
        // Set to true when we've seen a chunk with a `usage` field (the final chunk).
        // This prevents the chars/4 heuristic from double-counting completion tokens.
        has_usage_field: bool,
    }
}

impl<S> Stream for StreamTokenCounter<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>>,
{
    type Item = Result<bytes::Bytes, axum::Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();

        match this.inner.poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(chunk))) => {
                let data = chunk.as_ref();

                // Try to extract usage from SSE data lines
                for line in data.split(|&b| b == b'\n') {
                    let line = std::str::from_utf8(line).unwrap_or("");
                    if let Some(data_str) = line.strip_prefix("data: ") {
                        if data_str.trim() == "[DONE]" {
                            continue;
                        }
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data_str) {
                            // Check for usage in this chunk (some providers send it in the final chunk)
                            if let Some(u) = json.get("usage") {
                                if let (Some(pt), Some(ct)) = (
                                    u.get("prompt_tokens").and_then(|v| v.as_u64()),
                                    u.get("completion_tokens").and_then(|v| v.as_u64()),
                                ) {
                                    this.usage.prompt_tokens.store(pt, Ordering::Relaxed);
                                    this.usage
                                        .completion_tokens
                                        .store(ct, Ordering::Relaxed);
                                    this.usage.total_tokens.store(pt + ct, Ordering::Relaxed);
                                    *this.has_usage_field = true;
                                }
                            }

                            // Fallback: count completion tokens from content delta
                            // ~4 chars per token heuristic — skip if we got a usage field
                            // to avoid double-counting
                            if !*this.has_usage_field {
                                if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                                    if let Some(delta) = choices.first().and_then(|c| c.get("delta")) {
                                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                            let tokens = (content.len() as u64) / 4;
                                            if tokens > 0 {
                                                this.usage.completion_tokens.fetch_add(tokens, Ordering::Relaxed);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                std::task::Poll::Ready(Some(Ok(chunk)))
            }
            std::task::Poll::Ready(Some(Err(e))) => {
                std::task::Poll::Ready(Some(Err(axum::Error::new(std::io::Error::other(
                    e.to_string(),
                )))))
            }
            std::task::Poll::Ready(None) => {
                // Stream ended — if we have completion tokens but no prompt tokens,
                // estimate prompt tokens from the original request (stored elsewhere)
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

fn build_streaming_response(body: Body) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .header("X-Accel-Buffering", "no")
        .body(body)
        .unwrap()
}

/// Minimum token difference threshold to trigger reconciliation.
/// Avoids micro-adjustments for sub-10-token differences.
const RECONCILE_TOKEN_THRESHOLD: u64 = 10;

/// Perform the actual credit reconciliation after a stream completes.
///
/// Compares the estimated cost (deducted upfront by the handler) against
/// the actual cost calculated from real token counts, then adjusts:
///   - Overcharge → add_credits (kind = "refund")
///   - Undercharge → deduct_credits (kind = "settlement")
fn reconcile_credits(reconcile_info: &ReconcileInfo, usage: &TokenUsage) {
    // Use compare_exchange to ensure only one reconciliation fires
    if reconcile_info
        .reconciled
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return; // Already reconciled
    }

    let snap = usage.snapshot();
    let actual_tokens = if snap.total_tokens > 0 {
        snap.total_tokens
    } else {
        // Fallback: estimate input + known completion
        snap.prompt_tokens + snap.completion_tokens
    };

    let cpt = reconcile_info.cost_per_1k_tokens / 1000.0;
    let actual_cost = actual_tokens as f64 * cpt;
    let estimated_cost = reconcile_info.estimated_cost_usd;

    let diff = actual_cost - estimated_cost;
    let token_diff = if cpt > 0.0 {
        (actual_cost / cpt - estimated_cost / cpt).abs() as u64
    } else {
        0
    };

    // Skip trivial reconciliations (< RECONCILE_TOKEN_THRESHOLD tokens difference)
    if token_diff < RECONCILE_TOKEN_THRESHOLD {
        info!(
            user_id = %reconcile_info.user_id,
            model = %reconcile_info.model,
            estimated_cost = estimated_cost,
            actual_cost = actual_cost,
            "Stream reconciliation skipped: token difference below threshold"
        );
        return;
    }

    if diff < 0.0 {
        // Overcharged: refund the difference
        let refund_amount = diff.abs();
        let tx_id = uuid::Uuid::new_v4().to_string();
        let desc = format!(
            "Stream reconciliation refund: {} (est ${:.6}, actual ${:.6}, {} tokens)",
            reconcile_info.model,
            estimated_cost,
            actual_cost,
            actual_tokens,
        );
        match reconcile_info.db.add_credits(
            &tx_id,
            &reconcile_info.user_id,
            refund_amount,
            "refund",
            &desc,
            None,
        ) {
            Ok(tx) => {
                info!(
                    user_id = %reconcile_info.user_id,
                    model = %reconcile_info.model,
                    refund = refund_amount,
                    balance_after = tx.balance_after,
                    estimated_cost = estimated_cost,
                    actual_cost = actual_cost,
                    actual_tokens = actual_tokens,
                    "Stream reconciliation: refunded overcharge"
                );
            }
            Err(e) => {
                warn!(
                    user_id = %reconcile_info.user_id,
                    error = %e,
                    refund = refund_amount,
                    "Stream reconciliation: failed to refund overcharge"
                );
            }
        }
    } else if diff > 0.0 {
        // Undercharged: deduct the extra (user was pre-authorized, so balance should cover it)
        let tx_id = uuid::Uuid::new_v4().to_string();
        let desc = format!(
            "Stream reconciliation settlement: {} (est ${:.6}, actual ${:.6}, {} tokens)",
            reconcile_info.model,
            estimated_cost,
            actual_cost,
            actual_tokens,
        );
        match reconcile_info
            .db
            .deduct_credits(&tx_id, &reconcile_info.user_id, diff, &desc)
        {
            Ok(balance_after) => {
                info!(
                    user_id = %reconcile_info.user_id,
                    model = %reconcile_info.model,
                    extra_deducted = diff,
                    balance_after = balance_after,
                    estimated_cost = estimated_cost,
                    actual_cost = actual_cost,
                    actual_tokens = actual_tokens,
                    "Stream reconciliation: deducted undercharge"
                );
            }
            Err(e) => {
                warn!(
                    user_id = %reconcile_info.user_id,
                    error = %e,
                    extra_deducted = diff,
                    "Stream reconciliation: failed to deduct undercharge"
                );
            }
        }
    }
}

pin_project! {
    struct ReconcileStreamInner<S> {
        #[pin]
        inner: S,
        usage: Arc<TokenUsage>,
        reconcile_info: ReconcileInfo,
        has_usage_field: bool,
        fired: Arc<AtomicBool>,
    }
}

impl<S> Stream for ReconcileStreamInner<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>>,
{
    type Item = Result<bytes::Bytes, axum::Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();

        match this.inner.poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(chunk))) => {
                let data = chunk.as_ref();

                // Try to extract usage from SSE data lines
                for line in data.split(|&b| b == b'\n') {
                    let line = std::str::from_utf8(line).unwrap_or("");
                    if let Some(data_str) = line.strip_prefix("data: ") {
                        if data_str.trim() == "[DONE]" {
                            continue;
                        }
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data_str) {
                            // Check for usage in this chunk (some providers send it in the final chunk)
                            if let Some(u) = json.get("usage") {
                                if let (Some(pt), Some(ct)) = (
                                    u.get("prompt_tokens").and_then(|v| v.as_u64()),
                                    u.get("completion_tokens").and_then(|v| v.as_u64()),
                                ) {
                                    this.usage.prompt_tokens.store(pt, Ordering::Relaxed);
                                    this.usage
                                        .completion_tokens
                                        .store(ct, Ordering::Relaxed);
                                    this.usage.total_tokens.store(pt + ct, Ordering::Relaxed);
                                    *this.has_usage_field = true;
                                }
                            }

                            // Fallback: count completion tokens from content delta
                            if !*this.has_usage_field {
                                if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                                    if let Some(delta) = choices.first().and_then(|c| c.get("delta")) {
                                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                            let tokens = (content.len() as u64) / 4;
                                            if tokens > 0 {
                                                this.usage.completion_tokens.fetch_add(tokens, Ordering::Relaxed);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                std::task::Poll::Ready(Some(Ok(chunk)))
            }
            std::task::Poll::Ready(Some(Err(e))) => {
                // Stream errored — still reconcile with whatever tokens we have
                if !this.fired.load(Ordering::Relaxed) {
                    let usage = this.usage.clone();
                    let reconcile_info = this.reconcile_info.clone();
                    tokio::spawn(async move {
                        reconcile_credits(&reconcile_info, &usage);
                    });
                }
                std::task::Poll::Ready(Some(Err(axum::Error::new(std::io::Error::other(
                    e.to_string(),
                )))))
            }
            std::task::Poll::Ready(None) => {
                // Stream completed — fire reconciliation in background
                if !this.fired.load(Ordering::Relaxed) {
                    let usage = this.usage.clone();
                    let reconcile_info = this.reconcile_info.clone();
                    tokio::spawn(async move {
                        reconcile_credits(&reconcile_info, &usage);
                    });
                }
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

/// Errors that can occur during proxying
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("No providers available")]
    NoProviders,

    #[error("All {attempts} provider(s) failed")]
    AllProvidersFailed { attempts: usize },

    #[error("Request validation error: {0}")]
    Validation(String),

    #[error("Rate limit exceeded")]
    RateLimited,

    #[error("Tier rate limit exceeded: {tier} — {reset_hint}")]
    TierRateLimited {
        tier: String,
        reset_hint: String,
    },

    #[error(
        "Insufficient credits: balance ${balance_usd:.4}, need ${estimated_cost_usd:.4}"
    )]
    InsufficientCredits {
        balance_usd: f64,
        estimated_cost_usd: f64,
    },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, message): (StatusCode, String) = match &self {
            ProxyError::NoProviders => (
                StatusCode::SERVICE_UNAVAILABLE,
                "No providers available for this model. Please try again later.".into(),
            ),
            ProxyError::AllProvidersFailed { attempts } => (
                StatusCode::BAD_GATEWAY,
                format!(
                    "All {} provider(s) failed. Please try again.",
                    attempts
                ),
            ),
            ProxyError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ProxyError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded. You've used all your free requests for today. Create an account for more.".into(),
            ),
            ProxyError::TierRateLimited { tier, reset_hint } => (
                StatusCode::TOO_MANY_REQUESTS,
                format!(
                    "Rate limit exceeded for {} tier ({}). Upgrade your plan or wait for the window to reset.",
                    tier, reset_hint
                ),
            ),
            ProxyError::InsufficientCredits {
                balance_usd,
                estimated_cost_usd,
            } => (
                StatusCode::PAYMENT_REQUIRED,
                format!(
                    "Insufficient credits. Balance: ${:.2}, estimated cost: ${:.4}. Add credits to continue.",
                    balance_usd, estimated_cost_usd
                ),
            ),
            ProxyError::Http(e) => (
                StatusCode::BAD_GATEWAY,
                format!("Upstream error: {}", e),
            ),
        };

        let body = serde_json::json!({
            "error": {
                "message": message,
                "type": "relay_error",
                "code": status.as_u16(),
            }
        });

        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }
}
