//! Federated Training Module
//!
//! Provides federated learning infrastructure including FedAvg and FedProx aggregation,
//! distributed training coordination, gradient management with differential privacy,
//! and a full REST API for managing federated training workflows.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Core Configuration & Types
// ---------------------------------------------------------------------------

/// Global configuration for federated training sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedConfig {
    /// Unique identifier for this configuration.
    pub id: String,
    /// Name of the federated training session.
    pub name: String,
    /// Aggregation strategy: "fedavg", "fedprox", "secure", or "dp".
    pub aggregation_strategy: AggregationStrategy,
    /// Total number of rounds to execute.
    pub total_rounds: u32,
    /// Minimum number of participants required per round.
    pub min_participants: u32,
    /// Target number of participants per round.
    pub target_participants: u32,
    /// Fraction of participants to sample each round (0.0–1.0).
    pub participation_fraction: f64,
    /// FedProx proximal term coefficient mu (only used when strategy is FedProx).
    #[serde(default)]
    pub proximal_mu: f64,
    /// Differential privacy: target epsilon.
    #[serde(default)]
    pub dp_epsilon: f64,
    /// Differential privacy: noise multiplier (sigma).
    #[serde(default = "default_noise_multiplier")]
    pub dp_noise_multiplier: f64,
    /// Differential privacy: max gradient norm for clipping.
    #[serde(default = "default_max_grad_norm")]
    pub dp_max_grad_norm: f64,
    /// Learning rate used by participants.
    pub learning_rate: f64,
    /// Whether to compress gradients before transmission.
    #[serde(default)]
    pub compress_gradients: bool,
    /// Compression ratio when compression is enabled (0.0–1.0).
    #[serde(default = "default_compression_ratio")]
    pub compression_ratio: f64,
    /// Timeout for each round in seconds.
    #[serde(default = "default_round_timeout")]
    pub round_timeout_secs: u64,
    /// Maximum number of gradient update retries per participant per round.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Optional model architecture identifier.
    #[serde(default)]
    pub model_architecture: Option<String>,
    /// Creation timestamp (unix epoch millis).
    pub created_at: u64,
}

fn default_noise_multiplier() -> f64 {
    1.1
}
fn default_max_grad_norm() -> f64 {
    1.0
}
fn default_compression_ratio() -> f64 {
    0.5
}
fn default_round_timeout() -> u64 {
    300
}
fn default_max_retries() -> u32 {
    3
}

/// Supported aggregation strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    FedAvg,
    FedProx,
    Secure,
    Dp,
}

impl std::fmt::Display for AggregationStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AggregationStrategy::FedAvg => write!(f, "fedavg"),
            AggregationStrategy::FedProx => write!(f, "fedprox"),
            AggregationStrategy::Secure => write!(f, "secure"),
            AggregationStrategy::Dp => write!(f, "dp"),
        }
    }
}

impl FederatedConfig {
    /// Create a new federated configuration with sensible defaults.
    pub fn new(name: String, aggregation_strategy: AggregationStrategy, total_rounds: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            aggregation_strategy,
            total_rounds,
            min_participants: 2,
            target_participants: 10,
            participation_fraction: 1.0,
            proximal_mu: 0.01,
            dp_epsilon: 10.0,
            dp_noise_multiplier: default_noise_multiplier(),
            dp_max_grad_norm: default_max_grad_norm(),
            learning_rate: 0.01,
            compress_gradients: false,
            compression_ratio: default_compression_ratio(),
            round_timeout_secs: default_round_timeout(),
            max_retries: default_max_retries(),
            model_architecture: None,
            created_at: now_millis(),
        }
    }
}

// ---------------------------------------------------------------------------
// Participant
// ---------------------------------------------------------------------------

/// Status of a participant node in the federated network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStatus {
    Active,
    Idle,
    Disconnected,
    Banned,
}

/// A node participating in federated training.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantNode {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub status: ParticipantStatus,
    /// Number of training samples held by this participant.
    pub num_samples: u64,
    /// Computational capacity score (higher = more powerful).
    pub capacity_score: f64,
    pub registered_at: u64,
    pub last_heartbeat: u64,
    pub total_rounds_completed: u32,
    pub total_gradients_submitted: u32,
}

impl ParticipantNode {
    pub fn new(name: String, endpoint: String, num_samples: u64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            endpoint,
            status: ParticipantStatus::Active,
            num_samples,
            capacity_score: 1.0,
            registered_at: now_millis(),
            last_heartbeat: now_millis(),
            total_rounds_completed: 0,
            total_gradients_submitted: 0,
        }
    }

    pub fn heartbeat(&mut self) {
        self.last_heartbeat = now_millis();
    }
}

// ---------------------------------------------------------------------------
// Federated Round & Round Participant
// ---------------------------------------------------------------------------

/// Status of a single federated round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    Pending,
    InProgress,
    Aggregating,
    Completed,
    Failed,
    TimedOut,
}

/// A single participant's contribution within a round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundParticipant {
    pub participant_id: String,
    /// Number of samples used for local training.
    pub num_samples: u64,
    /// Number of local epochs run.
    pub local_epochs: u32,
    /// Gradient update vector (flattened).
    pub gradient: Vec<f64>,
    /// Compressed gradient (optional, when compression is used).
    pub compressed_gradient: Option<Vec<u8>>,
    /// Proximal term value (FedProx).
    pub proximal_term: f64,
    /// Whether the gradient has been verified.
    pub verified: bool,
    /// Timestamp when the gradient was submitted.
    pub submitted_at: u64,
    /// Number of retries for this submission.
    pub retries: u32,
}

/// A single round of federated training.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedRound {
    pub id: String,
    pub training_id: String,
    pub round_number: u32,
    pub status: RoundStatus,
    pub participants: Vec<RoundParticipant>,
    /// Aggregated gradient after this round.
    pub aggregated_gradient: Option<Vec<f64>>,
    /// Global model weights after aggregation.
    pub global_model: Option<Vec<f64>>,
    /// Loss value after aggregation.
    pub evaluation_loss: Option<f64>,
    /// Accuracy after aggregation.
    pub evaluation_accuracy: Option<f64>,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    /// Total samples across all participants this round.
    pub total_samples: u64,
}

impl FederatedRound {
    pub fn new(training_id: String, round_number: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            training_id,
            round_number,
            status: RoundStatus::Pending,
            participants: Vec::new(),
            aggregated_gradient: None,
            global_model: None,
            evaluation_loss: None,
            evaluation_accuracy: None,
            started_at: None,
            completed_at: None,
            total_samples: 0,
        }
    }

    /// Check if the round has received enough participants.
    pub fn has_min_participants(&self, min: u32) -> bool {
        (self.participants.len() as u32) >= min
    }

    /// Sum total samples from all round participants.
    pub fn compute_total_samples(&mut self) {
        self.total_samples = self.participants.iter().map(|p| p.num_samples).sum();
    }
}

// ---------------------------------------------------------------------------
// Federated Training Session
// ---------------------------------------------------------------------------

