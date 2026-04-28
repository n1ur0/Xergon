//! End-to-End Integration Test Suite
//!
//! Provides a comprehensive E2E testing framework for Xergon Network's
//! inference pipeline: user request -> relay routing -> provider selection ->
//! inference -> result return -> cost settlement.
//!
//! REST endpoints:
//! - POST /v1/e2e/run         — Run a single test or full suite
//! - GET  /v1/e2e/results     — List all test results
//! - GET  /v1/e2e/results/:id — Get a specific test result
//! - GET  /v1/e2e/suite/:cat  — Run or list tests by category
//! - POST /v1/e2e/retry       — Retry failed tests
//! - GET  /v1/e2e/export      — Export results as JSON

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Test status outcomes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestStatus {
    Pass,
    Fail,
    Skip,
    Error,
}

/// Supported test categories for the E2E suite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TestCategory {
    ContractDeploy,
    InferenceFlow,
    Settlement,
    WalletConnect,
    ProviderRegistration,
}

impl std::fmt::Display for TestCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestCategory::ContractDeploy => write!(f, "contract_deploy"),
            TestCategory::InferenceFlow => write!(f, "inference_flow"),
            TestCategory::Settlement => write!(f, "settlement"),
            TestCategory::WalletConnect => write!(f, "wallet_connect"),
            TestCategory::ProviderRegistration => write!(f, "provider_registration"),
        }
    }
}

impl std::str::FromStr for TestCategory {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contract_deploy" => Ok(TestCategory::ContractDeploy),
            "inference_flow" => Ok(TestCategory::InferenceFlow),
            "settlement" => Ok(TestCategory::Settlement),
            "wallet_connect" => Ok(TestCategory::WalletConnect),
            "provider_registration" => Ok(TestCategory::ProviderRegistration),
            _ => Err(format!("Unknown test category: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// TestStep
// ---------------------------------------------------------------------------

/// A single step within an E2E test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    pub step_id: String,
    pub name: String,
    pub expected_outcome: String,
    pub actual_outcome: String,
    pub status: TestStatus,
    pub duration_ms: u64,
}

impl TestStep {
    pub fn new(step_id: impl Into<String>, name: impl Into<String>, expected: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            name: name.into(),
            expected_outcome: expected.into(),
            actual_outcome: String::new(),
            status: TestStatus::Skip,
            duration_ms: 0,
        }
    }

    pub fn pass(mut self, actual: impl Into<String>, duration_ms: u64) -> Self {
        self.actual_outcome = actual.into();
        self.status = TestStatus::Pass;
        self.duration_ms = duration_ms;
        self
    }

    pub fn fail(mut self, actual: impl Into<String>, duration_ms: u64) -> Self {
        self.actual_outcome = actual.into();
        self.status = TestStatus::Fail;
        self.duration_ms = duration_ms;
        self
    }

    pub fn error(mut self, actual: impl Into<String>, duration_ms: u64) -> Self {
        self.actual_outcome = actual.into();
        self.status = TestStatus::Error;
        self.duration_ms = duration_ms;
        self
    }
}

// ---------------------------------------------------------------------------
// E2ETestResult
// ---------------------------------------------------------------------------

/// Result of a single E2E test execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2ETestResult {
    pub test_id: String,
    pub category: TestCategory,
    pub status: TestStatus,
    pub duration_ms: u64,
    pub output: String,
    pub error_message: Option<String>,
    pub steps: Vec<TestStep>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// E2ETestConfig
// ---------------------------------------------------------------------------

/// Configuration for the E2E test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2ETestConfig {
    /// Which categories to run.
    pub categories: Vec<TestCategory>,
    /// Timeout per test in seconds.
    pub timeout_secs: u64,
    /// Whether to run tests in parallel within a category.
    pub parallel: bool,
}

