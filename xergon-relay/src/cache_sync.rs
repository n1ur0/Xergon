#![allow(dead_code)]
//! Distributed Cache Synchronization
//!
//! Periodically syncs inference cache entries across relay nodes using
//! version-based conflict resolution, peer health tracking, incremental
//! sync, optional compression, and anti-entropy reconciliation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::proxy::AppState;
use axum::response::IntoResponse;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Conflict resolution strategy for cache entries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Keep the entry with the highest version / latest timestamp.
    LastWriteWins,
    /// Keep the entry that was created first (oldest survives).
    FirstWriteWins,
    /// Merge values by keeping the longer payload.
    Merge,
    /// The node that originally created the entry always wins.
    ProviderAuthority,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        Self::LastWriteWins
    }
}

/// Configuration for distributed cache synchronization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSyncConfig {
    pub enabled: bool,
    pub sync_interval_secs: u64,
    pub peer_urls: Vec<String>,
    pub max_sync_entries: usize,
    pub conflict_resolution: ConflictStrategy,
    pub sync_timeout_secs: u64,
    pub compression: bool,
}

impl Default for CacheSyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sync_interval_secs: 30,
            peer_urls: Vec::new(),
            max_sync_entries: 100,
            conflict_resolution: ConflictStrategy::default(),
            sync_timeout_secs: 5,
            compression: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single cache entry that can be synchronized across nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSyncEntry {
    pub key: String,
    pub value: Vec<u8>,
    pub version: u64,
    pub checksum: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source_node: String,
    pub ttl: Option<DateTime<Utc>>,
}

impl CacheSyncEntry {
    /// Compute SHA-256 checksum of the value.
    pub fn compute_checksum(value: &[u8]) -> String {
        hex::encode(Sha256::digest(value))
    }
}

/// Health status of a sync peer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PeerStatus {
    Active,
    Inactive,
    Degraded,
    Banned,
}

/// Metadata for a peer relay node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSyncNode {
    pub id: String,
    pub url: String,
    pub last_sync: Option<DateTime<Utc>>,
    pub entries_synced: u64,
    pub errors: u64,
    pub status: PeerStatus,
}

impl CacheSyncNode {
    pub fn new(id: String, url: String) -> Self {
        Self {
            id,
            url,
            last_sync: None,
            entries_synced: 0,
            errors: 0,
            status: PeerStatus::Active,
        }
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Aggregated sync statistics (lock-free).
#[derive(Debug, Default, Serialize)]
pub struct SyncStats {
    pub total_syncs: AtomicU64,
    pub entries_pushed: AtomicU64,
    pub entries_pulled: AtomicU64,
    pub conflicts_resolved: AtomicU64,
    pub sync_errors: AtomicU64,
    pub bytes_transferred: AtomicU64,
}

impl SyncStats {
    pub fn snapshot(&self) -> SyncStatsSnapshot {
        SyncStatsSnapshot {
            total_syncs: self.total_syncs.load(Ordering::Relaxed),
            entries_pushed: self.entries_pushed.load(Ordering::Relaxed),
            entries_pulled: self.entries_pulled.load(Ordering::Relaxed),
            conflicts_resolved: self.conflicts_resolved.load(Ordering::Relaxed),
            sync_errors: self.sync_errors.load(Ordering::Relaxed),
            bytes_transferred: self.bytes_transferred.load(Ordering::Relaxed),
        }
    }
}

/// A point-in-time copy of sync stats (for API responses).
#[derive(Debug, Serialize)]
pub struct SyncStatsSnapshot {
    pub total_syncs: u64,
    pub entries_pushed: u64,
    pub entries_pulled: u64,
    pub conflicts_resolved: u64,
    pub sync_errors: u64,
    pub bytes_transferred: u64,
}

// ---------------------------------------------------------------------------
// Sync payload for wire transfer
// ---------------------------------------------------------------------------

/// Batch of entries sent between nodes during sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBatch {
    pub node_id: String,
    pub entries: Vec<CacheSyncEntry>,
    /// If true, entries with higher version should replace local entries.
    pub incremental: bool,
}

// ---------------------------------------------------------------------------
// CacheSynchronizer
// ---------------------------------------------------------------------------

/// The main distributed cache synchronizer.
pub struct CacheSynchronizer {
    config: std::sync::RwLock<CacheSyncConfig>,
    local_entries: DashMap<String, CacheSyncEntry>,
    peers: DashMap<String, CacheSyncNode>,
    node_id: String,
    stats: SyncStats,
    /// Monotonically increasing version counter.
    version_counter: AtomicU64,
}

impl CacheSynchronizer {
    /// Create a new synchronizer with the given config and node identity.
    pub fn new(config: CacheSyncConfig, node_id: String) -> Self {
        // Seed peers from config
        let peers = DashMap::new();
        for url in &config.peer_urls {
            let id = Self::derive_peer_id(url);
            peers.insert(id.clone(), CacheSyncNode::new(id, url.clone()));
        }

        Self {
            config: std::sync::RwLock::new(config),
            local_entries: DashMap::new(),
            peers,
            node_id,
            stats: SyncStats::default(),
            version_counter: AtomicU64::new(1),
        }
    }

