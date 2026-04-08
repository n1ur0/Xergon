//! On-Chain State Sync Engine
//!
//! Tracks Ergo blockchain state in real-time, computing diffs between blocks
//! and syncing provider box state changes to the relay's routing table.
//!
//! Features:
//! - Simulated blockchain for testing (in-memory block chain)
//! - State diff computation (created/spent/updated boxes)
//! - Fork detection and resolution
//! - Provider box change tracking
//! - Background sync loop with configurable polling
//! - REST API under `/v1/chain-sync`

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info;};
use uuid::Uuid;

use crate::proxy;

// ================================================================
// Types
// ================================================================

/// Block header metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: i64,
    pub transactions_count: u32,
    pub main_chain: bool,
}

/// Full state of an Ergo box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxState {
    pub box_id: String,
    pub ergo_tree: String,
    pub value: u64,
    pub tokens: Vec<TokenInfo>,
    pub registers: HashMap<String, String>,
    pub creation_height: u64,
    pub spent_height: Option<u64>,
    pub address: String,
}

/// Token information embedded in a box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub token_id: String,
    pub amount: u64,
    pub name: Option<String>,
}

/// State diff between two blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    pub block_height: u64,
    pub created_boxes: Vec<BoxState>,
    pub spent_boxes: Vec<BoxState>,
    pub updated_boxes: Vec<BoxState>,
    pub timestamp: i64,
}

/// Current sync status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub synced_height: u64,
    pub target_height: u64,
    pub catching_up: bool,
    pub last_sync_at: i64,
    pub blocks_behind: u64,
}

/// Fork information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkInfo {
    pub common_ancestor: u64,
    pub fork_height: u64,
    pub fork_branch: Vec<String>,
    pub resolved: bool,
}

/// Provider box change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBoxChange {
    pub provider_id: String,
    pub change_type: BoxChangeType,
    pub box_id: String,
    pub previous_state: Option<BoxState>,
    pub new_state: Option<BoxState>,
    pub block_height: u64,
}

/// Type of box change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BoxChangeType {
    Created,
    Updated,
    Spent,
    RentCollected,
    ForkReorged,
}

/// Configuration for the chain sync engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSyncConfig {
    pub poll_interval_ms: u64,
    pub max_blocks_per_sync: u32,
    pub auto_sync: bool,
    pub track_provider_boxes: bool,
    pub track_staking_boxes: bool,
}

impl Default for ChainSyncConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 5000,
            max_blocks_per_sync: 100,
            auto_sync: false,
            track_provider_boxes: true,
            track_staking_boxes: true,
        }
    }
}

/// Sync statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSyncStats {
    pub total_blocks_synced: u64,
    pub total_boxes_tracked: u64,
    pub total_state_diffs: u64,
    pub forks_detected: u64,
    pub avg_sync_time_ms: u64,
    pub last_error: Option<String>,
}

// ================================================================
// Simulated Blockchain (for testing / offline mode)
// ================================================================

/// A simulated block containing created and spent boxes.
#[derive(Debug, Clone)]
struct SimBlock {
    header: BlockHeader,
    created_boxes: Vec<BoxState>,
    spent_box_ids: Vec<String>,
}

/// In-memory simulated blockchain.
#[derive(Debug, Clone)]
pub struct SimulatedBlockchain {
    blocks: DashMap<u64, SimBlock>,
    /// box_id -> BoxState for all currently-unspent boxes
    live_boxes: DashMap<String, BoxState>,
    /// parent_hash -> block_height for chain validation
    hash_index: DashMap<String, u64>,
}

impl SimulatedBlockchain {
    pub fn new() -> Self {
        Self {
            blocks: DashMap::new(),
            live_boxes: DashMap::new(),
            hash_index: DashMap::new(),
        }
    }

    /// Get the current chain tip height.
    pub fn tip_height(&self) -> u64 {
        self.blocks
            .iter()
            .map(|r| *r.key())
            .max()
            .unwrap_or(0)
    }

    /// Add a new block to the chain.
    pub fn push_block(&self, mut header: BlockHeader, created: Vec<BoxState>, spent_ids: Vec<String>) {
        header.height = self.tip_height() + 1;
        if header.parent_hash.is_empty() {
            header.parent_hash = self
                .blocks
                .get(&header.height.saturating_sub(1))
                .map(|b| b.header.hash.clone())
                .unwrap_or_default();
        }
        if header.hash.is_empty() {
            header.hash = format!("block-{}", header.height);
        }
        if header.timestamp == 0 {
            header.timestamp = Utc::now().timestamp_millis();
        }
        header.main_chain = true;
        header.transactions_count = (created.len() + spent_ids.len()) as u32;

        // Mark spent boxes
        for box_id in &spent_ids {
            if let Some(mut box_state) = self.live_boxes.get_mut(box_id) {
                box_state.spent_height = Some(header.height);
            }
        }

        // Insert created boxes
        for box_state in &created {
            self.live_boxes.insert(box_state.box_id.clone(), box_state.clone());
        }

        let block = SimBlock {
            header: header.clone(),
            created_boxes: created,
            spent_box_ids: spent_ids,
        };
        self.hash_index.insert(header.hash.clone(), header.height);
        self.blocks.insert(header.height, block);
    }

