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
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::airdrop::AirdropService;
use crate::config::PricingConfig;
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
    /// Airdrop service (Some if enabled, None if disabled)
    pub airdrop: Option<Arc<AirdropService>>,
    /// GPU rental config (Some if enabled, None if disabled)
    pub gpu_rental_config: Option<Arc<crate::config::GpuRentalConfig>>,
    /// GPU usage metering (Some if gpu_rental is enabled)
    pub usage_meter: Option<Arc<crate::gpu_rental::metering::UsageMeter>>,
    /// GPU SSH tunnel manager (Some if gpu_rental + ssh is enabled)
    pub tunnel_manager: Option<Arc<crate::gpu_rental::tunnel::TunnelManager>>,
    /// P2P engine for provider-to-provider communication (Some if enabled)
    pub p2p_engine: Option<Arc<crate::p2p::P2PEngine>>,
    /// Auto model pull system (Some if enabled)
    pub auto_pull: Option<Arc<crate::auto_model_pull::AutoModelPull>>,
    /// Usage proof rollup system (Some if enabled)
    pub rollup: Option<Arc<crate::rollup::UsageRollup>>,
    /// Prometheus metrics collector
    pub metrics: Arc<crate::metrics::MetricsCollector>,
    /// Loaded inference model names (for health endpoint)
    pub models_loaded: Arc<RwLock<Vec<String>>>,
    /// Pricing configuration (read from config file, mutable for updates)
    pub pricing: Arc<RwLock<PricingConfig>>,
    /// Path to the config file (for writing pricing updates)
    pub config_path: std::path::PathBuf,
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


/// Build a structured JSON error response.
fn json_error(status: StatusCode, error_type: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": {
                "type": error_type,
                "message": message,
                "code": status.as_u16(),
            }
        })),
    )
        .into_response()
}

/// Map a StatusCode to a default error type string.
fn status_to_error_type(status: &StatusCode) -> &'static str {
    match status.as_u16() {
        400 => "invalid_request",
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        409 => "invalid_request",
        429 => "rate_limit_error",
        503 => "service_unavailable",
        _ => "internal_error",
    }
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
        _ => json_error(StatusCode::UNAUTHORIZED, "unauthorized", "Invalid or missing management API key"),
    }
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub provider: ProviderStatus,
    pub pown_status: PownStatus,
    pub pown_health: NodeHealthState,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub region: String,
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
        .route("/xergon/pricing", get(pricing_get_handler).post(pricing_post_handler))
        .with_state(state.clone());

    // If management_api_key is configured, require auth on all /xergon/* routes
    let xergon_routes = if !state.management_api_key.is_empty() {
        let api_key = Arc::new(state.management_api_key.clone());
        info!("Management API key configured — /xergon/* endpoints require authentication");
        xergon_routes.layer(middleware::from_fn(move |req, next| {
            check_management_api_key(req, next, api_key.to_string())
        }))
    } else {
        xergon_routes
    };

    // Public airdrop routes (not behind management API key — accessible to new users)
    let airdrop_routes = Router::new()
        .route("/api/airdrop/request", post(airdrop_request_handler))
        .route("/api/airdrop/eligibility", post(airdrop_eligibility_handler))
        .route("/api/airdrop/stats", get(airdrop_stats_handler))
        .with_state(state.clone());

    // GPU Bazar rental routes
    let gpu_routes = Router::new()
        .route("/api/gpu/list", post(gpu_list_handler))
        .route("/api/gpu/rent", post(gpu_rent_handler))
        .route("/api/gpu/claim", post(gpu_claim_handler))
        .route("/api/gpu/refund", post(gpu_refund_handler))
        .route("/api/gpu/my-rentals", get(gpu_my_rentals_handler))
        .route("/api/gpu/extend", post(gpu_extend_handler))
        .route("/api/gpu/my-listings", get(gpu_my_listings_handler))
        .route("/api/gpu/sessions", get(gpu_sessions_handler))
        .route("/api/gpu/sessions/{rental_id}", get(gpu_session_handler))
        .route("/api/gpu/tunnel", post(gpu_tunnel_create_handler))
        .route("/api/gpu/tunnel/{tunnel_id}", delete(gpu_tunnel_close_handler))
        .route("/api/gpu/rate", post(gpu_rate_handler))
        .route("/api/gpu/reputation/{pk}", get(gpu_reputation_handler))
        .with_state(state.clone());

    // P2P provider-to-provider communication routes (public)
    let p2p_routes = Router::new()
        .route("/api/peer/info", get(peer_info_handler))
        .route("/api/peer/models", get(peer_models_handler))
        .route("/api/peer/model-notify", post(peer_model_notify_handler))
        .route("/api/peer/proxy-request", post(peer_proxy_request_handler))
        .with_state(state.clone());

    // Monitoring routes (public — no auth required)
    let monitoring_routes = Router::new()
        .route("/api/health", get(api_health_handler))
        .route("/api/metrics", get(api_metrics_handler))
        .with_state(state.clone());

    Router::new().merge(xergon_routes).merge(airdrop_routes).merge(gpu_routes).merge(p2p_routes).merge(monitoring_routes).layer(cors)
}

