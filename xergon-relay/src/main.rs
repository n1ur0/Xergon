//! Xergon Relay - Thin stateless router
//!
//! Sits between frontend users and Xergon providers.
//! Features:
//! - OpenAI-compatible API (POST /v1/chat/completions, GET /v1/models)
//! - Smart routing (latency + PoNW + load)
//! - Fallback chain on provider failure
//! - SSE streaming for responses
//! - Provider health polling
//! - Ergo chain-state provider discovery

mod adaptive_retry;
mod adaptive_router;
mod admin;
mod api_key_manager;
mod api_version;
mod auth;
mod audit;
mod auto_register;
mod balance;
mod cache_sync;
mod capability_negotiation;
mod chain_adapters;
mod connection_pool_v2;
mod multi_region;
mod oracle_aggregator;
mod protocol_versioning;
mod cache;
mod cache_middleware;
mod chain;
mod chain_cache;
pub mod cost_estimator;
mod circuit_breaker;
mod coalesce;
mod dynamic_pricing;
mod coalesce_buffer;
mod config;
mod dedup;
pub mod ensemble_router;
mod encrypted_inference;
mod demand;
mod degradation;
mod events;
mod free_tier;
mod geo_router;
mod gossip;
mod graphql;
mod grpc;
mod handlers;
mod health;
mod health_score;
mod load_shed;
mod metrics;
mod middleware;
mod model_registry;
mod openapi;
mod priority_queue;
mod provider;
pub mod proxy;
pub mod quantum_crypto;
pub mod zkp_verification;
pub mod homomorphic_compute;
pub mod cross_provider_orchestration;
pub mod speculative_decoding;
pub mod request_fusion;
pub mod continuous_batching;
pub mod token_streaming;
mod rate_limit;
mod rate_limit_tiers;
mod rent_guard;
mod reputation_bonding;
mod staking_rewards;
mod schemas;
mod semantic_cache;
pub mod scheduling_optimizer;
pub mod utxo_consolidation;
pub mod storage_rent_monitor;
pub mod tokenomics_engine;
mod sla;
mod stream_buffer;
mod telemetry;
mod request_dedup_v2;
mod response_cache_headers;
mod content_negotiation;
mod rate_limiter_v2;
mod middleware_chain;
mod cors_v2;
mod tracing_middleware;
mod usage_analytics;
mod websocket_v2;
mod health_monitor_v2;
mod cross_chain_bridge;
mod cross_chain_event_router;
mod api_gateway;
mod babel_fee_integration;
mod request_coalescing;
mod protocol_adapter;
mod util;
mod webhook;
mod ws;
mod ws_pool;

