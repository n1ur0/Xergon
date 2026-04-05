//! P2P Provider-to-Provider Communication
//!
//! Enables Xergon agents to communicate with each other for:
//! - Peer info exchange (models, capacity, load)
//! - Model notification (announce new models to peers)
//! - Peer verification (checking uptime, model availability)
//!
//! Approach: Uses the existing Ergo P2P network as the discovery layer,
//! but communicates via HTTP between agent REST APIs.
//!
//! Endpoints added to the agent:
//!   GET  /api/peer/info       — returns this agent's info (models, capacity, load)
//!   POST /api/peer/model-notify — notify peers about a new model
//!   GET  /api/peer/models     — list models available on this peer
//!   POST /api/peer/proxy-request — proxy an inference request to another provider (load balancing)
//!
//! Peer discovery integration:
//! - Existing peer_discovery.rs scans Ergo peers for Xergon agents
//! - When a peer is discovered, exchange agent info via REST
//! - Store peer agent info in a local cache
//! - Periodically re-check peers for updated info

use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

use crate::config::XergonConfig;

/// Information about this agent for P2P exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAgentInfo {
    /// Provider identifier
    pub provider_id: String,
    /// Provider display name
    pub provider_name: String,
    /// Provider region
    pub region: String,
    /// List of models this agent serves
    pub models: Vec<String>,
    /// Provider endpoint (base URL)
    pub endpoint: String,
    /// Current load factor (0.0 = idle, 1.0 = at capacity)
    pub load_factor: f64,
    /// PoNW score
    pub pown_score: f64,
    /// Whether the agent is healthy and accepting requests
    pub is_healthy: bool,
    /// Total tokens processed (cumulative)
    pub total_tokens: u64,
    /// Total requests served (cumulative)
    pub total_requests: u64,
    /// Timestamp of this info (ISO 8601)
    pub timestamp: String,
}

/// A model notification sent between peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelNotifyRequest {
    /// The model being announced
    pub model_name: String,
    /// The announcing provider's endpoint
    pub provider_endpoint: String,
    /// The announcing provider's ID
    pub provider_id: String,
}

/// A proxy request for load balancing between providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequest {
    /// The inference request body (OpenAI-compatible)
    pub request_body: serde_json::Value,
    /// The target provider endpoint
    pub target_endpoint: String,
    /// Timeout in seconds
    pub timeout_secs: u32,
}

/// Response from a proxy request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResponse {
    /// The response body from the target provider
    pub response_body: serde_json::Value,
    /// The target provider's endpoint
    pub target_endpoint: String,
    /// Round-trip latency in ms
    pub latency_ms: u64,
}

/// Configuration for P2P communication
#[derive(Debug, Clone, Deserialize)]
pub struct P2PConfig {
    /// Enable P2P peer communication (default: true)
    #[serde(default = "default_p2p_enabled")]
    pub enabled: bool,
    /// How often to re-check peers for updated info (seconds, default: 120)
    #[serde(default = "default_peer_refresh_interval")]
    pub peer_refresh_interval_secs: u64,
    /// Timeout for P2P API calls (seconds, default: 5)
    #[serde(default = "default_p2p_timeout")]
    pub timeout_secs: u64,
    /// Maximum number of peer agent info entries to cache (default: 50)
    #[serde(default = "default_max_peers")]
    pub max_cached_peers: usize,
}

fn default_p2p_enabled() -> bool {
    true
}
fn default_peer_refresh_interval() -> u64 {
    120
}
fn default_p2p_timeout() -> u64 {
    5
}
fn default_max_peers() -> usize {
    50
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            enabled: default_p2p_enabled(),
            peer_refresh_interval_secs: default_peer_refresh_interval(),
            timeout_secs: default_p2p_timeout(),
            max_cached_peers: default_max_peers(),
        }
    }
}

/// The P2P communication engine
pub struct P2PEngine {
    config: P2PConfig,
    http_client: Client,
    /// Cache of peer agent info, keyed by endpoint URL
    peer_cache: Arc<DashMap<String, PeerAgentInfo>>,
    /// This agent's own info (updated periodically)
    self_info: Arc<tokio::sync::RwLock<PeerAgentInfo>>,
}

impl P2PEngine {
    /// Create a new P2P engine.
    pub fn new(config: P2PConfig, xergon_config: &XergonConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .connect_timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("Failed to build HTTP client for P2P engine")?;

        let self_info = PeerAgentInfo {
            provider_id: xergon_config.provider_id.clone(),
            provider_name: xergon_config.provider_name.clone(),
            region: xergon_config.region.clone(),
            models: Vec::new(),
            endpoint: String::new(), // Set later when API server is bound
            load_factor: 0.0,
            pown_score: 0.0,
            is_healthy: true,
            total_tokens: 0,
            total_requests: 0,
            timestamp: Utc::now().to_rfc3339(),
        };

        Ok(Self {
            config,
            http_client,
            peer_cache: Arc::new(DashMap::new()),
            self_info: Arc::new(tokio::sync::RwLock::new(self_info)),
        })
    }

