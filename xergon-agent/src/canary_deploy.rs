use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CanaryStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum CanaryStatus {
    Pending,
    Deploying,
    Monitoring,
    Stable,
    RollingBack,
    RolledBack,
    Failed,
}

// ---------------------------------------------------------------------------
// CanaryConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CanaryConfig {
    pub model_id: String,
    pub canary_weight: f64,
    pub max_weight: f64,
    pub step_weight: f64,
    pub step_interval_secs: u64,
    pub rollback_threshold: f64,
    pub metrics_window_secs: u64,
    pub auto_rollback: bool,
}

impl Default for CanaryConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            canary_weight: 0.05,
            max_weight: 1.0,
            step_weight: 0.10,
            step_interval_secs: 300,
            rollback_threshold: 0.10,
            metrics_window_secs: 600,
            auto_rollback: true,
        }
    }
}

// ---------------------------------------------------------------------------
// CanaryMetrics
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CanaryMetrics {
    pub error_rate: f64,
    pub latency_p50: f64,
    pub latency_p99: f64,
    pub tokens_per_sec: f64,
    pub cpu_util: f64,
    pub memory_util: f64,
}

// ---------------------------------------------------------------------------
// CanaryObservation
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CanaryObservation {
    pub timestamp: DateTime<Utc>,
    pub weight: f64,
    pub metrics: CanaryMetrics,
}

// ---------------------------------------------------------------------------
// CanaryDeploymentSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CanaryDeploymentSnapshot {
    pub id: String,
    pub model_id: String,
    pub status: CanaryStatus,
    pub current_weight: f64,
    pub baseline_metrics: CanaryMetrics,
    pub observations: Vec<CanaryObservation>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub rollback_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// CanaryDeployment
// ---------------------------------------------------------------------------

pub struct CanaryDeployment {
    pub id: String,
    pub config: CanaryConfig,
    status: Arc<std::sync::RwLock<CanaryStatus>>,
    current_weight: Arc<std::sync::RwLock<f64>>,
    pub baseline_metrics: CanaryMetrics,
    observations: Arc<std::sync::RwLock<Vec<CanaryObservation>>>,
    pub started_at: DateTime<Utc>,
    completed_at: Arc<std::sync::RwLock<Option<DateTime<Utc>>>>,
    rollback_reason: Arc<std::sync::Mutex<Option<String>>>,
}

impl CanaryDeployment {
    pub fn new(config: CanaryConfig, baseline_metrics: CanaryMetrics) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config,
            status: Arc::new(std::sync::RwLock::new(CanaryStatus::Pending)),
            current_weight: Arc::new(std::sync::RwLock::new(0.0)),
            baseline_metrics,
            observations: Arc::new(std::sync::RwLock::new(Vec::new())),
            started_at: Utc::now(),
            completed_at: Arc::new(std::sync::RwLock::new(None)),
            rollback_reason: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    // -- accessors ----------------------------------------------------------

    pub fn get_status(&self) -> CanaryStatus {
        let guard = self.status.read().unwrap();
        guard.clone()
    }

    pub fn set_status(&self, status: CanaryStatus) {
        let mut guard = self.status.write().unwrap();
        *guard = status;
    }

    pub fn get_current_weight(&self) -> f64 {
        let guard = self.current_weight.read().unwrap();
        *guard
    }

    pub fn set_current_weight(&self, weight: f64) {
        let mut guard = self.current_weight.write().unwrap();
        *guard = weight;
    }

    pub fn get_observations(&self) -> Vec<CanaryObservation> {
        let guard = self.observations.read().unwrap();
        guard.clone()
    }

    pub fn add_observation(&self, observation: CanaryObservation) {
        let mut guard = self.observations.write().unwrap();
        guard.push(observation);
    }

    pub fn get_completed_at(&self) -> Option<DateTime<Utc>> {
        let guard = self.completed_at.read().unwrap();
        *guard
    }

    pub fn set_completed_at(&self, ts: Option<DateTime<Utc>>) {
        let mut guard = self.completed_at.write().unwrap();
        *guard = ts;
    }

    pub fn get_rollback_reason(&self) -> Option<String> {
        let guard = self.rollback_reason.lock().unwrap();
        guard.clone()
    }

    pub fn set_rollback_reason(&self, reason: Option<String>) {
        let mut guard = self.rollback_reason.lock().unwrap();
        *guard = reason;
    }