    // -- Public API ----------------------------------------------------------

    /// Insert or update a local cache entry.
    pub fn put(&self, key: String, value: Vec<u8>, ttl_secs: Option<u64>) {
        let now = Utc::now();
        let version = self.version_counter.fetch_add(1, Ordering::Relaxed);
        let checksum = CacheSyncEntry::compute_checksum(&value);

        self.local_entries.insert(key.clone(), CacheSyncEntry {
            key,
            value,
            version,
            checksum,
            created_at: now,
            updated_at: now,
            source_node: self.node_id.clone(),
            ttl: ttl_secs.map(|s| now + chrono::Duration::seconds(s as i64)),
        });
    }

    /// Get a local cache entry (returns None if expired or missing).
    pub fn get(&self, key: &str) -> Option<CacheSyncEntry> {
        self.local_entries.get(key).and_then(|e| {
            let entry = e.value().clone();
            if let Some(ttl) = entry.ttl {
                if Utc::now() > ttl {
                    drop(e);
                    self.local_entries.remove(key);
                    return None;
                }
            }
            Some(entry)
        })
    }

    /// Remove a local cache entry.
    pub fn remove(&self, key: &str) {
        self.local_entries.remove(key);
    }

    /// Number of local entries.
    pub fn len(&self) -> usize {
        self.local_entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.local_entries.is_empty()
    }

    /// Add a peer node.
    pub fn add_peer(&self, id: String, url: String) {
        self.peers.insert(id.clone(), CacheSyncNode::new(id, url));
    }

    /// Remove a peer node.
    pub fn remove_peer(&self, id: &str) {
        self.peers.remove(id);
    }

    /// Update sync configuration at runtime.
    pub fn update_config(&self, update: CacheSyncConfigUpdate) {
        let mut cfg = self.config.write().unwrap();
        if let Some(enabled) = update.enabled {
            cfg.enabled = enabled;
        }
        if let Some(interval) = update.sync_interval_secs {
            cfg.sync_interval_secs = interval;
        }
        if let Some(max) = update.max_sync_entries {
            cfg.max_sync_entries = max;
        }
        if let Some(strategy) = update.conflict_resolution {
            cfg.conflict_resolution = strategy;
        }
        if let Some(timeout) = update.sync_timeout_secs {
            cfg.sync_timeout_secs = timeout;
        }
        if let Some(comp) = update.compression {
            cfg.compression = comp;
        }
    }

    /// Check if sync is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.read().unwrap().enabled
    }

    /// Get sync interval.
    pub fn sync_interval(&self) -> Duration {
        Duration::from_secs(self.config.read().unwrap().sync_interval_secs)
    }

    /// Trigger an immediate full sync with all peers.
    pub async fn trigger_sync(&self, client: &reqwest::Client) {
        self.sync_all_peers(client).await;
    }

    /// Get current configuration.
    pub fn get_config(&self) -> CacheSyncConfig {
        self.config.read().unwrap().clone()
    }