/// Status of the overall federated training session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingStatus {
    Created,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// A complete federated training session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedTraining {
    pub id: String,
    pub config: FederatedConfig,
    pub status: TrainingStatus,
    pub participant_ids: Vec<String>,
    pub rounds: Vec<FederatedRound>,
    pub current_round: u32,
    /// Current global model weights.
    pub global_model: Vec<f64>,
    /// Best loss observed across all rounds.
    pub best_loss: Option<f64>,
    pub best_round: Option<u32>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl FederatedTraining {
    /// Create a new federated training session from a configuration.
    pub fn create(config: FederatedConfig) -> Self {
        let id = config.id.clone();
        Self {
            id,
            config,
            status: TrainingStatus::Created,
            participant_ids: Vec::new(),
            rounds: Vec::new(),
            current_round: 0,
            global_model: Vec::new(),
            best_loss: None,
            best_round: None,
            created_at: now_millis(),
            updated_at: now_millis(),
        }
    }

    /// Register a participant for this training session.
    pub fn register_participant(&mut self, participant_id: String) -> bool {
        if self.participant_ids.contains(&participant_id) {
            return false;
        }
        self.participant_ids.push(participant_id);
        self.updated_at = now_millis();
        true
    }

    /// Start the next round. Returns the round on success, None if training is done or not running.
    pub fn start_round(&mut self) -> Option<FederatedRound> {
        if self.status != TrainingStatus::Running {
            return None;
        }
        if self.current_round >= self.config.total_rounds {
            return None;
        }
        self.current_round += 1;
        let mut round = FederatedRound::new(self.id.clone(), self.current_round);
        round.status = RoundStatus::InProgress;
        round.started_at = Some(now_millis());
        self.rounds.push(round.clone());
        self.updated_at = now_millis();
        Some(round)
    }

    /// Submit a gradient from a participant for the current round.
    pub fn submit_gradient(
        &mut self,
        participant_id: String,
        gradient: Vec<f64>,
        num_samples: u64,
        local_epochs: u32,
    ) -> Result<(), String> {
        let round = self
            .rounds
            .last_mut()
            .ok_or_else(|| "No active round".to_string())?;
        if round.status != RoundStatus::InProgress {
            return Err("Current round is not in progress".to_string());
        }
        if round.participants.iter().any(|p| p.participant_id == participant_id) {
            return Err("Participant already submitted for this round".to_string());
        }
        let rp = RoundParticipant {
            participant_id,
            num_samples,
            local_epochs,
            gradient,
            compressed_gradient: None,
            proximal_term: 0.0,
            verified: false,
            submitted_at: now_millis(),
            retries: 0,
        };
        round.participants.push(rp);
        round.compute_total_samples();
        self.updated_at = now_millis();
        Ok(())
    }

    /// Aggregate gradients for the current round using the configured strategy.
    /// Returns the aggregated gradient vector.
    pub fn aggregate(&mut self) -> Result<Vec<f64>, String> {
        let round = self
            .rounds
            .last_mut()
            .ok_or_else(|| "No active round".to_string())?;

        if round.participants.is_empty() {
            return Err("No gradients to aggregate".to_string());
        }

        round.status = RoundStatus::Aggregating;

        let total_samples = round.total_samples;
        if total_samples == 0 {
            return Err("Total samples is zero".to_string());
        }

        let grad_len = round.participants[0].gradient.len();
        if grad_len == 0 {
            return Err("Gradient vector is empty".to_string());
        }

        // Validate all gradients have the same length
        for p in &round.participants {
            if p.gradient.len() != grad_len {
                return Err(format!(
                    "Gradient length mismatch: expected {}, got {}",
                    grad_len,
                    p.gradient.len()
                ));
            }
        }

        // FedAvg weighted aggregation
        let mut aggregated = vec![0.0f64; grad_len];
        for p in &round.participants {
            let weight = p.num_samples as f64 / total_samples as f64;
            for (i, &g) in p.gradient.iter().enumerate() {
                aggregated[i] += weight * g;
            }
        }

        // FedProx: add proximal term adjustment
        if self.config.aggregation_strategy == AggregationStrategy::FedProx {
            let mu = self.config.proximal_mu;
            if !self.global_model.is_empty() && self.global_model.len() == grad_len {
                for (i, &gm) in self.global_model.iter().enumerate() {
                    // proximal_term for each participant was 0 here; we approximate:
                    // the proximal adjustment penalises deviation from global model
                    let avg_proximal: f64 = round
                        .participants
                        .iter()
                        .map(|p| p.proximal_term)
                        .sum::<f64>()
                        / round.participants.len() as f64;
                    aggregated[i] -= mu * avg_proximal * (aggregated[i] - gm);
                }
            }
        }

        // Differential privacy noise injection
        if self.config.aggregation_strategy == AggregationStrategy::Dp {
            let sigma = self.config.dp_noise_multiplier;
            let max_norm = self.config.dp_max_grad_norm;
            for val in &mut aggregated {
                let noise: f64 = pseudo_random_gaussian(sigma * max_norm);
                *val += noise;
            }
        }

        round.aggregated_gradient = Some(aggregated.clone());
        round.global_model = Some(aggregated.clone());
        self.updated_at = now_millis();
        Ok(aggregated)
    }

    /// Evaluate the aggregated model with given loss and accuracy.
    pub fn evaluate(&mut self, loss: f64, accuracy: f64) -> Result<(), String> {
        let round = self
            .rounds
            .last_mut()
            .ok_or_else(|| "No active round".to_string())?;
        round.evaluation_loss = Some(loss);
        round.evaluation_accuracy = Some(accuracy);

        // Track best model
        if self.best_loss.map_or(true, |bl| loss < bl) {
            self.best_loss = Some(loss);
            self.best_round = Some(round.round_number);
            if let Some(ref model) = round.global_model {
                self.global_model = model.clone();
            }
        }
        self.updated_at = now_millis();
        Ok(())
    }

    /// Complete the current round.
    pub fn complete_round(&mut self) -> Result<(), String> {
        let round = self
            .rounds
            .last_mut()
            .ok_or_else(|| "No active round".to_string())?;
        if round.status == RoundStatus::Completed {
            return Err("Round already completed".to_string());
        }
        round.status = RoundStatus::Completed;
        round.completed_at = Some(now_millis());
        self.updated_at = now_millis();
        Ok(())
    }

    /// Complete the entire training session.
    pub fn complete(&mut self) {
        self.status = TrainingStatus::Completed;
        self.updated_at = now_millis();
    }

    /// Fail the training session.
    pub fn fail(&mut self, reason: String) {
        self.status = TrainingStatus::Failed;
        if let Some(round) = self.rounds.last_mut() {
            round.status = RoundStatus::Failed;
        }
        self.updated_at = now_millis();
        let _ = reason; // In production, store reason
    }
}

// ---------------------------------------------------------------------------
// Training Job & Distributed Coordinator
// ---------------------------------------------------------------------------

/// Status of a distributed training job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Created,
    Assigned,
    Running,
    Checkpointed,
    Resumed,
    Completed,
    Failed,
    Cancelled,
}

/// A shard of data assigned to a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataShard {
    pub shard_id: String,
    pub job_id: String,
    pub worker_id: Option<String>,
    pub start_index: u64,
    pub end_index: u64,
    pub num_samples: u64,
    pub assigned_at: Option<u64>,
    pub completed: bool,
}

/// Progress report from a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressReport {
    pub job_id: String,
    pub worker_id: String,
    pub shard_id: String,
    pub epochs_completed: u32,
    pub total_epochs: u32,
    pub current_loss: f64,
    pub current_accuracy: f64,
    pub samples_processed: u64,
    pub timestamp: u64,
}

/// A checkpoint snapshot of a training job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub job_id: String,
    pub round_number: u32,
    pub global_model: Vec<f64>,
    pub loss: f64,
    pub accuracy: f64,
    pub created_at: u64,
}

/// A distributed training job managed by the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingJob {
    pub id: String,
    pub name: String,
    pub status: JobStatus,
    pub total_shards: u32,
    pub shards: Vec<DataShard>,
    pub checkpoints: Vec<Checkpoint>,
    pub progress_reports: Vec<ProgressReport>,
    pub current_loss: f64,
    pub current_accuracy: f64,
    pub total_epochs: u32,
    pub epochs_completed: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

impl TrainingJob {
    pub fn new(name: String, total_shards: u32, total_epochs: u32) -> Self {
        let id = Uuid::new_v4().to_string();
        let mut shards = Vec::new();
        for i in 0..total_shards {
            shards.push(DataShard {
                shard_id: format!("{}-shard-{}", id, i),
                job_id: id.clone(),
                worker_id: None,
                start_index: (i as u64) * 1000,
                end_index: ((i + 1) as u64) * 1000,
                num_samples: 1000,
                assigned_at: None,
                completed: false,
            });
        }
        Self {
            id,
            name,
            status: JobStatus::Created,
            total_shards,
            shards,
            checkpoints: Vec::new(),
            progress_reports: Vec::new(),
            current_loss: f64::INFINITY,
            current_accuracy: 0.0,
            total_epochs,
            epochs_completed: 0,
            created_at: now_millis(),
            updated_at: now_millis(),
        }
    }

    pub fn assign_shard(&mut self, shard_id: &str, worker_id: &str) -> Result<(), String> {
        let shard = self
            .shards
            .iter_mut()
            .find(|s| s.shard_id == shard_id)
            .ok_or_else(|| "Shard not found".to_string())?;
        if shard.worker_id.is_some() {
            return Err("Shard already assigned".to_string());
        }
        shard.worker_id = Some(worker_id.to_string());
        shard.assigned_at = Some(now_millis());
        self.status = JobStatus::Assigned;
        self.updated_at = now_millis();
        Ok(())
    }

    pub fn report_progress(&mut self, report: ProgressReport) -> Result<(), String> {
        self.progress_reports.push(report.clone());
        self.current_loss = report.current_loss;
        self.current_accuracy = report.current_accuracy;
        self.status = JobStatus::Running;
        self.updated_at = now_millis();
        Ok(())
    }

