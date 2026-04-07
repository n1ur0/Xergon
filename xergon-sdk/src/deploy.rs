use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

// ---------------------------------------------------------------------------
// DeployConfig
// ---------------------------------------------------------------------------

/// Configuration for a model deployment.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeployConfig {
    /// Unique model identifier.
    pub model_id: String,
    /// Human-readable model name.
    pub model_name: String,
    /// Deployment version string.
    pub version: String,
    /// Source URL to pull the model from (e.g. Ollama registry).
    pub source_url: String,
    /// Inference backend (default: "ollama").
    pub backend: String,
    /// Number of GPU layers; 0 means auto-detect.
    pub gpu_layers: u32,
    /// Context window size in tokens.
    pub context_size: u32,
    /// Batch size for inference.
    pub batch_size: u32,
    /// Maximum tokens per generation.
    pub max_tokens: u32,
    /// Sampling temperature.
    pub temperature: f64,
    /// Repetition penalty.
    pub repeat_penalty: f64,
    /// Optional seed for deterministic sampling.
    pub seed: Option<u64>,
    /// Extra environment variables passed to the runtime.
    pub env_vars: HashMap<String, String>,
    /// Arbitrary labels for organisation / filtering.
    pub labels: HashMap<String, String>,
}

impl Default for DeployConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            model_name: String::new(),
            version: String::from("0.1.0"),
            source_url: String::new(),
            backend: String::from("ollama"),
            gpu_layers: 0,
            context_size: 4096,
            batch_size: 32,
            max_tokens: 2048,
            temperature: 0.7,
            repeat_penalty: 1.1,
            seed: None,
            env_vars: HashMap::new(),
            labels: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// DeployStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a deployment.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum DeployStatus {
    Pending,
    Pulling,
    Loading,
    Running,
    Failed,
    Stopped,
    RollingBack,
}

impl std::fmt::Display for DeployStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployStatus::Pending => write!(f, "Pending"),
            DeployStatus::Pulling => write!(f, "Pulling"),
            DeployStatus::Loading => write!(f, "Loading"),
            DeployStatus::Running => write!(f, "Running"),
            DeployStatus::Failed => write!(f, "Failed"),
            DeployStatus::Stopped => write!(f, "Stopped"),
            DeployStatus::RollingBack => write!(f, "RollingBack"),
        }
    }
}

// ---------------------------------------------------------------------------
// DeployTarget
// ---------------------------------------------------------------------------

/// Hardware / infrastructure target for a deployment.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeployTarget {
    /// Human-readable target name.
    pub name: String,
    /// API endpoint of the target.
    pub endpoint: String,
    /// GPU model type (e.g. "a100").
    pub gpu_type: String,
    /// Number of GPUs available.
    pub gpu_count: u32,
    /// Region where the target resides.
    pub region: String,
    /// Available VRAM in GB.
    pub available_vram_gb: f64,
}

// ---------------------------------------------------------------------------
// HealthCheck
// ---------------------------------------------------------------------------

/// Result of a single health check probe.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HealthCheck {
    /// Type / name of the check.
    pub check_type: String,
    /// "pass", "fail", or "warn".
    pub status: String,
    /// Human-readable message.
    pub message: String,
    /// Observed latency in milliseconds.
    pub latency_ms: Option<u64>,
    /// When the check was performed.
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// HealthConfig
// ---------------------------------------------------------------------------

/// Configuration controlling health-check behaviour.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HealthConfig {
    /// Endpoint to probe.
    pub endpoint: String,
    /// Per-probe timeout in milliseconds.
    pub timeout_ms: u64,
    /// Interval between probes in milliseconds.
    pub interval_ms: u64,
    /// Maximum retry attempts before declaring failure.
    pub max_retries: u32,
    /// Expected HTTP status code.
    pub expected_status: u16,
    /// Optional expected latency ceiling.
    pub expected_latency_ms: Option<u64>,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            endpoint: String::from("http://localhost:8080/health"),
            timeout_ms: 5000,
            interval_ms: 10_000,
            max_retries: 3,
            expected_status: 200,
            expected_latency_ms: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Deployment (live, interior-mutable)
