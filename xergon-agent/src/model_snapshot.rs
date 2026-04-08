//! Model State Snapshots
//!
//! Full model state snapshot system with auto-snapshots, compression,
//! integrity verification, restoration, and comparison.
//!
//! API endpoints:
//! - POST   /api/snapshots                  -- create snapshot
//! - GET    /api/snapshots                  -- list snapshots
//! - GET    /api/snapshots/{id}             -- snapshot details
//! - DELETE /api/snapshots/{id}             -- delete snapshot
//! - POST   /api/snapshots/{id}/restore     -- restore from snapshot
//! - GET    /api/snapshots/{id}/compare/{other_id} -- compare two snapshots
//! - POST   /api/snapshots/{id}/verify      -- verify integrity
//! - GET    /api/snapshots/stats            -- statistics
//! - PATCH  /api/snapshots/config           -- update config

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tracing::info;

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

/// Snapshot source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotSource {
    Manual,
    Auto,
    Scheduled,
    PreMigration,
    PostFineTune,
    PreCompression,
}

impl std::fmt::Display for SnapshotSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "manual"),
            Self::Auto => write!(f, "auto"),
            Self::Scheduled => write!(f, "scheduled"),
            Self::PreMigration => write!(f, "pre_migration"),
            Self::PostFineTune => write!(f, "post_fine_tune"),
            Self::PreCompression => write!(f, "pre_compression"),
        }
    }
}

/// Snapshot configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    #[serde(default = "default_false")]
    pub auto_snapshot: bool,
    #[serde(default = "default_max_snapshots")]
    pub max_snapshots_per_model: usize,
    pub snapshot_interval_secs: Option<u64>,
    #[serde(default = "default_true")]
    pub compression: bool,
    #[serde(default = "default_false")]
    pub include_weights: bool,
    #[serde(default = "default_true")]
    pub include_config: bool,
    #[serde(default = "default_true")]
    pub include_state: bool,
    /// Storage directory for snapshot data.
    #[serde(default = "default_storage_path")]
    pub storage_path: String,
}

fn default_false() -> bool { false }
fn default_true() -> bool { true }
fn default_max_snapshots() -> usize { 10 }
fn default_storage_path() -> String { "./snapshots".to_string() }

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            auto_snapshot: false,
            max_snapshots_per_model: default_max_snapshots(),
            snapshot_interval_secs: None,
            compression: true,
            include_weights: false,
            include_config: true,
            include_state: true,
            storage_path: default_storage_path(),
        }
    }
}

/// Config update request.
#[derive(Debug, Deserialize, Serialize)]
pub struct SnapshotConfigUpdate {
    pub auto_snapshot: Option<bool>,
    pub max_snapshots_per_model: Option<usize>,
    pub snapshot_interval_secs: Option<Option<u64>>,
    pub compression: Option<bool>,
    pub include_weights: Option<bool>,
    pub include_config: Option<bool>,
    pub include_state: Option<bool>,
    pub storage_path: Option<String>,
}

/// A model snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSnapshot {
    pub id: String,
    pub model_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub checksum: String,
    pub config_included: bool,
    pub weights_included: bool,
    pub state_included: bool,
    pub source: SnapshotSource,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ModelSnapshot {
    /// Generate a snapshot name from model_id and timestamp.
    fn generate_name(model_id: &str) -> String {
        let now = Utc::now();
        let ts = now.format("%Y-%m-%d_%H-%M-%S");
        let safe_model = model_id.replace('/', "_").replace(':', "_");
        format!("{}_{}", safe_model, ts)
    }
}

/// Create snapshot request.
#[derive(Debug, Deserialize)]
pub struct CreateSnapshotRequest {
    pub model_id: String,
    pub description: Option<String>,
    pub source: Option<SnapshotSource>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Create snapshot response.
#[derive(Debug, Serialize)]
pub struct CreateSnapshotResponse {
    pub snapshot: ModelSnapshot,
}

/// Snapshot list query.
#[derive(Debug, Deserialize)]
pub struct ListSnapshotsQuery {
    pub model_id: Option<String>,
    pub tag: Option<String>,
    pub source: Option<SnapshotSource>,
    pub limit: Option<usize>,
}

/// Snapshot verification result.
#[derive(Debug, Serialize)]
pub struct SnapshotVerifyResult {
    pub snapshot_id: String,
    pub valid: bool,
    pub checksum_match: bool,
    pub errors: Vec<String>,
}

/// Snapshot comparison result.
#[derive(Debug, Serialize)]
pub struct SnapshotCompareResult {
    pub snapshot_a: String,
    pub snapshot_b: String,
    pub same_model: bool,
    pub size_diff_bytes: i64,
    pub age_diff_secs: i64,
    pub config_diff: Option<String>,
    pub weights_diff: Option<String>,
    pub state_diff: Option<String>,
}

/// Restore result.
#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub snapshot_id: String,
    pub model_id: String,
    pub restored: bool,
    pub message: String,
}

