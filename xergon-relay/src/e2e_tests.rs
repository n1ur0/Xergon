//! End-to-end integration tests for the relay chat completion flow.
//!
//! Tests the full flow: request -> relay routing -> provider selection -> response.
//! Uses in-process mock axum servers — no network access required beyond loopback.
//!
//! Run with:
//!   cargo test -p xergon-relay -- e2e

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
    routing::post,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::config::{
    AuthConfig, BalanceConfig, BridgeConfig, ChainConfig, IncentiveConfig, OracleConfig,
    RateLimitConfig, ProviderSettings, RelayConfig, RelaySettings,
};
use crate::demand::DemandTracker;
use crate::handlers::chat::chat_completions_handler;
use crate::metrics::RelayMetrics;
use crate::proxy::AppState;
use crate::provider::ProviderRegistry;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal [`RelayConfig`] for testing.
///
/// Disables auth, balance checks, chain scanning, rate limiting, and incentives
/// so the test only exercises the request -> proxy -> provider -> response path.
fn test_config(provider_endpoint: &str) -> Arc<RelayConfig> {
    Arc::new(RelayConfig {
        relay: RelaySettings {
            listen_addr: "0.0.0.0:0".into(),
            cors_origins: "*".into(),
            health_poll_interval_secs: 300,
            provider_timeout_secs: 5,
            max_fallback_attempts: 1,
            circuit_failure_threshold: 5,
            circuit_recovery_timeout_secs: 30,
            circuit_half_open_max_probes: 2,
            sticky_session_ttl_secs: 1800,
            onboarding_auth_token: None,
        },
        providers: ProviderSettings {
            known_endpoints: vec![provider_endpoint.to_string()],
        },
        chain: ChainConfig {
            enabled: false,
            ..Default::default()
        },
        balance: BalanceConfig {
            enabled: false,
            ..Default::default()
        },
        auth: AuthConfig {
            enabled: false,
            ..Default::default()
        },
        incentive: IncentiveConfig {
            rarity_bonus_enabled: false,
            ..Default::default()
        },
        bridge: BridgeConfig::default(),
        rate_limit: RateLimitConfig {
            enabled: false,
            ..Default::default()
        },
        oracle: OracleConfig::default(),
        free_tier: crate::config::FreeTierConfig::default(),
        events: crate::config::EventsConfig::default(),
        gossip: crate::config::GossipConfig::default(),
        health_v2: crate::config::HealthV2Config::default(),
        ws_chat: crate::config::WsChatConfig::default(),
        dedup: crate::config::DedupConfig::default(),
        cache: crate::config::CacheConfig::default(),
        circuit_breaker: crate::circuit_breaker::CircuitBreakerConfig::default(),
        load_shed: crate::load_shed::LoadShedConfig::default(),
        degradation: crate::degradation::DegradationConfig::default(),
        coalesce: crate::coalesce::CoalesceConfig::default(),
        stream_buffer: crate::stream_buffer::StreamBufferConfig::default(),
        adaptive_routing: crate::config::AdaptiveRoutingConfig::default(),
        telemetry: crate::config::TelemetryConfig::default(),
        admin: crate::config::AdminConfig::default(),
        auto_register: crate::auto_register::AutoRegistrationConfig::default(),
        cache_sync: crate::cache_sync::CacheSyncConfig::default(),
        multi_region: crate::multi_region::RegionConfig::default(),
        ws_pool: crate::config::WsPoolConfig::default(),
    })
}