// ---------------------------------------------------------------------------

/// A live deployment with interior mutability for status transitions.
pub struct Deployment {
    /// Unique deployment identifier.
    pub id: String,
    /// Configuration used for this deployment.
    pub config: DeployConfig,
    /// Current lifecycle status.
    pub status: Arc<RwLock<DeployStatus>>,
    /// Infrastructure target.
    pub target: DeployTarget,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// When the deployment transitioned to Pulling.
    pub started_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// When the deployment reached a terminal state.
    pub finished_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// Error message if the deployment failed.
    pub error: Arc<Mutex<Option<String>>>,
    /// Historical health check results.
    pub health_checks: Arc<RwLock<Vec<HealthCheck>>>,
}

impl Deployment {
    /// Produce an immutable point-in-time snapshot.
    pub fn snapshot(&self) -> DeploymentSnapshot {
        let status = self.status.read().unwrap().clone();
        let started_at = *self.started_at.read().unwrap();
        let finished_at = *self.finished_at.read().unwrap();
        let error = self.error.lock().unwrap().clone();
        let health_checks = self.health_checks.read().unwrap().clone();
        DeploymentSnapshot {
            id: self.id.clone(),
            config: self.config.clone(),
            status,
            target: self.target.clone(),
            created_at: self.created_at,
            started_at,
            finished_at,
            error,
            health_checks,
        }
    }
}

// ---------------------------------------------------------------------------
// DeploymentSnapshot
// ---------------------------------------------------------------------------

/// Immutable point-in-time view of a deployment.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeploymentSnapshot {
    pub id: String,
    pub config: DeployConfig,
    pub status: DeployStatus,
    pub target: DeployTarget,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub health_checks: Vec<HealthCheck>,
}

// ---------------------------------------------------------------------------
// RollbackConfig
// ---------------------------------------------------------------------------

/// Configuration for rolling back a deployment to a previous version.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RollbackConfig {
    /// Deployment to roll back.
    pub deployment_id: String,
    /// Explicit version to roll back to. None means "previous version".
    pub target_version: Option<String>,
    /// Whether to preserve runtime data during rollback.
    pub preserve_data: bool,
    /// Force rollback even if health checks fail.
    pub force: bool,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            deployment_id: String::new(),
            target_version: None,
            preserve_data: true,
            force: false,
        }
    }
}

// ---------------------------------------------------------------------------
// DeployMetrics
// ---------------------------------------------------------------------------

/// Aggregate deployment statistics.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DeployMetrics {
    pub total_deployments: u64,
    pub successful: u64,
    pub failed: u64,
    pub rolled_back: u64,
    pub avg_deploy_time_secs: f64,
}

// ---------------------------------------------------------------------------
// DeploymentManager
// ---------------------------------------------------------------------------

/// Central manager for model deployments, configurations, and targets.
pub struct DeploymentManager {
    /// Active deployments keyed by id.
    deployments: DashMap<String, Arc<Deployment>>,
    /// Named deployment configs.
    configs: DashMap<String, DeployConfig>,
    /// Registered deploy targets.
    targets: DashMap<String, DeployTarget>,
    /// Aggregate metrics.
    metrics: Arc<RwLock<DeployMetrics>>,
}

