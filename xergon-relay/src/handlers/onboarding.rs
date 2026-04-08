#![allow(dead_code)]
//! Provider Onboarding API
//!
//! Endpoints:
//!   POST /v1/providers/onboard           -- register a new provider
//!   GET  /v1/providers/onboard/:pk       -- get onboarding status
//!   POST /v1/providers/onboard/:pk/test  -- test provider connectivity
//!   DELETE /v1/providers/:pk             -- deregister provider

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::proxy::AppState;
use crate::provider::XergonAgentStatus;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OnboardRequest {
    pub endpoint: String,
    pub region: String,
    pub auth_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OnboardResponse {
    pub provider_pk: String,
    pub models: Vec<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct OnboardingStatus {
    pub status: String,
    pub steps_completed: Vec<String>,
    pub steps_remaining: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TestResponse {
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
    pub model_tested: String,
}

#[derive(Debug, Serialize)]
pub struct DeregisterResponse {
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ---------------------------------------------------------------------------
// POST /v1/providers/onboard
// ---------------------------------------------------------------------------

pub async fn onboard_provider_handler(
    State(state): State<AppState>,
    Json(req): Json<OnboardRequest>,
) -> impl IntoResponse {
    let endpoint = req.endpoint.trim_end_matches('/').to_string();

    // Validate auth_token if relay config requires it
    if let Some(ref required_token) = state.config.relay.onboarding_auth_token {
        match &req.auth_token {
            Some(token) if token == required_token => {}
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse {
                        error: "Invalid or missing auth_token for onboarding".into(),
                    }),
                )
                    .into_response();
            }
        }
    }

    // Try to reach the provider's /xergon/status
    let status_url = format!("{}/xergon/status", endpoint);
    let start = std::time::Instant::now();

    let status_result = state
        .http_client
        .get(&status_url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match status_result {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<XergonAgentStatus>().await {
                Ok(agent_status) => {
                    // Extract provider public key
                    let provider_pk = agent_status
                        .provider
                        .as_ref()
                        .map(|p| p.id.clone())
                        .unwrap_or_default();

                    if provider_pk.is_empty() {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                error: "Provider did not return a valid provider ID in /xergon/status"
                                    .into(),
                            }),
                        )
                            .into_response();
                    }

                    // Extract models
                    let models: Vec<String> = if let Some(ref pown) = agent_status.pown_status {
                        if pown.ai_enabled && !pown.ai_model.is_empty() {
                            vec![pown.ai_model.clone()]
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };

                    // Add to provider registry
                    state.provider_registry.add_provider(endpoint.clone(), false);

                    // Update region and model info on the provider entry
                    if let Some(mut provider) =
                        state.provider_registry.providers.get_mut(&endpoint)
                    {
                        provider.status = Some(agent_status);
                        provider.latency_ms = latency_ms;
                        provider.is_healthy = true;
                        provider.last_healthy_at = chrono::Utc::now();
                        provider.last_successful_check = std::time::Instant::now();
                    }

                    // Sync models into model registry
                    let sync_models: Vec<crate::model_registry::SyncModelInfo> = models
                        .iter()
                        .map(|m| crate::model_registry::SyncModelInfo {
                            model_id: m.clone(),
                            context_length: 4096, // default, will be enriched by health polling
                            pricing_nanoerg_per_million_tokens: 0,
                        })
                        .collect();

                    state.model_registry.sync_from_provider(
                        &provider_pk,
                        &endpoint,
                        sync_models,
                    );

                    info!(
                        provider_pk = %provider_pk,
                        endpoint = %endpoint,
                        models = ?models,
                        latency_ms,
                        "Provider onboarded successfully"
                    );

                    (
                        StatusCode::OK,
                        Json(OnboardResponse {
                            provider_pk,
                            models,
                            status: "registered".to_string(),
                        }),
                    )
                        .into_response()
                }
                Err(e) => {
                    warn!(endpoint = %endpoint, error = %e, "Failed to parse provider status");
                    (
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse {
                            error: format!("Failed to parse provider status response: {}", e),
                        }),
                    )
                        .into_response()
                }
            }
        }
        Ok(resp) => {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!(
                        "Provider returned error status: {}",
                        resp.status()
                    ),
                }),
            )
                .into_response()
        }
        Err(e) => {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Provider endpoint not reachable: {}", e),
                }),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// GET /v1/providers/onboard/:provider_pk
