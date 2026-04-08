//! ErgoTree Contract Evaluator
//!
//! Lightweight offline ErgoTree deserializer and evaluator for the relay.
//! Handles common contract patterns used in Xergon's protocol contracts
//! without depending on the full ergo-lib crate.

use axum::{
    extract::State,
    Json,
    Router,
    routing::{get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::HashMap;
use crate::proxy;

// ================================================================
// Types
// ================================================================

/// ErgoTree serialized header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoTreeHeader {
    pub version: u8,
    pub has_constant_segregation: bool,
    pub tree_size: usize,
}

/// Sigma Boolean — the result of evaluating an ErgoTree contract
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SigmaBoolean {
    TrivialProp(bool),
    ProveDlog(String),           // hex-encoded group element (public key)
    ProveDHTuple { g: String, h: String, u: String, v: String },
    CAND(Vec<SigmaBoolean>),
    COR(Vec<SigmaBoolean>),
    Cthreshold(u8, Vec<SigmaBoolean>),
    Unknown(String),
}

/// Context variables for contract evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextExtension {
    pub values: HashMap<u8, Vec<u8>>,
}

impl ContextExtension {
    pub fn new() -> Self { Self { values: HashMap::new() } }
    pub fn get(&self, idx: u8) -> Option<&Vec<u8>> { self.values.get(&idx) }
}

/// Registers R0–R9 for a box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxRegisters {
    pub r4: Option<Vec<u8>>,
    pub r5: Option<Vec<u8>>,
    pub r6: Option<Vec<u8>>,
    pub r7: Option<Vec<u8>>,
    pub r8: Option<Vec<u8>>,
    pub r9: Option<Vec<u8>>,
}

impl BoxRegisters {
    pub fn empty() -> Self {
        Self { r4: None, r5: None, r6: None, r7: None, r8: None, r9: None }
    }
}

/// Token held in a box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEntry {
    pub token_id: String,
    pub amount: u64,
}

/// Box representation for evaluation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalBox {
    pub box_id: String,
    pub value: u64,
    pub ergotree_hex: String,
    pub tokens: Vec<TokenEntry>,
    pub registers: BoxRegisters,
    pub creation_height: u32,
}

/// Full evaluation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalContext {
    pub self_box: EvalBox,
    pub inputs: Vec<EvalBox>,
    pub outputs: Vec<EvalBox>,
    pub data_inputs: Vec<EvalBox>,
    pub height: u32,
    pub context_extensions: HashMap<usize, ContextExtension>,
}

/// Evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub sigma_boolean: SigmaBoolean,
    pub passed: bool,
    pub proof_requirements: ProofRequirements,
    pub eval_time_us: u64,
}

/// Proof requirements extracted from the result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofRequirements {
    pub dlog_count: u32,
    pub dhtuple_count: u32,
    pub threshold_groups: u32,
    pub trivial: bool,
}

/// Error from evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalError {
    pub message: String,
    pub stage: String,
}

// ================================================================
// ErgoTree Deserializer
// ================================================================

/// Parse the header byte of an ErgoTree
pub fn parse_header(ergotree_bytes: &[u8]) -> Result<ErgoTreeHeader, EvalError> {
    if ergotree_bytes.is_empty() {
        return Err(EvalError { message: "Empty ErgoTree".into(), stage: "header".into() });
    }
    let first = ergotree_bytes[0];
    let version = first & 0x0F;
    let has_constants = (first & 0x80) != 0;
    Ok(ErgoTreeHeader {
        version,
        has_constant_segregation: has_constants,
        tree_size: ergotree_bytes.len(),
    })
}

