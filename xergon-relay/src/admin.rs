//! Admin API for the Xergon relay
//!
//! Provides endpoints for provider management, system stats, cache control,
//! and relay configuration. All endpoints require `X-Admin-Key` header
//! authentication matching the configured admin API key.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tracing::{info, warn};

use crate::degradation::DegradationLevel;
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Auth helper
// ---------------------------------------------------------------------------

/// Verify the X-Admin-Key header from the request against the configured key.
fn verify_admin_key(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected_key = &state.config.admin.api_key;
    let provided = headers
        .get("x-admin-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided.is_empty() || provided != expected_key {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CacheInvalidateRequest {
    pub prefix: String,
}

#[derive(Debug, Deserialize)]
pub struct DegradationSetRequest {
    pub level: DegradationLevel,
}

#[derive(Debug, Deserialize)]
pub struct ProviderPatchRequest {
    pub region: Option<String>,
    pub labels: Option<std::collections::HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Handler helpers
// ---------------------------------------------------------------------------

fn admin_error(msg: &str, status: StatusCode) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": msg })),
    ).into_response()
}

fn admin_ok(value: serde_json::Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

fn admin_no_content() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------------------------------------------------------
// Provider detail serialization
// ---------------------------------------------------------------------------

fn provider_to_json(p: &crate::provider::Provider) -> serde_json::Value {
    serde_json::json!({
        "endpoint": p.endpoint,
        "healthy": p.is_healthy,
        "suspended": p.suspended.load(Ordering::Relaxed),
        "draining": p.draining.load(Ordering::Relaxed),
        "latency_ms": p.latency_ms,
        "active_requests": p.active_requests.load(Ordering::Relaxed),
        "consecutive_failures": p.consecutive_failures,
        "circuit_state": p.circuit_state.to_string(),
        "from_chain": p.from_chain,
        "region": p.region,
        "total_requests": p.total_requests,
        "failed_requests": p.failed_requests,
        "success_rate": format!("{:.4}", p.success_rate()),
        "pown_score": p.pown_score,
        "served_models": p.served_models,
        "last_health_check_secs_ago": p.last_health_check.elapsed().as_secs(),
        "last_healthy_at": p.last_healthy_at.to_rfc3339(),
        "provider_id": p.status.as_ref()
            .and_then(|s| s.provider.as_ref())
            .map(|p| p.id.clone()),
        "provider_name": p.status.as_ref()
            .and_then(|s| s.provider.as_ref())
            .map(|p| p.name.clone()),
    })
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /admin/providers -- list all providers with full details
async fn list_providers_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let providers: Vec<serde_json::Value> = state
        .provider_registry
        .providers
        .iter()
        .map(|r| provider_to_json(r.value()))
        .collect();

    admin_ok(serde_json::json!({
        "providers": providers,
        "total": providers.len(),
    }))
}

/// GET /admin/providers/:id -- get single provider details
async fn get_provider_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    // Look up by endpoint or by provider ID from status
    let provider = state
        .provider_registry
        .providers
        .iter()
        .find(|r| {
            let p = r.value();
            p.endpoint == id
                || p.status
                    .as_ref()
                    .and_then(|s| s.provider.as_ref())
                    .map(|prov| prov.id == id)
                    .unwrap_or(false)
        })
        .map(|r| r.value().clone());

    match provider {
        Some(p) => admin_ok(provider_to_json(&p)),
        None => admin_error("Provider not found", StatusCode::NOT_FOUND),
    }
}

/// POST /admin/providers/:id/suspend -- suspend a provider
async fn suspend_provider_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let found = state
        .provider_registry
        .providers
        .iter()
        .find(|r| {
            let p = r.value();
            p.endpoint == id
                || p.status
                    .as_ref()
                    .and_then(|s| s.provider.as_ref())
                    .map(|prov| prov.id == id)
                    .unwrap_or(false)
        });

    match found {
        Some(entry) => {
            entry.value().suspended.store(true, Ordering::Relaxed);
            let ep = entry.value().endpoint.clone();
            info!(endpoint = %ep, "Provider suspended by admin");
            admin_ok(serde_json::json!({
                "endpoint": ep,
                "status": "suspended",
            }))
        }
        None => admin_error("Provider not found", StatusCode::NOT_FOUND),
    }
}

