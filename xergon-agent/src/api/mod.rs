//! REST API for the Xergon agent
//!
//! Exposes endpoints for:
//! - `/xergon/status` — Status endpoint that other Xergon agents probe
//! - `/xergon/peers` — Current peer discovery state
//! - `/xergon/health` — Basic health check
//! - `/xergon/settlement` — Settlement engine status and history

use axum::{
    body::Body,
    extract::State,
    http::{StatusCode, Request},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::config::{AgentConfig, XergonConfig};
use crate::node_health::NodeHealthState;
use crate::peer_discovery::PeerDiscoveryState;
use crate::pown::PownStatus;
use crate::settlement::SettlementEngine;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub xergon_config: XergonConfig,
    pub pown_status: Arc<RwLock<PownStatus>>,
    pub peer_state: Arc<RwLock<PeerDiscoveryState>>,
    pub node_health: Arc<RwLock<NodeHealthState>>,
    pub settlement: Option<Arc<SettlementEngine>>,
    /// Process start time for computing actual uptime
    pub start_time: std::time::Instant,
    /// Optional management API key. If empty, endpoints are open.
    pub management_api_key: String,
}

/// Build a CORS layer restricted to localhost origins only.
fn localhost_cors() -> CorsLayer {
    use axum::http::HeaderValue;
    CorsLayer::new()
        .allow_origin([
            "http://127.0.0.1:9099".parse::<HeaderValue>().unwrap(),
            "http://localhost:9099".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
            "http://localhost:3000".parse::<HeaderValue>().unwrap(),
        ])
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            if let Ok(s) = origin.to_str() {
                s.starts_with("http://127.0.0.1:") || s.starts_with("http://localhost:")
            } else {
                false
            }
        }))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
        ])
}

/// Middleware that validates the Bearer token if management_api_key is configured.
async fn check_management_api_key(
    req: Request<Body>,
    next: Next,
    api_key: String,
) -> impl IntoResponse {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let provided_key = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "));

    match provided_key {
        Some(key) if key == api_key => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            "Invalid or missing management API key",
        ).into_response(),
    }
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub provider: ProviderStatus,
    pub pown_status: PownStatus,
    pub pown_health: NodeHealthState,
    // TODO(W4.8): Remove this field — always None, never populated.
    // Kept for now to avoid breaking any external consumers parsing the response.
    pub epoch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub region: String,
    // TODO(W4.8): Remove this field — always None, never populated.
    // Kept for now to avoid breaking any external consumers parsing the response.
    pub public_node_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PeersResponse {
    pub peers_checked: usize,
    pub unique_xergon_peers_seen: usize,
    pub xergon_peers: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub provider_id: String,
    pub uptime_secs: u64,
}

pub fn build_router(state: AppState) -> Router {
    let cors = localhost_cors();

    let xergon_routes = Router::new()
        .route("/xergon/status", get(status_handler))
        .route("/xergon/peers", get(peers_handler))
        .route("/xergon/health", get(health_handler))
        .route("/xergon/settlement", get(settlement_handler))
        .route("/xergon/dashboard", get(dashboard_handler))
        .route("/xergon/usage", post(usage_report_handler))
        .with_state(state.clone());

    // If management_api_key is configured, require auth on all /xergon/* routes
    let xergon_routes = if !state.management_api_key.is_empty() {
        let api_key = state.management_api_key.clone();
        info!("Management API key configured — /xergon/* endpoints require authentication");
        xergon_routes.layer(middleware::from_fn(move |req, next| {
            let api_key = api_key.clone();
            check_management_api_key(req, next, api_key)
        }))
    } else {
        xergon_routes
    };

    Router::new()
        .merge(xergon_routes)
        .layer(cors)
}

