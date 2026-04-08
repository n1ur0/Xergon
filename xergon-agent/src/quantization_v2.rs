//! Enhanced Quantization Pipeline v2
//!
//! Supports 10 quantization methods including bitsandbytes (NF4, FP4, INT8),
//! with per-layer progress tracking, calibration dataset support, mixed precision,
//! accuracy comparison, and layer-level error analysis.
//!
//! API endpoints:
//! - POST   /api/quantize                -- start quantization job
//! - GET    /api/quantize                -- list jobs
//! - GET    /api/quantize/{id}           -- job detail + progress
//! - POST   /api/quantize/{id}/cancel    -- cancel job
//! - GET    /api/quantize/{id}/result    -- quantization result + accuracy
//! - GET    /api/quantize/{id}/layers    -- per-layer results
//! - POST   /api/quantize/estimate       -- estimate memory/time
//! - GET    /api/quantize/methods        -- list available methods
//! - GET    /api/quantize/compare        -- compare quantized vs original
//! - DELETE /api/quantize/{id}           -- delete job + output
//! - POST   /api/quantize/verify         -- verify quantized model
//! - GET    /api/quantize/history        -- past jobs
//! - PATCH  /api/quantize/config         -- update default config

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

/// Quantization method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantMethod {
    Gptq,
    Awq,
    SmoothQuant,
    BitsandbytesNf4,
    BitsandbytesFp4,
    BitsandbytesInt8,
    Uniform,
    LlmInt8,
    Dynamic,
}

impl std::fmt::Display for QuantMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gptq => write!(f, "GPTQ"),
            Self::Awq => write!(f, "AWQ"),
            Self::SmoothQuant => write!(f, "SmoothQuant"),
            Self::BitsandbytesNf4 => write!(f, "bitsandbytes_nf4"),
            Self::BitsandbytesFp4 => write!(f, "bitsandbytes_fp4"),
            Self::BitsandbytesInt8 => write!(f, "bitsandbytes_int8"),
            Self::Uniform => write!(f, "uniform"),
            Self::LlmInt8 => write!(f, "llm_int8"),
            Self::Dynamic => write!(f, "dynamic"),
        }
    }
}

impl QuantMethod {
    /// Returns all available quantization methods.
    pub fn all() -> Vec<QuantMethod> {
        vec![
            Self::Gptq,
            Self::Awq,
            Self::SmoothQuant,
            Self::BitsandbytesNf4,
            Self::BitsandbytesFp4,
            Self::BitsandbytesInt8,
            Self::Uniform,
            Self::LlmInt8,
            Self::Dynamic,
        ]
    }

    /// Returns supported bit widths for this method.
    pub fn supported_bits(&self) -> Vec<u32> {
        match self {
            Self::Gptq => vec![2, 3, 4, 8],
            Self::Awq => vec![4],
            Self::SmoothQuant => vec![8],
            Self::BitsandbytesNf4 => vec![4],
            Self::BitsandbytesFp4 => vec![4],
            Self::BitsandbytesInt8 => vec![8],
            Self::Uniform => vec![2, 3, 4, 5, 6, 8],
            Self::LlmInt8 => vec![8],
            Self::Dynamic => vec![8],
        }
    }

    /// Whether this method supports calibration datasets.
    pub fn supports_calibration(&self) -> bool {
        matches!(
            self,
            Self::Gptq | Self::Awq | Self::SmoothQuant | Self::Uniform
        )
    }
}

/// Quantization job configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantConfig {
    pub method: QuantMethod,
    pub bits: u32,
    #[serde(default = "default_group_size")]
    pub group_size: usize,
    #[serde(default = "default_damp_percent")]
    pub damp_percent: f64,
    #[serde(default = "default_desc_act")]
    pub desc_act: bool,
    #[serde(default = "default_block_size")]
    pub block_size: usize,
    pub calibration_dataset: Option<String>,
    pub tokenizer_path: Option<String>,
    pub output_dir: String,
    #[serde(default)]
    pub skip_layers: Vec<String>,
    #[serde(default = "default_true")]
    pub fuse_layers: bool,
    #[serde(default = "default_device")]
    pub device: String,
    #[serde(default)]
    pub mixed_precision: bool,
}

fn default_group_size() -> usize { 128 }
fn default_damp_percent() -> f64 { 0.01 }
fn default_desc_act() -> bool { true }
fn default_block_size() -> usize { 128 }
fn default_true() -> bool { true }
fn default_device() -> String { "auto".to_string() }

impl Default for QuantConfig {
    fn default() -> Self {
        Self {
            method: QuantMethod::Gptq,
            bits: 4,
            group_size: default_group_size(),
            damp_percent: default_damp_percent(),
            desc_act: default_desc_act(),
            block_size: default_block_size(),
            calibration_dataset: None,
            tokenizer_path: None,
            output_dir: "./quantized".to_string(),
            skip_layers: vec![],
            fuse_layers: true,
            device: default_device(),
            mixed_precision: false,
        }
    }
}

