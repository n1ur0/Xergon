//! Health-check endpoint handler for the Xergon relay.

use axum::extract::State;
use axum::{Json, Router, routing::get};
use serde::Serialize;

#[allow(unused_imports)]
use crate::metrics::RelayMetrics;
use crate::proxy::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub ergo_node_connected: bool,
    pub active_providers: u64,
    pub degraded_providers: u64,
    pub total_providers: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<crate::cache::CacheStats>,
}

pub fn build_health_router() -> Router<AppState> {
    Router::new()
        .route("/v1/health", get(health_handler))
        .route("/v1/metrics", get(metrics_handler))
        .route("/api/oracle/rate", get(oracle_rate_handler))
}

async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let cache_stats = if state.config.cache.enabled {
        Some(state.response_cache.stats())
    } else {
        None
    };

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: state.relay_metrics.uptime_seconds(),
        ergo_node_connected: state
            .chain_scanner
            .as_ref()
            .map(|_s| true) // chain_scanner present means chain is enabled
            .unwrap_or(false),
        active_providers: state.provider_registry.healthy_provider_count() as u64,
        total_providers: state.provider_registry.providers.len() as u64,
        degraded_providers: state.provider_registry.degraded_provider_count() as u64,
        cache: cache_stats,
    })
}

async fn metrics_handler(State(state): State<AppState>) -> String {
    state.relay_metrics.render_prometheus()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
            uptime_secs: 92520,
            ergo_node_connected: true,
            active_providers: 15,
            total_providers: 23,
            degraded_providers: 3,
            cache: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"active_providers\":15"));
        assert!(json.contains("\"total_providers\":23"));
        // cache: None should be omitted
        assert!(!json.contains("\"cache\":null"));
    }
}

/// GET /api/oracle/rate — Return the current ERG/USD rate from the oracle cache
#[derive(Debug, Serialize)]
pub struct OracleRateResponse {
    pub erg_usd: Option<f64>,
}

async fn oracle_rate_handler(State(state): State<AppState>) -> Json<OracleRateResponse> {
    let erg_usd = state
        .erg_usd_rate
        .read()
        .ok()
        .and_then(|r| *r);
    Json(OracleRateResponse { erg_usd })
}