    pub fn checkpoint(&mut self, round_number: u32, model: Vec<f64>, loss: f64, accuracy: f64) -> Checkpoint {
        let cp = Checkpoint {
            id: Uuid::new_v4().to_string(),
            job_id: self.id.clone(),
            round_number,
            global_model: model,
            loss,
            accuracy,
            created_at: now_millis(),
        };
        self.checkpoints.push(cp.clone());
        self.status = JobStatus::Checkpointed;
        self.updated_at = now_millis();
        cp
    }

    pub fn resume(&mut self) -> Result<(), String> {
        if self.checkpoints.is_empty() {
            return Err("No checkpoints to resume from".to_string());
        }
        self.status = JobStatus::Resumed;
        self.updated_at = now_millis();
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.status = JobStatus::Cancelled;
        self.updated_at = now_millis();
    }

    /// Get the latest checkpoint.
    pub fn latest_checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoints.last()
    }
}

/// Coordinates distributed training jobs across workers.
#[derive(Debug, Clone)]
pub struct DistributedCoordinator {
    jobs: Arc<RwLock<HashMap<String, TrainingJob>>>,
}

impl DistributedCoordinator {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new training job.
    pub async fn create_job(
        &self,
        name: String,
        total_shards: u32,
        total_epochs: u32,
    ) -> TrainingJob {
        let job = TrainingJob::new(name, total_shards, total_epochs);
        let job_clone = job.clone();
        self.jobs.write().await.insert(job.id.clone(), job);
        job_clone
    }

    /// Assign a shard to a worker.
    pub async fn assign_shard(
        &self,
        job_id: &str,
        shard_id: &str,
        worker_id: &str,
    ) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| "Job not found".to_string())?;
        job.assign_shard(shard_id, worker_id)
    }

    /// Report worker progress.
    pub async fn report_progress(&self, report: ProgressReport) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(&report.job_id)
            .ok_or_else(|| "Job not found".to_string())?;
        job.report_progress(report)
    }

    /// Create a checkpoint for a job.
    pub async fn checkpoint(
        &self,
        job_id: &str,
        round_number: u32,
        model: Vec<f64>,
        loss: f64,
        accuracy: f64,
    ) -> Result<Checkpoint, String> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| "Job not found".to_string())?;
        Ok(job.checkpoint(round_number, model, loss, accuracy))
    }

    /// Resume a job from its latest checkpoint.
    pub async fn resume(&self, job_id: &str) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| "Job not found".to_string())?;
        job.resume()
    }

    /// Cancel a job.
    pub async fn cancel(&self, job_id: &str) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(job_id)
            .ok_or_else(|| "Job not found".to_string())?;
        job.cancel();
        Ok(())
    }

    /// Get a job by ID.
    pub async fn get_job(&self, job_id: &str) -> Option<TrainingJob> {
        self.jobs.read().await.get(job_id).cloned()
    }

    /// List all jobs.
    pub async fn list_jobs(&self) -> Vec<TrainingJob> {
        self.jobs.read().await.values().cloned().collect()
    }
}

// ---------------------------------------------------------------------------
// Gradient Aggregator
// ---------------------------------------------------------------------------

/// Aggregation method selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMethod {
    FedAvg,
    Secure,
    Dp,
}

/// Verification result for a gradient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientVerification {
    pub is_valid: bool,
    pub norm: f64,
    pub has_nan: bool,
    pub has_inf: bool,
    pub details: String,
}

/// The gradient aggregator handles merging participant updates.
pub struct GradientAggregator {
    method: AggregationMethod,
    dp_epsilon: f64,
    dp_noise_multiplier: f64,
    dp_max_grad_norm: f64,
    compress_ratio: f64,
}

impl GradientAggregator {
    pub fn new(method: AggregationMethod) -> Self {
        Self {
            method,
            dp_epsilon: 10.0,
            dp_noise_multiplier: 1.1,
            dp_max_grad_norm: 1.0,
            compress_ratio: 0.5,
        }
    }

    pub fn with_dp(mut self, epsilon: f64, noise_multiplier: f64, max_grad_norm: f64) -> Self {
        self.dp_epsilon = epsilon;
        self.dp_noise_multiplier = noise_multiplier;
        self.dp_max_grad_norm = max_grad_norm;
        self
    }

    pub fn with_compression(mut self, ratio: f64) -> Self {
        self.compress_ratio = ratio.clamp(0.01, 1.0);
        self
    }

    /// Aggregate gradients from multiple participants.
    /// `gradients` is a list of (gradient_vector, num_samples) tuples.
    pub fn aggregate(&self, gradients: &[(Vec<f64>, u64)]) -> Result<Vec<f64>, String> {
        if gradients.is_empty() {
            return Err("No gradients to aggregate".to_string());
        }

        let grad_len = gradients[0].0.len();
        if grad_len == 0 {
            return Err("Gradient vector is empty".to_string());
        }

        // Validate all gradients
        for (g, _) in gradients {
            if g.len() != grad_len {
                return Err(format!(
                    "Gradient length mismatch: expected {}, got {}",
                    grad_len,
                    g.len()
                ));
            }
        }

        let result = match self.method {
            AggregationMethod::FedAvg => self.fedavg_aggregate(gradients, grad_len),
            AggregationMethod::Secure => self.secure_aggregate(gradients, grad_len),
            AggregationMethod::Dp => self.dp_aggregate(gradients, grad_len),
        };

        Ok(result)
    }

    /// FedAvg: weighted average by sample count.
    fn fedavg_aggregate(&self, gradients: &[(Vec<f64>, u64)], grad_len: usize) -> Vec<f64> {
        let total_samples: u64 = gradients.iter().map(|(_, s)| *s).sum();
        if total_samples == 0 {
            return vec![0.0; grad_len];
        }

        let mut aggregated = vec![0.0f64; grad_len];
        for (gradient, samples) in gradients {
            let weight = *samples as f64 / total_samples as f64;
            for (i, &g) in gradient.iter().enumerate() {
                aggregated[i] += weight * g;
            }
        }
        aggregated
    }

    /// Secure aggregation (simulated): same as FedAvg but with verification.
    fn secure_aggregate(&self, gradients: &[(Vec<f64>, u64)], grad_len: usize) -> Vec<f64> {
        // In a real implementation, this would use SMPC/HE protocols.
        // Here we simulate by verifying each gradient before aggregating.
        let verified: Vec<_> = gradients
            .iter()
            .filter(|(g, _)| {
                let v = Self::verify_gradient(g);
                v.is_valid
            })
            .collect();

        if verified.is_empty() {
            return vec![0.0; grad_len];
        }

        let total_samples: u64 = verified.iter().map(|(_, s)| *s).sum();
        if total_samples == 0 {
            return vec![0.0; grad_len];
        }

        let mut aggregated = vec![0.0f64; grad_len];
        for (gradient, samples) in verified {
            let weight = *samples as f64 / total_samples as f64;
            for (i, &g) in gradient.iter().enumerate() {
                aggregated[i] += weight * g;
            }
        }
        aggregated
    }

    /// Differential Privacy aggregation: clip + noise.
    fn dp_aggregate(&self, gradients: &[(Vec<f64>, u64)], grad_len: usize) -> Vec<f64> {
        // Clip each gradient
        let clipped: Vec<Vec<f64>> = gradients
            .iter()
            .map(|(g, _)| Self::clip_gradient(g, self.dp_max_grad_norm))
            .collect();

        let total_samples: u64 = gradients.iter().map(|(_, s)| *s).sum();
        if total_samples == 0 {
            return vec![0.0; grad_len];
        }

        // Average the clipped gradients
        let mut aggregated = vec![0.0f64; grad_len];
        for (gradient, samples) in clipped.iter().zip(gradients.iter().map(|(_, s)| *s)) {
            let weight = samples as f64 / total_samples as f64;
            for (i, &g) in gradient.iter().enumerate() {
                aggregated[i] += weight * g;
            }
        }

        // Add Gaussian noise calibrated to dp_noise_multiplier
        let noise_scale = self.dp_noise_multiplier * self.dp_max_grad_norm / (total_samples as f64).sqrt();
        for val in &mut aggregated {
            let noise = pseudo_random_gaussian(noise_scale);
            *val += noise;
        }
        aggregated
    }

