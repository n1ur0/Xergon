//! Xergon Agent — Rust sidecar for Ergo nodes
//!
//! Provides:
//! - Peer discovery (finds other Xergon agents via Ergo peer list)
//! - PoNW scoring (Proof-of-Node-Work)
//! - Node health monitoring
//! - REST API for marketplace integration

mod api;
mod config;
mod hardware;
mod inference;
mod node_health;
mod peer_discovery;
mod pown;
mod relay_client;
mod settlement;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use api::AppState;
use config::AgentConfig;
use node_health::NodeHealthChecker;
use peer_discovery::PeerDiscovery;
use pown::PownCalculator;
use settlement::SettlementEngine;

#[derive(Parser, Debug)]
#[command(name = "xergon-agent", about = "Xergon Agent — P2P AI compute node for Ergo")]
struct Args {
    /// Path to config file
    #[arg(short, long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "xergon_agent=info".into()),
        )
        .with_target(false)
        .with_thread_ids(false)
        .init();

    let args = Args::parse();

    info!("Xergon Agent starting...");

    let config = AgentConfig::load_from(args.config).context("Failed to load configuration")?;

    info!(
        provider_id = %config.xergon.provider_id,
        region = %config.xergon.region,
        ergo_url = %config.ergo_node.rest_url,
        listen_addr = %config.api.listen_addr,
        "Configuration loaded"
    );

    let health_checker = NodeHealthChecker::new(config.ergo_node.rest_url.clone())
        .context("Failed to create node health checker")?;

    let pown = Arc::new(PownCalculator::new(config.xergon.clone()));

    let discovery = Arc::new(PeerDiscovery::new(
        config.peer_discovery.clone(),
        config.ergo_node.rest_url.clone(),
    ).context("Failed to create peer discovery")?);

    discovery.load_peers().await
        .context("Failed to load persisted peers")?;

    // Initial node health check
    match health_checker.check_health(&config.xergon.ergo_address).await {
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

    let peer_state = Arc::new(RwLock::new(peer_discovery::PeerDiscoveryState::default()));
    let node_health_state = Arc::new(RwLock::new(node_health::NodeHealthState {
        best_height_local: 0,
        ergo_address: config.xergon.ergo_address.clone(),
        is_synced: false,
        last_header_id: None,
        node_height: 0,
        node_id: initial_node_id.clone(),
        peer_count: 0,
        timestamp: chrono::Utc::now().timestamp(),
    }));

    let mut app_state = AppState {
        xergon_config: config.xergon.clone(),
        pown_status: pown.status(),
        peer_state: peer_state.clone(),
        node_health: node_health_state.clone(),
        settlement: None,
        start_time: std::time::Instant::now(),
        management_api_key: config.api.api_key.clone(),
    };

    // Initialize settlement engine if enabled
    if config.settlement.enabled {
        info!(
            interval_secs = config.settlement.interval_secs,
            dry_run = config.settlement.dry_run,
            "Initializing ERG settlement engine"
        );

        match SettlementEngine::new(
            config.settlement.clone(),
            config.ergo_node.rest_url.clone(),
        ) {
            Ok(engine) => {
                if let Err(e) = engine.init().await {
                    tracing::warn!(error = %e, "Settlement engine init failed, starting without persistence");
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

                // Keep handles alive (we won't abort them on shutdown since settlements
                // should complete if in-progress)
                drop(settlement_handle);
                drop(confirm_handle);

                info!("ERG settlement engine started");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create settlement engine, running without settlement");
            }
        }
    } else {
        info!("Settlement engine disabled (set [settlement].enabled = true to enable)");
    }

    // Initialize relay registration client
    let relay_client = if config.relay.register_on_start && !config.relay.relay_url.is_empty() && !config.relay.token.is_empty() {
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

        match relay_client::RelayClient::new(
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
                    Err(e) => tracing::warn!(error = %e, "Initial relay registration failed (will retry)"),
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

    // Spawn relay heartbeat loop
    let relay_handle = if let Some(ref client) = relay_client {
        let client = client.clone();
        let pown = pown.clone();
        let handle = tokio::spawn(async move {
            client.spawn_heartbeat_loop(move || {
                // Get current model from pown status
                let status = pown.status();
                // We can't block here, so we use try_read
                let result = match status.try_read() {
                    Ok(s) if !s.ai_model.is_empty() => vec![s.ai_model.clone()],
                    _ => vec![],
                };
                result
            });
        });
        Some(handle)
    } else {
        None
    };

    // Spawn llama-server health check loop
    let llama_handle = {
        let pown = pown.clone();
        let llama_url = config.llama_server.url.clone();
        let llama_interval = config.llama_server.health_check_interval_secs;

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
            let mut last_known_model: Option<String> = None;

            loop {
                let detected = probe_llama_server(&client, &llama_url).await;
                let detected = if detected.is_none() {
                    // Try fallback port if primary failed
                    if let Some(ref fb) = fallback_url {
                        probe_llama_server(&client, fb).await
                    } else {
                        None
                    }
                } else {
                    detected
                };

                match detected {
                    Some(model_name) => {
                        if last_known_model.as_deref() != Some(&model_name) {
                            info!(
                                model = %model_name,
                                "llama-server detected — AI inference backend available"
                            );
                        }
                        // Call update_ai_stats to mark AI as enabled with model name.
                        // Pass (0, 0) so we don't inflate counters — only the model name matters here.
                        pown.update_ai_stats(&model_name, 0, 0).await;
                        last_known_model = Some(model_name);
                    }
                    None => {
                        if last_known_model.is_some() {
                            info!("llama-server not responding — AI backend marked as offline");
                        }
                        last_known_model = None;
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
                        ).await;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Discovery cycle failed");
                    }
                }

                tokio::time::sleep(interval).await;
            }
        })
    };

    // Start API server (blocks until shutdown)
    if config.inference.enabled {
        info!(
            backend_url = %config.inference.url,
            "Inference proxy enabled — adding /v1/chat/completions and /v1/models routes"
        );

        let inference_state = inference::InferenceState {
            config: config.inference.clone(),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(config.inference.timeout_secs))
                .build()
                .expect("Failed to create inference HTTP client"),
            pown: pown.clone(),
            detected_model: Arc::new(RwLock::new(None)),
            settlement: app_state.settlement.clone(),
            provider_id: initial_node_id.clone(),
            provider_ergo_address: config.xergon.ergo_address.clone(),
        };

        api::serve_with_inference(&config, app_state, inference_state).await?;
    } else {
        info!("Inference proxy disabled (set [inference].enabled = true to enable)");
        api::serve(&config, app_state).await?;
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

/// Probe llama-server at the given base URL for model information.
///
/// Sends GET {url}/v1/models and extracts the first model ID from the
/// OpenAI-compatible response: `{"data": [{"id": "model-name", ...}]}`.
async fn probe_llama_server(client: &reqwest::Client, base_url: &str) -> Option<String> {
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

    // Extract first model id from {"data": [{"id": "...", ...}]}
    body.get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}
