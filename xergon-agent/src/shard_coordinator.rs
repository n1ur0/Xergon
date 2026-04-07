//! Distributed Model Serving v2 — Shard Coordinator for the Xergon Network agent.
//!
//! Handles shard coordination, tensor pipeline management, and cross-provider
//! inference merging. Shards are independent compute slices that each serve a
//! contiguous range of transformer layers. A *pipeline* chains one or more
//! shards to serve a complete model, and the coordinator merges partial
//! outputs into a final result using configurable strategies.
//!
//! This is a pure service module (no axum routes).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Merge strategy used when combining shard outputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Concatenate shard outputs in layer order.
    Concatenate,
    /// Weighted blend using shard_weight and confidence.
    WeightedBlend,
    /// Pick the highest-confidence shard output.
    SmartSelect,
}

impl Default for MergeStrategy {
    fn default() -> Self {
        Self::WeightedBlend
    }
}

/// Global shard-coordinator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardConfig {
    /// Number of shards per pipeline (default 4).
    #[serde(default = "default_shard_size")]
    pub shard_size: u32,
    /// Enable pipeline-parallel execution.
    #[serde(default)]
    pub pipeline_parallel: bool,
    /// Overlap in tokens between adjacent shards.
    #[serde(default)]
    pub overlap_tokens: u32,
    /// Strategy for merging shard results.
    #[serde(default)]
    pub merge_strategy: MergeStrategy,
    /// Timeout per shard in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u32,
    /// Number of retries on shard failure.
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
}

fn default_shard_size() -> u32 {
    4
}
fn default_timeout_ms() -> u32 {
    30_000
}
fn default_retry_count() -> u32 {
    3
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            shard_size: default_shard_size(),
            pipeline_parallel: false,
            overlap_tokens: 0,
            merge_strategy: MergeStrategy::default(),
            timeout_ms: default_timeout_ms(),
            retry_count: default_retry_count(),
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Status of a single shard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ShardStatus {
    Active,
    Inactive,
    Overloaded,
    Failed,
}

/// A single compute shard that serves a contiguous range of transformer layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shard {
    pub id: String,
    pub model_id: String,
    pub provider_id: String,
    pub start_layer: u32,
    pub end_layer: u32,
    pub status: ShardStatus,
    /// Health score in [0, 1].
    pub health_score: f64,
    pub latency_ms: u64,
    /// Weight used by WeightedBlend merge strategy.
    pub shard_weight: f64,
}

impl Shard {
    /// Create a new shard with sensible defaults.
    pub fn new(
        id: String,
        model_id: String,
        provider_id: String,
        start_layer: u32,
        end_layer: u32,
    ) -> Self {
        Self {
            id,
            model_id,
            provider_id,
            start_layer,
            end_layer,
            status: ShardStatus::Active,
            health_score: 1.0,
            latency_ms: 0,
            shard_weight: 1.0,
        }
    }

    /// Number of layers this shard covers.
    pub fn layer_count(&self) -> u32 {
        self.end_layer.saturating_sub(self.start_layer) + 1
    }
}

/// Status of a tensor pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    Pending,
    Active,
    Completed,
    Failed,
}

/// A pipeline chains one or more shards to serve a complete model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorPipeline {
    pub pipeline_id: String,
    pub model_id: String,
    /// Ordered shard IDs.
    pub shards: Vec<String>,
    pub status: PipelineStatus,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_latency_ms: u64,
    /// Creation timestamp as unix milliseconds.
    pub created_at: u64,
}

/// Request priority levels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Inference parameters carried alongside the prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceParameters {
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
}

fn default_temperature() -> f64 {
    0.7
}
fn default_max_tokens() -> u32 {
    256
}
fn default_top_p() -> f64 {
    0.9
}

impl Default for InferenceParameters {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            top_p: default_top_p(),
        }
    }
}

/// An incoming inference request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: String,
    pub model_id: String,
    pub prompt: String,
    #[serde(default)]
    pub parameters: InferenceParameters,
    #[serde(default)]
    pub priority: Priority,
}

/// Partial result from a single shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardResult {
    pub shard_id: String,
    pub output_chunk: String,
    pub token_count: u32,
    pub latency_ms: u64,
    /// Confidence in [0, 1].
    pub confidence: f64,
    pub error: Option<String>,
}

/// The final merged result after combining all shard outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedResult {
    pub request_id: String,
    pub final_output: String,
    pub total_tokens: u32,
    pub total_latency_ms: u64,
    pub merge_strategy_used: MergeStrategy,
    pub shard_results: Vec<ShardResult>,
    /// Quality score in [0, 1].
    pub quality_score: f64,
}

// ---------------------------------------------------------------------------
// Coordinator stats / reports
// ---------------------------------------------------------------------------

/// Aggregated coordinator statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorStats {
    pub total_shards: usize,
    pub active_shards: usize,
    pub total_pipelines: usize,
    pub active_pipelines: usize,
    pub completed_pipelines: usize,
    pub failed_pipelines: usize,
    pub total_requests_routed: u64,
    pub average_latency_ms: f64,
}

/// Per-shard health entry for the health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardHealthEntry {
    pub shard_id: String,
    pub model_id: String,
    pub status: ShardStatus,
    pub health_score: f64,
    pub latency_ms: u64,
}

/// Overall health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub overall_health: f64,
    pub shard_count: usize,
    pub healthy_shards: usize,
    pub degraded_shards: usize,
    pub failed_shards: usize,
    pub shards: Vec<ShardHealthEntry>,
}

/// Historical pipeline entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineHistoryEntry {
    pub pipeline_id: String,
    pub model_id: String,
    pub status: PipelineStatus,
    pub total_latency_ms: u64,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// Shard Coordinator
// ---------------------------------------------------------------------------

