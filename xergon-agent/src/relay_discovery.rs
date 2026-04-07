//! Multi-Relay Discovery
//!
//! Discovers relays via on-chain registry boxes and provides fallback to
//! configured relay URL. Supports health checking, latency-based selection,
//! and automatic failover when a relay goes down.
//!
//! Flow:
//! 1. Scan UTXO for relay registry boxes (via ErgoTree or token scan)
//! 2. Health-check each discovered relay (GET /health)
//! 3. Select best relay by latency
//! 4. If current relay fails, failover to next-best
//! 5. If no relays found on-chain, fall back to configured relay URL
//!
//! Config section: [relay]
//!   discovery_enabled = true              # Enable on-chain relay discovery
//!   registry_tree_hex = ""                # Hex of Relay Registry ErgoTree
//!   registry_nft_token_id = ""            # Token ID to scan for relay registry boxes

use anyhow::{Context, Result};
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::chain::client::ErgoNodeClient;

/// A discovered relay endpoint with health metadata
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredRelay {
    /// Relay base URL (e.g., "http://relay1.xergon.ai:9090")
    pub endpoint: String,
    /// Box ID on-chain where this relay is registered
    pub box_id: String,
    /// Measured round-trip latency in ms
    pub latency_ms: u64,
    /// Whether the relay is currently healthy
    pub is_healthy: bool,
    /// Last time we confirmed this relay is alive
    pub last_healthy_at: chrono::DateTime<chrono::Utc>,
    /// Region reported by the relay (if available)
    pub region: String,
    /// Number of consecutive health check failures
    pub consecutive_failures: u32,
    /// Whether this relay was discovered from chain (vs static config fallback)
    pub from_chain: bool,
}

/// Configuration for relay discovery
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelayDiscoveryConfig {
    /// Enable on-chain relay discovery (default: true)
    #[serde(default = "default_discovery_enabled")]
    pub discovery_enabled: bool,
    /// Hex-encoded ErgoTree for relay registry boxes
    #[serde(default)]
    pub registry_tree_hex: String,
    /// Token ID to scan for relay registry boxes (alternative to tree scan)
    #[serde(default)]
    pub registry_nft_token_id: String,
    /// How often to rescan the chain for relays (seconds, default: 300)
    #[serde(default = "default_scan_interval")]
    pub scan_interval_secs: u64,
    /// Timeout for relay health checks (seconds, default: 5)
    #[serde(default = "default_health_timeout")]
    pub health_timeout_secs: u64,
}

fn default_discovery_enabled() -> bool {
    true
}
fn default_scan_interval() -> u64 {
    300
}
fn default_health_timeout() -> u64 {
    5
}

impl Default for RelayDiscoveryConfig {
    fn default() -> Self {
        Self {
            discovery_enabled: default_discovery_enabled(),
            registry_tree_hex: String::new(),
            registry_nft_token_id: String::new(),
            scan_interval_secs: default_scan_interval(),
            health_timeout_secs: default_health_timeout(),
        }
    }
}

/// The relay discovery engine
pub struct RelayDiscovery {
    config: RelayDiscoveryConfig,
    http_client: Client,
    chain_client: ErgoNodeClient,
    /// All known relays (keyed by endpoint URL)
    known_relays: Arc<DashMap<String, DiscoveredRelay>>,
    /// Whether at least one on-chain scan has completed
    scanned: AtomicBool,
    /// Fallback relay URL from static config
    fallback_relay_url: String,
}

