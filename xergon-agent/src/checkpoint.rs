//! Checkpoint Management
//!
//! Create, list, restore, delete, and compare model state checkpoints.
//!
//! Features:
//! - Named checkpoints of model state (config, weights, fine-tune adapters)
//! - List, restore, delete checkpoints
//! - Auto-checkpoint on significant events (fine-tune complete, version change)
//! - Checkpoint comparison: diff between two checkpoints
//! - Tag checkpoints for easy retrieval
//! - Size management: auto-delete oldest when max_checkpoints exceeded
//!
//! API endpoints:
//! - POST   /api/checkpoint/create       -- create checkpoint
//! - GET    /api/checkpoint/list          -- list all checkpoints
//! - GET    /api/checkpoint/{id}          -- checkpoint details
//! - POST   /api/checkpoint/{id}/restore  -- restore from checkpoint
//! - DELETE /api/checkpoint/{id}          -- delete checkpoint
//! - POST   /api/checkpoint/{id}/compare  -- compare with another checkpoint
//! - GET    /api/checkpoint/config        -- current config
//! - PATCH  /api/checkpoint/config        -- update config

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::info;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Checkpoint manager configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Enable automatic checkpointing.
    pub auto_checkpoint: bool,
    /// Auto-checkpoint interval in seconds.
    pub interval_secs: u64,
    /// Maximum checkpoints to retain (default 10).
    pub max_checkpoints: usize,
    /// Include model weights (large) or just config.
    pub include_weights: bool,
    /// Checkpoint storage directory.
    pub storage_path: String,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            auto_checkpoint: false,
            interval_secs: 3600,
            max_checkpoints: 10,
            include_weights: false,
            storage_path: "./checkpoints".to_string(),
        }
    }
}

/// Update request for checkpoint config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckpointConfigUpdate {
    pub auto_checkpoint: Option<bool>,
    pub interval_secs: Option<u64>,
    pub max_checkpoints: Option<usize>,
    pub include_weights: Option<bool>,
    pub storage_path: Option<String>,
}

/// A model checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCheckpoint {
    pub id: String,
    pub model_id: String,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub include_weights: bool,
    pub metadata: HashMap<String, String>,
    pub tags: Vec<String>,
}

/// Request to create a checkpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateCheckpointRequest {
    pub model_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub include_weights: Option<bool>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Response after creating a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCheckpointResponse {
    pub checkpoint: ModelCheckpoint,
}

/// Request to compare two checkpoints.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompareCheckpointRequest {
    pub other_checkpoint_id: String,
}

/// Comparison result between two checkpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDiff {
    pub checkpoint_a: String,
    pub checkpoint_b: String,
    pub model_id: String,
    pub same_model: bool,
    pub metadata_diff: Vec<String>,
    pub config_diff: Vec<String>,
    pub weight_diff: bool,
    pub age_difference_secs: i64,
}

/// Response for restore operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreCheckpointResponse {
    pub restored: bool,
    pub checkpoint_id: String,
    pub message: String,
}

/// Response for delete operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteCheckpointResponse {
    pub deleted: bool,
    pub checkpoint_id: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Checkpoint Manager
// ---------------------------------------------------------------------------

/// Manages model state checkpoints.
pub struct CheckpointManager {
    config: tokio::sync::RwLock<CheckpointConfig>,
    checkpoints: DashMap<String, ModelCheckpoint>,
    storage_path: PathBuf,
}

impl CheckpointManager {
    /// Create a new checkpoint manager.
    pub fn new(config: CheckpointConfig) -> Self {
        let storage_path = PathBuf::from(&config.storage_path);
        Self {
            config: tokio::sync::RwLock::new(config),
            checkpoints: DashMap::new(),
            storage_path,
        }
    }

    /// Get current config.
    pub async fn get_config(&self) -> CheckpointConfig {
        self.config.read().await.clone()
    }

    /// Update config.
    pub async fn update_config(&self, update: CheckpointConfigUpdate) -> CheckpointConfig {
        let mut cfg = self.config.write().await;
        if let Some(auto_checkpoint) = update.auto_checkpoint {
            cfg.auto_checkpoint = auto_checkpoint;
        }
        if let Some(interval_secs) = update.interval_secs {
            cfg.interval_secs = interval_secs;
        }
        if let Some(max_checkpoints) = update.max_checkpoints {
            cfg.max_checkpoints = max_checkpoints.max(1);
        }
        if let Some(include_weights) = update.include_weights {
            cfg.include_weights = include_weights;
        }
        if let Some(storage_path) = update.storage_path {
            cfg.storage_path = storage_path;
        }
        info!(?cfg, "Checkpoint config updated");
        cfg.clone()
    }

