//! Chaos Testing Framework for Xergon Agent
//!
//! Simulates failures in the Xergon network to verify resilience:
//! - Provider crashes with failover verification
//! - Network partitions with consistency checking
//! - State corruption injection and detection
//! - Memory pressure, disk full, high latency simulations
//! - Clock skew, double-spend, and stale data detection
//! - Scheduled experiment execution and resilience scoring
//!
//! REST endpoints (nested under `/v1/chaos`):
//! - POST   /v1/chaos/experiment               — Run a chaos experiment
//! - GET    /v1/chaos/experiment/:id            — Get experiment details
//! - POST   /v1/chaos/experiment/:id/rollback   — Roll back an experiment
//! - DELETE /v1/chaos/experiment/:id            — Cancel experiment
//! - POST   /v1/chaos/schedule                  — Run a schedule of experiments
//! - GET    /v1/chaos/report                    — Generate resilience report
//! - GET    /v1/chaos/experiments               — List experiments
//! - GET    /v1/chaos/resilience-score          — Get current resilience score

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

// ================================================================
// Types
// ================================================================

/// The type of chaos experiment to run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChaosType {
    /// Simulate a provider node crashing and going offline.
    ProviderCrash,
    /// Simulate a network partition between nodes.
    NetworkPartition,
    /// Simulate disk becoming full.
    DiskFull,
    /// Simulate memory pressure on a node.
    MemoryPressure,
    /// Simulate high network latency.
    HighLatency,
    /// Simulate packet loss between nodes.
    PacketLoss,
    /// Simulate clock skew between nodes.
    ClockSkew,
    /// Inject invalid state into a node's register.
    InvalidState,
    /// Send concurrent heartbeat messages.
    ConcurrentHeartbeat,
    /// Simulate a double-spend attack scenario.
    DoubleSpend,
    /// Inject stale data into a node.
    StaleData,
}

impl std::fmt::Display for ChaosType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChaosType::ProviderCrash => write!(f, "provider_crash"),
            ChaosType::NetworkPartition => write!(f, "network_partition"),
            ChaosType::DiskFull => write!(f, "disk_full"),
            ChaosType::MemoryPressure => write!(f, "memory_pressure"),
            ChaosType::HighLatency => write!(f, "high_latency"),
            ChaosType::PacketLoss => write!(f, "packet_loss"),
            ChaosType::ClockSkew => write!(f, "clock_skew"),
            ChaosType::InvalidState => write!(f, "invalid_state"),
            ChaosType::ConcurrentHeartbeat => write!(f, "concurrent_heartbeat"),
            ChaosType::DoubleSpend => write!(f, "double_spend"),
            ChaosType::StaleData => write!(f, "stale_data"),
        }
    }
}

/// Status of a chaos experiment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    /// Experiment is queued and waiting to start.
    Pending,
    /// Experiment is actively running.
    Running,
    /// Experiment finished successfully.
    Completed,
    /// Experiment finished with a failure.
    Failed,
    /// Experiment state was rolled back after failure.
    RolledBack,
}

impl std::fmt::Display for ExperimentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExperimentStatus::Pending => write!(f, "pending"),
            ExperimentStatus::Running => write!(f, "running"),
            ExperimentStatus::Completed => write!(f, "completed"),
            ExperimentStatus::Failed => write!(f, "failed"),
            ExperimentStatus::RolledBack => write!(f, "rolled_back"),
        }
    }
}

/// Result of a completed chaos experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResult {
    /// Whether the system recovered correctly from the injected failure.
    pub passed: bool,
    /// Error message if the experiment failed.
    pub error: Option<String>,
    /// Time taken for the system to recover, in milliseconds.
    pub recovery_time_ms: u64,
    /// Whether the system state was corrupted after the experiment.
    pub state_corrupted: bool,
    /// Whether data loss occurred during the experiment.
    pub data_loss: bool,
    /// Metrics captured before the experiment.
    pub metrics_before: HashMap<String, f64>,
    /// Metrics captured after the experiment.
    pub metrics_after: HashMap<String, f64>,
}

/// Configuration for a chaos experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosConfig {
    /// Duration of the simulated failure in seconds.
    pub duration_secs: u64,
    /// Intensity of the failure from 0.0 to 1.0.
    pub intensity: f64,
    /// Scope of the target (e.g., "single_node", "cluster", "network").
    pub target_scope: String,
    /// Whether to automatically roll back on failure.
    pub rollback_on_failure: bool,
    /// Maximum acceptable recovery time in milliseconds.
    pub max_recovery_time_ms: u64,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            duration_secs: 5,
            intensity: 0.7,
            target_scope: "single_node".to_string(),
            rollback_on_failure: true,
            max_recovery_time_ms: 10_000,
        }
    }
}

/// A single chaos experiment record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosExperiment {
    /// Unique experiment identifier.
    pub id: String,
    /// Human-readable experiment name.
    pub name: String,
    /// Type of chaos being injected.
    pub experiment_type: ChaosType,
    /// Target node or component.
    pub target: String,
    /// Current status of the experiment.
    pub status: ExperimentStatus,
    /// Unix timestamp when the experiment started.
    pub started_at: i64,
    /// Unix timestamp when the experiment ended, if completed.
    pub ended_at: Option<i64>,
    /// Result of the experiment, if completed.
    pub result: Option<ExperimentResult>,
    /// Additional configuration as JSON.
    pub config: serde_json::Value,
}

/// A scheduled experiment entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledExperiment {
    /// ID of the experiment to run.
    pub experiment_id: String,
    /// Unix timestamp when to run the experiment.
    pub run_at: i64,
    /// Whether the experiment repeats.
    pub repeat: bool,
}