    /// Get status for the status endpoint.
    pub fn get_status(&self) -> CacheSyncStatus {
        let cfg = self.config.read().unwrap().clone();
        let peers: Vec<CacheSyncNode> = self.peers.iter().map(|r| r.value().clone()).collect();
        CacheSyncStatus {
            enabled: cfg.enabled,
            node_id: self.node_id.clone(),
            local_entries: self.local_entries.len(),
            peer_count: peers.len(),
            peers,
            stats: self.stats.snapshot(),
            config: cfg,
        }
    }

    /// Get paginated local entries.
    pub fn get_entries(&self, offset: usize, limit: usize) -> Vec<CacheSyncEntry> {
        self.local_entries
            .iter()
            .skip(offset)
            .take(limit)
            .map(|r| r.value().clone())
            .collect()
    }

    // -- Internal sync logic -------------------------------------------------

    /// Run sync against all active peers.
    async fn sync_all_peers(&self, client: &reqwest::Client) {
        let cfg = self.config.read().unwrap().clone();
        if !cfg.enabled || self.peers.is_empty() {
            return;
        }

        self.stats.total_syncs.fetch_add(1, Ordering::Relaxed);

        // Collect active peers
        let peer_list: Vec<(String, String)> = self.peers
            .iter()
            .filter(|r| r.value().status == PeerStatus::Active)
            .map(|r| (r.key().clone(), r.value().url.clone()))
            .collect();

        for (peer_id, peer_url) in &peer_list {
            if let Err(e) = self.sync_with_peer(client, peer_id, peer_url, &cfg).await {
                self.stats.sync_errors.fetch_add(1, Ordering::Relaxed);
                // Update peer error count and status
                if let Some(mut peer) = self.peers.get_mut(peer_id) {
                    peer.errors += 1;
                    if peer.errors > 10 {
                        peer.status = PeerStatus::Degraded;
                        warn!(peer = %peer_id, "Peer degraded after 10 errors");
                    }
                }
                warn!(peer = %peer_id, error = %e, "Sync with peer failed");
            }
        }

        // Anti-entropy: prune expired entries
        self.prune_expired();

        debug!(
            entries = self.local_entries.len(),
            "Sync cycle complete"
        );
    }

    /// Sync with a single peer: push local changes, pull remote changes.
    async fn sync_with_peer(
        &self,
        client: &reqwest::Client,
        peer_id: &str,
        peer_url: &str,
        cfg: &CacheSyncConfig,
    ) -> anyhow::Result<()> {
        let timeout = Duration::from_secs(cfg.sync_timeout_secs);

        // Build local batch (incremental: only send entries updated in the
        // last sync_interval * 2)
        let cutoff = Utc::now() - chrono::Duration::seconds((cfg.sync_interval_secs * 2) as i64);
        let mut local_batch: Vec<CacheSyncEntry> = self.local_entries
            .iter()
            .filter(|e| {
                e.value().updated_at > cutoff || e.value().source_node == self.node_id
            })
            .take(cfg.max_sync_entries)
            .map(|r| r.value().clone())
            .collect();

        // Compress values if configured
        if cfg.compression {
            for entry in &mut local_batch {
                entry.value = compress_data(&entry.value)?;
            }
        }

        let batch = SyncBatch {
            node_id: self.node_id.clone(),
            entries: local_batch,
            incremental: true,
        };

        let body = serde_json::to_vec(&batch)?;
        self.stats.bytes_transferred.fetch_add(body.len() as u64, Ordering::Relaxed);

        // POST to peer's sync endpoint
        let url = format!("{}/api/cache-sync/receive", peer_url.trim_end_matches('/'));
        let resp = client
            .post(&url)
            .timeout(timeout)
            .header("Content-Type", "application/json")
            .header("X-Node-Id", &self.node_id)
            .body(body)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Peer returned status {}", resp.status());
        }

        // Parse remote batch from response
        let remote_data = resp.bytes().await?;
        self.stats.bytes_transferred.fetch_add(remote_data.len() as u64, Ordering::Relaxed);

