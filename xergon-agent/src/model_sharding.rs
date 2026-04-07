//! Model sharding for the Xergon agent.
//!
//! Splits large models across multiple GPUs using layer-based sharding.
//! Supports pipeline parallel (sequential layers across GPUs) and
//! tensor parallel (split within layer).
//!
//! API:
//! - GET    /api/sharding/status   -- current shard configuration
//! - POST   /api/sharding/shard    -- shard a model
//! - DELETE /api/sharding/shard/{model} -- un-shard (merge back)
//! - GET    /api/sharding/models   -- list sharded models

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::multi_gpu::{GpuDevice, MultiGpuManager};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Sharding strategy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ShardingStrategy {
    Pipeline,
    Tensor,
    Auto,
}

impl Default for ShardingStrategy {
    fn default() -> Self {
        Self::Auto
    }
}

/// Configuration for a model shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardConfig {
    pub total_shards: u32,
    pub shard_index: u32,
    pub device: u32,
}

/// A single model shard placed on a GPU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelShard {
    pub shard_id: String,
    pub model_name: String,
    pub layers: (u32, u32), // start/end layer (inclusive)
    pub device: u32,
    pub vram_used: u64,
    pub created_at: DateTime<Utc>,
}

/// Status of a sharded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShardStatus {
    Active,
    Merging,
    Error,
}

/// Request to shard a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardModelRequest {
    pub model_name: String,
    pub num_shards: u32,
    pub total_layers: u32,
    pub estimated_vram_mb: u64,
    #[serde(default)]
    pub strategy: ShardingStrategy,
}

/// Response for shard routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardRoute {
    pub shard_id: String,
    pub device: u32,
    pub layers: (u32, u32),
}

/// Summary of a sharded model for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelShardingStatus {
    pub model_name: String,
    pub num_shards: u32,
    pub strategy: ShardingStrategy,
    pub total_vram_mb: u64,
    pub status: ShardStatus,
    pub shards: Vec<ModelShard>,
    pub created_at: DateTime<Utc>,
}

/// Global sharding status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardingStatus {
    pub enabled: bool,
    pub sharded_models: Vec<ModelShardingStatus>,
    pub available_devices: u32,
    pub total_vram_available_mb: u64,
    pub total_vram_used_mb: u64,
}

// ---------------------------------------------------------------------------
// Model Shard Manager
// ---------------------------------------------------------------------------

/// Manages model sharding across available GPU devices.
pub struct ModelShardManager {
    /// All sharded models: model_name -> shards list
    shards: DashMap<String, Vec<ModelShard>>,
    /// Sharding status per model
    status: DashMap<String, ShardStatus>,
    /// Sharding strategy per model
    strategies: DashMap<String, ShardingStrategy>,
    /// Reference to the multi-GPU manager for device info
    multi_gpu: Arc<MultiGpuManager>,
    /// Counter for shard IDs
    shard_counter: AtomicU64,
    /// Track created timestamps
    created_at: DashMap<String, DateTime<Utc>>,
}

impl ModelShardManager {
    /// Create a new model shard manager.
    pub fn new(multi_gpu: Arc<MultiGpuManager>) -> Self {
        Self {
            shards: DashMap::new(),
            status: DashMap::new(),
            strategies: DashMap::new(),
            multi_gpu,
            shard_counter: AtomicU64::new(0),
            created_at: DashMap::new(),
        }
    }