    // -- snapshot -----------------------------------------------------------

    pub fn snapshot(&self) -> CanaryDeploymentSnapshot {
        CanaryDeploymentSnapshot {
            id: self.id.clone(),
            model_id: self.config.model_id.clone(),
            status: self.get_status(),
            current_weight: self.get_current_weight(),
            baseline_metrics: self.baseline_metrics.clone(),
            observations: self.get_observations(),
            started_at: self.started_at,
            completed_at: self.get_completed_at(),
            rollback_reason: self.get_rollback_reason(),
        }
    }
}

// ---------------------------------------------------------------------------
// CanaryMetricsSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CanaryMetricsSnapshot {
    pub total_deployments: u64,
    pub active_deployments: u64,
    pub successful: u64,
    pub rolled_back: u64,
    pub avg_time_to_stable_secs: f64,
}

// ---------------------------------------------------------------------------
// CanaryDeployer
// ---------------------------------------------------------------------------

pub struct CanaryDeployer {
    deployments: DashMap<String, Arc<CanaryDeployment>>,
    total_deployments: AtomicU64,
    total_successful: AtomicU64,
    total_rolled_back: AtomicU64,
}

impl CanaryDeployer {
    pub fn new() -> Self {
        Self {
            deployments: DashMap::new(),
            total_deployments: AtomicU64::new(0),
            total_successful: AtomicU64::new(0),
            total_rolled_back: AtomicU64::new(0),
        }
    }

    /// Create a new canary deployment in Pending state with initial canary weight.
    pub fn create(&self, config: CanaryConfig) -> Result<CanaryDeploymentSnapshot, String> {
        if config.model_id.is_empty() {
            return Err("model_id must not be empty".into());
        }
        if config.canary_weight <= 0.0 || config.canary_weight > 1.0 {
            return Err("canary_weight must be in (0, 1]".into());
        }
        if config.max_weight <= 0.0 || config.max_weight > 1.0 {
            return Err("max_weight must be in (0, 1]".into());
        }
        if config.step_weight <= 0.0 {
            return Err("step_weight must be > 0".into());
        }
        if config.canary_weight > config.max_weight {
            return Err("canary_weight must not exceed max_weight".into());
        }

        let baseline = CanaryMetrics::default();
        let deployment = Arc::new(CanaryDeployment::new(config.clone(), baseline));
        deployment.set_status(CanaryStatus::Deploying);
        deployment.set_current_weight(config.canary_weight);
        let id = deployment.id.clone();
        let snapshot = deployment.snapshot();
        self.deployments.insert(id, deployment);
        self.total_deployments.fetch_add(1, Ordering::SeqCst);
        Ok(snapshot)
    }

    /// Retrieve a deployment snapshot by id.
    pub fn get(&self, id: &str) -> Option<CanaryDeploymentSnapshot> {
        self.deployments.get(id).map(|d| d.snapshot())
    }

    /// Advance the canary weight by step_weight, up to max_weight.
    /// Only deployments in Monitoring state can be advanced.
    pub fn advance(&self, id: &str) -> Result<CanaryDeploymentSnapshot, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment {} not found", id))?;

        let status = entry.get_status();
        if status != CanaryStatus::Monitoring {
            return Err(format!(
                "cannot advance deployment in {:?} state, expected Monitoring",
                status
            ));
        }

        entry.set_status(CanaryStatus::Deploying);
        let current = entry.get_current_weight();
        let step = entry.config.step_weight;
        let max = entry.config.max_weight;
        let new_weight = (current + step).min(max);
        entry.set_current_weight(new_weight);
        entry.set_status(CanaryStatus::Monitoring);

