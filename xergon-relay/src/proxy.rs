//! Proxy layer — forwards inference requests to xergon-agent providers
//!
//! Features:
//! - SSE streaming passthrough with token counting
//! - Fallback chain on provider failure
//! - Timeout handling

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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

use crate::auth::{AuthError, AuthVerifier};
use crate::chain::ChainScanner;
use crate::chain_cache::ChainCache;
use crate::demand::DemandTracker;
use crate::free_tier::FreeTierTracker;
use crate::provider::ProviderRegistry;
use crate::rate_limit::RateLimitState;
use crate::events::EventBroadcaster;
use crate::ws::WsBroadcaster;
use crate::gossip::GossipService;

/// Shared app state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<crate::config::RelayConfig>,
    pub provider_registry: Arc<ProviderRegistry>,
    pub http_client: Client,
    /// Simple in-memory usage store (keyed by request ID)
    pub usage_store: Arc<DashMap<String, UsageRecord>>,
    /// Optional chain scanner for on-chain provider discovery
    pub chain_scanner: Option<Arc<ChainScanner>>,
    /// Cached chain-discovered providers (refreshed by background task)
    pub chain_cache: Option<Arc<ChainCache>>,
    /// Optional balance checker for on-chain staking balance verification
    pub balance_checker: Option<Arc<crate::balance::BalanceChecker>>,
    /// Signature-based auth verifier
    pub auth_verifier: Option<Arc<AuthVerifier>>,
    /// Prometheus metrics collector
    pub relay_metrics: Arc<crate::metrics::RelayMetrics>,
    /// Rate limiter state (None if rate limiting is disabled)
    pub rate_limit_state: Option<Arc<RateLimitState>>,
    /// Per-model demand tracker (sliding window)
    pub demand: Arc<DemandTracker>,
    /// Cached ERG/USD rate from oracle pool (refreshed by background task)
    pub erg_usd_rate: Arc<std::sync::RwLock<Option<f64>>>,
    /// Free tier request tracker (None if disabled)
    pub free_tier_tracker: Option<Arc<FreeTierTracker>>,
    /// SSE event broadcaster for real-time status updates
    pub event_broadcaster: Arc<EventBroadcaster>,
    /// WebSocket broadcaster for real-time provider status
    pub ws_broadcaster: Arc<WsBroadcaster>,
    /// Gossip service for multi-relay consensus (None if disabled)
    pub gossip_service: Option<Arc<GossipService>>,
}

/// A single usage record
#[allow(dead_code)]
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
    /// Rarity multiplier applied for this request (1.0 = no bonus)
    pub rarity_multiplier: f64,
}

/// Result of a proxied request — includes the response and optional usage info
pub struct ProxyResult {
    pub response: Response,
    pub provider: String,
    pub latency_ms: u64,
    /// Actual token counts from the response (non-streaming) or final SSE chunk.
    /// Shared via Arc so streaming wrapper can write while handler reads later.
    pub usage: Option<Arc<TokenUsage>>,
}

/// Token usage extracted from a provider response.
/// Uses atomics so the streaming wrapper can write while the handler reads.
#[derive(Debug, Default)]
pub struct TokenUsage {
    pub prompt_tokens: AtomicU64,
    pub completion_tokens: AtomicU64,
    pub total_tokens: AtomicU64,
}