/// Build a minimal [`AppState`] for testing with one healthy provider.
fn build_test_state(mock_url: &str) -> AppState {
    let config = test_config(mock_url);
    let registry = Arc::new(ProviderRegistry::new(config.clone()));

    // Mark the provider as healthy so the router selects it
    if let Some(mut provider) = registry.providers.get_mut(mock_url) {
        provider.is_healthy = true;
    }

    let health_scorer = std::sync::Arc::new(crate::health_score::HealthScorer::new(
        crate::health_score::HealthScoringConfig::default(),
    ));
    let geo_router = std::sync::Arc::new(crate::geo_router::GeoRouter::new());
    let adaptive_router = std::sync::Arc::new(crate::adaptive_router::AdaptiveRouter::new(
        std::sync::Arc::clone(&health_scorer),
        std::sync::Arc::clone(&geo_router),
        crate::adaptive_router::RoutingConfig::default(),
    ));

    AppState {
        config,
        provider_registry: registry,
        http_client: reqwest::Client::new(),
        usage_store: Arc::new(dashmap::DashMap::new()),
        chain_scanner: None,
        chain_cache: None,
        balance_checker: None,
        auth_verifier: None,
        relay_metrics: Arc::new(RelayMetrics::new()),
        rate_limit_state: None,
        demand: Arc::new(DemandTracker::new(300)),
        erg_usd_rate: Arc::new(std::sync::RwLock::new(None)),
        free_tier_tracker: None,
        event_broadcaster: Arc::new(crate::events::EventBroadcaster::new(10)),
        ws_broadcaster: Arc::new(crate::ws::WsBroadcaster::new(10)),
        gossip_service: None,
        request_dedup: crate::dedup::RequestDedup::new(true, 30),
        ws_chat_connections: crate::ws::WsChatConnectionCounter::new(),
        model_registry: std::sync::Arc::new(crate::model_registry::ModelRegistry::new()),
        response_cache: std::sync::Arc::new(crate::cache::ResponseCache::new(
            crate::cache::CacheConfig::default(),
        )),
        circuit_breaker: std::sync::Arc::new(crate::circuit_breaker::CircuitBreaker::new(
            crate::circuit_breaker::CircuitBreakerConfig::default(),
        )),
        load_shedder: std::sync::Arc::new(crate::load_shed::LoadShedder::new(
            crate::load_shed::LoadShedConfig::default(),
        )),
        degradation_manager: std::sync::Arc::new(crate::degradation::DegradationManager::new(
            crate::degradation::DegradationConfig::default(),
        )),
        request_multiplexer: std::sync::Arc::new(crate::coalesce_buffer::RequestMultiplexer::default()),
        health_scorer,
        geo_router,
        adaptive_router,
        ws_pool: std::sync::Arc::new(crate::ws_pool::WsConnectionPool::new(
            crate::config::WsPoolConfig::default(),
        )),
        priority_queue: std::sync::Arc::new(crate::priority_queue::PriorityQueue::new(
            crate::priority_queue::PriorityQueueConfig::default(),
        )),
        file_store: std::sync::Arc::new(crate::handlers::upload::FileStore::new(
            std::path::PathBuf::from("/tmp/xergon-test-uploads"), 100_000_000,
        )),
        tier_manager: std::sync::Arc::new(crate::rate_limit_tiers::TierManager::new()),
        webhook_manager: crate::webhook::WebhookManager::new(),
        audit_logger: crate::audit::AuditLogger::new(),
        api_key_manager: crate::api_key_manager::ApiKeyManager::new(),
        usage_analytics: crate::usage_analytics::UsageAnalytics::new(),
        auto_register: None,
        sla_tracker: crate::sla::SlaTracker::new(),
        adaptive_retry: std::sync::Arc::new(crate::adaptive_retry::AdaptiveRetry::new(
            crate::adaptive_retry::AdaptiveRetryConfig::default(),
        )),
        cache_synchronizer: None,
        multi_region_router: None,
        semantic_cache: std::sync::Arc::new(crate::semantic_cache::SemanticCache::new()),
        request_audit_buffer: crate::audit::RequestAuditBuffer::new(1000),
        auth_audit_buffer: crate::audit::AuthAuditBuffer::new(1000),
        compliance_audit_buffer: crate::audit::ComplianceAuditBuffer::new(1000),
        bonding_manager: std::sync::Arc::new(crate::reputation_bonding::BondingManager::new(
            crate::reputation_bonding::BondingConfig::default(),
        )),
        staking_pool: std::sync::Arc::new(crate::staking_rewards::StakingRewardPool::new(
            crate::staking_rewards::StakingRewardConfig::default(),
        )),
        oracle_aggregator: std::sync::Arc::new(crate::oracle_aggregator::OracleAggregator::new(
            crate::oracle_aggregator::OracleAggregatorConfig::default(),
        )),
        chainAdapterManager: std::sync::Arc::new(crate::chain_adapters::ChainAdapterManager::new(
            crate::chain_adapters::ChainAdapterConfig::default(),
        )),
        encrypted_inference: std::sync::Arc::new(crate::encrypted_inference::EncryptedInferenceState::new(
            crate::encrypted_inference::EncryptionConfig::default(),
        )),
        quantum_crypto: std::sync::Arc::new(crate::quantum_crypto::QuantumCryptoState::new(
            crate::quantum_crypto::QuantumCryptoConfig::default(),
        )),
        zkp_verification: std::sync::Arc::new(crate::zkp_verification::ZKPVerificationState::new()),
        homomorphic_compute: std::sync::Arc::new(crate::homomorphic_compute::HomomorphicComputeState::new()),
        cross_provider_orchestrator: std::sync::Arc::new(crate::cross_provider_orchestration::CrossProviderOrchestrator::new()),
        speculative_coordinator: std::sync::Arc::new(crate::speculative_decoding::SpeculativeDecodingCoordinator::new()),
        request_fusion: std::sync::Arc::new(crate::request_fusion::RequestFusionEngine::new(
            crate::request_fusion::FusionConfig::default(),
        )),
        continuous_batching: std::sync::Arc::new(crate::continuous_batching::ContinuousBatchingEngine::new(
            crate::continuous_batching::BatchConfig::default(),
        )),
        token_streaming: std::sync::Arc::new(crate::token_streaming::TokenStreamingMultiplexer::new(
            crate::token_streaming::StreamConfig::default(),
        )),
        cost_estimator: std::sync::Arc::new(crate::cost_estimator::InferenceCostEstimator::new()),
        scheduling_optimizer: std::sync::Arc::new(crate::scheduling_optimizer::SchedulingOptimizer::default()),
        dynamic_pricing_engine: std::sync::Arc::new(crate::dynamic_pricing::DynamicPricingEngine::new()),
        capability_negotiation: std::sync::Arc::new(crate::capability_negotiation::CapabilityNegotiator::new()),
        protocol_registry: std::sync::Arc::new(crate::protocol_versioning::ProtocolRegistry::new("1.0.0")),
        connection_pool_v2: std::sync::Arc::new(crate::connection_pool_v2::ConnectionPoolV2::new(
            crate::connection_pool_v2::PoolConfig::default(),
        )),
        request_dedup_v2: std::sync::Arc::new(crate::request_dedup_v2::RequestDedupV2::new()),
        response_cache_headers: std::sync::Arc::new(crate::response_cache_headers::ResponseCache::new()),
        content_negotiator: std::sync::Arc::new(crate::content_negotiation::ContentNegotiator::new()),
        rate_limiter_v2: std::sync::Arc::new(crate::rate_limiter_v2::RateLimiterV2::new()),
        middleware_chain: std::sync::Arc::new(crate::middleware_chain::MiddlewareChain::new()),
        cors_manager_v2: std::sync::Arc::new(crate::cors_v2::CorsManagerV2::new()),
        websocket_v2: std::sync::Arc::new(crate::websocket_v2::WebSocketV2::new()),
        health_monitor_v2: std::sync::Arc::new(crate::health_monitor_v2::HealthMonitorV2::default()),
        api_gateway: std::sync::Arc::new(crate::api_gateway::ApiGateway::new()),
        babel_fee_manager: std::sync::Arc::new(crate::babel_fee_integration::BabelFeeManager::new()),
        request_coalescer: std::sync::Arc::new(crate::request_coalescing::RequestCoalescer::new()),
        protocol_adapter: std::sync::Arc::new(crate::protocol_adapter::ProtocolAdapter::new()),
        ensemble_router: std::sync::Arc::new(crate::ensemble_router::EnsembleRouter::new()),
    }
}

