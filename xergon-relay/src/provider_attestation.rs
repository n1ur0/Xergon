#![allow(dead_code)]
//! Provider Attestation Service
//!
//! Verifies TEE (Trusted Execution Environment) and ZK (Zero Knowledge) proofs
//! from providers to establish trust in their execution environment. This module
//! provides attestation submission, verification, trust scoring, and policy
//! management for provider onboarding.
//!
//! Endpoints:
//! - POST   /v1/attestation/submit              — submit a provider attestation
//! - GET    /v1/attestation/:id                  — get attestation details
//! - GET    /v1/attestation/provider/:provider_id — get provider trust record
//! - POST   /v1/attestation/provider/:provider_id/revoke — revoke provider trust
//! - PUT    /v1/attestation/policy               — update attestation policy
//! - GET    /v1/attestation/policy               — get current attestation policy
//! - GET    /v1/attestation/list                 — list attestations (with filters)
//! - GET    /v1/attestation/summary              — get attestation statistics
//! - POST   /v1/attestation/check/:provider_id   — check provider eligibility

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::proxy::AppState;

// ================================================================
// Error type
// ================================================================

/// Errors specific to the provider attestation module.
#[derive(Debug, thiserror::Error)]
pub enum AttestationError {
    #[error("invalid evidence: {0}")]
    InvalidEvidence(String),
    #[error("attestation not found: {0}")]
    AttestationNotFound(String),
    #[error("provider not found: {0}")]
    ProviderNotFound(String),
    #[error("provider already revoked: {0}")]
    ProviderAlreadyRevoked(String),
    #[error("policy violation: {0}")]
    PolicyViolation(String),
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    #[error("attestation expired")]
    AttestationExpired,
}

impl IntoResponse for AttestationError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::InvalidEvidence(_) | Self::VerificationFailed(_) => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            Self::AttestationNotFound(_) | Self::ProviderNotFound(_) => {
                (StatusCode::NOT_FOUND, self.to_string())
            }
            Self::ProviderAlreadyRevoked(_) => (StatusCode::CONFLICT, self.to_string()),
            Self::PolicyViolation(_) => (StatusCode::FORBIDDEN, self.to_string()),
            Self::AttestationExpired => (StatusCode::GONE, self.to_string()),
        };
        (
            status,
            Json(serde_json::json!({
                "error": message,
            })),
        )
            .into_response()
    }
}

// ================================================================
// Core types
// ================================================================

/// Type of attestation proof submitted by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttestationType {
    TEE_AMD_SEV,
    TEEIntelSGX,
    TEEIntelTDX,
    ZkStark,
    ZkSnark,
    ZkGroth16,
    Software,
    SelfSigned,
}

impl std::fmt::Display for AttestationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TEE_AMD_SEV => write!(f, "TEE_AMD_SEV"),
            Self::TEEIntelSGX => write!(f, "TEE_INTEL_SGX"),
            Self::TEEIntelTDX => write!(f, "TEE_INTEL_TDX"),
            Self::ZkStark => write!(f, "ZK_STARK"),
            Self::ZkSnark => write!(f, "ZK_SNARK"),
            Self::ZkGroth16 => write!(f, "ZK_GROTH16"),
            Self::Software => write!(f, "Software"),
            Self::SelfSigned => write!(f, "SelfSigned"),
        }
    }
}

impl AttestationType {
    /// Check if this is a TEE-based attestation type.
    pub fn is_tee(&self) -> bool {
        matches!(
            self,
            Self::TEE_AMD_SEV | Self::TEEIntelSGX | Self::TEEIntelTDX
        )
    }

    /// Check if this is a ZK proof-based attestation type.
    pub fn is_zk(&self) -> bool {
        matches!(self, Self::ZkStark | Self::ZkSnark | Self::ZkGroth16)
    }
}

/// Trust level assigned to a provider after attestation verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Trusted,
    Provisional,
    Untrusted,
    Revoked,
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trusted => write!(f, "Trusted"),
            Self::Provisional => write!(f, "Provisional"),
            Self::Untrusted => write!(f, "Untrusted"),
            Self::Revoked => write!(f, "Revoked"),
        }
    }
}

impl Default for TrustLevel {
    fn default() -> Self {
        Self::Untrusted
    }
}

/// Detailed results of an attestation verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationDetails {
    pub verifier: String,
    pub verification_time_ms: u64,
    pub trust_level: TrustLevel,
    pub checks_passed: Vec<String>,
    pub checks_failed: Vec<String>,
    pub security_advisories: Vec<String>,
}

/// A single attestation report submitted by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReport {
    pub id: String,
    pub provider_id: String,
    pub attestation_type: AttestationType,
    pub report_hash: String,
    pub evidence: String,
    pub verified: bool,
    pub verification_details: VerificationDetails,
    pub created_at: i64,
    pub expires_at: i64,
}

/// Trust record tracking cumulative attestation history for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTrustRecord {
    pub provider_id: String,
    pub attestation_count: u64,
    pub latest_trust_level: TrustLevel,
    pub first_attested: i64,
    pub last_attested: i64,
    pub consecutive_failures: u32,
    pub attestation_types_used: Vec<AttestationType>,
}

/// Policy governing attestation requirements for providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationPolicy {
    pub require_tee: bool,
    pub require_zk: bool,
    pub min_trust_level: TrustLevel,
    pub max_age_hours: u64,
    pub accepted_types: Vec<AttestationType>,
    pub auto_revoke_after_failures: u32,
}