/// Proxy a chat completions request to the best available provider.
///
/// Tries providers in ranked order. On failure, falls back to the next.
/// If stream=true, wraps the SSE stream with a token counter.
pub async fn proxy_chat_completion(
    state: &AppState,
    request_body: serde_json::Value,
    headers: &HeaderMap,
    request_id: &str,
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
        // Select next best provider (excluding already-tried), model-aware
        let provider = state
            .provider_registry
            .select_provider_for_model(&model, &tried, &state.demand)
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
            request_id = request_id,
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
            // Skip hop-by-hop, auth, and content headers (set by .json() below)
            if matches!(
                name_str.as_str(),
                "host"
                    | "connection"
                    | "transfer-encoding"
                    | "authorization"
                    | "x-forwarded-for"
                    | "x-real-ip"
                    | "content-length"
                    | "content-type"
            ) {
                continue;
            }
            if let Ok(val) = HeaderValue::from_bytes(value.as_bytes()) {
                req_builder = req_builder.header(name.as_str(), val);
            }
        }

        req_builder = req_builder.json(&request_body);

        // Forward the relay request ID to the provider for end-to-end correlation
        if let Ok(val) = request_id.parse::<axum::http::HeaderValue>() {
            req_builder = req_builder.header("X-Request-Id", val);
        }

        match req_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let latency_ms = start.elapsed().as_millis() as u64;

                info!(
                    request_id = request_id,
                    provider = %provider.endpoint,
                    status = %resp.status(),
                    latency_ms,
                    "Provider responded successfully"
                );

                if is_stream {
                    // Stream SSE response with token counting
                    let stream = resp.bytes_stream();
                    let (tracking_body, usage) = counting_stream_body(stream);
                    let response = build_streaming_response(tracking_body);

                    return Ok(ProxyResult {
                        response,
                        provider: provider.endpoint.clone(),
                        latency_ms,
                        usage: Some(usage),
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
                    request_id = request_id,
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
                            });
                        }
                        Err(_) => continue,
                    }
                }
                continue; // Retry on 429 or 5xx
            }
            Err(e) => {
                warn!(
                    request_id = request_id,
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
    tu.completion_tokens
        .store(completion_tokens, Ordering::Relaxed);
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
                                    this.usage.completion_tokens.store(ct, Ordering::Relaxed);
                                    this.usage.total_tokens.store(pt + ct, Ordering::Relaxed);
                                    *this.has_usage_field = true;
                                }
                            }

                            // Fallback: count completion tokens from content delta
                            // ~4 chars per token heuristic — skip if we got a usage field
                            // to avoid double-counting
                            if !*this.has_usage_field {
                                if let Some(choices) =
                                    json.get("choices").and_then(|c| c.as_array())
                                {
                                    if let Some(delta) =
                                        choices.first().and_then(|c| c.get("delta"))
                                    {
                                        if let Some(content) =
                                            delta.get("content").and_then(|c| c.as_str())
                                        {
                                            let tokens = (content.len() as u64) / 4;
                                            if tokens > 0 {
                                                this.usage
                                                    .completion_tokens
                                                    .fetch_add(tokens, Ordering::Relaxed);
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
            std::task::Poll::Ready(Some(Err(e))) => std::task::Poll::Ready(Some(Err(
                axum::Error::new(std::io::Error::other(e.to_string())),
            ))),
            std::task::Poll::Ready(None) => {
                // Stream ended
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

/// Errors that can occur during proxying
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("No providers available")]
    NoProviders,

    #[error("All {attempts} provider(s) failed")]
    AllProvidersFailed { attempts: usize },

    #[error("Request validation error: {0}")]
    Validation(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Insufficient ERG balance: {0}")]
    InsufficientBalance(String),

    #[error("Authentication error: {0}")]
    Unauthorized(String),
}

impl From<AuthError> for ProxyError {
    fn from(err: AuthError) -> Self {
        ProxyError::Unauthorized(err.to_string())
    }
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
            ProxyError::Http(e) => (
                StatusCode::BAD_GATEWAY,
                format!("Upstream error: {}", e),
            ),
            ProxyError::InsufficientBalance(msg) => (
                StatusCode::PAYMENT_REQUIRED,
                msg.clone(),
            ),
            ProxyError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                msg.clone(),
            ),
        };

        let (error_type, error_code) = match &self {
            ProxyError::InsufficientBalance(_) => ("insufficient_balance", "payment_required"),
            ProxyError::Unauthorized(_) => ("auth_error", "unauthorized"),
            _ => ("relay_error", ""),
        };

        let code_val = if error_code.is_empty() {
            serde_json::json!(status.as_u16())
        } else {
            serde_json::json!(error_code)
        };

        let body = serde_json::json!({
            "error": {
                "message": message,
                "type": error_type,
                "code": code_val,
            }
        });

        Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }
}