// ---------------------------------------------------------------------------

pub async fn onboarding_status_handler(
    State(state): State<AppState>,
    Path(provider_pk): Path<String>,
) -> impl IntoResponse {
    // Find the provider by looking for its public key in status
    let mut found = false;
    let mut endpoint_reachable = false;
    let mut chain_registered = false;
    let mut models_synced = false;
    let mut health_passing = false;

    for entry in state.provider_registry.providers.iter() {
        let provider = entry.value();
        let pk = provider
            .status
            .as_ref()
            .and_then(|s| s.provider.as_ref())
            .map(|p| p.id.clone())
            .unwrap_or_default();

        if pk == provider_pk {
            found = true;
            endpoint_reachable = true;

            // Check if on-chain registered
            if provider.from_chain {
                chain_registered = true;
            }

            // Check if models are synced
            if !provider.served_models.is_empty()
                || provider
                    .status
                    .as_ref()
                    .and_then(|s| s.pown_status.as_ref())
                    .map(|p| !p.ai_model.is_empty())
                    .unwrap_or(false)
            {
                models_synced = true;
            }

            // Check health
            if provider.is_healthy {
                health_passing = true;
            }

            break;
        }
    }

    // Also check model registry for this provider
    let has_models_in_registry = {
        let mut found_any = false;
        for entry in state.model_registry.entries.iter() {
            if entry.value().provider_pk == provider_pk.to_lowercase() {
                found_any = true;
                break;
            }
        }
        found_any
    };
    if has_models_in_registry {
        models_synced = true;
    }

    // Also check chain cache for this provider
    if let Some(ref cache) = state.chain_cache {
        if let Some(providers) = cache.get_providers_or_empty_if_populated() {
            for cp in &providers {
                if cp.provider_pk == provider_pk {
                    chain_registered = true;
                    break;
                }
            }
        }
    }

    if !found {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Provider {} not found in registry", provider_pk),
            }),
        )
            .into_response();
    }

    let _all_steps = vec![
        "endpoint_reachable",
        "chain_registered",
        "models_synced",
        "health_passing",
    ];

    let mut steps_completed = Vec::new();
    let mut steps_remaining = Vec::new();

    if endpoint_reachable {
        steps_completed.push("endpoint_reachable".to_string());
    } else {
        steps_remaining.push("endpoint_reachable".to_string());
    }
    if chain_registered {
        steps_completed.push("chain_registered".to_string());
    } else {
        steps_remaining.push("chain_registered".to_string());
    }
    if models_synced {
        steps_completed.push("models_synced".to_string());
    } else {
        steps_remaining.push("models_synced".to_string());
    }
    if health_passing {
        steps_completed.push("health_passing".to_string());
    } else {
        steps_remaining.push("health_passing".to_string());
    }

    let status = if steps_remaining.is_empty() {
        "complete"
    } else if steps_completed.is_empty() {
        "pending"
    } else {
        "in_progress"
    };

    Json(OnboardingStatus {
        status: status.to_string(),
        steps_completed,
        steps_remaining,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// POST /v1/providers/onboard/:provider_pk/test
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TestRequest {
    pub model: Option<String>,
}

pub async fn test_provider_handler(
    State(state): State<AppState>,
    Path(provider_pk): Path<String>,
) -> impl IntoResponse {
    // Find the provider endpoint by public key
    let endpoint = {
        let mut found = None;
        for entry in state.provider_registry.providers.iter() {
            let provider = entry.value();
            let pk = provider
                .status
                .as_ref()
                .and_then(|s| s.provider.as_ref())
                .map(|p| p.id.clone())
                .unwrap_or_default();
            if pk == provider_pk {
                found = Some(provider.endpoint.clone());
                break;
            }
        }
        match found {
            Some(ep) => ep,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: format!("Provider {} not found in registry", provider_pk),
                    }),
                )
                    .into_response();
            }
        }
    };

    // Determine which model to test
    let model_to_test = {
        let provider = state
            .provider_registry
            .providers
            .get(&endpoint)
            .map(|r| r.value().clone());

        match provider {
            Some(ref p) => {
                // Use the model from pown_status
                p.status
                    .as_ref()
                    .and_then(|s| s.pown_status.as_ref())
                    .map(|pown| pown.ai_model.clone())
                    .unwrap_or_else(|| "unknown".to_string())
            }
            None => "unknown".to_string(),
        }
    };

    // Send a small test chat completion request
    let test_url = format!("{}/v1/chat/completions", endpoint);
    let test_body = serde_json::json!({
        "model": model_to_test,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
        "stream": false
    });

    let start = std::time::Instant::now();

    let result = state
        .http_client
        .post(&test_url)
        .timeout(std::time::Duration::from_secs(15))
        .json(&test_body)
        .send()
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(resp) if resp.status().is_success() => {
            // Consume the body to avoid resource leaks
            let _ = resp.bytes().await;
            Json(TestResponse {
                latency_ms,
                success: true,
                error: None,
                model_tested: model_to_test,
            })
            .into_response()
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            // Truncate error body for readability
            let error_msg = if body.len() > 200 {
                format!("{} (truncated)", &body[..200])
            } else {
                body
            };
            Json(TestResponse {
                latency_ms,
                success: false,
                error: Some(format!("HTTP {}: {}", status, error_msg)),
                model_tested: model_to_test,
            })
            .into_response()
        }
        Err(e) => Json(TestResponse {
            latency_ms,
            success: false,
            error: Some(format!("Request failed: {}", e)),
            model_tested: model_to_test,
        })
        .into_response(),
    }
}

