#![allow(dead_code)]
//! Contract Verification Engine
//!
//! Verifies ErgoScript contracts used in the Xergon protocol.
//! Validates ErgoTree, checks register layouts, analyzes spend paths,
//! estimates box sizes and fees.

use axum::{
    extract::State,
    Json,
    Router,
    routing::{get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use crate::proxy;

// ================================================================
// Types
// ================================================================

/// Sigma type prefix bytes for detection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SigmaType {
    /// 0e08 = Coll[Byte]
    CollByte,
    /// 0e05 = SString (also Coll[Byte])
    SString,
    /// 0e29 = SInt
    SInt,
    /// 0e21 = SLong
    SLong,
    /// 0e0b = GroupElement (EC point)
    GroupElement,
    /// 0e08cd02 = SigmaProp (proveDlog)
    SigmaProp,
    /// 0e0c = SBoolean
    SBoolean,
    /// Unknown / unparseable
    Unknown(String),
}

impl SigmaType {
    fn from_prefix(hex: &str) -> Self {
        let clean = hex.trim_start_matches("0x");
        if clean.starts_with("0e08cd02") {
            SigmaType::SigmaProp
        } else if clean.starts_with("0e0b") {
            SigmaType::GroupElement
        } else if clean.starts_with("0e21") {
            SigmaType::SLong
        } else if clean.starts_with("0e29") {
            SigmaType::SInt
        } else if clean.starts_with("0e0c") {
            SigmaType::SBoolean
        } else if clean.starts_with("0e08") || clean.starts_with("0e05") {
            SigmaType::CollByte
        } else {
            SigmaType::Unknown(hex.to_string())
        }
    }

    fn prefix_bytes(&self) -> &'static str {
        match self {
            SigmaType::CollByte => "0e08",
            SigmaType::SString => "0e05",
            SigmaType::SInt => "0e29",
            SigmaType::SLong => "0e21",
            SigmaType::GroupElement => "0e0b",
            SigmaType::SigmaProp => "0e08cd02",
            SigmaType::SBoolean => "0e0c",
            SigmaType::Unknown(_) => "??",
        }
    }
}

/// Register specification for a contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSpec {
    /// Register number (R4-R9)
    pub register: u8,
    /// Expected Sigma type
    pub expected_type: SigmaType,
    /// Human-readable purpose
    pub purpose: String,
    /// Whether this register is required
    pub required: bool,
}

/// Contract specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSpec {
    /// Contract name (e.g. "provider_box", "user_staking")
    pub name: String,
    /// Contract type category
    pub contract_type: String,
    /// Expected register layout
    pub registers: Vec<RegisterSpec>,
    /// Expected tokens (name, required flag)
    pub tokens: Vec<TokenSpec>,
    /// Description of what the contract does
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpec {
    pub name: String,
    pub required: bool,
    pub purpose: String,
}

/// Spend path analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendPath {
    /// Path identifier
    pub path_id: String,
    /// Human-readable description
    pub description: String,
    /// Required proofs (e.g. "proveDlog(R4)")
    pub required_proofs: Vec<String>,
    /// Conditions that must hold
    pub conditions: Vec<String>,
    /// Security score 1-10
    pub security_score: u8,
    /// Whether this path has time-lock protection
    pub has_time_lock: bool,
    /// Detected vulnerabilities
    pub vulnerabilities: Vec<String>,
}

/// Verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub valid: bool,
    pub contract_name: String,
    pub ergo_tree_hex: String,
    pub checks: Vec<CheckResult>,
    pub overall_score: u8,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub check_name: String,
    pub passed: bool,
    pub message: String,
    pub severity: String,
}

/// Register verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterVerificationResult {
    pub valid: bool,
    pub contract_name: String,
    pub register_results: Vec<RegisterCheckResult>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterCheckResult {
    pub register: String,
    pub expected_type: SigmaType,
    pub actual_type: SigmaType,
    pub type_match: bool,
    pub purpose: String,
    pub required: bool,
}

/// Box estimation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxEstimation {
    pub estimated_size_bytes: usize,
    pub min_value_nanoerg: u64,
    pub min_value_erg: f64,
    pub rent_cycles_survivable: u64,
    pub fee_per_cycle_nanoerg: u64,
    pub breakdown: SizeBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeBreakdown {
    pub ergo_tree_bytes: usize,
    pub register_bytes: usize,
    pub token_bytes: usize,
    pub overhead_bytes: usize,
}

/// Security report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    pub generated_at: String,
    pub contracts: Vec<ContractSecuritySummary>,
    pub overall_score: u8,
    pub critical_issues: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSecuritySummary {
    pub name: String,
    pub score: u8,
    pub spend_paths: Vec<SpendPath>,
    pub vulnerabilities: Vec<String>,
    pub strengths: Vec<String>,
}