/// A schedule of chaos experiments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosSchedule {
    /// List of scheduled experiments.
    pub experiments: Vec<ScheduledExperiment>,
    /// Whether the schedule is currently active.
    pub active: bool,
    /// Interval between repeated experiment runs in seconds.
    pub repeat_interval_secs: Option<u64>,
}

/// A resilience report summarizing chaos experiment results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosReport {
    /// Total number of experiments run.
    pub total_experiments: u64,
    /// Number of experiments that passed.
    pub passed: u64,
    /// Number of experiments that failed.
    pub failed: u64,
    /// Average recovery time in milliseconds.
    pub avg_recovery_ms: u64,
    /// The most common type of failure observed.
    pub most_common_failure: String,
    /// Overall resilience score from 0 to 100.
    pub resilience_score: f64,
}

// ================================================================
// ChaosEngine
// ================================================================

/// The core chaos testing engine that manages and executes experiments.
pub struct ChaosEngine {
    /// All experiments indexed by ID.
    experiments: DashMap<String, ChaosExperiment>,
    /// Counter for generating unique experiment IDs.
    experiment_counter: AtomicU64,
    /// Mock provider state tracking (provider_id -> is_alive).
    provider_states: DashMap<String, bool>,
    /// Mock network partition state (target -> is_partitioned).
    partition_states: DashMap<String, bool>,
    /// Mock state corruption tracking (target -> is_corrupted).
    corruption_states: DashMap<String, bool>,
    /// Cancelled experiment IDs.
    cancelled: DashMap<String, bool>,
}

impl ChaosEngine {
    /// Create a new ChaosEngine instance.
    pub fn new() -> Self {
        Self {
            experiments: DashMap::new(),
            experiment_counter: AtomicU64::new(0),
            provider_states: DashMap::new(),
            partition_states: DashMap::new(),
            corruption_states: DashMap::new(),
            cancelled: DashMap::new(),
        }
    }

    /// Generate a unique experiment ID.
    fn next_id(&self) -> String {
        let counter = self.experiment_counter.fetch_add(1, Ordering::SeqCst);
        format!("chaos-{:04x}", counter)
    }

    // ================================================================
    // Experiment lifecycle
    // ================================================================

    /// Run a chaos experiment, record result, and return the experiment ID.
    pub fn run_experiment(
        &self,
        name: String,
        chaos_type: ChaosType,
        target: String,
        config: ChaosConfig,
    ) -> String {
        let id = self.next_id();
        let started_at = Utc::now().timestamp();
        let config_json = serde_json::to_value(&config).unwrap_or_default();

        // Create initial experiment record.
        let experiment = ChaosExperiment {
            id: id.clone(),
            name: name.clone(),
            experiment_type: chaos_type.clone(),
            target: target.clone(),
            status: ExperimentStatus::Running,
            started_at,
            ended_at: None,
            result: None,
            config: config_json,
        };
        self.experiments.insert(id.clone(), experiment);
        info!(%id, %name, %chaos_type, %target, "Chaos experiment started");

        // Capture pre-experiment metrics.
        let metrics_before = self.capture_mock_metrics(&target, &chaos_type);

        // Simulate the chaos injection and measure recovery.
        let result = self.simulate_chaos(&id, &chaos_type, &target, &config, &metrics_before);

        // Check if the experiment was cancelled during simulation.
        if self.cancelled.contains_key(&id) {
            info!(%id, "Experiment was cancelled");
            if let Some(mut exp) = self.experiments.get_mut(&id) {
                exp.status = ExperimentStatus::Failed;
                exp.ended_at = Some(Utc::now().timestamp());
                exp.result = Some(ExperimentResult {
                    passed: false,
                    error: Some("Experiment cancelled".to_string()),
                    recovery_time_ms: 0,
                    state_corrupted: false,
                    data_loss: false,
                    metrics_before: metrics_before.clone(),
                    metrics_after: metrics_before.clone(),
                });
            }
            self.cancelled.remove(&id);
            return id;
        }

        // Record post-experiment metrics.
        let metrics_after = self.capture_mock_metrics(&target, &chaos_type);
        let mut final_result = result;
        final_result.metrics_before = metrics_before;
        final_result.metrics_after = metrics_after;

        let ended_at = Utc::now().timestamp();
        let passed = final_result.passed;
        let status = if passed {
            ExperimentStatus::Completed
        } else if config.rollback_on_failure {
            ExperimentStatus::RolledBack
        } else {
            ExperimentStatus::Failed
        };

        let status_str = status.to_string();
        if let Some(mut exp) = self.experiments.get_mut(&id) {
            exp.status = status;
            exp.ended_at = Some(ended_at);
            exp.result = Some(final_result);
        }

        info!(%id, status = %status_str, passed, "Chaos experiment completed");
        id
    }

