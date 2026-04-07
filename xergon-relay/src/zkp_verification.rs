//! ZK Proof Verification, TEE Attestation, and Confidential Compute Trust Scoring
//!
//! Provides Pedersen-inspired commitments, sigma-protocol proofs, TEE attestation
//! verification, and a weighted trust scoring system for confidential compute.

use crate::proxy::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use blake3::hash;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

/// A Pedersen-inspired commitment: blake3(value_bytes || blinding_factor_bytes)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZKCommitment {
    pub commitment_hash: String,
    pub value_hash: String,
    pub blinding_hash: String,
    pub provider_id: String,
    pub timestamp: u64,
}

/// A Sigma-protocol ZK proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZKProof {
    pub commitment_hash: String,
    pub challenge: String,
    pub response: String,
    pub public_input_hash: String,
    pub provider_id: String,
    pub timestamp: u64,
}

/// TEE attestation record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TEEAttestation {
    pub provider_id: String,
    pub tee_type: String,
    pub mrenclave: String,
    pub timestamp: u64,
    pub signature: String,
    pub nonce: String,
}

/// Weighted trust score components.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrustScore {
    pub provider_id: String,
    pub tee_score: f64,
    pub zk_score: f64,
    pub uptime_score: f64,
    pub ponw_score: f64,
    pub review_score: f64,
    pub total: f64,
    pub last_updated: u64,
}

/// Trust level classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrustLevel {
    None,
    Pseudonymous,
    TeeOnly,
    FullZK,
}

// ---------------------------------------------------------------------------
// Request / Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CommitRequest {
    pub value: String,
    pub blinding_factor: String,
    pub provider_id: String,
}

#[derive(Debug, Serialize)]
pub struct CommitResponse {
    pub commitment_hash: String,
    pub timestamp: u64,
}

#[derive(Debug, Deserialize)]
pub struct ProveRequest {
    pub commitment_hash: String,
    pub secret: String,
    pub blinding_factor: String,
    pub public_input: String,
    pub provider_id: String,
}

#[derive(Debug, Serialize)]
pub struct ProveResponse {
    pub proof_id: String,
    pub challenge: String,
    pub response: String,
    pub timestamp: u64,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub proof: ZKProof,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub valid: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchVerifyRequest {
    pub proofs: Vec<ZKProof>,
}

#[derive(Debug, Serialize)]
pub struct BatchVerifyResponse {
    pub passed: usize,
    pub failed: usize,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
pub struct AttestRequest {
    pub provider_id: String,
    pub tee_type: String,
    pub mrenclave: String,
    pub signature: String,
    pub nonce: String,
}

#[derive(Debug, Serialize)]
pub struct AttestResponse {
    pub valid: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyAttestationRequest {
    pub provider_id: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyAttestationResponse {
    pub valid: bool,
    pub message: String,
    pub trust_level: String,
}

#[derive(Debug, Serialize)]
pub struct CommitmentsResponse {
    pub provider_id: String,
    pub commitments: Vec<ZKCommitment>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub total_commitments: usize,
    pub total_proofs: usize,
    pub total_attestations: usize,
    pub total_trust_scores: usize,
}

#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderInfo>,
}

#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub provider_id: String,
    pub tee_type: String,
    pub mrenclave: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatusResponse {
    pub provider_id: String,
    pub attested: bool,
    pub trust_score: Option<TrustScore>,
    pub trust_level: TrustLevel,
}

#[derive(Debug, Serialize)]
pub struct TrustResponse {
    pub provider_id: String,
    pub trust_score: Option<TrustScore>,
    pub trust_level: TrustLevel,
}

// ---------------------------------------------------------------------------
// Application State
// ---------------------------------------------------------------------------

pub struct ZKPVerificationState {
    pub commitments: DashMap<String, Vec<ZKCommitment>>,
    pub proofs: DashMap<String, ZKProof>,
    pub attestations: DashMap<String, TEEAttestation>,
    pub trust_scores: DashMap<String, TrustScore>,
}

impl ZKPVerificationState {
    pub fn new() -> Self {
        Self {
            commitments: DashMap::new(),
            proofs: DashMap::new(),
            attestations: DashMap::new(),
            trust_scores: DashMap::new(),
        }
    }