#[cfg(test)]
mod e2e_tests;

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};
use axum::middleware as axum_middleware;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use proxy::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xergon_relay=info,tower_http=debug".into()),
        )
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info!("Xergon Relay starting...");

    let config = config::RelayConfig::load().context("Failed to load configuration")?;
    config.validate().map_err(|e| anyhow::anyhow!("Configuration validation failed: {e}"))?;
    let config = Arc::new(config);

    info!(
        listen_addr = %config.relay.listen_addr,
        known_providers = config.providers.known_endpoints.len(),
        health_poll_secs = config.relay.health_poll_interval_secs,
        chain_enabled = config.chain.enabled,
        chain_node = %config.chain.ergo_node_url,
        chain_scan_secs = config.chain.scan_interval_secs,
        chain_cache_ttl_secs = config.chain.cache_ttl_secs,
        "Configuration loaded"
    );

    // Build shared state
    let provider_registry = Arc::new(provider::ProviderRegistry::new(config.clone()));
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.relay.provider_timeout_secs))
        .build()
        .context("Failed to build HTTP client")?;

    // Create chain scanner (optional — None if chain is disabled)
    let chain_scanner = if config.chain.enabled {
        Some(Arc::new(chain::ChainScanner::new(Arc::new(config.chain.clone()))))
    } else {
        None
    };

    // Create balance checker (optional — None if balance checking is disabled)
    let balance_checker = if config.balance.enabled {
        info!(
            balance_enabled = true,
            min_nanoerg = config.balance.min_balance_nanoerg,
            cache_ttl = config.balance.cache_ttl_secs,
            free_tier = config.balance.free_tier_requests,
            "Balance checking enabled"
        );
        Some(Arc::new(balance::BalanceChecker::new(&config.balance)))
    } else {
        info!("Balance checking is disabled");
        None
    };

    // Create auth verifier (optional — None if auth is disabled)
    let auth_verifier = if config.auth.enabled {
        info!(
            auth_enabled = true,
            max_age_secs = config.auth.max_age_secs,
            replay_cache_size = config.auth.replay_cache_size,
            require_staking_box = config.auth.require_staking_box,
            "Signature-based auth enabled"
        );
        Some(Arc::new(auth::AuthVerifier::new(&config.auth)))
    } else {
        info!("Signature-based auth is disabled");
        None
    };

    // Create chain cache (optional — None if chain is disabled)
    let chain_cache = if config.chain.enabled {
        Some(Arc::new(chain_cache::ChainCache::new(
            std::time::Duration::from_secs(config.chain.cache_ttl_secs),
        )))
    } else {
        None
    };

    let relay_metrics = Arc::new(metrics::RelayMetrics::new());

    // Create demand tracker (5-minute sliding window)
    let demand_tracker = Arc::new(demand::DemandTracker::new(300));

    // Create rate limiter state (before AppState so we share the same metrics)
    // Rate limits are now balance-based: ERG staking balance determines tier.
    let rate_limit_state: Option<Arc<rate_limit::RateLimitState>> = if config.rate_limit.enabled {
        info!(
            enabled = true,
            balance_checker_available = balance_checker.is_some(),
            "Balance-based rate limiting enabled"
        );
        Some(Arc::new(rate_limit::RateLimitState::new(
            config.rate_limit.clone(),
            relay_metrics.clone(),
            balance_checker.clone(),
        )))
    } else {
        info!("Rate limiting is disabled");
        None
    };

    // Create free tier tracker (optional — None if disabled)
    let free_tier_tracker: Option<Arc<free_tier::FreeTierTracker>> =
        if config.free_tier.enabled {
            info!(
                enabled = true,
                max_requests = config.free_tier.max_requests,
                decay_hours = config.free_tier.decay_hours,
                "Free tier tracking enabled"
            );
            Some(Arc::new(free_tier::FreeTierTracker::new(
                config.free_tier.max_requests,
                config.free_tier.decay_hours,
            )))
        } else {
            info!("Free tier tracking is disabled");
            None
        };

    // Create SSE event broadcaster
    let event_broadcaster = Arc::new(events::EventBroadcaster::new(
        config.events.max_subscribers + 1, // +1 headroom for internal use
    ));
    if config.events.enabled {
        info!(
            max_subscribers = config.events.max_subscribers,
            "SSE events endpoint enabled"
        );
    } else {
        info!("SSE events endpoint is disabled");
    }

    // Create WebSocket broadcaster for real-time provider status
    let ws_broadcaster = Arc::new(ws::WsBroadcaster::new(
        config.events.max_subscribers + 1,
    ));
    info!("WebSocket /ws/status endpoint enabled");

    // Create model registry (aggregated view of all models across providers)
    let model_registry = Arc::new(model_registry::ModelRegistry::new());

    // Create response cache
    let cache_config = cache::CacheConfig {
        enabled: config.cache.enabled,
        max_entries: config.cache.max_entries,
        default_ttl_secs: config.cache.default_ttl_secs,
        model_list_ttl_secs: config.cache.model_list_ttl_secs,
        provider_list_ttl_secs: config.cache.provider_list_ttl_secs,
        health_ttl_secs: config.cache.health_ttl_secs,
        max_entry_size_bytes: config.cache.max_entry_size_bytes,
    };
    let response_cache = Arc::new(cache::ResponseCache::new(cache_config));

    if config.cache.enabled {
        info!(
            max_entries = config.cache.max_entries,
            default_ttl = config.cache.default_ttl_secs,
            models_ttl = config.cache.model_list_ttl_secs,
            providers_ttl = config.cache.provider_list_ttl_secs,
            health_ttl = config.cache.health_ttl_secs,
            max_entry_size = config.cache.max_entry_size_bytes,
            "Response caching enabled"
        );
    } else {
        info!("Response caching is disabled");
    }

    // Create gossip service for multi-relay consensus
    let gossip_service = {
        let gs = Arc::new(gossip::GossipService::new(&config.gossip));
        if gs.is_enabled() {
            info!(
                relay_id = %gs.relay_id(),
                peers = ?gs.get_peers(),
                "Gossip consensus enabled"
            );
            Some(gs)
        } else {
            info!("Gossip consensus disabled (no peers configured) -- solo mode");
            None
        }
    };

    // Clone http_client for gossip background task ( AppState moves it)
    let gossip_http_client = http_client.clone();

    // Create circuit breaker (standalone, supplements provider-embedded CB)
    let circuit_breaker = Arc::new(circuit_breaker::CircuitBreaker::new(
        config.circuit_breaker.clone(),
    ));
    info!(
        enabled = true,
        failure_threshold = config.circuit_breaker.failure_threshold,
        timeout_secs = config.circuit_breaker.timeout_secs,
        "Circuit breaker enabled"
    );

    // Create load shedder
    let load_shedder = Arc::new(load_shed::LoadShedder::new(config.load_shed.clone()));
    if config.load_shed.enabled {
        info!(
            enabled = true,
            max_concurrent = config.load_shed.max_concurrent_requests,
            max_queue = config.load_shed.max_queue_size,
            "Load shedding enabled"
        );
        load_shedder.start_monitor();
    } else {
        info!("Load shedding is disabled");
    }

    // Create degradation manager
    let degradation_manager = Arc::new(degradation::DegradationManager::new(
        config.degradation.clone(),
    ));
    if config.degradation.enabled {
        info!(
            enabled = true,
            auto_degrade = config.degradation.auto_degrade,
            reduced_max_tokens = config.degradation.reduced_max_tokens,
            "Graceful degradation enabled"
        );
    } else {
        info!("Graceful degradation is disabled");
    }

    // Create request multiplexer (coalescing + stream buffering)
    let request_multiplexer = Arc::new(coalesce_buffer::RequestMultiplexer::new(
        coalesce_buffer::MultiplexerConfig {
            coalesce: config.coalesce.clone(),
            stream_buffer: config.stream_buffer.clone(),
            channel_buffer_size: 64,
        },
    ));
    if config.coalesce.enabled {
        info!(
            enabled = true,
            max_wait_ms = config.coalesce.max_wait_ms,
            max_batch_size = config.coalesce.max_batch_size,
            "Request coalescing enabled"
        );
    } else {
        info!("Request coalescing is disabled");
    }
    if config.stream_buffer.enabled {
        info!(
            enabled = true,
            max_buffer_size = config.stream_buffer.max_buffer_size,
            max_buffer_bytes = config.stream_buffer.max_buffer_bytes,
            "Stream buffering enabled"
        );
    } else {
        info!("Stream buffering is disabled");
    }

    // --- Adaptive routing: HealthScorer, GeoRouter, AdaptiveRouter ---
    let health_scorer = Arc::new(health_score::HealthScorer::new(
        health_score::HealthScoringConfig::default(),
    ));
    info!("HealthScorer initialized (adaptive routing)");

    let geo_router = Arc::new(geo_router::GeoRouter::new());
    info!("GeoRouter initialized (adaptive routing)");

    let adaptive_router_config = adaptive_router::RoutingConfig {
        strategy: adaptive_router::RoutingStrategy::HealthScore,
        max_retries_per_provider: 2,
        fallback_enabled: true,
        geo_routing_enabled: config.adaptive_routing.geo_routing_enabled,
        sticky_sessions: true,
        sticky_ttl_secs: config.adaptive_routing.sticky_session_ttl_secs,
        circuit_breaker_threshold: config.adaptive_routing.circuit_breaker_threshold,
    };
    let adaptive_router = Arc::new(adaptive_router::AdaptiveRouter::new(
        health_scorer.clone(),
        geo_router.clone(),
        adaptive_router_config,
    ));
    info!(
        strategy = %config.adaptive_routing.strategy,
        geo_enabled = config.adaptive_routing.geo_routing_enabled,
        sticky_ttl_secs = config.adaptive_routing.sticky_session_ttl_secs,
        "AdaptiveRouter initialized"
    );

    // Initialize WebSocket connection pool
    let ws_pool = Arc::new(ws_pool::WsConnectionPool::new(
        config.ws_pool.clone(),
    ));
    let ws_pool_maintenance_handle = ws_pool.clone().start_maintenance_task();
    info!(
        enabled = config.ws_pool.enabled,
        max_per_provider = config.ws_pool.max_per_provider,
        idle_timeout_secs = config.ws_pool.idle_timeout_secs,
        max_age_secs = config.ws_pool.max_age_secs,
        "WS connection pool initialized"
    );

    let mut state = AppState {
        config: config.clone(),
        provider_registry: provider_registry.clone(),
        http_client: http_client.clone(),
        usage_store: Arc::new(dashmap::DashMap::new()),
        chain_scanner: chain_scanner.clone(),
        chain_cache: chain_cache.clone(),
        balance_checker: balance_checker.clone(),
        auth_verifier,
        relay_metrics,
        rate_limit_state: rate_limit_state.clone(),
        demand: demand_tracker.clone(),
        erg_usd_rate: Arc::new(std::sync::RwLock::new(None)),
        free_tier_tracker: free_tier_tracker.clone(),
        event_broadcaster: event_broadcaster.clone(),
        ws_broadcaster: ws_broadcaster.clone(),
        gossip_service: gossip_service.clone(),
        request_dedup: dedup::RequestDedup::new(
            config.dedup.enabled,
            config.dedup.window_secs,
        ),
        ws_chat_connections: ws::WsChatConnectionCounter::new(),
        ws_pool: ws_pool.clone(),
        model_registry: model_registry.clone(),
        response_cache: response_cache.clone(),
        circuit_breaker: circuit_breaker.clone(),
        load_shedder: load_shedder.clone(),
        degradation_manager: degradation_manager.clone(),
        request_multiplexer: request_multiplexer.clone(),
        adaptive_router: adaptive_router.clone(),
        health_scorer: health_scorer.clone(),
        geo_router: geo_router.clone(),
        priority_queue: Arc::new(priority_queue::PriorityQueue::new(
            priority_queue::PriorityQueueConfig::default(),
        )),
        file_store: Arc::new(handlers::upload::FileStore::new(
            std::path::PathBuf::from("./uploads"),
            100 * 1024 * 1024, // 100MB default
        )),
        tier_manager: Arc::new(rate_limit_tiers::TierManager::new()),
        webhook_manager: webhook::WebhookManager::new(),
        audit_logger: audit::AuditLogger::new(),
        api_key_manager: api_key_manager::ApiKeyManager::new(),
        usage_analytics: usage_analytics::UsageAnalytics::new(),
        auto_register: None, // initialized below if enabled
        sla_tracker: sla::SlaTracker::new(),
        adaptive_retry: Arc::new(adaptive_retry::AdaptiveRetry::new(
            adaptive_retry::AdaptiveRetryConfig::default(),
        )),
        cache_synchronizer: None, // initialized below if enabled
        multi_region_router: None, // initialized below if configured
        semantic_cache: Arc::new(semantic_cache::SemanticCache::new()),
        request_audit_buffer: audit::RequestAuditBuffer::new(10_000),
        auth_audit_buffer: audit::AuthAuditBuffer::new(10_000),
        compliance_audit_buffer: audit::ComplianceAuditBuffer::new(10_000),
        bonding_manager: Arc::new(reputation_bonding::BondingManager::new(
            reputation_bonding::BondingConfig::default(),
        )),
        staking_pool: Arc::new(staking_rewards::StakingRewardPool::new(
            staking_rewards::StakingRewardConfig::default(),
        )),
        oracle_aggregator: Arc::new(oracle_aggregator::OracleAggregator::new(
            oracle_aggregator::OracleAggregatorConfig::default(),
        )),
        chainAdapterManager: Arc::new(chain_adapters::ChainAdapterManager::new(
            chain_adapters::ChainAdapterConfig::default(),
        )),
        encrypted_inference: Arc::new(encrypted_inference::EncryptedInferenceState::new(
            encrypted_inference::EncryptionConfig::default(),
        )),
        quantum_crypto: Arc::new(quantum_crypto::QuantumCryptoState::new(
            quantum_crypto::QuantumCryptoConfig::default(),
        )),
        zkp_verification: Arc::new(zkp_verification::ZKPVerificationState::new()),
        homomorphic_compute: Arc::new(homomorphic_compute::HomomorphicComputeState::new()),
        cross_provider_orchestrator: Arc::new(cross_provider_orchestration::CrossProviderOrchestrator::new()),
        speculative_coordinator: Arc::new(speculative_decoding::SpeculativeDecodingCoordinator::new()),
        request_fusion: Arc::new(request_fusion::RequestFusionEngine::new(
            request_fusion::FusionConfig::default(),
        )),
        continuous_batching: Arc::new(continuous_batching::ContinuousBatchingEngine::new(
            continuous_batching::BatchConfig::default(),
        )),
        token_streaming: Arc::new(token_streaming::TokenStreamingMultiplexer::new(
            token_streaming::StreamConfig::default(),
        )),
        cost_estimator: Arc::new(cost_estimator::InferenceCostEstimator::new()),
        scheduling_optimizer: Arc::new(scheduling_optimizer::SchedulingOptimizer::default()),
        dynamic_pricing_engine: Arc::new(dynamic_pricing::DynamicPricingEngine::new()),
        capability_negotiation: Arc::new(capability_negotiation::CapabilityNegotiator::new()),
        protocol_registry: Arc::new(protocol_versioning::ProtocolRegistry::new("1.0.0")),
        connection_pool_v2: Arc::new(connection_pool_v2::ConnectionPoolV2::new(
            connection_pool_v2::PoolConfig::default(),
        )),
        request_dedup_v2: Arc::new(request_dedup_v2::RequestDedupV2::new()),
        response_cache_headers: Arc::new(response_cache_headers::ResponseCache::new()),
        content_negotiator: Arc::new(content_negotiation::ContentNegotiator::new()),
        rate_limiter_v2: Arc::new(rate_limiter_v2::RateLimiterV2::new()),
        middleware_chain: Arc::new(middleware_chain::MiddlewareChain::new()),
        cors_manager_v2: Arc::new(cors_v2::CorsManagerV2::new()),
        websocket_v2: Arc::new(websocket_v2::WebSocketV2::new()),
        health_monitor_v2: Arc::new(health_monitor_v2::HealthMonitorV2::default()),
        api_gateway: Arc::new(api_gateway::ApiGateway::new()),
        babel_fee_manager: Arc::new(babel_fee_integration::BabelFeeManager::new()),
        request_coalescer: Arc::new(request_coalescing::RequestCoalescer::new()),
        protocol_adapter: Arc::new(protocol_adapter::ProtocolAdapter::new()),
        ensemble_router: Arc::new(ensemble_router::EnsembleRouter::new()),
    };

    // Wire the shared webhook manager into the SLA tracker
    state.sla_tracker.set_webhook_manager(state.webhook_manager.clone());

    // Spawn provider health polling loop
    let health_handle = {
        let registry = provider_registry.clone();
        let config_ref = config.clone();
        let broadcaster = event_broadcaster.clone();
        let ws_broadcaster = ws_broadcaster.clone();
        let model_reg = model_registry.clone();
        let health_scorer_ref = health_scorer.clone();
        let geo_router_ref = geo_router.clone();
        tokio::spawn(async move {
            loop {
                // Capture health state before polling
                let before_health: std::collections::HashMap<String, bool> = registry
                    .providers
                    .iter()
                    .map(|r| (r.key().clone(), r.value().is_healthy))
                    .collect();

                // Run one health poll cycle
                let endpoints: Vec<String> =
                    registry.providers.iter().map(|r| r.key().clone()).collect();

                let handles: Vec<_> = endpoints
                    .into_iter()
                    .map(|ep| {
                        let registry = registry.clone();
                        let model_reg = model_reg.clone();
                        tokio::spawn(async move {
                            registry.poll_provider(&ep).await;
                            // Sync model registry after health poll
                            sync_models_from_provider(&registry, &model_reg, &ep);
                        })
                    })
                    .collect();

                for handle in handles {
                    if let Err(e) = handle.await {
                        error!(error = %e, "Health poll task panicked");
                    }
                }

                // Publish health-transition events
                events::publish_health_offline_events(&before_health, &registry, &broadcaster);
                events::publish_health_online_events(&before_health, &registry, &broadcaster);

                // Notify WebSocket clients of any health changes
                for entry in registry.providers.iter() {
                    let provider = entry.value();
                    let now_healthy = provider.is_healthy;
                    let before = before_health.get(&provider.endpoint).copied().unwrap_or(false);
                    if before != now_healthy {
                        let id = provider
                            .status
                            .as_ref()
                            .and_then(|s| s.provider.as_ref())
                            .map(|p| p.id.clone())
                            .unwrap_or_else(|| provider.endpoint.clone());
                        let status = if now_healthy { "online" } else { "offline" };
                        ws_broadcaster.notify_provider_status(id, status.to_string());
                    }
                }

                let healthy = registry.healthy_provider_count();
                let degraded = registry.degraded_provider_count();
                let total = registry.providers.len();
                info!(healthy, degraded, total, "Health poll cycle complete");

                // Sync provider registrations with health scorer
                for entry in registry.providers.iter() {
                    let provider = entry.value();
                    let pk = provider
                        .status
                        .as_ref()
                        .and_then(|s| s.provider.as_ref())
                        .map(|p| p.id.clone())
                        .unwrap_or_else(|| provider.endpoint.clone());
                    health_scorer_ref.register_provider(&pk);

                    // Record health outcome in the scorer
                    if provider.is_healthy {
                        health_scorer_ref.record_success(&pk, 0);
                    } else {
                        health_scorer_ref.record_failure(&pk, "health_poll_unhealthy");
                    }
                }

                tokio::time::sleep(Duration::from_secs(
                    config_ref.relay.health_poll_interval_secs,
                ))
                .await;
            }
        })
    };

    // Spawn gossip consensus background task (if enabled)
    let gossip_handle = if let Some(ref gs) = gossip_service {
        gs.clone().start_gossip(
            provider_registry.clone(),
            ws_broadcaster.clone(),
            gossip_http_client,
        )
    } else {
        None
    };

    // Spawn chain-state provider discovery loop (if enabled)
    let chain_handle = if let Some(chain_scanner) = chain_scanner.clone() {
        let registry = provider_registry.clone();
        let cache = chain_cache.clone().expect("chain_cache must exist when chain_scanner exists");
        let poll_interval = config.chain.cache_ttl_secs;

        // Perform initial scan
        {
            let scanner = chain_scanner.clone();
            let reg = registry.clone();
            let hs = health_scorer.clone();
            let cache_clone = cache.clone();
            let broadcaster = event_broadcaster.clone();
            tokio::spawn(async move {
                info!("Performing initial chain scan for providers...");
                if scanner.check_node_health().await {
                    info!("Ergo node is reachable");
                    broadcaster.publish(events::RelayEvent::NodeStatusChange {
                        healthy: true,
                        message: "Ergo node reachable".into(),
                    });
                } else {
                    warn!("Ergo node is not reachable - chain discovery disabled until node is available");
                    broadcaster.publish(events::RelayEvent::NodeStatusChange {
                        healthy: false,
                        message: "Ergo node unreachable".into(),
                    });
                }
                let providers = scanner.scan().await;
                info!(count = providers.len(), "Initial chain scan complete");
                // Publish events for all discovered providers (initial scan treats all as new)
                if !providers.is_empty() {
                    events::diff_and_publish_events(&[], &providers, &broadcaster);
                }
                cache_clone.update(providers.clone());
                reg.sync_from_chain(&providers);

                // Bridge on-chain PoNW reputation into HealthScorer
                for cp in &providers {
                    hs.update_reputation_from_pown(&cp.provider_pk, cp.pown_score);
                }
                let gpu_listings = scanner.scan_gpu_listings().await;
                info!(count = gpu_listings.len(), "Initial GPU listing scan complete");
                cache_clone.update_gpu_listings(gpu_listings);

                let gpu_rentals = scanner.scan_gpu_rentals().await;
                info!(count = gpu_rentals.len(), "Initial GPU rental scan complete");
                cache_clone.update_gpu_rentals(gpu_rentals);
            });
        }

        Some(tokio::spawn(async move {
            let event_broadcaster = event_broadcaster.clone();
            let hs = health_scorer.clone();
            // Wait before starting periodic scans
            tokio::time::sleep(Duration::from_secs(poll_interval)).await;

            loop {
                info!("Starting periodic chain scan...");

                // Use std::panic::catch_unwind equivalent for async via tokio::spawn
                // The scan methods return Vec directly (handle errors internally),
                // but the entire task could panic from unexpected errors.
                // Wrap in a safe pattern: catch any panic at the task level.
                let scan_result = tokio::task::spawn({
                    let scanner = chain_scanner.clone();
                    async move { scanner.scan().await }
                })
                .await;

                match scan_result {
                    Ok(providers) => {
                        let count = providers.len();
                        if count == 0 && cache.is_populated() {
                            // Empty result while we had data before — possible scan failure
                            cache.record_scan_failure();
                            warn!("Chain scan returned 0 providers — serving stale data");
                        } else {
                            // Diff old vs new providers and publish events
                            let old_providers = cache.get_providers_or_empty();
                            events::diff_and_publish_events(&old_providers, &providers, &event_broadcaster);

                            cache.update(providers.clone());
                            registry.sync_from_chain(&providers);

                            // Bridge on-chain PoNW reputation into HealthScorer
                            for cp in &providers {
                                hs.update_reputation_from_pown(&cp.provider_pk, cp.pown_score);
                            }

                            info!(count, interval_secs = poll_interval, "Chain cache refreshed");
                        }
                    }
                    Err(e) => {
                        cache.record_scan_failure();
                        error!(error = %e, "Periodic chain scan task panicked — serving stale data");
                    }
                }

                // Also refresh GPU listings and rentals
                let gpu_result = tokio::task::spawn({
                    let scanner = chain_scanner.clone();
                    async move { scanner.scan_gpu_listings().await }
                })
                .await;

                match gpu_result {
                    Ok(gpu_listings) => {
                        let gpu_count = gpu_listings.len();
                        cache.update_gpu_listings(gpu_listings);
                        info!(count = gpu_count, "GPU listing cache refreshed");
                    }
                    Err(e) => {
                        warn!(error = %e, "GPU listing scan task panicked — serving stale data");
                    }
                }

                let rental_result = tokio::task::spawn({
                    let scanner = chain_scanner.clone();
                    async move { scanner.scan_gpu_rentals().await }
                })
                .await;

                match rental_result {
                    Ok(gpu_rentals) => {
                        let rental_count = gpu_rentals.len();
                        cache.update_gpu_rentals(gpu_rentals);
                        info!(count = rental_count, "GPU rental cache refreshed");
                    }
                    Err(e) => {
                        warn!(error = %e, "GPU rental scan task panicked — serving stale data");
                    }
                }

                // Log cache health status periodically
                info!(
                    cache_healthy = cache.is_healthy(),
                    cache_stale = cache.is_stale(),
                    cache_populated = cache.is_populated(),
                    cache_age_secs = cache.age().as_secs(),
                    stale_provider_count = cache.stale_provider_count(),
                    scan_failures = cache.scan_failure_count(),
                    "Chain cache health status"
                );

                tokio::time::sleep(Duration::from_secs(poll_interval)).await;
            }
        }))
    } else {
        info!("Chain-state provider discovery is disabled");
        None
    };

    // Spawn oracle rate refresh task (if configured with a pool NFT token ID)
    let oracle_handle = if let Some(ref pool_nft_id) = config.oracle.pool_nft_token_id {
        if let Some(ref scanner) = chain_scanner {
            let scanner = scanner.clone();
            let erg_usd_rate = state.erg_usd_rate.clone();
            let refresh_secs = config.oracle.refresh_secs;
            let pool_nft_id = pool_nft_id.clone();

            info!(
                pool_nft = %pool_nft_id,
                refresh_secs = refresh_secs,
                "Oracle ERG/USD rate refresh enabled"
            );

            // Perform initial fetch
            {
                let scanner = scanner.clone();
                let rate_lock = erg_usd_rate.clone();
                let nft = pool_nft_id.clone();
                tokio::spawn(async move {
                    match scanner.fetch_oracle_rate(&nft).await {
                        Ok(rate_opt) => {
                            if let Some(rate) = rate_opt {
                                info!(rate = rate, "Initial oracle ERG/USD rate fetched");
                                if let Ok(mut w) = rate_lock.write() {
                                    *w = Some(rate);
                                }
                            } else {
                                warn!("Oracle pool box not found during initial fetch");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to fetch initial oracle rate");
                        }
                    }
                });
            }

            Some(tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(refresh_secs)).await;
                loop {
                    match scanner.fetch_oracle_rate(&pool_nft_id).await {
                        Ok(rate_opt) => {
                            if let Some(rate) = rate_opt {
                                info!(rate = rate, interval_secs = refresh_secs, "Oracle ERG/USD rate refreshed");
                                if let Ok(mut w) = erg_usd_rate.write() {
                                    *w = Some(rate);
                                }
                            } else {
                                debug!("Oracle pool box not found during periodic refresh");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to refresh oracle rate");
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(refresh_secs)).await;
                }
            }))
        } else {
            warn!("Oracle pool_nft_token_id configured but chain scanning is disabled — oracle disabled");
            None
        }
    } else {
        info!("Oracle integration not configured (no pool_nft_token_id)");
        None
    };

    // Spawn rate limiter cleanup task (every 5 minutes)
    let rate_limit_cleanup_handle = if let Some(rl_state) = rate_limit_state.clone() {
        Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await;
                rl_state.cleanup().await;
            }
        }))
    } else {
        None
    };

    // Spawn free tier cleanup task (every 5 minutes)
    let free_tier_cleanup_handle = if let Some(ft_tracker) = free_tier_tracker.clone() {
        Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await;
                ft_tracker.cleanup();
            }
        }))
    } else {
        None
    };

    // Spawn demand tracker prune task (every 60 seconds)
    let demand_prune_handle = {
        let tracker = demand_tracker.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                tracker.prune();
            }
        })
    };

    // Spawn request dedup prune task (every 30 seconds)
    let dedup_prune_handle = {
        let dedup = state.request_dedup.clone();
        let dedup_window = config.dedup.window_secs;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(
                    (dedup_window / 2).max(10),
                ))
                .await;
                dedup.prune_expired();
                debug!(in_flight = dedup.in_flight_count(), "Dedup prune cycle");
            }
        })
    };

    // Spawn model registry stale entry prune task (every 5 minutes)
    let model_prune_handle = {
        let model_reg = model_registry.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await;
                // Prune models not seen in 10 minutes (3x typical health poll interval)
                model_reg.prune_stale_models(Duration::from_secs(600));
            }
        })
    };

    // Spawn SLA evaluation task (every 60 seconds)
    let _sla_eval_handle = {
        let sla_tracker = state.sla_tracker.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                sla_tracker.evaluate_all();
                debug!("SLA evaluation cycle complete");
            }
        })
    };

    // Spawn SLA data prune task (every 10 minutes)
    let _sla_prune_handle = {
        let sla_tracker = state.sla_tracker.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(600)).await;
                sla_tracker.prune();
            }
        })
    };

    // Spawn cache cleanup task (every 30 seconds)
    let cache_cleanup_handle = if config.cache.enabled {
        let cache = response_cache.clone();
        Some(cache.start_cleanup_task(30))
    } else {
        None
    };

    // Spawn multiplexer cleanup task (every 30 seconds)
    let _multiplex_cleanup_handle = {
        let mux = request_multiplexer.clone();
        Some(mux.start_cleanup_task(30))
    };

    // Initialize provider auto-registration service (if enabled)
    let auto_register_handle = if config.auto_register.enabled {
        info!(
            auto_register_enabled = true,
            relay_url = %config.auto_register.relay_url,
            check_interval_secs = config.auto_register.check_interval_secs,
            "Provider auto-registration enabled"
        );
        let ar_service = Arc::new(auto_register::AutoRegistrationService::new(
            config.auto_register.clone(),
        ));
        state.auto_register = Some(ar_service.clone());
        Some(ar_service.start())
    } else {
        info!("Provider auto-registration is disabled");
        None
    };

    // Initialize distributed cache synchronizer (if enabled)
    let cache_sync_handle = if config.cache_sync.enabled {
        info!(
            cache_sync_enabled = true,
            sync_interval_secs = config.cache_sync.sync_interval_secs,
            peers = config.cache_sync.peer_urls.len(),
            max_sync_entries = config.cache_sync.max_sync_entries,
            compression = config.cache_sync.compression,
            "Distributed cache synchronization enabled"
        );
        let node_id = {
            let encoded = hex::encode(&config.relay.listen_addr.as_bytes());
            format!("relay-{}", &encoded[..8])
        };
        let cache_sync = Arc::new(cache_sync::CacheSynchronizer::new(
            config.cache_sync.clone(),
            node_id,
        ));
        state.cache_synchronizer = Some(cache_sync.clone());
        Some(cache_sync.start_sync_task(http_client.clone()))
    } else {
        info!("Distributed cache synchronization is disabled");
        None
    };

    // Initialize multi-region router (if any regions configured)
    let region_health_handle = if !config.multi_region.regions.is_empty() {
        info!(
            multi_region_enabled = true,
            region_count = config.multi_region.regions.len(),
            strategy = %config.multi_region.routing_strategy,
            failover = config.multi_region.failover_enabled,
            "Multi-region routing enabled"
        );
        let router = Arc::new(multi_region::MultiRegionRouter::new(
            config.multi_region.clone(),
        ));
        state.multi_region_router = Some(router.clone());
        Some(router.start_health_check_task(http_client.clone()))
    } else {
        info!("Multi-region routing is disabled (no regions configured)");
        None
    };

    // Log WebSocket chat config
    if config.ws_chat.enabled {
        info!(
            ws_chat_enabled = true,
            ws_chat_max_connections = config.ws_chat.max_connections,
            "WebSocket chat transport enabled at /v1/chat/ws"
        );
    } else {
        info!("WebSocket chat transport is disabled");
    }

    // Log dedup config
    if config.dedup.enabled {
        info!(
            dedup_enabled = true,
            dedup_window_secs = config.dedup.window_secs,
            "Request deduplication enabled"
        );
    } else {
        info!("Request deduplication is disabled");
    }

    // Build CORS layer
    let cors = if config.relay.cors_origins == "*" {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<axum::http::HeaderValue> = config
            .relay
            .cors_origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    };

    // Build router
    let router = Router::new()
        // API versioning and documentation
        .route(
            "/api/versions",
            get(api_version::list_versions_handler),
        )
        .route(
            "/v1/openapi.json",
            get(openapi::openapi_spec_handler),
        )
        .route(
            "/v1/docs",
            get(openapi::docs_handler),
        )
        // OpenAI-compatible endpoints
        .route(
            "/v1/chat/completions",
            post(handlers::chat::chat_completions_handler),
        )
        .route(
            "/v1/embeddings",
            post(handlers::embeddings::embeddings_handler),
        )
        .route(
            "/v1/images/generations",
            post(handlers::images::images_generations_handler),
        )
        // Audio/Speech endpoints
        .route(
            "/v1/audio/speech",
            post(handlers::audio::speech_handler),
        )
        .route(
            "/v1/audio/transcriptions",
            post(handlers::audio::transcriptions_handler),
        )
        .route(
            "/v1/audio/translations",
            post(handlers::audio::translations_handler),
        )
        .route("/v1/models", get(handlers::models::list_models_handler))
        // Public leaderboard
        .route(
            "/v1/leaderboard",
            get(handlers::leaderboard::leaderboard_handler),
        )
        // Provider listing (chain + in-memory merge)
        .route(
            "/v1/providers",
            get(handlers::providers::list_providers_handler),
        )
        // Provider onboarding
        .route(
            "/v1/providers/onboard",
            post(handlers::onboarding::onboard_provider_handler),
        )
        .route(
            "/v1/providers/onboard/{provider_pk}",
            get(handlers::onboarding::onboarding_status_handler),
        )
        .route(
            "/v1/providers/onboard/{provider_pk}/test",
            post(handlers::onboarding::test_provider_handler),
        )
        // Provider deregistration
        .route(
            "/v1/providers/{provider_pk}",
            delete(handlers::onboarding::deregister_provider_handler),
        )
        // Model listing and detail
        .route("/v1/models", get(handlers::models::list_models_handler))
        .route(
            "/v1/models/{model_id}",
            get(handlers::model_detail::get_model_detail_handler),
        )
        // User balance check
        .route(
            "/v1/balance/{user_pk}",
            get(handlers::balance::balance_handler),
        )
        // GPU Bazar endpoints
        .route(
            "/v1/gpu/listings",
            get(handlers::gpu::list_gpu_listings_handler),
        )
        .route(
            "/v1/gpu/listings/{listing_id}",
            get(handlers::gpu::get_gpu_listing_handler),
        )
        .route(
            "/v1/gpu/rent",
            post(handlers::gpu::rent_gpu_handler),
        )
        .route(
            "/v1/gpu/rentals/{renter_pk}",
            get(handlers::gpu::get_gpu_rentals_handler),
        )
        .route(
            "/v1/gpu/pricing",
            get(handlers::gpu::get_gpu_pricing_handler),
        )
        .route(
            "/v1/gpu/rate",
            post(handlers::gpu::rate_gpu_handler),
        )
        .route(
            "/v1/gpu/reputation/{public_key}",
            get(handlers::gpu::get_gpu_reputation_handler),
        )
        // Auth status
        .route(
            "/v1/auth/status",
            get(handlers::auth::auth_status_handler),
        )
        // Incentive system: rarity bonuses
        .route(
            "/v1/incentive/status",
            get(handlers::incentive::incentive_status_handler),
        )
        .route(
            "/v1/incentive/models",
            get(handlers::incentive::incentive_models_handler),
        )
        .route(
            "/v1/incentive/models/{model}",
            get(handlers::incentive::incentive_model_detail_handler),
        )
        // Cross-chain payment bridge
        .route(
            "/v1/bridge/status",
            get(handlers::bridge::bridge_status_handler),
        )
        .route(
            "/v1/bridge/invoices",
            get(handlers::bridge::list_invoices_handler),
        )
        .route(
            "/v1/bridge/invoice/{id}",
            get(handlers::bridge::get_invoice_status_handler),
        )
        .route(
            "/v1/bridge/create-invoice",
            post(handlers::bridge::create_invoice_handler),
        )
        .route(
            "/v1/bridge/confirm",
            post(handlers::bridge::confirm_payment_handler),
        )
        .route(
            "/v1/bridge/refund",
            post(handlers::bridge::refund_invoice_handler),
        )
        // File upload management
        .route(
            "/v1/files",
            post(handlers::upload::upload_file_handler),
        )
        .route(
            "/v1/files",
            get(handlers::upload::list_files_handler),
        )
        .route(
            "/v1/files/{file_id}",
            get(handlers::upload::get_file_handler),
        )
        .route(
            "/v1/files/{file_id}",
            delete(handlers::upload::delete_file_handler),
        )
        .route(
            "/v1/files/{file_id}/content",
            get(handlers::upload::get_file_content_handler),
        )
        // Tier info endpoint
        .route(
            "/v1/tier",
            get(rate_limit_tiers::tier_info_handler),
        );

    // SSE events endpoint (conditionally mounted)
    let router = if config.events.enabled {
        router.route("/v1/events", get(events::events_handler))
    } else {
        router
    };

    // WebSocket real-time provider status endpoint
    let router = router.route("/ws/status", get(ws::ws_handler));

    // WebSocket chat transport endpoint (conditionally mounted)
    let router = if config.ws_chat.enabled {
        router.route("/v1/chat/ws", get(ws::ws_chat_handler))
    } else {
        router
    };

    // Gossip consensus endpoints
    let router = router.merge(gossip::build_gossip_router());

    // gRPC-like transport endpoints (protobuf over HTTP)
    let router = router.merge(grpc::build_grpc_router());

    // Adaptive retry API endpoints
    let router = router.merge(adaptive_retry::build_retry_router());

    // Admin API (conditionally mounted when enabled and api_key is configured)
    let router = if config.admin.enabled && !config.admin.api_key.is_empty() {
        info!(
            "Admin API enabled at /admin/* (key required)"
        );
        router
            .merge(admin::build_admin_router(state.clone()))
            .merge(webhook::build_webhook_router(state.clone()))
            .merge(audit::build_audit_router(state.clone()))
            .merge(api_key_manager::build_api_key_router(state.clone()))
            .merge(usage_analytics::build_analytics_router(state.clone()))
            .merge(auto_register::build_auto_register_router())
            .merge(sla::build_sla_router(state.clone()))
            .merge(semantic_cache::build_semantic_cache_router())
            .merge(reputation_bonding::build_bonding_router(state.clone()))
            .merge(staking_rewards::build_staking_router(state.clone()))
    } else {
        router
    };

    // Cache sync and multi-region endpoints (always available)
    let router = router
        .merge(cache_sync::build_cache_sync_router(state.clone()))
        .merge(multi_region::build_region_router(state.clone()))
        .merge(oracle_aggregator::build_oracle_router(state.clone()))
        .merge(chain_adapters::build_chain_router(state.clone()))
        .merge(encrypted_inference::build_encrypted_inference_router(state.clone()))
        .merge(quantum_crypto::build_quantum_crypto_router(state.clone()))
        .merge(zkp_verification::build_router())
        .merge(homomorphic_compute::build_router(state.clone()))
        .merge(cross_provider_orchestration::build_router(state.clone()))
        .merge(speculative_decoding::build_router(state.clone()))
        .merge(request_fusion::build_router(state.clone()))
        .merge(continuous_batching::build_router(state.clone()))
        .merge(token_streaming::build_router(state.clone()))
        .merge(cost_estimator::build_router(state.clone()))
        .merge(scheduling_optimizer::build_router(state.clone()))
        .merge(dynamic_pricing::build_router(state.clone()))
        .merge(capability_negotiation::build_router(state.clone()))
        .merge(protocol_versioning::build_router(state.clone()))
        .merge(connection_pool_v2::build_router(state.clone()))
        .merge(request_dedup_v2::build_router(state.clone()))
        .merge(response_cache_headers::build_router(state.clone()))
        .merge(content_negotiation::build_router(state.clone()))
        .merge(rate_limiter_v2::build_router(state.clone()))
        .merge(middleware_chain::build_router(state.clone()))
        .merge(cors_v2::build_router(state.clone()))
        .merge(websocket_v2::build_ws_v2_router())
        .merge(health_monitor_v2::build_health_v2_router())
        .merge(api_gateway::build_gateway_router())
        .merge(babel_fee_integration::build_router(state.clone()))
        .merge(request_coalescing::build_router(state.clone()))
        .merge(protocol_adapter::build_router(state.clone()))
        .merge(ensemble_router::build_router(state.clone()));

    // GraphQL API endpoint
    let graphql_schema = Arc::new(graphql::build_schema(Arc::new(state.clone())));
    info!("GraphQL API enabled at /api/graphql");
    let router = router.merge(graphql::build_graphql_router(graphql_schema));

    let app = router
        // Health check
        .route("/health", get(health_handler))
        .route("/ready", get(readiness_handler))
        .merge(health::build_health_router())
        // Cache management endpoints
        .route("/v1/cache", delete(cache_invalidate_all_handler))
        .route("/v1/cache/{prefix}", delete(cache_invalidate_prefix_handler))
        .route("/v1/cache/stats", get(cache_stats_handler))
        // Circuit breaker, load shedding, degradation status endpoints
        .route("/v1/circuit-breakers", get(circuit_breakers_handler))
        .route("/v1/load-shed/stats", get(load_shed_stats_handler))
        .route("/v1/degradation", get(degradation_get_handler))
        .route("/v1/degradation", post(degradation_post_handler))
        // Coalesce and stream buffer stats/cancel endpoints
        .route("/v1/coalesce/stats", get(coalesce_stats_handler))
        .route("/v1/coalesce/{hash}", delete(coalesce_cancel_handler))
        .route("/v1/stream-buffers/stats", get(stream_buffer_stats_handler))
        // Dedup stats endpoint
        .route("/v1/dedup/stats", get(dedup_stats_handler))
        // WebSocket pool stats endpoint
        .route("/v1/ws/pool/stats", get(ws_pool_stats_handler))
        // Priority queue stats endpoint
        .route("/v1/priority/stats", get(priority_stats_handler))
        // Cost estimation endpoints
        .route("/v1/cost/estimate", get(handlers::cost::cost_estimate_handler))
        .route("/v1/cost/estimate-batch", post(handlers::cost::cost_estimate_batch_handler))
        // Adaptive routing management endpoints
        .route("/v1/routing/stats", get(handlers::routing::routing_stats_handler))
        .route("/v1/routing/health", get(handlers::routing::routing_health_handler))
        .route("/v1/routing/geo", get(handlers::routing::routing_geo_handler))
        .route("/v1/routing/strategy", put(handlers::routing::set_routing_strategy_handler))
        // OpenTelemetry tracing status
        .route("/v1/tracing/status", get(tracing_middleware::tracing_status_handler))
        // Cache middleware (outermost — before other middleware so it runs first on request)
        .layer(axum_middleware::from_fn_with_state(
            cache_middleware::CacheLayerState::new(
                response_cache.clone(),
                &cache::CacheConfig {
                    enabled: config.cache.enabled,
                    max_entries: config.cache.max_entries,
                    default_ttl_secs: config.cache.default_ttl_secs,
                    model_list_ttl_secs: config.cache.model_list_ttl_secs,
                    provider_list_ttl_secs: config.cache.provider_list_ttl_secs,
                    health_ttl_secs: config.cache.health_ttl_secs,
                    max_entry_size_bytes: config.cache.max_entry_size_bytes,
                },
            ),
            cache_middleware::cache_middleware,
        ))
        // Schema validation middleware (innermost — validates request bodies before handlers)
        .layer(axum_middleware::from_fn(schemas::schema_validation_middleware))
        // Request ID middleware (innermost — runs first on request, last on response)
        .layer(axum_middleware::from_fn(middleware::request_id_middleware))
        // API versioning middleware (adds X-API-Version, Deprecation, Sunset headers)
        .layer(axum_middleware::from_fn(api_version::version_middleware))
        // Rate limiting middleware (must be applied before .with_state)
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // OpenTelemetry HTTP span middleware (creates distributed tracing spans)
        .layer(axum_middleware::from_fn(tracing_middleware::otel_http_middleware))
        .layer(axum_middleware::from_fn(middleware::security_headers_middleware))
        .with_state(state);

    // Start server
    let addr: std::net::SocketAddr = config.relay.listen_addr.parse()?;
    info!(addr = %addr, "Starting Xergon relay server");

    // Initialize OpenTelemetry (if enabled via config or env var)
    let _telemetry_guard = telemetry::init_telemetry(
        &config.telemetry.service_name,
        &config.telemetry.otlp_endpoint,
        config.telemetry.enabled,
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    health_handle.abort();
    if let Some(handle) = chain_handle {
        handle.abort();
    }
    if let Some(handle) = rate_limit_cleanup_handle {
        handle.abort();
    }
    if let Some(handle) = free_tier_cleanup_handle {
        handle.abort();
    }
    if let Some(handle) = oracle_handle {
        handle.abort();
    }
    if let Some(handle) = gossip_handle {
        handle.abort();
    }
    if let Some(handle) = auto_register_handle {
        handle.abort();
    }
    if let Some(handle) = cache_sync_handle {
        handle.abort();
    }
    if let Some(handle) = region_health_handle {
        handle.abort();
    }
    dedup_prune_handle.abort();
    demand_prune_handle.abort();

    // Shut down WebSocket connection pool
    ws_pool_maintenance_handle.abort();
    ws_pool.close_all().await;
    if let Some(handle) = cache_cleanup_handle {
        handle.abort();
    }

    info!("Xergon Relay stopped");
    Ok(())
}

