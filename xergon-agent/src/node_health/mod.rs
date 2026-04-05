//! Ergo node health monitoring
//!
//! Polls the local Ergo node's REST API to track:
//! - Sync status
//! - Peer count
//! - Chain height
//! - Node ID

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

/// Ergo node info from /info endpoint
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ErgoNodeInfo {
    pub network: String,
    pub name: String,
    pub state_type: String,
    pub peers_count: usize,
    pub headers_height: Option<u32>,
    pub full_height: Option<u32>,
    pub best_header_id: Option<String>,
    pub is_mining: bool,
    pub launch_time: Option<u64>,
    pub app_version: Option<String>,
}

/// Node health state (exposed via API)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeHealthState {
    pub best_height_local: u32,
    pub ergo_address: String,
    pub is_synced: bool,
    pub last_header_id: Option<String>,
    pub node_height: u32,
    pub node_id: String,
    pub peer_count: usize,
    pub timestamp: i64,
}

/// Node health checker
#[derive(Clone)]
pub struct NodeHealthChecker {
    ergo_rest_url: String,
    http_client: Client,
}

impl NodeHealthChecker {
    pub fn new(ergo_rest_url: String) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            ergo_rest_url,
            http_client,
        })
    }

    /// Fetch node info from Ergo REST API
    pub async fn fetch_node_info(&self) -> Result<ErgoNodeInfo> {
        let url = format!("{}/info", self.ergo_rest_url.trim_end_matches('/'));

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch node info")?;

        if !resp.status().is_success() {
            anyhow::bail!("Ergo node returned status {}", resp.status());
        }

        resp.json().await.context("Failed to parse node info")
    }

    /// Derive a stable node_id (64-char hex SHA-256) from the ergo_address.
    ///
    /// This serves as the Xergon agent's unique identity for peer dedup.
    /// Since Ergo's /nodeInfo endpoint requires auth and may not be available,
    /// we derive a stable identity from the provider's ergo_address which is
    /// already a unique identifier in the Xergon context.
    pub fn derive_node_id(ergo_address: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"xergon:node_id:v1:");
        hasher.update(ergo_address.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Compute current health state
    pub async fn check_health(&self, ergo_address: &str) -> Result<NodeHealthState> {
        let info = self.fetch_node_info().await?;

        // Determine if synced: full height exists and is close to best known
        let best_height = info.headers_height.unwrap_or(0);
        let local_height = info.full_height.unwrap_or(best_height);

        // Consider synced if full height is within 2 blocks of headers
        let is_synced = info.full_height.is_some() && best_height.saturating_sub(local_height) <= 2;

        if !is_synced {
            warn!(local_height, best_height, "Node is not fully synced");
        }

        // Derive node_id from ergo_address (stable, deterministic, unique per provider)
        let node_id = Self::derive_node_id(ergo_address);

        debug!(
            network = %info.network,
            peers = info.peers_count,
            height = local_height,
            synced = is_synced,
            node_id = %node_id,
            "Node health check"
        );

        Ok(NodeHealthState {
            best_height_local: best_height,
            ergo_address: ergo_address.to_string(),
            is_synced,
            last_header_id: info.best_header_id,
            node_height: local_height,
            node_id,
            peer_count: info.peers_count,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }
}