/// Full audit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAuditResult {
    pub verification: VerificationResult,
    pub register_check: RegisterVerificationResult,
    pub spend_analysis: Vec<SpendPath>,
    pub box_estimation: BoxEstimation,
    pub security_score: u8,
}

// ================================================================
// Contract Registry (built-in specs)
// ================================================================

pub struct ContractRegistry {
    specs: DashMap<String, ContractSpec>,
}

impl ContractRegistry {
    pub fn new() -> Self {
        let registry = Self {
            specs: DashMap::new(),
        };
        registry.load_builtin_specs();
        registry
    }

    fn load_builtin_specs(&self) {
        // Provider Box
        self.specs.insert("provider_box".to_string(), ContractSpec {
            name: "provider_box".to_string(),
            contract_type: "state_box".to_string(),
            description: "Per-provider state box, updated on heartbeat. Identified by singleton Provider NFT.".to_string(),
            registers: vec![
                RegisterSpec { register: 4, expected_type: SigmaType::GroupElement, purpose: "Provider public key (proveDlog)".to_string(), required: true },
                RegisterSpec { register: 5, expected_type: SigmaType::CollByte, purpose: "Endpoint URL (UTF-8 encoded)".to_string(), required: true },
                RegisterSpec { register: 6, expected_type: SigmaType::CollByte, purpose: "Models served (JSON array)".to_string(), required: true },
                RegisterSpec { register: 7, expected_type: SigmaType::SInt, purpose: "PoNW score (0-1000)".to_string(), required: true },
                RegisterSpec { register: 8, expected_type: SigmaType::SInt, purpose: "Last heartbeat height".to_string(), required: true },
                RegisterSpec { register: 9, expected_type: SigmaType::CollByte, purpose: "Region (UTF-8 encoded)".to_string(), required: true },
            ],
            tokens: vec![
                TokenSpec { name: "Provider NFT".to_string(), required: true, purpose: "Singleton NFT identifying this provider".to_string() },
            ],
        });

        // User Staking Box
        self.specs.insert("user_staking".to_string(), ContractSpec {
            name: "user_staking".to_string(),
            contract_type: "balance_box".to_string(),
            description: "User balance box. ERG value IS the balance. Only user can spend.".to_string(),
            registers: vec![
                RegisterSpec { register: 4, expected_type: SigmaType::SigmaProp, purpose: "User public key (SigmaProp for proveDlog)".to_string(), required: true },
                RegisterSpec { register: 5, expected_type: SigmaType::SLong, purpose: "Last activity timestamp".to_string(), required: false },
            ],
            tokens: vec![],
        });

        // Usage Proof Box
        self.specs.insert("usage_proof".to_string(), ContractSpec {
            name: "usage_proof".to_string(),
            contract_type: "receipt_box".to_string(),
            description: "Immutable receipt created after each inference request. No spending restriction.".to_string(),
            registers: vec![
                RegisterSpec { register: 4, expected_type: SigmaType::CollByte, purpose: "User public key hash".to_string(), required: true },
                RegisterSpec { register: 5, expected_type: SigmaType::CollByte, purpose: "Provider NFT ID".to_string(), required: true },
                RegisterSpec { register: 6, expected_type: SigmaType::SLong, purpose: "Token count (input)".to_string(), required: true },
                RegisterSpec { register: 7, expected_type: SigmaType::SLong, purpose: "Token count (output)".to_string(), required: true },
                RegisterSpec { register: 8, expected_type: SigmaType::CollByte, purpose: "Model ID".to_string(), required: true },
                RegisterSpec { register: 9, expected_type: SigmaType::SLong, purpose: "Timestamp".to_string(), required: true },
            ],
            tokens: vec![],
        });

        // Treasury Box
        self.specs.insert("treasury".to_string(), ContractSpec {
            name: "treasury".to_string(),
            contract_type: "governance_box".to_string(),
            description: "Protocol treasury holding ERG for airdrops, incentives, and governance.".to_string(),
            registers: vec![
                RegisterSpec { register: 4, expected_type: SigmaType::SigmaProp, purpose: "Governance authority key".to_string(), required: true },
                RegisterSpec { register: 5, expected_type: SigmaType::SLong, purpose: "Total ERG allocated".to_string(), required: true },
            ],
            tokens: vec![
                TokenSpec { name: "Xergon Network NFT".to_string(), required: true, purpose: "Protocol identity singleton".to_string() },
            ],
        });

        // Governance Proposal Box
        self.specs.insert("governance_proposal".to_string(), ContractSpec {
            name: "governance_proposal".to_string(),
            contract_type: "governance_box".to_string(),
            description: "On-chain governance proposal with voting state.".to_string(),
            registers: vec![
                RegisterSpec { register: 4, expected_type: SigmaType::CollByte, purpose: "Proposal description hash".to_string(), required: true },
                RegisterSpec { register: 5, expected_type: SigmaType::SLong, purpose: "Votes for".to_string(), required: true },
                RegisterSpec { register: 6, expected_type: SigmaType::SLong, purpose: "Votes against".to_string(), required: true },
                RegisterSpec { register: 7, expected_type: SigmaType::SLong, purpose: "Creation height".to_string(), required: true },
                RegisterSpec { register: 8, expected_type: SigmaType::SLong, purpose: "Voting deadline height".to_string(), required: true },
                RegisterSpec { register: 9, expected_type: SigmaType::CollByte, purpose: "Proposer public key hash".to_string(), required: true },
            ],
            tokens: vec![],
        });

        // Provider Slashing Box
        self.specs.insert("provider_slashing".to_string(), ContractSpec {
            name: "provider_slashing".to_string(),
            contract_type: "penalty_box".to_string(),
            description: "Slashing evidence box for penalizing misbehaving providers.".to_string(),
            registers: vec![
                RegisterSpec { register: 4, expected_type: SigmaType::CollByte, purpose: "Provider NFT ID".to_string(), required: true },
                RegisterSpec { register: 5, expected_type: SigmaType::CollByte, purpose: "Evidence hash".to_string(), required: true },
                RegisterSpec { register: 6, expected_type: SigmaType::SLong, purpose: "Slash amount (nanoERG)".to_string(), required: true },
                RegisterSpec { register: 7, expected_type: SigmaType::SLong, purpose: "Report timestamp".to_string(), required: true },
            ],
            tokens: vec![],
        });
    }

