//! Proof submission pipeline for the Xergon Network agent.
//!
//! End-to-end pipeline for constructing, submitting, and verifying sigma proofs
//! on-chain. Providers submit proofs of work (PoNW), inference attestations,
//! and other cryptographic commitments. This module is distinct from proof_verifier
//! (which handles ZK proof verification) and focuses on the full submission lifecycle:
//! constructing sigma proofs, submitting them on-chain, tracking verification receipts,
//! and detecting fraud patterns.

use axum::{
    extract::{Path, Query, State},
    Json, Router,
    routing::{get, post},
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ================================================================
// Enums
// ================================================================

/// Types of proofs that can be submitted through the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProofType {
    /// Proof of Network Work - demonstrates computational contribution
    PoNW,
    /// Attestation of inference correctness
    InferenceAttestation,
    /// Hash commitment to model artifacts
    ModelHashCommitment,
    /// Provider registration proof
    ProviderRegistration,
    /// Stake proof on-chain
    StakeProof,
    /// Slashing condition proof
    SlashingProof,
    /// Challenge-response proof
    ChallengeResponse,
}

impl Default for ProofType {
    fn default() -> Self {
        Self::PoNW
    }
}

impl std::fmt::Display for ProofType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PoNW => write!(f, "ponw"),
            Self::InferenceAttestation => write!(f, "inference_attestation"),
            Self::ModelHashCommitment => write!(f, "model_hash_commitment"),
            Self::ProviderRegistration => write!(f, "provider_registration"),
            Self::StakeProof => write!(f, "stake_proof"),
            Self::SlashingProof => write!(f, "slashing_proof"),
            Self::ChallengeResponse => write!(f, "challenge_response"),
        }
    }
}

/// Categories of fraud detected in the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FraudType {
    /// Commitment hash does not match proof data
    InvalidCommitment,
    /// Proof is older than the freshness window
    StaleProof,
    /// Same commitment submitted twice by same provider
    DoubleSubmission,
    /// Provider signature is invalid
    InvalidSignature,
    /// Proof data has been tampered with
    TamperedData,
    /// Proof was previously submitted (replay)
    ReplayAttack,
}

impl std::fmt::Display for FraudType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCommitment => write!(f, "invalid_commitment"),
            Self::StaleProof => write!(f, "stale_proof"),
            Self::DoubleSubmission => write!(f, "double_submission"),
            Self::InvalidSignature => write!(f, "invalid_signature"),
            Self::TamperedData => write!(f, "tampered_data"),
            Self::ReplayAttack => write!(f, "replay_attack"),
        }
    }
}

/// Status of a proof batch in the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    /// Batch created, not yet submitted
    Pending,
    /// Batch submitted to verification
    Submitted,
    /// Some proofs in batch verified
    PartiallyVerified,
    /// All proofs in batch verified
    AllVerified,
    /// Batch verification failed
    Failed,
}

impl Default for BatchStatus {
    fn default() -> Self {
        Self::Pending
    }
}

// ================================================================
// Data Types
// ================================================================

/// A single proof check within verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofCheck {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

/// Result of verifying a proof submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub valid: bool,
    pub verifier: String,
    pub verification_time_ms: u64,
    pub checks: Vec<ProofCheck>,
    pub fraud_detected: bool,
    pub fraud_type: Option<FraudType>,
}

/// A proof submission in the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofSubmission {
    pub id: String,
    pub provider_id: String,
    pub proof_type: ProofType,
    pub proof_data: String,
    pub commitment_hash: String,
    pub public_inputs: Vec<String>,
    pub submitted_at: i64,
    pub verified: bool,
    pub verification_result: Option<VerificationResult>,
    pub on_chain_tx_id: Option<String>,
    pub gas_used: u64,
}

/// Verification receipt (BLAKE3-based).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofReceipt {
    pub submission_id: String,
    pub receipt_hash: String,
    pub block_height: u64,
    pub timestamp: i64,
    pub verifier_pubkey: String,
    pub on_chain: bool,
}

/// A batch of proof submissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofBatch {
    pub batch_id: String,
    pub proofs: Vec<ProofSubmission>,
    pub submitted_at: i64,
    pub status: BatchStatus,
}

/// Pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub max_batch_size: u32,
    pub verification_timeout_ms: u64,
    pub auto_submit: bool,
    pub fraud_threshold: f64,
    pub max_retries: u32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            verification_timeout_ms: 5000,
            auto_submit: true,
            fraud_threshold: 0.95,
            max_retries: 3,
        }
    }
}

/// Pipeline statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStats {
    pub total_submitted: u64,
    pub total_verified: u64,
    pub total_rejected: u64,
    pub fraud_detected: u64,
    pub avg_verification_ms: u64,
    pub batches_processed: u64,
}

// ================================================================
// Internal fraud tracking entry
// ================================================================

#[derive(Debug, Clone)]
struct FraudRecord {
    provider_id: String,
    fraud_type: FraudType,
    submission_id: String,
    #[allow(dead_code)]
    detected_at: i64,
}

// ================================================================
// ProofPipeline
// ================================================================

/// Full proof submission pipeline with verification, batching, and fraud detection.
pub struct ProofPipeline {
    submissions: DashMap<String, ProofSubmission>,
    receipts: DashMap<String, ProofReceipt>,
    batches: DashMap<String, ProofBatch>,
    commitment_index: DashMap<String, String>, // commitment_hash -> submission_id
    fraud_records: DashMap<String, FraudRecord>,
    provider_commitments: DashMap<String, Vec<String>>, // provider_id -> [commitment_hashes]
    config: DashMap<String, PipelineConfig>, // single key "config"
    stats_total_submitted: AtomicU64,
    stats_total_verified: AtomicU64,
    stats_total_rejected: AtomicU64,
    stats_fraud_detected: AtomicU64,
    stats_verification_time_sum: AtomicU64,
    stats_verification_count: AtomicU64,
    stats_batches_processed: AtomicU64,
    #[allow(dead_code)]
    submission_counter: AtomicU64,
    #[allow(dead_code)]
    batch_counter: AtomicU64,
    current_block_height: AtomicU64,
}

impl ProofPipeline {
    /// Create a new proof pipeline with default configuration.
    pub fn new() -> Self {
        Self::with_config(PipelineConfig::default())
    }