/// Build the full router including inference proxy routes.
/// Called from main.rs when inference is enabled.
///
/// Uses Router<()> so we can merge routes with different state types.
pub fn build_router_with_inference(
    state: AppState,
    inference_state: crate::inference::InferenceState,
) -> Router<()> {
    let cors = localhost_cors();

    let xergon_routes = Router::new()
        .route("/xergon/status", get(status_handler))
        .route("/xergon/peers", get(peers_handler))
        .route("/xergon/health", get(health_handler))
        .route("/xergon/settlement", get(settlement_handler))
        .route("/xergon/dashboard", get(dashboard_handler))
        .route("/xergon/usage", post(usage_report_handler))
        .route("/xergon/pricing", get(pricing_get_handler).post(pricing_post_handler))
        .with_state(state.clone());

    // If management_api_key is configured, require auth on all /xergon/* routes
    let xergon_routes = if !state.management_api_key.is_empty() {
        let api_key = Arc::new(state.management_api_key.clone());
        info!("Management API key configured — /xergon/* endpoints require authentication");
        xergon_routes.layer(middleware::from_fn(move |req, next| {
            check_management_api_key(req, next, api_key.to_string())
        }))
    } else {
        xergon_routes
    };

    let inference_routes = crate::inference::build_router(inference_state);

    // Public airdrop routes (not behind management API key — accessible to new users)
    let airdrop_routes = Router::new()
        .route("/api/airdrop/request", post(airdrop_request_handler))
        .route("/api/airdrop/eligibility", post(airdrop_eligibility_handler))
        .route("/api/airdrop/stats", get(airdrop_stats_handler))
        .with_state(state.clone());

    // GPU Bazar rental routes
    let gpu_routes = Router::new()
        .route("/api/gpu/list", post(gpu_list_handler))
        .route("/api/gpu/rent", post(gpu_rent_handler))
        .route("/api/gpu/claim", post(gpu_claim_handler))
        .route("/api/gpu/refund", post(gpu_refund_handler))
        .route("/api/gpu/my-rentals", get(gpu_my_rentals_handler))
        .route("/api/gpu/extend", post(gpu_extend_handler))
        .route("/api/gpu/my-listings", get(gpu_my_listings_handler))
        .route("/api/gpu/sessions", get(gpu_sessions_handler))
        .route("/api/gpu/sessions/{rental_id}", get(gpu_session_handler))
        .route("/api/gpu/tunnel", post(gpu_tunnel_create_handler))
        .route("/api/gpu/tunnel/{tunnel_id}", delete(gpu_tunnel_close_handler))
        .route("/api/gpu/rate", post(gpu_rate_handler))
        .route("/api/gpu/reputation/{pk}", get(gpu_reputation_handler))
        .with_state(state.clone());

    // P2P provider-to-provider communication routes (public)
    let p2p_routes = Router::new()
        .route("/api/peer/info", get(peer_info_handler))
        .route("/api/peer/models", get(peer_models_handler))
        .route("/api/peer/model-notify", post(peer_model_notify_handler))
        .route("/api/peer/proxy-request", post(peer_proxy_request_handler))
        .with_state(state.clone());

    // Monitoring routes (public — no auth required)
    let monitoring_routes = Router::new()
        .route("/api/health", get(api_health_handler))
        .route("/api/metrics", get(api_metrics_handler))
        .with_state(state.clone());

    Router::new()
        .merge(xergon_routes)
        .merge(inference_routes)
        .merge(airdrop_routes)
        .merge(gpu_routes)
        .merge(p2p_routes)
        .merge(monitoring_routes)
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
        },
        pown_status: pown.clone(),
        pown_health: health.clone(),
    }))
}

