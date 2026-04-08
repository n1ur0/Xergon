//! Federated learning coordination for the Xergon agent.
//!
//! Provides:
//! - Federated training round coordination across multiple providers
//! - FedAvg / FedProx aggregation of weight deltas
//! - Knowledge distillation pipeline (teacher -> student)
//! - Cross-provider distributed training (data-parallel, model-parallel)
//!
//! API:
//! - POST   /api/federated/start           -- start a federated training round
//! - POST   /api/federated/join            -- provider joins a training round
//! - POST   /api/federated/submit-delta    -- submit weight deltas after local training
//! - GET    /api/federated/rounds          -- list training rounds
//! - GET    /api/federated/rounds/{id}     -- round status + participant progress
//! - POST   /api/federated/aggregate       -- trigger manual aggregation
//! - DELETE /api/federated/rounds/{id}     -- cancel a round
//! - POST   /api/distillation/start        -- start knowledge distillation job
//! - GET    /api/distillation/jobs         -- list distillation jobs
//! - GET    /api/distillation/jobs/{id}    -- distillation job status
//! - POST   /api/cross-provider/train      -- start cross-provider training
//! - GET    /api/cross-provider/jobs       -- list cross-provider jobs
//! - GET    /api/cross-provider/jobs/{id}  -- job status

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

use crate::api::AppState;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// Status of a federated training round.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    /// Accepting provider registrations.
    Collecting,
    /// Providers are training locally.
    Training,
    /// Aggregating submitted weight deltas.
    Aggregating,
    /// Round finished successfully.
    Complete,
    /// Round was cancelled or failed.
    Failed,
    /// Round was explicitly cancelled.
    Cancelled,
}

impl std::fmt::Display for RoundStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Collecting => write!(f, "collecting"),
            Self::Training => write!(f, "training"),
            Self::Aggregating => write!(f, "aggregating"),
            Self::Complete => write!(f, "complete"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Aggregation strategy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    FedAvg,
    FedProx,
}

impl Default for AggregationStrategy {
    fn default() -> Self {
        Self::FedAvg
    }
}

/// Configuration for federated round aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationConfig {
    /// Minimum number of providers required before aggregation can proceed.
    #[serde(default = "default_min_participants")]
    pub min_participants: u32,

    /// Maximum number of training rounds.
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,

    /// Learning rate applied during aggregation.
    #[serde(default = "default_learning_rate")]
    pub learning_rate: f64,

    /// Aggregation algorithm.
    #[serde(default)]
    pub aggregation_strategy: AggregationStrategy,

    /// FedProx proximal term mu (only used when strategy is FedProx).
    #[serde(default = "default_fedprox_mu")]
    pub fedprox_mu: f64,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        Self {
            min_participants: default_min_participants(),
            max_rounds: default_max_rounds(),
            learning_rate: default_learning_rate(),
            aggregation_strategy: AggregationStrategy::default(),
            fedprox_mu: default_fedprox_mu(),
        }
    }
}

/// A weight delta submitted by a provider after local training.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightDelta {
    /// Base64-encoded serialized gradient updates.
    pub delta_base64: String,

    /// Provider that produced this delta.
    pub provider_id: String,

    /// Number of samples in the provider's local dataset (for weighting).
    pub dataset_size: u64,

    /// When this delta was submitted.
    pub timestamp: DateTime<Utc>,

    /// Optional checksum for integrity verification.
    #[serde(default)]
    pub checksum: Option<String>,

    /// Number of local training epochs performed.
    #[serde(default = "default_local_epochs")]
    pub local_epochs: u32,

    /// Local training loss after training.
    #[serde(default)]
    pub local_loss: Option<f64>,
}

/// Participant in a federated round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// Provider identifier.
    pub provider_id: String,

    /// Size of the provider's local dataset.
    pub dataset_size: u64,

    /// When the provider joined the round.
    pub joined_at: DateTime<Utc>,

    /// Current status of this participant.
    pub status: ParticipantStatus,

    /// Weight delta submitted by this provider (if any).
    pub delta: Option<WeightDelta>,
}

/// Status of a participant within a federated round.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStatus {
    /// Joined but not yet training.
    Registered,
    /// Actively training locally.
    Training,
    /// Submitted weight delta.
    Submitted,
    /// Training failed.
    Failed,
}

/// A federated training round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedRound {
    /// Unique round identifier.
    pub round_id: u64,

    /// Model being trained.
    pub model_id: String,

    /// Current round number (within a multi-round federated session).
    pub round_number: u32,

    /// Current status.
    pub status: RoundStatus,

    /// Aggregation configuration.
    pub config: AggregationConfig,

    /// Registered participants.
    pub participants: HashMap<String, Participant>,

    /// Aggregated global model delta (base64-encoded, set after aggregation).
    #[serde(default)]
    pub aggregated_delta: Option<String>,

    /// Deadline for delta submissions.
    pub deadline: DateTime<Utc>,

    /// When the round was created.
    pub created_at: DateTime<Utc>,

    /// When the round completed.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message if the round failed.
    #[serde(default)]
    pub error: Option<String>,

    /// Total number of samples across all participants.
    #[serde(default)]
    pub total_samples: u64,
}

// ---------------------------------------------------------------------------
// Distillation types
// ---------------------------------------------------------------------------

/// Status of a distillation job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DistillationStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for DistillationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Configuration for knowledge distillation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationConfig {
    /// Teacher model identifier (and optionally provider).
    pub teacher_model: String,

    /// Student model identifier (and optionally provider).
    pub student_model: String,

    /// Temperature for softmax in distillation loss.
    #[serde(default = "default_temperature")]
    pub temperature: f64,

    /// Weight for distillation loss (alpha). Total loss = alpha * distill + (1-alpha) * ce.
    #[serde(default = "default_alpha")]
    pub alpha: f64,

    /// Number of distillation epochs.
    #[serde(default = "default_epochs")]
    pub epochs: u32,

    /// Batch size for distillation training.
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,

    /// Learning rate for student model.
    #[serde(default = "default_learning_rate")]
    pub learning_rate: f64,

    /// Provider hosting the teacher model.
    #[serde(default)]
    pub teacher_provider: Option<String>,

    /// Provider hosting the student model.
    #[serde(default)]
    pub student_provider: Option<String>,

    /// Dataset path for distillation ( unlabeled data used for distillation ).
    #[serde(default)]
    pub dataset_path: Option<String>,
}

impl DistillationConfig {
    /// Validate the distillation configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.teacher_model.is_empty() {
            return Err("teacher_model must not be empty".into());
        }
        if self.student_model.is_empty() {
            return Err("student_model must not be empty".into());
        }
        if self.teacher_model == self.student_model {
            return Err("teacher_model and student_model must be different".into());
        }
        if self.temperature <= 0.0 {
            return Err("temperature must be positive".into());
        }
        if self.temperature > 100.0 {
            return Err("temperature must be <= 100.0".into());
        }
        if self.alpha < 0.0 || self.alpha > 1.0 {
            return Err("alpha must be in [0.0, 1.0]".into());
        }
        if self.epochs == 0 {
            return Err("epochs must be >= 1".into());
        }
        if self.epochs > 1000 {
            return Err("epochs must be <= 1000".into());
        }
        if self.learning_rate <= 0.0 {
            return Err("learning_rate must be positive".into());
        }
        if self.learning_rate > 1.0 {
            return Err("learning_rate must be <= 1.0".into());
        }
        Ok(())
    }
}

/// A knowledge distillation job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationJob {
    /// Unique job identifier.
    pub job_id: u64,

    /// Distillation configuration.
    pub config: DistillationConfig,

    /// Current status.
    pub status: DistillationStatus,

    /// Current epoch progress.
    #[serde(default)]
    pub current_epoch: u32,

    /// Distillation loss (KL divergence component).
    #[serde(default)]
    pub distill_loss: Option<f64>,

    /// Cross-entropy loss component.
    #[serde(default)]
    pub ce_loss: Option<f64>,

    /// Combined loss.
    #[serde(default)]
    pub total_loss: Option<f64>,

    /// When the job was created.
    pub created_at: DateTime<Utc>,

    /// When the job started.
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,

    /// When the job completed.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message if the job failed.
    #[serde(default)]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Cross-provider training types
