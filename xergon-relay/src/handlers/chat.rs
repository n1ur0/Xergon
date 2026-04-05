//! POST /v1/chat/completions — OpenAI-compatible chat completions
//!
//! Stateless handler: validates the request, checks user balance,
//! proxies to the best available provider with fallback chain,
//! stores an anonymous usage record in memory.

use axum::{extract::State, http::HeaderMap, response::Response};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::auth::AuthVerifier;
use crate::balance::BalanceError;
use crate::proxy::{proxy_chat_completion, ProxyError};
use crate::util::extract_client_ip;

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

/// POST /v1/chat/completions handler
pub async fn chat_completions_handler(
    State(state): State<crate::proxy::AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response, ProxyError> {
    // Extract request ID from middleware before consuming the request
    let request_id = request
        .extensions()
        .get::<crate::middleware::RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Increment chat request counter
    state.relay_metrics.inc_chat_requests();

    // Check raw body size limit (100KB)
    let body_bytes = axum::body::to_bytes(request.into_body(), 100_001)
        .await
        .map_err(|e| ProxyError::Validation(format!("Failed to read request body: {}", e)))?;
    if body_bytes.len() > 100_000 {
        return Err(ProxyError::Validation(
            "Request body exceeds 100KB limit".to_string(),
        ));
    }

    // Parse JSON body into typed request
    let req: ChatCompletionRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Validation(format!("Invalid JSON: {}", e)))?;
    // Re-serialize to Value for proxy forwarding
    let body: serde_json::Value = serde_json::to_value(&req)
        .map_err(|e| ProxyError::Validation(format!("Failed to serialize request: {}", e)))?;
    let model = req.model;

    let client_ip = extract_client_ip(&headers);

    info!(
        request_id = request_id,
        model = %model,
        ip = %client_ip,
        "Processing chat completion request"
    );

    // ── Signature-based auth verification ───────────────────────────
    // If auth is enabled and the request contains X-Xergon-* headers,
    // verify the signature before proceeding.
    let authenticated_public_key = if state.config.auth.enabled
        && AuthVerifier::has_auth_headers(&headers)
    {
        if let Some(verifier) = &state.auth_verifier {
            let auth = AuthVerifier::extract_auth(&headers)?;

            // Verify: timestamp freshness + replay protection
            verifier.verify(&auth, "POST", "/v1/chat/completions", &body_bytes)?;

            // If require_staking_box, check on-chain balance via BalanceChecker
            if verifier.requires_staking_box() {
                if let Some(checker) = &state.balance_checker {
                    match checker
                        .check_balance(&auth.public_key, state.config.balance.min_balance_nanoerg)
                        .await
                    {
                        Ok(result) => {
                            if !result.is_free_tier {
                                info!(
                                    public_key = %&auth.public_key[..auth.public_key.len().min(16)],
                                    balance_nanoerg = result.balance_nanoerg,
                                    boxes = result.staking_boxes_count,
                                    "Authenticated user staking balance verified"
                                );
                            }
                        }
                        Err(BalanceError::InsufficientBalance {
                            have_nanoerg,
                            min_nanoerg,
                        }) => {
                            warn!(
                                public_key = %&auth.public_key[..auth.public_key.len().min(16)],
                                have_nanoerg,
                                need_nanoerg = min_nanoerg,
                                "Authenticated user has no staking box — rejecting"
                            );
                            state.relay_metrics.inc_errors("402");
                            return Err(ProxyError::Unauthorized(format!(
                                "No staking box found for public key. Have {} nanoERG, need {} nanoERG.",
                                have_nanoerg, min_nanoerg,
                            )));
                        }
                        Err(e) => {
                            // Balance check failed (node down, etc.) — log but don't block.
                            warn!(
                                public_key = %&auth.public_key[..auth.public_key.len().min(16)],
                                error = %e,
                                "Staking box check failed — allowing request (fail-open)"
                            );
                        }
                    }
                }
            }

            info!(
                public_key = %&auth.public_key[..auth.public_key.len().min(16)],
                "Request authenticated via signature"
            );

            Some(auth.public_key)
        } else {
            // Auth config enabled but no verifier constructed — allow unauthenticated
            // (shouldn't happen in practice)
            warn!("Auth enabled but no AuthVerifier constructed — skipping auth check");
            None
        }
    } else {
        // No X-Xergon-* headers present — fall through to existing balance/free_tier behavior
        None
    };

    // ── Balance check middleware ──────────────────────────────────
    // If balance checking is enabled and a checker is configured,
    // verify the user has sufficient ERG staking balance before proxying.
    // This applies to unauthenticated requests (no X-Xergon-* headers).
    if state.config.balance.enabled && authenticated_public_key.is_none() {
        if let Some(checker) = &state.balance_checker {
            // Extract user identity: prefer X-User-PK header, fall back to IP
            let user_id = headers
                .get("x-user-pk")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| client_ip.clone());

            match checker
                .check_balance(&user_id, state.config.balance.min_balance_nanoerg)
                .await
            {
                Ok(result) => {
                    if !result.is_free_tier {
                        info!(
                            user = %user_id,
                            balance_nanoerg = result.balance_nanoerg,
                            boxes = result.staking_boxes_count,
                            "User balance verified"
                        );
                    }
                }
                Err(BalanceError::InsufficientBalance {
                    have_nanoerg,
                    min_nanoerg,
                }) => {
                    warn!(
                        user = %user_id,
                        have_nanoerg,
                        need_nanoerg = min_nanoerg,
                        "Insufficient ERG balance — rejecting request"
                    );
                    state.relay_metrics.inc_errors("402");
                    return Err(ProxyError::InsufficientBalance(format!(
                        "Insufficient ERG balance. Have {} nanoERG (~{:.6} ERG), need {} nanoERG (~{:.6} ERG). Send ERG to your staking address to continue.",
                        have_nanoerg,
                        have_nanoerg as f64 / 1_000_000_000.0,
                        min_nanoerg,
                        min_nanoerg as f64 / 1_000_000_000.0,
                    )));
                }
                Err(e) => {
                    // Balance check failed (node down, etc.) — log but don't block.
                    // Fail-open so the relay stays functional even if the node is down.
                    warn!(
                        user = %user_id,
                        error = %e,
                        "Balance check failed — allowing request (fail-open)"
                    );
                }
            }
        }
    }

    // ── Free tier check ──────────────────────────────────────────
    // If the user has no ERG balance (free tier), check the free tier tracker.
    // Users with ERG balance > 0 bypass this check entirely.
    if let Some(tracker) = &state.free_tier_tracker {
        // Determine the user key for free tier tracking.
        // Prefer the authenticated public key, then X-User-PK header, then IP.
        let free_tier_key = authenticated_public_key
            .as_deref()
            .or_else(|| {
                headers
                    .get("x-user-pk")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or(&client_ip);

        // Only apply free tier limits to users we can identify by key (not raw IP).
        // This avoids applying free tier limits to unauthenticated requests with no
        // public key, which would be overly restrictive.
        if authenticated_public_key.is_some()
            || headers.get("x-user-pk").is_some()
        {
            // Check if the user has ERG balance (paying user) by querying balance checker
            let has_erg_balance = if let Some(checker) = &state.balance_checker {
                match checker.check_balance(free_tier_key, 0).await {
                    Ok(result) => result.balance_nanoerg > 0,
                    Err(_) => false, // Assume no balance on error
                }
            } else {
                false // No checker → assume free tier
            };

            if !has_erg_balance {
                // User has 0 ERG — apply free tier limit
                match tracker.check_and_increment(free_tier_key) {
                    crate::free_tier::FreeTierCheck::Free => {
                        // Within free tier quota — proceed
                    }
                    crate::free_tier::FreeTierCheck::Exhausted { used, limit } => {
                        warn!(
                            user = %free_tier_key,
                            used,
                            limit,
                            "Free tier exhausted — rejecting request"
                        );
                        state.relay_metrics.inc_errors("402");
                        return Err(ProxyError::InsufficientBalance(format!(
                            "Free tier limit reached ({} requests). Deposit ERG to continue.",
                            limit
                        )));
                    }
                }
            }
        }
    }

    // Proxy to provider with fallback chain
    let proxy_result = match proxy_chat_completion(&state, body, &headers, &request_id).await {
        Ok(result) => result,
        Err(e) => {
            // Record error metrics
            match &e {
                ProxyError::NoProviders => {
                    state.relay_metrics.inc_errors("503");
                }
                ProxyError::InsufficientBalance(_) => {
                    state.relay_metrics.inc_errors("402");
                }
                ProxyError::AllProvidersFailed { .. } => {
                    state.relay_metrics.inc_errors("502");
                }
                ProxyError::Validation(_) => {
                    state.relay_metrics.inc_errors("400");
                }
                ProxyError::Unauthorized(_) => {
                    state.relay_metrics.inc_errors("401");
                }
                ProxyError::Http(_) => {
                    state.relay_metrics.inc_errors("502");
                }
            }
            return Err(e);
        }
    };

    // Record successful request latency
    state.relay_metrics.observe_request_latency_ms(proxy_result.latency_ms);

    // Compute rarity multiplier for this model
    let rarity_multiplier = if state.config.incentive.rarity_bonus_enabled {
        crate::provider::model_rarity_from_registry(
            &state.provider_registry,
            &model,
            state.config.incentive.rarity_max_multiplier,
        )
    } else {
        1.0
    };

    if rarity_multiplier > 1.0 {
        info!(
            model = %model,
            rarity_multiplier = rarity_multiplier,
            "Rare model bonus applied"
        );
    }

    // Store anonymous usage record in memory
    let record = crate::proxy::UsageRecord {
        request_id: request_id.to_string(),
        ip: client_ip,
        model: model.clone(),
        tokens_in: proxy_result
            .usage
            .as_ref()
            .map(|u| u.prompt_tokens.load(std::sync::atomic::Ordering::Relaxed) as u32)
            .unwrap_or(0),
        tokens_out: proxy_result
            .usage
            .as_ref()
            .map(|u| {
                u.completion_tokens
                    .load(std::sync::atomic::Ordering::Relaxed) as u32
            })
            .unwrap_or(0),
        provider: proxy_result.provider.clone(),
        latency_ms: proxy_result.latency_ms,
        created_at: chrono::Utc::now(),
        rarity_multiplier,
    };
    state
        .usage_store
        .insert(record.request_id.clone(), record);

    info!(
        request_id = request_id,
        status = "success",
        tokens_in = proxy_result.usage.as_ref().map(|u| u.prompt_tokens.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(0),
        tokens_out = proxy_result.usage.as_ref().map(|u| u.completion_tokens.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(0),
        provider = %proxy_result.provider,
        latency_ms = proxy_result.latency_ms,
        "Chat completion done"
    );

    // Record demand for this model
    state.demand.record(&model, 1);

    Ok(proxy_result.response)
}
