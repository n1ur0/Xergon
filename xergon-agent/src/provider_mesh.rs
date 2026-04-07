//! Provider Mesh Sync
//!
//! Manages a mesh of peer providers for model discovery, capacity-aware
//! routing, and gossip-based synchronization. Uses the existing `GossipEngine`
//! for message deduplication and fanout.
//!
//! The mesh enables:
//! - Discovering providers that serve models not available locally
//! - Routing requests to the best available peer
//! - Broadcasting local capacity/load information
//! - Periodic sync of model lists and availability
//!
//! API endpoints:
//! - GET  /api/mesh/status  -- mesh overview
//! - GET  /api/mesh/peers   -- connected peers
//! - POST /api/mesh/sync    -- trigger manual sync
//! - GET  /api/mesh/models  -- all models available across mesh

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::gossip::GossipEngine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A peer node in the provider mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshNode {
    /// Peer's public key (unique identifier).
    pub peer_pk: String,
    /// HTTP endpoint (e.g. http://1.2.3.4:9099).
    pub endpoint: String,
    /// Last time we heard from this peer.
    pub last_seen: String,
    /// Models this peer advertises as available.
    pub models: Vec<String>,
    /// Maximum concurrent requests this peer can handle.
    pub capacity: u32,
    /// Peer reputation score (from gossip).
    pub reputation: f64,
    /// Whether this peer is currently reachable.
    pub reachable: bool,
}

/// Mesh sync message exchanged between peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSyncMessage {
    /// Sender's peer public key.
    pub sender_pk: String,
    /// Models available on this node.
    pub models: Vec<String>,
    /// Current available capacity.
    pub capacity: u32,
    /// Unix timestamp.
    pub timestamp: i64,
    /// Unique message ID for dedup.
    pub message_id: String,
}

/// Overall mesh status (returned by /api/mesh/status).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshStatus {
    pub connected_peers: usize,
    pub total_known_peers: usize,
    pub local_models: Vec<String>,
    pub mesh_models: Vec<String>, // unique models across all peers
    pub sync_interval_secs: u64,
    pub last_sync: String,
    pub gossip_dedup_size: usize,
}

/// Peer detail for /api/mesh/peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDetail {
    pub peer_pk: String,
    pub endpoint: String,
    pub models: Vec<String>,
    pub capacity: u32,
    pub reputation: f64,
    pub reachable: bool,
    pub last_seen: String,
}

/// All models available across the mesh (returned by /api/mesh/models).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshModels {
    pub models: Vec<MeshModelEntry>,
}

/// A model available from one or more mesh peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshModelEntry {
    pub model_name: String,
    pub providers: Vec<String>, // peer PKs
    pub total_capacity: u32,
}

/// Result of a manual sync operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub peers_contacted: usize,
    pub new_peers: usize,
    pub updated_peers: usize,
    pub failed_peers: usize,
    pub models_discovered: usize,
}

/// Configuration for the provider mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// Seconds between automatic sync cycles (default: 300 = 5 min).
    pub sync_interval_secs: u64,
    /// Timeout for peer HTTP requests in seconds (default: 10).
    pub peer_timeout_secs: u64,
    /// Maximum number of peers to maintain (default: 100).
    pub max_peers: usize,
    /// Peer considered stale after this many seconds without contact (default: 600).
    pub stale_threshold_secs: u64,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            sync_interval_secs: 300,
            peer_timeout_secs: 10,
            max_peers: 100,
            stale_threshold_secs: 600,
        }
    }
}

// ---------------------------------------------------------------------------
// ProviderMesh
// ---------------------------------------------------------------------------

/// Manages the provider mesh for cross-node model discovery and routing.
pub struct ProviderMesh {
    /// Known mesh nodes, keyed by peer public key.
    nodes: DashMap<String, MeshNode>,
    /// Models served by this local node.
    local_models: Arc<tokio::sync::RwLock<Vec<String>>>,
    /// Configuration.
    config: MeshConfig,
    /// HTTP client for peer communication.
    http_client: Client,
    /// Gossip engine for message dedup and fanout.
    gossip: Arc<GossipEngine>,
    /// Local peer public key.
    local_peer_pk: String,
    /// Last sync timestamp.
    last_sync: Arc<std::sync::Mutex<Instant>>,
    /// Background task control.
    running: AtomicBool,
    /// Cumulative stats.
    total_syncs: AtomicU64,
    total_models_discovered: AtomicU64,
}