/// GET /health - Basic liveness
async fn health_handler() -> &'static str {
    "ok"
}

/// GET /ready - Readiness check (at least 1 healthy provider)
async fn readiness_handler(State(state): State<AppState>) -> impl IntoResponse {
    let healthy = state.provider_registry.healthy_provider_count();
    if healthy > 0 {
        (axum::http::StatusCode::OK, "ready")
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "no healthy providers",
        )
    }
}

/// DELETE /v1/cache - Clear all cache entries
async fn cache_invalidate_all_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    state.response_cache.clear();
    tracing::info!("Cache cleared (all entries)");
    (axum::http::StatusCode::OK, "cache cleared")
}

/// DELETE /v1/cache/:prefix - Clear cache entries matching prefix
async fn cache_invalidate_prefix_handler(
    State(state): State<AppState>,
    axum::extract::Path(prefix): axum::extract::Path<String>,
) -> impl IntoResponse {
    let cache_prefix = format!("GET::_:{}", prefix);
    state.response_cache.invalidate_prefix(&cache_prefix);
    tracing::info!(prefix = %prefix, "Cache invalidated by prefix");
    (axum::http::StatusCode::OK, "cache invalidated")
}

/// GET /v1/cache/stats - Get cache statistics
async fn cache_stats_handler(
    State(state): State<AppState>,
) -> axum::Json<cache::CacheStats> {
    axum::Json(state.response_cache.stats())
}