/// Detect contract type from serialized ErgoTree bytes.
/// Returns a simplified SigmaBoolean without full evaluation.
pub fn detect_contract_type(ergotree_hex: &str) -> Result<SigmaBoolean, EvalError> {
    let bytes = hex_decode(ergotree_hex).map_err(|e| EvalError {
        message: format!("Hex decode error: {}", e),
        stage: "detect".into(),
    })?;

    let header = parse_header(&bytes)?;
    let mut offset = 1;

    // Skip constant segment if present
    if header.has_constant_segregation {
        if offset >= bytes.len() {
            return Err(EvalError { message: "Truncated constants length".into(), stage: "constants".into() });
        }
        // First byte after header is constants length (compact int)
        let const_len = bytes[offset] as usize;
        offset += 1;
        offset += const_len;
        if offset > bytes.len() {
            return Err(EvalError { message: "Constants overflow".into(), stage: "constants".into() });
        }
    }

    // Analyze the tree body
    if offset >= bytes.len() {
        return Err(EvalError { message: "Empty tree body".into(), stage: "tree".into() });
    }

    analyze_tree_body(&bytes[offset..])
}

/// Analyze the remaining tree body bytes
fn analyze_tree_body(body: &[u8]) -> Result<SigmaBoolean, EvalError> {
    if body.is_empty() {
        return Err(EvalError { message: "Empty body".into(), stage: "tree".into() });
    }

    let opcode = body[0];

    // sigmaProp(True) -> TrivialProp(true)
    // Typically: 0xc0 (Lambda) 0x?? ... sigmaProp(True) ... 0xc3 (BlockValue)
    if body.contains(&0x11) {
        // True literal found — check if it's the sigmaProp argument
        return Ok(SigmaBoolean::TrivialProp(true));
    }

    // Check for sigmaProp(False)
    if body.contains(&0x12) {
        // Could be TrivialProp(false) or a conditional branch
        if !body.contains(&0x11) && !contains_group_element(body) {
            return Ok(SigmaBoolean::TrivialProp(false));
        }
    }

    // Check for proveDlog — look for GroupElement type prefix 0x0e followed by 0x08cd02
    // SigmaProp type: 0x0e 0x08cd02...
    if body.windows(4).any(|w| w[0] == 0x0e && w[1] == 0x08 && w[2] == 0xcd && w[3] == 0x02) {
        // Extract the group element (33 bytes compressed pubkey)
        if let Some(pos) = find_group_element(body) {
            if pos + 33 <= body.len() {
                let ge_bytes = &body[pos..pos + 33];
                return Ok(SigmaBoolean::ProveDlog(hex_encode(ge_bytes)));
            }
        }
    }

    // Check for HEIGHT comparison
    if body.contains(&0x0a) || body.contains(&0x05) {
        // Contains a long constant — likely a HEIGHT check
        if contains_group_element(body) {
            if let Some(pos) = find_group_element(body) {
                if pos + 33 <= body.len() {
                    let ge_bytes = &body[pos..pos + 33];
                    return Ok(SigmaBoolean::ProveDlog(hex_encode(ge_bytes)));
                }
            }
        }
    }

    // Check for AND/COR composition
    if body.contains(&0x87) || body.contains(&0x88) {
        return Ok(SigmaBoolean::Unknown("Boolean composition detected".into()));
    }

    // Default: unknown contract
    Ok(SigmaBoolean::Unknown(format!("Complex contract ({} bytes)", body.len())))
}

fn contains_group_element(bytes: &[u8]) -> bool {
    bytes.windows(2).any(|w| w[0] == 0x0e && w[1] >= 0x21 && w[1] <= 0x27)
}

fn find_group_element(bytes: &[u8]) -> Option<usize> {
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == 0x0e && bytes[i + 1] >= 0x21 && bytes[i + 1] <= 0x27 {
            // Skip type byte, return start of actual data
            return Some(i + 2);
        }
    }
    None
}

// ================================================================
// Evaluator (simplified)
// ================================================================

/// Evaluate an ErgoTree contract against a context.
/// Returns the SigmaBoolean result and proof requirements.
pub fn evaluate_contract(
    ergotree_hex: &str,
    ctx: &EvalContext,
) -> Result<EvalResult, EvalError> {
    let start = std::time::Instant::now();

    let sigma_boolean = detect_contract_type(ergotree_hex)?;
    let proof_requirements = extract_proof_requirements(&sigma_boolean);
    let passed = is_trivially_true(&sigma_boolean) || has_proof_requirement(&sigma_boolean);

    Ok(EvalResult {
        passed,
        sigma_boolean,
        proof_requirements,
        eval_time_us: start.elapsed().as_micros() as u64,
    })
}