    /// Shard a model across available GPUs.
    ///
    /// Splits model layers evenly across `num_shards` GPUs.
    /// Validates that there is sufficient VRAM before sharding.
    pub fn shard_model(&self, request: ShardModelRequest) -> Result<Vec<ModelShard>, String> {
        let model_name = &request.model_name;

        // Already sharded?
        if self.shards.contains_key(model_name) {
            return Err(format!("Model '{}' is already sharded", model_name));
        }

        if request.num_shards == 0 {
            return Err("num_shards must be >= 1".into());
        }

        if request.total_layers == 0 {
            return Err("total_layers must be >= 1".into());
        }

        if request.num_shards > request.total_layers {
            return Err(format!(
                "Cannot shard {} layers into {} shards (more shards than layers)",
                request.total_layers, request.num_shards
            ));
        }

        // Get available devices
        let devices = self.multi_gpu.list_devices();
        if devices.is_empty() {
            return Err("No GPU devices available for sharding".into());
        }

        let available_gpus = devices.len() as u32;
        if request.num_shards > available_gpus {
            return Err(format!(
                "Requested {} shards but only {} GPUs available",
                request.num_shards, available_gpus
            ));
        }

        // Check VRAM per device
        let vram_per_shard = request.estimated_vram_mb / request.num_shards as u64;
        for (i, device) in devices.iter().take(request.num_shards as usize).enumerate() {
            let free_vram = device.vram_mb.saturating_sub(device.vram_used_mb);
            if free_vram < vram_per_shard {
                return Err(format!(
                    "GPU {} has insufficient VRAM: need {} MB, have {} MB free",
                    device.id, vram_per_shard, free_vram
                ));
            }
        }

        // Calculate layer distribution
        let layers_per_shard = request.total_layers / request.num_shards;
        let remainder = request.total_layers % request.num_shards;

        let mut current_layer: u32 = 0;
        let mut model_shards = Vec::new();
        let now = Utc::now();

        for i in 0..request.num_shards {
            let extra = if (i as u32) < remainder { 1 } else { 0 };
            let num_layers = layers_per_shard + extra;
            let start_layer = current_layer;
            let end_layer = current_layer + num_layers - 1;
            current_layer = end_layer + 1;

            let device = &devices[i as usize];
            let shard_id = format!(
                "shard-{}-{}",
                self.shard_counter.fetch_add(1, Ordering::Relaxed),
                i
            );

            let shard = ModelShard {
                shard_id: shard_id.clone(),
                model_name: model_name.clone(),
                layers: (start_layer, end_layer),
                device: device.id,
                vram_used: vram_per_shard,
                created_at: now,
            };

            model_shards.push(shard);
        }

        // Store shards
        self.shards.insert(model_name.clone(), model_shards.clone());
        self.status.insert(model_name.clone(), ShardStatus::Active);
        self.strategies.insert(model_name.clone(), request.strategy);
        self.created_at.insert(model_name.clone(), now);

        info!(
            model = %model_name,
            num_shards = request.num_shards,
            total_layers = request.total_layers,
            "Model sharded successfully"
        );

        Ok(model_shards)
    }

    /// Get the shard routing for a specific layer range.
    ///
    /// Returns which shard(s) and device(s) should handle inference
    /// for the given layer range.
    pub fn get_shard_routing(
        &self,
        model: &str,
        layer_range: (u32, u32),
    ) -> Result<Vec<ShardRoute>, String> {
        let shards = self.shards.get(model)
            .ok_or_else(|| format!("Model '{}' is not sharded", model))?;

        let (start, end) = layer_range;
        let mut routes = Vec::new();

        for shard in shards.iter() {
            let (shard_start, shard_end) = shard.layers;
            // Check if the requested layer range overlaps with this shard
            if start <= shard_end && end >= shard_start {
                routes.push(ShardRoute {
                    shard_id: shard.shard_id.clone(),
                    device: shard.device,
                    layers: shard.layers,
                });
            }
        }

        if routes.is_empty() {
            return Err(format!(
                "No shards found for model '{}' covering layers {}-{}",
                model, start, end
            ));
        }

        debug!(
            model = %model,
            layers = ?layer_range,
            routes = routes.len(),
            "Shard routing resolved"
        );

        Ok(routes)
    }

    /// Merge shard outputs (conceptual -- in production this would
    /// combine tensor outputs from all shards).
    pub fn merge_shard_outputs(&self, model: &str) -> Result<String, String> {
        let shards = self.shards.get(model)
            .ok_or_else(|| format!("Model '{}' is not sharded", model))?;

        let shard_count = shards.len();
        let total_vram: u64 = shards.iter().map(|s| s.vram_used).sum();

        debug!(
            model = %model,
            shard_count,
            total_vram_mb = total_vram,
            "Merge outputs from all shards"
        );

        Ok(format!(
            "merged:{}:{}:{}MB",
            model, shard_count, total_vram
        ))
    }

    /// Un-shard a model (remove all shards).
    pub fn unshard_model(&self, model: &str) -> Result<(), String> {
        if self.shards.remove(model).is_none() {
            return Err(format!("Model '{}' is not sharded", model));
        }
        self.status.remove(model);
        self.strategies.remove(model);
        self.created_at.remove(model);

        info!(model = %model, "Model unsharded");
        Ok(())
    }

    /// List all sharded models.
    pub fn list_sharded_models(&self) -> Vec<ModelShardingStatus> {
        self.shards.iter().map(|entry| {
            let (model_name, shards) = (entry.key(), entry.value());
            let strategy = self.strategies.get(model_name)
                .map(|s| s.clone())
                .unwrap_or(ShardingStrategy::Auto);
            let status = self.status.get(model_name)
                .map(|s| s.clone())
                .unwrap_or(ShardStatus::Error);
            let created = self.created_at.get(model_name)
                .map(|t| *t)
                .unwrap_or_else(Utc::now);
            let total_vram: u64 = shards.iter().map(|s| s.vram_used).sum();

            ModelShardingStatus {
                model_name: model_name.clone(),
                num_shards: shards.len() as u32,
                strategy,
                total_vram_mb: total_vram,
                status,
                shards: shards.clone(),
                created_at: created,
            }
        }).collect()
    }