/// GET /xergon/peers — Our current peer discovery state
async fn peers_handler(State(state): State<AppState>) -> Json<PeersResponse> {
    let peer_state = state.peer_state.read().await;
    Json(PeersResponse {
        peers_checked: peer_state.peers_checked,
        unique_xergon_peers_seen: peer_state.unique_xergon_peers_seen,
        xergon_peers: peer_state
            .xergon_peers
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
    pub cost_nanoerg: u64,
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
                    req.cost_nanoerg,
                )
                .await;

            info!(
                provider_id = %req.provider_id,
                tokens_in = req.tokens_in,
                tokens_out = req.tokens_out,
                cost_nanoerg = req.cost_nanoerg,
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
                address: v
                    .get("address")
                    .and_then(|a| a.as_str())
                    .unwrap_or("")
                    .to_string(),
                connection_type: "direct".to_string(),
                height: v.get("height").and_then(|h| h.as_u64()).unwrap_or(0),
                last_seen: v
                    .get("lastSeen")
                    .and_then(|l| l.as_str())
                    .unwrap_or("")
                    .to_string(),
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
                        id: b_json
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        tx_id: b_json
                            .get("tx_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        amount_nanoerg: b_json
                            .get("amount_nanoerg")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        amount_erg: b_json
                            .get("amount_nanoerg")
                            .and_then(|v| v.as_i64())
                            .map(|n| n as f64 / 1e9)
                            .unwrap_or(0.0),
                        status: b_json
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("pending")
                            .to_string(),
                        created_at: b_json
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        confirmed_at: b_json
                            .get("confirmed_at")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
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
pub async fn serve_with_inference(
    config: &AgentConfig,
    state: AppState,
    inference_state: crate::inference::InferenceState,
) -> anyhow::Result<()> {
    let router = build_router_with_inference(state, inference_state);
    let addr: std::net::SocketAddr = config.api.listen_addr.parse()?;

    info!(addr = %addr, "Starting Xergon agent API server (inference proxy enabled)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Airdrop API handlers
// ---------------------------------------------------------------------------

/// POST /api/airdrop/request
///
/// Request an ERG airdrop for a new user wallet.
/// Body: { "public_key": "<hex>" }
/// Response: { "tx_id": "...", "amount_nanoerg": 10000000, "status": "airdropped" }
async fn airdrop_request_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::airdrop::AirdropRequest>,
) -> Result<Json<crate::airdrop::AirdropResponse>, Response> {
    let service = match &state.airdrop {
        Some(s) => s,
        None => {
            return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "Airdrop service is not enabled on this agent"));
        }
    };

    match service.execute_airdrop(&req.public_key).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::warn!(error = %e, pk = %req.public_key, "Airdrop request failed");
            let status = match &e {
                crate::airdrop::AirdropError::Disabled => StatusCode::SERVICE_UNAVAILABLE,
                crate::airdrop::AirdropError::CooldownActive { .. } => StatusCode::TOO_MANY_REQUESTS,
                crate::airdrop::AirdropError::BudgetExhausted { .. } => StatusCode::INSUFFICIENT_STORAGE,
                crate::airdrop::AirdropError::InvalidPublicKey(_) => StatusCode::BAD_REQUEST,
                crate::airdrop::AirdropError::WalletLocked => StatusCode::SERVICE_UNAVAILABLE,
                crate::airdrop::AirdropError::NodeWalletError(_) => StatusCode::BAD_GATEWAY,
                crate::airdrop::AirdropError::HttpError(_) => StatusCode::BAD_GATEWAY,
            };
            Err(json_error(status, status_to_error_type(&status), &e.to_string()))
        }
    }
}

/// POST /api/airdrop/eligibility
///
/// Check if a public key is eligible for an airdrop without actually executing it.
/// Body: { "public_key": "<hex>" }
/// Response: { "eligible": true/false, "reason": "..." }
async fn airdrop_eligibility_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::airdrop::AirdropRequest>,
) -> Result<Json<crate::airdrop::EligibilityResponse>, Response> {
    let service = match &state.airdrop {
        Some(s) => s,
        None => {
            return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "Airdrop service is not enabled on this agent"));
        }
    };

    match service.check_eligibility(&req.public_key) {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            let status = match &e {
                crate::airdrop::AirdropError::InvalidPublicKey(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err(json_error(status, status_to_error_type(&status), &e.to_string()))
        }
    }
}