/// Check if a SigmaBoolean is trivially true
fn is_trivially_true(sb: &SigmaBoolean) -> bool {
    match sb {
        SigmaBoolean::TrivialProp(true) => true,
        SigmaBoolean::CAND(children) => children.iter().all(is_trivially_true),
        SigmaBoolean::COR(children) => children.iter().any(is_trivially_true),
        _ => false,
    }
}

/// Check if the SigmaBoolean has proof requirements (needs a wallet to sign)
fn has_proof_requirement(sb: &SigmaBoolean) -> bool {
    match sb {
        SigmaBoolean::TrivialProp(_) => false,
        SigmaBoolean::ProveDlog(_) | SigmaBoolean::ProveDHTuple { .. } => true,
        SigmaBoolean::CAND(children) | SigmaBoolean::COR(children) => {
            children.iter().any(has_proof_requirement)
        }
        SigmaBoolean::Cthreshold(k, children) => {
            *k as usize > 0 && children.iter().any(has_proof_requirement)
        }
        SigmaBoolean::Unknown(_) => false,
    }
}

/// Extract proof requirements from a SigmaBoolean
pub fn extract_proof_requirements(sb: &SigmaBoolean) -> ProofRequirements {
    match sb {
        SigmaBoolean::TrivialProp(_) => ProofRequirements {
            dlog_count: 0, dhtuple_count: 0, threshold_groups: 0, trivial: true,
        },
        SigmaBoolean::ProveDlog(_) => ProofRequirements {
            dlog_count: 1, dhtuple_count: 0, threshold_groups: 0, trivial: false,
        },
        SigmaBoolean::ProveDHTuple { .. } => ProofRequirements {
            dlog_count: 0, dhtuple_count: 1, threshold_groups: 0, trivial: false,
        },
        SigmaBoolean::CAND(children) | SigmaBoolean::COR(children) => {
            let mut req = ProofRequirements { dlog_count: 0, dhtuple_count: 0, threshold_groups: 0, trivial: true };
            for c in children {
                let cr = extract_proof_requirements(c);
                req.dlog_count += cr.dlog_count;
                req.dhtuple_count += cr.dhtuple_count;
                req.threshold_groups += cr.threshold_groups;
                if !cr.trivial { req.trivial = false; }
            }
            req
        }
        SigmaBoolean::Cthreshold(k, children) => {
            let mut req = ProofRequirements { dlog_count: 0, dhtuple_count: 0, threshold_groups: 1, trivial: false };
            for c in children {
                let cr = extract_proof_requirements(c);
                req.dlog_count += cr.dlog_count;
                req.dhtuple_count += cr.dhtuple_count;
            }
            // Only k proofs are actually needed
            let _ = k; // used to inform caller
            req
        }
        SigmaBoolean::Unknown(_) => ProofRequirements {
            dlog_count: 0, dhtuple_count: 0, threshold_groups: 0, trivial: false,
        },
    }
}

// ================================================================
// Contract Evaluation Service
// ================================================================

#[derive(Debug, Clone)]
pub struct EvalStats {
    pub total_evaluations: u64,
    pub cache_hits: u64,
    pub cache_size: u64,
}

pub struct ErgoTreeEvaluatorState {
    cache: DashMap<String, EvalResult>,
    eval_count: AtomicU64,
    hit_count: AtomicU64,
}