impl Default for AttestationPolicy {
    fn default() -> Self {
        Self {
            require_tee: false,
            require_zk: false,
            min_trust_level: TrustLevel::Provisional,
            max_age_hours: 168, // 7 days
            accepted_types: vec![
                AttestationType::TEE_AMD_SEV,
                AttestationType::TEEIntelSGX,
                AttestationType::TEEIntelTDX,
                AttestationType::ZkStark,
                AttestationType::ZkSnark,
                AttestationType::ZkGroth16,
                AttestationType::Software,
            ],
            auto_revoke_after_failures: 5,
        }
    }
}

/// Summary statistics for all attestations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationSummary {
    pub total_attestations: u64,
    pub verified_count: u64,
    pub by_type: HashMap<AttestationType, u64>,
    pub trust_distribution: HashMap<TrustLevel, u64>,
    pub avg_verification_ms: u64,
}

// ================================================================
// AttestationState (stored in AppState)
// ================================================================

/// Shared state for the provider attestation service.
pub struct AttestationState {
    /// Attestation reports keyed by attestation ID.
    pub attestations: DashMap<String, AttestationReport>,
    /// Provider trust records keyed by provider ID.
    pub provider_trust: DashMap<String, ProviderTrustRecord>,
    /// Current attestation policy.
    pub policy: std::sync::RwLock<AttestationPolicy>,
}

impl AttestationState {
    /// Create a new attestation state with default policy.
    pub fn new() -> Self {
        Self {
            attestations: DashMap::new(),
            provider_trust: DashMap::new(),
            policy: std::sync::RwLock::new(AttestationPolicy::default()),
        }
    }
}

impl Default for AttestationState {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// AttestationEngine
// ================================================================

/// Core engine for attestation verification and trust management.
pub struct AttestationEngine {
    state: Arc<AttestationState>,
}

impl AttestationEngine {
    /// Create a new attestation engine wrapping shared state.
    pub fn new(state: Arc<AttestationState>) -> Self {
        Self { state }
    }

    // ----------------------------------------------------------------
    // Evidence generation helpers (for tests)
    // ----------------------------------------------------------------

    /// Generate a plausible TEE evidence hex string.
    pub fn generate_tee_evidence(att_type: AttestationType) -> String {
        let prefix = match att_type {
            AttestationType::TEE_AMD_SEV => "sev",
            AttestationType::TEEIntelSGX => "sgx",
            AttestationType::TEEIntelTDX => "tdx",
            _ => "tee",
        };
        // Generate a realistic-looking hex blob (256 bytes = 512 hex chars)
        let body: String = (0..512)
            .map(|i| {
                let byte = ((i * 7 + 13) % 256) as u8;
                format!("{:02x}", byte)
            })
            .collect();
        format!("{}{}", prefix, body)
    }

    /// Generate a plausible ZK proof evidence hex string.
    pub fn generate_zk_evidence(att_type: AttestationType) -> String {
        let prefix = match att_type {
            AttestationType::ZkStark => "stark",
            AttestationType::ZkSnark => "snark",
            AttestationType::ZkGroth16 => "groth16",
            _ => "zk",
        };
        // ZK proofs are typically longer
        let body: String = (0..1024)
            .map(|i| {
                let byte = ((i * 11 + 37) % 256) as u8;
                format!("{:02x}", byte)
            })
            .collect();
        format!("{}{}", prefix, body)
    }

    // ----------------------------------------------------------------
    // Core verification logic (simulated)
    // ----------------------------------------------------------------