/// GET /v1/circuit-breakers - List all circuit breaker states
async fn circuit_breakers_handler(
    State(state): State<AppState>,
) -> axum::Json<serde_json::Value> {
    let all_states = state.circuit_breaker.get_all_states();
    let metrics = state.circuit_breaker.metrics();

    let providers: Vec<serde_json::Value> = all_states
        .into_iter()
        .map(|(name, cb_state, failures)| {
            serde_json::json!({
                "provider": name,
                "state": cb_state,
                "consecutive_failures": failures,
            })
        })
        .collect();

    axum::Json(serde_json::json!({
        "metrics": metrics,
        "providers": providers,
    }))
}

/// GET /v1/load-shed/stats - Load shedding statistics
async fn load_shed_stats_handler(
    State(state): State<AppState>,
) -> axum::Json<load_shed::LoadShedStats> {
    axum::Json(state.load_shedder.get_stats())
}

/// GET /v1/degradation - Current degradation level
async fn degradation_get_handler(
    State(state): State<AppState>,
) -> axum::Json<serde_json::Value> {
    let level = state.degradation_manager.current_level();
    let config = state.degradation_manager.get_degraded_config();

    axum::Json(serde_json::json!({
        "level": level,
        "config": config,
    }))
}

/// POST /v1/degradation - Manually set degradation level (admin)
async fn degradation_post_handler(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let level_str = body.get("level").and_then(|v| v.as_str()).unwrap_or("full");
    let level = degradation::DegradationLevel::from_str_lossy(level_str);
    state.degradation_manager.set_level(level);

    let config = state.degradation_manager.get_degraded_config();
    (axum::http::StatusCode::OK, axum::Json(serde_json::json!({
        "level": level,
        "config": config,
    })))
}