/// GET /api/airdrop/stats
///
/// Get airdrop service statistics.
/// Response: { "enabled": true, "total_airdropped_erg": 0.5, ... }
async fn airdrop_stats_handler(
    State(state): State<AppState>,
) -> Result<Json<crate::airdrop::AirdropStats>, Response> {
    let service = match &state.airdrop {
        Some(s) => s,
        None => {
            return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "Airdrop service is not enabled on this agent"));
        }
    };

    Ok(Json(service.stats()))
}

// ---------------------------------------------------------------------------
// GPU Bazar API handlers (Phase 4)
// ---------------------------------------------------------------------------

/// POST /api/gpu/list -- Browse available GPU listings.
async fn gpu_list_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_rental::types::BrowseListingsRequest>,
) -> Result<Json<crate::gpu_rental::types::BrowseListingsResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    let listings = match crate::gpu_rental::scanner::scan_gpu_listings(&client, &config.listing_tree_hex).await {
        Ok(l) => l,
        Err(e) => return Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Failed to scan listings: {}", e))),
    };

    let current_height = client.get_height().await.unwrap_or(0);

    let filtered = crate::gpu_rental::scanner::filter_listings(
        listings,
        req.min_vram_gb,
        req.max_price_per_hour,
        req.region.as_deref(),
        req.gpu_type_contains.as_deref(),
    );

    Ok(Json(crate::gpu_rental::types::BrowseListingsResponse {
        listings: filtered,
        current_height,
    }))
}

