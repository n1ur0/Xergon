//! Proof verification pipeline for the Xergon Network agent.
//!
//! Handles ZK proof submission, verification, batching, receipts,
//! and fraud detection for on-chain proof-of-inference.

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Types of proofs that can be verified.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProofType {
    Compute,
    Usage,
    Stake,
    Reputation,
    Governance,
    Ensemble,
}

impl Default for ProofType {
    fn default() -> Self {
        Self::Compute
    }
}

/// Fraud categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FraudType {
    DoubleSpend,
    StaleProof,
    InvalidClaim,
    Sybil,
    Collusion,
}

/// Severity levels for fraud reports and anomalies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for Severity {
    fn default() -> Self {
        Self::Medium
    }
}

/// Status of a fraud report investigation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Pending,
    Investigating,
    Confirmed,
    Dismissed,
}

impl Default for ReportStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Status of a proof verification batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Pending,
    Verifying,
    Completed,
    Failed,
}

impl Default for BatchStatus {
    fn default() -> Self {
        Self::Pending
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Configuration for the proof verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofConfig {
    pub max_batch_size: usize,
    pub verification_timeout_ms: u64,
    pub fraud_threshold: f64,
    pub auto_verify: bool,
    pub proof_types_enabled: Vec<String>,
}

impl Default for ProofConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            verification_timeout_ms: 5000,
            fraud_threshold: 0.95,
            auto_verify: true,
            proof_types_enabled: vec![
                "compute".into(),
                "usage".into(),
                "stake".into(),
                "reputation".into(),
                "governance".into(),
                "ensemble".into(),
            ],
        }
    }
}

/// A submitted proof awaiting or past verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofSubmission {
    pub proof_id: String,
    pub proof_type: ProofType,
    pub submitter: String,
    pub provider_id: String,
    pub claim: serde_json::Value,
    pub proof_data: String,
    pub timestamp: u64,
    pub block_height: u64,
}

/// Result of verifying a single proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub proof_id: String,
    pub verified: bool,
    pub confidence: f64,
    pub verifier_id: String,
    pub details: String,
    pub timestamp: u64,
    pub gas_used: u64,
    pub error: Option<String>,
}

/// Immutable receipt after successful verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReceipt {
    pub receipt_id: String,
    pub proof_id: String,
    pub result_hash: String,
    pub verified_at: u64,
    pub block_height: u64,
    pub verifier_signature: String,
}

/// A fraud report submitted against a proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudReport {
    pub report_id: String,
    pub proof_id: String,
    pub reporter: String,
    pub fraud_type: FraudType,
    pub evidence: serde_json::Value,
    pub severity: Severity,
    pub status: ReportStatus,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}

/// A batch of proofs verified together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofBatch {
    pub batch_id: String,
    pub proof_ids: Vec<String>,
    pub status: BatchStatus,
    pub results_count: u64,
    pub created_at: u64,
    pub completed_at: Option<u64>,
}

// ---------------------------------------------------------------------------
// ProofVerifier
// ---------------------------------------------------------------------------

/// Core proof verification pipeline.
pub struct ProofVerifier {
    proofs: DashMap<String, ProofSubmission>,
    results: DashMap<String, VerificationResult>,
    receipts: DashMap<String, VerificationReceipt>,
    fraud_reports: DashMap<String, FraudReport>,
    batches: DashMap<String, ProofBatch>,
    config: RwLock<ProofConfig>,
    proof_counter: AtomicU64,
    batch_counter: AtomicU64,
    report_counter: AtomicU64,
    result_counter: AtomicU64,
}

impl ProofVerifier {
    /// Create a new proof verifier with default configuration.
    pub fn new() -> Self {
        Self::with_config(ProofConfig::default())
    }

    /// Create a new proof verifier with custom configuration.
    pub fn with_config(config: ProofConfig) -> Self {
        Self {
            proofs: DashMap::new(),
            results: DashMap::new(),
            receipts: DashMap::new(),
            fraud_reports: DashMap::new(),
            batches: DashMap::new(),
            config: RwLock::new(config),
            proof_counter: AtomicU64::new(0),
            batch_counter: AtomicU64::new(0),
            report_counter: AtomicU64::new(0),
            result_counter: AtomicU64::new(0),
        }
    }

    // -- Proof submission --------------------------------------------------