/// GET /v1/coalesce/stats - Coalescing statistics
async fn coalesce_stats_handler(
    State(state): State<AppState>,
) -> axum::Json<serde_json::Value> {
    let stats = state.request_multiplexer.coalesce_stats();
    axum::Json(serde_json::json!({
        "active_batches": stats.active_batches,
        "total_coalesced": stats.total_coalesced.load(std::sync::atomic::Ordering::Relaxed),
        "total_batches": stats.total_batches.load(std::sync::atomic::Ordering::Relaxed),
        "total_completed": stats.total_completed.load(std::sync::atomic::Ordering::Relaxed),
        "total_cancelled": stats.total_cancelled.load(std::sync::atomic::Ordering::Relaxed),
        "avg_batch_size": stats.avg_batch_size(),
    }))
}

/// DELETE /v1/coalesce/:hash - Cancel a coalesced batch
async fn coalesce_cancel_handler(
    State(state): State<AppState>,
    axum::extract::Path(hash): axum::extract::Path<String>,
) -> impl IntoResponse {
    let cancelled = state.request_multiplexer.cancel_batch(&hash);
    if cancelled {
        (axum::http::StatusCode::OK, format!("coalesce batch {} cancelled", hash))
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("coalesce batch {} not found", hash),
        )
    }
}

