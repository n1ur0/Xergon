//! Xergon Agent — Rust sidecar for Ergo nodes
//!
//! Provides:
//! - Peer discovery (finds other Xergon agents via Ergo peer list)
//! - PoNW scoring (Proof-of-Node-Work)
//! - Node health monitoring
//! - REST API for marketplace integration
//!
//! CLI subcommands:
//!   run        — Start the agent (default)
//!   setup      — Interactive first-run configuration wizard
//!   status     — Query a running agent's status
//!   update     — Check for / apply updates
//!   provider   — Provider management (set-price, list-prices, remove-price)













use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use xergon_agent::api::AppState;
use xergon_agent::config::AgentConfig;
use xergon_agent::node_health::NodeHealthChecker;
use xergon_agent::peer_discovery::PeerDiscovery;
use xergon_agent::pown::PownCalculator;
use xergon_agent::settlement::SettlementEngine;

#[derive(Parser, Debug)]
#[command(
    name = "xergon-agent",
    about = "Xergon Agent — P2P AI compute node for Ergo",
    version,
    propagate_version = true,
    arg_required_else_help = true,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the Xergon agent (default behavior)
    Run {
        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Interactive first-run setup wizard
    Setup,
    /// Show status of a running agent
    Status,
    /// Check for updates
    Update,
    /// Provider management commands
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },
}

#[derive(Subcommand, Debug)]
enum ProviderAction {
    /// Set per-model pricing (nanoERG per 1M tokens)
    SetPrice {
        /// Model identifier (e.g. "llama-3.1-8b")
        #[arg(short, long)]
        model: String,
        /// Price in nanoERG per 1M tokens
        #[arg(short, long)]
        price: u64,
        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// List current pricing configuration
    ListPrices {
        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Remove a per-model price override
    RemovePrice {
        /// Model identifier to remove
        #[arg(short, long)]
        model: String,
        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { config } => run_command(config).await,
        Commands::Setup => xergon_agent::setup::run_interactive_setup().await,
        Commands::Status => status_command().await,
        Commands::Update => update_command(),
        Commands::Provider { action } => provider_command(action),
    }
}

// ---------------------------------------------------------------------------
// `run` subcommand — the original main() logic
// ---------------------------------------------------------------------------

async fn run_command(config_path: Option<PathBuf>) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xergon_agent=info".into()),
        )
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info!("Xergon Agent starting...");

    let resolved_config_path = resolve_config_path(config_path);
    let config = AgentConfig::load_from(Some(resolved_config_path.clone())).context("Failed to load configuration")?;
    config.validate().map_err(|e| anyhow::anyhow!("Configuration validation failed: {e}"))?;

    info!(
        provider_id = %config.xergon.provider_id,
        region = %config.xergon.region,
        ergo_url = %config.ergo_node.rest_url,
        listen_addr = %config.api.listen_addr,
        "Configuration loaded"
    );

    // Initialize contract compilation pipeline
    {
        let mut contract_overrides = std::collections::HashMap::new();
        let cc = &config.contracts;
        if !cc.provider_box_hex.is_empty() {
            contract_overrides.insert("provider_box_hex".into(), cc.provider_box_hex.clone());
        }
        if !cc.provider_registration_hex.is_empty() {
            contract_overrides.insert("provider_registration_hex".into(), cc.provider_registration_hex.clone());
        }
        if !cc.treasury_box_hex.is_empty() {
            contract_overrides.insert("treasury_box_hex".into(), cc.treasury_box_hex.clone());
        }
        if !cc.usage_proof_hex.is_empty() {
            contract_overrides.insert("usage_proof_hex".into(), cc.usage_proof_hex.clone());
        }
        if !cc.user_staking_hex.is_empty() {
            contract_overrides.insert("user_staking_hex".into(), cc.user_staking_hex.clone());
        }
        xergon_agent::contract_compile::init_config_overrides(contract_overrides);

        let (total, valid, invalid) = xergon_agent::contract_compile::validate_all_contracts();
        let contract_names = xergon_agent::contract_compile::list_contracts()
            .join(", ");
        info!(
            total,
            valid,
            invalid,
            contracts = %contract_names,
            "Loaded compiled contracts"
        );
    }

    let health_checker = NodeHealthChecker::new(config.ergo_node.rest_url.clone())
        .context("Failed to create node health checker")?;

    let pown = Arc::new(PownCalculator::new(config.xergon.clone()));

    let discovery = Arc::new(
        PeerDiscovery::new(
            config.peer_discovery.clone(),
            config.ergo_node.rest_url.clone(),
        )
        .context("Failed to create peer discovery")?,
    );

    discovery
        .load_peers()
        .await
        .context("Failed to load persisted peers")?;

    // Initial node health check
    match health_checker
        .check_health(&config.xergon.ergo_address)
        .await
    {
        Ok(health) => {
            info!(
                synced = health.is_synced,
                peers = health.peer_count,
                height = health.node_height,
                node_id = %health.node_id,
                "Initial node health check passed"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Initial node health check failed — Ergo node may not be running yet"
            );
        }
    }

    // Derive node_id eagerly so /xergon/status never returns empty node_id
    let initial_node_id = NodeHealthChecker::derive_node_id(&config.xergon.ergo_address);
    pown.set_node_id(initial_node_id.clone()).await;

    let peer_state = Arc::new(RwLock::new(xergon_agent::peer_discovery::PeerDiscoveryState::default()));
    let node_health_state = Arc::new(RwLock::new(xergon_agent::node_health::NodeHealthState {
        best_height_local: 0,
        ergo_address: config.xergon.ergo_address.clone(),
        is_synced: false,
        last_header_id: None,
        node_height: 0,
        node_id: initial_node_id.clone(),
        peer_count: 0,
        timestamp: chrono::Utc::now().timestamp(),
    }));

    let multi_gpu = Arc::new(xergon_agent::multi_gpu::MultiGpuManager::new());

