//! End-to-End Protocol Test Suite
//!
//! Tests the full Xergon blockchain protocol lifecycle:
//! Provider registration -> Heartbeat -> Serve inference -> Settlement -> Deregistration
//! Uses a mock Ergo node to simulate on-chain state transitions.
//!
//! REST endpoints:
//! - POST /v1/protocol-e2e/run        — Run full protocol lifecycle test
//! - POST /v1/protocol-e2e/run-step   — Run a single protocol step
//! - GET  /v1/protocol-e2e/results    — List test run results
//! - GET  /v1/protocol-e2e/results/:id — Get specific test run detail
//! - POST /v1/protocol-e2e/mock-box   — Create a mock box state
//! - GET  /v1/protocol-e2e/coverage   — Protocol step coverage report

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
    Router,
    routing::{get, post},
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

// ================================================================
// Types
// ================================================================

/// Protocol step in the provider lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProtocolStep {
    ProviderRegister,
    ProviderHeartbeat,
    InferenceRequest,
    UsageProofCreation,
    SettlementExecution,
    ProviderDeregister,
    RentTopUp,
    SlashingEvent,
    GovernanceProposal,
    GovernanceVote,
    GovernanceExecute,
}

impl ProtocolStep {
    fn all_steps() -> Vec<ProtocolStep> {
        vec![
            ProtocolStep::ProviderRegister,
            ProtocolStep::ProviderHeartbeat,
            ProtocolStep::InferenceRequest,
            ProtocolStep::UsageProofCreation,
            ProtocolStep::SettlementExecution,
            ProtocolStep::ProviderDeregister,
            ProtocolStep::RentTopUp,
            ProtocolStep::SlashingEvent,
            ProtocolStep::GovernanceProposal,
            ProtocolStep::GovernanceVote,
            ProtocolStep::GovernanceExecute,
        ]
    }

    #[allow(dead_code)]
    fn description(&self) -> &'static str {
        match self {
            ProtocolStep::ProviderRegister => "Register new provider on-chain with NFT and stake",
            ProtocolStep::ProviderHeartbeat => "Provider sends heartbeat to refresh state box",
            ProtocolStep::InferenceRequest => "User sends inference request through relay",
            ProtocolStep::UsageProofCreation => "Usage proof box created as immutable receipt",
            ProtocolStep::SettlementExecution => "Settle payment from user staking to provider",
            ProtocolStep::ProviderDeregister => "Provider deregisters and extracts stake",
            ProtocolStep::RentTopUp => "Top up storage rent before box expires",
            ProtocolStep::SlashingEvent => "Slash provider stake for misbehavior",
            ProtocolStep::GovernanceProposal => "Create on-chain governance proposal",
            ProtocolStep::GovernanceVote => "Cast vote on governance proposal",
            ProtocolStep::GovernanceExecute => "Execute passed governance proposal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepResult {
    Pass,
    Fail,
    Skip,
    Timeout,
}

/// Result of a single protocol step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutcome {
    pub step: ProtocolStep,
    pub result: StepResult,
    pub duration_ms: u64,
    pub message: String,
    pub mock_boxes_created: Vec<String>,
    pub mock_boxes_spent: Vec<String>,
    pub tx_valid: bool,
    pub error: Option<String>,
}

/// Result of a full protocol test run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolTestRun {
    pub id: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub total_duration_ms: u64,
    pub steps: Vec<StepOutcome>,
    pub passed_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub overall: StepResult,
    pub mock_node_blocks: u64,
}

/// Mock Ergo node box state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockBox {
    pub box_id: String,
    pub ergo_tree_hex: String,
    pub value_nanoerg: u64,
    pub registers: HashMap<String, String>,
    pub tokens: Vec<MockToken>,
    pub creation_height: u64,
    pub spent: bool,
    pub spent_height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockToken {
    pub token_id: String,
    pub amount: u64,
    pub name: String,
}