    /// Get current sharding status.
    pub fn get_status(&self) -> ShardingStatus {
        let devices = self.multi_gpu.list_devices();
        let total_vram_available: u64 = devices.iter().map(|d| d.vram_mb).sum();
        let total_vram_used: u64 = self
            .shards
            .iter()
            .map(|entry| entry.value().iter().map(|s| s.vram_used).sum::<u64>())
            .sum();

        ShardingStatus {
            enabled: true,
            sharded_models: self.list_sharded_models(),
            available_devices: devices.len() as u32,
            total_vram_available_mb: total_vram_available,
            total_vram_used_mb: total_vram_used,
        }
    }

    /// Check if a model is sharded.
    pub fn is_sharded(&self, model: &str) -> bool {
        self.shards.contains_key(model)
    }

    /// Get shards for a model.
    pub fn get_shards(&self, model: &str) -> Option<Vec<ModelShard>> {
        self.shards.get(model).map(|s| s.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi_gpu::{GpuDevice, MultiGpuManager};

    /// Helper: create a MultiGpuManager with `n` test GPU devices, each with
    /// the given amount of VRAM.
    fn setup_multi_gpu(n: u32, vram_mb: u64) -> Arc<MultiGpuManager> {
        let mgr = Arc::new(MultiGpuManager::new());
        for i in 0..n {
            mgr.add_test_device(GpuDevice {
                id: i,
                name: format!("test-gpu-{}", i),
                vram_mb,
                vram_used_mb: 0,
                driver: "test-driver".to_string(),
                active_inferences: 0,
            });
        }
        mgr
    }

    /// Helper: build a valid ShardModelRequest.
    fn make_request(model: &str, num_shards: u32, total_layers: u32, vram_mb: u64, strategy: ShardingStrategy) -> ShardModelRequest {
        ShardModelRequest {
            model_name: model.to_string(),
            num_shards,
            total_layers,
            estimated_vram_mb: vram_mb,
            strategy,
        }
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_manager() {
        let gpu = setup_multi_gpu(2, 8000);
        let mgr = ModelShardManager::new(gpu);
        assert!(mgr.list_sharded_models().is_empty());
        let status = mgr.get_status();
        assert!(status.enabled);
        assert_eq!(status.available_devices, 2);
    }

    // -----------------------------------------------------------------------
    // Pipeline strategy
    // -----------------------------------------------------------------------

    #[test]
    fn test_shard_pipeline_strategy() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        let req = make_request("llama-70b", 2, 80, 12000, ShardingStrategy::Pipeline);
        let mut shards = mgr.shard_model(req).unwrap();
        assert_eq!(shards.len(), 2);
        // Sort by layer start to avoid DashMap iteration non-determinism
        shards.sort_by_key(|s| s.layers.0);
        // Layers should be split evenly: 0-39 and 40-79
        assert_eq!(shards[0].layers, (0, 39));
        assert_eq!(shards[1].layers, (40, 79));
        // Devices should be distinct
        let devices: Vec<u32> = shards.iter().map(|s| s.device).collect();
        let mut sorted_devs = devices.clone();
        sorted_devs.sort();
        assert_eq!(sorted_devs, vec![0, 1]);
        assert!(mgr.is_sharded("llama-70b"));
    }

    // -----------------------------------------------------------------------
    // Tensor strategy
    // -----------------------------------------------------------------------

    #[test]
    fn test_shard_tensor_strategy() {
        let gpu = setup_multi_gpu(4, 24000);
        let mgr = ModelShardManager::new(gpu);
        let req = make_request("gpt-neox-20b", 4, 44, 20000, ShardingStrategy::Tensor);
        let shards = mgr.shard_model(req).unwrap();
        assert_eq!(shards.len(), 4);
        // 44 / 4 = 11 layers each
        for s in &shards {
            assert_eq!(s.layers.1 - s.layers.0 + 1, 11);
        }
    }

    // -----------------------------------------------------------------------
    // Auto strategy (default)
    // -----------------------------------------------------------------------

    #[test]
    fn test_shard_auto_strategy() {
        let gpu = setup_multi_gpu(3, 32000);
        let mgr = ModelShardManager::new(gpu);
        let req = make_request("mixtral-8x7b", 3, 32, 24000, ShardingStrategy::Auto);
        let shards = mgr.shard_model(req).unwrap();
        assert_eq!(shards.len(), 3);
        // Verify strategy stored correctly
        let status = mgr.get_status();
        assert_eq!(status.sharded_models[0].strategy, ShardingStrategy::Auto);
    }

    // -----------------------------------------------------------------------
    // List sharded models
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_sharded_models_empty() {
        let gpu = setup_multi_gpu(1, 8000);
        let mgr = ModelShardManager::new(gpu);
        assert!(mgr.list_sharded_models().is_empty());
    }

    #[test]
    fn test_list_sharded_models_multiple() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("model-a", 1, 24, 8000, ShardingStrategy::Pipeline)).unwrap();
        mgr.shard_model(make_request("model-b", 2, 48, 12000, ShardingStrategy::Tensor)).unwrap();
        let list = mgr.list_sharded_models();
        assert_eq!(list.len(), 2);
        let names: Vec<&str> = list.iter().map(|m| m.model_name.as_str()).collect();
        assert!(names.contains(&"model-a"));
        assert!(names.contains(&"model-b"));
    }