/// GET /v1/stream-buffers/stats - Stream buffer statistics
async fn stream_buffer_stats_handler(
    State(state): State<AppState>,
) -> axum::Json<serde_json::Value> {
    let stats = state.request_multiplexer.buffer_stats();
    axum::Json(serde_json::json!({
        "active_buffers": stats.active_buffers,
        "total_created": stats.total_created,
        "total_cleaned": stats.total_cleaned,
        "total_subscribers": stats.total_subscribers,
        "total_chunks": stats.total_chunks,
        "total_bytes": stats.total_bytes,
        "buffers": stats.buffers,
    }))
}

/// GET /v1/dedup/stats - Request deduplication statistics
async fn dedup_stats_handler(
    State(state): State<AppState>,
) -> axum::Json<serde_json::Value> {
    let stats = state.request_dedup.get_stats();
    axum::Json(serde_json::json!({
        "total_requests": stats.total_requests,
        "dedup_hits": stats.dedup_hits,
        "dedup_misses": stats.dedup_misses,
        "active_cached_responses": stats.active_cached_responses,
        "cache_size_bytes": stats.cache_size_bytes,
        "enabled": stats.enabled,
        "window_secs": stats.window_secs,
        "dedup_ttl_secs": stats.dedup_ttl_secs,
    }))
}