/// Build an [`AppState`] with no providers at all.
fn build_empty_state() -> AppState {
    let base = test_config("http://127.0.0.1:1");
    let config = Arc::new(RelayConfig {
        providers: ProviderSettings {
            known_endpoints: vec![],
        },
        ..(*base).clone()
    });

    let registry = Arc::new(ProviderRegistry::new(config.clone()));

    let health_scorer = std::sync::Arc::new(crate::health_score::HealthScorer::new(
        crate::health_score::HealthScoringConfig::default(),
    ));
    let geo_router = std::sync::Arc::new(crate::geo_router::GeoRouter::new());
    let adaptive_router = std::sync::Arc::new(crate::adaptive_router::AdaptiveRouter::new(
        std::sync::Arc::clone(&health_scorer),
        std::sync::Arc::clone(&geo_router),
        crate::adaptive_router::RoutingConfig::default(),
    ));

    AppState {
        config,
        provider_registry: registry,
        http_client: reqwest::Client::new(),
        usage_store: Arc::new(dashmap::DashMap::new()),
        chain_scanner: None,
        chain_cache: None,
        balance_checker: None,
        auth_verifier: None,
        relay_metrics: Arc::new(RelayMetrics::new()),
        rate_limit_state: None,
        demand: Arc::new(DemandTracker::new(300)),
        erg_usd_rate: Arc::new(std::sync::RwLock::new(None)),
        free_tier_tracker: None,
        event_broadcaster: Arc::new(crate::events::EventBroadcaster::new(10)),
        ws_broadcaster: Arc::new(crate::ws::WsBroadcaster::new(10)),
        gossip_service: None,
        request_dedup: crate::dedup::RequestDedup::new(true, 30),
        ws_chat_connections: crate::ws::WsChatConnectionCounter::new(),
        model_registry: std::sync::Arc::new(crate::model_registry::ModelRegistry::new()),
        response_cache: std::sync::Arc::new(crate::cache::ResponseCache::new(
            crate::cache::CacheConfig::default(),
        )),
        circuit_breaker: std::sync::Arc::new(crate::circuit_breaker::CircuitBreaker::new(
            crate::circuit_breaker::CircuitBreakerConfig::default(),
        )),
        load_shedder: std::sync::Arc::new(crate::load_shed::LoadShedder::new(
            crate::load_shed::LoadShedConfig::default(),
        )),
        degradation_manager: std::sync::Arc::new(crate::degradation::DegradationManager::new(
            crate::degradation::DegradationConfig::default(),
        )),
        request_multiplexer: std::sync::Arc::new(crate::coalesce_buffer::RequestMultiplexer::default()),
        health_scorer,
        geo_router,
        adaptive_router,
        ws_pool: std::sync::Arc::new(crate::ws_pool::WsConnectionPool::new(
            crate::config::WsPoolConfig::default(),
        )),
        priority_queue: std::sync::Arc::new(crate::priority_queue::PriorityQueue::new(
            crate::priority_queue::PriorityQueueConfig::default(),
        )),
        file_store: std::sync::Arc::new(crate::handlers::upload::FileStore::new(
            std::path::PathBuf::from("/tmp/xergon-test-uploads"), 100_000_000,
        )),
        tier_manager: std::sync::Arc::new(crate::rate_limit_tiers::TierManager::new()),
        webhook_manager: crate::webhook::WebhookManager::new(),
        audit_logger: crate::audit::AuditLogger::new(),
        api_key_manager: crate::api_key_manager::ApiKeyManager::new(),
        usage_analytics: crate::usage_analytics::UsageAnalytics::new(),
        auto_register: None,
        sla_tracker: crate::sla::SlaTracker::new(),
        adaptive_retry: std::sync::Arc::new(crate::adaptive_retry::AdaptiveRetry::new(
            crate::adaptive_retry::AdaptiveRetryConfig::default(),
        )),
        cache_synchronizer: None,
        multi_region_router: None,
        semantic_cache: std::sync::Arc::new(crate::semantic_cache::SemanticCache::new()),
        request_audit_buffer: crate::audit::RequestAuditBuffer::new(1000),
        auth_audit_buffer: crate::audit::AuthAuditBuffer::new(1000),
        compliance_audit_buffer: crate::audit::ComplianceAuditBuffer::new(1000),
        bonding_manager: std::sync::Arc::new(crate::reputation_bonding::BondingManager::new(
            crate::reputation_bonding::BondingConfig::default(),
        )),
        staking_pool: std::sync::Arc::new(crate::staking_rewards::StakingRewardPool::new(
            crate::staking_rewards::StakingRewardConfig::default(),
        )),
        oracle_aggregator: std::sync::Arc::new(crate::oracle_aggregator::OracleAggregator::new(
            crate::oracle_aggregator::OracleAggregatorConfig::default(),
        )),
        chainAdapterManager: std::sync::Arc::new(crate::chain_adapters::ChainAdapterManager::new(
            crate::chain_adapters::ChainAdapterConfig::default(),
        )),
        encrypted_inference: std::sync::Arc::new(crate::encrypted_inference::EncryptedInferenceState::new(
            crate::encrypted_inference::EncryptionConfig::default(),
        )),
        quantum_crypto: std::sync::Arc::new(crate::quantum_crypto::QuantumCryptoState::new(
            crate::quantum_crypto::QuantumCryptoConfig::default(),
        )),
        zkp_verification: std::sync::Arc::new(crate::zkp_verification::ZKPVerificationState::new()),
        homomorphic_compute: std::sync::Arc::new(crate::homomorphic_compute::HomomorphicComputeState::new()),
        cross_provider_orchestrator: std::sync::Arc::new(crate::cross_provider_orchestration::CrossProviderOrchestrator::new()),
        speculative_coordinator: std::sync::Arc::new(crate::speculative_decoding::SpeculativeDecodingCoordinator::new()),
        request_fusion: std::sync::Arc::new(crate::request_fusion::RequestFusionEngine::new(
            crate::request_fusion::FusionConfig::default(),
        )),
        continuous_batching: std::sync::Arc::new(crate::continuous_batching::ContinuousBatchingEngine::new(
            crate::continuous_batching::BatchConfig::default(),
        )),
        token_streaming: std::sync::Arc::new(crate::token_streaming::TokenStreamingMultiplexer::new(
            crate::token_streaming::StreamConfig::default(),
        )),
        cost_estimator: std::sync::Arc::new(crate::cost_estimator::InferenceCostEstimator::new()),
        scheduling_optimizer: std::sync::Arc::new(crate::scheduling_optimizer::SchedulingOptimizer::default()),
        dynamic_pricing_engine: std::sync::Arc::new(crate::dynamic_pricing::DynamicPricingEngine::new()),
        capability_negotiation: std::sync::Arc::new(crate::capability_negotiation::CapabilityNegotiator::new()),
        protocol_registry: std::sync::Arc::new(crate::protocol_versioning::ProtocolRegistry::new("1.0.0")),
        connection_pool_v2: std::sync::Arc::new(crate::connection_pool_v2::ConnectionPoolV2::new(
            crate::connection_pool_v2::PoolConfig::default(),
        )),
        request_dedup_v2: std::sync::Arc::new(crate::request_dedup_v2::RequestDedupV2::new()),
        response_cache_headers: std::sync::Arc::new(crate::response_cache_headers::ResponseCache::new()),
        content_negotiator: std::sync::Arc::new(crate::content_negotiation::ContentNegotiator::new()),
        rate_limiter_v2: std::sync::Arc::new(crate::rate_limiter_v2::RateLimiterV2::new()),
        middleware_chain: std::sync::Arc::new(crate::middleware_chain::MiddlewareChain::new()),
        cors_manager_v2: std::sync::Arc::new(crate::cors_v2::CorsManagerV2::new()),
        websocket_v2: std::sync::Arc::new(crate::websocket_v2::WebSocketV2::new()),
        health_monitor_v2: std::sync::Arc::new(crate::health_monitor_v2::HealthMonitorV2::default()),
        api_gateway: std::sync::Arc::new(crate::api_gateway::ApiGateway::new()),
        babel_fee_manager: std::sync::Arc::new(crate::babel_fee_integration::BabelFeeManager::new()),
        request_coalescer: std::sync::Arc::new(crate::request_coalescing::RequestCoalescer::new()),
        protocol_adapter: std::sync::Arc::new(crate::protocol_adapter::ProtocolAdapter::new()),
        ensemble_router: std::sync::Arc::new(crate::ensemble_router::EnsembleRouter::new()),
    }
}

