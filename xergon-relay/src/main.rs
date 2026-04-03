//! Xergon Relay — Marketplace backend
//!
//! Sits between frontend users and Xergon providers.
//! Features:
//! - OpenAI-compatible API (POST /v1/chat/completions, GET /v1/models)
//! - Smart routing (latency + PoNW + load)
//! - Fallback chain on provider failure
//! - SSE streaming for responses
//! - Anonymous rate limiting (10 req/day by IP)
//! - Provider health polling
//! - User authentication (email/password + JWT)
//! - Credit system with Stripe integration

mod auth;
mod config;
mod credits;
mod db;
mod handlers;
mod provider;
mod proxy;
mod rate_limit;
mod registration;
mod usage_report;
mod util;

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

use axum::{extract::State, response::IntoResponse, routing::{delete, get, post, put}, Router};
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

    let config = Arc::new(config::RelayConfig::load().context("Failed to load configuration")?);

    info!(
        listen_addr = %config.relay.listen_addr,
        known_providers = config.providers.known_endpoints.len(),
        health_poll_secs = config.relay.health_poll_interval_secs,
        anon_rate_limit = config.relay.anonymous_rate_limit_per_day,
        usage_report_secs = config.relay.usage_report_interval_secs,
        db_path = %config.database.path,
        stripe_configured = !config.stripe.secret_key.is_empty(),
        "Configuration loaded"
    );

    // Open database
    let db = Arc::new(
        db::Db::open(std::path::Path::new(&config.database.path))
            .context("Failed to open database")?,
    );

    // Build shared state
    let provider_registry = Arc::new(provider::ProviderRegistry::new(config.clone()));
    let rate_limiter = Arc::new(rate_limit::RateLimiter::new(
        config.relay.anonymous_rate_limit_per_day,
    ));
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.relay.provider_timeout_secs))
        .build()
        .context("Failed to build HTTP client")?;

    // Build provider directory for dynamic registration
    let provider_directory = Arc::new(registration::ProviderDirectory::new(
        config.providers.registration_token.clone(),
    ));

    let state = AppState {
        config: config.clone(),
        provider_registry: provider_registry.clone(),
        rate_limiter,
        http_client,
        usage_store: Arc::new(dashmap::DashMap::new()),
        db: db.clone(),
        provider_directory: provider_directory.clone(),
    };

    // Spawn provider directory expiry loop (removes stale providers)
    provider_directory.clone().spawn_expiry_loop();

    // Spawn provider health polling loop
    let health_handle = {
        let registry = provider_registry.clone();
        let directory = provider_directory.clone();
        let config_ref = config.clone(); // Clone Arc for use in closure
        tokio::spawn(async move {
            loop {
                // Sync registered providers into the health-polling registry
                for endpoint in directory.active_endpoints() {
                    registry.add_provider(endpoint);
                }

                // Run one health poll cycle
                let endpoints: Vec<String> = registry
                    .providers
                    .iter()
                    .map(|r| r.key().clone())
                    .collect();

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

                let healthy = registry.healthy_provider_count();
                let total = registry.providers.len();
                info!(healthy, total, "Health poll cycle complete");

                tokio::time::sleep(Duration::from_secs(config_ref.relay.health_poll_interval_secs)).await;
            }
        })
    };

    // Spawn periodic rate limiter cleanup (every 6 hours)
    let cleanup_handle = {
        let limiter = state.rate_limiter.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
                limiter.cleanup();
            }
        })
    };

    // Spawn auto-replenish background loop (every 5 minutes)
    let replenish_handle = {
        let db = state.db.clone();
        let packs = credits::default_credit_packs();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5 * 60)).await;
                match db.get_users_needing_replenish() {
                    Ok(users) => {
                        for (user, balance) in &users {
                            if let Some(ref pack_id) = user.replenish_pack_id {
                                let total = packs.iter().find(|p| &p.id == pack_id).map(|p| {
                                    p.amount_usd + p.bonus_credits_usd
                                });
                                if let Some(total_usd) = total {
                                    let tx_id = uuid::Uuid::new_v4().to_string();
                                    match db.add_credits(
                                        &tx_id,
                                        &user.id,
                                        total_usd,
                                        "auto_replenish",
                                        &format!("Auto-replenish: {} (balance was ${:.2})", pack_id, balance),
                                        None,
                                    ) {
                                        Ok(_) => {
                                            // Update last_replenish_at idempotency guard
                                            if let Err(e) = db.update_last_replenish_at(&user.id) {
                                                tracing::warn!(
                                                    user_id = %user.id,
                                                    error = %e,
                                                    "Failed to update last_replenish_at"
                                                );
                                            }
                                            tracing::info!(
                                                user_id = %user.id,
                                                pack_id = %pack_id,
                                                amount = total_usd,
                                                "Auto-replenish credits added"
                                            );
                                        }
                                        Err(e) => tracing::warn!(
                                            user_id = %user.id,
                                            error = %e,
                                            "Auto-replenish failed"
                                        ),
                                    }
                                } else {
                                    tracing::warn!(
                                        user_id = %user.id,
                                        pack_id = %pack_id,
                                        "Auto-replenish: pack_id not found in packs list"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "Auto-replenish: failed to query users"),
                }
            }
        })
    };

    // Spawn usage reporting background loop (reports aggregated usage to provider agents)
    let usage_report_handle = {
        let usage_store = state.usage_store.clone();
        let provider_directory = provider_directory.clone();
        let http_client = state.http_client.clone();
        let config = config.clone();
        tokio::spawn(async move {
            usage_report::run_usage_report_loop(
                usage_store,
                provider_directory,
                http_client,
                config,
            )
            .await;
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
    let app = Router::new()
        // Provider registration endpoints (authenticated via X-Provider-Token)
        .merge(registration::build_router())
        // Auth endpoints (public)
        .route("/v1/auth/signup", post(auth::signup_handler))
        .route("/v1/auth/login", post(auth::login_handler))
        .route("/v1/auth/me", get(auth::me_handler))
        .route("/v1/auth/forgot-password", post(auth::forgot_password_handler))
        .route("/v1/auth/reset-password", post(auth::reset_password_handler))
        .route("/v1/auth/profile", put(auth::update_profile_handler))
        .route("/v1/auth/password", put(auth::change_password_handler))
        .route("/v1/auth/wallet", put(auth::update_wallet_handler))
        // Credits endpoints (authenticated)
        .route("/v1/credits/balance", get(credits::get_balance_handler))
        .route("/v1/credits/transactions", get(credits::get_transactions_handler))
        .route("/v1/credits/packs", get(credits::get_packs_handler))
        .route("/v1/credits/purchase", post(credits::purchase_handler))
        .route("/v1/credits/auto-replenish", get(credits::get_auto_replenish_handler))
        .route("/v1/credits/auto-replenish", put(credits::update_auto_replenish_handler))
        // Usage analytics endpoints (authenticated)
        .route("/v1/user/usage/stats", get(handlers::usage::usage_stats_handler))
        .route("/v1/user/usage/history", get(handlers::usage::usage_history_handler))
        .route("/v1/user/api-keys", post(handlers::api_keys::create_api_key_handler))
        .route("/v1/user/api-keys", get(handlers::api_keys::list_api_keys_handler))
        .route("/v1/user/api-keys/{id}", delete(handlers::api_keys::revoke_api_key_handler))
        // Stripe webhook (public — verified via signature)
        .route("/v1/webhooks/stripe", post(credits::stripe_webhook_handler))
        // Simple inference API (marketplace frontend compat)
        .route("/v1/inference", post(handlers::inference::inference_handler))
        .route("/v1/inference/stream", post(handlers::inference::inference_stream_handler))
        // OpenAI-compatible endpoints
        .route("/v1/chat/completions", post(handlers::chat::chat_completions_handler))
        .route("/v1/models", get(handlers::models::list_models_handler))
        // Health check
        .route("/health", get(health_handler))
        .route("/ready", get(readiness_handler))
        // Public leaderboard
        .route("/v1/leaderboard", get(handlers::leaderboard::leaderboard_handler))
        // Admin API endpoints (authenticated via X-Admin-Token header)
        .route("/v1/admin/users", get(handlers::admin::list_users_handler))
        .route("/v1/admin/users/{id}/tier", put(handlers::admin::update_user_tier_handler))
        .route("/v1/admin/users/{id}/credits", put(handlers::admin::adjust_user_credits_handler))
        .route("/v1/admin/providers", get(handlers::admin::list_providers_handler))
        .route("/v1/admin/stats", get(handlers::admin::get_stats_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr: std::net::SocketAddr = config.relay.listen_addr.parse()?;
    info!(addr = %addr, "Starting Xergon relay server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    health_handle.abort();
    cleanup_handle.abort();
    replenish_handle.abort();
    usage_report_handle.abort();

    info!("Xergon Relay stopped");
    Ok(())
}

/// GET /health — Basic liveness
async fn health_handler() -> &'static str {
    "ok"
}

/// GET /ready — Readiness check (at least 1 healthy provider)
async fn readiness_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let healthy = state.provider_registry.healthy_provider_count();
    if healthy > 0 {
        (axum::http::StatusCode::OK, "ready")
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, "no healthy providers")
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl-C handler");
    info!("Shutdown signal received");
}