/// GET /v1/priority/stats - Priority queue statistics
async fn priority_stats_handler(
    State(state): State<AppState>,
) -> axum::Json<serde_json::Value> {
    let stats = state.priority_queue.get_stats();
    axum::Json(serde_json::json!({
        "enabled": stats.enabled,
        "total_queued": stats.total_queued,
        "max_total": stats.max_total,
        "max_per_level": stats.max_per_level,
        "total_enqueued": stats.total_enqueued,
        "total_dequeued": stats.total_dequeued,
        "total_rejected": stats.total_rejected,
        "total_expired": stats.total_expired,
        "queue_depths": stats.queue_depths,
    }))
}

/// GET /v1/ws/pool/stats - WebSocket connection pool statistics
async fn ws_pool_stats_handler(
    State(state): State<AppState>,
) -> axum::Json<serde_json::Value> {
    let stats = state.ws_pool.stats();
    axum::Json(serde_json::json!({
        "enabled": stats.enabled,
        "total_connections": stats.total_connections,
        "provider_count": stats.provider_count,
        "hits": stats.hits,
        "misses": stats.misses,
        "hit_rate": stats.hit_rate,
        "total_created": stats.total_created,
        "total_closed": stats.total_closed,
        "per_provider": stats.per_provider,
    }))
}