/// Build the full router including inference proxy routes.
/// Called from main.rs when inference is enabled.
///
/// Uses Router<()> so we can merge routes with different state types.
pub fn build_router_with_inference(state: AppState, inference_state: crate::inference::InferenceState) -> Router<()> {
    let cors = localhost_cors();

    let xergon_routes = Router::new()
        .route("/xergon/status", get(status_handler))
        .route("/xergon/peers", get(peers_handler))
        .route("/xergon/health", get(health_handler))
        .route("/xergon/settlement", get(settlement_handler))
        .route("/xergon/dashboard", get(dashboard_handler))
        .route("/xergon/usage", post(usage_report_handler))
        .with_state(state.clone());

    // If management_api_key is configured, require auth on all /xergon/* routes
    let xergon_routes = if !state.management_api_key.is_empty() {
        let api_key = state.management_api_key.clone();
        info!("Management API key configured — /xergon/* endpoints require authentication");
        xergon_routes.layer(middleware::from_fn(move |req, next| {
            let api_key = api_key.clone();
            check_management_api_key(req, next, api_key)
        }))
    } else {
        xergon_routes
    };

    let inference_routes = crate::inference::build_router(inference_state);

    Router::new()
        .merge(xergon_routes)
        .merge(inference_routes)
        .layer(cors)
}

/// GET /xergon/status — Called by other Xergon agents to discover us
async fn status_handler(State(state): State<AppState>) -> Result<Json<StatusResponse>, StatusCode> {
    let pown = state.pown_status.read().await;
    let health = state.node_health.read().await;

    Ok(Json(StatusResponse {
        provider: ProviderStatus {
            id: state.xergon_config.provider_id.clone(),
            name: state.xergon_config.provider_name.clone(),
            region: state.xergon_config.region.clone(),
            public_node_id: None,
        },
        pown_status: pown.clone(),
        pown_health: health.clone(),
        epoch: None,
    }))
}

/// GET /xergon/peers — Our current peer discovery state
async fn peers_handler(State(state): State<AppState>) -> Json<PeersResponse> {
    let peer_state = state.peer_state.read().await;
    Json(PeersResponse {
        peers_checked: peer_state.peers_checked,
        unique_xergon_peers_seen: peer_state.unique_xergon_peers_seen,
        xergon_peers: peer_state.xergon_peers
            .iter()
            .map(|p| serde_json::to_value(p).unwrap_or_default())
            .collect(),
    })
}

/// GET /xergon/health — Basic liveness check
async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        provider_id: state.xergon_config.provider_id.clone(),
        uptime_secs: state.start_time.elapsed().as_secs(),
    })
}

/// Settlement API response types
#[derive(Debug, Serialize)]
pub struct SettlementApiResponse {
    pub enabled: bool,
    pub summary: Option<crate::settlement::models::SettlementSummary>,
    pub pending_providers: usize,
    pub recent_batches: Vec<serde_json::Value>,
}

/// GET /xergon/settlement — Settlement engine status and history
async fn settlement_handler(State(state): State<AppState>) -> Json<SettlementApiResponse> {
    match &state.settlement {
        Some(engine) => {
            let summary = engine.summary().await;
            let pending = engine.pending_summary().await;
            let ledger = engine.ledger().await;
            let recent_batches: Vec<serde_json::Value> = ledger
                .batches
                .iter()
                .take(10)
                .map(|b| serde_json::to_value(b).unwrap_or_default())
                .collect();

            Json(SettlementApiResponse {
                enabled: true,
                summary: Some(summary),
                pending_providers: pending.len(),
                recent_batches,
            })
        }
        None => Json(SettlementApiResponse {
            enabled: false,
            summary: None,
            pending_providers: 0,
            recent_batches: vec![],
        }),
    }
}