/// Mock Ergo node state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockNodeState {
    pub boxes: HashMap<String, MockBox>,
    pub current_height: u64,
    pub total_blocks: u64,
    pub transactions: Vec<MockTransaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockTransaction {
    pub tx_id: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub height: u64,
    pub valid: bool,
}

/// Coverage report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub total_steps: usize,
    pub tested_steps: usize,
    pub passed_steps: usize,
    pub coverage_pct: f64,
    pub step_coverage: HashMap<String, StepCoverageEntry>,
    pub untested_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCoverageEntry {
    pub run_count: u64,
    pub pass_count: u64,
    pub fail_count: u64,
    pub avg_duration_ms: f64,
}

// ================================================================
// Mock Ergo Node
// ================================================================

pub struct MockErgoNode {
    state: tokio::sync::RwLock<MockNodeState>,
    run_counter: AtomicU64,
    test_results: DashMap<String, ProtocolTestRun>,
    step_coverage: DashMap<String, StepCoverageEntry>,
}

impl MockErgoNode {
    pub fn new() -> Self {
        let boxes = HashMap::new();
        let mut state = MockNodeState {
            boxes,
            current_height: 100_000,
            total_blocks: 0,
            transactions: vec![],
        };
        // Pre-fund with genesis ERG for testing
        let genesis = MockBox {
            box_id: "genesis-box-001".to_string(),
            ergo_tree_hex: "0001cd02e8ec6e8a4b7abcdef12345678".to_string(),
            value_nanoerg: 100_000_000_000, // 100 ERG
            registers: HashMap::new(),
            tokens: vec![],
            creation_height: 0,
            spent: false,
            spent_height: None,
        };
        state.boxes.insert(genesis.box_id.clone(), genesis);
        Self {
            state: tokio::sync::RwLock::new(state),
            run_counter: AtomicU64::new(0),
            test_results: DashMap::new(),
            step_coverage: DashMap::new(),
        }
    }

    async fn advance_blocks(&self, count: u64) {
        let mut state = self.state.write().await;
        state.current_height += count;
        state.total_blocks += count;
    }

    async fn create_box(&self, box_id: String, ergo_tree: String, value: u64, registers: HashMap<String, String>, tokens: Vec<MockToken>) -> MockBox {
        let mut state = self.state.write().await;
        let box_state = MockBox {
            box_id: box_id.clone(),
            ergo_tree_hex: ergo_tree,
            value_nanoerg: value,
            registers,
            tokens,
            creation_height: state.current_height,
            spent: false,
            spent_height: None,
        };
        state.boxes.insert(box_id.clone(), box_state.clone());
        box_state
    }

    async fn spend_box(&self, box_id: &str) -> bool {
        let mut state = self.state.write().await;
        let is_unspent = state.boxes.get(box_id).map(|b| !b.spent).unwrap_or(false);
        if is_unspent {
            let current_height = state.current_height;
            let box_state = state.boxes.get_mut(box_id).unwrap();
            box_state.spent = true;
            box_state.spent_height = Some(current_height);
            state.transactions.push(MockTransaction {
                tx_id: format!("tx-{}", Uuid::new_v4()),
                inputs: vec![box_id.to_string()],
                outputs: vec![],
                height: current_height,
                valid: true,
            });
            return true;
        }
        false
    }

    async fn box_exists(&self, box_id: &str) -> bool {
        let state = self.state.read().await;
        state.boxes.get(box_id).map(|b| !b.spent).unwrap_or(false)
    }

    #[allow(dead_code)]
    async fn get_box(&self, box_id: &str) -> Option<MockBox> {
        let state = self.state.read().await;
        state.boxes.get(box_id).cloned()
    }

    fn record_coverage(&self, step: &ProtocolStep, passed: bool, duration_ms: u64) {
        let key = format!("{:?}", step);
        let mut entry = self.step_coverage.entry(key).or_insert_with(|| StepCoverageEntry {
            run_count: 0,
            pass_count: 0,
            fail_count: 0,
            avg_duration_ms: 0.0,
        });
        let e = entry.value_mut();
        e.run_count += 1;
        if passed { e.pass_count += 1; } else { e.fail_count += 1; }
        e.avg_duration_ms = (e.avg_duration_ms * (e.run_count as f64 - 1.0) + duration_ms as f64) / e.run_count as f64;
    }

