//! Chain Adapters module.
//!
//! Multi-chain adapter abstraction for cross-chain oracle feeds and settlement.
//! Provides a unified interface for interacting with Ergo, Ethereum, Polygon,
//! Solana, and other chains. Manages cross-chain message passing, health
//! monitoring, and settlement confirmation tracking.
//!
//! Cross-chain messages include price updates, settlement proofs, and provider
//! attestations that need to be relayed between chains for Xergon's
//! multi-chain reputation system.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Top-level chain adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainAdapterConfig {
    /// Individual chain adapter definitions.
    pub adapters: Vec<ChainAdapterConfigItem>,
    /// Default chain for operations when none is specified.
    pub default_chain: String,
    /// Minimum value threshold for settlement messages.
    pub settlement_threshold: f64,
    /// Number of confirmations required before a message is considered confirmed.
    pub confirmation_blocks: u32,
}

impl Default for ChainAdapterConfig {
    fn default() -> Self {
        Self {
            adapters: Vec::new(),
            default_chain: "ergo".to_string(),
            settlement_threshold: 0.01,
            confirmation_blocks: 6,
        }
    }
}

/// Per-chain adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainAdapterConfigItem {
    /// Chain identifier (e.g. "ergo", "ethereum", "polygon", "solana").
    pub chain_id: String,
    /// Human-readable chain name.
    pub chain_name: String,
    /// Node RPC URL.
    pub node_url: String,
    /// Optional API key for node access.
    pub api_key: Option<String>,
    /// Whether this adapter is active.
    pub enabled: bool,
    /// Approximate block time in seconds.
    pub block_time_secs: u64,
    /// Native token symbol.
    pub native_token: String,
    /// Block explorer base URL.
    pub explorer_url: String,
}