    /// Create a new proof pipeline with custom configuration.
    pub fn with_config(config: PipelineConfig) -> Self {
        let pipeline = Self {
            submissions: DashMap::new(),
            receipts: DashMap::new(),
            batches: DashMap::new(),
            commitment_index: DashMap::new(),
            fraud_records: DashMap::new(),
            provider_commitments: DashMap::new(),
            config: DashMap::new(),
            stats_total_submitted: AtomicU64::new(0),
            stats_total_verified: AtomicU64::new(0),
            stats_total_rejected: AtomicU64::new(0),
            stats_fraud_detected: AtomicU64::new(0),
            stats_verification_time_sum: AtomicU64::new(0),
            stats_verification_count: AtomicU64::new(0),
            stats_batches_processed: AtomicU64::new(0),
            submission_counter: AtomicU64::new(0),
            batch_counter: AtomicU64::new(0),
            current_block_height: AtomicU64::new(1_000_000),
        };
        pipeline.config.insert("config".to_string(), config);
        pipeline
    }

    // -- Config management -------------------------------------------------

    fn get_config_value(&self) -> PipelineConfig {
        self.config
            .get("config")
            .map(|c| c.value().clone())
            .unwrap_or_default()
    }

    /// Get the current pipeline configuration.
    pub fn get_config(&self) -> PipelineConfig {
        self.get_config_value()
    }

    /// Update pipeline configuration.
    pub fn update_config(&self, config: PipelineConfig) {
        self.config.insert("config".to_string(), config);
        info!("Pipeline configuration updated");
    }

    // -- Proof submission --------------------------------------------------

    /// Submit a single proof to the pipeline.
    pub fn submit_proof(
        &self,
        provider_id: &str,
        proof_type: ProofType,
        proof_data: &str,
        public_inputs: Vec<String>,
    ) -> Result<ProofSubmission, String> {
        let cfg = self.get_config_value();

        // Validate proof data is hex
        if proof_data.is_empty() {
            return Err("Proof data cannot be empty".to_string());
        }
        if let Err(e) = hex::decode(proof_data) {
            if !proof_data.starts_with("0x") {
                if let Err(_) = hex::decode(&format!("0x{}", proof_data)) {
                    return Err(format!("Invalid hex proof data: {}", e));
                }
            }
        }

        // Compute commitment hash (BLAKE3 of proof_data + public_inputs)
        let mut hasher = blake3::Hasher::new();
        hasher.update(proof_data.as_bytes());
        for input in &public_inputs {
            hasher.update(input.as_bytes());
        }
        let commitment_hash = format!("blake3:{}", hasher.finalize().to_hex());

        // Check for double submission (same commitment from same provider)
        if let Some(existing_id) = self.commitment_index.get(&commitment_hash) {
            if let Some(existing) = self.submissions.get(&*existing_id) {
                if existing.provider_id == provider_id {
                    return Err(format!(
                        "Double submission: commitment {} already submitted by {}",
                        commitment_hash, provider_id
                    ));
                }
            }
        }

        // Check for replay attack (same proof_data from same provider)
        if let Some(mut commitments) = self.provider_commitments.get_mut(provider_id) {
            for existing_hash in commitments.iter() {
                if *existing_hash == commitment_hash {
                    return Err(format!(
                        "Replay attack detected: provider {} already submitted commitment {}",
                        provider_id, commitment_hash
                    ));
                }
            }
            commitments.push(commitment_hash.clone());
        } else {
            self.provider_commitments
                .insert(provider_id.to_string(), vec![commitment_hash.clone()]);
        }

        let id = format!("ps_{}", Uuid::new_v4().to_string().replace('-', ""));
        let submitted_at = Utc::now().timestamp_millis();

        let submission = ProofSubmission {
            id: id.clone(),
            provider_id: provider_id.to_string(),
            proof_type: proof_type.clone(),
            proof_data: proof_data.to_string(),
            commitment_hash: commitment_hash.clone(),
            public_inputs: public_inputs.clone(),
            submitted_at,
            verified: false,
            verification_result: None,
            on_chain_tx_id: None,
            gas_used: 0,
        };

        self.submissions.insert(id.clone(), submission.clone());
        self.commitment_index
            .insert(commitment_hash.clone(), id.clone());
        self.stats_total_submitted.fetch_add(1, Ordering::Relaxed);

        info!(
            "Proof submitted: id={}, provider={}, type={}",
            id, provider_id, proof_type
        );
        debug!("Commitment hash: {}", commitment_hash);

        // Auto-verify if configured
        if cfg.auto_submit {
            match self.verify_proof(&id) {
                Ok(_) => debug!("Auto-verification succeeded for {}", id),
                Err(e) => warn!("Auto-verification failed for {}: {}", id, e),
            }
        }

        Ok(submission)
    }

    /// Submit a batch of proofs.
    pub fn submit_batch(&self, proofs: Vec<ProofSubmission>) -> Result<ProofBatch, String> {
        let cfg = self.get_config_value();

        if proofs.is_empty() {
            return Err("Batch cannot be empty".to_string());
        }
        if proofs.len() > cfg.max_batch_size as usize {
            return Err(format!(
                "Batch size {} exceeds max {}",
                proofs.len(),
                cfg.max_batch_size
            ));
        }

        let batch_id = format!("batch_{}", Uuid::new_v4().to_string().replace('-', ""));
        let submitted_at = Utc::now().timestamp_millis();

        // Store each proof and validate
        for proof in &proofs {
            // Check for double submission
            if let Some(existing_id) = self.commitment_index.get(&proof.commitment_hash) {
                if let Some(existing) = self.submissions.get(&*existing_id) {
                    if existing.provider_id == proof.provider_id {
                        return Err(format!(
                            "Double submission in batch: commitment {} from provider {}",
                            proof.commitment_hash, proof.provider_id
                        ));
                    }
                }
            }

            self.submissions.insert(proof.id.clone(), proof.clone());
            self.commitment_index
                .insert(proof.commitment_hash.clone(), proof.id.clone());
            self.stats_total_submitted.fetch_add(1, Ordering::Relaxed);
        }

        let batch = ProofBatch {
            batch_id: batch_id.clone(),
            proofs: proofs.clone(),
            submitted_at,
            status: BatchStatus::Submitted,
        };

        self.batches.insert(batch_id.clone(), batch.clone());

        info!(
            "Batch submitted: id={}, size={}",
            batch_id,
            proofs.len()
        );

        Ok(batch)
    }

    // -- Proof verification ------------------------------------------------