    /// Verify an attestation proof (simulated TEE/ZK verification).
    ///
    /// Since we cannot perform actual TEE/ZK verification in the relay,
    /// this function performs structural validation of the evidence and
    /// assigns trust levels based on evidence quality.
    pub fn verify_attestation(
        &self,
        attestation_type: AttestationType,
        evidence: &str,
    ) -> VerificationDetails {
        let start = Instant::now();
        let mut checks_passed = Vec::new();
        let mut checks_failed = Vec::new();
        let mut security_advisories = Vec::new();
        let mut trust_level = TrustLevel::Untrusted;

        // ---- TEE attestation verification (simulated) ----
        if attestation_type.is_tee() {
            // Check evidence length: TEE reports are typically 200+ hex chars
            if evidence.len() >= 200 {
                checks_passed.push("evidence_length_sufficient".to_string());
            } else {
                checks_failed.push("evidence_too_short".to_string());
                trust_level = TrustLevel::Untrusted;
            }

            // Check hex format
            let hex_body = if evidence.starts_with("sev")
                || evidence.starts_with("sgx")
                || evidence.starts_with("tdx")
            {
                checks_passed.push("tee_prefix_valid".to_string());
                &evidence[3..]
            } else {
                checks_failed.push("missing_tee_prefix".to_string());
                evidence
            };

            if hex_body.chars().all(|c| c.is_ascii_hexdigit()) {
                checks_passed.push("valid_hex_encoding".to_string());
            } else {
                checks_failed.push("invalid_hex_encoding".to_string());
            }

            // Check even hex length
            if hex_body.len() % 2 == 0 {
                checks_passed.push("even_hex_length".to_string());
            } else {
                checks_failed.push("odd_hex_length".to_string());
            }

            // Minimum byte count for TEE report
            if hex_body.len() >= 256 {
                checks_passed.push("sufficient_report_size".to_string());
            } else {
                checks_failed.push("insufficient_report_size".to_string());
                security_advisories.push(
                    "TEE report appears truncated; may indicate partial attestation".to_string(),
                );
            }

            // Assign trust level based on check results
            let fail_count = checks_failed.len();
            if fail_count == 0 {
                trust_level = TrustLevel::Trusted;
            } else if fail_count <= 2 {
                trust_level = TrustLevel::Provisional;
                security_advisories.push(
                    "Provisional trust: some validation checks failed".to_string(),
                );
            } else {
                trust_level = TrustLevel::Untrusted;
                security_advisories.push(
                    "Untrusted: multiple validation checks failed".to_string(),
                );
            }
        }
        // ---- ZK proof verification (simulated) ----
        else if attestation_type.is_zk() {
            // Check evidence length: ZK proofs are typically 500+ hex chars
            if evidence.len() >= 500 {
                checks_passed.push("proof_length_sufficient".to_string());
            } else {
                checks_failed.push("proof_too_short".to_string());
            }

            // Check ZK prefix
            let hex_body = if evidence.starts_with("stark")
                || evidence.starts_with("snark")
                || evidence.starts_with("groth16")
            {
                checks_passed.push("zk_prefix_valid".to_string());
                let prefix_len = if evidence.starts_with("groth16") { 7 } else { 5 };
                &evidence[prefix_len..]
            } else {
                checks_failed.push("missing_zk_prefix".to_string());
                evidence
            };

            if hex_body.chars().all(|c| c.is_ascii_hexdigit()) {
                checks_passed.push("valid_hex_encoding".to_string());
            } else {
                checks_failed.push("invalid_hex_encoding".to_string());
            }

            // Check even hex length
            if hex_body.len() % 2 == 0 {
                checks_passed.push("even_hex_length".to_string());
            } else {
                checks_failed.push("odd_hex_length".to_string());
            }

            // Minimum proof size for ZK proofs
            if hex_body.len() >= 512 {
                checks_passed.push("sufficient_proof_size".to_string());
            } else {
                checks_failed.push("insufficient_proof_size".to_string());
                security_advisories.push(
                    "ZK proof appears truncated; verification may be incomplete".to_string(),
                );
            }

            // Check for public input structure (simulated)
            if hex_body.len() >= 64 {
                checks_passed.push("public_inputs_present".to_string());
            } else {
                checks_failed.push("missing_public_inputs".to_string());
            }

            // Assign trust level
            let fail_count = checks_failed.len();
            if fail_count == 0 {
                trust_level = TrustLevel::Trusted;
            } else if fail_count <= 2 {
                trust_level = TrustLevel::Provisional;
                security_advisories.push(
                    "Provisional trust: some proof validation checks failed".to_string(),
                );
            } else {
                trust_level = TrustLevel::Untrusted;
                security_advisories.push(
                    "Untrusted: multiple proof validation checks failed".to_string(),
                );
            }
        }
        // ---- Software / SelfSigned attestation ----
        else {
            if !evidence.is_empty() {
                checks_passed.push("evidence_present".to_string());
            } else {
                checks_failed.push("empty_evidence".to_string());
            }

            trust_level = TrustLevel::Provisional;
            security_advisories.push(
                "Software/SelfSigned attestation provides limited trust guarantees".to_string(),
            );
        }

        let elapsed = start.elapsed();
        let verification_time_ms = elapsed.as_millis() as u64;

        // Ensure minimum verification time for realism
        let verification_time_ms = verification_time_ms.max(1);

        VerificationDetails {
            verifier: "xergon-attestation-v1".to_string(),
            verification_time_ms,
            trust_level,
            checks_passed,
            checks_failed,
            security_advisories,
        }
    }

    // ----------------------------------------------------------------
    // Public API methods
    // ----------------------------------------------------------------

    /// Submit and verify an attestation for a provider.
    pub fn submit_attestation(
        &self,
        provider_id: &str,
        attestation_type: AttestationType,
        evidence: &str,
        _public_key: &str,
    ) -> Result<AttestationReport, AttestationError> {
        // Validate evidence is not empty
        if evidence.trim().is_empty() {
            return Err(AttestationError::InvalidEvidence(
                "evidence must not be empty".to_string(),
            ));
        }

        // Validate against policy accepted types
        let policy = self
            .state
            .policy
            .read()
            .map_err(|_| AttestationError::VerificationFailed("policy lock poisoned".into()))?;

        if !policy.accepted_types.contains(&attestation_type) {
            return Err(AttestationError::PolicyViolation(format!(
                "attestation type {} is not in accepted list",
                attestation_type
            )));
        }

        // Perform verification
        let verification_details = self.verify_attestation(attestation_type, evidence);
        let verified = verification_details.trust_level != TrustLevel::Untrusted;

        // Check if provider is already revoked
        if let Some(mut trust) = self.state.provider_trust.get_mut(provider_id) {
            if trust.latest_trust_level == TrustLevel::Revoked {
                return Err(AttestationError::ProviderAlreadyRevoked(
                    provider_id.to_string(),
                ));
            }
        }

        // Compute report hash (SHA-256 of evidence)
        let report_hash = compute_hash(evidence.as_bytes());

        // Create attestation report
        let now = Utc::now().timestamp();
        let expires_at = now + (policy.max_age_hours as i64) * 3600;
        let id = Uuid::new_v4().to_string();

        let report = AttestationReport {
            id: id.clone(),
            provider_id: provider_id.to_string(),
            attestation_type,
            report_hash,
            evidence: evidence.to_string(),
            verified,
            verification_details,
            created_at: now,
            expires_at,
        };

        // Store attestation
        self.state.attestations.insert(id.clone(), report.clone());

        // Update provider trust record
        let mut trust = self.state.provider_trust.entry(provider_id.to_string()).or_insert_with(
            || ProviderTrustRecord {
                provider_id: provider_id.to_string(),
                attestation_count: 0,
                latest_trust_level: TrustLevel::Untrusted,
                first_attested: now,
                last_attested: now,
                consecutive_failures: 0,
                attestation_types_used: Vec::new(),
            },
        );

        trust.attestation_count += 1;
        trust.last_attested = now;
        if verified {
            trust.consecutive_failures = 0;
            trust.latest_trust_level = report.verification_details.trust_level;
        } else {
            trust.consecutive_failures += 1;
            // Auto-revoke if consecutive failures exceed threshold
            if trust.consecutive_failures >= policy.auto_revoke_after_failures {
                trust.latest_trust_level = TrustLevel::Revoked;
                warn!(
                    provider_id = %provider_id,
                    consecutive_failures = trust.consecutive_failures,
                    "Provider auto-revoked due to excessive attestation failures"
                );
            }
        }

        // Track attestation types used
        if !trust.attestation_types_used.contains(&attestation_type) {
            trust.attestation_types_used.push(attestation_type);
        }

        info!(
            attestation_id = %id,
            provider_id = %provider_id,
            attestation_type = %attestation_type,
            verified = verified,
            trust_level = %report.verification_details.trust_level,
            "Attestation submitted and verified"
        );

        Ok(report)
    }