    /// Apply DP noise to an existing gradient vector.
    pub fn apply_dp_noise(&self, gradient: &[f64], noise_multiplier: f64, max_norm: f64) -> Vec<f64> {
        let clipped = Self::clip_gradient(gradient, max_norm);
        let noise_scale = noise_multiplier * max_norm;
        clipped
            .into_iter()
            .map(|v| v + pseudo_random_gaussian(noise_scale))
            .collect()
    }

    /// Verify a gradient: check for NaN, Inf, and reasonable norms.
    pub fn verify_gradient(gradient: &[f64]) -> GradientVerification {
        let mut has_nan = false;
        let mut has_inf = false;
        let mut sum_sq = 0.0f64;
        for &v in gradient {
            if v.is_nan() {
                has_nan = true;
            }
            if v.is_infinite() {
                has_inf = true;
            }
            sum_sq += v * v;
        }
        let norm = sum_sq.sqrt();
        let is_valid = !has_nan && !has_inf && norm.is_finite() && norm < 1e6;
        let details = if has_nan {
            "Gradient contains NaN values".to_string()
        } else if has_inf {
            "Gradient contains infinite values".to_string()
        } else if norm >= 1e6 {
            format!("Gradient norm {} exceeds threshold", norm)
        } else {
            "Gradient is valid".to_string()
        };
        GradientVerification {
            is_valid,
            norm,
            has_nan,
            has_inf,
            details,
        }
    }

    /// Clip a gradient to a maximum L2 norm.
    pub fn clip_gradient(gradient: &[f64], max_norm: f64) -> Vec<f64> {
        let mut sum_sq = 0.0f64;
        for &v in gradient {
            sum_sq += v * v;
        }
        let norm = sum_sq.sqrt();
        if norm <= max_norm || norm == 0.0 {
            return gradient.to_vec();
        }
        let scale = max_norm / norm;
        gradient.iter().map(|&v| v * scale).collect()
    }

    /// Compress a gradient using simple top-k sparsification.
    /// Returns a sparse representation: list of (index, value) pairs.
    pub fn compress(&self, gradient: &[f64]) -> Vec<u8> {
        let k = ((gradient.len() as f64) * self.compress_ratio) as usize;
        let k = k.max(1).min(gradient.len());

        // Find top-k indices by absolute value
        let mut indexed: Vec<(usize, f64)> = gradient.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(std::cmp::Ordering::Equal));
        indexed.truncate(k);

        // Encode as bytes: [count_u32 LE][index_u32 LE, value_f64 LE]...
        let mut bytes = Vec::with_capacity(4 + k * 12);
        bytes.extend_from_slice(&(k as u32).to_le_bytes());
        for (idx, val) in &indexed {
            bytes.extend_from_slice(&(*idx as u32).to_le_bytes());
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        bytes
    }

    /// Decompress a gradient from sparse representation.
    pub fn decompress(&self, compressed: &[u8], original_len: usize) -> Vec<f64> {
        if compressed.len() < 4 {
            return vec![0.0; original_len];
        }
        let k = u32::from_le_bytes([compressed[0], compressed[1], compressed[2], compressed[3]]) as usize;
        let mut gradient = vec![0.0; original_len];
        for i in 0..k {
            let offset = 4 + i * 12;
            if offset + 12 > compressed.len() {
                break;
            }
            let idx = u32::from_le_bytes([
                compressed[offset],
                compressed[offset + 1],
                compressed[offset + 2],
                compressed[offset + 3],
            ]) as usize;
            let val = f64::from_le_bytes([
                compressed[offset + 4],
                compressed[offset + 5],
                compressed[offset + 6],
                compressed[offset + 7],
                compressed[offset + 8],
                compressed[offset + 9],
                compressed[offset + 10],
                compressed[offset + 11],
            ]);
            if idx < original_len {
                gradient[idx] = val;
            }
        }
        gradient
    }
}

// ---------------------------------------------------------------------------
// Federated Training Engine (state container)
// ---------------------------------------------------------------------------

/// The central engine holding all federated training state.
pub struct FederatedTrainingEngine {
    pub trainings: DashMap<String, FederatedTraining>,
    pub participants: DashMap<String, ParticipantNode>,
    pub coordinator: DistributedCoordinator,
    pub aggregator: RwLock<GradientAggregator>,
    round_counter: AtomicU64,
}

impl FederatedTrainingEngine {
    /// Create a new engine with default state.
    pub fn new() -> Self {
        Self {
            trainings: DashMap::new(),
            participants: DashMap::new(),
            coordinator: DistributedCoordinator::new(),
            aggregator: RwLock::new(GradientAggregator::new(AggregationMethod::FedAvg)),
            round_counter: AtomicU64::new(0),
        }
    }

    /// Generate a unique round sequence number.
    pub fn next_round_id(&self) -> u64 {
        self.round_counter.fetch_add(1, Ordering::SeqCst)
    }

    // -- Federated Training Operations --

    /// Create a new federated training session.
    pub fn create_training(&self, config: FederatedConfig) -> FederatedTraining {
        let training = FederatedTraining::create(config);
        self.trainings
            .insert(training.id.clone(), training.clone());
        training
    }

    /// Register a participant node.
    pub fn register_participant(&self, node: ParticipantNode) -> ParticipantNode {
        self.participants
            .insert(node.id.clone(), node.clone());
        node
    }

    /// Start the next round for a training session.
    pub fn start_round(&self, training_id: &str) -> Result<FederatedRound, String> {
        let mut training = self
            .trainings
            .get_mut(training_id)
            .ok_or_else(|| "Training not found".to_string())?;
        training.start_round().ok_or_else(|| "Cannot start round".to_string())
    }

    /// Submit a gradient for the current round.
    pub fn submit_gradient(
        &self,
        training_id: &str,
        participant_id: &str,
        gradient: Vec<f64>,
        num_samples: u64,
        local_epochs: u32,
    ) -> Result<(), String> {
        let mut training = self
            .trainings
            .get_mut(training_id)
            .ok_or_else(|| "Training not found".to_string())?;
        training.submit_gradient(participant_id.to_string(), gradient, num_samples, local_epochs)
    }

    /// Aggregate gradients for the current round.
    pub fn aggregate_gradients(&self, training_id: &str) -> Result<Vec<f64>, String> {
        let mut training = self
            .trainings
            .get_mut(training_id)
            .ok_or_else(|| "Training not found".to_string())?;
        training.aggregate()
    }

    /// Evaluate the current round.
    pub fn evaluate_round(
        &self,
        training_id: &str,
        loss: f64,
        accuracy: f64,
    ) -> Result<(), String> {
        let mut training = self
            .trainings
            .get_mut(training_id)
            .ok_or_else(|| "Training not found".to_string())?;
        training.evaluate(loss, accuracy)
    }

    /// Complete the current round.
    pub fn complete_round(&self, training_id: &str) -> Result<(), String> {
        let mut training = self
            .trainings
            .get_mut(training_id)
            .ok_or_else(|| "Training not found".to_string())?;
        training.complete_round()
    }

    /// Complete the training session.
    pub fn complete_training(&self, training_id: &str) -> Result<(), String> {
        let mut training = self
            .trainings
            .get_mut(training_id)
            .ok_or_else(|| "Training not found".to_string())?;
        training.complete();
        Ok(())
    }

    /// Get a training session.
    pub fn get_training(&self, training_id: &str) -> Option<FederatedTraining> {
        self.trainings.get(training_id).map(|t| t.clone())
    }

    /// List all training sessions.
    pub fn list_trainings(&self) -> Vec<FederatedTraining> {
        self.trainings.iter().map(|t| t.value().clone()).collect()
    }

    /// Get a participant.
    pub fn get_participant(&self, participant_id: &str) -> Option<ParticipantNode> {
        self.participants.get(participant_id).map(|p| p.clone())
    }

    /// List all participants.
    pub fn list_participants(&self) -> Vec<ParticipantNode> {
        self.participants.iter().map(|p| p.value().clone()).collect()
    }

    /// Update participant heartbeat.
    pub fn heartbeat(&self, participant_id: &str) -> Result<(), String> {
        let mut node = self
            .participants
            .get_mut(participant_id)
            .ok_or_else(|| "Participant not found".to_string())?;
        node.heartbeat();
        Ok(())
    }

    /// Get a training's rounds.
    pub fn get_training_rounds(&self, training_id: &str) -> Option<Vec<FederatedRound>> {
        self.trainings
            .get(training_id)
            .map(|t| t.value().rounds.clone())
    }