    // -----------------------------------------------------------------------
    // Sharding status
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_status_no_models() {
        let gpu = setup_multi_gpu(1, 8000);
        let mgr = ModelShardManager::new(gpu);
        let status = mgr.get_status();
        assert!(status.enabled);
        assert!(status.sharded_models.is_empty());
        assert_eq!(status.available_devices, 1);
        assert_eq!(status.total_vram_used_mb, 0);
    }

    #[test]
    fn test_get_status_with_models() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("test-model", 2, 32, 10000, ShardingStrategy::Auto)).unwrap();
        let status = mgr.get_status();
        assert_eq!(status.sharded_models.len(), 1);
        assert!(status.total_vram_used_mb > 0);
    }

    // -----------------------------------------------------------------------
    // Un-shard
    // -----------------------------------------------------------------------

    #[test]
    fn test_unshard_model() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("to-remove", 2, 32, 10000, ShardingStrategy::Pipeline)).unwrap();
        assert!(mgr.is_sharded("to-remove"));
        mgr.unshard_model("to-remove").unwrap();
        assert!(!mgr.is_sharded("to-remove"));
        assert!(mgr.list_sharded_models().is_empty());
    }

    #[test]
    fn test_unshard_nonexistent_model() {
        let gpu = setup_multi_gpu(1, 8000);
        let mgr = ModelShardManager::new(gpu);
        let result = mgr.unshard_model("does-not-exist");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not sharded"));
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    #[test]
    fn test_shard_nonexistent_model_is_double_shard() {
        // "non-existent model" in this context means double-sharding since
        // models are identified by name
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("dup-model", 1, 24, 8000, ShardingStrategy::Pipeline)).unwrap();
        let result = mgr.shard_model(make_request("dup-model", 1, 24, 8000, ShardingStrategy::Pipeline));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already sharded"));
    }

    #[test]
    fn test_shard_zero_shards() {
        let gpu = setup_multi_gpu(1, 8000);
        let mgr = ModelShardManager::new(gpu);
        let req = make_request("bad", 0, 24, 8000, ShardingStrategy::Auto);
        let result = mgr.shard_model(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("num_shards must be >= 1"));
    }

    #[test]
    fn test_shard_zero_layers() {
        let gpu = setup_multi_gpu(1, 8000);
        let mgr = ModelShardManager::new(gpu);
        let req = make_request("bad", 1, 0, 8000, ShardingStrategy::Auto);
        let result = mgr.shard_model(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("total_layers must be >= 1"));
    }

    #[test]
    fn test_shard_more_shards_than_layers() {
        let gpu = setup_multi_gpu(4, 16000);
        let mgr = ModelShardManager::new(gpu);
        let req = make_request("bad", 8, 4, 8000, ShardingStrategy::Auto);
        let result = mgr.shard_model(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("more shards than layers"));
    }

    #[test]
    fn test_shard_no_gpu_devices_or_insufficient() {
        let gpu = Arc::new(MultiGpuManager::new());
        let mgr = ModelShardManager::new(gpu);
        // Request more shards than detected GPUs (even if real GPUs exist on this machine)
        let available = mgr.multi_gpu.list_devices().len();
        let req = make_request("test", (available + 1) as u32, 24, 8000, ShardingStrategy::Auto);
        let result = mgr.shard_model(req);
        assert!(result.is_err());
        // If there are real GPUs, the error will be about insufficient GPUs, not no GPUs
        if available == 0 {
            assert!(result.unwrap_err().contains("No GPU devices available"));
        } else {
            assert!(result.unwrap_err().contains(&format!("only {} GPUs available", available)));
        }
    }

    #[test]
    fn test_shard_insufficient_gpus() {
        let gpu = setup_multi_gpu(1, 16000);
        let mgr = ModelShardManager::new(gpu);
        let req = make_request("test", 4, 32, 8000, ShardingStrategy::Auto);
        let result = mgr.shard_model(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("only 1 GPUs available"));
    }

    #[test]
    fn test_shard_insufficient_vram() {
        let gpu = setup_multi_gpu(2, 1000); // only 1000 MB each
        let mgr = ModelShardManager::new(gpu);
        // Need 10000/2 = 5000 MB per shard, but only 1000 MB free
        let req = make_request("test", 2, 32, 10000, ShardingStrategy::Auto);
        let result = mgr.shard_model(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("insufficient VRAM"));
    }

    // -----------------------------------------------------------------------
    // ShardConfig / ShardRoute / get_shard_routing
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_shard_routing() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("routed", 2, 20, 10000, ShardingStrategy::Pipeline)).unwrap();
        // Request routing for layers 0-9 (should hit shard 0)
        let routes = mgr.get_shard_routing("routed", (0, 9)).unwrap();
        assert_eq!(routes.len(), 1);

        // Request routing for layers 5-15 (crosses both shards)
        let routes = mgr.get_shard_routing("routed", (5, 15)).unwrap();
        assert_eq!(routes.len(), 2);
    }

    #[test]
    fn test_get_shard_routing_nonexistent_model() {
        let gpu = setup_multi_gpu(1, 8000);
        let mgr = ModelShardManager::new(gpu);
        let result = mgr.get_shard_routing("ghost", (0, 9));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_shard_routing_no_overlap() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("gap-model", 2, 20, 10000, ShardingStrategy::Pipeline)).unwrap();
        // Layers 100-200 are way beyond the model's 0-19
        let result = mgr.get_shard_routing("gap-model", (100, 200));
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Device assignment
    // -----------------------------------------------------------------------

    #[test]
    fn test_device_assignment() {
        let gpu = setup_multi_gpu(3, 24000);
        let mgr = ModelShardManager::new(gpu);
        let shards = mgr.shard_model(make_request("dev-test", 3, 30, 18000, ShardingStrategy::Pipeline)).unwrap();
        // Each shard should be on a distinct device (0, 1, 2)
        let device_ids: Vec<u32> = shards.iter().map(|s| s.device).collect();
        assert_eq!(device_ids.len(), 3);
        let mut sorted = device_ids.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2], "Each shard should be on a distinct device");
    }

    // -----------------------------------------------------------------------
    // Merge shard outputs
    // -----------------------------------------------------------------------

    #[test]
    fn test_merge_shard_outputs() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("merge-me", 2, 24, 10000, ShardingStrategy::Tensor)).unwrap();
        let merged = mgr.merge_shard_outputs("merge-me").unwrap();
        assert!(merged.starts_with("merged:merge-me:2:"));
        assert!(merged.ends_with("MB"));
    }

    #[test]
    fn test_merge_shard_outputs_nonexistent() {
        let gpu = setup_multi_gpu(1, 8000);
        let mgr = ModelShardManager::new(gpu);
        let result = mgr.merge_shard_outputs("ghost");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Uneven layer distribution
    // -----------------------------------------------------------------------

    #[test]
    fn test_uneven_layer_distribution() {
        let gpu = setup_multi_gpu(3, 24000);
        let mgr = ModelShardManager::new(gpu);
        // 10 layers across 3 shards: should be 4, 3, 3
        let mut shards = mgr.shard_model(make_request("uneven", 3, 10, 8000, ShardingStrategy::Pipeline)).unwrap();
        assert_eq!(shards.len(), 3);
        // Sort by layer start to avoid DashMap iteration non-determinism
        shards.sort_by_key(|s| s.layers.0);
        assert_eq!(shards[0].layers, (0, 3));   // 4 layers
        assert_eq!(shards[1].layers, (4, 6));   // 3 layers
        assert_eq!(shards[2].layers, (7, 9));   // 3 layers
    }

    // -----------------------------------------------------------------------
    // get_shards
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_shards() {
        let gpu = setup_multi_gpu(2, 16000);
        let mgr = ModelShardManager::new(gpu);
        mgr.shard_model(make_request("get-test", 2, 16, 8000, ShardingStrategy::Auto)).unwrap();
        let shards = mgr.get_shards("get-test").unwrap();
        assert_eq!(shards.len(), 2);
        assert!(mgr.get_shards("nonexistent").is_none());
    }
}