    let mut app_state = AppState {
        xergon_config: config.xergon.clone(),
        ergo_node_url: config.ergo_node.rest_url.clone(),
        pown_status: pown.status(),
        peer_state: peer_state.clone(),
        node_health: node_health_state.clone(),
        settlement: None,
        start_time: std::time::Instant::now(),
        management_api_key: config.api.api_key.clone(),
        airdrop: None,
        gpu_rental_config: None,
        usage_meter: None,
        tunnel_manager: None,
        p2p_engine: None,
        auto_pull: None,
        model_discovery: None,
        model_cache: None,
        rollup: None,
        metrics: Arc::new(xergon_agent::metrics::MetricsCollector::new()),
        metrics_store: Arc::new(xergon_agent::metrics::MetricsStore::new()),
        models_loaded: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        pricing: Arc::new(tokio::sync::RwLock::new(config.pricing.clone())),
        config_path: resolved_config_path,
        oracle: None,
        provider_registry_config: if config.provider_registry.enabled {
            Some(Arc::new(config.provider_registry.clone()))
        } else {
            None
        },
        orchestrator: Arc::new(xergon_agent::orchestration::Orchestrator::new(
            xergon_agent::orchestration::OrchestrationConfig::default(),
        )),
        benchmark_suite: None,
        auto_healer: None,
        download_progress: None,
        marketplace_sync: None,
        config_reloader: None,
        model_registry: Arc::new(xergon_agent::model_versioning::ModelVersionRegistry::new()),
        auto_scaler: None,
        reputation_dashboard: None,
        inference_queue: None,
        model_health_monitor: None,
        provider_mesh: None,
        fine_tune: Arc::new(xergon_agent::fine_tune::FineTuneManager::new(2)),
        ab_testing: Arc::new(xergon_agent::ab_testing::ABTestManager::new()),
        multi_gpu: multi_gpu.clone(),
        container_runtime: Arc::new(xergon_agent::container::ContainerManager::new()),
        model_shard_manager: Arc::new(xergon_agent::model_sharding::ModelShardManager::new(
            multi_gpu.clone(),
        )),
        distributed_inference: Arc::new(xergon_agent::distributed_inference::DistributedInferenceManager::new()),
        sandbox_manager: Arc::new(xergon_agent::sandbox::SandboxManager::new()),
        marketplace_listing: Arc::new(xergon_agent::marketplace_listing::MarketplaceListingManager::new(
            "xergon-provider".to_string(),
        )),
        observability: Arc::new(xergon_agent::observability::ObservabilityManager::new(
            xergon_agent::observability::ObservabilityConfig::default(),
        )),
        compression: Arc::new(xergon_agent::model_compression::CompressionManager::new(2)),
        inference_cache: Arc::new(xergon_agent::inference_cache::InferenceCache::new_default()),
        gpu_memory: Arc::new(xergon_agent::gpu_memory::GpuMemoryManager::new()),
        model_migration: Arc::new(xergon_agent::model_migration::ModelMigrationManager::new()),
        warmup_pool: Arc::new(xergon_agent::warmup::WarmupPool::new(
            xergon_agent::warmup::WarmupConfig::default(),
        )),
        inference_batcher: Arc::new(xergon_agent::inference_batch::InferenceBatcher::new(
            xergon_agent::inference_batch::BatchConfig::default(),
        )),
        checkpoint_manager: Arc::new(xergon_agent::checkpoint::CheckpointManager::new(
            xergon_agent::checkpoint::CheckpointConfig::default(),
        )),
        quota_manager: Arc::new(xergon_agent::resource_quotas::ResourceQuotaManager::new(
            xergon_agent::resource_quotas::QuotaConfig::default(),
        )),
        profiler: Arc::new(xergon_agent::inference_profiler::InferenceProfiler::default()),
        gpu_scheduler: Arc::new(xergon_agent::gpu_scheduler::GpuScheduler::default()),
        artifact_storage: Arc::new(xergon_agent::artifact_storage::ArtifactStorage::default()),
        content_safety: Arc::new(tokio::sync::RwLock::new(
            xergon_agent::content_safety::ContentSafetyFilter::new(),
        )),
        quantization_v2: Arc::new(xergon_agent::quantization_v2::QuantizationV2Manager::new(
            xergon_agent::quantization_v2::QuantConfig::default(),
        )),
        priority_queue: Arc::new(xergon_agent::priority_queue::PriorityQueueManager::new(
            xergon_agent::priority_queue::PriorityQueueConfig::default(),
        )),
        model_snapshot: Arc::new(xergon_agent::model_snapshot::SnapshotManager::new(
            xergon_agent::model_snapshot::SnapshotConfig::default(),
        )),
        alignment_trainer: Arc::new(xergon_agent::alignment_training::AlignmentTrainer::new()),
        model_serve_manager: Arc::new(xergon_agent::model_serving::ModelServeManager::new(
            xergon_agent::model_serving::ModelServeConfig::default(),
        )),
        dynamic_batcher: Arc::new(xergon_agent::dynamic_batcher::DynamicBatcher::new(
            xergon_agent::dynamic_batcher::DynamicBatchConfig::default(),
        )),
        ab_testing_v2: Arc::new(xergon_agent::ab_testing_v2::ABTestV2Manager::new()),
        federated_learning: Some(Arc::new(xergon_agent::federated_learning::FederatedState::new())),
        extended_model_registry: Some(Arc::new(xergon_agent::model_registry::ModelRegistry::new())),
        model_optimizer: Some(Arc::new(xergon_agent::model_optimizer::ModelOptimizer::new())),
        federated_training: Some(Arc::new(xergon_agent::federated_training::FederatedTrainingEngine::new())),
        tensor_pipeline: Arc::new(xergon_agent::tensor_pipeline::TensorPipelineManager::new()),
        e2e_suite: xergon_agent::e2e_integration::create_default_suite(),
        circuit_breaker: Arc::new(xergon_agent::self_healing_circuit_breaker::SelfHealingCircuitBreaker::new()),
        model_drift_detector: Arc::new(xergon_agent::model_drift::ModelDriftDetector::new()),
        inference_observability: Arc::new(xergon_agent::inference_observability::InferenceObservability::new()),
        lineage_graph: Arc::new(xergon_agent::model_lineage_graph::LineageGraph::new()),
        model_hash_chain: Arc::new(xergon_agent::model_hash_chain::ModelHashChain::new()),
        prompt_versioning: Arc::new(xergon_agent::prompt_versioning::PromptVersionManager::new()),
        inference_sandbox: Arc::new(xergon_agent::inference_sandbox::InferenceSandbox::new()),
        model_access_control: Arc::new(xergon_agent::model_access_control::ModelAccessControl::new()),
        audit_aggregator: Arc::new(xergon_agent::audit_log_aggregator::AuditLogAggregator::new()),
        feature_flags: Arc::new(xergon_agent::feature_flags::FeatureFlagService::new()),
        experiments: Arc::new(xergon_agent::experiment_framework::ExperimentFramework::new()),
        inference_gateway: Arc::new(xergon_agent::inference_gateway::InferenceGateway::new()),
        oracle_feeds: Arc::new(xergon_agent::ergo_oracle_feeds::ErgoOracleService::new()),
        cost_accountant: Arc::new(xergon_agent::ergo_cost_accounting::ErgoCostAccountant::new()),
        sigma_usd_pricer: Arc::new(xergon_agent::sigma_usd_pricing::SigmaUsdPricer::new()),
        lifecycle_manager: None,
        chaos_engine: Arc::new(xergon_agent::chaos_testing::ChaosEngine::new()),
        sigma_proof_builder: Some(Arc::new(xergon_agent::sigma_proof_builder::SigmaProofBuilderState::new())),
        token_operations: Some(Arc::new(xergon_agent::token_operations::TokenOperationsState::new())),
    };