        Ok(entry.snapshot())
    }

    /// Record metrics for a deployment, adding an observation.
    /// If auto_rollback is enabled, evaluates whether a rollback is needed.
    pub fn record_metrics(
        &self,
        id: &str,
        metrics: CanaryMetrics,
    ) -> Result<CanaryObservation, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment {} not found", id))?;

        let status = entry.get_status();
        if status != CanaryStatus::Monitoring && status != CanaryStatus::Deploying {
            return Err(format!("cannot record metrics for deployment in {:?} state", status));
        }

        let weight = entry.get_current_weight();
        let observation = CanaryObservation {
            timestamp: Utc::now(),
            weight,
            metrics: metrics.clone(),
        };

        entry.add_observation(observation.clone());

        // Auto-rollback check
        if entry.config.auto_rollback {
            if let Some(reason) = Self::evaluate_rollback(&entry) {
                let _ = self.rollback(id, &reason);
            }
        }

        Ok(observation)
    }

    /// Roll back a deployment. Sets status to RollingBack, records reason,
    /// then moves to RolledBack.
    pub fn rollback(&self, id: &str, reason: &str) -> Result<CanaryDeploymentSnapshot, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment {} not found", id))?;

        let status = entry.get_status();
        match status {
            CanaryStatus::Stable
            | CanaryStatus::RolledBack
            | CanaryStatus::RollingBack
            | CanaryStatus::Failed => {
                return Err(format!("cannot rollback deployment in {:?} state", status));
            }
            _ => {}
        }

        entry.set_status(CanaryStatus::RollingBack);
        entry.set_rollback_reason(Some(reason.to_string()));
        entry.set_completed_at(Some(Utc::now()));
        entry.set_current_weight(0.0);
        entry.set_status(CanaryStatus::RolledBack);

        self.total_rolled_back.fetch_add(1, Ordering::SeqCst);

        Ok(entry.snapshot())
    }

    /// Mark a deployment as Stable if current_weight >= max_weight.
    pub fn mark_stable(&self, id: &str) -> Result<CanaryDeploymentSnapshot, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment {} not found", id))?;

        let status = entry.get_status();
        if status != CanaryStatus::Monitoring {
            return Err(format!(
                "cannot mark deployment in {:?} state as stable, expected Monitoring",
                status
            ));
        }

        let current = entry.get_current_weight();
        let max = entry.config.max_weight;
        if current < max {
            return Err(format!(
                "current weight {} is less than max weight {}; advance first",
                current, max
            ));
        }

        entry.set_status(CanaryStatus::Stable);
        entry.set_completed_at(Some(Utc::now()));

        self.total_successful.fetch_add(1, Ordering::SeqCst);

        Ok(entry.snapshot())
    }

    /// Get the status of a deployment by id.
    pub fn get_status(&self, id: &str) -> Option<CanaryStatus> {
        self.deployments.get(id).map(|d| d.get_status())
    }

    /// List all deployments as snapshots.
    pub fn list_deployments(&self) -> Vec<CanaryDeploymentSnapshot> {
        self.deployments.iter().map(|entry| entry.value().snapshot()).collect()
    }

    /// List deployments filtered by status.
    pub fn list_by_status(&self, status: &CanaryStatus) -> Vec<CanaryDeploymentSnapshot> {
        self.deployments
            .iter()
            .filter(|entry| &entry.value().get_status() == status)
            .map(|entry| entry.value().snapshot())
            .collect()
    }

    /// Check health of a deployment: returns true if current metrics are within
    /// the rollback threshold of baseline.
    pub fn check_health(&self, id: &str) -> Result<bool, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment {} not found", id))?;

        let observations = entry.get_observations();
        if observations.is_empty() {
            return Ok(true);
        }

        let latest = observations.last().unwrap();
        let baseline = &entry.baseline_metrics;
        let threshold = entry.config.rollback_threshold;

        let error_ok = latest.metrics.error_rate
            <= baseline.error_rate * (1.0 + threshold);
        let latency_ok = latest.metrics.latency_p99 <= baseline.latency_p99 * 1.5;

        Ok(error_ok && latency_ok)
    }

    /// Aggregate metrics across all deployments.
    pub fn get_metrics(&self) -> CanaryMetricsSnapshot {
        let total = self.total_deployments.load(Ordering::SeqCst);
        let successful = self.total_successful.load(Ordering::SeqCst);
        let rolled_back = self.total_rolled_back.load(Ordering::SeqCst);

        let active = self
            .deployments
            .iter()
            .filter(|e| {
                let s = e.value().get_status();
                s == CanaryStatus::Deploying || s == CanaryStatus::Monitoring
            })
            .count() as u64;

        // Average time to stable for completed deployments
        let stable_times: Vec<f64> = self
            .deployments
            .iter()
            .filter_map(|e| {
                let d = e.value();
                if d.get_status() == CanaryStatus::Stable {
                    d.get_completed_at().map(|ca| {
                        (ca - d.started_at).num_seconds() as f64
                    })
                } else {
                    None
                }
            })
            .collect();

        let avg_time = if stable_times.is_empty() {
            0.0
        } else {
            stable_times.iter().sum::<f64>() / stable_times.len() as f64
        };

        CanaryMetricsSnapshot {
            total_deployments: total,
            active_deployments: active,
            successful,
            rolled_back,
            avg_time_to_stable_secs: avg_time,
        }
    }

    /// Evaluate whether the latest observation triggers a rollback.
    /// Returns Some(reason) if error_rate exceeds baseline * (1 + threshold)
    /// or latency_p99 exceeds baseline * 1.5.
    pub fn evaluate_rollback(deployment: &CanaryDeployment) -> Option<String> {
        let observations = deployment.get_observations();
        if observations.is_empty() {
            return None;
        }

        let latest = observations.last().unwrap();
        let baseline = &deployment.baseline_metrics;
        let threshold = deployment.config.rollback_threshold;

        let baseline_err = baseline.error_rate * (1.0 + threshold);
        if latest.metrics.error_rate > baseline_err {
            return Some(format!(
                "Error rate exceeded threshold: {:.4}% vs baseline {:.4}%",
                latest.metrics.error_rate * 100.0,
                baseline.error_rate * 100.0,
            ));
        }

        let baseline_p99 = baseline.latency_p99 * 1.5;
        if latest.metrics.latency_p99 > baseline_p99 {
            return Some(format!(
                "P99 latency exceeded threshold: {:.2}ms vs baseline {:.2}ms",
                latest.metrics.latency_p99, baseline.latency_p99,
            ));
        }

        None
    }
}

