//! POST /v1/images/generations -- OpenAI-compatible image generation proxy
//!
//! Stateless handler: validates the request, proxies to the best available
//! provider with fallback chain, stores an anonymous usage record in memory.

use axum::{body::Body, extract::State, http::{HeaderMap, HeaderValue, StatusCode}, response::Response};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, info_span};

use crate::proxy::{ProxyError, AppState, ProxyResult};
use crate::util::extract_client_ip;

/// OpenAI-compatible image generation request
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ImageGenerationRequest {
    pub model: String,
    pub prompt: String,
    pub n: Option<u32>,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub response_format: Option<String>,
    pub style: Option<String>,
    pub user: Option<String>,
}

/// POST /v1/images/generations handler
pub async fn images_generations_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response, ProxyError> {
    // Extract request ID from middleware before consuming the request
    let request_id = request
        .extensions()
        .get::<crate::middleware::RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Check raw body size limit (100KB -- prompts can be long)
    let body_bytes = axum::body::to_bytes(request.into_body(), 100_001)
        .await
        .map_err(|e| ProxyError::Validation(format!("Failed to read request body: {}", e)))?;
    if body_bytes.len() > 100_000 {
        return Err(ProxyError::Validation(
            "Request body exceeds 100KB limit".to_string(),
        ));
    }

    // Parse JSON body into typed request
    let req: ImageGenerationRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Validation(format!("Invalid JSON: {}", e)))?;

    // Re-serialize to Value for proxy forwarding
    let body: serde_json::Value = serde_json::to_value(&req)
        .map_err(|e| ProxyError::Validation(format!("Failed to serialize request: {}", e)))?;

    let model = req.model.clone();
    let client_ip = extract_client_ip(&headers);

    info!(
        request_id = %request_id,
        model = %model,
        ip = %client_ip,
        n = req.n.unwrap_or(1),
        size = req.size.as_deref().unwrap_or("1024x1024"),
        "Processing image generation request"
    );

    // Proxy to provider with fallback chain
    let proxy_result = proxy_images(&state, body, &headers, &request_id, &client_ip).await?;

    // Record successful request latency
    state.relay_metrics.observe_request_latency_ms(proxy_result.latency_ms);

    info!(
        request_id = %request_id,
        status = "success",
        provider = %proxy_result.provider,
        latency_ms = proxy_result.latency_ms,
        "Image generation request done"
    );

    // Record demand for this model
    state.demand.record(&model, 1);

    Ok(proxy_result.response)
}