    pub fn get_spec(&self, name: &str) -> Option<ContractSpec> {
        self.specs.get(name).map(|r| r.value().clone())
    }

    pub fn all_specs(&self) -> Vec<ContractSpec> {
        self.specs.iter().map(|r| r.value().clone()).collect()
    }

    pub fn register_custom(&self, spec: ContractSpec) {
        self.specs.insert(spec.name.clone(), spec);
    }
}

// ================================================================
// Verification Engine
// ================================================================

pub struct ContractVerifier {
    registry: Arc<ContractRegistry>,
}

impl ContractVerifier {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ContractRegistry::new()),
        }
    }

    /// Validate ErgoTree hex string
    pub fn validate_ergo_tree(&self, ergo_tree_hex: &str) -> VerificationResult {
        let mut checks = Vec::new();
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // Check 1: Valid hex
        let hex_clean = ergo_tree_hex.trim_start_matches("0x");
        let is_valid_hex = hex_clean.chars().all(|c| c.is_ascii_hexdigit());
        checks.push(CheckResult {
            check_name: "valid_hex".to_string(),
            passed: is_valid_hex,
            message: if is_valid_hex { "ErgoTree is valid hex".to_string() } else { "ErgoTree contains non-hex characters".to_string() },
            severity: "critical".to_string(),
        });
        if !is_valid_hex {
            errors.push("Invalid hex encoding".to_string());
        }

        // Check 2: Minimum length (ErgoTree header is at least 4 bytes = 8 hex chars)
        let has_min_length = hex_clean.len() >= 8;
        checks.push(CheckResult {
            check_name: "min_length".to_string(),
            passed: has_min_length,
            message: format!("ErgoTree length: {} bytes (min: 4)", hex_clean.len() / 2),
            severity: "critical".to_string(),
        });

        // Check 3: Version byte (first byte should be 0x00 for v1)
        if hex_clean.len() >= 2 {
            let version = u8::from_str_radix(&hex_clean[0..2], 16).unwrap_or(0xFF);
            let valid_version = version == 0x00;
            checks.push(CheckResult {
                check_name: "version_byte".to_string(),
                passed: valid_version,
                message: format!("ErgoTree version: 0x{:02x}", version),
                severity: "warning".to_string(),
            });
            if !valid_version {
                warnings.push(format!("Non-standard ErgoTree version: 0x{:02x}", version));
            }
        }

        // Check 4: Header size byte (second byte)
        if hex_clean.len() >= 4 {
            let header_size = u8::from_str_radix(&hex_clean[2..4], 16).unwrap_or(0xFF);
            let valid_header = (1..=10).contains(&header_size);
            checks.push(CheckResult {
                check_name: "header_size".to_string(),
                passed: valid_header,
                message: format!("Header size: {} bytes", header_size),
                severity: "warning".to_string(),
            });
        }

        // Check 5: Contains proveDlog (basic spending protection)
        let has_prove_dlog = hex_clean.contains("cd02");
        checks.push(CheckResult {
            check_name: "has_prove_dlog".to_string(),
            passed: has_prove_dlog,
            message: if has_prove_dlog { "Contract requires proveDlog (signature)".to_string() } else { "No proveDlog found - contract may be unprotected!".to_string() },
            severity: "critical".to_string(),
        });
        if !has_prove_dlog {
            errors.push("Contract has no proveDlog - potentially unprotected spending".to_string());
        }

        // Check 6: Has HEIGHT check (time-lock awareness)
        let has_height = hex_clean.contains("0422") || hex_clean.contains("HEIGHT");
        checks.push(CheckResult {
            check_name: "has_height_check".to_string(),
            passed: has_height,
            message: if has_height { "Contract references HEIGHT".to_string() } else { "No HEIGHT reference found".to_string() },
            severity: "info".to_string(),
        });
        if !has_height {
            warnings.push("Contract does not reference HEIGHT - no time-locked operations".to_string());
        }

        // Check 7: Detects AND/OR logic complexity
        let and_count = hex_clean.matches("cff8").count();
        let or_count = hex_clean.matches("cff4").count();
        checks.push(CheckResult {
            check_name: "logic_complexity".to_string(),
            passed: and_count + or_count < 10,
            message: format!("AND branches: {}, OR branches: {}", and_count, or_count),
            severity: "info".to_string(),
        });
        if and_count + or_count >= 10 {
            warnings.push("High logic complexity - consider simplifying for auditability".to_string());
        }

        let all_passed = checks.iter().all(|c| c.passed || c.severity == "info");
        let critical_passed = checks.iter().filter(|c| c.severity == "critical").all(|c| c.passed);
        let score = if critical_passed && warnings.is_empty() { 9 }
                     else if critical_passed { 7 }
                     else if all_passed { 5 }
                     else { 3 };

        VerificationResult {
            valid: critical_passed,
            contract_name: "unknown".to_string(),
            ergo_tree_hex: ergo_tree_hex.to_string(),
            checks,
            overall_score: score,
            warnings,
            errors,
        }
    }

    /// Verify registers against contract spec
    pub fn verify_registers(
        &self,
        contract_name: &str,
        registers: &HashMap<String, String>,
    ) -> RegisterVerificationResult {
        let spec = match self.registry.get_spec(contract_name) {
            Some(s) => s,
            None => {
                return RegisterVerificationResult {
                    valid: false,
                    contract_name: contract_name.to_string(),
                    register_results: vec![],
                    warnings: vec![],
                    errors: vec![format!("Unknown contract: {}", contract_name)],
                };
            }
        };

        let mut results = Vec::new();
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        for reg_spec in &spec.registers {
            let reg_key = format!("R{}", reg_spec.register);
            let actual_hex = registers.get(&reg_key).cloned().unwrap_or_default();

            let actual_type = if actual_hex.is_empty() {
                SigmaType::Unknown("empty".to_string())
            } else {
                SigmaType::from_prefix(&actual_hex)
            };

            let type_match = actual_hex.is_empty() && !reg_spec.required
                || !actual_hex.is_empty() && std::mem::discriminant(&actual_type) == std::mem::discriminant(&reg_spec.expected_type);

            if !reg_spec.required && actual_hex.is_empty() {
                results.push(RegisterCheckResult {
                    register: reg_key,
                    expected_type: reg_spec.expected_type.clone(),
                    actual_type: SigmaType::Unknown("empty".to_string()),
                    type_match: true,
                    purpose: reg_spec.purpose.clone(),
                    required: false,
                });
            } else if !type_match {
                let msg = format!(
                    "R{} type mismatch: expected {:?}, got {:?}",
                    reg_spec.register, reg_spec.expected_type, actual_type
                );
                if reg_spec.required {
                    errors.push(msg.clone());
                } else {
                    warnings.push(msg.clone());
                }
                results.push(RegisterCheckResult {
                    register: reg_key,
                    expected_type: reg_spec.expected_type.clone(),
                    actual_type,
                    type_match: false,
                    purpose: reg_spec.purpose.clone(),
                    required: reg_spec.required,
                });
            } else {
                results.push(RegisterCheckResult {
                    register: reg_key,
                    expected_type: reg_spec.expected_type.clone(),
                    actual_type,
                    type_match: true,
                    purpose: reg_spec.purpose.clone(),
                    required: reg_spec.required,
                });
            }
        }

        let valid = errors.is_empty();
        RegisterVerificationResult {
            valid,
            contract_name: contract_name.to_string(),
            register_results: results,
            warnings,
            errors,
        }
    }

    /// Analyze spend paths for a contract
    pub fn analyze_spend_paths(&self, contract_name: &str) -> Vec<SpendPath> {
        let mut paths = Vec::new();

        match contract_name {
            "provider_box" => {
                paths.push(SpendPath {
                    path_id: "heartbeat_update".to_string(),
                    description: "Provider updates heartbeat (spends box, recreates with new R8)".to_string(),
                    required_proofs: vec!["proveDlog(R4)".to_string()],
                    conditions: vec!["Output preserves Provider NFT".to_string(), "R4 unchanged".to_string()],
                    security_score: 9,
                    has_time_lock: false,
                    vulnerabilities: vec![],
                });
                paths.push(SpendPath {
                    path_id: "deregister".to_string(),
                    description: "Provider deregisters and extracts ERG".to_string(),
                    required_proofs: vec!["proveDlog(R4)".to_string()],
                    conditions: vec![],
                    security_score: 8,
                    has_time_lock: false,
                    vulnerabilities: vec!["No cooldown period after deregistration".to_string()],
                });
            }
            "user_staking" => {
                paths.push(SpendPath {
                    path_id: "payment".to_string(),
                    description: "User pays for inference (box value decreases)".to_string(),
                    required_proofs: vec!["proveDlog(R4)".to_string()],
                    conditions: vec!["Output value <= input value - fee".to_string()],
                    security_score: 9,
                    has_time_lock: false,
                    vulnerabilities: vec![],
                });
                paths.push(SpendPath {
                    path_id: "top_up".to_string(),
                    description: "User adds more ERG to staking box".to_string(),
                    required_proofs: vec!["proveDlog(R4)".to_string()],
                    conditions: vec!["Output value > input value".to_string()],
                    security_score: 10,
                    has_time_lock: false,
                    vulnerabilities: vec![],
                });
            }
            "usage_proof" => {
                paths.push(SpendPath {
                    path_id: "anyone_can_create".to_string(),
                    description: "Anyone can create a usage proof box (receipt)".to_string(),
                    required_proofs: vec![],
                    conditions: vec![],
                    security_score: 6,
                    has_time_lock: false,
                    vulnerabilities: vec!["No spending restriction - box is immutable receipt".to_string(), "Could be spammed with fake proofs".to_string()],
                });
            }
            "treasury" => {
                paths.push(SpendPath {
                    path_id: "governance_spend".to_string(),
                    description: "Governance authority spends treasury funds".to_string(),
                    required_proofs: vec!["proveDlog(R4) - governance key".to_string()],
                    conditions: vec!["Output preserves Xergon Network NFT".to_string()],
                    security_score: 8,
                    has_time_lock: false,
                    vulnerabilities: vec!["Single key controls treasury - consider multi-sig".to_string()],
                });
            }
            "governance_proposal" => {
                paths.push(SpendPath {
                    path_id: "vote".to_string(),
                    description: "Cast a vote on the proposal".to_string(),
                    required_proofs: vec!["proveDlog(voter_key)".to_string()],
                    conditions: vec!["HEIGHT < deadline".to_string()],
                    security_score: 8,
                    has_time_lock: true,
                    vulnerabilities: vec![],
                });
                paths.push(SpendPath {
                    path_id: "execute".to_string(),
                    description: "Execute the proposal after voting deadline".to_string(),
                    required_proofs: vec!["proveDlog(proposer_key)".to_string()],
                    conditions: vec!["HEIGHT >= deadline".to_string(), "votes_for > votes_against".to_string()],
                    security_score: 9,
                    has_time_lock: true,
                    vulnerabilities: vec![],
                });
            }
            _ => {
                paths.push(SpendPath {
                    path_id: "unknown".to_string(),
                    description: "No spend path analysis available for this contract".to_string(),
                    required_proofs: vec![],
                    conditions: vec![],
                    security_score: 0,
                    has_time_lock: false,
                    vulnerabilities: vec!["Unknown contract - manual review required".to_string()],
                });
            }
        }

        paths
    }

    /// Estimate box size and minimum value
    pub fn estimate_box(
        &self,
        ergo_tree_hex: &str,
        registers: &HashMap<String, String>,
        token_count: usize,
    ) -> BoxEstimation {
        // ErgoTree size (hex chars / 2 = bytes)
        let ergo_tree_bytes = ergo_tree_hex.trim_start_matches("0x").len() / 2;

        // Register sizes
        let mut register_bytes = 0usize;
        for (_, hex_val) in registers {
            register_bytes += hex_val.trim_start_matches("0x").len() / 2;
            // Each register has a 2-byte overhead (register ID + type tag)
            register_bytes += 2;
        }

        // Token size: each token = 32 bytes (token ID) + 8 bytes (amount)
        let token_bytes = if token_count > 0 {
            4 + (token_count * 40) // 4 bytes for token count prefix
        } else {
            0
        };

        // Box overhead: 2 bytes (box flags) + variable (but ~3 bytes minimum)
        let overhead_bytes = 5;

        let estimated_size_bytes = ergo_tree_bytes + register_bytes + token_bytes + overhead_bytes;

        // Minimum box value: size * 360 nanoERG/byte
        let fee_per_byte: u64 = 360;
        let min_value_nanoerg = (estimated_size_bytes as u64) * fee_per_byte;
        let min_value_erg = min_value_nanoerg as f64 / 1_000_000_000.0;

        // Rent cycles: how many 4-year cycles can this box survive?
        // One cycle costs: estimated_size * 360 nanoERG
        // Using a typical box value of 1 ERG for estimation
        let typical_box_value: u64 = 1_000_000_000;
        let rent_cycles = if min_value_nanoerg > 0 {
            typical_box_value / min_value_nanoerg
        } else {
            0
        };

        BoxEstimation {
            estimated_size_bytes,
            min_value_nanoerg,
            min_value_erg,
            rent_cycles_survivable: rent_cycles,
            fee_per_cycle_nanoerg: min_value_nanoerg,
            breakdown: SizeBreakdown {
                ergo_tree_bytes,
                register_bytes,
                token_bytes,
                overhead_bytes,
            },
        }
    }

    /// Generate security report for all contracts
    pub fn generate_security_report(&self) -> SecurityReport {
        let mut contracts = Vec::new();
        let mut all_critical = Vec::new();
        let mut all_recommendations = Vec::new();
        let mut total_score = 0u32;
        let count = self.registry.all_specs().len();

        for spec in self.registry.all_specs() {
            let paths = self.analyze_spend_paths(&spec.name);
            let mut vulns = Vec::new();
            let mut strengths = Vec::new();
            let mut score = 10u8;

            for path in &paths {
                vulns.extend(path.vulnerabilities.clone());
                if path.has_time_lock {
                    strengths.push(format!("{}: has time-lock protection", path.path_id));
                }
                if !path.required_proofs.is_empty() {
                    strengths.push(format!("{}: requires proof(s)", path.path_id));
                }
                if path.security_score < 5 {
                    score = score.min(path.security_score);
                    all_critical.push(format!("{}: {}", spec.name, path.description));
                }
            }

            if spec.tokens.iter().any(|t| t.required && t.name.contains("NFT")) {
                strengths.push("Uses singleton NFT for identity".to_string());
            }

            total_score += score as u32;
            contracts.push(ContractSecuritySummary {
                name: spec.name.clone(),
                score,
                spend_paths: paths,
                vulnerabilities: vulns,
                strengths,
            });
        }

        all_recommendations.push("Add multi-sig for treasury spending".to_string());
        all_recommendations.push("Consider rate-limiting governance proposals".to_string());
        all_recommendations.push("Add cooldown to provider deregistration".to_string());

        let overall = if count > 0 { (total_score / count as u32) as u8 } else { 0 };

        SecurityReport {
            generated_at: chrono::Utc::now().to_rfc3339(),
            contracts,
            overall_score: overall,
            critical_issues: all_critical,
            recommendations: all_recommendations,
        }
    }

    /// Full audit: verify + registers + spend paths + box estimation
    pub fn full_audit(
        &self,
        contract_name: &str,
        ergo_tree_hex: &str,
        registers: &HashMap<String, String>,
        token_count: usize,
    ) -> FullAuditResult {
        let mut verification = self.validate_ergo_tree(ergo_tree_hex);
        verification.contract_name = contract_name.to_string();

        let register_check = self.verify_registers(contract_name, registers);
        let spend_analysis = self.analyze_spend_paths(contract_name);
        let box_estimation = self.estimate_box(ergo_tree_hex, registers, token_count);

        let security_score = if verification.valid && register_check.valid { 8 }
                             else if verification.valid { 5 }
                             else { 3 };

        FullAuditResult {
            verification,
            register_check,
            spend_analysis,
            box_estimation,
            security_score,
        }
    }
}