/// Sync models from a provider's health status into the model registry.
/// Called after each health poll for a provider.
fn sync_models_from_provider(
    registry: &std::sync::Arc<provider::ProviderRegistry>,
    model_reg: &std::sync::Arc<model_registry::ModelRegistry>,
    endpoint: &str,
) {
    let provider = match registry.providers.get(endpoint) {
        Some(p) => p.value().clone(),
        None => return,
    };

    // Get provider public key
    let provider_pk = provider
        .status
        .as_ref()
        .and_then(|s| s.provider.as_ref())
        .map(|p| p.id.clone())
        .unwrap_or_default();

    if provider_pk.is_empty() {
        return;
    }

    // Collect models from served_models (chain-synced) and pown_status (live)
    let mut models: Vec<model_registry::SyncModelInfo> = Vec::new();

    // Models from chain sync (served_models)
    for model_id in &provider.served_models {
        let price = provider
            .model_pricing
            .get(&model_id.to_lowercase())
            .copied()
            .unwrap_or(0);
        models.push(model_registry::SyncModelInfo {
            model_id: model_id.clone(),
            context_length: 4096, // default, enriched by chain data
            pricing_nanoerg_per_million_tokens: price,
        });
    }

    // Model from live pown_status (may overlap with served_models, deduped by sync)
    if let Some(ref pown) = provider
        .status
        .as_ref()
        .and_then(|s| s.pown_status.as_ref())
    {
        if pown.ai_enabled && !pown.ai_model.is_empty() {
            let model_lower = pown.ai_model.to_lowercase();
            // Only add if not already in models (avoid double-counting)
            if !models.iter().any(|m| m.model_id == model_lower) {
                models.push(model_registry::SyncModelInfo {
                    model_id: pown.ai_model.clone(),
                    context_length: 4096,
                    pricing_nanoerg_per_million_tokens: 0,
                });
            }
        }
    }

    if !models.is_empty() {
        model_reg.sync_from_provider(&provider_pk, endpoint, models);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received (SIGINT or SIGTERM)");
}