/// Proxy an image generation request to the best available provider.
///
/// Tries providers in ranked order. On failure, falls back to the next.
pub async fn proxy_images(
    state: &AppState,
    request_body: serde_json::Value,
    headers: &HeaderMap,
    request_id: &str,
    client_ip: &str,
) -> Result<ProxyResult, ProxyError> {
    let model = request_body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown")
        .to_string();

    let proxy_span = info_span!(
        "proxy.images",
        xergon.model = %model,
        xergon.request_id = request_id,
        xergon.provider_pk = tracing::field::Empty,
        xergon.latency_ms = tracing::field::Empty,
    );
    let _proxy_guard = proxy_span.enter();

    let max_attempts = state.config.relay.max_fallback_attempts;
    let mut tried: Vec<String> = Vec::new();

    // Derive sticky session key and check for existing session
    let session_key = crate::provider::ProviderRegistry::derive_session_key(headers, client_ip);
    let sticky_provider = state.provider_registry.get_sticky_provider(&session_key);

    // If we have a sticky provider, try it first
    if let Some(ref sticky) = sticky_provider {
        let endpoint = sticky.endpoint.clone();
        tried.push(endpoint.clone());

        match try_proxy_images_to_provider(
            state,
            &sticky.endpoint,
            &request_body,
            headers,
            request_id,
            &model,
        )
        .await
        {
            Ok(result) => {
                state.provider_registry.record_success(&sticky.endpoint);
                state.provider_registry.set_sticky_session(&session_key, &sticky.endpoint);
                state.adaptive_router.record_outcome(&sticky.endpoint, result.latency_ms, true);
                return Ok(result);
            }
            Err(ProxyError::NoProviders | ProxyError::AllProvidersFailed { .. }) => {
                state.provider_registry.record_failure(&sticky.endpoint);
                state.adaptive_router.record_outcome(&sticky.endpoint, 0, false);
                warn!(
                    request_id,
                    sticky_endpoint = %sticky.endpoint,
                    "Sticky provider failed for images, falling back to normal selection"
                );
            }
            Err(e) => {
                state.provider_registry.record_failure(&sticky.endpoint);
                return Err(e);
            }
        }
    }

    for _attempt in 0..max_attempts {
        let selected_endpoint = if state.config.adaptive_routing.enabled {
            let eligible_providers = state
                .provider_registry
                .ranked_providers_for_model(Some(&model));

            let routing_info: Vec<crate::adaptive_router::ProviderRoutingInfo> = eligible_providers
                .iter()
                .filter(|p| !tried.contains(&p.endpoint))
                .map(|p| crate::proxy::provider_to_routing_info(p))
                .collect();

            let routing_request = crate::adaptive_router::RoutingRequest::new(&model);

            match state
                .adaptive_router
                .select_provider(&routing_request, &routing_info)
            {
                Ok(decision) => {
                    info!(
                        request_id,
                        strategy = %decision.strategy_used,
                        provider = %decision.provider_endpoint,
                        "AdaptiveRouter selected provider for images"
                    );
                    Some(decision.provider_endpoint)
                }
                Err(e) => {
                    warn!(
                        request_id,
                        error = %e,
                        "AdaptiveRouter failed for images, falling back to legacy"
                    );
                    state
                        .provider_registry
                        .select_provider_for_model(&model, &tried, &state.demand)
                        .map(|p| p.endpoint)
                }
            }
        } else {
            state
                .provider_registry
                .select_provider_for_model(&model, &tried, &state.demand)
                .map(|p| p.endpoint)
        };

        let endpoint = selected_endpoint.ok_or_else(|| {
            if tried.is_empty() {
                ProxyError::NoProviders
            } else {
                ProxyError::AllProvidersFailed {
                    attempts: tried.len(),
                }
            }
        })?;

        tried.push(endpoint.clone());

        match try_proxy_images_to_provider(
            state,
            &endpoint,
            &request_body,
            headers,
            request_id,
            &model,
        )
        .await
        {
            Ok(result) => {
                state.provider_registry.record_success(&endpoint);
                state.provider_registry.set_sticky_session(&session_key, &endpoint);
                state.adaptive_router.record_outcome(&endpoint, result.latency_ms, true);
                return Ok(result);
            }
            Err(ProxyError::NoProviders | ProxyError::AllProvidersFailed { .. }) => {
                state.provider_registry.record_failure(&endpoint);
                state.adaptive_router.record_outcome(&endpoint, 0, false);
                continue;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    Err(ProxyError::AllProvidersFailed {
        attempts: tried.len(),
    })
}

/// Attempt to proxy an image generation request to a specific provider endpoint.
async fn try_proxy_images_to_provider(
    state: &AppState,
    endpoint: &str,
    request_body: &serde_json::Value,
    headers: &HeaderMap,
    request_id: &str,
    model: &str,
) -> Result<ProxyResult, ProxyError> {
    // Look up the provider
    let provider = state
        .provider_registry
        .providers
        .get(endpoint)
        .map(|p| p.value().clone())
        .ok_or(ProxyError::NoProviders)?;

    // Acquire a slot on this provider
    let _guard = match state.provider_registry.acquire_provider(endpoint) {
        Some(guard) => guard,
        None => {
            warn!(
                provider = %endpoint,
                "Provider disappeared from registry before acquire (images), skipping"
            );
            return Err(ProxyError::NoProviders);
        }
    };

    let provider_url = format!(
        "{}/v1/images/generations",
        provider.endpoint.trim_end_matches('/')
    );

    info!(
        request_id = request_id,
        provider = %endpoint,
        model = %model,
        "Proxying image generation request to provider"
    );

    let start = std::time::Instant::now();

    // Build forwarded request
    let mut req_builder = state
        .http_client
        .post(&provider_url)
        .timeout(std::time::Duration::from_secs(
            // Image generation can take longer -- use 2x the normal timeout
            state.config.relay.provider_timeout_secs * 2,
        ));

    // Forward relevant headers
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
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

    req_builder = req_builder.json(request_body);

    // Forward the relay request ID
    if let Ok(val) = request_id.parse::<HeaderValue>() {
        req_builder = req_builder.header("X-Request-Id", val);
    }

    match req_builder.send().await {
        Ok(resp) if resp.status().is_success() => {
            let latency_ms = start.elapsed().as_millis() as u64;

            info!(
                request_id = request_id,
                provider = %endpoint,
                status = %resp.status(),
                latency_ms,
                "Provider responded successfully (images)"
            );

            // Image generation always returns a full JSON response (no streaming)
            match resp.bytes().await {
                Ok(body) => {
                    let response = Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/json")
                        .header("X-Provider", endpoint)
                        .header("X-Latency-Ms", latency_ms.to_string())
                        .body(Body::from(body))
                        .unwrap();
                    Ok(ProxyResult {
                        response,
                        provider: endpoint.to_string(),
                        latency_ms,
                        usage: None, // Image generation doesn't have standard token usage
                    })
                }
                Err(e) => {
                    warn!(provider = %endpoint, error = %e, "Failed to read images response body");
                    Err(ProxyError::AllProvidersFailed { attempts: 1 })
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(
                request_id = request_id,
                provider = %endpoint,
                status = %status,
                "Provider returned error status (images)"
            );
            if status.is_client_error() && status.as_u16() != 429 {
                match resp.bytes().await {
                    Ok(body) => {
                        let response = Response::builder()
                            .status(status)
                            .header("Content-Type", "application/json")
                            .body(Body::from(body))
                            .unwrap();
                        Ok(ProxyResult {
                            response,
                            provider: endpoint.to_string(),
                            latency_ms: 0,
                            usage: None,
                        })
                    }
                    Err(_) => Err(ProxyError::AllProvidersFailed { attempts: 1 }),
                }
            } else {
                Err(ProxyError::AllProvidersFailed { attempts: 1 })
            }
        }
        Err(e) => {
            warn!(
                request_id = request_id,
                provider = %endpoint,
                error = %e,
                "Image generation provider request failed"
            );
            Err(ProxyError::AllProvidersFailed { attempts: 1 })
        }
    }
}