/// Main coordinator that manages shards, pipelines, and inference merging.
pub struct ShardCoordinator {
    /// All registered shards keyed by shard id.
    shards: DashMap<String, Shard>,
    /// All pipelines keyed by pipeline id.
    pipelines: DashMap<String, TensorPipeline>,
    /// Recent merged results keyed by request id.
    results: DashMap<String, MergedResult>,
    /// Mutable configuration.
    config: Arc<StdRwLock<ShardConfig>>,
    /// Monotonic counters.
    shard_counter: AtomicU64,
    pipeline_counter: AtomicU64,
    request_counter: AtomicU64,
    total_requests_routed: AtomicU64,
    cumulative_latency_ms: AtomicU64,
}

impl ShardCoordinator {
    /// Create a new coordinator with the given configuration.
    pub fn new(config: ShardConfig) -> Self {
        Self {
            shards: DashMap::new(),
            pipelines: DashMap::new(),
            results: DashMap::new(),
            config: Arc::new(StdRwLock::new(config)),
            shard_counter: AtomicU64::new(0),
            pipeline_counter: AtomicU64::new(0),
            request_counter: AtomicU64::new(0),
            total_requests_routed: AtomicU64::new(0),
            cumulative_latency_ms: AtomicU64::new(0),
        }
    }