    // Initialize benchmark suite
    let benchmark_suite = Arc::new(xergon_agent::benchmark::BenchmarkSuite::new());
    app_state.benchmark_suite = Some(benchmark_suite);

    // Initialize auto-heal system if enabled
    let auto_heal_config = xergon_agent::auto_heal::AutoHealConfig::default();
    let auto_healer = Arc::new(xergon_agent::auto_heal::AutoHealer::new(
        auto_heal_config,
        config.ergo_node.rest_url.clone(),
        config.relay.relay_url.clone(),
    ));
    app_state.auto_healer = Some(auto_healer.clone());
    auto_healer.spawn_loop();

    // Initialize airdrop service if enabled
    if config.airdrop.enabled {
        let ergo_url = config
            .airdrop
            .ergo_node_url
            .clone()
            .unwrap_or_else(|| config.ergo_node.rest_url.clone());
        let airdrop_service = xergon_agent::airdrop::AirdropService::new(config.airdrop.clone(), ergo_url);
        info!(
            amount_nanoerg = config.airdrop.amount_nanoerg,
            max_total_erg = config.airdrop.max_total_erg,
            cooldown_secs = config.airdrop.cooldown_secs,
            "Airdrop service enabled"
        );
        app_state.airdrop = Some(Arc::new(airdrop_service));
    } else {
        info!("Airdrop service disabled (set [airdrop].enabled = true to enable)");
    }

    // Initialize oracle service if enabled
    if config.oracle.enabled {
        match xergon_agent::oracle_service::OracleService::new(
            config.oracle.clone(),
            config.ergo_node.rest_url.clone(),
        ) {
            Ok(oracle_svc) => {
                let oracle_svc = Arc::new(oracle_svc);
                oracle_svc.spawn_refresh_loop();
                info!(
                    pool_nft = %config.oracle.pool_nft_id,
                    refresh_secs = config.oracle.refresh_interval_secs,
                    "Oracle service enabled — ERG/USD price feed from oracle pool box"
                );
                app_state.oracle = Some(oracle_svc);
            }
            Err(e) => {
                warn!(error = %e, "Failed to initialize oracle service");
            }
        }
    } else {
        info!("Oracle service disabled (set [oracle].enabled = true to enable)");
    }

    // Auto-register provider on-chain if configured
    if config.provider_registry.enabled && config.provider_registry.auto_register {
        let node_url = config.ergo_node.rest_url.clone();
        let client = xergon_agent::chain::client::ErgoNodeClient::new(node_url);
        let endpoint = if config.provider_registry.endpoint_url.is_empty() {
            format!(
                "http://0.0.0.0:{}",
                config.api.listen_addr.split(':').last().unwrap_or("9099")
            )
        } else {
            config.provider_registry.endpoint_url.clone()
        };
        match xergon_agent::provider_registry::auto_register_if_needed(
            &client,
            &config.provider_registry,
            &config.xergon.provider_name,
            &endpoint,
            &config.provider_registry.provider_pk_hex,
            config.provider_registry.price_per_token,
        )
        .await
        {
            Ok(Some(result)) => info!(tx_id = %result.tx_id, "Auto-registered provider on-chain"),
            Ok(None) => info!("Provider already registered on-chain"),
            Err(e) => warn!(error = %e, "Auto-registration failed (non-fatal)"),
        }
    }

    // Initialize GPU rental subsystem (metering + tunnels)
    if config.gpu_rental.enabled {
        info!(
            ergo_url = %config.gpu_rental.ergo_node_url,
            ssh_enabled = config.gpu_rental.ssh_enabled,
            metering_interval_secs = config.gpu_rental.metering_check_interval_secs,
            ssh_username = %config.gpu_rental.ssh_username,
            "Initializing GPU rental subsystem"
        );

        let gpu_config = Arc::new(config.gpu_rental.clone());
        app_state.gpu_rental_config = Some(gpu_config.clone());

        // Create usage meter
        let usage_meter = Arc::new(
            xergon_agent::gpu_rental::metering::UsageMeter::new(config.gpu_rental.metering_check_interval_secs)
        );
        app_state.usage_meter = Some(usage_meter.clone());

        // Spawn metering loop (checks chain height + session expiry)
        let metering_chain_client = xergon_agent::chain::client::ErgoNodeClient::new(
            config.gpu_rental.ergo_node_url.clone(),
        );
        usage_meter.spawn_metering_loop(metering_chain_client);

        // Create tunnel manager if SSH is enabled
        if config.gpu_rental.ssh_enabled {
            let tunnel_config = xergon_agent::gpu_rental::tunnel::TunnelConfig {
                ssh_port_range: config.gpu_rental.ssh_tunnel_port_range.clone(),
                ssh_username: config.gpu_rental.ssh_username.clone(),
            };
            let tunnel_manager = Arc::new(xergon_agent::gpu_rental::tunnel::TunnelManager::new(&tunnel_config));
            app_state.tunnel_manager = Some(tunnel_manager);
            info!(
                port_range = %config.gpu_rental.ssh_tunnel_port_range,
                "SSH tunnel manager enabled"
            );
        }

        info!("GPU rental subsystem initialized");
    } else {
        info!("GPU rental disabled (set [gpu_rental].enabled = true to enable)");
    }