        let remote_batch: SyncBatch = serde_json::from_slice(&remote_data)?;
        let mut entries_received = 0u64;
        let mut conflicts = 0u64;

        for remote_entry in remote_batch.entries {
            let mut remote_entry = remote_entry;
            // Decompress if needed
            if cfg.compression {
                remote_entry.value = decompress_data(&remote_entry.value)?;
            }
            let key = remote_entry.key.clone();
            entries_received += 1;

            // Resolve conflict / merge
            let resolved = self.resolve_conflict(&key, remote_entry, cfg.conflict_resolution);
            if resolved.was_conflict {
                conflicts += 1;
            }
            self.local_entries.insert(key, resolved.entry);
        }

        self.stats.entries_pushed.fetch_add(batch.entries.len() as u64, Ordering::Relaxed);
        self.stats.entries_pulled.fetch_add(entries_received, Ordering::Relaxed);
        self.stats.conflicts_resolved.fetch_add(conflicts, Ordering::Relaxed);

        // Update peer metadata
        if let Some(mut peer) = self.peers.get_mut(peer_id) {
            peer.last_sync = Some(Utc::now());
            peer.entries_synced += entries_received;
            peer.errors = 0; // reset on success
            if peer.status == PeerStatus::Degraded {
                peer.status = PeerStatus::Active;
            }
        }

        info!(
            peer = %peer_id,
            pushed = batch.entries.len(),
            pulled = entries_received,
            conflicts = conflicts,
            "Peer sync complete"
        );