impl DeploymentManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            deployments: DashMap::new(),
            configs: DashMap::new(),
            targets: DashMap::new(),
            metrics: Arc::new(RwLock::new(DeployMetrics::default())),
        }
    }

    // -- Deploy lifecycle ----------------------------------------------------

    /// Prepare a new deployment in `Pending` state.
    pub fn prepare(
        &self,
        config: DeployConfig,
        target: DeployTarget,
    ) -> DeploymentSnapshot {
        let id = generate_id();
        let now = Utc::now();
        let deployment = Deployment {
            id: id.clone(),
            config,
            status: Arc::new(RwLock::new(DeployStatus::Pending)),
            target,
            created_at: now,
            started_at: Arc::new(RwLock::new(None)),
            finished_at: Arc::new(RwLock::new(None)),
            error: Arc::new(Mutex::new(None)),
            health_checks: Arc::new(RwLock::new(Vec::new())),
        };
        let snap = deployment.snapshot();
        self.deployments.insert(id, Arc::new(deployment));
        snap
    }

    /// Execute a deployment (mock). Transitions Pending -> Pulling -> Loading
    /// -> Running, recording health checks.
    pub fn deploy(&self, id: &str) -> Result<DeploymentSnapshot, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment '{}' not found", id))?;

        {
            let mut status = entry.status.write().unwrap();
            if *status != DeployStatus::Pending && *status != DeployStatus::Stopped {
                return Err(format!(
                    "cannot deploy from status '{}'; expected Pending or Stopped",
                    status
                ));
            }
            *status = DeployStatus::Pulling;
        }
        *entry.started_at.write().unwrap() = Some(Utc::now());

        // Mock: simulate pull phase
        {
            let mut status = entry.status.write().unwrap();
            *status = DeployStatus::Loading;
        }

        // Mock: simulate successful load -> running
        {
            let mut status = entry.status.write().unwrap();
            *status = DeployStatus::Running;
        }
        *entry.finished_at.write().unwrap() = Some(Utc::now());

        // Record mock health checks
        {
            let mut checks = entry.health_checks.write().unwrap();
            checks.push(HealthCheck {
                check_type: String::from("liveness"),
                status: String::from("pass"),
                message: String::from("service is live"),
                latency_ms: Some(12),
                timestamp: Utc::now(),
            });
            checks.push(HealthCheck {
                check_type: String::from("readiness"),
                status: String::from("pass"),
                message: String::from("model loaded and ready"),
                latency_ms: Some(45),
                timestamp: Utc::now(),
            });
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().unwrap();
            metrics.total_deployments += 1;
            metrics.successful += 1;
            let n = metrics.successful;
            metrics.avg_deploy_time_secs =
                (metrics.avg_deploy_time_secs * ((n - 1) as f64) + 3.5) / (n as f64);
        }

        Ok(entry.snapshot())
    }

    /// Stop a running deployment.
    pub fn stop(&self, id: &str) -> Result<DeploymentSnapshot, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment '{}' not found", id))?;

        {
            let mut status = entry.status.write().unwrap();
            if *status != DeployStatus::Running {
                return Err(format!("cannot stop deployment in status '{}'", status));
            }
            *status = DeployStatus::Stopped;
        }
        *entry.finished_at.write().unwrap() = Some(Utc::now());

        Ok(entry.snapshot())
    }

    /// Roll back a deployment (mock). Transitions to RollingBack, then back
    /// to Running with the previous version.
    pub fn rollback(&self, config: RollbackConfig) -> Result<DeploymentSnapshot, String> {
        let entry = self
            .deployments
            .get(&config.deployment_id)
            .ok_or_else(|| {
                format!(
                    "deployment '{}' not found",
                    config.deployment_id
                )
            })?;

        {
            let status = entry.status.read().unwrap();
            if *status != DeployStatus::Running && *status != DeployStatus::Failed {
                return Err(format!(
                    "cannot rollback from status '{}'",
                    status
                ));
            }
        }

        // Transition to RollingBack
        {
            let mut status = entry.status.write().unwrap();
            *status = DeployStatus::RollingBack;
        }

        // Mock: restore previous version
        if let Some(ref ver) = config.target_version {
            let mut cfg = entry.config.clone();
            cfg.version = ver.clone();
            // We don't mutate the Deployment's config directly; this is
            // simulated by updating health checks to reflect the rollback.
        }

        // Mock: back to running
        {
            let mut status = entry.status.write().unwrap();
            *status = DeployStatus::Running;
        }
        *entry.finished_at.write().unwrap() = Some(Utc::now());

        // Record rollback health check
        {
            let mut checks = entry.health_checks.write().unwrap();
            checks.push(HealthCheck {
                check_type: String::from("rollback"),
                status: String::from("pass"),
                message: String::from("rollback completed successfully"),
                latency_ms: Some(120),
                timestamp: Utc::now(),
            });
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().unwrap();
            metrics.rolled_back += 1;
        }

        Ok(entry.snapshot())
    }

    // -- Health ---------------------------------------------------------------

    /// Run mock health checks against a deployment.
    pub fn verify_health(
        &self,
        id: &str,
        health_config: &HealthConfig,
    ) -> Result<Vec<HealthCheck>, String> {
        let entry = self
            .deployments
            .get(id)
            .ok_or_else(|| format!("deployment '{}' not found", id))?;

        let current_status = entry.status.read().unwrap().clone();
        if current_status != DeployStatus::Running {
            return Err(format!(
                "deployment '{}' is not Running (current: {})",
                id, current_status
            ));
        }

        let mut checks = Vec::new();
        let now = Utc::now();

        // Mock liveness check
        let latency = 5 + (id.len() as u64 % 50);
        let pass = latency < health_config.timeout_ms;
        checks.push(HealthCheck {
            check_type: String::from("liveness"),
            status: if pass {
                String::from("pass")
            } else {
                String::from("fail")
            },
            message: if pass {
                String::from("liveness probe succeeded")
            } else {
                String::from("liveness probe timed out")
            },
            latency_ms: Some(latency),
            timestamp: now,
        });

        // Mock readiness check
        let latency2 = 20 + (id.len() as u64 % 100);
        let readiness_pass = latency2 < health_config.timeout_ms;
        checks.push(HealthCheck {
            check_type: String::from("readiness"),
            status: if readiness_pass {
                String::from("pass")
            } else {
                String::from("fail")
            },
            message: if readiness_pass {
                String::from("model is ready for inference")
            } else {
                String::from("model not ready yet")
            },
            latency_ms: Some(latency2),
            timestamp: now,
        });

        // Mock GPU memory check
        checks.push(HealthCheck {
            check_type: String::from("gpu_memory"),
            status: String::from("pass"),
            message: String::from("GPU memory within acceptable bounds"),
            latency_ms: Some(2),
            timestamp: now,
        });

        // Check against expected_latency_ms if configured
        if let Some(max_lat) = health_config.expected_latency_ms {
            let overall_ok = checks.iter().all(|c| {
                c.status == "pass"
                    && c.latency_ms
                        .map(|l| l <= max_lat)
                        .unwrap_or(true)
            });
            if !overall_ok {
                checks.push(HealthCheck {
                    check_type: String::from("latency_threshold"),
                    status: String::from("warn"),
                    message: format!(
                        "some checks exceeded expected latency of {} ms",
                        max_lat
                    ),
                    latency_ms: None,
                    timestamp: now,
                });
            }
        }

        // Persist checks
        {
            let mut stored = entry.health_checks.write().unwrap();
            stored.extend(checks.clone());
        }

        Ok(checks)
    }

    // -- Query helpers -------------------------------------------------------

    /// Get a snapshot of a single deployment by id.
    pub fn get_deployment(&self, id: &str) -> Option<DeploymentSnapshot> {
        self.deployments.get(id).map(|d| d.snapshot())
    }

    /// List all deployments.
    pub fn list_deployments(&self) -> Vec<DeploymentSnapshot> {
        self.deployments.iter().map(|d| d.value().snapshot()).collect()
    }

    /// List deployments matching a specific status.
    pub fn list_by_status(&self, status: DeployStatus) -> Vec<DeploymentSnapshot> {
        self.deployments
            .iter()
            .filter(|d| {
                let s = d.value().status.read().unwrap();
                *s == status
            })
            .map(|d| d.value().snapshot())
            .collect()
    }

    // -- Config management ---------------------------------------------------

    /// Retrieve a named config.
    pub fn get_config(&self, name: &str) -> Option<DeployConfig> {
        self.configs.get(name).map(|c| c.value().clone())
    }

    /// Persist a named config.
    pub fn save_config(&self, name: String, config: DeployConfig) -> Result<(), String> {
        self.configs.insert(name, config);
        Ok(())
    }

    /// List all named configs.
    pub fn list_configs(&self) -> Vec<(String, DeployConfig)> {
        self.configs
            .iter()
            .map(|c| (c.key().clone(), c.value().clone()))
            .collect()
    }

    // -- Target management ---------------------------------------------------

    /// Register a new deploy target.
    pub fn add_target(&self, target: DeployTarget) -> Result<DeployTarget, String> {
        let name = target.name.clone();
        if self.targets.contains_key(&name) {
            return Err(format!("target '{}' already exists", name));
        }
        let out = target.clone();
        self.targets.insert(name, target);
        Ok(out)
    }

    /// Remove a deploy target by name.
    pub fn remove_target(&self, name: &str) -> Result<(), String> {
        self.targets
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| format!("target '{}' not found", name))
    }

    /// List all registered targets.
    pub fn list_targets(&self) -> Vec<DeployTarget> {
        self.targets.iter().map(|t| t.value().clone()).collect()
    }

    // -- Metrics -------------------------------------------------------------

    /// Return current aggregate metrics.
    pub fn get_metrics(&self) -> DeployMetrics {
        self.metrics.read().unwrap().clone()
    }
}