impl Default for E2ETestConfig {
    fn default() -> Self {
        Self {
            categories: vec![
                TestCategory::ContractDeploy,
                TestCategory::InferenceFlow,
                TestCategory::Settlement,
                TestCategory::WalletConnect,
                TestCategory::ProviderRegistration,
            ],
            timeout_secs: 120,
            parallel: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Registered test definition
// ---------------------------------------------------------------------------

/// A registered test in the suite.
struct RegisteredTest {
    category: TestCategory,
    name: String,
    runner: fn() -> Pin<Box<dyn Future<Output = E2ETestResult> + Send>>,
}

use std::pin::Pin;
use std::future::Future;

// ---------------------------------------------------------------------------
// E2ETestSuite
// ---------------------------------------------------------------------------

/// The main E2E test suite manager.
pub struct E2ETestSuite {
    /// Registry of registered tests: test_id -> RegisteredTest
    tests: DashMap<String, RegisteredTest>,
    /// History of all test results: test_id -> list of results
    results: DashMap<String, Vec<E2ETestResult>>,
    /// Configuration
    config: RwLock<E2ETestConfig>,
    /// Counter for generating test IDs
    #[allow(dead_code)]
    test_counter: AtomicU64,
}

impl E2ETestSuite {
    /// Create a new empty test suite.
    pub fn new() -> Self {
        Self {
            tests: DashMap::new(),
            results: DashMap::new(),
            config: RwLock::new(E2ETestConfig::default()),
            test_counter: AtomicU64::new(0),
        }
    }

    /// Create with a custom configuration.
    pub fn with_config(config: E2ETestConfig) -> Self {
        Self {
            tests: DashMap::new(),
            results: DashMap::new(),
            config: RwLock::new(config),
            test_counter: AtomicU64::new(0),
        }
    }

    /// Register a new test.
    pub fn register_test(
        &self,
        test_id: impl Into<String>,
        category: TestCategory,
        name: impl Into<String>,
        runner: fn() -> Pin<Box<dyn Future<Output = E2ETestResult> + Send>>,
    ) {
        let id = test_id.into();
        self.tests.insert(id.clone(), RegisteredTest {
            category,
            name: name.into(),
            runner,
        });
        debug!(test_id = %id, "E2E test registered");
    }

    /// Run a single test by ID.
    pub async fn run_test(&self, test_id: &str) -> Result<E2ETestResult, String> {
        let entry = self.tests.get(test_id)
            .ok_or_else(|| format!("Test not found: {}", test_id))?;

        let runner = entry.value().runner;
        drop(entry);

        let cfg = self.config.read().await;
        let timeout = Duration::from_secs(cfg.timeout_secs);
        drop(cfg);

        let result = tokio::time::timeout(timeout, runner()).await;

        let result = match result {
            Ok(mut r) => {
                r.test_id = test_id.to_string();
                r
            }
            Err(_) => {
                E2ETestResult {
                    test_id: test_id.to_string(),
                    category: TestCategory::InferenceFlow,
                    status: TestStatus::Error,
                    duration_ms: timeout.as_millis() as u64,
                    output: String::new(),
                    error_message: Some(format!("Test timed out after {}s", timeout.as_secs())),
                    steps: vec![],
                    timestamp: Utc::now(),
                }
            }
        };

        self.results.entry(test_id.to_string())
            .or_default()
            .push(result.clone());

        info!(
            test_id = %test_id,
            status = ?result.status,
            duration_ms = result.duration_ms,
            "E2E test completed"
        );

        Ok(result)
    }

    /// Run all tests in the suite.
    pub async fn run_suite(&self) -> Vec<E2ETestResult> {
        let cfg = self.config.read().await;
        let parallel = cfg.parallel;
        drop(cfg);

        let test_ids: Vec<String> = self.tests.iter()
            .map(|entry| entry.key().clone())
            .collect();

        let mut results = Vec::new();

        if parallel {
            let mut handles = Vec::new();
            for id in test_ids {
                let _suite = self as *const E2ETestSuite as usize;
                let id_clone = id.clone();
                // We cannot capture &self across tasks, so we use a simpler approach
                // and just run sequentially when parallel is false
                handles.push(tokio::spawn(async move {
                    // Placeholder - actual parallelism would need Arc<E2ETestSuite>
                    id_clone
                }));
            }
            for handle in handles {
                if let Ok(id) = handle.await {
                    if let Ok(result) = self.run_test(&id).await {
                        results.push(result);
                    }
                }
            }
        } else {
            for id in test_ids {
                match self.run_test(&id).await {
                    Ok(result) => results.push(result),
                    Err(e) => warn!(test_id = %id, error = %e, "Failed to run test"),
                }
            }
        }

        info!(total = results.len(), "E2E suite run completed");
        results
    }

    /// Get all results for a test ID.
    pub fn get_results(&self, test_id: &str) -> Vec<E2ETestResult> {
        self.results.get(test_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Get the latest result for each test.
    pub fn get_latest_results(&self) -> Vec<E2ETestResult> {
        self.results.iter()
            .filter_map(|entry| entry.value().last().cloned())
            .collect()
    }

    /// Get results filtered by category.
    pub fn get_results_by_category(&self, category: &TestCategory) -> Vec<E2ETestResult> {
        self.results.iter()
            .filter(|entry| {
                if let Some(r) = entry.value().last() {
                    r.category == *category
                } else {
                    false
                }
            })
            .filter_map(|entry| entry.value().last().cloned())
            .collect()
    }

    /// Retry all previously failed tests.
    pub async fn retry_failed(&self) -> Vec<E2ETestResult> {
        let failed_ids: Vec<String> = self.results.iter()
            .filter(|entry| {
                entry.value().last()
                    .map(|r| r.status == TestStatus::Fail || r.status == TestStatus::Error)
                    .unwrap_or(false)
            })
            .map(|entry| entry.key().clone())
            .collect();

        info!(count = failed_ids.len(), "Retrying failed E2E tests");

        let mut results = Vec::new();
        for id in failed_ids {
            match self.run_test(&id).await {
                Ok(result) => results.push(result),
                Err(e) => warn!(test_id = %id, error = %e, "Retry failed"),
            }
        }
        results
    }

    /// Export all results as a serializable map.
    pub fn export_results(&self) -> HashMap<String, Vec<E2ETestResult>> {
        let mut export = HashMap::new();
        for entry in self.results.iter() {
            export.insert(entry.key().clone(), entry.value().clone());
        }
        export
    }

    /// Update the configuration.
    pub async fn update_config(&self, config: E2ETestConfig) {
        let mut cfg = self.config.write().await;
        *cfg = config;
    }

    /// Get the current configuration.
    pub async fn get_config(&self) -> E2ETestConfig {
        self.config.read().await.clone()
    }

    /// Get all registered test IDs.
    pub fn list_tests(&self) -> Vec<(String, TestCategory, String)> {
        self.tests.iter()
            .map(|entry| {
                (entry.key().clone(), entry.value().category.clone(), entry.value().name.clone())
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Mock test runners — simulate full inference flow
// ---------------------------------------------------------------------------

/// Mock test that simulates a full inference flow:
/// user request -> relay routing -> provider selection -> inference -> result return -> cost settlement
async fn mock_inference_flow_test() -> E2ETestResult {
    let start = Instant::now();
    let mut steps = Vec::new();

    // Step 1: User request submission
    let step_start = Instant::now();
    let user_request_step = TestStep::new("step-1", "User request submission", "Request accepted by relay");
    let user_request_result = simulate_user_request().await;
    let step1 = match user_request_result {
        Ok(msg) => user_request_step.pass(msg, step_start.elapsed().as_millis() as u64),
        Err(msg) => user_request_step.fail(msg, step_start.elapsed().as_millis() as u64),
    };
    steps.push(step1);

    // Step 2: Relay routing
    let step_start = Instant::now();
    let relay_step = TestStep::new("step-2", "Relay routing", "Request routed to available provider");
    let relay_result = simulate_relay_routing().await;
    let step2 = match relay_result {
        Ok(msg) => relay_step.pass(msg, step_start.elapsed().as_millis() as u64),
        Err(msg) => relay_step.fail(msg, step_start.elapsed().as_millis() as u64),
    };
    steps.push(step2);

    // Step 3: Provider selection
    let step_start = Instant::now();
    let selection_step = TestStep::new("step-3", "Provider selection", "Best provider selected based on latency/reputation");
    let selection_result = simulate_provider_selection().await;
    let step3 = match selection_result {
        Ok(msg) => selection_step.pass(msg, step_start.elapsed().as_millis() as u64),
        Err(msg) => selection_step.fail(msg, step_start.elapsed().as_millis() as u64),
    };
    steps.push(step3);

    // Step 4: Model inference
    let step_start = Instant::now();
    let inference_step = TestStep::new("step-4", "Model inference", "Inference completed successfully");
    let inference_result = simulate_inference().await;
    let step4 = match inference_result {
        Ok(msg) => inference_step.pass(msg, step_start.elapsed().as_millis() as u64),
        Err(msg) => inference_step.fail(msg, step_start.elapsed().as_millis() as u64),
    };
    steps.push(step4);

    // Step 5: Result return
    let step_start = Instant::now();
    let result_step = TestStep::new("step-5", "Result return", "Result delivered to user");
    let result_returned = simulate_result_return().await;
    let step5 = match result_returned {
        Ok(msg) => result_step.pass(msg, step_start.elapsed().as_millis() as u64),
        Err(msg) => result_step.fail(msg, step_start.elapsed().as_millis() as u64),
    };
    steps.push(step5);

    // Step 6: Cost settlement
    let step_start = Instant::now();
    let settlement_step = TestStep::new("step-6", "Cost settlement", "Settlement recorded on-chain");
    let settlement_result = simulate_cost_settlement().await;
    let step6 = match settlement_result {
        Ok(msg) => settlement_step.pass(msg, step_start.elapsed().as_millis() as u64),
        Err(msg) => settlement_step.fail(msg, step_start.elapsed().as_millis() as u64),
    };
    steps.push(step6);

    let all_passed = steps.iter().all(|s| s.status == TestStatus::Pass);
    let output = steps.iter()
        .map(|s| format!("{}: {} ({})", s.step_id, s.name, match s.status {
            TestStatus::Pass => "PASS",
            TestStatus::Fail => "FAIL",
            TestStatus::Skip => "SKIP",
            TestStatus::Error => "ERROR",
        }))
        .collect::<Vec<_>>()
        .join("; ");

    E2ETestResult {
        test_id: String::new(), // filled by run_test
        category: TestCategory::InferenceFlow,
        status: if all_passed { TestStatus::Pass } else { TestStatus::Fail },
        duration_ms: start.elapsed().as_millis() as u64,
        output,
        error_message: if all_passed { None } else { Some("One or more steps failed".into()) },
        steps,
        timestamp: Utc::now(),
    }
}

async fn mock_contract_deploy_test() -> E2ETestResult {
    let start = Instant::now();
    let mut steps = Vec::new();

    let step = TestStep::new("step-1", "Contract compilation", "Contract compiled successfully");
    let step1 = step.pass("P2S script generated: sigma-rust v0.28", 15);
    steps.push(step1);

    let step = TestStep::new("step-2", "Contract registration", "Contract box created on-chain");
    let step2 = step.pass("Box ID: abc123...registered", 200);
    steps.push(step2);

    let all_passed = steps.iter().all(|s| s.status == TestStatus::Pass);

    E2ETestResult {
        test_id: String::new(),
        category: TestCategory::ContractDeploy,
        status: if all_passed { TestStatus::Pass } else { TestStatus::Fail },
        duration_ms: start.elapsed().as_millis() as u64,
        output: "Contract deployment flow completed".into(),
        error_message: None,
        steps,
        timestamp: Utc::now(),
    }
}

async fn mock_settlement_test() -> E2ETestResult {
    let start = Instant::now();
    let mut steps = Vec::new();

    let step = TestStep::new("step-1", "Usage proof generation", "Proof generated");
    let step1 = step.pass("Merkle root: deadbeef...", 10);
    steps.push(step1);

    let step = TestStep::new("step-2", "Settlement execution", "Funds transferred");
    let step2 = step.pass("100 nanoERG settled to provider", 150);
    steps.push(step2);

    let all_passed = steps.iter().all(|s| s.status == TestStatus::Pass);

    E2ETestResult {
        test_id: String::new(),
        category: TestCategory::Settlement,
        status: if all_passed { TestStatus::Pass } else { TestStatus::Fail },
        duration_ms: start.elapsed().as_millis() as u64,
        output: "Settlement flow completed".into(),
        error_message: None,
        steps,
        timestamp: Utc::now(),
    }
}

async fn mock_wallet_connect_test() -> E2ETestResult {
    let start = Instant::now();
    let mut steps = Vec::new();

    let step = TestStep::new("step-1", "Wallet connection", "Ergo node connected");
    let step1 = step.pass("Node height: 847291, synced: true", 50);
    steps.push(step1);

    let step = TestStep::new("step-2", "Address verification", "Address matches config");
    let step2 = step.pass("Address: 9f...ZK", 5);
    steps.push(step2);

    let all_passed = steps.iter().all(|s| s.status == TestStatus::Pass);

    E2ETestResult {
        test_id: String::new(),
        category: TestCategory::WalletConnect,
        status: if all_passed { TestStatus::Pass } else { TestStatus::Fail },
        duration_ms: start.elapsed().as_millis() as u64,
        output: "Wallet connection flow completed".into(),
        error_message: None,
        steps,
        timestamp: Utc::now(),
    }
}

async fn mock_provider_registration_test() -> E2ETestResult {
    let start = Instant::now();
    let mut steps = Vec::new();

    let step = TestStep::new("step-1", "Provider info submission", "Info submitted to relay");
    let step1 = step.pass("Provider registered with relay", 100);
    steps.push(step1);

    let step = TestStep::new("step-2", "On-chain registration", "Registration box created");
    let step2 = step.pass("Registration box: reg-001", 200);
    steps.push(step2);

    let all_passed = steps.iter().all(|s| s.status == TestStatus::Pass);

    E2ETestResult {
        test_id: String::new(),
        category: TestCategory::ProviderRegistration,
        status: if all_passed { TestStatus::Pass } else { TestStatus::Fail },
        duration_ms: start.elapsed().as_millis() as u64,
        output: "Provider registration flow completed".into(),
        error_message: None,
        steps,
        timestamp: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Mock simulation helpers
// ---------------------------------------------------------------------------

async fn simulate_user_request() -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(5)).await;
    Ok("Request accepted by relay (id: req-001)".into())
}

async fn simulate_relay_routing() -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(10)).await;
    Ok("Routed to provider-42 (latency: 12ms)".into())
}

async fn simulate_provider_selection() -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(8)).await;
    Ok("Selected provider-42 (reputation: 0.98, GPU: A100)".into())
}

async fn simulate_inference() -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(25)).await;
    Ok("Inference complete (tokens: 42 in, 128 out, latency: 45ms)".into())
}

async fn simulate_result_return() -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(3)).await;
    Ok("Result delivered to user (200 OK)".into())
}

async fn simulate_cost_settlement() -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(7)).await;
    Ok("Settlement: 50 nanoERG transferred (tx: tx-abc)".into())
}