// ---------------------------------------------------------------------------

/// Training strategy for cross-provider jobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossProviderStrategy {
    /// Same model on each provider, different data batches.
    DataParallel,
    /// Different model layers on different providers.
    ModelParallel,
}

impl std::fmt::Display for CrossProviderStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DataParallel => write!(f, "data_parallel"),
            Self::ModelParallel => write!(f, "model_parallel"),
        }
    }
}

/// Status of a cross-provider training job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossProviderStatus {
    Pending,
    SettingUp,
    Training,
    Synchronising,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for CrossProviderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::SettingUp => write!(f, "setting_up"),
            Self::Training => write!(f, "training"),
            Self::Synchronising => write!(f, "synchronising"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Configuration for cross-provider training.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossProviderConfig {
    /// Training strategy.
    pub strategy: CrossProviderStrategy,

    /// Number of worker providers.
    #[serde(default = "default_num_workers")]
    pub num_workers: u32,

    /// Batch size per worker.
    #[serde(default = "default_batch_size")]
    pub batch_size_per_worker: u32,

    /// Gradient synchronisation interval in steps.
    #[serde(default = "default_sync_interval")]
    pub sync_interval: u32,

    /// Model identifier.
    pub model_id: String,

    /// Total training epochs.
    #[serde(default = "default_epochs")]
    pub epochs: u32,

    /// Learning rate.
    #[serde(default = "default_learning_rate")]
    pub learning_rate: f64,

    /// List of provider IDs assigned to this job.
    #[serde(default)]
    pub providers: Vec<String>,
}

impl CrossProviderConfig {
    /// Validate the cross-provider configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.model_id.is_empty() {
            return Err("model_id must not be empty".into());
        }
        if self.num_workers == 0 {
            return Err("num_workers must be >= 1".into());
        }
        if self.num_workers > 256 {
            return Err("num_workers must be <= 256".into());
        }
        if self.batch_size_per_worker == 0 {
            return Err("batch_size_per_worker must be >= 1".into());
        }
        if self.sync_interval == 0 {
            return Err("sync_interval must be >= 1".into());
        }
        if self.epochs == 0 {
            return Err("epochs must be >= 1".into());
        }
        if self.learning_rate <= 0.0 {
            return Err("learning_rate must be positive".into());
        }
        Ok(())
    }
}

/// A cross-provider training job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossProviderJob {
    /// Unique job identifier.
    pub job_id: u64,

    /// Training configuration.
    pub config: CrossProviderConfig,

    /// Current status.
    pub status: CrossProviderStatus,

    /// Current epoch progress.
    #[serde(default)]
    pub current_epoch: u32,

    /// Current step within the epoch.
    #[serde(default)]
    pub current_step: u32,

    /// Total steps completed across all workers.
    #[serde(default)]
    pub total_steps: u64,

    /// Training loss.
    #[serde(default)]
    pub loss: Option<f64>,

    /// Per-worker progress.
    #[serde(default)]
    pub worker_progress: HashMap<String, WorkerProgress>,

    /// When the job was created.
    pub created_at: DateTime<Utc>,

    /// When the job started.
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,

    /// When the job completed.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message if the job failed.
    #[serde(default)]
    pub error: Option<String>,
}

/// Progress of a single worker in a cross-provider job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProgress {
    /// Provider ID.
    pub provider_id: String,

    /// Steps completed by this worker.
    pub steps_completed: u64,

    /// Current loss on this worker.
    pub loss: Option<f64>,

    /// Worker status.
    pub status: String,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Thread-safe state for federated learning operations.
#[derive(Debug, Clone)]
pub struct FederatedState {
    /// Next round ID counter.
    pub next_round_id: Arc<AtomicU64>,

    /// Next distillation job ID counter.
    pub next_distill_id: Arc<AtomicU64>,

    /// Next cross-provider job ID counter.
    pub next_cross_id: Arc<AtomicU64>,

    /// Active federated rounds.
    pub rounds: Arc<DashMap<u64, FederatedRound>>,

    /// Active distillation jobs.
    pub distill_jobs: Arc<DashMap<u64, DistillationJob>>,

    /// Active cross-provider training jobs.
    pub cross_jobs: Arc<DashMap<u64, CrossProviderJob>>,
}

impl FederatedState {
    /// Create a new federated state.
    pub fn new() -> Self {
        Self {
            next_round_id: Arc::new(AtomicU64::new(1)),
            next_distill_id: Arc::new(AtomicU64::new(1)),
            next_cross_id: Arc::new(AtomicU64::new(1)),
            rounds: Arc::new(DashMap::new()),
            distill_jobs: Arc::new(DashMap::new()),
            cross_jobs: Arc::new(DashMap::new()),
        }
    }

