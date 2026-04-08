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
use crate::dynamic_pricing;
use dashmap::DashMap;
use futures_core::Stream;
use pin_project_lite::pin_project;
use reqwest::Client;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn, info_span};

use crate::auth::{AuthError, AuthVerifier};
use crate::chain::ChainScanner;
use crate::chain_cache::ChainCache;
use crate::coalesce_buffer::RequestMultiplexer;
use crate::demand::DemandTracker;
use crate::dedup::RequestDedup;
use crate::free_tier::FreeTierTracker;
use crate::model_registry::ModelRegistry;
use crate::provider::ProviderRegistry;
use crate::rate_limit::RateLimitState;
use crate::events::EventBroadcaster;
use crate::ws::{WsBroadcaster, WsChatConnectionCounter};
use crate::ws_pool::WsConnectionPool;
use crate::gossip::GossipService;
use crate::adaptive_retry::AdaptiveRetry;
use crate::adaptive_router;
use crate::health_score;
use crate::geo_router;
use crate::provider::Provider;
use crate::priority_queue::{EnqueueResult, RequestPriority};
use crate::reputation_bonding;
use crate::staking_rewards;
use crate::oracle_aggregator;
use crate::chain_adapters;
use crate::encrypted_inference;
use crate::quantum_crypto;
use crate::zkp_verification;
use crate::cost_estimator;
use crate::scheduling_optimizer;
use crate::homomorphic_compute;
use crate::cross_provider_orchestration;
use crate::ensemble_router;
use crate::speculative_decoding;
use crate::request_fusion;
use crate::continuous_batching;
use crate::token_streaming;
use crate::capability_negotiation;
use crate::protocol_versioning;
use crate::connection_pool_v2;
use crate::request_dedup_v2;
use crate::response_cache_headers;
use crate::content_negotiation;
use crate::rate_limiter_v2::RateLimiterV2;
use crate::middleware_chain::MiddlewareChain;
use crate::cors_v2::CorsManagerV2;
use crate::websocket_v2::WebSocketV2;
use crate::health_monitor_v2::HealthMonitorV2;
use crate::api_gateway::ApiGateway;
use crate::babel_fee_integration::BabelFeeManager;
use crate::request_coalescing::RequestCoalescer;
use crate::protocol_adapter::ProtocolAdapter;

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
    /// Request deduplication tracker
    pub request_dedup: RequestDedup,
    /// WebSocket chat connection counter
    pub ws_chat_connections: WsChatConnectionCounter,
    /// WebSocket connection pool for backend providers
    pub ws_pool: Arc<WsConnectionPool>,
    /// Model registry (aggregated view of all models across providers)
    pub model_registry: Arc<ModelRegistry>,
    /// Response cache with ETag support
    pub response_cache: Arc<crate::cache::ResponseCache>,
    /// Standalone circuit breaker with metrics (supplements provider-embedded CB)
    pub circuit_breaker: Arc<crate::circuit_breaker::CircuitBreaker>,
    /// Load shedder for request prioritization under heavy load
    pub load_shedder: Arc<crate::load_shed::LoadShedder>,
    /// Graceful degradation manager
    pub degradation_manager: Arc<crate::degradation::DegradationManager>,
    /// Request multiplexer (coalescing + stream buffering)
    pub request_multiplexer: Arc<RequestMultiplexer>,
    /// Adaptive router for intelligent provider selection
    pub adaptive_router: Arc<adaptive_router::AdaptiveRouter>,
    /// Health scorer for provider quality metrics
    pub health_scorer: Arc<health_score::HealthScorer>,
    /// Geo router for proximity-based provider selection
    pub geo_router: Arc<geo_router::GeoRouter>,
    /// Priority queue for request scheduling when providers are at capacity
    pub priority_queue: Arc<crate::priority_queue::PriorityQueue>,
    /// File upload store (in-memory metadata + local disk storage)
    pub file_store: Arc<crate::handlers::upload::FileStore>,
    /// Multi-tier rate limit manager
    pub tier_manager: Arc<crate::rate_limit_tiers::TierManager>,
    /// Webhook event delivery manager
    pub webhook_manager: crate::webhook::WebhookManager,
    /// Audit event logger
    pub audit_logger: crate::audit::AuditLogger,
    /// API key management
    pub api_key_manager: crate::api_key_manager::ApiKeyManager,
    /// Usage analytics aggregation
    pub usage_analytics: crate::usage_analytics::UsageAnalytics,
    /// Provider auto-registration service (None if disabled)
    pub auto_register: Option<Arc<crate::auto_register::AutoRegistrationService>>,
    /// SLA tracker for per-provider service level monitoring
    pub sla_tracker: crate::sla::SlaTracker,
    /// Adaptive retry engine with token-bucket budgeting
    pub adaptive_retry: Arc<AdaptiveRetry>,
    /// Distributed cache synchronizer (None if disabled)
    pub cache_synchronizer: Option<Arc<crate::cache_sync::CacheSynchronizer>>,
    /// Multi-region router (None if disabled)
    pub multi_region_router: Option<Arc<crate::multi_region::MultiRegionRouter>>,
    /// Semantic similarity cache for deduplicating similar queries
    pub semantic_cache: Arc<crate::semantic_cache::SemanticCache>,
    /// Typed audit buffer for request-level events
    pub request_audit_buffer: crate::audit::RequestAuditBuffer,
    /// Typed audit buffer for authentication events
    pub auth_audit_buffer: crate::audit::AuthAuditBuffer,
    /// Typed audit buffer for compliance events
    pub compliance_audit_buffer: crate::audit::ComplianceAuditBuffer,
    /// Reputation bonding manager for provider stake bonding and slashing
    pub bonding_manager: Arc<reputation_bonding::BondingManager>,
    /// Staking reward pool for yield distribution
    pub staking_pool: Arc<staking_rewards::StakingRewardPool>,
    /// Multi-oracle price aggregator with failover
    pub oracle_aggregator: Arc<oracle_aggregator::OracleAggregator>,
    /// Multi-chain adapter manager for cross-chain settlement
    #[allow(non_snake_case)]
    pub chainAdapterManager: Arc<chain_adapters::ChainAdapterManager>,
    /// Encrypted inference engine for E2E encrypted routing
    pub encrypted_inference: Arc<encrypted_inference::EncryptedInferenceState>,
    /// Quantum-resistant cryptographic primitives engine
    pub quantum_crypto: Arc<quantum_crypto::QuantumCryptoState>,
    /// ZK proof verification, TEE attestation, and trust scoring
    pub zkp_verification: Arc<zkp_verification::ZKPVerificationState>,
    pub homomorphic_compute: Arc<homomorphic_compute::HomomorphicComputeState>,
    pub cross_provider_orchestrator: Arc<cross_provider_orchestration::CrossProviderOrchestrator>,
    /// Speculative decoding coordinator for draft-model acceleration
    pub speculative_coordinator: Arc<speculative_decoding::SpeculativeDecodingCoordinator>,
    /// Request fusion engine for multi-query batching
    pub request_fusion: Arc<request_fusion::RequestFusionEngine>,
    pub continuous_batching: Arc<continuous_batching::ContinuousBatchingEngine>,
    pub token_streaming: Arc<token_streaming::TokenStreamingMultiplexer>,
    /// Inference cost estimator for per-request cost tracking
    pub cost_estimator: Arc<cost_estimator::InferenceCostEstimator>,
    /// Request scheduling optimizer for intelligent provider selection
    pub scheduling_optimizer: Arc<scheduling_optimizer::SchedulingOptimizer>,
    /// Dynamic pricing engine for demand/supply-based price adjustments
    pub dynamic_pricing_engine: Arc<dynamic_pricing::DynamicPricingEngine>,
    /// Provider capability negotiation engine
    pub capability_negotiation: Arc<capability_negotiation::CapabilityNegotiator>,
    /// Protocol version registry and negotiation
    pub protocol_registry: Arc<protocol_versioning::ProtocolRegistry>,
    /// Enhanced connection pool v2
    pub connection_pool_v2: Arc<connection_pool_v2::ConnectionPoolV2>,
    /// Enhanced request deduplication v2 (BLAKE3 + fuzzy + response caching)
    pub request_dedup_v2: Arc<request_dedup_v2::RequestDedupV2>,
    /// HTTP response cache with header management and conditional requests
    pub response_cache_headers: Arc<response_cache_headers::ResponseCache>,
    /// Content negotiation engine for inference responses
    pub content_negotiator: Arc<content_negotiation::ContentNegotiator>,
    /// Advanced rate limiter v2 with multi-algorithm support
    pub rate_limiter_v2: Arc<RateLimiterV2>,
    /// Middleware chain for request processing pipeline
    pub middleware_chain: Arc<MiddlewareChain>,
    /// Enhanced CORS manager with per-path rules
    pub cors_manager_v2: Arc<CorsManagerV2>,
    /// Enhanced WebSocket V2 with channels, presence, and message history
    pub websocket_v2: Arc<WebSocketV2>,
    /// Deep health monitor V2 with dependency tracking
    pub health_monitor_v2: Arc<HealthMonitorV2>,
    /// API gateway with routing, auth gating, and stats
    pub api_gateway: Arc<ApiGateway>,
    /// Babel fee manager for Ergo transaction fee calculation
    pub babel_fee_manager: Arc<BabelFeeManager>,
    /// Request coalescer for batching similar requests
    pub request_coalescer: Arc<RequestCoalescer>,
    /// Protocol adapter for normalizing requests across providers
    pub protocol_adapter: Arc<ProtocolAdapter>,
    /// Multi-model ensemble router for fan-out and response aggregation
    pub ensemble_router: Arc<ensemble_router::EnsembleRouter>,
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