/// POST /admin/providers/:id/resume -- resume a suspended provider
async fn resume_provider_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let found = state
        .provider_registry
        .providers
        .iter()
        .find(|r| {
            let p = r.value();
            p.endpoint == id
                || p.status
                    .as_ref()
                    .and_then(|s| s.provider.as_ref())
                    .map(|prov| prov.id == id)
                    .unwrap_or(false)
        });

    match found {
        Some(entry) => {
            entry.value().suspended.store(false, Ordering::Relaxed);
            entry.value().draining.store(false, Ordering::Relaxed);
            let ep = entry.value().endpoint.clone();
            info!(endpoint = %ep, "Provider resumed by admin");
            admin_ok(serde_json::json!({
                "endpoint": ep,
                "status": "active",
            }))
        }
        None => admin_error("Provider not found", StatusCode::NOT_FOUND),
    }
}

/// DELETE /admin/providers/:id -- remove a provider entirely
async fn remove_provider_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    // Resolve endpoint from ID if needed
    let endpoint = state
        .provider_registry
        .providers
        .iter()
        .find(|r| {
            let p = r.value();
            p.endpoint == id
                || p.status
                    .as_ref()
                    .and_then(|s| s.provider.as_ref())
                    .map(|prov| prov.id == id)
                    .unwrap_or(false)
        })
        .map(|r| r.value().endpoint.clone());

    match endpoint {
        Some(ep) => {
            let removed = state.provider_registry.remove_provider(&ep, true);
            if removed {
                info!(endpoint = %ep, "Provider removed by admin");
                admin_ok(serde_json::json!({
                    "endpoint": ep,
                    "status": "removed",
                }))
            } else {
                admin_error("Provider not found", StatusCode::NOT_FOUND)
            }
        }
        None => admin_error("Provider not found", StatusCode::NOT_FOUND),
    }
}

/// POST /admin/providers/:id/drain -- gracefully drain connections
async fn drain_provider_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let found = state
        .provider_registry
        .providers
        .iter()
        .find(|r| {
            let p = r.value();
            p.endpoint == id
                || p.status
                    .as_ref()
                    .and_then(|s| s.provider.as_ref())
                    .map(|prov| prov.id == id)
                    .unwrap_or(false)
        });

    match found {
        Some(entry) => {
            entry.value().draining.store(true, Ordering::Relaxed);
            let ep = entry.value().endpoint.clone();
            let active = entry.value().active_requests.load(Ordering::Relaxed);
            info!(endpoint = %ep, active, "Provider draining started by admin");
            admin_ok(serde_json::json!({
                "endpoint": ep,
                "status": "draining",
                "active_requests": active,
            }))
        }
        None => admin_error("Provider not found", StatusCode::NOT_FOUND),
    }
}

/// PATCH /admin/providers/:id -- update provider config
async fn patch_provider_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ProviderPatchRequest>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let found = state
        .provider_registry
        .providers
        .get_mut(&id)
        .or_else(|| {
            // Try to find by provider ID
            let ep = state
                .provider_registry
                .providers
                .iter()
                .find(|r| {
                    r.value()
                        .status
                        .as_ref()
                        .and_then(|s| s.provider.as_ref())
                        .map(|prov| prov.id == id)
                        .unwrap_or(false)
                })
                .map(|r| r.key().clone())?;
            state.provider_registry.providers.get_mut(&ep)
        });

    match found {
        Some(mut provider) => {
            if let Some(ref region) = body.region {
                provider.region = Some(region.clone());
            }
            // labels stored in model_pricing keys conventionally; for now just acknowledge
            let updated = serde_json::json!({
                "endpoint": provider.endpoint,
                "region": provider.region,
            });
            info!(endpoint = %provider.endpoint, "Provider patched by admin");
            admin_ok(updated)
        }
        None => admin_error("Provider not found", StatusCode::NOT_FOUND),
    }
}