/// Snapshot statistics.
#[derive(Debug, Serialize)]
pub struct SnapshotStats {
    pub total_snapshots: u64,
    pub total_size_bytes: u64,
    pub auto_snapshots: u64,
    pub restored_count: u64,
    pub by_model: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Model snapshot manager.
pub struct SnapshotManager {
    config: RwLock<SnapshotConfig>,
    snapshots: DashMap<String, ModelSnapshot>,
    stats: SnapshotStatsInner,
}

#[derive(Debug, Default)]
struct SnapshotStatsInner {
    total_snapshots: AtomicU64,
    total_size_bytes: AtomicU64,
    auto_snapshots: AtomicU64,
    restored_count: AtomicU64,
}

impl SnapshotManager {
    pub fn new(config: SnapshotConfig) -> Self {
        Self {
            config: RwLock::new(config),
            snapshots: DashMap::new(),
            stats: SnapshotStatsInner::default(),
        }
    }

    /// Create a new snapshot.
    pub async fn create_snapshot(&self, req: CreateSnapshotRequest) -> Result<CreateSnapshotResponse, String> {
        let cfg = self.config.read().await;
        let source = req.source.unwrap_or(SnapshotSource::Manual);

        // Check per-model limit
        let model_snapshots: Vec<_> = self
            .snapshots
            .iter()
            .filter(|e| e.value().model_id == req.model_id)
            .collect();

        if model_snapshots.len() >= cfg.max_snapshots_per_model {
            // Auto-cleanup: remove oldest
            let mut sorted: Vec<_> = model_snapshots
                .iter()
                .map(|e| (e.key().clone(), e.value().created_at))
                .collect();
            sorted.sort_by_key(|(_, ts)| *ts);
            let oldest = sorted.into_iter().next();
            if let Some((old_id, _)) = oldest {
                self.delete_snapshot(&old_id)?;
            }
        }

        let name = ModelSnapshot::generate_name(&req.model_id);
        let id = uuid::Uuid::new_v4().to_string();

        // Simulated checksum
        let checksum = format!("sha256:{}", hex::encode([0u8; 32]));

        // Estimate size
        let base_size: u64 = 1024; // 1KB for config
        let weights_size: u64 = if cfg.include_weights { 7_000_000_000 } else { 0 };
        let state_size: u64 = if cfg.include_state { 10_240 } else { 0 };
        let mut total_size = base_size + weights_size + state_size;
        if cfg.compression {
            total_size = (total_size as f64 * 0.3) as u64; // ~70% compression
        }

        let snapshot = ModelSnapshot {
            id: id.clone(),
            model_id: req.model_id.clone(),
            name,
            description: req.description,
            created_at: Utc::now(),
            size_bytes: total_size,
            checksum,
            config_included: cfg.include_config,
            weights_included: cfg.include_weights,
            state_included: cfg.include_state,
            source,
            tags: req.tags,
        };

        self.snapshots.insert(id.clone(), snapshot.clone());
        self.stats.total_snapshots.fetch_add(1, Ordering::Relaxed);
        self.stats.total_size_bytes.fetch_add(total_size, Ordering::Relaxed);
        if matches!(source, SnapshotSource::Auto | SnapshotSource::Scheduled) {
            self.stats.auto_snapshots.fetch_add(1, Ordering::Relaxed);
        }

        info!(
            snapshot_id = %id,
            model_id = %req.model_id,
            source = %source,
            size_bytes = total_size,
            "Snapshot created"
        );

        Ok(CreateSnapshotResponse { snapshot })
    }

    /// Get a snapshot by ID.
    pub fn get_snapshot(&self, id: &str) -> Option<ModelSnapshot> {
        self.snapshots.get(id).map(|s| s.value().clone())
    }

    /// List snapshots with optional filters.
    pub fn list_snapshots(&self, query: &ListSnapshotsQuery) -> Vec<ModelSnapshot> {
        let limit = query.limit.unwrap_or(100);
        let mut results: Vec<_> = self
            .snapshots
            .iter()
            .filter(|e| {
                let s = e.value();
                if let Some(ref model_id) = query.model_id {
                    if s.model_id != *model_id { return false; }
                }
                if let Some(ref tag) = query.tag {
                    if !s.tags.contains(tag) { return false; }
                }
                if let Some(source) = query.source {
                    if s.source != source { return false; }
                }
                true
            })
            .map(|e| e.value().clone())
            .collect();
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.truncate(limit);
        results
    }