/// Convert a [`Provider`] from the registry into the lightweight
/// [`ProviderRoutingInfo`] needed by the [`AdaptiveRouter`].
///
/// Maps provider fields as follows:
/// - `provider_pk`: extracted from `/xergon/status` provider.id, or falls back to endpoint
/// - `endpoint`: direct copy
/// - `active_requests`: live atomic count
/// - `model_pricing`: direct copy of per-model pricing map
/// - `region`: from on-chain R9 register or `/xergon/status`
pub fn provider_to_routing_info(provider: &Provider) -> adaptive_router::ProviderRoutingInfo {
    let provider_pk = provider
        .status
        .as_ref()
        .and_then(|s| s.provider.as_ref())
        .map(|p| p.id.clone())
        .unwrap_or_else(|| provider.endpoint.clone());

    adaptive_router::ProviderRoutingInfo {
        provider_pk,
        endpoint: provider.endpoint.clone(),
        active_requests: provider.active_requests.load(std::sync::atomic::Ordering::Relaxed),
        model_pricing: provider.model_pricing.clone(),
        region: provider.region.clone(),
    }
}

/// Extract request priority from headers.
///
/// Uses the `X-Priority` header if present (values: 0=critical, 1=high, 2=normal, 3=low).
/// Falls back to `Normal` for authenticated users, `Low` for anonymous.
fn extract_priority(headers: &HeaderMap, authenticated: bool) -> RequestPriority {
    // Check for explicit priority header
    if let Some(priority_val) = headers.get("x-priority").and_then(|v| v.to_str().ok()) {
        match priority_val.trim() {
            "0" | "critical" => return RequestPriority::Critical,
            "1" | "high" => return RequestPriority::High,
            "2" | "normal" => return RequestPriority::Normal,
            "3" | "low" => return RequestPriority::Low,
            _ => {}
        }
    }

    // Infer from authentication status
    if authenticated {
        // Authenticated users with staking boxes get High priority
        RequestPriority::High
    } else {
        RequestPriority::Low
    }
}

