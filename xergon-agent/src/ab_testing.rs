use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TestStatus {
    Draft,
    Running,
    Paused,
    Completed,
    RolledBack,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Config / value structs
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VariantConfig {
    pub model_id: String,
    pub weight: f64,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TestConfig {
    pub name: String,
    pub description: String,
    pub variants: Vec<VariantConfig>,
    #[serde(default = "default_metrics")]
    pub metrics_to_track: Vec<String>,
    #[serde(default = "default_min_samples")]
    pub min_samples: u32,
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f64,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,
}

fn default_metrics() -> Vec<String> {
    vec!["latency".into(), "accuracy".into(), "user_satisfaction".into()]
}

fn default_min_samples() -> u32 {
    100
}

fn default_confidence() -> f64 {
    0.95
}

fn default_max_duration() -> u64 {
    86_400
}

// ---------------------------------------------------------------------------
// TestVariant (not Clone because AtomicU64 + DashMap)
// ---------------------------------------------------------------------------

pub struct TestVariant {
    pub id: String,
    pub model_id: String,
    pub weight: f64,
    pub samples: AtomicU64,
    pub metrics: DashMap<String, f64>,
    pub created_at: DateTime<Utc>,
}

impl TestVariant {
    pub fn new(id: String, model_id: String, weight: f64) -> Self {
        Self {
            id,
            model_id,
            weight,
            samples: AtomicU64::new(0),
            metrics: DashMap::new(),
            created_at: Utc::now(),
        }
    }

    pub fn snapshot(&self) -> TestVariantSnapshot {
        let metrics: HashMap<String, f64> = self
            .metrics
            .iter()
            .map(|r| (r.key().clone(), *r.value()))
            .collect();
        TestVariantSnapshot {
            id: self.id.clone(),
            model_id: self.model_id.clone(),
            weight: self.weight,
            samples: self.samples.load(Ordering::Relaxed),
            metrics,
            created_at: self.created_at,
        }
    }

    pub fn record_sample(&self) {
        self.samples.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_metric(&self, name: &str, value: f64) {
        // Store the latest observed value. Callers that need aggregation
        // (mean, sum, etc.) should layer that on top.
        self.metrics.insert(name.to_string(), value);
    }
}

// ---------------------------------------------------------------------------
// TestVariantSnapshot (Clone-able)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TestVariantSnapshot {
    pub id: String,
    pub model_id: String,
    pub weight: f64,
    pub samples: u64,
    pub metrics: HashMap<String, f64>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ABTestResult
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ABTestResult {
    pub variant_id: String,
    pub metric_name: String,
    pub value: f64,
    pub improvement_pct: f64,
    pub p_value: f64,
    pub significant: bool,
}

// ---------------------------------------------------------------------------
// ABTest (shared via inner Arc<RwLock> / Arc<Mutex>)
// ---------------------------------------------------------------------------

pub struct ABTest {
    pub id: String,
    pub config: TestConfig,
    pub status: Arc<RwLock<TestStatus>>,
    pub variants: DashMap<String, TestVariant>,
    pub winner: Arc<Mutex<Option<String>>>,
    pub started_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    pub completed_at: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl ABTest {
    pub fn new(id: String, config: TestConfig) -> Self {
        let variants = DashMap::new();
        for vc in &config.variants {
            let vid = Uuid::new_v4().to_string();
            let tv = TestVariant::new(vid.clone(), vc.model_id.clone(), vc.weight);
            variants.insert(vid, tv);
        }
        Self {
            id,
            config,
            status: Arc::new(RwLock::new(TestStatus::Draft)),
            variants,
            winner: Arc::new(Mutex::new(None)),
            started_at: Arc::new(RwLock::new(None)),
            completed_at: Arc::new(RwLock::new(None)),
        }
    }

    // -- accessors --

    pub fn get_status(&self) -> TestStatus {
        self.status.read().unwrap().clone()
    }

    pub fn set_status(&self, s: TestStatus) {
        *self.status.write().unwrap() = s;
    }

    pub fn get_winner(&self) -> Option<String> {
        self.winner.lock().unwrap().clone()
    }

    pub fn set_winner(&self, w: Option<String>) {
        *self.winner.lock().unwrap() = w;
    }

    pub fn get_completed_at(&self) -> Option<DateTime<Utc>> {
        *self.completed_at.read().unwrap()
    }

    pub fn set_completed_at(&self, dt: Option<DateTime<Utc>>) {
        *self.completed_at.write().unwrap() = dt;
    }

    // -- snapshot --

    pub fn snapshot(&self, results: Vec<ABTestResult>) -> ABTestSnapshot {
        let variant_snaps: Vec<TestVariantSnapshot> =
            self.variants.iter().map(|r| r.value().snapshot()).collect();
        ABTestSnapshot {
            id: self.id.clone(),
            name: self.config.name.clone(),
            description: self.config.description.clone(),
            status: self.get_status(),
            variants: variant_snaps,
            winner: self.get_winner(),
            started_at: *self.started_at.read().unwrap(),
            completed_at: self.get_completed_at(),
            results,
        }
    }
}

// ---------------------------------------------------------------------------
// ABTestSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ABTestSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: TestStatus,
    pub variants: Vec<TestVariantSnapshot>,
    pub winner: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub results: Vec<ABTestResult>,
}

// ---------------------------------------------------------------------------
// ABTestMetricsSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ABTestMetricsSnapshot {
    pub total_tests: u64,
    pub running_tests: u64,
    pub completed_tests: u64,
    pub total_samples: u64,
    pub avg_improvement_pct: f64,
}

// ---------------------------------------------------------------------------
// ABTestingFramework
// ---------------------------------------------------------------------------

pub struct ABTestingFramework {
    pub tests: DashMap<String, Arc<ABTest>>,
    pub total_tests_created: AtomicU64,
    pub total_completed: AtomicU64,
    pub total_rolled_back: AtomicU64,
}

impl ABTestingFramework {
    pub fn new() -> Self {
        Self {
            tests: DashMap::new(),
            total_tests_created: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_rolled_back: AtomicU64::new(0),
        }
    }

    // -- CRUD lifecycle --

    pub fn create_test(&self, config: TestConfig) -> Result<ABTestSnapshot, String> {
        if config.variants.len() < 2 {
            return Err("At least 2 variants are required".into());
        }
        let total_weight: f64 = config.variants.iter().map(|v| v.weight).sum();
        if (total_weight - 1.0).abs() > 0.05 {
            return Err(format!(
                "Variant weights must sum to ~1.0 (got {:.4})",
                total_weight
            ));
        }

        let id = Uuid::new_v4().to_string();
        let test = Arc::new(ABTest::new(id.clone(), config));
        let snap = test.snapshot(vec![]);
        self.tests.insert(id.clone(), test);
        self.total_tests_created.fetch_add(1, Ordering::Relaxed);
        Ok(snap)
    }

    pub fn get_test(&self, id: &str) -> Option<ABTestSnapshot> {
        let test = self.tests.get(id)?;
        let results = self.get_results(id).unwrap_or_default();
        Some(test.snapshot(results))
    }

    pub fn start_test(&self, id: &str) -> Result<ABTestSnapshot, String> {
        let test = self
            .tests
            .get(id)
            .ok_or_else(|| format!("Test {} not found", id))?;
        let status = test.get_status();
        if status != TestStatus::Draft {
            return Err(format!("Cannot start test in {:?} state", status));
        }
        test.set_status(TestStatus::Running);
        *test.started_at.write().unwrap() = Some(Utc::now());
        Ok(test.snapshot(vec![]))
    }

    pub fn pause_test(&self, id: &str) -> Result<ABTestSnapshot, String> {
        let test = self
            .tests
            .get(id)
            .ok_or_else(|| format!("Test {} not found", id))?;
        let status = test.get_status();
        if status != TestStatus::Running {
            return Err(format!("Cannot pause test in {:?} state", status));
        }
        test.set_status(TestStatus::Paused);
        Ok(test.snapshot(vec![]))
    }

    pub fn resume_test(&self, id: &str) -> Result<ABTestSnapshot, String> {
        let test = self
            .tests
            .get(id)
            .ok_or_else(|| format!("Test {} not found", id))?;
        let status = test.get_status();
        if status != TestStatus::Paused {
            return Err(format!("Cannot resume test in {:?} state", status));
        }
        test.set_status(TestStatus::Running);
        Ok(test.snapshot(vec![]))
    }

    pub fn complete_test(&self, id: &str) -> Result<ABTestSnapshot, String> {
        let test = self
            .tests
            .get(id)
            .ok_or_else(|| format!("Test {} not found", id))?;
        let status = test.get_status();
        if status != TestStatus::Running && status != TestStatus::Paused {
            return Err(format!("Cannot complete test in {:?} state", status));
        }

        let winner_id = self.determine_winner(id)?;
        test.set_winner(winner_id.clone());
        test.set_status(TestStatus::Completed);
        test.set_completed_at(Some(Utc::now()));
        self.total_completed.fetch_add(1, Ordering::Relaxed);

        let results = self.get_results(id).unwrap_or_default();
        Ok(test.snapshot(results))
    }

    pub fn cancel_test(&self, id: &str) -> Result<ABTestSnapshot, String> {
        let test = self
            .tests
            .get(id)
            .ok_or_else(|| format!("Test {} not found", id))?;
        let status = test.get_status();
        if status == TestStatus::Completed || status == TestStatus::Cancelled {
            return Err(format!("Cannot cancel test in {:?} state", status));
        }
        test.set_status(TestStatus::Cancelled);
        Ok(test.snapshot(vec![]))
    }

    pub fn rollback_test(&self, id: &str) -> Result<ABTestSnapshot, String> {
        let test = self
            .tests
            .get(id)
            .ok_or_else(|| format!("Test {} not found", id))?;
        let status = test.get_status();
        if status != TestStatus::Completed {
            return Err(format!("Cannot rollback test in {:?} state", status));
        }
        test.set_status(TestStatus::RolledBack);
        self.total_completed.fetch_sub(1, Ordering::Relaxed);
        self.total_rolled_back.fetch_add(1, Ordering::Relaxed);
        Ok(test.snapshot(vec![]))
    }

    // -- metric recording --

    pub fn record_metric(
        &self,
        test_id: &str,
        variant_id: &str,
        metric_name: &str,
        value: f64,
    ) -> Result<(), String> {
        let test = self
            .tests
            .get(test_id)
            .ok_or_else(|| format!("Test {} not found", test_id))?;
        let status = test.get_status();
        if status != TestStatus::Running {
            return Err(format!("Cannot record metrics on a {:?} test", status));
        }
        let variant = test
            .variants
            .get(variant_id)
            .ok_or_else(|| format!("Variant {} not found", variant_id))?;
        variant.record_sample();
        variant.add_metric(metric_name, value);
        Ok(())
    }

    // -- analysis --

    pub fn get_results(&self, test_id: &str) -> Result<Vec<ABTestResult>, String> {
        let test = self
            .tests
            .get(test_id)
            .ok_or_else(|| format!("Test {} not found", test_id))?;

        let variant_vec: Vec<TestVariantSnapshot> =
            test.variants.iter().map(|r| r.value().snapshot()).collect();

        if variant_vec.is_empty() {
            return Ok(vec![]);
        }

        // Use first variant as control
        let control = &variant_vec[0];
        let control_samples = control.samples;
        if control_samples == 0 {
            return Ok(vec![]);
        }
        let control_mean = control.metrics.get("latency").copied().unwrap_or(0.0);
        let control_std = (control.metrics.get("latency_std").copied().unwrap_or(0.0))
            .max(0.001);
        let mut results = Vec::new();

        for treatment in &variant_vec[1..] {
            let treatment_samples = treatment.samples;
            if treatment_samples == 0 {
                continue;
            }
            let treatment_mean = treatment.metrics.get("latency").copied().unwrap_or(0.0);
            let treatment_std = (treatment.metrics.get("latency_std").copied().unwrap_or(0.0))
                .max(0.001);

            let n = ((control_samples as f64).ceil() as u32).max(1) as u64;
            let p_value = Self::simulate_p_value(
                control_mean,
                control_std,
                treatment_mean,
                treatment_std,
                n,
            );

            let improvement_pct = if control_mean.abs() < 1e-9 {
                0.0
            } else {
                ((control_mean - treatment_mean) / control_mean.abs()) * 100.0
            };

            let significant = p_value < (1.0 - test.config.confidence_threshold);

            results.push(ABTestResult {
                variant_id: treatment.id.clone(),
                metric_name: "latency".into(),
                value: treatment_mean,
                improvement_pct,
                p_value,
                significant,
            });
        }
        Ok(results)
    }

    pub fn determine_winner(&self, test_id: &str) -> Result<Option<String>, String> {
        let test = self
            .tests
            .get(test_id)
            .ok_or_else(|| format!("Test {} not found", test_id))?;

        let min_samples = test.config.min_samples as u64;
        let variant_vec: Vec<TestVariantSnapshot> =
            test.variants.iter().map(|r| r.value().snapshot()).collect();

        // Check minimum samples
        for v in &variant_vec {
            if v.samples < min_samples {
                return Ok(None);
            }
        }

        let results = self.get_results(test_id)?;
        if results.is_empty() {
            // If we can't compute results, pick the variant with the most samples
            let best = variant_vec
                .iter()
                .max_by_key(|v| v.samples)
                .map(|v| v.id.clone());
            return Ok(best);
        }

        // Pick the result with the greatest positive improvement (lower latency = better)
        let best_result = results
            .iter()
            .filter(|r| r.significant)
            .max_by(|a, b| {
                a.improvement_pct
                    .partial_cmp(&b.improvement_pct)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some(r) = best_result {
            return Ok(Some(r.variant_id.clone()));
        }

        // No significant result – fall back to variant with lowest latency metric
        let best_variant = variant_vec
            .iter()
            .min_by(|a, b| {
                let a_lat = a.metrics.get("latency").copied().unwrap_or(f64::MAX);
                let b_lat = b.metrics.get("latency").copied().unwrap_or(f64::MAX);
                a_lat.partial_cmp(&b_lat).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|v| v.id.clone());
        Ok(best_variant)
    }

    // -- listing --

    pub fn list_tests(&self) -> Vec<ABTestSnapshot> {
        self.tests
            .iter()
            .map(|r| {
                let test = r.value();
                let results = self.get_results(&test.id).unwrap_or_default();
                test.snapshot(results)
            })
            .collect()
    }

    pub fn list_tests_by_status(&self, status: &TestStatus) -> Vec<ABTestSnapshot> {
        self.tests
            .iter()
            .filter(|r| &r.value().get_status() == status)
            .map(|r| {
                let test = r.value();
                let results = self.get_results(&test.id).unwrap_or_default();
                test.snapshot(results)
            })
            .collect()
    }

    // -- metrics --

    pub fn get_metrics(&self) -> ABTestMetricsSnapshot {
        let mut running = 0u64;
        let mut completed = 0u64;
        let mut total_samples = 0u64;
        let mut improvements = Vec::new();

        for entry in self.tests.iter() {
            let test = entry.value();
            match test.get_status() {
                TestStatus::Running => running += 1,
                TestStatus::Completed => completed += 1,
                _ => {}
            }
            for v in test.variants.iter() {
                total_samples += v.value().samples.load(Ordering::Relaxed);
            }
            if let Ok(res) = self.get_results(&test.id) {
                for r in &res {
                    improvements.push(r.improvement_pct);
                }
            }
        }

        let avg_improvement = if improvements.is_empty() {
            0.0
        } else {
            improvements.iter().sum::<f64>() / improvements.len() as f64
        };

        ABTestMetricsSnapshot {
            total_tests: self.total_tests_created.load(Ordering::Relaxed),
            running_tests: running,
            completed_tests: completed,
            total_samples,
            avg_improvement_pct: avg_improvement,
        }
    }

    // -- statistical helpers --

    /// Approximate normal CDF using a simple rational approximation.
    fn normal_cdf(x: f64) -> f64 {
        // Abramowitz and Stegun approximation 26.2.17
        const A1: f64 = 0.254_829_592;
        const A2: f64 = -0.284_496_736;
        const A3: f64 = 1.421_413_741;
        const A4: f64 = -1.453_152_027;
        const A5: f64 = 1.061_405_429;
        const P: f64 = 0.327_591_1;

        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let x = (x * sign).abs();

        let t = 1.0 / (1.0 + P * x);
        let y = 1.0
            - (((((A5 * t + A4) * t) + A3) * t + A2) * t + A1) * t * (-x * x / 2.0).exp();

        0.5 * (1.0 + sign * y)
    }

    /// Mock Z-test p-value computation.
    ///
    /// Computes z = (treatment_mean - control_mean) / sqrt(control_std^2/n + treatment_std^2/n)
    /// then returns p = 2 * (1 - normal_cdf(|z|)).
    pub fn simulate_p_value(
        control_mean: f64,
        control_std: f64,
        treatment_mean: f64,
        treatment_std: f64,
        n: u64,
    ) -> f64 {
        if n == 0 {
            return 1.0;
        }
        let n_f = n as f64;
        let denom = (control_std * control_std / n_f + treatment_std * treatment_std / n_f).sqrt();
        if denom < 1e-12 {
            return 1.0;
        }
        let z = (treatment_mean - control_mean) / denom;
        let abs_z = z.abs();
        let p = 2.0 * (1.0 - Self::normal_cdf(abs_z));
        p.min(1.0).max(0.0)
    }
}

impl Default for ABTestingFramework {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Compatibility types for api/mod.rs
// ---------------------------------------------------------------------------

/// Compatibility type alias for the API layer.
pub type ABTestManager = ABTestingFramework;

/// Request to create a new experiment (maps to TestConfig).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateExperimentRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub variants: Vec<VariantConfig>,
    #[serde(default)]
    pub metrics_to_track: Vec<String>,
    #[serde(default = "default_min_samples")]
    pub min_samples: u32,
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f64,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,
}

/// Experiment is an alias for ABTestSnapshot.
pub type Experiment = ABTestSnapshot;

/// Feedback request for recording metrics against an experiment variant.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FeedbackRequest {
    pub variant_id: String,
    pub metric_name: String,
    pub value: f64,
}

impl ABTestingFramework {
    /// API-compatible method: create an experiment from a CreateExperimentRequest.
    pub fn create_experiment(&self, req: CreateExperimentRequest) -> Result<ABTestSnapshot, String> {
        let config = TestConfig {
            name: req.name,
            description: req.description,
            variants: req.variants,
            metrics_to_track: req.metrics_to_track,
            min_samples: req.min_samples,
            confidence_threshold: req.confidence_threshold,
            max_duration_secs: req.max_duration_secs,
        };
        self.create_test(config)
    }

    /// API-compatible method: list all experiments.
    pub fn list_experiments(&self) -> Vec<ABTestSnapshot> {
        self.list_tests()
    }

    /// API-compatible method: get a single experiment.
    pub fn get_experiment(&self, id: &str) -> Option<ABTestSnapshot> {
        self.get_test(id)
    }

    /// API-compatible method: submit feedback for a variant.
    pub fn submit_feedback(&self, experiment_id: &str, req: FeedbackRequest) -> Result<(), String> {
        self.record_metric(experiment_id, &req.variant_id, &req.metric_name, req.value)
    }

    /// API-compatible method: pause an experiment.
    pub fn pause_experiment(&self, id: &str) -> Result<ABTestSnapshot, String> {
        self.pause_test(id)
    }

    /// API-compatible method: resume a paused experiment.
    pub fn resume_experiment(&self, id: &str) -> Result<ABTestSnapshot, String> {
        self.resume_test(id)
    }

    /// API-compatible method: end an experiment (complete and determine winner).
    pub fn end_experiment(&self, id: &str) -> Result<ABTestSnapshot, String> {
        self.complete_test(id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_framework() -> ABTestingFramework {
        ABTestingFramework::new()
    }

    fn sample_config() -> TestConfig {
        TestConfig {
            name: "test-model-comparison".into(),
            description: "Compare gpt-4 vs gpt-3.5".into(),
            variants: vec![
                VariantConfig {
                    model_id: "gpt-4".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
                VariantConfig {
                    model_id: "gpt-3.5".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
            ],
            metrics_to_track: vec!["latency".into()],
            min_samples: 10,
            confidence_threshold: 0.95,
            max_duration_secs: 3600,
        }
    }

    // 1. Framework creation
    #[test]
    fn test_framework_new() {
        let fw = make_framework();
        assert_eq!(fw.total_tests_created.load(Ordering::Relaxed), 0);
        assert_eq!(fw.tests.len(), 0);
    }

    // 2. Default trait
    #[test]
    fn test_framework_default() {
        let fw = ABTestingFramework::default();
        assert_eq!(fw.tests.len(), 0);
    }

    // 3. Create test with valid config
    #[test]
    fn test_create_test_valid() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        assert_eq!(snap.name, "test-model-comparison");
        assert_eq!(snap.status, TestStatus::Draft);
        assert_eq!(snap.variants.len(), 2);
        assert_eq!(fw.total_tests_created.load(Ordering::Relaxed), 1);
    }

    // 4. Create test with fewer than 2 variants fails
    #[test]
    fn test_create_test_too_few_variants() {
        let fw = make_framework();
        let cfg = TestConfig {
            name: "bad".into(),
            description: "x".into(),
            variants: vec![VariantConfig {
                model_id: "m1".into(),
                weight: 1.0,
                metadata: HashMap::new(),
            }],
            metrics_to_track: vec![],
            min_samples: 100,
            confidence_threshold: 0.95,
            max_duration_secs: 86400,
        };
        let result = fw.create_test(cfg);
        assert!(result.is_err());
    }

    // 5. Create test with bad weights fails
    #[test]
    fn test_create_test_bad_weights() {
        let fw = make_framework();
        let cfg = TestConfig {
            name: "bad-weights".into(),
            description: "x".into(),
            variants: vec![
                VariantConfig {
                    model_id: "m1".into(),
                    weight: 0.9,
                    metadata: HashMap::new(),
                },
                VariantConfig {
                    model_id: "m2".into(),
                    weight: 0.01,
                    metadata: HashMap::new(),
                },
            ],
            metrics_to_track: vec![],
            min_samples: 100,
            confidence_threshold: 0.95,
            max_duration_secs: 86400,
        };
        let result = fw.create_test(cfg);
        assert!(result.is_err());
    }

    // 6. Get test returns snapshot
    #[test]
    fn test_get_test() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        let fetched = fw.get_test(&snap.id).unwrap();
        assert_eq!(fetched.id, snap.id);
        assert_eq!(fetched.name, snap.name);
    }

    // 7. Get non-existent test returns None
    #[test]
    fn test_get_test_not_found() {
        let fw = make_framework();
        assert!(fw.get_test("nonexistent").is_none());
    }

    // 8. Start test transitions Draft -> Running
    #[test]
    fn test_start_test() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        let started = fw.start_test(&snap.id).unwrap();
        assert_eq!(started.status, TestStatus::Running);
        assert!(started.started_at.is_some());
    }

    // 9. Start test from wrong state fails
    #[test]
    fn test_start_test_wrong_state() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        fw.start_test(&snap.id).unwrap();
        let err = fw.start_test(&snap.id);
        assert!(err.is_err());
    }

    // 10. Pause test
    #[test]
    fn test_pause_test() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        fw.start_test(&snap.id).unwrap();
        let paused = fw.pause_test(&snap.id).unwrap();
        assert_eq!(paused.status, TestStatus::Paused);
    }

    // 11. Pause from wrong state fails
    #[test]
    fn test_pause_test_wrong_state() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        let err = fw.pause_test(&snap.id);
        assert!(err.is_err());
    }

    // 12. Resume test
    #[test]
    fn test_resume_test() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        fw.start_test(&snap.id).unwrap();
        fw.pause_test(&snap.id).unwrap();
        let resumed = fw.resume_test(&snap.id).unwrap();
        assert_eq!(resumed.status, TestStatus::Running);
    }

    // 13. Cancel test
    #[test]
    fn test_cancel_test() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        let cancelled = fw.cancel_test(&snap.id).unwrap();
        assert_eq!(cancelled.status, TestStatus::Cancelled);
    }

    // 14. Cancel completed test fails
    #[test]
    fn test_cancel_completed_test_fails() {
        let fw = make_framework();
        let cfg = TestConfig {
            name: "cancel-me".into(),
            description: "x".into(),
            variants: vec![
                VariantConfig {
                    model_id: "m1".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
                VariantConfig {
                    model_id: "m2".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
            ],
            metrics_to_track: vec![],
            min_samples: 1,
            confidence_threshold: 0.95,
            max_duration_secs: 86400,
        };
        let snap = fw.create_test(cfg).unwrap();
        fw.start_test(&snap.id).unwrap();
        fw.complete_test(&snap.id).unwrap();
        let err = fw.cancel_test(&snap.id);
        assert!(err.is_err());
    }

    // 15. Record metric
    #[test]
    fn test_record_metric() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        fw.start_test(&snap.id).unwrap();
        let variant_id = &snap.variants[0].id;
        fw.record_metric(&snap.id, variant_id, "latency", 42.0)
            .unwrap();
        let fetched = fw.get_test(&snap.id).unwrap();
        assert_eq!(fetched.variants[0].samples, 1);
        assert_eq!(
            fetched.variants[0].metrics.get("latency").copied(),
            Some(42.0)
        );
    }

    // 16. Record metric on non-running test fails
    #[test]
    fn test_record_metric_wrong_state() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        let variant_id = &snap.variants[0].id;
        let err = fw.record_metric(&snap.id, variant_id, "latency", 1.0);
        assert!(err.is_err());
    }

    // 17. Record metric with invalid variant fails
    #[test]
    fn test_record_metric_invalid_variant() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        fw.start_test(&snap.id).unwrap();
        let err = fw.record_metric(&snap.id, "no-such-variant", "latency", 1.0);
        assert!(err.is_err());
    }

    // 18. Complete test determines winner
    #[test]
    fn test_complete_test() {
        let fw = make_framework();
        let cfg = TestConfig {
            name: "complete-test".into(),
            description: "x".into(),
            variants: vec![
                VariantConfig {
                    model_id: "m1".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
                VariantConfig {
                    model_id: "m2".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
            ],
            metrics_to_track: vec!["latency".into()],
            min_samples: 1,
            confidence_threshold: 0.95,
            max_duration_secs: 86400,
        };
        let snap = fw.create_test(cfg).unwrap();
        fw.start_test(&snap.id).unwrap();

        // Record metrics for both variants
        let v1_id = snap.variants[0].id.clone();
        let v2_id = snap.variants[1].id.clone();

        for _ in 0..20 {
            fw.record_metric(&snap.id, &v1_id, "latency", 100.0).unwrap();
        }
        for _ in 0..20 {
            fw.record_metric(&snap.id, &v2_id, "latency", 50.0).unwrap();
        }

        let completed = fw.complete_test(&snap.id).unwrap();
        assert_eq!(completed.status, TestStatus::Completed);
        assert!(completed.winner.is_some());
        assert!(completed.completed_at.is_some());
        assert_eq!(fw.total_completed.load(Ordering::Relaxed), 1);
    }

    // 19. Rollback test
    #[test]
    fn test_rollback_test() {
        let fw = make_framework();
        let cfg = TestConfig {
            name: "rollback-test".into(),
            description: "x".into(),
            variants: vec![
                VariantConfig {
                    model_id: "m1".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
                VariantConfig {
                    model_id: "m2".into(),
                    weight: 0.5,
                    metadata: HashMap::new(),
                },
            ],
            metrics_to_track: vec![],
            min_samples: 1,
            confidence_threshold: 0.95,
            max_duration_secs: 86400,
        };
        let snap = fw.create_test(cfg).unwrap();
        fw.start_test(&snap.id).unwrap();
        fw.complete_test(&snap.id).unwrap();
        let rolled_back = fw.rollback_test(&snap.id).unwrap();
        assert_eq!(rolled_back.status, TestStatus::RolledBack);
        assert_eq!(fw.total_completed.load(Ordering::Relaxed), 0);
        assert_eq!(fw.total_rolled_back.load(Ordering::Relaxed), 1);
    }

    // 20. Rollback non-completed test fails
    #[test]
    fn test_rollback_wrong_state() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        let err = fw.rollback_test(&snap.id);
        assert!(err.is_err());
    }

    // 21. List tests
    #[test]
    fn test_list_tests() {
        let fw = make_framework();
        fw.create_test(sample_config()).unwrap();
        fw.create_test(sample_config()).unwrap();
        let list = fw.list_tests();
        assert_eq!(list.len(), 2);
    }

    // 22. List tests by status
    #[test]
    fn test_list_tests_by_status() {
        let fw = make_framework();
        let s1 = fw.create_test(sample_config()).unwrap();
        let s2 = fw.create_test(sample_config()).unwrap();
        fw.start_test(&s1.id).unwrap();
        // s2 stays Draft
        let running = fw.list_tests_by_status(&TestStatus::Running);
        let drafts = fw.list_tests_by_status(&TestStatus::Draft);
        assert_eq!(running.len(), 1);
        assert_eq!(drafts.len(), 1);
    }

    // 23. Get metrics snapshot
    #[test]
    fn test_get_metrics() {
        let fw = make_framework();
        fw.create_test(sample_config()).unwrap();
        let metrics = fw.get_metrics();
        assert_eq!(metrics.total_tests, 1);
        assert_eq!(metrics.running_tests, 0);
        assert_eq!(metrics.completed_tests, 0);
        assert_eq!(metrics.total_samples, 0);
    }

    // 24. Simulate p-value (no difference)
    #[test]
    fn test_simulate_p_value_no_difference() {
        let p = ABTestingFramework::simulate_p_value(100.0, 10.0, 100.0, 10.0, 100);
        assert!(p > 0.5, "p-value should be high when means are equal, got {}", p);
    }

    // 25. Simulate p-value (large difference)
    #[test]
    fn test_simulate_p_value_large_difference() {
        let p = ABTestingFramework::simulate_p_value(100.0, 10.0, 50.0, 10.0, 1000);
        assert!(p < 0.05, "p-value should be low for large difference, got {}", p);
    }

    // 26. Simulate p-value edge cases
    #[test]
    fn test_simulate_p_value_edge_cases() {
        // n = 0
        let p = ABTestingFramework::simulate_p_value(1.0, 1.0, 2.0, 1.0, 0);
        assert_eq!(p, 1.0);

        // zero std
        let p = ABTestingFramework::simulate_p_value(1.0, 0.0, 1.0, 0.0, 100);
        assert_eq!(p, 1.0);
    }

    // 27. Normal CDF approximation sanity
    #[test]
    fn test_normal_cdf() {
        assert!(ABTestingFramework::normal_cdf(0.0) > 0.49);
        assert!(ABTestingFramework::normal_cdf(0.0) < 0.51);
        assert!(ABTestingFramework::normal_cdf(-10.0) < 0.01);
        assert!(ABTestingFramework::normal_cdf(10.0) > 0.99);
    }

    // 28. TestVariant snapshot
    #[test]
    fn test_variant_snapshot() {
        let tv = TestVariant::new("v1".into(), "model-a".into(), 0.5);
        tv.record_sample();
        tv.add_metric("latency", 42.0);
        let snap = tv.snapshot();
        assert_eq!(snap.id, "v1");
        assert_eq!(snap.model_id, "model-a");
        assert_eq!(snap.weight, 0.5);
        assert_eq!(snap.samples, 1);
        assert_eq!(snap.metrics.get("latency").copied(), Some(42.0));
    }

    // 29. Determine winner with insufficient samples
    #[test]
    fn test_determine_winner_insufficient_samples() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        fw.start_test(&snap.id).unwrap();
        // No metrics recorded, so samples = 0 < min_samples (10)
        let winner = fw.determine_winner(&snap.id).unwrap();
        assert!(winner.is_none());
    }

    // 30. TestStatus derive traits
    #[test]
    fn test_status_traits() {
        let s1 = TestStatus::Draft;
        let s2 = s1.clone();
        assert_eq!(s1, s2);
        let _ = format!("{:?}", s1);
    }

    // 31. ABTestSnapshot derive traits
    #[test]
    fn test_snapshot_traits() {
        let snap = ABTestSnapshot {
            id: "id".into(),
            name: "name".into(),
            description: "desc".into(),
            status: TestStatus::Draft,
            variants: vec![],
            winner: None,
            started_at: None,
            completed_at: None,
            results: vec![],
        };
        let cloned = snap.clone();
        assert_eq!(snap.id, cloned.id);
        let _ = format!("{:?}", snap);
    }

    // 32. Multiple metrics tracking
    #[test]
    fn test_multiple_metrics() {
        let fw = make_framework();
        let snap = fw.create_test(sample_config()).unwrap();
        fw.start_test(&snap.id).unwrap();
        let vid = &snap.variants[0].id;
        fw.record_metric(&snap.id, vid, "latency", 100.0).unwrap();
        fw.record_metric(&snap.id, vid, "accuracy", 0.95).unwrap();
        fw.record_metric(&snap.id, vid, "user_satisfaction", 4.5).unwrap();
        let fetched = fw.get_test(&snap.id).unwrap();
        assert_eq!(fetched.variants[0].samples, 3);
    }

    // 33. Weight tolerance (0.97 is close enough to 1.0)
    #[test]
    fn test_weight_tolerance() {
        let fw = make_framework();
        let cfg = TestConfig {
            name: "tolerance".into(),
            description: "x".into(),
            variants: vec![
                VariantConfig {
                    model_id: "m1".into(),
                    weight: 0.48,
                    metadata: HashMap::new(),
                },
                VariantConfig {
                    model_id: "m2".into(),
                    weight: 0.49,
                    metadata: HashMap::new(),
                },
            ],
            metrics_to_track: vec![],
            min_samples: 100,
            confidence_threshold: 0.95,
            max_duration_secs: 86400,
        };
        // 0.48 + 0.49 = 0.97, within 0.05 of 1.0
        let result = fw.create_test(cfg);
        assert!(result.is_ok());
    }
}