    // Initialize settlement engine if enabled
    if config.settlement.enabled {
        info!(
            interval_secs = config.settlement.interval_secs,
            dry_run = config.settlement.dry_run,
            "Initializing ERG settlement engine"
        );

        match SettlementEngine::new(config.settlement.clone(), config.ergo_node.rest_url.clone()) {
            Ok(engine) => {
                if let Err(e) = engine.init().await {
                    tracing::warn!(error = %e, "Settlement engine init failed, starting without persistence");
                }

                // Load per-model pricing from config (mirrors what goes on-chain in Provider Box R6)
                {
                    let mut pricing: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
                    for model_id in &config.inference.served_models {
                        let price = config.pricing.models.get(model_id)
                            .copied()
                            .unwrap_or(config.pricing.default_price_per_1m_tokens);
                        pricing.insert(model_id.clone(), price);
                    }
                    engine.update_model_pricing(pricing).await;
                }

                let engine = Arc::new(engine);
                app_state.settlement = Some(engine.clone());

                // Spawn the settlement loop
                let settlement_engine = engine.clone();
                let settlement_handle = tokio::spawn(async move {
                    settlement_engine.run_loop().await;
                });

                // Spawn the confirmation polling loop (independent of settlement loop)
                let confirm_engine = engine.clone();
                let confirm_handle = tokio::spawn(async move {
                    confirm_engine.confirm_loop().await;
                });

                // Spawn the on-chain settlement loop if chain_enabled
                if config.settlement.chain_enabled {
                    let chain_engine = engine.clone();
                    let provider_address = config.xergon.ergo_address.clone();
                    let chain_handle = tokio::spawn(async move {
                        chain_engine.run_chain_settlement_loop(provider_address).await;
                    });
                    info!(
                        provider = %config.xergon.ergo_address,
                        interval_secs = config.settlement.interval_secs,
                        "On-chain (eUTXO) settlement loop spawned"
                    );
                    drop(chain_handle);
                }

                // Keep handles alive (we won't abort them on shutdown since settlements
                // should complete if in-progress)
                drop(settlement_handle);
                drop(confirm_handle);

                info!("ERG settlement engine started");

                // NOTE: Batch settlement can be enabled alongside the periodic settlement loop.
                // To use batch settlement instead of (or in addition to) the periodic loop,
                // create a BatchSettlement instance here and call add_payment() after each
                // inference request. Example:
                //
                //   use xergon_agent::settlement::batch::BatchSettlement;
                //   let batch_settlement = Arc::new(BatchSettlement::new(
                //       /* tx_service */,
                //       10,                                        // batch_size
                //       std::time::Duration::from_secs(300),      // flush_interval
                //       config.settlement.min_settlement_nanoerg, // min_payment (dust threshold)
                //   ));
                //   // Spawn background periodic flush
                //   batch_settlement.start_background_flush();
                //
                // Then in the inference handler, after computing cost_nanoerg:
                //   batch_settlement.add_payment(&user_addr, &provider_addr, cost_nanoerg, &model).await;
                //
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create settlement engine, running without settlement");
            }
        }
    } else {
        info!("Settlement engine disabled (set [settlement].enabled = true to enable)");
    }

    // Initialize P2P provider-to-provider communication engine
    if config.p2p.enabled {
        info!(
            peer_refresh_interval_secs = config.p2p.peer_refresh_interval_secs,
            timeout_secs = config.p2p.timeout_secs,
            max_cached_peers = config.p2p.max_cached_peers,
            "Initializing P2P provider-to-provider communication"
        );

        match xergon_agent::p2p::P2PEngine::new(config.p2p.clone(), &config.xergon) {
            Ok(p2p_engine) => {
                // Set the agent's own endpoint
                let self_endpoint = std::env::var("XERGON__RELAY__AGENT_ENDPOINT")
                    .unwrap_or_else(|_| {
                        let addr = &config.api.listen_addr;
                        addr.replace("0.0.0.0", "127.0.0.1")
                            .replace("[::]", "127.0.0.1")
                    });
                p2p_engine.set_self_endpoint(self_endpoint).await;

                let p2p_engine = Arc::new(p2p_engine);
                app_state.p2p_engine = Some(p2p_engine.clone());

                // Spawn P2P peer refresh loop
                let p2p_for_refresh = p2p_engine.clone();
                let discovery_for_p2p = discovery.clone();
                let p2p_interval = config.p2p.peer_refresh_interval_secs;
                tokio::spawn(async move {
                    // Wait before starting periodic refresh
                    tokio::time::sleep(std::time::Duration::from_secs(p2p_interval)).await;
                    loop {
                        // Get known Xergon peer endpoints from peer discovery
                        let peer_endpoints = discovery_for_p2p.get_xergon_peer_endpoints().await;
                        if !peer_endpoints.is_empty() {
                            info!(
                                peers = peer_endpoints.len(),
                                "P2P: refreshing peer info"
                            );
                            p2p_for_refresh.refresh_peers(&peer_endpoints).await;
                            info!(
                                cached = p2p_for_refresh.cached_peer_count(),
                                "P2P: peer refresh complete"
                            );
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(p2p_interval)).await;
                    }
                });

                info!("P2P provider-to-provider communication enabled");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create P2P engine — running without P2P");
            }
        }
    } else {
        info!("P2P communication disabled (set [p2p].enabled = true to enable)");
    }

    // Initialize automatic model pulling
    if config.auto_model_pull.enabled {
        info!(
            pull_timeout_secs = config.auto_model_pull.pull_timeout_secs,
            max_concurrent_pulls = config.auto_model_pull.max_concurrent_pulls,
            pre_pull_count = config.auto_model_pull.pre_pull_models.len(),
            "Initializing automatic model pulling"
        );

        let mut pull_config = config.auto_model_pull.clone();
        // Default backend_url to inference.url if not set
        if pull_config.backend_url.is_empty() {
            pull_config.backend_url = config.inference.url.clone();
        }

        match xergon_agent::auto_model_pull::AutoModelPull::new(pull_config) {
            Ok(auto_pull) => {
                let auto_pull = Arc::new(auto_pull);
                app_state.auto_pull = Some(auto_pull.clone());
                app_state.download_progress = Some(Arc::new(auto_pull.progress_tracker().clone()));

                // Spawn model watcher to keep local model list fresh
                auto_pull.clone().spawn_model_watcher();

                // Pre-pull configured models in background
                if !config.auto_model_pull.pre_pull_models.is_empty() {
                    let pre_pull = auto_pull.clone();
                    let models = config.auto_model_pull.pre_pull_models.clone();
                    tokio::spawn(async move {
                        // Give the backend a few seconds to start
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        pre_pull.pre_pull_models(&models).await;
                    });
                }

                info!("Automatic model pulling enabled");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create auto model pull system");
            }
        }
    } else {
        info!("Auto model pull disabled (set [auto_model_pull].enabled = true to enable)");
    }