    /// Submit a new proof for verification.
    pub fn submit_proof(&self, mut proof: ProofSubmission) -> Result<String, String> {
        // Check for double-spend (duplicate proof_id)
        if self.proofs.contains_key(&proof.proof_id) {
            return Err(format!("Duplicate proof_id: {}", proof.proof_id));
        }

        // Validate proof type is enabled
        let cfg = self.config.read().unwrap();
        let type_str = serde_json::to_string(&proof.proof_type).unwrap_or_default();
        if !cfg.proof_types_enabled.contains(&type_str) {
            return Err(format!("Proof type not enabled: {}", type_str));
        }
        drop(cfg);

        let now = Utc::now().timestamp_millis() as u64;
        if proof.timestamp == 0 {
            proof.timestamp = now;
        }
        let proof_id = proof.proof_id.clone();
        self.proofs.insert(proof_id.clone(), proof);
        self.proof_counter.fetch_add(1, Ordering::Relaxed);

        // Auto-verify if configured
        let cfg = self.config.read().unwrap();
        let auto = cfg.auto_verify;
        drop(cfg);
        if auto {
            let _ = self.verify_proof(&proof_id);
        }

        Ok(proof_id)
    }

    /// Get a proof by ID.
    pub fn get_proof(&self, proof_id: &str) -> Option<ProofSubmission> {
        self.proofs.get(proof_id).map(|p| p.value().clone())
    }

    /// List all proofs.
    pub fn list_proofs(&self) -> Vec<ProofSubmission> {
        self.proofs.iter().map(|p| p.value().clone()).collect()
    }

    /// List proofs filtered by type.
    pub fn list_proofs_by_type(&self, proof_type: &ProofType) -> Vec<ProofSubmission> {
        self.proofs
            .iter()
            .filter(|p| &p.value().proof_type == proof_type)
            .map(|p| p.value().clone())
            .collect()
    }

    /// List proofs filtered by provider.
    pub fn list_proofs_by_provider(&self, provider_id: &str) -> Vec<ProofSubmission> {
        self.proofs
            .iter()
            .filter(|p| p.value().provider_id == provider_id)
            .map(|p| p.value().clone())
            .collect()
    }

    // -- Verification ------------------------------------------------------

