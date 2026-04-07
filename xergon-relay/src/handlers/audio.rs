//! OpenAI-compatible audio endpoints for the Xergon relay.
//!
//! - POST /v1/audio/speech        -- Text-to-Speech (streams binary audio)
//! - POST /v1/audio/transcriptions -- Speech-to-Text (multipart upload)
//! - POST /v1/audio/translations  -- Translate audio to English (multipart upload)

use axum::{
    body::Body,
    extract::{Multipart, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, info_span, Instrument};

use crate::proxy::{ProxyError, AppState, ProxyResult};
use crate::util::extract_client_ip;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// OpenAI-compatible TTS request
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SpeechRequest {
    pub model: String,
    /// The text to generate audio for. Max 4096 characters.
    pub input: String,
    /// The voice to use (e.g. "alloy", "echo", "fable", "onyx", "nova", "shimmer").
    pub voice: String,
    /// Output format: "mp3", "opus", "aac", "flac", "wav", "pcm".
    #[serde(default = "default_format")]
    pub response_format: String,
    /// Speed of the generated audio (0.25 to 4.0).
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_format() -> String {
    "mp3".to_string()
}

fn default_speed() -> f32 {
    1.0
}

/// OpenAI-compatible transcription response
#[derive(Debug, Serialize)]
pub struct TranscriptionResponse {
    pub text: String,
}

/// OpenAI-compatible file object (for transcription responses)
#[derive(Debug, Serialize)]
pub struct TranscriptionFileResponse {
    pub text: String,
}

// ---------------------------------------------------------------------------
// Content-type mapping
// ---------------------------------------------------------------------------

fn content_type_for_format(fmt: &str) -> &'static str {
    match fmt {
        "opus" => "audio/opus",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "pcm" => "audio/pcm",
        _ => "audio/mpeg", // mp3 default
    }
}

// ---------------------------------------------------------------------------
// POST /v1/audio/speech
// ---------------------------------------------------------------------------

/// Text-to-Speech handler.  Validates the request, proxies to the best
/// available provider, and streams the binary audio response back.
pub async fn speech_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response, ProxyError> {
    let request_id = request
        .extensions()
        .get::<crate::middleware::RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Read body
    let body_bytes = axum::body::to_bytes(request.into_body(), 50_001)
        .await
        .map_err(|e| ProxyError::Validation(format!("Failed to read request body: {}", e)))?;
    if body_bytes.len() > 50_000 {
        return Err(ProxyError::Validation(
            "Request body exceeds 50KB limit for TTS".to_string(),
        ));
    }

    let req: SpeechRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| ProxyError::Validation(format!("Invalid JSON: {}", e)))?;

    // Validate speed range
    if req.speed < 0.25 || req.speed > 4.0 {
        return Err(ProxyError::Validation(
            "speed must be between 0.25 and 4.0".to_string(),
        ));
    }

    // Validate input length
    if req.input.len() > 4096 {
        return Err(ProxyError::Validation(
            "input exceeds 4096 character limit".to_string(),
        ));
    }

    let model = req.model.clone();
    let client_ip = extract_client_ip(&headers);
    let content_type = content_type_for_format(&req.response_format);

    info!(
        request_id = %request_id,
        model = %model,
        voice = %req.voice,
        format = %req.response_format,
        ip = %client_ip,
        "Processing TTS request"
    );

    let proxy_result = proxy_audio(
        &state,
        serde_json::to_value(&req).unwrap(),
        "/v1/audio/speech",
        &headers,
        &request_id,
        &client_ip,
        &model,
    )
    .await?;

    state.relay_metrics.observe_request_latency_ms(proxy_result.latency_ms);

    info!(
        request_id = %request_id,
        status = "success",
        provider = %proxy_result.provider,
        latency_ms = proxy_result.latency_ms,
        "TTS request done"
    );

    state.demand.record(&model, 1);

    // Return the binary audio response with the correct content-type
    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("X-Provider", &proxy_result.provider)
        .header("X-Latency-Ms", proxy_result.latency_ms.to_string())
        .body(proxy_result.response.into_body())
        .unwrap();

    Ok(response)
}

// ---------------------------------------------------------------------------
// POST /v1/audio/transcriptions
// ---------------------------------------------------------------------------