    /// Set the agent's own endpoint URL (called after API server is bound).
    pub async fn set_self_endpoint(&self, endpoint: String) {
        let mut info = self.self_info.write().await;
        info.endpoint = endpoint;
    }

    /// Update the agent's own info (called from heartbeat/pown update).
    pub async fn update_self_info(
        &self,
        models: Vec<String>,
        load_factor: f64,
        pown_score: f64,
        total_tokens: u64,
        total_requests: u64,
    ) {
        let mut info = self.self_info.write().await;
        info.models = models;
        info.load_factor = load_factor;
        info.pown_score = pown_score;
        info.total_tokens = total_tokens;
        info.total_requests = total_requests;
        info.timestamp = Utc::now().to_rfc3339();
        info.is_healthy = true;
    }

    /// Get this agent's info for the /api/peer/info endpoint.
    pub async fn get_self_info(&self) -> PeerAgentInfo {
        self.self_info.read().await.clone()
    }

    /// Get the current load factor (for external querying).
    pub async fn get_load_factor(&self) -> f64 {
        self.self_info.read().await.load_factor
    }

    /// Exchange info with a peer agent.
    ///
    /// Calls GET /api/peer/info on the peer and caches the response.
    /// Returns the peer's info if successful.
    pub async fn exchange_peer_info(&self, peer_endpoint: &str) -> Result<PeerAgentInfo> {
        let url = format!(
            "{}/api/peer/info",
            peer_endpoint.trim_end_matches('/')
        );

        let resp = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .send()
            .await
            .context("Failed to reach peer for info exchange")?
            .error_for_status()
            .context("Peer returned error for info exchange")?;

        let peer_info: PeerAgentInfo = resp
            .json()
            .await
            .context("Failed to parse peer info response")?;

        // Cache the peer info
        if self.peer_cache.len() >= self.config.max_cached_peers {
            // Evict the oldest entry (simple strategy: remove first)
            if let Some(oldest) = self.peer_cache.iter().next() {
                let key = oldest.key().clone();
                drop(oldest);
                self.peer_cache.remove(&key);
            }
        }

        self.peer_cache
            .insert(peer_info.endpoint.clone(), peer_info.clone());

        debug!(
            peer_id = %peer_info.provider_id,
            peer_endpoint = %peer_info.endpoint,
            models = peer_info.models.len(),
            "Exchanged info with peer"
        );

        Ok(peer_info)
    }

    /// Notify a peer about a new model.
    pub async fn notify_peer_model(
        &self,
        peer_endpoint: &str,
        model_name: &str,
        self_endpoint: &str,
        self_provider_id: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/api/peer/model-notify",
            peer_endpoint.trim_end_matches('/')
        );

        let req = ModelNotifyRequest {
            model_name: model_name.to_string(),
            provider_endpoint: self_endpoint.to_string(),
            provider_id: self_provider_id.to_string(),
        };

        self.http_client
            .post(&url)
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .json(&req)
            .send()
            .await
            .context("Failed to notify peer about model")?
            .error_for_status()
            .context("Peer returned error for model notification")?;

        info!(
            model = %model_name,
            peer_endpoint = %peer_endpoint,
            "Notified peer about new model"
        );

