//! REST API for the Xergon agent
//!
//! Exposes endpoints for:
//! - `/xergon/status` — Status endpoint that other Xergon agents probe
//! - `/xergon/peers` — Current peer discovery state
//! - `/xergon/health` — Basic health check
//! - `/xergon/settlement` — Settlement engine status and history
//! - `/v1/governance/*` — Governance proposal lifecycle API

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, patch, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};

pub mod governance;
pub mod orchestration;

use crate::airdrop::AirdropService;
use crate::auth::AuthState;
use crate::audit::AuditConfig;
use crate::config::PricingConfig;
use crate::config::{AgentConfig, XergonConfig};
use crate::rate_limit::{RateLimitConfig, RateLimitState};
use crate::node_health::NodeHealthState;
use crate::peer_discovery::PeerDiscoveryState;
use crate::pown::PownStatus;
use crate::settlement::SettlementEngine;
use crate::content_safety::{
    safety_add_pattern_handler, safety_check_handler, safety_config_handler,
    safety_config_update_handler, safety_filter_handler, safety_patterns_handler,
    safety_remove_pattern_handler, safety_scan_handler, safety_stats_handler,
    safety_violations_handler,
};
use crate::download_progress::ProgressTracker;
use axum::response::sse::{Event, KeepAlive, Sse};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub xergon_config: XergonConfig,
    pub ergo_node_url: String,
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
    /// Model discovery service (Some if enabled)
    pub model_discovery: Option<Arc<crate::model_discovery::ModelDiscovery>>,
    /// Model cache service (Some if enabled)
    pub model_cache: Option<Arc<crate::model_cache::ModelCache>>,
    /// Usage proof rollup system (Some if enabled)
    pub rollup: Option<Arc<crate::rollup::UsageRollup>>,
    /// Prometheus metrics collector
    pub metrics: Arc<crate::metrics::MetricsCollector>,
    /// DashMap-backed label-aware metrics store (HTTP request tracking, etc.)
    pub metrics_store: Arc<crate::metrics::MetricsStore>,
    /// Loaded inference model names (for health endpoint)
    pub models_loaded: Arc<RwLock<Vec<String>>>,
    /// Pricing configuration (read from config file, mutable for updates)
    pub pricing: Arc<RwLock<PricingConfig>>,
    /// Oracle service for ERG/USD rate (Some if enabled)
    pub oracle: Option<Arc<crate::oracle_service::OracleService>>,
    /// Provider registry configuration (Some if enabled)
    pub provider_registry_config: Option<Arc<crate::config::ProviderRegistryConfig>>,
    /// Path to the config file (for writing pricing updates)
    pub config_path: std::path::PathBuf,
    /// Multi-agent orchestrator (shared across API handlers)
    pub orchestrator: Arc<crate::orchestration::Orchestrator>,
    /// Benchmark suite for model performance testing
    pub benchmark_suite: Option<Arc<crate::benchmark::BenchmarkSuite>>,
    /// Auto-heal system for provider health monitoring and recovery
    pub auto_healer: Option<Arc<crate::auto_heal::AutoHealer>>,
    /// Download progress tracker for active model pulls
    pub download_progress: Option<Arc<ProgressTracker>>,
    /// Marketplace sync engine (pushes provider info to relay for marketplace display)
    pub marketplace_sync: Option<Arc<crate::marketplace_sync::MarketplaceSync>>,
    /// Config hot-reloader (Some if enabled)
    pub config_reloader: Option<Arc<crate::config_reload::ConfigReloader>>,
    /// Model version registry
    pub model_registry: Arc<crate::model_versioning::ModelVersionRegistry>,
    /// Auto-scaling system for inference demand monitoring
    pub auto_scaler: Option<Arc<crate::auto_scale::AutoScaler>>,
    /// Reputation dashboard service
    pub reputation_dashboard: Option<Arc<crate::reputation_dashboard::ReputationDashboard>>,
    /// Priority inference queue (Some if enabled)
    pub inference_queue: Option<Arc<crate::inference_queue::InferenceQueue>>,
    /// Model health monitor (Some if enabled)
    pub model_health_monitor: Option<Arc<crate::model_health::ModelHealthMonitor>>,
    /// Provider mesh sync (Some if enabled)
    pub provider_mesh: Option<Arc<crate::provider_mesh::ProviderMesh>>,
    /// Fine-tune orchestration manager
    pub fine_tune: Arc<crate::fine_tune::FineTuneManager>,
    /// A/B testing manager
    pub ab_testing: Arc<crate::ab_testing::ABTestManager>,
    /// Multi-GPU inference manager
    pub multi_gpu: Arc<crate::multi_gpu::MultiGpuManager>,
    /// Container runtime manager
    pub container_runtime: Arc<crate::container::ContainerManager>,
    /// Model shard manager (split models across GPUs)
    pub model_shard_manager: Arc<crate::model_sharding::ModelShardManager>,
    /// Distributed inference manager (route to remote nodes)
    pub distributed_inference: Arc<crate::distributed_inference::DistributedInferenceManager>,
    pub tensor_pipeline: Arc<crate::tensor_pipeline::TensorPipelineManager>,
    /// Sandbox manager (inference isolation)
    pub sandbox_manager: Arc<crate::sandbox::SandboxManager>,
    /// Marketplace listing manager
    pub marketplace_listing: Arc<crate::marketplace_listing::MarketplaceListingManager>,
    /// Inference observability manager
    pub observability: Arc<crate::observability::ObservabilityManager>,
    /// Model compression manager
    pub compression: Arc<crate::model_compression::CompressionManager>,
    /// Inference response cache
    pub inference_cache: Arc<crate::inference_cache::InferenceCache>,
    /// GPU memory manager
    pub gpu_memory: Arc<crate::gpu_memory::GpuMemoryManager>,
    /// Model migration manager
    pub model_migration: Arc<crate::model_migration::ModelMigrationManager>,
    /// Model warm-up pool
    pub warmup_pool: Arc<crate::warmup::WarmupPool>,
    /// Inference batcher (aggregates requests for same model)
    pub inference_batcher: Arc<crate::inference_batch::InferenceBatcher>,
    /// Checkpoint manager (model state snapshots)
    pub checkpoint_manager: Arc<crate::checkpoint::CheckpointManager>,
    /// Resource quota manager (per-user/API-key limits)
    pub quota_manager: Arc<crate::resource_quotas::ResourceQuotaManager>,
    /// Inference profiler
    pub profiler: Arc<crate::inference_profiler::InferenceProfiler>,
    /// GPU-aware scheduler
    pub gpu_scheduler: Arc<crate::gpu_scheduler::GpuScheduler>,
    /// Model artifact storage
    pub artifact_storage: Arc<crate::artifact_storage::ArtifactStorage>,
    /// Content safety filter for inference output safety
    pub content_safety: Arc<tokio::sync::RwLock<crate::content_safety::ContentSafetyFilter>>,
    /// Enhanced quantization v2 manager
    pub quantization_v2: Arc<crate::quantization_v2::QuantizationV2Manager>,
    /// Priority queue manager
    pub priority_queue: Arc<crate::priority_queue::PriorityQueueManager>,
    /// Model snapshot manager
    pub model_snapshot: Arc<crate::model_snapshot::SnapshotManager>,
    /// Alignment training manager
    pub alignment_trainer: Arc<crate::alignment_training::AlignmentTrainer>,
    /// Model serving manager
    pub model_serve_manager: Arc<crate::model_serving::ModelServeManager>,
    /// Dynamic batching engine
    pub dynamic_batcher: Arc<crate::dynamic_batcher::DynamicBatcher>,
    /// A/B testing v2 manager
    pub ab_testing_v2: Arc<crate::ab_testing_v2::ABTestV2Manager>,
    /// Federated learning coordinator (Some if enabled)
    pub federated_learning: Option<Arc<crate::federated_learning::FederatedState>>,
    /// Extended model registry with automated rollback
    pub extended_model_registry: Option<Arc<crate::model_registry::ModelRegistry>>,
    /// Model optimizer (neural arch search, quantization, latency tuning)
    pub model_optimizer: Option<Arc<crate::model_optimizer::ModelOptimizer>>,
    pub federated_training: Option<Arc<crate::federated_training::FederatedTrainingEngine>>,
    /// E2E integration test suite
    pub e2e_suite: Arc<crate::e2e_integration::E2ETestSuite>,
    /// Self-healing circuit breaker
    pub circuit_breaker: Arc<crate::self_healing_circuit_breaker::SelfHealingCircuitBreaker>,
    /// Model drift detector
    pub model_drift_detector: Arc<crate::model_drift::ModelDriftDetector>,
    /// Inference observability (distributed tracing)
    pub inference_observability: Arc<crate::inference_observability::InferenceObservability>,
    /// Model lineage graph
    pub lineage_graph: Arc<crate::model_lineage_graph::LineageGraph>,
    /// Model hash chain (immutable, append-only artifact ledger)
    pub model_hash_chain: Arc<crate::model_hash_chain::ModelHashChain>,
    /// Prompt versioning manager
    pub prompt_versioning: Arc<crate::prompt_versioning::PromptVersionManager>,
    /// Inference sandbox manager
    pub inference_sandbox: Arc<crate::inference_sandbox::InferenceSandbox>,
    /// Model access control (RBAC) manager
    pub model_access_control: Arc<crate::model_access_control::ModelAccessControl>,
    /// Audit log aggregator
    pub audit_aggregator: Arc<crate::audit_log_aggregator::AuditLogAggregator>,
    /// Feature flag service
    pub feature_flags: Arc<crate::feature_flags::FeatureFlagService>,
    /// Experiment framework
    pub experiments: Arc<crate::experiment_framework::ExperimentFramework>,
    /// Inference gateway
    pub inference_gateway: Arc<crate::inference_gateway::InferenceGateway>,
    /// EIP-23 oracle feed service (multi-source ERG price aggregation)
    pub oracle_feeds: Arc<crate::ergo_oracle_feeds::ErgoOracleService>,
    /// ERG-denominated cost accounting service
    pub cost_accountant: Arc<crate::ergo_cost_accounting::ErgoCostAccountant>,
    /// SigmaUSD stablecoin pricing service
    pub sigma_usd_pricer: Arc<crate::sigma_usd_pricing::SigmaUsdPricer>,
    /// Provider lifecycle manager (on-chain box registration, heartbeat, rent protection)
    pub lifecycle_manager: Option<Arc<crate::provider_lifecycle::ProviderLifecycleState>>,
    /// Chaos testing engine for resilience verification
    pub chaos_engine: Arc<crate::chaos_testing::ChaosEngine>,
    /// Sigma proof builder for ZK proof construction
    pub sigma_proof_builder: Option<Arc<crate::sigma_proof_builder::SigmaProofBuilderState>>,
    /// Token operations (EIP-4 mint/burn/transfer)
    pub token_operations: Option<Arc<crate::token_operations::TokenOperationsState>>,
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
           axum::http::Method::PUT,
           axum::http::Method::DELETE,
       ])
       .allow_headers([
           axum::http::header::AUTHORIZATION,
           axum::http::header::CONTENT_TYPE,
           axum::http::header::ACCEPT,
           axum::http::header::ACCESS_CONTROL_REQUEST_HEADERS,
           axum::http::header::ACCESS_CONTROL_REQUEST_METHOD,
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
    build_router_inner(state, None, None, None)
}

/// Build the router with rate limiting and audit trail middleware.
pub fn build_router_with_middleware(
    state: AppState,
    rate_limit_config: Option<RateLimitConfig>,
    audit_config: Option<AuditConfig>,
) -> Router {
    build_router_inner(state, rate_limit_config, audit_config, None)
}

/// Internal router builder shared by all variants.
///
/// Returns Router<()> — all routes have their state extracted via `.with_state()`
/// so they can be merged into a single stateless router.
fn build_router_inner(
    state: AppState,
    rate_limit_config: Option<RateLimitConfig>,
    audit_config: Option<AuditConfig>,
    inference_state: Option<crate::inference::InferenceState>,
) -> Router<()> {
    let cors = localhost_cors();

    let xergon_routes = Router::new()
        .route("/xergon/status", get(status_handler))
        .route("/xergon/peers", get(peers_handler))
        .route("/xergon/health", get(health_handler))
        .route("/xergon/settlement", get(settlement_handler))
        .route("/api/settlement/execute", post(settlement_execute_handler))
        .route("/api/settlement/boxes", get(settlement_boxes_handler))
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

    // Inference routes (if provided)
    let inference_routes = if let Some(istate) = inference_state {
        let r = crate::inference::build_router(istate);
        Some(r)
    } else {
        None
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

    // Monitoring routes (public -- no auth required)
    let monitoring_routes = Router::new()
        .route("/api/health", get(api_health_handler))
        .route("/api/health/deep", get(api_health_deep_handler))
        .route("/api/metrics", get(api_metrics_handler))
        .route("/metrics", get(prometheus_metrics_handler))
        .route("/metrics/json", get(metrics_json_handler))
        .route("/api/oracle/rate", get(oracle_rate_handler))
        .with_state(state.clone());

    // Model discovery routes (public)
    let discovery_routes = Router::new()
        .route("/api/discovery/models", get(discovery_models_handler))
        .route("/api/discovery/recommended", get(discovery_recommended_handler))
        .route("/api/discovery/scan", post(discovery_scan_handler))
        .with_state(state.clone());

    // Model cache routes (public)
    let cache_routes = Router::new()
        .route("/api/cache/stats", get(cache_stats_handler))
        .route("/api/cache/models", get(cache_models_handler))
        .route("/api/cache/models/{model_id}", delete(cache_evict_handler))
        .route("/api/cache/models/{model_id}/pin", post(cache_pin_handler))
        .with_state(state.clone());

    // Benchmark and auto-heal routes (management)
    let benchmark_routes = Router::new()
        .route("/api/benchmark/run", post(benchmark_run_handler))
        .route("/api/benchmark/results", get(benchmark_results_handler))
        .route("/api/benchmark/history/{model}", get(benchmark_history_handler))
        .with_state(state.clone());

    let auto_heal_routes = Router::new()
        .route("/api/auto-heal/check", post(auto_heal_check_handler))
        .route("/api/auto-heal/status", get(auto_heal_status_handler))
        .route("/api/auto-heal/config", get(auto_heal_config_handler))
        .with_state(state.clone());

    // Download progress routes (public -- for monitoring model pulls)
    let progress_routes = Router::new()
        .route("/api/models/pull/progress", get(pull_progress_list_handler))
        .route("/api/models/pull/progress/{model}", get(pull_progress_handler))
        .route("/api/models/pull/progress/{model}/stream", get(pull_progress_stream_handler))
        .route("/api/models/pull/cancel/{model}", post(pull_cancel_handler))
        .with_state(state.clone());

    // Provider on-chain registration routes
    let provider_registry_routes = Router::new()
        .route("/api/providers/register", post(provider_register_handler))
        .route("/api/providers/on-chain", get(provider_on_chain_list_handler))
        .route("/api/providers/{box_id}/update", put(provider_update_handler))
        .with_state(state.clone());

    // Marketplace sync routes
    let marketplace_sync_routes = Router::new()
        .route("/api/marketplace/sync", post(marketplace_sync_trigger_handler))
        .route("/api/marketplace/sync/status", get(marketplace_sync_status_handler))
        .route("/api/marketplace/sync/config", get(marketplace_sync_config_handler).patch(marketplace_sync_config_update_handler))
        .with_state(state.clone());

    // Config hot-reload routes
    let config_reload_routes = Router::new()
        .route("/api/config/reload", post(config_reload_handler))
        .route("/api/config/reload/status", get(config_reload_status_handler))
        .with_state(state.clone());

    // Model versioning routes
    let model_version_routes = Router::new()
        .route("/api/models/versions", get(model_versions_list_all_handler))
        .route("/api/models/versions/{model}", get(model_versions_list_handler))
        .route("/api/models/versions/{model}/{version}", get(model_versions_get_handler))
        .route("/api/models/versions/{model}/{version}/activate", post(model_versions_activate_handler))
        .route("/api/models/versions/{model}/{version}/tag", post(model_versions_set_tag_handler))
        .route("/api/models/versions/{model}/{version}", delete(model_versions_delete_handler))
        .with_state(state.clone());

    // Auto-scaling routes
    let auto_scale_routes = Router::new()
        .route("/api/auto-scale/status", get(auto_scale_status_handler))
        .route("/api/auto-scale/trigger", post(auto_scale_trigger_handler))
        .route("/api/auto-scale/config", get(auto_scale_config_handler).patch(auto_scale_config_update_handler))
        .with_state(state.clone());

    // Reputation dashboard routes
    let reputation_routes = Router::new()
        .route("/api/reputation/leaderboard", get(reputation_leaderboard_handler))
        .route("/api/reputation/provider/{pk}", get(reputation_provider_handler))
        .route("/api/reputation/stats", get(reputation_stats_handler))
        .route("/api/reputation/history/{pk}", get(reputation_history_handler))
        .with_state(state.clone());

    // Inference queue routes
    let queue_routes = Router::new()
        .route("/api/queue/status", get(queue_status_handler))
        .route("/api/queue/stats", get(queue_stats_handler))
        .route("/api/queue/clear", post(queue_clear_handler))
        .with_state(state.clone());

    // Model health routes
    let model_health_routes = Router::new()
        .route("/api/models/health", get(models_health_handler))
        .route("/api/models/health/{model}", get(models_health_single_handler))
        .route("/api/models/health/{model}/check", post(models_health_check_handler))
        .with_state(state.clone());

    // Provider mesh routes
    let mesh_routes = Router::new()
        .route("/api/mesh/status", get(mesh_status_handler))
        .route("/api/mesh/peers", get(mesh_peers_handler))
        .route("/api/mesh/sync", post(mesh_sync_handler))
        .route("/api/mesh/models", get(mesh_models_handler))
        .with_state(state.clone());

    // Fine-tune orchestration routes
    let fine_tune_routes = Router::new()
        .route("/api/fine-tune/create", post(fine_tune_create_handler))
        .route("/api/fine-tune/jobs", get(fine_tune_jobs_handler))
        .route("/api/fine-tune/jobs/{id}", get(fine_tune_job_handler))
        .route("/api/fine-tune/jobs/{id}/cancel", post(fine_tune_cancel_handler))
        .route("/api/fine-tune/jobs/{id}/export", post(fine_tune_export_handler))
        .with_state(state.clone());

    // A/B testing routes
    let experiment_routes = Router::new()
        .route("/api/experiments/create", post(experiment_create_handler))
        .route("/api/experiments", get(experiment_list_handler))
        .route("/api/experiments/{id}", get(experiment_get_handler))
        .route("/api/experiments/{id}/pause", post(experiment_pause_handler))
        .route("/api/experiments/{id}/resume", post(experiment_resume_handler))
        .route("/api/experiments/{id}/end", post(experiment_end_handler))
        .route("/api/experiments/{id}/feedback", post(experiment_feedback_handler))
        .with_state(state.clone());

    // Multi-GPU management routes
    let multi_gpu_routes = Router::new()
        .route("/api/gpu/devices", get(gpu_devices_handler))
        .route("/api/gpu/multi/config", get(gpu_config_handler).patch(gpu_config_update_handler))
        .route("/api/gpu/usage", get(gpu_usage_handler))
        .with_state(state.clone());

    // Container runtime routes
    let container_routes = Router::new()
        .route("/api/containers/create", post(container_create_handler))
        .route("/api/containers", get(container_list_handler))
        .route("/api/containers/{id}", get(container_get_handler))
        .route("/api/containers/{id}/stop", post(container_stop_handler))
        .route("/api/containers/{id}/start", post(container_start_handler))
        .route("/api/containers/{id}/logs", get(container_logs_handler))
        .with_state(state.clone());

    // Model sharding routes
    let sharding_routes = Router::new()
        .route("/api/sharding/status", get(sharding_status_handler))
        .route("/api/sharding/shard", post(sharding_shard_handler))
        .route("/api/sharding/shard/{model}", delete(sharding_unshard_handler))
        .route("/api/sharding/models", get(sharding_models_handler))
        .with_state(state.clone());

    // Distributed inference routes
    let distributed_routes = Router::new()
        .route("/api/distributed/nodes", get(distributed_nodes_handler))
        .route("/api/distributed/status", get(distributed_status_handler))
        .route("/api/distributed/forward", post(distributed_forward_handler))
        .route("/api/distributed/metrics", get(distributed_metrics_handler))
        .with_state(state.clone());

    // Tensor pipeline routes
    let tensor_pipeline_routes = Router::new()
        .route("/api/tensor-pipeline/status", get(tensor_pipeline_status_handler))
        .route("/api/tensor-pipeline/config", get(tensor_pipeline_config_handler).post(tensor_pipeline_config_update_handler))
        .route("/api/tensor-pipeline/stats", get(tensor_pipeline_stats_handler))
        .route("/api/tensor-pipeline/execute/{id}", post(tensor_pipeline_execute_handler))
        .route("/api/tensor-pipeline/pipelines", get(tensor_pipeline_list_handler))
        .route("/api/tensor-pipeline/pipelines/{id}", get(tensor_pipeline_get_handler).delete(tensor_pipeline_delete_handler))
        .route("/api/tensor-pipeline/pipelines/{id}/pause", post(tensor_pipeline_pause_handler))
        .route("/api/tensor-pipeline/pipelines/{id}/resume", post(tensor_pipeline_resume_handler))
        .with_state(state.clone());

    // Sandbox routes
    let sandbox_routes = Router::new()
        .route("/api/sandbox/status", get(sandbox_status_handler))
        .route("/api/sandbox/config", patch(sandbox_config_handler))
        .route("/api/sandbox/metrics", get(sandbox_metrics_handler))
        .route("/api/sandbox/test", post(sandbox_test_handler))
        .with_state(state.clone());

    // Marketplace listing routes
    let marketplace_listing_routes = Router::new()
        .route("/api/marketplace/listings", post(marketplace_create_listing_handler))
        .route("/api/marketplace/listings", get(marketplace_list_listings_handler))
        .route("/api/marketplace/listings/{id}", get(marketplace_get_listing_handler))
        .route("/api/marketplace/listings/{id}", patch(marketplace_update_listing_handler))
        .route("/api/marketplace/listings/{id}", delete(marketplace_delete_listing_handler))
        .route("/api/marketplace/listings/{id}/publish", post(marketplace_publish_listing_handler))
        .route("/api/marketplace/listings/{id}/deprecate", post(marketplace_deprecate_listing_handler))
        .with_state(state.clone());

    // Observability routes (inference tracing + metrics)
    let observability_routes = Router::new()
        .route("/api/observability/traces", get(observability_traces_handler))
        .route("/api/observability/traces/{trace_id}", get(observability_trace_handler))
        .route("/api/observability/metrics", get(observability_metrics_handler))
        .route("/api/observability/config", get(observability_config_handler).patch(observability_config_update_handler))
        .with_state(state.clone());

    // Model compression routes
    let compression_routes = Router::new()
        .route("/api/compression/create", post(compression_create_handler))
        .route("/api/compression/jobs", get(compression_jobs_handler))
        .route("/api/compression/jobs/{id}", get(compression_job_handler))
        .route("/api/compression/jobs/{id}/cancel", post(compression_cancel_handler))
        .route("/api/compression/jobs/{id}", delete(compression_delete_handler))
        .route("/api/compression/estimate", post(compression_estimate_handler))
        .with_state(state.clone());

    // Inference cache routes
    let inference_cache_routes = Router::new()
        .route("/api/inference-cache/stats", get(inference_cache_stats_handler))
        .route("/api/inference-cache/clear", delete(inference_cache_clear_handler))
        .route("/api/inference-cache/entries", get(inference_cache_entries_handler))
        .route("/api/inference-cache/entries/{id}", delete(inference_cache_evict_handler))
        .route("/api/inference-cache/config", patch(inference_cache_config_handler))
        .with_state(state.clone());

    // GPU memory management routes
    let gpu_memory_routes = Router::new()
        .route("/api/gpu-memory/devices", get(gpu_memory_devices_handler))
        .route("/api/gpu-memory/allocations", get(gpu_memory_allocations_handler))
        .route("/api/gpu-memory/available", get(gpu_memory_available_handler))
        .route("/api/gpu-memory/allocate", post(gpu_memory_allocate_handler))
        .route("/api/gpu-memory/allocate/{region_id}", delete(gpu_memory_deallocate_handler))
        .route("/api/gpu-memory/fragmentation", get(gpu_memory_fragmentation_handler))
        .route("/api/gpu-memory/defrag", post(gpu_memory_defrag_handler))
        .with_state(state.clone());

    // Model warm-up pool routes
    let warmup_routes = Router::new()
        .route("/api/warmup/status", get(warmup_status_handler))
        .route("/api/warmup/load", post(warmup_load_handler))
        .route("/api/warmup/unload", post(warmup_unload_handler))
        .route("/api/warmup/config", patch(warmup_config_handler))
        .route("/api/warmup/stats", get(warmup_stats_handler))
        .with_state(state.clone());

    // Model migration routes
    let migration_routes = Router::new()
        .route("/api/migration/create", post(migration_create_handler))
        .route("/api/migration/jobs", get(migration_jobs_handler))
        .route("/api/migration/jobs/{id}", get(migration_job_handler))
        .route("/api/migration/jobs/{id}/pause", post(migration_pause_handler))
        .route("/api/migration/jobs/{id}/resume", post(migration_resume_handler))
        .route("/api/migration/jobs/{id}/cancel", post(migration_cancel_handler))
        .route("/api/migration/validate", post(migration_validate_handler))
        .with_state(state.clone());

    // Inference batching routes
    let batch_routes = Router::new()
        .route("/api/batch/stats", get(batch_stats_handler))
        .route("/api/batch/config", get(batch_config_handler).patch(batch_config_update_handler))
        .route("/api/batch/flush", post(batch_flush_handler))
        .with_state(state.clone());

    // Checkpoint management routes
    let checkpoint_routes = Router::new()
        .route("/api/checkpoint/create", post(checkpoint_create_handler))
        .route("/api/checkpoint/list", get(checkpoint_list_handler))
        .route("/api/checkpoint/{id}", get(checkpoint_get_handler).delete(checkpoint_delete_handler))
        .route("/api/checkpoint/{id}/restore", post(checkpoint_restore_handler))
        .route("/api/checkpoint/{id}/compare", post(checkpoint_compare_handler))
        .route("/api/checkpoint/config", get(checkpoint_config_handler).patch(checkpoint_config_update_handler))
        .with_state(state.clone());

    // Resource quota routes
    let quota_routes = Router::new()
        .route("/api/quotas", get(quotas_list_handler))
        .route("/api/quotas/config", get(quotas_config_handler).patch(quotas_config_update_handler))
        .route("/api/quotas/alerts", get(quotas_alerts_handler))
        .route("/api/quotas/{subject_id}", get(quotas_get_handler).put(quotas_set_handler))
        .route("/api/quotas/{subject_id}/usage", get(quotas_usage_handler))
        .route("/api/quotas/{subject_id}/reset", post(quotas_reset_handler))
        .with_state(state.clone());

    // Inference profiler routes
    let profiler_routes = Router::new()
        .route("/api/profiler/stats", get(profiler_stats_handler))
        .route("/api/profiler/profiles", get(profiler_list_handler).delete(profiler_clear_handler))
        .route("/api/profiler/profiles/{id}", get(profiler_get_handler))
        .route("/api/profiler/models/{model}/summary", get(profiler_model_summary_handler))
        .route("/api/profiler/compare", get(profiler_compare_handler))
        .route("/api/profiler/collect", post(profiler_collect_handler))
        .route("/api/profiler/config", patch(profiler_config_handler))
        .with_state(state.clone());

    // GPU scheduler routes
    let gpu_scheduler_routes = Router::new()
        .route("/api/gpu-scheduler/status", get(gpu_scheduler_status_handler))
        .route("/api/gpu-scheduler/devices", get(gpu_scheduler_devices_handler))
        .route("/api/gpu-scheduler/queue", get(gpu_scheduler_queue_handler))
        .route("/api/gpu-scheduler/affinity", post(gpu_scheduler_set_affinity_handler))
        .route("/api/gpu-scheduler/affinity/{model}", delete(gpu_scheduler_clear_affinity_handler))
        .route("/api/gpu-scheduler/stats", get(gpu_scheduler_stats_handler))
        .route("/api/gpu-scheduler/config", patch(gpu_scheduler_config_handler))
        .with_state(state.clone());

    // Artifact storage routes
    let artifact_routes = Router::new()
        .route("/api/artifacts/upload", post(artifact_upload_handler))
        .route("/api/artifacts", get(artifact_list_handler))
        .route("/api/artifacts/{id}", get(artifact_get_handler).delete(artifact_delete_handler))
        .route("/api/artifacts/{id}/download", get(artifact_download_handler))
        .route("/api/artifacts/{id}/verify", get(artifact_verify_handler))
        .route("/api/artifacts/stats", get(artifact_stats_handler))
        .route("/api/artifacts/config", patch(artifact_config_handler))
        .route("/api/artifacts/cleanup", post(artifact_cleanup_handler))
        .with_state(state.clone());

    // Contracts API routes (SDK-facing: /v1/contracts/*)
    // These endpoints are called by the TypeScript SDK for on-chain operations.
    let contracts_routes = Router::new()
        // Provider endpoints
        .route("/v1/contracts/provider/register", post(contracts_provider_register_handler))
        .route("/v1/contracts/provider/status", get(contracts_provider_status_handler))
        .route("/v1/contracts/providers", get(contracts_providers_list_handler))
        // Staking endpoints
        .route("/v1/contracts/staking/create", post(contracts_staking_create_handler))
        .route("/v1/contracts/staking/balance/{user_pk}", get(contracts_staking_balance_handler))
        .route("/v1/contracts/staking/boxes/{user_pk}", get(contracts_staking_boxes_handler))
        // Settlement endpoints
        .route("/v1/contracts/settlement/build", post(contracts_settlement_build_handler))
        .route("/v1/contracts/settlement/settleable", get(contracts_settlement_boxes_handler))
        // Oracle endpoints
        .route("/v1/contracts/oracle/rate", get(contracts_oracle_rate_handler))
        .route("/v1/contracts/oracle/status", get(contracts_oracle_status_handler))
        // Governance endpoints
        .route("/v1/contracts/governance/proposal", post(contracts_governance_proposal_handler))
        .route("/v1/contracts/governance/vote", post(contracts_governance_vote_handler))
        .route("/v1/contracts/governance/proposals", get(contracts_governance_proposals_handler))
        .with_state(state.clone());

    // Governance lifecycle API routes (SDK-facing: /v1/governance/*)
    let governance_routes = governance::build_governance_router();

    // Orchestration API routes (multi-agent task scheduling)
    let orchestration_routes = orchestration::build_orchestration_router(state.orchestrator.clone());

    // ErgoAuth routes (JWT-based authentication)
    let auth_state = AuthState {
        jwt_secret: Arc::new(if state.management_api_key.is_empty() {
            // Derive a JWT secret from the provider_id if no API key is set
            crate::wallet::blake2b256(state.xergon_config.provider_id.as_bytes())
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect()
        } else {
            state.management_api_key.clone()
        }),
        challenges: Arc::new(crate::auth::ChallengeStore::new()),
        ergo_node_url: state.ergo_node_url.clone(),
        provider_registry_config: state.provider_registry_config.clone(),
    };

    // Start the challenge cleanup background task
    crate::auth::ChallengeStore::start_cleanup_task(auth_state.challenges.clone());

    let ergoauth_routes = Router::new()
        .route("/v1/auth/ergoauth/challenge", post(ergoauth_challenge_handler))
        .route("/v1/auth/ergoauth/verify", post(ergoauth_verify_handler))
        .with_state(auth_state.clone());

    // Alignment training routes
    let alignment_routes = crate::alignment_training::build_alignment_router(state.clone());

    // Content safety filter routes
    let safety_routes = Router::new()
        .route("/api/safety/config", get(safety_config_handler).patch(safety_config_update_handler))
        .route("/api/safety/check", post(safety_check_handler))
        .route("/api/safety/filter", post(safety_filter_handler))
        .route("/api/safety/stats", get(safety_stats_handler))
        .route("/api/safety/violations", get(safety_violations_handler))
        .route("/api/safety/patterns", post(safety_add_pattern_handler).get(safety_patterns_handler))
        .route("/api/safety/patterns/{name}", delete(safety_remove_pattern_handler))
        .route("/api/safety/scan", post(safety_scan_handler))
        .with_state(state.clone());

    // Quantization v2 routes
    let quant_v2_routes = Router::new()
        .route("/api/quantize", post(quantize_start_handler).get(quantize_list_handler))
        .route("/api/quantize/{id}", get(quantize_get_handler).delete(quantize_delete_handler))
        .route("/api/quantize/{id}/cancel", post(quantize_cancel_handler))
        .route("/api/quantize/{id}/result", get(quantize_result_handler))
        .route("/api/quantize/{id}/layers", get(quantize_layers_handler))
        .route("/api/quantize/estimate", post(quantize_estimate_handler))
        .route("/api/quantize/methods", get(quantize_methods_handler))
        .route("/api/quantize/compare", get(quantize_compare_handler))
        .route("/api/quantize/verify", post(quantize_verify_handler))
        .route("/api/quantize/history", get(quantize_history_handler))
        .route("/api/quantize/config", patch(quantize_config_handler))
        .with_state(state.clone());

    // Priority queue routes
    let priority_queue_routes = Router::new()
        .route("/api/priority-queue/status", get(priority_queue_status_handler))
        .route("/api/priority-queue/tasks", get(priority_queue_tasks_handler))
        .route("/api/priority-queue/enqueue", post(priority_queue_enqueue_handler))
        .route("/api/priority-queue/{id}", delete(priority_queue_remove_handler))
        .route("/api/priority-queue/stats", get(priority_queue_stats_handler))
        .route("/api/priority-queue/config", patch(priority_queue_config_handler))
        .route("/api/priority-queue/clear", post(priority_queue_clear_handler))
        .with_state(state.clone());

    // Model snapshot routes
    let snapshot_routes = Router::new()
        .route("/api/snapshots", post(snapshot_create_handler).get(snapshot_list_handler))
        .route("/api/snapshots/{id}", get(snapshot_get_handler).delete(snapshot_delete_handler))
        .route("/api/snapshots/{id}/restore", post(snapshot_restore_handler))
        .route("/api/snapshots/{id}/compare/{other_id}", get(snapshot_compare_handler))
        .route("/api/snapshots/{id}/verify", post(snapshot_verify_handler))
        .route("/api/snapshots/stats", get(snapshot_stats_handler))
        .route("/api/snapshots/config", patch(snapshot_config_handler))
        .with_state(state.clone());

    let mut router = Router::new()
        .merge(xergon_routes)
        .merge(airdrop_routes)
        .merge(gpu_routes)
        .merge(p2p_routes)
        .merge(monitoring_routes)
        .merge(discovery_routes)
        .merge(cache_routes)
        .merge(benchmark_routes)
        .merge(auto_heal_routes)
        .merge(progress_routes)
        .merge(provider_registry_routes)
        .merge(marketplace_sync_routes)
        .merge(config_reload_routes)
        .merge(model_version_routes)
        .merge(contracts_routes)
        .merge(governance_routes)
        .merge(orchestration_routes)
        .merge(auto_scale_routes)
        .merge(reputation_routes)
        .merge(queue_routes)
        .merge(model_health_routes)
        .merge(mesh_routes)
        .merge(fine_tune_routes)
        .merge(experiment_routes)
        .merge(multi_gpu_routes)
        .merge(container_routes)
        .merge(sharding_routes)
        .merge(distributed_routes)
        .merge(tensor_pipeline_routes)
        .merge(sandbox_routes)
        .merge(marketplace_listing_routes)
        .merge(observability_routes)
        .merge(compression_routes)
        .merge(inference_cache_routes)
        .merge(gpu_memory_routes)
        .merge(warmup_routes)
        .merge(migration_routes)
        .merge(batch_routes)
        .merge(checkpoint_routes)
        .merge(quota_routes)
        .merge(profiler_routes)
        .merge(gpu_scheduler_routes)
        .merge(artifact_routes)
        .merge(safety_routes)
        .merge(quant_v2_routes)
        .merge(priority_queue_routes)
        .merge(snapshot_routes)
        .merge(ergoauth_routes)
        .merge(alignment_routes)
        .merge(crate::federated_learning::build_federated_router(state.clone()))
        .merge(crate::model_serving::build_serving_router(state.clone()))
        .merge(crate::dynamic_batcher::build_batching_router(state.clone()))
        .merge(crate::ab_testing_v2::build_abv2_router(state.clone()))
        .merge(crate::model_registry::build_model_registry_router(state.clone()))
        .merge(crate::model_optimizer::build_model_optimizer_router(state.clone()))
        .merge(crate::federated_training::build_federated_training_router(state.clone()))
        .merge(crate::e2e_integration::build_e2e_router(state.clone()))
        .merge(crate::self_healing_circuit_breaker::build_circuit_breaker_router(state.clone()))
        .merge(crate::model_drift::build_drift_router(state.clone()))
        .merge(crate::inference_observability::build_inference_observability_router(state.clone()))
        .merge(crate::model_lineage_graph::build_lineage_router(state.clone()))
        .merge(crate::model_hash_chain::build_hash_chain_router(state.model_hash_chain.clone()))
        .merge(crate::prompt_versioning::build_prompt_versioning_router(state.clone()))
        .merge(crate::inference_sandbox::build_inference_sandbox_router(state.clone()))
        .merge(crate::model_access_control::build_rbac_router(state.clone()))
        .merge(crate::audit_log_aggregator::build_audit_router(state.clone()))
        .merge(crate::feature_flags::build_feature_flags_router(state.clone()))
        .merge(crate::experiment_framework::build_experiment_router(state.clone()))
        .merge(crate::inference_gateway::build_gateway_router(state.clone()))
        .merge(crate::ergo_oracle_feeds::build_oracle_feeds_router(state.clone()))
        .merge(crate::ergo_cost_accounting::build_cost_accounting_router(state.clone()))
        .merge(crate::sigma_usd_pricing::build_sigma_usd_pricing_router(state.clone()))
        .merge(crate::provider_lifecycle::build_lifecycle_router(state.clone()).with_state(state.clone()))
        .merge(crate::sigma_proof_builder::build_router(state.clone()))
        .merge(crate::token_operations::build_router(state.clone()));

    // Merge inference routes if provided
    if let Some(inf_routes) = inference_routes {
        router = router.merge(inf_routes);
    }

    // Apply audit layer (innermost — runs after auth, close to handlers)
    if let Some(audit_cfg) = audit_config {
        info!("Audit logging middleware enabled");
        let logger = crate::audit::AuditLogger::new(audit_cfg);
        router = router.layer(middleware::from_fn_with_state(logger, crate::audit::audit_middleware));
    }

    // Apply metrics layer (tracks HTTP request count, duration, active requests)
    {
        info!("Metrics middleware enabled");
        let metrics_store = state.metrics_store.clone();
        let metrics_collector = state.metrics.clone();
        router = router.layer(middleware::from_fn(
            move |req, next| {
                metrics_middleware(req, next, metrics_store.clone(), metrics_collector.clone())
            },
        ));
    }

    // Apply rate limit layer (outermost — runs before auth)
    if let Some(rl_cfg) = rate_limit_config {
        let rl_state = RateLimitState::new(rl_cfg);
        if rl_state.is_enabled() {
            info!(
                ip_rpm = rl_state.config.ip_rpm,
                ip_burst = rl_state.config.ip_burst,
                key_rpm = rl_state.config.key_rpm,
                key_burst = rl_state.config.key_burst,
                "Rate limiting middleware enabled"
            );
            // Start background cleanup task
            let cleanup_state = std::sync::Arc::new(rl_state.clone());
            RateLimitState::start_cleanup_task(cleanup_state);
            router = router.layer(middleware::from_fn_with_state(rl_state, crate::rate_limit::rate_limit_middleware));
        }
    }

    router.layer(cors)
}

/// Build the full router including inference proxy routes.
/// Called from main.rs when inference is enabled.
///
/// Uses Router<()> so we can merge routes with different state types.
pub fn build_router_with_inference(
    state: AppState,
    inference_state: crate::inference::InferenceState,
) -> Router<()> {
    build_router_inner(state, None, None, Some(inference_state))
}

/// Build the router with inference, rate limiting, and audit trail middleware.
pub fn build_router_with_inference_and_middleware(
    state: AppState,
    inference_state: crate::inference::InferenceState,
    rate_limit_config: Option<RateLimitConfig>,
    audit_config: Option<AuditConfig>,
) -> Router<()> {
    build_router_inner(state, rate_limit_config, audit_config, Some(inference_state))
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
    let rate_limit_config = Some(config.rate_limit.clone());
    let audit_config = Some(config.audit.clone());
    let router = build_router_with_middleware(state, rate_limit_config, audit_config);
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
    let rate_limit_config = Some(config.rate_limit.clone());
    let audit_config = Some(config.audit.clone());
    let router = build_router_with_inference_and_middleware(
        state, inference_state, rate_limit_config, audit_config,
    );
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
        &client, &config.rental_tree_hex, &req.listing_box_id, req.hours, &req.renter_pk_hex,
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
        &req.rater_pk,
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
// Metrics middleware (Phase 33 — auto-tracks HTTP request count/duration)
// ---------------------------------------------------------------------------

/// Middleware that automatically tracks HTTP request metrics:
/// - `xergon_agent_http_requests_total` (counter by method, path, status)
/// - `xergon_agent_http_request_duration_seconds` (histogram by method, path)
/// - `xergon_agent_active_requests` (gauge)
async fn metrics_middleware(
    req: Request<Body>,
    next: Next,
    metrics_store: std::sync::Arc<crate::metrics::MetricsStore>,
    metrics_collector: std::sync::Arc<crate::metrics::MetricsCollector>,
) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Increment active requests gauge
    metrics_collector.inc_active_requests();

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let duration = start.elapsed();

    // Decrement active requests gauge
    metrics_collector.dec_active_requests();

    let status = response.status().as_u16().to_string();

    // Record in MetricsStore (label-aware)
    let labels = vec![
        ("method".to_string(), method.clone()),
        ("path".to_string(), path.clone()),
        ("status".to_string(), status.clone()),
    ];

    metrics_store.counter_inc(
        "xergon_agent_http_requests_total",
        "Total HTTP requests",
        &labels,
        1.0,
    );

    let duration_labels = vec![
        ("method".to_string(), method),
        ("path".to_string(), path),
    ];

    metrics_store.histogram_observe(
        "xergon_agent_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &duration_labels,
        duration.as_secs_f64(),
    );

    response
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

    // Combine built-in metrics with MetricsStore HTTP metrics
    let mut output = state
        .metrics
        .render_prometheus(total, node_score, network_score, ai_score);
    output.push_str(&state.metrics_store.format_prometheus());
    output
}

/// GET /metrics — Standard Prometheus text exposition format endpoint
async fn prometheus_metrics_handler(State(state): State<AppState>) -> String {
    api_metrics_handler(State(state)).await
}

/// GET /metrics/json — Machine-readable JSON metrics
async fn metrics_json_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pown = state.pown_status.read().await;
    let total = pown.work_points;

    let health = state.node_health.read().await;
    let node_score = if health.is_synced { 30 } else { 0 };
    let network_score = pown.unique_xergon_peers_seen.min(10) as u64 * 10;
    let ai_score = if total > node_score + network_score {
        total - node_score - network_score
    } else {
        0
    };

    let mut json = state
        .metrics
        .render_json(total, node_score, network_score, ai_score);

    // Merge MetricsStore data
    let store_json = state.metrics_store.format_json();
    if let Some(store_metrics) = store_json.get("metrics") {
        json.as_object_mut()
            .unwrap()
            .insert("http_metrics".to_string(), store_metrics.clone());
    }

    Json(json)
}

/// GET /api/health/deep — Deep health check with per-component status
async fn api_health_deep_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<crate::health_deep::DeepHealthResponse>) {
    let checks =
        crate::health_deep::HealthChecker::check_all(&state.node_health, &state.metrics).await;
    let overall_status = crate::health_deep::aggregate_status(&checks);

    let status_code = if overall_status == "unhealthy" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };

    let response = crate::health_deep::DeepHealthResponse {
        status: overall_status,
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: state.start_time.elapsed().as_secs(),
        checks,
    };

    (status_code, Json(response))
}