    /// Create a coordinator with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(ShardConfig::default())
    }

    /// Read the current configuration (snapshot).
    pub fn get_config(&self) -> ShardConfig {
        self.config.read().unwrap().clone()
    }

    /// Update the configuration.
    pub fn update_config(&self, new_config: ShardConfig) {
        *self.config.write().unwrap() = new_config;
        info!("ShardCoordinator config updated");
    }

    // -----------------------------------------------------------------------
    // Shard CRUD
    // -----------------------------------------------------------------------

    /// Register a new shard. Returns the assigned shard id.
    pub fn register_shard(
        &self,
        model_id: String,
        provider_id: String,
        start_layer: u32,
        end_layer: u32,
    ) -> Shard {
        let id = self.next_shard_id();
        let shard = Shard::new(id.clone(), model_id, provider_id, start_layer, end_layer);
        let snapshot = shard.clone();
        self.shards.insert(id, shard);
        info!(shard_id = %snapshot.id, "Shard registered");
        snapshot
    }

    /// Unregister a shard by id. Returns true if it existed.
    pub fn unregister_shard(&self, shard_id: &str) -> bool {
        let removed = self.shards.remove(shard_id).is_some();
        if removed {
            info!(%shard_id, "Shard unregistered");
        }
        removed
    }

    /// Get a shard by id.
    pub fn get_shard(&self, shard_id: &str) -> Option<Shard> {
        self.shards.get(shard_id).map(|r| r.value().clone())
    }

    /// List all shards.
    pub fn list_shards(&self) -> Vec<Shard> {
        self.shards.iter().map(|r| r.value().clone()).collect()
    }

    /// List shards filtered by model id.
    pub fn list_shards_by_model(&self, model_id: &str) -> Vec<Shard> {
        self.shards
            .iter()
            .filter(|r| r.value().model_id == model_id)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Update the health score and latency of a shard.
    pub fn update_shard_health(
        &self,
        shard_id: &str,
        health_score: f64,
        latency_ms: u64,
    ) -> bool {
        if let Some(mut entry) = self.shards.get_mut(shard_id) {
            let shard = entry.value_mut();
            shard.health_score = health_score.clamp(0.0, 1.0);
            shard.latency_ms = latency_ms;
            // Derive status from health.
            if health_score < 0.3 {
                shard.status = ShardStatus::Failed;
            } else if health_score < 0.6 {
                shard.status = ShardStatus::Overloaded;
            } else if health_score < 0.8 {
                shard.status = ShardStatus::Inactive;
            } else {
                shard.status = ShardStatus::Active;
            }
            debug!(
                shard_id = %shard_id,
                health = health_score,
                latency = latency_ms,
                status = ?shard.status,
                "Shard health updated"
            );
            true
        } else {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Pipeline management
    // -----------------------------------------------------------------------

    /// Create a pipeline for a model, automatically assigning shards.
    ///
    /// Layers are divided evenly across the configured shard_size. If fewer
    /// active shards are available, all available ones are used.
    pub fn create_pipeline(
        &self,
        model_id: &str,
        total_layers: u32,
        input_tokens: u32,
    ) -> Result<TensorPipeline, String> {
        let shard_size = {
            // Read config synchronously. We use try_read for non-async context.
            // Since this is called from sync code, we do a blocking read via
            // block_in_place when inside an async runtime, or directly otherwise.
            // For simplicity we spawn a quick tokio block.
            let cfg = self.blocking_config();
            cfg.shard_size
        };

        let available: Vec<Shard> = self
            .shards
            .iter()
            .filter(|r| {
                let s = r.value();
                s.model_id == model_id && s.status == ShardStatus::Active
            })
            .map(|r| r.value().clone())
            .collect();

        if available.is_empty() {
            return Err(format!("No active shards available for model {}", model_id));
        }

        let num_shards = shard_size.min(available.len() as u32) as usize;
        let layers_per_shard = (total_layers as f64 / num_shards as f64).ceil() as u32;

        let pipeline_id = self.next_pipeline_id();
        let mut shard_ids = Vec::with_capacity(num_shards);

        for (i, shard) in available.iter().take(num_shards).enumerate() {
            let start = i as u32 * layers_per_shard;
            let end = ((i as u32 + 1) * layers_per_shard).min(total_layers).max(start);
            // Update the shard's layer range.
            if let Some(mut entry) = self.shards.get_mut(&shard.id) {
                entry.value_mut().start_layer = start;
                entry.value_mut().end_layer = end;
            }
            shard_ids.push(shard.id.clone());
        }

        let now = Utc::now().timestamp_millis() as u64;
        let pipeline = TensorPipeline {
            pipeline_id: pipeline_id.clone(),
            model_id: model_id.to_string(),
            shards: shard_ids,
            status: PipelineStatus::Active,
            input_tokens,
            output_tokens: 0,
            total_latency_ms: 0,
            created_at: now,
        };

        self.pipelines.insert(pipeline_id.clone(), pipeline.clone());
        info!(
            pipeline_id = %pipeline_id,
            model_id = %model_id,
            num_shards,
            "Pipeline created"
        );
        Ok(pipeline)
    }

    /// Execute a pipeline: simulate shard-by-shard inference and collect results.
    ///
    /// Each shard processes the input sequentially (pipeline parallel) and
    /// produces a partial output. Failures are retried up to `retry_count`.
    pub fn execute_pipeline(
        &self,
        pipeline_id: &str,
        request: &InferenceRequest,
    ) -> Result<Vec<ShardResult>, String> {
        let pipeline = self
            .pipelines
            .get(pipeline_id)
            .ok_or_else(|| format!("Pipeline {} not found", pipeline_id))?;

        if pipeline.status != PipelineStatus::Active {
            return Err(format!("Pipeline {} is not active (status: {:?})", pipeline_id, pipeline.status));
        }

        let shard_ids = pipeline.shards.clone();
        let cfg = self.blocking_config();
        let retry_count = cfg.retry_count;
        let timeout_ms = cfg.timeout_ms;
        drop(pipeline);

        let mut results = Vec::with_capacity(shard_ids.len());
        let mut total_latency: u64 = 0;

        for shard_id in &shard_ids {
            let mut attempts = 0u32;
            let mut result: Option<ShardResult> = None;

            while attempts <= retry_count {
                attempts += 1;
                if let Some(shard) = self.shards.get(shard_id) {
                    let simulated_latency = shard.latency_ms.max(10) + 5;
                    // Simulate timeout check.
                    if simulated_latency > timeout_ms as u64 {
                        result = Some(ShardResult {
                            shard_id: shard_id.clone(),
                            output_chunk: String::new(),
                            token_count: 0,
                            latency_ms: simulated_latency,
                            confidence: 0.0,
                            error: Some(format!("Shard {} timed out ({}ms > {}ms)", shard_id, simulated_latency, timeout_ms)),
                        });
                        warn!(%shard_id, "Shard timed out on attempt {}", attempts);
                        continue;
                    }

                    let token_count = request.parameters.max_tokens / shard_ids.len() as u32;
                    // Simulate output based on shard layer position.
                    let _layer_pos = shard.start_layer as usize;
                    let chunk = format!(
                        "[shard:{} layers:{}-{} prompt_len:{} tokens:{}]",
                        shard_id,
                        shard.start_layer,
                        shard.end_layer,
                        request.prompt.len(),
                        token_count
                    );

                    result = Some(ShardResult {
                        shard_id: shard_id.clone(),
                        output_chunk: chunk,
                        token_count,
                        latency_ms: simulated_latency,
                        confidence: shard.health_score,
                        error: None,
                    });
                    break; // success
                } else {
                    result = Some(ShardResult {
                        shard_id: shard_id.clone(),
                        output_chunk: String::new(),
                        token_count: 0,
                        latency_ms: 0,
                        confidence: 0.0,
                        error: Some(format!("Shard {} not found", shard_id)),
                    });
                    break;
                }
            }

            if let Some(r) = result {
                total_latency += r.latency_ms;
                results.push(r);
            }
        }

        // Update pipeline status and latency.
        if let Some(mut pipeline) = self.pipelines.get_mut(pipeline_id) {
            let p = pipeline.value_mut();
            let _has_error = results.iter().any(|r| r.error.is_some());
            let all_failed = results.iter().all(|r| r.error.is_some());
            if all_failed {
                p.status = PipelineStatus::Failed;
            } else {
                p.status = PipelineStatus::Completed;
            }
            p.total_latency_ms = total_latency;
            p.output_tokens = results.iter().map(|r| r.token_count).sum();
        }

        Ok(results)
    }

    /// Merge shard results using the configured strategy.
    pub fn merge_results(
        &self,
        request_id: &str,
        shard_results: &[ShardResult],
        strategy: Option<MergeStrategy>,
    ) -> MergedResult {
        let strategy = strategy.unwrap_or_else(|| {
            self.blocking_config().merge_strategy.clone()
        });

        let final_output = match strategy {
            MergeStrategy::Concatenate => self.merge_concatenate(shard_results),
            MergeStrategy::WeightedBlend => self.merge_weighted_blend(shard_results),
            MergeStrategy::SmartSelect => self.merge_smart_select(shard_results),
        };

        let total_tokens: u32 = shard_results.iter().map(|r| r.token_count).sum();
        let total_latency_ms: u64 = shard_results.iter().map(|r| r.latency_ms).sum();
        let quality_score = self.compute_quality_score(shard_results);

        let merged = MergedResult {
            request_id: request_id.to_string(),
            final_output,
            total_tokens,
            total_latency_ms,
            merge_strategy_used: strategy,
            shard_results: shard_results.to_vec(),
            quality_score,
        };

        // Store in recent results history.
        self.results.insert(request_id.to_string(), merged.clone());

        info!(
            request_id = %request_id,
            strategy = ?merged.merge_strategy_used,
            quality = merged.quality_score,
            "Results merged"
        );
        merged
    }

    // -----------------------------------------------------------------------
    // Request routing
    // -----------------------------------------------------------------------

    /// Route an inference request: find the best pipeline, execute, and merge.
    pub fn route_request(&self, request: InferenceRequest) -> Result<MergedResult, String> {
        self.total_requests_routed.fetch_add(1, Ordering::Relaxed);

        // Find best active pipeline for the model.
        let best_pipeline = self
            .pipelines
            .iter()
            .filter(|r| {
                let p = r.value();
                p.model_id == request.model_id && p.status == PipelineStatus::Active
            })
            .min_by_key(|r| r.value().total_latency_ms);

        let pipeline_id = match best_pipeline {
            Some(entry) => entry.key().clone(),
            None => {
                // Auto-create a pipeline with 32 layers as a default.
                let p = self.create_pipeline(&request.model_id, 32, request.prompt.len() as u32)?;
                p.pipeline_id
            }
        };

        let shard_results = self.execute_pipeline(&pipeline_id, &request)?;
        let merged = self.merge_results(&request.request_id, &shard_results, None);

        self.cumulative_latency_ms
            .fetch_add(merged.total_latency_ms, Ordering::Relaxed);

        Ok(merged)
    }

    // -----------------------------------------------------------------------
    // Stats / reports
    // -----------------------------------------------------------------------

    /// Get aggregated coordinator statistics.
    pub fn get_stats(&self) -> CoordinatorStats {
        let total_shards = self.shards.len();
        let active_shards = self
            .shards
            .iter()
            .filter(|r| r.value().status == ShardStatus::Active)
            .count();
        let total_pipelines = self.pipelines.len();
        let active_pipelines = self
            .pipelines
            .iter()
            .filter(|r| r.value().status == PipelineStatus::Active)
            .count();
        let completed_pipelines = self
            .pipelines
            .iter()
            .filter(|r| r.value().status == PipelineStatus::Completed)
            .count();
        let failed_pipelines = self
            .pipelines
            .iter()
            .filter(|r| r.value().status == PipelineStatus::Failed)
            .count();
        let routed = self.total_requests_routed.load(Ordering::Relaxed);
        let cum_lat = self.cumulative_latency_ms.load(Ordering::Relaxed);
        let avg_latency = if routed > 0 {
            cum_lat as f64 / routed as f64
        } else {
            0.0
        };

        CoordinatorStats {
            total_shards,
            active_shards,
            total_pipelines,
            active_pipelines,
            completed_pipelines,
            failed_pipelines,
            total_requests_routed: routed,
            average_latency_ms: avg_latency,
        }
    }

    /// Get a health report across all shards.
    pub fn get_health_report(&self) -> HealthReport {
        let mut entries: Vec<ShardHealthEntry> = self
            .shards
            .iter()
            .map(|r| {
                let s = r.value();
                ShardHealthEntry {
                    shard_id: s.id.clone(),
                    model_id: s.model_id.clone(),
                    status: s.status.clone(),
                    health_score: s.health_score,
                    latency_ms: s.latency_ms,
                }
            })
            .collect();

        entries.sort_by(|a, b| a.health_score.partial_cmp(&b.health_score).unwrap_or(std::cmp::Ordering::Equal).reverse());

        let shard_count = entries.len();
        let healthy = entries.iter().filter(|e| e.status == ShardStatus::Active).count();
        let failed = entries.iter().filter(|e| e.status == ShardStatus::Failed).count();
        let degraded = shard_count - healthy - failed;

        let overall = if shard_count > 0 {
            entries.iter().map(|e| e.health_score).sum::<f64>() / shard_count as f64
        } else {
            0.0
        };

        HealthReport {
            overall_health: overall,
            shard_count,
            healthy_shards: healthy,
            degraded_shards: degraded,
            failed_shards: failed,
            shards: entries,
        }
    }

    /// Get recent pipeline history (all pipelines in the map).
    pub fn get_pipeline_history(&self) -> Vec<PipelineHistoryEntry> {
        self.pipelines
            .iter()
            .map(|r| {
                let p = r.value();
                PipelineHistoryEntry {
                    pipeline_id: p.pipeline_id.clone(),
                    model_id: p.model_id.clone(),
                    status: p.status.clone(),
                    total_latency_ms: p.total_latency_ms,
                    input_tokens: p.input_tokens,
                    output_tokens: p.output_tokens,
                    created_at: p.created_at,
                }
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Auto-scaling
    // -----------------------------------------------------------------------

    /// Auto-scale shards based on current load.
    ///
    /// If more than `overloaded_threshold` fraction of shards are overloaded
    /// or failed, new shards are added. If fewer than `underloaded_threshold`
    /// are active and load is low, excess inactive shards are removed.
    ///
    /// Returns the number of shards added and removed.
    pub fn auto_scale(&self) -> (u32, u32) {
        let total = self.shards.len();
        if total == 0 {
            return (0, 0);
        }

        let overloaded_or_failed = self
            .shards
            .iter()
            .filter(|r| {
                matches!(
                    r.value().status,
                    ShardStatus::Overloaded | ShardStatus::Failed
                )
            })
            .count();

        let active = self
            .shards
            .iter()
            .filter(|r| r.value().status == ShardStatus::Active)
            .count();

        let overload_ratio = overloaded_or_failed as f64 / total as f64;
        let mut added: u32 = 0;
        let mut removed: u32 = 0;

        // Add shards if more than 50% are overloaded/failed.
        if overload_ratio > 0.5 {
            let to_add = ((total as f64 * 0.25).ceil()) as u32;
            // Determine a model_id from existing shards.
            let model_id = self
                .shards
                .iter()
                .next()
                .map(|r| r.value().model_id.clone())
                .unwrap_or_else(|| "default-model".to_string());

            for i in 0..to_add {
                let provider_id = format!("auto-provider-{}", i);
                let start = 0u32;
                let end = 8u32;
                self.register_shard(
                    model_id.clone(),
                    provider_id,
                    start,
                    end,
                );
                added += 1;
            }
            info!(added, "Auto-scaled: added shards");
        }

        // Remove inactive shards if load is low and we have excess.
        if overload_ratio < 0.2 && active < total / 2 {
            let inactive_ids: Vec<String> = self
                .shards
                .iter()
                .filter(|r| r.value().status == ShardStatus::Inactive)
                .map(|r| r.key().clone())
                .collect();

            let to_remove = (inactive_ids.len() as f64 * 0.5).ceil() as usize;
            for id in inactive_ids.iter().take(to_remove) {
                if self.unregister_shard(id) {
                    removed += 1;
                }
            }
            if removed > 0 {
                info!(removed, "Auto-scaled: removed inactive shards");
            }
        }

        (added, removed)
    }

    // -----------------------------------------------------------------------
    // Merge strategies (private)
    // -----------------------------------------------------------------------

    /// Concatenate all non-error shard outputs in order.
    fn merge_concatenate(&self, results: &[ShardResult]) -> String {
        results
            .iter()
            .filter(|r| r.error.is_none())
            .map(|r| r.output_chunk.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Weighted blend: each shard output is weighted by (shard_weight * confidence).
    fn merge_weighted_blend(&self, results: &[ShardResult]) -> String {
        let mut weighted_parts: Vec<(f64, String)> = Vec::new();
        let mut total_weight = 0.0f64;

        for r in results {
            if r.error.is_none() {
                let shard_weight = self
                    .shards
                    .get(&r.shard_id)
                    .map(|s| s.value().shard_weight)
                    .unwrap_or(1.0);
                let weight = shard_weight * r.confidence;
                if weight > 0.0 {
                    weighted_parts.push((weight, r.output_chunk.clone()));
                    total_weight += weight;
                }
            }
        }

        if total_weight <= 0.0 || weighted_parts.is_empty() {
            return String::new();
        }

        // Blend: interleave proportional chunks from each shard.
        // For simulation we produce a weighted concatenation indicator.
        weighted_parts
            .iter()
            .map(|(w, chunk)| {
                let fraction = w / total_weight;
                format!("[w:{:.2}] {}", fraction, chunk)
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// Smart select: pick the shard output with the highest confidence.
    fn merge_smart_select(&self, results: &[ShardResult]) -> String {
        results
            .iter()
            .filter(|r| r.error.is_none())
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|r| {
                format!(
                    "[best:{}] {}",
                    r.shard_id, r.output_chunk
                )
            })
            .unwrap_or_default()
    }

    /// Compute quality score as average confidence of non-error results.
    fn compute_quality_score(&self, results: &[ShardResult]) -> f64 {
        let non_error: Vec<&ShardResult> =
            results.iter().filter(|r| r.error.is_none()).collect();
        if non_error.is_empty() {
            return 0.0;
        }
        non_error.iter().map(|r| r.confidence).sum::<f64>() / non_error.len() as f64
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn next_shard_id(&self) -> String {
        let id = self.shard_counter.fetch_add(1, Ordering::Relaxed);
        format!("shard-{}", id)
    }

    fn next_pipeline_id(&self) -> String {
        let id = self.pipeline_counter.fetch_add(1, Ordering::Relaxed);
        format!("pipeline-{}", id)
    }

    /// Read the config synchronously.
    fn blocking_config(&self) -> ShardConfig {
        self.config.read().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // Helper: create a coordinator with 2 shards for "model-a".
    fn setup_coordinator_with_shards() -> ShardCoordinator {
        let coord = ShardCoordinator::with_defaults();
        coord.register_shard("model-a".into(), "provider-1".into(), 0, 7);
        coord.register_shard("model-a".into(), "provider-2".into(), 8, 15);
        coord
    }

    fn make_request(model_id: &str, prompt: &str) -> InferenceRequest {
        InferenceRequest {
            request_id: format!("req-{}", uuid::Uuid::new_v4()),
            model_id: model_id.to_string(),
            prompt: prompt.to_string(),
            parameters: InferenceParameters::default(),
            priority: Priority::Normal,
        }
    }

    // -- Shard CRUD tests ---------------------------------------------------

    #[test]
    fn test_register_shard() {
        let coord = ShardCoordinator::with_defaults();
        let shard = coord.register_shard("m1".into(), "p1".into(), 0, 7);
        assert_eq!(shard.status, ShardStatus::Active);
        assert_eq!(shard.health_score, 1.0);
        assert_eq!(shard.layer_count(), 8);
        assert!(coord.get_shard(&shard.id).is_some());
    }

    #[test]
    fn test_register_multiple_shards_increments_id() {
        let coord = ShardCoordinator::with_defaults();
        let s1 = coord.register_shard("m".into(), "p".into(), 0, 3);
        let s2 = coord.register_shard("m".into(), "p".into(), 4, 7);
        assert_ne!(s1.id, s2.id);
        assert!(s2.id.ends_with("1"));
    }

    #[test]
    fn test_unregister_shard() {
        let coord = ShardCoordinator::with_defaults();
        let shard = coord.register_shard("m".into(), "p".into(), 0, 7);
        assert!(coord.unregister_shard(&shard.id));
        assert!(!coord.unregister_shard(&shard.id)); // already removed
        assert!(coord.get_shard(&shard.id).is_none());
    }

    #[test]
    fn test_get_shard() {
        let coord = ShardCoordinator::with_defaults();
        let shard = coord.register_shard("m".into(), "p".into(), 0, 7);
        let fetched = coord.get_shard(&shard.id).unwrap();
        assert_eq!(fetched.id, shard.id);
        assert_eq!(fetched.model_id, "m");
    }

    #[test]
    fn test_get_shard_missing() {
        let coord = ShardCoordinator::with_defaults();
        assert!(coord.get_shard("nonexistent").is_none());
    }

    #[test]
    fn test_list_shards() {
        let coord = ShardCoordinator::with_defaults();
        coord.register_shard("m1".into(), "p1".into(), 0, 3);
        coord.register_shard("m2".into(), "p2".into(), 4, 7);
        coord.register_shard("m1".into(), "p3".into(), 8, 11);
        assert_eq!(coord.list_shards().len(), 3);
        assert_eq!(coord.list_shards_by_model("m1").len(), 2);
        assert_eq!(coord.list_shards_by_model("m2").len(), 1);
    }

    #[test]
    fn test_update_shard_health_active() {
        let coord = ShardCoordinator::with_defaults();
        let shard = coord.register_shard("m".into(), "p".into(), 0, 7);
        assert!(coord.update_shard_health(&shard.id, 0.95, 42));
        let s = coord.get_shard(&shard.id).unwrap();
        assert_eq!(s.health_score, 0.95);
        assert_eq!(s.latency_ms, 42);
        assert_eq!(s.status, ShardStatus::Active);
    }

    #[test]
    fn test_update_shard_health_derives_status() {
        let coord = ShardCoordinator::with_defaults();
        let shard = coord.register_shard("m".into(), "p".into(), 0, 7);

        coord.update_shard_health(&shard.id, 0.2, 500);
        assert_eq!(coord.get_shard(&shard.id).unwrap().status, ShardStatus::Failed);

        coord.update_shard_health(&shard.id, 0.5, 200);
        assert_eq!(coord.get_shard(&shard.id).unwrap().status, ShardStatus::Overloaded);

        coord.update_shard_health(&shard.id, 0.7, 100);
        assert_eq!(coord.get_shard(&shard.id).unwrap().status, ShardStatus::Inactive);

        coord.update_shard_health(&shard.id, 0.85, 50);
        assert_eq!(coord.get_shard(&shard.id).unwrap().status, ShardStatus::Active);
    }

    #[test]
    fn test_update_shard_health_missing_returns_false() {
        let coord = ShardCoordinator::with_defaults();
        assert!(!coord.update_shard_health("nope", 1.0, 0));
    }

    #[test]
    fn test_health_score_clamped() {
        let coord = ShardCoordinator::with_defaults();
        let shard = coord.register_shard("m".into(), "p".into(), 0, 7);
        coord.update_shard_health(&shard.id, 1.5, 0);
        assert_eq!(coord.get_shard(&shard.id).unwrap().health_score, 1.0);
        coord.update_shard_health(&shard.id, -0.5, 0);
        assert_eq!(coord.get_shard(&shard.id).unwrap().health_score, 0.0);
    }

    // -- Pipeline tests -----------------------------------------------------

    #[test]
    fn test_create_pipeline() {
        let coord = setup_coordinator_with_shards();
        let pipeline = coord.create_pipeline("model-a", 16, 100).unwrap();
        assert_eq!(pipeline.model_id, "model-a");
        assert_eq!(pipeline.shards.len(), 2);
        assert_eq!(pipeline.status, PipelineStatus::Active);
        assert!(pipeline.created_at > 0);
    }

    #[test]
    fn test_create_pipeline_no_shards() {
        let coord = ShardCoordinator::with_defaults();
        let result = coord.create_pipeline("no-such-model", 16, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_pipeline_assigns_layer_ranges() {
        let coord = setup_coordinator_with_shards();
        let pipeline = coord.create_pipeline("model-a", 16, 50).unwrap();
        // Check that shards were updated with correct layer ranges.
        for sid in &pipeline.shards {
            let shard = coord.get_shard(sid).unwrap();
            assert!(shard.end_layer >= shard.start_layer);
        }
    }

    #[test]
    fn test_execute_pipeline() {
        let coord = setup_coordinator_with_shards();
        let pipeline = coord.create_pipeline("model-a", 16, 100).unwrap();
        let request = make_request("model-a", "hello world");
        let results = coord.execute_pipeline(&pipeline.pipeline_id, &request).unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.error.is_none());
            assert!(r.token_count > 0);
            assert!(r.latency_ms > 0);
        }
    }

    #[test]
    fn test_execute_pipeline_not_found() {
        let coord = ShardCoordinator::with_defaults();
        let request = make_request("m", "test");
        let result = coord.execute_pipeline("nonexistent", &request);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_pipeline_completed_status() {
        let coord = setup_coordinator_with_shards();
        let pipeline = coord.create_pipeline("model-a", 16, 50).unwrap();
        let request = make_request("model-a", "test");
        coord.execute_pipeline(&pipeline.pipeline_id, &request).unwrap();
        let p = coord.pipelines.get(&pipeline.pipeline_id).unwrap();
        assert_eq!(p.status, PipelineStatus::Completed);
        assert!(p.total_latency_ms > 0);
        assert!(p.output_tokens > 0);
    }

    // -- Merge strategy tests -----------------------------------------------

    #[test]
    fn test_merge_concatenate() {
        let coord = ShardCoordinator::with_defaults();
        let results = vec![
            ShardResult {
                shard_id: "s1".into(),
                output_chunk: "hello".into(),
                token_count: 5,
                latency_ms: 10,
                confidence: 0.9,
                error: None,
            },
            ShardResult {
                shard_id: "s2".into(),
                output_chunk: "world".into(),
                token_count: 5,
                latency_ms: 12,
                confidence: 0.8,
                error: None,
            },
        ];
        let merged = coord.merge_results("req-1", &results, Some(MergeStrategy::Concatenate));
        assert_eq!(merged.merge_strategy_used, MergeStrategy::Concatenate);
        assert!(merged.final_output.contains("hello"));
        assert!(merged.final_output.contains("world"));
        assert_eq!(merged.total_tokens, 10);
        assert_eq!(merged.total_latency_ms, 22);
    }

    #[test]
    fn test_merge_weighted_blend() {
        let coord = ShardCoordinator::with_defaults();
        // Register shards so weights are available.
        coord.register_shard("m".into(), "p1".into(), 0, 7);
        coord.register_shard("m".into(), "p2".into(), 8, 15);

        let results = vec![
            ShardResult {
                shard_id: "shard-0".into(),
                output_chunk: "alpha".into(),
                token_count: 5,
                latency_ms: 10,
                confidence: 0.9,
                error: None,
            },
            ShardResult {
                shard_id: "shard-1".into(),
                output_chunk: "beta".into(),
                token_count: 5,
                latency_ms: 15,
                confidence: 0.5,
                error: None,
            },
        ];
        let merged = coord.merge_results("req-2", &results, Some(MergeStrategy::WeightedBlend));
        assert_eq!(merged.merge_strategy_used, MergeStrategy::WeightedBlend);
        assert!(merged.final_output.contains("w:"));
        assert!(merged.final_output.contains("alpha"));
    }

    #[test]
    fn test_merge_smart_select() {
        let coord = ShardCoordinator::with_defaults();
        let results = vec![
            ShardResult {
                shard_id: "s-low".into(),
                output_chunk: "low confidence output".into(),
                token_count: 5,
                latency_ms: 10,
                confidence: 0.3,
                error: None,
            },
            ShardResult {
                shard_id: "s-high".into(),
                output_chunk: "high confidence output".into(),
                token_count: 10,
                latency_ms: 20,
                confidence: 0.95,
                error: None,
            },
        ];
        let merged = coord.merge_results("req-3", &results, Some(MergeStrategy::SmartSelect));
        assert_eq!(merged.merge_strategy_used, MergeStrategy::SmartSelect);
        assert!(merged.final_output.contains("s-high"));
        assert!(merged.final_output.contains("high confidence output"));
    }

    #[test]
    fn test_merge_with_errors() {
        let coord = ShardCoordinator::with_defaults();
        let results = vec![
            ShardResult {
                shard_id: "s1".into(),
                output_chunk: String::new(),
                token_count: 0,
                latency_ms: 100,
                confidence: 0.0,
                error: Some("timeout".into()),
            },
            ShardResult {
                shard_id: "s2".into(),
                output_chunk: "ok".into(),
                token_count: 5,
                latency_ms: 10,
                confidence: 0.8,
                error: None,
            },
        ];
        let merged = coord.merge_results("req-err", &results, Some(MergeStrategy::Concatenate));
        assert!(merged.final_output.contains("ok"));
        assert!(!merged.final_output.contains("timeout"));
        // Quality score based only on non-error results.
        assert!((merged.quality_score - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_merge_all_errors_quality_zero() {
        let coord = ShardCoordinator::with_defaults();
        let results = vec![ShardResult {
            shard_id: "s1".into(),
            output_chunk: String::new(),
            token_count: 0,
            latency_ms: 50,
            confidence: 0.0,
            error: Some("failed".into()),
        }];
        let merged = coord.merge_results("req-fail", &results, Some(MergeStrategy::SmartSelect));
        assert_eq!(merged.quality_score, 0.0);
        assert!(merged.final_output.is_empty());
    }

    // -- Routing tests ------------------------------------------------------

    #[test]
    fn test_route_request_creates_pipeline_if_none() {
        let coord = setup_coordinator_with_shards();
        let request = make_request("model-a", "auto route test");
        let merged = coord.route_request(request).unwrap();
        assert!(!merged.final_output.is_empty());
        assert!(merged.total_tokens > 0);
        assert!(merged.quality_score > 0.0);
        // Stats should reflect the routed request.
        let stats = coord.get_stats();
        assert_eq!(stats.total_requests_routed, 1);
    }

    #[test]
    fn test_route_request_reuses_existing_pipeline() {
        let coord = setup_coordinator_with_shards();
        // Pre-create a pipeline.
        let pipeline = coord.create_pipeline("model-a", 16, 50).unwrap();
        let request = make_request("model-a", "reuse test");
        let merged = coord.route_request(request).unwrap();
        // The merged result should reference the existing pipeline's shards.
        assert_eq!(merged.shard_results.len(), pipeline.shards.len());
    }

    // -- Stats / health tests -----------------------------------------------

    #[test]
    fn test_get_stats_empty() {
        let coord = ShardCoordinator::with_defaults();
        let stats = coord.get_stats();
        assert_eq!(stats.total_shards, 0);
        assert_eq!(stats.total_pipelines, 0);
        assert_eq!(stats.average_latency_ms, 0.0);
    }

    #[test]
    fn test_get_stats_after_activity() {
        let coord = setup_coordinator_with_shards();
        coord.create_pipeline("model-a", 16, 100).unwrap();
        let request = make_request("model-a", "stats test");
        let _ = coord.route_request(request);
        let stats = coord.get_stats();
        assert_eq!(stats.total_shards, 2);
        assert!(stats.active_shards >= 1);
        assert_eq!(stats.total_pipelines, 1);
        assert_eq!(stats.total_requests_routed, 1);
        assert!(stats.average_latency_ms > 0.0);
    }

    #[test]
    fn test_get_health_report() {
        let coord = setup_coordinator_with_shards();
        let report = coord.get_health_report();
        assert_eq!(report.shard_count, 2);
        assert_eq!(report.healthy_shards, 2);
        assert!(report.overall_health > 0.0);
        assert_eq!(report.shards.len(), 2);
    }

    #[test]
    fn test_get_pipeline_history() {
        let coord = setup_coordinator_with_shards();
        coord.create_pipeline("model-a", 16, 50).unwrap();
        coord.create_pipeline("model-a", 16, 80).unwrap();
        let history = coord.get_pipeline_history();
        assert_eq!(history.len(), 2);
    }

    // -- Auto-scale tests ---------------------------------------------------

    #[test]
    fn test_auto_scale_adds_when_overloaded() {
        let coord = ShardCoordinator::with_defaults();
        // Register shards and mark most as overloaded/failed.
        let s1 = coord.register_shard("m".into(), "p1".into(), 0, 7);
        let s2 = coord.register_shard("m".into(), "p2".into(), 8, 15);
        coord.update_shard_health(&s1.id, 0.1, 500); // Failed
        coord.update_shard_health(&s2.id, 0.2, 400); // Failed

        let (added, removed) = coord.auto_scale();
        assert!(added > 0);
        assert_eq!(removed, 0);
        // Total shards should have increased.
        assert!(coord.shards.len() > 2);
    }

    #[test]
    fn test_auto_scale_no_op_when_healthy() {
        let coord = setup_coordinator_with_shards();
        let before = coord.shards.len();
        let (added, removed) = coord.auto_scale();
        assert_eq!(added, 0);
        assert_eq!(removed, 0);
        assert_eq!(coord.shards.len(), before);
    }

    // -- Serialization tests ------------------------------------------------

    #[test]
    fn test_shard_serialization_roundtrip() {
        let shard = Shard::new("s1".into(), "m1".into(), "p1".into(), 0, 15);
        let json = serde_json::to_string(&shard).unwrap();
        let deserialized: Shard = serde_json::from_str(&json).unwrap();
        assert_eq!(shard.id, deserialized.id);
        assert_eq!(shard.model_id, deserialized.model_id);
        assert_eq!(shard.start_layer, deserialized.start_layer);
        assert_eq!(shard.end_layer, deserialized.end_layer);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let cfg = ShardConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: ShardConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.shard_size, deserialized.shard_size);
        assert_eq!(cfg.merge_strategy, deserialized.merge_strategy);
        assert_eq!(cfg.timeout_ms, deserialized.timeout_ms);
    }

    #[test]
    fn test_merged_result_serialization() {
        let result = MergedResult {
            request_id: "req-1".into(),
            final_output: "hello world".into(),
            total_tokens: 10,
            total_latency_ms: 50,
            merge_strategy_used: MergeStrategy::Concatenate,
            shard_results: vec![],
            quality_score: 0.85,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: MergedResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result.request_id, deserialized.request_id);
        assert_eq!(result.quality_score, deserialized.quality_score);
    }

    #[test]
    fn test_inference_request_serialization() {
        let req = InferenceRequest {
            request_id: "r1".into(),
            model_id: "m1".into(),
            prompt: "test".into(),
            parameters: InferenceParameters::default(),
            priority: Priority::High,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: InferenceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.priority, Priority::High);
    }

    // -- Concurrent access tests --------------------------------------------

    #[test]
    fn test_concurrent_shard_registration() {
        let coord = Arc::new(ShardCoordinator::with_defaults());
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let c = Arc::clone(&coord);
                thread::spawn(move || {
                    c.register_shard(
                        format!("model-{}", i),
                        format!("provider-{}", i),
                        i * 4,
                        (i + 1) * 4 - 1,
                    );
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(coord.list_shards().len(), 10);
    }

    #[test]
    fn test_concurrent_pipeline_creation_and_routing() {
        let coord = Arc::new(setup_coordinator_with_shards());
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let c = Arc::clone(&coord);
                thread::spawn(move || {
                    let _pipeline = c.create_pipeline("model-a", 16, 50).unwrap();
                    let req = InferenceRequest {
                        request_id: format!("concurrent-req-{}", i),
                        model_id: "model-a".into(),
                        prompt: format!("prompt {}", i),
                        parameters: InferenceParameters::default(),
                        priority: Priority::Normal,
                    };
                    c.route_request(req).unwrap()
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let stats = coord.get_stats();
        assert_eq!(stats.total_requests_routed, 5);
        assert!(stats.total_pipelines >= 5);
    }

    #[test]
    fn test_concurrent_health_updates() {
        let coord = Arc::new(setup_coordinator_with_shards());
        let shard_id = coord.list_shards()[0].id.clone();

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let c = Arc::clone(&coord);
                let sid = shard_id.clone();
                thread::spawn(move || {
                    let health = (i as f64) / 20.0;
                    c.update_shard_health(&sid, health, i * 10);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Shard should still be accessible and have a valid health score.
        let shard = coord.get_shard(&shard_id).unwrap();
        assert!(shard.health_score >= 0.0 && shard.health_score <= 1.0);
    }

    // -- Default / edge-case tests ------------------------------------------

    #[test]
    fn test_default_config_values() {
        let cfg = ShardConfig::default();
        assert_eq!(cfg.shard_size, 4);
        assert!(!cfg.pipeline_parallel);
        assert_eq!(cfg.overlap_tokens, 0);
        assert_eq!(cfg.merge_strategy, MergeStrategy::WeightedBlend);
        assert_eq!(cfg.timeout_ms, 30_000);
        assert_eq!(cfg.retry_count, 3);
    }

    #[test]
    fn test_default_inference_parameters() {
        let params = InferenceParameters::default();
        assert!((params.temperature - 0.7).abs() < 0.001);
        assert_eq!(params.max_tokens, 256);
        assert!((params.top_p - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_default_priority() {
        assert_eq!(Priority::default(), Priority::Normal);
    }

    #[test]
    fn test_shard_layer_count() {
        let shard = Shard::new("s".into(), "m".into(), "p".into(), 5, 12);
        assert_eq!(shard.layer_count(), 8); // 12 - 5 + 1
    }

    #[test]
    fn test_pipeline_history_empty() {
        let coord = ShardCoordinator::with_defaults();
        assert!(coord.get_pipeline_history().is_empty());
    }

    #[test]
    fn test_results_stored_after_merge() {
        let coord = ShardCoordinator::with_defaults();
        let results = vec![ShardResult {
            shard_id: "s1".into(),
            output_chunk: "output".into(),
            token_count: 3,
            latency_ms: 10,
            confidence: 0.9,
            error: None,
        }];
        coord.merge_results("req-store", &results, Some(MergeStrategy::Concatenate));
        assert!(coord.results.get("req-store").is_some());
    }
}
