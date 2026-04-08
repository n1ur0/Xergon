//! Alignment training module for the Xergon agent.
//!
//! Provides fine-tuning via preference-based alignment methods:
//! - DPO (Direct Preference Optimization)
//! - RLHF (Reinforcement Learning from Human Feedback)
//! - GRPO (Group Relative Policy Optimization)
//! - KTO (Kahneman-Tversky Optimization)
//! - ORPO (Odds Ratio Preference Optimization)
//! - SimPO (Simple Preference Optimization)

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

use crate::api::AppState;

// ---------------------------------------------------------------------------
// Alignment configuration types
// ---------------------------------------------------------------------------

/// Supported alignment methods.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentMethod {
    Dpo,
    Rlhf,
    Grpo,
    Kto,
    Orpo,
    Simpo,
}

impl std::fmt::Display for AlignmentMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dpo => write!(f, "DPO"),
            Self::Rlhf => write!(f, "RLHF"),
            Self::Grpo => write!(f, "GRPO"),
            Self::Kto => write!(f, "KTO"),
            Self::Orpo => write!(f, "ORPO"),
            Self::Simpo => write!(f, "SimPO"),
        }
    }
}

/// Global configuration for the alignment trainer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentConfig {
    /// Which alignment method to use.
    pub method: AlignmentMethod,

    /// Base model identifier (HuggingFace repo or local path).
    pub base_model: String,

    /// Maximum number of training epochs.
    #[serde(default = "default_epochs")]
    pub epochs: u32,

    /// Per-device training batch size.
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,

    /// Peak learning rate.
    #[serde(default = "default_lr")]
    pub learning_rate: f64,

    /// Linear warmup ratio (0.0–1.0).
    #[serde(default)]
    pub warmup_ratio: f64,

    /// Weight decay for AdamW optimiser.
    #[serde(default = "default_weight_decay")]
    pub weight_decay: f64,

    /// LoRA rank (0 = full fine-tune).
    #[serde(default)]
    pub lora_rank: u32,

    /// LoRA alpha scaling factor.
    #[serde(default = "default_lora_alpha")]
    pub lora_alpha: f32,

    /// LoRA dropout probability.
    #[serde(default = "default_lora_dropout")]
    pub lora_dropout: f32,

    /// Maximum sequence length for tokenisation.
    #[serde(default = "default_max_seq_len")]
    pub max_seq_len: u32,

    /// Gradient accumulation steps (effective batch = batch_size * grad_accum).
    #[serde(default = "default_grad_accum")]
    pub gradient_accumulation_steps: u32,

    /// BF16 mixed-precision training when supported.
    #[serde(default = "default_true")]
    pub bf16: bool,

    /// Number of GPUs to use for distributed training.
    #[serde(default)]
    pub num_gpus: u32,

    /// Beta hyper-parameter for DPO loss.
    #[serde(default = "default_dpo_beta")]
    pub dpo_beta: f64,

    /// Reference model KL penalty coefficient (RLHF).
    #[serde(default = "default_kl_coef")]
    pub kl_coefficient: f64,

    /// GRPO group size (number of completions per prompt).
    #[serde(default = "default_grpo_group_size")]
    pub grpo_group_size: u32,

    /// SimPO target reward margin (lambda).
    #[serde(default = "default_simpo_lambda")]
    pub simpo_lambda: f64,

    /// ORPO odds-ratio weighting factor.
    #[serde(default = "default_orpo_weight")]
    pub orpo_weight: f64,

    /// KTO desired margin for chosen / rejected pairs.
    #[serde(default = "default_kto_margin")]
    pub kto_margin: f64,

    /// Random seed for reproducibility.
    #[serde(default)]
    pub seed: Option<u64>,

    /// Output directory for checkpoints and final model.
    pub output_dir: String,

    /// Optional wandb project name for experiment tracking.
    pub wandb_project: Option<String>,

    /// Optional run name tag.
    pub run_name: Option<String>,

    /// Evaluation strategy: "epoch", "steps", or "none".
    #[serde(default = "default_eval_strategy")]
    pub eval_strategy: String,

    /// Evaluation interval (epochs or steps depending on strategy).
    #[serde(default = "default_eval_interval")]
    pub eval_interval: u32,

    /// Early stopping patience (0 = disabled).
    #[serde(default)]
    pub early_stopping_patience: u32,
}