// ================================================================
// REST API
// ================================================================

pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    let verifier = Arc::new(ContractVerifier::new());
    Router::new()
        .route("/v1/contracts/verify", post(verify_ergo_tree))
        .route("/v1/contracts/verify-registers", post(verify_registers))
        .route("/v1/contracts/registry", get(list_registry))
        .route("/v1/contracts/:name", get(get_contract_spec))
        .route("/v1/contracts/analyze-spends", post(analyze_spends))
        .route("/v1/contracts/estimate-box", post(estimate_box))
        .route("/v1/contracts/security-report", get(security_report))
        .route("/v1/contracts/audit", post(full_audit))
        .with_state(verifier)
}

async fn verify_ergo_tree(
    State(verifier): State<Arc<ContractVerifier>>,
    Json(body): Json<serde_json::Value>,
) -> Json<VerificationResult> {
    let hex = body.get("ergo_tree_hex")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Json(verifier.validate_ergo_tree(hex))
}

#[derive(Deserialize)]
struct VerifyRegistersRequest {
    contract_name: String,
    registers: HashMap<String, String>,
}

async fn verify_registers(
    State(verifier): State<Arc<ContractVerifier>>,
    Json(body): Json<VerifyRegistersRequest>,
) -> Json<RegisterVerificationResult> {
    Json(verifier.verify_registers(&body.contract_name, &body.registers))
}