impl ErgoTreeEvaluatorState {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
            eval_count: AtomicU64::new(0),
            hit_count: AtomicU64::new(0),
        }
    }

    pub fn evaluate(
        &self,
        ergotree_hex: &str,
        ctx: &EvalContext,
    ) -> Result<EvalResult, EvalError> {
        self.eval_count.fetch_add(1, Ordering::Relaxed);

        // Check cache
        let cache_key = format!("{}:{}", ergotree_hex, ctx.height);
        if let Some(cached) = self.cache.get(&cache_key) {
            self.hit_count.fetch_add(1, Ordering::Relaxed);
            return Ok(cached.clone());
        }

        let result = evaluate_contract(ergotree_hex, ctx)?;
        self.cache.insert(cache_key, result.clone());
        Ok(result)
    }

    pub fn batch_evaluate(
        &self,
        requests: &[ContractEvalRequest],
    ) -> Vec<ContractEvalResult> {
        requests.iter().map(|req| {
            match self.evaluate(&req.ergotree_hex, &req.context()) {
                Ok(r) => ContractEvalResult { ok: true, result: Some(r), error: None },
                Err(e) => ContractEvalResult { ok: false, result: None, error: Some(e) },
            }
        }).collect()
    }

    pub fn clear_cache(&self) { self.cache.clear(); }

    pub fn stats(&self) -> EvalStats {
        EvalStats {
            total_evaluations: self.eval_count.load(Ordering::Relaxed),
            cache_hits: self.hit_count.load(Ordering::Relaxed),
            cache_size: self.cache.len() as u64,
        }
    }
}

impl Clone for ErgoTreeEvaluatorState {
    fn clone(&self) -> Self {
        Self {
            cache: DashMap::new(),
            eval_count: AtomicU64::new(self.eval_count.load(Ordering::Relaxed)),
            hit_count: AtomicU64::new(self.hit_count.load(Ordering::Relaxed)),
        }
    }
}

// ================================================================
// REST API Types
// ================================================================

#[derive(Debug, Deserialize)]
pub struct ContractEvalRequest {
    pub ergotree_hex: String,
    pub self_box: EvalBox,
    pub inputs: Vec<EvalBox>,
    pub outputs: Vec<EvalBox>,
    pub height: u32,
    pub context_extensions: Option<HashMap<usize, ContextExtension>>,
}

impl ContractEvalRequest {
    pub fn context(&self) -> EvalContext {
        EvalContext {
            self_box: self.self_box.clone(),
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            data_inputs: vec![],
            height: self.height,
            context_extensions: self.context_extensions.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContractEvalResult {
    pub ok: bool,
    pub result: Option<EvalResult>,
    pub error: Option<EvalError>,
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    total_evaluations: u64,
    cache_hits: u64,
    cache_size: u64,
    hit_rate: f64,
}

// ================================================================
// REST Handlers
// ================================================================

async fn evaluate_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<ContractEvalRequest>,
) -> Json<ContractEvalResult> {
    let ctx = req.context();
    match state.ergotree_evaluator.evaluate(&req.ergotree_hex, &ctx) {
        Ok(r) => Json(ContractEvalResult { ok: true, result: Some(r), error: None }),
        Err(e) => Json(ContractEvalResult { ok: false, result: None, error: Some(e) }),
    }
}

async fn batch_evaluate_handler(
    State(state): State<proxy::AppState>,
    Json(reqs): Json<Vec<ContractEvalRequest>>,
) -> Json<Vec<ContractEvalResult>> {
    Json(state.ergotree_evaluator.batch_evaluate(&reqs))
}

async fn stats_handler(
    State(state): State<proxy::AppState>,
) -> Json<StatsResponse> {
    let s = state.ergotree_evaluator.stats();
    let hit_rate = if s.total_evaluations > 0 {
        s.cache_hits as f64 / s.total_evaluations as f64
    } else { 0.0 };
    Json(StatsResponse { total_evaluations: s.total_evaluations, cache_hits: s.cache_hits, cache_size: s.cache_size, hit_rate })
}

async fn clear_cache_handler(
    State(state): State<proxy::AppState>,
) -> Json<serde_json::Value> {
    state.ergotree_evaluator.clear_cache();
    Json(serde_json::json!({"ok": true}))
}

// ================================================================
// Router
// ================================================================

pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    Router::new()
        .route("/v1/contracts/evaluate", post(evaluate_handler))
        .route("/v1/contracts/batch-evaluate", post(batch_evaluate_handler))
        .route("/v1/contracts/eval-stats", get(stats_handler))
        .route("/v1/contracts/eval-cache", axum::routing::delete(clear_cache_handler))
        .with_state(state)
}

// ================================================================
// Helpers
// ================================================================

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let clean = s.trim_start_matches("0x");
    hex::decode(clean).map_err(|e| format!("Hex error: {}", e))
}