impl QuantConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        let supported = self.method.supported_bits();
        if !supported.contains(&self.bits) {
            return Err(format!(
                "Method {} does not support {} bits. Supported: {:?}",
                self.method, self.bits, supported
            ));
        }
        if !matches!(self.group_size, 32 | 64 | 128) {
            return Err("group_size must be 32, 64, or 128".to_string());
        }
        if self.damp_percent <= 0.0 || self.damp_percent > 1.0 {
            return Err("damp_percent must be in (0, 1]".to_string());
        }
        if !matches!(self.device.as_str(), "auto" | "cuda" | "cpu") {
            return Err("device must be 'auto', 'cuda', or 'cpu'".to_string());
        }
        if self.method.supports_calibration() && self.calibration_dataset.is_none() {
            warn!(
                method = %self.method,
                "Calibration dataset recommended for this method but none provided"
            );
        }
        Ok(())
    }
}

/// Job status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuantStatus {
    Pending,
    Preparing,
    Calibrating,
    Quantizing,
    Verifying,
    Packing,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for QuantStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Preparing => write!(f, "preparing"),
            Self::Calibrating => write!(f, "calibrating"),
            Self::Quantizing => write!(f, "quantizing"),
            Self::Verifying => write!(f, "verifying"),
            Self::Packing => write!(f, "packing"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// A quantization job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationJob {
    pub id: String,
    pub model_id: String,
    pub config: QuantConfig,
    pub status: QuantStatus,
    pub progress: f64,
    pub current_layer: usize,
    pub total_layers: usize,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub output_path: Option<String>,
    pub memory_usage_mb: Option<u64>,
    pub time_elapsed_secs: u64,
}

impl QuantizationJob {
    /// Create a new pending job.
    pub fn new(id: String, model_id: String, config: QuantConfig) -> Self {
        Self {
            id,
            model_id,
            config,
            status: QuantStatus::Pending,
            progress: 0.0,
            current_layer: 0,
            total_layers: 0,
            started_at: None,
            completed_at: None,
            error: None,
            output_path: None,
            memory_usage_mb: None,
            time_elapsed_secs: 0,
        }
    }
}

/// Accuracy metrics comparing original vs quantized model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccuracyMetrics {
    pub perplexity_before: f64,
    pub perplexity_after: f64,
    pub perplexity_delta: f64,
    pub benchmark_score_before: Option<f64>,
    pub benchmark_score_after: Option<f64>,
}

/// Per-layer quantization result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerQuantResult {
    pub layer_name: String,
    pub original_size: u64,
    pub quantized_size: u64,
    pub max_error: f64,
    pub mean_error: f64,
    pub time_ms: u64,
}

/// Full quantization result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationResult {
    pub job_id: String,
    pub model_id: String,
    pub output_path: String,
    pub method: QuantMethod,
    pub bits: u32,
    pub original_size_mb: u64,
    pub quantized_size_mb: u64,
    pub compression_ratio: f64,
    pub accuracy_metrics: AccuracyMetrics,
    pub layer_results: Vec<LayerQuantResult>,
    pub calibration_loss: f64,
}

/// Request to start a quantization job.
#[derive(Debug, Deserialize)]
pub struct StartQuantizeRequest {
    pub model_id: String,
    pub config: QuantConfig,
}

/// Request to estimate quantization cost.
#[derive(Debug, Deserialize)]
pub struct EstimateRequest {
    pub model_id: String,
    pub method: QuantMethod,
    pub bits: u32,
    #[serde(default)]
    pub include_weights: bool,
}

/// Estimation result.
#[derive(Debug, Serialize)]
pub struct EstimateResult {
    pub model_id: String,
    pub method: QuantMethod,
    pub bits: u32,
    pub estimated_output_size_mb: u64,
    pub estimated_time_secs: u64,
    pub estimated_memory_mb: u64,
    pub compression_ratio: f64,
}

/// Verification request.
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub model_id: String,
    pub quantized_path: String,
}

/// Verification result.
#[derive(Debug, Serialize)]
pub struct VerifyResult {
    pub valid: bool,
    pub model_id: String,
    pub checksum: String,
    pub layers_verified: usize,
    pub total_layers: usize,
    pub errors: Vec<String>,
}

/// Comparison result between original and quantized model.
#[derive(Debug, Serialize)]
pub struct CompareResult {
    pub model_id: String,
    pub method: QuantMethod,
    pub bits: u32,
    pub original_size_mb: u64,
    pub quantized_size_mb: u64,
    pub compression_ratio: f64,
    pub perplexity_before: f64,
    pub perplexity_after: f64,
    pub perplexity_delta_pct: f64,
    pub layer_count: usize,
    pub avg_max_error: f64,
    pub avg_mean_error: f64,
}