/// GET /admin/stats -- relay-wide stats
async fn stats_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let registry = &state.provider_registry;
    let total_providers = registry.providers.len();
    let healthy = registry.healthy_provider_count();
    let degraded = registry.degraded_provider_count();

    let total_active_requests: u32 = registry
        .providers
        .iter()
        .map(|p| p.active_requests.load(Ordering::Relaxed))
        .sum();

    let total_requests: u64 = registry
        .providers
        .iter()
        .map(|p| p.total_requests)
        .sum();

    let total_failures: u64 = registry
        .providers
        .iter()
        .map(|p| p.failed_requests)
        .sum();

    let suspended_count = registry
        .providers
        .iter()
        .filter(|p| p.suspended.load(Ordering::Relaxed))
        .count();

    let draining_count = registry
        .providers
        .iter()
        .filter(|p| p.draining.load(Ordering::Relaxed))
        .count();

    let cache_stats = state.response_cache.stats();
    let ws_pool_stats = state.ws_pool.stats();

    let pq = &state.priority_queue;
    let queue_depth_normal = pq.queue_depth(crate::priority_queue::RequestPriority::Normal);
    let queue_depth_high = pq.queue_depth(crate::priority_queue::RequestPriority::High);
    let queue_depth_low = pq.queue_depth(crate::priority_queue::RequestPriority::Low);

    let uptime_secs = state.relay_metrics.uptime_seconds();

    admin_ok(serde_json::json!({
        "uptime_secs": uptime_secs,
        "providers": {
            "total": total_providers,
            "healthy": healthy,
            "degraded": degraded,
            "suspended": suspended_count,
            "draining": draining_count,
        },
        "requests": {
            "total_proxied": total_requests,
            "total_failed": total_failures,
            "active": total_active_requests,
        },
        "cache": {
            "entries": cache_stats.entries,
            "size_bytes": cache_stats.size_bytes,
            "hits": cache_stats.hits,
            "misses": cache_stats.misses,
        },
        "ws_pool": {
            "total_connections": ws_pool_stats.total_connections,
            "provider_count": ws_pool_stats.provider_count,
            "hits": ws_pool_stats.hits,
        },
        "priority_queue": {
            "high": queue_depth_high,
            "normal": queue_depth_normal,
            "low": queue_depth_low,
        },
    }))
}

/// GET /admin/health -- aggregated health of all subsystems
async fn health_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let registry = &state.provider_registry;
    let healthy = registry.healthy_provider_count();
    let total = registry.providers.len();
    let provider_health = if total == 0 {
        "unknown".to_string()
    } else if healthy == total {
        "healthy".to_string()
    } else if healthy > 0 {
        "degraded".to_string()
    } else {
        "unhealthy".to_string()
    };

    // Cache health
    let cache_stats = state.response_cache.stats();
    let cache_health = if state.config.cache.enabled {
        "enabled"
    } else {
        "disabled"
    };

    // WS pool health
    let ws_pool_stats = state.ws_pool.stats();
    let ws_pool_health = if state.config.ws_pool.enabled {
        "enabled"
    } else {
        "disabled"
    };

    // Rate limiter health
    let rate_limit_health = if state.rate_limit_state.is_some() {
        "enabled"
    } else {
        "disabled"
    };

    // Dedup health
    let dedup_health = if state.config.dedup.enabled {
        "enabled"
    } else {
        "disabled"
    };

    // Degradation level
    let degradation_level = state.degradation_manager.current_level();

    // DB (chain scanner) health
    let db_health = if state.chain_scanner.is_some() {
        if let Some(ref cache) = state.chain_cache {
            if cache.is_healthy() { "healthy" } else { "stale" }
        } else {
            "unknown"
        }
    } else {
        "disabled"
    };

    let overall = if provider_health == "healthy"
        && degradation_level == DegradationLevel::Full
    {
        "healthy"
    } else if provider_health == "unhealthy" {
        "unhealthy"
    } else {
        "degraded"
    };

    admin_ok(serde_json::json!({
        "overall": overall,
        "providers": {
            "status": provider_health,
            "healthy": healthy,
            "total": total,
        },
        "db": {
            "status": db_health,
        },
        "cache": {
            "status": cache_health,
            "entries": cache_stats.entries,
            "hit_rate": if cache_stats.hits + cache_stats.misses > 0 {
                format!("{:.2}%", cache_stats.hits as f64 / (cache_stats.hits + cache_stats.misses) as f64 * 100.0)
            } else {
                "N/A".to_string()
            },
        },
        "ws_pool": {
            "status": ws_pool_health,
            "total": ws_pool_stats.total_connections,
            "providers": ws_pool_stats.provider_count,
        },
        "rate_limiter": {
            "status": rate_limit_health,
        },
        "dedup": {
            "status": dedup_health,
        },
        "degradation": {
            "level": serde_json::to_value(degradation_level).unwrap_or(serde_json::json!("unknown")),
        },
    }))
}

/// POST /admin/cache/clear -- clear response cache
async fn cache_clear_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let before = state.response_cache.stats().entries;
    state.response_cache.clear();
    info!(cleared = before, "Admin cleared response cache");
    admin_ok(serde_json::json!({
        "status": "cleared",
        "entries_removed": before,
    }))
}

/// POST /admin/cache/invalidate -- invalidate cache entries by prefix
async fn cache_invalidate_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CacheInvalidateRequest>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let before = state.response_cache.stats().entries;
    state.response_cache.invalidate_prefix(&body.prefix);
    let after = state.response_cache.stats().entries;
    let removed = before.saturating_sub(after);
    info!(prefix = %body.prefix, removed, "Admin invalidated cache prefix");
    admin_ok(serde_json::json!({
        "status": "invalidated",
        "prefix": body.prefix,
        "entries_removed": removed,
    }))
}