impl Default for DeploymentManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a short pseudo-random deployment id.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Simple mixing to avoid obvious sequential ids.
    let mixed = nanos.wrapping_mul(0x517cc1b727220a95);
    format!("{:016x}", mixed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> DeployConfig {
        DeployConfig {
            model_id: String::from("mistral-7b"),
            model_name: String::from("Mistral 7B"),
            version: String::from("1.0.0"),
            source_url: String::from("https://registry.xergon.io/mistral-7b"),
            backend: String::from("ollama"),
            gpu_layers: 0,
            context_size: 4096,
            batch_size: 32,
            max_tokens: 2048,
            temperature: 0.7,
            repeat_penalty: 1.1,
            seed: None,
            env_vars: HashMap::new(),
            labels: HashMap::new(),
        }
    }

    fn sample_target() -> DeployTarget {
        DeployTarget {
            name: String::from("gpu-node-1"),
            endpoint: String::from("https://gpu1.xergon.io"),
            gpu_type: String::from("a100"),
            gpu_count: 1,
            region: String::from("us-east-1"),
            available_vram_gb: 80.0,
        }
    }

    // -- DeployConfig tests --------------------------------------------------

    #[test]
    fn deploy_config_default_values() {
        let cfg = DeployConfig::default();
        assert_eq!(cfg.backend, "ollama");
        assert_eq!(cfg.gpu_layers, 0);
        assert_eq!(cfg.context_size, 4096);
        assert_eq!(cfg.batch_size, 32);
        assert_eq!(cfg.max_tokens, 2048);
        assert!((cfg.temperature - 0.7).abs() < f64::EPSILON);
        assert!((cfg.repeat_penalty - 1.1).abs() < f64::EPSILON);
        assert!(cfg.seed.is_none());
        assert!(cfg.env_vars.is_empty());
        assert!(cfg.labels.is_empty());
    }

    #[test]
    fn deploy_config_serialisation_round_trip() {
        let cfg = sample_config();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: DeployConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model_id, cfg.model_id);
        assert_eq!(back.version, cfg.version);
    }

    #[test]
    fn deploy_config_with_env_vars_and_labels() {
        let mut cfg = sample_config();
        cfg.env_vars
            .insert(String::from("OLLAMA_HOST"), String::from("0.0.0.0"));
        cfg.labels
            .insert(String::from("team"), String::from("ml"));
        assert_eq!(cfg.env_vars.len(), 1);
        assert_eq!(cfg.labels.len(), 1);
    }

    #[test]
    fn deploy_config_clone_is_independent() {
        let mut cfg = sample_config();
        let mut clone = cfg.clone();
        clone.temperature = 0.9;
        assert!((cfg.temperature - 0.7).abs() < f64::EPSILON);
        assert!((clone.temperature - 0.9).abs() < f64::EPSILON);
    }

    // -- DeployStatus tests --------------------------------------------------

    #[test]
    fn deploy_status_display() {
        assert_eq!(format!("{}", DeployStatus::Pending), "Pending");
        assert_eq!(format!("{}", DeployStatus::Running), "Running");
        assert_eq!(format!("{}", DeployStatus::Failed), "Failed");
        assert_eq!(format!("{}", DeployStatus::RollingBack), "RollingBack");
    }

    #[test]
    fn deploy_status_serialisation_round_trip() {
        for s in [
            DeployStatus::Pending,
            DeployStatus::Pulling,
            DeployStatus::Loading,
            DeployStatus::Running,
            DeployStatus::Failed,
            DeployStatus::Stopped,
            DeployStatus::RollingBack,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: DeployStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    // -- DeployTarget tests --------------------------------------------------

    #[test]
    fn deploy_target_round_trip() {
        let t = sample_target();
        let json = serde_json::to_string(&t).unwrap();
        let back: DeployTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, t.name);
        assert_eq!(back.gpu_type, t.gpu_type);
        assert!((back.available_vram_gb - 80.0).abs() < f64::EPSILON);
    }

    // -- HealthConfig tests --------------------------------------------------

    #[test]
    fn health_config_defaults() {
        let hc = HealthConfig::default();
        assert_eq!(hc.timeout_ms, 5000);
        assert_eq!(hc.interval_ms, 10_000);
        assert_eq!(hc.max_retries, 3);
        assert_eq!(hc.expected_status, 200);
        assert!(hc.expected_latency_ms.is_none());
    }

    // -- HealthCheck tests ---------------------------------------------------

    #[test]
    fn health_check_serialisation() {
        let hc = HealthCheck {
            check_type: String::from("liveness"),
            status: String::from("pass"),
            message: String::from("ok"),
            latency_ms: Some(10),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&hc).unwrap();
        let back: HealthCheck = serde_json::from_str(&json).unwrap();
        assert_eq!(back.check_type, hc.check_type);
        assert_eq!(back.status, "pass");
        assert_eq!(back.latency_ms, Some(10));
    }

    // -- RollbackConfig tests ------------------------------------------------

    #[test]
    fn rollback_config_defaults() {
        let rc = RollbackConfig::default();
        assert!(rc.deployment_id.is_empty());
        assert!(rc.target_version.is_none());
        assert!(rc.preserve_data);
        assert!(!rc.force);
    }

    // -- DeployMetrics tests -------------------------------------------------

    #[test]
    fn deploy_metrics_default() {
        let m = DeployMetrics::default();
        assert_eq!(m.total_deployments, 0);
        assert_eq!(m.successful, 0);
        assert_eq!(m.failed, 0);
        assert_eq!(m.rolled_back, 0);
        assert!((m.avg_deploy_time_secs - 0.0).abs() < f64::EPSILON);
    }

    // -- DeploymentManager: prepare + deploy ---------------------------------

    #[test]
    fn prepare_sets_pending_status() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        assert_eq!(snap.status, DeployStatus::Pending);
        assert!(!snap.id.is_empty());
    }

    #[test]
    fn deploy_transitions_to_running() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let id = snap.id.clone();
        let result = mgr.deploy(&id).unwrap();
        assert_eq!(result.status, DeployStatus::Running);
        assert!(result.started_at.is_some());
        assert!(result.finished_at.is_some());
    }

    #[test]
    fn deploy_records_health_checks() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let id = snap.id.clone();
        mgr.deploy(&id).unwrap();
        let deployed = mgr.get_deployment(&id).unwrap();
        assert!(deployed.health_checks.len() >= 2);
    }

    #[test]
    fn deploy_updates_metrics() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        mgr.deploy(&snap.id).unwrap();
        let metrics = mgr.get_metrics();
        assert_eq!(metrics.total_deployments, 1);
        assert_eq!(metrics.successful, 1);
    }

    #[test]
    fn deploy_unknown_id_returns_error() {
        let mgr = DeploymentManager::new();
        let result = mgr.deploy("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn deploy_from_wrong_status_returns_error() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        mgr.deploy(&snap.id).unwrap();
        // Already running, should fail
        let result = mgr.deploy(&snap.id);
        assert!(result.is_err());
    }

    // -- DeploymentManager: stop ---------------------------------------------

    #[test]
    fn stop_running_deployment() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let id = snap.id.clone();
        mgr.deploy(&id).unwrap();
        let stopped = mgr.stop(&id).unwrap();
        assert_eq!(stopped.status, DeployStatus::Stopped);
    }

    #[test]
    fn stop_non_running_returns_error() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let result = mgr.stop(&snap.id);
        assert!(result.is_err());
    }

    #[test]
    fn stop_unknown_id_returns_error() {
        let mgr = DeploymentManager::new();
        let result = mgr.stop("nope");
        assert!(result.is_err());
    }

    // -- DeploymentManager: rollback -----------------------------------------

    #[test]
    fn rollback_running_deployment() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let id = snap.id.clone();
        mgr.deploy(&id).unwrap();

        let rc = RollbackConfig {
            deployment_id: id.clone(),
            target_version: Some(String::from("0.9.0")),
            preserve_data: true,
            force: false,
        };
        let result = mgr.rollback(rc).unwrap();
        assert_eq!(result.status, DeployStatus::Running);
    }

    #[test]
    fn rollback_updates_rolled_back_metric() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let id = snap.id.clone();
        mgr.deploy(&id).unwrap();

        let rc = RollbackConfig {
            deployment_id: id,
            target_version: None,
            preserve_data: false,
            force: true,
        };
        mgr.rollback(rc).unwrap();
        let metrics = mgr.get_metrics();
        assert_eq!(metrics.rolled_back, 1);
    }

    // -- DeploymentManager: health -------------------------------------------

    #[test]
    fn verify_health_on_running_deployment() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let id = snap.id.clone();
        mgr.deploy(&id).unwrap();

        let hc = HealthConfig::default();
        let checks = mgr.verify_health(&id, &hc).unwrap();
        assert!(!checks.is_empty());
        assert!(checks.iter().any(|c| c.check_type == "liveness"));
        assert!(checks.iter().any(|c| c.check_type == "readiness"));
    }

    #[test]
    fn verify_health_on_non_running_returns_error() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let hc = HealthConfig::default();
        let result = mgr.verify_health(&snap.id, &hc);
        assert!(result.is_err());
    }

    #[test]
    fn verify_health_unknown_id_returns_error() {
        let mgr = DeploymentManager::new();
        let hc = HealthConfig::default();
        let result = mgr.verify_health("nope", &hc);
        assert!(result.is_err());
    }

    // -- DeploymentManager: queries ------------------------------------------

    #[test]
    fn get_deployment_returns_none_for_missing() {
        let mgr = DeploymentManager::new();
        assert!(mgr.get_deployment("missing").is_none());
    }

    #[test]
    fn list_deployments_includes_all() {
        let mgr = DeploymentManager::new();
        let s1 = mgr.prepare(sample_config(), sample_target());
        let s2 = mgr.prepare(sample_config(), sample_target());
        let list = mgr.list_deployments();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|d| d.id == s1.id));
        assert!(list.iter().any(|d| d.id == s2.id));
    }

    #[test]
    fn list_by_status_filters_correctly() {
        let mgr = DeploymentManager::new();
        let s1 = mgr.prepare(sample_config(), sample_target());
        mgr.prepare(sample_config(), sample_target());
        mgr.deploy(&s1.id).unwrap();

        let running = mgr.list_by_status(DeployStatus::Pending);
        assert_eq!(running.len(), 1);

        let running = mgr.list_by_status(DeployStatus::Running);
        assert_eq!(running.len(), 1);
    }

    // -- DeploymentManager: config management --------------------------------

    #[test]
    fn save_and_get_config() {
        let mgr = DeploymentManager::new();
        let cfg = sample_config();
        mgr.save_config(String::from("mistral"), cfg.clone()).unwrap();
        let fetched = mgr.get_config("mistral").unwrap();
        assert_eq!(fetched.model_id, cfg.model_id);
    }

    #[test]
    fn get_config_missing_returns_none() {
        let mgr = DeploymentManager::new();
        assert!(mgr.get_config("nope").is_none());
    }

    #[test]
    fn list_configs() {
        let mgr = DeploymentManager::new();
        mgr.save_config(String::from("a"), sample_config()).unwrap();
        let mut cfg_b = sample_config();
        cfg_b.model_id = String::from("phi-3");
        mgr.save_config(String::from("b"), cfg_b).unwrap();
        let list = mgr.list_configs();
        assert_eq!(list.len(), 2);
    }

    // -- DeploymentManager: target management --------------------------------

    #[test]
    fn add_and_list_target() {
        let mgr = DeploymentManager::new();
        let t = sample_target();
        mgr.add_target(t.clone()).unwrap();
        let targets = mgr.list_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, t.name);
    }

    #[test]
    fn add_duplicate_target_returns_error() {
        let mgr = DeploymentManager::new();
        mgr.add_target(sample_target()).unwrap();
        let result = mgr.add_target(sample_target());
        assert!(result.is_err());
    }

    #[test]
    fn remove_target() {
        let mgr = DeploymentManager::new();
        mgr.add_target(sample_target()).unwrap();
        mgr.remove_target("gpu-node-1").unwrap();
        assert!(mgr.list_targets().is_empty());
    }

    #[test]
    fn remove_missing_target_returns_error() {
        let mgr = DeploymentManager::new();
        let result = mgr.remove_target("nope");
        assert!(result.is_err());
    }

    // -- DeploymentManager: metrics ------------------------------------------

    #[test]
    fn metrics_update_across_multiple_deploys() {
        let mgr = DeploymentManager::new();
        for _ in 0..3 {
            let snap = mgr.prepare(sample_config(), sample_target());
            mgr.deploy(&snap.id).unwrap();
        }
        let m = mgr.get_metrics();
        assert_eq!(m.total_deployments, 3);
        assert_eq!(m.successful, 3);
        assert!(m.avg_deploy_time_secs > 0.0);
    }

    // -- DeploymentManager: default ------------------------------------------

    #[test]
    fn deployment_manager_default() {
        let mgr = DeploymentManager::default();
        assert!(mgr.list_deployments().is_empty());
        assert!(mgr.list_configs().is_empty());
        assert!(mgr.list_targets().is_empty());
    }

    // -- DeploymentSnapshot tests --------------------------------------------

    #[test]
    fn snapshot_serialisation_round_trip() {
        let mgr = DeploymentManager::new();
        let snap = mgr.prepare(sample_config(), sample_target());
        let id = snap.id.clone();
        mgr.deploy(&id).unwrap();
        let snap = mgr.get_deployment(&id).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        let back: DeploymentSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, snap.id);
        assert_eq!(back.status, DeployStatus::Running);
    }

    // -- Cast-before-method rule verification --------------------------------

    #[test]
    fn cast_before_method_pattern() {
        let val: f64 = 3.7;
        let result: u32 = ((val).ceil() as u32).max(1);
        assert_eq!(result, 4);
    }

    #[test]
    fn cast_before_method_zero_input() {
        let val: f64 = 0.0;
        let result: u32 = ((val).ceil() as u32).max(1);
        assert_eq!(result, 1);
    }
}