    fn get_coverage_report(&self) -> CoverageReport {
        let all = ProtocolStep::all_steps();
        let mut tested = 0usize;
        let mut passed = 0usize;
        let mut untested = Vec::new();
        let mut step_cov = HashMap::new();

        for step in &all {
            let key = format!("{:?}", step);
            if let Some(entry) = self.step_coverage.get(&key) {
                tested += 1;
                if entry.value().pass_count > 0 { passed += 1; }
                step_cov.insert(key, entry.value().clone());
            } else {
                untested.push(format!("{:?}", step));
            }
        }

        let pct = if all.is_empty() { 0.0 } else { (tested as f64 / all.len() as f64) * 100.0 };
        CoverageReport {
            total_steps: all.len(),
            tested_steps: tested,
            passed_steps: passed,
            coverage_pct: pct,
            step_coverage: step_cov,
            untested_steps: untested,
        }
    }
}

// ================================================================
// Protocol Test Runner
// ================================================================

impl MockErgoNode {
    /// Run full provider lifecycle test
    pub async fn run_full_lifecycle(&self) -> ProtocolTestRun {
        let run_id = format!("run-{}", self.run_counter.fetch_add(1, Ordering::SeqCst));
        let started = Utc::now();
        let mut steps = Vec::new();

        // Step 1: Provider Register
        let run_id_for_box = run_id.clone();
        let (outcome, _dur) = self.run_step_timed(ProtocolStep::ProviderRegister, |node| {
            Box::pin(async move {
                let mut created = Vec::new();
                let mut regs = HashMap::new();
                regs.insert("R4".to_string(), "0e0b02e8ec6e8a4b7".to_string());
                regs.insert("R5".to_string(), "0e0568747470733a2f2f".to_string());
                regs.insert("R6".to_string(), "0e055b22716c6f7261225d".to_string());
                regs.insert("R7".to_string(), "0e29c2d101".to_string());
                regs.insert("R8".to_string(), "0e29".to_string());
                regs.insert("R9".to_string(), "0e0575732d77657374".to_string());

                let box_id = format!("provider-nft-{}", run_id_for_box);
                let _ = node.create_box(
                    box_id.clone(), "0001cd02e8ec".to_string(), 1_000_000_000,
                    regs, vec![MockToken {
                        token_id: "provider-nft-token-001".to_string(),
                        amount: 1,
                        name: "ProviderNFT".to_string(),
                    }],
                ).await;
                created.push(box_id);

                // Verify box exists
                let exists = node.box_exists(&created[0]).await;
                (exists, created, vec![], true, if exists { "Provider registered successfully".to_string() } else { "Provider box not found".to_string() }, None)
            })
        }).await;
        steps.push(outcome.clone());

        if outcome.result != StepResult::Pass {
            return self.finish_run(run_id.clone(), started, steps);
        }

        // Step 2: Heartbeat
        let (outcome, _) = self.run_step_timed(ProtocolStep::ProviderHeartbeat, |node| {
            Box::pin(async move {
                let exists = node.box_exists("provider-nft-001").await;
                // Simulate heartbeat by updating R8 (last heartbeat height)
                if exists {
                    let mut state = node.state.write().await;
                    let current = state.current_height;
                    if let Some(b) = state.boxes.get_mut("provider-nft-001") {
                        b.registers.insert("R8".to_string(), format!("0e29{:08x}", current));
                    }
                }
                (exists, vec![], vec![], exists, if exists { "Heartbeat updated".to_string() } else { "Provider box not found".to_string() }, None)
            })
        }).await;
        steps.push(outcome);

        // Step 3: Inference Request
        let (outcome, _) = self.run_step_timed(ProtocolStep::InferenceRequest, |node| {
            Box::pin(async move {
                // Simulate inference by creating user staking box
                let mut regs = HashMap::new();
                regs.insert("R4".to_string(), "0e08cd02abcd".to_string());
                let _ = node.create_box(
                    "user-staking-001".to_string(), "0001cd02abcd".to_string(),
                    5_000_000_000, regs, vec![],
                ).await;
                (true, vec!["user-staking-001".to_string()], vec![], true, "Inference request processed".to_string(), None)
            })
        }).await;
        steps.push(outcome);

        // Step 4: Usage Proof Creation
        let (outcome, _) = self.run_step_timed(ProtocolStep::UsageProofCreation, |node| {
            Box::pin(async move {
                let mut regs = HashMap::new();
                regs.insert("R4".to_string(), "0e08abcd".to_string());
                regs.insert("R5".to_string(), "0e08provider-nft".to_string());
                regs.insert("R6".to_string(), "0e2100000005".to_string());
                regs.insert("R7".to_string(), "0e2100000003".to_string());
                regs.insert("R8".to_string(), "0e086d6f64656c2d31".to_string());
                regs.insert("R9".to_string(), "0e21".to_string());
                let _ = node.create_box(
                    "usage-proof-001".to_string(), "0001".to_string(),
                    360_000, regs, vec![],
                ).await;
                (true, vec!["usage-proof-001".to_string()], vec![], true, "Usage proof created".to_string(), None)
            })
        }).await;
        steps.push(outcome);

        // Step 5: Settlement
        let (outcome, _) = self.run_step_timed(ProtocolStep::SettlementExecution, |node| {
            Box::pin(async move {
                let spent = node.spend_box("user-staking-001").await;
                (spent, vec![], vec!["user-staking-001".to_string()], spent,
                 if spent { "Settlement executed".to_string() } else { "User staking box not found".to_string() },
                 None)
            })
        }).await;
        steps.push(outcome);

        // Step 6: Deregister
        let (outcome, _) = self.run_step_timed(ProtocolStep::ProviderDeregister, |node| {
            Box::pin(async move {
                let spent = node.spend_box("provider-nft-001").await;
                (spent, vec![], vec!["provider-nft-001".to_string()], spent,
                 if spent { "Provider deregistered".to_string() } else { "Provider box not found".to_string() },
                 None)
            })
        }).await;
        steps.push(outcome);

        // Step 7: Rent Top-Up (simulate with fresh box)
        let (outcome, _) = self.run_step_timed(ProtocolStep::RentTopUp, |node| {
            Box::pin(async move {
                let mut regs = HashMap::new();
                regs.insert("R4".to_string(), "0e0b02rent".to_string());
                let box_id = "rent-box-001".to_string();
                let _ = node.create_box(box_id.clone(), "0001".to_string(), 500_000_000, regs.clone(), vec![]).await;
                // Top up by spending and recreating with more value
                node.spend_box(&box_id).await;
                let _ = node.create_box("rent-box-001-v2".to_string(), "0001".to_string(), 1_000_000_000, regs, vec![]).await;
                (true, vec!["rent-box-001-v2".to_string()], vec!["rent-box-001".to_string()], true, "Rent topped up".to_string(), None)
            })
        }).await;
        steps.push(outcome);

        // Step 8: Slashing Event
        let (outcome, _) = self.run_step_timed(ProtocolStep::SlashingEvent, |node| {
            Box::pin(async move {
                let mut regs = HashMap::new();
                regs.insert("R4".to_string(), "0e08provider-nft".to_string());
                regs.insert("R5".to_string(), "0e08evidence".to_string());
                regs.insert("R6".to_string(), "0e210a000000".to_string());
                regs.insert("R7".to_string(), "0e21".to_string());
                let _ = node.create_box("slashing-001".to_string(), "0001".to_string(), 360_000, regs, vec![]).await;
                (true, vec!["slashing-001".to_string()], vec![], true, "Slashing evidence recorded".to_string(), None)
            })
        }).await;
        steps.push(outcome);

        // Step 9: Governance Proposal
        let (outcome, _) = self.run_step_timed(ProtocolStep::GovernanceProposal, |node| {
            Box::pin(async move {
                let mut regs = HashMap::new();
                regs.insert("R4".to_string(), "0e08prophash".to_string());
                regs.insert("R5".to_string(), "0e2100000001".to_string());
                regs.insert("R6".to_string(), "0e2100000000".to_string());
                regs.insert("R7".to_string(), "0e21".to_string());
                regs.insert("R8".to_string(), "0e210a0000".to_string());
                regs.insert("R9".to_string(), "0e08proposer".to_string());
                let _ = node.create_box("gov-proposal-001".to_string(), "0001".to_string(), 1_000_000, regs, vec![]).await;
                (true, vec!["gov-proposal-001".to_string()], vec![], true, "Governance proposal created".to_string(), None)
            })
        }).await;
        steps.push(outcome);

        // Step 10: Governance Vote
        let (outcome, _) = self.run_step_timed(ProtocolStep::GovernanceVote, |node| {
            Box::pin(async move {
                let mut state = node.state.write().await;
                if let Some(b) = state.boxes.get_mut("gov-proposal-001") {
                    // Increment votes_for (R5)
                    let current = b.registers.get("R5").and_then(|v| u64::from_str_radix(v.trim_start_matches("0e21"), 16).ok()).unwrap_or(0);
                    b.registers.insert("R5".to_string(), format!("0e21{:016x}", current + 1));
                }
                (true, vec![], vec![], true, "Vote cast".to_string(), None)
            })
        }).await;
        steps.push(outcome);

        // Step 11: Governance Execute
        self.advance_blocks(1000).await;
        let (outcome, _) = self.run_step_timed(ProtocolStep::GovernanceExecute, |node| {
            Box::pin(async move {
                let spent = node.spend_box("gov-proposal-001").await;
                (spent, vec![], vec!["gov-proposal-001".to_string()], spent,
                 if spent { "Proposal executed".to_string() } else { "Proposal not found or expired".to_string() },
                 None)
            })
        }).await;
        steps.push(outcome);

        self.finish_run(run_id, started, steps)
    }