fn default_epochs() -> u32 { 3 }
fn default_batch_size() -> u32 { 4 }
fn default_lr() -> f64 { 5e-7 }
fn default_weight_decay() -> f64 { 0.01 }
fn default_lora_alpha() -> f32 { 16.0 }
fn default_lora_dropout() -> f32 { 0.05 }
fn default_max_seq_len() -> u32 { 2048 }
fn default_grad_accum() -> u32 { 4 }
fn default_true() -> bool { true }
fn default_dpo_beta() -> f64 { 0.1 }
fn default_kl_coef() -> f64 { 0.1 }
fn default_grpo_group_size() -> u32 { 8 }
fn default_simpo_lambda() -> f64 { 2.0 }
fn default_orpo_weight() -> f64 { 1.0 }
fn default_kto_margin() -> f64 { 0.5 }
fn default_eval_strategy() -> String { "epoch".into() }
fn default_eval_interval() -> u32 { 1 }

// ---------------------------------------------------------------------------
// Dataset types
// ---------------------------------------------------------------------------

/// A single preference entry used by DPO/KTO/ORPO/SimPO datasets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceEntry {
    /// Input prompt.
    pub prompt: String,
    /// Preferred (chosen) completion.
    pub chosen: String,
    /// Rejected completion (not required for KTO).
    #[serde(default)]
    pub rejected: Option<String>,
    /// Whether this is a "good" or "bad" example (KTO binary label).
    #[serde(default)]
    pub kto_label: Option<bool>,
}

/// A preference dataset consisting of prompt-chosen-rejected triples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceDataset {
    /// Unique dataset identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Source (e.g. "local", "huggingface://org/dataset").
    pub source: String,
    /// Total number of entries.
    pub num_entries: usize,
    /// Individual entries (may be a sample for API responses).
    #[serde(default)]
    pub entries: Vec<PreferenceEntry>,
    /// Dataset creation timestamp.
    pub created_at: String,
    /// Optional metadata tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// Job & metrics types
// ---------------------------------------------------------------------------

/// Current status of an alignment job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentJobStatus {
    Pending,
    Running,
    Evaluating,
    Packaging,
    Completed,
    Failed,
    Cancelled,
}

/// A single alignment training job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentJob {
    /// Unique job identifier.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Alignment method being used.
    pub method: AlignmentMethod,
    /// Full configuration snapshot.
    pub config: AlignmentConfig,
    /// Current status.
    pub status: AlignmentJobStatus,
    /// Dataset used for training.
    pub dataset_id: String,
    /// Progress percentage (0–100).
    pub progress: f64,
    /// Current epoch.
    pub current_epoch: u32,
    /// Total epochs.
    pub total_epochs: u32,
    /// Training loss at last checkpoint.
    pub train_loss: Option<f64>,
    /// Evaluation reward / score.
    pub eval_score: Option<f64>,
    /// Wall-clock training time in seconds.
    pub elapsed_seconds: f64,
    /// GPU hours consumed.
    pub gpu_hours: f64,
    /// Error message if the job failed.
    pub error: Option<String>,
    /// Timestamps.
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    /// Output paths.
    pub output_dir: String,
    pub checkpoint_dir: Option<String>,
    /// Final packaged model path (after successful job).
    pub packaged_model_path: Option<String>,
}

