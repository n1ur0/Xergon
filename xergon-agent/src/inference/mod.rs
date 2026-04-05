//! Inference proxy — forwards OpenAI-compatible requests to the local LLM backend.
//!
//! Supports both Ollama (http://localhost:11434) and llama.cpp server.
//! The backend is auto-detected at startup via /v1/models probe.
//!
//! Endpoints:
//! - `POST /v1/chat/completions` — proxy to backend (streaming + non-streaming)
//! - `GET /v1/models` — list available models from backend

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures_util::{Stream, StreamExt};
use reqwest::Client;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::InferenceConfig;
use crate::pown::PownCalculator;

/// Inference proxy state
#[derive(Clone)]
pub struct InferenceState {
    pub config: InferenceConfig,
    pub http_client: Client,
    pub pown: Arc<PownCalculator>,
    /// Cached model name from last successful probe
    pub detected_model: Arc<RwLock<Option<String>>>,
    /// Settlement engine for recording usage events (optional)
    pub settlement: Option<Arc<crate::settlement::SettlementEngine>>,
    /// Provider ID for settlement recording
    pub provider_id: String,
    /// Provider ERG address for settlement recording
    pub provider_ergo_address: String,
    /// Auto model pull system (optional, enabled via config)
    pub auto_pull: Option<Arc<crate::auto_model_pull::AutoModelPull>>,
}

/// Build inference routes
pub fn build_router(state: InferenceState) -> Router {
    let protected_routes = Router::new()
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/models", get(models_handler))
        // Reject request bodies larger than 10 MB before buffering.
        // Without this, Axum buffers the entire body into `Bytes` before
        // the handler runs, allowing arbitrarily large payloads.
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024))
        .with_state(state.clone());

    // If api_key is configured, add auth middleware; otherwise, routes are open
    if !state.config.api_key.is_empty() {
        let api_key = state.config.api_key.clone();
        protected_routes.layer(middleware::from_fn(move |req, next| {
            let api_key = api_key.clone();
            check_api_key(req, next, api_key)
        }))
    } else {
        protected_routes
    }
}

/// Middleware that validates the Bearer token if inference.api_key is configured.
async fn check_api_key(req: Request<Body>, next: Next, api_key: String) -> impl IntoResponse {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let provided_key = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "));

    match provided_key {
        Some(key) if key == api_key => next.run(req).await,
        _ => error_response(StatusCode::UNAUTHORIZED, "Invalid or missing API key"),
    }
}

