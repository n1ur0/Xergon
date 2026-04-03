//! POST /v1/inference and POST /v1/inference/stream
//!
//! Simple inference API that converts the Xergon request format
//! (model + prompt) into OpenAI-compatible chat completions format.
//! This lets the marketplace frontend use a clean API while the relay
//! proxies to providers using the standard OpenAI format.

use axum::{
    extract::State,
    http::HeaderMap,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::proxy::{proxy_chat_completion, AppState, ProxyError, ReconcileInfo};
use crate::util::{extract_client_ip, hash_ip};

/// Simple inference request (marketplace format)
#[derive(Debug, Deserialize)]
pub struct InferenceRequest {
    pub model: String,
    pub prompt: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

/// Simple inference response
#[derive(Debug, Serialize)]
pub struct InferenceResponse {
    pub id: String,
    pub content: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits_charged: f64,
}

/// Convert InferenceRequest to OpenAI chat completion format
fn to_chat_body(req: &InferenceRequest, stream: bool) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": req.model,
        "messages": [{ "role": "user", "content": req.prompt }],
        "stream": stream,
    });

    if let Some(max_tokens) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    body
}

/// Extract content and token counts from an OpenAI chat completion response
fn extract_response(body: &serde_json::Value) -> Option<(String, u64, u64)> {
    let choices = body.get("choices")?.as_array()?.first()?;
    let message = choices.get("message")?;
    let content = message.get("content")?.as_str()?.to_string();

    let usage = body.get("usage");
    let prompt_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Some((content, prompt_tokens, completion_tokens))
}

/// POST /v1/inference — simple non-streaming inference
pub async fn inference_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<InferenceRequest>,
) -> Result<Json<InferenceResponse>, ProxyError> {
    info!(
        model = %req.model,
        prompt_len = req.prompt.len(),
        "Processing simple inference request"
    );

    // Check prompt length limit (100KB)
    if req.prompt.len() > 100_000 {
        return Err(ProxyError::Validation(
            "Prompt exceeds 100KB limit".to_string(),
        ));
    }

    let client_ip = extract_client_ip(&headers);

    // Try to extract JWT claims (auth is optional — anonymous users get limited access)
    let auth_result = crate::auth::authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db);
    let is_authenticated = auth_result.is_ok();
    let identity = auth_result.unwrap_or(None);

    if is_authenticated {
        let identity = identity.as_ref().unwrap();

        // Per-tier rate limiting
        let (max_requests, window_secs) = match identity.tier.as_str() {
            "pro" => (10_000u32, 30 * 24 * 60 * 60),
            _ => (10u32, 24 * 60 * 60),
        };
        let rate_key = format!("user:{}", identity.sub);
        let (allowed, _) =
            state
                .rate_limiter
                .check_with_window(&rate_key, max_requests, window_secs);
        if !allowed {
            return Err(ProxyError::TierRateLimited {
                tier: identity.tier.clone(),
                reset_hint: format!("{} requests per {} days", max_requests, window_secs / 86400),
            });
        }

        // Credit balance check
        let cost_per_token = state.config.credits.cost_per_1k_tokens / 1000.0;
        let max_tokens = req.max_tokens.unwrap_or(1024) as u64;
        let estimated_input = (req.prompt.len() / 4) as u64;
        let estimated_cost = (estimated_input + max_tokens) as f64 * cost_per_token;

        let balance = state.db.get_credit_balance(&identity.sub).unwrap_or(0.0);
        if balance < estimated_cost {
            return Err(ProxyError::InsufficientCredits {
                balance_usd: balance,
                estimated_cost_usd: estimated_cost,
            });
        }
    } else {
        // Anonymous rate limiting
        let (allowed, _) = state.rate_limiter.check(&client_ip);
        if !allowed {
            return Err(ProxyError::RateLimited);
        }
    }

    let chat_body = to_chat_body(&req, false);
    let proxy_result = proxy_chat_completion(&state, chat_body, &headers, None).await?;

    // Extract the response body
    let status = proxy_result.response.status();
    let body_bytes = axum::body::to_bytes(proxy_result.response.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| ProxyError::Validation(format!("Failed to read response: {}", e)))?;

    if !status.is_success() {
        let error_json: serde_json::Value =
            serde_json::from_slice(&body_bytes).unwrap_or_else(|_| {
                serde_json::json!({ "error": { "message": String::from_utf8_lossy(&body_bytes) } })
            });
        let msg = error_json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Inference failed");
        return Err(ProxyError::Validation(msg.to_string()));
    }

    let json: serde_json::Value =
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::json!({}));

    let (content, input_tokens, output_tokens) =
        extract_response(&json).unwrap_or_else(|| (String::new(), 0, 0));

    // Calculate and deduct credits for authenticated users
    let credits_charged = if is_authenticated {
        let identity = identity.as_ref().unwrap();
        let cost_per_token = state.config.credits.cost_per_1k_tokens / 1000.0;
        let total_tokens = input_tokens + output_tokens;
        let cost = if total_tokens > 0 {
            total_tokens as f64 * cost_per_token
        } else {
            let estimated = (req.prompt.len() / 4) as u64 + 100;
            estimated as f64 * cost_per_token
        };

        let tx_id = uuid::Uuid::new_v4().to_string();
        let desc = format!(
            "Inference: {} ({} in + {} out = {} total)",
            req.model, input_tokens, output_tokens, total_tokens.max(input_tokens + output_tokens)
        );
        match state.db.deduct_credits(&tx_id, &identity.sub, cost, &desc) {
            Ok(balance_after) => {
                info!(
                    user_id = %identity.sub,
                    deducted = cost,
                    balance_after = balance_after,
                    "Credits deducted for inference"
                );
            }
            Err(e) => {
                tracing::warn!(
                    user_id = %identity.sub,
                    error = %e,
                    "Failed to deduct credits after successful inference"
                );
            }
        }

        cost
    } else {
        0.0
    };

    // Store usage record (in-memory + DB persistence)
    let record = crate::proxy::UsageRecord {
        request_id: uuid::Uuid::new_v4().to_string(),
        ip: client_ip.clone(),
        model: req.model.clone(),
        tokens_in: input_tokens as u32,
        tokens_out: output_tokens as u32,
        provider: proxy_result.provider.clone(),
        latency_ms: proxy_result.latency_ms,
        created_at: chrono::Utc::now(),
        is_anonymous: !is_authenticated,
    };
    state.usage_store.insert(record.request_id.clone(), record);

    // Persist usage to database
    let hashed_ip = hash_ip(&client_ip);
    let user_id = if is_authenticated {
        Some(identity.as_ref().unwrap().sub.as_str())
    } else {
        None
    };
    let tier = if is_authenticated {
        identity.as_ref().unwrap().tier.as_str()
    } else {
        "anonymous"
    };
    if let Err(e) = state.db.insert_usage_record(
        user_id,
        &proxy_result.provider,
        &req.model,
        input_tokens as i64,
        output_tokens as i64,
        credits_charged,
        credits_charged, // cost_usd equals cost_credits here (both in USD)
        tier,
        Some(&hashed_ip),
    ) {
        tracing::warn!(error = %e, "Failed to persist usage record to DB");
    }

    Ok(Json(InferenceResponse {
        id: json
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or("unknown")
            .to_string(),
        content,
        model: req.model,
        input_tokens,
        output_tokens,
        credits_charged,
    }))
}