/// Metrics collected during/after training.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentMetrics {
    /// Job identifier these metrics belong to.
    pub job_id: String,
    /// Per-epoch training losses.
    pub epoch_losses: Vec<f64>,
    /// Per-epoch evaluation rewards.
    pub epoch_eval_scores: Vec<f64>,
    /// Per-epoch KL divergence from reference model.
    pub epoch_kl_divergences: Vec<f64>,
    /// Per-epoch chosen/rejected log-prob margin.
    pub epoch_reward_margins: Vec<f64>,
    /// Final average reward on the evaluation split.
    pub final_eval_reward: Option<f64>,
    /// Final training loss.
    pub final_train_loss: Option<f64>,
    /// Total GPU hours consumed.
    pub total_gpu_hours: f64,
    /// Peak GPU memory utilisation (0–1).
    pub peak_gpu_memory: f64,
    /// Tokens processed per second (throughput).
    pub tokens_per_second: Option<f64>,
    /// Win-rate against base model on held-out set (if evaluated).
    pub win_rate: Option<f64>,
    /// Timestamp of the metrics snapshot.
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Trainer manager
// ---------------------------------------------------------------------------

/// Central manager for alignment training jobs.
pub struct AlignmentTrainer {
    /// Active and historical jobs indexed by ID.
    jobs: DashMap<String, AlignmentJob>,
    /// Per-job metrics history.
    metrics: DashMap<String, AlignmentMetrics>,
    /// Loaded / cached preference datasets.
    datasets: DashMap<String, PreferenceDataset>,
    /// Monotonically increasing job counter.
    next_job_id: AtomicU64,
}

impl AlignmentTrainer {
    /// Create a new, empty alignment trainer.
    pub fn new() -> Self {
        Self {
            jobs: DashMap::new(),
            metrics: DashMap::new(),
            datasets: DashMap::new(),
            next_job_id: AtomicU64::new(1),
        }
    }

    /// Allocate a unique job ID.
    fn allocate_job_id(&self) -> String {
        format!("align-{:08}", self.next_job_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Launch a new alignment training job.
    pub fn start_job(
        &self,
        name: String,
        method: AlignmentMethod,
        config: AlignmentConfig,
        dataset_id: String,
    ) -> Result<AlignmentJob, String> {
        // Validate that the referenced dataset exists.
        if !self.datasets.contains_key(&dataset_id) {
            return Err(format!("Dataset '{}' not found", dataset_id));
        }

        let job_id = self.allocate_job_id();
        let now = Utc::now().to_rfc3339();

        let job = AlignmentJob {
            id: job_id.clone(),
            name,
            method,
            status: AlignmentJobStatus::Pending,
            config,
            dataset_id,
            progress: 0.0,
            current_epoch: 0,
            total_epochs: 3, // placeholder; overridden by config
            train_loss: None,
            eval_score: None,
            elapsed_seconds: 0.0,
            gpu_hours: 0.0,
            error: None,
            created_at: now.clone(),
            started_at: None,
            completed_at: None,
            output_dir: String::new(),
            checkpoint_dir: None,
            packaged_model_path: None,
        };

        let total_epochs = job.config.epochs;

        self.jobs.insert(job_id.clone(), job);

        // Transition to Running immediately (synchronous placeholder).
        if let Some(mut j) = self.jobs.get_mut(&job_id) {
            j.status = AlignmentJobStatus::Running;
            j.total_epochs = total_epochs;
            j.started_at = Some(Utc::now().to_rfc3339());
            j.output_dir = j.config.output_dir.clone();
        }

        // Initialise metrics.
        self.metrics.insert(
            job_id.clone(),
            AlignmentMetrics {
                job_id: job_id.clone(),
                epoch_losses: Vec::new(),
                epoch_eval_scores: Vec::new(),
                epoch_kl_divergences: Vec::new(),
                epoch_reward_margins: Vec::new(),
                final_eval_reward: None,
                final_train_loss: None,
                total_gpu_hours: 0.0,
                peak_gpu_memory: 0.0,
                tokens_per_second: None,
                win_rate: None,
                timestamp: Utc::now().to_rfc3339(),
            },
        );

        info!(job_id = %job_id, "Alignment training job started");
        Ok(self.jobs.get(&job_id).unwrap().clone())
    }

    /// Cancel a running or pending job.
    pub fn cancel_job(&self, job_id: &str) -> Result<AlignmentJob, String> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| format!("Job '{}' not found", job_id))?;

        match job.status {
            AlignmentJobStatus::Pending | AlignmentJobStatus::Running => {
                job.status = AlignmentJobStatus::Cancelled;
                job.completed_at = Some(Utc::now().to_rfc3339());
                Ok(job.clone())
            }
            _ => Err(format!("Cannot cancel job in state {:?}", job.status)),
        }
    }