/// POST /v1/chat/completions — proxy to backend LLM
async fn chat_completions_handler(
    State(state): State<InferenceState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Enforce request body size limit (10 MB)
    const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;
    if body.len() > MAX_BODY_SIZE {
        return error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!(
                "Request body too large: {} bytes (max {} MB)",
                body.len(),
                MAX_BODY_SIZE / 1024 / 1024
            ),
        );
    }

    let backend_url = format!(
        "{}/v1/chat/completions",
        state.config.url.trim_end_matches('/')
    );

    let is_stream = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
        .unwrap_or(false);

    let model = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("model")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    info!(backend = %backend_url, model = %model, stream = is_stream, "Proxying inference request");

    // --- Auto model pull check ---
    // If auto_pull is configured and model is not available, trigger pull and return 503
    if let Some(ref auto_pull) = state.auto_pull {
        if !auto_pull.is_model_available(&model).await {
            warn!(model = %model, "Model not available locally, triggering auto-pull");
            let pull_result = auto_pull.pull_model(&model).await;
            match pull_result {
                crate::auto_model_pull::PullResult::AlreadyAvailable => {
                    // Race condition: model became available between check and pull
                }
                crate::auto_model_pull::PullResult::PullFailed { error } => {
                    let retry_after = auto_pull.retry_after_secs();
                    let body = serde_json::json!({
                        "error": {
                            "message": format!("Model '{}' is not available. {}", model, error),
                            "type": "xergon_model_unavailable",
                            "code": 503,
                            "pull_status": "in_progress",
                        }
                    });
                    return Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .header("Content-Type", "application/json")
                        .header("Retry-After", retry_after.to_string())
                        .body(Body::from(serde_json::to_string(&body).unwrap()))
                        .unwrap();
                }
                crate::auto_model_pull::PullResult::PulledFromPeer { .. }
                | crate::auto_model_pull::PullResult::PulledFromRegistry { .. } => {
                    info!(model = %model, "Model pull completed, proceeding with request");
                    // The model might need a moment to become available; proceed anyway
                }
            }
        }
    }

    // Build forwarded request
    let mut req_builder = state
        .http_client
        .post(&backend_url)
        .timeout(std::time::Duration::from_secs(state.config.timeout_secs));

    // Forward relevant headers (skip hop-by-hop)
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if matches!(
            name_str.as_str(),
            "host" | "connection" | "transfer-encoding" | "content-length"
        ) {
            continue;
        }
        if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
            req_builder = req_builder.header(name.as_str(), val);
        }
    }

    req_builder = req_builder
        .header("Content-Type", "application/json")
        .body(body.to_vec());

    match req_builder.send().await {
        Ok(resp) if resp.status().is_success() => {
            if is_stream {
                // Stream SSE events back to client while counting completion tokens.
                // Uses Arc<Mutex> counter + tokio::spawn for PoNW update to avoid lifetime issues
                // with async scan closures.
                let stream = resp.bytes_stream();
                let token_counter: Arc<std::sync::Mutex<u64>> = Arc::new(std::sync::Mutex::new(0));
                let has_final_usage: Arc<std::sync::Mutex<bool>> =
                    Arc::new(std::sync::Mutex::new(false));
                // Set to `true` when `[DONE]` is received so the drop guard
                // knows the normal PoNW update was already spawned.
                let completed_normally: Arc<std::sync::Mutex<bool>> =
                    Arc::new(std::sync::Mutex::new(false));
                let pown = state.pown.clone();
                let settlement = state.settlement.clone();
                let provider_id = state.provider_id.clone();
                let provider_ergo_address = state.provider_ergo_address.clone();
                let model_clone = model.clone();

                let completed_normally_for_inspect = completed_normally.clone();
                let token_counter_for_inspect = token_counter.clone();
                let has_final_usage_for_inspect = has_final_usage.clone();
                let counting_stream = stream.inspect(move |chunk_result| {
                    if let Ok(chunk) = chunk_result {
                        let text = String::from_utf8_lossy(chunk);
                        for line in text.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data.trim() == "[DONE]" {
                                    // Stream finished — spawn PoNW/settlement update
                                    let tokens = *token_counter_for_inspect.lock().unwrap();
                                    if tokens > 0 {
                                        *completed_normally_for_inspect.lock().unwrap() = true;
                                        let pown = pown.clone();
                                        let settlement = settlement.clone();
                                        let provider_id = provider_id.clone();
                                        let provider_ergo_address = provider_ergo_address.clone();
                                        let model = model_clone.clone();
                                        tokio::spawn(async move {
                                            pown.update_ai_stats(&model, tokens, 1).await;
                                            info!(
                                                model = %model,
                                                completion_tokens = tokens,
                                                "Streaming PoNW AI stats updated"
                                            );
                                            if let Some(ref s) = settlement {
                                                let (cost_per_1k, price_source) = s.resolve_cost_per_1k(&model).await;
                                                let cost_nanoerg = tokens as u64 * cost_per_1k / 1000;
                                                info!(
                                                    model = %model,
                                                    cost_nanoerg = cost_nanoerg,
                                                    price_source = price_source,
                                                    "Streaming usage cost calculated"
                                                );
                                                s.record_usage(
                                                    &provider_id,
                                                    &provider_ergo_address,
                                                    0,
                                                    tokens,
                                                    cost_nanoerg,
                                                )
                                                .await;
                                            }
                                        });
                                    }
                                } else if let Ok(json) =
                                    serde_json::from_str::<serde_json::Value>(data)
                                {
                                    let chunk_tokens = extract_completion_tokens(&json);
                                    if json.get("usage").is_some() {
                                        // Final chunk with usage field — exact total count
                                        if chunk_tokens > 0 {
                                            *token_counter_for_inspect.lock().unwrap() =
                                                chunk_tokens;
                                            *has_final_usage_for_inspect.lock().unwrap() = true;
                                        }
                                    } else if !*has_final_usage_for_inspect.lock().unwrap() {
                                        // Intermediate chunk — accumulate heuristic estimate
                                        *token_counter_for_inspect.lock().unwrap() += chunk_tokens;
                                    }
                                }
                            }
                        }
                    }
                });

                // Wrap the stream in a drop guard so that if the client
                // disconnects mid-stream, any partial token usage is still
                // recorded to PoNW / settlement.
                let guarded_stream = GuardedStream {
                    inner: counting_stream,
                    _guard: StreamingDropGuard {
                        token_counter: token_counter.clone(),
                        completed_normally: completed_normally.clone(),
                        model: model.clone(),
                        pown: state.pown.clone(),
                        settlement: state.settlement.clone(),
                        provider_id: state.provider_id.clone(),
                        provider_ergo_address: state.provider_ergo_address.clone(),
                    },
                };

                let body = Body::from_stream(guarded_stream);
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .header("Connection", "keep-alive")
                    .header("X-Accel-Buffering", "no")
                    .body(body)
                    .unwrap()
            } else {
                // Collect full response, update PoNW stats
                match resp.bytes().await {
                    Ok(body_bytes) => {
                        // Update PoNW AI stats from response
                        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                            update_pown_from_response(&state, &model, &json).await;
                        }

                        Response::builder()
                            .status(StatusCode::OK)
                            .header("Content-Type", "application/json")
                            .body(Body::from(body_bytes))
                            .unwrap()
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to read backend response");
                        error_response(
                            StatusCode::BAD_GATEWAY,
                            &format!("Backend read error: {}", e),
                        )
                    }
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(status = %status, "Backend returned error");
            match resp.bytes().await {
                Ok(body_bytes) => Response::builder()
                    .status(status)
                    .header("Content-Type", "application/json")
                    .body(Body::from(body_bytes))
                    .unwrap(),
                Err(_) => error_response(status, "Backend error (no body)"),
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to connect to backend");
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("LLM backend unavailable: {}", e),
            )
        }
    }
}