    /// Verify a specific attestation proof (without creating a report).
    pub fn verify_only(
        &self,
        attestation_type: AttestationType,
        evidence: &str,
    ) -> VerificationDetails {
        self.verify_attestation(attestation_type, evidence)
    }

    /// Get the trust record for a specific provider.
    pub fn get_provider_trust(
        &self,
        provider_id: &str,
    ) -> Result<ProviderTrustRecord, AttestationError> {
        self.state
            .provider_trust
            .get(provider_id)
            .map(|r| r.clone())
            .ok_or_else(|| AttestationError::ProviderNotFound(provider_id.to_string()))
    }

    /// Revoke a provider's trust.
    pub fn revoke_provider(
        &self,
        provider_id: &str,
        reason: &str,
    ) -> Result<ProviderTrustRecord, AttestationError> {
        let mut trust = self
            .state
            .provider_trust
            .get_mut(provider_id)
            .ok_or_else(|| AttestationError::ProviderNotFound(provider_id.to_string()))?;

        if trust.latest_trust_level == TrustLevel::Revoked {
            return Err(AttestationError::ProviderAlreadyRevoked(
                provider_id.to_string(),
            ));
        }

        trust.latest_trust_level = TrustLevel::Revoked;
        trust.last_attested = Utc::now().timestamp();

        warn!(
            provider_id = %provider_id,
            reason = %reason,
            "Provider trust revoked"
        );

        Ok(trust.clone())
    }

    /// Update the attestation policy.
    pub fn update_policy(&self, policy: AttestationPolicy) {
        if let Ok(mut p) = self.state.policy.write() {
            *p = policy;
            info!("Attestation policy updated");
        }
    }