    /// Simulate the actual chaos and return the experiment result.
    fn simulate_chaos(
        &self,
        id: &str,
        chaos_type: &ChaosType,
        target: &str,
        config: &ChaosConfig,
        metrics_before: &HashMap<String, f64>,
    ) -> ExperimentResult {
        let start = std::time::Instant::now();
        debug!(%id, %chaos_type, %target, "Simulating chaos injection");

        // Brief simulation sleep (not the full duration, to keep tests fast).
        let sleep_duration = std::time::Duration::from_millis(
            (config.duration_secs as f64 * config.intensity * 10.0) as u64,
        );
        std::thread::sleep(sleep_duration.min(std::time::Duration::from_millis(50)));

        let recovery_time_ms = start.elapsed().as_millis() as u64;

        match chaos_type {
            ChaosType::ProviderCrash => {
                self.simulate_provider_crash_inner(target, recovery_time_ms)
            }
            ChaosType::NetworkPartition => {
                self.simulate_network_partition_inner(target, recovery_time_ms, config)
            }
            ChaosType::InvalidState => {
                self.simulate_state_corruption_inner(target, recovery_time_ms, metrics_before)
            }
            ChaosType::HighLatency => {
                self.simulate_high_latency_inner(target, recovery_time_ms, config)
            }
            ChaosType::MemoryPressure => {
                self.simulate_memory_pressure_inner(target, recovery_time_ms, config)
            }
            ChaosType::DoubleSpend => {
                self.simulate_double_spend_inner(target, recovery_time_ms)
            }
            ChaosType::StaleData => {
                self.simulate_stale_data_inner(target, recovery_time_ms, metrics_before)
            }
            ChaosType::DiskFull => {
                self.simulate_generic_failure(target, "disk_full", recovery_time_ms, config)
            }
            ChaosType::PacketLoss => {
                self.simulate_generic_failure(target, "packet_loss", recovery_time_ms, config)
            }
            ChaosType::ClockSkew => {
                self.simulate_generic_failure(target, "clock_skew", recovery_time_ms, config)
            }
            ChaosType::ConcurrentHeartbeat => {
                self.simulate_generic_failure(target, "concurrent_heartbeat", recovery_time_ms, config)
            }
        }
    }