    /// Verify a single proof submission.
    pub fn verify_proof(&self, submission_id: &str) -> Result<VerificationResult, String> {
        let start = std::time::Instant::now();

        // Clone submission data so we don't hold the DashMap lock during verification
        let submission = {
            let entry = self
                .submissions
                .get(submission_id)
                .ok_or_else(|| format!("Submission not found: {}", submission_id))?;
            if entry.value().verified {
                return Err(format!("Submission {} already verified", submission_id));
            }
            entry.value().clone()
        };

        let cfg = self.get_config_value();
        let mut checks: Vec<ProofCheck> = Vec::new();
        let mut fraud_detected = false;
        let mut fraud_type: Option<FraudType> = None;

        // Check 1: Commitment hash integrity
        let mut hasher = blake3::Hasher::new();
        hasher.update(submission.proof_data.as_bytes());
        for input in &submission.public_inputs {
            hasher.update(input.as_bytes());
        }
        let expected_hash = format!("blake3:{}", hasher.finalize().to_hex());
        let commitment_valid = expected_hash == submission.commitment_hash;
        checks.push(ProofCheck {
            name: "commitment_hash_integrity".to_string(),
            passed: commitment_valid,
            details: if commitment_valid {
                "Commitment hash matches proof data".to_string()
            } else {
                format!(
                    "Commitment mismatch: expected {}, got {}",
                    expected_hash, submission.commitment_hash
                )
            },
        });

        if !commitment_valid {
            fraud_detected = true;
            fraud_type = Some(FraudType::InvalidCommitment);
        }

        // Check 2: Timestamp freshness (reject proofs older than 1 hour)
        let now = Utc::now().timestamp_millis();
        let age_ms = now - submission.submitted_at;
        let freshness_limit_ms = 3_600_000i64; // 1 hour
        let is_fresh = age_ms >= 0 && age_ms < freshness_limit_ms;
        checks.push(ProofCheck {
            name: "timestamp_freshness".to_string(),
            passed: is_fresh,
            details: if is_fresh {
                format!("Proof is {}ms old", age_ms)
            } else {
                format!("Proof is stale: {}ms old (limit: {}ms)", age_ms, freshness_limit_ms)
            },
        });

        if !is_fresh {
            fraud_detected = true;
            fraud_type = Some(FraudType::StaleProof);
        }

        // Check 3: Proof data is valid hex
        let hex_valid = hex::decode(&submission.proof_data).is_ok()
            || hex::decode(&format!("0x{}", &submission.proof_data)).is_ok();
        checks.push(ProofCheck {
            name: "proof_data_hex_valid".to_string(),
            passed: hex_valid,
            details: if hex_valid {
                "Proof data is valid hex".to_string()
            } else {
                "Proof data contains invalid hex characters".to_string()
            },
        });

        if !hex_valid {
            fraud_detected = true;
            fraud_type = Some(FraudType::TamperedData);
        }

        // Type-specific checks
        match &submission.proof_type {
            ProofType::PoNW => {
                // Check that public inputs contain a valid timestamp
                let has_timestamp = submission.public_inputs.iter().any(|i| {
                    i.parse::<i64>().is_ok()
                        || i.starts_with("timestamp:")
                            && i.strip_prefix("timestamp:")
                                .map(|v| v.parse::<i64>().is_ok())
                                .unwrap_or(false)
                });
                checks.push(ProofCheck {
                    name: "ponw_timestamp_input".to_string(),
                    passed: has_timestamp,
                    details: if has_timestamp {
                        "PoNW contains valid timestamp in public inputs".to_string()
                    } else {
                        "PoNW missing timestamp in public inputs".to_string()
                    },
                });

                // Check proof data length (minimum 32 bytes for a valid PoNW)
                let proof_bytes = hex::decode(&submission.proof_data).unwrap_or_default();
                let sufficient_length = proof_bytes.len() >= 32;
                checks.push(ProofCheck {
                    name: "ponw_proof_length".to_string(),
                    passed: sufficient_length,
                    details: if sufficient_length {
                        format!("PoNW proof data is {} bytes", proof_bytes.len())
                    } else {
                        format!(
                            "PoNW proof data too short: {} bytes (minimum: 32)",
                            proof_bytes.len()
                        )
                    },
                });
            }
            ProofType::InferenceAttestation => {
                // Check that public inputs contain a model reference
                let has_model_ref = !submission.public_inputs.is_empty()
                    && submission
                        .public_inputs
                        .iter()
                        .any(|i| i.starts_with("model:"));
                checks.push(ProofCheck {
                    name: "inference_model_reference".to_string(),
                    passed: has_model_ref,
                    details: if has_model_ref {
                        "Inference attestation contains model reference".to_string()
                    } else {
                        "Inference attestation missing model reference in public inputs".to_string()
                    },
                });

                // Verify provider signature simulation (check for sig prefix in proof_data)
                let has_sig = submission.proof_data.len() >= 64; // 32 bytes minimum signature
                checks.push(ProofCheck {
                    name: "inference_provider_signature".to_string(),
                    passed: has_sig,
                    details: if has_sig {
                        "Provider signature present in proof data".to_string()
                    } else {
                        "Provider signature missing or too short".to_string()
                    },
                });

                if !has_sig {
                    fraud_type = Some(FraudType::InvalidSignature);
                }
            }
            ProofType::ModelHashCommitment => {
                // Verify BLAKE3 hash structure
                let valid_blake3 = submission.commitment_hash.starts_with("blake3:")
                    && submission.commitment_hash.len() > 7;
                checks.push(ProofCheck {
                    name: "model_hash_blake3_format".to_string(),
                    passed: valid_blake3,
                    details: if valid_blake3 {
                        "Model hash commitment uses BLAKE3 format".to_string()
                    } else {
                        "Model hash commitment missing BLAKE3 prefix".to_string()
                    },
                });

                // Check that public inputs contain artifact references
                let has_artifacts = !submission.public_inputs.is_empty();
                checks.push(ProofCheck {
                    name: "model_artifact_references".to_string(),
                    passed: has_artifacts,
                    details: if has_artifacts {
                        format!(
                            "Model commitment references {} artifacts",
                            submission.public_inputs.len()
                        )
                    } else {
                        "Model commitment has no artifact references".to_string()
                    },
                });
            }
            ProofType::ProviderRegistration => {
                // Check that provider_id is non-empty
                let valid_provider = !submission.provider_id.is_empty();
                checks.push(ProofCheck {
                    name: "registration_provider_id".to_string(),
                    passed: valid_provider,
                    details: if valid_provider {
                        format!("Registration for provider: {}", submission.provider_id)
                    } else {
                        "Registration has empty provider_id".to_string()
                    },
                });
            }
            ProofType::StakeProof => {
                // Check for stake amount in public inputs
                let has_stake = submission
                    .public_inputs
                    .iter()
                    .any(|i| i.starts_with("stake:"));
                checks.push(ProofCheck {
                    name: "stake_amount_present".to_string(),
                    passed: has_stake,
                    details: if has_stake {
                        "Stake proof contains stake amount".to_string()
                    } else {
                        "Stake proof missing stake amount in public inputs".to_string()
                    },
                });
            }
            ProofType::SlashingProof => {
                // Check for slashing reason
                let has_reason = submission
                    .public_inputs
                    .iter()
                    .any(|i| i.starts_with("reason:"));
                checks.push(ProofCheck {
                    name: "slashing_reason_present".to_string(),
                    passed: has_reason,
                    details: if has_reason {
                        "Slashing proof contains reason".to_string()
                    } else {
                        "Slashing proof missing reason in public inputs".to_string()
                    },
                });
            }
            ProofType::ChallengeResponse => {
                // Check for challenge nonce
                let has_nonce = submission
                    .public_inputs
                    .iter()
                    .any(|i| i.starts_with("challenge:"));
                checks.push(ProofCheck {
                    name: "challenge_nonce_present".to_string(),
                    passed: has_nonce,
                    details: if has_nonce {
                        "Challenge response contains nonce".to_string()
                    } else {
                        "Challenge response missing challenge nonce".to_string()
                    },
                });
            }
        }

        // Double submission check
        if let Some(existing_id) = self.commitment_index.get(&submission.commitment_hash) {
            if *existing_id != submission_id {
                fraud_detected = true;
                fraud_type = Some(FraudType::DoubleSubmission);
                checks.push(ProofCheck {
                    name: "double_submission_check".to_string(),
                    passed: false,
                    details: format!(
                        "Commitment {} already submitted as {}",
                        submission.commitment_hash, *existing_id
                    ),
                });
            }
        }

        // Determine overall validity
        let all_passed = checks.iter().all(|c| c.passed);
        let verification_time_ms = start.elapsed().as_millis() as u64;

        // Check timeout
        if verification_time_ms > cfg.verification_timeout_ms {
            warn!(
                "Verification timeout for {}: {}ms > {}ms",
                submission_id, verification_time_ms, cfg.verification_timeout_ms
            );
        }

        // Record fraud if detected (before building result to avoid move issues)
        if fraud_detected {
            self.stats_fraud_detected.fetch_add(1, Ordering::Relaxed);
            if let Some(ref ft) = fraud_type {
                let record = FraudRecord {
                    provider_id: submission.provider_id.clone(),
                    fraud_type: ft.clone(),
                    submission_id: submission_id.to_string(),
                    detected_at: Utc::now().timestamp_millis(),
                };
                self.fraud_records
                    .insert(format!("fraud_{}", Uuid::new_v4()), record);
            }
            warn!(
                "Fraud detected for submission {}: {:?}",
                submission_id, fraud_type
            );
        }

        let result = VerificationResult {
            valid: all_passed,
            verifier: "xergon-pipeline-v1".to_string(),
            verification_time_ms,
            checks: checks.clone(),
            fraud_detected,
            fraud_type: if fraud_detected { fraud_type.clone() } else { None },
        };

        // Generate receipt
        let receipt = self.generate_receipt(submission_id, &result);

        // Update submission in the map
        if let Some(mut entry) = self.submissions.get_mut(submission_id) {
            entry.value_mut().verified = true;
            entry.value_mut().verification_result = Some(result.clone());
            entry.value_mut().gas_used = 21_000 + (checks.len() as u64) * 3_000;
            entry.value_mut().on_chain_tx_id =
                Some(format!("tx_{}", Uuid::new_v4().to_string().replace('-', "")));
        }

        // Update stats
        if all_passed {
            self.stats_total_verified.fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats_total_rejected.fetch_add(1, Ordering::Relaxed);
        }
        self.stats_verification_time_sum
            .fetch_add(verification_time_ms, Ordering::Relaxed);
        self.stats_verification_count.fetch_add(1, Ordering::Relaxed);

        info!(
            "Proof verified: id={}, valid={}, fraud={}, time={}ms",
            submission_id, all_passed, fraud_detected, verification_time_ms
        );

        // Store receipt
        self.receipts.insert(submission_id.to_string(), receipt);

        Ok(result)
    }