    /// Verify a single proof (simulated).
    pub fn verify_proof(&self, proof_id: &str) -> Result<VerificationResult, String> {
        let proof = self
            .proofs
            .get(proof_id)
            .ok_or_else(|| format!("Proof not found: {}", proof_id))?
            .value()
            .clone();

        let now = Utc::now().timestamp_millis() as u64;

        // Simulated verification logic based on proof type
        let (verified, confidence, details) = match proof.proof_type {
            ProofType::Compute => {
                // Check claim has required fields
                let has_input = proof.claim.get("input").is_some();
                let has_output = proof.claim.get("output").is_some();
                let has_model = proof.claim.get("model_id").is_some();
                let valid = has_input && has_output && has_model;
                let conf = if valid { 0.92 } else { 0.1 };
                (
                    valid,
                    conf,
                    if valid {
                        "Compute proof verified: input/output/model match".into()
                    } else {
                        "Compute proof invalid: missing required claim fields".into()
                    },
                )
            }
            ProofType::Usage => {
                let has_tokens = proof.claim.get("tokens").is_some();
                let has_request_id = proof.claim.get("request_id").is_some();
                let valid = has_tokens && has_request_id;
                (
                    valid,
                    if valid { 0.88 } else { 0.05 },
                    if valid {
                        "Usage proof verified: token count and request ID match".into()
                    } else {
                        "Usage proof invalid: missing token/request data".into()
                    },
                )
            }
            ProofType::Stake => {
                let has_amount = proof.claim.get("amount").is_some();
                let has_duration = proof.claim.get("duration").is_some();
                let valid = has_amount && has_duration;
                (
                    valid,
                    if valid { 0.95 } else { 0.0 },
                    if valid {
                        "Stake proof verified: amount and duration confirmed".into()
                    } else {
                        "Stake proof invalid: missing stake parameters".into()
                    },
                )
            }
            ProofType::Reputation => {
                let has_score = proof.claim.get("score").is_some();
                let has_reviews = proof.claim.get("reviews").is_some();
                let valid = has_score && has_reviews;
                (
                    valid,
                    if valid { 0.85 } else { 0.15 },
                    if valid {
                        "Reputation proof verified: score and reviews match".into()
                    } else {
                        "Reputation proof invalid: missing reputation data".into()
                    },
                )
            }
            ProofType::Governance => {
                let has_proposal = proof.claim.get("proposal_id").is_some();
                let has_vote = proof.claim.get("vote").is_some();
                let valid = has_proposal && has_vote;
                (
                    valid,
                    if valid { 0.97 } else { 0.0 },
                    if valid {
                        "Governance proof verified: proposal and vote confirmed".into()
                    } else {
                        "Governance proof invalid: missing governance data".into()
                    },
                )
            }
            ProofType::Ensemble => {
                let has_models = proof.claim.get("model_ids").is_some();
                let has_result = proof.claim.get("result").is_some();
                let valid = has_models && has_result;
                (
                    valid,
                    if valid { 0.90 } else { 0.1 },
                    if valid {
                        "Ensemble proof verified: model group and result match".into()
                    } else {
                        "Ensemble proof invalid: missing ensemble data".into()
                    },
                )
            }
        };

        let gas_used = match proof.proof_type {
            ProofType::Compute => 15000,
            ProofType::Usage => 5000,
            ProofType::Stake => 8000,
            ProofType::Reputation => 10000,
            ProofType::Governance => 12000,
            ProofType::Ensemble => 20000,
        };

        let result = VerificationResult {
            proof_id: proof_id.to_string(),
            verified,
            confidence,
            verifier_id: "xergon-verifier-v1".to_string(),
            details,
            timestamp: now,
            gas_used,
            error: if verified { None } else { Some("Verification failed".into()) },
        };

        self.results.insert(proof_id.to_string(), result.clone());
        self.result_counter.fetch_add(1, Ordering::Relaxed);

        // Generate receipt on success
        if verified {
            let receipt = VerificationReceipt {
                receipt_id: format!("rcpt-{}", self.result_counter.load(Ordering::Relaxed)),
                proof_id: proof_id.to_string(),
                result_hash: format!("{:x}", md5_hex(&proof.proof_data)),
                verified_at: now,
                block_height: proof.block_height,
                verifier_signature: format!("sig-{}-{}", proof_id, now),
            };
            self.receipts.insert(proof_id.to_string(), receipt);
        }

        Ok(result)
    }