    // Initialize model discovery service
    if config.model_discovery.enabled {
        info!(
            max_model_size_gb = config.model_discovery.max_model_size_gb,
            refresh_interval_secs = config.model_discovery.refresh_interval_secs,
            "Initializing HuggingFace model discovery"
        );

        match xergon_agent::model_discovery::ModelDiscovery::new(config.model_discovery.clone(), None) {
            Ok(discovery) => {
                let discovery = Arc::new(discovery);
                app_state.model_discovery = Some(discovery.clone());

                // Spawn background refresh loop
                let discovery_clone = discovery.clone();
                let refresh_interval = config.model_discovery.refresh_interval_secs;
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(refresh_interval)).await;
                        let _ = discovery_clone.scan().await;
                    }
                });

                // Run initial scan
                let initial_scan = discovery.clone();
                tokio::spawn(async move {
                    let _ = initial_scan.scan().await;
                });

                info!("HuggingFace model discovery enabled");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to initialize model discovery");
            }
        }
    } else {
        info!("Model discovery disabled (set [model_discovery].enabled = true to enable)");
    }

    // Initialize model cache with LRU eviction
    if config.model_cache.enabled {
        info!(
            max_size_gb = config.model_cache.max_size_gb,
            eviction_threshold = config.model_cache.eviction_threshold_percent,
            "Initializing model cache with LRU eviction"
        );

        match xergon_agent::model_cache::ModelCache::new(config.model_cache.clone()) {
            Ok(cache) => {
                let cache = Arc::new(cache);
                app_state.model_cache = Some(cache.clone());

                // Spawn background eviction checker
                let cache_clone = cache.clone();
                let check_interval = config.model_cache.eviction_check_interval_secs;
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(check_interval)).await;
                        let _ = cache_clone.check_and_evict().await;
                    }
                });

                info!("Model cache with LRU eviction enabled");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to initialize model cache");
            }
        }
    } else {
        info!("Model cache disabled (set [model_cache].enabled = true to enable)");
    }

    // Initialize usage proof rollup system
    if config.rollup.enabled {
        info!(
            epoch_duration_secs = config.rollup.epoch_duration_secs,
            min_proofs = config.rollup.min_proofs_per_commitment,
            max_proofs = config.rollup.max_proofs_per_commitment,
            "Initializing usage proof rollup system"
        );

        let rollup = Arc::new(xergon_agent::rollup::UsageRollup::new(config.rollup.clone()));
        app_state.rollup = Some(rollup.clone());

        // Spawn the commitment loop
        let rollup_chain_client = xergon_agent::chain::client::ErgoNodeClient::new(
            config.ergo_node.rest_url.clone(),
        );
        rollup.clone().spawn_commitment_loop(rollup_chain_client);

        info!("Usage proof rollup system enabled");
    } else {
        info!("Usage proof rollup disabled (set [rollup].enabled = true to enable)");
    }

    // Initialize storage rent monitoring
    if config.storage_rent.enabled {
        let watched_count = config.storage_rent.watched_boxes.len();
        if watched_count == 0 {
            info!("Storage rent monitoring enabled but no watched_boxes configured — nothing to monitor");
        } else {
            info!(
                check_interval_blocks = config.storage_rent.check_interval_blocks,
                buffer_factor = config.storage_rent.topup_buffer_factor,
                min_topup_nanoerg = config.storage_rent.min_topup_amount_nanoerg,
                watched = watched_count,
                "Initializing storage rent monitor"
            );

            let rent_client = xergon_agent::chain::client::ErgoNodeClient::new(
                config.ergo_node.rest_url.clone(),
            );
            let rent_monitor = xergon_agent::storage_rent::StorageRentMonitor::new(
                rent_client,
                config.storage_rent.clone(),
            );
            rent_monitor.spawn();

            info!("Storage rent monitor spawned as background task");
        }
    } else {
        info!("Storage rent monitoring disabled (set [storage_rent].enabled = true to enable)");
    }

    // Initialize relay registration client
    let relay_client = if config.relay.register_on_start
        && !config.relay.relay_url.is_empty()
        && !config.relay.token.is_empty()
    {
        // Determine the externally-reachable endpoint for this agent.
        // Warns if listen_addr is a wildcard (0.0.0.0 or [::]) since the
        // auto-detected endpoint will be localhost-only and unreachable
        // from other machines. Set XERGON__RELAY__AGENT_ENDPOINT explicitly
        // for production deployments.
        let agent_endpoint = std::env::var("XERGON__RELAY__AGENT_ENDPOINT")
            .unwrap_or_else(|_| {
                let addr = &config.api.listen_addr;
                if addr.contains("0.0.0.0") || addr.contains("[::]") {
                    tracing::warn!(
                        listen_addr = %addr,
                        "Agent is listening on a wildcard address. Relay registration will use 127.0.0.1 \
                         which is only reachable locally. Set XERGON__RELAY__AGENT_ENDPOINT for production."
                    );
                    addr.replace("0.0.0.0", "127.0.0.1")
                        .replace("[::]", "127.0.0.1")
                } else {
                    addr.clone()
                }
            });

        match xergon_agent::relay_client::RelayClient::new(
            config.relay.clone(),
            config.xergon.provider_id.clone(),
            config.xergon.provider_name.clone(),
            config.xergon.region.clone(),
            config.xergon.ergo_address.clone(),
            agent_endpoint,
        ) {
            Ok(client) => {
                info!(
                    relay = %config.relay.relay_url,
                    agent_endpoint = %config.api.listen_addr,
                    heartbeat_interval = config.relay.heartbeat_interval_secs,
                    "Relay registration client initialized"
                );

                // Initial registration attempt (best-effort — will retry via heartbeat loop)
                let models = vec![]; // Models detected later by llama-server probe
                match client.register(models).await {
                    Ok(()) => info!("Successfully registered with relay"),
                    Err(e) => {
                        tracing::warn!(error = %e, "Initial relay registration failed (will retry)")
                    }
                }

                Some(Arc::new(client))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create relay client — provider discovery disabled");
                None
            }
        }
    } else {
        info!("Relay registration disabled (set [relay].register_on_start = true and configure relay_url + token)");
        None
    };

    // Initialize on-chain transaction support (Phase 2)
    let chain_client = xergon_agent::chain::client::ErgoNodeClient::new(
        config.ergo_node.rest_url.clone(),
    );
    // We need to use the xergon_agent lib's ChainTxConfig, not the local config module's.
    // Convert by re-serializing through serde (they have the same fields).
    let chain_tx_config: xergon_agent::config::ChainTxConfig =
        serde_json::from_value(serde_json::to_value(&config.chain)?)
            .context("Failed to convert chain config")?;
    let _usage_proof_accumulator = if chain_tx_config.usage_proof_tx_enabled
        || chain_tx_config.heartbeat_tx_enabled
    {
        let acc = Arc::new(xergon_agent::chain::usage_proofs::UsageProofAccumulator::new(
            chain_tx_config.clone(),
            chain_client.clone(),
        ));

        if chain_tx_config.usage_proof_tx_enabled {
            info!(
                batch_interval_secs = chain_tx_config.usage_proof_batch_interval_secs,
                "On-chain usage proof submission enabled"
            );
            acc.clone().spawn_batch_loop();
        }

        if chain_tx_config.heartbeat_tx_enabled {
            info!(
                provider_nft = %chain_tx_config.provider_nft_token_id,
                "On-chain heartbeat tx submission enabled"
            );
        }

        Some(acc)
    } else {
        info!("On-chain transaction features disabled (set [chain].heartbeat_tx_enabled or [chain].usage_proof_tx_enabled = true)");
        None
    };

    // Spawn relay heartbeat loop
    let relay_handle = if let Some(ref client) = relay_client {
        let client = client.clone();
        let pown = pown.clone();
        let chain_tx_config_for_hb = chain_tx_config;
        let chain_client_for_hb = chain_client.clone();
        let region = config.xergon.region.clone();
        let hb_endpoint = config.provider_registry.endpoint_url.clone();
        let pricing_cfg = config.pricing.clone();
        let served_models = config.inference.served_models.clone();
        let handle = tokio::spawn(async move {
            // Build optional on-chain heartbeat callback
            let served_models_for_onchain = served_models.clone();
            let on_chain_cb: Option<Box<dyn Fn() + Send + Sync>> =
                if chain_tx_config_for_hb.heartbeat_tx_enabled {
                    let cc = chain_client_for_hb;
                    let nft_id = chain_tx_config_for_hb.provider_nft_token_id.clone();
                    let hb_region = region.clone();
                    let pown = pown.clone();
                    Some(Box::new(move || {
                        let cc = cc.clone();
                        let nft_id = nft_id.clone();
                        let hb_region = hb_region.clone();
                        let hb_endpoint = hb_endpoint.clone();
                        let pown = pown.clone();
                        let pricing_cfg = pricing_cfg.clone();
                        let served_models = served_models_for_onchain.clone();
                        tokio::spawn(async move {
                            let models_r6 = pricing_cfg.build_r6_json(&served_models);
                            // Read current PoNW score (work_points) for R7
                            let pown_status = pown.status();
                            let status = pown_status.read().await;
                            let ponw = status.work_points as i32;
                            drop(status);
                            match xergon_agent::chain::transactions::submit_heartbeat_tx(
                                &cc, &nft_id, &hb_endpoint, &models_r6, ponw, &hb_region,
                            ).await {
                                Ok(tx_id) => info!(tx_id = %tx_id, "On-chain heartbeat submitted"),
                                Err(e) => warn!(error = %e, "On-chain heartbeat tx failed (non-fatal)"),
                            }
                        });
                    }))
                } else {
                    None
                };

            let served_models_for_relay = Arc::new(served_models);
            client.spawn_heartbeat_loop(
                move || {
                    // Prefer served_models from config (full multi-model list).
                    // Fall back to pown status single model for backward compat
                    // when served_models is not configured.
                    if !served_models_for_relay.is_empty() {
                        Vec::clone(&served_models_for_relay)
                    } else {
                        let status = pown.status();
                        let guard = status.try_read();
                        match guard {
                            Ok(s) if !s.ai_model.is_empty() => vec![s.ai_model.clone()],
                            _ => vec![],
                        }
                    }
                },
                on_chain_cb,
            );
        });
        Some(handle)
    } else {
        None
    };

    // Spawn inference backend health check loop
    // Probes llama_server, fallback port, and the inference URL (e.g. Ollama)
    let llama_handle = {
        let pown = pown.clone();
        let llama_url = config.llama_server.url.clone();
        let llama_interval = config.llama_server.health_check_interval_secs;
        let inference_url = config.inference.url.clone();
        let models_loaded = app_state.models_loaded.clone();

        // Also try fallback port (8081) if the primary is 8080
        let fallback_url = if llama_url.ends_with(":8080") {
            Some(llama_url.replace(":8080", ":8081"))
        } else {
            None
        };

        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("Failed to create HTTP client for llama-server probe");

            let interval = std::time::Duration::from_secs(llama_interval);
            let mut last_known_models: Vec<String> = Vec::new();

            // Build list of URLs to probe: llama_server, fallback, inference
            let mut probe_urls = vec![llama_url.clone()];
            if let Some(fb) = &fallback_url {
                probe_urls.push(fb.clone());
            }
            // Add inference URL if different from llama_server
            if inference_url != llama_url && !probe_urls.contains(&inference_url) {
                probe_urls.push(inference_url.clone());
            }

            loop {
                let mut detected = None;
                for url in &probe_urls {
                    detected = probe_llama_server_all_models(&client, url).await;
                    if detected.is_some() {
                        break;
                    }
                }

                match detected {
                    Some(models) => {
                        if models != last_known_models {
                            info!(
                                models = ?models,
                                model_count = models.len(),
                                "llama-server detected — AI inference backend available"
                            );
                        }
                        // Update pown with the first model name (backward compat with
                        // single-model fields in PownStatus), but also populate the
                        // full list in models_loaded for the health endpoint.
                        let first_model = models.first().map(|s| s.as_str()).unwrap_or("");
                        pown.update_ai_stats(first_model, 0, 0).await;

                        // Populate models_loaded for the health API endpoint
                        {
                            let mut loaded = models_loaded.write().await;
                            *loaded = models.clone();
                        }

                        last_known_models = models;
                    }
                    None => {
                        if !last_known_models.is_empty() {
                            info!("llama-server not responding — AI backend marked as offline");
                        }
                        // Clear models_loaded when backend is offline
                        {
                            let mut loaded = models_loaded.write().await;
                            loaded.clear();
                        }
                        last_known_models = Vec::new();
                    }
                }

                tokio::time::sleep(interval).await;
            }
        })
    };

    // Spawn peer discovery loop
    let discovery_handle = {
        let discovery = discovery.clone();
        let peer_state = peer_state.clone();
        let pown = pown.clone();
        let node_health_state = node_health_state.clone();
        let interval_secs = config.peer_discovery.discovery_interval_secs;
        let ergo_address = config.xergon.ergo_address.clone();

        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(interval_secs);

            loop {
                if let Ok(health) = health_checker.check_health(&ergo_address).await {
                    *node_health_state.write().await = health;
                }

                match discovery.run_discovery_cycle().await {
                    Ok(metrics) => {
                        let state = discovery.get_state(Some(metrics.clone())).await;
                        let unique_xergon_peers_seen = state.unique_xergon_peers_seen;
                        let total_xergon_confirmations = state.total_xergon_confirmations;
                        *peer_state.write().await = state;

                        let health = node_health_state.read().await;
                        pown.tick(
                            &health,
                            discovery.peers_checked(),
                            unique_xergon_peers_seen,
                            total_xergon_confirmations,
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Discovery cycle failed");
                    }
                }

                tokio::time::sleep(interval).await;
            }
        })
    };

    // Initialize marketplace sync (periodic push of provider info to relay)
    if config.marketplace_sync.enabled && !config.relay.relay_url.is_empty() {
        info!(
            interval_secs = config.marketplace_sync.sync_interval_secs,
            include_benchmarks = config.marketplace_sync.include_benchmarks,
            include_models = config.marketplace_sync.include_models,
            include_gpu_info = config.marketplace_sync.include_gpu_info,
            "Initializing marketplace sync"
        );

        let marketplace_sync = Arc::new(xergon_agent::marketplace_sync::MarketplaceSync::new(
            config.marketplace_sync.clone(),
            config.relay.relay_url.clone(),
            Arc::new(app_state.clone()),
        ));
        app_state.marketplace_sync = Some(marketplace_sync.clone());

        // Spawn the sync loop
        marketplace_sync.spawn_loop();

        info!("Marketplace sync enabled");
    } else {
        info!("Marketplace sync disabled (enable [marketplace_sync].enabled and configure relay.relay_url)");
    }

    // Start API server (blocks until shutdown)
    if config.inference.enabled {
        // Determine effective backend URL based on backend_type
        let effective_url = if config.inference.backend_type == xergon_agent::config::InferenceBackendType::LlamaCpp {
            // When using llama.cpp backend, prefer the dedicated llama_server URL
            if !config.llama_server.url.is_empty() {
                config.llama_server.url.clone()
            } else {
                config.inference.url.clone()
            }
        } else {
            config.inference.url.clone()
        };

        info!(
            backend_type = ?config.inference.backend_type,
            backend_url = %effective_url,
            "Inference proxy enabled — adding /v1/chat/completions and /v1/models routes"
        );

        // Build llama.cpp config if using llama.cpp backend
        let llama_config = if config.inference.backend_type == xergon_agent::config::InferenceBackendType::LlamaCpp {
            Some(config.llama_server.clone())
        } else {
            None
        };

        let inference_state = xergon_agent::inference::InferenceState {
            config: config.inference.clone(),
            llama_config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(
                    config.inference.timeout_secs,
                ))
                .build()
                .expect("Failed to create inference HTTP client"),
            pown: pown.clone(),
            detected_model: Arc::new(RwLock::new(None)),
            settlement: app_state.settlement.clone(),
            provider_id: initial_node_id.clone(),
            provider_ergo_address: config.xergon.ergo_address.clone(),
            auto_pull: app_state.auto_pull.clone(),
        };

        xergon_agent::api::serve_with_inference(&config, app_state, inference_state).await?;
    } else {
        info!("Inference proxy disabled (set [inference].enabled = true to enable)");
        xergon_agent::api::serve(&config, app_state).await?;
    }

    discovery_handle.abort();
    llama_handle.abort();

    // Deregister from relay on graceful shutdown
    if let Some(client) = relay_client {
        client.deregister().await;
    }
    if let Some(handle) = relay_handle {
        handle.abort();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// `status` subcommand
// ---------------------------------------------------------------------------

async fn status_command() -> Result<()> {
    let status_url = "http://127.0.0.1:9099/xergon/status";
    let ergo_info_url = "http://127.0.0.1:9053/info";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    // --- Agent status ---
    let resp = client.get(status_url).send().await;

    match resp {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await?;

            println!();
            println!("  ╔══════════════════════════════════════════════╗");
            println!("  ║           XERGON AGENT STATUS                 ║");
            println!("  ╚══════════════════════════════════════════════╝");
            println!();

            // Provider info
            if let Some(pid) = body.get("provider_id").and_then(|v| v.as_str()) {
                println!("  Provider ID:     {}", pid);
            }
            if let Some(name) = body.get("provider_name").and_then(|v| v.as_str()) {
                println!("  Provider Name:   {}", name);
            }
            if let Some(region) = body.get("region").and_then(|v| v.as_str()) {
                println!("  Region:          {}", region);
            }
            if let Some(addr) = body.get("ergo_address").and_then(|v| v.as_str()) {
                println!("  Ergo Address:    {}", addr);
            }
            println!();

            // PoNW score
            if let Some(pown) = body.get("pown_score").and_then(|v| v.as_f64()) {
                println!("  PoNW Score:      {:.2}", pown);
            }
            if let Some(uptime) = body.get("uptime_secs").and_then(|v| v.as_u64()) {
                let hours = uptime / 3600;
                let mins = (uptime % 3600) / 60;
                let secs = uptime % 60;
                println!("  Uptime:          {}h {}m {}s", hours, mins, secs);
            }
            println!();

            // AI model
            if let Some(model) = body.get("ai_model").and_then(|v| v.as_str()) {
                if model.is_empty() {
                    println!("  AI Model:        (none detected)");
                } else {
                    println!("  AI Model:        {}", model);
                }
            }

            // Node health
            if let Some(synced) = body.get("is_synced").and_then(|v| v.as_bool()) {
                let icon = if synced { "✓" } else { "✗" };
                println!("  Node Synced:    {} {}", icon, synced);
            }
            if let Some(height) = body.get("node_height").and_then(|v| v.as_u64()) {
                println!("  Node Height:     {}", height);
            }
            if let Some(peers) = body.get("ergo_peer_count").and_then(|v| v.as_u64()) {
                println!("  Ergo Peers:      {}", peers);
            }

            // Peer discovery
            if let Some(xpeers) = body.get("xergon_peers_found").and_then(|v| v.as_u64()) {
                println!("  Xergon Peers:    {}", xpeers);
            }
            if let Some(confs) = body.get("total_confirmations").and_then(|v| v.as_u64()) {
                println!("  Confirmations:   {}", confs);
            }

            println!();
        }
        Ok(resp) => {
            anyhow::bail!(
                "Agent returned HTTP {} — is it running?",
                resp.status()
            );
        }
        Err(e) => {
            anyhow::bail!(
                "Cannot connect to agent at {}\n  {}\n  Make sure xergon-agent is running: xergon-agent run",
                status_url,
                e
            );
        }
    }

    // --- Ergo node info (best-effort) ---
    if let Ok(resp) = client.get(ergo_info_url).send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                println!("  ─── Ergo Node ───");
                if let Some(ver) = body.get("appVersion").and_then(|v| v.as_str()) {
                    println!("  Version:     {}", ver);
                }
                if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
                    println!("  Network:     {}", name);
                }
                if let Ok(full) = serde_json::to_string_pretty(&body) {
                    // Show a compact version
                    if let Some(h) = body.get("fullHeight").and_then(|v| v.as_u64()) {
                        println!("  Full Height: {}", h);
                    }
                    if let Some(h) = body.get("headersHeight").and_then(|v| v.as_u64()) {
                        println!("  Headers:     {}", h);
                    }
                    let _ = full; // suppress unused warning
                }
                println!();
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// `update` subcommand
// ---------------------------------------------------------------------------