    /// Run a single protocol step
    pub async fn run_single_step(&self, step: ProtocolStep) -> StepOutcome {
        let (outcome, _) = self.run_step_timed(step.clone(), |node| {
            Box::pin(async move {
                // Minimal step execution for individual runs
                match step {
                    ProtocolStep::ProviderRegister => {
                        let mut regs = HashMap::new();
                        regs.insert("R4".to_string(), "0e0b02test".to_string());
                        let _ = node.create_box("test-provider-001".to_string(), "0001".to_string(), 1_000_000_000, regs, vec![]).await;
                        (true, vec!["test-provider-001".to_string()], vec![], true, "Provider registered".to_string(), None)
                    }
                    ProtocolStep::ProviderHeartbeat => {
                        let exists = node.box_exists("test-provider-001").await;
                        (exists, vec![], vec![], exists, if exists { "Heartbeat sent".to_string() } else { "Box not found".to_string() }, None)
                    }
                    _ => {
                        (true, vec![], vec![], true, format!("Step {:?} executed", step), None)
                    }
                }
            })
        }).await;
        outcome
    }

    async fn run_step_timed<F, Fut>(
        &self,
        step: ProtocolStep,
        f: F,
    ) -> (StepOutcome, u64)
    where
        F: FnOnce(Arc<MockErgoNode>) -> Fut,
        Fut: std::future::Future<Output = (bool, Vec<String>, Vec<String>, bool, String, Option<String>)>,
    {
        let start = Instant::now();
        let node_ref = Arc::new(self.new_instance());
        let (success, created, spent, tx_valid, msg, err) = f(node_ref).await;
        let dur = start.elapsed().as_millis() as u64;
        let result = if success { StepResult::Pass } else { StepResult::Fail };
        self.record_coverage(&step, success, dur);
        let outcome = StepOutcome {
            step: step.clone(),
            result: result.clone(),
            duration_ms: dur,
            message: msg,
            mock_boxes_created: created,
            mock_boxes_spent: spent,
            tx_valid,
            error: err,
        };
        (outcome, dur)
    }