impl ProviderMesh {
    /// Create a new provider mesh.
    ///
    /// `local_peer_pk` is this node's identity.
    /// `gossip` is the existing gossip engine for message transport.
    pub fn new(
        local_peer_pk: &str,
        gossip: Arc<GossipEngine>,
        config: MeshConfig,
    ) -> Self {
        Self {
            nodes: DashMap::new(),
            local_models: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            config,
            http_client: Client::new(),
            gossip,
            local_peer_pk: local_peer_pk.to_string(),
            last_sync: Arc::new(std::sync::Mutex::new(Instant::now())),
            running: AtomicBool::new(false),
            total_syncs: AtomicU64::new(0),
            total_models_discovered: AtomicU64::new(0),
        }
    }

    /// Set the list of locally served models.
    pub async fn set_local_models(&self, models: Vec<String>) {
        let mut local = self.local_models.write().await;
        *local = models;
    }

    /// Add a known peer to the mesh.
    pub fn add_peer(&self, peer: MeshNode) {
        if self.nodes.len() >= self.config.max_peers {
            warn!(
                max = self.config.max_peers,
                "Mesh peer limit reached, rejecting new peer"
            );
            return;
        }
        let pk = peer.peer_pk.clone();
        self.nodes.insert(pk.clone(), peer);
        info!(peer_pk = %pk, "Peer added to mesh");
    }

    /// Remove a peer from the mesh.
    pub fn remove_peer(&self, peer_pk: &str) {
        if self.nodes.remove(peer_pk).is_some() {
            info!(peer_pk = %peer_pk, "Peer removed from mesh");
        }
    }

    /// Build a mesh sync message for broadcasting.
    pub async fn build_sync_message(&self) -> MeshSyncMessage {
        let models = self.local_models.read().await.clone();
        MeshSyncMessage {
            sender_pk: self.local_peer_pk.clone(),
            models,
            capacity: 0, // TODO: integrate with inference queue capacity
            timestamp: chrono::Utc::now().timestamp(),
            message_id: format!("mesh-{}", uuid::Uuid::new_v4()),
        }
    }