impl Default for ChainAdapterConfigItem {
    fn default() -> Self {
        Self {
            chain_id: String::new(),
            chain_name: String::new(),
            node_url: String::new(),
            api_key: None,
            enabled: true,
            block_time_secs: 120,
            native_token: "UNKNOWN".to_string(),
            explorer_url: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Messages that can be relayed across chains.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CrossChainMessage {
    /// Price update from another chain.
    PriceUpdate {
        chain: String,
        pair: String,
        price: f64,
        signature: Vec<u8>,
    },
    /// Proof that a settlement transaction occurred.
    SettlementProof {
        chain: String,
        tx_hash: String,
        amount: u64,
    },
    /// Attestation of a provider's reputation score from another chain.
    ProviderAttestation {
        chain: String,
        provider_id: String,
        score: u64,
        #[serde(rename = "sig")]
        sig: Vec<u8>,
    },
}

impl CrossChainMessage {
    /// Get the chain this message originated from.
    pub fn source_chain(&self) -> &str {
        match self {
            CrossChainMessage::PriceUpdate { chain, .. } => chain,
            CrossChainMessage::SettlementProof { chain, .. } => chain,
            CrossChainMessage::ProviderAttestation { chain, .. } => chain,
        }
    }

    /// Unique identifier for deduplication.
    pub fn message_id(&self) -> String {
        match self {
            CrossChainMessage::PriceUpdate { chain, pair, price, .. } => {
                format!("price:{}:{}:{:.8}", chain, pair, price)
            }
            CrossChainMessage::SettlementProof { chain, tx_hash, .. } => {
                format!("settlement:{}:{}", chain, tx_hash)
            }
            CrossChainMessage::ProviderAttestation { chain, provider_id, score, .. } => {
                format!("attestation:{}:{}:{}", chain, provider_id, score)
            }
        }
    }
}

/// Health status for a single chain.
#[derive(Debug, Clone, Serialize)]
pub struct ChainHealth {
    pub chain_id: String,
    pub chain_name: String,
    pub is_syncing: bool,
    pub last_block_height: u64,
    pub last_block_time: DateTime<Utc>,
    pub peer_count: u32,
    pub is_healthy: bool,
}

/// Overall manager status.
#[derive(Debug, Clone, Serialize)]
pub struct ManagerStatus {
    pub is_healthy: bool,
    pub total_chains: usize,
    pub healthy_chains: usize,
    pub pending_messages: usize,
    pub confirmed_messages: usize,
    pub uptime_seconds: u64,
}

/// Result of a chain sync operation.
#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub chain_id: String,
    pub success: bool,
    pub synced_to_height: u64,
    pub messages_processed: usize,
    pub error: Option<String>,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Internal adapter state
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct AdapterState {
    config: ChainAdapterConfigItem,
    health: Arc<RwLock<ChainHealth>>,
    sync_height: AtomicU64,
    message_count: AtomicU64,
}

// ---------------------------------------------------------------------------
// ChainAdapterManager
// ---------------------------------------------------------------------------

/// Manager for multiple chain adapters, providing cross-chain message passing.
pub struct ChainAdapterManager {
    config: RwLock<ChainAdapterConfig>,
    adapters: DashMap<String, AdapterState>,
    cross_chain_state: RwLock<CrossChainState>,
    http_client: reqwest::Client,
    started_at: DateTime<Utc>,
}

/// Mutable cross-chain state (behind RwLock).
struct CrossChainState {
    last_sync_height: HashMap<String, u64>,
    pending_messages: VecDeque<CrossChainMessage>,
    confirmed_messages: Vec<CrossChainMessage>,
    chain_health: HashMap<String, ChainHealth>,
}

impl CrossChainState {
    fn new() -> Self {
        Self {
            last_sync_height: HashMap::new(),
            pending_messages: VecDeque::new(),
            confirmed_messages: Vec::new(),
            chain_health: HashMap::new(),
        }
    }
}

impl ChainAdapterManager {
    /// Create a new chain adapter manager with the given configuration.
    pub fn new(config: ChainAdapterConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(2)
            .build()
            .unwrap_or_default();

        let mgr = Self {
            adapters: DashMap::new(),
            config: RwLock::new(config),
            cross_chain_state: RwLock::new(CrossChainState::new()),
            http_client,
            started_at: Utc::now(),
        };

        // Register adapters from initial config (sync snapshot)
        let cfg_snapshot = mgr.config.try_read()
            .map(|c| c.adapters.clone())
            .unwrap_or_default();
        for adapter in cfg_snapshot {
            mgr.register_adapter_inner(adapter);
        }

        info!("ChainAdapterManager initialized");
        mgr
    }

    /// Register a new chain adapter.
    pub async fn register_adapter(&self, adapter_config: ChainAdapterConfigItem) {
        self.register_adapter_inner(adapter_config);
    }

    fn register_adapter_inner(&self, adapter_config: ChainAdapterConfigItem) {
        let chain_id = adapter_config.chain_id.clone();
        let chain_name = adapter_config.chain_name.clone();
        let now = Utc::now();

        let health = ChainHealth {
            chain_id: chain_id.clone(),
            chain_name: chain_name.clone(),
            is_syncing: false,
            last_block_height: 0,
            last_block_time: now,
            peer_count: 0,
            is_healthy: false,
        };

        self.adapters.insert(
            chain_id.clone(),
            AdapterState {
                config: adapter_config,
                health: Arc::new(RwLock::new(health)),
                sync_height: AtomicU64::new(0),
                message_count: AtomicU64::new(0),
            },
        );

        info!(chain = %chain_id, name = %chain_name, "Chain adapter registered");
    }

    /// Get adapter state for a specific chain.
    pub fn get_adapter(&self, chain_id: &str) -> Option<ChainAdapterConfigItem> {
        self.adapters.get(chain_id).map(|a| a.config.clone())
    }

    /// Synchronize a specific chain (fetch latest block info).
    pub async fn sync_chain(&self, chain_id: &str) -> SyncResult {
        let start = std::time::Instant::now();

        let adapter = match self.adapters.get(chain_id) {
            Some(a) => a,
            None => {
                return SyncResult {
                    chain_id: chain_id.to_string(),
                    success: false,
                    synced_to_height: 0,
                    messages_processed: 0,
                    error: Some(format!("Chain '{}' not found", chain_id)),
                    duration_ms: start.elapsed().as_millis() as u64,
                };
            }
        };

        let config = adapter.config.clone();
        let url = format!(
            "{}/blocks/height",
            config.node_url.trim_end_matches('/')
        );

        let mut req = self.http_client.get(&url).timeout(Duration::from_secs(10));
        if let Some(key) = &config.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                let height = body
                    .get("height")
                    .or_else(|| body.get("number"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                // Update adapter health
                {
                    let mut health = adapter.health.write().await;
                    health.last_block_height = height;
                    health.last_block_time = Utc::now();
                    health.is_syncing = false;
                    health.is_healthy = height > 0;
                }
                adapter.sync_height.store(height, Ordering::Relaxed);

                // Update cross-chain state
                {
                    let mut state = self.cross_chain_state.write().await;
                    state.last_sync_height.insert(chain_id.to_string(), height);
                    state.chain_health.insert(
                        chain_id.to_string(),
                        adapter.health.read().await.clone(),
                    );
                }

                let msgs_processed = 0; // Would process pending messages in real impl
                info!(chain = %chain_id, height, "Chain synced");

                SyncResult {
                    chain_id: chain_id.to_string(),
                    success: true,
                    synced_to_height: height,
                    messages_processed: msgs_processed,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                }
            }
            Ok(resp) => {
                let status = resp.status();
                warn!(chain = %chain_id, status = %status, "Chain sync failed");
                SyncResult {
                    chain_id: chain_id.to_string(),
                    success: false,
                    synced_to_height: 0,
                    messages_processed: 0,
                    error: Some(format!("HTTP {}", status)),
                    duration_ms: start.elapsed().as_millis() as u64,
                }
            }
            Err(e) => {
                warn!(chain = %chain_id, error = %e, "Chain sync error");
                SyncResult {
                    chain_id: chain_id.to_string(),
                    success: false,
                    synced_to_height: 0,
                    messages_processed: 0,
                    error: Some(e.to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                }
            }
        }
    }

    /// Synchronize all registered chains.
    pub async fn sync_all(&self) -> Vec<SyncResult> {
        let chain_ids: Vec<String> = self.adapters.iter().map(|a| a.key().clone()).collect();
        let mut results = Vec::new();
        for chain_id in chain_ids {
            results.push(self.sync_chain(&chain_id).await);
        }
        results
    }

    /// Send a cross-chain message (adds to pending queue).
    pub async fn send_message(&self, msg: CrossChainMessage) -> Result<String, String> {
        let msg_id = msg.message_id();

        // Check for duplicates
        {
            let state = self.cross_chain_state.read().await;
            for pending in &state.pending_messages {
                if pending.message_id() == msg_id {
                    return Err(format!("Duplicate message: {}", msg_id));
                }
            }
            for confirmed in &state.confirmed_messages {
                if confirmed.message_id() == msg_id {
                    return Err(format!("Already confirmed: {}", msg_id));
                }
            }
        }

        {
            let mut state = self.cross_chain_state.write().await;
            state.pending_messages.push_back(msg.clone());
        }

        // Update message count on source adapter
        let chain = msg.source_chain();
        if let Some(adapter) = self.adapters.get(chain) {
            adapter.message_count.fetch_add(1, Ordering::Relaxed);
        }

        info!(msg_id = %msg_id, chain = %msg.source_chain(), "Cross-chain message queued");
        Ok(msg_id)
    }

    /// Confirm a pending message (move from pending to confirmed).
    pub async fn confirm_message(&self, chain_id: &str, msg_id: &str) -> Result<(), String> {
        let mut state = self.cross_chain_state.write().await;

        let pos = state
            .pending_messages
            .iter()
            .position(|m| m.source_chain() == chain_id && m.message_id() == msg_id)
            .ok_or_else(|| format!("Pending message not found: {}", msg_id))?;

        let msg = state.pending_messages.remove(pos).unwrap();
        state.confirmed_messages.push(msg);

        // Keep confirmed list bounded
        if state.confirmed_messages.len() > 1000 {
            state.confirmed_messages.drain(..100);
        }

        info!(msg_id = %msg_id, chain = %chain_id, "Cross-chain message confirmed");
        Ok(())
    }

    /// Get health status for a specific chain.
    pub async fn get_chain_health(&self, chain_id: &str) -> Result<ChainHealth, String> {
        let health_lock = {
            let adapter = self.adapters
                .get(chain_id)
                .ok_or_else(|| format!("Chain '{}' not found", chain_id))?;
            adapter.health.clone()
        };
        let guard = health_lock.read().await;
        let health = guard.clone();
        drop(guard);
        Ok(health)
    }

    /// Get health for all chains.
    pub async fn get_all_health(&self) -> Vec<ChainHealth> {
        let mut result = Vec::new();
        for adapter in self.adapters.iter() {
            let health_lock = adapter.health.clone();
            let health = health_lock.read().await.clone();
            result.push(health);
        }
        result
    }

    /// Get overall manager status.
    pub async fn get_status(&self) -> ManagerStatus {
        let healths = self.get_all_health().await;
        let healthy = healths.iter().filter(|h| h.is_healthy).count();
        let state = self.cross_chain_state.read().await;

        ManagerStatus {
            is_healthy: healthy > 0,
            total_chains: healths.len(),
            healthy_chains: healthy,
            pending_messages: state.pending_messages.len(),
            confirmed_messages: state.confirmed_messages.len(),
            uptime_seconds: (Utc::now() - self.started_at).num_seconds().unsigned_abs(),
        }
    }

    /// Get pending cross-chain messages.
    pub async fn get_pending_messages(&self) -> Vec<CrossChainMessage> {
        let state = self.cross_chain_state.read().await;
        state.pending_messages.iter().cloned().collect()
    }

    /// Get confirmed cross-chain messages.
    pub async fn get_confirmed_messages(&self) -> Vec<CrossChainMessage> {
        let state = self.cross_chain_state.read().await;
        state.confirmed_messages.clone()
    }

    /// Update the manager configuration.
    pub async fn update_config(&self, new_config: ChainAdapterConfig) {
        for adapter in &new_config.adapters {
            self.register_adapter_inner(adapter.clone());
        }
        *self.config.write().await = new_config;
        info!("ChainAdapterManager config updated");
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

fn err(msg: &str, code: StatusCode) -> Response {
    (code, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn ok(val: serde_json::Value) -> Response {
    (StatusCode::OK, Json(val)).into_response()
}

/// GET /admin/chains/status — All chains status.
async fn status_handler(State(state): State<AppState>) -> Response {
    let status: ManagerStatus = state.chainAdapterManager.get_status().await;
    let healths: Vec<ChainHealth> = state.chainAdapterManager.get_all_health().await;
    ok(serde_json::json!({
        "status": status,
        "chains": healths,
    }))
}

/// GET /admin/chains/:chain_id/health — Single chain health.
async fn chain_health_handler(
    State(state): State<AppState>,
    Path(chain_id): Path<String>,
) -> Response {
    let result: Result<ChainHealth, String> = state.chainAdapterManager.get_chain_health(chain_id.as_str()).await;
    match result {
        Ok(health) => ok(serde_json::to_value(&health).unwrap_or_default()),
        Err(ref e) => err(e.as_str(), StatusCode::NOT_FOUND),
    }
}

/// POST /admin/chains/:chain_id/sync — Sync a specific chain.
async fn sync_handler(
    State(state): State<AppState>,
    Path(chain_id): Path<String>,
) -> Response {
    let result: SyncResult = state.chainAdapterManager.sync_chain(chain_id.as_str()).await;
    ok(serde_json::to_value(&result).unwrap_or_default())
}

/// GET /admin/chains/messages — Pending messages.
async fn messages_handler(State(state): State<AppState>) -> Response {
    let pending: Vec<CrossChainMessage> = state.chainAdapterManager.get_pending_messages().await;
    let confirmed: Vec<CrossChainMessage> = state.chainAdapterManager.get_confirmed_messages().await;
    ok(serde_json::json!({
        "pending": pending,
        "confirmed_count": confirmed.len(),
    }))
}

/// POST /admin/chains/messages/send — Send a cross-chain message.
async fn send_message_handler(
    State(state): State<AppState>,
    Json(body): Json<CrossChainMessage>,
) -> Response {
    let result: Result<String, String> = state.chainAdapterManager.send_message(body).await;
    match result {
        Ok(msg_id) => ok(serde_json::json!({
            "status": "queued",
            "message_id": msg_id,
        })),
        Err(ref e) => err(e.as_str(), StatusCode::BAD_REQUEST),
    }
}

/// GET /admin/chains/confirmations — Recent confirmations.
async fn confirmations_handler(State(state): State<AppState>) -> Response {
    let confirmed: Vec<CrossChainMessage> = state.chainAdapterManager.get_confirmed_messages().await;
    let recent: Vec<CrossChainMessage> = confirmed.into_iter().rev().take(50).collect();
    ok(serde_json::to_value(&recent).unwrap_or_default())
}

/// Build the chain adapters admin router.
pub fn build_chain_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/chains/status", get(status_handler))
        .route("/admin/chains/{chain_id}/health", get(chain_health_handler))
        .route("/admin/chains/{chain_id}/sync", post(sync_handler))
        .route("/admin/chains/messages", get(messages_handler))
        .route("/admin/chains/messages/send", post(send_message_handler))
        .route("/admin/chains/confirmations", get(confirmations_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers --

    fn make_ergo_adapter() -> ChainAdapterConfigItem {
        ChainAdapterConfigItem {
            chain_id: "ergo".to_string(),
            chain_name: "Ergo".to_string(),
            node_url: "https://ergo-node.example.com".to_string(),
            api_key: None,
            enabled: true,
            block_time_secs: 120,
            native_token: "ERG".to_string(),
            explorer_url: "https://explorer.ergoplatform.com".to_string(),
        }
    }

    fn make_ethereum_adapter() -> ChainAdapterConfigItem {
        ChainAdapterConfigItem {
            chain_id: "ethereum".to_string(),
            chain_name: "Ethereum".to_string(),
            node_url: "https://eth-node.example.com".to_string(),
            api_key: Some("test-key".to_string()),
            enabled: true,
            block_time_secs: 12,
            native_token: "ETH".to_string(),
            explorer_url: "https://etherscan.io".to_string(),
        }
    }

    fn make_polygon_adapter() -> ChainAdapterConfigItem {
        ChainAdapterConfigItem {
            chain_id: "polygon".to_string(),
            chain_name: "Polygon".to_string(),
            node_url: "https://polygon-node.example.com".to_string(),
            api_key: None,
            enabled: true,
            block_time_secs: 2,
            native_token: "MATIC".to_string(),
            explorer_url: "https://polygonscan.com".to_string(),
        }
    }

    fn default_manager_config() -> ChainAdapterConfig {
        ChainAdapterConfig {
            adapters: vec![make_ergo_adapter()],
            default_chain: "ergo".to_string(),
            settlement_threshold: 0.01,
            confirmation_blocks: 6,
        }
    }

    fn price_update_msg(chain: &str, pair: &str, price: f64) -> CrossChainMessage {
        CrossChainMessage::PriceUpdate {
            chain: chain.to_string(),
            pair: pair.to_string(),
            price,
            signature: vec![1, 2, 3],
        }
    }

    fn settlement_proof_msg(chain: &str, tx_hash: &str, amount: u64) -> CrossChainMessage {
        CrossChainMessage::SettlementProof {
            chain: chain.to_string(),
            tx_hash: tx_hash.to_string(),
            amount,
        }
    }

    fn provider_attestation_msg(chain: &str, provider_id: &str, score: u64) -> CrossChainMessage {
        CrossChainMessage::ProviderAttestation {
            chain: chain.to_string(),
            provider_id: provider_id.to_string(),
            score,
            sig: vec![4, 5, 6],
        }
    }

    // -- Config Defaults --

    #[test]
    fn test_default_chain_adapter_config() {
        let cfg = ChainAdapterConfig::default();
        assert_eq!(cfg.default_chain, "ergo");
        assert!(cfg.adapters.is_empty());
        assert!((cfg.settlement_threshold - 0.01).abs() < 1e-9);
        assert_eq!(cfg.confirmation_blocks, 6);
    }

    #[test]
    fn test_default_chain_adapter_config_item() {
        let item = ChainAdapterConfigItem::default();
        assert!(item.chain_id.is_empty());
        assert!(item.chain_name.is_empty());
        assert!(item.node_url.is_empty());
        assert!(item.api_key.is_none());
        assert!(item.enabled);
        assert_eq!(item.block_time_secs, 120);
        assert_eq!(item.native_token, "UNKNOWN");
        assert!(item.explorer_url.is_empty());
    }

    // -- Manager Construction & Adapter Registration --

    #[test]
    fn test_new_manager_registers_initial_adapters() {
        let cfg = default_manager_config();
        let mgr = ChainAdapterManager::new(cfg);
        assert_eq!(mgr.adapters.len(), 1);
        assert!(mgr.adapters.contains_key("ergo"));
    }

    #[tokio::test]
    async fn test_register_adapter() {
        let mgr = ChainAdapterManager::new(ChainAdapterConfig::default());
        assert_eq!(mgr.adapters.len(), 0);

        mgr.register_adapter(make_ergo_adapter()).await;
        assert_eq!(mgr.adapters.len(), 1);
        assert!(mgr.adapters.contains_key("ergo"));
    }

    #[tokio::test]
    async fn test_register_multiple_adapters() {
        let cfg = ChainAdapterConfig {
            adapters: vec![make_ergo_adapter(), make_ethereum_adapter(), make_polygon_adapter()],
            default_chain: "ergo".to_string(),
            settlement_threshold: 0.01,
            confirmation_blocks: 6,
        };
        let mgr = ChainAdapterManager::new(cfg);
        assert_eq!(mgr.adapters.len(), 3);
        assert!(mgr.adapters.contains_key("ergo"));
        assert!(mgr.adapters.contains_key("ethereum"));
        assert!(mgr.adapters.contains_key("polygon"));
    }

    #[test]
    fn test_get_adapter_found() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let adapter = mgr.get_adapter("ergo").unwrap();
        assert_eq!(adapter.chain_id, "ergo");
        assert_eq!(adapter.chain_name, "Ergo");
        assert_eq!(adapter.native_token, "ERG");
    }

    #[test]
    fn test_get_adapter_not_found() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let result = mgr.get_adapter("solana");
        assert!(result.is_none());
    }

    // -- Chain Health (initial) --

    #[tokio::test]
    async fn test_chain_health_initial_state() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let health = mgr.get_chain_health("ergo").await.unwrap();
        assert_eq!(health.chain_id, "ergo");
        assert_eq!(health.chain_name, "Ergo");
        assert!(!health.is_healthy); // No sync yet
        assert!(!health.is_syncing);
        assert_eq!(health.last_block_height, 0);
        assert_eq!(health.peer_count, 0);
    }

    #[tokio::test]
    async fn test_chain_health_not_found() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let result = mgr.get_chain_health("solana").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_all_health() {
        let cfg = ChainAdapterConfig {
            adapters: vec![make_ergo_adapter(), make_ethereum_adapter()],
            default_chain: "ergo".to_string(),
            settlement_threshold: 0.01,
            confirmation_blocks: 6,
        };
        let mgr = ChainAdapterManager::new(cfg);
        let healths = mgr.get_all_health().await;
        assert_eq!(healths.len(), 2);
    }

    // -- Manager Status --

    #[tokio::test]
    async fn test_manager_status_initial() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let status = mgr.get_status().await;
        assert!(!status.is_healthy); // No healthy chains yet
        assert_eq!(status.total_chains, 1);
        assert_eq!(status.healthy_chains, 0);
        assert_eq!(status.pending_messages, 0);
        assert_eq!(status.confirmed_messages, 0);
    }

    // -- Cross-Chain Messages --

    #[tokio::test]
    async fn test_send_price_update_message() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let msg = price_update_msg("ergo", "ERG/USD", 1.50);
        let msg_id = mgr.send_message(msg).await.unwrap();
        assert!(msg_id.contains("price:ergo:ERG/USD:1.50000000"));
    }

    #[tokio::test]
    async fn test_send_settlement_proof_message() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let msg = settlement_proof_msg("ergo", "abc123", 1000);
        let msg_id = mgr.send_message(msg).await.unwrap();
        assert_eq!(msg_id, "settlement:ergo:abc123");
    }

    #[tokio::test]
    async fn test_send_provider_attestation_message() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let msg = provider_attestation_msg("ergo", "provider-42", 850);
        let msg_id = mgr.send_message(msg).await.unwrap();
        assert_eq!(msg_id, "attestation:ergo:provider-42:850");
    }

    #[tokio::test]
    async fn test_duplicate_message_rejected() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let msg = price_update_msg("ergo", "ERG/USD", 1.50);
        mgr.send_message(msg.clone()).await.unwrap();
        let result = mgr.send_message(msg).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate"));
    }

    #[tokio::test]
    async fn test_get_pending_messages() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        assert!(mgr.get_pending_messages().await.is_empty());

        mgr.send_message(price_update_msg("ergo", "ERG/USD", 1.0)).await.unwrap();
        mgr.send_message(settlement_proof_msg("ergo", "tx1", 500)).await.unwrap();

        let pending = mgr.get_pending_messages().await;
        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn test_confirm_message() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let msg = price_update_msg("ergo", "ERG/USD", 2.0);
        let msg_id = mgr.send_message(msg).await.unwrap();

        let pending = mgr.get_pending_messages().await;
        assert_eq!(pending.len(), 1);

        mgr.confirm_message("ergo", &msg_id).await.unwrap();

        let pending = mgr.get_pending_messages().await;
        assert_eq!(pending.len(), 0);

        let confirmed = mgr.get_confirmed_messages().await;
        assert_eq!(confirmed.len(), 1);
    }