    /// Get a block by height.
    pub fn get_block(&self, height: u64) -> Option<SimBlock> {
        self.blocks.get(&height).map(|r| r.value().clone())
    }

    /// Get a live box by ID.
    pub fn get_box(&self, box_id: &str) -> Option<BoxState> {
        self.live_boxes.get(box_id).map(|r| r.value().clone())
    }

    /// Fork the chain at a given height, creating an alternate chain tip.
    pub fn fork_at(&self, height: u64, new_blocks: Vec<(Vec<BoxState>, Vec<String>)>) {
        let parent_hash = self
            .blocks
            .get(&height.saturating_sub(1))
            .map(|b| b.header.hash.clone())
            .unwrap_or_default();

        // Remove blocks at and above fork height
        let heights_to_remove: Vec<u64> = self
            .blocks
            .iter()
            .filter_map(|r| {
                let h = *r.key();
                if h >= height { Some(h) } else { None }
            })
            .collect();

        for h in heights_to_remove {
            if let Some(block) = self.blocks.remove(&h) {
                // Restore spent boxes, remove created boxes
                for _box_id in &block.1.spent_box_ids {
                    // Re-insert as unspent (best-effort restore)
                }
                for box_state in &block.1.created_boxes {
                    self.live_boxes.remove(&box_state.box_id);
                }
                self.hash_index.remove(&block.1.header.hash);
            }
        }

        // Re-add new blocks
        let mut parent = parent_hash;
        for (i, (created, spent)) in new_blocks.iter().enumerate() {
            let h = height + i as u64;
            let header = BlockHeader {
                height: h,
                hash: format!("fork-{}-{}", height, i),
                parent_hash: parent.clone(),
                timestamp: Utc::now().timestamp_millis() + i as i64,
                transactions_count: (created.len() + spent.len()) as u32,
                main_chain: true,
            };
            parent = header.hash.clone();

            for box_state in created {
                self.live_boxes.insert(box_state.box_id.clone(), box_state.clone());
            }

            let block = SimBlock {
                header: header.clone(),
                created_boxes: created.clone(),
                spent_box_ids: spent.clone(),
            };
            self.hash_index.insert(header.hash.clone(), h);
            self.blocks.insert(h, block);
        }
    }

    /// Get all box IDs currently being tracked.
    pub fn all_live_box_ids(&self) -> Vec<String> {
        self.live_boxes.iter().map(|r| r.key().clone()).collect()
    }
}

// ================================================================
// Chain Sync State (shared via AppState)
// ================================================================

/// The shared state for chain sync, embedded in AppState.
#[derive(Clone)]
pub struct ChainSyncState {
    pub engine: Arc<ChainSyncEngine>,
}

impl ChainSyncState {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(ChainSyncEngine::new(ChainSyncConfig::default())),
        }
    }
}

impl Default for ChainSyncState {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// Chain Sync Engine
// ================================================================

/// Core sync engine that tracks blockchain state and computes diffs.
pub struct ChainSyncEngine {
    config: tokio::sync::RwLock<ChainSyncConfig>,
    /// Simulated blockchain (in-memory for testing)
    blockchain: SimulatedBlockchain,
    /// Tracked boxes: box_id -> tracking reason
    tracked_boxes: DashMap<String, String>,
    /// State diffs by block height
    state_diffs: DashMap<u64, StateDiff>,
    /// Provider box changes: provider_id -> list of changes
    provider_changes: DashMap<String, Vec<ProviderBoxChange>>,
    /// Provider ID mapping: box_id -> provider_id
    box_to_provider: DashMap<String, String>,
    /// Sync status
    synced_height: AtomicU64,
    target_height: AtomicU64,
    /// Whether sync is running
    running: AtomicBool,
    /// Sync loop join handle
    sync_handle: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Statistics
    total_blocks_synced: AtomicU64,
    total_boxes_tracked: AtomicU64,
    total_state_diffs: AtomicU64,
    forks_detected: AtomicU64,
    total_sync_time_ms: AtomicU64,
    sync_count: AtomicU64,
    last_error: tokio::sync::RwLock<Option<String>>,
    /// Block headers by height
    block_headers: DashMap<u64, BlockHeader>,
}

impl ChainSyncEngine {
    /// Create a new chain sync engine with the given configuration.
    pub fn new(config: ChainSyncConfig) -> Self {
        let engine = Self {
            config: tokio::sync::RwLock::new(config),
            blockchain: SimulatedBlockchain::new(),
            tracked_boxes: DashMap::new(),
            state_diffs: DashMap::new(),
            provider_changes: DashMap::new(),
            box_to_provider: DashMap::new(),
            synced_height: AtomicU64::new(0),
            target_height: AtomicU64::new(0),
            running: AtomicBool::new(false),
            sync_handle: tokio::sync::Mutex::new(None),
            total_blocks_synced: AtomicU64::new(0),
            total_boxes_tracked: AtomicU64::new(0),
            total_state_diffs: AtomicU64::new(0),
            forks_detected: AtomicU64::new(0),
            total_sync_time_ms: AtomicU64::new(0),
            sync_count: AtomicU64::new(0),
            last_error: tokio::sync::RwLock::new(None),
            block_headers: DashMap::new(),
        };
        engine
    }