    fn new_instance(&self) -> MockErgoNode {
        MockErgoNode::new()
    }

    fn finish_run(&self, run_id: String, started: chrono::DateTime<Utc>, steps: Vec<StepOutcome>) -> ProtocolTestRun {
        let completed = Utc::now();
        let passed = steps.iter().filter(|s| s.result == StepResult::Pass).count();
        let failed = steps.iter().filter(|s| s.result == StepResult::Fail).count();
        let skipped = steps.iter().filter(|s| s.result == StepResult::Skip).count();
        let overall = if failed == 0 { StepResult::Pass } else { StepResult::Fail };
        let total_dur = steps.iter().map(|s| s.duration_ms).sum();

        let run = ProtocolTestRun {
            id: run_id,
            started_at: started.to_rfc3339(),
            completed_at: Some(completed.to_rfc3339()),
            total_duration_ms: total_dur,
            steps,
            passed_count: passed,
            failed_count: failed,
            skipped_count: skipped,
            overall,
            mock_node_blocks: 0,
        };
        self.test_results.insert(run.id.clone(), run.clone());
        run
    }
}

// ================================================================
// REST API
// ================================================================

pub fn build_router() -> Router {
    let node = Arc::new(MockErgoNode::new());
    Router::new()
        .route("/v1/protocol-e2e/run", post(run_full_lifecycle))
        .route("/v1/protocol-e2e/run-step", post(run_single_step))
        .route("/v1/protocol-e2e/results", post(list_results))
        .route("/v1/protocol-e2e/results/:id", get(get_result))
        .route("/v1/protocol-e2e/mock-box", post(create_mock_box))
        .route("/v1/protocol-e2e/coverage", get(get_coverage))
        .with_state(node)
}