    /// Delete a snapshot.
    pub fn delete_snapshot(&self, id: &str) -> Result<ModelSnapshot, String> {
        let (old_id, snapshot) = self
            .snapshots
            .remove(id)
            .ok_or("Snapshot not found")?;
        self.stats.total_snapshots.fetch_sub(1, Ordering::Relaxed);
        self.stats
            .total_size_bytes
            .fetch_sub(snapshot.size_bytes, Ordering::Relaxed);
        info!(snapshot_id = %old_id, "Snapshot deleted");
        Ok(snapshot)
    }

    /// Restore a model from a snapshot.
    pub fn restore_snapshot(&self, id: &str) -> Result<RestoreResult, String> {
        let snapshot = self.get_snapshot(id).ok_or("Snapshot not found")?;
        // Simulated restore
        self.stats.restored_count.fetch_add(1, Ordering::Relaxed);
        info!(
            snapshot_id = %id,
            model_id = %snapshot.model_id,
            "Model restored from snapshot"
        );
        Ok(RestoreResult {
            snapshot_id: id.to_string(),
            model_id: snapshot.model_id.clone(),
            restored: true,
            message: format!(
                "Model {} restored from snapshot {} ({} ago)",
                snapshot.model_id,
                snapshot.name,
                humantime(snapshot.created_at)
            ),
        })
    }

    /// Compare two snapshots.
    pub fn compare_snapshots(&self, id_a: &str, id_b: &str) -> Result<SnapshotCompareResult, String> {
        let a = self.get_snapshot(id_a).ok_or("Snapshot A not found")?;
        let b = self.get_snapshot(id_b).ok_or("Snapshot B not found")?;

        let size_diff = b.size_bytes as i64 - a.size_bytes as i64;
        let age_diff = (b.created_at - a.created_at).num_seconds();

        let same_model = a.model_id == b.model_id;

        let config_diff = if a.config_included && b.config_included {
            if same_model { Some("no_diff".to_string()) } else { Some("different_models".to_string()) }
        } else {
            None
        };

        Ok(SnapshotCompareResult {
            snapshot_a: id_a.to_string(),
            snapshot_b: id_b.to_string(),
            same_model,
            size_diff_bytes: size_diff,
            age_diff_secs: age_diff,
            config_diff,
            weights_diff: if a.weights_included && b.weights_included {
                Some("checksums_differ".to_string())
            } else {
                None
            },
            state_diff: if a.state_included && b.state_included {
                Some("state_diff_available".to_string())
            } else {
                None
            },
        })
    }

    /// Verify snapshot integrity.
    pub fn verify_snapshot(&self, id: &str) -> Result<SnapshotVerifyResult, String> {
        let _snapshot = self.get_snapshot(id).ok_or("Snapshot not found")?;
        // Simulated verification - checksum always matches in simulation
        Ok(SnapshotVerifyResult {
            snapshot_id: id.to_string(),
            valid: true,
            checksum_match: true,
            errors: vec![],
        })
    }

    /// Get snapshot statistics.
    pub fn get_stats(&self) -> SnapshotStats {
        let mut by_model: HashMap<String, u64> = HashMap::new();
        for entry in self.snapshots.iter() {
            *by_model
                .entry(entry.value().model_id.clone())
                .or_insert(0) += 1;
        }
        SnapshotStats {
            total_snapshots: self.stats.total_snapshots.load(Ordering::Relaxed),
            total_size_bytes: self.stats.total_size_bytes.load(Ordering::Relaxed),
            auto_snapshots: self.stats.auto_snapshots.load(Ordering::Relaxed),
            restored_count: self.stats.restored_count.load(Ordering::Relaxed),
            by_model,
        }
    }

    /// Get current config.
    pub async fn get_config(&self) -> SnapshotConfig {
        self.config.read().await.clone()
    }

    /// Update config.
    pub async fn update_config(&self, update: SnapshotConfigUpdate) -> SnapshotConfig {
        let mut cfg = self.config.write().await;
        if let Some(auto_snapshot) = update.auto_snapshot { cfg.auto_snapshot = auto_snapshot; }
        if let Some(max) = update.max_snapshots_per_model { cfg.max_snapshots_per_model = max; }
        if let Some(interval) = update.snapshot_interval_secs { cfg.snapshot_interval_secs = interval; }
        if let Some(compression) = update.compression { cfg.compression = compression; }
        if let Some(weights) = update.include_weights { cfg.include_weights = weights; }
        if let Some(config) = update.include_config { cfg.include_config = config; }
        if let Some(state) = update.include_state { cfg.include_state = state; }
        if let Some(path) = update.storage_path { cfg.storage_path = path; }
        cfg.clone()
    }
}

/// Format a DateTime as a human-readable relative time.
fn humantime(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now - dt;
    let secs = diff.num_seconds().unsigned_abs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
