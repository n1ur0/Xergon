//! Xergon Peer Discovery
//!
//! Discovers other Xergon agents by probing the Ergo node's peer list.
//!
//! Flow:
//! 1. Fetch all connected Ergo peers from the local node's REST API
//! 2. For each peer, extract their IP address
//! 3. Probe each IP on the Xergon agent port (default 9099) with GET /xergon/status
//! 4. If the peer responds with a valid Xergon status, record it as a confirmed Xergon peer
//! 5. Persist discovered peers between restarts

use anyhow::{Context, Result};
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::PeerDiscoveryConfig;

/// A discovered Xergon peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XergonPeer {
    /// The peer's provider ID
    pub provider_id: String,
    /// The peer's display name
    pub provider_name: String,
    /// The peer's region
    pub region: String,
    /// The peer's Ergo address
    pub ergo_address: String,
    /// The peer's node ID (SHA-256 of their Ergo node public key)
    pub node_id: String,
    /// The address we used to reach them
    pub discovered_addr: SocketAddr,
    /// When we first discovered this peer
    pub first_seen: chrono::DateTime<chrono::Utc>,
    /// When we last confirmed this peer is alive
    pub last_seen: chrono::DateTime<chrono::Utc>,
    /// Number of successful probes
    pub confirmations: u64,
}

/// The status response from a remote Xergon agent
#[derive(Debug, Deserialize)]
pub struct RemoteXergonStatus {
    pub provider: Option<ProviderInfo>,
    pub pown_status: Option<PownStatusInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub region: String,
}

#[derive(Debug, Deserialize)]
pub struct PownStatusInfo {
    pub node_id: String,
    pub ergo_address: String,
}

/// An Ergo peer as reported by the node's REST API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // TODO: fields will be used for peer info display
pub struct ErgoPeer {
    pub address: Option<String>,
    #[allow(dead_code)]
    pub name: Option<String>,
    #[allow(dead_code)]
    pub last_message: Option<i64>,
    #[allow(dead_code)]
    pub last_handshake: Option<i64>,
    #[allow(dead_code)]
    pub connection_type: Option<String>,
    #[serde(default)]
    pub declared_address: Option<String>,
}

/// Metrics from a single discovery cycle
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiscoveryMetrics {
    pub ergo_peers_fetched: usize,
    pub ergo_peers_with_address: usize,
    pub peers_probed: usize,
    pub xergon_peers_found: usize,
    pub new_peers: usize,
    pub failed_probes: usize,
    pub cycle_duration_ms: u64,
}

/// State of the peer discovery system
#[derive(Debug, Clone, Default, Serialize)]
pub struct PeerDiscoveryState {
    pub peers_checked: usize,
    pub unique_xergon_peers_seen: usize,
    pub xergon_peers: Vec<XergonPeer>,
    pub last_cycle_metrics: Option<DiscoveryMetrics>,
    pub last_cycle_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_cycles: usize,
    pub total_xergon_confirmations: usize,
}

/// The peer discovery engine
pub struct PeerDiscovery {
    config: PeerDiscoveryConfig,
    http_client: Client,
    ergo_rest_url: String,
    /// All known Xergon peers (keyed by node_id for dedup)
    known_peers: Arc<DashMap<String, XergonPeer>>,
    /// Cumulative unique peers ever seen
    unique_peers_seen: Arc<RwLock<HashSet<String>>>,
    /// Total peers checked across all cycles
    total_peers_checked: Arc<std::sync::atomic::AtomicUsize>,
    /// Total cycles run
    total_cycles: Arc<std::sync::atomic::AtomicUsize>,
    /// Total confirmations received
    total_confirmations: Arc<std::sync::atomic::AtomicUsize>,
    /// Peers file path for persistence
    peers_file: Option<PathBuf>,
}