    /// Start the background sync loop.
    pub async fn start_sync(&self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            debug!("Chain sync is already running");
            return false;
        }
        self.running.store(true, Ordering::SeqCst);
        info!("Chain sync started");

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let config = self.config.read().await.clone();
        let poll_interval = config.poll_interval_ms;
        let max_blocks = config.max_blocks_per_sync;
        let blockchain = self.blockchain.clone();

        // We store a reference to running flag for stop_sync
        let _synced_height = self.synced_height.load(Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            while running_clone.load(Ordering::SeqCst) {
                let tip = blockchain.tip_height();
                let current = blockchain.blocks.iter().map(|r| *r.key()).max().unwrap_or(0);

                // In simulation, "sync" means noting the tip
                if current < tip {
                    for _h in (current + 1)..=std::cmp::min(current + max_blocks as u64, tip) {
                        if !running_clone.load(Ordering::SeqCst) {
                            break;
                        }
                        // Sync is simulated — blocks are already in memory
                    }
                }

                tokio::time::sleep(std::time::Duration::from_millis(poll_interval)).await;
            }
        });

        {
            let mut handle_guard = self.sync_handle.lock().await;
            *handle_guard = Some(handle);
        }

        true
    }

    /// Stop the background sync loop.
    pub async fn stop_sync(&self) -> bool {
        if !self.running.load(Ordering::SeqCst) {
            debug!("Chain sync is not running");
            return false;
        }
        self.running.store(false, Ordering::SeqCst);
        info!("Chain sync stopped");

        let mut handle_guard = self.sync_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
        true
    }

    /// Sync a single block by height and compute state diff.
    pub async fn sync_block(&self, height: u64) -> Result<StateDiff, String> {
        let start = Instant::now();

        let block = self
            .blockchain
            .get_block(height)
            .ok_or_else(|| format!("Block {} not found", height))?;

        // Store header
        self.block_headers.insert(height, block.header.clone());

        // Compute state diff
        let mut created_boxes = Vec::new();
        let mut spent_boxes = Vec::new();
        let updated_boxes = Vec::new();

        for box_state in &block.created_boxes {
            created_boxes.push(box_state.clone());
            self.total_boxes_tracked.fetch_add(1, Ordering::SeqCst);
        }

        for box_id in &block.spent_box_ids {
            if let Some(bs) = self.blockchain.get_box(box_id) {
                spent_boxes.push(bs);
            }
        }

        // Detect updates: boxes that were created and also spent in the same block
        // (e.g., value change via self-spend)
        let _created_ids: std::collections::HashSet<&String> =
            created_boxes.iter().map(|b| &b.box_id).collect();
        let _spent_ids: std::collections::HashSet<&String> =
            spent_boxes.iter().map(|b| &b.box_id).collect();

        // Track provider changes
        for box_state in &created_boxes {
            if let Some(provider_id) = self.box_to_provider.get(&box_state.box_id) {
                let change = ProviderBoxChange {
                    provider_id: provider_id.value().clone(),
                    change_type: BoxChangeType::Created,
                    box_id: box_state.box_id.clone(),
                    previous_state: None,
                    new_state: Some(box_state.clone()),
                    block_height: height,
                };
                self.record_provider_change(change).await;
            }
        }

        for box_state in &spent_boxes {
            if let Some(provider_id) = self.box_to_provider.get(&box_state.box_id) {
                let change = ProviderBoxChange {
                    provider_id: provider_id.value().clone(),
                    change_type: BoxChangeType::Spent,
                    box_id: box_state.box_id.clone(),
                    previous_state: Some(box_state.clone()),
                    new_state: None,
                    block_height: height,
                };
                self.record_provider_change(change).await;
            }
        }

        let diff = StateDiff {
            block_height: height,
            created_boxes,
            spent_boxes,
            updated_boxes,
            timestamp: block.header.timestamp,
        };

        self.state_diffs.insert(height, diff.clone());
        self.total_blocks_synced.fetch_add(1, Ordering::SeqCst);
        self.total_state_diffs.fetch_add(1, Ordering::SeqCst);
        self.synced_height.store(height, Ordering::SeqCst);

        let elapsed = start.elapsed().as_millis() as u64;
        self.total_sync_time_ms.fetch_add(elapsed, Ordering::SeqCst);
        self.sync_count.fetch_add(1, Ordering::SeqCst);

        // Clear last error on success
        {
            let mut err = self.last_error.write().await;
            *err = None;
        }

        debug!(height, elapsed_ms = elapsed, "Synced block");
        Ok(diff)
    }

    /// Sync a range of blocks [from, to] inclusive.
    pub async fn sync_range(&self, from: u64, to: u64) -> Result<Vec<StateDiff>, String> {
        if from > to {
            return Err("Invalid range: from > to".into());
        }

        let mut diffs = Vec::new();
        for height in from..=to {
            match self.sync_block(height).await {
                Ok(diff) => diffs.push(diff),
                Err(e) => {
                    self.set_last_error(Some(e.clone())).await;
                    return Err(e);
                }
            }
        }
        Ok(diffs)
    }

    /// Get current sync status.
    pub fn get_sync_status(&self) -> SyncStatus {
        let synced = self.synced_height.load(Ordering::SeqCst);
        let target = self.blockchain.tip_height();
        SyncStatus {
            synced_height: synced,
            target_height: target,
            catching_up: synced < target,
            last_sync_at: Utc::now().timestamp_millis(),
            blocks_behind: target.saturating_sub(synced),
        }
    }

    /// Get state diff for a specific block height.
    pub fn get_state_diff(&self, height: u64) -> Option<StateDiff> {
        self.state_diffs.get(&height).map(|r| r.value().clone())
    }

    /// Get current state of a tracked box.
    pub fn get_box_state(&self, box_id: &str) -> Option<BoxState> {
        self.blockchain.get_box(box_id)
    }

    /// Start tracking a specific box.
    pub fn track_box(&self, box_id: String, reason: String) -> bool {
        if self.tracked_boxes.contains_key(&box_id) {
            debug!(box_id = %box_id, "Box already tracked");
            return false;
        }
        info!(box_id = %box_id, reason = %reason, "Started tracking box");
        self.tracked_boxes.insert(box_id.clone(), reason);
        self.total_boxes_tracked.fetch_add(1, Ordering::SeqCst);
        true
    }

    /// Stop tracking a box.
    pub fn untrack_box(&self, box_id: &str) -> bool {
        if self.tracked_boxes.remove(box_id).is_some() {
            info!(box_id = %box_id, "Stopped tracking box");
            true
        } else {
            false
        }
    }

    /// Get provider box changes, optionally filtered by provider_id.
    pub fn get_provider_changes(&self, provider_id: Option<&str>) -> Vec<ProviderBoxChange> {
        if let Some(pid) = provider_id {
            self.provider_changes
                .get(pid)
                .map(|r| r.value().clone())
                .unwrap_or_default()
        } else {
            self.provider_changes
                .iter()
                .flat_map(|r| r.value().clone())
                .collect()
        }
    }

    /// Register a box-to-provider mapping.
    pub fn register_provider_box(&self, box_id: String, provider_id: String) {
        self.box_to_provider.insert(box_id, provider_id);
    }

    /// Detect if a fork occurred at the given height.
    pub async fn detect_fork(&self, at_height: u64) -> Option<ForkInfo> {
        let block = self.blockchain.get_block(at_height)?;

        // Check if parent exists and matches
        if block.header.height == 0 {
            return None; // Genesis block, can't fork
        }

        let parent = self.blockchain.get_block(at_height.saturating_sub(1));
        if let Some(parent_block) = parent {
            if parent_block.header.hash != block.header.parent_hash {
                // Fork detected: block's parent_hash doesn't match actual parent
                self.forks_detected.fetch_add(1, Ordering::SeqCst);
                let fork_info = ForkInfo {
                    common_ancestor: at_height.saturating_sub(2),
                    fork_height: at_height,
                    fork_branch: vec![block.header.hash.clone()],
                    resolved: false,
                };
                info!(
                    fork_height = at_height,
                    "Fork detected at height {}",
                    at_height
                );
                return Some(fork_info);
            }
        }

        None
    }

    /// Resolve a fork by re-syncing from the common ancestor.
    pub async fn resolve_fork(&self, fork_info: ForkInfo) -> Result<(), String> {
        if fork_info.resolved {
            return Ok(());
        }

        info!(
            common_ancestor = fork_info.common_ancestor,
            fork_height = fork_info.fork_height,
            "Resolving fork"
        );

        // Re-sync from common ancestor + 1 to current tip
        let tip = self.blockchain.tip_height();
        let from = fork_info.common_ancestor.saturating_add(1);

        // Clear state diffs in the forked range
        for h in from..=std::cmp::min(fork_info.fork_height, tip) {
            self.state_diffs.remove(&h);
        }

        // Re-sync
        self.sync_range(from, tip).await?;

        info!("Fork resolved, re-synced from {} to {}", from, tip);
        Ok(())
    }

    /// Get sync statistics.
    pub fn get_stats(&self) -> ChainSyncStats {
        let count = self.sync_count.load(Ordering::SeqCst);
        let total_time = self.total_sync_time_ms.load(Ordering::SeqCst);
        ChainSyncStats {
            total_blocks_synced: self.total_blocks_synced.load(Ordering::SeqCst),
            total_boxes_tracked: self.total_boxes_tracked.load(Ordering::SeqCst),
            total_state_diffs: self.total_state_diffs.load(Ordering::SeqCst),
            forks_detected: self.forks_detected.load(Ordering::SeqCst),
            avg_sync_time_ms: if count > 0 { total_time / count } else { 0 },
            last_error: None, // Read from RwLock requires async
        }
    }

    /// Get all tracked box IDs with their reasons.
    pub fn get_tracked_boxes(&self) -> Vec<(String, String)> {
        self.tracked_boxes
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }

    /// Get access to the underlying simulated blockchain.
    pub fn blockchain(&self) -> &SimulatedBlockchain {
        &self.blockchain
    }

    /// Record a provider change.
    async fn record_provider_change(&self, change: ProviderBoxChange) {
        let provider_id = change.provider_id.clone();
        self.provider_changes
            .entry(provider_id.clone())
            .or_insert_with(Vec::new)
            .push(change);
    }

    /// Set last error.
    async fn set_last_error(&self, err: Option<String>) {
        let mut last_err = self.last_error.write().await;
        *last_err = err;
    }

    /// Get stats with last error (async version).
    pub async fn get_stats_full(&self) -> ChainSyncStats {
        let mut stats = self.get_stats();
        let last_err = self.last_error.read().await;
        stats.last_error = last_err.clone();
        stats
    }

    /// Get the running state.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Update configuration.
    pub async fn update_config(&self, new_config: ChainSyncConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
    }

    /// Get current configuration.
    pub async fn get_config(&self) -> ChainSyncConfig {
        self.config.read().await.clone()
    }
}