/// GET /v1/models — proxy to backend
async fn models_handler(State(state): State<InferenceState>) -> impl IntoResponse {
    let backend_url = format!("{}/v1/models", state.config.url.trim_end_matches('/'));

    match state
        .http_client
        .get(&backend_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.bytes().await {
            Ok(body) => {
                // Cache the first model name
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
                    if let Some(name) = extract_first_model(&json) {
                        *state.detected_model.write().await = Some(name);
                    }
                }
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap()
            }
            Err(e) => error_response(
                StatusCode::BAD_GATEWAY,
                &format!("Backend read error: {}", e),
            ),
        },
        Ok(resp) => {
            let status = resp.status();
            error_response(status, "Backend /v1/models error")
        }
        Err(e) => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            &format!("LLM backend unavailable: {}", e),
        ),
    }
}

/// Update PoNW AI stats from a non-streaming completion response.
/// Also records usage to the settlement engine for ERG payment tracking.
async fn update_pown_from_response(state: &InferenceState, model: &str, json: &serde_json::Value) {
    let usage = json.get("usage");
    let prompt_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = extract_completion_tokens(json);
    let total_tokens = prompt_tokens + completion_tokens;

    if total_tokens > 0 {
        state.pown.update_ai_stats(model, total_tokens, 1).await;
        info!(
            model = %model,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            "PoNW AI stats updated"
        );

        // Record usage to settlement engine for ERG payment tracking
        if let Some(ref settlement) = state.settlement {
            let (cost_per_1k, price_source) = settlement.resolve_cost_per_1k(model).await;
            let cost_nanoerg = total_tokens as u64 * cost_per_1k / 1000;
            settlement
                .record_usage(
                    &state.provider_id,
                    &state.provider_ergo_address,
                    prompt_tokens,
                    completion_tokens,
                    cost_nanoerg,
                )
                .await;

            // TODO: To use batch settlement instead of per-request record_usage,
            // call batch_settlement.add_payment(&user_addr, &provider_addr, cost_nanoerg, &model).await
            // here. The BatchSettlement accumulates payments and flushes consolidated
            // transactions per provider, reducing on-chain tx volume.

            info!(
                model = %model,
                cost_nanoerg = cost_nanoerg,
                price_source = price_source,
                "Usage recorded to settlement engine"
            );
        }
    }
}