    /// Get a specific round.
    pub fn get_round(&self, training_id: &str, round_number: u32) -> Option<FederatedRound> {
        self.trainings.get(training_id).and_then(|t| {
            t.value()
                .rounds
                .iter()
                .find(|r| r.round_number == round_number)
                .cloned()
        })
    }

    /// Get training statistics.
    pub fn get_training_stats(&self, training_id: &str) -> Option<TrainingStats> {
        let training = self.trainings.get(training_id)?;
        let t = training.value();
        Some(TrainingStats {
            training_id: t.id.clone(),
            status: t.status,
            current_round: t.current_round,
            total_rounds: t.config.total_rounds,
            num_participants: t.participant_ids.len() as u32,
            best_loss: t.best_loss,
            best_round: t.best_round,
            created_at: t.created_at,
            updated_at: t.updated_at,
        })
    }

    /// Get system-wide statistics.
    pub fn get_system_stats(&self) -> SystemStats {
        SystemStats {
            total_trainings: self.trainings.len() as u32,
            active_trainings: self
                .trainings
                .iter()
                .filter(|t| t.value().status == TrainingStatus::Running)
                .count() as u32,
            total_participants: self.participants.len() as u32,
            active_participants: self
                .participants
                .iter()
                .filter(|p| p.value().status == ParticipantStatus::Active)
                .count() as u32,
        }
    }
}

impl Default for FederatedTrainingEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for a single training session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingStats {
    pub training_id: String,
    pub status: TrainingStatus,
    pub current_round: u32,
    pub total_rounds: u32,
    pub num_participants: u32,
    pub best_loss: Option<f64>,
    pub best_round: Option<u32>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// System-wide statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub total_trainings: u32,
    pub active_trainings: u32,
    pub total_participants: u32,
    pub active_participants: u32,
}

// ---------------------------------------------------------------------------
// REST API Request/Response Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTrainingRequest {
    pub name: String,
    pub aggregation_strategy: String,
    pub total_rounds: u32,
    pub min_participants: Option<u32>,
    pub learning_rate: Option<f64>,
    pub proximal_mu: Option<f64>,
    pub dp_epsilon: Option<f64>,
    pub dp_noise_multiplier: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterParticipantRequest {
    pub name: String,
    pub endpoint: String,
    pub num_samples: u64,
    pub capacity_score: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitGradientRequest {
    pub training_id: String,
    pub participant_id: String,
    pub gradient: Vec<f64>,
    pub num_samples: u64,
    pub local_epochs: u32,
}

#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    pub training_id: String,
    pub loss: f64,
    pub accuracy: f64,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub name: String,
    pub total_shards: u32,
    pub total_epochs: u32,
}

#[derive(Debug, Deserialize)]
pub struct AssignShardRequest {
    pub job_id: String,
    pub shard_id: String,
    pub worker_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ReportProgressRequest {
    pub job_id: String,
    pub worker_id: String,
    pub shard_id: String,
    pub epochs_completed: u32,
    pub total_epochs: u32,
    pub current_loss: f64,
    pub current_accuracy: f64,
    pub samples_processed: u64,
}

#[derive(Debug, Deserialize)]
pub struct CheckpointRequest {
    pub job_id: String,
    pub round_number: u32,
    pub model: Vec<f64>,
    pub loss: f64,
    pub accuracy: f64,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg),
        }
    }
}

// ---------------------------------------------------------------------------
// REST Router
// ---------------------------------------------------------------------------

/// Build the federated training router with all REST endpoints.
pub fn build_federated_training_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{get, post};

    let engine = state
        .federated_training
        .clone()
        .unwrap_or_else(|| Arc::new(FederatedTrainingEngine::new()));

    axum::Router::new()
        // -- Training lifecycle (6 endpoints) --
        .route("/federated/trainings", post(create_training_handler))
        .route("/federated/trainings", get(list_trainings_handler))
        .route("/federated/trainings/{id}", get(get_training_handler))
        .route("/federated/trainings/{id}/start", post(start_round_handler))
        .route("/federated/trainings/{id}/complete", post(complete_training_handler))
        .route("/federated/trainings/{id}/stats", get(get_training_stats_handler))
        // -- Gradient operations (4 endpoints) --
        .route("/federated/gradients/submit", post(submit_gradient_handler))
        .route("/federated/gradients/aggregate", post(aggregate_handler))
        .route("/federated/gradients/evaluate", post(evaluate_handler))
        .route("/federated/gradients/complete-round", post(complete_round_handler))
        // -- Participant management (3 endpoints) --
        .route("/federated/participants", post(register_participant_handler))
        .route("/federated/participants", get(list_participants_handler))
        .route("/federated/participants/{id}/heartbeat", post(heartbeat_handler))
        // -- Distributed coordinator (4 endpoints) --
        .route("/federated/jobs", post(create_job_handler))
        .route("/federated/jobs/{id}/assign", post(assign_shard_handler))
        .route("/federated/jobs/{id}/progress", post(report_progress_handler))
        .route("/federated/jobs/{id}/checkpoint", post(checkpoint_handler))
        // -- Utility (1 endpoint) --
        .route("/federated/system/stats", get(system_stats_handler))
        .with_state(engine)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

async fn create_training_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<CreateTrainingRequest>,
) -> (StatusCode, Json<ApiResponse<FederatedTraining>>) {
    let strategy = match req.aggregation_strategy.to_lowercase().as_str() {
        "fedavg" => AggregationStrategy::FedAvg,
        "fedprox" => AggregationStrategy::FedProx,
        "secure" => AggregationStrategy::Secure,
        "dp" => AggregationStrategy::Dp,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<FederatedTraining>::err(
                    "Invalid aggregation strategy".to_string(),
                )),
            );
        }
    };

    let mut config = FederatedConfig::new(req.name, strategy, req.total_rounds);
    if let Some(mp) = req.min_participants {
        config.min_participants = mp;
    }
    if let Some(lr) = req.learning_rate {
        config.learning_rate = lr;
    }
    if let Some(mu) = req.proximal_mu {
        config.proximal_mu = mu;
    }
    if let Some(eps) = req.dp_epsilon {
        config.dp_epsilon = eps;
    }
    if let Some(nm) = req.dp_noise_multiplier {
        config.dp_noise_multiplier = nm;
    }

    let training = engine.create_training(config);
    (StatusCode::OK, Json(ApiResponse::ok(training)))
}

async fn list_trainings_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
) -> (StatusCode, Json<ApiResponse<Vec<FederatedTraining>>>) {
    let trainings = engine.list_trainings();
    (StatusCode::OK, Json(ApiResponse::ok(trainings)))
}

async fn get_training_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<FederatedTraining>>) {
    match engine.get_training(&id) {
        Some(t) => (StatusCode::OK, Json(ApiResponse::ok(t))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<FederatedTraining>::err("Training not found".to_string())),
        ),
    }
}

async fn start_round_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<FederatedRound>>) {
    match engine.start_round(&id) {
        Ok(round) => (StatusCode::OK, Json(ApiResponse::ok(round))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<FederatedRound>::err(e)),
        ),
    }
}

async fn complete_training_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    match engine.complete_training(&id) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Training completed".to_string())),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<String>::err(e)),
        ),
    }
}

async fn get_training_stats_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<TrainingStats>>) {
    match engine.get_training_stats(&id) {
        Some(stats) => (StatusCode::OK, Json(ApiResponse::ok(stats))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<TrainingStats>::err(
                "Training not found".to_string(),
            )),
        ),
    }
}

async fn submit_gradient_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<SubmitGradientRequest>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    match engine.submit_gradient(
        &req.training_id,
        &req.participant_id,
        req.gradient,
        req.num_samples,
        req.local_epochs,
    ) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Gradient submitted".to_string())),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<String>::err(e)),
        ),
    }
}

async fn aggregate_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<serde_json::Value>,
) -> (StatusCode, Json<ApiResponse<Vec<f64>>>) {
    let training_id = req["training_id"].as_str().unwrap_or("");
    match engine.aggregate_gradients(training_id) {
        Ok(gradient) => (StatusCode::OK, Json(ApiResponse::ok(gradient))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<Vec<f64>>::err(e)),
        ),
    }
}

async fn evaluate_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<EvaluateRequest>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    match engine.evaluate_round(&req.training_id, req.loss, req.accuracy) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Evaluation recorded".to_string())),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<String>::err(e)),
        ),
    }
}

async fn complete_round_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<serde_json::Value>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    let training_id = req["training_id"].as_str().unwrap_or("");
    match engine.complete_round(training_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Round completed".to_string())),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<String>::err(e)),
        ),
    }
}