    /// Verify all proofs in a batch.
    pub fn verify_batch(&self, batch_id: &str) -> Result<Vec<VerificationResult>, String> {
        let mut batch = self
            .batches
            .get_mut(batch_id)
            .ok_or_else(|| format!("Batch not found: {}", batch_id))?;

        batch.status = BatchStatus::Submitted;
        let proof_ids: Vec<String> = batch.proofs.iter().map(|p| p.id.clone()).collect();
        drop(batch);

        let mut results = Vec::new();
        let mut verified_count = 0u32;
        let mut failed_count = 0u32;

        for proof_id in &proof_ids {
            match self.verify_proof(proof_id) {
                Ok(result) => {
                    if result.valid {
                        verified_count += 1;
                    } else {
                        failed_count += 1;
                    }
                    results.push(result);
                }
                Err(e) => {
                    failed_count += 1;
                    warn!("Failed to verify proof {} in batch {}: {}", proof_id, batch_id, e);
                    results.push(VerificationResult {
                        valid: false,
                        verifier: "xergon-pipeline-v1".to_string(),
                        verification_time_ms: 0,
                        checks: vec![ProofCheck {
                            name: "verification_error".to_string(),
                            passed: false,
                            details: e,
                        }],
                        fraud_detected: false,
                        fraud_type: None,
                    });
                }
            }
        }

        // Update batch status
        if let Some(mut batch) = self.batches.get_mut(batch_id) {
            if failed_count == 0 {
                batch.status = BatchStatus::AllVerified;
            } else if verified_count > 0 {
                batch.status = BatchStatus::PartiallyVerified;
            } else {
                batch.status = BatchStatus::Failed;
            }
        }

        self.stats_batches_processed.fetch_add(1, Ordering::Relaxed);

        info!(
            "Batch verified: id={}, total={}, passed={}, failed={}",
            batch_id,
            results.len(),
            verified_count,
            failed_count
        );

        Ok(results)
    }

    // -- Receipt generation ------------------------------------------------

    fn generate_receipt(&self, submission_id: &str, result: &VerificationResult) -> ProofReceipt {
        // BLAKE3 hash of submission_id + verification result
        let mut hasher = blake3::Hasher::new();
        hasher.update(submission_id.as_bytes());
        hasher.update(result.valid.to_string().as_bytes());
        hasher.update(
            result
                .checks
                .iter()
                .map(|c| c.passed.to_string())
                .collect::<Vec<_>>()
                .join(",")
                .as_bytes(),
        );
        let receipt_hash = hasher.finalize().to_hex();

        ProofReceipt {
            submission_id: submission_id.to_string(),
            receipt_hash: format!("blake3:{}", receipt_hash),
            block_height: self.current_block_height.load(Ordering::Relaxed),
            timestamp: Utc::now().timestamp_millis(),
            verifier_pubkey: "xergon-verifier-pubkey-001".to_string(),
            on_chain: result.valid,
        }
    }

    // -- Query methods -----------------------------------------------------

    /// Get a proof submission by ID.
    pub fn get_submission(&self, id: &str) -> Option<ProofSubmission> {
        self.submissions.get(id).map(|s| s.value().clone())
    }

    /// Get a verification receipt by submission ID.
    pub fn get_receipt(&self, submission_id: &str) -> Option<ProofReceipt> {
        self.receipts.get(submission_id).map(|r| r.value().clone())
    }