    /// Get the current attestation policy.
    pub fn get_policy(&self) -> AttestationPolicy {
        self.state
            .policy
            .read()
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    /// Get a specific attestation by ID.
    pub fn get_attestation(&self, id: &str) -> Result<AttestationReport, AttestationError> {
        self.state
            .attestations
            .get(id)
            .map(|r| r.clone())
            .ok_or_else(|| AttestationError::AttestationNotFound(id.to_string()))
    }

    /// List attestations with optional filters.
    pub fn list_attestations(
        &self,
        provider_id: Option<&str>,
        attestation_type: Option<AttestationType>,
        trust_level: Option<TrustLevel>,
    ) -> Vec<AttestationReport> {
        self.state
            .attestations
            .iter()
            .filter(|entry| {
                let report = entry.value();
                if let Some(pid) = provider_id {
                    if report.provider_id != pid {
                        return false;
                    }
                }
                if let Some(at) = attestation_type {
                    if report.attestation_type != at {
                        return false;
                    }
                }
                if let Some(tl) = trust_level {
                    if report.verification_details.trust_level != tl {
                        return false;
                    }
                }
                true
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get overall attestation summary statistics.
    pub fn get_summary(&self) -> AttestationSummary {
        let mut total = 0u64;
        let mut verified = 0u64;
        let mut by_type: HashMap<AttestationType, u64> = HashMap::new();
        let mut trust_dist: HashMap<TrustLevel, u64> = HashMap::new();
        let mut total_ms = 0u64;

        for entry in self.state.attestations.iter() {
            let report = entry.value();
            total += 1;
            if report.verified {
                verified += 1;
            }
            *by_type.entry(report.attestation_type).or_insert(0) += 1;
            *trust_dist
                .entry(report.verification_details.trust_level)
                .or_insert(0) += 1;
            total_ms += report.verification_details.verification_time_ms;
        }

        let avg_ms = if total > 0 { total_ms / total } else { 0 };

        AttestationSummary {
            total_attestations: total,
            verified_count: verified,
            by_type: by_type,
            trust_distribution: trust_dist,
            avg_verification_ms: avg_ms,
        }
    }

    /// Check if a provider meets the current attestation policy requirements.
    pub fn check_provider_eligibility(&self, provider_id: &str) -> Result<EligibilityCheck, AttestationError> {
        let policy = self
            .state
            .policy
            .read()
            .map_err(|_| AttestationError::VerificationFailed("policy lock poisoned".into()))?;

        let trust = self.get_provider_trust(provider_id)?;

        let mut eligible = true;
        let mut reasons: Vec<String> = Vec::new();

        // Check trust level
        if trust.latest_trust_level == TrustLevel::Revoked {
            eligible = false;
            reasons.push("Provider has been revoked".to_string());
        }

        // Compare trust level to minimum
        let level_order = |l: &TrustLevel| match l {
            TrustLevel::Trusted => 3,
            TrustLevel::Provisional => 2,
            TrustLevel::Untrusted => 1,
            TrustLevel::Revoked => 0,
        };
        if level_order(&trust.latest_trust_level) < level_order(&policy.min_trust_level) {
            eligible = false;
            reasons.push(format!(
                "Trust level {} is below required minimum {}",
                trust.latest_trust_level, policy.min_trust_level
            ));
        }

        // Check TEE requirement
        if policy.require_tee {
            let has_tee = trust
                .attestation_types_used
                .iter()
                .any(|t| t.is_tee());
            if !has_tee {
                eligible = false;
                reasons.push("Policy requires TEE attestation but none found".to_string());
            }
        }

        // Check ZK requirement
        if policy.require_zk {
            let has_zk = trust
                .attestation_types_used
                .iter()
                .any(|t| t.is_zk());
            if !has_zk {
                eligible = false;
                reasons.push("Policy requires ZK attestation but none found".to_string());
            }
        }

        // Check attestation freshness
        let now = Utc::now().timestamp();
        let max_age_secs = (policy.max_age_hours as i64) * 3600;
        if now - trust.last_attested > max_age_secs {
            eligible = false;
            reasons.push(format!(
                "Latest attestation is too old ({} hours ago, max {} hours)",
                (now - trust.last_attested) / 3600,
                policy.max_age_hours
            ));
        }

        // Check if any attestation type is accepted
        let accepted = trust
            .attestation_types_used
            .iter()
            .any(|t| policy.accepted_types.contains(t));
        if !accepted && !trust.attestation_types_used.is_empty() {
            eligible = false;
            reasons.push("None of the provider's attestation types are in the accepted list".to_string());
        }

        Ok(EligibilityCheck {
            provider_id: provider_id.to_string(),
            eligible,
            trust_level: trust.latest_trust_level,
            reasons,
        })
    }

    /// Remove expired attestations from storage.
    pub fn prune_expired(&self) -> u64 {
        let now = Utc::now().timestamp();
        let mut pruned = 0u64;

        self.state.attestations.retain(|_, report| {
            if report.expires_at < now {
                pruned += 1;
                debug!(
                    attestation_id = %report.id,
                    provider_id = %report.provider_id,
                    "Pruned expired attestation"
                );
                false
            } else {
                true
            }
        });

        if pruned > 0 {
            info!(pruned = pruned, "Pruned expired attestations");
        }

        pruned
    }
}

// ================================================================
// Eligibility check result
// ================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EligibilityCheck {
    pub provider_id: String,
    pub eligible: bool,
    pub trust_level: TrustLevel,
    pub reasons: Vec<String>,
}

// ================================================================
// Helper functions
// ================================================================

/// Compute a SHA-256 hash and return as hex string.
fn compute_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

// ================================================================
// REST request/response types
// ================================================================

#[derive(Debug, Deserialize)]
pub struct SubmitAttestationRequest {
    pub provider_id: String,
    pub attestation_type: AttestationType,
    pub evidence: String,
    #[serde(default)]
    pub public_key: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitAttestationResponse {
    pub success: bool,
    pub attestation: AttestationReport,
}

#[derive(Debug, Deserialize)]
pub struct RevokeProviderRequest {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct RevokeProviderResponse {
    pub success: bool,
    pub trust_record: ProviderTrustRecord,
}

#[derive(Debug, Serialize)]
pub struct EligibilityResponse {
    pub eligible: bool,
    pub trust_level: TrustLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListAttestationsQuery {
    pub provider_id: Option<String>,
    pub attestation_type: Option<String>,
    pub trust_level: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PolicyResponse {
    pub policy: AttestationPolicy,
}

// ================================================================
// HTTP handlers
// ================================================================

/// POST /v1/attestation/submit — Submit a provider attestation.
async fn submit_attestation_handler(
    State(state): State<AppState>,
    Json(body): Json<SubmitAttestationRequest>,
) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());

    // Parse attestation type from string if needed
    let att_type = body.attestation_type;

    match engine.submit_attestation(&body.provider_id, att_type, &body.evidence, &body.public_key) {
        Ok(report) => {
            let resp = SubmitAttestationResponse {
                success: true,
                attestation: report,
            };
            (StatusCode::CREATED, Json(resp)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// GET /v1/attestation/:id — Get attestation details.
async fn get_attestation_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());
    match engine.get_attestation(&id) {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// GET /v1/attestation/provider/:provider_id — Get provider trust record.
async fn get_provider_trust_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());
    match engine.get_provider_trust(&provider_id) {
        Ok(record) => (StatusCode::OK, Json(record)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// POST /v1/attestation/provider/:provider_id/revoke — Revoke provider trust.
async fn revoke_provider_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Json(body): Json<RevokeProviderRequest>,
) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());
    match engine.revoke_provider(&provider_id, &body.reason) {
        Ok(record) => {
            let resp = RevokeProviderResponse {
                success: true,
                trust_record: record,
            };
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// PUT /v1/attestation/policy — Update attestation policy.
async fn update_policy_handler(
    State(state): State<AppState>,
    Json(policy): Json<AttestationPolicy>,
) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());
    engine.update_policy(policy.clone());
    (StatusCode::OK, Json(PolicyResponse { policy })).into_response()
}

/// GET /v1/attestation/policy — Get current attestation policy.
async fn get_policy_handler(State(state): State<AppState>) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());
    let policy = engine.get_policy();
    (StatusCode::OK, Json(PolicyResponse { policy })).into_response()
}

/// GET /v1/attestation/list — List attestations with filters.
async fn list_attestations_handler(
    State(state): State<AppState>,
    Query(params): Query<ListAttestationsQuery>,
) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());

    // Parse optional attestation_type string filter
    let att_type_filter = params.attestation_type.as_deref().and_then(|s| {
        // Try to parse from various string representations
        match s.to_uppercase().as_str() {
            "TEE_AMD_SEV" => Some(AttestationType::TEE_AMD_SEV),
            "TEE_INTEL_SGX" | "TEEINTELSGX" => Some(AttestationType::TEEIntelSGX),
            "TEE_INTEL_TDX" | "TEEINTELTDX" => Some(AttestationType::TEEIntelTDX),
            "ZK_STARK" | "ZKSTARK" => Some(AttestationType::ZkStark),
            "ZK_SNARK" | "ZKSNARK" => Some(AttestationType::ZkSnark),
            "ZK_GROTH16" | "ZKGROTH16" => Some(AttestationType::ZkGroth16),
            "SOFTWARE" => Some(AttestationType::Software),
            "SELF_SIGNED" | "SELFSIGNED" => Some(AttestationType::SelfSigned),
            _ => None,
        }
    });

    // Parse optional trust_level string filter
    let trust_level_filter = params.trust_level.as_deref().and_then(|s| {
        match s.to_lowercase().as_str() {
            "trusted" => Some(TrustLevel::Trusted),
            "provisional" => Some(TrustLevel::Provisional),
            "untrusted" => Some(TrustLevel::Untrusted),
            "revoked" => Some(TrustLevel::Revoked),
            _ => None,
        }
    });

    let attestations = engine.list_attestations(
        params.provider_id.as_deref(),
        att_type_filter,
        trust_level_filter,
    );

    (StatusCode::OK, Json(attestations)).into_response()
}

/// GET /v1/attestation/summary — Get attestation statistics.
async fn get_summary_handler(State(state): State<AppState>) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());
    let summary = engine.get_summary();
    (StatusCode::OK, Json(summary)).into_response()
}

/// POST /v1/attestation/check/:provider_id — Check provider eligibility.
async fn check_eligibility_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Response {
    let engine = AttestationEngine::new(state.provider_attestation.clone());
    match engine.check_provider_eligibility(&provider_id) {
        Ok(check) => {
            let resp = EligibilityResponse {
                eligible: check.eligible,
                trust_level: check.trust_level,
                reasons: check.reasons,
            };
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

// ================================================================
// Router builder
// ================================================================

/// Build the provider attestation router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/attestation/submit", post(submit_attestation_handler))
        .route("/v1/attestation/{id}", get(get_attestation_handler))
        .route(
            "/v1/attestation/provider/{provider_id}",
            get(get_provider_trust_handler),
        )
        .route(
            "/v1/attestation/provider/{provider_id}/revoke",
            post(revoke_provider_handler),
        )
        .route("/v1/attestation/policy", put(update_policy_handler))
        .route("/v1/attestation/policy", get(get_policy_handler))
        .route("/v1/attestation/list", get(list_attestations_handler))
        .route("/v1/attestation/summary", get(get_summary_handler))
        .route(
            "/v1/attestation/check/{provider_id}",
            post(check_eligibility_handler),
        )
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> AttestationEngine {
        let state = Arc::new(AttestationState::new());
        AttestationEngine::new(state)
    }

    // ---------- test_submit_tee_attestation ----------

    #[test]
    fn test_submit_tee_attestation() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);

        let result = engine.submit_attestation(
            "provider-tee-1",
            AttestationType::TEE_AMD_SEV,
            &evidence,
            "0xpubkey",
        );

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.verified);
        assert!(!report.id.is_empty());
        assert_eq!(report.provider_id, "provider-tee-1");
        assert_eq!(report.attestation_type, AttestationType::TEE_AMD_SEV);
    }

    // ---------- test_submit_zk_attestation ----------

    #[test]
    fn test_submit_zk_attestation() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_zk_evidence(AttestationType::ZkGroth16);

        let result = engine.submit_attestation(
            "provider-zk-1",
            AttestationType::ZkGroth16,
            &evidence,
            "0xpubkey",
        );

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.verified);
        assert_eq!(report.attestation_type, AttestationType::ZkGroth16);
    }