/// Speech-to-Text handler.  Accepts multipart/form-data with a file field,
/// forwards to the provider, returns the transcription.
pub async fn transcriptions_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, ProxyError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let client_ip = extract_client_ip(&headers);

    let mut model = "whisper-1".to_string();
    let mut language: Option<String> = None;
    let mut response_format = "json".to_string();
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename = "audio.mp3".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ProxyError::Validation(format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                filename = field.file_name().unwrap_or("audio.mp3").to_string();
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ProxyError::Validation(format!("Failed to read file: {}", e)))?
                        .to_vec(),
                );
            }
            "model" => {
                model = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "whisper-1".to_string());
            }
            "language" => {
                language = Some(
                    field
                        .text()
                        .await
                        .unwrap_or_default(),
                );
            }
            "response_format" => {
                response_format = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "json".to_string());
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    let file_data = file_bytes.ok_or_else(|| {
        ProxyError::Validation("Missing required field: file".to_string())
    })?;

    info!(
        request_id = %request_id,
        model = %model,
        language = ?language,
        format = %response_format,
        filename = %filename,
        file_size = file_data.len(),
        ip = %client_ip,
        "Processing transcription request"
    );

    // Build the multipart form to forward to the provider
    let proxy_result = proxy_audio_multipart(
        &state,
        "file",
        &filename,
        &file_data,
        &model,
        language.as_deref(),
        &response_format,
        "/v1/audio/transcriptions",
        &headers,
        &request_id,
        &client_ip,
    )
    .await?;

    state.relay_metrics.observe_request_latency_ms(proxy_result.latency_ms);

    info!(
        request_id = %request_id,
        status = "success",
        provider = %proxy_result.provider,
        latency_ms = proxy_result.latency_ms,
        "Transcription request done"
    );

    Ok(proxy_result.response)
}

// ---------------------------------------------------------------------------
// POST /v1/audio/translations
// ---------------------------------------------------------------------------

/// Audio translation handler.  Accepts multipart/form-data with a file field,
/// forwards to the provider, returns English translation.
pub async fn translations_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Response, ProxyError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let client_ip = extract_client_ip(&headers);

    let mut model = "whisper-1".to_string();
    let mut response_format = "json".to_string();
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename = "audio.mp3".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ProxyError::Validation(format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                filename = field.file_name().unwrap_or("audio.mp3").to_string();
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ProxyError::Validation(format!("Failed to read file: {}", e)))?
                        .to_vec(),
                );
            }
            "model" => {
                model = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "whisper-1".to_string());
            }
            "response_format" => {
                response_format = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "json".to_string());
            }
            _ => {}
        }
    }

    let file_data = file_bytes.ok_or_else(|| {
        ProxyError::Validation("Missing required field: file".to_string())
    })?;

    info!(
        request_id = %request_id,
        model = %model,
        format = %response_format,
        filename = %filename,
        file_size = file_data.len(),
        ip = %client_ip,
        "Processing translation request"
    );

    let proxy_result = proxy_audio_multipart(
        &state,
        "file",
        &filename,
        &file_data,
        &model,
        None, // no language for translation (always to English)
        &response_format,
        "/v1/audio/translations",
        &headers,
        &request_id,
        &client_ip,
    )
    .await?;

    state.relay_metrics.observe_request_latency_ms(proxy_result.latency_ms);

    info!(
        request_id = %request_id,
        status = "success",
        provider = %proxy_result.provider,
        latency_ms = proxy_result.latency_ms,
        "Translation request done"
    );

    Ok(proxy_result.response)
}

// ---------------------------------------------------------------------------
// Proxy helpers
// ---------------------------------------------------------------------------