/// A valid OpenAI-compatible chat completion response body.
fn mock_chat_response() -> Value {
    json!({
        "id": "chatcmpl-mock-abc123",
        "object": "chat.completion",
        "created": 1700000000,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello from mock provider!"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15
        }
    })
}

/// Handler for the mock provider's `/v1/chat/completions` endpoint.
async fn mock_provider_handler() -> axum::Json<Value> {
    axum::Json(mock_chat_response())
}

/// Start a mock inference provider on a random loopback port.
///
/// Returns `(base_url, JoinHandle)` where `base_url` is e.g. `http://127.0.0.1:54321`.
async fn start_mock_provider() -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind mock provider listener");
    let addr = listener
        .local_addr()
        .expect("Failed to get mock provider local address");
    let base_url = format!("http://{}", addr);

    let app = Router::new().route("/v1/chat/completions", post(mock_provider_handler));

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("Mock provider server error");
    });

    // Brief pause so the server is ready to accept connections
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, handle)
}

/// Build the relay router with the chat handler and request-id middleware.
fn build_relay_app(state: AppState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(chat_completions_handler))
        .layer(axum_middleware::from_fn(
            crate::middleware::request_id_middleware,
        ))
        .with_state(state)
}

/// Build a standard chat completion request body.
fn chat_request_body(model: &str) -> Value {
    json!({
        "model": model,
        "messages": [{"role": "user", "content": "Say hello"}],
        "stream": false
    })
}