        Ok(())
    }

    /// Proxy an inference request to another provider (load balancing).
    ///
    /// This is used when this provider is overloaded and needs to redirect
    /// a request to a less-loaded peer.
    pub async fn proxy_request(
        &self,
        target_endpoint: &str,
        request_body: serde_json::Value,
        timeout_secs: u32,
    ) -> Result<ProxyResponse> {
        let url = format!(
            "{}/v1/chat/completions",
            target_endpoint.trim_end_matches('/')
        );

        let start = std::time::Instant::now();

        let resp = self
            .http_client
            .post(&url)
            .timeout(Duration::from_secs(timeout_secs as u64))
            .json(&request_body)
            .send()
            .await
            .context("Failed to proxy request to peer")?
            .error_for_status()
            .context("Peer returned error for proxied request")?;

        let response_body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse proxied response")?;

        let latency_ms = start.elapsed().as_millis() as u64;

        info!(
            target = %target_endpoint,
            latency_ms,
            "Proxied request to peer"
        );

        Ok(ProxyResponse {
            response_body,
            target_endpoint: target_endpoint.to_string(),
            latency_ms,
        })
    }

    /// Find the least-loaded peer that has a specific model.
    /// Returns the peer info if found, or None.
    pub fn find_best_peer_for_model(&self, model: &str) -> Option<PeerAgentInfo> {
        let mut candidates: Vec<PeerAgentInfo> = self
            .peer_cache
            .iter()
            .filter(|entry| {
                let info = entry.value();
                info.is_healthy
                    && info.models.iter().any(|m| m.eq_ignore_ascii_case(model))
                    && info.load_factor < 0.9 // Don't redirect to overloaded peers
            })
            .map(|entry| entry.value().clone())
            .collect();

        candidates.sort_by(|a, b| {
            a.load_factor
                .partial_cmp(&b.load_factor)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates.into_iter().next()
    }

    /// Get all cached peer infos.
    pub fn get_cached_peers(&self) -> Vec<PeerAgentInfo> {
        self.peer_cache.iter().map(|r| r.value().clone()).collect()
    }

    /// Get the number of cached peers.
    pub fn cached_peer_count(&self) -> usize {
        self.peer_cache.len()
    }

    /// Run a peer refresh cycle:
    /// Re-exchange info with all known peers from the peer discovery system.
    pub async fn refresh_peers(&self, peer_endpoints: &[String]) {
        if !self.config.enabled {
            return;
        }

        let handles: Vec<_> = peer_endpoints
            .iter()
            .filter(|ep| !ep.is_empty())
            .map(|ep| {
                let engine = self.clone_inner();
                let ep = ep.clone();
                tokio::spawn(async move {
                    if let Err(e) = engine.exchange_peer_info(&ep).await {
                        debug!(
                            peer_endpoint = %ep,
                            error = %e,
                            "Peer info exchange failed (will retry next cycle)"
                        );
                    }
                })
            })
            .collect();

        for handle in handles {
            if let Err(e) = handle.await {
                debug!(error = %e, "Peer refresh task failed");
            }
        }
    }

    /// Clone the inner state for spawning tasks.
    fn clone_inner(&self) -> P2PEngineInner {
        P2PEngineInner {
            http_client: self.http_client.clone(),
            peer_cache: self.peer_cache.clone(),
            config: self.config.clone(),
        }
    }
}

/// Lightweight cloneable handle for spawned tasks.
#[derive(Clone)]
struct P2PEngineInner {
    http_client: Client,
    peer_cache: Arc<DashMap<String, PeerAgentInfo>>,
    config: P2PConfig,
}

impl P2PEngineInner {
    async fn exchange_peer_info(&self, peer_endpoint: &str) -> Result<PeerAgentInfo> {
        let url = format!(
            "{}/api/peer/info",
            peer_endpoint.trim_end_matches('/')
        );

        let resp = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .send()
            .await
            .context("Failed to reach peer for info exchange")?
            .error_for_status()
            .context("Peer returned error for info exchange")?;

        let peer_info: PeerAgentInfo = resp
            .json()
            .await
            .context("Failed to parse peer info response")?;

        if self.peer_cache.len() >= self.config.max_cached_peers {
            if let Some(oldest) = self.peer_cache.iter().next() {
                let key = oldest.key().clone();
                drop(oldest);
                self.peer_cache.remove(&key);
            }
        }

        self.peer_cache
            .insert(peer_info.endpoint.clone(), peer_info.clone());

        Ok(peer_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2p_config_defaults() {
        let config = P2PConfig::default();
        assert!(config.enabled);
        assert_eq!(config.peer_refresh_interval_secs, 120);
        assert_eq!(config.timeout_secs, 5);
        assert_eq!(config.max_cached_peers, 50);
    }

    #[test]
    fn test_p2p_config_deserialize() {
        let config: P2PConfig = serde_json::from_value(serde_json::json!({
            "enabled": false,
            "peer_refresh_interval_secs": 60,
            "timeout_secs": 10,
            "max_cached_peers": 100
        }))
        .expect("deserialization should succeed");

        assert!(!config.enabled);
        assert_eq!(config.peer_refresh_interval_secs, 60);
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.max_cached_peers, 100);
    }

    #[test]
    fn test_peer_agent_info_serialization() {
        let info = PeerAgentInfo {
            provider_id: "test-provider".to_string(),
            provider_name: "Test Node".to_string(),
            region: "us-east".to_string(),
            models: vec!["llama-3.1-8b".to_string()],
            endpoint: "http://127.0.0.1:9099".to_string(),
            load_factor: 0.5,
            pown_score: 42.0,
            is_healthy: true,
            total_tokens: 1000,
            total_requests: 50,
            timestamp: Utc::now().to_rfc3339(),
        };

        let json = serde_json::to_value(&info).expect("serialization should succeed");
        assert_eq!(json["provider_id"], "test-provider");
        assert_eq!(json["models"][0], "llama-3.1-8b");
        assert_eq!(json["load_factor"], 0.5);
    }
}