/// GET /api/oracle/rate — Current ERG/USD rate from oracle pool box
#[derive(Debug, Serialize)]
pub struct OracleRateResponse {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erg_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_raw: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
}

async fn oracle_rate_handler(State(state): State<AppState>) -> Json<OracleRateResponse> {
    match &state.oracle {
        Some(svc) => {
            let rate = svc.get_oracle_rate().await;
            match rate {
                Some(r) => Json(OracleRateResponse {
                    enabled: true,
                    erg_usd: Some(r.erg_usd),
                    rate_raw: Some(r.rate_raw),
                    epoch: Some(r.epoch),
                    box_id: Some(r.box_id),
                    fetched_at: Some(r.fetched_at.to_rfc3339()),
                }),
                None => Json(OracleRateResponse {
                    enabled: true,
                    erg_usd: None,
                    rate_raw: None,
                    epoch: None,
                    box_id: None,
                    fetched_at: None,
                }),
            }
        }
        None => Json(OracleRateResponse {
            enabled: false,
            erg_usd: None,
            rate_raw: None,
            epoch: None,
            box_id: None,
            fetched_at: None,
        }),
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

    info!(
        target_endpoint = %req.target_endpoint,
        timeout_secs = req.timeout_secs,
        "Forwarding proxy request to target provider"
    );

    match _engine.proxy_request(&req.target_endpoint, req.request_body, req.timeout_secs).await {
        Ok(proxy_resp) => Ok(Json(serde_json::to_value(proxy_resp).unwrap_or_else(|_| serde_json::json!({
            "error": "failed to serialize proxy response"
        })))),
        Err(e) => {
            tracing::error!(error = %e, target = %req.target_endpoint, "Proxy forwarding failed");
            Err(json_error(
                StatusCode::BAD_GATEWAY,
                "proxy_failed",
                &format!("Failed to forward request to target provider: {e}"),
            ))
        }
    }
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
// Provider Registry API handlers (Phase 7 -- on-chain registration)
// ---------------------------------------------------------------------------

/// POST /api/providers/register -- Register a new provider on-chain
#[derive(Debug, Deserialize)]
pub struct ProviderRegisterRequest {
    pub name: String,
    pub endpoint: String,
    pub price_per_token: u64,
    /// Provider PK hex (33 bytes). If empty, uses config value.
    #[serde(default)]
    pub provider_pk_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ProviderRegisterResponse {
    pub ok: bool,
    pub tx_id: String,
    pub provider_nft_id: String,
    pub provider_box_id: String,
}

async fn provider_register_handler(
    State(state): State<AppState>,
    Json(req): Json<ProviderRegisterRequest>,
) -> Result<Json<ProviderRegisterResponse>, Response> {
    let config = match &state.provider_registry_config {
        Some(c) => c.clone(),
        None => {
            return Ok(Json(ProviderRegisterResponse {
                ok: false,
                tx_id: String::new(),
                provider_nft_id: String::new(),
                provider_box_id: String::new(),
            }));
        }
    };

    if req.name.trim().is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "invalid_request", "name is required"));
    }
    if req.endpoint.trim().is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "invalid_request", "endpoint is required"));
    }

    let params = crate::provider_registry::RegisterProviderParams {
        provider_name: req.name.clone(),
        endpoint_url: req.endpoint.clone(),
        price_per_token: req.price_per_token,
        staking_address: String::new(),
        provider_pk_hex: if req.provider_pk_hex.is_empty() {
            config.provider_pk_hex.clone()
        } else {
            req.provider_pk_hex
        },
    };

    let node_url = std::env::var("XERGON__ERGO_NODE__REST_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());
    let client = crate::chain::client::ErgoNodeClient::new(node_url);

    match crate::provider_registry::register_provider_on_chain(&client, &config, &params).await {
        Ok(result) => {
            info!(
                tx_id = %result.tx_id,
                provider_name = %req.name,
                "Provider registered on-chain via API"
            );
            Ok(Json(ProviderRegisterResponse {
                ok: true,
                tx_id: result.tx_id,
                provider_nft_id: result.provider_nft_id,
                provider_box_id: result.provider_box_id,
            }))
        }
        Err(e) => {
            tracing::error!(error = %e, "Provider registration failed");
            Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "registration_failed",
                &format!("Provider registration failed: {e}"),
            ))
        }
    }
}