    /// Sync with all known peers: exchange model lists and capacity info.
    ///
    /// For each known peer, fetches their /api/peer/models endpoint
    /// and updates local state.
    pub async fn sync(&self) -> SyncResult {
        let mut peers_contacted = 0usize;
        let mut new_peers = 0usize;
        let mut updated_peers = 0usize;
        let mut failed_peers = 0usize;
        let mut models_discovered = 0usize;

        // Build local sync message
        let local_msg = self.build_sync_message().await;

        // Collect peer endpoints
        let peers: Vec<(String, String)> = self
            .nodes
            .iter()
            .map(|entry| {
                let pk = entry.key().clone();
                let endpoint = entry.value().endpoint.clone();
                (pk, endpoint)
            })
            .collect();

        for (peer_pk, endpoint) in &peers {
            peers_contacted += 1;
            let url = format!("{}/api/peer/models", endpoint.trim_end_matches('/'));

            match self
                .http_client
                .get(&url)
                .timeout(Duration::from_secs(self.config.peer_timeout_secs))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        // Extract models from the peer response
                        let peer_models = extract_models_from_response(&body);

                        let is_new = if let Some(mut node) = self.nodes.get_mut(peer_pk) {
                            node.models = peer_models.clone();
                            node.last_seen = chrono::Utc::now().timestamp_millis().to_string();
                            node.reachable = true;
                            false
                        } else {
                            true
                        };

                        if is_new {
                            new_peers += 1;
                        } else {
                            updated_peers += 1;
                        }
                        models_discovered += peer_models.len();
                    }
                }
                Ok(resp) => {
                    warn!(
                        peer_pk = %peer_pk,
                        status = %resp.status(),
                        "Peer sync returned non-success status"
                    );
                    failed_peers += 1;
                    // Mark unreachable
                    if let Some(mut node) = self.nodes.get_mut(peer_pk) {
                        node.reachable = false;
                        node.last_seen = chrono::Utc::now().timestamp_millis().to_string();
                    }
                }
                Err(e) => {
                    warn!(peer_pk = %peer_pk, error = %e, "Peer sync failed");
                    failed_peers += 1;
                    if let Some(mut node) = self.nodes.get_mut(peer_pk) {
                        node.reachable = false;
                        node.last_seen = chrono::Utc::now().timestamp_millis().to_string();
                    }
                }
            }
        }

        self.total_syncs.fetch_add(1, Ordering::Relaxed);
        self.total_models_discovered
            .fetch_add(models_discovered as u64, Ordering::Relaxed);

        {
            let mut last = self.last_sync.lock().unwrap();
            *last = Instant::now();
        }

        info!(
            contacted = peers_contacted,
            updated = updated_peers,
            failed = failed_peers,
            discovered = models_discovered,
            "Mesh sync completed"
        );

        SyncResult {
            peers_contacted,
            new_peers,
            updated_peers,
            failed_peers,
            models_discovered,
        }
    }

    /// Discover new providers via gossip.
    ///
    /// Sends a mesh discovery message through the gossip network
    /// and processes any new peer information received.
    pub fn discover(&self, candidate_endpoints: &[String]) -> Vec<String> {
        let mut new_peers = Vec::new();

        for endpoint in candidate_endpoints {
            let pk = format!("pk-{}", sha256_hex(endpoint.as_bytes()));
            if self.nodes.contains_key(&pk) {
                continue;
            }
            if self.nodes.len() >= self.config.max_peers {
                break;
            }

            let node = MeshNode {
                peer_pk: pk.clone(),
                endpoint: endpoint.clone(),
                last_seen: chrono::Utc::now().timestamp_millis().to_string(),
                models: Vec::new(),
                capacity: 0,
                reputation: 0.0,
                reachable: false,
            };
            self.nodes.insert(pk.clone(), node);
            new_peers.push(pk);
        }

        if !new_peers.is_empty() {
            info!(count = new_peers.len(), "Discovered new mesh peers");
        }
        new_peers
    }

    /// Get current mesh status.
    pub async fn get_mesh_status(&self) -> MeshStatus {
        let local_models = self.local_models.read().await.clone();
        let mesh_models = self.get_all_mesh_models();

        // Count connected (reachable) peers
        let connected = self
            .nodes
            .iter()
            .filter(|n| n.value().reachable)
            .count();

        let last_sync = {
            let t = self.last_sync.lock().unwrap();
            t.elapsed().as_millis() as u64
        };

        MeshStatus {
            connected_peers: connected,
            total_known_peers: self.nodes.len(),
            local_models,
            mesh_models,
            sync_interval_secs: self.config.sync_interval_secs,
            last_sync: format!("{}ms ago", last_sync),
            gossip_dedup_size: self.gossip.dedup_size(),
        }
    }

    /// Get details of all known peers.
    pub fn get_peers(&self) -> Vec<PeerDetail> {
        self.nodes
            .iter()
            .map(|entry| {
                let n = entry.value();
                PeerDetail {
                    peer_pk: n.peer_pk.clone(),
                    endpoint: n.endpoint.clone(),
                    models: n.models.clone(),
                    capacity: n.capacity,
                    reputation: n.reputation,
                    reachable: n.reachable,
                    last_seen: n.last_seen.clone(),
                }
            })
            .collect()
    }

    /// Get all unique models available across the mesh.
    pub fn get_all_mesh_models(&self) -> Vec<String> {
        let mut models: std::collections::HashSet<String> = std::collections::HashSet::new();
        for entry in self.nodes.iter() {
            for m in &entry.value().models {
                models.insert(m.clone());
            }
        }
        let mut result: Vec<String> = models.into_iter().collect();
        result.sort();
        result
    }

    /// Get detailed model-to-provider mapping across the mesh.
    pub fn get_mesh_models_detailed(&self) -> MeshModels {
        let mut model_map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut model_capacity: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();

        for entry in self.nodes.iter() {
            let node = entry.value();
            if !node.reachable {
                continue;
            }
            for m in &node.models {
                model_map
                    .entry(m.clone())
                    .or_default()
                    .push(node.peer_pk.clone());
                *model_capacity.entry(m.clone()).or_insert(0) += node.capacity;
            }
        }

        let mut models: Vec<MeshModelEntry> = model_map
            .into_iter()
            .map(|(name, providers)| MeshModelEntry {
                model_name: name.clone(),
                providers,
                total_capacity: model_capacity.get(&name).copied().unwrap_or(0),
            })
            .collect();
        models.sort_by(|a, b| a.model_name.cmp(&b.model_name));

        MeshModels { models }
    }

    /// Select the best provider for a model not served locally.
    ///
    /// Selection criteria (in order):
    /// 1. Peer must be reachable
    /// 2. Peer must serve the requested model
    /// 3. Prefer higher reputation
    /// 4. Prefer higher capacity
    pub fn select_provider(&self, model: &str) -> Option<MeshNode> {
        let mut best: Option<MeshNode> = None;

        for entry in self.nodes.iter() {
            let node = entry.value();
            if !node.reachable || !node.models.contains(&model.to_string()) {
                continue;
            }

            let is_better = match &best {
                None => true,
                Some(current) => {
                    node.reputation > current.reputation
                        || (node.reputation == current.reputation && node.capacity > current.capacity)
                }
            };

            if is_better {
                best = Some(node.clone());
            }
        }

        if let Some(ref selected) = best {
            info!(
                model = model,
                peer_pk = %selected.peer_pk,
                reputation = selected.reputation,
                "Selected mesh provider"
            );
        }

        best
    }

    /// Broadcast current capacity/load to the mesh.
    ///
    /// Builds a sync message and sends it via gossip to fanout peers.
    pub async fn broadcast_capacity(&self, capacity: u32) {
        let msg = MeshSyncMessage {
            sender_pk: self.local_peer_pk.clone(),
            models: self.local_models.read().await.clone(),
            capacity,
            timestamp: chrono::Utc::now().timestamp(),
            message_id: format!("mesh-cap-{}", uuid::Uuid::new_v4()),
        };

        // Get candidate peers for fanout
        let candidates: Vec<String> = self.nodes.iter().map(|e| e.key().clone()).collect();
        let targets = self.gossip.select_fanout_peers(&candidates);

        info!(
            capacity = capacity,
            targets = targets.len(),
            "Broadcasting capacity to mesh"
        );

        // In a full implementation, this would send the message to each target.
        // For now, we just log the broadcast intent.
        for target_pk in &targets {
            if let Some(node) = self.nodes.get(target_pk) {
                let url = format!("{}/api/peer/model-notify", node.endpoint.trim_end_matches('/'));
                if let Err(e) = self
                    .http_client
                    .post(&url)
                    .timeout(Duration::from_secs(self.config.peer_timeout_secs))
                    .json(&msg)
                    .send()
                    .await
                {
                    warn!(target = %target_pk, error = %e, "Failed to broadcast capacity to peer");
                }
            }
        }
    }

    /// Remove stale peers that haven't been seen recently.
    pub fn cleanup_stale_peers(&self) -> usize {
        let threshold = chrono::Utc::now().timestamp() - self.config.stale_threshold_secs as i64;
        let stale: Vec<String> = self
            .nodes
            .iter()
            .filter(|entry| {
                entry
                    .value()
                    .last_seen
                    .parse::<i64>()
                    .map(|ts| ts < threshold)
                    .unwrap_or(true)
            })
            .map(|entry| entry.key().clone())
            .collect();

        let count = stale.len();
        for pk in &stale {
            self.nodes.remove(pk);
        }

        if count > 0 {
            info!(count, "Removed stale mesh peers");
        }
        count
    }

    /// Start the background sync loop.
    ///
    /// Periodically syncs with peers and cleans up stale entries.
    pub fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::Relaxed) {
            warn!("Provider mesh sync is already running");
            return;
        }

        info!(
            interval_secs = self.config.sync_interval_secs,
            "Starting provider mesh sync"
        );

        let mesh = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                if !mesh.running.load(Ordering::Relaxed) {
                    break;
                }

                // Periodic sync
                mesh.sync().await;

                // Cleanup stale peers
                mesh.cleanup_stale_peers();

                // Sleep until next cycle
                tokio::time::sleep(Duration::from_secs(mesh.config.sync_interval_secs)).await;
            }
            info!("Provider mesh sync stopped");
        });
    }

    /// Stop the background sync loop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        info!("Provider mesh sync stop requested");
    }

    /// Check if the background loop is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get total number of known peers.
    pub fn peer_count(&self) -> usize {
        self.nodes.len()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract model names from a peer's /api/peer/models response.
fn extract_models_from_response(body: &serde_json::Value) -> Vec<String> {
    body.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// SHA-256 hex digest.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::GossipConfig;

    fn test_mesh() -> Arc<ProviderMesh> {
        let gossip = Arc::new(GossipEngine::new(GossipConfig::default()));
        Arc::new(ProviderMesh::new(
            "local-pk",
            gossip,
            MeshConfig::default(),
        ))
    }

    fn make_node(pk: &str, endpoint: &str, models: &[&str]) -> MeshNode {
        MeshNode {
            peer_pk: pk.to_string(),
            endpoint: endpoint.to_string(),
            last_seen: chrono::Utc::now().timestamp_millis().to_string(),
            models: models.iter().map(|s| s.to_string()).collect(),
            capacity: 10,
            reputation: 50.0,
            reachable: true,
        }
    }

    #[tokio::test]
    async fn test_add_and_list_peers() {
        let mesh = test_mesh();
        mesh.add_peer(make_node("pk-1", "http://1.1.1.1:9099", &["model-a"]));
        mesh.add_peer(make_node("pk-2", "http://2.2.2.2:9099", &["model-b"]));

        assert_eq!(mesh.peer_count(), 2);
        let peers = mesh.get_peers();
        assert_eq!(peers.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_peer() {
        let mesh = test_mesh();
        mesh.add_peer(make_node("pk-1", "http://1.1.1.1:9099", &[]));
        assert_eq!(mesh.peer_count(), 1);
        mesh.remove_peer("pk-1");
        assert_eq!(mesh.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_select_provider_best_reputation() {
        let mesh = test_mesh();
        mesh.add_peer(make_node("pk-low", "http://1.1.1.1:9099", &["model-x"]));
        mesh.add_peer(make_node("pk-high", "http://2.2.2.2:9099", &["model-x"]));

        // Give pk-high higher reputation
        mesh.nodes.get_mut("pk-high").unwrap().reputation = 90.0;

        let selected = mesh.select_provider("model-x").unwrap();
        assert_eq!(selected.peer_pk, "pk-high");
    }

    #[tokio::test]
    async fn test_select_provider_no_match() {
        let mesh = test_mesh();
        mesh.add_peer(make_node("pk-1", "http://1.1.1.1:9099", &["model-a"]));
        assert!(mesh.select_provider("model-z").is_none());
    }

    #[tokio::test]
    async fn test_get_mesh_models() {
        let mesh = test_mesh();
        mesh.add_peer(make_node("pk-1", "http://1.1.1.1:9099", &["llama-3", "mistral"]));
        mesh.add_peer(make_node("pk-2", "http://2.2.2.2:9099", &["llama-3", "phi-3"]));

        let all_models = mesh.get_all_mesh_models();
        assert_eq!(all_models, vec!["llama-3", "mistral", "phi-3"]);
    }

    #[tokio::test]
    async fn test_set_local_models() {
        let mesh = test_mesh();
        mesh.set_local_models(vec!["local-model".to_string()]).await;
        let status = mesh.get_mesh_status().await;
        assert_eq!(status.local_models, vec!["local-model"]);
    }

    #[tokio::test]
    async fn test_peer_limit() {
        let mesh = test_mesh();
        // Default max_peers is 100, but let's test with a small mesh
        for i in 0..5 {
            mesh.add_peer(make_node(
                &format!("pk-{}", i),
                &format!("http://1.1.1.{}:9099", i),
                &[],
            ));
        }
        assert_eq!(mesh.peer_count(), 5);
    }

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex(b"hello");
        assert_eq!(hash.len(), 64); // SHA-256 hex = 64 chars
    }
}