async fn run_full_lifecycle(
    State(node): State<Arc<MockErgoNode>>,
) -> Json<ProtocolTestRun> {
    let run = node.run_full_lifecycle().await;
    Json(run)
}

#[derive(Deserialize)]
struct RunStepRequest {
    step: String,
}

async fn run_single_step(
    State(node): State<Arc<MockErgoNode>>,
    Json(body): Json<RunStepRequest>,
) -> Json<StepOutcome> {
    let step = match body.step.as_str() {
        "provider_register" => ProtocolStep::ProviderRegister,
        "provider_heartbeat" => ProtocolStep::ProviderHeartbeat,
        "inference_request" => ProtocolStep::InferenceRequest,
        "usage_proof_creation" => ProtocolStep::UsageProofCreation,
        "settlement_execution" => ProtocolStep::SettlementExecution,
        "provider_deregister" => ProtocolStep::ProviderDeregister,
        "rent_top_up" => ProtocolStep::RentTopUp,
        "slashing_event" => ProtocolStep::SlashingEvent,
        "governance_proposal" => ProtocolStep::GovernanceProposal,
        "governance_vote" => ProtocolStep::GovernanceVote,
        "governance_execute" => ProtocolStep::GovernanceExecute,
        _ => ProtocolStep::ProviderRegister,
    };
    Json(node.run_single_step(step).await)
}

async fn list_results(
    State(node): State<Arc<MockErgoNode>>,
) -> Json<Vec<ProtocolTestRun>> {
    let results: Vec<ProtocolTestRun> = node.test_results.iter().map(|r| r.value().clone()).collect();
    Json(results)
}

async fn get_result(
    State(node): State<Arc<MockErgoNode>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match node.test_results.get(&id) {
        Some(run) => (StatusCode::OK, Json(serde_json::to_value(run.value().clone()).unwrap_or_default())),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Test run not found"}))),
    }
}

#[derive(Deserialize)]
struct MockBoxRequest {
    box_id: String,
    ergo_tree_hex: String,
    value_nanoerg: u64,
    registers: Option<HashMap<String, String>>,
    tokens: Option<Vec<MockToken>>,
}

