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

mod auth;
mod balance;
mod chain;
mod chain_cache;
mod config;
mod demand;
mod events;
mod free_tier;
mod gossip;
mod handlers;
mod health;
mod metrics;
mod middleware;
mod provider;
mod proxy;
mod rate_limit;
mod util;
mod ws;

#[cfg(test)]
mod e2e_tests;

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
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

    let state = AppState {
        config: config.clone(),
        provider_registry: provider_registry.clone(),
        http_client,
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
    };

    // Spawn provider health polling loop
    let health_handle = {
        let registry = provider_registry.clone();
        let config_ref = config.clone();
        let broadcaster = event_broadcaster.clone();
        let ws_broadcaster = ws_broadcaster.clone();
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
                        tokio::spawn(async move {
                            registry.poll_provider(&ep).await;
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

                // Also scan for GPU listings and rentals
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
        // OpenAI-compatible endpoints
        .route(
            "/v1/chat/completions",
            post(handlers::chat::chat_completions_handler),
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
        );

    // SSE events endpoint (conditionally mounted)
    let router = if config.events.enabled {
        router.route("/v1/events", get(events::events_handler))
    } else {
        router
    };

    // WebSocket real-time provider status endpoint
    let router = router.route("/ws/status", get(ws::ws_handler));

    // Gossip consensus endpoints
    let router = router.merge(gossip::build_gossip_router());

    let app = router
        // Health check
        .route("/health", get(health_handler))
        .route("/ready", get(readiness_handler))
        .merge(health::build_health_router())
        // Request ID middleware (innermost — runs first on request, last on response)
        .layer(axum_middleware::from_fn(middleware::request_id_middleware))
        // Rate limiting middleware (must be applied before .with_state)
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(axum_middleware::from_fn(middleware::security_headers_middleware))
        .with_state(state);

    // Start server
    let addr: std::net::SocketAddr = config.relay.listen_addr.parse()?;
    info!(addr = %addr, "Starting Xergon relay server");

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
    demand_prune_handle.abort();

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