/// POST /v1/inference/stream — streaming inference (SSE)
pub async fn inference_stream_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<InferenceRequest>,
) -> Result<Response, ProxyError> {
    info!(
        model = %req.model,
        prompt_len = req.prompt.len(),
        "Processing streaming inference request"
    );

    // Check prompt length limit (100KB)
    if req.prompt.len() > 100_000 {
        return Err(ProxyError::Validation(
            "Prompt exceeds 100KB limit".to_string(),
        ));
    }

    let client_ip = extract_client_ip(&headers);

    // Try to extract JWT claims (auth is optional)
    let auth_result = crate::auth::authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db);
    let is_authenticated = auth_result.is_ok();
    let identity = auth_result.unwrap_or(None);

    if is_authenticated {
        let identity = identity.as_ref().unwrap();

        // Per-tier rate limiting
        let (max_requests, window_secs) = match identity.tier.as_str() {
            "pro" => (10_000u32, 30 * 24 * 60 * 60),
            _ => (10u32, 24 * 60 * 60),
        };
        let rate_key = format!("user:{}", identity.sub);
        let (allowed, _) =
            state
                .rate_limiter
                .check_with_window(&rate_key, max_requests, window_secs);
        if !allowed {
            return Err(ProxyError::TierRateLimited {
                tier: identity.tier.clone(),
                reset_hint: format!("{} requests per {} days", max_requests, window_secs / 86400),
            });
        }

        // Credit balance check
        let cost_per_token = state.config.credits.cost_per_1k_tokens / 1000.0;
        let max_tokens = req.max_tokens.unwrap_or(1024) as u64;
        let estimated_input = (req.prompt.len() / 4) as u64;
        let estimated_cost = (estimated_input + max_tokens) as f64 * cost_per_token;

        let balance = state.db.get_credit_balance(&identity.sub).unwrap_or(0.0);
        if balance < estimated_cost {
            return Err(ProxyError::InsufficientCredits {
                balance_usd: balance,
                estimated_cost_usd: estimated_cost,
            });
        }
    } else {
        // Anonymous rate limiting
        let (allowed, _) = state.rate_limiter.check(&client_ip);
        if !allowed {
            return Err(ProxyError::RateLimited);
        }
    }

    let chat_body = to_chat_body(&req, true);

    // Build reconcile_info before proxy call so the stream wrapper can reconcile after completion
    let reconcile_info_for_proxy = if let Some(identity) = &identity {
        let cost_per_token = state.config.credits.cost_per_1k_tokens / 1000.0;
        let estimated_input = (req.prompt.len() / 4) as u64;
        let max_tokens = req.max_tokens.unwrap_or(1024) as u64;
        let estimated_cost = (estimated_input + max_tokens) as f64 * cost_per_token;

        Some(ReconcileInfo {
            user_id: identity.sub.clone(),
            estimated_cost_usd: estimated_cost,
            cost_per_1k_tokens: state.config.credits.cost_per_1k_tokens,
            model: req.model.clone(),
            db: state.db.clone(),
            reconciled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    } else {
        None
    };

    let proxy_result = proxy_chat_completion(&state, chat_body, &headers, reconcile_info_for_proxy).await?;

    // ── Streaming: pre-authorize estimated cost ──
    // (Reconciliation happens automatically when the stream wrapper completes)
    // Also persist usage records to DB
    let hashed_ip = hash_ip(&client_ip);

    if let Some(identity) = &identity {
        let cost_per_token = state.config.credits.cost_per_1k_tokens / 1000.0;
        let estimated_input = (req.prompt.len() / 4) as u64;
        let max_tokens = req.max_tokens.unwrap_or(1024) as u64;
        let estimated_cost = (estimated_input + max_tokens) as f64 * cost_per_token;

        let tx_id = uuid::Uuid::new_v4().to_string();
        let desc = format!(
            "Pre-auth (stream): {} (est {} in + {} out = {} total)",
            req.model, estimated_input, max_tokens, estimated_input + max_tokens
        );
        match state.db.deduct_credits(&tx_id, &identity.sub, estimated_cost, &desc) {
            Ok(balance_after) => {
                info!(
                    user_id = %identity.sub,
                    pre_auth = estimated_cost,
                    balance_after = balance_after,
                    "Credits pre-authorized for streaming inference"
                );
            }
            Err(e) => {
                tracing::warn!(
                    user_id = %identity.sub,
                    error = %e,
                    "Failed to pre-authorize credits for streaming inference"
                );
            }
        }

        // Store usage record (pre-auth)
        let record = crate::proxy::UsageRecord {
            request_id: tx_id.clone(),
            ip: client_ip.clone(),
            model: req.model.clone(),
            tokens_in: estimated_input as u32,
            tokens_out: 0,
            provider: proxy_result.provider.clone(),
            latency_ms: proxy_result.latency_ms,
            created_at: chrono::Utc::now(),
            is_anonymous: false,
        };
        state.usage_store.insert(record.request_id.clone(), record);

        // Persist to DB
        if let Err(e) = state.db.insert_usage_record(
            Some(&identity.sub),
            &proxy_result.provider,
            &req.model,
            estimated_input as i64,
            0,
            estimated_cost,
            estimated_cost,
            &identity.tier,
            Some(&hashed_ip),
        ) {
            tracing::warn!(error = %e, "Failed to persist usage record to DB");
        }

        // Return the streaming response (reconciliation is handled by the stream wrapper)
        Ok(proxy_result.response)
    } else {
        // Anonymous streaming — no credit deduction
        let record = crate::proxy::UsageRecord {
            request_id: uuid::Uuid::new_v4().to_string(),
            ip: client_ip.clone(),
            model: req.model.clone(),
            tokens_in: 0,
            tokens_out: 0,
            provider: proxy_result.provider.clone(),
            latency_ms: proxy_result.latency_ms,
            created_at: chrono::Utc::now(),
            is_anonymous: true,
        };
        state.usage_store.insert(record.request_id.clone(), record);

        // Persist anonymous usage to DB
        if let Err(e) = state.db.insert_usage_record(
            None,
            &proxy_result.provider,
            &req.model,
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