/// Proxy a JSON audio request (TTS) to the best available provider.
async fn proxy_audio(
    state: &AppState,
    request_body: serde_json::Value,
    provider_path: &str,
    headers: &HeaderMap,
    request_id: &str,
    client_ip: &str,
    model: &str,
) -> Result<ProxyResult, ProxyError> {
    let proxy_span = info_span!(
        "proxy.audio",
        xergon.model = %model,
        xergon.request_id = request_id,
    );
    let _proxy_guard = proxy_span.enter();

    let max_attempts = state.config.relay.max_fallback_attempts;
    let mut tried: Vec<String> = Vec::new();

    for _attempt in 0..max_attempts {
        let selected_endpoint = if state.config.adaptive_routing.enabled {
            let eligible_providers = state
                .provider_registry
                .ranked_providers_for_model(Some(model));

            let routing_info: Vec<crate::adaptive_router::ProviderRoutingInfo> = eligible_providers
                .iter()
                .filter(|p| !tried.contains(&p.endpoint))
                .map(|p| crate::proxy::provider_to_routing_info(p))
                .collect();

            let routing_request = crate::adaptive_router::RoutingRequest::new(model);

            match state
                .adaptive_router
                .select_provider(&routing_request, &routing_info)
            {
                Ok(decision) => Some(decision.provider_endpoint),
                Err(_) => state
                    .provider_registry
                    .select_provider_for_model(model, &tried, &state.demand)
                    .map(|p| p.endpoint),
            }
        } else {
            state
                .provider_registry
                .select_provider_for_model(model, &tried, &state.demand)
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

        match try_proxy_audio_to_provider(
            state,
            &endpoint,
            &request_body,
            headers,
            request_id,
            model,
            provider_path,
        )
        .await
        {
            Ok(result) => {
                state.provider_registry.record_success(&endpoint);
                state.adaptive_router.record_outcome(&endpoint, result.latency_ms, true);
                return Ok(result);
            }
            Err(ProxyError::NoProviders | ProxyError::AllProvidersFailed { .. }) => {
                state.provider_registry.record_failure(&endpoint);
                state.adaptive_router.record_outcome(&endpoint, 0, false);
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(ProxyError::AllProvidersFailed {
        attempts: tried.len(),
    })
}

/// Attempt to proxy a JSON audio request to a specific provider.
async fn try_proxy_audio_to_provider(
    state: &AppState,
    endpoint: &str,
    request_body: &serde_json::Value,
    headers: &HeaderMap,
    request_id: &str,
    model: &str,
    provider_path: &str,
) -> Result<ProxyResult, ProxyError> {
    let provider = state
        .provider_registry
        .providers
        .get(endpoint)
        .map(|p| p.value().clone())
        .ok_or(ProxyError::NoProviders)?;

    let _guard = match state.provider_registry.acquire_provider(endpoint) {
        Some(guard) => guard,
        None => return Err(ProxyError::NoProviders),
    };

    let provider_url = format!(
        "{}{}",
        provider.endpoint.trim_end_matches('/'),
        provider_path,
    );

    info!(
        request_id,
        provider = %endpoint,
        model,
        "Proxying audio request to provider"
    );

    let start = std::time::Instant::now();

    let mut req_builder = state
        .http_client
        .post(&provider_url)
        .timeout(std::time::Duration::from_secs(
            state.config.relay.provider_timeout_secs,
        ));

    // Forward headers (skip hop-by-hop)
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if matches!(
            name_str.as_str(),
            "host" | "connection" | "transfer-encoding"
                | "authorization" | "x-forwarded-for"
                | "x-real-ip" | "content-length"
                | "content-type"
        ) {
            continue;
        }
        if let Ok(val) = HeaderValue::from_bytes(value.as_bytes()) {
            req_builder = req_builder.header(name.as_str(), val);
        }
    }

    req_builder = req_builder.json(request_body);

    if let Ok(val) = request_id.parse::<HeaderValue>() {
        req_builder = req_builder.header("X-Request-Id", val);
    }

    match req_builder.send().await {
        Ok(resp) if resp.status().is_success() => {
            let latency_ms = start.elapsed().as_millis() as u64;

            // For TTS, stream the bytes directly
            match resp.bytes().await {
                Ok(body) => {
                    let response = Response::builder()
                        .status(StatusCode::OK)
                        .header("X-Provider", endpoint)
                        .header("X-Latency-Ms", latency_ms.to_string())
                        .body(Body::from(body))
                        .unwrap();
                    Ok(ProxyResult {
                        response,
                        provider: endpoint.to_string(),
                        latency_ms,
                        usage: None,
                    })
                }
                Err(e) => {
                    warn!(provider = %endpoint, error = %e, "Failed to read audio response body");
                    Err(ProxyError::AllProvidersFailed { attempts: 1 })
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(
                request_id,
                provider = %endpoint,
                status = %status,
                "Provider returned error status (audio)"
            );
            Err(ProxyError::AllProvidersFailed { attempts: 1 })
        }
        Err(e) => {
            warn!(
                request_id,
                provider = %endpoint,
                error = %e,
                "Audio provider request failed"
            );
            Err(ProxyError::AllProvidersFailed { attempts: 1 })
        }
    }
}

/// Proxy a multipart audio request (transcription/translation) to a provider.
async fn proxy_audio_multipart(
    state: &AppState,
    file_field_name: &str,
    filename: &str,
    file_data: &[u8],
    model: &str,
    language: Option<&str>,
    response_format: &str,
    provider_path: &str,
    headers: &HeaderMap,
    request_id: &str,
    client_ip: &str,
) -> Result<ProxyResult, ProxyError> {
    let proxy_span = info_span!(
        "proxy.audio_multipart",
        xergon.model = %model,
        xergon.request_id = request_id,
    );
    let _proxy_guard = proxy_span.enter();

    let max_attempts = state.config.relay.max_fallback_attempts;
    let mut tried: Vec<String> = Vec::new();

    for _attempt in 0..max_attempts {
        let selected_endpoint = state
            .provider_registry
            .select_provider_for_model(model, &tried, &state.demand)
            .map(|p| p.endpoint);

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

        match try_proxy_audio_multipart_to_provider(
            state,
            &endpoint,
            file_field_name,
            filename,
            file_data,
            model,
            language,
            response_format,
            provider_path,
            headers,
            request_id,
        )
        .await
        {
            Ok(result) => {
                state.provider_registry.record_success(&endpoint);
                return Ok(result);
            }
            Err(ProxyError::NoProviders | ProxyError::AllProvidersFailed { .. }) => {
                state.provider_registry.record_failure(&endpoint);
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(ProxyError::AllProvidersFailed {
        attempts: tried.len(),
    })
}

/// Attempt to proxy a multipart audio request to a specific provider.
async fn try_proxy_audio_multipart_to_provider(
    state: &AppState,
    endpoint: &str,
    file_field_name: &str,
    filename: &str,
    file_data: &[u8],
    model: &str,
    language: Option<&str>,
    response_format: &str,
    provider_path: &str,
    headers: &HeaderMap,
    request_id: &str,
) -> Result<ProxyResult, ProxyError> {
    let provider = state
        .provider_registry
        .providers
        .get(endpoint)
        .map(|p| p.value().clone())
        .ok_or(ProxyError::NoProviders)?;

    let _guard = match state.provider_registry.acquire_provider(endpoint) {
        Some(guard) => guard,
        None => return Err(ProxyError::NoProviders),
    };

    let provider_url = format!(
        "{}{}",
        provider.endpoint.trim_end_matches('/'),
        provider_path,
    );

    info!(
        request_id,
        provider = %endpoint,
        model,
        "Proxying multipart audio request to provider"
    );

    let start = std::time::Instant::now();

    // Build multipart form
    let file_field_owned = file_field_name.to_string();
    let file_part = reqwest::multipart::Part::bytes(file_data.to_vec())
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")
        .unwrap();

    let mut form = reqwest::multipart::Form::new()
        .part(file_field_owned, file_part)
        .text("model", model.to_string())
        .text("response_format", response_format.to_string());

    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }

    let mut req_builder = state
        .http_client
        .post(&provider_url)
        .timeout(std::time::Duration::from_secs(
            state.config.relay.provider_timeout_secs,
        ))
        .multipart(form);

    // Forward relevant headers
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if matches!(
            name_str.as_str(),
            "host" | "connection" | "transfer-encoding"
                | "authorization" | "x-forwarded-for"
                | "x-real-ip" | "content-length"
                | "content-type"
        ) {
            continue;
        }
        if let Ok(val) = HeaderValue::from_bytes(value.as_bytes()) {
            req_builder = req_builder.header(name.as_str(), val);
        }
    }

    if let Ok(val) = request_id.parse::<HeaderValue>() {
        req_builder = req_builder.header("X-Request-Id", val);
    }

    match req_builder.send().await {
        Ok(resp) if resp.status().is_success() => {
            let latency_ms = start.elapsed().as_millis() as u64;

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
                        usage: None,
                    })
                }
                Err(e) => {
                    warn!(provider = %endpoint, error = %e, "Failed to read multipart audio response");
                    Err(ProxyError::AllProvidersFailed { attempts: 1 })
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(
                request_id,
                provider = %endpoint,
                status = %status,
                "Provider returned error status (multipart audio)"
            );
            Err(ProxyError::AllProvidersFailed { attempts: 1 })
        }
        Err(e) => {
            warn!(
                request_id,
                provider = %endpoint,
                error = %e,
                "Multipart audio provider request failed"
            );
            Err(ProxyError::AllProvidersFailed { attempts: 1 })
        }
    }
}