    /// List submissions with optional filters.
    pub fn list_submissions(
        &self,
        provider_id: Option<&str>,
        proof_type: Option<&ProofType>,
        verified: Option<bool>,
    ) -> Vec<ProofSubmission> {
        self.submissions
            .iter()
            .filter(|entry| {
                let s = entry.value();
                if let Some(pid) = provider_id {
                    if s.provider_id != pid {
                        return false;
                    }
                }
                if let Some(pt) = proof_type {
                    if &s.proof_type != pt {
                        return false;
                    }
                }
                if let Some(v) = verified {
                    if s.verified != v {
                        return false;
                    }
                }
                true
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get a batch by ID.
    pub fn get_batch(&self, batch_id: &str) -> Option<ProofBatch> {
        self.batches.get(batch_id).map(|b| b.value().clone())
    }

    /// List batches with optional status filter.
    pub fn list_batches(&self, status: Option<&BatchStatus>) -> Vec<ProofBatch> {
        self.batches
            .iter()
            .filter(|entry| {
                if let Some(s) = status {
                    &entry.value().status != s
                } else {
                    false
                }
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Detect fraud patterns for a provider.
    pub fn detect_fraud(&self, provider_id: &str) -> FraudDetectionResult {
        let mut fraud_count = 0u64;
        let mut fraud_types: Vec<FraudType> = Vec::new();
        let mut fraud_submissions: Vec<String> = Vec::new();

        // Check fraud records for this provider
        for record in self.fraud_records.iter() {
            if record.value().provider_id == provider_id {
                fraud_count += 1;
                fraud_types.push(record.value().fraud_type.clone());
                fraud_submissions.push(record.value().submission_id.clone());
            }
        }

        // Check provider's submissions for patterns
        let provider_submissions = self.list_submissions(Some(provider_id), None, None);
        let mut double_commitments = 0u64;
        let mut rejected_count = 0u64;

        for sub in &provider_submissions {
            if let Some(ref vr) = sub.verification_result {
                if !vr.valid {
                    rejected_count += 1;
                }
            }
        }

        // Check commitment reuse
        if let Some(commitments) = self.provider_commitments.get(provider_id) {
            let unique: std::collections::HashSet<_> = commitments.iter().collect();
            double_commitments = (commitments.len() - unique.len()) as u64;
        }

        // Calculate fraud score
        let total = provider_submissions.len() as f64;
        let fraud_score = if total > 0.0 {
            ((rejected_count as f64) + (fraud_count as f64)) / total
        } else {
            0.0
        };

        let cfg = self.get_config_value();
        let is_flagged = fraud_score > cfg.fraud_threshold || fraud_count > 3;

        FraudDetectionResult {
            provider_id: provider_id.to_string(),
            fraud_count,
            fraud_types,
            fraud_submissions,
            rejected_submissions: rejected_count,
            double_commitments,
            fraud_score,
            is_flagged,
            checked_at: Utc::now().timestamp_millis(),
        }
    }

    /// Get pipeline statistics.
    pub fn get_stats(&self) -> PipelineStats {
        let total_submitted = self.stats_total_submitted.load(Ordering::Relaxed);
        let total_verified = self.stats_total_verified.load(Ordering::Relaxed);
        let total_rejected = self.stats_total_rejected.load(Ordering::Relaxed);
        let fraud_detected = self.stats_fraud_detected.load(Ordering::Relaxed);
        let batches_processed = self.stats_batches_processed.load(Ordering::Relaxed);

        let verification_count = self.stats_verification_count.load(Ordering::Relaxed);
        let verification_time_sum = self.stats_verification_time_sum.load(Ordering::Relaxed);
        let avg_verification_ms = if verification_count > 0 {
            verification_time_sum / verification_count
        } else {
            0
        };

        PipelineStats {
            total_submitted,
            total_verified,
            total_rejected,
            fraud_detected,
            avg_verification_ms,
            batches_processed,
        }
    }
}

impl Default for ProofPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// Fraud Detection Result
// ================================================================

/// Result of fraud detection analysis for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudDetectionResult {
    pub provider_id: String,
    pub fraud_count: u64,
    pub fraud_types: Vec<FraudType>,
    pub fraud_submissions: Vec<String>,
    pub rejected_submissions: u64,
    pub double_commitments: u64,
    pub fraud_score: f64,
    pub is_flagged: bool,
    pub checked_at: i64,
}

// ================================================================
// REST API Types
// ================================================================

#[derive(Debug, Deserialize)]
pub struct SubmitProofRequest {
    pub provider_id: String,
    pub proof_type: ProofType,
    pub proof_data: String,
    pub public_inputs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SubmitProofResponse {
    pub success: bool,
    pub submission_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitBatchRequest {
    pub proofs: Vec<BatchProofEntry>,
}

#[derive(Debug, Deserialize)]
pub struct BatchProofEntry {
    pub provider_id: String,
    pub proof_type: ProofType,
    pub proof_data: String,
    pub public_inputs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SubmitBatchResponse {
    pub success: bool,
    pub batch_id: String,
    pub proof_count: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub success: bool,
    pub result: Option<VerificationResult>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSubmissionsQuery {
    pub provider_id: Option<String>,
    pub proof_type: Option<String>,
    pub verified: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListBatchesQuery {
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub stats: PipelineStats,
}

// ================================================================
// REST Handlers
// ================================================================

/// POST /v1/proof-pipeline/submit - Submit a proof
async fn handle_submit_proof(
    State(pipeline): State<Arc<ProofPipeline>>,
    Json(req): Json<SubmitProofRequest>,
) -> Json<SubmitProofResponse> {
    match pipeline.submit_proof(
        &req.provider_id,
        req.proof_type,
        &req.proof_data,
        req.public_inputs,
    ) {
        Ok(submission) => Json(SubmitProofResponse {
            success: true,
            submission_id: submission.id,
            message: "Proof submitted successfully".to_string(),
        }),
        Err(e) => Json(SubmitProofResponse {
            success: false,
            submission_id: String::new(),
            message: e,
        }),
    }
}

/// POST /v1/proof-pipeline/batch - Submit a batch
async fn handle_submit_batch(
    State(pipeline): State<Arc<ProofPipeline>>,
    Json(req): Json<SubmitBatchRequest>,
) -> Json<SubmitBatchResponse> {
    let cfg = pipeline.get_config();

    if req.proofs.len() > cfg.max_batch_size as usize {
        return Json(SubmitBatchResponse {
            success: false,
            batch_id: String::new(),
            proof_count: req.proofs.len(),
            message: format!("Batch size {} exceeds max {}", req.proofs.len(), cfg.max_batch_size),
        });
    }

    // Build ProofSubmission entries
    let mut submissions = Vec::new();
    for entry in &req.proofs {
        let mut hasher = blake3::Hasher::new();
        hasher.update(entry.proof_data.as_bytes());
        for input in &entry.public_inputs {
            hasher.update(input.as_bytes());
        }
        let commitment_hash = format!("blake3:{}", hasher.finalize().to_hex());

        let id = format!("ps_{}", Uuid::new_v4().to_string().replace('-', ""));

        submissions.push(ProofSubmission {
            id: id.clone(),
            provider_id: entry.provider_id.clone(),
            proof_type: entry.proof_type.clone(),
            proof_data: entry.proof_data.clone(),
            commitment_hash,
            public_inputs: entry.public_inputs.clone(),
            submitted_at: Utc::now().timestamp_millis(),
            verified: false,
            verification_result: None,
            on_chain_tx_id: None,
            gas_used: 0,
        });
    }

    match pipeline.submit_batch(submissions) {
        Ok(batch) => Json(SubmitBatchResponse {
            success: true,
            batch_id: batch.batch_id,
            proof_count: batch.proofs.len(),
            message: "Batch submitted successfully".to_string(),
        }),
        Err(e) => Json(SubmitBatchResponse {
            success: false,
            batch_id: String::new(),
            proof_count: req.proofs.len(),
            message: e,
        }),
    }
}

/// POST /v1/proof-pipeline/verify/:id - Verify a proof
async fn handle_verify_proof(
    State(pipeline): State<Arc<ProofPipeline>>,
    Path(id): Path<String>,
) -> Json<VerifyResponse> {
    match pipeline.verify_proof(&id) {
        Ok(result) => Json(VerifyResponse {
            success: true,
            result: Some(result),
            message: "Proof verified successfully".to_string(),
        }),
        Err(e) => Json(VerifyResponse {
            success: false,
            result: None,
            message: e,
        }),
    }
}

/// POST /v1/proof-pipeline/batch-verify/:batch_id - Verify a batch
async fn handle_verify_batch(
    State(pipeline): State<Arc<ProofPipeline>>,
    Path(batch_id): Path<String>,
) -> Json<serde_json::Value> {
    match pipeline.verify_batch(&batch_id) {
        Ok(results) => {
            let passed = results.iter().filter(|r| r.valid).count();
            let failed = results.len() - passed;
            Json(serde_json::json!({
                "success": true,
                "batch_id": batch_id,
                "total": results.len(),
                "passed": passed,
                "failed": failed,
                "results": results,
                "message": format!("Batch verified: {} passed, {} failed", passed, failed)
            }))
        }
        Err(e) => Json(serde_json::json!({
            "success": false,
            "batch_id": batch_id,
            "message": e
        })),
    }
}

/// GET /v1/proof-pipeline/submission/:id - Get submission details
async fn handle_get_submission(
    State(pipeline): State<Arc<ProofPipeline>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match pipeline.get_submission(&id) {
        Some(submission) => Json(serde_json::json!({
            "success": true,
            "submission": submission
        })),
        None => Json(serde_json::json!({
            "success": false,
            "message": format!("Submission not found: {}", id)
        })),
    }
}

/// GET /v1/proof-pipeline/receipt/:id - Get verification receipt
async fn handle_get_receipt(
    State(pipeline): State<Arc<ProofPipeline>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match pipeline.get_receipt(&id) {
        Some(receipt) => Json(serde_json::json!({
            "success": true,
            "receipt": receipt
        })),
        None => Json(serde_json::json!({
            "success": false,
            "message": format!("Receipt not found for submission: {}", id)
        })),
    }
}

/// GET /v1/proof-pipeline/submissions - List submissions
async fn handle_list_submissions(
    State(pipeline): State<Arc<ProofPipeline>>,
    Query(query): Query<ListSubmissionsQuery>,
) -> Json<serde_json::Value> {
    let proof_type: Option<ProofType> = query.proof_type.as_deref().and_then(|s| {
        match s.to_lowercase().as_str() {
            "ponw" => Some(ProofType::PoNW),
            "inference_attestation" => Some(ProofType::InferenceAttestation),
            "model_hash_commitment" => Some(ProofType::ModelHashCommitment),
            "provider_registration" => Some(ProofType::ProviderRegistration),
            "stake_proof" => Some(ProofType::StakeProof),
            "slashing_proof" => Some(ProofType::SlashingProof),
            "challenge_response" => Some(ProofType::ChallengeResponse),
            _ => None,
        }
    });

    let verified: Option<bool> = query.verified.as_deref().and_then(|s| s.parse().ok());

    let submissions = pipeline.list_submissions(
        query.provider_id.as_deref(),
        proof_type.as_ref(),
        verified,
    );

    Json(serde_json::json!({
        "success": true,
        "count": submissions.len(),
        "submissions": submissions
    }))
}

/// GET /v1/proof-pipeline/batches - List batches
async fn handle_list_batches(
    State(pipeline): State<Arc<ProofPipeline>>,
    Query(query): Query<ListBatchesQuery>,
) -> Json<serde_json::Value> {
    let status: Option<BatchStatus> = query.status.as_deref().and_then(|s| {
        match s.to_lowercase().as_str() {
            "pending" => Some(BatchStatus::Pending),
            "submitted" => Some(BatchStatus::Submitted),
            "partially_verified" => Some(BatchStatus::PartiallyVerified),
            "all_verified" => Some(BatchStatus::AllVerified),
            "failed" => Some(BatchStatus::Failed),
            _ => None,
        }
    });

    let batches = pipeline.list_batches(status.as_ref());

    Json(serde_json::json!({
        "success": true,
        "count": batches.len(),
        "batches": batches
    }))
}

/// GET /v1/proof-pipeline/fraud-check/:provider_id - Check for fraud
async fn handle_fraud_check(
    State(pipeline): State<Arc<ProofPipeline>>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    let result = pipeline.detect_fraud(&provider_id);
    Json(serde_json::json!({
        "success": true,
        "fraud_check": result
    }))
}

/// GET /v1/proof-pipeline/stats - Get pipeline statistics
async fn handle_stats(
    State(pipeline): State<Arc<ProofPipeline>>,
) -> Json<StatsResponse> {
    Json(StatsResponse {
        stats: pipeline.get_stats(),
    })
}

// ================================================================
// Router
// ================================================================

/// Build the axum Router for the proof pipeline module.
pub fn router(state: Arc<ProofPipeline>) -> Router {
    Router::new()
        .route(
            "/v1/proof-pipeline/submit",
            post(handle_submit_proof),
        )
        .route(
            "/v1/proof-pipeline/batch",
            post(handle_submit_batch),
        )
        .route(
            "/v1/proof-pipeline/verify/:id",
            post(handle_verify_proof),
        )
        .route(
            "/v1/proof-pipeline/batch-verify/:batch_id",
            post(handle_verify_batch),
        )
        .route(
            "/v1/proof-pipeline/submission/:id",
            get(handle_get_submission),
        )
        .route(
            "/v1/proof-pipeline/receipt/:id",
            get(handle_get_receipt),
        )
        .route(
            "/v1/proof-pipeline/submissions",
            get(handle_list_submissions),
        )
        .route(
            "/v1/proof-pipeline/batches",
            get(handle_list_batches),
        )
        .route(
            "/v1/proof-pipeline/fraud-check/:provider_id",
            get(handle_fraud_check),
        )
        .route(
            "/v1/proof-pipeline/stats",
            get(handle_stats),
        )
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pipeline() -> ProofPipeline {
        ProofPipeline::with_config(PipelineConfig {
            auto_submit: false, // Disable auto-submit for tests
            ..Default::default()
        })
    }

    fn valid_hex_proof(len_bytes: usize) -> String {
        "aabbccdd".repeat((len_bytes + 3) / 4)
    }

    #[test]
    fn test_submit_ponw_proof() {
        let pipeline = make_pipeline();
        let proof_data = valid_hex_proof(64);
        let result = pipeline.submit_proof(
            "provider_001",
            ProofType::PoNW,
            &proof_data,
            vec!["timestamp:1700000000000".to_string()],
        );

        assert!(result.is_ok());
        let submission = result.unwrap();
        assert_eq!(submission.provider_id, "provider_001");
        assert_eq!(submission.proof_type, ProofType::PoNW);
        assert!(!submission.verified);
        assert!(submission.id.starts_with("ps_"));
        assert!(submission.commitment_hash.starts_with("blake3:"));
    }

    #[test]
    fn test_submit_inference_attestation() {
        let pipeline = make_pipeline();
        let proof_data = valid_hex_proof(64);
        let result = pipeline.submit_proof(
            "provider_002",
            ProofType::InferenceAttestation,
            &proof_data,
            vec!["model:gpt-test-v1".to_string(), "input_hash:abc123".to_string()],
        );

        assert!(result.is_ok());
        let submission = result.unwrap();
        assert_eq!(submission.proof_type, ProofType::InferenceAttestation);
        assert_eq!(submission.public_inputs.len(), 2);
    }

    #[test]
    fn test_verify_valid_proof() {
        let pipeline = make_pipeline();
        let proof_data = valid_hex_proof(64);
        let submission = pipeline
            .submit_proof(
                "provider_003",
                ProofType::PoNW,
                &proof_data,
                vec!["timestamp:1700000000000".to_string()],
            )
            .unwrap();

        let result = pipeline.verify_proof(&submission.id);
        assert!(result.is_ok());
        let vr = result.unwrap();
        assert!(vr.valid);
        assert!(!vr.fraud_detected);
        assert!(vr.verification_time_ms < 1000);
        assert!(vr.checks.len() >= 3); // commitment, freshness, hex, ponw-specific
    }

    #[test]
    fn test_verify_invalid_proof() {
        let pipeline = make_pipeline();
        let submission = pipeline
            .submit_proof(
                "provider_004",
                ProofType::PoNW,
                &valid_hex_proof(64),
                vec!["timestamp:1700000000000".to_string()],
            )
            .unwrap();

        // Tamper with the commitment hash to make it invalid
        if let Some(mut sub) = pipeline.submissions.get_mut(&submission.id) {
            sub.commitment_hash = "blake3:deadbeef".to_string();
        }

        let result = pipeline.verify_proof(&submission.id);
        assert!(result.is_ok());
        let vr = result.unwrap();
        assert!(!vr.valid);
        assert!(vr.fraud_detected);
        assert_eq!(vr.fraud_type, Some(FraudType::InvalidCommitment));
    }

    #[test]
    fn test_batch_submission() {
        let pipeline = make_pipeline();
        let proofs: Vec<ProofSubmission> = (0..3)
            .map(|i| {
                let proof_data = valid_hex_proof(64);
                let mut hasher = blake3::Hasher::new();
                hasher.update(proof_data.as_bytes());
                let commitment_hash = format!("blake3:{}", hasher.finalize().to_hex());
                ProofSubmission {
                    id: format!("ps_batch_test_{}", i),
                    provider_id: format!("provider_batch_{}", i),
                    proof_type: ProofType::PoNW,
                    proof_data,
                    commitment_hash,
                    public_inputs: vec![format!("timestamp:{}", 1700000000000i64 + i as i64)],
                    submitted_at: Utc::now().timestamp_millis(),
                    verified: false,
                    verification_result: None,
                    on_chain_tx_id: None,
                    gas_used: 0,
                }
            })
            .collect();

        let result = pipeline.submit_batch(proofs);
        assert!(result.is_ok());
        let batch = result.unwrap();
        assert_eq!(batch.proofs.len(), 3);
        assert_eq!(batch.status, BatchStatus::Submitted);
        assert!(batch.batch_id.starts_with("batch_"));
    }

    #[test]
    fn test_batch_verification() {
        let pipeline = make_pipeline();
        let proofs: Vec<ProofSubmission> = (0..3)
            .map(|i| {
                let proof_data = valid_hex_proof(64);
                let mut hasher = blake3::Hasher::new();
                hasher.update(proof_data.as_bytes());
                hasher.update(format!("timestamp:{}", 1700000000000i64 + i as i64).as_bytes());
                let commitment_hash = format!("blake3:{}", hasher.finalize().to_hex());
                ProofSubmission {
                    id: format!("ps_bv_{}", i),
                    provider_id: "provider_bv".to_string(),
                    proof_type: ProofType::PoNW,
                    proof_data,
                    commitment_hash,
                    public_inputs: vec![format!("timestamp:{}", 1700000000000i64 + i as i64)],
                    submitted_at: Utc::now().timestamp_millis(),
                    verified: false,
                    verification_result: None,
                    on_chain_tx_id: None,
                    gas_used: 0,
                }
            })
            .collect();

        let batch = pipeline.submit_batch(proofs).unwrap();
        let results = pipeline.verify_batch(&batch.batch_id).unwrap();

        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.valid));

        let stats = pipeline.get_stats();
        assert_eq!(stats.total_verified, 3);
        assert_eq!(stats.batches_processed, 1);
    }

    #[test]
    fn test_double_submission_detection() {
        let pipeline = make_pipeline();
        let proof_data = valid_hex_proof(64);

        let result1 = pipeline.submit_proof(
            "provider_ds",
            ProofType::PoNW,
            &proof_data,
            vec!["timestamp:1700000000000".to_string()],
        );
        assert!(result1.is_ok());

        // Same provider, same proof data -> double submission
        let result2 = pipeline.submit_proof(
            "provider_ds",
            ProofType::PoNW,
            &proof_data,
            vec!["timestamp:1700000000000".to_string()],
        );
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("Double submission"));
    }

    #[test]
    fn test_replay_attack_detection() {
        let pipeline = make_pipeline();
        let proof_data = valid_hex_proof(64);

        // First submission succeeds
        let result1 = pipeline.submit_proof(
            "provider_replay",
            ProofType::PoNW,
            &proof_data,
            vec!["timestamp:1700000000000".to_string()],
        );
        assert!(result1.is_ok());

        // Second submission with same data triggers replay detection
        // (this tests the provider_commitments tracking)
        let result2 = pipeline.submit_proof(
            "provider_replay",
            ProofType::PoNW,
            &proof_data,
            vec!["timestamp:1700000000000".to_string()],
        );
        assert!(result2.is_err());
    }

    #[test]
    fn test_fraud_detection() {
        let pipeline = make_pipeline();

        // Submit and verify an invalid proof (tampered commitment)
        let proof_data = valid_hex_proof(64);
        let submission = pipeline
            .submit_proof(
                "provider_fraud",
                ProofType::PoNW,
                &proof_data,
                vec!["timestamp:1700000000000".to_string()],
            )
            .unwrap();

        // Tamper with commitment
        if let Some(mut sub) = pipeline.submissions.get_mut(&submission.id) {
            sub.commitment_hash = "blake3:deadbeef".to_string();
        }

        pipeline.verify_proof(&submission.id).unwrap();

        // Check fraud detection
        let fraud_result = pipeline.detect_fraud("provider_fraud");
        assert_eq!(fraud_result.fraud_count, 1);
        assert!(fraud_result.fraud_types.contains(&FraudType::InvalidCommitment));
    }

    #[test]
    fn test_receipt_generation() {
        let pipeline = make_pipeline();
        let proof_data = valid_hex_proof(64);
        let submission = pipeline
            .submit_proof(
                "provider_receipt",
                ProofType::PoNW,
                &proof_data,
                vec!["timestamp:1700000000000".to_string()],
            )
            .unwrap();

        pipeline.verify_proof(&submission.id).unwrap();

        let receipt = pipeline.get_receipt(&submission.id);
        assert!(receipt.is_some());
        let r = receipt.unwrap();
        assert_eq!(r.submission_id, submission.id);
        assert!(r.receipt_hash.starts_with("blake3:"));
        assert!(r.on_chain);
        assert!(r.block_height > 0);
        assert!(!r.verifier_pubkey.is_empty());
    }

    #[test]
    fn test_submission_filtering() {
        let pipeline = make_pipeline();

        // Submit different types
        pipeline
            .submit_proof(
                "provider_filter",
                ProofType::PoNW,
                &valid_hex_proof(64),
                vec!["timestamp:1700000000000".to_string()],
            )
            .unwrap();
        pipeline
            .submit_proof(
                "provider_filter",
                ProofType::InferenceAttestation,
                &valid_hex_proof(64),
                vec!["model:test".to_string()],
            )
            .unwrap();
        pipeline
            .submit_proof(
                "provider_other",
                ProofType::PoNW,
                &valid_hex_proof(64),
                vec!["timestamp:1700000000000".to_string()],
            )
            .unwrap();

        // Filter by provider
        let by_provider = pipeline.list_submissions(Some("provider_filter"), None, None);
        assert_eq!(by_provider.len(), 2);

        // Filter by type
        let by_type = pipeline.list_submissions(None, Some(&ProofType::PoNW), None);
        assert_eq!(by_type.len(), 2);

        // Filter by verified (none verified yet since auto_submit is off)
        let verified = pipeline.list_submissions(None, None, Some(true));
        assert_eq!(verified.len(), 0);

        let unverified = pipeline.list_submissions(None, None, Some(false));
        assert!(unverified.len() >= 3);
    }

    #[test]
    fn test_config_update() {
        let pipeline = make_pipeline();

        let initial = pipeline.get_config();
        assert_eq!(initial.max_batch_size, 100);

        let new_config = PipelineConfig {
            max_batch_size: 50,
            verification_timeout_ms: 3000,
            auto_submit: false,
            fraud_threshold: 0.9,
            max_retries: 5,
        };
        pipeline.update_config(new_config.clone());

        let updated = pipeline.get_config();
        assert_eq!(updated.max_batch_size, 50);
        assert_eq!(updated.verification_timeout_ms, 3000);
        assert_eq!(updated.fraud_threshold, 0.9);
        assert_eq!(updated.max_retries, 5);
    }

    #[test]
    fn test_stats_tracking() {
        let pipeline = make_pipeline();

        // Submit and verify proofs
        for i in 0..5 {
            let submission = pipeline
                .submit_proof(
                    &format!("provider_stats_{}", i),
                    ProofType::PoNW,
                    &valid_hex_proof(64),
                    vec!["timestamp:1700000000000".to_string()],
                )
                .unwrap();
            pipeline.verify_proof(&submission.id).unwrap();
        }

        let stats = pipeline.get_stats();
        assert_eq!(stats.total_submitted, 5);
        assert_eq!(stats.total_verified, 5);
        assert_eq!(stats.total_rejected, 0);
        assert_eq!(stats.fraud_detected, 0);
        // avg_verification_ms can be 0 on fast machines
    }

    #[test]
    fn test_concurrent_submissions() {
        use std::thread;

        let pipeline = Arc::new(make_pipeline());
        let mut handles = Vec::new();

        for i in 0..10 {
            let p = pipeline.clone();
            handles.push(thread::spawn(move || {
                let proof_data = valid_hex_proof(64 + i);
                p.submit_proof(
                    &format!("provider_concurrent_{}", i),
                    ProofType::PoNW,
                    &proof_data,
                    vec![format!("timestamp:{}", 1700000000000i64 + i as i64)],
                )
            }));
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, 10);

        let stats = pipeline.get_stats();
        assert_eq!(stats.total_submitted, 10);
    }

    #[test]
    fn test_stale_proof_rejection() {
        let pipeline = make_pipeline();

        // Create a submission with an old timestamp
        let proof_data = valid_hex_proof(64);
        let submission = pipeline
            .submit_proof(
                "provider_stale",
                ProofType::PoNW,
                &proof_data,
                vec!["timestamp:1700000000000".to_string()],
            )
            .unwrap();

        // Manually set the submitted_at to be very old (2 hours ago)
        if let Some(mut sub) = pipeline.submissions.get_mut(&submission.id) {
            sub.submitted_at = Utc::now().timestamp_millis() - 7_200_000; // 2 hours
        }

        let result = pipeline.verify_proof(&submission.id).unwrap();
        assert!(!result.valid);
        assert!(result.fraud_detected);

        // Check that StaleProof fraud type is set
        let stale_check = result.checks.iter().find(|c| c.name == "timestamp_freshness");
        assert!(stale_check.is_some());
        assert!(!stale_check.unwrap().passed);
    }
}