impl RelayDiscovery {
    /// Create a new relay discovery engine.
    ///
    /// `fallback_relay_url` is the relay URL from `[relay].relay_url` config,
    /// used when no relays are found on-chain.
    pub fn new(
        config: RelayDiscoveryConfig,
        ergo_node_url: String,
        fallback_relay_url: String,
    ) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.health_timeout_secs))
            .connect_timeout(Duration::from_secs(config.health_timeout_secs))
            .pool_max_idle_per_host(0)
            .build()
            .context("Failed to build HTTP client for relay discovery")?;

        Ok(Self {
            config,
            http_client,
            chain_client: ErgoNodeClient::new(ergo_node_url),
            known_relays: Arc::new(DashMap::new()),
            scanned: AtomicBool::new(false),
            fallback_relay_url,
        })
    }

    /// Run a single discovery cycle:
    /// 1. Scan chain for relay registry boxes
    /// 2. Health-check all known relays
    /// 3. Add fallback relay if no others are healthy
    pub async fn run_discovery_cycle(&self) -> Result<usize> {
        // Step 1: Scan chain for relays
        if self.config.discovery_enabled {
            self.scan_chain_relays().await;
        }

        // Step 2: Health-check all known relays
        self.health_check_all().await;

        // Step 3: Ensure fallback is available if no chain relays are healthy
        self.ensure_fallback();

        self.scanned.store(true, Ordering::Relaxed);

        let count = self.known_relays.len();
        let healthy = self.known_relays.iter().filter(|r| r.is_healthy).count();
        info!(
            total = count,
            healthy,
            from_chain = count,
            "Relay discovery cycle complete"
        );

        Ok(count)
    }

    /// Scan the Ergo blockchain for relay registry boxes.
    async fn scan_chain_relays(&self) {
        // Strategy 1: Scan by token ID (if configured)
        if !self.config.registry_nft_token_id.is_empty() {
            match self
                .chain_client
                .get_boxes_by_token_id(&self.config.registry_nft_token_id)
                .await
            {
                Ok(boxes) => {
                    for box_val in boxes {
                        self.parse_and_add_relay_box(&box_val);
                    }
                    return;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to scan boxes by token ID for relay registry");
                }
            }
        }

        // Strategy 2: Scan by ErgoTree (if configured)
        if !self.config.registry_tree_hex.is_empty() {
            match self
                .chain_client
                .get_boxes_by_ergo_tree(&self.config.registry_tree_hex)
                .await
            {
                Ok(boxes) => {
                    for box_val in boxes {
                        self.parse_and_add_relay_box(&box_val);
                    }
                    return;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to scan boxes by ErgoTree for relay registry");
                }
            }
        }

        debug!("No relay registry scan configured (registry_tree_hex and registry_nft_token_id both empty)");
    }

    /// Parse a raw box into a DiscoveredRelay and add it to the known list.
    fn parse_and_add_relay_box(&self, box_val: &crate::chain::types::RawBox) {
        // Extract endpoint from R5 register (string)
        let endpoint = match self.extract_register_string(box_val, "R5") {
            Some(e) => e,
            None => {
                debug!(
                    box_id = %box_val.box_id,
                    "Relay registry box missing R5 (endpoint) — skipping"
                );
                return;
            }
        };

        // Validate URL
        if reqwest::Url::parse(&endpoint).is_err() {
            warn!(
                box_id = %box_val.box_id,
                endpoint = %endpoint,
                "Invalid relay endpoint URL from chain — skipping"
            );
            return;
        }

        // Extract region from additional registers (optional, try R7)
        let region = self
            .extract_register_string(box_val, "R7")
            .unwrap_or_default();

        let ep = endpoint.trim_end_matches('/').to_string();

        self.known_relays
            .entry(ep.clone())
            .and_modify(|existing| {
                existing.box_id = box_val.box_id.clone();
                existing.from_chain = true;
                if !region.is_empty() {
                    existing.region = region.clone();
                }
            })
            .or_insert(DiscoveredRelay {
                endpoint: ep,
                box_id: box_val.box_id.clone(),
                latency_ms: 0,
                is_healthy: false,
                last_healthy_at: chrono::Utc::now(),
                region,
                consecutive_failures: 0,
                from_chain: true,
            });

        debug!(
            box_id = %box_val.box_id,
            endpoint = %endpoint,
            "Discovered relay from chain"
        );
    }

    /// Extract a string value from a box register.
    /// Handles both Sigma-serialized and plain string formats.
    fn extract_register_string(
        &self,
        box_val: &crate::chain::types::RawBox,
        register_key: &str,
    ) -> Option<String> {
        let register_value = box_val.additional_registers.get(register_key)?;

        // Could be a direct string, or an object with "value" field (Sigma-serialized)
        if register_value.is_string() {
            register_value.as_str().map(String::from)
        } else {
            register_value
                .get("value")
                .and_then(|inner| {
                    if inner.is_string() {
                        inner.as_str().map(String::from)
                    } else if inner.is_number() {
                        Some(inner.to_string())
                    } else {
                        None
                    }
                })
        }
    }

    /// Health-check all known relays concurrently.
    async fn health_check_all(&self) {
        let endpoints: Vec<String> = self
            .known_relays
            .iter()
            .map(|r| r.key().clone())
            .collect();

        let handles: Vec<_> = endpoints
            .into_iter()
            .map(|ep| {
                let client = self.http_client.clone();
                let relays = self.known_relays.clone();
                tokio::spawn(async move {
                    let url = format!("{}/health", ep.trim_end_matches('/'));
                    let start = std::time::Instant::now();

                    let result = client
                        .get(&url)
                        .timeout(Duration::from_secs(5))
                        .send()
                        .await;

                    let latency_ms = start.elapsed().as_millis() as u64;

                    if let Some(mut relay) = relays.get_mut(&ep) {
                        match result {
                            Ok(resp) if resp.status().is_success() => {
                                relay.is_healthy = true;
                                relay.latency_ms = latency_ms;
                                relay.last_healthy_at = chrono::Utc::now();
                                relay.consecutive_failures = 0;
                            }
                            _ => {
                                relay.is_healthy = false;
                                relay.consecutive_failures += 1;
                            }
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            if let Err(e) = handle.await {
                debug!(error = %e, "Relay health check task failed");
            }
        }
    }

    /// Ensure the fallback relay is available if no chain relays are healthy.
    fn ensure_fallback(&self) {
        let has_healthy = self
            .known_relays
            .iter()
            .any(|r| r.is_healthy);

        if !has_healthy && !self.fallback_relay_url.is_empty() {
            let ep = self.fallback_relay_url.trim_end_matches('/').to_string();
            self.known_relays
                .entry(ep.clone())
                .and_modify(|existing| {
                    existing.from_chain = false;
                })
                .or_insert(DiscoveredRelay {
                    endpoint: ep,
                    box_id: String::new(),
                    latency_ms: 0,
                    is_healthy: false, // Will be updated by health check
                    last_healthy_at: chrono::Utc::now(),
                    region: String::new(),
                    consecutive_failures: 0,
                    from_chain: false,
                });
        }
    }

    /// Get the best available relay, sorted by latency (lowest first).
    /// Returns None if no relays are known.
    pub fn best_relay(&self) -> Option<DiscoveredRelay> {
        let mut relays: Vec<DiscoveredRelay> = self
            .known_relays
            .iter()
            .filter(|r| r.is_healthy)
            .map(|r| r.value().clone())
            .collect();

        if relays.is_empty() {
            // Fall back to any known relay (even unhealthy) for auto-retry
            relays = self
                .known_relays
                .iter()
                .filter(|r| r.consecutive_failures < 5)
                .map(|r| r.value().clone())
                .collect();
        }

        relays.sort_by_key(|r| r.latency_ms);
        relays.into_iter().next()
    }

    /// Get the next relay to try, excluding already-tried endpoints.
    pub fn next_relay(&self, exclude: &[String]) -> Option<DiscoveredRelay> {
        let mut relays: Vec<DiscoveredRelay> = self
            .known_relays
            .iter()
            .filter(|r| {
                let ep = r.key();
                !exclude.iter().any(|e| e == ep) && r.consecutive_failures < 5
            })
            .map(|r| r.value().clone())
            .collect();

        relays.sort_by_key(|r| r.latency_ms);
        relays.into_iter().next()
    }

    /// Get all known relays (for display/debug).
    pub fn all_relays(&self) -> Vec<DiscoveredRelay> {
        self.known_relays.iter().map(|r| r.value().clone()).collect()
    }

    /// Check if at least one discovery scan has completed.
    pub fn has_scanned(&self) -> bool {
        self.scanned.load(Ordering::Relaxed)
    }

    /// Health-check a single relay endpoint (used for manual checks).
    pub async fn check_relay_health(&self, endpoint: &str) -> bool {
        let url = format!("{}/health", endpoint.trim_end_matches('/'));
        self.http_client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_discovery_config_defaults() {
        let config = RelayDiscoveryConfig::default();
        assert!(config.discovery_enabled);
        assert!(config.registry_tree_hex.is_empty());
        assert!(config.registry_nft_token_id.is_empty());
        assert_eq!(config.scan_interval_secs, 300);
        assert_eq!(config.health_timeout_secs, 5);
    }

    #[test]
    fn test_relay_discovery_config_deserialize() {
        let config: RelayDiscoveryConfig = serde_json::from_value(serde_json::json!({
            "discovery_enabled": false,
            "registry_tree_hex": "abcd1234",
            "registry_nft_token_id": "token123",
            "scan_interval_secs": 600,
            "health_timeout_secs": 10
        }))
        .expect("deserialization should succeed");

        assert!(!config.discovery_enabled);
        assert_eq!(config.registry_tree_hex, "abcd1234");
        assert_eq!(config.registry_nft_token_id, "token123");
        assert_eq!(config.scan_interval_secs, 600);
        assert_eq!(config.health_timeout_secs, 10);
    }
}