/// POST /xergon/usage — Receive usage reports from the relay
///
/// The relay periodically aggregates per-provider usage from its DashMap
/// and POSTs it here so the settlement engine has authoritative cost data.
#[derive(Debug, Deserialize)]
pub struct UsageReportRequest {
    pub provider_id: String,
    pub ergo_address: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd: f64,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UsageReportResponse {
    pub ok: bool,
    pub recorded: bool,
}

async fn usage_report_handler(
    State(state): State<AppState>,
    Json(req): Json<UsageReportRequest>,
) -> Result<Json<UsageReportResponse>, StatusCode> {
    match &state.settlement {
        Some(engine) => {
            engine
                .record_usage(
                    &req.provider_id,
                    &req.ergo_address,
                    req.tokens_in,
                    req.tokens_out,
                    req.cost_usd,
                )
                .await;

            info!(
                provider_id = %req.provider_id,
                tokens_in = req.tokens_in,
                tokens_out = req.tokens_out,
                cost_usd = req.cost_usd,
                request_id = ?req.request_id,
                "Usage report received from relay"
            );

            Ok(Json(UsageReportResponse {
                ok: true,
                recorded: true,
            }))
        }
        None => {
            info!(
                provider_id = %req.provider_id,
                "Usage report received but settlement engine is disabled"
            );
            Ok(Json(UsageReportResponse {
                ok: true,
                recorded: false,
            }))
        }
    }
}

/// GET /xergon/dashboard — Aggregated provider dashboard data
///
/// Single endpoint that returns everything the frontend needs:
/// node health, peer list, PoNW score, provider score, settlement history.
/// This replaces the 4 separate Paperclip API calls the dashboard used to make.
#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    /// Node health status (synced, height, peers, uptime)
    pub node_status: Option<NodeStatusView>,
    /// Peer list
    pub peers: Vec<PeerView>,
    /// PoNW / AI Points data
    pub ai_points: Option<AiPointsView>,
    /// Provider scoring
    pub provider_score: Option<ProviderScoreView>,
    /// GPU / hardware info for the inference backend
    pub hardware: Option<HardwareView>,
    /// Settlement history from the settlement engine
    pub settlements: Vec<SettlementView>,
    /// Whether the provider has an ergo address configured
    pub has_wallet: bool,
}

#[derive(Debug, Serialize)]
pub struct NodeStatusView {
    pub synced: bool,
    pub height: u64,
    pub best_height: u64,
    pub peers: usize,
    pub uptime_seconds: u64,
    pub version: String,
    pub ergo_address: String,
}

#[derive(Debug, Serialize)]
pub struct PeerView {
    pub address: String,
    pub connection_type: String,
    pub height: u64,
    pub last_seen: String,
}

#[derive(Debug, Serialize)]
pub struct AiPointsView {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_tokens: u64,
    pub ai_points: u64,
    pub by_model: Vec<AiPointsModelView>,
}

#[derive(Debug, Serialize)]
pub struct AiPointsModelView {
    pub model: String,
    pub total_tokens: u64,
    pub points: u64,
    pub difficulty_multiplier: f64,
}

#[derive(Debug, Serialize)]
pub struct ProviderScoreView {
    pub weighted_composite_score: f64,
    pub best_composite_score: f64,
}