/// POST /api/gpu/rent -- Initiate a GPU rental.
async fn gpu_rent_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_rental::types::RentGpuRequest>,
) -> Result<Json<crate::gpu_rental::types::RentGpuResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    if req.hours < 1 || req.hours > config.max_rental_hours {
        return Err(json_error(StatusCode::BAD_REQUEST, "invalid_request", &format!("Hours must be 1-{}", config.max_rental_hours)));
    }

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    match crate::gpu_rental::transactions::rent_gpu_tx(
        &client, &config.rental_tree_hex, &req.listing_box_id, req.hours, &req.renter_address,
    ).await {
        Ok((tx_id, deadline, cost)) => Ok(Json(crate::gpu_rental::types::RentGpuResponse {
            tx_id,
            box_id: String::new(), // Will be known after tx confirmation
            rental_nft_id: String::new(),
            deadline_height: deadline,
            cost_nanoerg: cost,
            status: "rental_initiated".to_string(),
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Rental failed: {}", e))),
    }
}

/// POST /api/gpu/claim -- Provider claims payment after rental period.
async fn gpu_claim_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_rental::types::ClaimRentalRequest>,
) -> Result<Json<crate::gpu_rental::types::RentalActionResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    match crate::gpu_rental::transactions::claim_rental_tx(
        &client, &req.rental_box_id, &req.provider_address,
    ).await {
        Ok(tx_id) => Ok(Json(crate::gpu_rental::types::RentalActionResponse {
            tx_id,
            status: "claimed".to_string(),
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Claim failed: {}", e))),
    }
}

/// POST /api/gpu/refund -- Renter refunds before deadline.
async fn gpu_refund_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_rental::types::RefundRentalRequest>,
) -> Result<Json<crate::gpu_rental::types::RentalActionResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    match crate::gpu_rental::transactions::refund_rental_tx(
        &client, &req.rental_box_id, &req.renter_address,
    ).await {
        Ok(tx_id) => Ok(Json(crate::gpu_rental::types::RentalActionResponse {
            tx_id,
            status: "refunded".to_string(),
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Refund failed: {}", e))),
    }
}

/// GET /api/gpu/my-rentals -- List active rentals.
async fn gpu_my_rentals_handler(
    State(state): State<AppState>,
) -> Result<Json<crate::gpu_rental::types::MyRentalsResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    let current_height = client.get_height().await.unwrap_or(0);

    match crate::gpu_rental::scanner::scan_gpu_rentals(&client, &config.rental_tree_hex).await {
        Ok(rentals) => Ok(Json(crate::gpu_rental::types::MyRentalsResponse {
            rentals,
            current_height,
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Failed to scan rentals: {}", e))),
    }
}

/// POST /api/gpu/extend -- Extend an active rental.
async fn gpu_extend_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_rental::types::ExtendRentalRequest>,
) -> Result<Json<crate::gpu_rental::types::RentalActionResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    match crate::gpu_rental::transactions::extend_rental_tx(
        &client, &req.rental_box_id, req.additional_hours,
    ).await {
        Ok(tx_id) => Ok(Json(crate::gpu_rental::types::RentalActionResponse {
            tx_id,
            status: "extended".to_string(),
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Extend failed: {}", e))),
    }
}

/// GET /api/gpu/my-listings -- List provider's own GPU listings.
async fn gpu_my_listings_handler(
    State(state): State<AppState>,
) -> Result<Json<crate::gpu_rental::types::MyListingsResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    let current_height = client.get_height().await.unwrap_or(0);

    match crate::gpu_rental::scanner::scan_gpu_listings(&client, &config.listing_tree_hex).await {
        Ok(listings) => Ok(Json(crate::gpu_rental::types::MyListingsResponse {
            listings,
            current_height,
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Failed to scan listings: {}", e))),
    }
}

// ---------------------------------------------------------------------------
// GPU Session & Tunnel API handlers (usage metering + SSH tunnels)
// ---------------------------------------------------------------------------

/// Response for listing sessions.
#[derive(Debug, Serialize)]
struct SessionsListResponse {
    pub sessions: Vec<crate::gpu_rental::metering::UsageSnapshot>,
    pub active_count: usize,
}

/// GET /api/gpu/sessions -- List all active metering sessions.
async fn gpu_sessions_handler(
    State(state): State<AppState>,
) -> Result<Json<SessionsListResponse>, Response> {
    let meter = match &state.usage_meter {
        Some(m) => m,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let sessions = meter.active_sessions();
    let active_count = meter.active_count();

    Ok(Json(SessionsListResponse {
        sessions,
        active_count,
    }))
}

/// GET /api/gpu/sessions/{rental_id} -- Get usage for a specific session.
async fn gpu_session_handler(
    State(state): State<AppState>,
    axum::extract::Path(rental_id): axum::extract::Path<String>,
) -> Result<Json<crate::gpu_rental::metering::UsageSnapshot>, Response> {
    let meter = match &state.usage_meter {
        Some(m) => m,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    match meter.get_usage(&rental_id) {
        Some(snapshot) => Ok(Json(snapshot)),
        None => Err(json_error(StatusCode::NOT_FOUND, "not_found", &format!("No session found for rental {}", rental_id))),
    }
}

/// POST /api/gpu/tunnel -- Create an SSH/Jupyter tunnel for a rental.
async fn gpu_tunnel_create_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_rental::tunnel::CreateTunnelRequest>,
) -> Result<Json<crate::gpu_rental::tunnel::ActiveTunnel>, Response> {
    let tunnel_mgr = match &state.tunnel_manager {
        Some(m) => m,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "SSH tunneling not enabled")),
    };

    let meter = match &state.usage_meter {
        Some(m) => m,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    // Verify the session exists
    let session = meter.get_usage(&req.rental_id).ok_or_else(|| {
        json_error(StatusCode::NOT_FOUND, "not_found", &format!("No active session for rental {}", req.rental_id))
    })?;

    if !session.is_active {
        return Err(json_error(StatusCode::CONFLICT, "invalid_request", &format!("Session for rental {} is not active (expired or stopped)", req.rental_id)));
    }

    // Determine remote host and port
    let remote_host = req.remote_host.unwrap_or_else(|| "127.0.0.1".to_string());
    let remote_port = req.remote_port.unwrap_or(match req.tunnel_type {
        crate::gpu_rental::tunnel::TunnelType::Ssh => 22,
        crate::gpu_rental::tunnel::TunnelType::Jupyter => 8888,
        crate::gpu_rental::tunnel::TunnelType::Custom => 22,
    });

    // Create the tunnel
    match tunnel_mgr.create_tunnel(&req.rental_id, &remote_host, remote_port, req.tunnel_type) {
        Ok(tunnel) => {
            // Attach tunnel info to the metering session
            let tunnel_info = crate::gpu_rental::tunnel::TunnelInfo {
                tunnel_id: tunnel.tunnel_id.clone(),
                tunnel_type: tunnel.tunnel_type,
                local_port: tunnel.local_port,
                remote_host: tunnel.remote_host.clone(),
                remote_port: tunnel.remote_port,
            };
            let _ = meter.attach_tunnel(&req.rental_id, tunnel_info);

            Ok(Json(tunnel))
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to create tunnel");
            Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Failed to create tunnel: {}", e)))
        }
    }
}

/// DELETE /api/gpu/tunnel/{tunnel_id} -- Close a tunnel.
async fn gpu_tunnel_close_handler(
    State(state): State<AppState>,
    axum::extract::Path(tunnel_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, Response> {
    let tunnel_mgr = match &state.tunnel_manager {
        Some(m) => m,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "SSH tunneling not enabled")),
    };

    match tunnel_mgr.close_tunnel(&tunnel_id) {
        Ok(()) => Ok(Json(serde_json::json!({
            "tunnel_id": tunnel_id,
            "status": "closed",
        }))),
        Err(e) => Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", &format!("Failed to close tunnel: {}", e))),
    }
}

// ---------------------------------------------------------------------------
// GPU Rating & Reputation API handlers
// ---------------------------------------------------------------------------

/// POST /api/gpu/rate -- Submit a GPU rental rating.
///
/// Creates a rating box on-chain. The rater's wallet must be unlocked.
async fn gpu_rate_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_rental::rating::types::SubmitRatingRequest>,
) -> Result<Json<crate::gpu_rental::rating::types::SubmitRatingResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    match crate::gpu_rental::rating::transactions::submit_rating_tx(
        &client,
        &config.rating_tree_hex,
        &req.rental_box_id,
        &req.rated_pk,
        &req.role,
        req.rating,
        req.comment.as_deref(),
    )
    .await
    {
        Ok(tx_id) => Ok(Json(crate::gpu_rental::rating::types::SubmitRatingResponse {
            tx_id,
            box_id: String::new(), // Box ID not available from wallet payment API response
            status: "submitted".to_string(),
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Rating submission failed: {}", e))),
    }
}

/// GET /api/gpu/reputation/{pk} -- Get aggregated reputation for a public key.
///
/// Scans all rating boxes on-chain and computes average rating, star breakdown,
/// and separate provider/renter reputation scores.
async fn gpu_reputation_handler(
    State(state): State<AppState>,
    axum::extract::Path(pk): axum::extract::Path<String>,
) -> Result<Json<crate::gpu_rental::rating::types::ReputationResponse>, Response> {
    let config = match &state.gpu_rental_config {
        Some(c) => c,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "GPU rental not enabled")),
    };

    let client = crate::chain::client::ErgoNodeClient::new(config.ergo_node_url.clone());

    match crate::gpu_rental::rating::scanner::compute_reputation(
        &client,
        &config.rating_tree_hex,
        &pk,
    )
    .await
    {
        Ok(reputation) => Ok(Json(crate::gpu_rental::rating::types::ReputationResponse {
            reputation,
        })),
        Err(e) => Err(json_error(StatusCode::BAD_GATEWAY, "internal_error", &format!("Failed to compute reputation: {}", e))),
    }
}

// ---------------------------------------------------------------------------
// Monitoring API handlers (Phase 6 — Prometheus metrics + health)
// ---------------------------------------------------------------------------

/// GET /api/health — Enhanced health check for operators / Prometheus
#[derive(Debug, Serialize)]
pub struct ApiHealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub ergo_node_connected: bool,
    pub inference_backend: String,
    pub models_loaded: Vec<String>,
}

async fn api_health_handler(State(state): State<AppState>) -> Json<ApiHealthResponse> {
    let health = state.node_health.read().await;
    let models = state.models_loaded.read().await;

    Json(ApiHealthResponse {
        status: if health.is_synced {
            "ok".to_string()
        } else {
            "degraded".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: state.start_time.elapsed().as_secs(),
        ergo_node_connected: health.is_synced,
        inference_backend: if !models.is_empty() {
            "ollama".to_string()
        } else {
            "none".to_string()
        },
        models_loaded: models.clone(),
    })
}

/// GET /api/metrics — Prometheus-compatible metrics endpoint
async fn api_metrics_handler(State(state): State<AppState>) -> String {
    let pown = state.pown_status.read().await;
    let total = pown.work_points;

    // Approximate component breakdown for Prometheus labels
    let health = state.node_health.read().await;
    let node_score = if health.is_synced { 30 } else { 0 };
    let network_score = pown.unique_xergon_peers_seen.min(10) as u64 * 10;
    let ai_score = if total > node_score + network_score {
        total - node_score - network_score
    } else {
        0
    };

    state
        .metrics
        .render_prometheus(total, node_score, network_score, ai_score)
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

// ---------------------------------------------------------------------------
// P2P Provider-to-Provider Communication handlers (Phase 5)
// ---------------------------------------------------------------------------

/// GET /api/peer/info -- Returns this agent's peer info (models, capacity, load).
async fn peer_info_handler(State(state): State<AppState>) -> Result<Json<crate::p2p::PeerAgentInfo>, Response> {
    let engine = match &state.p2p_engine {
        Some(e) => e,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "P2P communication not enabled")),
    };
    Ok(Json(engine.get_self_info().await))
}

/// GET /api/peer/models -- List models available on this peer.
async fn peer_models_handler(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    let engine = match &state.p2p_engine {
        Some(e) => e,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "P2P communication not enabled")),
    };
    let info = engine.get_self_info().await;
    Ok(Json(serde_json::json!({
        "provider_id": info.provider_id,
        "models": info.models,
        "load_factor": info.load_factor,
        "is_healthy": info.is_healthy,
    })))
}

/// POST /api/peer/model-notify -- Receive a model notification from a peer.
async fn peer_model_notify_handler(
    State(_state): State<AppState>,
    Json(req): Json<crate::p2p::ModelNotifyRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    info!(
        model = %req.model_name,
        provider_id = %req.provider_id,
        provider_endpoint = %req.provider_endpoint,
        "Received model notification from peer"
    );
    // For MVP: just log and acknowledge. Future: update peer cache with new model info.
    Ok(Json(serde_json::json!({
        "status": "acknowledged",
        "model": req.model_name,
        "provider_id": req.provider_id,
    })))
}

/// POST /api/peer/proxy-request -- Proxy an inference request to this provider (load balancing).
async fn peer_proxy_request_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::p2p::ProxyRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    let _engine = match &state.p2p_engine {
        Some(e) => e,
        None => return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", "P2P communication not enabled")),
    };

    // Check if this provider is not too loaded to handle the proxy request
    let load = _engine.get_load_factor().await;
    if load >= 0.9 {
        return Err(json_error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", &format!("Provider is overloaded (load_factor={:.2}), cannot accept proxy requests", load)));
    }

    // For MVP: return a 501 indicating the proxy endpoint is not yet wired to inference
    info!(
        target_endpoint = %req.target_endpoint,
        timeout_secs = req.timeout_secs,
        "Received proxy request from peer"
    );

    Err(json_error(StatusCode::NOT_IMPLEMENTED, "internal_error", "Proxy request forwarding not yet wired. Use relay for load balancing."))
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Pricing endpoints — GET / POST /xergon/pricing
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PricingResponse {
    pub default_price_per_1m_tokens: u64,
    pub models: std::collections::HashMap<String, u64>,
}

#[derive(Debug, Deserialize)]
pub struct PricingUpdateRequest {
    pub model: String,
    pub price: u64,
}

#[derive(Debug, Serialize)]
pub struct PricingUpdateResponse {
    pub ok: bool,
    pub default_price_per_1m_tokens: u64,
    pub models: std::collections::HashMap<String, u64>,
}

/// GET /xergon/pricing — Return current pricing configuration
async fn pricing_get_handler(State(state): State<AppState>) -> Json<PricingResponse> {
    let pricing = state.pricing.read().await;
    Json(PricingResponse {
        default_price_per_1m_tokens: pricing.default_price_per_1m_tokens,
        models: pricing.models.clone(),
    })
}

/// POST /xergon/pricing — Update a model price (writes to config file)
async fn pricing_post_handler(
    State(state): State<AppState>,
    Json(body): Json<PricingUpdateRequest>,
) -> Result<Json<PricingUpdateResponse>, Response> {
    let model = body.model.trim().to_string();
    if model.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "invalid_request", "model field is required"));
    }

    // Update in-memory config
    {
        let mut pricing = state.pricing.write().await;
        pricing.models.insert(model.clone(), body.price);
    }

    // Persist to config file
    let config_path = &state.config_path;
    if config_path.exists() {
        let write_result = tokio::task::spawn_blocking({
            let path = config_path.clone();
            let model = model.clone();
            let price = body.price;
            move || -> Result<(), String> {
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read config: {}", e))?;
                let mut doc = raw
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|e| format!("Failed to parse config: {}", e))?;

                // Ensure [pricing] table exists
                doc.entry("pricing").or_insert_with(toml_edit::table);
                if let Some(pricing) = doc.get_mut("pricing") {
                    if let Some(table) = pricing.as_table_mut() {
                        table.entry("models").or_insert_with(toml_edit::table);
                        if let Some(models) = table.get_mut("models") {
                            if let Some(models_table) = models.as_table_mut() {
                                models_table[&model] = toml_edit::value(price as i64);
                            }
                        }
                    }
                }

                std::fs::write(&path, doc.to_string())
                    .map_err(|e| format!("Failed to write config: {}", e))?;
                Ok(())
            }
        })
        .await;

        if let Err(e) = write_result {
            return Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", &format!("Failed to persist pricing: {:?}", e)));
        }
        if let Err(e) = write_result.unwrap() {
            return Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", &format!("Failed to persist pricing: {}", e)));
        }
    } else {
        tracing::warn!(
            path = %config_path.display(),
            "Config file not found, pricing updated in-memory only"
        );
    }

    let pricing = state.pricing.read().await;
    Ok(Json(PricingUpdateResponse {
        ok: true,
        default_price_per_1m_tokens: pricing.default_price_per_1m_tokens,
        models: pricing.models.clone(),
    }))
}

// ---------------------------------------------------------------------------
// Tests — W7 fixes: localhost_cors
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Method, Request};
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

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
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
    fn cors_service() -> impl tower::Service<
        Request<axum::body::Body>,
        Response = axum::response::Response,
        Error = std::convert::Infallible,
    > {
        ServiceBuilder::new()
            .layer(localhost_cors())
            .service(OkService)
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_localhost_9099() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("http://localhost:9099"))
            .await
            .unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://localhost:9099");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_127_0_0_1_9099() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("http://127.0.0.1:9099"))
            .await
            .unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://127.0.0.1:9099");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_localhost_3000() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("http://localhost:3000"))
            .await
            .unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://localhost:3000");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_127_0_0_1_3000() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("http://127.0.0.1:3000"))
            .await
            .unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://127.0.0.1:3000");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_localhost_arbitrary_port() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("http://localhost:5173"))
            .await
            .unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        // The predicate-based AllowOrigin should echo back the origin
        assert_eq!(origin, "http://localhost:5173");
    }

    #[tokio::test]
    async fn test_localhost_cors_accepts_127_0_0_1_arbitrary_port() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("http://127.0.0.1:8080"))
            .await
            .unwrap();
        let origin = resp.headers().get("access-control-allow-origin").unwrap();
        assert_eq!(origin, "http://127.0.0.1:8080");
    }

    #[tokio::test]
    async fn test_localhost_cors_rejects_foreign_origin() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("https://evil.example.com"))
            .await
            .unwrap();
        let allowed = resp
            .headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap_or(""));
        // A foreign origin should NOT get an allow-origin header echoing it back
        assert_ne!(allowed, Some("https://evil.example.com"));
    }

    #[tokio::test]
    async fn test_localhost_cors_rejects_http_subdomain() {
        let svc = cors_service();
        let resp = svc
            .oneshot(preflight_req_with_origin("http://malicious.localhost:3000"))
            .await
            .unwrap();
        let allowed = resp
            .headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap_or(""));
        // Subdomain of localhost should NOT be allowed
        assert_ne!(allowed, Some("http://malicious.localhost:3000"));
    }
}