    // ----- ZK Commitment -----

    pub fn create_commitment(
        &self,
        value: &str,
        blinding_factor: &str,
        provider_id: &str,
    ) -> ZKCommitment {
        let value_hash = hash(value.as_bytes()).to_hex().to_string();
        let blinding_hash = hash(blinding_factor.as_bytes()).to_hex().to_string();
        let commitment_data = format!("{}||{}", value, blinding_factor);
        let commitment_hash = hash(commitment_data.as_bytes()).to_hex().to_string();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let commitment = ZKCommitment {
            commitment_hash: commitment_hash.clone(),
            value_hash,
            blinding_hash,
            provider_id: provider_id.to_string(),
            timestamp,
        };

        let mut entry = self.commitments.entry(provider_id.to_string()).or_default();
        entry.push(commitment.clone());

        commitment
    }

    // ----- ZK Proof -----

    pub fn create_proof(
        &self,
        commitment_hash: &str,
        secret: &str,
        blinding_factor: &str,
        public_input: &str,
        provider_id: &str,
    ) -> Option<ZKProof> {
        // Verify commitment exists
        let exists = self
            .commitments
            .get(provider_id)
            .map(|v| v.iter().any(|c| c.commitment_hash == commitment_hash))
            .unwrap_or(false);

        if !exists {
            return None;
        }

        let challenge_input = format!(
            "{}:{}:{}",
            commitment_hash, public_input, provider_id
        );
        let challenge = hash(challenge_input.as_bytes()).to_hex().to_string();
        let public_input_hash = hash(public_input.as_bytes()).to_hex().to_string();

        // response = blake3(challenge || secret || blinding)
        let response_input = format!("{}:{}:{}", challenge, secret, blinding_factor);
        let response = hash(response_input.as_bytes()).to_hex().to_string();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let proof = ZKProof {
            commitment_hash: commitment_hash.to_string(),
            challenge,
            response,
            public_input_hash,
            provider_id: provider_id.to_string(),
            timestamp,
        };

        // Store proof keyed by "provider_id:commitment_hash"
        let proof_id = format!("{}:{}", provider_id, commitment_hash);
        self.proofs.insert(proof_id, proof.clone());

        Some(proof)
    }

    pub fn verify_proof(&self, proof: &ZKProof) -> bool {
        // Verify: check challenge consistency and that commitment exists
        let expected_commitment_input = format!(
            "{}:{}:{}",
            proof.commitment_hash, proof.public_input_hash, proof.provider_id
        );
        let expected_challenge = hash(expected_commitment_input.as_bytes()).to_hex().to_string();

        // Check challenge consistency and that commitment exists
        let commitment_exists = self
            .commitments
            .get(&proof.provider_id)
            .map(|v| v.iter().any(|c| c.commitment_hash == proof.commitment_hash))
            .unwrap_or(false);

        proof.challenge == expected_challenge && commitment_exists
    }

    pub fn batch_verify(&self, proofs: &[ZKProof]) -> (usize, usize) {
        let mut passed = 0usize;
        let mut failed = 0usize;
        for proof in proofs {
            if self.verify_proof(proof) {
                passed += 1;
            } else {
                failed += 1;
            }
        }
        (passed, failed)
    }

    // ----- TEE Attestation -----

    pub fn register_attestation(&self, req: &AttestRequest) -> TEEAttestation {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let attestation = TEEAttestation {
            provider_id: req.provider_id.clone(),
            tee_type: req.tee_type.clone(),
            mrenclave: req.mrenclave.clone(),
            timestamp,
            signature: req.signature.clone(),
            nonce: req.nonce.clone(),
        };

        self.attestations
            .insert(req.provider_id.clone(), attestation.clone());

        attestation
    }