/// GET /api/providers/on-chain -- List all on-chain providers
#[derive(Debug, Serialize)]
pub struct ProviderOnChainListResponse {
    pub providers: Vec<crate::provider_registry::OnChainProvider>,
    pub count: usize,
}

async fn provider_on_chain_list_handler(
    State(_state): State<AppState>,
) -> Result<Json<ProviderOnChainListResponse>, Response> {
    let node_url = std::env::var("XERGON__ERGO_NODE__REST_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());

    match crate::provider_registry::query_provider_boxes(&node_url).await {
        Ok(providers) => {
            let count = providers.len();
            Ok(Json(ProviderOnChainListResponse { providers, count }))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to query on-chain providers");
            Err(json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "query_failed",
                &format!("Failed to query on-chain providers: {e}"),
            ))
        }
    }
}

/// PUT /api/providers/{box_id}/update -- Update provider pricing/endpoint
#[derive(Debug, Deserialize)]
pub struct ProviderUpdateRequest {
    pub price_per_token: Option<u64>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderUpdateResponse {
    pub ok: bool,
    pub tx_id: String,
    pub new_box_id: String,
}

async fn provider_update_handler(
    State(_state): State<AppState>,
    axum::extract::Path(box_id): axum::extract::Path<String>,
    Json(req): Json<ProviderUpdateRequest>,
) -> Result<Json<ProviderUpdateResponse>, Response> {
    if req.price_per_token.is_none() && req.endpoint.is_none() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "At least one of price_per_token or endpoint must be provided",
        ));
    }

    let node_url = std::env::var("XERGON__ERGO_NODE__REST_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());
    let client = crate::chain::client::ErgoNodeClient::new(node_url);

    let params = crate::provider_registry::UpdateProviderParams {
        box_id: box_id.clone(),
        new_price: req.price_per_token,
        new_endpoint: req.endpoint,
    };

    match crate::provider_registry::update_provider_status(&client, &params).await {
        Ok(result) => {
            info!(
                tx_id = %result.tx_id,
                box_id = %box_id,
                new_box_id = %result.new_box_id,
                "Provider box updated via API"
            );
            Ok(Json(ProviderUpdateResponse {
                ok: true,
                tx_id: result.tx_id,
                new_box_id: result.new_box_id,
            }))
        }
        Err(e) => {
            tracing::error!(error = %e, box_id = %box_id, "Provider update failed");
            Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "update_failed",
                &format!("Provider update failed: {e}"),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// eUTXO Settlement API endpoints
// ---------------------------------------------------------------------------

/// POST /api/settlement/execute -- Execute a real on-chain settlement
///
/// Body: { provider_address, staking_box_ids, fee_amounts, max_fee_nanoerg }
/// Response: { tx_id, boxes_settled, total_erg_settled }
#[derive(Debug, Deserialize)]
pub struct SettlementExecuteRequest {
    /// Provider's Ergo address (or P2S from provider_box ErgoTree)
    pub provider_address: String,
    /// Staking box IDs to spend as inputs
    pub staking_box_ids: Vec<String>,
    /// Fee amounts in nanoERG (one per staking box)
    pub fee_amounts: Vec<u64>,
    /// Maximum transaction fee in nanoERG
    #[serde(default = "default_max_fee")]
    pub max_fee_nanoerg: u64,
}

fn default_max_fee() -> u64 {
    1_000_000 // 0.001 ERG
}

async fn settlement_execute_handler(
    State(state): State<AppState>,
    Json(req): Json<SettlementExecuteRequest>,
) -> Response {
    use crate::settlement::eutxo::{EutxoSettlementEngine, SettlementTxParams};

    // Validate inputs
    if req.provider_address.is_empty() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "provider_address is required",
        );
    }
    if req.staking_box_ids.is_empty() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "staking_box_ids must not be empty",
        );
    }
    if req.staking_box_ids.len() != req.fee_amounts.len() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "staking_box_ids and fee_amounts must have the same length",
        );
    }

    let default_config = crate::config::SettlementConfig::default();
    let engine = match EutxoSettlementEngine::new(default_config, state.ergo_node_url.clone()) {
        Ok(e) => e,
        Err(e) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to create settlement engine: {}", e),
            );
        }
    };

    let params = SettlementTxParams {
        provider_address: req.provider_address.clone(),
        staking_box_ids: req.staking_box_ids.clone(),
        fee_amounts: req.fee_amounts.clone(),
        change_address: state.xergon_config.ergo_address.clone(),
        max_fee_nanoerg: req.max_fee_nanoerg,
    };

    match engine.execute_settlement(params).await {
        Ok(result) => Json(serde_json::json!({
            "tx_id": result.tx_id,
            "boxes_settled": result.boxes_settled,
            "total_erg_settled": result.total_erg_settled,
        }))
        .into_response(),
        Err(e) => {
            let error_msg = e.to_string();
            let (status, error_type) = if error_msg.contains("wallet is locked") {
                (StatusCode::SERVICE_UNAVAILABLE, "wallet_locked")
            } else if error_msg.contains("already been spent") {
                (StatusCode::CONFLICT, "box_already_spent")
            } else if error_msg.contains("insufficient") {
                (StatusCode::PAYMENT_REQUIRED, "insufficient_funds")
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "settlement_failed")
            };

            warn!(
                error = %error_msg,
                error_type = error_type,
                "Settlement execution failed"
            );

            json_error(status, error_type, &error_msg)
        }
    }
}

