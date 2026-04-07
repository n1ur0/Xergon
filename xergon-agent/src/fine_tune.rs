//! Fine-tuning orchestration for the Xergon agent.
//!
//! Manages fine-tuning jobs (LoRA, QLoRA, Full) with VRAM validation,
//! progress tracking, cancellation, and adapter export.
//!
//! API:
//! - POST   /api/fine-tune/create           -- create fine-tune job
//! - GET    /api/fine-tune/jobs             -- list jobs
//! - GET    /api/fine-tune/jobs/{id}        -- job status + progress
//! - POST   /api/fine-tune/jobs/{id}/cancel -- cancel job
//! - DELETE /api/fine-tune/jobs/{id}        -- delete job
//! - POST   /api/fine-tune/jobs/{id}/export -- export trained adapter

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FineTuneMethod {
    LoRA,
    QLoRA,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuneConfig {
    pub model: String,
    pub dataset_path: String,
    pub method: FineTuneMethod,
    pub epochs: u32,
    pub learning_rate: f64,
    pub batch_size: u32,
    pub lora_r: u32,
    pub lora_alpha: u32,
    pub max_seq_length: u32,
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FineTuneStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuneJob {
    pub id: String,
    pub config: FineTuneConfig,
    pub status: FineTuneStatus,
    pub progress: f64,
    pub epoch: u32,
    pub total_epochs: u32,
    pub loss: f64,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobRequest {
    pub model: String,
    pub dataset_path: String,
    pub method: Option<FineTuneMethod>,
    pub epochs: Option<u32>,
    pub learning_rate: Option<f64>,
    pub batch_size: Option<u32>,
    pub lora_r: Option<u32>,
    pub lora_alpha: Option<u32>,
    pub max_seq_length: Option<u32>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobResponse {
    pub id: String,
    pub status: FineTuneStatus,
    pub estimated_vram_mb: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub format: Option<String>,
    pub destination: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResponse {
    pub job_id: String,
    pub adapter_path: String,
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Fine-tune manager
// ---------------------------------------------------------------------------

/// VRAM estimates for different fine-tuning methods (in MiB) per billion parameters.
const VRAM_PER_B_LORA: u64 = 6000;
const VRAM_PER_B_QLORA: u64 = 4000;
const VRAM_PER_B_FULL: u64 = 16000;

/// Minimum base VRAM overhead regardless of model size.
const BASE_VRAM_OVERHEAD_MB: u64 = 2000;

/// Estimate model parameter count in billions from model name.
fn estimate_model_params_gb(model: &str) -> f64 {
    let lower = model.to_lowercase();
    // Common naming patterns: "llama-7b", "mistral-7b", "llama-13b", etc.
    for suffix in &["70b", "65b", "34b", "33b", "13b", "8b", "7b", "3b", "1b", "0.5b"] {
        if lower.contains(suffix) {
            let num_str = suffix.trim_end_matches('b');
            return num_str.parse::<f64>().unwrap_or(7.0);
        }
    }
    // Default assumption
    7.0
}

/// Estimate VRAM required for a fine-tuning job.
pub fn estimate_vram(config: &FineTuneConfig) -> u64 {
    let params_b = estimate_model_params_gb(&config.model);
    let per_b = match config.method {
        FineTuneMethod::LoRA => VRAM_PER_B_LORA,
        FineTuneMethod::QLoRA => VRAM_PER_B_QLORA,
        FineTuneMethod::Full => VRAM_PER_B_FULL,
    };
    // Scale VRAM by sequence length factor
    let seq_factor = if config.max_seq_length > 2048 {
        (config.max_seq_length as f64) / 2048.0
    } else {
        1.0
    };
    // Batch size factor
    let batch_factor = if config.batch_size > 4 {
        (config.batch_size as f64) / 4.0
    } else {
        1.0
    };

    let estimated = (params_b * per_b as f64 * seq_factor * batch_factor) as u64 + BASE_VRAM_OVERHEAD_MB;
    estimated
}

/// Validate fine-tune config before creating a job.
fn validate_config(config: &FineTuneConfig) -> Result<(), String> {
    if config.model.is_empty() {
        return Err("Model name is required".into());
    }
    if config.dataset_path.is_empty() {
        return Err("Dataset path is required".into());
    }
    if config.epochs == 0 {
        return Err("Epochs must be at least 1".into());
    }
    if config.epochs > 100 {
        return Err("Epochs cannot exceed 100".into());
    }
    if config.learning_rate <= 0.0 || config.learning_rate > 1.0 {
        return Err("Learning rate must be between 0 and 1".into());
    }
    if config.batch_size == 0 {
        return Err("Batch size must be at least 1".into());
    }
    if config.max_seq_length == 0 || config.max_seq_length > 131072 {
        return Err("Max sequence length must be between 1 and 131072".into());
    }
    if config.method != FineTuneMethod::Full {
        if config.lora_r == 0 {
            return Err("LoRA rank (lora_r) must be at least 1".into());
        }
        if config.lora_alpha == 0 {
            return Err("LoRA alpha must be at least 1".into());
        }
    }

    // Check dataset path exists
    if !std::path::Path::new(&config.dataset_path).exists() {
        return Err(format!("Dataset path does not exist: {}", config.dataset_path));
    }

    Ok(())
}

pub struct FineTuneManager {
    jobs: DashMap<String, FineTuneJob>,
    processes: DashMap<String, Child>,
    active_jobs: AtomicU32,
    max_concurrent_jobs: u32,
}

impl FineTuneManager {
    pub fn new(max_concurrent_jobs: u32) -> Self {
        Self {
            jobs: DashMap::new(),
            processes: DashMap::new(),
            active_jobs: AtomicU32::new(0),
            max_concurrent_jobs,
        }
    }

    /// Check if there is available VRAM for a fine-tuning job.
    pub fn check_vram_available(&self, required_mb: u64) -> Result<(), String> {
        let hw = crate::hardware::detect_hardware();
        let available_mb: u64 = hw
            .gpus
            .iter()
            .map(|g| g.vram_mb - g.vram_used_mb.unwrap_or(0))
            .sum();

        if hw.gpus.is_empty() {
            return Err("No GPUs detected on this system".into());
        }

        if available_mb < required_mb {
            return Err(format!(
                "Insufficient VRAM: required {} MB, available {} MB across {} GPU(s)",
                required_mb,
                available_mb,
                hw.gpus.len()
            ));
        }

        Ok(())
    }

    /// Create a new fine-tune job.
    pub fn create_job(&self, req: CreateJobRequest) -> Result<CreateJobResponse, String> {
        let method = req.method.unwrap_or(FineTuneMethod::LoRA);
        let config = FineTuneConfig {
            model: req.model,
            dataset_path: req.dataset_path,
            method: method.clone(),
            epochs: req.epochs.unwrap_or(3),
            learning_rate: req.learning_rate.unwrap_or(2e-5),
            batch_size: req.batch_size.unwrap_or(4),
            lora_r: req.lora_r.unwrap_or(8),
            lora_alpha: req.lora_alpha.unwrap_or(16),
            max_seq_length: req.max_seq_length.unwrap_or(2048),
            output_dir: req.output_dir.unwrap_or_else(|| "./fine-tune-output".into()),
        };

        validate_config(&config)?;

        let estimated_vram = estimate_vram(&config);
        self.check_vram_available(estimated_vram)?;

        if self.active_jobs.load(Ordering::Relaxed) >= self.max_concurrent_jobs {
            return Err(format!(
                "Maximum concurrent jobs ({}) reached. Wait for a job to finish.",
                self.max_concurrent_jobs
            ));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let job = FineTuneJob {
            id: id.clone(),
            config,
            status: FineTuneStatus::Queued,
            progress: 0.0,
            epoch: 0,
            total_epochs: 0,
            loss: 0.0,
            started_at: None,
            completed_at: None,
            error: None,
        };

        self.jobs.insert(id.clone(), job);

        Ok(CreateJobResponse {
            id,
            status: FineTuneStatus::Queued,
            estimated_vram_mb: estimated_vram,
            message: "Job created and queued".into(),
        })
    }

    /// List all fine-tune jobs.
    pub fn list_jobs(&self) -> Vec<FineTuneJob> {
        self.jobs.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a specific job by ID.
    pub fn get_job(&self, id: &str) -> Option<FineTuneJob> {
        self.jobs.get(id).map(|r| r.value().clone())
    }

    /// Cancel a running or queued job.
    pub fn cancel_job(&self, id: &str) -> Result<(), String> {
        let mut job = self
            .jobs
            .get_mut(id)
            .ok_or_else(|| format!("Job {} not found", id))?;

        match job.status {
            FineTuneStatus::Queued => {
                job.status = FineTuneStatus::Cancelled;
                job.completed_at = Some(Utc::now());
                info!(job_id = %id, "Cancelled queued fine-tune job");
                Ok(())
            }
            FineTuneStatus::Running => {
                // Kill the subprocess
                if let Some((_, mut child)) = self.processes.remove(id) {
                    match child.start_kill() {
                        Ok(()) => {
                            job.status = FineTuneStatus::Cancelled;
                            job.completed_at = Some(Utc::now());
                            self.active_jobs.fetch_sub(1, Ordering::Relaxed);
                            info!(job_id = %id, "Cancelled running fine-tune job");
                            Ok(())
                        }
                        Err(e) => {
                            Err(format!("Failed to kill fine-tune process: {}", e))
                        }
                    }
                } else {
                    job.status = FineTuneStatus::Cancelled;
                    job.completed_at = Some(Utc::now());
                    self.active_jobs.fetch_sub(1, Ordering::Relaxed);
                    Ok(())
                }
            }
            FineTuneStatus::Completed => Err("Cannot cancel a completed job".into()),
            FineTuneStatus::Failed => Err("Cannot cancel a failed job".into()),
            FineTuneStatus::Cancelled => Err("Job is already cancelled".into()),
        }
    }

    /// Delete a job.
    pub fn delete_job(&self, id: &str) -> Result<(), String> {
        // Cancel first if running
        let _ = self.cancel_job(id);

        if self.jobs.remove(id).is_some() {
            self.processes.remove(id);
            Ok(())
        } else {
            Err(format!("Job {} not found", id))
        }
    }

    /// Export a completed job's adapter.
    pub fn export_job(&self, id: &str, _req: ExportRequest) -> Result<ExportResponse, String> {
        let job = self
            .jobs
            .get(id)
            .ok_or_else(|| format!("Job {} not found", id))?;

        if job.status != FineTuneStatus::Completed {
            return Err(format!(
                "Cannot export job with status {:?}. Only completed jobs can be exported.",
                job.status
            ));
        }

        let adapter_path = format!("{}/adapter", job.config.output_dir);

        // Check if adapter files exist
        if !std::path::Path::new(&adapter_path).exists() {
            return Err(format!("Adapter files not found at {}", adapter_path));
        }

        let metadata = serde_json::json!({
            "model": job.config.model,
            "method": format!("{:?}", job.config.method),
            "epochs": job.config.epochs,
            "final_loss": job.loss,
            "lora_r": job.config.lora_r,
            "lora_alpha": job.config.lora_alpha,
            "max_seq_length": job.config.max_seq_length,
            "completed_at": job.completed_at,
        });

        Ok(ExportResponse {
            job_id: id.to_string(),
            adapter_path,
            metadata,
        })
    }

    /// Start a queued job (called by background task or on-demand).
    pub async fn start_job(&self, id: &str) -> Result<(), String> {
        let mut job = self
            .jobs
            .get_mut(id)
            .ok_or_else(|| format!("Job {} not found", id))?;

        if job.status != FineTuneStatus::Queued {
            return Err(format!("Job {} is not queued (status: {:?})", id, job.status));
        }

        job.status = FineTuneStatus::Running;
        job.started_at = Some(Utc::now());
        job.total_epochs = job.config.epochs;
        job.epoch = 0;
        job.progress = 0.0;
        job.loss = 0.0;

        self.active_jobs.fetch_add(1, Ordering::Relaxed);

        // Launch fine-tuning subprocess
        let config = job.config.clone();
        let job_id = id.to_string();
        let jobs_ref = self.jobs.clone();

        // Build the training command
        let cmd = build_training_command(&config);

        match tokio::process::Command::new(&cmd.program)
            .args(&cmd.args)
            .env("XERGON_FT_JOB_ID", &job_id)
            .env("XERGON_FT_MODEL", &config.model)
            .env("XERGON_FT_METHOD", format!("{:?}", config.method))
            .env("XERGON_FT_EPOCHS", config.epochs.to_string())
            .env("XERGON_FT_LR", config.learning_rate.to_string())
            .env("XERGON_FT_BATCH_SIZE", config.batch_size.to_string())
            .env("XERGON_FT_LORA_R", config.lora_r.to_string())
            .env("XERGON_FT_LORA_ALPHA", config.lora_alpha.to_string())
            .env("XERGON_FT_MAX_SEQ", config.max_seq_length.to_string())
            .env("XERGON_FT_OUTPUT_DIR", &config.output_dir)
            .env("XERGON_FT_DATASET", &config.dataset_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => {
                info!(job_id = %id, program = %cmd.program, "Started fine-tune subprocess");
                // Store the child process for cancellation
                // Note: we can't store it in DashMap with async spawning,
                // so we spawn a monitor task instead
                let child_id = id.to_string();
                let active_ref = &self.active_jobs as *const AtomicU32;
                // SAFETY: FineTuneManager lives for the program lifetime
                let active = unsafe { &*active_ref };

                tokio::spawn(async move {
                    monitor_training_process(child, child_id, jobs_ref, active, config.epochs).await;
                });
                Ok(())
            }
            Err(e) => {
                let mut j = self.jobs.get_mut(id).unwrap();
                j.status = FineTuneStatus::Failed;
                j.completed_at = Some(Utc::now());
                j.error = Some(format!("Failed to start training process: {}", e));
                self.active_jobs.fetch_sub(1, Ordering::Relaxed);
                Err(format!("Failed to start training process: {}", e))
            }
        }
    }

    /// Start queued jobs up to concurrency limit.
    pub async fn start_queued_jobs(&self) {
        let queued: Vec<String> = self
            .jobs
            .iter()
            .filter(|r| r.value().status == FineTuneStatus::Queued)
            .map(|r| r.key().clone())
            .collect();

        for job_id in queued {
            if self.active_jobs.load(Ordering::Relaxed) >= self.max_concurrent_jobs {
                break;
            }
            match self.start_job(&job_id).await {
                Ok(()) => debug!(job_id = %job_id, "Started queued fine-tune job"),
                Err(e) => warn!(job_id = %job_id, error = %e, "Failed to start queued job"),
            }
        }
    }
}

struct TrainingCommand {
    program: String,
    args: Vec<String>,
}

/// Build the training command based on method and available tools.
fn build_training_command(config: &FineTuneConfig) -> TrainingCommand {
    // Try to use Ollama's fine-tuning or a custom Python trainer
    let mut args = Vec::new();

    // Check if xergon-trainer script exists
    let program = if std::path::Path::new("xergon-trainer").exists() {
        "xergon-trainer".to_string()
    } else if std::path::Path::new("/usr/local/bin/xergon-trainer").exists() {
        "/usr/local/bin/xergon-trainer".to_string()
    } else {
        // Fallback: use python3 with a training script
        "python3".to_string()
    };

    if program.contains("python") {
        args.push("-m".to_string());
        args.push("xergon.train".to_string());
    }

    args.push("--model".to_string());
    args.push(config.model.clone());
    args.push("--dataset".to_string());
    args.push(config.dataset_path.clone());
    args.push("--method".to_string());
    args.push(format!("{:?}", config.method).to_lowercase());
    args.push("--epochs".to_string());
    args.push(config.epochs.to_string());
    args.push("--lr".to_string());
    args.push(config.learning_rate.to_string());
    args.push("--batch-size".to_string());
    args.push(config.batch_size.to_string());
    args.push("--output-dir".to_string());
    args.push(config.output_dir.clone());

    if config.method != FineTuneMethod::Full {
        args.push("--lora-r".to_string());
        args.push(config.lora_r.to_string());
        args.push("--lora-alpha".to_string());
        args.push(config.lora_alpha.to_string());
    }

    args.push("--max-seq-length".to_string());
    args.push(config.max_seq_length.to_string());

    TrainingCommand { program, args }
}

/// Monitor a training subprocess and update job state.
async fn monitor_training_process(
    mut child: Child,
    job_id: String,
    jobs: DashMap<String, FineTuneJob>,
    active: &AtomicU32,
    total_epochs: u32,
) {
    // Read stderr for progress lines
    let stderr = child.stderr.take();
    if let Some(mut stderr) = stderr {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            // Parse progress lines like: {"epoch": 1, "loss": 0.5234, "progress": 0.33}
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(mut job) = jobs.get_mut(&job_id) {
                    if let Some(epoch) = parsed.get("epoch").and_then(|v| v.as_u64()) {
                        job.epoch = epoch as u32;
                    }
                    if let Some(loss) = parsed.get("loss").and_then(|v| v.as_f64()) {
                        job.loss = loss;
                    }
                    if let Some(progress) = parsed.get("progress").and_then(|v| v.as_f64()) {
                        job.progress = progress;
                    }
                }
            }
        }
    }

    // Wait for process to finish
    let status = child.wait().await.unwrap_or_default();

    if let Some(mut job) = jobs.get_mut(&job_id) {
        job.completed_at = Some(Utc::now());
        if status.success() {
            job.status = FineTuneStatus::Completed;
            job.progress = 1.0;
            job.epoch = total_epochs;
            info!(job_id = %job_id, "Fine-tune job completed successfully");
        } else {
            job.status = FineTuneStatus::Failed;
            job.error = Some(format!("Training process exited with code: {:?}", status.code()));
            error!(job_id = %job_id, code = ?status.code(), "Fine-tune job failed");
        }
    }

    active.fetch_sub(1, Ordering::Relaxed);
}