fn update_command() -> Result<()> {
    println!("  Checking for updates...");

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let update_script = std::path::Path::new(&home)
        .join(".xergon")
        .join("bin")
        .join("update.sh");

    if update_script.exists() {
        println!("  Running update script: {}", update_script.display());
        let status = std::process::Command::new("sh")
            .arg(&update_script)
            .status()?;
        if status.success() {
            println!("  Update complete!");
        } else {
            println!("  Update script exited with code: {:?}", status.code());
        }
        std::process::exit(status.code().unwrap_or(1));
    } else {
        println!("  No local update script found at {}", update_script.display());
        println!();
        println!("  To update xergon-agent, run:");
        println!("    curl -sSL https://degens.world/xergon | sh");
        println!();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve the config file path (same logic as AgentConfig::load_from).
fn resolve_config_path(cli_path: Option<PathBuf>) -> PathBuf {
    cli_path
        .or_else(|| std::env::var("XERGON_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

// ---------------------------------------------------------------------------
// `provider` subcommand
// ---------------------------------------------------------------------------

fn provider_command(action: ProviderAction) -> Result<()> {
    match action {
        ProviderAction::SetPrice { model, price, config } => {
            provider_set_price(model, price, config)
        }
        ProviderAction::ListPrices { config } => {
            provider_list_prices(config)
        }
        ProviderAction::RemovePrice { model, config } => {
            provider_remove_price(model, config)
        }
    }
}

/// Set or update a per-model price in the config file.
fn provider_set_price(model: String, price: u64, config_path: Option<PathBuf>) -> Result<()> {
    let path = resolve_config_path(config_path);

    if !path.exists() {
        anyhow::bail!(
            "Config file not found at: {}\n  Run 'xergon-agent setup' first to create a configuration.",
            path.display()
        );
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let mut doc = raw.parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse config as TOML: {}", path.display()))?;

    // Ensure [pricing] table exists
    doc.entry("pricing").or_insert_with(toml_edit::table);

    // Ensure [pricing.models] table exists
    if let Some(pricing) = doc.get_mut("pricing") {
        if let Some(table) = pricing.as_table_mut() {
            table.entry("models").or_insert_with(toml_edit::table);
        }
    }

    // Set the model price
    if let Some(pricing) = doc.get_mut("pricing") {
        if let Some(pricing_table) = pricing.as_table_mut() {
            if let Some(models) = pricing_table.get_mut("models") {
                if let Some(models_table) = models.as_table_mut() {
                    let was_new = !models_table.contains_key(&model);
                    models_table[&model] = toml_edit::value(price as i64);
                    if was_new {
                        println!("  Added price for model '{}': {} nanoERG/1M tokens", model, price);
                    } else {
                        println!("  Updated price for model '{}': {} nanoERG/1M tokens", model, price);
                    }
                }
            }
        }
    }

    // Write back
    let output = doc.to_string();
    std::fs::write(&path, &output)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    println!("  Config updated: {}", path.display());
    println!("  Restart or wait for next heartbeat for changes to take effect.");
    Ok(())
}

/// List current pricing from the config file.
fn provider_list_prices(config_path: Option<PathBuf>) -> Result<()> {
    let path = resolve_config_path(config_path);

    if !path.exists() {
        anyhow::bail!("Config file not found at: {}", path.display());
    }

    let config = AgentConfig::load_from(Some(path.clone()))
        .with_context(|| format!("Failed to load config: {}", path.display()))?;

    println!();
    println!("  Pricing Configuration ({})", path.display());
    println!();
    println!("  Default price: {} nanoERG/1M tokens", config.pricing.default_price_per_1m_tokens);
    println!();

    if config.pricing.models.is_empty() {
        println!("  No per-model overrides set.");
        println!("  All models use the default price.");
    } else {
        println!("  Per-model overrides:");
        // Sort by model name for consistent output
        let mut sorted: Vec<_> = config.pricing.models.iter().collect();
        sorted.sort_by_key(|(k, _)| k.to_string());
        for (model, price) in sorted {
            let is_default = *price == config.pricing.default_price_per_1m_tokens;
            let note = if is_default { " (same as default)" } else { "" };
            println!("    {}  {} nanoERG/1M tokens{}", model, price, note);
        }
    }

    println!();
    println!("  Use 'xergon-agent provider set-price --model <id> --price <n>' to set overrides.");
    println!();
    Ok(())
}

/// Remove a per-model price override from the config file.
fn provider_remove_price(model: String, config_path: Option<PathBuf>) -> Result<()> {
    let path = resolve_config_path(config_path);

    if !path.exists() {
        anyhow::bail!("Config file not found at: {}", path.display());
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let mut doc = raw.parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse config as TOML: {}", path.display()))?;

    // Remove the model entry from [pricing.models]
    let removed = if let Some(pricing) = doc.get_mut("pricing") {
        if let Some(pricing_table) = pricing.as_table_mut() {
            if let Some(models) = pricing_table.get_mut("models") {
                if let Some(models_table) = models.as_table_mut() {
                    models_table.remove(&model).is_some()
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    if !removed {
        anyhow::bail!(
            "No price override found for model '{}'.\n  Use 'xergon-agent provider list-prices' to see current overrides.",
            model
        );
    }

    // Write back
    let output = doc.to_string();
    std::fs::write(&path, &output)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    println!("  Removed price override for model '{}'", model);
    println!("  Config updated: {}", path.display());
    println!("  This model will now use the default price.");
    Ok(())
}

/// Probe llama-server at the given base URL for model information.
///
/// Sends GET {url}/v1/models and extracts the first model ID from the
/// OpenAI-compatible response: `{"data": [{"id": "model-name", ...}]}`.
#[allow(dead_code)]
// Kept as a convenience wrapper; currently only probe_llama_server_all_models is used
async fn probe_llama_server(client: &reqwest::Client, base_url: &str) -> Option<String> {
    probe_llama_server_all_models(client, base_url)
        .await
        .and_then(|mut models| models.pop())
}

/// Probe llama-server at the given base URL and return ALL model IDs.
///
/// Sends GET {url}/v1/models and extracts every model ID from the
/// OpenAI-compatible response: `{"data": [{"id": "model-name", ...}]}`.
async fn probe_llama_server_all_models(
    client: &reqwest::Client,
    base_url: &str,
) -> Option<Vec<String>> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => return None,
    };

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Extract all model ids from {"data": [{"id": "...", ...}, ...]}
    let models: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if models.is_empty() { None } else { Some(models) }
}