#[derive(Debug, Serialize)]
pub struct SettlementView {
    pub id: String,
    pub tx_id: String,
    pub amount_nanoerg: i64,
    pub amount_erg: f64,
    pub status: String,
    pub created_at: String,
    pub confirmed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HardwareView {
    pub devices: Vec<HardwareDevice>,
    pub last_reported_at: Option<String>,
    /// Total VRAM across all GPUs (MiB)
    pub total_vram_mb: u64,
    /// Number of CPU cores
    pub cpu_cores: usize,
    /// System RAM in GiB
    pub ram_gb: f64,
    /// OS string (e.g. "linux", "macos")
    pub os: String,
}

#[derive(Debug, Serialize)]
pub struct HardwareDevice {
    pub name: String,
    pub device_name: String,
    pub vendor: String,
    pub vram_bytes: u64,
    pub vram_mb: u64,
    pub vram_used_mb: Option<u64>,
    pub compute_version: String,
    pub detection_method: String,
    pub is_active: bool,
    pub driver: String,
}

async fn dashboard_handler(State(state): State<AppState>) -> Json<DashboardResponse> {
    let pown = state.pown_status.read().await;
    let health = state.node_health.read().await;
    let peer_state = state.peer_state.read().await;

    let has_wallet = !state.xergon_config.ergo_address.is_empty();

    // Node status
    let node_status = Some(NodeStatusView {
        synced: health.is_synced,
        height: health.node_height as u64,
        best_height: health.best_height_local as u64,
        peers: health.peer_count,
        uptime_seconds: state.start_time.elapsed().as_secs(),
        version: "0.1.0".to_string(),
        ergo_address: state.xergon_config.ergo_address.clone(),
    });

    // Peer list
    let peers: Vec<PeerView> = peer_state
        .xergon_peers
        .iter()
        .map(|p| {
            let v = serde_json::to_value(p).unwrap_or_default();
            PeerView {
                address: v.get("address").and_then(|a| a.as_str()).unwrap_or("").to_string(),
                connection_type: "direct".to_string(),
                height: v.get("height").and_then(|h| h.as_u64()).unwrap_or(0),
                last_seen: v.get("lastSeen").and_then(|l| l.as_str()).unwrap_or("").to_string(),
            }
        })
        .collect();

    // AI Points (from PoNW status)
    let ai_points = if pown.ai_enabled || pown.ai_total_tokens > 0 {
        Some(AiPointsView {
            total_input_tokens: 0,
            total_output_tokens: pown.ai_total_tokens,
            total_tokens: pown.ai_total_tokens,
            ai_points: pown.ai_points,
            by_model: if !pown.ai_model.is_empty() {
                vec![AiPointsModelView {
                    model: pown.ai_model.clone(),
                    total_tokens: pown.ai_total_tokens,
                    points: pown.ai_points,
                    difficulty_multiplier: 1.0,
                }]
            } else {
                vec![]
            },
        })
    } else {
        None
    };

    // Provider score (derived from PoNW work_points)
    let work_points = pown.work_points;
    let provider_score = Some(ProviderScoreView {
        weighted_composite_score: if work_points > 0 {
            (work_points as f64).min(100.0)
        } else {
            0.0
        },
        best_composite_score: work_points as f64,
    });

    // Settlements from settlement engine
    let settlements: Vec<SettlementView> = match &state.settlement {
        Some(engine) => {
            let ledger = engine.ledger().await;
            ledger
                .batches
                .iter()
                .map(|b| {
                    let b_json = serde_json::to_value(b).unwrap_or_default();
                    SettlementView {
                        id: b_json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        tx_id: b_json.get("tx_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        amount_nanoerg: b_json.get("amount_nanoerg").and_then(|v| v.as_i64()).unwrap_or(0),
                        amount_erg: b_json.get("amount_nanoerg").and_then(|v| v.as_i64())
                            .map(|n| n as f64 / 1e9).unwrap_or(0.0),
                        status: b_json.get("status").and_then(|v| v.as_str()).unwrap_or("pending").to_string(),
                        created_at: b_json.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        confirmed_at: b_json.get("confirmed_at").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    }
                })
                .collect()
        }
        None => vec![],
    };

    // Hardware — use real GPU detection (cached)
    let hw = crate::hardware::detect_hardware();
    let hardware = Some(HardwareView {
        devices: hw
            .gpus
            .iter()
            .map(|gpu| HardwareDevice {
                name: gpu.name.clone(),
                device_name: format!("{} GPU", gpu.driver),
                vendor: gpu.driver.clone(),
                vram_bytes: gpu.vram_mb * 1024 * 1024,
                vram_mb: gpu.vram_mb,
                vram_used_mb: gpu.vram_used_mb,
                compute_version: String::new(),
                detection_method: format!("{}-driver", gpu.driver),
                is_active: pown.ai_enabled,
                driver: gpu.driver.clone(),
            })
            .collect(),
        last_reported_at: Some(chrono::Utc::now().to_rfc3339()),
        total_vram_mb: hw.total_vram_mb,
        cpu_cores: hw.cpu_cores,
        ram_gb: hw.ram_gb,
        os: hw.os.clone(),
    });

    Json(DashboardResponse {
        node_status,
        peers,
        ai_points,
        provider_score,
        hardware,
        settlements,
        has_wallet,
    })
}

/// Start the API server
pub async fn serve(config: &AgentConfig, state: AppState) -> anyhow::Result<()> {
    let router = build_router(state);
    let addr: std::net::SocketAddr = config.api.listen_addr.parse()?;

    info!(addr = %addr, "Starting Xergon agent API server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Start the API server with inference proxy routes
pub async fn serve_with_inference(config: &AgentConfig, state: AppState, inference_state: crate::inference::InferenceState) -> anyhow::Result<()> {
    let router = build_router_with_inference(state, inference_state);
    let addr: std::net::SocketAddr = config.api.listen_addr.parse()?;

    info!(addr = %addr, "Starting Xergon agent API server (inference proxy enabled)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl-C handler");
    info!("Shutdown signal received");
}

// ---------------------------------------------------------------------------
// Tests — W7 fixes: localhost_cors
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, Method};
    use tower::ServiceBuilder;
    use tower::ServiceExt; // for .oneshot()

    /// Helper: build a CORS OPTIONS preflight request with the given Origin.
    fn preflight_req_with_origin(origin: &str) -> Request<axum::body::Body> {
        Request::builder()
            .method(Method::OPTIONS)
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .body(axum::body::Body::empty())
            .unwrap()
    }

    /// A no-op service that just returns 200 OK — used as the inner service
    /// for the CORS layer so we can test CORS headers in isolation.
    struct OkService;

    impl tower::Service<Request<axum::body::Body>> for OkService {
        type Response = axum::response::Response;
        type Error = std::convert::Infallible;
        type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<axum::body::Body>) -> Self::Future {
            std::future::ready(Ok(axum::response::Response::builder()
                .status(200)
                .body(axum::body::Body::empty())
                .unwrap()))
        }
    }

    /// Build a CORS-wrapped service ready for oneshot requests.
    fn cors_service() -> impl tower::Service<Request<axum::body::Body>, Response = axum::response::Response, Error = std::convert::Infallible> {
        ServiceBuilder::new()
            .layer(localhost_cors())
            .service(OkService)
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_localhost_9099() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("http://localhost:9099")).await.unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://localhost:9099");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_127_0_0_1_9099() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("http://127.0.0.1:9099")).await.unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://127.0.0.1:9099");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_localhost_3000() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("http://localhost:3000")).await.unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://localhost:3000");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_127_0_0_1_3000() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("http://127.0.0.1:3000")).await.unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://127.0.0.1:3000");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_localhost_arbitrary_port() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("http://localhost:5173")).await.unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        // The predicate-based AllowOrigin should echo back the origin
        assert_eq!(origin, "http://localhost:5173");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_127_0_0_1_arbitrary_port() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("http://127.0.0.1:8080")).await.unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://127.0.0.1:8080");
    }

    #[tokio::test]
    async fn test_localhost_cors_rejects_foreign_origin() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("https://evil.example.com")).await.unwrap();
        let allowed = resp.headers().get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap_or(""));
        // A foreign origin should NOT get an allow-origin header echoing it back
        assert_ne!(allowed, Some("https://evil.example.com"));
    }

    #[tokio::test]
    async fn test_localhost_cors_rejects_http_subdomain() {
        let svc = cors_service();
        let resp = svc.oneshot(preflight_req_with_origin("http://malicious.localhost:3000")).await.unwrap();
        let allowed = resp.headers().get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap_or(""));
        // Subdomain of localhost should NOT be allowed
        assert_ne!(allowed, Some("http://malicious.localhost:3000"));
    }
}