/// Build an HTTP POST request to `/v1/chat/completions`.
fn build_chat_request(body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(body).expect("Failed to serialize request body"),
        ))
        .expect("Failed to build request")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// E2E: Full chat completion flow through the relay.
///
/// 1. Mock provider returns a valid OpenAI response.
/// 2. Relay routes to the provider and proxies the response back.
/// 3. Response contains correct OpenAI fields, request ID, and provider headers.
/// 4. Usage is tracked in the usage store.
#[tokio::test]
async fn e2e_chat_completion_happy_path() {
    // 1. Start mock inference backend
    let (mock_url, _handle) = start_mock_provider().await;

    // 2. Build AppState with the mock as a registered healthy provider
    let state = build_test_state(&mock_url);

    // 3. Build the relay router
    let app = build_relay_app(state.clone());

    // 4. Send a chat completion request
    let body = chat_request_body("test-model");
    let response = app
        .oneshot(build_chat_request(&body))
        .await
        .expect("Relay request failed");

    // 5. Verify 200 OK
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK, got {}",
        response.status()
    );

    // 6. Verify X-Request-Id header is present and non-empty
    let request_id_str = response
        .headers()
        .get("x-request-id")
        .expect("Missing X-Request-Id header in response")
        .to_str()
        .expect("X-Request-Id is not valid UTF-8")
        .to_string();
    assert!(
        !request_id_str.is_empty(),
        "X-Request-Id should not be empty"
    );

    // 7. Verify X-Provider header matches the mock backend
    let provider_header = response
        .headers()
        .get("x-provider")
        .expect("Missing X-Provider header in response")
        .to_str()
        .expect("X-Provider is not valid UTF-8")
        .to_string();
    assert_eq!(
        provider_header,
        mock_url.as_str(),
        "X-Provider should be the mock provider URL"
    );

    // 8. Verify X-Latency-Ms header is present and positive
    let latency_ms: u64 = response
        .headers()
        .get("x-latency-ms")
        .expect("Missing X-Latency-Ms header in response")
        .to_str()
        .expect("X-Latency-Ms is not valid UTF-8")
        .parse()
        .expect("X-Latency-Ms is not a valid number");
    assert!(latency_ms > 0, "X-Latency-Ms should be positive");

    // 9. Verify response body is a valid OpenAI chat completion
    let body_bytes = axum::body::to_bytes(response.into_body(), 100_000)
        .await
        .expect("Failed to read response body");
    let resp_json: Value =
        serde_json::from_slice(&body_bytes).expect("Response body should be valid JSON");

    assert_eq!(
        resp_json["object"], "chat.completion",
        "Response 'object' field should be 'chat.completion'"
    );
    assert!(
        resp_json.get("choices").is_some(),
        "Response should contain 'choices'"
    );
    assert_eq!(
        resp_json["choices"][0]["message"]["role"], "assistant",
        "First choice role should be 'assistant'"
    );
    assert_eq!(
        resp_json["choices"][0]["message"]["content"], "Hello from mock provider!",
        "Assistant content should match the mock response"
    );
    assert_eq!(
        resp_json["choices"][0]["finish_reason"], "stop",
        "Finish reason should be 'stop'"
    );

    // 10. Verify usage fields in response
    let usage = resp_json
        .get("usage")
        .expect("Response should contain 'usage'");
    assert_eq!(usage["prompt_tokens"], 10);
    assert_eq!(usage["completion_tokens"], 5);
    assert_eq!(usage["total_tokens"], 15);

    // 11. Verify usage_store has exactly one record for our request
    assert_eq!(
        state.usage_store.len(),
        1,
        "usage_store should have exactly one record"
    );
    let record = state
        .usage_store
        .get(&request_id_str)
        .expect("usage_store should contain record for our request ID");
    assert_eq!(record.model, "test-model");
    assert_eq!(record.tokens_in, 10);
    assert_eq!(record.tokens_out, 5);
    assert_eq!(record.provider, mock_url);
    assert!(record.latency_ms > 0);
}