/// Extract first model ID from OpenAI /v1/models response
fn extract_first_model(json: &serde_json::Value) -> Option<String> {
    json.get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

/// Extract completion tokens from an OpenAI-compatible response JSON.
///
/// Resolution order:
/// 1. `usage.completion_tokens` — exact count returned by the backend.
/// 2. Characters ÷ 4 heuristic applied to response content — a rough
///    approximation that works for both non-streaming (`choices[].message.content`)
///    and streaming chunks (`choices[].delta.content`).
///
/// # Heuristic accuracy
///
/// The chars/4 fallback assumes ~4 characters per token, which is a reasonable
/// average for English text tokenized with common BPE schemes (e.g. cl100k_base).
/// Code, non-English text, and repeated tokens will deviate significantly.
/// Ideally a proper tokenizer (tiktoken, HuggingFace `tokenizers`) should be
/// used for accurate counting, but the chars/4 heuristic is kept as a cheap
/// best-effort fallback when the backend does not report usage.
fn extract_completion_tokens(response_json: &serde_json::Value) -> u64 {
    // 1. Prefer exact usage field
    if let Some(ct) = response_json
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_u64())
    {
        if ct > 0 {
            return ct;
        }
    }

    // 2. Fall back to chars/4 heuristic on response content
    let mut total_chars: u64 = 0;
    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            // Non-streaming full response: choices[].message.content
            if let Some(content) = choice
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                total_chars += content.len() as u64;
            }
            // Streaming chunk: choices[].delta.content
            if let Some(content) = choice
                .get("delta")
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                total_chars += content.len() as u64;
            }
        }
    }

    // Ceiling division: (n + divisor - 1) / divisor
    total_chars.div_ceil(4)
}

fn error_response(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "message": message,
            "type": "xergon_agent_error",
            "code": status.as_u16(),
        }
    });
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// W7.22: Drop guard — records partial token usage on client disconnect
// ---------------------------------------------------------------------------

/// Drop guard that records partial token usage when a streaming response body
/// is dropped before the stream completed normally (i.e. client disconnect).
struct StreamingDropGuard {
    token_counter: Arc<std::sync::Mutex<u64>>,
    /// Set to `true` when the `[DONE]` sentinel is received, indicating the
    /// LLM finished generation and the normal PoNW update was already spawned.
    completed_normally: Arc<std::sync::Mutex<bool>>,
    model: String,
    pown: Arc<PownCalculator>,
    settlement: Option<Arc<crate::settlement::SettlementEngine>>,
    provider_id: String,
    provider_ergo_address: String,
}

impl Drop for StreamingDropGuard {
    fn drop(&mut self) {
        // If the stream completed normally, the PoNW update was already
        // spawned from the `[DONE]` handler — nothing to do here.
        if *self.completed_normally.lock().unwrap() {
            return;
        }

        let tokens = *self.token_counter.lock().unwrap();
        if tokens == 0 {
            return;
        }

        warn!(
            model = %self.model,
            completion_tokens = tokens,
            "Client disconnected during streaming; recording partial token usage"
        );

        // We cannot `.await` inside `Drop`, so spawn the async update via the
        // current tokio runtime handle.
        let handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h,
            Err(_) => return, // runtime shutting down — best-effort
        };