/// GET /api/settlement/boxes -- List settleable staking boxes
///
/// Query params: max_boxes (default 50), min_confirmations (default 30)
/// Response: { boxes: Vec<SettleableBox>, total_value: u64 }
async fn settlement_boxes_handler(
    State(state): State<AppState>,
) -> Response {
    use crate::settlement::eutxo::find_settleable_boxes;

    let default_config = crate::config::SettlementConfig::default();

    match find_settleable_boxes(
        &state.ergo_node_url,
        50,
        default_config.min_confirmations,
    )
    .await
    {
        Ok(result) => Json(serde_json::json!({
            "boxes": result.boxes,
            "total_value": result.total_value,
        }))
        .into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to list settleable boxes");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "node_error",
                &format!("Failed to query settleable boxes: {}", e),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Contracts API handlers (SDK-facing: /v1/contracts/*)
//
// These endpoints mirror the TypeScript SDK's contracts-api.ts expectations.
// Request/response shapes use snake_case JSON keys to match the SDK's
// `client.get<RawType>(...)` / `client.post<RawType>(...)` conventions.
//
// Endpoints fall into three categories:
//   A) Fully implemented — delegate to existing backend functions
//   B) Proxied from existing internal handlers (oracle, providers, settlement)
//   C) Not yet implemented — return 501 Not Implemented
// ---------------------------------------------------------------------------

// -- Contracts Request/Response types --------------------------------------

/// POST /v1/contracts/provider/register
/// (Matches SDK RegisterProviderApiParams)
#[derive(Debug, Deserialize)]
pub struct ContractsProviderRegisterRequest {
    pub provider_name: String,
    pub region: String,
    pub endpoint: String,
    pub models: Vec<String>,
    pub ergo_address: String,
    pub provider_pk_hex: String,
}

/// POST /v1/contracts/provider/register response
/// (Matches SDK RegisterProviderResult)
#[derive(Debug, Serialize)]
pub struct ContractsProviderRegisterResponse {
    pub tx_id: String,
    pub provider_nft_id: String,
    pub provider_box_id: String,
}

/// GET /v1/contracts/provider/status response
/// (Matches SDK ProviderBoxStatus raw shape)
#[derive(Debug, Serialize)]
pub struct ContractsProviderStatusResponse {
    pub box_id: String,
    pub provider_nft_id: String,
    pub provider_name: String,
    pub endpoint: String,
    pub price_per_token: String,
    pub min_stake: String,
    pub value: String,
    pub height: i32,
    pub confirmations: u32,
}

/// GET /v1/contracts/providers response (list on-chain providers)
/// (Matches SDK OnChainProvider raw shape)
#[derive(Debug, Serialize)]
pub struct ContractsOnChainProviderRaw {
    pub box_id: String,
    pub provider_nft_id: String,
    pub provider_name: String,
    pub endpoint: String,
    pub models: Vec<String>,
    pub region: String,
    pub value_nanoerg: String,
    pub active: bool,
}

/// POST /v1/contracts/staking/create
/// (Matches SDK CreateStakingBoxApiParams)
#[derive(Debug, Deserialize)]
pub struct ContractsStakingCreateRequest {
    pub user_pk_hex: String,
    pub amount_nanoerg: String,
}

/// POST /v1/contracts/staking/create response
/// (Matches SDK CreateStakingBoxResult)
#[derive(Debug, Serialize)]
pub struct ContractsStakingCreateResponse {
    pub tx_id: String,
    pub staking_box_id: String,
    pub amount_nanoerg: String,
}

/// GET /v1/contracts/staking/balance/{user_pk} response
/// (Matches SDK UserStakingBalance raw shape)
#[derive(Debug, Serialize)]
pub struct ContractsStakingBalanceResponse {
    pub user_pk_hex: String,
    pub total_balance_nanoerg: String,
    pub staking_box_count: u32,
    pub boxes: Vec<ContractsStakingBoxRaw>,
}

/// GET /v1/contracts/staking/boxes/{user_pk} response (individual box info)
#[derive(Debug, Serialize)]
pub struct ContractsStakingBoxesResponse {
    pub user_pk_hex: String,
    pub total_balance_nanoerg: String,
    pub staking_box_count: u32,
    pub boxes: Vec<ContractsStakingBoxRaw>,
}

/// Individual staking box info (used in balance/boxes responses)
#[derive(Debug, Serialize)]
pub struct ContractsStakingBoxRaw {
    pub box_id: String,
    pub value_nanoerg: String,
    pub creation_height: i32,
    pub confirmations: u32,
}

/// POST /v1/contracts/settlement/build
/// (Matches SDK BuildSettlementApiParams)
#[derive(Debug, Deserialize)]
pub struct ContractsSettlementBuildRequest {
    pub staking_box_ids: Vec<String>,
    pub fee_amounts: Vec<String>,
    pub provider_address: String,
    pub max_fee_nanoerg: String,
}

/// POST /v1/contracts/settlement/build response
/// (Matches SDK BuildSettlementResult)
#[derive(Debug, Serialize)]
pub struct ContractsSettlementBuildResponse {
    pub unsigned_tx: serde_json::Value,
    pub total_fees_nanoerg: String,
    pub net_settlement_nanoerg: String,
    pub estimated_tx_fee: String,
}

/// GET /v1/contracts/settlement/settleable response
/// (Matches SDK SettleableBox raw shape)
#[derive(Debug, Serialize)]
pub struct ContractsSettleableBoxRaw {
    pub box_id: String,
    pub value_nanoerg: String,
    pub user_pk_hex: String,
    pub provider_nft_id: String,
    pub fee_amount_nanoerg: String,
}

/// GET /v1/contracts/oracle/rate response
#[derive(Debug, Serialize)]
pub struct ContractsOracleRateResponse {
    pub rate: String,
    pub epoch: i32,
    pub box_id: String,
    pub erg_usd: f64,
}

/// GET /v1/contracts/oracle/status response
/// (Matches SDK OraclePoolStatus raw shape)
#[derive(Debug, Serialize)]
pub struct ContractsOracleStatusResponse {
    pub epoch: i32,
    pub erg_usd: f64,
    pub rate: String,
    pub pool_box_id: String,
    pub last_update_height: i32,
}

/// POST /v1/contracts/governance/proposal
#[derive(Debug, Deserialize)]
pub struct ContractsGovernanceProposalRequest {
    /// Box ID of the governance proposal box to target
    pub gov_box_id: String,
    /// Minimum votes needed to pass (e.g. 51)
    pub threshold: i32,
    /// Total number of eligible voters
    pub total_voters: i32,
    /// Block height when voting ends
    pub end_height: i32,
    /// Raw proposal content (will be hashed into R9)
    #[serde(default)]
    pub proposal_data: String,
    /// Optional: proposal metadata for logging
    #[serde(default)]
    pub proposal_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub proposer_pk_hex: String,
}

/// POST /v1/contracts/governance/vote
#[derive(Debug, Deserialize)]
pub struct ContractsGovernanceVoteRequest {
    /// Box ID of the governance proposal box to target
    pub gov_box_id: String,
    /// Voter's compressed secp256k1 public key (hex)
    pub voter_pk_hex: String,
}

/// GET /v1/contracts/governance/proposals response
#[derive(Debug, Serialize)]
pub struct ContractsGovernanceProposalsResponse {
    pub proposals: Vec<serde_json::Value>,
    pub count: usize,
}

// -- Helper: build ErgoNodeClient from env ---------------------------------

fn node_client_from_env() -> crate::chain::client::ErgoNodeClient {
    let node_url = std::env::var("XERGON__ERGO_NODE__REST_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());
    crate::chain::client::ErgoNodeClient::new(node_url)
}

// -- Sigma serialization helpers ---------------------------------------------

/// Encode a byte array as a Sigma `Coll[Byte]` (tag 0x0e + VLB length + data).
/// Returns the hex-encoded result suitable for use in Ergo box registers.
fn sigma_encode_coll_byte(data: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(2 + data.len());
    bytes.push(0x0e); // Coll[Byte] tag
    // Variable-length byte encoding of the length
    let len = data.len();
    if len <= 0x7f {
        bytes.push(len as u8);
    } else if len <= 0x3fff {
        bytes.push(((len >> 8) & 0x3f) as u8 | 0x80);
        bytes.push((len & 0xff) as u8);
    } else {
        // 3-byte VLB (covers up to 2^21 - 1, far more than any register)
        bytes.push(((len >> 16) & 0x1f) as u8 | 0xc0);
        bytes.push(((len >> 8) & 0xff) as u8);
        bytes.push((len & 0xff) as u8);
    }
    bytes.extend_from_slice(data);
    hex::encode(&bytes)
}

/// Encode a u64 as a Sigma `Long` (tag 0x08 + big-endian 8 bytes).
/// Returns the hex-encoded result suitable for use in Ergo box registers.
fn sigma_encode_long(value: u64) -> String {
    let mut bytes = Vec::with_capacity(9);
    bytes.push(0x08); // SLong tag
    bytes.extend_from_slice(&value.to_be_bytes());
    hex::encode(&bytes)
}

/// Decode a Sigma `Coll[Byte]` (hex-encoded) and return the inner data as hex.
/// Input format: "0e" + VLB(length) + data_hex
/// Returns empty string if the input is malformed.
fn sigma_decode_coll_byte_to_hex(hex_str: &str) -> String {
    let bytes = match hex::decode(hex_str) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };

    if bytes.is_empty() || bytes[0] != 0x0e {
        return String::new();
    }

    // Parse VLB length
    let (data_len, consumed) = if bytes.len() < 2 {
        return String::new();
    } else {
        let first = bytes[1] as usize;
        if first <= 0x7f {
            // 1-byte VLB
            (first, 1usize)
        } else if first & 0xc0 == 0x80 {
            // 2-byte VLB
            if bytes.len() < 3 {
                return String::new();
            }
            let len = ((first & 0x3f) as usize) << 8 | (bytes[2] as usize);
            (len, 2usize)
        } else if first & 0xe0 == 0xc0 {
            // 3-byte VLB
            if bytes.len() < 4 {
                return String::new();
            }
            let len = ((first & 0x1f) as usize) << 16
                | (bytes[2] as usize) << 8
                | (bytes[3] as usize);
            (len, 3usize)
        } else {
            return String::new();
        }
    };

    let data_start = 1 + consumed;
    if data_start + data_len > bytes.len() {
        return String::new();
    }

    hex::encode(&bytes[data_start..data_start + data_len])
}

// -- Handler implementations ------------------------------------------------

/// POST /v1/contracts/provider/register
///
/// Register a new provider on-chain. Delegates to the existing
/// `provider_registry::register_provider_on_chain` function.
async fn contracts_provider_register_handler(
    State(state): State<AppState>,
    Json(req): Json<ContractsProviderRegisterRequest>,
) -> Response {
    // Validate required fields
    if req.provider_name.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "provider_name is required");
    }
    if req.endpoint.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "endpoint is required");
    }
    if req.provider_pk_hex.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "provider_pk_hex is required");
    }

    let config = match &state.provider_registry_config {
        Some(c) => c.clone(),
        None => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "not_configured",
                "Provider registry is not configured on this agent. Set [provider_registry] in config.",
            );
        }
    };

    let params = crate::provider_registry::RegisterProviderParams {
        provider_name: req.provider_name.clone(),
        endpoint_url: req.endpoint.clone(),
        price_per_token: 0, // will use config default
        staking_address: req.ergo_address.clone(),
        provider_pk_hex: req.provider_pk_hex.clone(),
    };

    let client = node_client_from_env();

    match crate::provider_registry::register_provider_on_chain(&client, &config, &params).await {
        Ok(result) => {
            info!(
                tx_id = %result.tx_id,
                provider_name = %req.provider_name,
                "Provider registered via /v1/contracts/provider/register"
            );
            Json(ContractsProviderRegisterResponse {
                tx_id: result.tx_id,
                provider_nft_id: result.provider_nft_id,
                provider_box_id: result.provider_box_id,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "Provider registration via contracts API failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "registration_failed",
                &format!("Provider registration failed: {e}"),
            )
        }
    }
}

/// GET /v1/contracts/provider/status
///
/// Query provider box status by NFT token ID.
/// Accepts an optional `nft_id` query parameter. If provided, uses the node API
/// to look up boxes by token ID directly. Otherwise falls back to scanning all
/// provider boxes and filtering client-side.
async fn contracts_provider_status_handler(
    State(_state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let nft_id = params.get("nft_id").map(|s| s.as_str()).unwrap_or("");

    if nft_id.is_empty() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "missing_nft_id",
            "Query parameter `nft_id` is required. Example: /v1/contracts/provider/status?nft_id=<token_id>",
        );
    }

    // Use the node API to look up boxes containing this NFT token ID directly
    let client = node_client_from_env();

    match client.get_boxes_by_token_id(nft_id).await {
        Ok(boxes) => {
            if boxes.is_empty() {
                return json_error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!("No boxes found with NFT token ID: {nft_id}"),
                );
            }

            // Find the provider box among the results (the one matching the provider_registration ErgoTree)
            let provider_tree_hex = match crate::contract_compile::get_contract_hex("provider_registration") {
                Some(tree) => tree,
                None => {
                    // If we can't get the contract tree, return the first box as-is
                    let box_data = &boxes[0];
                    return Json(ContractsProviderStatusResponse {
                        box_id: box_data.box_id.clone(),
                        provider_nft_id: nft_id.to_string(),
                        provider_name: String::new(),
                        endpoint: box_data
                            .additional_registers
                            .get("R5")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        price_per_token: box_data
                            .additional_registers
                            .get("R6")
                            .and_then(|v| v.as_str())
                            .unwrap_or("0")
                            .to_string(),
                        min_stake: String::new(),
                        value: box_data.value.to_string(),
                        height: box_data.creation_height,
                        confirmations: 0,
                    })
                    .into_response();
                }
            };

            // Try to find a box that matches both the NFT token and the provider ErgoTree
            let provider_box = boxes
                .iter()
                .find(|b| b.ergo_tree == provider_tree_hex);

            let box_data = match provider_box {
                Some(b) => b,
                None => {
                    // No provider box found with this NFT; return 404
                    return json_error(
                        StatusCode::NOT_FOUND,
                        "not_provider_box",
                        &format!("No provider registration box found for NFT token ID: {nft_id}"),
                    );
                }
            };

            Json(ContractsProviderStatusResponse {
                box_id: box_data.box_id.clone(),
                provider_nft_id: nft_id.to_string(),
                provider_name: String::new(),
                endpoint: box_data
                    .additional_registers
                    .get("R5")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                price_per_token: box_data
                    .additional_registers
                    .get("R6")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0")
                    .to_string(),
                min_stake: String::new(),
                value: box_data.value.to_string(),
                height: box_data.creation_height,
                confirmations: 0,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, nft_id = %nft_id, "Failed to query boxes by token ID");
            json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "query_failed",
                &format!("Failed to query boxes by token ID: {e}"),
            )
        }
    }
}

/// GET /v1/contracts/providers
///
/// List all on-chain providers. Delegates to existing `provider_registry::query_provider_boxes`.
async fn contracts_providers_list_handler(
    State(state): State<AppState>,
) -> Response {
    let node_url = state.ergo_node_url.clone();

    match crate::provider_registry::query_provider_boxes(&node_url).await {
        Ok(providers) => {
            let mapped: Vec<ContractsOnChainProviderRaw> = providers
                .into_iter()
                .map(|p| ContractsOnChainProviderRaw {
                    box_id: p.box_id,
                    provider_nft_id: p.nft_token_id,
                    provider_name: String::new(), // not stored in current OnChainProvider
                    endpoint: p.endpoint,
                    models: Vec::new(),           // not stored in current OnChainProvider
                    region: String::new(),        // not stored in current OnChainProvider
                    value_nanoerg: p.value_nanoerg.to_string(),
                    active: true,                 // all UTXO-set providers are considered active
                })
                .collect();
            Json(mapped).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to query on-chain providers via contracts API");
            json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "query_failed",
                &format!("Failed to query on-chain providers: {e}"),
            )
        }
    }
}

/// POST /v1/contracts/staking/create
///
/// Plan a user staking box creation. Returns a JSON description of the required
/// box outputs that the client can use to build and sign the transaction via
/// the Ergo node wallet API.
///
/// The user_staking contract expects:
///   R4 = Coll[Byte] (user public key hex)
///   R5 = Long (stake amount in nanoERG)
async fn contracts_staking_create_handler(
    State(_state): State<AppState>,
    Json(req): Json<ContractsStakingCreateRequest>,
) -> Response {
    // Validate inputs
    if req.user_pk_hex.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "user_pk_hex is required");
    }
    let amount_nanoerg: u64 = match req.amount_nanoerg.parse() {
        Ok(v) if v > 0 => v,
        _ => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "amount_nanoerg must be a positive integer (nanoERG)",
            );
        }
    };

    // Look up the compiled user_staking ErgoTree
    let staking_tree_hex = match crate::contract_compile::get_contract_hex("user_staking") {
        Some(tree) => tree,
        None => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "contract_not_compiled",
                "user_staking contract is not compiled. Run the agent with compiled contracts.",
            );
        }
    };

    // Build the register values as Sigma-serialized hex
    // R4: Coll[Byte] encoding of user_pk_hex bytes
    let user_pk_bytes = match hex::decode(&req.user_pk_hex) {
        Ok(b) => b,
        Err(_) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "user_pk_hex must be valid hex-encoded bytes",
            );
        }
    };

    // Sigma Coll[Byte] encoding: 0x0e + VLB(length) + data
    let r4_encoded = sigma_encode_coll_byte(&user_pk_bytes);

    // R5: Long encoding of amount_nanoerg (Sigma SLong = 0x08 + big-endian 8 bytes)
    let r5_encoded = sigma_encode_long(amount_nanoerg);

    // Return the staking box plan for the client to build a transaction
    // The minimum ERG value for a box is ~0.001 ERG (1_000_000 nanoERG)
    let min_box_value = std::cmp::max(amount_nanoerg, 1_000_000u64);

    let plan = serde_json::json!({
        "ergo_tree": staking_tree_hex,
        "value": min_box_value.to_string(),
        "registers": {
            "R4": r4_encoded,
            "R5": r5_encoded,
        },
        "assets": [],
        "creation_height": 0, // will be set by the node at signing time
    });

    // We can't produce a tx_id or staking_box_id until the transaction is signed and submitted.
    // Return a plan that the SDK can use to call the node wallet API.
    // For compatibility with ContractsStakingCreateResponse shape, return placeholders
    // and include the plan in an additional field.
    Json(serde_json::json!({
        "status": "plan_ready",
        "tx_id": null,
        "staking_box_id": null,
        "amount_nanoerg": min_box_value.to_string(),
        "plan": plan,
        "message": "Use this plan with the Ergo node wallet API to build and submit the staking transaction. The node wallet will produce the actual tx_id and staking_box_id.",
    }))
    .into_response()
}

/// GET /v1/contracts/staking/balance/{user_pk}
///
/// Query a user's total staking balance across all their staking boxes.
async fn contracts_staking_balance_handler(
    State(_state): State<AppState>,
    axum::extract::Path(user_pk): axum::extract::Path<String>,
) -> Response {
    let client = node_client_from_env();

    // Try to find staking boxes by scanning the UTXO set for boxes
    // that contain the user's public key in their registers.
    // This requires the compiled user_staking ErgoTree to be available.
    let staking_tree_hex = match crate::contract_compile::get_contract_hex("user_staking") {
        Some(tree) => tree,
        None => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "contract_not_compiled",
                "user_staking contract is not compiled. Run with compiled contracts.",
            );
        }
    };

    match client.get_boxes_by_ergo_tree(&staking_tree_hex).await {
        Ok(boxes) => {
            let mut total_balance: u64 = 0;
            let mut matched_boxes: Vec<ContractsStakingBoxRaw> = Vec::new();

            for box_data in &boxes {
                // Check if this box's R4 register contains the user's PK
                // The user_staking contract stores user PK in R4 as Coll[Byte]
                // The raw register value is Sigma-serialized: 0x0e + VLB(len) + hex_data
                // We must decode the Coll[Byte] and compare the decoded PK against user_pk,
                // NOT do a substring match on the raw serialized hex.
                let is_match = box_data
                    .additional_registers
                    .get("R4")
                    .and_then(|v| v.as_str())
                    .map(|r4_hex| {
                        // Decode the Sigma Coll[Byte]: extract the data payload and hex-encode it
                        let decoded_pk = sigma_decode_coll_byte_to_hex(r4_hex);
                        decoded_pk == user_pk
                    })
                    .unwrap_or(false);

                if is_match {
                    total_balance += box_data.value;
                    matched_boxes.push(ContractsStakingBoxRaw {
                        box_id: box_data.box_id.clone(),
                        value_nanoerg: box_data.value.to_string(),
                        creation_height: box_data.creation_height,
                        confirmations: 0, // would need current height to compute
                    });
                }
            }

            let count = matched_boxes.len() as u32;
            Json(ContractsStakingBalanceResponse {
                user_pk_hex: user_pk,
                total_balance_nanoerg: total_balance.to_string(),
                staking_box_count: count,
                boxes: matched_boxes,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to query staking boxes");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "query_failed",
                &format!("Failed to query staking boxes: {e}"),
            )
        }
    }
}

/// GET /v1/contracts/staking/boxes/{user_pk}
///
/// Get all staking boxes for a given user (same underlying query as balance).
async fn contracts_staking_boxes_handler(
    State(_state): State<AppState>,
    axum::extract::Path(user_pk): axum::extract::Path<String>,
) -> Response {
    let client = node_client_from_env();

    let staking_tree_hex = match crate::contract_compile::get_contract_hex("user_staking") {
        Some(tree) => tree,
        None => {
            return json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "contract_not_compiled",
                "user_staking contract is not compiled. Run with compiled contracts.",
            );
        }
    };

    match client.get_boxes_by_ergo_tree(&staking_tree_hex).await {
        Ok(boxes) => {
            let mut total_balance: u64 = 0;
            let mut matched_boxes: Vec<ContractsStakingBoxRaw> = Vec::new();

            for box_data in &boxes {
                let is_match = box_data
                    .additional_registers
                    .get("R4")
                    .and_then(|v| v.as_str())
                    .map(|r4_hex| {
                        let decoded_pk = sigma_decode_coll_byte_to_hex(r4_hex);
                        decoded_pk == user_pk
                    })
                    .unwrap_or(false);

                if is_match {
                    total_balance += box_data.value;
                    matched_boxes.push(ContractsStakingBoxRaw {
                        box_id: box_data.box_id.clone(),
                        value_nanoerg: box_data.value.to_string(),
                        creation_height: box_data.creation_height,
                        confirmations: 0,
                    });
                }
            }

            let count = matched_boxes.len() as u32;
            Json(ContractsStakingBoxesResponse {
                user_pk_hex: user_pk,
                total_balance_nanoerg: total_balance.to_string(),
                staking_box_count: count,
                boxes: matched_boxes,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to query staking boxes");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "query_failed",
                &format!("Failed to query staking boxes: {e}"),
            )
        }
    }
}

/// POST /v1/contracts/settlement/build
///
/// Build a settlement transaction. Delegates to the existing
/// `EutxoSettlementEngine::execute_settlement`.
async fn contracts_settlement_build_handler(
    State(state): State<AppState>,
    Json(req): Json<ContractsSettlementBuildRequest>,
) -> Response {
    use crate::settlement::eutxo::{EutxoSettlementEngine, SettlementTxParams};

    // Validate inputs
    if req.provider_address.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "provider_address is required");
    }
    if req.staking_box_ids.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "staking_box_ids must not be empty");
    }
    if req.staking_box_ids.len() != req.fee_amounts.len() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "staking_box_ids and fee_amounts must have the same length",
        );
    }

    // Parse fee amounts from strings to u64
    let fee_amounts: Result<Vec<u64>, _> = req
        .fee_amounts
        .iter()
        .map(|s| s.parse::<u64>())
        .collect();
    let fee_amounts = match fee_amounts {
        Ok(fees) => fees,
        Err(e) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("Invalid fee_amounts: {e}"),
            );
        }
    };

    let max_fee: u64 = req
        .max_fee_nanoerg
        .parse::<u64>()
        .unwrap_or(1_000_000);

    let default_config = crate::config::SettlementConfig::default();
    let engine = match EutxoSettlementEngine::new(default_config, state.ergo_node_url.clone()) {
        Ok(e) => e,
        Err(e) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "engine_error",
                &format!("Failed to create settlement engine: {e}"),
            );
        }
    };

    let total_fees: u64 = fee_amounts.iter().copied().sum();
    let tx_fee = 1_000_000u64; // estimated
    let net = total_fees.saturating_sub(tx_fee);

    let params = SettlementTxParams {
        provider_address: req.provider_address.clone(),
        staking_box_ids: req.staking_box_ids.clone(),
        fee_amounts,
        change_address: state.xergon_config.ergo_address.clone(),
        max_fee_nanoerg: max_fee,
    };

    match engine.execute_settlement(params).await {
        Ok(result) => {
            Json(ContractsSettlementBuildResponse {
                unsigned_tx: serde_json::json!({
                    "tx_id": result.tx_id,
                    "note": "Transaction was signed and broadcast by the agent node wallet. tx_id is the on-chain ID."
                }),
                total_fees_nanoerg: total_fees.to_string(),
                net_settlement_nanoerg: net.to_string(),
                estimated_tx_fee: tx_fee.to_string(),
            })
            .into_response()
        }
        Err(e) => {
            let error_msg = e.to_string();
            let (status, error_type) = if error_msg.contains("wallet is locked") {
                (StatusCode::SERVICE_UNAVAILABLE, "wallet_locked")
            } else if error_msg.contains("already been spent") {
                (StatusCode::CONFLICT, "box_already_spent")
            } else if error_msg.contains("insufficient") {
                (StatusCode::PAYMENT_REQUIRED, "insufficient_funds")
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "settlement_failed")
            };

            warn!(error = %error_msg, error_type, "Settlement build failed via contracts API");
            json_error(status, error_type, &error_msg)
        }
    }
}

/// GET /v1/contracts/settlement/settleable
///
/// Get staking boxes ready for settlement. Delegates to existing
/// `settlement::eutxo::find_settleable_boxes`.
async fn contracts_settlement_boxes_handler(
    State(state): State<AppState>,
) -> Response {
    let default_config = crate::config::SettlementConfig::default();

    match crate::settlement::eutxo::find_settleable_boxes(
        &state.ergo_node_url,
        50,
        default_config.min_confirmations,
    )
    .await
    {
        Ok(result) => {
            let mapped: Vec<ContractsSettleableBoxRaw> = result
                .boxes
                .into_iter()
                .map(|b| ContractsSettleableBoxRaw {
                    box_id: b.box_id,
                    value_nanoerg: b.value.to_string(),
                    user_pk_hex: String::new(), // not available from current SettleableBox struct
                    provider_nft_id: String::new(), // not available from current struct
                    fee_amount_nanoerg: "0".to_string(), // fee tracking not yet in struct
                })
                .collect();
            Json(mapped).into_response()
        }
        Err(e) => {
            warn!(error = %e, "Failed to list settleable boxes via contracts API");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "node_error",
                &format!("Failed to query settleable boxes: {e}"),
            )
        }
    }
}

/// GET /v1/contracts/oracle/rate
///
/// Get the current ERG/USD rate from the oracle pool.
/// Delegates to the existing OracleService.
async fn contracts_oracle_rate_handler(
    State(state): State<AppState>,
) -> Response {
    match &state.oracle {
        Some(svc) => {
            let rate = svc.get_oracle_rate().await;
            match rate {
                Some(r) => Json(ContractsOracleRateResponse {
                    rate: r.rate_raw.to_string(),
                    epoch: r.epoch,
                    box_id: r.box_id,
                    erg_usd: r.erg_usd,
                })
                .into_response(),
                None => json_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "oracle_unavailable",
                    "Oracle rate is not available. The oracle pool box may not exist or has no data.",
                ),
            }
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "oracle_disabled",
            "Oracle service is not enabled on this agent. Configure [oracle] in config.",
        ),
    }
}

/// GET /v1/contracts/oracle/status
///
/// Get detailed oracle pool status (epoch, rate, box ID, update height).
async fn contracts_oracle_status_handler(
    State(state): State<AppState>,
) -> Response {
    match &state.oracle {
        Some(svc) => {
            let rate = svc.get_oracle_rate().await;
            match rate {
                Some(r) => {
                    // Parse the fetched_at timestamp to get the last update height
                    // The oracle service stores this internally; use the epoch as a proxy
                    let client = node_client_from_env();
                    let last_update_height = client.get_height().await.unwrap_or(0);

                    Json(ContractsOracleStatusResponse {
                        epoch: r.epoch,
                        erg_usd: r.erg_usd,
                        rate: r.rate_raw.to_string(),
                        pool_box_id: r.box_id,
                        last_update_height,
                    })
                    .into_response()
                }
                None => json_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "oracle_unavailable",
                    "Oracle pool status is not available.",
                ),
            }
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "oracle_disabled",
            "Oracle service is not enabled on this agent.",
        ),
    }
}

/// POST /v1/contracts/governance/proposal
///
/// Create a governance proposal on-chain.
/// Delegates to `protocol::actions::plan_create_proposal` and returns the
/// resulting `GovernanceTxPlan` describing the successor box registers.
async fn contracts_governance_proposal_handler(
    State(_state): State<AppState>,
    Json(req): Json<ContractsGovernanceProposalRequest>,
) -> Response {
    // Validate required fields
    if req.gov_box_id.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "gov_box_id is required");
    }
    if req.threshold <= 0 {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "threshold must be > 0");
    }
    if req.total_voters <= 0 {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "total_voters must be > 0");
    }
    if req.end_height <= 0 {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "end_height must be > 0");
    }

    // Build proposal data bytes from the human-readable fields.
    // If `proposal_data` is non-empty use it directly; otherwise compose
    // from type + title + description.
    let proposal_data = if !req.proposal_data.is_empty() {
        req.proposal_data.as_bytes().to_vec()
    } else {
        format!("{}:{}:{}", req.proposal_type, req.title, req.description)
            .into_bytes()
    };

    let client = node_client_from_env();

    match crate::protocol::actions::plan_create_proposal(
        &client,
        &req.gov_box_id,
        req.threshold,
        req.total_voters,
        req.end_height,
        &proposal_data,
    )
    .await
    {
        Ok(plan) => {
            info!(
                gov_box_id = %req.gov_box_id,
                proposal_id = plan.output_registers.r5_active_proposal_id,
                is_valid = plan.is_valid,
                "Governance proposal plan created via /v1/contracts/governance/proposal"
            );
            // If the plan has validation errors, still return it but with
            // a descriptive summary so the caller knows why it is not valid.
            if !plan.is_valid {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "plan": serde_json::from_value::<serde_json::Value>(
                            serde_json::to_value(&plan).unwrap_or_default()
                        ).unwrap_or_default(),
                        "warning": "Plan created but validation failed",
                        "validation_errors": plan.validation_errors,
                    })),
                )
                    .into_response();
            }
            Json(serde_json::json!({ "plan": plan })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "Governance proposal planning failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "plan_failed",
                &format!("Governance proposal planning failed: {e}"),
            )
        }
    }
}

/// POST /v1/contracts/governance/vote
///
/// Vote on a governance proposal.
/// Delegates to `protocol::actions::plan_vote` and returns the
/// resulting `GovernanceTxPlan`.
async fn contracts_governance_vote_handler(
    State(_state): State<AppState>,
    Json(req): Json<ContractsGovernanceVoteRequest>,
) -> Response {
    // Validate required fields
    if req.gov_box_id.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "gov_box_id is required");
    }
    if req.voter_pk_hex.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "invalid_request", "voter_pk_hex is required");
    }

    let client = node_client_from_env();

    match crate::protocol::actions::plan_vote(&client, &req.gov_box_id, &req.voter_pk_hex).await {
        Ok(plan) => {
            info!(
                gov_box_id = %req.gov_box_id,
                voter_pk = %req.voter_pk_hex,
                is_valid = plan.is_valid,
                "Governance vote plan created via /v1/contracts/governance/vote"
            );
            if !plan.is_valid {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "plan": plan,
                        "warning": "Plan created but validation failed",
                        "validation_errors": plan.validation_errors,
                    })),
                )
                    .into_response();
            }
            Json(serde_json::json!({ "plan": plan })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "Governance vote planning failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "plan_failed",
                &format!("Governance vote planning failed: {e}"),
            )
        }
    }
}

/// GET /v1/contracts/governance/proposals
///
/// Query the current governance box state.
/// Fetches the governance box from the UTXO set by `gov_box_id` query parameter
/// and returns its parsed register state via `specs::validate_governance_box`.
async fn contracts_governance_proposals_handler(
    State(_state): State<AppState>,
) -> Response {
    // The gov_box_id can come from env or from a query param.
    // We read it from env for the GET handler (no request body available).
    let gov_box_id = match std::env::var("XERGON__GOVERNANCE__BOX_ID") {
        Ok(id) if !id.trim().is_empty() => id,
        _ => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "missing_gov_box_id",
                "gov_box_id is required. Set XERGON__GOVERNANCE__BOX_ID env var.",
            );
        }
    };

    let client = node_client_from_env();

    // Fetch the raw box from the node
    let raw_box = match client.get_box(&gov_box_id).await {
        Ok(box_val) => box_val,
        Err(e) => {
            tracing::error!(error = %e, gov_box_id = %gov_box_id, "Failed to fetch governance box");
            return json_error(
                StatusCode::NOT_FOUND,
                "box_not_found",
                &format!("Governance box {} not found or node unreachable: {e}", gov_box_id),
            );
        }
    };

    // Get current height for validation
    let current_height = match client.get_height().await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get chain height");
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "node_error",
                &format!("Failed to get chain height: {e}"),
            );
        }
    };

    // Validate and parse the governance box registers
    match crate::protocol::specs::validate_governance_box(&raw_box, current_height) {
        Ok(gov_box) => {
            info!(
                gov_box_id = %gov_box_id,
                proposal_count = gov_box.proposal_count,
                active_proposal_id = gov_box.active_proposal_id,
                "Governance box state queried via /v1/contracts/governance/proposals"
            );
            Json(serde_json::json!({
                "proposals": [serde_json::to_value(&gov_box).unwrap_or_default()],
                "count": 1,
                "current_height": current_height,
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, gov_box_id = %gov_box_id, "Governance box validation failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "validation_failed",
                &format!("Governance box validation failed: {e}"),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// ErgoAuth API handlers (JWT-based authentication)
// ---------------------------------------------------------------------------

/// POST /v1/auth/ergoauth/challenge
///
/// Generate an ErgoAuth challenge for a wallet to sign.
/// Body: { "address": "<ergo_address>" }
/// Response: { "challenge": "<hex>", "sigma_boolean": "<hex>", "expires_at": <unix_ts> }
async fn ergoauth_challenge_handler(
    State(auth_state): State<AuthState>,
    Json(req): Json<crate::auth::ChallengeRequest>,
) -> Result<Json<crate::auth::ChallengeResponse>, Response> {
    if req.address.trim().is_empty() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "address is required",
        ));
    }

    match crate::auth::generate_challenge(&req.address, &auth_state.challenges) {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => {
            tracing::error!(error = %e, "Failed to generate challenge");
            Err(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "challenge_failed",
                &format!("Failed to generate challenge: {}", e),
            ))
        }
    }
}

/// POST /v1/auth/ergoauth/verify
///
/// Verify an ErgoAuth signed challenge and issue a JWT token.
/// Body: { "address": "...", "challenge": "...", "sigma_boolean": "...", "proof": "...",
///         "signing_message": "...", "provider_pk_hex": "..." }
/// Response: { "token": "<jwt>", "token_type": "Bearer", "expires_in": 86400,
///             "registration": { "tx_id": "...", "provider_nft_id": "...", "provider_box_id": "..." } }
async fn ergoauth_verify_handler(
    State(auth_state): State<AuthState>,
    Json(req): Json<crate::auth::VerifyRequest>,
) -> Result<Json<crate::auth::VerifyResponse>, Response> {
    match crate::auth::process_ergoauth_verify(&auth_state, req).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => {
            tracing::warn!(error = %e, "ErgoAuth verification failed");
            let error_type = if e.to_string().contains("not found") {
                "challenge_expired"
            } else if e.to_string().contains("mismatch") {
                "challenge_mismatch"
            } else if e.to_string().contains("signature") || e.to_string().contains("verification") {
                "invalid_signature"
            } else {
                "verification_failed"
            };
            Err(json_error(
                StatusCode::UNAUTHORIZED,
                error_type,
                &format!("ErgoAuth verification failed: {}", e),
            ))
        }
    }
}

// Tests -- W7 fixes: localhost_cors
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Model Discovery API handlers
// ---------------------------------------------------------------------------

use axum::extract::Query;

/// GET /api/discovery/models?architecture=llama&max_size=20gb&sort=downloads
async fn discovery_models_handler(
    State(state): State<AppState>,
    Query(params): Query<crate::model_discovery::DiscoveryQuery>,
) -> Response {
    let discovery = match &state.model_discovery {
        Some(d) => d,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Model discovery is not enabled",
                    "hint": "Set [model_discovery].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    let response = discovery.get_models(&params).await;
    Json(response).into_response()
}

/// GET /api/discovery/recommended
async fn discovery_recommended_handler(
    State(state): State<AppState>,
) -> Response {
    let discovery = match &state.model_discovery {
        Some(d) => d,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Model discovery is not enabled",
                    "hint": "Set [model_discovery].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    let recommended = discovery.get_recommended().await;
    Json(serde_json::json!({
        "models": recommended,
        "count": recommended.len(),
    }))
    .into_response()
}

/// POST /api/discovery/scan — trigger a manual scan
async fn discovery_scan_handler(
    State(state): State<AppState>,
) -> Response {
    let discovery = match &state.model_discovery {
        Some(d) => d,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Model discovery is not enabled",
                    "hint": "Set [model_discovery].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    // Check if already scanning
    if discovery.is_scanning().await {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "status": "already_scanning",
                "error": "A discovery scan is already in progress"
            })),
        )
            .into_response();
    }

    // Run scan in background, return immediately
    let discovery_clone = discovery.clone();
    tokio::spawn(async move {
        let _ = discovery_clone.scan().await;
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "scan_started",
            "message": "Discovery scan initiated. Results will be available shortly."
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Model Cache API handlers
// ---------------------------------------------------------------------------

/// GET /api/cache/stats
async fn cache_stats_handler(
    State(state): State<AppState>,
) -> Response {
    let cache = match &state.model_cache {
        Some(c) => c,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Model cache is not enabled",
                    "hint": "Set [model_cache].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    let stats = cache.stats().await;
    Json(stats).into_response()
}

/// GET /api/cache/models
async fn cache_models_handler(
    State(state): State<AppState>,
) -> Response {
    let cache = match &state.model_cache {
        Some(c) => c,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Model cache is not enabled",
                    "hint": "Set [model_cache].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    let models = cache.list_models().await;
    Json(serde_json::json!({
        "models": models,
        "count": models.len(),
    }))
    .into_response()
}

/// DELETE /api/cache/models/{model_id}
async fn cache_evict_handler(
    State(state): State<AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Response {
    let cache = match &state.model_cache {
        Some(c) => c,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Model cache is not enabled",
                    "hint": "Set [model_cache].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    let (evicted, freed) = match cache.evict_model(&model_id).await {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to evict model: {}", e)
                })),
            )
                .into_response();
        }
    };

    let status = if evicted { StatusCode::OK } else { StatusCode::NOT_FOUND };
    (
        status,
        Json(crate::model_cache::EvictResponse {
            model_id,
            evicted,
            freed_bytes: freed,
            error: if !evicted {
                Some("Model not found or is pinned".to_string())
            } else {
                None
            },
        }),
    )
        .into_response()
}

/// POST /api/cache/models/{model_id}/pin
///
/// Request body: `{ "pinned": true }` or `{ "pinned": false }`
async fn cache_pin_handler(
    State(state): State<AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let cache = match &state.model_cache {
        Some(c) => c,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Model cache is not enabled",
                    "hint": "Set [model_cache].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    let pinned = body.get("pinned").and_then(|v| v.as_bool()).unwrap_or(true);

    let result = if pinned {
        cache.pin_model(&model_id).await
    } else {
        cache.unpin_model(&model_id).await
    };

    match result {
        Ok(found) => {
            if found {
                Json(crate::model_cache::PinResponse {
                    model_id,
                    pinned,
                    error: None,
                })
                .into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(crate::model_cache::PinResponse {
                        model_id,
                        pinned,
                        error: Some("Model not found in cache".to_string()),
                    }),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::model_cache::PinResponse {
                model_id,
                pinned,
                error: Some(format!("Failed to update pin: {}", e)),
            }),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Benchmark API handlers
// ---------------------------------------------------------------------------

/// POST /api/benchmark/run
///
/// Run a benchmark for a specific model.
/// Body: { "model": "llama3", "benchmark_type": "full"|"latency"|"throughput"|"memory"|"accuracy",
///         "prompt_tokens": 64, "concurrent_requests": 4, "timeout_secs": 60 }
async fn benchmark_run_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::benchmark::BenchmarkRunRequest>,
) -> Response {
    let suite = match &state.benchmark_suite {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Benchmark suite is not enabled",
                    "hint": "Benchmark suite requires initialization at startup"
                })),
            )
                .into_response();
        }
    };

    let result = suite
        .run_benchmark(
            &req.model,
            &req.benchmark_type,
            req.prompt_tokens,
            req.concurrent_requests,
            req.timeout_secs,
        )
        .await;

    Json(result).into_response()
}

/// GET /api/benchmark/results
///
/// Get recent benchmark results (last 100).
async fn benchmark_results_handler(
    State(state): State<AppState>,
) -> Response {
    let suite = match &state.benchmark_suite {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Benchmark suite is not enabled"
                })),
            )
                .into_response();
        }
    };

    let results = suite.get_recent_results(100).await;
    Json(serde_json::json!({
        "results": results,
        "count": results.len(),
    }))
    .into_response()
}

/// GET /api/benchmark/history/{model}
///
/// Get benchmark history for a specific model.
async fn benchmark_history_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    let suite = match &state.benchmark_suite {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Benchmark suite is not enabled"
                })),
            )
                .into_response();
        }
    };

    let results = suite.get_model_history(&model).await;
    Json(serde_json::json!({
        "model": model,
        "results": results,
        "count": results.len(),
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Auto-Heal API handlers
// ---------------------------------------------------------------------------

/// POST /api/auto-heal/check
///
/// Trigger a manual auto-heal diagnostic check and corrective action cycle.
async fn auto_heal_check_handler(
    State(state): State<AppState>,
) -> Response {
    let healer = match &state.auto_healer {
        Some(h) => h,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Auto-heal system is not enabled",
                    "hint": "Set [auto_heal].enabled = true in config.toml"
                })),
            )
                .into_response();
        }
    };

    let status = healer.check_and_heal().await;
    Json(status).into_response()
}

/// GET /api/auto-heal/status
///
/// Get current auto-heal status (last check, actions taken, health summary).
async fn auto_heal_status_handler(
    State(state): State<AppState>,
) -> Response {
    let healer = match &state.auto_healer {
        Some(h) => h,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Auto-heal system is not enabled"
                })),
            )
                .into_response();
        }
    };

    let status = healer.get_status().await;
    Json(status).into_response()
}

/// GET /api/auto-heal/config
///
/// Get auto-heal configuration.
async fn auto_heal_config_handler(
    State(state): State<AppState>,
) -> Response {
    let healer = match &state.auto_healer {
        Some(h) => h,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Auto-heal system is not enabled"
                })),
            )
                .into_response();
        }
    };

    Json(healer.get_config()).into_response()
}

// ---------------------------------------------------------------------------
// Download progress handlers
// ---------------------------------------------------------------------------

/// GET /api/models/pull/progress -- list all active downloads with progress
async fn pull_progress_list_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match &state.download_progress {
        Some(tracker) => {
            let all = tracker.get_all_progress();
            Json(serde_json::json!({
                "downloads": all,
                "count": all.len(),
            }))
        }
        None => Json(serde_json::json!({
            "downloads": [],
            "count": 0,
            "hint": "Enable [auto_model_pull] to track download progress",
        })),
    }
}

/// GET /api/models/pull/progress/{model} -- get progress for specific model
async fn pull_progress_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    match &state.download_progress {
        Some(tracker) => match tracker.get_progress(&model) {
            Some(progress) => Json(serde_json::json!(progress)).into_response(),
            None => json_error(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("No download progress found for model '{}'", model),
            ),
        },
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_configured",
            "Download progress tracking is not enabled",
        ),
    }
}

/// GET /api/models/pull/progress/{model}/stream -- SSE stream of progress events
async fn pull_progress_stream_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    match &state.download_progress {
        Some(tracker) => {
            let initial = match tracker.get_progress(&model) {
                Some(p) => p,
                None => {
                    return json_error(
                        StatusCode::NOT_FOUND,
                        "not_found",
                        &format!("No download progress found for model '{}'", model),
                    );
                }
            };

            let initial_json = serde_json::to_string(&initial).unwrap_or_default();
            let model_filter = model.clone();
            let tracker = tracker.clone();

            let stream = async_stream::stream! {
                yield Ok::<_, std::convert::Infallible>(
                    Event::default().data(&initial_json)
                );

                let mut rx = tracker.subscribe();

                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            if event.model.eq_ignore_ascii_case(&model_filter) {
                                let json = serde_json::to_string(&event).unwrap_or_default();
                                yield Ok::<_, std::convert::Infallible>(
                                    Event::default().data(&json)
                                );

                                if matches!(
                                    event.status,
                                    crate::download_progress::DownloadStatus::Completed
                                        | crate::download_progress::DownloadStatus::Failed
                                        | crate::download_progress::DownloadStatus::Cancelled
                                ) {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!(skipped = n, "SSE subscriber lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
            };

            Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_configured",
            "Download progress tracking is not enabled",
        ),
    }
}

/// POST /api/models/pull/cancel/{model} -- cancel an in-progress download
async fn pull_cancel_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    match &state.download_progress {
        Some(tracker) => {
            if tracker.request_cancel(&model) {
                Json(serde_json::json!({
                    "model": model,
                    "status": "cancelled",
                    "message": "Download cancellation requested",
                }))
                .into_response()
            } else {
                json_error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!(
                        "No active download found for model '{}' (may already be completed/failed)",
                        model
                    ),
                )
            }
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_configured",
            "Download progress tracking is not enabled",
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Marketplace sync handlers
// ---------------------------------------------------------------------------

/// POST /api/marketplace/sync -- trigger a manual marketplace sync
async fn marketplace_sync_trigger_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sync = match &state.marketplace_sync {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Marketplace sync is not configured",
                    "hint": "Set [marketplace_sync].enabled = true and configure relay.relay_url"
                })),
            ));
        }
    };

    match sync.sync_now().await {
        Ok(()) => Ok(Json(serde_json::json!({
            "status": "success",
            "message": "Marketplace sync completed successfully",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "status": "error",
                "error": e,
                "timestamp": chrono::Utc::now().to_rfc3339()
            })),
        )),
    }
}

/// GET /api/marketplace/sync/status -- get last sync status
async fn marketplace_sync_status_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match &state.marketplace_sync {
        Some(sync) => {
            let status = sync.get_last_sync_status().await;
            Json(serde_json::json!({
                "enabled": true,
                "last_sync": status
            }))
        }
        None => Json(serde_json::json!({
            "enabled": false,
            "message": "Marketplace sync is not configured"
        })),
    }
}

/// GET /api/marketplace/sync/config -- get current sync configuration
async fn marketplace_sync_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match &state.marketplace_sync {
        Some(sync) => {
            let config = sync.get_config().await;
            Json(serde_json::json!({
                "enabled": config.enabled,
                "sync_interval_secs": config.sync_interval_secs,
                "include_benchmarks": config.include_benchmarks,
                "include_models": config.include_models,
                "include_gpu_info": config.include_gpu_info
            }))
        }
        None => Json(serde_json::json!({
            "enabled": false,
            "message": "Marketplace sync is not configured"
        })),
    }
}

/// PATCH /api/marketplace/sync/config -- update sync configuration at runtime
async fn marketplace_sync_config_update_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sync = match &state.marketplace_sync {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Marketplace sync is not configured"
                })),
            ));
        }
    };

    // Read current config and merge updates
    let current = sync.get_config().await;
    let updated = crate::marketplace_sync::MarketplaceSyncConfig {
        enabled: body
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(current.enabled),
        sync_interval_secs: body
            .get("sync_interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(current.sync_interval_secs),
        include_benchmarks: body
            .get("include_benchmarks")
            .and_then(|v| v.as_bool())
            .unwrap_or(current.include_benchmarks),
        include_models: body
            .get("include_models")
            .and_then(|v| v.as_bool())
            .unwrap_or(current.include_models),
        include_gpu_info: body
            .get("include_gpu_info")
            .and_then(|v| v.as_bool())
            .unwrap_or(current.include_gpu_info),
    };

    sync.update_config(updated).await;

    let config = sync.get_config().await;
    Ok(Json(serde_json::json!({
        "status": "updated",
        "config": {
            "enabled": config.enabled,
            "sync_interval_secs": config.sync_interval_secs,
            "include_benchmarks": config.include_benchmarks,
            "include_models": config.include_models,
            "include_gpu_info": config.include_gpu_info
        }
    })))
}

// ---------------------------------------------------------------------------
// Config hot-reload API handlers
// ---------------------------------------------------------------------------

/// POST /api/config/reload -- trigger manual config reload
async fn config_reload_handler(
    State(state): State<AppState>,
) -> Response {
    match &state.config_reloader {
        Some(reloader) => {
            let status = reloader.reload().await;
            if status.last_success {
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "status": "reloaded",
                        "config_path": status.config_path.to_string_lossy(),
                        "diff_summary": status.diff_summary,
                        "total_reloads": status.total_reloads,
                    })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "status": "error",
                        "error": status.last_error,
                        "config_path": status.config_path.to_string_lossy(),
                    })),
                )
                    .into_response()
            }
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "config_reload_disabled",
            "Config hot-reload is not enabled",
        ),
    }
}

/// GET /api/config/reload/status -- get reload status
async fn config_reload_status_handler(
    State(state): State<AppState>,
) -> Response {
    match &state.config_reloader {
        Some(reloader) => {
            let status = reloader.get_config_status().await;
            Json(serde_json::to_value(status).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "config_reload_disabled",
            "Config hot-reload is not enabled",
        ),
    }
}

// ---------------------------------------------------------------------------
// Model versioning API handlers
// ---------------------------------------------------------------------------

/// GET /api/models/versions -- list all models with their versions
async fn model_versions_list_all_handler(
    State(state): State<AppState>,
) -> Json<Vec<crate::model_versioning::ModelVersionsSummary>> {
    Json(state.model_registry.list_all())
}

/// GET /api/models/versions/{model} -- list versions for a model
async fn model_versions_list_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    let versions = state.model_registry.list_versions(&model);
    if versions.is_empty() {
        json_error(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("model '{}' not found or has no versions", model),
        )
    } else {
        Json(serde_json::json!({
            "model": model,
            "versions": versions,
            "active_version": state.model_registry.get_active_version(&model)
                .map(|v| v.version)
                .unwrap_or_default(),
        }))
            .into_response()
    }
}

/// GET /api/models/versions/{model}/{version} -- get specific version details
async fn model_versions_get_handler(
    State(state): State<AppState>,
    axum::extract::Path((model, version)): axum::extract::Path<(String, String)>,
) -> Response {
    match state.model_registry.get_version(&model, &version) {
        Some(v) => Json(v).into_response(),
        None => json_error(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("version '{}' not found for model '{}'", version, model),
        ),
    }
}

/// POST /api/models/versions/{model}/{version}/activate -- set as active version
async fn model_versions_activate_handler(
    State(state): State<AppState>,
    axum::extract::Path((model, version)): axum::extract::Path<(String, String)>,
) -> Response {
    match state.model_registry.set_active_version(&model, &version) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "activated",
                "model": model,
                "version": version,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "error": e,
            })),
        )
            .into_response(),
    }
}

/// POST /api/models/versions/{model}/{version}/tag -- set a tag
async fn model_versions_set_tag_handler(
    State(state): State<AppState>,
    axum::extract::Path((model, version)): axum::extract::Path<(String, String)>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Response {
    let tag = match body.get("tag").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "request body must include { \"tag\": \"...\" }",
            );
        }
    };

    match state.model_registry.set_tag(&model, &version, tag) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "tagged",
                "model": model,
                "version": version,
                "tag": tag,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "error": e,
            })),
        )
            .into_response(),
    }
}

/// DELETE /api/models/versions/{model}/{version} -- remove a version
async fn model_versions_delete_handler(
    State(state): State<AppState>,
    axum::extract::Path((model, version)): axum::extract::Path<(String, String)>,
) -> Response {
    match state.model_registry.remove_version(&model, &version) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "removed",
                "model": model,
                "version": version,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "error": e,
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Auto-scale API handlers
// ---------------------------------------------------------------------------

/// GET /api/auto-scale/status
async fn auto_scale_status_handler(
    State(state): State<AppState>,
) -> Result<Json<crate::auto_scale::AutoScaleStatus>, StatusCode> {
    let scaler = state
        .auto_scaler
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(scaler.get_status().await))
}

/// POST /api/auto-scale/trigger
async fn auto_scale_trigger_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::auto_scale::ScaleAction>>, StatusCode> {
    let scaler = state
        .auto_scaler
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(scaler.trigger().await))
}

/// GET /api/auto-scale/config
async fn auto_scale_config_handler(
    State(state): State<AppState>,
) -> Result<Json<crate::auto_scale::AutoScaleConfig>, StatusCode> {
    let scaler = state
        .auto_scaler
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(scaler.get_config().await))
}

/// PATCH /api/auto-scale/config
async fn auto_scale_config_update_handler(
    State(state): State<AppState>,
    Json(config): Json<crate::auto_scale::AutoScaleConfig>,
) -> StatusCode {
    let Some(scaler) = state.auto_scaler.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    scaler.update_config(config).await;
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// Reputation Dashboard API handlers
// ---------------------------------------------------------------------------

/// GET /api/reputation/leaderboard?limit=20
async fn reputation_leaderboard_handler(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<crate::reputation_dashboard::LeaderboardEntry>>, StatusCode> {
    let dashboard = state
        .reputation_dashboard
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit: usize = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let leaderboard = dashboard.get_leaderboard(limit);
    Ok(Json(leaderboard))
}

/// GET /api/reputation/provider/{pk}
async fn reputation_provider_handler(
    State(state): State<AppState>,
    Path(pk): Path<String>,
) -> Result<Json<crate::reputation_dashboard::ProviderReputationDetail>, StatusCode> {
    let dashboard = state
        .reputation_dashboard
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    dashboard
        .get_provider_reputation_detail(&pk)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET /api/reputation/stats
async fn reputation_stats_handler(
    State(state): State<AppState>,
) -> Result<Json<crate::reputation_dashboard::NetworkReputationStats>, StatusCode> {
    let dashboard = state
        .reputation_dashboard
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(dashboard.get_network_stats()))
}

/// GET /api/reputation/history/{pk}?days=30
async fn reputation_history_handler(
    State(state): State<AppState>,
    Path(pk): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<crate::reputation_dashboard::ReputationHistory>, StatusCode> {
    let dashboard = state
        .reputation_dashboard
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let days: usize = params
        .get("days")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    Ok(Json(dashboard.get_reputation_history(&pk, days)))
}

// ---------------------------------------------------------------------------
// Inference Queue handlers
// ---------------------------------------------------------------------------

/// GET /api/queue/status
async fn queue_status_handler(State(state): State<AppState>) -> Response {
    match &state.inference_queue {
        Some(queue) => {
            let status = queue.get_status().await;
            Json(serde_json::to_value(status).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Inference queue is not enabled",
        ),
    }
}

/// GET /api/queue/stats
async fn queue_stats_handler(State(state): State<AppState>) -> Response {
    match &state.inference_queue {
        Some(queue) => {
            let stats = queue.get_stats();
            Json(serde_json::to_value(stats).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Inference queue is not enabled",
        ),
    }
}

/// POST /api/queue/clear -- admin: clear all queues
async fn queue_clear_handler(State(state): State<AppState>) -> Response {
    match &state.inference_queue {
        Some(queue) => {
            queue.clear().await;
            Json(serde_json::json!({ "status": "ok", "message": "All queues cleared" }))
                .into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Inference queue is not enabled",
        ),
    }
}

// ---------------------------------------------------------------------------
// Model Health handlers
// ---------------------------------------------------------------------------

/// GET /api/models/health
async fn models_health_handler(State(state): State<AppState>) -> Response {
    match &state.model_health_monitor {
        Some(monitor) => {
            let summary = monitor.get_all_health().await;
            Json(serde_json::to_value(summary).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Model health monitor is not enabled",
        ),
    }
}

/// GET /api/models/health/{model}
async fn models_health_single_handler(
    State(state): State<AppState>,
    Path(model): Path<String>,
) -> Response {
    match &state.model_health_monitor {
        Some(monitor) => match monitor.get_health(&model).await {
            Some(health) => Json(serde_json::to_value(health).unwrap_or_default()).into_response(),
            None => json_error(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("Model '{}' not found in health monitor", model),
            ),
        },
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Model health monitor is not enabled",
        ),
    }
}

/// POST /api/models/health/{model}/check -- trigger manual health check
async fn models_health_check_handler(
    State(state): State<AppState>,
    Path(model): Path<String>,
) -> Response {
    match &state.model_health_monitor {
        Some(monitor) => {
            let result = monitor.check_model(&model).await;
            Json(serde_json::to_value(result).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Model health monitor is not enabled",
        ),
    }
}

// ---------------------------------------------------------------------------
// Provider Mesh handlers
// ---------------------------------------------------------------------------

/// GET /api/mesh/status
async fn mesh_status_handler(State(state): State<AppState>) -> Response {
    match &state.provider_mesh {
        Some(mesh) => {
            let status = mesh.get_mesh_status().await;
            Json(serde_json::to_value(status).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Provider mesh is not enabled",
        ),
    }
}

/// GET /api/mesh/peers
async fn mesh_peers_handler(State(state): State<AppState>) -> Response {
    match &state.provider_mesh {
        Some(mesh) => {
            let peers = mesh.get_peers();
            Json(serde_json::to_value(peers).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Provider mesh is not enabled",
        ),
    }
}

/// POST /api/mesh/sync -- trigger manual sync
async fn mesh_sync_handler(State(state): State<AppState>) -> Response {
    match &state.provider_mesh {
        Some(mesh) => {
            let result = mesh.sync().await;
            Json(serde_json::to_value(result).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Provider mesh is not enabled",
        ),
    }
}

/// GET /api/mesh/models
async fn mesh_models_handler(State(state): State<AppState>) -> Response {
    match &state.provider_mesh {
        Some(mesh) => {
            let models = mesh.get_mesh_models_detailed();
            Json(serde_json::to_value(models).unwrap_or_default()).into_response()
        }
        None => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "Provider mesh is not enabled",
        ),
    }
}

// ---------------------------------------------------------------------------
// Fine-tune orchestration API handlers
// ---------------------------------------------------------------------------

/// POST /api/fine-tune/create
async fn fine_tune_create_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::fine_tune::CreateJobRequest>,
) -> Response {
    match state.fine_tune.create_job(req) {
        Ok(resp) => (StatusCode::CREATED, Json(resp)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/fine-tune/jobs
async fn fine_tune_jobs_handler(State(state): State<AppState>) -> Json<Vec<crate::fine_tune::FineTuneJob>> {
    Json(state.fine_tune.list_jobs())
}

/// GET /api/fine-tune/jobs/{id}
async fn fine_tune_job_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.fine_tune.get_job(&id) {
        Some(job) => Json(job).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Job not found"}))).into_response(),
    }
}

/// POST /api/fine-tune/jobs/{id}/cancel
async fn fine_tune_cancel_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.fine_tune.cancel_job(&id) {
        Ok(()) => Json(serde_json::json!({"status": "cancelled", "id": id})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/fine-tune/jobs/{id}/export
async fn fine_tune_export_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::fine_tune::ExportRequest>,
) -> Response {
    match state.fine_tune.export_job(&id, req) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// A/B Testing API handlers
// ---------------------------------------------------------------------------

/// POST /api/experiments/create
async fn experiment_create_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::ab_testing::CreateExperimentRequest>,
) -> Response {
    match state.ab_testing.create_experiment(req) {
        Ok(exp) => (StatusCode::CREATED, Json(exp)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/experiments
async fn experiment_list_handler(State(state): State<AppState>) -> Json<Vec<crate::ab_testing::Experiment>> {
    Json(state.ab_testing.list_experiments())
}

/// GET /api/experiments/{id}
async fn experiment_get_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing.get_experiment(&id) {
        Some(exp) => Json(exp).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Experiment not found"}))).into_response(),
    }
}

/// POST /api/experiments/{id}/pause
async fn experiment_pause_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing.pause_experiment(&id) {
        Ok(exp) => Json(exp).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/experiments/{id}/resume
async fn experiment_resume_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing.resume_experiment(&id) {
        Ok(exp) => Json(exp).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/experiments/{id}/end
async fn experiment_end_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.ab_testing.end_experiment(&id) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/experiments/{id}/feedback
async fn experiment_feedback_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::ab_testing::FeedbackRequest>,
) -> Response {
    match state.ab_testing.submit_feedback(&id, req) {
        Ok(()) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Multi-GPU management API handlers
// ---------------------------------------------------------------------------

/// GET /api/gpu/devices
async fn gpu_devices_handler(State(state): State<AppState>) -> Json<Vec<crate::multi_gpu::GpuDevice>> {
    Json(state.multi_gpu.list_devices())
}

/// GET /api/gpu/multi/config
async fn gpu_config_handler(State(state): State<AppState>) -> Response {
    let config = state.multi_gpu.get_config().await;
    Json(config).into_response()
}

/// PATCH /api/gpu/multi/config
async fn gpu_config_update_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::multi_gpu::UpdateGpuConfigRequest>,
) -> Response {
    match state.multi_gpu.update_config(req).await {
        Ok(config) => Json(config).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/gpu/usage
async fn gpu_usage_handler(State(state): State<AppState>) -> Json<Vec<crate::multi_gpu::GpuUsageInfo>> {
    Json(state.multi_gpu.get_usage())
}

// ---------------------------------------------------------------------------
// Container runtime API handlers
// ---------------------------------------------------------------------------

/// POST /api/containers/create
async fn container_create_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::container::CreateContainerRequest>,
) -> Response {
    match state.container_runtime.create_container(req).await {
        Ok(container) => (StatusCode::CREATED, Json(container)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/containers
async fn container_list_handler(State(state): State<AppState>) -> Json<Vec<crate::container::Container>> {
    Json(state.container_runtime.list_containers())
}

/// GET /api/containers/{id}
async fn container_get_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.container_runtime.get_container(&id) {
        Some(container) => Json(container).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Container not found"}))).into_response(),
    }
}

/// POST /api/containers/{id}/stop
async fn container_stop_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.container_runtime.stop_container(&id).await {
        Ok(container) => Json(container).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/containers/{id}/start
async fn container_start_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.container_runtime.start_container(&id).await {
        Ok(container) => Json(container).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/containers/{id}/logs
async fn container_logs_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<crate::container::ContainerLogsQuery>,
) -> Response {
    match state.container_runtime.get_container_logs(&id, query.tail).await {
        Ok(logs) => Json(serde_json::json!({"logs": logs})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Model sharding API handlers
// ---------------------------------------------------------------------------

/// GET /api/sharding/status
async fn sharding_status_handler(State(state): State<AppState>) -> Json<crate::model_sharding::ShardingStatus> {
    Json(state.model_shard_manager.get_status())
}

/// POST /api/sharding/shard
async fn sharding_shard_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::model_sharding::ShardModelRequest>,
) -> Response {
    match state.model_shard_manager.shard_model(req) {
        Ok(shards) => (StatusCode::CREATED, Json(shards)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// DELETE /api/sharding/shard/{model}
async fn sharding_unshard_handler(
    State(state): State<AppState>,
    Path(model): Path<String>,
) -> Response {
    match state.model_shard_manager.unshard_model(&model) {
        Ok(()) => Json(serde_json::json!({"status": "unsharded", "model": model})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/sharding/models
async fn sharding_models_handler(State(state): State<AppState>) -> Json<Vec<crate::model_sharding::ModelShardingStatus>> {
    Json(state.model_shard_manager.list_sharded_models())
}

// ---------------------------------------------------------------------------
// Distributed inference API handlers
// ---------------------------------------------------------------------------

/// GET /api/distributed/nodes
async fn distributed_nodes_handler(State(state): State<AppState>) -> Json<Vec<crate::distributed_inference::InferenceNode>> {
    Json(state.distributed_inference.list_nodes().await)
}

/// GET /api/distributed/status
async fn distributed_status_handler(State(state): State<AppState>) -> Json<crate::distributed_inference::ClusterStatus> {
    Json(state.distributed_inference.get_cluster_status().await)
}

/// POST /api/distributed/forward
async fn distributed_forward_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::distributed_inference::ForwardRequest>,
) -> Response {
    match state.distributed_inference.forward_request(req).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/distributed/metrics
async fn distributed_metrics_handler(State(state): State<AppState>) -> Json<crate::distributed_inference::DistributedMetrics> {
    Json(state.distributed_inference.get_metrics().await)
}

// ---------------------------------------------------------------------------
// Sandbox API handlers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tensor Pipeline API handlers
// ---------------------------------------------------------------------------

/// GET /api/tensor-pipeline/status
async fn tensor_pipeline_status_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pipelines = state.tensor_pipeline.list_pipelines();
    let metrics = state.tensor_pipeline.get_metrics();
    Json(serde_json::json!({
        "pipelines": pipelines.len(),
        "metrics": metrics,
    }))
}

/// GET /api/tensor-pipeline/config
async fn tensor_pipeline_config_handler(State(state): State<AppState>) -> Json<Vec<crate::tensor_pipeline::PipelineState>> {
    Json(state.tensor_pipeline.list_pipelines())
}

/// POST /api/tensor-pipeline/config — creates a new pipeline
async fn tensor_pipeline_config_update_handler(
    State(state): State<AppState>,
    Json(config): Json<crate::tensor_pipeline::PipelineConfig>,
) -> Response {
    let created = state.tensor_pipeline.create_pipeline(config);
    Json(serde_json::json!({
        "id": created.id,
        "status": created.status,
    }))
    .into_response()
}

/// GET /api/tensor-pipeline/stats
async fn tensor_pipeline_stats_handler(State(state): State<AppState>) -> Json<crate::tensor_pipeline::PipelineMetricsSnapshot> {
    Json(state.tensor_pipeline.get_metrics())
}

/// POST /api/tensor-pipeline/execute/{id}
async fn tensor_pipeline_execute_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::tensor_pipeline::ExecuteRequest>,
) -> Response {
    match state.tensor_pipeline.execute(&id, req) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/tensor-pipeline/pipelines
async fn tensor_pipeline_list_handler(State(state): State<AppState>) -> Json<Vec<crate::tensor_pipeline::PipelineState>> {
    Json(state.tensor_pipeline.list_pipelines())
}

/// GET /api/tensor-pipeline/pipelines/{id}
async fn tensor_pipeline_get_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.tensor_pipeline.get_pipeline(&id) {
        Some(pipeline) => Json(pipeline).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Pipeline not found"}))).into_response(),
    }
}

/// DELETE /api/tensor-pipeline/pipelines/{id}
async fn tensor_pipeline_delete_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let deleted = state.tensor_pipeline.delete_pipeline(&id);
    Json(serde_json::json!({"deleted": deleted}))
}

/// POST /api/tensor-pipeline/pipelines/{id}/pause
async fn tensor_pipeline_pause_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.tensor_pipeline.pause_pipeline(&id) {
        Ok(()) => Json(serde_json::json!({"status": "paused"})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/tensor-pipeline/pipelines/{id}/resume
async fn tensor_pipeline_resume_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.tensor_pipeline.resume_pipeline(&id) {
        Ok(()) => Json(serde_json::json!({"status": "resumed"})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Sandbox API handlers
// ---------------------------------------------------------------------------

/// GET /api/sandbox/status
async fn sandbox_status_handler(State(state): State<AppState>) -> Json<crate::sandbox::SandboxStatusResponse> {
    Json(state.sandbox_manager.get_status().await)
}

/// PATCH /api/sandbox/config
async fn sandbox_config_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::sandbox::UpdateSandboxConfigRequest>,
) -> Response {
    match state.sandbox_manager.update_config(req).await {
        Ok(config) => Json(config).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/sandbox/metrics
async fn sandbox_metrics_handler(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let sandbox_id = query.get("sandbox_id").cloned().unwrap_or_default();
    if sandbox_id.is_empty() {
        // Return overall status as metrics
        return Json(state.sandbox_manager.get_status().await).into_response();
    }
    match state.sandbox_manager.get_metrics(&sandbox_id) {
        Ok(metrics) => Json(metrics).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/sandbox/test
async fn sandbox_test_handler(State(state): State<AppState>) -> Json<crate::sandbox::SandboxTestResult> {
    Json(state.sandbox_manager.test_sandbox().await)
}

// ---------------------------------------------------------------------------
// Marketplace listing API handlers
// ---------------------------------------------------------------------------

/// POST /api/marketplace/listings
async fn marketplace_create_listing_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::marketplace_listing::CreateListingRequest>,
) -> Response {
    match state.marketplace_listing.create_listing(req).await {
        Ok(listing) => (StatusCode::CREATED, Json(listing)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/marketplace/listings
async fn marketplace_list_listings_handler(State(state): State<AppState>) -> Json<Vec<crate::marketplace_listing::ListingSummary>> {
    Json(state.marketplace_listing.list_listings())
}

/// GET /api/marketplace/listings/{id}
async fn marketplace_get_listing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.marketplace_listing.get_listing(&id) {
        Some(listing) => Json(listing).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Listing not found"}))).into_response(),
    }
}

/// PATCH /api/marketplace/listings/{id}
async fn marketplace_update_listing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::marketplace_listing::UpdateListingRequest>,
) -> Response {
    match state.marketplace_listing.update_listing(&id, req) {
        Ok(listing) => Json(listing).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// DELETE /api/marketplace/listings/{id}
async fn marketplace_delete_listing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.marketplace_listing.delete_listing(&id) {
        Ok(()) => Json(serde_json::json!({"status": "deleted", "id": id})).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/marketplace/listings/{id}/publish
async fn marketplace_publish_listing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.marketplace_listing.publish_listing(&id) {
        Ok(listing) => Json(listing).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/marketplace/listings/{id}/deprecate
async fn marketplace_deprecate_listing_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.marketplace_listing.deprecate_listing(&id) {
        Ok(listing) => Json(listing).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Observability API handlers
// ---------------------------------------------------------------------------

/// GET /api/observability/traces?model=&limit=50
async fn observability_traces_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<Vec<crate::observability::InferenceSpan>> {
    let model = params.get("model").map(|s| s.as_str());
    let limit: usize = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(50);
    let traces = state.observability.query_traces(model, limit).await;
    Json(traces)
}

/// GET /api/observability/traces/{trace_id}
async fn observability_trace_handler(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
) -> Response {
    let spans = state.observability.get_trace(&trace_id).await;
    if spans.is_empty() {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Trace not found"}))).into_response()
    } else {
        Json(spans).into_response()
    }
}

/// GET /api/observability/metrics
async fn observability_metrics_handler(State(state): State<AppState>) -> Json<std::collections::HashMap<String, f64>> {
    Json(state.observability.get_metrics())
}

/// GET /api/observability/config
async fn observability_config_handler(State(state): State<AppState>) -> Json<crate::observability::ObservabilityConfig> {
    Json(state.observability.get_config().await)
}

/// PATCH /api/observability/config
async fn observability_config_update_handler(
    State(state): State<AppState>,
    Json(update): Json<crate::observability::ObservabilityConfigUpdate>,
) -> Json<crate::observability::ObservabilityConfig> {
    state.observability.update_config(update).await;
    Json(state.observability.get_config().await)
}

// ---------------------------------------------------------------------------
// Model Compression API handlers
// ---------------------------------------------------------------------------

/// POST /api/compression/create
async fn compression_create_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::model_compression::CreateCompressionJobRequest>,
) -> Response {
    match state.compression.create_job(req) {
        Ok(resp) => (StatusCode::CREATED, Json(resp)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/compression/jobs
async fn compression_jobs_handler(State(state): State<AppState>) -> Json<Vec<crate::model_compression::CompressionJob>> {
    Json(state.compression.list_jobs())
}

/// GET /api/compression/jobs/{id}
async fn compression_job_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.compression.get_job(&id) {
        Some(job) => Json(job).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Job not found"}))).into_response(),
    }
}

/// POST /api/compression/jobs/{id}/cancel
async fn compression_cancel_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.compression.cancel_job(&id) {
        Ok(()) => Json(serde_json::json!({"status": "cancelled", "id": id})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// DELETE /api/compression/jobs/{id}
async fn compression_delete_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.compression.delete_job(&id) {
        Ok(()) => Json(serde_json::json!({"status": "deleted", "id": id})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/compression/estimate
async fn compression_estimate_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::model_compression::CompressionEstimateRequest>,
) -> Json<crate::model_compression::CompressionEstimate> {
    Json(state.compression.estimate(req))
}

// ---------------------------------------------------------------------------
// Inference Cache API handlers
// ---------------------------------------------------------------------------

/// GET /api/inference-cache/stats
async fn inference_cache_stats_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let stats = state.inference_cache.stats();
    let entries = state.inference_cache.len();
    let mut map = serde_json::to_value(stats).unwrap_or_default();
    if let Some(obj) = map.as_object_mut() {
        obj.insert("entries".to_string(), serde_json::json!(entries));
    }
    Json(map)
}

/// DELETE /api/inference-cache/clear
async fn inference_cache_clear_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let evicted = state.inference_cache.clear();
    Json(serde_json::json!({"status": "cleared", "evicted": evicted}))
}

/// GET /api/inference-cache/entries
async fn inference_cache_entries_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let offset = params.get("offset").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let limit = params.get("limit").and_then(|v| v.parse::<usize>().ok()).unwrap_or(50);
    let entries = state.inference_cache.list_entries(offset, limit);
    Json(entries).into_response()
}

/// DELETE /api/inference-cache/entries/{id}
async fn inference_cache_evict_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    if state.inference_cache.evict(&id) {
        Json(serde_json::json!({"status": "evicted", "id": id})).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Entry not found"}))).into_response()
    }
}

/// PATCH /api/inference-cache/config
async fn inference_cache_config_handler(
    State(state): State<AppState>,
    Json(update): Json<crate::inference_cache::UpdateCacheConfigRequest>,
) -> Json<crate::inference_cache::InferenceCacheConfig> {
    Json(state.inference_cache.update_config(update).await)
}

// ---------------------------------------------------------------------------
// GPU Memory API handlers
// ---------------------------------------------------------------------------

/// GET /api/gpu-memory/devices
async fn gpu_memory_devices_handler(State(state): State<AppState>) -> Json<Vec<crate::gpu_memory::DeviceMemoryInfo>> {
    Json(state.gpu_memory.get_device_memory_info())
}

/// GET /api/gpu-memory/allocations
async fn gpu_memory_allocations_handler(State(state): State<AppState>) -> Json<Vec<crate::gpu_memory::GpuMemoryRegion>> {
    Json(state.gpu_memory.get_allocations())
}

/// GET /api/gpu-memory/available
async fn gpu_memory_available_handler(State(state): State<AppState>) -> Json<std::collections::HashMap<u32, u64>> {
    Json(state.gpu_memory.get_all_available())
}

/// POST /api/gpu-memory/allocate
async fn gpu_memory_allocate_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::gpu_memory::AllocateRequest>,
) -> Response {
    match state.gpu_memory.allocate(req.device_id, req.size_mb, &req.owner) {
        Ok(region_id) => (StatusCode::CREATED, Json(crate::gpu_memory::AllocateResponse {
            region_id: region_id.clone(),
            device_id: req.device_id,
            offset: 0,
            size_mb: req.size_mb,
            owner: req.owner,
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// DELETE /api/gpu-memory/allocate/{region_id}
async fn gpu_memory_deallocate_handler(
    State(state): State<AppState>,
    Path(region_id): Path<String>,
) -> Response {
    match state.gpu_memory.deallocate(&region_id) {
        Ok(freed_mb) => Json(serde_json::json!({"status": "deallocated", "freed_mb": freed_mb})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/gpu-memory/fragmentation
async fn gpu_memory_fragmentation_handler(State(state): State<AppState>) -> Response {
    let devices = state.gpu_memory.get_devices();
    let mut results = Vec::new();
    for device in &devices {
        if let Some(stats) = state.gpu_memory.get_fragmentation(device.id) {
            results.push(stats);
        }
    }
    Json(results).into_response()
}

/// POST /api/gpu-memory/defrag
async fn gpu_memory_defrag_handler(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Response {
    let device_id = req.get("device_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    match state.gpu_memory.suggest_defrag(device_id) {
        Ok(plan) => Json(plan).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Model Migration API handlers
// ---------------------------------------------------------------------------

/// POST /api/migration/create
async fn migration_create_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::model_migration::CreateMigrationRequest>,
) -> Response {
    let resp = state.model_migration.create_from_request(req);
    (StatusCode::CREATED, Json(resp)).into_response()
}

/// GET /api/migration/jobs
async fn migration_jobs_handler(State(state): State<AppState>) -> Json<Vec<crate::model_migration::MigrationJob>> {
    Json(state.model_migration.list_jobs())
}

/// GET /api/migration/jobs/{id}
async fn migration_job_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.model_migration.get_job(&id) {
        Some(job) => Json(job).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Job not found"}))).into_response(),
    }
}

/// POST /api/migration/jobs/{id}/pause
async fn migration_pause_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.model_migration.pause_job(&id) {
        Ok(()) => Json(serde_json::json!({"status": "paused", "id": id})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/migration/jobs/{id}/resume
async fn migration_resume_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.model_migration.resume_job(&id) {
        Ok(checkpoint) => Json(serde_json::json!({"status": "resumed", "id": id, "checkpoint": checkpoint})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/migration/jobs/{id}/cancel
async fn migration_cancel_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.model_migration.cancel_job(&id) {
        Ok(()) => Json(serde_json::json!({"status": "cancelled", "id": id})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/migration/validate
async fn migration_validate_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::model_migration::ValidateMigrationRequest>,
) -> Json<crate::model_migration::MigrationValidationResult> {
    Json(state.model_migration.validate_migration(req).await)
}

// ---------------------------------------------------------------------------
// Warm-Up Pool handlers
// ---------------------------------------------------------------------------

/// GET /api/warmup/status
async fn warmup_status_handler(State(state): State<AppState>) -> Response {
    let status = state.warmup_pool.get_status().await;
    Json(serde_json::to_value(status).unwrap_or_default()).into_response()
}

/// POST /api/warmup/load
async fn warmup_load_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::warmup::LoadModelRequest>,
) -> Response {
    match state
        .warmup_pool
        .warm_model(&req.model_id, req.vram_reserved, req.priority)
        .await
    {
        Ok(status) => (
            StatusCode::OK,
            Json(crate::warmup::LoadModelResponse {
                model_id: req.model_id,
                status,
                message: "Model warmed up successfully".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(crate::warmup::LoadModelResponse {
                model_id: req.model_id,
                status: crate::warmup::WarmupStatus::Unloaded,
                message: e,
            }),
        )
            .into_response(),
    }
}

/// POST /api/warmup/unload
async fn warmup_unload_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::warmup::UnloadModelRequest>,
) -> Response {
    let evicted = state.warmup_pool.unload_model(&req.model_id);
    (
        StatusCode::OK,
        Json(crate::warmup::UnloadModelResponse {
            model_id: req.model_id,
            evicted,
            message: if evicted {
                "Model unloaded".to_string()
            } else {
                "Model not found in pool".to_string()
            },
        }),
    )
        .into_response()
}

/// PATCH /api/warmup/config
async fn warmup_config_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::warmup::UpdateWarmupConfigRequest>,
) -> Response {
    let new_config = state.warmup_pool.update_config(req).await;
    Json(serde_json::to_value(new_config).unwrap_or_default()).into_response()
}

/// GET /api/warmup/stats
async fn warmup_stats_handler(State(state): State<AppState>) -> Response {
    let stats = state.warmup_pool.get_stats();
    Json(serde_json::to_value(stats).unwrap_or_default()).into_response()
}

// ---------------------------------------------------------------------------
// Inference Batching API handlers
// ---------------------------------------------------------------------------

/// GET /api/batch/stats
async fn batch_stats_handler(State(state): State<AppState>) -> Response {
    let stats = state.inference_batcher.get_stats();
    let pending = state.inference_batcher.pending_count();
    let effective = state.inference_batcher.effective_batch_size();
    Json(crate::inference_batch::BatchStatsResponse {
        stats,
        pending_count: pending,
        effective_batch_size: effective,
    })
    .into_response()
}

/// GET /api/batch/config
async fn batch_config_handler(State(state): State<AppState>) -> Response {
    let config = state.inference_batcher.get_config().await;
    Json(config).into_response()
}

/// PATCH /api/batch/config
async fn batch_config_update_handler(
    State(state): State<AppState>,
    Json(update): Json<crate::inference_batch::BatchConfigUpdate>,
) -> Response {
    let config = state.inference_batcher.update_config(update).await;
    Json(config).into_response()
}

/// POST /api/batch/flush
async fn batch_flush_handler(State(state): State<AppState>) -> Response {
    state.inference_batcher.flush().await;
    Json(crate::inference_batch::BatchFlushResponse {
        flushed: true,
        message: "Pending batches flushed".to_string(),
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// Checkpoint Management API handlers
// ---------------------------------------------------------------------------

/// POST /api/checkpoint/create
async fn checkpoint_create_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::checkpoint::CreateCheckpointRequest>,
) -> Response {
    let resp = state.checkpoint_manager.create(req).await;
    (axum::http::StatusCode::CREATED, Json(resp)).into_response()
}

/// GET /api/checkpoint/list
async fn checkpoint_list_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let model_id = params.get("model_id").map(|s| s.as_str());
    let tag = params.get("tag").map(|s| s.as_str());
    let list = state.checkpoint_manager.list(model_id, tag).await;
    Json(list).into_response()
}

/// GET /api/checkpoint/{id}
async fn checkpoint_get_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.checkpoint_manager.get(&id) {
        Some(cp) => Json(cp).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Checkpoint not found"})),
        )
            .into_response(),
    }
}

/// POST /api/checkpoint/{id}/restore
async fn checkpoint_restore_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let resp = state.checkpoint_manager.restore(&id).await;
    let status = if resp.restored {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    };
    (status, Json(resp)).into_response()
}

/// DELETE /api/checkpoint/{id}
async fn checkpoint_delete_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let resp = state.checkpoint_manager.delete(&id).await;
    let status = if resp.deleted {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    };
    (status, Json(resp)).into_response()
}

/// POST /api/checkpoint/{id}/compare
async fn checkpoint_compare_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<crate::checkpoint::CompareCheckpointRequest>,
) -> Response {
    match state.checkpoint_manager.compare(&id, &req.other_checkpoint_id).await {
        Some(diff) => Json(diff).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "One or both checkpoints not found"})),
        )
            .into_response(),
    }
}

/// GET /api/checkpoint/config
async fn checkpoint_config_handler(State(state): State<AppState>) -> Response {
    let config = state.checkpoint_manager.get_config().await;
    Json(config).into_response()
}

/// PATCH /api/checkpoint/config
async fn checkpoint_config_update_handler(
    State(state): State<AppState>,
    Json(update): Json<crate::checkpoint::CheckpointConfigUpdate>,
) -> Response {
    let config = state.checkpoint_manager.update_config(update).await;
    Json(config).into_response()
}

// ---------------------------------------------------------------------------
// Resource Quota API handlers
// ---------------------------------------------------------------------------

/// GET /api/quotas
async fn quotas_list_handler(State(state): State<AppState>) -> Response {
    let entries = state.quota_manager.list_quotas().await;
    Json(entries).into_response()
}

/// GET /api/quotas/config
async fn quotas_config_handler(State(state): State<AppState>) -> Response {
    let config = state.quota_manager.get_config().await;
    Json(config).into_response()
}

/// PATCH /api/quotas/config
async fn quotas_config_update_handler(
    State(state): State<AppState>,
    Json(update): Json<crate::resource_quotas::QuotaConfigUpdate>,
) -> Response {
    let config = state.quota_manager.update_config(update).await;
    Json(config).into_response()
}

/// GET /api/quotas/alerts
async fn quotas_alerts_handler(State(state): State<AppState>) -> Response {
    let alerts = state.quota_manager.get_alerts(None, 100).await;
    Json(alerts).into_response()
}

/// GET /api/quotas/{subject_id}
async fn quotas_get_handler(
    State(state): State<AppState>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
) -> Response {
    match state.quota_manager.get_quota(&subject_id).await {
        Some(entry) => Json(entry).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Quota entry not found"})),
        )
            .into_response(),
    }
}

/// PUT /api/quotas/{subject_id}
async fn quotas_set_handler(
    State(state): State<AppState>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
    Json(req): Json<crate::resource_quotas::SetQuotaRequest>,
) -> Response {
    let set = state.quota_manager.set_quota(&subject_id, req.quota).await;
    if set {
        (axum::http::StatusCode::OK, Json(serde_json::json!({"message": "Quota updated"}))).into_response()
    } else {
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to set quota"}))).into_response()
    }
}

/// GET /api/quotas/{subject_id}/usage
async fn quotas_usage_handler(
    State(state): State<AppState>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
) -> Response {
    match state.quota_manager.get_usage(&subject_id).await {
        Some(usage) => Json(usage).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Usage not found for subject"})),
        )
            .into_response(),
    }
}

/// POST /api/quotas/{subject_id}/reset
async fn quotas_reset_handler(
    State(state): State<AppState>,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
) -> Response {
    let resp = state.quota_manager.reset_usage(&subject_id).await;
    Json(resp).into_response()
}

// =========================================================================
// Inference Profiler Handlers
// =========================================================================

/// GET /api/profiler/stats
async fn profiler_stats_handler(State(state): State<AppState>) -> Response {
    let stats = state.profiler.get_stats().await;
    Json(stats).into_response()
}

/// GET /api/profiler/profiles
async fn profiler_list_handler(State(state): State<AppState>) -> Response {
    let profiles = state.profiler.list_profiles(0, 50);
    Json(profiles).into_response()
}

/// GET /api/profiler/profiles/{id}
async fn profiler_get_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.profiler.get_profile(&id) {
        Some(profile) => Json(profile).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Profile not found").into_response(),
    }
}

/// GET /api/profiler/models/{model}/summary
async fn profiler_model_summary_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    let summary = state.profiler.get_model_summary(&model).await;
    Json(summary).into_response()
}

/// GET /api/profiler/compare?id_a=..&id_b=..
async fn profiler_compare_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let id_a = match params.get("id_a") {
        Some(v) => v.clone(),
        None => return (axum::http::StatusCode::BAD_REQUEST, "Missing id_a").into_response(),
    };
    let id_b = match params.get("id_b") {
        Some(v) => v.clone(),
        None => return (axum::http::StatusCode::BAD_REQUEST, "Missing id_b").into_response(),
    };
    match state.profiler.compare_profiles(&id_a, &id_b) {
        Ok(comparison) => Json(comparison).into_response(),
        Err(e) => (axum::http::StatusCode::NOT_FOUND, e).into_response(),
    }
}

/// POST /api/profiler/collect
async fn profiler_collect_handler(State(state): State<AppState>) -> Response {
    let id = state.profiler.manual_collect().await;
    Json(serde_json::json!({ "profile_id": id })).into_response()
}

/// DELETE /api/profiler/profiles
async fn profiler_clear_handler(State(state): State<AppState>) -> Response {
    let count = state.profiler.clear_profiles();
    Json(serde_json::json!({ "cleared": count })).into_response()
}

/// PATCH /api/profiler/config
async fn profiler_config_handler(
    State(state): State<AppState>,
    axum::extract::Json(update): axum::extract::Json<crate::inference_profiler::UpdateProfilerConfigRequest>,
) -> Response {
    let config = state.profiler.update_config(update).await;
    Json(config).into_response()
}

// =========================================================================
// GPU Scheduler Handlers
// =========================================================================

/// GET /api/gpu-scheduler/status
async fn gpu_scheduler_status_handler(State(state): State<AppState>) -> Response {
    let status = state.gpu_scheduler.get_status().await;
    Json(status).into_response()
}

/// GET /api/gpu-scheduler/devices
async fn gpu_scheduler_devices_handler(State(state): State<AppState>) -> Response {
    let devices = state.gpu_scheduler.get_device_schedules();
    Json(devices).into_response()
}

/// GET /api/gpu-scheduler/queue
async fn gpu_scheduler_queue_handler(State(state): State<AppState>) -> Response {
    let queue = state.gpu_scheduler.get_queue();
    Json(queue).into_response()
}

/// POST /api/gpu-scheduler/affinity
async fn gpu_scheduler_set_affinity_handler(
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> Response {
    let model = match body.get("model").and_then(|v| v.as_str()) {
        Some(v) => v.to_string(),
        None => return (axum::http::StatusCode::BAD_REQUEST, "Missing model").into_response(),
    };
    let device_id = match body.get("device_id").and_then(|v| v.as_u64()) {
        Some(v) => v as u32,
        None => return (axum::http::StatusCode::BAD_REQUEST, "Missing device_id").into_response(),
    };
    state.gpu_scheduler.set_affinity(&model, device_id);
    Json(serde_json::json!({ "model": model, "device_id": device_id })).into_response()
}

/// DELETE /api/gpu-scheduler/affinity/{model}
async fn gpu_scheduler_clear_affinity_handler(
    State(state): State<AppState>,
    axum::extract::Path(model): axum::extract::Path<String>,
) -> Response {
    state.gpu_scheduler.clear_affinity(&model);
    Json(serde_json::json!({ "cleared": model })).into_response()
}

/// GET /api/gpu-scheduler/stats
async fn gpu_scheduler_stats_handler(State(state): State<AppState>) -> Response {
    let stats = state.gpu_scheduler.get_stats();
    Json(stats).into_response()
}

/// PATCH /api/gpu-scheduler/config
async fn gpu_scheduler_config_handler(
    State(state): State<AppState>,
    axum::extract::Json(update): axum::extract::Json<crate::gpu_scheduler::UpdateGpuSchedulerConfigRequest>,
) -> Response {
    let config = state.gpu_scheduler.update_config(update).await;
    Json(config).into_response()
}

// =========================================================================
// Artifact Storage Handlers
// =========================================================================

/// POST /api/artifacts/upload
async fn artifact_upload_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<crate::artifact_storage::CreateArtifactRequest>,
) -> Response {
    match state.artifact_storage.create_artifact(req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// GET /api/artifacts
async fn artifact_list_handler(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<crate::artifact_storage::ListArtifactsQuery>,
) -> Response {
    let artifacts = state.artifact_storage.list_artifacts(&query);
    Json(artifacts).into_response()
}

/// GET /api/artifacts/{id}
async fn artifact_get_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.artifact_storage.get_artifact(&id) {
        Some(artifact) => Json(artifact).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Artifact not found").into_response(),
    }
}

/// GET /api/artifacts/{id}/download
async fn artifact_download_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.artifact_storage.read_data(&id).await {
        Ok(data) => {
            let headers = [
                ("content-type", "application/octet-stream"),
                ("content-length", &data.len().to_string()),
            ];
            (headers, data).into_response()
        }
        Err(e) => (axum::http::StatusCode::NOT_FOUND, e).into_response(),
    }
}

/// DELETE /api/artifacts/{id}
async fn artifact_delete_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.artifact_storage.delete_artifact(&id).await {
        Ok(artifact) => Json(artifact).into_response(),
        Err(e) => (axum::http::StatusCode::NOT_FOUND, e).into_response(),
    }
}

/// GET /api/artifacts/{id}/verify
async fn artifact_verify_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.artifact_storage.verify_artifact(&id).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (axum::http::StatusCode::NOT_FOUND, e).into_response(),
    }
}

/// GET /api/artifacts/stats
async fn artifact_stats_handler(State(state): State<AppState>) -> Response {
    let stats = state.artifact_storage.get_stats().await;
    Json(stats).into_response()
}

/// PATCH /api/artifacts/config
async fn artifact_config_handler(
    State(state): State<AppState>,
    axum::extract::Json(update): axum::extract::Json<crate::artifact_storage::UpdateArtifactStorageConfigRequest>,
) -> Response {
    let config = state.artifact_storage.update_config(update).await;
    Json(config).into_response()
}

/// POST /api/artifacts/cleanup
async fn artifact_cleanup_handler(State(state): State<AppState>) -> Response {
    let result = state.artifact_storage.cleanup(None).await;
    Json(result).into_response()
}

// ---------------------------------------------------------------------------
// Quantization v2 Handlers
// ---------------------------------------------------------------------------

async fn quantize_start_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<crate::quantization_v2::StartQuantizeRequest>,
) -> Response {
    match state.quantization_v2.start_job(req.model_id, req.config).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({"job_id": id}))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn quantize_list_handler(State(state): State<AppState>) -> Response {
    Json(state.quantization_v2.list_jobs(None)).into_response()
}

async fn quantize_get_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.quantization_v2.get_job(&id) {
        Some(job) => Json(job).into_response(),
        None => (StatusCode::NOT_FOUND, "Job not found").into_response(),
    }
}

async fn quantize_cancel_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.quantization_v2.cancel_job(&id) {
        Ok(()) => Json(serde_json::json!({"cancelled": true})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn quantize_result_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.quantization_v2.get_result(&id) {
        Some(result) => Json(result).into_response(),
        None => (StatusCode::NOT_FOUND, "Result not found").into_response(),
    }
}

async fn quantize_layers_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    Json(state.quantization_v2.get_layer_results(&id)).into_response()
}

async fn quantize_estimate_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<crate::quantization_v2::EstimateRequest>,
) -> Response {
    Json(state.quantization_v2.estimate(&req)).into_response()
}

async fn quantize_methods_handler(State(_state): State<AppState>) -> Response {
    Json(crate::quantization_v2::QuantizationV2Manager::list_methods()).into_response()
}

async fn quantize_compare_handler(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let job_id = query.get("job_id").map(|s| s.as_str()).unwrap_or("");
    match state.quantization_v2.compare(job_id) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn quantize_verify_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<crate::quantization_v2::VerifyRequest>,
) -> Response {
    Json(state.quantization_v2.verify(&req)).into_response()
}

async fn quantize_history_handler(State(state): State<AppState>) -> Response {
    Json(state.quantization_v2.history(100)).into_response()
}

async fn quantize_delete_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.quantization_v2.delete_job(&id) {
        Ok(()) => Json(serde_json::json!({"deleted": true})).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn quantize_config_handler(
    State(state): State<AppState>,
    axum::extract::Json(update): axum::extract::Json<crate::quantization_v2::QuantConfigUpdate>,
) -> Response {
    let config = state.quantization_v2.update_config(update).await;
    Json(config).into_response()
}

// ---------------------------------------------------------------------------
// Priority Queue Handlers
// ---------------------------------------------------------------------------

async fn priority_queue_status_handler(State(state): State<AppState>) -> Response {
    let status = state.priority_queue.status().await;
    Json(status).into_response()
}

async fn priority_queue_tasks_handler(State(state): State<AppState>) -> Response {
    Json(state.priority_queue.list_tasks(1000)).into_response()
}

async fn priority_queue_enqueue_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<crate::priority_queue::EnqueueRequest>,
) -> Response {
    match state.priority_queue.enqueue(req).await {
        Ok(resp) => (StatusCode::CREATED, Json(resp)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn priority_queue_remove_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.priority_queue.remove_task(&id) {
        Ok(task) => Json(task).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn priority_queue_stats_handler(State(state): State<AppState>) -> Response {
    Json(state.priority_queue.stats()).into_response()
}

async fn priority_queue_config_handler(
    State(state): State<AppState>,
    axum::extract::Json(update): axum::extract::Json<crate::priority_queue::PriorityQueueConfigUpdate>,
) -> Response {
    let config = state.priority_queue.update_config(update).await;
    Json(config).into_response()
}

async fn priority_queue_clear_handler(State(state): State<AppState>) -> Response {
    let count = state.priority_queue.clear_all();
    Json(serde_json::json!({"cleared": count})).into_response()
}

// ---------------------------------------------------------------------------
// Model Snapshot Handlers
// ---------------------------------------------------------------------------

async fn snapshot_create_handler(
    State(state): State<AppState>,
    axum::extract::Json(req): axum::extract::Json<crate::model_snapshot::CreateSnapshotRequest>,
) -> Response {
    match state.model_snapshot.create_snapshot(req).await {
        Ok(resp) => (StatusCode::CREATED, Json(resp)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn snapshot_list_handler(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<crate::model_snapshot::ListSnapshotsQuery>,
) -> Response {
    Json(state.model_snapshot.list_snapshots(&query)).into_response()
}

async fn snapshot_get_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.model_snapshot.get_snapshot(&id) {
        Some(snapshot) => Json(snapshot).into_response(),
        None => (StatusCode::NOT_FOUND, "Snapshot not found").into_response(),
    }
}

async fn snapshot_delete_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.model_snapshot.delete_snapshot(&id) {
        Ok(_) => Json(serde_json::json!({"deleted": true})).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn snapshot_restore_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.model_snapshot.restore_snapshot(&id) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn snapshot_compare_handler(
    State(state): State<AppState>,
    axum::extract::Path((id, other_id)): axum::extract::Path<(String, String)>,
) -> Response {
    match state.model_snapshot.compare_snapshots(&id, &other_id) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn snapshot_verify_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    match state.model_snapshot.verify_snapshot(&id) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn snapshot_stats_handler(State(state): State<AppState>) -> Response {
    Json(state.model_snapshot.get_stats()).into_response()
}

async fn snapshot_config_handler(
    State(state): State<AppState>,
    axum::extract::Json(update): axum::extract::Json<crate::model_snapshot::SnapshotConfigUpdate>,
) -> Response {
    let config = state.model_snapshot.update_config(update).await;
    Json(config).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Method, Request};
    use tower::ServiceBuilder;
    use tower::ServiceExt;

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