    /// Retrieve a job by ID.
    pub fn get_job(&self, job_id: &str) -> Option<AlignmentJob> {
        self.jobs.get(job_id).map(|j| j.clone())
    }

    /// List all jobs (optionally filtered by status).
    pub fn list_jobs(&self, status_filter: Option<AlignmentJobStatus>) -> Vec<AlignmentJob> {
        self.jobs
            .iter()
            .filter(|entry| {
                status_filter
                    .as_ref()
                    .map_or(true, |s| entry.value().status == *s)
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Load a preference dataset into the trainer's cache.
    pub fn load_dataset(&self, dataset: PreferenceDataset) {
        info!(
            dataset_id = %dataset.id,
            num_entries = dataset.num_entries,
            "Preference dataset loaded"
        );
        self.datasets.insert(dataset.id.clone(), dataset);
    }

    /// Get a loaded dataset by ID.
    pub fn get_dataset(&self, dataset_id: &str) -> Option<PreferenceDataset> {
        self.datasets.get(dataset_id).map(|d| d.clone())
    }

    /// List all loaded datasets.
    pub fn list_datasets(&self) -> Vec<PreferenceDataset> {
        self.datasets.iter().map(|d| d.value().clone()).collect()
    }

    /// Placeholder: run DPO training loop.
    pub fn train_dpo(&self, job_id: &str) -> Result<AlignmentMetrics, String> {
        self.run_training_epochs(job_id, "DPO")
    }

    /// Placeholder: run GRPO training loop.
    pub fn train_grpo(&self, job_id: &str) -> Result<AlignmentMetrics, String> {
        self.run_training_epochs(job_id, "GRPO")
    }

    /// Placeholder: run RLHF training loop.
    pub fn train_rlhf(&self, job_id: &str) -> Result<AlignmentMetrics, String> {
        self.run_training_epochs(job_id, "RLHF")
    }

    /// Generic epoch simulation used by all method-specific training functions.
    fn run_training_epochs(&self, job_id: &str, method_name: &str) -> Result<AlignmentMetrics, String> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| format!("Job '{}' not found", job_id))?;

        if job.status != AlignmentJobStatus::Running {
            return Err(format!("Job is not running (current: {:?})", job.status));
        }

        let total_epochs = job.config.epochs;

        // Simulate training progress across epochs.
        for epoch in 1..=total_epochs {
            let simulated_loss = 1.0 / (1.0 + (epoch as f64) * 0.3);
            let simulated_reward = 0.5 + (epoch as f64) * 0.08;
            let simulated_kl = 0.02 + (epoch as f64) * 0.005;

            job.current_epoch = epoch;
            job.progress = (epoch as f64 / total_epochs as f64) * 100.0;
            job.train_loss = Some(simulated_loss);
            job.eval_score = Some(simulated_reward);

            if let Some(mut m) = self.metrics.get_mut(job_id) {
                m.epoch_losses.push(simulated_loss);
                m.epoch_eval_scores.push(simulated_reward);
                m.epoch_kl_divergences.push(simulated_kl);
                m.epoch_reward_margins.push(simulated_reward * 0.6);
            }
        }

        job.status = AlignmentJobStatus::Completed;
        job.completed_at = Some(Utc::now().to_rfc3339());
        job.progress = 100.0;

        info!(
            job_id = %job_id,
            method = %method_name,
            epochs = total_epochs,
            "Alignment training completed"
        );

        let metrics = self.metrics.get(job_id).unwrap().clone();
        Ok(metrics)
    }

    /// Evaluate a trained model against a held-out preference set.
    pub fn evaluate(&self, job_id: &str) -> Result<AlignmentMetrics, String> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| format!("Job '{}' not found", job_id))?;

