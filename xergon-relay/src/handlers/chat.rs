//! POST /v1/chat/completions — OpenAI-compatible chat completions
//!
//! Validates the request, checks auth/tier rate limits, deducts credits
//! for authenticated users, and proxies to the best available provider
//! with fallback chain.

use axum::{
    extract::State,
    http::HeaderMap,
    response::Response,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::proxy::{proxy_chat_completion, ProxyError, ReconcileInfo};
use crate::util::{extract_client_ip, hash_ip};

/// OpenAI-compatible chat completion request
#[derive(Debug, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub stop: Option<serde_json::Value>,
    pub user: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: serde_json::Value,
}

/// OpenAI-compatible error response
#[allow(dead_code)] // TODO: planned for structured error responses
#[derive(Debug, Serialize)]
pub struct ChatCompletionError {
    pub error: ErrorDetail,
}

#[allow(dead_code)] // TODO: planned for structured error responses
#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: Option<u16>,
}

/// Rate limit tiers (requests per window)
/// free: 10/day, pro: 10000/month
struct TierLimits {
    max_requests: u32,
    window_secs: u64,
}

fn tier_limits(tier: &str) -> TierLimits {
    match tier {
        "pro" => TierLimits {
            max_requests: 10_000,
            window_secs: 30 * 24 * 60 * 60, // 30 days
        },
        _ => TierLimits {
            max_requests: 10,
            window_secs: 24 * 60 * 60, // 1 day
        },
    }
}

/// Cost per 1K tokens in USD — used for credit deduction
/// Configurable via RelayConfig, defaults to $0.002/1K tokens
fn cost_per_token(cost_per_1k: f64) -> f64 {
    cost_per_1k / 1000.0
}