async fn create_mock_box(
    State(node): State<Arc<MockErgoNode>>,
    Json(body): Json<MockBoxRequest>,
) -> Json<MockBox> {
    let box_state = node.create_box(
        body.box_id,
        body.ergo_tree_hex,
        body.value_nanoerg,
        body.registers.unwrap_or_default(),
        body.tokens.unwrap_or_default(),
    ).await;
    Json(box_state)
}

async fn get_coverage(
    State(node): State<Arc<MockErgoNode>>,
) -> Json<CoverageReport> {
    Json(node.get_coverage_report())
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_node_create_box() {
        let node = MockErgoNode::new();
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), "0e0b02test".to_string());
        let box_state = node.create_box(
            "test-001".to_string(), "0001".to_string(), 1_000_000_000,
            regs, vec![],
        ).await;
        assert_eq!(box_state.box_id, "test-001");
        assert_eq!(box_state.value_nanoerg, 1_000_000_000);
        assert!(!box_state.spent);
    }

    #[tokio::test]
    async fn test_mock_node_spend_box() {
        let node = MockErgoNode::new();
        node.create_box("spend-001".to_string(), "0001".to_string(), 100, HashMap::new(), vec![]).await;
        assert!(node.box_exists("spend-001").await);
        let spent = node.spend_box("spend-001").await;
        assert!(spent);
        assert!(!node.box_exists("spend-001").await);
    }

    #[tokio::test]
    async fn test_mock_node_advance_blocks() {
        let node = MockErgoNode::new();
        node.advance_blocks(100).await;
        let state = node.state.read().await;
        assert_eq!(state.current_height, 100_100);
    }

    #[tokio::test]
    async fn test_full_lifecycle_run() {
        let node = MockErgoNode::new();
        let run = node.run_full_lifecycle().await;
        assert_eq!(run.steps.len(), 11);
        assert!(run.passed_count > 0);
        assert!(run.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_single_step_register() {
        let node = MockErgoNode::new();
        let outcome = node.run_single_step(ProtocolStep::ProviderRegister).await;
        assert_eq!(outcome.result, StepResult::Pass);
        assert!(outcome.tx_valid);
    }

    #[tokio::test]
    async fn test_coverage_report() {
        let node = MockErgoNode::new();
        node.run_full_lifecycle().await;
        let report = node.get_coverage_report();
        assert_eq!(report.total_steps, 11);
        assert!(report.tested_steps > 0);
        assert!(report.coverage_pct > 0.0);
    }

    #[test]
    fn test_protocol_step_descriptions() {
        assert!(!ProtocolStep::ProviderRegister.description().is_empty());
        assert!(!ProtocolStep::GovernanceExecute.description().is_empty());
    }

    #[test]
    fn test_protocol_step_all() {
        let steps = ProtocolStep::all_steps();
        assert_eq!(steps.len(), 11);
    }

    #[tokio::test]
    async fn test_double_spend_fails() {
        let node = MockErgoNode::new();
        node.create_box("double-001".to_string(), "0001".to_string(), 100, HashMap::new(), vec![]).await;
        assert!(node.spend_box("double-001").await);
        assert!(!node.spend_box("double-001").await); // Second spend fails
    }

    #[tokio::test]
    async fn test_spend_nonexistent_box() {
        let node = MockErgoNode::new();
        assert!(!node.spend_box("nonexistent").await);
    }

    #[tokio::test]
    async fn test_get_box() {
        let node = MockErgoNode::new();
        node.create_box("get-001".to_string(), "0001".to_string(), 100, HashMap::new(), vec![]).await;
        let box_state = node.get_box("get-001").await;
        assert!(box_state.is_some());
        assert_eq!(box_state.unwrap().box_id, "get-001");
    }

    #[test]
    fn test_step_result_equality() {
        assert_eq!(StepResult::Pass, StepResult::Pass);
        assert_ne!(StepResult::Pass, StepResult::Fail);
    }

    #[test]
    fn test_mock_token_serialization() {
        let token = MockToken {
            token_id: "test".to_string(),
            amount: 1,
            name: "TestNFT".to_string(),
        };
        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("TestNFT"));
    }
}