/// GET /admin/config -- get current relay config (redacted)
async fn config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let cfg = &state.config;

    admin_ok(serde_json::json!({
        "relay": {
            "listen_addr": cfg.relay.listen_addr,
            "cors_origins": cfg.relay.cors_origins,
            "health_poll_interval_secs": cfg.relay.health_poll_interval_secs,
            "provider_timeout_secs": cfg.relay.provider_timeout_secs,
            "max_fallback_attempts": cfg.relay.max_fallback_attempts,
        },
        "chain": {
            "enabled": cfg.chain.enabled,
            "ergo_node_url": cfg.chain.ergo_node_url,
            "scan_interval_secs": cfg.chain.scan_interval_secs,
        },
        "balance": {
            "enabled": cfg.balance.enabled,
            "min_balance_nanoerg": cfg.balance.min_balance_nanoerg,
        },
        "auth": {
            "enabled": cfg.auth.enabled,
            "max_age_secs": cfg.auth.max_age_secs,
        },
        "rate_limit": {
            "enabled": cfg.rate_limit.enabled,
            "ip_rpm": cfg.rate_limit.ip_rpm,
            "key_rpm": cfg.rate_limit.key_rpm,
        },
        "cache": {
            "enabled": cfg.cache.enabled,
            "max_entries": cfg.cache.max_entries,
            "default_ttl_secs": cfg.cache.default_ttl_secs,
        },
        "circuit_breaker": {
            "failure_threshold": cfg.circuit_breaker.failure_threshold,
            "timeout_secs": cfg.circuit_breaker.timeout_secs,
        },
        "load_shed": {
            "enabled": cfg.load_shed.enabled,
            "max_concurrent_requests": cfg.load_shed.max_concurrent_requests,
            "max_queue_size": cfg.load_shed.max_queue_size,
        },
        "degradation": {
            "enabled": cfg.degradation.enabled,
            "auto_degrade": cfg.degradation.auto_degrade,
        },
        "coalesce": {
            "enabled": cfg.coalesce.enabled,
            "max_wait_ms": cfg.coalesce.max_wait_ms,
        },
        "adaptive_routing": {
            "enabled": cfg.adaptive_routing.enabled,
            "strategy": cfg.adaptive_routing.strategy,
            "geo_routing_enabled": cfg.adaptive_routing.geo_routing_enabled,
        },
        "admin": {
            "enabled": cfg.admin.enabled,
            "api_key": "[REDACTED]",
        },
    }))
}

/// POST /admin/degradation/set -- set degradation level
async fn degradation_set_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DegradationSetRequest>,
) -> Response {
    if let Err(status) = verify_admin_key(&state, &headers) {
        return admin_error("Invalid or missing admin key", status);
    }

    let old_level = state.degradation_manager.current_level();
    state.degradation_manager.set_level(body.level);
    info!(
        old = ?old_level,
        new = ?body.level,
        "Admin set degradation level"
    );
    admin_ok(serde_json::json!({
        "status": "ok",
        "old_level": serde_json::to_value(old_level).unwrap_or(serde_json::json!("unknown")),
        "new_level": serde_json::to_value(body.level).unwrap_or(serde_json::json!("unknown")),
    }))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the admin API router. Mounted under `/admin` in main.rs.
/// Only mounted when `config.admin.enabled == true` and `config.admin.api_key` is non-empty.
pub fn build_admin_router(state: AppState) -> Router<AppState> {
    Router::new()
        // Provider management
        .route("/admin/providers", get(list_providers_handler))
        .route("/admin/providers/{id}", get(get_provider_handler))
        .route("/admin/providers/{id}/suspend", post(suspend_provider_handler))
        .route("/admin/providers/{id}/resume", post(resume_provider_handler))
        .route("/admin/providers/{id}/drain", post(drain_provider_handler))
        .route("/admin/providers/{id}", delete(remove_provider_handler))
        .route("/admin/providers/{id}", patch(patch_provider_handler))
        // System stats & health
        .route("/admin/stats", get(stats_handler))
        .route("/admin/health", get(health_handler))
        // Cache management
        .route("/admin/cache/clear", post(cache_clear_handler))
        .route("/admin/cache/invalidate", post(cache_invalidate_handler))
        // Config (redacted)
        .route("/admin/config", get(config_handler))
        // Degradation control
        .route("/admin/degradation/set", post(degradation_set_handler))
        .with_state(state)
}