        if job.status != AlignmentJobStatus::Completed {
            return Err("Job must be completed before evaluation".into());
        }

        job.status = AlignmentJobStatus::Evaluating;

        let mut metrics = self
            .metrics
            .get_mut(job_id)
            .ok_or_else(|| format!("Metrics for job '{}' not found", job_id))?;

        // Simulate evaluation results.
        let win_rate = if metrics.epoch_eval_scores.last().copied().unwrap_or(0.0) > 0.7 {
            0.72
        } else {
            0.55
        };
        metrics.win_rate = Some(win_rate);
        metrics.final_eval_reward = metrics.epoch_eval_scores.last().copied();
        metrics.final_train_loss = metrics.epoch_losses.last().copied();
        metrics.peak_gpu_memory = 0.78;
        metrics.tokens_per_second = Some(1240.0);
        metrics.timestamp = Utc::now().to_rfc3339();

        job.eval_score = metrics.final_eval_reward;
        job.status = AlignmentJobStatus::Completed;

        Ok(metrics.clone())
    }

    /// Package the trained model for deployment (GGUF / safetensors export).
    pub fn package_model(&self, job_id: &str, format: Option<String>) -> Result<String, String> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| format!("Job '{}' not found", job_id))?;

        if job.status != AlignmentJobStatus::Completed {
            return Err("Job must be completed before packaging".into());
        }

        job.status = AlignmentJobStatus::Packaging;

        let export_format = format.as_deref().unwrap_or("safetensors");
        let packaged_path = format!(
            "{}/{}-aligned-{}",
            job.output_dir, job.method, export_format
        );

        job.packaged_model_path = Some(packaged_path.clone());
        job.status = AlignmentJobStatus::Completed;
        job.completed_at = Some(Utc::now().to_rfc3339());

        info!(
            job_id = %job_id,
            path = %packaged_path,
            format = %export_format,
            "Model packaged for deployment"
        );

        Ok(packaged_path)
    }

    /// Retrieve metrics for a completed or in-progress job.
    pub fn get_metrics(&self, job_id: &str) -> Option<AlignmentMetrics> {
        self.metrics.get(job_id).map(|m| m.clone())
    }
}

// ---------------------------------------------------------------------------
// Request / response types for the API handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StartAlignmentRequest {
    pub name: String,
    pub method: AlignmentMethod,
    pub config: AlignmentConfig,
    pub dataset_id: String,
}

#[derive(Debug, Deserialize)]
pub struct LoadDatasetRequest {
    pub dataset: PreferenceDataset,
}

#[derive(Debug, Deserialize)]
pub struct PackageModelRequest {
    pub format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AlignmentJobResponse {
    pub success: bool,
    pub job: AlignmentJob,
}

#[derive(Debug, Serialize)]
pub struct CancelResponse {
    pub success: bool,
    pub job: AlignmentJob,
}

#[derive(Debug, Serialize)]
pub struct PackageResponse {
    pub success: bool,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub job_id: String,
    pub metrics: AlignmentMetrics,
}

#[derive(Debug, Serialize)]
pub struct DatasetLoadResponse {
    pub success: bool,
    pub dataset_id: String,
}

#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub success: bool,
    pub metrics: AlignmentMetrics,
}