// ---------------------------------------------------------------------------
// Helper: wrap async mock tests into the fn pointer signature
// ---------------------------------------------------------------------------

fn wrap_inference_flow() -> Pin<Box<dyn Future<Output = E2ETestResult> + Send>> {
    Box::pin(mock_inference_flow_test())
}

fn wrap_contract_deploy() -> Pin<Box<dyn Future<Output = E2ETestResult> + Send>> {
    Box::pin(mock_contract_deploy_test())
}

fn wrap_settlement() -> Pin<Box<dyn Future<Output = E2ETestResult> + Send>> {
    Box::pin(mock_settlement_test())
}

fn wrap_wallet_connect() -> Pin<Box<dyn Future<Output = E2ETestResult> + Send>> {
    Box::pin(mock_wallet_connect_test())
}

fn wrap_provider_registration() -> Pin<Box<dyn Future<Output = E2ETestResult> + Send>> {
    Box::pin(mock_provider_registration_test())
}

/// Create a fully populated E2ETestSuite with all default mock tests registered.
pub fn create_default_suite() -> Arc<E2ETestSuite> {
    let suite = Arc::new(E2ETestSuite::new());
    suite.register_test("inference-flow", TestCategory::InferenceFlow, "Full inference flow", wrap_inference_flow);
    suite.register_test("contract-deploy", TestCategory::ContractDeploy, "Contract deployment", wrap_contract_deploy);
    suite.register_test("settlement", TestCategory::Settlement, "Cost settlement", wrap_settlement);
    suite.register_test("wallet-connect", TestCategory::WalletConnect, "Wallet connectivity", wrap_wallet_connect);
    suite.register_test("provider-registration", TestCategory::ProviderRegistration, "Provider registration", wrap_provider_registration);
    suite
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RunTestRequest {
    pub test_id: Option<String>,
    pub category: Option<String>,
    pub config: Option<E2ETestConfig>,
}

#[derive(Debug, Serialize)]
pub struct RunTestResponse {
    pub results: Vec<E2ETestResult>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub errors: usize,
}

#[derive(Debug, Deserialize)]
pub struct RetryRequest {
    pub max_retries: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub exported_at: chrono::DateTime<chrono::Utc>,
    pub total_tests: usize,
    pub results: HashMap<String, Vec<E2ETestResult>>,
}

#[derive(Debug, Serialize)]
pub struct TestListResponse {
    pub tests: Vec<TestInfo>,
}

#[derive(Debug, Serialize)]
pub struct TestInfo {
    pub test_id: String,
    pub category: TestCategory,
    pub name: String,
    pub latest_status: Option<TestStatus>,
    pub latest_duration_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn run_e2e_handler(
    State(state): State<Arc<E2ETestSuite>>,
    Json(body): Json<RunTestRequest>,
) -> Result<Json<RunTestResponse>, StatusCode> {
    if let Some(config) = body.config {
        state.update_config(config).await;
    }

    let results = if let Some(test_id) = body.test_id {
        match state.run_test(&test_id).await {
            Ok(result) => vec![result],
            Err(e) => return Err(StatusCode::NOT_FOUND.with_body(e)),
        }
    } else if let Some(category_str) = body.category {
        let category: TestCategory = category_str.parse().map_err(|_| {
            StatusCode::BAD_REQUEST.with_body(format!("Invalid category: {}", category_str))
        })?;
        let all = state.get_results_by_category(&category);
        if all.is_empty() {
            // Run the suite to populate results
            state.run_suite().await
        } else {
            all
        }
    } else {
        state.run_suite().await
    };

    let passed = results.iter().filter(|r| r.status == TestStatus::Pass).count();
    let failed = results.iter().filter(|r| r.status == TestStatus::Fail).count();
    let errors = results.iter().filter(|r| r.status == TestStatus::Error).count();

    Ok(Json(RunTestResponse {
        total: results.len(),
        passed,
        failed,
        errors,
        results,
    }))
}

async fn get_results_handler(
    State(state): State<Arc<E2ETestSuite>>,
) -> Json<RunTestResponse> {
    let results = state.get_latest_results();
    let passed = results.iter().filter(|r| r.status == TestStatus::Pass).count();
    let failed = results.iter().filter(|r| r.status == TestStatus::Fail).count();
    let errors = results.iter().filter(|r| r.status == TestStatus::Error).count();

    Json(RunTestResponse {
        total: results.len(),
        passed,
        failed,
        errors,
        results,
    })
}

async fn get_result_by_id_handler(
    State(state): State<Arc<E2ETestSuite>>,
    Path(test_id): Path<String>,
) -> Result<Json<Vec<E2ETestResult>>, StatusCode> {
    let results = state.get_results(&test_id);
    if results.is_empty() {
        Err(StatusCode::NOT_FOUND.with_body(format!("No results for test: {}", test_id)))
    } else {
        Ok(Json(results))
    }
}

async fn get_suite_by_category_handler(
    State(state): State<Arc<E2ETestSuite>>,
    Path(category): Path<String>,
) -> Result<Json<RunTestResponse>, StatusCode> {
    let cat: TestCategory = category.parse().map_err(|e: String| {
        StatusCode::BAD_REQUEST.with_body(e)
    })?;
    let results = state.get_results_by_category(&cat);
    let passed = results.iter().filter(|r| r.status == TestStatus::Pass).count();
    let failed = results.iter().filter(|r| r.status == TestStatus::Fail).count();
    let errors = results.iter().filter(|r| r.status == TestStatus::Error).count();

    Ok(Json(RunTestResponse {
        total: results.len(),
        passed,
        failed,
        errors,
        results,
    }))
}

async fn retry_handler(
    State(state): State<Arc<E2ETestSuite>>,
    _body: Json<RetryRequest>,
) -> Json<RunTestResponse> {
    let results = state.retry_failed().await;
    let passed = results.iter().filter(|r| r.status == TestStatus::Pass).count();
    let failed = results.iter().filter(|r| r.status == TestStatus::Fail).count();
    let errors = results.iter().filter(|r| r.status == TestStatus::Error).count();

    Json(RunTestResponse {
        total: results.len(),
        passed,
        failed,
        errors,
        results,
    })
}

async fn export_handler(
    State(state): State<Arc<E2ETestSuite>>,
) -> Json<ExportResponse> {
    let results = state.export_results();
    let total = results.len();

    Json(ExportResponse {
        exported_at: Utc::now(),
        total_tests: total,
        results,
    })
}

// Helper trait for StatusCode with body
trait WithBody {
    fn with_body(self, body: impl Into<String>) -> Self;
}

impl WithBody for StatusCode {
    fn with_body(self, _body: impl Into<String>) -> Self {
        self
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the E2E integration test router.
pub fn build_e2e_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{get, post};

    let suite = state.e2e_suite.clone();

    axum::Router::new()
        .route("/v1/e2e/run", post(run_e2e_handler))
        .route("/v1/e2e/results", get(get_results_handler))
        .route("/v1/e2e/results/{test_id}", get(get_result_by_id_handler))
        .route("/v1/e2e/suite/{category}", get(get_suite_by_category_handler))
        .route("/v1/e2e/retry", post(retry_handler))
        .route("/v1/e2e/export", get(export_handler))
        .with_state(suite)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_suite() -> Arc<E2ETestSuite> {
        let suite = Arc::new(E2ETestSuite::new());
        suite.register_test("test-1", TestCategory::InferenceFlow, "Test inference", wrap_inference_flow);
        suite.register_test("test-2", TestCategory::Settlement, "Test settlement", wrap_settlement);
        suite
    }

    #[tokio::test]
    async fn test_register_test() {
        let suite = E2ETestSuite::new();
        suite.register_test("my-test", TestCategory::InferenceFlow, "My test", wrap_inference_flow);
        assert!(suite.tests.contains_key("my-test"));
        let tests = suite.list_tests();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].0, "my-test");
    }

    #[tokio::test]
    async fn test_run_single_test() {
        let suite = create_test_suite();
        let result = suite.run_test("test-1").await.unwrap();
        assert_eq!(result.category, TestCategory::InferenceFlow);
        assert_eq!(result.status, TestStatus::Pass);
        assert!(!result.steps.is_empty());
    }

    #[tokio::test]
    async fn test_run_suite() {
        let suite = create_test_suite();
        let results = suite.run_suite().await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.status == TestStatus::Pass));
    }

    #[tokio::test]
    async fn test_result_retrieval() {
        let suite = create_test_suite();
        suite.run_test("test-1").await.unwrap();
        let results = suite.get_results("test-1");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].test_id, "test-1");
    }

    #[tokio::test]
    async fn test_latest_results() {
        let suite = create_test_suite();
        suite.run_suite().await;
        let latest = suite.get_latest_results();
        assert_eq!(latest.len(), 2);
    }

    #[tokio::test]
    async fn test_retry_failed() {
        let suite = create_test_suite();
        // Run tests (all pass), then retry (no failures)
        suite.run_suite().await;
        let retried = suite.retry_failed().await;
        assert_eq!(retried.len(), 0);
    }

    #[tokio::test]
    async fn test_export_results() {
        let suite = create_test_suite();
        suite.run_suite().await;
        let export = suite.export_results();
        assert_eq!(export.len(), 2);
        assert!(export.contains_key("test-1"));
        assert!(export.contains_key("test-2"));
    }

    #[tokio::test]
    async fn test_category_filtering() {
        let suite = create_test_suite();
        suite.run_suite().await;
        let inference_results = suite.get_results_by_category(&TestCategory::InferenceFlow);
        assert_eq!(inference_results.len(), 1);
        let settlement_results = suite.get_results_by_category(&TestCategory::Settlement);
        assert_eq!(settlement_results.len(), 1);
    }

    #[tokio::test]
    async fn test_timeout_handling() {
        let suite = Arc::new(E2ETestSuite::with_config(E2ETestConfig {
            categories: vec![TestCategory::InferenceFlow],
            timeout_secs: 0, // Immediate timeout
            parallel: false,
        }));
        suite.register_test("timeout-test", TestCategory::InferenceFlow, "Timeout test", wrap_inference_flow);
        let result = suite.run_test("timeout-test").await.unwrap();
        assert_eq!(result.status, TestStatus::Error);
        assert!(result.error_message.is_some());
        assert!(result.error_message.as_ref().unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        let suite = Arc::new(E2ETestSuite::with_config(E2ETestConfig {
            categories: vec![TestCategory::InferenceFlow],
            timeout_secs: 30,
            parallel: true,
        }));
        suite.register_test("p-test-1", TestCategory::InferenceFlow, "Parallel 1", wrap_inference_flow);
        suite.register_test("p-test-2", TestCategory::Settlement, "Parallel 2", wrap_settlement);
        let results = suite.run_suite().await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_step_tracking() {
        let suite = create_test_suite();
        let result = suite.run_test("test-1").await.unwrap();
        assert!(!result.steps.is_empty());
        // The inference flow mock has 6 steps
        assert!(result.steps.len() >= 6);
        for step in &result.steps {
            assert!(!step.step_id.is_empty());
            assert!(!step.name.is_empty());
            assert!(!step.expected_outcome.is_empty());
        }
    }
}