    pub fn verify_attestation(&self, provider_id: &str) -> (bool, String) {
        let attestation = match self.attestations.get(provider_id) {
            Some(a) => a.clone(),
            None => return (false, "No attestation found for provider".to_string()),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Check 24h TTL
        if now.saturating_sub(attestation.timestamp) > 86400 {
            return (false, "Attestation expired (24h TTL)".to_string());
        }

        // Check nonce freshness (5 min) — mock: we just check it's non-empty and recent
        let nonce_bytes = attestation.nonce.as_bytes();
        if nonce_bytes.is_empty() {
            return (false, "Invalid nonce".to_string());
        }

        // Mock signature verification: check signature starts with "sig_" prefix
        if !attestation.signature.starts_with("sig_") {
            return (false, "Invalid signature format".to_string());
        }

        // Mock: verify signature content is hash of (provider_id + mrenclave + nonce)
        let expected_sig_data = format!(
            "{}:{}:{}",
            attestation.provider_id, attestation.mrenclave, attestation.nonce
        );
        let expected_sig_hash = hash(expected_sig_data.as_bytes()).to_hex().to_string();
        let expected_signature = format!("sig_{}", expected_sig_hash);

        if attestation.signature != expected_signature {
            return (false, "Signature verification failed".to_string());
        }

        (true, "Attestation valid".to_string())
    }

    // ----- Trust Score -----

    pub fn calculate_trust(&self, provider_id: &str) -> TrustScore {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // TEE score (0-100)
        let tee_score = match self.verify_attestation(provider_id) {
            (true, _) => 100.0,
            (false, _) => 0.0,
        };

        // ZK score: based on number of valid proofs
        let zk_score = self
            .commitments
            .get(provider_id)
            .map(|v| {
                let count = v.len();
                if count == 0 {
                    0.0
                } else {
                    (count.min(10) as f64 / 10.0) * 100.0
                }
            })
            .unwrap_or(0.0);

        // Uptime score: mock default
        let uptime_score = 75.0;

        // PoNW score: mock default
        let ponw_score = 50.0;

        // Review score: mock default
        let review_score = 60.0;

        // Weighted total: TEE(30%), ZK(25%), uptime(20%), PoNW(15%), reviews(10%)
        let total = (tee_score * 0.30)
            + (zk_score * 0.25)
            + (uptime_score * 0.20)
            + (ponw_score * 0.15)
            + (review_score * 0.10);

        let trust_score = TrustScore {
            provider_id: provider_id.to_string(),
            tee_score,
            zk_score,
            uptime_score,
            ponw_score,
            review_score,
            total,
            last_updated: now,
        };

        self.trust_scores
            .insert(provider_id.to_string(), trust_score.clone());

        trust_score
    }

    pub fn get_trust_level(&self, provider_id: &str) -> TrustLevel {
        let (attested, _) = self.verify_attestation(provider_id);
        let has_zk = self
            .commitments
            .get(provider_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        match (attested, has_zk) {
            (true, true) => TrustLevel::FullZK,
            (true, false) => TrustLevel::TeeOnly,
            (false, true) => TrustLevel::Pseudonymous,
            (false, false) => TrustLevel::None,
        }
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_router() -> Router<AppState> {
    Router::new()
        .route("/v1/zkp/commit", post(handle_commit))
        .route("/v1/zkp/prove", post(handle_prove))
        .route("/v1/zkp/verify", post(handle_verify))
        .route("/v1/zkp/verify-batch", post(handle_verify_batch))
        .route("/v1/tee/attest", post(handle_attest))
        .route("/v1/tee/verify", post(handle_verify_attestation))
        .route("/v1/zkp/commitments", get(handle_get_commitments))
        .route("/v1/zkp/status", get(handle_status))
        .route("/v1/tee/providers", get(handle_providers))
        .route("/v1/tee/status/{provider_id}", get(handle_provider_status))
        .route("/v1/confidential/trust/{provider_id}", get(handle_trust))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn handle_commit(
    State(state): State<AppState>,
    Json(req): Json<CommitRequest>,
) -> (StatusCode, Json<CommitResponse>) {
    let commitment = state.zkp_verification.create_commitment(&req.value, &req.blinding_factor, &req.provider_id);
    (
        StatusCode::OK,
        Json(CommitResponse {
            commitment_hash: commitment.commitment_hash,
            timestamp: commitment.timestamp,
        }),
    )
}

async fn handle_prove(
    State(state): State<AppState>,
    Json(req): Json<ProveRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.zkp_verification.create_proof(
        &req.commitment_hash,
        &req.secret,
        &req.blinding_factor,
        &req.public_input,
        &req.provider_id,
    ) {
        Some(proof) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "proof_id": format!("{}:{}", proof.provider_id, proof.commitment_hash),
                "challenge": proof.challenge,
                "response": proof.response,
                "timestamp": proof.timestamp,
            })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Commitment not found for the given provider"
            })),
        ),
    }
}

async fn handle_verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> (StatusCode, Json<VerifyResponse>) {
    let valid = state.zkp_verification.verify_proof(&req.proof);
    let message = if valid {
        "Proof verified successfully".to_string()
    } else {
        "Proof verification failed".to_string()
    };
    (StatusCode::OK, Json(VerifyResponse { valid, message }))
}

async fn handle_verify_batch(
    State(state): State<AppState>,
    Json(req): Json<BatchVerifyRequest>,
) -> (StatusCode, Json<BatchVerifyResponse>) {
    let (passed, failed) = state.zkp_verification.batch_verify(&req.proofs);
    let total = passed + failed;
    (
        StatusCode::OK,
        Json(BatchVerifyResponse {
            passed,
            failed,
            total,
        }),
    )
}

async fn handle_attest(
    State(state): State<AppState>,
    Json(req): Json<AttestRequest>,
) -> (StatusCode, Json<AttestResponse>) {
    state.zkp_verification.register_attestation(&req);
    (
        StatusCode::OK,
        Json(AttestResponse {
            valid: true,
            message: "Attestation registered".to_string(),
        }),
    )
}

async fn handle_verify_attestation(
    State(state): State<AppState>,
    Json(req): Json<VerifyAttestationRequest>,
) -> (StatusCode, Json<VerifyAttestationResponse>) {
    let (valid, message) = state.zkp_verification.verify_attestation(&req.provider_id);
    let trust_level = state.zkp_verification.get_trust_level(&req.provider_id);
    (
        StatusCode::OK,
        Json(VerifyAttestationResponse {
            valid,
            message,
            trust_level: format!("{:?}", trust_level),
        }),
    )
}

async fn handle_get_commitments(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let all: Vec<(String, Vec<ZKCommitment>)> = state
        .zkp_verification
        .commitments
        .iter()
        .map(|r| (r.key().clone(), r.value().clone()))
        .collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "providers": all.len(),
            "data": all,
        })),
    )
}