/// POST /v1/chat/completions handler
pub async fn chat_completions_handler(
    State(state): State<crate::proxy::AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response, ProxyError> {
    // Check raw body size limit (100KB)
    let body_bytes = axum::body::to_bytes(request.into_body(), 100_001).await
        .map_err(|e| ProxyError::Validation(format!("Failed to read request body: {}", e)))?;
    if body_bytes.len() > 100_000 {
        return Err(ProxyError::Validation(
            "Request body exceeds 100KB limit".to_string(),
        ));
    }

    // Parse JSON body into typed request
    let req: ChatCompletionRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Validation(format!("Invalid JSON: {}", e)))?;
    // Re-serialize to Value for proxy forwarding and token estimation
    let body: serde_json::Value = serde_json::to_value(&req)
        .map_err(|e| ProxyError::Validation(format!("Failed to serialize request: {}", e)))?;
    let model = req.model;

    let client_ip = extract_client_ip(&headers);

    // Try to extract JWT claims or API key (auth is optional — anonymous users get limited access)
    let auth_result = crate::auth::authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db);
    // authenticate_request returns Result<Option<AuthIdentity>, AppError>
    // Flatten: is_authenticated = true only when we get Some(identity)
    let (is_authenticated, claims) = match auth_result {
        Ok(Some(ident)) => (true, Some(ident)),
        Ok(None) => (false, None),
        Err(e) => {
            // Invalid token provided — log but don't reject (anonymous allowed)
            tracing::debug!(err = ?e, "Auth provided but invalid, treating as anonymous");
            (false, None)
        }
    };

    if is_authenticated {
        let user_claims = claims.as_ref().unwrap();

        // ── Per-tier rate limiting for authenticated users ──
        let limits = tier_limits(&user_claims.tier);
        let rate_key = format!("user:{}", user_claims.sub);
        let (allowed, remaining) =
            state
                .rate_limiter
                .check_with_window(&rate_key, limits.max_requests, limits.window_secs);
        if !allowed {
            info!(
                user_id = %user_claims.sub,
                tier = %user_claims.tier,
                "User rate limit exceeded"
            );
            return Err(ProxyError::TierRateLimited {
                tier: user_claims.tier.clone(),
                reset_hint: format!(
                    "{} requests per {} days",
                    limits.max_requests,
                    limits.window_secs / 86400
                ),
            });
        }

        // ── Credit balance check for authenticated users ──
        let max_tokens_requested = body
            .get("max_tokens")
            .and_then(|m| m.as_u64())
            .unwrap_or(1024) as u32;

        // Estimate cost: input tokens (from messages) + output tokens
        let estimated_input_tokens = estimate_input_tokens(&body);
        let total_estimated_tokens =
            (estimated_input_tokens + max_tokens_requested as u64) as u64;
        let estimated_cost =
            total_estimated_tokens as f64 * cost_per_token(state.config.credits.cost_per_1k_tokens);

        // Check balance before proxying
        let balance = state
            .db
            .get_credit_balance(&user_claims.sub)
            .unwrap_or(0.0);

        if balance < estimated_cost {
            return Err(ProxyError::InsufficientCredits {
                balance_usd: balance,
                estimated_cost_usd: estimated_cost,
            });
        }

        info!(
            model = %model,
            user_id = %user_claims.sub,
            tier = %user_claims.tier,
            estimated_tokens = total_estimated_tokens,
            estimated_cost = estimated_cost,
            balance = balance,
            remaining_rate = remaining,
            "Processing authenticated chat completion request"
        );
    } else {
        // ── Anonymous rate limiting (IP-based) ──
        let (allowed, remaining) = state.rate_limiter.check(&client_ip);
        if !allowed {
            return Err(ProxyError::RateLimited);
        }

        // Anonymous max tokens enforcement
        let max_tokens = body
            .get("max_tokens")
            .and_then(|m| m.as_u64())
            .unwrap_or(500);

        if max_tokens
            > state.config.relay.anonymous_max_tokens_per_request as u64
        {
            return Err(ProxyError::Validation(format!(
                "Anonymous users are limited to {} tokens per request. Sign in for higher limits.",
                state.config.relay.anonymous_max_tokens_per_request
            )));
        }

        info!(
            model = %model,
            ip = %client_ip,
            remaining = remaining,
            "Processing anonymous chat completion request"
        );
    }

    // Proxy to provider with fallback chain
    // For streaming with authenticated users: build reconcile_info before the proxy call
    // so the stream wrapper can reconcile credits after the client finishes consuming.
    let is_stream = req.stream;

    let reconcile_info_for_proxy = if is_stream {
        if let Some(user_claims) = &claims {
            let cost_per_1k = state.config.credits.cost_per_1k_tokens;
            let cpt = cost_per_1k / 1000.0;
            let estimated_input_tokens = estimate_input_tokens(&body);
            let max_tokens_requested = body
                .get("max_tokens")
                .and_then(|m| m.as_u64())
                .unwrap_or(1024) as u64;
            let estimated_cost = (estimated_input_tokens + max_tokens_requested) as f64 * cpt;

            Some(ReconcileInfo {
                user_id: user_claims.sub.clone(),
                estimated_cost_usd: estimated_cost,
                cost_per_1k_tokens: cost_per_1k,
                model: model.clone(),
                db: state.db.clone(),
                reconciled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
        } else {
            None
        }
    } else {
        None
    };

    let proxy_result = proxy_chat_completion(&state, body.clone(), &headers, reconcile_info_for_proxy).await?;

    // ── Deduct credits and build response ──
    // For streaming: pre-authorize (deduct estimated cost), reconciliation is handled by stream wrapper.
    // For non-streaming: deduct based on actual usage from the complete response.
    if let Some(user_claims) = &claims {
        let cost_per_1k = state.config.credits.cost_per_1k_tokens;
        let cpt = cost_per_1k / 1000.0;

        if is_stream {
            // ── Streaming: pre-authorize estimated cost ──
            // (Reconciliation happens automatically when the stream wrapper completes)
            let estimated_input_tokens = estimate_input_tokens(&body);
            let max_tokens_requested = body
                .get("max_tokens")
                .and_then(|m| m.as_u64())
                .unwrap_or(1024) as u64;
            let total_estimated_tokens = estimated_input_tokens + max_tokens_requested;
            let estimated_cost = total_estimated_tokens as f64 * cpt;

            let tx_id = uuid::Uuid::new_v4().to_string();
            let desc = format!(
                "Pre-auth: {} (est {} in + {} out = {} total, stream)",
                model, estimated_input_tokens, max_tokens_requested, total_estimated_tokens
            );

            match state.db.deduct_credits(&tx_id, &user_claims.sub, estimated_cost, &desc) {
                Ok(balance_after) => {
                    info!(
                        user_id = %user_claims.sub,
                        pre_auth = estimated_cost,
                        balance_after = balance_after,
                        "Credits pre-authorized for streaming inference"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        user_id = %user_claims.sub,
                        error = %e,
                        "Failed to pre-authorize credits for streaming inference"
                    );
                }
            }

            // Store usage record (pre-auth)
            let record = crate::proxy::UsageRecord {
                request_id: tx_id.clone(),
                ip: client_ip.clone(),
                model: model.clone(),
                tokens_in: estimated_input_tokens as u32,
                tokens_out: 0,
                provider: proxy_result.provider.clone(),
                latency_ms: proxy_result.latency_ms,
                created_at: chrono::Utc::now(),
                is_anonymous: false,
            };
            state.usage_store.insert(record.request_id.clone(), record);

            // Persist to DB
            let hashed_ip = hash_ip(&client_ip);
            if let Err(e) = state.db.insert_usage_record(
                Some(&user_claims.sub),
                &proxy_result.provider,
                &model,
                estimated_input_tokens as i64,
                0,
                estimated_cost,
                estimated_cost,
                &user_claims.tier,
                Some(&hashed_ip),
            ) {
                tracing::warn!(error = %e, "Failed to persist usage record to DB");
            }

            // Return the streaming response (reconciliation is handled by the stream wrapper)
            Ok(proxy_result.response)
        } else {
            // ── Non-streaming: deduct based on actual usage ──
            let (actual_cost, description) = match &proxy_result.usage {
                Some(usage) => {
                    let snap = usage.snapshot();
                    if snap.total_tokens > 0 {
                        let cost = snap.total_tokens as f64 * cpt;
                        let desc = format!(
                            "Inference: {} ({} in + {} out = {} total)",
                            model, snap.prompt_tokens, snap.completion_tokens, snap.total_tokens
                        );
                        (cost, desc)
                    } else if snap.completion_tokens > 0 {
                        let estimated_input = estimate_input_tokens(&body) as u64;
                        let total = estimated_input + snap.completion_tokens;
                        let cost = total as f64 * cpt;
                        let desc = format!(
                            "Inference: {} (~{} in + {} out = ~{} total, streamed)",
                            model, estimated_input, snap.completion_tokens, total
                        );
                        (cost, desc)
                    } else {
                        let cost = 0.001_f64.max(cpt * 100.0);
                        let desc = format!("Inference: {} (minimum estimate)", model);
                        (cost, desc)
                    }
                }
                None => {
                    let cost = 0.001_f64.max(cpt * 100.0); // ~100 tokens minimum
                    let desc = format!("Inference: {} (minimum estimate)", model);
                    (cost, desc)
                }
            };

            let tx_id = uuid::Uuid::new_v4().to_string();

            match state.db.deduct_credits(&tx_id, &user_claims.sub, actual_cost, &description) {
                Ok(balance_after) => {
                    info!(
                        user_id = %user_claims.sub,
                        deducted = actual_cost,
                        balance_after = balance_after,
                        "Credits deducted for inference"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        user_id = %user_claims.sub,
                        error = %e,
                        "Failed to deduct credits after successful inference"
                    );
                }
            }

            // Store usage record
            let record = crate::proxy::UsageRecord {
                request_id: tx_id,
                ip: client_ip.clone(),
                model: model.clone(),
                tokens_in: proxy_result
                    .usage
                    .as_ref()
                    .map(|u| u.prompt_tokens.load(std::sync::atomic::Ordering::Relaxed) as u32)
                    .unwrap_or(0),
                tokens_out: proxy_result
                    .usage
                    .as_ref()
                    .map(|u| u.completion_tokens.load(std::sync::atomic::Ordering::Relaxed) as u32)
                    .unwrap_or(0),
                provider: proxy_result.provider.clone(),
                latency_ms: proxy_result.latency_ms,
                created_at: chrono::Utc::now(),
                is_anonymous: false,
            };
            state
                .usage_store
                .insert(record.request_id.clone(), record);

            // Persist usage to database
            let hashed_ip = hash_ip(&client_ip);
            let prompt_tokens = proxy_result
                .usage
                .as_ref()
                .map(|u| u.prompt_tokens.load(std::sync::atomic::Ordering::Relaxed) as i64)
                .unwrap_or(0);
            let completion_tokens = proxy_result
                .usage
                .as_ref()
                .map(|u| u.completion_tokens.load(std::sync::atomic::Ordering::Relaxed) as i64)
                .unwrap_or(0);
            if let Err(e) = state.db.insert_usage_record(
                Some(&user_claims.sub),
                &proxy_result.provider,
                &model,
                prompt_tokens,
                completion_tokens,
                actual_cost,
                actual_cost, // cost_usd equals cost_credits here (both in USD)
                &user_claims.tier,
                Some(&hashed_ip),
            ) {
                tracing::warn!(error = %e, "Failed to persist usage record to DB");
            }

            Ok(proxy_result.response)
        }
    } else {
        // ── Anonymous user: store usage record, no credit deduction ──
        let record = crate::proxy::UsageRecord {
            request_id: uuid::Uuid::new_v4().to_string(),
            ip: client_ip.clone(),
            model: model.clone(),
            tokens_in: 0,
            tokens_out: 0,
            provider: proxy_result.provider.clone(),
            latency_ms: proxy_result.latency_ms,
            created_at: chrono::Utc::now(),
            is_anonymous: true,
        };
        state
            .usage_store
            .insert(record.request_id.clone(), record);

        // Persist anonymous usage to database
        let hashed_ip = hash_ip(&client_ip);
        if let Err(e) = state.db.insert_usage_record(
            None,
            &proxy_result.provider,
            &model,
            0,
            0,
            0.0,
            0.0,
            "anonymous",
            Some(&hashed_ip),
        ) {
            tracing::warn!(error = %e, "Failed to persist anonymous usage record to DB");
        }

        Ok(proxy_result.response)
    }
}

/// Rough estimate of input tokens from messages array.
/// Real implementation would use a tokenizer (tiktoken).
fn estimate_input_tokens(body: &serde_json::Value) -> u64 {
    let messages = match body.get("messages").and_then(|m| m.as_array()) {
        Some(arr) => arr,
        None => return 10,
    };

    // Rough heuristic: ~4 chars per token on average
    let total_chars: usize = messages
        .iter()
        .filter_map(|msg| {
            let content = msg.get("content")?;
            match content {
                serde_json::Value::String(s) => Some(s.len()),
                serde_json::Value::Array(parts) => {
                    // Handle OpenAI's multi-part content format
                    Some(
                        parts
                            .iter()
                            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                            .map(|t| t.len())
                            .sum(),
                    )
                }
                _ => None,
            }
        })
        .sum();

    // 4 chars ≈ 1 token, plus overhead per message
    let message_overhead = messages.len() * 4; // role + formatting tokens
    ((total_chars / 4) + message_overhead) as u64
}