/// E2E: When no providers are available, the relay returns 503.
#[tokio::test]
async fn e2e_chat_completion_no_providers_returns_503() {
    let state = build_empty_state();
    let app = build_relay_app(state.clone());

    let body = chat_request_body("test-model");
    let response = app
        .oneshot(build_chat_request(&body))
        .await
        .expect("Relay request should not panic");

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Expected 503 when no providers are available"
    );

    // Usage store should be empty — request never reached a provider
    assert_eq!(state.usage_store.len(), 0);
}

/// E2E: Invalid JSON body returns 400.
#[tokio::test]
async fn e2e_chat_completion_invalid_json_returns_400() {
    let (mock_url, _handle) = start_mock_provider().await;
    let state = build_test_state(&mock_url);
    let app = build_relay_app(state.clone());

    let request = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("Content-Type", "application/json")
        .body(Body::from("not valid json {{{"))
        .expect("Failed to build request");

    let response = app.oneshot(request).await.expect("Relay request should not panic");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 for invalid JSON body"
    );
    assert_eq!(state.usage_store.len(), 0);
}

/// E2E: Missing required fields (model, messages) returns 400.
#[tokio::test]
async fn e2e_chat_completion_missing_fields_returns_400() {
    let (mock_url, _handle) = start_mock_provider().await;
    let state = build_test_state(&mock_url);
    let app = build_relay_app(state.clone());

    // Send a body with no "model" or "messages" fields
    let body = json!({"stream": false});
    let response = app
        .oneshot(build_chat_request(&body))
        .await
        .expect("Relay request should not panic");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected 400 for missing required fields"
    );
    assert_eq!(state.usage_store.len(), 0);
}