    /// Verify a batch of proofs.
    pub fn verify_batch(&self, proof_ids: &[String]) -> Result<ProofBatch, String> {
        let cfg = self.config.read().unwrap();
        if proof_ids.len() > cfg.max_batch_size {
            return Err(format!(
                "Batch size {} exceeds max {}",
                proof_ids.len(),
                cfg.max_batch_size
            ));
        }
        drop(cfg);

        let now = Utc::now().timestamp_millis() as u64;
        let batch_id = format!(
            "batch-{}",
            self.batch_counter.fetch_add(1, Ordering::Relaxed)
        );

        let mut batch = ProofBatch {
            batch_id: batch_id.clone(),
            proof_ids: proof_ids.to_vec(),
            status: BatchStatus::Verifying,
            results_count: 0,
            created_at: now,
            completed_at: None,
        };

        let mut verified_count = 0u64;
        for pid in proof_ids {
            if self.proofs.contains_key(pid) {
                match self.verify_proof(pid) {
                    Ok(r) => {
                        if r.verified {
                            verified_count += 1;
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        batch.results_count = verified_count;
        batch.status = BatchStatus::Completed;
        batch.completed_at = Some(Utc::now().timestamp_millis() as u64);

        let result_batch = batch.clone();
        self.batches.insert(batch_id, batch);
        Ok(result_batch)
    }

    // -- Results & receipts ------------------------------------------------

    pub fn get_result(&self, proof_id: &str) -> Option<VerificationResult> {
        self.results.get(proof_id).map(|r| r.value().clone())
    }

    pub fn list_results(&self) -> Vec<VerificationResult> {
        self.results.iter().map(|r| r.value().clone()).collect()
    }

    pub fn get_receipt(&self, proof_id: &str) -> Option<VerificationReceipt> {
        self.receipts.get(proof_id).map(|r| r.value().clone())
    }

    pub fn list_receipts(&self) -> Vec<VerificationReceipt> {
        self.receipts.iter().map(|r| r.value().clone()).collect()
    }

    // -- Fraud detection ---------------------------------------------------

    /// Report a suspected fraud.
    pub fn report_fraud(
        &self,
        proof_id: &str,
        reporter: &str,
        fraud_type: FraudType,
        evidence: serde_json::Value,
        severity: Severity,
    ) -> Result<String, String> {
        if !self.proofs.contains_key(proof_id) {
            return Err(format!("Proof not found: {}", proof_id));
        }

        let now = Utc::now().timestamp_millis() as u64;
        let report_id = format!(
            "fraud-{}",
            self.report_counter.fetch_add(1, Ordering::Relaxed)
        );

        let report = FraudReport {
            report_id: report_id.clone(),
            proof_id: proof_id.to_string(),
            reporter: reporter.to_string(),
            fraud_type,
            evidence,
            severity,
            status: ReportStatus::Pending,
            created_at: now,
            resolved_at: None,
        };

        self.fraud_reports.insert(report_id.clone(), report);
        Ok(report_id)
    }

    pub fn get_fraud_report(&self, report_id: &str) -> Option<FraudReport> {
        self.fraud_reports.get(report_id).map(|r| r.value().clone())
    }

    pub fn list_fraud_reports(&self) -> Vec<FraudReport> {
        self.fraud_reports.iter().map(|r| r.value().clone()).collect()
    }

    pub fn resolve_fraud_report(&self, report_id: &str, confirmed: bool) -> Result<(), String> {
        let mut report = self
            .fraud_reports
            .get_mut(report_id)
            .ok_or_else(|| format!("Report not found: {}", report_id))?;

        report.status = if confirmed {
            ReportStatus::Confirmed
        } else {
            ReportStatus::Dismissed
        };
        report.resolved_at = Some(Utc::now().timestamp_millis() as u64);
        Ok(())
    }

    /// Detect potential Sybil attacks (same submitter, different providers).
    pub fn detect_sybil(&self) -> Vec<(String, Vec<String>)> {
        let mut submitter_providers: HashMap<String, Vec<String>> = HashMap::new();
        for entry in self.proofs.iter() {
            let p = entry.value();
            submitter_providers
                .entry(p.submitter.clone())
                .or_default()
                .push(p.provider_id.clone());
        }

        submitter_providers
            .into_iter()
            .filter(|(_, providers)| {
                let unique: std::collections::HashSet<_> = providers.iter().cloned().collect();
                unique.len() > 1
            })
            .map(|(s, p)| (s, p))
            .collect()
    }

    /// Detect double-spend (same proof_data used with different proof_ids).
    pub fn detect_double_spend(&self) -> Vec<(String, String)> {
        let mut data_to_id: HashMap<String, Vec<String>> = HashMap::new();
        for entry in self.proofs.iter() {
            let p = entry.value();
            data_to_id
                .entry(p.proof_data.clone())
                .or_default()
                .push(p.proof_id.clone());
        }

        let mut collisions = Vec::new();
        for (_, ids) in data_to_id {
            if ids.len() > 1 {
                for i in 0..ids.len() {
                    for j in (i + 1)..ids.len() {
                        collisions.push((ids[i].clone(), ids[j].clone()));
                    }
                }
            }
        }
        collisions
    }

    // -- Stats -------------------------------------------------------------

    pub fn get_stats(&self) -> ProofVerifierStats {
        let total_proofs = self.proofs.len() as u64;
        let total_results = self.results.len() as u64;
        let verified = self
            .results
            .iter()
            .filter(|r| r.value().verified)
            .count() as u64;
        let failed = total_results - verified;
        let total_batches = self.batches.len() as u64;
        let total_fraud = self.fraud_reports.len() as u64;
        let confirmed_fraud = self
            .fraud_reports
            .iter()
            .filter(|r| r.value().status == ReportStatus::Confirmed)
            .count() as u64;

        ProofVerifierStats {
            total_proofs,
            total_results,
            verified,
            failed,
            total_batches,
            total_fraud_reports: total_fraud,
            confirmed_fraud,
        }
    }

    pub fn get_verification_rate(&self) -> f64 {
        let total = self.results.len();
        if total == 0 {
            return 0.0;
        }
        let verified = self.results.iter().filter(|r| r.value().verified).count();
        verified as f64 / total as f64
    }

    pub fn get_fraud_rate(&self) -> f64 {
        let total = self.fraud_reports.len();
        if total == 0 {
            return 0.0;
        }
        let confirmed = self
            .fraud_reports
            .iter()
            .filter(|r| r.value().status == ReportStatus::Confirmed)
            .count();
        confirmed as f64 / total as f64
    }

    pub fn get_batch_stats(&self) -> BatchStats {
        let total = self.batches.len() as u64;
        let completed = self
            .batches
            .iter()
            .filter(|b| b.value().status == BatchStatus::Completed)
            .count() as u64;
        let total_verified: u64 = self
            .batches
            .iter()
            .map(|b| b.value().results_count)
            .sum();
        BatchStats {
            total_batches: total,
            completed_batches: completed,
            total_proofs_verified: total_verified,
        }
    }

    // -- Config ------------------------------------------------------------

    pub fn get_config(&self) -> ProofConfig {
        self.config.read().unwrap().clone()
    }

    pub fn update_config(&self, config: ProofConfig) {
        *self.config.write().unwrap() = config;
    }
}

/// Statistics about the proof verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofVerifierStats {
    pub total_proofs: u64,
    pub total_results: u64,
    pub verified: u64,
    pub failed: u64,
    pub total_batches: u64,
    pub total_fraud_reports: u64,
    pub confirmed_fraud: u64,
}

/// Statistics about verification batches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStats {
    pub total_batches: u64,
    pub completed_batches: u64,
    pub total_proofs_verified: u64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Simple hash for proof data (not cryptographic, simulated).
fn md5_hex(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_proof(proof_id: &str, proof_type: ProofType, submitter: &str, provider_id: &str) -> ProofSubmission {
        let mut claim = serde_json::Map::new();
        match proof_type {
            ProofType::Compute => {
                claim.insert("input".into(), serde_json::Value::String("test input".into()));
                claim.insert("output".into(), serde_json::Value::String("test output".into()));
                claim.insert("model_id".into(), serde_json::Value::String("model-1".into()));
            }
            ProofType::Usage => {
                claim.insert("tokens".into(), serde_json::Value::Number(100.into()));
                claim.insert("request_id".into(), serde_json::Value::String("req-1".into()));
            }
            ProofType::Stake => {
                claim.insert("amount".into(), serde_json::Value::Number(1000.into()));
                claim.insert("duration".into(), serde_json::Value::Number(30.into()));
            }
            ProofType::Reputation => {
                claim.insert("score".into(), serde_json::Value::Number(95.into()));
                claim.insert("reviews".into(), serde_json::Value::Number(10.into()));
            }
            ProofType::Governance => {
                claim.insert("proposal_id".into(), serde_json::Value::String("prop-1".into()));
                claim.insert("vote".into(), serde_json::Value::String("for".into()));
            }
            ProofType::Ensemble => {
                claim.insert("model_ids".into(), serde_json::json!(["m1", "m2"]));
                claim.insert("result".into(), serde_json::Value::String("ensemble result".into()));
            }
        }
        ProofSubmission {
            proof_id: proof_id.to_string(),
            proof_type,
            submitter: submitter.to_string(),
            provider_id: provider_id.to_string(),
            claim: serde_json::Value::Object(claim),
            proof_data: format!("proof-data-{}", proof_id),
            timestamp: 1000,
            block_height: 500000,
        }
    }

    fn valid_proof() -> ProofSubmission {
        make_proof("proof-1", ProofType::Compute, "alice", "provider-a")
    }

    #[test]
    fn test_submit_and_get_proof() {
        let verifier = ProofVerifier::new();
        let proof = valid_proof();
        let id = verifier.submit_proof(proof).unwrap();
        assert_eq!(id, "proof-1");
        let retrieved = verifier.get_proof("proof-1").unwrap();
        assert_eq!(retrieved.submitter, "alice");
    }

    #[test]
    fn test_duplicate_proof_rejected() {
        let verifier = ProofVerifier::new();
        let _ = verifier.submit_proof(valid_proof());
        let result = verifier.submit_proof(valid_proof());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn test_list_proofs() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(make_proof("p1", ProofType::Compute, "a", "prov-a")).unwrap();
        verifier.submit_proof(make_proof("p2", ProofType::Usage, "b", "prov-b")).unwrap();
        verifier.submit_proof(make_proof("p3", ProofType::Stake, "c", "prov-c")).unwrap();
        assert_eq!(verifier.list_proofs().len(), 3);
    }

    #[test]
    fn test_list_proofs_by_type() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(make_proof("p1", ProofType::Compute, "a", "prov-a")).unwrap();
        verifier.submit_proof(make_proof("p2", ProofType::Usage, "b", "prov-b")).unwrap();
        verifier.submit_proof(make_proof("p3", ProofType::Compute, "c", "prov-c")).unwrap();
        let compute = verifier.list_proofs_by_type(&ProofType::Compute);
        assert_eq!(compute.len(), 2);
        let usage = verifier.list_proofs_by_type(&ProofType::Usage);
        assert_eq!(usage.len(), 1);
    }

    #[test]
    fn test_list_proofs_by_provider() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(make_proof("p1", ProofType::Compute, "a", "prov-a")).unwrap();
        verifier.submit_proof(make_proof("p2", ProofType::Compute, "b", "prov-a")).unwrap();
        verifier.submit_proof(make_proof("p3", ProofType::Compute, "c", "prov-b")).unwrap();
        let by_a = verifier.list_proofs_by_provider("prov-a");
        assert_eq!(by_a.len(), 2);
    }

    #[test]
    fn test_verify_compute_proof() {
        let verifier = ProofVerifier::with_config(ProofConfig { auto_verify: false, ..Default::default() });
        verifier.submit_proof(valid_proof()).unwrap();
        let result = verifier.verify_proof("proof-1").unwrap();
        assert!(result.verified);
        assert!(result.confidence > 0.9);
        assert_eq!(result.gas_used, 15000);
    }

    #[test]
    fn test_verify_invalid_proof() {
        let verifier = ProofVerifier::with_config(ProofConfig { auto_verify: false, ..Default::default() });
        let proof = ProofSubmission {
            proof_id: "bad-proof".into(),
            proof_type: ProofType::Compute,
            submitter: "alice".into(),
            provider_id: "prov-a".into(),
            claim: serde_json::Value::Object(serde_json::Map::new()), // empty claim
            proof_data: "data".into(),
            timestamp: 1000,
            block_height: 500000,
        };
        verifier.submit_proof(proof).unwrap();
        let result = verifier.verify_proof("bad-proof").unwrap();
        assert!(!result.verified);
        assert!(result.confidence < 0.5);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_verify_receipt_generated() {
        let verifier = ProofVerifier::with_config(ProofConfig { auto_verify: false, ..Default::default() });
        verifier.submit_proof(valid_proof()).unwrap();
        verifier.verify_proof("proof-1").unwrap();
        let receipt = verifier.get_receipt("proof-1").unwrap();
        assert!(receipt.receipt_id.starts_with("rcpt-"));
        assert_eq!(receipt.proof_id, "proof-1");
    }

    #[test]
    fn test_verify_batch() {
        let verifier = ProofVerifier::with_config(ProofConfig { auto_verify: false, ..Default::default() });
        verifier.submit_proof(make_proof("p1", ProofType::Compute, "a", "prov-a")).unwrap();
        verifier.submit_proof(make_proof("p2", ProofType::Usage, "b", "prov-b")).unwrap();
        verifier.submit_proof(make_proof("p3", ProofType::Stake, "c", "prov-c")).unwrap();

        let batch = verifier.verify_batch(&["p1".into(), "p2".into(), "p3".into()]).unwrap();
        assert_eq!(batch.status, BatchStatus::Completed);
        assert_eq!(batch.results_count, 3);
        assert!(batch.completed_at.is_some());
    }

    #[test]
    fn test_batch_size_limit() {
        let verifier = ProofVerifier::new();
        let ids: Vec<String> = (0..200).map(|i| format!("p{}", i)).into_iter().collect();
        let result = verifier.verify_batch(&ids);
        assert!(result.is_err());
    }

    #[test]
    fn test_report_fraud() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(valid_proof()).unwrap();
        let report_id = verifier
            .report_fraud(
                "proof-1",
                "bob",
                FraudType::InvalidClaim,
                serde_json::json!({"reason": "claim mismatch"}),
                Severity::High,
            )
            .unwrap();
        let report = verifier.get_fraud_report(&report_id).unwrap();
        assert_eq!(report.proof_id, "proof-1");
        assert_eq!(report.reporter, "bob");
        assert_eq!(report.fraud_type, FraudType::InvalidClaim);
    }

    #[test]
    fn test_resolve_fraud_confirmed() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(valid_proof()).unwrap();
        let report_id = verifier
            .report_fraud("proof-1", "bob", FraudType::DoubleSpend, serde_json::json!({}), Severity::Critical)
            .unwrap();
        verifier.resolve_fraud_report(&report_id, true).unwrap();
        let report = verifier.get_fraud_report(&report_id).unwrap();
        assert_eq!(report.status, ReportStatus::Confirmed);
        assert!(report.resolved_at.is_some());
    }

    #[test]
    fn test_detect_sybil() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(make_proof("p1", ProofType::Compute, "alice", "prov-a")).unwrap();
        verifier.submit_proof(make_proof("p2", ProofType::Compute, "alice", "prov-b")).unwrap();
        verifier.submit_proof(make_proof("p3", ProofType::Compute, "alice", "prov-c")).unwrap();
        verifier.submit_proof(make_proof("p4", ProofType::Compute, "bob", "prov-a")).unwrap();

        let sybils = verifier.detect_sybil();
        assert_eq!(sybils.len(), 1);
        assert_eq!(sybils[0].0, "alice");
        assert_eq!(sybils[0].1.len(), 3);
    }