async fn list_registry(
    State(verifier): State<Arc<ContractVerifier>>,
) -> Json<Vec<ContractSpec>> {
    Json(verifier.registry.all_specs())
}

async fn get_contract_spec(
    State(verifier): State<Arc<ContractVerifier>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    match verifier.registry.get_spec(&name) {
        Some(spec) => Json(serde_json::to_value(spec).unwrap_or_default()),
        None => Json(serde_json::json!({"error": format!("Contract '{}' not found", name)})),
    }
}

#[derive(Deserialize)]
struct AnalyzeSpendsRequest {
    contract_name: String,
}

async fn analyze_spends(
    State(verifier): State<Arc<ContractVerifier>>,
    Json(body): Json<AnalyzeSpendsRequest>,
) -> Json<Vec<SpendPath>> {
    Json(verifier.analyze_spend_paths(&body.contract_name))
}

#[derive(Deserialize)]
struct EstimateBoxRequest {
    ergo_tree_hex: String,
    registers: HashMap<String, String>,
    token_count: Option<usize>,
}

async fn estimate_box(
    State(verifier): State<Arc<ContractVerifier>>,
    Json(body): Json<EstimateBoxRequest>,
) -> Json<BoxEstimation> {
    Json(verifier.estimate_box(&body.ergo_tree_hex, &body.registers, body.token_count.unwrap_or(0)))
}