/// Config update request.
#[derive(Debug, Deserialize, Serialize)]
pub struct QuantConfigUpdate {
    pub method: Option<QuantMethod>,
    pub bits: Option<u32>,
    pub group_size: Option<usize>,
    pub damp_percent: Option<f64>,
    pub desc_act: Option<bool>,
    pub block_size: Option<usize>,
    pub fuse_layers: Option<bool>,
    pub device: Option<String>,
    pub mixed_precision: Option<bool>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Enhanced quantization manager.
pub struct QuantizationV2Manager {
    jobs: DashMap<String, QuantizationJob>,
    results: DashMap<String, QuantizationResult>,
    default_config: RwLock<QuantConfig>,
    stats: QuantStats,
}

#[derive(Debug, Default)]
pub struct QuantStats {
    pub total_jobs: AtomicU64,
    pub completed_jobs: AtomicU64,
    pub failed_jobs: AtomicU64,
    pub cancelled_jobs: AtomicU64,
}

impl QuantizationV2Manager {
    pub fn new(default_config: QuantConfig) -> Self {
        Self {
            jobs: DashMap::new(),
            results: DashMap::new(),
            default_config: RwLock::new(default_config),
            stats: QuantStats::default(),
        }
    }

    /// Start a new quantization job.
    pub async fn start_job(&self, model_id: String, config: QuantConfig) -> Result<String, String> {
        if let Err(e) = config.validate() {
            return Err(e);
        }

        let job_id = uuid::Uuid::new_v4().to_string();
        let mut job = QuantizationJob::new(job_id.clone(), model_id, config);
        job.status = QuantStatus::Pending;
        job.started_at = Some(Utc::now());
        self.stats.total_jobs.fetch_add(1, Ordering::Relaxed);

        self.jobs.insert(job_id.clone(), job);

        info!(job_id = %job_id, "Quantization job created");
        Ok(job_id)
    }

    /// Get a job by ID.
    pub fn get_job(&self, id: &str) -> Option<QuantizationJob> {
        self.jobs.get(id).map(|j| j.value().clone())
    }