impl Default for CanaryDeployer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CanaryConfig {
        CanaryConfig {
            model_id: "test-model-v2".to_string(),
            canary_weight: 0.05,
            max_weight: 1.0,
            step_weight: 0.10,
            step_interval_secs: 300,
            rollback_threshold: 0.10,
            metrics_window_secs: 600,
            auto_rollback: true,
        }
    }

    fn good_metrics() -> CanaryMetrics {
        CanaryMetrics {
            error_rate: 0.005,
            latency_p50: 20.0,
            latency_p99: 50.0,
            tokens_per_sec: 100.0,
            cpu_util: 30.0,
            memory_util: 40.0,
        }
    }

    fn bad_metrics() -> CanaryMetrics {
        CanaryMetrics {
            error_rate: 0.25,
            latency_p50: 500.0,
            latency_p99: 1200.0,
            tokens_per_sec: 5.0,
            cpu_util: 95.0,
            memory_util: 90.0,
        }
    }

    // -- 1. CanaryConfig default ------------------------------------------------
    #[test]
    fn test_canary_config_default() {
        let config = CanaryConfig::default();
        assert_eq!(config.model_id, "");
        assert!((config.canary_weight - 0.05).abs() < 1e-9);
        assert!((config.max_weight - 1.0).abs() < 1e-9);
        assert!((config.step_weight - 0.10).abs() < 1e-9);
        assert_eq!(config.step_interval_secs, 300);
        assert!((config.rollback_threshold - 0.10).abs() < 1e-9);
        assert_eq!(config.metrics_window_secs, 600);
        assert!(config.auto_rollback);
    }

    // -- 2. CanaryMetrics default -----------------------------------------------
    #[test]
    fn test_canary_metrics_default() {
        let m = CanaryMetrics::default();
        assert!((m.error_rate - 0.0).abs() < 1e-9);
        assert!((m.latency_p50 - 0.0).abs() < 1e-9);
        assert!((m.latency_p99 - 0.0).abs() < 1e-9);
        assert!((m.tokens_per_sec - 0.0).abs() < 1e-9);
        assert!((m.cpu_util - 0.0).abs() < 1e-9);
        assert!((m.memory_util - 0.0).abs() < 1e-9);
    }

    // -- 3. CanaryStatus equality -----------------------------------------------
    #[test]
    fn test_canary_status_equality() {
        assert_eq!(CanaryStatus::Pending, CanaryStatus::Pending);
        assert_ne!(CanaryStatus::Pending, CanaryStatus::Monitoring);
    }

    // -- 4. Create deployment succeeds ------------------------------------------
    #[test]
    fn test_create_deployment_success() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        assert_eq!(snapshot.model_id, "test-model-v2");
        assert_eq!(snapshot.status, CanaryStatus::Deploying);
        assert!((snapshot.current_weight - 0.05).abs() < 1e-9);
        assert!(!snapshot.id.is_empty());
    }

    // -- 5. Create deployment with empty model_id fails -------------------------
    #[test]
    fn test_create_deployment_empty_model_id() {
        let deployer = CanaryDeployer::new();
        let mut config = test_config();
        config.model_id = String::new();
        let result = deployer.create(config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("model_id"));
    }

    // -- 6. Create deployment with invalid canary_weight fails ------------------
    #[test]
    fn test_create_deployment_invalid_weight() {
        let deployer = CanaryDeployer::new();
        let mut config = test_config();
        config.canary_weight = 0.0;
        let result = deployer.create(config);
        assert!(result.is_err());
    }

    // -- 7. Get deployment by id ------------------------------------------------
    #[test]
    fn test_get_deployment() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let created = deployer.create(config).unwrap();
        let fetched = deployer.get(&created.id).unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.model_id, created.model_id);
    }

    // -- 8. Get non-existent deployment -----------------------------------------
    #[test]
    fn test_get_nonexistent_deployment() {
        let deployer = CanaryDeployer::new();
        assert!(deployer.get("no-such-id").is_none());
    }

    // -- 9. Advance deployment weight -------------------------------------------
    #[test]
    fn test_advance_deployment() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        // Move to Monitoring
        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        let advanced = deployer.advance(&id).unwrap();
        assert!((advanced.current_weight - 0.15).abs() < 1e-9);
        assert_eq!(advanced.status, CanaryStatus::Monitoring);
    }

    // -- 10. Advance caps at max_weight ----------------------------------------
    #[test]
    fn test_advance_caps_at_max_weight() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
            entry.set_current_weight(0.95);
        }

        let advanced = deployer.advance(&id).unwrap();
        assert!((advanced.current_weight - 1.0).abs() < 1e-9);
    }

    // -- 11. Advance non-monitoring deployment fails ----------------------------
    #[test]
    fn test_advance_non_monitoring_fails() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let result = deployer.advance(&snapshot.id);
        assert!(result.is_err());
    }

    // -- 12. Record good metrics -----------------------------------------------
    #[test]
    fn test_record_metrics_good() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        let obs = deployer.record_metrics(&id, good_metrics()).unwrap();
        assert!(!obs.timestamp.to_string().is_empty());
        assert_eq!(obs.metrics.error_rate, 0.005);

        let fetched = deployer.get(&id).unwrap();
        assert_eq!(fetched.observations.len(), 1);
        // Should still be Monitoring because metrics are good
        assert_eq!(fetched.status, CanaryStatus::Monitoring);
    }

    // -- 13. Record bad metrics triggers auto-rollback --------------------------
    #[test]
    fn test_record_metrics_bad_triggers_rollback() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        // Record bad metrics -- baseline error_rate is 0.0, so any error triggers rollback
        let _obs = deployer.record_metrics(&id, bad_metrics()).unwrap();

        let fetched = deployer.get(&id).unwrap();
        assert_eq!(fetched.status, CanaryStatus::RolledBack);
        assert!(fetched.rollback_reason.is_some());
    }

    // -- 14. Manual rollback ---------------------------------------------------
    #[test]
    fn test_manual_rollback() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        let result = deployer.rollback(&id, "manual rollback requested").unwrap();
        assert_eq!(result.status, CanaryStatus::RolledBack);
        assert_eq!(result.rollback_reason, Some("manual rollback requested".to_string()));
        assert!((result.current_weight - 0.0).abs() < 1e-9);
        assert!(result.completed_at.is_some());
    }

    // -- 15. Rollback of already rolled back deployment fails -------------------
    #[test]
    fn test_rollback_already_rolled_back_fails() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        let _ = deployer.rollback(&id, "first rollback").unwrap();
        let result = deployer.rollback(&id, "second rollback");
        assert!(result.is_err());
    }

    // -- 16. Mark stable succeeds at max weight --------------------------------
    #[test]
    fn test_mark_stable_success() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
            entry.set_current_weight(1.0);
        }

        let result = deployer.mark_stable(&id).unwrap();
        assert_eq!(result.status, CanaryStatus::Stable);
        assert!(result.completed_at.is_some());
    }

    // -- 17. Mark stable fails when weight too low -----------------------------
    #[test]
    fn test_mark_stable_weight_too_low() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
            entry.set_current_weight(0.5);
        }

        let result = deployer.mark_stable(&id);
        assert!(result.is_err());
    }

    // -- 18. get_status --------------------------------------------------------
    #[test]
    fn test_get_status() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let status = deployer.get_status(&snapshot.id).unwrap();
        assert_eq!(status, CanaryStatus::Deploying);
    }

    // -- 19. list_deployments --------------------------------------------------
    #[test]
    fn test_list_deployments() {
        let deployer = CanaryDeployer::new();
        assert!(deployer.list_deployments().is_empty());

        let config = test_config();
        deployer.create(config).unwrap();
        assert_eq!(deployer.list_deployments().len(), 1);
    }

    // -- 20. list_by_status ----------------------------------------------------
    #[test]
    fn test_list_by_status() {
        let deployer = CanaryDeployer::new();
        let config1 = test_config();
        let snap1 = deployer.create(config1).unwrap();

        let mut config2 = test_config();
        config2.model_id = "other-model".to_string();
        let snap2 = deployer.create(config2).unwrap();

        {
            let entry = deployer.deployments.get(&snap2.id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        let deploying = deployer.list_by_status(&CanaryStatus::Deploying);
        assert_eq!(deploying.len(), 1);
        assert_eq!(deploying[0].id, snap1.id);

        let monitoring = deployer.list_by_status(&CanaryStatus::Monitoring);
        assert_eq!(monitoring.len(), 1);
        assert_eq!(monitoring[0].id, snap2.id);
    }

    // -- 21. check_health healthy -----------------------------------------------
    #[test]
    fn test_check_health_healthy() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        // With default baseline (all zeros), any error_rate > 0 is unhealthy
        // but check_health returns true if no observations yet
        assert!(deployer.check_health(&id).unwrap());
    }

    // -- 22. check_health unhealthy with bad metrics ---------------------------
    #[test]
    fn test_check_health_unhealthy() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
            // Record observation with high error rate
            let obs = CanaryObservation {
                timestamp: Utc::now(),
                weight: 0.05,
                metrics: bad_metrics(),
            };
            entry.add_observation(obs);
        }

        // Baseline is all zeros, so 0.25 > 0 * (1 + 0.10) = 0 => unhealthy
        assert!(!deployer.check_health(&id).unwrap());
    }

    // -- 23. get_metrics aggregate ---------------------------------------------
    #[test]
    fn test_get_metrics() {
        let deployer = CanaryDeployer::new();
        let metrics = deployer.get_metrics();
        assert_eq!(metrics.total_deployments, 0);
        assert_eq!(metrics.active_deployments, 0);
        assert_eq!(metrics.successful, 0);
        assert_eq!(metrics.rolled_back, 0);

        let config = test_config();
        let snap = deployer.create(config).unwrap();
        {
            let entry = deployer.deployments.get(&snap.id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        let metrics = deployer.get_metrics();
        assert_eq!(metrics.total_deployments, 1);
        assert_eq!(metrics.active_deployments, 1);
    }

    // -- 24. evaluate_rollback with good metrics returns None -------------------
    #[test]
    fn test_evaluate_rollback_good_metrics() {
        let config = test_config();
        let deployment = CanaryDeployment::new(config, CanaryMetrics::default());
        deployment.set_status(CanaryStatus::Monitoring);

        // With baseline at 0.0 and threshold 0.10, error_rate 0.0 is fine
        let obs = CanaryObservation {
            timestamp: Utc::now(),
            weight: 0.05,
            metrics: CanaryMetrics {
                error_rate: 0.0,
                latency_p99: 0.0,
                ..Default::default()
            },
        };
        deployment.add_observation(obs);

        assert!(CanaryDeployer::evaluate_rollback(&deployment).is_none());
    }

    // -- 25. evaluate_rollback with bad error rate ------------------------------
    #[test]
    fn test_evaluate_rollback_bad_error_rate() {
        let config = test_config();
        let baseline = CanaryMetrics {
            error_rate: 0.01,
            latency_p99: 100.0,
            ..Default::default()
        };
        let deployment = CanaryDeployment::new(config, baseline);
        deployment.set_status(CanaryStatus::Monitoring);

        // error_rate 0.20 > 0.01 * 1.10 = 0.011
        let obs = CanaryObservation {
            timestamp: Utc::now(),
            weight: 0.05,
            metrics: CanaryMetrics {
                error_rate: 0.20,
                latency_p99: 50.0,
                ..Default::default()
            },
        };
        deployment.add_observation(obs);

        let reason = CanaryDeployer::evaluate_rollback(&deployment);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("Error rate exceeded threshold"));
    }

    // -- 26. evaluate_rollback with bad latency --------------------------------
    #[test]
    fn test_evaluate_rollback_bad_latency() {
        let config = test_config();
        let baseline = CanaryMetrics {
            error_rate: 0.001,
            latency_p99: 100.0,
            ..Default::default()
        };
        let deployment = CanaryDeployment::new(config, baseline);
        deployment.set_status(CanaryStatus::Monitoring);

        // error_rate 0.001 <= 0.001 * 1.10 = 0.0011, but latency 200 > 100 * 1.5 = 150
        let obs = CanaryObservation {
            timestamp: Utc::now(),
            weight: 0.05,
            metrics: CanaryMetrics {
                error_rate: 0.001,
                latency_p99: 200.0,
                ..Default::default()
            },
        };
        deployment.add_observation(obs);

        let reason = CanaryDeployer::evaluate_rollback(&deployment);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("P99 latency exceeded threshold"));
    }

    // -- 27. snapshot captures current state ------------------------------------
    #[test]
    fn test_snapshot_captures_state() {
        let config = test_config();
        let baseline = CanaryMetrics {
            error_rate: 0.02,
            ..Default::default()
        };
        let deployment = CanaryDeployment::new(config, baseline);

        deployment.set_status(CanaryStatus::Monitoring);
        deployment.set_current_weight(0.50);
        deployment.set_completed_at(Some(Utc::now()));
        deployment.set_rollback_reason(Some("test".to_string()));

        let snap = deployment.snapshot();
        assert_eq!(snap.status, CanaryStatus::Monitoring);
        assert!((snap.current_weight - 0.50).abs() < 1e-9);
        assert!((snap.baseline_metrics.error_rate - 0.02).abs() < 1e-9);
        assert!(snap.completed_at.is_some());
        assert_eq!(snap.rollback_reason, Some("test".to_string()));
    }

    // -- 28. CanaryDeployer default --------------------------------------------
    #[test]
    fn test_canary_deployer_default() {
        let deployer = CanaryDeployer::default();
        assert_eq!(deployer.total_deployments.load(Ordering::SeqCst), 0);
    }

    // -- 29. Create with canary_weight > max_weight fails ----------------------
    #[test]
    fn test_create_weight_exceeds_max() {
        let deployer = CanaryDeployer::new();
        let mut config = test_config();
        config.canary_weight = 0.8;
        config.max_weight = 0.5;
        let result = deployer.create(config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("canary_weight"));
    }

    // -- 30. Record metrics in Pending state fails -----------------------------
    #[test]
    fn test_record_metrics_pending_fails() {
        let deployer = CanaryDeployer::new();
        let mut config = test_config();
        config.auto_rollback = false;
        let snapshot = deployer.create(config).unwrap();

        // The deployment starts in Deploying, but we can't record in Pending
        {
            let entry = deployer.deployments.get(&snapshot.id).unwrap();
            entry.set_status(CanaryStatus::Pending);
        }

        let result = deployer.record_metrics(&snapshot.id, good_metrics());
        assert!(result.is_err());
    }

    // -- 31. Multiple advances reach max --------------------------------------
    #[test]
    fn test_multiple_advances_reach_max() {
        let deployer = CanaryDeployer::new();
        let config = test_config();
        let snapshot = deployer.create(config).unwrap();
        let id = snapshot.id.clone();

        {
            let entry = deployer.deployments.get(&id).unwrap();
            entry.set_status(CanaryStatus::Monitoring);
        }

        // Start at 0.05, step 0.10 each time
        // 0.05 -> 0.15 -> 0.25 -> ... -> 0.95 -> 1.0
        for i in 0..10 {
            let result = deployer.advance(&id).unwrap();
            let expected = ((0.05 + 0.10 * (i as f64 + 1.0)) as f64).min(1.0);
            assert!((result.current_weight - expected).abs() < 1e-9);
        }

        // Should be at max
        let final_snap = deployer.get(&id).unwrap();
        assert!((final_snap.current_weight - 1.0).abs() < 1e-9);
    }

    // -- 32. Deployment id is unique ------------------------------------------
    #[test]
    fn test_deployment_ids_unique() {
        let deployer = CanaryDeployer::new();
        let config1 = test_config();
        let config2 = test_config();
        let snap1 = deployer.create(config1).unwrap();
        let snap2 = deployer.create(config2).unwrap();
        assert_ne!(snap1.id, snap2.id);
    }
}