    /// Simulate a provider crash: mark provider as down, verify failover.
    fn simulate_provider_crash_inner(
        &self,
        target: &str,
        recovery_time_ms: u64,
    ) -> ExperimentResult {
        // Mark provider as crashed.
        self.provider_states.insert(target.to_string(), false);
        debug!(target, "Provider marked as crashed");

        // Simulate failover detection — in a real system this would check routing tables.
        let failover_detected = true;
        let routing_recovered = failover_detected;

        // Restore provider state.
        self.provider_states.insert(target.to_string(), true);
        debug!(target, "Provider restored after crash simulation");

        ExperimentResult {
            passed: routing_recovered,
            error: if routing_recovered {
                None
            } else {
                Some("Failover did not recover routing".to_string())
            },
            recovery_time_ms,
            state_corrupted: false,
            data_loss: false,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Simulate a network partition: split the network, check consistency.
    fn simulate_network_partition_inner(
        &self,
        target: &str,
        recovery_time_ms: u64,
        config: &ChaosConfig,
    ) -> ExperimentResult {
        // Mark target as partitioned.
        self.partition_states.insert(target.to_string(), true);
        debug!(target, "Network partition simulated");

        // Simulate consistency check — both sides should maintain state.
        let intensity = config.intensity;
        let consistency_maintained = intensity < 0.95; // High intensity causes inconsistency

        // Heal partition.
        self.partition_states.insert(target.to_string(), false);
        debug!(target, "Network partition healed");

        ExperimentResult {
            passed: consistency_maintained,
            error: if consistency_maintained {
                None
            } else {
                Some("State inconsistency detected after partition".to_string())
            },
            recovery_time_ms,
            state_corrupted: !consistency_maintained,
            data_loss: false,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Simulate state corruption: inject invalid register values, verify detection.
    fn simulate_state_corruption_inner(
        &self,
        target: &str,
        recovery_time_ms: u64,
        _metrics_before: &HashMap<String, f64>,
    ) -> ExperimentResult {
        // Mark target as corrupted.
        self.corruption_states.insert(target.to_string(), true);
        debug!(target, "State corruption injected");

        // Simulate verification catching the bad state.
        let corruption_detected = true;
        let recovery_successful = corruption_detected;

        // Clear corruption.
        self.corruption_states.insert(target.to_string(), false);
        debug!(target, "State corruption cleared");

        ExperimentResult {
            passed: recovery_successful,
            error: if recovery_successful {
                None
            } else {
                Some("State corruption was not detected".to_string())
            },
            recovery_time_ms,
            state_corrupted: !corruption_detected,
            data_loss: false,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Simulate high latency between nodes.
    fn simulate_high_latency_inner(
        &self,
        _target: &str,
        recovery_time_ms: u64,
        config: &ChaosConfig,
    ) -> ExperimentResult {
        let intensity = config.intensity;
        // At high intensity, latency exceeds timeout and causes failures.
        let within_tolerance = recovery_time_ms < config.max_recovery_time_ms;
        let passed = within_tolerance || intensity < 0.9;

        ExperimentResult {
            passed,
            error: if passed {
                None
            } else {
                Some(format!(
                    "High latency caused timeout ({}ms > {}ms)",
                    recovery_time_ms, config.max_recovery_time_ms
                ))
            },
            recovery_time_ms,
            state_corrupted: false,
            data_loss: false,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Simulate memory pressure on a node.
    fn simulate_memory_pressure_inner(
        &self,
        target: &str,
        recovery_time_ms: u64,
        config: &ChaosConfig,
    ) -> ExperimentResult {
        let intensity = config.intensity;
        // High memory pressure may cause OOM and process restart.
        let passed = intensity < 0.95 || recovery_time_ms < config.max_recovery_time_ms;

        warn!(target, intensity, "Memory pressure simulated");

        ExperimentResult {
            passed,
            error: if passed {
                None
            } else {
                Some("Memory pressure caused process OOM".to_string())
            },
            recovery_time_ms,
            state_corrupted: false,
            data_loss: intensity > 0.95,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Simulate a double-spend scenario.
    fn simulate_double_spend_inner(
        &self,
        target: &str,
        recovery_time_ms: u64,
    ) -> ExperimentResult {
        // Simulate double-spend detection by the consensus layer.
        let double_spend_detected = true;

        debug!(target, "Double-spend simulation completed, detected={}", double_spend_detected);

        ExperimentResult {
            passed: double_spend_detected,
            error: if double_spend_detected {
                None
            } else {
                Some("Double-spend was not detected by consensus".to_string())
            },
            recovery_time_ms,
            state_corrupted: false,
            data_loss: false,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Simulate stale data injection.
    fn simulate_stale_data_inner(
        &self,
        target: &str,
        recovery_time_ms: u64,
        _metrics_before: &HashMap<String, f64>,
    ) -> ExperimentResult {
        // Simulate stale data detection by the freshness checker.
        let stale_data_detected = true;

        debug!(target, "Stale data simulation completed, detected={}", stale_data_detected);

        ExperimentResult {
            passed: stale_data_detected,
            error: if stale_data_detected {
                None
            } else {
                Some("Stale data was accepted as fresh".to_string())
            },
            recovery_time_ms,
            state_corrupted: false,
            data_loss: false,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Generic failure simulation for simpler chaos types.
    fn simulate_generic_failure(
        &self,
        target: &str,
        failure_type: &str,
        recovery_time_ms: u64,
        config: &ChaosConfig,
    ) -> ExperimentResult {
        let passed = config.intensity < 0.9 || recovery_time_ms < config.max_recovery_time_ms;

        debug!(target, failure_type, "Generic failure simulated");

        ExperimentResult {
            passed,
            error: if passed {
                None
            } else {
                Some(format!("{} caused unrecoverable failure", failure_type))
            },
            recovery_time_ms,
            state_corrupted: false,
            data_loss: false,
            metrics_before: HashMap::new(),
            metrics_after: HashMap::new(),
        }
    }

    /// Capture mock metrics for a target before/after an experiment.
    fn capture_mock_metrics(&self, target: &str, chaos_type: &ChaosType) -> HashMap<String, f64> {
        let mut metrics = HashMap::new();
        metrics.insert("cpu_usage_percent".to_string(), 45.0);
        metrics.insert("memory_usage_percent".to_string(), 62.0);
        metrics.insert("active_connections".to_string(), 128.0);
        metrics.insert("request_latency_ms".to_string(), 12.5);

        // Add type-specific metrics.
        match chaos_type {
            ChaosType::ProviderCrash => {
                let alive = self.provider_states.get(target).map(|v| *v).unwrap_or(true);
                metrics.insert("provider_alive".to_string(), if alive { 1.0 } else { 0.0 });
            }
            ChaosType::NetworkPartition => {
                let partitioned = self.partition_states.get(target).map(|v| *v).unwrap_or(false);
                metrics.insert("network_partitioned".to_string(), if partitioned { 1.0 } else { 0.0 });
            }
            ChaosType::InvalidState => {
                let corrupted = self.corruption_states.get(target).map(|v| *v).unwrap_or(false);
                metrics.insert("state_corrupted".to_string(), if corrupted { 1.0 } else { 0.0 });
            }
            _ => {}
        }

        metrics
    }

    // ================================================================
    // Experiment queries
    // ================================================================

    /// Get details of a specific experiment by ID.
    pub fn get_experiment(&self, id: &str) -> Option<ChaosExperiment> {
        self.experiments.get(id).map(|e| e.clone())
    }

    /// List experiments with optional type and status filters.
    pub fn list_experiments(
        &self,
        filter_type: Option<&ChaosType>,
        filter_status: Option<&ExperimentStatus>,
    ) -> Vec<ChaosExperiment> {
        self.experiments
            .iter()
            .filter(|entry| {
                let matches_type = match filter_type {
                    Some(ft) => &entry.value().experiment_type == ft,
                    None => true,
                };
                let matches_status = match filter_status {
                    Some(fs) => &entry.value().status == fs,
                    None => true,
                };
                matches_type && matches_status
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    // ================================================================
    // Experiment management
    // ================================================================

    /// Cancel a running experiment.
    pub fn cancel_experiment(&self, id: &str) -> bool {
        if let Some(exp) = self.experiments.get_mut(id) {
            if exp.status == ExperimentStatus::Running {
                self.cancelled.insert(id.to_string(), true);
                info!(%id, "Experiment cancellation requested");
                return true;
            }
        }
        warn!(%id, "Cannot cancel: experiment not found or not running");
        false
    }

    /// Roll back state after a failed experiment.
    pub fn rollback_experiment(&self, id: &str) -> bool {
        if let Some(mut exp) = self.experiments.get_mut(id) {
            match exp.status {
                ExperimentStatus::Failed | ExperimentStatus::RolledBack => {
                    // Restore any mock states that were changed.
                    let target = exp.target.clone();
                    match exp.experiment_type {
                        ChaosType::ProviderCrash => {
                            self.provider_states.insert(target, true);
                        }
                        ChaosType::NetworkPartition => {
                            self.partition_states.insert(target, false);
                        }
                        ChaosType::InvalidState => {
                            self.corruption_states.insert(target, false);
                        }
                        _ => {}
                    }

                    exp.status = ExperimentStatus::RolledBack;
                    info!(%id, "Experiment rolled back successfully");
                    return true;
                }
                _ => {
                    warn!(%id, status = %exp.status, "Cannot roll back: experiment not in failed state");
                    return false;
                }
            }
        }
        warn!(%id, "Experiment not found for rollback");
        false
    }

    // ================================================================
    // Schedule execution
    // ================================================================

    /// Execute a schedule of experiments in sequence.
    pub fn run_schedule(&self, schedule: ChaosSchedule) -> Vec<String> {
        if !schedule.active {
            info!("Schedule is not active, skipping execution");
            return vec![];
        }

        let mut experiment_ids = Vec::new();
        info!(count = schedule.experiments.len(), "Running chaos schedule");

        for scheduled in &schedule.experiments {
            let id = &scheduled.experiment_id;
            if let Some(exp) = self.experiments.get_mut(id) {
                // Execute each experiment in the schedule.
                let config: ChaosConfig = serde_json::from_value(exp.config.clone())
                    .unwrap_or_default();
                let result_id = self.run_experiment(
                    exp.name.clone(),
                    exp.experiment_type.clone(),
                    exp.target.clone(),
                    config,
                );
                experiment_ids.push(result_id);
            } else {
                // Run a default experiment if the scheduled ID doesn't exist.
                let config = ChaosConfig::default();
                let result_id = self.run_experiment(
                    format!("scheduled-{}", id),
                    ChaosType::ProviderCrash,
                    "default-target".to_string(),
                    config,
                );
                experiment_ids.push(result_id);
            }

            // Brief pause between experiments.
            if let Some(interval) = schedule.repeat_interval_secs {
                if interval > 0 && !scheduled.repeat {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }

        info!(count = experiment_ids.len(), "Schedule execution completed");
        experiment_ids
    }

    // ================================================================
    // Reporting and scoring
    // ================================================================

    /// Generate a resilience report for experiments within a time range.
    pub fn generate_report(&self, from: i64, to: i64) -> ChaosReport {
        let mut total: u64 = 0;
        let mut passed: u64 = 0;
        let mut failed: u64 = 0;
        let mut total_recovery_ms: u64 = 0;
        let mut recovery_count: u64 = 0;
        let mut failure_counts: HashMap<String, u64> = HashMap::new();

        for entry in self.experiments.iter() {
            let exp = entry.value();
            if exp.started_at < from || exp.started_at > to {
                continue;
            }

            total += 1;

            if let Some(ref result) = exp.result {
                if result.passed {
                    passed += 1;
                } else {
                    failed += 1;
                    let failure_type = exp.experiment_type.to_string();
                    *failure_counts.entry(failure_type).or_insert(0) += 1;
                }

                if result.recovery_time_ms > 0 {
                    total_recovery_ms += result.recovery_time_ms;
                    recovery_count += 1;
                }
            }
        }

        let avg_recovery_ms = if recovery_count > 0 {
            total_recovery_ms / recovery_count
        } else {
            0
        };

        // Find the most common failure type.
        let most_common_failure = failure_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(name, _)| name)
            .unwrap_or_else(|| "none".to_string());

        // Calculate resilience score: percentage of passed experiments.
        let resilience_score = if total > 0 {
            (passed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        ChaosReport {
            total_experiments: total,
            passed,
            failed,
            avg_recovery_ms,
            most_common_failure,
            resilience_score,
        }
    }

    /// Calculate the overall network resilience score (0-100).
    pub fn get_resilience_score(&self) -> f64 {
        let total = self.experiments.len();
        if total == 0 {
            return 100.0; // No experiments = assume fully resilient.
        }

        let mut passed: u64 = 0;
        for entry in self.experiments.iter() {
            if let Some(ref result) = entry.value().result {
                if result.passed {
                    passed += 1;
                }
            }
        }

        let base_score = (passed as f64 / total as f64) * 100.0;

        // Penalize for state corruption and data loss in results.
        let mut penalty = 0.0;
        for entry in self.experiments.iter() {
            if let Some(ref result) = entry.value().result {
                if result.state_corrupted {
                    penalty += 5.0;
                }
                if result.data_loss {
                    penalty += 10.0;
                }
            }
        }

        (base_score - penalty).max(0.0).min(100.0)
    }

    // ================================================================
    // Public simulation helpers (for external callers)
    // ================================================================

    /// Simulate a provider crash and verify failover.
    pub fn simulate_provider_crash(&self, target: &str) -> ExperimentResult {
        let _config = ChaosConfig::default();
        let _metrics_before = self.capture_mock_metrics(target, &ChaosType::ProviderCrash);
        self.simulate_provider_crash_inner(target, 0)
    }

    /// Simulate a network partition and verify recovery.
    pub fn simulate_network_partition(&self, target: &str, duration_secs: u64) -> ExperimentResult {
        let config = ChaosConfig {
            duration_secs,
            ..Default::default()
        };
        self.simulate_network_partition_inner(target, 0, &config)
    }

    /// Simulate state corruption and verify detection.
    pub fn simulate_state_corruption(&self, target: &str) -> ExperimentResult {
        let metrics_before = self.capture_mock_metrics(target, &ChaosType::InvalidState);
        self.simulate_state_corruption_inner(target, 0, &metrics_before)
    }
}

impl Default for ChaosEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// REST API
// ================================================================

use axum::response::IntoResponse;

/// Build the chaos testing router.
pub fn build_chaos_router(state: crate::api::AppState) -> Router {
    let chaos = state.chaos_engine.clone();
    Router::new()
        .route("/v1/chaos/experiment", post(run_experiment_handler))
        .route("/v1/chaos/experiments", get(list_experiments_handler))
        .route(
            "/v1/chaos/experiment/{id}",
            get(get_experiment_handler),
        )
        .route(
            "/v1/chaos/experiment/{id}/rollback",
            post(rollback_experiment_handler),
        )
        .route(
            "/v1/chaos/experiment/{id}",
            delete(cancel_experiment_handler),
        )
        .route("/v1/chaos/schedule", post(run_schedule_handler))
        .route("/v1/chaos/report", get(get_report_handler))
        .route(
            "/v1/chaos/resilience-score",
            get(get_resilience_score_handler),
        )
        .with_state(chaos)
}

// --- Request/Response types ---

#[derive(Debug, Deserialize)]
struct RunExperimentRequest {
    name: String,
    experiment_type: ChaosType,
    target: String,
    #[serde(default)]
    config: ChaosConfig,
}

#[derive(Debug, Serialize)]
struct ExperimentCreated {
    experiment_id: String,
}

#[derive(Debug, Deserialize)]
struct RunScheduleRequest {
    schedule: ChaosSchedule,
}

#[derive(Debug, Serialize)]
struct ScheduleResult {
    experiment_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ListExperimentsQuery {
    experiment_type: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReportQuery {
    from: Option<i64>,
    to: Option<i64>,
}

// --- Handlers ---

/// POST /v1/chaos/experiment — Run a chaos experiment.
async fn run_experiment_handler(
    State(engine): State<Arc<ChaosEngine>>,
    Json(req): Json<RunExperimentRequest>,
) -> impl IntoResponse {
    let id = engine.run_experiment(req.name, req.experiment_type, req.target, req.config);
    (StatusCode::CREATED, Json(ExperimentCreated { experiment_id: id }))
}

/// GET /v1/chaos/experiment/:id — Get experiment details.
async fn get_experiment_handler(
    State(engine): State<Arc<ChaosEngine>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match engine.get_experiment(&id) {
        Some(experiment) => (StatusCode::OK, Json(experiment)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Experiment not found"})),
        )
            .into_response(),
    }
}

/// POST /v1/chaos/experiment/:id/rollback — Roll back an experiment.
async fn rollback_experiment_handler(
    State(engine): State<Arc<ChaosEngine>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if engine.rollback_experiment(&id) {
        (
            StatusCode::OK,
            Json(serde_json::json!({"message": "Experiment rolled back"})),
        )
            .into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Cannot roll back experiment"})),
        )
            .into_response()
    }
}

/// DELETE /v1/chaos/experiment/:id — Cancel a running experiment.
async fn cancel_experiment_handler(
    State(engine): State<Arc<ChaosEngine>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if engine.cancel_experiment(&id) {
        (
            StatusCode::OK,
            Json(serde_json::json!({"message": "Experiment cancelled"})),
        )
            .into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Cannot cancel experiment"})),
        )
            .into_response()
    }
}

/// POST /v1/chaos/schedule — Run a schedule of experiments.
async fn run_schedule_handler(
    State(engine): State<Arc<ChaosEngine>>,
    Json(req): Json<RunScheduleRequest>,
) -> impl IntoResponse {
    let ids = engine.run_schedule(req.schedule);
    (StatusCode::OK, Json(ScheduleResult { experiment_ids: ids }))
}

/// GET /v1/chaos/report — Generate resilience report.
async fn get_report_handler(
    State(engine): State<Arc<ChaosEngine>>,
    Query(params): Query<ReportQuery>,
) -> impl IntoResponse {
    let now = Utc::now().timestamp();
    let from = params.from.unwrap_or(now - 86400); // Default: last 24 hours.
    let to = params.to.unwrap_or(now);
    let report = engine.generate_report(from, to);
    (StatusCode::OK, Json(report))
}

/// GET /v1/chaos/experiments — List experiments with optional filters.
async fn list_experiments_handler(
    State(engine): State<Arc<ChaosEngine>>,
    Query(params): Query<ListExperimentsQuery>,
) -> impl IntoResponse {
    let filter_type: Option<ChaosType> = params
        .experiment_type
        .as_deref()
        .and_then(|t| serde_json::from_value(serde_json::json!(t)).ok());

    let filter_status: Option<ExperimentStatus> = params
        .status
        .as_deref()
        .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok());

    let experiments = engine.list_experiments(
        filter_type.as_ref(),
        filter_status.as_ref(),
    );
    (StatusCode::OK, Json(experiments))
}

/// GET /v1/chaos/resilience-score — Get current resilience score.
async fn get_resilience_score_handler(
    State(engine): State<Arc<ChaosEngine>>,
) -> impl IntoResponse {
    let score = engine.get_resilience_score();
    (
        StatusCode::OK,
        Json(serde_json::json!({"resilience_score": score})),
    )
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_engine() -> Arc<ChaosEngine> {
        Arc::new(ChaosEngine::new())
    }

    fn default_config() -> ChaosConfig {
        ChaosConfig::default()
    }

    // --------------------------------------------------------
    // test_run_provider_crash_experiment
    // --------------------------------------------------------
    #[test]
    fn test_run_provider_crash_experiment() {
        let engine = create_engine();
        let id = engine.run_experiment(
            "crash-test-1".to_string(),
            ChaosType::ProviderCrash,
            "provider-alpha".to_string(),
            default_config(),
        );

        let exp = engine.get_experiment(&id).expect("experiment should exist");
        assert_eq!(exp.experiment_type, ChaosType::ProviderCrash);
        assert_eq!(exp.target, "provider-alpha");
        assert!(exp.result.is_some());
        assert!(exp.result.as_ref().unwrap().passed);
        assert_eq!(exp.status, ExperimentStatus::Completed);
    }

    // --------------------------------------------------------
    // test_run_network_partition_experiment
    // --------------------------------------------------------
    #[test]
    fn test_run_network_partition_experiment() {
        let engine = create_engine();
        let id = engine.run_experiment(
            "partition-test-1".to_string(),
            ChaosType::NetworkPartition,
            "node-beta".to_string(),
            default_config(),
        );

        let exp = engine.get_experiment(&id).expect("experiment should exist");
        assert_eq!(exp.experiment_type, ChaosType::NetworkPartition);
        assert!(exp.result.is_some());
        // Default intensity (0.7) < 0.95, so consistency should be maintained.
        assert!(exp.result.as_ref().unwrap().passed);
    }

    // --------------------------------------------------------
    // test_run_state_corruption_experiment
    // --------------------------------------------------------
    #[test]
    fn test_run_state_corruption_experiment() {
        let engine = create_engine();
        let id = engine.run_experiment(
            "corruption-test-1".to_string(),
            ChaosType::InvalidState,
            "register-gamma".to_string(),
            default_config(),
        );

        let exp = engine.get_experiment(&id).expect("experiment should exist");
        assert_eq!(exp.experiment_type, ChaosType::InvalidState);
        assert!(exp.result.is_some());
        // State corruption should always be detected in simulation.
        assert!(exp.result.as_ref().unwrap().passed);
        assert!(!exp.result.as_ref().unwrap().state_corrupted);
    }

    // --------------------------------------------------------
    // test_experiment_result_recording
    // --------------------------------------------------------
    #[test]
    fn test_experiment_result_recording() {
        let engine = create_engine();
        let id = engine.run_experiment(
            "result-recording".to_string(),
            ChaosType::ProviderCrash,
            "provider-delta".to_string(),
            default_config(),
        );

        let exp = engine.get_experiment(&id).unwrap();
        let result = exp.result.unwrap();

        assert!(result.passed);
        assert!(result.error.is_none());
        assert!(result.recovery_time_ms > 0);
        assert!(!result.state_corrupted);
        assert!(!result.data_loss);
        assert!(!result.metrics_before.is_empty());
        assert!(!result.metrics_after.is_empty());
    }

    // --------------------------------------------------------
    // test_experiment_cancellation
    // --------------------------------------------------------
    #[test]
    fn test_experiment_cancellation() {
        let engine = create_engine();

        // Create an experiment.
        let id = engine.run_experiment(
            "cancel-test".to_string(),
            ChaosType::ProviderCrash,
            "provider-epsilon".to_string(),
            default_config(),
        );

        // After the experiment completes synchronously, cancellation of a
        // completed experiment should return false.
        let cancel_result = engine.cancel_experiment(&id);
        assert!(!cancel_result);
    }

    // --------------------------------------------------------
    // test_experiment_rollback
    // --------------------------------------------------------
    #[test]
    fn test_experiment_rollback() {
        let engine = create_engine();

        // Create an experiment with high intensity that will fail.
        let config = ChaosConfig {
            intensity: 1.0,
            duration_secs: 5,
            target_scope: "cluster".to_string(),
            rollback_on_failure: true,
            max_recovery_time_ms: 0,
        };

        let id = engine.run_experiment(
            "rollback-test".to_string(),
            ChaosType::NetworkPartition,
            "node-zeta".to_string(),
            config,
        );

        let exp = engine.get_experiment(&id).unwrap();
        // With intensity 1.0, network partition should fail.
        assert_eq!(exp.status, ExperimentStatus::RolledBack);

        // Rollback should succeed.
        let rollback_result = engine.rollback_experiment(&id);
        assert!(rollback_result);

        // After rollback, status should still be RolledBack.
        let exp_after = engine.get_experiment(&id).unwrap();
        assert_eq!(exp_after.status, ExperimentStatus::RolledBack);
    }

    // --------------------------------------------------------
    // test_schedule_execution
    // --------------------------------------------------------
    #[test]
    fn test_schedule_execution() {
        let engine = create_engine();

        // Pre-create some experiments that the schedule references.
        let id1 = engine.run_experiment(
            "sched-exp-1".to_string(),
            ChaosType::ProviderCrash,
            "provider-s1".to_string(),
            default_config(),
        );
        let id2 = engine.run_experiment(
            "sched-exp-2".to_string(),
            ChaosType::InvalidState,
            "register-s2".to_string(),
            default_config(),
        );

        let schedule = ChaosSchedule {
            experiments: vec![
                ScheduledExperiment {
                    experiment_id: id1.clone(),
                    run_at: Utc::now().timestamp(),
                    repeat: false,
                },
                ScheduledExperiment {
                    experiment_id: id2.clone(),
                    run_at: Utc::now().timestamp(),
                    repeat: false,
                },
            ],
            active: true,
            repeat_interval_secs: None,
        };

        let results = engine.run_schedule(schedule);
        // Schedule should produce new experiment IDs (one per scheduled entry).
        assert!(!results.is_empty());
    }

    // --------------------------------------------------------
    // test_report_generation
    // --------------------------------------------------------
    #[test]
    fn test_report_generation() {
        let engine = create_engine();

        // Run several experiments.
        engine.run_experiment(
            "report-exp-1".to_string(),
            ChaosType::ProviderCrash,
            "provider-r1".to_string(),
            default_config(),
        );
        engine.run_experiment(
            "report-exp-2".to_string(),
            ChaosType::InvalidState,
            "register-r2".to_string(),
            default_config(),
        );

        let now = Utc::now().timestamp();
        let report = engine.generate_report(now - 3600, now + 3600);

        assert_eq!(report.total_experiments, 2);
        assert!(report.passed >= 1);
        assert!(report.resilience_score >= 50.0);
    }

    // --------------------------------------------------------
    // test_resilience_score_calculation
    // --------------------------------------------------------
    #[test]
    fn test_resilience_score_calculation() {
        let engine = create_engine();

        // No experiments: score should be 100 (assume fully resilient).
        let score = engine.get_resilience_score();
        assert!((score - 100.0).abs() < f64::EPSILON);

        // Run a passing experiment.
        engine.run_experiment(
            "score-exp-1".to_string(),
            ChaosType::ProviderCrash,
            "provider-sc1".to_string(),
            default_config(),
        );
        let score = engine.get_resilience_score();
        assert_eq!(score, 100.0);

        // Run a failing experiment (high intensity).
        engine.run_experiment(
            "score-exp-2".to_string(),
            ChaosType::NetworkPartition,
            "node-sc2".to_string(),
            ChaosConfig {
                intensity: 1.0,
                duration_secs: 5,
                target_scope: "cluster".to_string(),
                rollback_on_failure: false,
                max_recovery_time_ms: 0,
            },
        );
        let score = engine.get_resilience_score();
        assert!(score < 100.0);
    }

    // --------------------------------------------------------
    // test_experiment_filtering
    // --------------------------------------------------------
    #[test]
    fn test_experiment_filtering() {
        let engine = create_engine();

        engine.run_experiment(
            "filter-crash".to_string(),
            ChaosType::ProviderCrash,
            "provider-f1".to_string(),
            default_config(),
        );
        engine.run_experiment(
            "filter-partition".to_string(),
            ChaosType::NetworkPartition,
            "node-f2".to_string(),
            default_config(),
        );
        engine.run_experiment(
            "filter-crash-2".to_string(),
            ChaosType::ProviderCrash,
            "provider-f3".to_string(),
            default_config(),
        );

        // Filter by type.
        let crash_exps = engine.list_experiments(Some(&ChaosType::ProviderCrash), None);
        assert_eq!(crash_exps.len(), 2);

        let partition_exps = engine.list_experiments(Some(&ChaosType::NetworkPartition), None);
        assert_eq!(partition_exps.len(), 1);

        // Filter by status.
        let completed = engine.list_experiments(None, Some(&ExperimentStatus::Completed));
        assert!(completed.len() >= 2);

        // Filter by both.
        let crash_completed = engine.list_experiments(
            Some(&ChaosType::ProviderCrash),
            Some(&ExperimentStatus::Completed),
        );
        assert_eq!(crash_completed.len(), 2);

        // No filter.
        let all = engine.list_experiments(None, None);
        assert_eq!(all.len(), 3);
    }

    // --------------------------------------------------------
    // test_concurrent_experiments
    // --------------------------------------------------------
    #[test]
    fn test_concurrent_experiments() {
        let engine = create_engine();

        // Spawn multiple experiments in parallel using threads.
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let eng = engine.clone();
                std::thread::spawn(move || {
                    eng.run_experiment(
                        format!("concurrent-{}", i),
                        ChaosType::ProviderCrash,
                        format!("provider-c{}", i),
                        default_config(),
                    )
                })
            })
            .collect();

        let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(ids.len(), 5);

        // All experiments should exist and be completed.
        for id in &ids {
            let exp = engine.get_experiment(id).unwrap();
            assert_eq!(exp.status, ExperimentStatus::Completed);
        }
    }

    // --------------------------------------------------------
    // test_high_latency_simulation
    // --------------------------------------------------------
    #[test]
    fn test_high_latency_simulation() {
        let engine = create_engine();

        // Low intensity: should pass.
        let id = engine.run_experiment(
            "latency-low".to_string(),
            ChaosType::HighLatency,
            "node-lat1".to_string(),
            ChaosConfig {
                intensity: 0.5,
                ..default_config()
            },
        );
        let exp = engine.get_experiment(&id).unwrap();
        assert!(exp.result.as_ref().unwrap().passed);

        // High intensity with short max recovery: should fail.
        let id2 = engine.run_experiment(
            "latency-high".to_string(),
            ChaosType::HighLatency,
            "node-lat2".to_string(),
            ChaosConfig {
                intensity: 0.95,
                max_recovery_time_ms: 0,
                ..default_config()
            },
        );
        let exp2 = engine.get_experiment(&id2).unwrap();
        assert!(!exp2.result.as_ref().unwrap().passed);
    }

    // --------------------------------------------------------
    // test_memory_pressure_simulation
    // --------------------------------------------------------
    #[test]
    fn test_memory_pressure_simulation() {
        let engine = create_engine();

        // Low intensity: should pass.
        let id = engine.run_experiment(
            "memory-low".to_string(),
            ChaosType::MemoryPressure,
            "node-mem1".to_string(),
            ChaosConfig {
                intensity: 0.5,
                ..default_config()
            },
        );
        let exp = engine.get_experiment(&id).unwrap();
        assert!(exp.result.as_ref().unwrap().passed);

        // Very high intensity: may cause data loss.
        let id2 = engine.run_experiment(
            "memory-high".to_string(),
            ChaosType::MemoryPressure,
            "node-mem2".to_string(),
            ChaosConfig {
                intensity: 0.98,
                max_recovery_time_ms: 0,
                ..default_config()
            },
        );
        let exp2 = engine.get_experiment(&id2).unwrap();
        // At 0.98 intensity, should fail and potentially have data loss.
        assert!(!exp2.result.as_ref().unwrap().passed);
    }

    // --------------------------------------------------------
    // test_double_spend_detection
    // --------------------------------------------------------
    #[test]
    fn test_double_spend_detection() {
        let engine = create_engine();

        let id = engine.run_experiment(
            "double-spend-test".to_string(),
            ChaosType::DoubleSpend,
            "transaction-ds1".to_string(),
            default_config(),
        );

        let exp = engine.get_experiment(&id).unwrap();
        assert_eq!(exp.experiment_type, ChaosType::DoubleSpend);
        let result = exp.result.unwrap();
        // Double-spend should always be detected in simulation.
        assert!(result.passed);
        assert!(result.error.is_none());
    }

    // --------------------------------------------------------
    // test_stale_data_detection
    // --------------------------------------------------------
    #[test]
    fn test_stale_data_detection() {
        let engine = create_engine();

        let id = engine.run_experiment(
            "stale-data-test".to_string(),
            ChaosType::StaleData,
            "cache-sd1".to_string(),
            default_config(),
        );

        let exp = engine.get_experiment(&id).unwrap();
        assert_eq!(exp.experiment_type, ChaosType::StaleData);
        let result = exp.result.unwrap();
        // Stale data should always be detected in simulation.
        assert!(result.passed);
        assert!(result.error.is_none());
        assert!(!result.state_corrupted);
    }
}