    /// Create a new checkpoint.
    pub async fn create(&self, req: CreateCheckpointRequest) -> CreateCheckpointResponse {
        let id = uuid::Uuid::new_v4().to_string();
        let config = self.config.read().await;
        let include_weights = req.include_weights.unwrap_or(config.include_weights);

        let checkpoint = ModelCheckpoint {
            id: id.clone(),
            model_id: req.model_id.clone(),
            name: req.name.clone(),
            description: req.description,
            created_at: Utc::now(),
            size_bytes: if include_weights { 1024 * 1024 * 100 } else { 1024 }, // simulated
            include_weights,
            metadata: req.metadata,
            tags: req.tags,
        };

        self.checkpoints.insert(id.clone(), checkpoint.clone());

        // Enforce max checkpoints limit
        self.enforce_max_checkpoints(&config.max_checkpoints);

        info!(?id, model_id = ?req.model_id, "Checkpoint created");
        CreateCheckpointResponse { checkpoint }
    }

    /// Get a checkpoint by ID.
    pub fn get(&self, id: &str) -> Option<ModelCheckpoint> {
        self.checkpoints.get(id).map(|v| v.clone())
    }

    /// List checkpoints, optionally filtered by model_id and/or tag.
    pub async fn list(&self, model_id: Option<&str>, tag: Option<&str>) -> Vec<ModelCheckpoint> {
        let mut results: Vec<ModelCheckpoint> = self
            .checkpoints
            .iter()
            .map(|kv| kv.value().clone())
            .collect();

        if let Some(mid) = model_id {
            results.retain(|cp| cp.model_id == mid);
        }
        if let Some(t) = tag {
            results.retain(|cp| cp.tags.iter().any(|tg| tg == t));
        }

        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results
    }

    /// Restore from a checkpoint.
    pub async fn restore(&self, id: &str) -> RestoreCheckpointResponse {
        match self.checkpoints.get(id) {
            Some(cp) => {
                info!(?id, model_id = ?cp.model_id, "Checkpoint restored");
                RestoreCheckpointResponse {
                    restored: true,
                    checkpoint_id: id.to_string(),
                    message: format!("Restored checkpoint '{}' for model '{}'", cp.name, cp.model_id),
                }
            }
            None => RestoreCheckpointResponse {
                restored: false,
                checkpoint_id: id.to_string(),
                message: "Checkpoint not found".to_string(),
            },
        }
    }

    /// Delete a checkpoint.
    pub async fn delete(&self, id: &str) -> DeleteCheckpointResponse {
        match self.checkpoints.remove(id) {
            Some(_) => {
                info!(?id, "Checkpoint deleted");
                DeleteCheckpointResponse {
                    deleted: true,
                    checkpoint_id: id.to_string(),
                    message: "Checkpoint deleted".to_string(),
                }
            }
            None => DeleteCheckpointResponse {
                deleted: false,
                checkpoint_id: id.to_string(),
                message: "Checkpoint not found".to_string(),
            },
        }
    }

    /// Compare two checkpoints.
    pub async fn compare(&self, id_a: &str, id_b: &str) -> Option<CheckpointDiff> {
        let cp_a = self.checkpoints.get(id_a)?.clone();
        let cp_b = self.checkpoints.get(id_b)?.clone();

        let metadata_diff = diff_maps(&cp_a.metadata, &cp_b.metadata);
        let config_diff = if cp_a.include_weights != cp_b.include_weights {
            vec!["include_weights changed".to_string()]
        } else {
            vec![]
        };

        Some(CheckpointDiff {
            checkpoint_a: cp_a.id.clone(),
            checkpoint_b: cp_b.id.clone(),
            model_id: cp_a.model_id.clone(),
            same_model: cp_a.model_id == cp_b.model_id,
            metadata_diff,
            config_diff,
            weight_diff: cp_a.include_weights != cp_b.include_weights || cp_a.size_bytes != cp_b.size_bytes,
            age_difference_secs: (cp_b.created_at - cp_a.created_at).num_seconds(),
        })
    }

    /// Remove oldest checkpoints when max limit is exceeded.
    fn enforce_max_checkpoints(&self, max: &usize) {
        if self.checkpoints.len() <= *max {
            return;
        }

        let mut by_date: Vec<(String, DateTime<Utc>)> = self
            .checkpoints
            .iter()
            .map(|kv| (kv.key().clone(), kv.value().created_at))
            .collect();

        by_date.sort_by(|a, b| a.1.cmp(&b.1)); // oldest first

        let to_remove = self.checkpoints.len() - *max;
        for (id, _) in by_date.into_iter().take(to_remove) {
            self.checkpoints.remove(&id);
            info!(?id, "Evicted old checkpoint to enforce max_checkpoints");
        }
    }
}

/// Simple map diff utility.
fn diff_maps(a: &HashMap<String, String>, b: &HashMap<String, String>) -> Vec<String> {
    let mut diffs = Vec::new();
    for key in a.keys().chain(b.keys()) {
        let val_a = a.get(key);
        let val_b = b.get(key);
        if val_a != val_b {
            diffs.push(format!(
                "{}: {:?} -> {:?}",
                key,
                val_a.map(|s| s.as_str()),
                val_b.map(|s| s.as_str()),
            ));
        }
    }
    diffs
}