    #[tokio::test]
    async fn test_confirm_message_not_found() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let result = mgr.confirm_message("ergo", "nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_already_confirmed_message_rejected() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let msg = price_update_msg("ergo", "ERG/USD", 3.0);
        let msg_id = mgr.send_message(msg.clone()).await.unwrap();

        mgr.confirm_message("ergo", &msg_id).await.unwrap();

        // Sending the same message again should fail as "Already confirmed"
        let result = mgr.send_message(msg).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Already confirmed"));
    }

    // -- CrossChainMessage source_chain --

    #[test]
    fn test_source_chain_price_update() {
        let msg = price_update_msg("ethereum", "ETH/USD", 2000.0);
        assert_eq!(msg.source_chain(), "ethereum");
    }

    #[test]
    fn test_source_chain_settlement_proof() {
        let msg = settlement_proof_msg("polygon", "tx-abc", 500);
        assert_eq!(msg.source_chain(), "polygon");
    }

    #[test]
    fn test_source_chain_provider_attestation() {
        let msg = provider_attestation_msg("solana", "prov-1", 900);
        assert_eq!(msg.source_chain(), "solana");
    }

    // -- CrossChainMessage message_id uniqueness --

    #[test]
    fn test_message_id_uniqueness_across_types() {
        let price = price_update_msg("ergo", "ERG/USD", 1.0);
        let settlement = settlement_proof_msg("ergo", "tx1", 100);
        let attestation = provider_attestation_msg("ergo", "prov1", 500);

        // All three should have different message IDs
        assert_ne!(price.message_id(), settlement.message_id());
        assert_ne!(price.message_id(), attestation.message_id());
        assert_ne!(settlement.message_id(), attestation.message_id());
    }