    /// Generate a new round ID.
    pub fn next_round_id_val(&self) -> u64 {
        self.next_round_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Generate a new distillation job ID.
    pub fn next_distill_id_val(&self) -> u64 {
        self.next_distill_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Generate a new cross-provider job ID.
    pub fn next_cross_id_val(&self) -> u64 {
        self.next_cross_id.fetch_add(1, Ordering::SeqCst)
    }
}

impl Default for FederatedState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// FedAvg aggregation logic
// ---------------------------------------------------------------------------

/// Perform Federated Averaging on a set of weight deltas.
///
/// Given N deltas with associated dataset sizes, computes:
///   global_delta = sum(delta_i * n_i) / sum(n_i)
///
/// Since we work with base64-encoded deltas, the actual aggregation produces
/// a placeholder result (the real GPU-side aggregation would happen on
/// the provider nodes). This function computes the weighted coefficients
/// and returns the aggregated metadata + a mock base64 payload.
pub fn federated_average(deltas: &[WeightDelta]) -> Result<AggregatedDelta, String> {
    if deltas.is_empty() {
        return Err("no deltas to aggregate".into());
    }

    let total_samples: u64 = deltas.iter().map(|d| d.dataset_size).sum();
    if total_samples == 0 {
        return Err("total dataset size is zero".into());
    }

    // Compute per-provider weight
    let weights: Vec<f64> = deltas
        .iter()
        .map(|d| d.dataset_size as f64 / total_samples as f64)
        .collect();

    // Verify weights sum to ~1.0
    let weight_sum: f64 = weights.iter().sum();
    if (weight_sum - 1.0).abs() > 1e-9 {
        return Err(format!("weight sum {} != 1.0", weight_sum));
    }

    // Average the local losses if available
    let avg_loss = deltas
        .iter()
        .filter_map(|d| d.local_loss)
        .collect::<Vec<_>>()
        .into_iter()
        .sum::<f64>()
        / deltas.iter().filter(|d| d.local_loss.is_some()).count().max(1) as f64;

    // In a real implementation, we would decode the base64 deltas, multiply
    // each by its weight, and sum them. Here we produce a deterministic
    // placeholder by concatenating the base64 payloads.
    let aggregated_payload = deltas
        .iter()
        .map(|d| d.delta_base64.as_str())
        .collect::<Vec<_>>()
        .join("|");

    Ok(AggregatedDelta {
        delta_base64: aggregated_payload,
        total_samples,
        num_providers: deltas.len() as u32,
        weights,
        avg_loss: Some(avg_loss),
    })
}

/// Result of federated aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedDelta {
    /// Aggregated weight delta (base64-encoded).
    pub delta_base64: String,

    /// Total number of samples across all providers.
    pub total_samples: u64,

    /// Number of providers contributing.
    pub num_providers: u32,

    /// Per-provider weight used in aggregation.
    pub weights: Vec<f64>,

    /// Average training loss across providers.
    pub avg_loss: Option<f64>,
}

/// Perform FedProx aggregation (FedAvg with proximal term).
///
/// FedProx adds a proximal term to the local objective:
///   local_obj = loss(w) + (mu/2) * ||w - w_global||^2
///
/// The server-side aggregation is still a weighted average, but the proximal
/// term penalises large deviations from the global model.
pub fn fedprox_aggregate(
    deltas: &[WeightDelta],
    mu: f64,
) -> Result<AggregatedDelta, String> {
    if mu < 0.0 {
        return Err("FedProx mu must be non-negative".into());
    }
    // Server-side aggregation is the same as FedAvg.
    // The proximal term only affects client-side training.
    let mut result = federated_average(deltas)?;
    // In a real implementation, mu would be communicated to clients
    // and used during their local training. We record it here for
    // downstream consumption.
    result.delta_base64 = format!("fedprox_mu={}|{}", mu, result.delta_base64);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Temperature-scaled softmax and KL divergence for distillation
// ---------------------------------------------------------------------------

/// Compute temperature-scaled softmax: softmax(logits / T).
///
/// For a vector of logits, applies temperature scaling before softmax.
pub fn temperature_softmax(logits: &[f64], temperature: f64) -> Vec<f64> {
    if logits.is_empty() || temperature <= 0.0 {
        return vec![];
    }

    let scaled: Vec<f64> = logits.iter().map(|l| l / temperature).collect();

    // Find max for numerical stability
    let max_val = scaled.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let exp_vals: Vec<f64> = scaled.iter().map(|s| (s - max_val).exp()).collect();
    let sum_exp: f64 = exp_vals.iter().sum();

    exp_vals.iter().map(|e| e / sum_exp).collect()
}

/// Compute KL divergence: KL(P || Q) = sum(P * log(P / Q)).
///
/// P and Q must be probability distributions (sum to 1.0).
pub fn kl_divergence(p: &[f64], q: &[f64]) -> Result<f64, String> {
    if p.len() != q.len() {
        return Err(format!(
            "distributions must have same length: {} vs {}",
            p.len(),
            q.len()
        ));
    }
    if p.is_empty() {
        return Err("distributions must not be empty".into());
    }

    let eps = 1e-10;
    let kl: f64 = p
        .iter()
        .zip(q.iter())
        .map(|(pi, qi)| {
            let pi = pi.max(eps);
            let qi = qi.max(eps);
            pi * (pi / qi).ln()
        })
        .sum();

    Ok(kl)
}

/// Compute the distillation loss: L = alpha * T^2 * KL(softmax(teacher/T) || softmax(student/T)) + (1 - alpha) * CE(student, labels).
///
/// When labels are None, only the KL component is returned.
pub fn distillation_loss(
    teacher_logits: &[f64],
    student_logits: &[f64],
    temperature: f64,
    alpha: f64,
    hard_labels: Option<&[f64]>,
) -> Result<DistillationLossResult, String> {
    if teacher_logits.len() != student_logits.len() {
        return Err(format!(
            "teacher and student logits must have same length: {} vs {}",
            teacher_logits.len(),
            student_logits.len()
        ));
    }
    if temperature <= 0.0 {
        return Err("temperature must be positive".into());
    }

    // Temperature-scaled softmax
    let teacher_probs = temperature_softmax(teacher_logits, temperature);
    let student_probs = temperature_softmax(student_logits, temperature);

    // KL divergence scaled by T^2
    let kl = kl_divergence(&teacher_probs, &student_probs)?;
    let scaled_kl = kl * temperature * temperature;

    // Cross-entropy with hard labels (if provided)
    let ce = if let Some(labels) = hard_labels {
        Some(cross_entropy_loss(&student_probs, labels)?)
    } else {
        None
    };

    // Combined loss
    let total = match ce {
        Some(ce_val) => alpha * scaled_kl + (1.0 - alpha) * ce_val,
        None => scaled_kl, // pure distillation if no hard labels
    };

    Ok(DistillationLossResult {
        kl_loss: scaled_kl,
        ce_loss: ce,
        total_loss: total,
        alpha,
        temperature,
    })
}

/// Cross-entropy loss: -sum(labels * log(probs)).
pub fn cross_entropy_loss(probs: &[f64], labels: &[f64]) -> Result<f64, String> {
    if probs.len() != labels.len() {
        return Err(format!(
            "probs and labels length mismatch: {} vs {}",
            probs.len(),
            labels.len()
        ));
    }

    let eps = 1e-10;
    let ce: f64 = probs
        .iter()
        .zip(labels.iter())
        .map(|(p, l)| -l * p.max(eps).ln())
        .sum();

    Ok(ce)
}

/// Result of distillation loss computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationLossResult {
    /// KL divergence component (scaled by T^2).
    pub kl_loss: f64,

    /// Cross-entropy component (if hard labels were provided).
    pub ce_loss: Option<f64>,

    /// Combined total loss.
    pub total_loss: f64,

    /// Alpha weight used.
    pub alpha: f64,

    /// Temperature used.
    pub temperature: f64,
}

// ---------------------------------------------------------------------------
// Round lifecycle management
// ---------------------------------------------------------------------------

/// Start a new federated training round.
pub fn start_round(
    state: &FederatedState,
    model_id: String,
    config: AggregationConfig,
    deadline_secs: i64,
) -> Result<FederatedRound, String> {
    if model_id.is_empty() {
        return Err("model_id must not be empty".into());
    }
    if config.min_participants == 0 {
        return Err("min_participants must be >= 1".into());
    }

    let round_id = state.next_round_id_val();
    let now = Utc::now();
    let deadline = now + Duration::seconds(deadline_secs);

    let round = FederatedRound {
        round_id,
        model_id,
        round_number: 1,
        status: RoundStatus::Collecting,
        config,
        participants: HashMap::new(),
        aggregated_delta: None,
        deadline,
        created_at: now,
        completed_at: None,
        error: None,
        total_samples: 0,
    };

    state.rounds.insert(round_id, round.clone());
    info!(round_id = round_id, "Started new federated training round");
    Ok(round)
}

/// Join a provider to an existing round.
pub fn join_round(
    state: &FederatedState,
    round_id: u64,
    provider_id: String,
    dataset_size: u64,
) -> Result<Participant, String> {
    if provider_id.is_empty() {
        return Err("provider_id must not be empty".into());
    }
    if dataset_size == 0 {
        return Err("dataset_size must be > 0".into());
    }

    let mut round = state
        .rounds
        .get_mut(&round_id)
        .ok_or_else(|| format!("round {} not found", round_id))?;

    if round.status != RoundStatus::Collecting {
        return Err(format!(
            "round {} is in {} status, cannot join",
            round_id, round.status
        ));
    }

    if round.participants.contains_key(&provider_id) {
        return Err(format!(
            "provider {} already joined round {}",
            provider_id, round_id
        ));
    }

    let participant = Participant {
        provider_id: provider_id.clone(),
        dataset_size,
        joined_at: Utc::now(),
        status: ParticipantStatus::Registered,
        delta: None,
    };

    round.participants.insert(provider_id.clone(), participant.clone());
    round.total_samples += dataset_size;

    debug!(
        round_id = round_id,
        provider_id = provider_id,
        "Provider joined federated round"
    );
    Ok(participant)
}

/// Submit a weight delta from a provider.
pub fn submit_delta(
    state: &FederatedState,
    round_id: u64,
    delta: WeightDelta,
) -> Result<(), String> {
    let mut round = state
        .rounds
        .get_mut(&round_id)
        .ok_or_else(|| format!("round {} not found", round_id))?;

    if round.status != RoundStatus::Training && round.status != RoundStatus::Collecting {
        return Err(format!(
            "round {} is in {} status, cannot submit delta",
            round_id, round.status
        ));
    }

    let provider_id = delta.provider_id.clone();
    let participant = round
        .participants
        .get_mut(&provider_id)
        .ok_or_else(|| format!("provider {} not in round {}", provider_id, round_id))?;

    participant.delta = Some(delta);
    participant.status = ParticipantStatus::Submitted;

    debug!(
        round_id = round_id,
        provider_id = provider_id,
        "Weight delta submitted"
    );

    // Check if all participants have submitted
    let all_submitted = round
        .participants
        .values()
        .all(|p| p.status == ParticipantStatus::Submitted);

    if all_submitted && round.participants.len() >= round.config.min_participants as usize {
        // Transition to aggregating
        // Note: We clone to release the borrow before aggregating
        drop(round);
        aggregate_round(state, round_id)?;
    }

    Ok(())
}

/// Trigger aggregation for a round.
pub fn aggregate_round(state: &FederatedState, round_id: u64) -> Result<AggregatedDelta, String> {
    let mut round = state
        .rounds
        .get_mut(&round_id)
        .ok_or_else(|| format!("round {} not found", round_id))?;

    if round.status == RoundStatus::Complete {
        return Err(format!("round {} already complete", round_id));
    }
    if round.status == RoundStatus::Cancelled || round.status == RoundStatus::Failed {
        return Err(format!("round {} is {}", round_id, round.status));
    }

    round.status = RoundStatus::Aggregating;

    // Collect submitted deltas
    let deltas: Vec<WeightDelta> = round
        .participants
        .values()
        .filter_map(|p| p.delta.clone())
        .collect();

    if deltas.len() < round.config.min_participants as usize {
        round.status = RoundStatus::Failed;
        round.error = Some(format!(
            "not enough deltas: {} < {}",
            deltas.len(),
            round.config.min_participants
        ));
        return Err(round.error.clone().unwrap());
    }

    drop(round); // Release borrow before calling pure aggregation

    // Perform aggregation
    let aggregated = match round_with_aggregation(state, round_id, &deltas) {
        Ok(a) => a,
        Err(e) => {
            let mut round = state.rounds.get_mut(&round_id).unwrap();
            round.status = RoundStatus::Failed;
            round.error = Some(e.clone());
            return Err(e);
        }
    };

    Ok(aggregated)
}

/// Internal helper: perform aggregation on collected deltas and update round state.
fn round_with_aggregation(
    state: &FederatedState,
    round_id: u64,
    deltas: &[WeightDelta],
) -> Result<AggregatedDelta, String> {
    let mut round = state.rounds.get_mut(&round_id).unwrap();

    let result = match round.config.aggregation_strategy {
        AggregationStrategy::FedAvg => federated_average(deltas)?,
        AggregationStrategy::FedProx => fedprox_aggregate(deltas, round.config.fedprox_mu)?,
    };

    round.aggregated_delta = Some(result.delta_base64.clone());
    round.status = RoundStatus::Complete;
    round.completed_at = Some(Utc::now());

    info!(
        round_id = round_id,
        num_providers = result.num_providers,
        total_samples = result.total_samples,
        "Federated round aggregation complete"
    );

    Ok(result)
}

/// Cancel a federated round.
pub fn cancel_round(state: &FederatedState, round_id: u64) -> Result<(), String> {
    let mut round = state
        .rounds
        .get_mut(&round_id)
        .ok_or_else(|| format!("round {} not found", round_id))?;

    match round.status {
        RoundStatus::Complete => return Err(format!("round {} already complete", round_id)),
        RoundStatus::Cancelled => return Err(format!("round {} already cancelled", round_id)),
        _ => {}
    }

    round.status = RoundStatus::Cancelled;
    round.completed_at = Some(Utc::now());

    info!(round_id = round_id, "Federated round cancelled");
    Ok(())
}

/// Get a round by ID.
pub fn get_round(state: &FederatedState, round_id: u64) -> Option<FederatedRound> {
    state.rounds.get(&round_id).map(|r| r.clone())
}

/// List all rounds.
pub fn list_rounds(state: &FederatedState) -> Vec<FederatedRound> {
    state.rounds.iter().map(|r| r.value().clone()).collect()
}

// ---------------------------------------------------------------------------
// Distillation job management
// ---------------------------------------------------------------------------

/// Start a new distillation job.
pub fn start_distillation(
    state: &FederatedState,
    config: DistillationConfig,
) -> Result<DistillationJob, String> {
    config.validate()?;

    let job_id = state.next_distill_id_val();
    let now = Utc::now();

    let job = DistillationJob {
        job_id,
        config,
        status: DistillationStatus::Pending,
        current_epoch: 0,
        distill_loss: None,
        ce_loss: None,
        total_loss: None,
        created_at: now,
        started_at: None,
        completed_at: None,
        error: None,
    };

    state.distill_jobs.insert(job_id, job.clone());
    info!(job_id = job_id, "Started distillation job");
    Ok(job)
}

/// Get a distillation job by ID.
pub fn get_distillation_job(state: &FederatedState, job_id: u64) -> Option<DistillationJob> {
    state.distill_jobs.get(&job_id).map(|j| j.clone())
}

/// List all distillation jobs.
pub fn list_distillation_jobs(state: &FederatedState) -> Vec<DistillationJob> {
    state
        .distill_jobs
        .iter()
        .map(|j| j.value().clone())
        .collect()
}

/// Cancel a distillation job.
pub fn cancel_distillation(state: &FederatedState, job_id: u64) -> Result<(), String> {
    let mut job = state
        .distill_jobs
        .get_mut(&job_id)
        .ok_or_else(|| format!("distillation job {} not found", job_id))?;

    match job.status {
        DistillationStatus::Completed => {
            return Err(format!("distillation job {} already complete", job_id))
        }
        DistillationStatus::Cancelled => {
            return Err(format!("distillation job {} already cancelled", job_id))
        }
        _ => {}
    }

    job.status = DistillationStatus::Cancelled;
    job.completed_at = Some(Utc::now());

    info!(job_id = job_id, "Distillation job cancelled");
    Ok(())
}

// ---------------------------------------------------------------------------
// Cross-provider job management
// ---------------------------------------------------------------------------

/// Start a new cross-provider training job.
pub fn start_cross_provider(
    state: &FederatedState,
    config: CrossProviderConfig,
) -> Result<CrossProviderJob, String> {
    config.validate()?;

    let job_id = state.next_cross_id_val();
    let now = Utc::now();

    // Initialize worker progress
    let worker_progress: HashMap<String, WorkerProgress> = config
        .providers
        .iter()
        .enumerate()
        .map(|(_i, pid)| {
            (
                pid.clone(),
                WorkerProgress {
                    provider_id: pid.clone(),
                    steps_completed: 0,
                    loss: None,
                    status: "pending".to_string(),
                },
            )
        })
        .collect();

    let job = CrossProviderJob {
        job_id,
        config,
        status: CrossProviderStatus::Pending,
        current_epoch: 0,
        current_step: 0,
        total_steps: 0,
        loss: None,
        worker_progress,
        created_at: now,
        started_at: None,
        completed_at: None,
        error: None,
    };

    state.cross_jobs.insert(job_id, job.clone());
    info!(job_id = job_id, "Started cross-provider training job");
    Ok(job)
}

/// Get a cross-provider job by ID.
pub fn get_cross_provider_job(state: &FederatedState, job_id: u64) -> Option<CrossProviderJob> {
    state.cross_jobs.get(&job_id).map(|j| j.clone())
}

/// List all cross-provider jobs.
pub fn list_cross_provider_jobs(state: &FederatedState) -> Vec<CrossProviderJob> {
    state
        .cross_jobs
        .iter()
        .map(|j| j.value().clone())
        .collect()
}

/// Cancel a cross-provider job.
pub fn cancel_cross_provider(state: &FederatedState, job_id: u64) -> Result<(), String> {
    let mut job = state
        .cross_jobs
        .get_mut(&job_id)
        .ok_or_else(|| format!("cross-provider job {} not found", job_id))?;

    match job.status {
        CrossProviderStatus::Completed => {
            return Err(format!("cross-provider job {} already complete", job_id))
        }
        CrossProviderStatus::Cancelled => {
            return Err(format!("cross-provider job {} already cancelled", job_id))
        }
        _ => {}
    }

    job.status = CrossProviderStatus::Cancelled;
    job.completed_at = Some(Utc::now());

    info!(job_id = job_id, "Cross-provider job cancelled");
    Ok(())
}

// ---------------------------------------------------------------------------
// Default values for serde
// ---------------------------------------------------------------------------

fn default_min_participants() -> u32 {
    3
}
fn default_max_rounds() -> u32 {
    100
}
fn default_learning_rate() -> f64 {
    0.01
}
fn default_fedprox_mu() -> f64 {
    0.01
}
fn default_temperature() -> f64 {
    4.0
}
fn default_alpha() -> f64 {
    0.7
}
fn default_epochs() -> u32 {
    3
}
fn default_batch_size() -> u32 {
    32
}
fn default_num_workers() -> u32 {
    2
}
fn default_sync_interval() -> u32 {
    50
}
fn default_local_epochs() -> u32 {
    1
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StartRoundRequest {
    pub model_id: String,
    #[serde(default)]
    pub config: AggregationConfig,
    #[serde(default = "default_deadline_secs")]
    pub deadline_secs: i64,
}

fn default_deadline_secs() -> i64 {
    3600
}

#[derive(Debug, Serialize)]
pub struct StartRoundResponse {
    pub round_id: u64,
    pub status: RoundStatus,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct JoinRoundRequest {
    pub round_id: u64,
    pub provider_id: String,
    pub dataset_size: u64,
}

#[derive(Debug, Serialize)]
pub struct JoinRoundResponse {
    pub round_id: u64,
    pub provider_id: String,
    pub status: ParticipantStatus,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitDeltaRequest {
    pub round_id: u64,
    pub delta_base64: String,
    pub provider_id: String,
    #[serde(default)]
    pub dataset_size: u64,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default = "default_local_epochs")]
    pub local_epochs: u32,
    #[serde(default)]
    pub local_loss: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct SubmitDeltaResponse {
    pub round_id: u64,
    pub provider_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct AggregateRequest {
    pub round_id: u64,
}

#[derive(Debug, Serialize)]
pub struct AggregateResponse {
    pub round_id: u64,
    pub status: RoundStatus,
    pub num_providers: u32,
    pub total_samples: u64,
    pub avg_loss: Option<f64>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct RoundStatusResponse {
    pub round: FederatedRound,
    pub participant_count: usize,
    pub submitted_count: usize,
    pub ready_to_aggregate: bool,
}

#[derive(Debug, Serialize)]
pub struct CancelRoundResponse {
    pub round_id: u64,
    pub status: RoundStatus,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct StartDistillationResponse {
    pub job_id: u64,
    pub status: DistillationStatus,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DistillationJobStatusResponse {
    pub job: DistillationJob,
}

#[derive(Debug, Serialize)]
pub struct StartCrossProviderResponse {
    pub job_id: u64,
    pub status: CrossProviderStatus,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct CrossProviderJobStatusResponse {
    pub job: CrossProviderJob,
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the federated learning sub-router.
pub fn build_federated_router(state: AppState) -> Router {
    Router::new()
        // Federated training round endpoints
        .route("/api/federated/start", post(start_round_handler))
        .route("/api/federated/join", post(join_round_handler))
        .route("/api/federated/submit-delta", post(submit_delta_handler))
        .route("/api/federated/rounds", get(list_rounds_handler))
        .route(
            "/api/federated/rounds/{id}",
            get(get_round_handler).delete(cancel_round_handler),
        )
        .route("/api/federated/aggregate", post(aggregate_handler))
        // Distillation endpoints
        .route("/api/distillation/start", post(start_distillation_handler))
        .route("/api/distillation/jobs", get(list_distillation_handler))
        .route("/api/distillation/jobs/{id}", get(get_distillation_handler))
        // Cross-provider endpoints
        .route("/api/cross-provider/train", post(start_cross_provider_handler))
        .route("/api/cross-provider/jobs", get(list_cross_provider_handler))
        .route("/api/cross-provider/jobs/{id}", get(get_cross_provider_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// API Handlers
// ---------------------------------------------------------------------------

/// POST /api/federated/start -- start a new federated training round.
async fn start_round_handler(
    State(state): State<AppState>,
    Json(req): Json<StartRoundRequest>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match start_round(fl, req.model_id, req.config, req.deadline_secs) {
            Ok(round) => (
                StatusCode::CREATED,
                Json(StartRoundResponse {
                    round_id: round.round_id,
                    status: round.status,
                    message: format!(
                        "Round {} created for model {}, deadline at {}",
                        round.round_id,
                        round.model_id,
                        round.deadline
                    ),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
    }
}

/// POST /api/federated/join -- provider joins a training round.
async fn join_round_handler(
    State(state): State<AppState>,
    Json(req): Json<JoinRoundRequest>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => {
            match join_round(fl, req.round_id, req.provider_id.clone(), req.dataset_size) {
                Ok(p) => {
                    let pid = p.provider_id.clone();
                    (StatusCode::OK,
                    Json(JoinRoundResponse {
                        round_id: req.round_id,
                        provider_id: p.provider_id,
                        status: p.status,
                        message: format!("Provider {} joined round {}", pid, req.round_id),
                    }),
                )
                    .into_response()
                }
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": e})),
                )
                    .into_response(),
            }
        }
    }
}

/// POST /api/federated/submit-delta -- submit weight deltas after local training.
async fn submit_delta_handler(
    State(state): State<AppState>,
    Json(req): Json<SubmitDeltaRequest>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => {
            let delta = WeightDelta {
                delta_base64: req.delta_base64,
                provider_id: req.provider_id.clone(),
                dataset_size: req.dataset_size,
                timestamp: Utc::now(),
                checksum: req.checksum,
                local_epochs: req.local_epochs,
                local_loss: req.local_loss,
            };
            match submit_delta(fl, req.round_id, delta) {
                Ok(()) => (
                    StatusCode::OK,
                    Json(SubmitDeltaResponse {
                        round_id: req.round_id,
                        provider_id: req.provider_id,
                        message: "Delta submitted successfully".into(),
                    }),
                )
                    .into_response(),
                Err(e) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": e})),
                )
                    .into_response(),
            }
        }
    }
}

/// GET /api/federated/rounds -- list training rounds.
async fn list_rounds_handler(State(state): State<AppState>) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => {
            let rounds = list_rounds(fl);
            (StatusCode::OK, Json(serde_json::json!({"rounds": rounds}))).into_response()
        }
    }
}

/// GET /api/federated/rounds/{id} -- get round status with participant progress.
async fn get_round_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match get_round(fl, id) {
            Some(round) => {
                let participant_count = round.participants.len();
                let submitted_count = round
                    .participants
                    .values()
                    .filter(|p| p.status == ParticipantStatus::Submitted)
                    .count();
                let ready_to_aggregate = submitted_count >= round.config.min_participants as usize
                    && submitted_count == participant_count
                    && participant_count > 0;
                (
                    StatusCode::OK,
                    Json(RoundStatusResponse {
                        round,
                        participant_count,
                        submitted_count,
                        ready_to_aggregate,
                    }),
                )
                    .into_response()
            }
            None => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("round {} not found", id)})),
            )
                .into_response(),
        },
    }
}

/// DELETE /api/federated/rounds/{id} -- cancel a round.
async fn cancel_round_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match cancel_round(fl, id) {
            Ok(()) => (
                StatusCode::OK,
                Json(CancelRoundResponse {
                    round_id: id,
                    status: RoundStatus::Cancelled,
                    message: format!("Round {} cancelled", id),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
    }
}

/// POST /api/federated/aggregate -- trigger manual aggregation.
async fn aggregate_handler(
    State(state): State<AppState>,
    Json(req): Json<AggregateRequest>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match aggregate_round(fl, req.round_id) {
            Ok(result) => (
                StatusCode::OK,
                Json(AggregateResponse {
                    round_id: req.round_id,
                    status: RoundStatus::Complete,
                    num_providers: result.num_providers,
                    total_samples: result.total_samples,
                    avg_loss: result.avg_loss,
                    message: format!(
                        "Aggregated {} deltas from {} samples",
                        result.num_providers, result.total_samples
                    ),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
    }
}

/// POST /api/distillation/start -- start knowledge distillation job.
async fn start_distillation_handler(
    State(state): State<AppState>,
    Json(req): Json<DistillationConfig>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match start_distillation(fl, req) {
            Ok(job) => (
                StatusCode::CREATED,
                Json(StartDistillationResponse {
                    job_id: job.job_id,
                    status: job.status,
                    message: format!(
                        "Distillation job {} started: {} -> {}",
                        job.job_id, job.config.teacher_model, job.config.student_model
                    ),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
    }
}

/// GET /api/distillation/jobs -- list distillation jobs.
async fn list_distillation_handler(State(state): State<AppState>) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => {
            let jobs = list_distillation_jobs(fl);
            (StatusCode::OK, Json(serde_json::json!({"jobs": jobs}))).into_response()
        }
    }
}

/// GET /api/distillation/jobs/{id} -- get distillation job status.
async fn get_distillation_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match get_distillation_job(fl, id) {
            Some(job) => (
                StatusCode::OK,
                Json(DistillationJobStatusResponse { job }),
            )
                .into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("distillation job {} not found", id)})),
            )
                .into_response(),
        },
    }
}

/// POST /api/cross-provider/train -- start cross-provider training.
async fn start_cross_provider_handler(
    State(state): State<AppState>,
    Json(req): Json<CrossProviderConfig>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match start_cross_provider(fl, req) {
            Ok(job) => (
                StatusCode::CREATED,
                Json(StartCrossProviderResponse {
                    job_id: job.job_id,
                    status: job.status,
                    message: format!(
                        "Cross-provider job {} started with {} workers ({})",
                        job.job_id,
                        job.config.num_workers,
                        job.config.strategy
                    ),
                }),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
    }
}

/// GET /api/cross-provider/jobs -- list cross-provider jobs.
async fn list_cross_provider_handler(State(state): State<AppState>) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => {
            let jobs = list_cross_provider_jobs(fl);
            (StatusCode::OK, Json(serde_json::json!({"jobs": jobs}))).into_response()
        }
    }
}

/// GET /api/cross-provider/jobs/{id} -- get job status.
async fn get_cross_provider_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    match state.federated_learning.as_ref() {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "federated learning not enabled"})),
        )
            .into_response(),
        Some(fl) => match get_cross_provider_job(fl, id) {
            Some(job) => (
                StatusCode::OK,
                Json(CrossProviderJobStatusResponse { job }),
            )
                .into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("cross-provider job {} not found", id)})),
            )
                .into_response(),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn new_state() -> FederatedState {
        FederatedState::new()
    }

    // -- Round lifecycle tests --

    #[test]
    fn test_start_round_basic() {
        let state = new_state();
        let round = start_round(
            &state,
            "llama-3.1-8b".to_string(),
            AggregationConfig::default(),
            3600,
        )
        .unwrap();

        assert_eq!(round.round_id, 1);
        assert_eq!(round.model_id, "llama-3.1-8b");
        assert_eq!(round.status, RoundStatus::Collecting);
        assert_eq!(round.config.min_participants, 3);
        assert!(round.participants.is_empty());
    }

    #[test]
    fn test_start_round_empty_model_id() {
        let state = new_state();
        let result = start_round(&state, "".to_string(), AggregationConfig::default(), 3600);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("model_id"));
    }

    #[test]
    fn test_start_round_zero_min_participants() {
        let state = new_state();
        let mut config = AggregationConfig::default();
        config.min_participants = 0;
        let result = start_round(&state, "model".to_string(), config, 3600);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("min_participants"));
    }

    #[test]
    fn test_round_id_increments() {
        let state = new_state();
        let r1 = start_round(&state, "m1".into(), AggregationConfig::default(), 3600).unwrap();
        let r2 = start_round(&state, "m2".into(), AggregationConfig::default(), 3600).unwrap();
        assert_eq!(r1.round_id, 1);
        assert_eq!(r2.round_id, 2);
    }

    #[test]
    fn test_join_round_basic() {
        let state = new_state();
        let round = start_round(&state, "model".into(), AggregationConfig::default(), 3600).unwrap();

        let p = join_round(&state, round.round_id, "provider-A".into(), 1000).unwrap();
        assert_eq!(p.provider_id, "provider-A");
        assert_eq!(p.dataset_size, 1000);
        assert_eq!(p.status, ParticipantStatus::Registered);

        let round = get_round(&state, round.round_id).unwrap();
        assert_eq!(round.participants.len(), 1);
        assert_eq!(round.total_samples, 1000);
    }

    #[test]
    fn test_join_round_duplicate_provider() {
        let state = new_state();
        let round = start_round(&state, "model".into(), AggregationConfig::default(), 3600).unwrap();
        join_round(&state, round.round_id, "provider-A".into(), 1000).unwrap();

        let result = join_round(&state, round.round_id, "provider-A".into(), 2000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already joined"));
    }

    #[test]
    fn test_join_round_empty_provider_id() {
        let state = new_state();
        let round = start_round(&state, "model".into(), AggregationConfig::default(), 3600).unwrap();

        let result = join_round(&state, round.round_id, "".into(), 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_join_round_zero_dataset_size() {
        let state = new_state();
        let round = start_round(&state, "model".into(), AggregationConfig::default(), 3600).unwrap();

        let result = join_round(&state, round.round_id, "provider-A".into(), 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("dataset_size"));
    }

    #[test]
    fn test_join_nonexistent_round() {
        let state = new_state();
        let result = join_round(&state, 999, "provider-A".into(), 1000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_submit_delta_and_auto_aggregate() {
        let state = new_state();
        let mut config = AggregationConfig::default();
        config.min_participants = 2;

        let round = start_round(&state, "model".into(), config, 3600).unwrap();
        join_round(&state, round.round_id, "p1".into(), 500).unwrap();
        join_round(&state, round.round_id, "p2".into(), 500).unwrap();

        // Transition round to training
        {
            let mut r = state.rounds.get_mut(&round.round_id).unwrap();
            r.status = RoundStatus::Training;
        }

        // Submit delta from p1
        let d1 = WeightDelta {
            delta_base64: "AAAA".into(),
            provider_id: "p1".into(),
            dataset_size: 500,
            timestamp: Utc::now(),
            checksum: None,
            local_epochs: 1,
            local_loss: Some(0.5),
        };
        submit_delta(&state, round.round_id, d1).unwrap();

        // p1 only submitted, should not auto-aggregate yet
        let r = get_round(&state, round.round_id).unwrap();
        assert_eq!(r.status, RoundStatus::Training);

        // Submit delta from p2 -> should auto-aggregate
        let d2 = WeightDelta {
            delta_base64: "BBBB".into(),
            provider_id: "p2".into(),
            dataset_size: 500,
            timestamp: Utc::now(),
            checksum: None,
            local_epochs: 1,
            local_loss: Some(0.6),
        };
        submit_delta(&state, round.round_id, d2).unwrap();

        let r = get_round(&state, round.round_id).unwrap();
        assert_eq!(r.status, RoundStatus::Complete);
        assert!(r.aggregated_delta.is_some());
    }

    #[test]
    fn test_cancel_round() {
        let state = new_state();
        let round = start_round(&state, "model".into(), AggregationConfig::default(), 3600).unwrap();
        join_round(&state, round.round_id, "p1".into(), 1000).unwrap();

        cancel_round(&state, round.round_id).unwrap();
        let r = get_round(&state, round.round_id).unwrap();
        assert_eq!(r.status, RoundStatus::Cancelled);
        assert!(r.completed_at.is_some());
    }

    #[test]
    fn test_cancel_already_complete_round() {
        let state = new_state();
        let round = start_round(&state, "model".into(), AggregationConfig::default(), 3600).unwrap();
        {
            let mut r = state.rounds.get_mut(&round.round_id).unwrap();
            r.status = RoundStatus::Complete;
        }
        let result = cancel_round(&state, round.round_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_rounds() {
        let state = new_state();
        start_round(&state, "m1".into(), AggregationConfig::default(), 3600).unwrap();
        start_round(&state, "m2".into(), AggregationConfig::default(), 3600).unwrap();

        let rounds = list_rounds(&state);
        assert_eq!(rounds.len(), 2);
    }

    #[test]
    fn test_get_nonexistent_round() {
        let state = new_state();
        assert!(get_round(&state, 999).is_none());
    }

    // -- Aggregation math tests --

    #[test]
    fn test_federated_average_basic() {
        let deltas = vec![
            WeightDelta {
                delta_base64: "AAAA".into(),
                provider_id: "p1".into(),
                dataset_size: 100,
                timestamp: Utc::now(),
                checksum: None,
                local_epochs: 1,
                local_loss: Some(0.5),
            },
            WeightDelta {
                delta_base64: "BBBB".into(),
                provider_id: "p2".into(),
                dataset_size: 300,
                timestamp: Utc::now(),
                checksum: None,
                local_epochs: 1,
                local_loss: Some(0.7),
            },
        ];

        let result = federated_average(&deltas).unwrap();
        assert_eq!(result.num_providers, 2);
        assert_eq!(result.total_samples, 400);
        assert!((result.weights[0] - 0.25).abs() < 1e-9);
        assert!((result.weights[1] - 0.75).abs() < 1e-9);
        assert!((result.avg_loss.unwrap() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_federated_average_equal_weights() {
        let deltas = vec![
            WeightDelta {
                delta_base64: "AA".into(),
                provider_id: "p1".into(),
                dataset_size: 500,
                timestamp: Utc::now(),
                checksum: None,
                local_epochs: 1,
                local_loss: None,
            },
            WeightDelta {
                delta_base64: "BB".into(),
                provider_id: "p2".into(),
                dataset_size: 500,
                timestamp: Utc::now(),
                checksum: None,
                local_epochs: 1,
                local_loss: None,
            },
        ];

        let result = federated_average(&deltas).unwrap();
        assert!((result.weights[0] - 0.5).abs() < 1e-9);
        assert!((result.weights[1] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_federated_average_empty() {
        let result = federated_average(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_federated_average_zero_dataset() {
        let deltas = vec![WeightDelta {
            delta_base64: "AA".into(),
            provider_id: "p1".into(),
            dataset_size: 0,
            timestamp: Utc::now(),
            checksum: None,
            local_epochs: 1,
            local_loss: None,
        }];
        let result = federated_average(&deltas);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("zero"));
    }

    #[test]
    fn test_fedprox_aggregate() {
        let deltas = vec![
            WeightDelta {
                delta_base64: "AA".into(),
                provider_id: "p1".into(),
                dataset_size: 500,
                timestamp: Utc::now(),
                checksum: None,
                local_epochs: 1,
                local_loss: None,
            },
            WeightDelta {
                delta_base64: "BB".into(),
                provider_id: "p2".into(),
                dataset_size: 500,
                timestamp: Utc::now(),
                checksum: None,
                local_epochs: 1,
                local_loss: None,
            },
        ];

        let result = fedprox_aggregate(&deltas, 0.01).unwrap();
        assert!(result.delta_base64.starts_with("fedprox_mu=0.01|"));
    }

    #[test]
    fn test_fedprox_negative_mu() {
        let deltas = vec![WeightDelta {
            delta_base64: "AA".into(),
            provider_id: "p1".into(),
            dataset_size: 500,
            timestamp: Utc::now(),
            checksum: None,
            local_epochs: 1,
            local_loss: None,
        }];
        let result = fedprox_aggregate(&deltas, -1.0);
        assert!(result.is_err());
    }

    // -- Distillation config validation tests --

    #[test]
    fn test_distillation_config_valid() {
        let config = DistillationConfig {
            teacher_model: "llama-70b".into(),
            student_model: "llama-8b".into(),
            temperature: 4.0,
            alpha: 0.7,
            epochs: 3,
            batch_size: 32,
            learning_rate: 0.01,
            teacher_provider: None,
            student_provider: None,
            dataset_path: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_distillation_config_empty_teacher() {
        let config = DistillationConfig {
            teacher_model: "".into(),
            student_model: "llama-8b".into(),
            ..DistillationConfig {
                teacher_model: String::new(),
                student_model: "llama-8b".into(),
                temperature: default_temperature(),
                alpha: default_alpha(),
                epochs: default_epochs(),
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_distillation_config_same_models() {
        let config = DistillationConfig {
            teacher_model: "llama-8b".into(),
            student_model: "llama-8b".into(),
            ..DistillationConfig {
                teacher_model: "llama-8b".into(),
                student_model: "llama-8b".into(),
                temperature: default_temperature(),
                alpha: default_alpha(),
                epochs: default_epochs(),
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("different"));
    }

    #[test]
    fn test_distillation_config_bad_temperature() {
        let config = DistillationConfig {
            teacher_model: "t".into(),
            student_model: "s".into(),
            temperature: 0.0,
            ..DistillationConfig {
                teacher_model: "t".into(),
                student_model: "s".into(),
                temperature: 0.0,
                alpha: default_alpha(),
                epochs: default_epochs(),
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_distillation_config_bad_alpha() {
        let config = DistillationConfig {
            teacher_model: "t".into(),
            student_model: "s".into(),
            alpha: 1.5,
            ..DistillationConfig {
                teacher_model: "t".into(),
                student_model: "s".into(),
                temperature: default_temperature(),
                alpha: 1.5,
                epochs: default_epochs(),
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_distillation_config_zero_epochs() {
        let config = DistillationConfig {
            teacher_model: "t".into(),
            student_model: "s".into(),
            epochs: 0,
            ..DistillationConfig {
                teacher_model: "t".into(),
                student_model: "s".into(),
                temperature: default_temperature(),
                alpha: default_alpha(),
                epochs: 0,
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };
        assert!(config.validate().is_err());
    }

    // -- Distillation loss math tests --

    #[test]
    fn test_temperature_softmax() {
        let logits = vec![1.0, 2.0, 3.0];
        let probs = temperature_softmax(&logits, 1.0);
        assert!((probs[0] < probs[1]) && (probs[1] < probs[2]));
        let sum: f64 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_temperature_softmax_high_temp() {
        let logits = vec![1.0, 2.0, 3.0];
        let probs_low = temperature_softmax(&logits, 0.5);
        let probs_high = temperature_softmax(&logits, 10.0);

        // Higher temperature -> more uniform distribution
        let entropy_low: f64 = -probs_low.iter().map(|p| if *p > 0.0 { p * p.ln() } else { 0.0 }).sum::<f64>();
        let entropy_high: f64 = -probs_high.iter().map(|p| if *p > 0.0 { p * p.ln() } else { 0.0 }).sum::<f64>();
        assert!(entropy_high > entropy_low);
    }

    #[test]
    fn test_kl_divergence_same_distribution() {
        let p = vec![0.25, 0.25, 0.25, 0.25];
        let kl = kl_divergence(&p, &p).unwrap();
        assert!(kl.abs() < 1e-9);
    }

    #[test]
    fn test_kl_divergence_different_distributions() {
        let p = vec![1.0, 0.0, 0.0];
        let q = vec![0.0, 1.0, 0.0];
        let kl = kl_divergence(&p, &q).unwrap();
        assert!(kl > 0.0);
    }

    #[test]
    fn test_kl_divergence_mismatched_lengths() {
        let p = vec![0.5, 0.5];
        let q = vec![0.33, 0.33, 0.34];
        assert!(kl_divergence(&p, &q).is_err());
    }

    #[test]
    fn test_distillation_loss_pure_kl() {
        let teacher = vec![2.0, 1.0, 0.1];
        let student = vec![1.8, 1.2, 0.1];

        let result = distillation_loss(&teacher, &student, 4.0, 0.7, None).unwrap();
        assert!(result.kl_loss > 0.0);
        assert!(result.ce_loss.is_none());
        assert!((result.total_loss - result.kl_loss).abs() < 1e-9);
    }

    #[test]
    fn test_distillation_loss_with_hard_labels() {
        let teacher = vec![2.0, 1.0, 0.1];
        let student = vec![1.8, 1.2, 0.1];
        let labels = vec![1.0, 0.0, 0.0]; // one-hot

        let result = distillation_loss(&teacher, &student, 4.0, 0.7, Some(&labels)).unwrap();
        assert!(result.kl_loss > 0.0);
        assert!(result.ce_loss.is_some());
        assert!(result.ce_loss.unwrap() > 0.0);
    }

    // -- Cross-provider config validation tests --

    #[test]
    fn test_cross_provider_config_valid() {
        let config = CrossProviderConfig {
            strategy: CrossProviderStrategy::DataParallel,
            num_workers: 4,
            batch_size_per_worker: 32,
            sync_interval: 50,
            model_id: "llama-8b".into(),
            epochs: 3,
            learning_rate: 0.01,
            providers: vec!["p1".into(), "p2".into(), "p3".into(), "p4".into()],
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cross_provider_config_empty_model() {
        let config = CrossProviderConfig {
            strategy: CrossProviderStrategy::DataParallel,
            num_workers: 2,
            batch_size_per_worker: 32,
            sync_interval: 50,
            model_id: "".into(),
            epochs: 3,
            learning_rate: 0.01,
            providers: vec![],
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cross_provider_config_zero_workers() {
        let config = CrossProviderConfig {
            strategy: CrossProviderStrategy::DataParallel,
            num_workers: 0,
            batch_size_per_worker: 32,
            sync_interval: 50,
            model_id: "m".into(),
            epochs: 3,
            learning_rate: 0.01,
            providers: vec![],
        };
        assert!(config.validate().is_err());
    }

    // -- Distillation job lifecycle tests --

    #[test]
    fn test_distillation_job_lifecycle() {
        let state = new_state();
        let config = DistillationConfig {
            teacher_model: "teacher-70b".into(),
            student_model: "student-8b".into(),
            ..DistillationConfig {
                teacher_model: "teacher-70b".into(),
                student_model: "student-8b".into(),
                temperature: default_temperature(),
                alpha: default_alpha(),
                epochs: default_epochs(),
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };

        let job = start_distillation(&state, config).unwrap();
        assert_eq!(job.job_id, 1);
        assert_eq!(job.status, DistillationStatus::Pending);

        // Cancel the job
        cancel_distillation(&state, job.job_id).unwrap();
        let job = get_distillation_job(&state, job.job_id).unwrap();
        assert_eq!(job.status, DistillationStatus::Cancelled);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_distillation_list_jobs() {
        let state = new_state();
        let config = DistillationConfig {
            teacher_model: "t1".into(),
            student_model: "s1".into(),
            ..DistillationConfig {
                teacher_model: "t1".into(),
                student_model: "s1".into(),
                temperature: default_temperature(),
                alpha: default_alpha(),
                epochs: default_epochs(),
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };
        start_distillation(&state, config).unwrap();
        let config2 = DistillationConfig {
            teacher_model: "t2".into(),
            student_model: "s2".into(),
            ..DistillationConfig {
                teacher_model: "t2".into(),
                student_model: "s2".into(),
                temperature: default_temperature(),
                alpha: default_alpha(),
                epochs: default_epochs(),
                batch_size: default_batch_size(),
                learning_rate: default_learning_rate(),
                teacher_provider: None,
                student_provider: None,
                dataset_path: None,
            }
        };
        start_distillation(&state, config2).unwrap();

        let jobs = list_distillation_jobs(&state);
        assert_eq!(jobs.len(), 2);
    }

    // -- Cross-provider job lifecycle tests --

    #[test]
    fn test_cross_provider_job_lifecycle() {
        let state = new_state();
        let config = CrossProviderConfig {
            strategy: CrossProviderStrategy::ModelParallel,
            num_workers: 2,
            batch_size_per_worker: 16,
            sync_interval: 10,
            model_id: "llama-70b".into(),
            epochs: 5,
            learning_rate: 0.001,
            providers: vec!["provider-A".into(), "provider-B".into()],
        };

        let job = start_cross_provider(&state, config).unwrap();
        assert_eq!(job.job_id, 1);
        assert_eq!(job.status, CrossProviderStatus::Pending);
        assert_eq!(job.worker_progress.len(), 2);

        cancel_cross_provider(&state, job.job_id).unwrap();
        let job = get_cross_provider_job(&state, job.job_id).unwrap();
        assert_eq!(job.status, CrossProviderStatus::Cancelled);
    }

    // -- Concurrency tests --

    #[test]
    fn test_concurrent_round_creation() {
        use std::thread;

        let state = Arc::new(new_state());
        let mut handles = vec![];

        for _ in 0..10 {
            let s = state.clone();
            handles.push(thread::spawn(move || {
                start_round(&s, "model".into(), AggregationConfig::default(), 3600).unwrap()
            }));
        }

        let rounds: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let ids: Vec<u64> = rounds.iter().map(|r| r.round_id).collect();

        // All IDs should be unique
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        sorted_ids.dedup();
        assert_eq!(ids.len(), sorted_ids.len());
        assert_eq!(ids.len(), 10);
    }

    #[test]
    fn test_concurrent_joins() {
        use std::thread;

        let state = Arc::new(new_state());
        let mut config = AggregationConfig::default();
        config.min_participants = 1;

        let round = start_round(&state, "model".into(), config, 3600).unwrap();
        let round_id = round.round_id;

        let mut handles = vec![];
        for i in 0..5 {
            let s = state.clone();
            handles.push(thread::spawn(move || {
                join_round(&s, round_id, format!("provider-{}", i), 100).unwrap()
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let round = get_round(&state, round_id).unwrap();
        assert_eq!(round.participants.len(), 5);
    }

    #[test]
    fn test_concurrent_delta_submission() {
        use std::thread;

        let state = Arc::new(new_state());
        let mut config = AggregationConfig::default();
        config.min_participants = 5;

        let round = start_round(&state, "model".into(), config, 3600).unwrap();
        let round_id = round.round_id;

        for i in 0..5 {
            join_round(&state, round_id, format!("p{}", i), 100).unwrap();
        }

        {
            let mut r = state.rounds.get_mut(&round_id).unwrap();
            r.status = RoundStatus::Training;
        }

        let mut handles = vec![];
        for i in 0..5 {
            let s = state.clone();
            handles.push(thread::spawn(move || {
                let delta = WeightDelta {
                    delta_base64: format!("delta-{}", i),
                    provider_id: format!("p{}", i),
                    dataset_size: 100,
                    timestamp: Utc::now(),
                    checksum: None,
                    local_epochs: 1,
                    local_loss: Some(0.5),
                };
                submit_delta(&s, round_id, delta).unwrap()
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let round = get_round(&state, round_id).unwrap();
        assert_eq!(round.status, RoundStatus::Complete);
        assert!(round.aggregated_delta.is_some());
    }

    // -- Aggregation config serialization tests --

    #[test]
    fn test_aggregation_config_serialization() {
        let config = AggregationConfig {
            min_participants: 5,
            max_rounds: 50,
            learning_rate: 0.001,
            aggregation_strategy: AggregationStrategy::FedProx,
            fedprox_mu: 0.1,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AggregationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.min_participants, 5);
        assert_eq!(deserialized.aggregation_strategy, AggregationStrategy::FedProx);
        assert!((deserialized.fedprox_mu - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_aggregation_config_defaults() {
        let config = AggregationConfig::default();
        assert_eq!(config.min_participants, 3);
        assert_eq!(config.max_rounds, 100);
        assert!((config.learning_rate - 0.01).abs() < 1e-9);
        assert_eq!(config.aggregation_strategy, AggregationStrategy::FedAvg);
    }

    // -- Status display tests --

    #[test]
    fn test_round_status_display() {
        assert_eq!(RoundStatus::Collecting.to_string(), "collecting");
        assert_eq!(RoundStatus::Training.to_string(), "training");
        assert_eq!(RoundStatus::Aggregating.to_string(), "aggregating");
        assert_eq!(RoundStatus::Complete.to_string(), "complete");
        assert_eq!(RoundStatus::Failed.to_string(), "failed");
        assert_eq!(RoundStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_strategy_display() {
        assert_eq!(CrossProviderStrategy::DataParallel.to_string(), "data_parallel");
        assert_eq!(CrossProviderStrategy::ModelParallel.to_string(), "model_parallel");
    }

    // -- Weight delta serialization tests --

    #[test]
    fn test_weight_delta_serialization() {
        let delta = WeightDelta {
            delta_base64: "SGVsbG8gV29ybGQ=".into(),
            provider_id: "provider-1".into(),
            dataset_size: 10000,
            timestamp: Utc::now(),
            checksum: Some("sha256:abc123".into()),
            local_epochs: 3,
            local_loss: Some(0.342),
        };

        let json = serde_json::to_string(&delta).unwrap();
        let deserialized: WeightDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.provider_id, "provider-1");
        assert_eq!(deserialized.dataset_size, 10000);
        assert_eq!(deserialized.local_epochs, 3);
        assert!((deserialized.local_loss.unwrap() - 0.342).abs() < 1e-9);
    }
}