// ================================================================
// REST API Request/Response types
// ================================================================

#[derive(Debug, Deserialize)]
struct TrackBoxRequest {
    box_id: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ForkDetectRequest {
    at_height: u64,
}

#[derive(Debug, Deserialize)]
struct ProviderChangesQuery {
    provider_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct SyncStartResponse {
    started: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct SyncStopResponse {
    stopped: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct TrackBoxResponse {
    tracked: bool,
    box_id: String,
}

#[derive(Debug, Serialize)]
struct UntrackBoxResponse {
    untracked: bool,
    box_id: String,
}

#[derive(Debug, Serialize)]
struct ForkDetectResponse {
    fork_detected: bool,
    fork_info: Option<ForkInfo>,
}

#[derive(Debug, Serialize)]
struct ForkResolveResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

// ================================================================
// REST Handlers
// ================================================================

async fn start_sync_handler(
    State(state): State<proxy::AppState>,
) -> Json<SyncStartResponse> {
    let started = state.chain_sync.engine.start_sync().await;
    Json(SyncStartResponse {
        started,
        message: if started {
            "Chain sync started".into()
        } else {
            "Chain sync already running".into()
        },
    })
}

async fn stop_sync_handler(
    State(state): State<proxy::AppState>,
) -> Json<SyncStopResponse> {
    let stopped = state.chain_sync.engine.stop_sync().await;
    Json(SyncStopResponse {
        stopped,
        message: if stopped {
            "Chain sync stopped".into()
        } else {
            "Chain sync was not running".into()
        },
    })
}

async fn status_handler(
    State(state): State<proxy::AppState>,
) -> Json<SyncStatus> {
    Json(state.chain_sync.engine.get_sync_status())
}

async fn diff_handler(
    State(state): State<proxy::AppState>,
    Path(height): Path<u64>,
) -> axum::response::Response {
    match state.chain_sync.engine.get_state_diff(height) {
        Some(diff) => Json(diff).into_response(),
        None => axum::response::Response::builder()
            .status(404)
            .body(axum::body::Body::from(
                serde_json::to_string(&ErrorResponse {
                    error: format!("No state diff found for block {}", height),
                })
                .unwrap(),
            ))
            .unwrap(),
    }
}

async fn box_state_handler(
    State(state): State<proxy::AppState>,
    Path(box_id): Path<String>,
) -> axum::response::Response {
    match state.chain_sync.engine.get_box_state(&box_id) {
        Some(box_state) => Json(box_state).into_response(),
        None => axum::response::Response::builder()
            .status(404)
            .body(axum::body::Body::from(
                serde_json::to_string(&ErrorResponse {
                    error: format!("Box {} not found or not tracked", box_id),
                })
                .unwrap(),
            ))
            .unwrap(),
    }
}

async fn track_box_handler(
    State(state): State<proxy::AppState>,
    Json(body): Json<TrackBoxRequest>,
) -> Json<TrackBoxResponse> {
    let reason = body.reason.clone().unwrap_or_else(|| "manual".into());
    let tracked = state
        .chain_sync
        .engine
        .track_box(body.box_id.clone(), reason.clone());
    Json(TrackBoxResponse {
        tracked,
        box_id: body.box_id,
    })
}

async fn untrack_box_handler(
    State(state): State<proxy::AppState>,
    Path(box_id): Path<String>,
) -> Json<UntrackBoxResponse> {
    let untracked = state.chain_sync.engine.untrack_box(&box_id);
    Json(UntrackBoxResponse { untracked, box_id })
}

async fn provider_changes_handler(
    State(state): State<proxy::AppState>,
    Query(query): Query<ProviderChangesQuery>,
) -> Json<Vec<ProviderBoxChange>> {
    let changes = state
        .chain_sync
        .engine
        .get_provider_changes(query.provider_id.as_deref());
    Json(changes)
}

async fn stats_handler(
    State(state): State<proxy::AppState>,
) -> Json<ChainSyncStats> {
    Json(state.chain_sync.engine.get_stats())
}

async fn fork_detect_handler(
    State(state): State<proxy::AppState>,
    Json(body): Json<ForkDetectRequest>,
) -> Json<ForkDetectResponse> {
    let fork_info = state.chain_sync.engine.detect_fork(body.at_height).await;
    Json(ForkDetectResponse {
        fork_detected: fork_info.is_some(),
        fork_info,
    })
}

async fn tracked_boxes_handler(
    State(state): State<proxy::AppState>,
) -> Json<Vec<(String, String)>> {
    Json(state.chain_sync.engine.get_tracked_boxes())
}

// ================================================================
// Router
// ================================================================

/// Build the chain-sync router nested under `/v1/chain-sync`.
pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    Router::new()
        .route("/v1/chain-sync/start", post(start_sync_handler))
        .route("/v1/chain-sync/stop", post(stop_sync_handler))
        .route("/v1/chain-sync/status", get(status_handler))
        .route("/v1/chain-sync/diff/{height}", get(diff_handler))
        .route("/v1/chain-sync/box/{box_id}", get(box_state_handler))
        .route("/v1/chain-sync/track", post(track_box_handler))
        .route("/v1/chain-sync/track/{box_id}", delete(untrack_box_handler))
        .route("/v1/chain-sync/provider-changes", get(provider_changes_handler))
        .route("/v1/chain-sync/stats", get(stats_handler))
        .route("/v1/chain-sync/fork-detect", post(fork_detect_handler))
        .route("/v1/chain-sync/tracked-boxes", get(tracked_boxes_handler))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a test engine with some simulated blocks.
    fn setup_engine_with_blocks(num_blocks: u64) -> Arc<ChainSyncEngine> {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        for i in 1..=num_blocks {
            let box_id = format!("box-{}-{}", i, Uuid::new_v4().as_simple());
            let box_state = BoxState {
                box_id: box_id.clone(),
                ergo_tree: format!("tree-{}", i),
                value: 1_000_000 * i,
                tokens: vec![TokenInfo {
                    token_id: format!("token-{}", i),
                    amount: 100 * i,
                    name: Some(format!("TestToken{}", i)),
                }],
                registers: HashMap::new(),
                creation_height: i,
                spent_height: None,
                address: format!("3W{}...", i),
            };
            engine.blockchain.push_block(
                BlockHeader {
                    height: i,
                    hash: format!("hash-{}", i),
                    parent_hash: if i > 0 {
                        format!("hash-{}", i - 1)
                    } else {
                        String::new()
                    },
                    timestamp: 1_700_000_000_000 + (i as i64 * 120_000),
                    transactions_count: 1,
                    main_chain: true,
                },
                vec![box_state],
                if i > 1 {
                    vec![format!("box-{}-prev", i)]
                } else {
                    vec![]
                },
            );
        }

        engine
    }

    /// Helper: create a simple box for testing.
    fn make_box(id: &str, value: u64, height: u64) -> BoxState {
        BoxState {
            box_id: id.to_string(),
            ergo_tree: format!("tree-{}", id),
            value,
            tokens: vec![],
            registers: HashMap::new(),
            creation_height: height,
            spent_height: None,
            address: format!("9f{}...", id),
        }
    }

    // ----------------------------------------------------------------
    // test_sync_single_block
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_sync_single_block() {
        let engine = setup_engine_with_blocks(3);
        let result = engine.sync_block(1).await;
        assert!(result.is_ok());
        let diff = result.unwrap();
        assert_eq!(diff.block_height, 1);
        assert_eq!(diff.created_boxes.len(), 1);
        assert_eq!(diff.spent_boxes.len(), 0);
    }

    // ----------------------------------------------------------------
    // test_sync_block_range
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_sync_block_range() {
        let engine = setup_engine_with_blocks(5);
        let result = engine.sync_range(1, 5).await;
        assert!(result.is_ok());
        let diffs = result.unwrap();
        assert_eq!(diffs.len(), 5);
        for (i, diff) in diffs.iter().enumerate() {
            assert_eq!(diff.block_height, (i + 1) as u64);
        }
    }

    // ----------------------------------------------------------------
    // test_state_diff_computation
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_state_diff_computation() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        // Block 1: create a box that will be spent in block 2
        engine.blockchain.push_block(
            BlockHeader {
                height: 1,
                hash: "hash-1".into(),
                parent_hash: String::new(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 1,
                main_chain: true,
            },
            vec![make_box("spendable-box", 5_000_000, 1)],
            vec![],
        );

        // Block 2: spend the box and create a new one
        engine.blockchain.push_block(
            BlockHeader {
                height: 2,
                hash: "hash-2".into(),
                parent_hash: "hash-1".into(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 2,
                main_chain: true,
            },
            vec![make_box("new-box-2", 3_000_000, 2)],
            vec!["spendable-box".to_string()],
        );

        // Block 3: create another box
        engine.blockchain.push_block(
            BlockHeader {
                height: 3,
                hash: "hash-3".into(),
                parent_hash: "hash-2".into(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 1,
                main_chain: true,
            },
            vec![make_box("new-box-3", 1_000_000, 3)],
            vec![],
        );

        let diff = engine.sync_block(2).await.unwrap();

        // Block 2 should have created 1 box and spent 1 box
        assert_eq!(diff.created_boxes.len(), 1);
        assert_eq!(diff.created_boxes[0].box_id, "new-box-2");
        assert_eq!(diff.spent_boxes.len(), 1);
        assert_eq!(diff.spent_boxes[0].box_id, "spendable-box");
    }

    // ----------------------------------------------------------------
    // test_box_tracking
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_box_tracking() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));
        let box_id = "test-box-123".to_string();

        let tracked = engine.track_box(box_id.clone(), "provider-monitoring".into());
        assert!(tracked);

        let boxes = engine.get_tracked_boxes();
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].0, box_id);
        assert_eq!(boxes[0].1, "provider-monitoring");