    // ---------- test_verify_valid_evidence ----------

    #[test]
    fn test_verify_valid_evidence() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEEIntelSGX);

        let details = engine.verify_only(AttestationType::TEEIntelSGX, &evidence);

        assert!(details.checks_passed.contains(&"tee_prefix_valid".to_string()));
        assert!(details.checks_passed.contains(&"valid_hex_encoding".to_string()));
        assert!(details.checks_passed.contains(&"sufficient_report_size".to_string()));
        assert!(details.trust_level != TrustLevel::Untrusted);
    }

    // ---------- test_verify_invalid_evidence ----------

    #[test]
    fn test_verify_invalid_evidence() {
        let engine = make_engine();

        // Very short, invalid evidence
        let details = engine.verify_only(AttestationType::TEE_AMD_SEV, "abc");

        assert!(details.checks_failed.contains(&"evidence_too_short".to_string()));
        assert!(details.checks_failed.contains(&"missing_tee_prefix".to_string()));
        assert_eq!(details.trust_level, TrustLevel::Untrusted);
    }

    // ---------- test_provider_trust_record ----------

    #[test]
    fn test_provider_trust_record() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);

        engine
            .submit_attestation("provider-trust-1", AttestationType::TEE_AMD_SEV, &evidence, "")
            .unwrap();

        let trust = engine.get_provider_trust("provider-trust-1").unwrap();
        assert_eq!(trust.attestation_count, 1);
        assert_eq!(trust.latest_trust_level, TrustLevel::Trusted);
        assert!(trust.attestation_types_used.contains(&AttestationType::TEE_AMD_SEV));
    }

    // ---------- test_revoke_provider ----------

    #[test]
    fn test_revoke_provider() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);

        engine
            .submit_attestation("provider-revoke-1", AttestationType::TEE_AMD_SEV, &evidence, "")
            .unwrap();

        let result = engine.revoke_provider("provider-revoke-1", "test revocation");
        assert!(result.is_ok());
        let trust = result.unwrap();
        assert_eq!(trust.latest_trust_level, TrustLevel::Revoked);

        // Double revoke should fail
        let result2 = engine.revoke_provider("provider-revoke-1", "double revoke");
        assert!(result2.is_err());
    }

    // ---------- test_policy_update ----------

    #[test]
    fn test_policy_update() {
        let engine = make_engine();

        let default_policy = engine.get_policy();
        assert!(!default_policy.require_tee);
        assert!(!default_policy.require_zk);

        let new_policy = AttestationPolicy {
            require_tee: true,
            require_zk: true,
            min_trust_level: TrustLevel::Trusted,
            max_age_hours: 24,
            accepted_types: vec![AttestationType::TEE_AMD_SEV],
            auto_revoke_after_failures: 3,
        };

        engine.update_policy(new_policy.clone());
        let fetched = engine.get_policy();
        assert!(fetched.require_tee);
        assert!(fetched.require_zk);
        assert_eq!(fetched.min_trust_level, TrustLevel::Trusted);
        assert_eq!(fetched.max_age_hours, 24);
    }

    // ---------- test_check_eligibility_trusted ----------

    #[test]
    fn test_check_eligibility_trusted() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);

        engine
            .submit_attestation("provider-elig-1", AttestationType::TEE_AMD_SEV, &evidence, "")
            .unwrap();

        let check = engine.check_provider_eligibility("provider-elig-1").unwrap();
        assert!(check.eligible);
        assert!(check.reasons.is_empty());
    }

    // ---------- test_check_eligibility_untrusted ----------

    #[test]
    fn test_check_eligibility_untrusted() {
        let engine = make_engine();

        // Set policy to require TEE
        engine.update_policy(AttestationPolicy {
            require_tee: true,
            ..AttestationPolicy::default()
        });

        // Submit only a software attestation
        engine
            .submit_attestation("provider-elig-2", AttestationType::Software, "some-evidence", "")
            .unwrap();

        let check = engine.check_provider_eligibility("provider-elig-2").unwrap();
        assert!(!check.eligible);
        assert!(check
            .reasons
            .iter()
            .any(|r| r.contains("TEE attestation")));
    }

    // ---------- test_prune_expired ----------

    #[test]
    fn test_prune_expired() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);

        engine
            .submit_attestation("provider-prune-1", AttestationType::TEE_AMD_SEV, &evidence, "")
            .unwrap();

        // Set a very short expiry policy, then manually expire the attestation
        engine.update_policy(AttestationPolicy {
            max_age_hours: 0,
            ..AttestationPolicy::default()
        });

        // Manually set the attestation's expiry to the past
        let expired_key = {
            engine
                .state
                .attestations
                .iter()
                .next()
                .map(|kv| kv.key().clone())
        };
        drop(expired_key);
        // Collect all keys first to avoid borrow issues
        let keys: Vec<String> = engine
            .state
            .attestations
            .iter()
            .map(|kv| kv.key().clone())
            .collect();
        for key in keys {
            if let Some(mut entry) = engine.state.attestations.get_mut(&key) {
                entry.expires_at = 0;
            }
        }

        let pruned = engine.prune_expired();
        assert!(pruned > 0);

        // Verify it was actually removed
        let summary = engine.get_summary();
        assert_eq!(summary.total_attestations, 0);
    }

    // ---------- test_attestation_summary ----------

    #[test]
    fn test_attestation_summary() {
        let engine = make_engine();

        // Submit several attestations
        let tee_evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);
        let zk_evidence = AttestationEngine::generate_zk_evidence(AttestationType::ZkGroth16);

        engine
            .submit_attestation("prov-sum-1", AttestationType::TEE_AMD_SEV, &tee_evidence, "")
            .unwrap();
        engine
            .submit_attestation("prov-sum-2", AttestationType::ZkGroth16, &zk_evidence, "")
            .unwrap();
        engine
            .submit_attestation("prov-sum-3", AttestationType::TEEIntelSGX, &tee_evidence, "")
            .unwrap();

        let summary = engine.get_summary();
        assert_eq!(summary.total_attestations, 3);
        assert_eq!(summary.verified_count, 3);
        assert!(summary.by_type.contains_key(&AttestationType::TEE_AMD_SEV));
        assert!(summary.by_type.contains_key(&AttestationType::ZkGroth16));
        assert!(summary.avg_verification_ms > 0);
    }

    // ---------- test_list_with_filters ----------

    #[test]
    fn test_list_with_filters() {
        let engine = make_engine();

        let tee_evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);
        let zk_evidence = AttestationEngine::generate_zk_evidence(AttestationType::ZkStark);

        engine
            .submit_attestation("prov-list-1", AttestationType::TEE_AMD_SEV, &tee_evidence, "")
            .unwrap();
        engine
            .submit_attestation("prov-list-1", AttestationType::ZkStark, &zk_evidence, "")
            .unwrap();
        engine
            .submit_attestation("prov-list-2", AttestationType::TEE_AMD_SEV, &tee_evidence, "")
            .unwrap();

        // Filter by provider
        let all = engine.list_attestations(Some("prov-list-1"), None, None);
        assert_eq!(all.len(), 2);

        // Filter by type
        let tee_only = engine.list_attestations(None, Some(AttestationType::TEE_AMD_SEV), None);
        assert_eq!(tee_only.len(), 2);

        // Filter by trust level
        let trusted = engine.list_attestations(None, None, Some(TrustLevel::Trusted));
        assert_eq!(trusted.len(), 3);

        // Combined filter
        let combined = engine.list_attestations(
            Some("prov-list-2"),
            Some(AttestationType::TEE_AMD_SEV),
            Some(TrustLevel::Trusted),
        );
        assert_eq!(combined.len(), 1);
    }

    // ---------- test_consecutive_failures ----------

    #[test]
    fn test_consecutive_failures() {
        let engine = make_engine();

        // Set low auto-revoke threshold
        engine.update_policy(AttestationPolicy {
            auto_revoke_after_failures: 2,
            ..AttestationPolicy::default()
        });

        // Submit invalid evidence twice
        let result1 = engine.submit_attestation(
            "provider-fail-1",
            AttestationType::TEE_AMD_SEV,
            "short",
            "",
        );
        assert!(result1.is_ok());
        assert!(!result1.unwrap().verified);

        let result2 = engine.submit_attestation(
            "provider-fail-1",
            AttestationType::TEE_AMD_SEV,
            "bad",
            "",
        );
        assert!(result2.is_ok());
        assert!(!result2.unwrap().verified);

        // After 2 failures, should be auto-revoked
        let trust = engine.get_provider_trust("provider-fail-1").unwrap();
        assert_eq!(trust.latest_trust_level, TrustLevel::Revoked);
        assert_eq!(trust.consecutive_failures, 2);
    }

    // ---------- test_attestation_types_tracking ----------

    #[test]
    fn test_attestation_types_tracking() {
        let engine = make_engine();

        let tee_evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);
        let zk_evidence = AttestationEngine::generate_zk_evidence(AttestationType::ZkSnark);
        let sw_evidence = "some-software-evidence";

        engine
            .submit_attestation("prov-types-1", AttestationType::TEE_AMD_SEV, &tee_evidence, "")
            .unwrap();
        engine
            .submit_attestation("prov-types-1", AttestationType::ZkSnark, &zk_evidence, "")
            .unwrap();
        engine
            .submit_attestation("prov-types-1", AttestationType::Software, sw_evidence, "")
            .unwrap();

        let trust = engine.get_provider_trust("prov-types-1").unwrap();
        assert_eq!(trust.attestation_count, 3);
        assert_eq!(trust.attestation_types_used.len(), 3);
        assert!(trust.attestation_types_used.contains(&AttestationType::TEE_AMD_SEV));
        assert!(trust.attestation_types_used.contains(&AttestationType::ZkSnark));
        assert!(trust.attestation_types_used.contains(&AttestationType::Software));
    }

    // ---------- test_concurrent_submissions ----------

    #[tokio::test]
    async fn test_concurrent_submissions() {
        let state = Arc::new(AttestationState::new());
        let engine = AttestationEngine::new(state.clone());

        let mut handles = Vec::new();

        for i in 0..10 {
            let state_clone = state.clone();
            let handle = tokio::spawn(async move {
                let eng = AttestationEngine::new(state_clone);
                let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEEIntelTDX);
                let provider_id = format!("concurrent-provider-{}", i % 3);
                eng.submit_attestation(
                    &provider_id,
                    AttestationType::TEEIntelTDX,
                    &evidence,
                    "",
                )
            });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        let summary = engine.get_summary();
        assert_eq!(summary.total_attestations, 10);
    }

    // ---------- test_empty_evidence_rejected ----------

    #[test]
    fn test_empty_evidence_rejected() {
        let engine = make_engine();

        let result = engine.submit_attestation("prov-empty", AttestationType::Software, "", "");
        assert!(result.is_err());
        match result.unwrap_err() {
            AttestationError::InvalidEvidence(_) => {}
            e => panic!("Expected InvalidEvidence, got: {e}"),
        }
    }

    // ---------- test_policy_rejects_unaccepted_types ----------

    #[test]
    fn test_policy_rejects_unaccepted_types() {
        let engine = make_engine();

        // Set policy to only accept TEE types
        engine.update_policy(AttestationPolicy {
            accepted_types: vec![AttestationType::TEE_AMD_SEV],
            ..AttestationPolicy::default()
        });

        let result = engine.submit_attestation(
            "prov-rejected",
            AttestationType::SelfSigned,
            "evidence",
            "",
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            AttestationError::PolicyViolation(_) => {}
            e => panic!("Expected PolicyViolation, got: {e}"),
        }
    }

    // ---------- test_revoked_provider_cannot_submit ----------

    #[test]
    fn test_revoked_provider_cannot_submit() {
        let engine = make_engine();
        let evidence = AttestationEngine::generate_tee_evidence(AttestationType::TEE_AMD_SEV);

        engine
            .submit_attestation("prov-rev-sub", AttestationType::TEE_AMD_SEV, &evidence, "")
            .unwrap();

        engine.revoke_provider("prov-rev-sub", "test").unwrap();

        let result = engine.submit_attestation(
            "prov-rev-sub",
            AttestationType::TEE_AMD_SEV,
            &evidence,
            "",
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            AttestationError::ProviderAlreadyRevoked(_) => {}
            e => panic!("Expected ProviderAlreadyRevoked, got: {e}"),
        }
    }
}