async fn register_participant_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<RegisterParticipantRequest>,
) -> (StatusCode, Json<ApiResponse<ParticipantNode>>) {
    let mut node = ParticipantNode::new(req.name, req.endpoint, req.num_samples);
    if let Some(cap) = req.capacity_score {
        node.capacity_score = cap;
    }
    let registered = engine.register_participant(node);
    (StatusCode::OK, Json(ApiResponse::ok(registered)))
}

async fn list_participants_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
) -> (StatusCode, Json<ApiResponse<Vec<ParticipantNode>>>) {
    let participants = engine.list_participants();
    (StatusCode::OK, Json(ApiResponse::ok(participants)))
}

async fn heartbeat_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    match engine.heartbeat(&id) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Heartbeat recorded".to_string())),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<String>::err(e)),
        ),
    }
}

async fn create_job_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<CreateJobRequest>,
) -> (StatusCode, Json<ApiResponse<TrainingJob>>) {
    let job = engine.coordinator.create_job(req.name, req.total_shards, req.total_epochs).await;
    (StatusCode::OK, Json(ApiResponse::ok(job)))
}

async fn assign_shard_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<AssignShardRequest>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    match engine
        .coordinator
        .assign_shard(&req.job_id, &req.shard_id, &req.worker_id)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Shard assigned".to_string())),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<String>::err(e)),
        ),
    }
}

async fn report_progress_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<ReportProgressRequest>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    let report = ProgressReport {
        job_id: req.job_id,
        worker_id: req.worker_id,
        shard_id: req.shard_id,
        epochs_completed: req.epochs_completed,
        total_epochs: req.total_epochs,
        current_loss: req.current_loss,
        current_accuracy: req.current_accuracy,
        samples_processed: req.samples_processed,
        timestamp: now_millis(),
    };
    match engine.coordinator.report_progress(report).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::ok("Progress recorded".to_string())),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<String>::err(e)),
        ),
    }
}

async fn checkpoint_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
    Json(req): Json<CheckpointRequest>,
) -> (StatusCode, Json<ApiResponse<Checkpoint>>) {
    match engine
        .coordinator
        .checkpoint(&req.job_id, req.round_number, req.model, req.loss, req.accuracy)
        .await
    {
        Ok(cp) => (StatusCode::OK, Json(ApiResponse::ok(cp))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<Checkpoint>::err(e)),
        ),
    }
}

async fn system_stats_handler(
    State(engine): State<Arc<FederatedTrainingEngine>>,
) -> (StatusCode, Json<ApiResponse<SystemStats>>) {
    let stats = engine.get_system_stats();
    (StatusCode::OK, Json(ApiResponse::ok(stats)))
}

// ---------------------------------------------------------------------------
// Utility Functions
// ---------------------------------------------------------------------------