/// E2E: The X-Request-Id in the response matches what was generated by the middleware.
#[tokio::test]
async fn e2e_chat_completion_request_id_consistency() {
    let (mock_url, _handle) = start_mock_provider().await;
    let state = build_test_state(&mock_url);
    let app = build_relay_app(state.clone());

    let body = chat_request_body("consistency-model");
    let response = app
        .oneshot(build_chat_request(&body))
        .await
        .expect("Relay request failed");

    assert_eq!(response.status(), StatusCode::OK);

    // Extract request ID from response header
    let rid_header = response
        .headers()
        .get("x-request-id")
        .expect("Missing X-Request-Id")
        .to_str()
        .unwrap()
        .to_string();

    // Read body (consume response)
    let body_bytes = axum::body::to_bytes(response.into_body(), 100_000)
        .await
        .unwrap();

    // The usage_store should have a record keyed by this exact request ID
    assert!(
        state.usage_store.contains_key(&rid_header),
        "usage_store should contain record for request ID '{}'",
        rid_header
    );
    let record = state.usage_store.get(&rid_header).unwrap();
    assert_eq!(record.model, "consistency-model");
    assert!(
        !body_bytes.is_empty(),
        "Response body should not be empty"
    );
}

/// E2E: Demand tracker is updated after a successful request.
#[tokio::test]
async fn e2e_chat_completion_demand_tracked() {
    let (mock_url, _handle) = start_mock_provider().await;
    let state = build_test_state(&mock_url);
    let app = build_relay_app(state.clone());

    let model = "demand-test-model";
    let body = chat_request_body(model);
    let response = app
        .oneshot(build_chat_request(&body))
        .await
        .expect("Relay request failed");

    assert_eq!(response.status(), StatusCode::OK);

    // Demand for this model should be recorded
    let demand_count = state.demand.demand(model);
    assert_eq!(
        demand_count, 1,
        "Demand tracker should record 1 request for '{}'",
        model
    );
    let demand_mult = state.demand.demand_multiplier(model);
    assert!(
        demand_mult > 1.0 && demand_mult < 1.1,
        "Demand multiplier for 1 request should be slightly above 1.0, got {}",
        demand_mult
    );
}