        Ok(())
    }

    /// Resolve a conflict between local and remote entry.
    fn resolve_conflict(
        &self,
        key: &str,
        remote: CacheSyncEntry,
        strategy: ConflictStrategy,
    ) -> ConflictResolution {
        match self.local_entries.get(key) {
            None => ConflictResolution {
                entry: remote,
                was_conflict: false,
            },
            Some(local) => {
                let local = local.value().clone();
                if local.version == remote.version && local.checksum == remote.checksum {
                    // Identical — no conflict
                    ConflictResolution {
                        entry: remote,
                        was_conflict: false,
                    }
                } else {
                    // Actual conflict — resolve by strategy
                    let winner = match strategy {
                        ConflictStrategy::LastWriteWins => {
                            if remote.updated_at > local.updated_at {
                                remote
                            } else {
                                local
                            }
                        }
                        ConflictStrategy::FirstWriteWins => {
                            if remote.created_at < local.created_at {
                                remote
                            } else {
                                local
                            }
                        }
                        ConflictStrategy::Merge => {
                            // Keep the longer payload
                            if remote.value.len() >= local.value.len() {
                                remote
                            } else {
                                local
                            }
                        }
                        ConflictStrategy::ProviderAuthority => {
                            // Original source node always wins
                            if remote.source_node == local.source_node && remote.version > local.version {
                                remote
                            } else {
                                local
                            }
                        }
                    };
                    ConflictResolution {
                        entry: winner,
                        was_conflict: true,
                    }
                }
            }
        }
    }

    /// Remove expired entries (anti-entropy).
    fn prune_expired(&self) {
        let now = Utc::now();
        self.local_entries.retain(|_, entry| {
            if let Some(ttl) = entry.ttl {
                now <= ttl
            } else {
                true
            }
        });
    }

    /// Derive a simple peer ID from URL.
    fn derive_peer_id(url: &str) -> String {
        let hash = hex::encode(Sha256::digest(url.as_bytes()));
        hash[..16].to_string()
    }

    /// Start the background sync task.
    pub fn start_sync_task(self: &Arc<Self>, client: reqwest::Client) -> tokio::task::JoinHandle<()> {
        let sync = self.clone();
        tokio::spawn(async move {
            loop {
                let interval = sync.sync_interval();
                tokio::time::sleep(interval).await;
                if sync.is_enabled() {
                    sync.sync_all_peers(&client).await;
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Conflict resolution result
// ---------------------------------------------------------------------------

struct ConflictResolution {
    entry: CacheSyncEntry,
    was_conflict: bool,
}

// ---------------------------------------------------------------------------
// Status types for API responses
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CacheSyncStatus {
    pub enabled: bool,
    pub node_id: String,
    pub local_entries: usize,
    pub peer_count: usize,
    pub peers: Vec<CacheSyncNode>,
    pub stats: SyncStatsSnapshot,
    pub config: CacheSyncConfig,
}

/// Partial config update for PATCH endpoint.
#[derive(Debug, Deserialize)]
pub struct CacheSyncConfigUpdate {
    pub enabled: Option<bool>,
    pub sync_interval_secs: Option<u64>,
    pub max_sync_entries: Option<usize>,
    pub conflict_resolution: Option<ConflictStrategy>,
    pub sync_timeout_secs: Option<u64>,
    pub compression: Option<bool>,
}

// ---------------------------------------------------------------------------
// Compression helpers
// ---------------------------------------------------------------------------

fn compress_data(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

fn decompress_data(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let mut decoder = GzDecoder::new(data);
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    Ok(output)
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use std::sync::Arc;

/// Build the cache-sync router.
pub fn build_cache_sync_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/cache-sync/status", get(cache_sync_status_handler))
        .route("/api/cache-sync/entries", get(cache_sync_entries_handler))
        .route("/api/cache-sync/trigger", post(cache_sync_trigger_handler))
        .route("/api/cache-sync/peers", post(cache_sync_add_peer_handler))
        .route("/api/cache-sync/peers/{node_id}", delete(cache_sync_remove_peer_handler))
        .route("/api/cache-sync/stats", get(cache_sync_stats_handler))
        .route("/api/cache-sync/config", patch(cache_sync_config_handler))
        .route("/api/cache-sync/receive", post(cache_sync_receive_handler))
        .with_state(state)
}

/// GET /api/cache-sync/status
async fn cache_sync_status_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let sync = match &state.cache_synchronizer {
        Some(s) => s,
        None => {
            return Json(serde_json::json!({
                "enabled": false,
                "error": "cache_synchronizer not initialized"
            }));
        }
    };
    let status = sync.get_status();
    Json(serde_json::json!(status))
}

/// GET /api/cache-sync/entries?offset=0&limit=50
async fn cache_sync_entries_handler(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let offset: usize = params.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0);
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);

    let entries = match &state.cache_synchronizer {
        Some(s) => s.get_entries(offset, limit),
        None => Vec::new(),
    };
    let total = state.cache_synchronizer.as_ref().map(|s| s.len()).unwrap_or(0);

    Json(serde_json::json!({
        "entries": entries,
        "offset": offset,
        "limit": limit,
        "total": total,
    }))
}

/// POST /api/cache-sync/trigger
async fn cache_sync_trigger_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match &state.cache_synchronizer {
        Some(s) => {
            let s = s.clone();
            let client = state.http_client.clone();
            tokio::spawn(async move {
                s.trigger_sync(&client).await;
            });
            (axum::http::StatusCode::ACCEPTED, "sync triggered")
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cache_synchronizer not initialized",
        ),
    }
}

/// POST /api/cache-sync/peers  { "id": "...", "url": "..." }
async fn cache_sync_add_peer_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");

    if id.is_empty() || url.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "id and url are required",
        );
    }

    match &state.cache_synchronizer {
        Some(s) => {
            s.add_peer(id.to_string(), url.to_string());
            (axum::http::StatusCode::OK, "peer added")
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cache_synchronizer not initialized",
        ),
    }
}

/// DELETE /api/cache-sync/peers/{node_id}
async fn cache_sync_remove_peer_handler(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match &state.cache_synchronizer {
        Some(s) => {
            s.remove_peer(&node_id);
            (axum::http::StatusCode::OK, "peer removed")
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cache_synchronizer not initialized",
        ),
    }
}

/// GET /api/cache-sync/stats
async fn cache_sync_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match &state.cache_synchronizer {
        Some(s) => {
            let status = s.get_status();
            Json(serde_json::json!({
                "stats": status.stats,
                "local_entries": status.local_entries,
                "peer_count": status.peer_count,
            }))
        }
        None => Json(serde_json::json!({
            "error": "cache_synchronizer not initialized"
        })),
    }
}

