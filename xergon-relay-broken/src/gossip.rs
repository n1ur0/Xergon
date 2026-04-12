//! Multi-relay gossip protocol for provider health consensus.
//!
//! Each relay maintains a list of peer relays. Periodically, it:
//! 1. Collects its current provider list
//! 2. Sends a GossipMessage to all peers via HTTP POST
//! 3. Receives GossipMessages from peers
//! 4. Merges peer data using conflict resolution rules:
//!    - Higher version wins
//!    - On version tie, prefer online status (optimistic)
//!    - Provider is offline only when >= 2 relays report it offline
//!
//! When GOSSIP_PEERS is empty, gossip is disabled and the relay works in solo mode.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router, routing::{get, post}};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::config::GossipConfig;
use crate::provider::ProviderRegistry;
use crate::proxy::AppState;
use crate::ws::WsBroadcaster;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Status of a provider as seen by a relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderGossipEntry {
    pub status: String,          // "online" or "offline"
    pub last_heartbeat: i64,     // unix timestamp
    pub version: u64,            // monotonic counter, incremented on status change
}

/// A view of all providers from a single peer relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerView {
    pub relay_id: String,
    pub timestamp: i64,
    pub providers: HashMap<String, ProviderGossipEntry>,
}

/// The merged consensus entry for a single provider.
#[derive(Debug, Clone, Serialize)]
pub struct ConsensusEntry {
    pub provider_id: String,
    pub status: String,
    pub offline_count: usize,
    pub total_reports: usize,
    pub highest_version: u64,
    pub last_updated: i64,
}

/// The gossip sync message sent between relays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub relay_id: String,
    pub timestamp: i64,
    pub providers: HashMap<String, ProviderGossipEntry>,
}

// ---------------------------------------------------------------------------
// GossipService
// ---------------------------------------------------------------------------

/// Manages gossip protocol state and background sync task.
pub struct GossipService {
    relay_id: String,
    peers: Vec<String>,
    local_provider_list: Arc<RwLock<HashMap<String, ProviderGossipEntry>>>,
    /// Per-provider version counter (incremented on local status change)
    version_counters: Arc<RwLock<HashMap<String, AtomicU64>>>,
    remote_views: Arc<RwLock<HashMap<String, PeerView>>>,
    consensus_list: Arc<RwLock<HashMap<String, ConsensusEntry>>>,
    gossip_interval: Duration,
    enabled: bool,
}

impl GossipService {
    /// Create a new gossip service. Returns disabled if no peers configured.
    pub fn new(config: &GossipConfig) -> Self {
        let enabled = config.enabled || !config.peers.is_empty();
        let peers: Vec<String> = config
            .peers
            .iter()
            .map(|p| p.trim_end_matches('/').to_string())
            .filter(|p| !p.is_empty())
            .collect();

        Self {
            relay_id: if config.relay_id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                config.relay_id.clone()
            },
            peers,
            local_provider_list: Arc::new(RwLock::new(HashMap::new())),
            version_counters: Arc::new(RwLock::new(HashMap::new())),
            remote_views: Arc::new(RwLock::new(HashMap::new())),
            consensus_list: Arc::new(RwLock::new(HashMap::new())),
            gossip_interval: Duration::from_secs(config.interval_secs.max(5)),
            enabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.peers.is_empty()
    }

    pub fn relay_id(&self) -> &str {
        &self.relay_id
    }

    /// Update local provider list from the provider registry.
    /// Called each health poll cycle.
    pub fn update_local_providers(&self, registry: &ProviderRegistry) {
        let mut local = self.local_provider_list.write().unwrap();
        let mut versions = self.version_counters.write().unwrap();

        for entry in registry.providers.iter() {
            let p = entry.value();
            let provider_id = p
                .status
                .as_ref()
                .and_then(|s| s.provider.as_ref())
                .map(|info| info.id.clone())
                .unwrap_or_else(|| p.endpoint.clone());

            let new_status = if p.is_healthy { "online" } else { "offline" };

            // Check if status changed
            let status_changed = local
                .get(&provider_id)
                .map(|e| e.status != new_status)
                .unwrap_or(true);

            if status_changed {
                // Increment version on status change
                let counter = versions
                    .entry(provider_id.clone())
                    .or_insert_with(|| AtomicU64::new(0));
                let new_version = counter.fetch_add(1, Ordering::Relaxed) + 1;

                local.insert(
                    provider_id.clone(),
                    ProviderGossipEntry {
                        status: new_status.to_string(),
                        last_heartbeat: chrono::Utc::now().timestamp(),
                        version: new_version,
                    },
                );
            } else {
                // Update heartbeat even if status unchanged
                if let Some(existing) = local.get_mut(&provider_id) {
                    existing.last_heartbeat = chrono::Utc::now().timestamp();
                } else {
                    // New provider with no prior state
                    let counter = versions
                        .entry(provider_id.clone())
                        .or_insert_with(|| AtomicU64::new(0));
                    local.insert(
                        provider_id.clone(),
                        ProviderGossipEntry {
                            status: new_status.to_string(),
                            last_heartbeat: chrono::Utc::now().timestamp(),
                            version: counter.load(Ordering::Relaxed),
                        },
                    );
                }
            }
        }
    }