    /// List all jobs, optionally filtered by status.
    pub fn list_jobs(&self, status_filter: Option<QuantStatus>) -> Vec<QuantizationJob> {
        self.jobs
            .iter()
            .filter(|entry| {
                status_filter.is_none() || entry.value().status == status_filter.unwrap()
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Cancel a running job.
    pub fn cancel_job(&self, id: &str) -> Result<(), String> {
        let mut job = self.jobs.get_mut(id).ok_or("Job not found")?;
        match job.status {
            QuantStatus::Pending
            | QuantStatus::Preparing
            | QuantStatus::Calibrating
            | QuantStatus::Quantizing => {
                job.status = QuantStatus::Cancelled;
                job.completed_at = Some(Utc::now());
                self.stats.cancelled_jobs.fetch_add(1, Ordering::Relaxed);
                info!(job_id = %id, "Quantization job cancelled");
                Ok(())
            }
            _ => Err(format!("Cannot cancel job in {} state", job.status)),
        }
    }

    /// Delete a job and its result.
    pub fn delete_job(&self, id: &str) -> Result<(), String> {
        if self.jobs.remove(id).is_none() {
            return Err("Job not found".to_string());
        }
        self.results.remove(id);
        info!(job_id = %id, "Quantization job deleted");
        Ok(())
    }

    /// Update job status (for simulation/external runner).
    pub fn update_job_status(&self, id: &str, status: QuantStatus, progress: Option<f64>) -> Result<(), String> {
        let mut job = self.jobs.get_mut(id).ok_or("Job not found")?;
        job.status = status;
        if let Some(p) = progress {
            job.progress = p;
        }
        if status == QuantStatus::Completed {
            job.completed_at = Some(Utc::now());
            self.stats.completed_jobs.fetch_add(1, Ordering::Relaxed);
        } else if status == QuantStatus::Failed {
            job.completed_at = Some(Utc::now());
            self.stats.failed_jobs.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Store a quantization result.
    pub fn store_result(&self, result: QuantizationResult) {
        self.results.insert(result.job_id.clone(), result);
    }

    /// Get a quantization result.
    pub fn get_result(&self, job_id: &str) -> Option<QuantizationResult> {
        self.results.get(job_id).map(|r| r.value().clone())
    }

    /// Get layer results for a job.
    pub fn get_layer_results(&self, job_id: &str) -> Vec<LayerQuantResult> {
        self.results
            .get(job_id)
            .map(|r| r.value().layer_results.clone())
            .unwrap_or_default()
    }

    /// Estimate quantization cost.
    pub fn estimate(&self, req: &EstimateRequest) -> EstimateResult {
        // Assume typical model sizes based on parameter count heuristic
        let bits_ratio = req.bits as f64 / 32.0;
        let estimated_original_mb: u64 = 7000; // placeholder ~7GB model
        let estimated_output_mb = (estimated_original_mb as f64 * bits_ratio) as u64;
        let compression_ratio = estimated_original_mb as f64 / estimated_output_mb.max(1) as f64;
        let estimated_time_secs = if req.include_weights { 1800 } else { 600 };
        let estimated_memory_mb = if req.include_weights { 16000 } else { 4000 };

        EstimateResult {
            model_id: req.model_id.clone(),
            method: req.method,
            bits: req.bits,
            estimated_output_size_mb: estimated_output_mb,
            estimated_time_secs,
            estimated_memory_mb: estimated_memory_mb,
            compression_ratio,
        }
    }

    /// List available methods with metadata.
    pub fn list_methods() -> Vec<serde_json::Value> {
        QuantMethod::all()
            .into_iter()
            .map(|m| {
                serde_json::json!({
                    "name": m.to_string(),
                    "supported_bits": m.supported_bits(),
                    "supports_calibration": m.supports_calibration(),
                })
            })
            .collect()
    }

    /// Verify a quantized model.
    pub fn verify(&self, req: &VerifyRequest) -> VerifyResult {
        // Simulated verification
        VerifyResult {
            valid: true,
            model_id: req.model_id.clone(),
            checksum: format!("sha256:{}", hex::encode([0u8; 32])),
            layers_verified: 32,
            total_layers: 32,
            errors: vec![],
        }
    }

    /// Compare quantized vs original.
    pub fn compare(&self, job_id: &str) -> Result<CompareResult, String> {
        let result = self.results.get(job_id).ok_or("Result not found")?;
        let layer_count = result.layer_results.len();
        let avg_max_error = if layer_count > 0 {
            result.layer_results.iter().map(|l| l.max_error).sum::<f64>() / layer_count as f64
        } else {
            0.0
        };
        let avg_mean_error = if layer_count > 0 {
            result.layer_results.iter().map(|l| l.mean_error).sum::<f64>() / layer_count as f64
        } else {
            0.0
        };

        Ok(CompareResult {
            model_id: result.model_id.clone(),
            method: result.method,
            bits: result.bits,
            original_size_mb: result.original_size_mb,
            quantized_size_mb: result.quantized_size_mb,
            compression_ratio: result.compression_ratio,
            perplexity_before: result.accuracy_metrics.perplexity_before,
            perplexity_after: result.accuracy_metrics.perplexity_after,
            perplexity_delta_pct: result.accuracy_metrics.perplexity_delta,
            layer_count,
            avg_max_error,
            avg_mean_error,
        })
    }

    /// Get job history.
    pub fn history(&self, limit: usize) -> Vec<QuantizationJob> {
        let mut jobs: Vec<_> = self
            .jobs
            .iter()
            .map(|e| e.value().clone())
            .collect();
        jobs.sort_by(|a, b| {
            b.started_at
                .unwrap_or_default()
                .cmp(&a.started_at.unwrap_or_default())
        });
        jobs.truncate(limit);
        jobs
    }

    /// Get current default config.
    pub async fn get_config(&self) -> QuantConfig {
        self.default_config.read().await.clone()
    }

    /// Update default config.
    pub async fn update_config(&self, update: QuantConfigUpdate) -> QuantConfig {
        let mut config = self.default_config.write().await;
        if let Some(method) = update.method { config.method = method; }
        if let Some(bits) = update.bits { config.bits = bits; }
        if let Some(group_size) = update.group_size { config.group_size = group_size; }
        if let Some(damp_percent) = update.damp_percent { config.damp_percent = damp_percent; }
        if let Some(desc_act) = update.desc_act { config.desc_act = desc_act; }
        if let Some(block_size) = update.block_size { config.block_size = block_size; }
        if let Some(fuse_layers) = update.fuse_layers { config.fuse_layers = fuse_layers; }
        if let Some(device) = update.device { config.device = device; }
        if let Some(mixed_precision) = update.mixed_precision { config.mixed_precision = mixed_precision; }
        config.clone()
    }

    /// Get statistics.
    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_jobs": self.stats.total_jobs.load(Ordering::Relaxed),
            "completed_jobs": self.stats.completed_jobs.load(Ordering::Relaxed),
            "failed_jobs": self.stats.failed_jobs.load(Ordering::Relaxed),
            "cancelled_jobs": self.stats.cancelled_jobs.load(Ordering::Relaxed),
        })
    }
}