    #[test]
    fn test_message_id_same_params_same_id() {
        let msg1 = price_update_msg("ergo", "ERG/USD", 1.5);
        let msg2 = price_update_msg("ergo", "ERG/USD", 1.5);
        assert_eq!(msg1.message_id(), msg2.message_id());
    }

    #[test]
    fn test_message_id_different_price_different_id() {
        let msg1 = price_update_msg("ergo", "ERG/USD", 1.5);
        let msg2 = price_update_msg("ergo", "ERG/USD", 2.0);
        assert_ne!(msg1.message_id(), msg2.message_id());
    }

    // -- Sync Chain (non-existent) --

    #[tokio::test]
    async fn test_sync_chain_not_found() {
        let mgr = ChainAdapterManager::new(default_manager_config());
        let result = mgr.sync_chain("solana").await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
        assert_eq!(result.synced_to_height, 0);
        assert_eq!(result.messages_processed, 0);
    }

    // -- Update Config --

    #[tokio::test]
    async fn test_update_config_syncs_adapters() {
        let mgr = ChainAdapterManager::new(ChainAdapterConfig::default());
        assert_eq!(mgr.adapters.len(), 0);

        let new_cfg = ChainAdapterConfig {
            adapters: vec![make_ethereum_adapter()],
            default_chain: "ethereum".to_string(),
            settlement_threshold: 0.05,
            confirmation_blocks: 12,
        };
        mgr.update_config(new_cfg).await;
        assert!(mgr.adapters.contains_key("ethereum"));
    }

    // -- Serialization Round-Trip --

    #[test]
    fn test_cross_chain_message_serde_price_update() {
        let msg = price_update_msg("ergo", "ERG/USD", 1.23);
        let json = serde_json::to_string(&msg).unwrap();
        let back: CrossChainMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg.message_id(), back.message_id());
    }

    #[test]
    fn test_cross_chain_message_serde_settlement() {
        let msg = settlement_proof_msg("ethereum", "0xabc", 999);
        let json = serde_json::to_string(&msg).unwrap();
        let back: CrossChainMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg.message_id(), back.message_id());
    }

    #[test]
    fn test_cross_chain_message_serde_attestation() {
        let msg = provider_attestation_msg("polygon", "prov-99", 750);
        let json = serde_json::to_string(&msg).unwrap();
        let back: CrossChainMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg.message_id(), back.message_id());
    }

    // -- SyncResult fields --

    #[test]
    fn test_sync_result_debug() {
        let result = SyncResult {
            chain_id: "test".to_string(),
            success: false,
            synced_to_height: 0,
            messages_processed: 0,
            error: Some("test error".to_string()),
            duration_ms: 42,
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("test"));
    }
}