/// PATCH /api/cache-sync/config
async fn cache_sync_config_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let update = CacheSyncConfigUpdate {
        enabled: body.get("enabled").and_then(|v| v.as_bool()),
        sync_interval_secs: body.get("sync_interval_secs").and_then(|v| v.as_u64()),
        max_sync_entries: body.get("max_sync_entries").and_then(|v| v.as_u64().map(|n| n as usize)),
        conflict_resolution: body.get("conflict_resolution")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok()),
        sync_timeout_secs: body.get("sync_timeout_secs").and_then(|v| v.as_u64()),
        compression: body.get("compression").and_then(|v| v.as_bool()),
    };

    match &state.cache_synchronizer {
        Some(s) => {
            s.update_config(update);
            let cfg = s.get_config();
            (axum::http::StatusCode::OK, axum::Json(serde_json::json!(cfg))).into_response()
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cache_synchronizer not initialized",
        ).into_response(),
    }
}

/// POST /api/cache-sync/receive — internal endpoint for receiving sync batches from peers.
async fn cache_sync_receive_handler(
    State(state): State<AppState>,
    Json(batch): Json<SyncBatch>,
) -> impl IntoResponse {
    match &state.cache_synchronizer {
        Some(s) => {
            let cfg = s.get_config();
            let mut entries_received = 0u64;
            let mut conflicts = 0u64;

            for remote_entry in batch.entries {
                let mut remote_entry = remote_entry;
                if cfg.compression {
                    remote_entry.value = match decompress_data(&remote_entry.value) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!(error = %e, "Failed to decompress entry from peer {}", batch.node_id);
                            continue;
                        }
                    };
                }
                let key = remote_entry.key.clone();
                entries_received += 1;

                let resolved = s.resolve_conflict(&key, remote_entry, cfg.conflict_resolution);
                if resolved.was_conflict {
                    conflicts += 1;
                }
                s.local_entries.insert(key, resolved.entry);
            }

            // Build response batch to send back
            let cutoff = Utc::now() - chrono::Duration::seconds((cfg.sync_interval_secs * 2) as i64);
            let mut response_entries: Vec<CacheSyncEntry> = s.local_entries
                .iter()
                .filter(|e| e.value().updated_at > cutoff || e.value().source_node == s.node_id)
                .take(cfg.max_sync_entries)
                .map(|r| r.value().clone())
                .collect();

            if cfg.compression {
                for entry in &mut response_entries {
                    if let Err(e) = compress_data(&entry.value).map(|c| entry.value = c) {
                        warn!(error = %e, "Failed to compress response entry");
                    }
                }
            }

            let response_batch = SyncBatch {
                node_id: s.node_id.clone(),
                entries: response_entries,
                incremental: true,
            };

            (axum::http::StatusCode::OK, axum::Json(response_batch)).into_response()
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cache_synchronizer not initialized",
        ).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CacheSyncConfig {
        CacheSyncConfig {
            enabled: false,
            sync_interval_secs: 30,
            peer_urls: vec![],
            max_sync_entries: 100,
            conflict_resolution: ConflictStrategy::LastWriteWins,
            sync_timeout_secs: 5,
            compression: false,
        }
    }

    #[test]
    fn test_new_synchronizer() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        assert!(sync.is_empty());
        assert_eq!(sync.len(), 0);
        assert!(!sync.is_enabled());
    }

    #[test]
    fn test_put_and_get_entry() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        sync.put("key1".to_string(), b"hello".to_vec(), None);
        let entry = sync.get("key1").unwrap();
        assert_eq!(entry.key, "key1");
        assert_eq!(entry.value, b"hello");
        assert_eq!(entry.source_node, "node1");
    }

    #[test]
    fn test_get_missing_entry() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        assert!(sync.get("nonexistent").is_none());
    }

    #[test]
    fn test_remove_entry() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        sync.put("key1".to_string(), b"hello".to_vec(), None);
        assert!(sync.get("key1").is_some());
        sync.remove("key1");
        assert!(sync.get("key1").is_none());
    }

    #[test]
    fn test_len_and_is_empty() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        assert!(sync.is_empty());
        assert_eq!(sync.len(), 0);
        sync.put("key1".to_string(), b"a".to_vec(), None);
        assert!(!sync.is_empty());
        assert_eq!(sync.len(), 1);
        sync.put("key2".to_string(), b"b".to_vec(), None);
        assert_eq!(sync.len(), 2);
    }

    #[test]
    fn test_add_and_remove_peer() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        sync.add_peer("peer1".to_string(), "http://peer1:8080".to_string());
        let status = sync.get_status();
        assert_eq!(status.peer_count, 1);
        sync.remove_peer("peer1");
        let status = sync.get_status();
        assert_eq!(status.peer_count, 0);
    }

    #[test]
    fn test_config_default() {
        let config = CacheSyncConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.sync_interval_secs, 30);
        assert_eq!(config.max_sync_entries, 100);
        assert!(config.compression);
    }

    #[test]
    fn test_update_config() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        let update = CacheSyncConfigUpdate {
            enabled: Some(true),
            sync_interval_secs: Some(60),
            max_sync_entries: Some(200),
            conflict_resolution: Some(ConflictStrategy::FirstWriteWins),
            sync_timeout_secs: None,
            compression: None,
        };
        sync.update_config(update);
        assert!(sync.is_enabled());
        assert_eq!(sync.sync_interval().as_secs(), 60);
        let cfg = sync.get_config();
        assert_eq!(cfg.max_sync_entries, 200);
        assert_eq!(cfg.conflict_resolution, ConflictStrategy::FirstWriteWins);
    }

    #[test]
    fn test_is_enabled() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        assert!(!sync.is_enabled());
        sync.update_config(CacheSyncConfigUpdate {
            enabled: Some(true),
            sync_interval_secs: None,
            max_sync_entries: None,
            conflict_resolution: None,
            sync_timeout_secs: None,
            compression: None,
        });
        assert!(sync.is_enabled());
    }

    #[test]
    fn test_sync_interval() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        assert_eq!(sync.sync_interval().as_secs(), 30);
    }

    #[test]
    fn test_get_status() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        sync.put("k1".to_string(), b"v1".to_vec(), None);
        let status = sync.get_status();
        assert_eq!(status.node_id, "node1");
        assert_eq!(status.local_entries, 1);
        assert!(!status.enabled);
    }

    #[test]
    fn test_get_entries_pagination() {
        let sync = CacheSynchronizer::new(test_config(), "node1".to_string());
        for i in 0..5 {
            sync.put(format!("key{}", i), vec![i as u8], None);
        }
        let page1 = sync.get_entries(0, 3);
        assert_eq!(page1.len(), 3);
        let page2 = sync.get_entries(3, 3);
        assert_eq!(page2.len(), 2);
        let empty = sync.get_entries(10, 3);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_compute_checksum() {
        let checksum1 = CacheSyncEntry::compute_checksum(b"hello");
        let checksum2 = CacheSyncEntry::compute_checksum(b"hello");
        let checksum3 = CacheSyncEntry::compute_checksum(b"world");
        assert_eq!(checksum1, checksum2);
        assert_ne!(checksum1, checksum3);
        assert!(!checksum1.is_empty());
    }

    #[test]
    fn test_peer_status_enum() {
        let node = CacheSyncNode::new("p1".to_string(), "http://p1".to_string());
        assert_eq!(node.status, PeerStatus::Active);
        assert_eq!(node.entries_synced, 0);
        assert_eq!(node.errors, 0);
        assert!(node.last_sync.is_none());
    }

    #[test]
    fn test_conflict_strategy_default() {
        assert_eq!(ConflictStrategy::default(), ConflictStrategy::LastWriteWins);
    }

    #[test]
    fn test_seeds_peers_from_config() {
        let config = CacheSyncConfig {
            peer_urls: vec!["http://peer1:8080".to_string()],
            ..test_config()
        };
        let sync = CacheSynchronizer::new(config, "node1".to_string());
        let status = sync.get_status();
        assert_eq!(status.peer_count, 1);
    }
}