    /// Merge a peer's view into the consensus list.
    /// Returns the set of providers that changed status (for WebSocket broadcast).
    pub fn merge_peer_view(&self, peer_view: PeerView) -> Vec<(String, String)> {
        // Store the peer's view
        self.remote_views
            .write()
            .unwrap()
            .insert(peer_view.relay_id.clone(), peer_view.clone());

        // Collect all provider IDs from all views (local + remote)
        let mut all_providers: HashMap<String, Vec<ProviderGossipEntry>> = HashMap::new();

        // Add local entries
        {
            let local = self.local_provider_list.read().unwrap();
            for (id, entry) in local.iter() {
                all_providers
                    .entry(id.clone())
                    .or_default()
                    .push(entry.clone());
            }
        }

        // Add remote entries
        {
            let remotes = self.remote_views.read().unwrap();
            for (_peer_id, view) in remotes.iter() {
                for (id, entry) in view.providers.iter() {
                    all_providers
                        .entry(id.clone())
                        .or_default()
                        .push(entry.clone());
                }
            }
        }

        let mut consensus = self.consensus_list.write().unwrap();
        let mut changes: Vec<(String, String)> = Vec::new();
        let now = chrono::Utc::now().timestamp();

        for (provider_id, entries) in all_providers.iter() {
            let total_reports = entries.len();

            // Count offline reports and find highest version
            let mut offline_count = 0usize;
            let mut highest_version = 0u64;
            let mut any_online = false;

            for entry in entries {
                if entry.status == "offline" {
                    offline_count += 1;
                } else {
                    any_online = true;
                }
                if entry.version > highest_version {
                    highest_version = entry.version;
                }
            }

            // Consensus rules:
            // - Offline only when >= 2 relays report offline
            // - If total_reports < 2 (solo or only 1 peer), trust local
            // - On version tie, prefer online (optimistic)
            let consensus_status = if total_reports >= 2 && offline_count >= 2 {
                "offline".to_string()
            } else if any_online {
                "online".to_string()
            } else {
                // All say offline but < 2 reports -- trust the one report
                "offline".to_string()
            };

            let prev_status = consensus
                .get(provider_id)
                .map(|e| e.status.clone())
                .unwrap_or_default();

            consensus.insert(
                provider_id.clone(),
                ConsensusEntry {
                    provider_id: provider_id.clone(),
                    status: consensus_status.clone(),
                    offline_count,
                    total_reports,
                    highest_version,
                    last_updated: now,
                },
            );

            // Track status transitions
            if !prev_status.is_empty() && prev_status != consensus_status {
                changes.push((provider_id.clone(), consensus_status.clone()));
            }
        }

        changes
    }

    /// Get a snapshot of the current consensus list.
    pub fn get_consensus_list(&self) -> HashMap<String, ConsensusEntry> {
        self.consensus_list.read().unwrap().clone()
    }

    /// Get the list of known peers.
    pub fn get_peers(&self) -> Vec<String> {
        self.peers.clone()
    }

    /// Build a GossipMessage from current local provider list.
    pub fn build_gossip_message(&self) -> GossipMessage {
        let local = self.local_provider_list.read().unwrap();
        GossipMessage {
            msg_type: "gossip_sync".to_string(),
            relay_id: self.relay_id.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            providers: local.clone(),
        }
    }