impl PeerDiscovery {
    pub fn new(
        config: PeerDiscoveryConfig,
        ergo_rest_url: String,
    ) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.probe_timeout_secs))
            .connect_timeout(Duration::from_secs(config.probe_timeout_secs))
            .pool_max_idle_per_host(0) // Don't keep connections alive for probing
            .build()
            .context("Failed to build HTTP client for peer discovery")?;

        let peers_file = config.peers_file.clone();

        Ok(Self {
            config,
            http_client,
            ergo_rest_url,
            known_peers: Arc::new(DashMap::new()),
            unique_peers_seen: Arc::new(RwLock::new(HashSet::new())),
            total_peers_checked: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            total_cycles: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            total_confirmations: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            peers_file,
        })
    }

    /// Load previously known peers from disk
    pub async fn load_peers(&self) -> Result<()> {
        if let Some(ref path) = self.peers_file {
            if path.exists() {
                let data = fs::read_to_string(path).await
                    .context("Failed to read peers file")?;
                let peers: Vec<XergonPeer> = serde_json::from_str(&data)
                    .context("Failed to parse peers file")?;
                let peer_count = peers.len();
                for peer in &peers {
                    self.unique_peers_seen.write().await.insert(peer.node_id.clone());
                    self.known_peers.insert(peer.node_id.clone(), peer.clone());
                }
                info!(peer_count, "Loaded known peers from disk");
            }
        }
        Ok(())
    }

    /// Save known peers to disk
    pub async fn save_peers(&self) -> Result<()> {
        if let Some(ref path) = self.peers_file {
            let peers: Vec<XergonPeer> = self.known_peers
                .iter()
                .map(|r| r.value().clone())
                .collect();
            let data = serde_json::to_string_pretty(&peers)
                .context("Failed to serialize peers")?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::write(path, data).await
                .context("Failed to write peers file")?;
            debug!(peer_count = peers.len(), "Saved peers to disk");
        }
        Ok(())
    }

    /// Fetch all Ergo peers from the local node
    async fn fetch_ergo_peers(&self) -> Result<Vec<ErgoPeer>> {
        let url = format!("{}/peers/all", self.ergo_rest_url.trim_end_matches('/'));
        let resp = self.http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch peers from Ergo node")?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "Ergo node returned status {} for /peers/all",
                resp.status()
            );
        }

        let peers: Vec<ErgoPeer> = resp
            .json()
            .await
            .context("Failed to parse Ergo peers response")?;

        Ok(peers)
    }

    /// Parse an Ergo peer address string (e.g., "/76.119.196.68:9020") into an IP
    fn parse_peer_ip(peer: &ErgoPeer) -> Option<IpAddr> {
        // Try declared_address first, then address
        let addr_str = peer.declared_address.as_ref()
            .or(peer.address.as_ref())?;

        // Handle format "/1.2.3.4:port"
        let clean = addr_str.trim_start_matches('/');
        let host = clean.split(':').next()?;

        host.parse::<IpAddr>().ok()
    }

    #[allow(dead_code)] // TODO: will be used for active peer probing
    async fn probe_xergon_agent(
        &self,
        ip: IpAddr,
    ) -> Result<(XergonPeer, RemoteXergonStatus)> {
        let addr = SocketAddr::new(ip, self.config.xergon_agent_port);
        let url = format!("http://{}/xergon/status", addr);

        debug!(peer_addr = %addr, "Probing peer for Xergon agent");

        let resp = self.http_client
            .get(&url)
            .header("X-Xergon-Probe", "1")
            .send()
            .await
            .context("Probe request failed")?;

        if !resp.status().is_success() {
            anyhow::bail!("Non-success status: {}", resp.status());
        }

        let status: RemoteXergonStatus = resp
            .json()
            .await
            .context("Failed to parse Xergon status response")?;

        let provider = status.provider.as_ref()
            .context("Missing provider field in Xergon status")?;
        let pown = status.pown_status.as_ref()
            .context("Missing pown_status field in Xergon status")?;

        // Validate that node_id is a valid 64-char hex string (SHA-256)
        if pown.node_id.len() != 64 || hex::decode(&pown.node_id).is_err() {
            anyhow::bail!("Invalid node_id format from peer");
        }

        let peer = XergonPeer {
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            region: provider.region.clone(),
            ergo_address: pown.ergo_address.clone(),
            node_id: pown.node_id.clone(),
            discovered_addr: addr,
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            confirmations: 1,
        };

        Ok((peer, status))
    }

    /// Run a single discovery cycle
    pub async fn run_discovery_cycle(&self) -> Result<DiscoveryMetrics> {
        let cycle_start = Instant::now();
        let mut metrics = DiscoveryMetrics::default();

        // Step 1: Fetch Ergo peers
        let ergo_peers = match self.fetch_ergo_peers().await {
            Ok(peers) => {
                metrics.ergo_peers_fetched = peers.len();
                info!(peer_count = peers.len(), "Fetched Ergo peers from node");
                peers
            }
            Err(e) => {
                error!(error = %e, "Failed to fetch Ergo peers — is the Ergo node running?");
                return Err(e);
            }
        };

        // Step 2: Extract IPs and limit to max per cycle
        let mut peer_ips: Vec<IpAddr> = ergo_peers
            .iter()
            .filter_map(Self::parse_peer_ip)
            .collect();

        // Deduplicate IPs
        let mut seen = HashSet::new();
        peer_ips.retain(|ip| seen.insert(*ip));

        metrics.ergo_peers_with_address = peer_ips.len();

        if peer_ips.is_empty() {
            warn!("No Ergo peers with parseable addresses — cannot discover Xergon peers");
            return Ok(metrics);
        }

        // Limit to max per cycle
        if peer_ips.len() > self.config.max_peers_per_cycle {
            peer_ips.truncate(self.config.max_peers_per_cycle);
        }

        // Step 3: Probe peers concurrently
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.max_concurrent_probes));
        let mut handles = Vec::new();

        for ip in peer_ips {
            let permit = semaphore.clone().acquire_owned().await?;
            let client = self.http_client.clone();
            let port = self.config.xergon_agent_port;

            handles.push(tokio::spawn(async move {
                let addr = SocketAddr::new(ip, port);
                let url = format!("http://{}/xergon/status", addr);

                let result = client
                    .get(&url)
                    .header("X-Xergon-Probe", "1")
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await;

                drop(permit);

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        resp.json::<RemoteXergonStatus>().await.ok().map(|status| (ip, status))
                    }
                    _ => None,
                }
            }));
        }

        metrics.peers_probed = handles.len();

        // Collect results
        for handle in handles {
            match handle.await {
                Ok(Some((ip, status))) => {
                    metrics.xergon_peers_found += 1;

                    if let (Some(ref provider), Some(ref pown)) = (&status.provider, &status.pown_status) {
                        let now = chrono::Utc::now();
                        let node_id = &pown.node_id;

                        let is_new = {
                            let mut seen = self.unique_peers_seen.write().await;
                            seen.insert(node_id.clone())
                        };

                        if is_new {
                            metrics.new_peers += 1;
                        }

                        // Update or insert peer
                        self.known_peers.entry(node_id.clone())
                            .and_modify(|existing| {
                                existing.last_seen = now;
                                existing.confirmations += 1;
                                // Update discovered addr if we found them on a different IP
                                existing.discovered_addr = SocketAddr::new(ip, self.config.xergon_agent_port);
                            })
                            .or_insert_with(|| XergonPeer {
                                provider_id: provider.id.clone(),
                                provider_name: provider.name.clone(),
                                region: provider.region.clone(),
                                ergo_address: pown.ergo_address.clone(),
                                node_id: node_id.clone(),
                                discovered_addr: SocketAddr::new(ip, self.config.xergon_agent_port),
                                first_seen: now,
                                last_seen: now,
                                confirmations: 1,
                            });

                        self.total_confirmations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        info!(
                            provider_id = %provider.id,
                            node_id = %node_id,
                            addr = %ip,
                            "Discovered Xergon peer"
                        );
                    }
                }
                Ok(None) => {
                    metrics.failed_probes += 1;
                }
                Err(e) => {
                    metrics.failed_probes += 1;
                    debug!(error = %e, "Probe task failed");
                }
            }
        }

        // Update cumulative counters
        let peers_checked = metrics.peers_probed;
        self.total_peers_checked.fetch_add(peers_checked, std::sync::atomic::Ordering::Relaxed);
        self.total_cycles.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        metrics.cycle_duration_ms = cycle_start.elapsed().as_millis() as u64;

        // Persist peers
        if let Err(e) = self.save_peers().await {
            warn!(error = %e, "Failed to persist peers to disk");
        }

        info!(
            ergo_peers = metrics.ergo_peers_fetched,
            probed = metrics.peers_probed,
            found = metrics.xergon_peers_found,
            new = metrics.new_peers,
            failed = metrics.failed_probes,
            duration_ms = metrics.cycle_duration_ms,
            "Discovery cycle complete"
        );

        Ok(metrics)
    }

    /// Get cumulative peers checked count
    pub fn peers_checked(&self) -> usize {
        self.total_peers_checked.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get current peer discovery state (for the API)
    pub async fn get_state(&self, last_metrics: Option<DiscoveryMetrics>) -> PeerDiscoveryState {
        let peers: Vec<XergonPeer> = self.known_peers
            .iter()
            .map(|r| r.value().clone())
            .collect();

        let unique_count = self.unique_peers_seen.read().await.len();

        PeerDiscoveryState {
            peers_checked: self.total_peers_checked.load(std::sync::atomic::Ordering::Relaxed),
            unique_xergon_peers_seen: unique_count,
            xergon_peers: peers,
            last_cycle_metrics: last_metrics,
            last_cycle_at: Some(chrono::Utc::now()),
            total_cycles: self.total_cycles.load(std::sync::atomic::Ordering::Relaxed),
            total_xergon_confirmations: self.total_confirmations.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

}