async fn security_report(
    State(verifier): State<Arc<ContractVerifier>>,
) -> Json<SecurityReport> {
    Json(verifier.generate_security_report())
}

#[derive(Deserialize)]
struct FullAuditRequest {
    contract_name: String,
    ergo_tree_hex: String,
    registers: HashMap<String, String>,
    token_count: Option<usize>,
}

async fn full_audit(
    State(verifier): State<Arc<ContractVerifier>>,
    Json(body): Json<FullAuditRequest>,
) -> Json<FullAuditResult> {
    Json(verifier.full_audit(&body.contract_name, &body.ergo_tree_hex, &body.registers, body.token_count.unwrap_or(0)))
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_builtin_specs() {
        let registry = ContractRegistry::new();
        assert!(registry.get_spec("provider_box").is_some());
        assert!(registry.get_spec("user_staking").is_some());
        assert!(registry.get_spec("usage_proof").is_some());
        assert!(registry.get_spec("treasury").is_some());
        assert!(registry.get_spec("governance_proposal").is_some());
        assert!(registry.get_spec("provider_slashing").is_some());
    }

    #[test]
    fn test_registry_provider_box_registers() {
        let registry = ContractRegistry::new();
        let spec = registry.get_spec("provider_box").unwrap();
        assert_eq!(spec.registers.len(), 6);
        assert_eq!(spec.registers[0].register, 4);
        assert_eq!(spec.registers[0].expected_type, SigmaType::GroupElement);
        assert!(spec.registers[0].required);
        assert_eq!(spec.tokens.len(), 1);
        assert!(spec.tokens[0].required);
    }

    #[test]
    fn test_validate_ergo_tree_valid() {
        let verifier = ContractVerifier::new();
        // Minimal ErgoTree with proveDlog: version=00, header=01, then sigma prop with proveDlog
        let hex = "0001cd02e8ec6e8a4b7...";
        let result = verifier.validate_ergo_tree(hex);
        assert!(result.checks.iter().any(|c| c.check_name == "valid_hex" && c.passed));
        assert!(result.checks.iter().any(|c| c.check_name == "has_prove_dlog" && c.passed));
    }

    #[test]
    fn test_validate_ergo_tree_invalid_hex() {
        let verifier = ContractVerifier::new();
        let result = verifier.validate_ergo_tree("zzzz");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Invalid hex")));
    }

    #[test]
    fn test_validate_ergo_tree_no_prove_dlog() {
        let verifier = ContractVerifier::new();
        let hex = "000108abcdef123456";
        let result = verifier.validate_ergo_tree(hex);
        assert!(result.errors.iter().any(|e| e.contains("unprotected")));
    }

    #[test]
    fn test_verify_registers_provider_box() {
        let verifier = ContractVerifier::new();
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), "0e0b02e8ec6e8a4b7".to_string()); // GroupElement prefix
        regs.insert("R5".to_string(), "0e056874747073".to_string()); // SString
        regs.insert("R6".to_string(), "0e055b225d".to_string());      // SString
        regs.insert("R7".to_string(), "0e29c2d101".to_string());      // SInt
        regs.insert("R8".to_string(), "0e29c2d101".to_string());      // SInt
        regs.insert("R9".to_string(), "0e057573".to_string());        // SString
        let result = verifier.verify_registers("provider_box", &regs);
        assert!(result.valid);
    }

    #[test]
    fn test_verify_registers_type_mismatch() {
        let verifier = ContractVerifier::new();
        let mut regs = HashMap::new();
        // R4 should be GroupElement (0e0b) but we give SInt (0e29)
        regs.insert("R4".to_string(), "0e29c2d101".to_string());
        let result = verifier.verify_registers("provider_box", &regs);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("type mismatch")));
    }

    #[test]
    fn test_verify_registers_unknown_contract() {
        let verifier = ContractVerifier::new();
        let result = verifier.verify_registers("nonexistent", &HashMap::new());
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Unknown contract")));
    }

    #[test]
    fn test_analyze_spend_paths_provider_box() {
        let verifier = ContractVerifier::new();
        let paths = verifier.analyze_spend_paths("provider_box");
        assert!(paths.len() >= 2);
        assert!(paths.iter().any(|p| p.path_id == "heartbeat_update"));
        assert!(paths.iter().any(|p| p.path_id == "deregister"));
        assert!(paths.iter().all(|p| p.security_score >= 5));
    }

    #[test]
    fn test_analyze_spend_paths_unknown() {
        let verifier = ContractVerifier::new();
        let paths = verifier.analyze_spend_paths("unknown_contract");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].security_score, 0);
    }

    #[test]
    fn test_estimate_box() {
        let verifier = ContractVerifier::new();
        let ergo_tree = "0001cd02e8ec6e8a4b7abcdef1234567890";
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), "0e0b02e8ec6e8a4b7abcdef".to_string());
        let result = verifier.estimate_box(ergo_tree, &regs, 1);
        assert!(result.estimated_size_bytes > 0);
        assert!(result.min_value_nanoerg > 0);
        assert!(result.min_value_erg > 0.0);
        assert_eq!(result.breakdown.ergo_tree_bytes, ergo_tree.len() / 2);
        assert!(result.breakdown.token_bytes > 0); // 1 token
    }

    #[test]
    fn test_estimate_box_no_tokens() {
        let verifier = ContractVerifier::new();
        let result = verifier.estimate_box("000108", &HashMap::new(), 0);
        assert_eq!(result.breakdown.token_bytes, 0);
    }

    #[test]
    fn test_security_report() {
        let verifier = ContractVerifier::new();
        let report = verifier.generate_security_report();
        assert!(!report.contracts.is_empty());
        assert!(report.overall_score > 0);
        assert!(!report.recommendations.is_empty());
    }

    #[test]
    fn test_full_audit() {
        let verifier = ContractVerifier::new();
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), "0e0b02e8ec6e8a4b7".to_string());
        regs.insert("R5".to_string(), "0e056874747073".to_string());
        let result = verifier.full_audit(
            "provider_box",
            "0001cd02e8ec6e8a4b7abcdef",
            &regs,
            1,
        );
        assert!(result.security_score > 0);
        assert!(!result.spend_analysis.is_empty());
        assert!(result.box_estimation.estimated_size_bytes > 0);
    }

    #[test]
    fn test_sigma_type_detection() {
        assert_eq!(SigmaType::from_prefix("0e08cd02abcd"), SigmaType::SigmaProp);
        assert_eq!(SigmaType::from_prefix("0e0babcd"), SigmaType::GroupElement);
        assert_eq!(SigmaType::from_prefix("0e29abcd"), SigmaType::SInt);
        assert_eq!(SigmaType::from_prefix("0e21abcd"), SigmaType::SLong);
        assert_eq!(SigmaType::from_prefix("0e0cabcd"), SigmaType::SBoolean);
        assert_eq!(SigmaType::from_prefix("0e08abcd"), SigmaType::CollByte);
        assert!(matches!(SigmaType::from_prefix("ffabcd"), SigmaType::Unknown(_)));
    }

    #[test]
    fn test_custom_contract_registration() {
        let registry = ContractRegistry::new();
        let custom = ContractSpec {
            name: "custom_test".to_string(),
            contract_type: "test".to_string(),
            description: "Test contract".to_string(),
            registers: vec![],
            tokens: vec![],
        };
        registry.register_custom(custom);
        assert!(registry.get_spec("custom_test").is_some());
        assert_eq!(registry.all_specs().len(), 7); // 6 builtin + 1 custom
    }
}