/// Current timestamp in milliseconds since Unix epoch.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Simple pseudo-random Gaussian noise (Box-Muller transform with deterministic seed).
/// NOTE: This is a simplified implementation for testing/demonstration.
/// Production code should use a proper CSPRNG.
pub fn pseudo_random_gaussian(std_dev: f64) -> f64 {
    use std::cell::Cell;
    thread_local! {
        static Z2: Cell<Option<f64>> = Cell::new(None);
    }
    Z2.with(|z2| {
        if let Some(z) = z2.get() {
            z2.set(None);
            return z * std_dev;
        }
        // Box-Muller transform using time-based seed approximation
        let u1: f64 = 1.0 - (now_nanos() as f64 / 1e18).fract();
        let u2: f64 = 1.0 - ((now_nanos() + 1) as f64 / 1e18).fract();
        let radius = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f64::consts::PI * u2;
        let z0 = radius * theta.cos();
        let z1 = radius * theta.sin();
        z2.set(Some(z1));
        z0 * std_dev
    })
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- FederatedConfig Tests --

    #[test]
    fn test_federated_config_new() {
        let config = FederatedConfig::new(
            "test-training".to_string(),
            AggregationStrategy::FedAvg,
            10,
        );
        assert_eq!(config.name, "test-training");
        assert_eq!(config.total_rounds, 10);
        assert_eq!(config.aggregation_strategy, AggregationStrategy::FedAvg);
        assert_eq!(config.min_participants, 2);
        assert!(config.created_at > 0);
    }

    #[test]
    fn test_federated_config_fedprox_defaults() {
        let config = FederatedConfig::new(
            "fedprox-test".to_string(),
            AggregationStrategy::FedProx,
            5,
        );
        assert_eq!(config.proximal_mu, 0.01);
    }

    #[test]
    fn test_aggregation_strategy_display() {
        assert_eq!(AggregationStrategy::FedAvg.to_string(), "fedavg");
        assert_eq!(AggregationStrategy::FedProx.to_string(), "fedprox");
        assert_eq!(AggregationStrategy::Secure.to_string(), "secure");
        assert_eq!(AggregationStrategy::Dp.to_string(), "dp");
    }

    // -- ParticipantNode Tests --

    #[test]
    fn test_participant_node_new() {
        let node = ParticipantNode::new("node1".to_string(), "http://localhost:8001".to_string(), 1000);
        assert_eq!(node.name, "node1");
        assert_eq!(node.num_samples, 1000);
        assert_eq!(node.status, ParticipantStatus::Active);
        assert!(node.registered_at > 0);
    }

    #[test]
    fn test_participant_heartbeat() {
        let mut node = ParticipantNode::new("node1".to_string(), "http://localhost:8001".to_string(), 500);
        let original = node.last_heartbeat;
        node.heartbeat();
        assert!(node.last_heartbeat >= original);
    }

    // -- FederatedTraining Tests --

    #[test]
    fn test_create_training() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let training = FederatedTraining::create(config);
        assert_eq!(training.status, TrainingStatus::Created);
        assert_eq!(training.current_round, 0);
        assert!(training.rounds.is_empty());
    }

    #[test]
    fn test_register_participant() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        assert!(training.register_participant("p1".to_string()));
        assert!(training.register_participant("p2".to_string()));
        assert_eq!(training.participant_ids.len(), 2);
        // Duplicate registration should fail
        assert!(!training.register_participant("p1".to_string()));
    }

    #[test]
    fn test_start_round_not_running() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        // Cannot start round when status is Created
        assert!(training.start_round().is_none());
    }

    #[test]
    fn test_start_round_running() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        let round = training.start_round().expect("should start round");
        assert_eq!(round.round_number, 1);
        assert_eq!(round.status, RoundStatus::InProgress);
        assert_eq!(training.current_round, 1);
    }

    #[test]
    fn test_submit_gradient() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        training.start_round().unwrap();

        let gradient = vec![0.1, 0.2, 0.3];
        assert!(training
            .submit_gradient("p1".to_string(), gradient.clone(), 100, 1)
            .is_ok());

        // Duplicate submission should fail
        assert!(training
            .submit_gradient("p1".to_string(), gradient, 100, 1)
            .is_err());
    }

    #[test]
    fn test_submit_gradient_no_round() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        let result = training.submit_gradient("p1".to_string(), vec![0.1], 100, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_aggregate_fedavg() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        training.start_round().unwrap();

        // Participant 1: 100 samples, gradient [2.0, 4.0]
        training
            .submit_gradient("p1".to_string(), vec![2.0, 4.0], 100, 1)
            .unwrap();
        // Participant 2: 300 samples, gradient [4.0, 8.0]
        training
            .submit_gradient("p2".to_string(), vec![4.0, 8.0], 300, 1)
            .unwrap();

        let aggregated = training.aggregate().unwrap();
        // Weighted: (100/400)*[2,4] + (300/400)*[4,8] = [0.25*2 + 0.75*4, 0.25*4 + 0.75*8]
        // = [0.5 + 3.0, 1.0 + 6.0] = [3.5, 7.0]
        assert!((aggregated[0] - 3.5).abs() < 1e-10);
        assert!((aggregated[1] - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_aggregate_empty_gradients() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        training.start_round().unwrap();
        let result = training.aggregate();
        assert!(result.is_err());
    }

    #[test]
    fn test_aggregate_length_mismatch() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        training.start_round().unwrap();

        training
            .submit_gradient("p1".to_string(), vec![1.0, 2.0], 100, 1)
            .unwrap();
        training
            .submit_gradient("p2".to_string(), vec![3.0, 4.0, 5.0], 200, 1)
            .unwrap();

        let result = training.aggregate();
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        training.start_round().unwrap();
        training
            .submit_gradient("p1".to_string(), vec![1.0, 2.0], 100, 1)
            .unwrap();
        training.aggregate().unwrap();

        assert!(training.evaluate(0.5, 0.9).is_ok());
        let round = &training.rounds[0];
        assert!((round.evaluation_loss.unwrap() - 0.5).abs() < 1e-10);
        assert!((round.evaluation_accuracy.unwrap() - 0.9).abs() < 1e-10);
        assert_eq!(training.best_loss, Some(0.5));
        assert_eq!(training.best_round, Some(1));
    }

    #[test]
    fn test_evaluate_tracks_best() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;

        // Round 1
        training.start_round().unwrap();
        training
            .submit_gradient("p1".to_string(), vec![1.0, 2.0], 100, 1)
            .unwrap();
        training.aggregate().unwrap();
        training.evaluate(0.8, 0.7).unwrap();
        training.complete_round().unwrap();

        // Round 2 (better loss)
        training.start_round().unwrap();
        training
            .submit_gradient("p1".to_string(), vec![0.5, 1.0], 100, 1)
            .unwrap();
        training.aggregate().unwrap();
        training.evaluate(0.3, 0.95).unwrap();

        assert_eq!(training.best_loss, Some(0.3));
        assert_eq!(training.best_round, Some(2));
    }

    #[test]
    fn test_complete_round() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        training.start_round().unwrap();
        training.complete_round().unwrap();
        assert_eq!(training.rounds[0].status, RoundStatus::Completed);
    }

    #[test]
    fn test_complete_training() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.complete();
        assert_eq!(training.status, TrainingStatus::Completed);
    }

    #[test]
    fn test_fail_training() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        training.start_round().unwrap();
        training.fail("OOM".to_string());
        assert_eq!(training.status, TrainingStatus::Failed);
    }

    // -- FederatedRound Tests --

    #[test]
    fn test_round_has_min_participants() {
        let mut round = FederatedRound::new("t1".to_string(), 1);
        assert!(!round.has_min_participants(2));
        round.participants.push(RoundParticipant {
            participant_id: "p1".to_string(),
            num_samples: 100,
            local_epochs: 1,
            gradient: vec![1.0],
            compressed_gradient: None,
            proximal_term: 0.0,
            verified: false,
            submitted_at: 0,
            retries: 0,
        });
        assert!(!round.has_min_participants(2));
        round.participants.push(RoundParticipant {
            participant_id: "p2".to_string(),
            num_samples: 200,
            local_epochs: 1,
            gradient: vec![2.0],
            compressed_gradient: None,
            proximal_term: 0.0,
            verified: false,
            submitted_at: 0,
            retries: 0,
        });
        assert!(round.has_min_participants(2));
    }

    #[test]
    fn test_round_compute_total_samples() {
        let mut round = FederatedRound::new("t1".to_string(), 1);
        round.participants.push(RoundParticipant {
            participant_id: "p1".to_string(),
            num_samples: 100,
            local_epochs: 1,
            gradient: vec![1.0],
            compressed_gradient: None,
            proximal_term: 0.0,
            verified: false,
            submitted_at: 0,
            retries: 0,
        });
        round.participants.push(RoundParticipant {
            participant_id: "p2".to_string(),
            num_samples: 250,
            local_epochs: 1,
            gradient: vec![2.0],
            compressed_gradient: None,
            proximal_term: 0.0,
            verified: false,
            submitted_at: 0,
            retries: 0,
        });
        round.compute_total_samples();
        assert_eq!(round.total_samples, 350);
    }

    // -- GradientAggregator Tests --

    #[test]
    fn test_aggregator_fedavg() {
        let agg = GradientAggregator::new(AggregationMethod::FedAvg);
        let gradients = vec![
            (vec![2.0, 4.0], 100u64),
            (vec![4.0, 8.0], 300u64),
        ];
        let result = agg.aggregate(&gradients).unwrap();
        assert!((result[0] - 3.5).abs() < 1e-10);
        assert!((result[1] - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_aggregator_secure() {
        let agg = GradientAggregator::new(AggregationMethod::Secure);
        let gradients = vec![
            (vec![1.0, 2.0], 50u64),
            (vec![3.0, 4.0], 50u64),
        ];
        let result = agg.aggregate(&gradients).unwrap();
        assert!((result[0] - 2.0).abs() < 1e-10);
        assert!((result[1] - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_aggregator_dp() {
        let agg = GradientAggregator::new(AggregationMethod::Dp);
        let gradients = vec![
            (vec![1.0, 1.0], 100u64),
            (vec![3.0, 3.0], 100u64),
        ];
        let result = agg.aggregate(&gradients).unwrap();
        // DP adds noise, so we check that result is finite and roughly centered
        assert!(result[0].is_finite());
        assert!(result[1].is_finite());
        // Without noise: [2.0, 2.0]; with noise should be somewhere nearby
        assert!(result[0].abs() < 100.0);
    }

    #[test]
    fn test_aggregator_empty() {
        let agg = GradientAggregator::new(AggregationMethod::FedAvg);
        let result = agg.aggregate(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_aggregator_zero_samples() {
        let agg = GradientAggregator::new(AggregationMethod::FedAvg);
        let gradients = vec![(vec![1.0, 2.0], 0u64)];
        let result = agg.aggregate(&gradients).unwrap();
        // Division by zero handled: returns zeros
        assert_eq!(result, vec![0.0, 0.0]);
    }

    #[test]
    fn test_verify_gradient_valid() {
        let result = GradientAggregator::verify_gradient(&[1.0, 2.0, 3.0]);
        assert!(result.is_valid);
        assert!(!result.has_nan);
        assert!(!result.has_inf);
    }

    #[test]
    fn test_verify_gradient_nan() {
        let result = GradientAggregator::verify_gradient(&[1.0, f64::NAN, 3.0]);
        assert!(!result.is_valid);
        assert!(result.has_nan);
    }

    #[test]
    fn test_verify_gradient_inf() {
        let result = GradientAggregator::verify_gradient(&[1.0, f64::INFINITY]);
        assert!(!result.is_valid);
        assert!(result.has_inf);
    }

    #[test]
    fn test_clip_gradient() {
        // Norm of [3.0, 4.0] = 5.0, clip to max_norm=2.0 -> scale=0.4 -> [1.2, 1.6]
        let result = GradientAggregator::clip_gradient(&[3.0, 4.0], 2.0);
        assert!((result[0] - 1.2).abs() < 1e-10);
        assert!((result[1] - 1.6).abs() < 1e-10);
    }

    #[test]
    fn test_clip_gradient_no_clip_needed() {
        let gradient = vec![1.0, 1.0];
        let result = GradientAggregator::clip_gradient(&gradient, 10.0);
        assert_eq!(result, gradient);
    }

    #[test]
    fn test_apply_dp_noise() {
        let agg = GradientAggregator::new(AggregationMethod::Dp);
        let gradient = vec![1.0, 2.0, 3.0];
        let noisy = agg.apply_dp_noise(&gradient, 0.1, 1.0);
        assert_eq!(noisy.len(), 3);
        for val in &noisy {
            assert!(val.is_finite());
        }
    }

    #[test]
    fn test_compress_decompress() {
        let agg = GradientAggregator::new(AggregationMethod::FedAvg).with_compression(0.5);
        let gradient = vec![0.1, 5.0, 0.2, 10.0, 0.3, 3.0, 0.4, 8.0];
        let compressed = agg.compress(&gradient);
        assert!(!compressed.is_empty());
        let decompressed = agg.decompress(&compressed, gradient.len());
        // Only top-4 values should be preserved
        assert_eq!(decompressed.len(), gradient.len());
        // The largest values should be present
        let max_val = *decompressed.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        assert_eq!(max_val, 10.0);
    }

    #[test]
    fn test_compress_decompress_empty() {
        let agg = GradientAggregator::new(AggregationMethod::FedAvg);
        let decompressed = agg.decompress(&[], 10);
        assert_eq!(decompressed, vec![0.0; 10]);
    }

    // -- FederatedTrainingEngine Tests --

    #[test]
    fn test_engine_new() {
        let engine = FederatedTrainingEngine::new();
        assert!(engine.trainings.is_empty());
        assert!(engine.participants.is_empty());
    }

    #[test]
    fn test_engine_create_training() {
        let engine = FederatedTrainingEngine::new();
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 5);
        let training = engine.create_training(config);
        assert_eq!(training.status, TrainingStatus::Created);
        assert!(engine.trainings.contains_key(&training.id));
    }

    #[test]
    fn test_engine_register_participant() {
        let engine = FederatedTrainingEngine::new();
        let node = ParticipantNode::new("node1".to_string(), "http://localhost:8001".to_string(), 500);
        let registered = engine.register_participant(node);
        assert!(engine.participants.contains_key(&registered.id));
    }

    #[test]
    fn test_engine_full_round() {
        let engine = FederatedTrainingEngine::new();
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 1);
        let training = engine.create_training(config);
        let tid = training.id.clone();

        // Simulate training
        {
            let mut t = engine.trainings.get_mut(&tid).unwrap();
            t.status = TrainingStatus::Running;
        }

        engine.start_round(&tid).unwrap();
        engine
            .submit_gradient(&tid, "p1", vec![1.0, 2.0], 100, 1)
            .unwrap();
        engine
            .submit_gradient(&tid, "p2", vec![3.0, 4.0], 200, 1)
            .unwrap();
        let agg = engine.aggregate_gradients(&tid).unwrap();
        // Weighted: (100/300)*[1,2] + (200/300)*[3,4] = [2.333, 3.333]
        assert!((agg[0] - 7.0 / 3.0).abs() < 1e-10);
        assert!((agg[1] - 10.0 / 3.0).abs() < 1e-10);
        engine.evaluate_round(&tid, 0.5, 0.9).unwrap();
        engine.complete_round(&tid).unwrap();
        engine.complete_training(&tid).unwrap();

        let t = engine.get_training(&tid).unwrap();
        assert_eq!(t.status, TrainingStatus::Completed);
        assert_eq!(t.best_loss, Some(0.5));
    }

    #[test]
    fn test_engine_get_training_stats() {
        let engine = FederatedTrainingEngine::new();
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 10);
        let training = engine.create_training(config);
        let stats = engine.get_training_stats(&training.id).unwrap();
        assert_eq!(stats.current_round, 0);
        assert_eq!(stats.total_rounds, 10);
    }

    #[test]
    fn test_engine_system_stats() {
        let engine = FederatedTrainingEngine::new();
        let stats = engine.get_system_stats();
        assert_eq!(stats.total_trainings, 0);
        assert_eq!(stats.total_participants, 0);

        engine.create_training(FederatedConfig::new(
            "t1".to_string(),
            AggregationStrategy::FedAvg,
            5,
        ));
        engine.register_participant(ParticipantNode::new(
            "p1".to_string(),
            "http://localhost".to_string(),
            100,
        ));

        let stats = engine.get_system_stats();
        assert_eq!(stats.total_trainings, 1);
        assert_eq!(stats.total_participants, 1);
    }

    #[test]
    fn test_engine_heartbeat() {
        let engine = FederatedTrainingEngine::new();
        let node = ParticipantNode::new("p1".to_string(), "http://localhost".to_string(), 100);
        let registered = engine.register_participant(node);
        assert!(engine.heartbeat(&registered.id).is_ok());
        assert!(engine.heartbeat("nonexistent").is_err());
    }

    // -- TrainingJob Tests --

    #[tokio::test]
    async fn test_training_job_new() {
        let job = TrainingJob::new("test-job".to_string(), 3, 10);
        assert_eq!(job.name, "test-job");
        assert_eq!(job.total_shards, 3);
        assert_eq!(job.shards.len(), 3);
        assert_eq!(job.status, JobStatus::Created);
    }

    #[tokio::test]
    async fn test_training_job_assign_shard() {
        let mut job = TrainingJob::new("test".to_string(), 2, 5);
        let shard_id = job.shards[0].shard_id.clone();
        assert!(job.assign_shard(&shard_id, "worker1").is_ok());
        assert_eq!(job.shards[0].worker_id.as_deref(), Some("worker1"));
        // Re-assign should fail
        assert!(job.assign_shard(&shard_id, "worker2").is_err());
    }

    #[tokio::test]
    async fn test_training_job_checkpoint_resume() {
        let mut job = TrainingJob::new("test".to_string(), 1, 5);
        let cp = job.checkpoint(1, vec![1.0, 2.0], 0.5, 0.9);
        assert_eq!(cp.round_number, 1);
        assert_eq!(job.checkpoints.len(), 1);
        assert!(job.resume().is_ok());
        assert_eq!(job.status, JobStatus::Resumed);
    }

    #[tokio::test]
    async fn test_training_job_cancel() {
        let mut job = TrainingJob::new("test".to_string(), 1, 5);
        job.cancel();
        assert_eq!(job.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_training_job_report_progress() {
        let mut job = TrainingJob::new("test".to_string(), 1, 5);
        let report = ProgressReport {
            job_id: job.id.clone(),
            worker_id: "w1".to_string(),
            shard_id: "s1".to_string(),
            epochs_completed: 3,
            total_epochs: 5,
            current_loss: 0.5,
            current_accuracy: 0.9,
            samples_processed: 500,
            timestamp: now_millis(),
        };
        assert!(job.report_progress(report).is_ok());
        assert_eq!(job.progress_reports.len(), 1);
    }

    // -- DistributedCoordinator Tests --

    #[tokio::test]
    async fn test_coordinator_create_job() {
        let coord = DistributedCoordinator::new();
        let job = coord.create_job("test".to_string(), 2, 5).await;
        assert_eq!(job.name, "test");
        assert_eq!(job.total_shards, 2);
    }

    #[tokio::test]
    async fn test_coordinator_assign_shard() {
        let coord = DistributedCoordinator::new();
        let job = coord.create_job("test".to_string(), 2, 5).await;
        let shard_id = job.shards[0].shard_id.clone();
        assert!(coord
            .assign_shard(&job.id, &shard_id, "worker1")
            .await
            .is_ok());
        assert!(coord
            .assign_shard("nonexistent", &shard_id, "worker1")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_coordinator_checkpoint_and_resume() {
        let coord = DistributedCoordinator::new();
        let job = coord.create_job("test".to_string(), 1, 5).await;
        coord
            .checkpoint(&job.id, 1, vec![1.0, 2.0], 0.5, 0.9)
            .await
            .unwrap();
        assert!(coord.resume(&job.id).await.is_ok());
        // Resume without checkpoint should fail
        let job2 = coord.create_job("test2".to_string(), 1, 5).await;
        assert!(coord.resume(&job2.id).await.is_err());
    }

    #[tokio::test]
    async fn test_coordinator_cancel() {
        let coord = DistributedCoordinator::new();
        let job = coord.create_job("test".to_string(), 1, 5).await;
        assert!(coord.cancel(&job.id).await.is_ok());
        assert!(coord.cancel("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_coordinator_list_jobs() {
        let coord = DistributedCoordinator::new();
        coord.create_job("j1".to_string(), 1, 5).await;
        coord.create_job("j2".to_string(), 2, 5).await;
        let jobs = coord.list_jobs().await;
        assert_eq!(jobs.len(), 2);
    }

    // -- ApiResponse Tests --

    #[test]
    fn test_api_response_ok() {
        let resp = ApiResponse::ok(42);
        assert!(resp.success);
        assert_eq!(resp.data, Some(42));
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_api_response_err() {
        let resp: ApiResponse<String> = ApiResponse::err("bad".to_string());
        assert!(!resp.success);
        assert!(resp.data.is_none());
        assert_eq!(resp.error, Some("bad".to_string()));
    }

    // -- Edge Cases --

    #[test]
    fn test_max_rounds_reached() {
        let config = FederatedConfig::new("test".to_string(), AggregationStrategy::FedAvg, 1);
        let mut training = FederatedTraining::create(config);
        training.status = TrainingStatus::Running;
        // Round 1
        assert!(training.start_round().is_some());
        // Round 2 should fail (max is 1)
        assert!(training.start_round().is_none());
    }

    #[test]
    fn test_empty_gradient_vector() {
        let agg = GradientAggregator::new(AggregationMethod::FedAvg);
        let result = agg.aggregate(&[(vec![], 100)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_secure_aggregator_rejects_invalid() {
        let agg = GradientAggregator::new(AggregationMethod::Secure);
        let gradients = vec![
            (vec![f64::NAN, 2.0], 100u64),
            (vec![3.0, 4.0], 100u64),
        ];
        let result = agg.aggregate(&gradients).unwrap();
        // Only the valid gradient should be included
        assert!((result[0] - 3.0).abs() < 1e-10);
        assert!((result[1] - 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_now_millis_is_reasonable() {
        let t = now_millis();
        // Should be a positive number, roughly current time
        assert!(t > 1_700_000_000_000); // After 2023
        assert!(t < 2_000_000_000_000); // Before 2033
    }
}