// ---------------------------------------------------------------------------
// DELETE /v1/providers/:provider_pk
// ---------------------------------------------------------------------------

pub async fn deregister_provider_handler(
    State(state): State<AppState>,
    Path(provider_pk): Path<String>,
) -> impl IntoResponse {
    // Find the provider endpoint by public key
    let endpoint = {
        let mut found = None;
        for entry in state.provider_registry.providers.iter() {
            let provider = entry.value();
            let pk = provider
                .status
                .as_ref()
                .and_then(|s| s.provider.as_ref())
                .map(|p| p.id.clone())
                .unwrap_or_default();
            if pk == provider_pk {
                found = Some(provider.endpoint.clone());
                break;
            }
        }
        match found {
            Some(ep) => ep,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: format!("Provider {} not found in registry", provider_pk),
                    }),
                )
                    .into_response();
            }
        }
    };

    // Remove from provider registry (force = true to allow removing any provider)
    state
        .provider_registry
        .remove_provider(&endpoint, true);

    // Remove from model registry
    state.model_registry.remove_provider(&provider_pk);

    info!(
        provider_pk = %provider_pk,
        endpoint = %endpoint,
        "Provider deregistered"
    );

    Json(DeregisterResponse {
        status: "removed".to_string(),
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// Helper for chain_cache to support get_providers_or_empty_if_populated
// ---------------------------------------------------------------------------

trait ChainCacheExt {
    fn get_providers_or_empty_if_populated(&self) -> Option<Vec<crate::chain::ChainProvider>>;
}

impl ChainCacheExt for std::sync::Arc<crate::chain_cache::ChainCache> {
    fn get_providers_or_empty_if_populated(&self) -> Option<Vec<crate::chain::ChainProvider>> {
        if self.is_populated() {
            Some(self.get_providers_or_empty())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use std::sync::Arc;
    use tower::ServiceExt;

    /// Build a test AppState (no chain, no auth, no rate limit).
    fn test_app_state() -> AppState {
        use crate::config::*;
        use crate::demand::DemandTracker;
        use crate::dedup::RequestDedup;
        use crate::events::EventBroadcaster;
        use crate::metrics::RelayMetrics;
        use crate::model_registry::ModelRegistry;
        use crate::ws::{WsBroadcaster, WsChatConnectionCounter};

        let config = Arc::new(RelayConfig {
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
                known_endpoints: vec![],
            },
            chain: ChainConfig {
                enabled: false,
                ..ChainConfig::default()
            },
            balance: BalanceConfig::default(),
            auth: AuthConfig {
                enabled: false,
                ..AuthConfig::default()
            },
            incentive: IncentiveConfig::default(),
            bridge: BridgeConfig::default(),
            rate_limit: RateLimitConfig {
                enabled: false,
                ..RateLimitConfig::default()
            },
            oracle: OracleConfig::default(),
            free_tier: FreeTierConfig::default(),
            events: EventsConfig::default(),
            gossip: GossipConfig::default(),
            health_v2: HealthV2Config::default(),
            ws_chat: WsChatConfig::default(),
            dedup: DedupConfig::default(),
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
        });

        let provider_registry = Arc::new(crate::provider::ProviderRegistry::new(config.clone()));
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();

        let health_scorer = Arc::new(crate::health_score::HealthScorer::new(
            crate::health_score::HealthScoringConfig::default(),
        ));
        let geo_router = Arc::new(crate::geo_router::GeoRouter::new());
        let adaptive_router = Arc::new(crate::adaptive_router::AdaptiveRouter::new(
            Arc::clone(&health_scorer),
            Arc::clone(&geo_router),
            crate::adaptive_router::RoutingConfig::default(),
        ));

        AppState {
            config,
            provider_registry,
            http_client,
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
            event_broadcaster: Arc::new(EventBroadcaster::new(10)),
            ws_broadcaster: Arc::new(WsBroadcaster::new(10)),
            gossip_service: None,
            request_dedup: crate::dedup::RequestDedup::new(false, 30),
            ws_chat_connections: WsChatConnectionCounter::new(),
            model_registry: Arc::new(ModelRegistry::new()),
            response_cache: Arc::new(crate::cache::ResponseCache::new(
                crate::cache::CacheConfig::default(),
            )),
            circuit_breaker: Arc::new(crate::circuit_breaker::CircuitBreaker::new(
                crate::circuit_breaker::CircuitBreakerConfig::default(),
            )),
            load_shedder: Arc::new(crate::load_shed::LoadShedder::new(
                crate::load_shed::LoadShedConfig::default(),
            )),
            degradation_manager: Arc::new(crate::degradation::DegradationManager::new(
                crate::degradation::DegradationConfig::default(),
            )),
            request_multiplexer: Arc::new(crate::coalesce_buffer::RequestMultiplexer::default()),
            health_scorer,
            geo_router,
            adaptive_router,
            ws_pool: Arc::new(crate::ws_pool::WsConnectionPool::new(
                crate::config::WsPoolConfig::default(),
            )),
            priority_queue: Arc::new(crate::priority_queue::PriorityQueue::new(
                crate::priority_queue::PriorityQueueConfig::default(),
            )),
            file_store: Arc::new(crate::handlers::upload::FileStore::new(
                std::path::PathBuf::from("/tmp/xergon-test-uploads"), 100_000_000,
            )),
            tier_manager: Arc::new(crate::rate_limit_tiers::TierManager::new()),
            webhook_manager: crate::webhook::WebhookManager::new(),
            audit_logger: crate::audit::AuditLogger::new(),
            api_key_manager: crate::api_key_manager::ApiKeyManager::new(),
            usage_analytics: crate::usage_analytics::UsageAnalytics::new(),
            auto_register: None,
            sla_tracker: crate::sla::SlaTracker::new(),
            adaptive_retry: Arc::new(crate::adaptive_retry::AdaptiveRetry::new(
                crate::adaptive_retry::AdaptiveRetryConfig::default(),
            )),
            cache_synchronizer: None,
            multi_region_router: None,
            semantic_cache: Arc::new(crate::semantic_cache::SemanticCache::new()),
            request_audit_buffer: crate::audit::RequestAuditBuffer::new(1000),
            auth_audit_buffer: crate::audit::AuthAuditBuffer::new(1000),
            compliance_audit_buffer: crate::audit::ComplianceAuditBuffer::new(1000),
            bonding_manager: Arc::new(crate::reputation_bonding::BondingManager::new(
                crate::reputation_bonding::BondingConfig::default(),
            )),
            staking_pool: Arc::new(crate::staking_rewards::StakingRewardPool::new(
                crate::staking_rewards::StakingRewardConfig::default(),
            )),
            oracle_aggregator: Arc::new(crate::oracle_aggregator::OracleAggregator::new(
                crate::oracle_aggregator::OracleAggregatorConfig::default(),
            )),
            chainAdapterManager: Arc::new(crate::chain_adapters::ChainAdapterManager::new(
                crate::chain_adapters::ChainAdapterConfig::default(),
            )),
            encrypted_inference: Arc::new(crate::encrypted_inference::EncryptedInferenceState::new(
                crate::encrypted_inference::EncryptionConfig::default(),
            )),
            quantum_crypto: Arc::new(crate::quantum_crypto::QuantumCryptoState::new(
                crate::quantum_crypto::QuantumCryptoConfig::default(),
            )),
            zkp_verification: Arc::new(crate::zkp_verification::ZKPVerificationState::new()),
            homomorphic_compute: Arc::new(crate::homomorphic_compute::HomomorphicComputeState::new()),
            cross_provider_orchestrator: Arc::new(
                crate::cross_provider_orchestration::CrossProviderOrchestrator::new(),
            ),
            speculative_coordinator: Arc::new(
                crate::speculative_decoding::SpeculativeDecodingCoordinator::new(),
            ),
            request_fusion: Arc::new(crate::request_fusion::RequestFusionEngine::new(
                crate::request_fusion::FusionConfig::default(),
            )),
            continuous_batching: Arc::new(crate::continuous_batching::ContinuousBatchingEngine::new(
                crate::continuous_batching::BatchConfig::default(),
            )),
            token_streaming: Arc::new(crate::token_streaming::TokenStreamingMultiplexer::new(
                crate::token_streaming::StreamConfig::default(),
            )),
            cost_estimator: Arc::new(crate::cost_estimator::InferenceCostEstimator::new()),
            scheduling_optimizer: Arc::new(crate::scheduling_optimizer::SchedulingOptimizer::default()),
            dynamic_pricing_engine: Arc::new(crate::dynamic_pricing::DynamicPricingEngine::new()),
            capability_negotiation: Arc::new(crate::capability_negotiation::CapabilityNegotiator::new()),
            protocol_registry: Arc::new(crate::protocol_versioning::ProtocolRegistry::new("1.0.0")),
            connection_pool_v2: Arc::new(crate::connection_pool_v2::ConnectionPoolV2::new(
                crate::connection_pool_v2::PoolConfig::default(),
            )),
            request_dedup_v2: Arc::new(crate::request_dedup_v2::RequestDedupV2::new()),
            response_cache_headers: Arc::new(crate::response_cache_headers::ResponseCache::new()),
            content_negotiator: Arc::new(crate::content_negotiation::ContentNegotiator::new()),
            rate_limiter_v2: Arc::new(crate::rate_limiter_v2::RateLimiterV2::new()),
            middleware_chain: Arc::new(crate::middleware_chain::MiddlewareChain::new()),
            cors_manager_v2: Arc::new(crate::cors_v2::CorsManagerV2::new()),
            websocket_v2: Arc::new(crate::websocket_v2::WebSocketV2::new()),
            health_monitor_v2: Arc::new(crate::health_monitor_v2::HealthMonitorV2::default()),
            api_gateway: Arc::new(crate::api_gateway::ApiGateway::new()),
            babel_fee_manager: Arc::new(crate::babel_fee_integration::BabelFeeManager::new()),
            request_coalescer: Arc::new(crate::request_coalescing::RequestCoalescer::new()),
            protocol_adapter: Arc::new(crate::protocol_adapter::ProtocolAdapter::new()),
            ensemble_router: Arc::new(crate::ensemble_router::EnsembleRouter::new()),
        }
    }

    #[tokio::test]
    async fn test_onboard_unreachable_endpoint() {
        let state = test_app_state();
        let app = axum::Router::new()
            .route("/v1/providers/onboard", axum::routing::post(onboard_provider_handler))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/providers/onboard")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"endpoint": "http://127.0.0.1:1", "region": "us-east"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_onboarding_status_not_found() {
        let state = test_app_state();
        let app = axum::Router::new()
            .route(
                "/v1/providers/onboard/{provider_pk}",
                axum::routing::get(onboarding_status_handler),
            )
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/providers/onboard/nonexistent-pk")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_test_provider_not_found() {
        let state = test_app_state();
        let app = axum::Router::new()
            .route(
                "/v1/providers/onboard/{provider_pk}/test",
                axum::routing::post(test_provider_handler),
            )
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/providers/onboard/nonexistent-pk/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_deregister_provider_not_found() {
        let state = test_app_state();
        let app = axum::Router::new()
            .route(
                "/v1/providers/{provider_pk}",
                axum::routing::delete(deregister_provider_handler),
            )
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/providers/nonexistent-pk")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