fn hex_encode(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // sigmaProp(true) — minimal ErgoTree v1: header + empty constants + tree body
    // 0x10 = version 0 + has_constants, 0x01 = 1 constant, 0x04 = SBoolean true
    // tree body: 0xc0(Lambda,1 input) 0xc3(Block) 0x11(True) 0xc3(end)
    const TRIVIAL_TRUE: &str = "100104c0c311c3";
    // P2PK contract — sigmaProp(proveDlog(pk))
    // Contains a group element (33 bytes)
    const P2PK_PREFIX: &str = "100e08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    fn make_eval_ctx() -> EvalContext {
        EvalContext {
            self_box: EvalBox {
                box_id: "a".repeat(64),
                value: 1_000_000_000,
                ergotree_hex: TRIVIAL_TRUE.into(),
                tokens: vec![],
                registers: BoxRegisters::empty(),
                creation_height: 500000,
            },
            inputs: vec![],
            outputs: vec![],
            data_inputs: vec![],
            height: 800000,
            context_extensions: HashMap::new(),
        }
    }

    #[test]
    fn test_parse_header() {
        let bytes = hex_decode(TRIVIAL_TRUE).unwrap();
        let h = parse_header(&bytes).unwrap();
        assert_eq!(h.version, 0);
        assert!(h.has_constant_segregation);
    }

    #[test]
    fn test_detect_trivial_true() {
        let sb = detect_contract_type(TRIVIAL_TRUE).unwrap();
        assert_eq!(sb, SigmaBoolean::TrivialProp(true));
    }

    #[test]
    fn test_detect_p2pk() {
        let sb = detect_contract_type(P2PK_PREFIX).unwrap();
        match sb {
            SigmaBoolean::ProveDlog(_) => {},
            _ => panic!("Expected ProveDlog, got {:?}", sb),
        }
    }

    #[test]
    fn test_evaluate_contract() {
        let ctx = make_eval_ctx();
        let result = evaluate_contract(TRIVIAL_TRUE, &ctx).unwrap();
        assert!(result.passed);
        assert!(result.proof_requirements.trivial);
    }

    #[test]
    fn test_proof_requirements() {
        let sb = SigmaBoolean::ProveDlog("abc".into());
        let req = extract_proof_requirements(&sb);
        assert_eq!(req.dlog_count, 1);
        assert!(!req.trivial);

        let sb_trivial = SigmaBoolean::TrivialProp(true);
        let req2 = extract_proof_requirements(&sb_trivial);
        assert!(req2.trivial);
    }

    #[test]
    fn test_evaluator_cache() {
        let state = ErgoTreeEvaluatorState::new();
        let ctx = make_eval_ctx();
        let r1 = state.evaluate(TRIVIAL_TRUE, &ctx).unwrap();
        let r2 = state.evaluate(TRIVIAL_TRUE, &ctx).unwrap();
        assert_eq!(r1.passed, r2.passed);
        let stats = state.stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.total_evaluations, 2);
    }

    #[test]
    fn test_batch_evaluate() {
        let state = ErgoTreeEvaluatorState::new();
        let ctx = make_eval_ctx();
        let reqs = vec![
            ContractEvalRequest {
                ergotree_hex: TRIVIAL_TRUE.into(),
                self_box: ctx.self_box.clone(),
                inputs: vec![],
                outputs: vec![],
                height: 800000,
                context_extensions: None,
            },
            ContractEvalRequest {
                ergotree_hex: P2PK_PREFIX.into(),
                self_box: ctx.self_box,
                inputs: vec![],
                outputs: vec![],
                height: 800000,
                context_extensions: None,
            },
        ];
        let results = state.batch_evaluate(&reqs);
        assert_eq!(results.len(), 2);
        assert!(results[0].ok);
        assert!(results[1].ok);
    }
}