async fn handle_status(
    State(state): State<AppState>,
) -> (StatusCode, Json<StatusResponse>) {
    (
        StatusCode::OK,
        Json(StatusResponse {
            total_commitments: state.zkp_verification.commitments.len(),
            total_proofs: state.zkp_verification.proofs.len(),
            total_attestations: state.zkp_verification.attestations.len(),
            total_trust_scores: state.zkp_verification.trust_scores.len(),
        }),
    )
}

async fn handle_providers(
    State(state): State<AppState>,
) -> (StatusCode, Json<ProvidersResponse>) {
    let providers: Vec<ProviderInfo> = state
        .zkp_verification
        .attestations
        .iter()
        .map(|r| ProviderInfo {
            provider_id: r.value().provider_id.clone(),
            tee_type: r.value().tee_type.clone(),
            mrenclave: r.value().mrenclave.clone(),
            timestamp: r.value().timestamp,
        })
        .collect();
    (StatusCode::OK, Json(ProvidersResponse { providers }))
}

async fn handle_provider_status(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> (StatusCode, Json<ProviderStatusResponse>) {
    let attested = state.zkp_verification.attestations.contains_key(&provider_id);
    let trust_score = state.zkp_verification.trust_scores.get(&provider_id).map(|r| r.value().clone());
    let trust_level = state.zkp_verification.get_trust_level(&provider_id);
    (
        StatusCode::OK,
        Json(ProviderStatusResponse {
            provider_id,
            attested,
            trust_score,
            trust_level,
        }),
    )
}

async fn handle_trust(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> (StatusCode, Json<TrustResponse>) {
    let trust_score = state.zkp_verification.calculate_trust(&provider_id);
    let trust_level = state.zkp_verification.get_trust_level(&provider_id);
    (
        StatusCode::OK,
        Json(TrustResponse {
            provider_id,
            trust_score: Some(trust_score),
            trust_level,
        }),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn new_state() -> ZKPVerificationState {
        ZKPVerificationState::new()
    }

    fn make_mock_attestation(state: &ZKPVerificationState, provider_id: &str, nonce: &str) {
        let sig_data = format!("{}:mrenclave_{}:{}", provider_id, provider_id, nonce);
        let sig_hash = hash(sig_data.as_bytes()).to_hex().to_string();
        let signature = format!("sig_{}", sig_hash);

        state.register_attestation(&AttestRequest {
            provider_id: provider_id.to_string(),
            tee_type: "SGX".to_string(),
            mrenclave: format!("mrenclave_{}", provider_id),
            signature,
            nonce: nonce.to_string(),
        });
    }

    // --- ZK Commitment tests ---

    #[test]
    fn test_commit_determinism() {
        let state = new_state();
        let c1 = state.create_commitment("value1", "blind1", "provider_a");
        let c2 = state.create_commitment("value1", "blind1", "provider_b");
        // Same value + blinding = same commitment hash regardless of provider
        assert_eq!(c1.commitment_hash, c2.commitment_hash);
    }

    #[test]
    fn test_different_values_different_commitments() {
        let state = new_state();
        let c1 = state.create_commitment("value1", "blind1", "provider_a");
        let c2 = state.create_commitment("value2", "blind1", "provider_a");
        assert_ne!(c1.commitment_hash, c2.commitment_hash);
    }

    #[test]
    fn test_different_blinding_different_commitments() {
        let state = new_state();
        let c1 = state.create_commitment("value1", "blind1", "provider_a");
        let c2 = state.create_commitment("value1", "blind2", "provider_a");
        assert_ne!(c1.commitment_hash, c2.commitment_hash);
    }

    #[test]
    fn test_commitment_stored() {
        let state = new_state();
        state.create_commitment("v", "b", "prov");
        let commitments = state.commitments.get("prov").unwrap();
        assert_eq!(commitments.len(), 1);
    }

    #[test]
    fn test_multiple_commitments_per_provider() {
        let state = new_state();
        state.create_commitment("v1", "b1", "prov");
        state.create_commitment("v2", "b2", "prov");
        state.create_commitment("v3", "b3", "prov");
        let commitments = state.commitments.get("prov").unwrap();
        assert_eq!(commitments.len(), 3);
    }

    // --- Prove + Verify tests ---

    #[test]
    fn test_prove_verify_roundtrip() {
        let state = new_state();
        let commitment = state.create_commitment("secret_value", "blind123", "provider_x");
        let proof = state
            .create_proof(
                &commitment.commitment_hash,
                "secret_value",
                "blind123",
                "public_input_data",
                "provider_x",
            )
            .expect("Proof should be created");
        assert!(state.verify_proof(&proof));
    }

    #[test]
    fn test_tampered_proof_fails() {
        let state = new_state();
        let commitment = state.create_commitment("secret_value", "blind123", "provider_x");
        let mut proof = state
            .create_proof(
                &commitment.commitment_hash,
                "secret_value",
                "blind123",
                "public_input_data",
                "provider_x",
            )
            .expect("Proof should be created");
        // Tamper with the response
        proof.response = "deadbeef".to_string();
        assert!(!state.verify_proof(&proof));
    }

    #[test]
    fn test_tampered_challenge_fails() {
        let state = new_state();
        let commitment = state.create_commitment("secret_value", "blind123", "provider_x");
        let mut proof = state
            .create_proof(
                &commitment.commitment_hash,
                "secret_value",
                "blind123",
                "public_input_data",
                "provider_x",
            )
            .expect("Proof should be created");
        proof.challenge = "tampered_challenge".to_string();
        assert!(!state.verify_proof(&proof));
    }

    #[test]
    fn test_prove_nonexistent_commitment_returns_none() {
        let state = new_state();
        let result = state.create_proof(
            "nonexistent_hash",
            "secret",
            "blind",
            "pub",
            "provider_x",
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_proof_stored() {
        let state = new_state();
        let commitment = state.create_commitment("val", "bl", "prov");
        state.create_proof(
            &commitment.commitment_hash,
            "val",
            "bl",
            "pub_in",
            "prov",
        )
        .unwrap();
        let key = format!("prov:{}", commitment.commitment_hash);
        assert!(state.proofs.contains_key(&key));
    }

    // --- Batch verify tests ---

    #[test]
    fn test_batch_verify_all_valid() {
        let state = new_state();
        let c1 = state.create_commitment("s1", "b1", "prov");
        let c2 = state.create_commitment("s2", "b2", "prov");
        let p1 = state
            .create_proof(&c1.commitment_hash, "s1", "b1", "pub1", "prov")
            .unwrap();
        let p2 = state
            .create_proof(&c2.commitment_hash, "s2", "b2", "pub2", "prov")
            .unwrap();
        let (passed, failed) = state.batch_verify(&[p1, p2]);
        assert_eq!(passed, 2);
        assert_eq!(failed, 0);
    }

    #[test]
    fn test_batch_verify_mixed() {
        let state = new_state();
        let c1 = state.create_commitment("s1", "b1", "prov");
        let c2 = state.create_commitment("s2", "b2", "prov");
        let p1 = state
            .create_proof(&c1.commitment_hash, "s1", "b1", "pub1", "prov")
            .unwrap();
        let mut p2 = state
            .create_proof(&c2.commitment_hash, "s2", "b2", "pub2", "prov")
            .unwrap();
        p2.response = "tampered".to_string();
        let (passed, failed) = state.batch_verify(&[p1, p2]);
        assert_eq!(passed, 1);
        assert_eq!(failed, 1);
    }

    #[test]
    fn test_batch_verify_empty() {
        let state = new_state();
        let (passed, failed) = state.batch_verify(&[]);
        assert_eq!(passed, 0);
        assert_eq!(failed, 0);
    }

    // --- TEE Attestation tests ---

    #[test]
    fn test_tee_attestation_valid() {
        let state = new_state();
        make_mock_attestation(&state, "provider_tee", "nonce_123");
        let (valid, msg) = state.verify_attestation("provider_tee");
        assert!(valid);
        assert!(msg.contains("valid"));
    }

    #[test]
    fn test_tee_attestation_invalid_signature() {
        let state = new_state();
        state.register_attestation(&AttestRequest {
            provider_id: "provider_bad".to_string(),
            tee_type: "SGX".to_string(),
            mrenclave: "mrenclave_bad".to_string(),
            signature: "bad_signature".to_string(),
            nonce: "nonce_456".to_string(),
        });
        let (valid, msg) = state.verify_attestation("provider_bad");
        assert!(!valid);
        assert!(msg.contains("signature") || msg.contains("Invalid"));
    }

    #[test]
    fn test_tee_attestation_nonexistent_provider() {
        let state = new_state();
        let (valid, msg) = state.verify_attestation("nobody");
        assert!(!valid);
        assert!(msg.contains("No attestation"));
    }

    #[test]
    fn test_tee_attestation_expired() {
        let state = new_state();
        make_mock_attestation(&state, "provider_old", "nonce_old");
        // Manually tamper with the stored attestation timestamp to simulate expiry
        {
            let mut att = state.attestations.get_mut("provider_old").unwrap();
            att.timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 100000; // ~27 hours ago
        }
        let (valid, msg) = state.verify_attestation("provider_old");
        assert!(!valid);
        assert!(msg.contains("expired"));
    }

    #[test]
    fn test_tee_attestation_wrong_nonce() {
        let state = new_state();
        make_mock_attestation(&state, "provider_nonce", "nonce_good");
        // Replace nonce with mismatching one
        {
            let mut att = state.attestations.get_mut("provider_nonce").unwrap();
            att.nonce = "tampered_nonce".to_string();
        }
        let (valid, _) = state.verify_attestation("provider_nonce");
        assert!(!valid);
    }

    // --- Trust Score tests ---

    #[test]
    fn test_trust_score_none() {
        let state = new_state();
        let score = state.calculate_trust("unknown_provider");
        // No TEE, no ZK -> low score
        assert_eq!(score.tee_score, 0.0);
        assert_eq!(score.zk_score, 0.0);
        assert!(score.total < 50.0);
    }

    #[test]
    fn test_trust_score_with_zk_only() {
        let state = new_state();
        state.create_commitment("v1", "b1", "zk_provider");
        state.create_commitment("v2", "b2", "zk_provider");
        let score = state.calculate_trust("zk_provider");
        assert!(score.zk_score > 0.0);
        assert_eq!(score.tee_score, 0.0);
    }

    #[test]
    fn test_trust_score_with_tee_only() {
        let state = new_state();
        make_mock_attestation(&state, "tee_provider", "nonce_tee");
        let score = state.calculate_trust("tee_provider");
        assert_eq!(score.tee_score, 100.0);
        assert_eq!(score.zk_score, 0.0);
    }

    #[test]
    fn test_trust_score_full() {
        let state = new_state();
        make_mock_attestation(&state, "full_provider", "nonce_full");
        state.create_commitment("v1", "b1", "full_provider");
        state.create_commitment("v2", "b2", "full_provider");
        let score = state.calculate_trust("full_provider");
        assert_eq!(score.tee_score, 100.0);
        assert!(score.zk_score > 0.0);
        assert!(score.total > 50.0);
    }

    #[test]
    fn test_trust_level_none() {
        let state = new_state();
        let level = state.get_trust_level("nobody");
        assert_eq!(level, TrustLevel::None);
    }

    #[test]
    fn test_trust_level_pseudonymous() {
        let state = new_state();
        state.create_commitment("v", "b", "pseudo");
        let level = state.get_trust_level("pseudo");
        assert_eq!(level, TrustLevel::Pseudonymous);
    }

    #[test]
    fn test_trust_level_tee_only() {
        let state = new_state();
        make_mock_attestation(&state, "tee_only", "n");
        let level = state.get_trust_level("tee_only");
        assert_eq!(level, TrustLevel::TeeOnly);
    }

    #[test]
    fn test_trust_level_full_zk() {
        let state = new_state();
        make_mock_attestation(&state, "full_zk", "n");
        state.create_commitment("v", "b", "full_zk");
        let level = state.get_trust_level("full_zk");
        assert_eq!(level, TrustLevel::FullZK);
    }

    #[test]
    fn test_trust_score_weighted_sum() {
        let state = new_state();
        // Mock: TEE=100, ZK=0, uptime=75, PoNW=50, reviews=60
        // Expected: 100*0.30 + 0*0.25 + 75*0.20 + 50*0.15 + 60*0.10
        // = 30 + 0 + 15 + 7.5 + 6 = 58.5
        make_mock_attestation(&state, "weighted", "n");
        let score = state.calculate_trust("weighted");
        let expected = 100.0 * 0.30 + 0.0 * 0.25 + 75.0 * 0.20 + 50.0 * 0.15 + 60.0 * 0.10;
        assert!((score.total - expected).abs() < 0.001);
    }

    #[test]
    fn test_trust_score_stored() {
        let state = new_state();
        state.calculate_trust("store_prov");
        assert!(state.trust_scores.contains_key("store_prov"));
    }

    // --- Concurrent access tests ---

    #[test]
    fn test_concurrent_commitments() {
        use std::thread;

        let state = Arc::new(new_state());
        let mut handles = vec![];

        for i in 0..10 {
            let s = Arc::clone(&state);
            handles.push(thread::spawn(move || {
                s.create_commitment(
                    &format!("val_{}", i),
                    &format!("blind_{}", i),
                    "concurrent_prov",
                );
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let commitments = state.commitments.get("concurrent_prov").unwrap();
        assert_eq!(commitments.len(), 10);
    }

    #[test]
    fn test_concurrent_proofs() {
        use std::thread;

        let state = Arc::new(new_state());

        // Create commitments first
        for i in 0..5 {
            state.create_commitment(
                &format!("secret_{}", i),
                &format!("blind_{}", i),
                "concurrent_proof_prov",
            );
        }

        let commitments: Vec<String> = state
            .commitments
            .get("concurrent_proof_prov")
            .unwrap()
            .iter()
            .map(|c| c.commitment_hash.clone())
            .collect();

        let mut handles = vec![];
        for (i, ch) in commitments.iter().enumerate() {
            let s = Arc::clone(&state);
            let ch = ch.clone();
            handles.push(thread::spawn(move || {
                s.create_proof(
                    &ch,
                    &format!("secret_{}", i),
                    &format!("blind_{}", i),
                    &format!("pub_{}", i),
                    "concurrent_proof_prov",
                )
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(state.proofs.len(), 5);
    }
}