        let pown = self.pown.clone();
        let settlement = self.settlement.clone();
        let provider_id = self.provider_id.clone();
        let provider_ergo_address = self.provider_ergo_address.clone();
        let model = self.model.clone();

        handle.spawn(async move {
            pown.update_ai_stats(&model, tokens, 1).await;
            if let Some(ref s) = settlement {
                let (cost_per_1k, price_source) = s.resolve_cost_per_1k(&model).await;
                let cost_nanoerg = tokens as u64 * cost_per_1k / 1000;
                info!(
                    model = %model,
                    cost_nanoerg = cost_nanoerg,
                    price_source = price_source,
                    "Drop guard usage cost calculated"
                );
                s.record_usage(&provider_id, &provider_ergo_address, 0, tokens, cost_nanoerg)
                    .await;
            }
        });
    }
}

/// Wrapper around a byte stream that owns a [`StreamingDropGuard`].
///
/// When the response body is dropped (either after normal completion or due to
/// client disconnect), the guard's `Drop` impl fires and records any partial
/// token usage that wasn't captured by the normal `[DONE]` handler.
struct GuardedStream<S> {
    inner: S,
    _guard: StreamingDropGuard,
}

impl<S, E> Stream for GuardedStream<S>
where
    S: Stream<Item = Result<axum::body::Bytes, E>> + Unpin,
{
    type Item = Result<axum::body::Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

// ---------------------------------------------------------------------------
// Tests — W7 fixes: extract_completion_tokens, StreamingDropGuard
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper to build a minimal PownCalculator for tests.
    fn make_pown() -> Arc<PownCalculator> {
        let cfg = crate::config::XergonConfig {
            provider_id: "test_provider".into(),
            provider_name: "Test".into(),
            region: "us-east".into(),
            ergo_address: String::new(),
        };
        Arc::new(PownCalculator::new(cfg))
    }

    // -----------------------------------------------------------------------
    // extract_completion_tokens
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_completion_tokens_exact_usage() {
        let resp = json!({
            "usage": { "completion_tokens": 42 },
            "choices": [{}]
        });
        assert_eq!(extract_completion_tokens(&resp), 42);
    }

    #[test]
    fn test_extract_completion_tokens_no_usage_falls_back_to_chars_div_4() {
        // 16 chars of content → ceil(16/4) = 4 tokens
        let resp = json!({
            "choices": [{
                "message": { "content": "0123456789abcdef" }
            }]
        });
        assert_eq!(extract_completion_tokens(&resp), 4);
    }

    #[test]
    fn test_extract_completion_tokens_empty_content_returns_zero() {
        let resp = json!({
            "choices": [{ "message": { "content": "" } }]
        });
        assert_eq!(extract_completion_tokens(&resp), 0);
    }

    #[test]
    fn test_extract_completion_tokens_no_choices_returns_zero() {
        let resp = json!({});
        assert_eq!(extract_completion_tokens(&resp), 0);
    }

    #[test]
    fn test_extract_completion_tokens_streaming_delta() {
        // Streaming chunk: choices[].delta.content
        let resp = json!({
            "choices": [{
                "delta": { "content": "Hello world" }
            }]
        });
        // "Hello world" = 11 chars → ceil(11/4) = 3
        assert_eq!(extract_completion_tokens(&resp), 3);
    }

    #[test]
    fn test_extract_completion_tokens_usage_priority_over_content() {
        // When usage.completion_tokens is present and > 0, it wins
        // even if content suggests a different count.
        let resp = json!({
            "usage": { "completion_tokens": 100 },
            "choices": [{
                "message": { "content": "short" }
            }]
        });
        // "short" = 5 chars → ceil(5/4) = 2, but usage says 100
        assert_eq!(extract_completion_tokens(&resp), 100);
    }

    #[test]
    fn test_extract_completion_tokens_usage_zero_falls_back() {
        // usage.completion_tokens == 0 should fall back to heuristic
        let resp = json!({
            "usage": { "completion_tokens": 0 },
            "choices": [{
                "message": { "content": "abcdefgh" }
            }]
        });
        // 8 chars → ceil(8/4) = 2
        assert_eq!(extract_completion_tokens(&resp), 2);
    }

    #[test]
    fn test_extract_completion_tokens_multiple_choices() {
        let resp = json!({
            "choices": [
                { "message": { "content": "aaaa" } },
                { "message": { "content": "bbbb" } }
            ]
        });
        // 4 + 4 = 8 chars → ceil(8/4) = 2
        assert_eq!(extract_completion_tokens(&resp), 2);
    }

    // -----------------------------------------------------------------------
    // StreamingDropGuard
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_drop_guard_records_on_unexpected_drop() {
        let pown = make_pown();
        let token_counter: Arc<std::sync::Mutex<u64>> = Arc::new(std::sync::Mutex::new(50));
        let completed_normally: Arc<std::sync::Mutex<bool>> =
            Arc::new(std::sync::Mutex::new(false));

        // Snapshot AI tokens before drop
        let status_before = pown.status().read().await.ai_total_tokens;

        {
            let _guard = StreamingDropGuard {
                token_counter: token_counter.clone(),
                completed_normally: completed_normally.clone(),
                model: "test-model".into(),
                pown: pown.clone(),
                settlement: None,
                provider_id: "test_provider".into(),
                provider_ergo_address: String::new(),
            };
            // Guard goes out of scope without completed_normally being set to true
        }

        // Give the spawned task a moment to execute
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_after = pown.status().read().await.ai_total_tokens;
        assert_eq!(status_after, status_before + 50);
    }

    #[tokio::test]
    async fn test_drop_guard_noop_when_completed_normally() {
        let pown = make_pown();
        let token_counter: Arc<std::sync::Mutex<u64>> = Arc::new(std::sync::Mutex::new(50));
        let completed_normally: Arc<std::sync::Mutex<bool>> =
            Arc::new(std::sync::Mutex::new(false));

        // Snapshot AI tokens before
        let status_before = pown.status().read().await.ai_total_tokens;

        {
            let guard = StreamingDropGuard {
                token_counter: token_counter.clone(),
                completed_normally: completed_normally.clone(),
                model: "test-model".into(),
                pown: pown.clone(),
                settlement: None,
                provider_id: "test_provider".into(),
                provider_ergo_address: String::new(),
            };
            // Simulate normal completion — set the flag before drop
            *guard.completed_normally.lock().unwrap() = true;
        }

        // Give any spawned task a moment
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_after = pown.status().read().await.ai_total_tokens;
        // Should NOT have recorded — guard was a no-op
        assert_eq!(status_after, status_before);
    }

    #[tokio::test]
    async fn test_drop_guard_zero_tokens_is_noop() {
        let pown = make_pown();
        let token_counter: Arc<std::sync::Mutex<u64>> = Arc::new(std::sync::Mutex::new(0));
        let completed_normally: Arc<std::sync::Mutex<bool>> =
            Arc::new(std::sync::Mutex::new(false));

        let status_before = pown.status().read().await.ai_total_tokens;

        {
            let _guard = StreamingDropGuard {
                token_counter: token_counter.clone(),
                completed_normally: completed_normally.clone(),
                model: "test-model".into(),
                pown: pown.clone(),
                settlement: None,
                provider_id: "test_provider".into(),
                provider_ergo_address: String::new(),
            };
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_after = pown.status().read().await.ai_total_tokens;
        assert_eq!(status_after, status_before);
    }
}