    #[test]
    fn test_detect_double_spend() {
        let verifier = ProofVerifier::new();
        let p1 = make_proof("p1", ProofType::Compute, "alice", "prov-a");
        let mut p2 = make_proof("p2", ProofType::Compute, "bob", "prov-b");
        p2.proof_data = p1.proof_data.clone(); // same proof data, different ID
        verifier.submit_proof(p1).unwrap();
        verifier.submit_proof(p2).unwrap();

        let doubles = verifier.detect_double_spend();
        assert_eq!(doubles.len(), 1);
    }

    #[test]
    fn test_get_stats() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(valid_proof()).unwrap();
        verifier.submit_proof(make_proof("p2", ProofType::Usage, "b", "prov-b")).unwrap();
        let stats = verifier.get_stats();
        assert_eq!(stats.total_proofs, 2);
        assert!(stats.verified >= 2); // auto-verified
    }

    #[test]
    fn test_verification_rate() {
        let verifier = ProofVerifier::with_config(ProofConfig { auto_verify: false, ..Default::default() });
        verifier.submit_proof(valid_proof()).unwrap();
        verifier.submit_proof(make_proof("p2", ProofType::Usage, "b", "prov-b")).unwrap();
        verifier.verify_proof("proof-1").unwrap();
        verifier.verify_proof("p2").unwrap();
        let rate = verifier.get_verification_rate();
        assert!(rate > 0.0);
    }

    #[test]
    fn test_fraud_rate() {
        let verifier = ProofVerifier::new();
        assert_eq!(verifier.get_fraud_rate(), 0.0);
        verifier.submit_proof(valid_proof()).unwrap();
        verifier.report_fraud("proof-1", "bob", FraudType::InvalidClaim, serde_json::json!({}), Severity::Low).unwrap();
        verifier.resolve_fraud_report("fraud-0", true).unwrap();
        assert!(verifier.get_fraud_rate() > 0.0);
    }

    #[test]
    fn test_config_update() {
        let verifier = ProofVerifier::new();
        let mut cfg = verifier.get_config();
        assert_eq!(cfg.max_batch_size, 100);
        cfg.max_batch_size = 50;
        verifier.update_config(cfg);
        assert_eq!(verifier.get_config().max_batch_size, 50);
    }

    #[test]
    fn test_serialization() {
        let verifier = ProofVerifier::new();
        verifier.submit_proof(valid_proof()).unwrap();
        let proof = verifier.get_proof("proof-1").unwrap();
        let json = serde_json::to_string(&proof).unwrap();
        let deserialized: ProofSubmission = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.proof_id, proof.proof_id);
    }

    #[test]
    fn test_concurrent_proof_submission() {
        use std::thread;

        let verifier = ProofVerifier::with_config(ProofConfig { auto_verify: false, ..Default::default() });
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let v = &verifier as *const ProofVerifier as usize;
                thread::spawn(move || {
                    let v = unsafe { &*(v as *const ProofVerifier) };
                    let proof = make_proof(&format!("p{}", i), ProofType::Compute, &format!("user-{}", i), "prov-a");
                    v.submit_proof(proof).unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(verifier.list_proofs().len(), 10);
    }
}