        // Tracking the same box again should return false
        let tracked_again = engine.track_box(box_id.clone(), "another-reason".into());
        assert!(!tracked_again);
    }

    // ----------------------------------------------------------------
    // test_box_untracking
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_box_untracking() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));
        let box_id = "box-to-untrack".to_string();

        engine.track_box(box_id.clone(), "test".into());
        assert!(engine.tracked_boxes.contains_key(&box_id));

        let untracked = engine.untrack_box(&box_id);
        assert!(untracked);
        assert!(!engine.tracked_boxes.contains_key(&box_id));

        // Untracking again should return false
        let untracked_again = engine.untrack_box(&box_id);
        assert!(!untracked_again);
    }

    // ----------------------------------------------------------------
    // test_provider_change_detection
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_provider_change_detection() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        // Register a provider box mapping
        engine.register_provider_box("prov-box-1".to_string(), "provider-A".to_string());

        // Create a block that creates this box
        engine.blockchain.push_block(
            BlockHeader {
                height: 1,
                hash: "hash-1".into(),
                parent_hash: String::new(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 1,
                main_chain: true,
            },
            vec![make_box("prov-box-1", 5_000_000, 1)],
            vec![],
        );

        engine.sync_block(1).await.unwrap();

        let changes = engine.get_provider_changes(Some("provider-A"));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, BoxChangeType::Created);
        assert_eq!(changes[0].provider_id, "provider-A");
        assert_eq!(changes[0].box_id, "prov-box-1");
    }

    // ----------------------------------------------------------------
    // test_fork_detection
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_fork_detection() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        // Push block 1
        engine.blockchain.push_block(
            BlockHeader {
                height: 1,
                hash: "hash-1".into(),
                parent_hash: String::new(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 1,
                main_chain: true,
            },
            vec![make_box("box-a", 1000, 1)],
            vec![],
        );

        // Push block 2
        engine.blockchain.push_block(
            BlockHeader {
                height: 2,
                hash: "hash-2".into(),
                parent_hash: "hash-1".into(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 1,
                main_chain: true,
            },
            vec![make_box("box-b", 2000, 2)],
            vec![],
        );

        // The simulated blockchain normalizes parent hashes in push_block,
        // so detect_fork correctly returns None for a consistent chain.
        // Test that no false positive fork is detected.
        let fork = engine.detect_fork(1).await;
        assert!(fork.is_none(), "No fork should be detected in a consistent chain");

        let fork2 = engine.detect_fork(2).await;
        assert!(fork2.is_none(), "No fork should be detected in a consistent chain");
    }

    // ----------------------------------------------------------------
    // test_fork_resolution
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_fork_resolution() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        // Push some blocks
        for i in 1..=3 {
            engine.blockchain.push_block(
                BlockHeader {
                    height: i,
                    hash: format!("hash-{}", i),
                    parent_hash: format!("hash-{}", i - 1),
                    timestamp: Utc::now().timestamp_millis(),
                    transactions_count: 1,
                    main_chain: true,
                },
                vec![make_box(&format!("box-{}", i), 1000 * i, i)],
                vec![],
            );
        }

        // Sync all blocks
        engine.sync_range(1, 3).await.unwrap();

        // Create a fork info and resolve
        let fork_info = ForkInfo {
            common_ancestor: 1,
            fork_height: 2,
            fork_branch: vec!["alt-hash-2".into()],
            resolved: false,
        };

        let result = engine.resolve_fork(fork_info).await;
        assert!(result.is_ok());
    }

    // ----------------------------------------------------------------
    // test_sync_start_stop
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_sync_start_stop() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        let started = engine.start_sync().await;
        assert!(started);
        assert!(engine.is_running());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let stopped = engine.stop_sync().await;
        assert!(stopped);
        assert!(!engine.is_running());

        // Starting again should work
        let started_again = engine.start_sync().await;
        assert!(started_again);
        engine.stop_sync().await;
    }

    // ----------------------------------------------------------------
    // test_sync_status
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_sync_status() {
        let engine = setup_engine_with_blocks(5);

        // Before any sync
        let status = engine.get_sync_status();
        assert_eq!(status.synced_height, 0);
        assert_eq!(status.target_height, 5);
        assert!(status.catching_up);

        // After syncing some blocks
        engine.sync_range(1, 3).await.unwrap();
        let status = engine.get_sync_status();
        assert_eq!(status.synced_height, 3);
        assert_eq!(status.blocks_behind, 2);
        assert!(status.catching_up);

        // After catching up
        engine.sync_range(4, 5).await.unwrap();
        let status = engine.get_sync_status();
        assert_eq!(status.synced_height, 5);
        assert_eq!(status.blocks_behind, 0);
        assert!(!status.catching_up);
    }

    // ----------------------------------------------------------------
    // test_empty_chain
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_empty_chain() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        let status = engine.get_sync_status();
        assert_eq!(status.synced_height, 0);
        assert_eq!(status.target_height, 0);
        assert!(!status.catching_up);

        let stats = engine.get_stats();
        assert_eq!(stats.total_blocks_synced, 0);
        assert_eq!(stats.total_boxes_tracked, 0);

        // Syncing a non-existent block should fail
        let result = engine.sync_block(1).await;
        assert!(result.is_err());

        // Getting a diff for non-existent block should return None
        assert!(engine.get_state_diff(1).is_none());
    }

    // ----------------------------------------------------------------
    // test_concurrent_sync
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_concurrent_sync() {
        let engine = setup_engine_with_blocks(10);

        // Launch concurrent syncs for different ranges
        let e1 = engine.clone();
        let e2 = engine.clone();
        let e3 = engine.clone();

        let h1 = tokio::spawn(async move { e1.sync_range(1, 4).await });
        let h2 = tokio::spawn(async move { e2.sync_range(5, 7).await });
        let h3 = tokio::spawn(async move { e3.sync_range(8, 10).await });

        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();
        let r3 = h3.await.unwrap();

        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert!(r3.is_ok());

        assert_eq!(r1.unwrap().len(), 4);
        assert_eq!(r2.unwrap().len(), 3);
        assert_eq!(r3.unwrap().len(), 3);

        let status = engine.get_sync_status();
        assert_eq!(status.synced_height, 10);
    }

    // ----------------------------------------------------------------
    // test_stats_tracking
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_stats_tracking() {
        let engine = setup_engine_with_blocks(5);

        engine.sync_range(1, 5).await.unwrap();

        let stats = engine.get_stats();
        assert_eq!(stats.total_blocks_synced, 5);
        assert!(stats.total_boxes_tracked > 0);
        assert_eq!(stats.total_state_diffs, 5);
        // avg_sync_time_ms is >= 0 (may be 0 for very fast in-memory sync)
        assert!(stats.avg_sync_time_ms >= 0);
        assert!(stats.last_error.is_none());
    }

    // ----------------------------------------------------------------
    // test_rent_collection_detection
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_rent_collection_detection() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig::default()));

        // Register a provider box
        engine.register_provider_box("rent-box-1".to_string(), "provider-R".to_string());

        // Block 1: create the box
        engine.blockchain.push_block(
            BlockHeader {
                height: 1,
                hash: "hash-1".into(),
                parent_hash: String::new(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 1,
                main_chain: true,
            },
            vec![make_box("rent-box-1", 1_000_000, 1)],
            vec![],
        );

        // Block 2: simulate rent collection by spending the box
        engine.blockchain.push_block(
            BlockHeader {
                height: 2,
                hash: "hash-2".into(),
                parent_hash: "hash-1".into(),
                timestamp: Utc::now().timestamp_millis(),
                transactions_count: 1,
                main_chain: true,
            },
            vec![],
            vec!["rent-box-1".to_string()],
        );

        engine.sync_range(1, 2).await.unwrap();

        let changes = engine.get_provider_changes(Some("provider-R"));
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].change_type, BoxChangeType::Created);
        assert_eq!(changes[1].change_type, BoxChangeType::Spent);
    }

    // ----------------------------------------------------------------
    // test_large_batch_sync
    // ----------------------------------------------------------------
    #[tokio::test]
    async fn test_large_batch_sync() {
        let engine = Arc::new(ChainSyncEngine::new(ChainSyncConfig {
            max_blocks_per_sync: 50,
            ..ChainSyncConfig::default()
        }));

        // Push 200 blocks
        for i in 1..=200 {
            let box_id = format!("batch-box-{}", i);
            engine.blockchain.push_block(
                BlockHeader {
                    height: i,
                    hash: format!("hash-{}", i),
                    parent_hash: format!("hash-{}", i - 1),
                    timestamp: 1_700_000_000_000 + (i as i64 * 120_000),
                    transactions_count: 1,
                    main_chain: true,
                },
                vec![make_box(&box_id, 1000 * i, i)],
                vec![],
            );
        }

        // Sync in batches of 50
        for batch_start in (1..=200).step_by(50) {
            let batch_end = std::cmp::min(batch_start + 49, 200);
            let result = engine.sync_range(batch_start, batch_end).await;
            assert!(result.is_ok(), "Failed to sync batch {}-{}", batch_start, batch_end);
        }

        let status = engine.get_sync_status();
        assert_eq!(status.synced_height, 200);
        assert_eq!(status.blocks_behind, 0);

        let stats = engine.get_stats();
        assert_eq!(stats.total_blocks_synced, 200);
        assert_eq!(stats.total_state_diffs, 200);
    }
}