/// Proxy a chat completions request to the best available provider.
///
/// Tries providers in ranked order. On failure, falls back to the next.
/// If stream=true, wraps the SSE stream with a token counter.
///
/// Features:
/// - Sticky sessions: reuses the same provider for a session if possible
/// - Circuit breaker: skips providers with open circuits, records success/failure
pub async fn proxy_chat_completion(
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

    let is_stream = request_body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    // Create the root proxy span for distributed tracing
    let proxy_span = info_span!(
        "proxy.chat_completion",
        xergon.model = %model,
        xergon.stream = is_stream,
        xergon.request_id = request_id,
        xergon.provider_pk = tracing::field::Empty,
        xergon.latency_ms = tracing::field::Empty,
        xergon.tokens_prompt = tracing::field::Empty,
        xergon.tokens_completion = tracing::field::Empty,
    );
    let _proxy_guard = proxy_span.enter();

    let max_attempts = state.config.relay.max_fallback_attempts;
    let mut tried: Vec<String> = Vec::new();

    // Derive sticky session key and check for existing session
    let session_key = ProviderRegistry::derive_session_key(headers, client_ip);
    let sticky_provider = state.provider_registry.get_sticky_provider(&session_key);

    // If we have a sticky provider, try it first
    if let Some(ref sticky) = sticky_provider {
        let endpoint = sticky.endpoint.clone();
        tried.push(endpoint.clone());

        match try_proxy_to_provider(
            state,
            &sticky.endpoint,
            &request_body,
            headers,
            request_id,
            &model,
            is_stream,
            0,
        )
        .await
        {
            Ok(result) => {
                // Successful sticky routing — record success and reinforce sticky session
                state.provider_registry.record_success(&sticky.endpoint);
                state.provider_registry.set_sticky_session(&session_key, &sticky.endpoint);
                state.adaptive_router.record_outcome(&sticky.endpoint, result.latency_ms, true);
                return Ok(result);
            }
            Err(ProxyError::NoProviders | ProxyError::AllProvidersFailed { .. }) => {
                // Sticky provider failed, fall through to normal selection
                state.provider_registry.record_failure(&sticky.endpoint);
                state.adaptive_router.record_outcome(&sticky.endpoint, 0, false);
                warn!(
                    request_id,
                    sticky_endpoint = %sticky.endpoint,
                    "Sticky provider failed, falling back to normal selection"
                );
            }
            Err(e) => {
                // Sticky provider failed with a client error or other non-retryable error
                state.provider_registry.record_failure(&sticky.endpoint);
                return Err(e);
            }
        }
    }

    for attempt in 0..max_attempts {
        // Select next best provider (excluding already-tried), model-aware.
        // Uses AdaptiveRouter when enabled (default), falls back to legacy scoring.
        let selection_span = info_span!(
            "proxy.selection",
            attempt = attempt,
            xergon.model = %model,
            tried_count = tried.len(),
        );
        let _selection_guard = selection_span.enter();

        let selected_endpoint = if state.config.adaptive_routing.enabled {
            // --- Adaptive routing path ---
            let eligible_providers = state
                .provider_registry
                .ranked_providers_for_model(Some(&model));

            let routing_info: Vec<adaptive_router::ProviderRoutingInfo> = eligible_providers
                .iter()
                .filter(|p| !tried.contains(&p.endpoint))
                .map(provider_to_routing_info)
                .collect();

            let routing_request = adaptive_router::RoutingRequest::new(&model);

            match state
                .adaptive_router
                .select_provider(&routing_request, &routing_info)
            {
                Ok(decision) => {
                    info!(
                        request_id,
                        strategy = %decision.strategy_used,
                        health_score = decision.health_score,
                        provider = %decision.provider_endpoint,
                        "AdaptiveRouter selected provider"
                    );
                    Some(decision.provider_endpoint)
                }
                Err(e) => {
                    warn!(
                        request_id,
                        error = %e,
                        "AdaptiveRouter failed to select provider, falling back to legacy"
                    );
                    // Fallback to legacy selection
                    state
                        .provider_registry
                        .select_provider_for_model(&model, &tried, &state.demand)
                        .map(|p| p.endpoint)
                }
            }
        } else {
            // --- Legacy routing path ---
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

        match try_proxy_to_provider(
            state,
            &endpoint,
            &request_body,
            headers,
            request_id,
            &model,
            is_stream,
            attempt,
        )
        .await
        {
            Ok(result) => {
                // Successful request — reset circuit breaker and set sticky session
                state.provider_registry.record_success(&endpoint);
                state.provider_registry.set_sticky_session(&session_key, &endpoint);
                // Feed outcome back to the AdaptiveRouter for learning
                state.adaptive_router.record_outcome(&endpoint, result.latency_ms, true);
                return Ok(result);
            }
            Err(ProxyError::NoProviders | ProxyError::AllProvidersFailed { .. }) => {
                // Record failure and try next provider
                state.provider_registry.record_failure(&endpoint);
                state.adaptive_router.record_outcome(&endpoint, 0, false);
                continue;
            }
            Err(e) => {
                // Non-retryable error (client error, auth, etc.)
                // Don't record as provider failure for circuit breaker since it's a client issue
                return Err(e);
            }
        }
    }

    // All providers exhausted or none available.
    // Try priority queue: enqueue the request for later processing.
    if state.priority_queue.is_enabled() {
        let priority = extract_priority(headers, false);
        let model_for_queue = Some(model.as_str());

        match state.priority_queue.enqueue(request_id, priority, model_for_queue) {
            EnqueueResult::Queued => {
                info!(
                    request_id,
                    priority = %priority,
                    model = %model,
                    tried = tried.len(),
                    "Request enqueued in priority queue (all providers busy)"
                );
                return Err(ProxyError::CapacityFull(format!(
                    "All providers are busy. Your request (priority: {}) has been queued. Position depends on priority level.",
                    priority
                )));
            }
            EnqueueResult::LevelFull | EnqueueResult::TotalFull => {
                warn!(
                    request_id,
                    priority = %priority,
                    model = %model,
                    "Priority queue full, rejecting request"
                );
                return Err(ProxyError::CapacityFull(format!(
                    "All providers are busy and the priority queue is full. Try again later or increase your request priority.",
                )));
            }
            EnqueueResult::Disabled => {
                // Priority queue disabled — fall through to normal error
            }
        }
    }

    Err(ProxyError::AllProvidersFailed {
        attempts: tried.len(),
    })
}

/// Attempt to proxy a request to a specific provider endpoint.
/// Returns Ok(ProxyResult) on success, Err(ProxyError) on failure.
/// Distinguishes retryable errors (5xx, timeout) from non-retryable (4xx).
async fn try_proxy_to_provider(
    state: &AppState,
    endpoint: &str,
    request_body: &serde_json::Value,
    headers: &HeaderMap,
    request_id: &str,
    model: &str,
    is_stream: bool,
    attempt: usize,
) -> Result<ProxyResult, ProxyError> {
    // Look up the provider to get its details
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
                "Provider disappeared from registry before acquire, skipping"
            );
            return Err(ProxyError::NoProviders);
        }
    };

    let provider_url = format!(
        "{}/v1/chat/completions",
        provider.endpoint.trim_end_matches('/')
    );

    info!(
        request_id = request_id,
        attempt = attempt + 1,
        provider = %endpoint,
        model = %model,
        stream = is_stream,
        "Proxying request to provider"
    );

    // Create a span for the actual HTTP forward to the provider
    let forward_span = info_span!(
        "proxy.forward",
        provider_endpoint = %endpoint,
        xergon.model = %model,
        xergon.stream = is_stream,
        attempt = attempt + 1,
    );
    let _forward_guard = forward_span.enter();

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

    req_builder = req_builder.json(request_body);

    // Forward the relay request ID to the provider for end-to-end correlation
    if let Ok(val) = request_id.parse::<axum::http::HeaderValue>() {
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
                "Provider responded successfully"
            );

            if is_stream {
                // Stream SSE response with token counting
                let stream_span = info_span!(
                    "proxy.stream",
                    provider_endpoint = %endpoint,
                    xergon.model = %model,
                );
                let _stream_guard = stream_span.enter();

                let stream = resp.bytes_stream();
                let (tracking_body, usage) = counting_stream_body(stream);
                let response = build_streaming_response(tracking_body);

                Ok(ProxyResult {
                    response,
                    provider: endpoint.to_string(),
                    latency_ms,
                    usage: Some(usage),
                })
            } else {
                // Collect the full response and extract usage
                match resp.bytes().await {
                    Ok(body) => {
                        let usage = extract_usage_from_json(&body).map(Arc::new);
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
                            usage,
                        })
                    }
                    Err(e) => {
                        warn!(provider = %endpoint, error = %e, "Failed to read provider response body");
                        Err(ProxyError::AllProvidersFailed { attempts: 1 })
                    }
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(
                request_id = request_id,
                provider = %endpoint,
                status = %status,
                "Provider returned error status"
            );
            // If it's a 4xx client error (not 429), don't retry — it's our fault
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
                // 429 or 5xx — retryable
                Err(ProxyError::AllProvidersFailed { attempts: 1 })
            }
        }
        Err(e) => {
            warn!(
                request_id = request_id,
                provider = %endpoint,
                error = %e,
                "Provider request failed"
            );
            Err(ProxyError::AllProvidersFailed { attempts: 1 })
        }
    }
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

    #[error("All providers at capacity, request queued: {0}")]
    CapacityFull(String),
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
            ProxyError::CapacityFull(msg) => (
                StatusCode::TOO_MANY_REQUESTS,
                msg.clone(),
            ),
        };

        let (error_type, error_code) = match &self {
            ProxyError::InsufficientBalance(_) => ("insufficient_balance", "payment_required"),
            ProxyError::Unauthorized(_) => ("auth_error", "unauthorized"),
            ProxyError::CapacityFull(_) => ("capacity_full", "too_many_requests"),
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