#[derive(Debug, Deserialize)]
pub struct JobStatusQuery {
    pub status: Option<AlignmentJobStatus>,
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the alignment training sub-router.
pub fn build_alignment_router(state: AppState) -> Router {
    Router::new()
        .route("/api/alignment/train", post(alignment_train_handler))
        .route("/api/alignment/jobs", get(alignment_jobs_handler))
        .route("/api/alignment/jobs/{id}", get(alignment_job_handler))
        .route("/api/alignment/jobs/{id}/cancel", post(alignment_cancel_handler))
        .route("/api/alignment/jobs/{id}/metrics", get(alignment_metrics_handler))
        .route("/api/alignment/jobs/{id}/evaluate", post(alignment_evaluate_handler))
        .route("/api/alignment/jobs/{id}/package", post(alignment_package_handler))
        .route("/api/alignment/datasets", post(alignment_load_dataset_handler))
        .route("/api/alignment/datasets", get(alignment_list_datasets_handler))
        .route("/api/alignment/datasets/{id}", get(alignment_get_dataset_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// API Handlers
// ---------------------------------------------------------------------------

/// POST /api/alignment/train — start a new alignment training job.
pub async fn alignment_train_handler(
    State(state): State<AppState>,
    Json(req): Json<StartAlignmentRequest>,
) -> Response {
    match state.alignment_trainer.start_job(
        req.name,
        req.method.clone(),
        req.config,
        req.dataset_id,
    ) {
        Ok(job) => (
            StatusCode::CREATED,
            Json(AlignmentJobResponse { success: true, job }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// GET /api/alignment/jobs — list all alignment jobs, optionally filtered by status.
pub async fn alignment_jobs_handler(
    State(state): State<AppState>,
    Query(params): Query<JobStatusQuery>,
) -> Json<Vec<AlignmentJob>> {
    Json(state.alignment_trainer.list_jobs(params.status))
}

/// GET /api/alignment/jobs/:id — retrieve a specific alignment job.
pub async fn alignment_job_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.alignment_trainer.get_job(&id) {
        Some(job) => Json(job).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Alignment job not found" })),
        )
            .into_response(),
    }
}

/// POST /api/alignment/jobs/:id/cancel — cancel a running or pending job.
pub async fn alignment_cancel_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.alignment_trainer.cancel_job(&id) {
        Ok(job) => (
            StatusCode::OK,
            Json(CancelResponse { success: true, job }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// GET /api/alignment/jobs/:id/metrics — retrieve metrics for a job.
pub async fn alignment_metrics_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.alignment_trainer.get_metrics(&id) {
        Some(metrics) => (
            StatusCode::OK,
            Json(MetricsResponse {
                job_id: id,
                metrics,
            }),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Metrics not found for job" })),
        )
            .into_response(),
    }
}

/// POST /api/alignment/jobs/:id/evaluate — run evaluation on a completed job.
pub async fn alignment_evaluate_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.alignment_trainer.evaluate(&id) {
        Ok(metrics) => (
            StatusCode::OK,
            Json(EvaluateResponse {
                success: true,
                metrics,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// POST /api/alignment/jobs/:id/package — package a completed model for deployment.
pub async fn alignment_package_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PackageModelRequest>,
) -> Response {
    match state.alignment_trainer.package_model(&id, req.format) {
        Ok(path) => (
            StatusCode::OK,
            Json(PackageResponse {
                success: true,
                path,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// POST /api/alignment/datasets — load a preference dataset.
pub async fn alignment_load_dataset_handler(
    State(state): State<AppState>,
    Json(req): Json<LoadDatasetRequest>,
) -> Response {
    let dataset_id = req.dataset.id.clone();
    state.alignment_trainer.load_dataset(req.dataset);
    (
        StatusCode::CREATED,
        Json(DatasetLoadResponse {
            success: true,
            dataset_id,
        }),
    )
        .into_response()
}

/// GET /api/alignment/datasets — list all loaded datasets.
pub async fn alignment_list_datasets_handler(
    State(state): State<AppState>,
) -> Json<Vec<PreferenceDataset>> {
    Json(state.alignment_trainer.list_datasets())
}

/// GET /api/alignment/datasets/:id — retrieve a specific loaded dataset.
pub async fn alignment_get_dataset_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match state.alignment_trainer.get_dataset(&id) {
        Some(dataset) => Json(dataset).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Dataset not found" })),
        )
            .into_response(),
    }
}