    /// Spawn the background gossip task.
    /// Periodically sends local state to peers and processes responses.
    pub fn start_gossip(
        self: Arc<Self>,
        registry: Arc<ProviderRegistry>,
        ws_broadcaster: Arc<WsBroadcaster>,
        http_client: reqwest::Client,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !self.is_enabled() {
            info!("Gossip disabled (no peers configured) -- running in solo mode");
            return None;
        }

        info!(
            relay_id = %self.relay_id,
            peers = ?self.peers,
            interval_secs = self.gossip_interval.as_secs(),
            "Starting gossip consensus service"
        );

        Some(tokio::spawn(async move {
            loop {
                // 1. Update local provider list from registry
                self.update_local_providers(&registry);

                // 2. Build gossip message
                let msg = self.build_gossip_message();

                // 3. Send to all peers concurrently
                let peer_urls: Vec<String> = self.peers.clone();
                let mut handles = Vec::new();

                for peer_url in peer_urls {
                    let msg_clone = msg.clone();
                    let client = http_client.clone();
                    let url = format!("{}/api/v1/gossip/sync", peer_url);

                    handles.push(tokio::spawn(async move {
                        match client
                            .post(&url)
                            .timeout(Duration::from_secs(10))
                            .json(&msg_clone)
                            .send()
                            .await
                        {
                            Ok(resp) if resp.status().is_success() => {
                                match resp.json::<GossipMessage>().await {
                                    Ok(peer_msg) => Ok(peer_msg),
                                    Err(e) => {
                                        debug!(error = %e, "Failed to parse gossip response");
                                        Err(e.to_string())
                                    }
                                }
                            }
                            Ok(resp) => {
                                debug!(status = %resp.status(), "Gossip sync rejected");
                                Err(format!("HTTP {}", resp.status()))
                            }
                            Err(e) => {
                                debug!(error = %e, "Gossip sync failed");
                                Err(e.to_string())
                            }
                        }
                    }));
                }

                // 4. Process peer responses and merge
                for handle in handles {
                    if let Ok(Ok(peer_msg)) = handle.await {
                        let peer_view = PeerView {
                            relay_id: peer_msg.relay_id.clone(),
                            timestamp: peer_msg.timestamp,
                            providers: peer_msg.providers,
                        };
                        let changes = self.merge_peer_view(peer_view);
                        // Broadcast any consensus-driven status changes via WebSocket
                        for (provider_id, status) in changes {
                            info!(
                                provider_id = %provider_id,
                                status = %status,
                                "Consensus status change from gossip"
                            );
                            ws_broadcaster.notify_provider_status(provider_id, status);
                        }
                    }
                }

                // 5. Sleep until next gossip round
                tokio::time::sleep(self.gossip_interval).await;
            }
        }))
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/gossip/sync -- receive gossip from a peer relay.
/// Returns our own provider list as the response.
pub async fn gossip_sync_handler(
    State(state): State<AppState>,
    Json(msg): Json<GossipMessage>,
) -> impl IntoResponse {
    if let Some(ref gossip) = state.gossip_service {
        // Merge incoming peer view
        let peer_view = PeerView {
            relay_id: msg.relay_id.clone(),
            timestamp: msg.timestamp,
            providers: msg.providers,
        };
        let changes = gossip.merge_peer_view(peer_view);

        // Broadcast consensus changes via WebSocket
        for (provider_id, status) in changes {
            info!(
                provider_id = %provider_id,
                status = %status,
                source = "incoming_gossip",
                "Consensus status change"
            );
            state
                .ws_broadcaster
                .notify_provider_status(provider_id, status);
        }

        // Respond with our own provider list
        let response = gossip.build_gossip_message();
        (StatusCode::OK, Json(response)).into_response()
    } else {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}

/// GET /api/v1/gossip/peers -- list known peer relays.
pub async fn gossip_peers_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    #[derive(Serialize)]
    struct PeersResponse {
        relay_id: String,
        enabled: bool,
        peers: Vec<String>,
        peer_count: usize,
    }

    match &state.gossip_service {
        Some(gossip) => {
            let peers = gossip.get_peers();
            let relay_id = gossip.relay_id().to_string();
            let enabled = gossip.is_enabled();
            Json(PeersResponse {
                relay_id,
                enabled,
                peer_count: peers.len(),
                peers,
            }).into_response()
        }
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// GET /api/v1/gossip/status -- show current consensus state.
pub async fn gossip_status_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    #[derive(Serialize)]
    struct StatusResponse {
        relay_id: String,
        enabled: bool,
        provider_count: usize,
        online_count: usize,
        offline_count: usize,
        providers: Vec<ConsensusEntry>,
    }

    match &state.gossip_service {
        Some(gossip) => {
            let consensus = gossip.get_consensus_list();
            let online_count = consensus.values().filter(|e| e.status == "online").count();
            let offline_count = consensus.values().filter(|e| e.status == "offline").count();
            let providers: Vec<ConsensusEntry> = consensus.into_values().collect();
            Json(StatusResponse {
                relay_id: gossip.relay_id().to_string(),
                enabled: gossip.is_enabled(),
                provider_count: providers.len(),
                online_count,
                offline_count,
                providers,
            }).into_response()
        }
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// Build the gossip router with all endpoints.
pub fn build_gossip_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/gossip/sync", post(gossip_sync_handler))
        .route("/api/v1/gossip/peers", get(gossip_peers_handler))
        .route("/api/v1/gossip/status", get(gossip_status_handler))
}
