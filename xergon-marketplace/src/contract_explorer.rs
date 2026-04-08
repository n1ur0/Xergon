use axum::{
    extract::{Path, Query, State},
    Json, Router,
    routing::{get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterLayout {
    pub register: String,
    pub sigma_type: String,
    pub description: String,
    pub example_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendPath {
    pub name: String,
    pub description: String,
    pub conditions: Vec<String>,
    pub required_proofs: Vec<String>,
    pub estimated_gas: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVisual {
    pub name: String,
    pub description: String,
    pub ergo_tree_hex: String,
    pub category: String,
    pub register_layout: Vec<RegisterLayout>,
    pub spend_paths: Vec<SpendPath>,
    pub token_requirements: Vec<String>,
    pub security_notes: Vec<String>,
    pub source_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    pub opcodes: Vec<String>,
    pub registers: HashMap<String, String>,
    pub tokens: Vec<String>,
    pub estimated_size: u32,
    pub contracts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareResult {
    pub common: Vec<String>,
    pub only_in_a: Vec<String>,
    pub only_in_b: Vec<String>,
    pub register_diffs: Vec<String>,
    pub size_diff: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaygroundEvalResult {
    pub valid: bool,
    pub result: String,
    pub gas_used: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetrics {
    pub total_contracts: u64,
    pub total_inspections: u64,
    pub total_comparisons: u64,
    pub total_playground_evals: u64,
    pub popular_contracts: Vec<(String, u64)>,
}

// Query / request structs

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    pub category: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InspectRequest {
    pub hex: String,
}

#[derive(Debug, Deserialize)]
pub struct CompareRequest {
    pub hex1: String,
    pub hex2: String,
}

#[derive(Debug, Deserialize)]
pub struct PlaygroundRequest {
    pub hex: String,
    pub inputs: String,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub struct ContractExplorerState {
    pub library: DashMap<String, ContractVisual>,
    pub inspections_total: AtomicU64,
    pub comparisons_total: AtomicU64,
    pub playground_evals: AtomicU64,
    pub contract_views: DashMap<String, AtomicU64>,
}

impl ContractExplorerState {
    pub fn new() -> Self {
        let state = Self {
            library: DashMap::new(),
            inspections_total: AtomicU64::new(0),
            comparisons_total: AtomicU64::new(0),
            playground_evals: AtomicU64::new(0),
            contract_views: DashMap::new(),
        };
        state.load_contracts();
        state
    }

    fn load_contracts(&self) {
        self.library.insert("ProviderBox".to_string(), make_provider_box());
        self.library.insert("ProviderRegistry".to_string(), make_provider_registry());
        self.library.insert("UsageProof".to_string(), make_usage_proof());
        self.library.insert("StakingBox".to_string(), make_staking_box());
        self.library.insert("TreasuryBox".to_string(), make_treasury_box());
        self.library.insert("GpuRental".to_string(), make_gpu_rental());

        // Initialise view counters
        for key in self.library.iter() {
            self.contract_views
                .insert(key.key().clone(), AtomicU64::new(0));
        }
    }

    // -- public helpers -----------------------------------------------------

    pub fn browse_library(&self, category: Option<&str>, search: Option<&str>) -> Vec<ContractVisual> {
        let mut results: Vec<ContractVisual> = self
            .library
            .iter()
            .filter(|entry| {
                let c = &entry.value();
                let cat_ok = category.map_or(true, |cat| {
                    c.category.eq_ignore_ascii_case(cat)
                });
                let search_ok = search.map_or(true, |term| {
                    let term_lower = term.to_lowercase();
                    c.name.to_lowercase().contains(&term_lower)
                        || c.description.to_lowercase().contains(&term_lower)
                        || c.category.to_lowercase().contains(&term_lower)
                });
                cat_ok && search_ok
            })
            .map(|e| e.value().clone())
            .collect();
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results
    }

    pub fn get_contract(&self, name: &str) -> Option<ContractVisual> {
        self.library.get(name).map(|e| e.value().clone())
    }

    pub fn inspect_ergotree(&self, hex: &str) -> InspectResult {
        self.inspections_total.fetch_add(1, Ordering::Relaxed);

        let mut opcodes: Vec<String> = Vec::new();
        let mut registers: HashMap<String, String> = HashMap::new();
        let mut tokens: Vec<String> = Vec::new();
        let mut contracts: Vec<String> = Vec::new();

        // Parse header bytes for ErgoTree version / constants count / etc.
        let bytes = hex::decode(hex).unwrap_or_default();
        if !bytes.is_empty() {
            let version = bytes[0];
            opcodes.push(format!("VERSION_{}", version));
            if bytes.len() > 1 {
                let flags = bytes[1];
                opcodes.push(format!("FLAGS_{:02x}", flags));
            }
            if bytes.len() > 2 {
                let constants_len = bytes[2];
                opcodes.push(format!("NUM_CONSTANTS_{}", constants_len));
            }
        }

        // Detect known patterns in the hex string
        let hex_lower = hex.to_lowercase();
        if hex_lower.contains("e4e500") {
            opcodes.push("PROVE_DLOG".to_string());
        }
        if hex_lower.contains("08cd") {
            opcodes.push("EQ_BYTES".to_string());
        }
        if hex_lower.contains("4e03") {
            opcodes.push("BY_INDEX".to_string());
        }
        if hex_lower.contains("d1db") {
            opcodes.push("FUNC_VALUE".to_string());
        }
        if hex_lower.contains("08cda5") {
            opcodes.push("COLL_SIZE_CHECK".to_string());
        }
        if hex_lower.contains("de") {
            opcodes.push("APPEND".to_string());
        }

        // Simulate register extraction based on length heuristics
        let estimated_regs = (bytes.len() / 34).min(6);
        for i in 0..estimated_regs {
            let reg_name = format!("R{}", 4 + i);
            registers.insert(reg_name, format!("0x{}", &hex[(i * 64)..std::cmp::min((i + 1) * 64, hex.len())]));
        }

        // Detect token-like patterns (32-byte hex sequences that look like token IDs)
        let clean = hex_lower.replace(|c: char| !c.is_ascii_hexdigit(), "");
        let mut pos = 0;
        while pos + 64 <= clean.len() && tokens.len() < 5 {
            let segment = &clean[pos..pos + 64];
            // Use a simple heuristic: if it doesn't look like repeated bytes, consider it
            if segment.chars().collect::<std::collections::HashSet<_>>().len() > 8 {
                tokens.push(format!("0x{}", segment));
            }
            pos += 64;
        }

        // Check if the hex matches any known contract
        for entry in self.library.iter() {
            if entry.value().ergo_tree_hex == hex {
                contracts.push(entry.value().name.clone());
            }
        }

        let estimated_size = (bytes.len() as u32).max(32);

        InspectResult {
            opcodes,
            registers,
            tokens,
            estimated_size,
            contracts,
        }
    }

    pub fn compare_contracts(&self, hex1: &str, hex2: &str) -> CompareResult {
        self.comparisons_total.fetch_add(1, Ordering::Relaxed);

        let set_a: std::collections::HashSet<String> = hex1
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| c.iter().collect())
            .collect();

        let set_b: std::collections::HashSet<String> = hex2
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|c| c.iter().collect())
            .collect();

        let common: Vec<String> = set_a.intersection(&set_b).cloned().collect();
        let only_in_a: Vec<String> = set_a.difference(&set_b).cloned().collect();
        let only_in_b: Vec<String> = set_b.difference(&set_a).cloned().collect();

        let mut common_vec: Vec<String> = common;
        common_vec.sort();
        let mut a_vec: Vec<String> = only_in_a;
        a_vec.sort();
        let mut b_vec: Vec<String> = only_in_b;
        b_vec.sort();

        // Register diff heuristic
        let reg_diffs: Vec<String> = if hex1.len() != hex2.len() {
            vec![format!(
                "Byte length differs: {} vs {}",
                hex1.len(),
                hex2.len()
            )]
        } else {
            let mut diffs = Vec::new();
            let bytes1 = hex::decode(hex1).unwrap_or_default();
            let bytes2 = hex::decode(hex2).unwrap_or_default();
            for (i, (a, b)) in bytes1.iter().zip(bytes2.iter()).enumerate() {
                if a != b {
                    diffs.push(format!("Byte {} differs: 0x{:02x} vs 0x{:02x}", i, a, b));
                }
            }
            diffs.truncate(20);
            diffs
        };

        let size_diff = (hex1.len() as i32) - (hex2.len() as i32);

        CompareResult {
            common: common_vec,
            only_in_a: a_vec,
            only_in_b: b_vec,
            register_diffs: reg_diffs,
            size_diff,
        }
    }

    pub fn playground_evaluate(&self, hex: &str, inputs_json: &str) -> PlaygroundEvalResult {
        self.playground_evals.fetch_add(1, Ordering::Relaxed);

        let bytes = hex::decode(hex);
        if bytes.is_err() {
            return PlaygroundEvalResult {
                valid: false,
                result: "failed".to_string(),
                gas_used: 0,
                error: Some("Invalid hex input".to_string()),
            };
        }
        let bytes = bytes.unwrap();

        if bytes.len() < 3 {
            return PlaygroundEvalResult {
                valid: false,
                result: "failed".to_string(),
                gas_used: 0,
                error: Some("ErgoTree too short (minimum 3 bytes)".to_string()),
            };
        }

        // Validate that inputs_json is valid JSON
        if let Err(e) = serde_json::from_str::<serde_json::Value>(inputs_json) {
            return PlaygroundEvalResult {
                valid: false,
                result: "failed".to_string(),
                gas_used: 0,
                error: Some(format!("Invalid JSON inputs: {}", e)),
            };
        }

        // Simulated evaluation
        let gas_used = (bytes.len() as u32) * 12 + 150;

        // Simple simulation: treat as valid if version byte is 0x00 or 0x01
        let valid = bytes[0] == 0x00 || bytes[0] == 0x01;

        if valid {
            PlaygroundEvalResult {
                valid: true,
                result: "true".to_string(),
                gas_used,
                error: None,
            }
        } else {
            PlaygroundEvalResult {
                valid: false,
                result: "false".to_string(),
                gas_used,
                error: Some(format!(
                    "Evaluation returned false (ErgoTree version 0x{:02x} not supported in simulator)",
                    bytes[0]
                )),
            }
        }
    }

    pub fn get_metrics(&self) -> ContractMetrics {
        let mut popular: Vec<(String, u64)> = self
            .contract_views
            .iter()
            .map(|e| (e.key().clone(), e.value().load(Ordering::Relaxed)))
            .collect();
        popular.sort_by(|a, b| b.1.cmp(&a.1));
        popular.truncate(10);

        ContractMetrics {
            total_contracts: self.library.len() as u64,
            total_inspections: self.inspections_total.load(Ordering::Relaxed),
            total_comparisons: self.comparisons_total.load(Ordering::Relaxed),
            total_playground_evals: self.playground_evals.load(Ordering::Relaxed),
            popular_contracts: popular,
        }
    }

    pub fn get_categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self
            .library
            .iter()
            .map(|e| e.value().category.clone())
            .collect();
        cats.sort();
        cats.dedup();
        cats
    }

    pub fn record_view(&self, contract_name: &str) {
        if let Some(counter) = self.contract_views.get(contract_name) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// Contract factory helpers
// ---------------------------------------------------------------------------

fn make_provider_box() -> ContractVisual {
    ContractVisual {
        name: "ProviderBox".to_string(),
        description: "Represents an active inference provider on the Xergon network. Holds provider metadata, supported models, and proof-of-work scores.".to_string(),
        ergo_tree_hex: "0008cd03e4e50008cd03a5d1db4e0308cde4e50008cd0384cda54e03d1db0808cda54e03d1db08e4e500".to_string(),
        category: "Inference".to_string(),
        register_layout: vec![
            RegisterLayout {
                register: "R4".to_string(),
                sigma_type: "Coll[Byte]".to_string(),
                description: "Unique provider identifier".to_string(),
                example_value: "Coll[Byte](0x4a, 0xbc, ...)".to_string(),
            },
            RegisterLayout {
                register: "R5".to_string(),
                sigma_type: "String".to_string(),
                description: "API endpoint URL for inference requests".to_string(),
                example_value: "\"https://inference.xergon.ai/v1\"".to_string(),
            },
            RegisterLayout {
                register: "R6".to_string(),
                sigma_type: "Coll[String]".to_string(),
                description: "List of supported model identifiers".to_string(),
                example_value: "Coll(\"llama-70b\", \"mixtral-8x7b\")".to_string(),
            },
            RegisterLayout {
                register: "R7".to_string(),
                sigma_type: "SigmaProp".to_string(),
                description: "Provider's authorization proof key".to_string(),
                example_value: "proveDlog(pk)".to_string(),
            },
            RegisterLayout {
                register: "R8".to_string(),
                sigma_type: "Int".to_string(),
                description: "Proof-of-work network reputation score".to_string(),
                example_value: "9500".to_string(),
            },
        ],
        spend_paths: vec![
            SpendPath {
                name: "heartbeat".to_string(),
                description: "Provider sends periodic liveness proof to maintain active status.".to_string(),
                conditions: vec!["R7 must prove valid signature".to_string(), "Last heartbeat within 24 hours".to_string()],
                required_proofs: vec!["SigmaProp (provider key)".to_string()],
                estimated_gas: 12_000,
            },
            SpendPath {
                name: "deregister".to_string(),
                description: "Provider voluntarily removes itself from the network.".to_string(),
                conditions: vec!["R7 signature required".to_string(), "No pending settlements".to_string()],
                required_proofs: vec!["SigmaProp (provider key)".to_string()],
                estimated_gas: 8_500,
            },
            SpendPath {
                name: "settle".to_string(),
                description: "Settle accumulated inference payments to provider wallet.".to_string(),
                conditions: vec!["Min 1 ERG accumulated".to_string(), "Settlement period elapsed".to_string()],
                required_proofs: vec!["SigmaProp (provider key)".to_string(), "UsageProof box reference".to_string()],
                estimated_gas: 25_000,
            },
        ],
        token_requirements: vec![
            "XRG token for staking collateral (min 100 XRG)".to_string(),
            "Provider NFT identifier in R4".to_string(),
        ],
        security_notes: vec![
            "Provider key rotation requires governance approval to prevent hijacking.".to_string(),
            "Heartbeat timeout of 24h ensures stale providers are automatically deactivated.".to_string(),
            "Settlement outputs must preserve minimum ERG box value to prevent dust attacks.".to_string(),
        ],
        source_code: r#"{
  // ProviderBox: inference provider state
  val providerPk = SELF.R7[SigmaProp].get
  val lastHeartbeat = OUTPUTS(0).R8[Int].get
  sigmaProp(
    providerPk || ( // heartbeat
      HEIGHT - lastHeartbeat < 1440 &&
      OUTPUTS(0).propositionBytes == SELF.propositionBytes
    )
  )
}"#.to_string(),
    }
}

fn make_provider_registry() -> ContractVisual {
    ContractVisual {
        name: "ProviderRegistry".to_string(),
        description: "Central registry contract managing provider onboarding, updates, and removal on the Xergon network.".to_string(),
        ergo_tree_hex: "0008cd03d1db08e4e50008cda54e0308cde4e50d1db084e0308cda54e03d1db08e4e50008cda5".to_string(),
        category: "Governance".to_string(),
        register_layout: vec![
            RegisterLayout {
                register: "R4".to_string(),
                sigma_type: "Coll[Byte]".to_string(),
                description: "Provider ID being registered or updated".to_string(),
                example_value: "Coll[Byte](0x7f, 0x3a, ...)".to_string(),
            },
            RegisterLayout {
                register: "R5".to_string(),
                sigma_type: "Coll[Byte]".to_string(),
                description: "ErgoTree hex of the corresponding ProviderBox".to_string(),
                example_value: "Coll[Byte](0x00, 0x08, ...)".to_string(),
            },
        ],
        spend_paths: vec![
            SpendPath {
                name: "register".to_string(),
                description: "Register a new provider with metadata and staking collateral.".to_string(),
                conditions: vec!["Admin or governance signature".to_string(), "Provider NFT not already registered".to_string(), "Minimum staking collateral present".to_string()],
                required_proofs: vec!["SigmaProp (admin/governance)".to_string()],
                estimated_gas: 35_000,
            },
            SpendPath {
                name: "update".to_string(),
                description: "Update provider endpoint, models, or metadata.".to_string(),
                conditions: vec!["Provider ID exists in registry".to_string(), "Valid governance vote or admin key".to_string()],
                required_proofs: vec!["SigmaProp (admin)".to_string()],
                estimated_gas: 20_000,
            },
            SpendPath {
                name: "remove".to_string(),
                description: "Remove a provider from the registry after deregistration.".to_string(),
                conditions: vec!["No active usage proofs reference this provider".to_string(), "Governance approval".to_string()],
                required_proofs: vec!["SigmaProp (governance)".to_string()],
                estimated_gas: 15_000,
            },
        ],
        token_requirements: vec![
            "Governance token (XGV) for voting on updates".to_string(),
            "Registry NFT to prevent spoofing".to_string(),
            "Min 100 XRG staking per registered provider".to_string(),
        ],
        security_notes: vec![
            "Registry updates require multi-sig governance to prevent unilateral changes.".to_string(),
            "Removed providers have a 72h grace period for settlement claims.".to_string(),
        ],
        source_code: r#"{
  // ProviderRegistry: governance-managed provider list
  val govToken = INPUTS(0).tokens(0)._1
  val adminPk = SELF.R4[SigmaProp].get
  val action = OUTPUTS(0).R5[Int].get // 0=register, 1=update, 2=remove
  sigmaProp(
    adminPk && govToken._1 == govTokenId
  )
}"#.to_string(),
    }
}

fn make_usage_proof() -> ContractVisual {
    ContractVisual {
        name: "UsageProof".to_string(),
        description: "Records proof of inference usage between a user and a provider, enabling trustless payment settlement.".to_string(),
        ergo_tree_hex: "0008cda54e03d1db08e4e50008cde4e50d1db084e0308cda54e03e4e50008cda5d1db08e4e5".to_string(),
        category: "Payment".to_string(),
        register_layout: vec![
            RegisterLayout {
                register: "R4".to_string(),
                sigma_type: "SigmaProp".to_string(),
                description: "User's public key committing to the usage".to_string(),
                example_value: "proveDlog(userPk)".to_string(),
            },
            RegisterLayout {
                register: "R5".to_string(),
                sigma_type: "Coll[Byte]".to_string(),
                description: "Provider ID that rendered the inference".to_string(),
                example_value: "Coll[Byte](0x4a, 0xbc, ...)".to_string(),
            },
            RegisterLayout {
                register: "R6".to_string(),
                sigma_type: "String".to_string(),
                description: "Model identifier used for inference".to_string(),
                example_value: "\"llama-70b\"".to_string(),
            },
            RegisterLayout {
                register: "R7".to_string(),
                sigma_type: "Int".to_string(),
                description: "Number of tokens or inference units consumed".to_string(),
                example_value: "4096".to_string(),
            },
            RegisterLayout {
                register: "R8".to_string(),
                sigma_type: "Long".to_string(),
                description: "Timestamp of the inference request".to_string(),
                example_value: "1712000000000L".to_string(),
            },
        ],
        spend_paths: vec![
            SpendPath {
                name: "submit".to_string(),
                description: "User submits a signed usage proof for settlement.".to_string(),
                conditions: vec!["User signature in R4".to_string(), "Valid provider ID in R5".to_string(), "Timestamp within 1h window".to_string()],
                required_proofs: vec!["SigmaProp (user key)".to_string()],
                estimated_gas: 18_000,
            },
            SpendPath {
                name: "settle".to_string(),
                description: "Provider claims payment after usage proof is confirmed.".to_string(),
                conditions: vec!["Proof matured (min 6 confirmations)".to_string(), "Provider matches R5".to_string()],
                required_proofs: vec!["SigmaProp (provider key)".to_string()],
                estimated_gas: 22_000,
            },
            SpendPath {
                name: "expire".to_string(),
                description: "Usage proof expires after timeout if not settled.".to_string(),
                conditions: vec!["HEIGHT > expiration height".to_string(), "Returns ERG to user".to_string()],
                required_proofs: vec![],
                estimated_gas: 10_000,
            },
        ],
        token_requirements: vec![
            "Payment token (XGP) for inference fees".to_string(),
            "Usage NFT to prevent replay attacks".to_string(),
        ],
        security_notes: vec![
            "Timestamp validation prevents stale proofs from being settled.".to_string(),
            "Usage NFT is burned on settlement to prevent double-spending.".to_string(),
            "6-block confirmation delay mitigates front-running on settlement claims.".to_string(),
        ],
        source_code: r#"{
  // UsageProof: trustless inference payment proof
  val userPk = SELF.R4[SigmaProp].get
  val providerId = SELF.R5[Coll[Byte]].get
  val ts = SELF.R8[Long].get
  val maturity = HEIGHT > SELF.creationInfo._1 + 6
  sigmaProp(
    userPk || (providerPk && maturity) || HEIGHT > ts + 720
  )
}"#.to_string(),
    }
}

fn make_staking_box() -> ContractVisual {
    ContractVisual {
        name: "StakingBox".to_string(),
        description: "Staking contract allowing users to lock XRG tokens and earn rewards for securing the Xergon inference network.".to_string(),
        ergo_tree_hex: "d1db08e4e50008cda54e0308cde4e50d1db084e0308cda54e03e4e50008cda5d1db08e4e50d1db".to_string(),
        category: "Staking".to_string(),
        register_layout: vec![
            RegisterLayout {
                register: "R4".to_string(),
                sigma_type: "SigmaProp".to_string(),
                description: "Staker's public key for withdrawal authorization".to_string(),
                example_value: "proveDlog(stakerPk)".to_string(),
            },
            RegisterLayout {
                register: "R5".to_string(),
                sigma_type: "Long".to_string(),
                description: "Current staked balance in nanoERG".to_string(),
                example_value: "10000000000L".to_string(),
            },
            RegisterLayout {
                register: "R6".to_string(),
                sigma_type: "Int".to_string(),
                description: "Block height when staking began (for reward calculation)".to_string(),
                example_value: "820000".to_string(),
            },
        ],
        spend_paths: vec![
            SpendPath {
                name: "deposit".to_string(),
                description: "Add more XRG tokens to the staking position.".to_string(),
                conditions: vec!["Staker signature required".to_string(), "Minimum additional deposit: 10 XRG".to_string()],
                required_proofs: vec!["SigmaProp (staker key)".to_string()],
                estimated_gas: 14_000,
            },
            SpendPath {
                name: "withdraw".to_string(),
                description: "Unstake tokens after lock period expires.".to_string(),
                conditions: vec!["Lock period elapsed (min 30 days / ~7200 blocks)".to_string(), "Staker signature".to_string()],
                required_proofs: vec!["SigmaProp (staker key)".to_string()],
                estimated_gas: 20_000,
            },
            SpendPath {
                name: "claim_rewards".to_string(),
                description: "Claim accumulated staking rewards without unstaking principal.".to_string(),
                conditions: vec!["Rewards available in attached box".to_string(), "Staker signature".to_string()],
                required_proofs: vec!["SigmaProp (staker key)".to_string()],
                estimated_gas: 16_000,
            },
        ],
        token_requirements: vec![
            "XRG token for staking".to_string(),
            "Staking NFT as position identifier".to_string(),
        ],
        security_notes: vec![
            "30-day minimum lock prevents short-term speculation and ensures network stability.".to_string(),
            "Rewards are calculated on-chain based on blocks staked, preventing oracle manipulation.".to_string(),
        ],
        source_code: r#"{
  // StakingBox: XRG staking with rewards
  val stakerPk = SELF.R4[SigmaProp].get
  val stakedAmount = SELF.R5[Long].get
  val startHeight = SELF.R6[Int].get
  val lockPeriod = 7200 // ~30 days
  sigmaProp(
    stakerPk && (
      // deposit: preserve proposition, increase balance
      (OUTPUTS(0).R5[Long].get >= stakedAmount && OUTPUTS(0).propositionBytes == SELF.propositionBytes) ||
      // withdraw: lock period must have elapsed
      (HEIGHT >= startHeight + lockPeriod) ||
      // claim rewards: separate reward box
      (OUTPUTS.size >= 2 && OUTPUTS(1).tokens(0)._1 == rewardTokenId)
    )
  )
}"#.to_string(),
    }
}

fn make_treasury_box() -> ContractVisual {
    ContractVisual {
        name: "TreasuryBox".to_string(),
        description: "Network treasury contract managing token allocations, airdrops, and community governance voting for Xergon funds.".to_string(),
        ergo_tree_hex: "08cde4e50d1db08e4e50008cda54e0308cda5d1db084e0308cde4e5008cda54e03e4e50d1db08".to_string(),
        category: "Treasury".to_string(),
        register_layout: vec![
            RegisterLayout {
                register: "R4".to_string(),
                sigma_type: "SigmaProp".to_string(),
                description: "Admin public key for treasury operations".to_string(),
                example_value: "proveDlog(adminPk)".to_string(),
            },
            RegisterLayout {
                register: "R5".to_string(),
                sigma_type: "Long".to_string(),
                description: "Total nanoERG allocated from treasury".to_string(),
                example_value: "50000000000000L".to_string(),
            },
            RegisterLayout {
                register: "R6".to_string(),
                sigma_type: "Int".to_string(),
                description: "Number of airdrops executed to date".to_string(),
                example_value: "12".to_string(),
            },
        ],
        spend_paths: vec![
            SpendPath {
                name: "airdrop".to_string(),
                description: "Distribute tokens to eligible network participants.".to_string(),
                conditions: vec!["Admin signature".to_string(), "Airdrop list in outputs".to_string(), "Per-address cap enforced".to_string()],
                required_proofs: vec!["SigmaProp (admin key)".to_string()],
                estimated_gas: 50_000,
            },
            SpendPath {
                name: "allocate".to_string(),
                description: "Allocate treasury funds to specific network programs or grants.".to_string(),
                conditions: vec!["Admin or governance multi-sig".to_string(), "Allocation within budget cap".to_string()],
                required_proofs: vec!["SigmaProp (admin key)".to_string()],
                estimated_gas: 30_000,
            },
            SpendPath {
                name: "governance_vote".to_string(),
                description: "Community governance vote on treasury spending proposals.".to_string(),
                conditions: vec!["Quorum reached".to_string(), "Vote weight proportional to XRG holdings".to_string()],
                required_proofs: vec!["SigmaProp (voter key)".to_string()],
                estimated_gas: 40_000,
            },
        ],
        token_requirements: vec![
            "Treasury NFT to identify the canonical treasury box".to_string(),
            "XGV governance token for voting rights".to_string(),
            "XRG token for airdrop distributions".to_string(),
            "Budget tracking token (internal)".to_string(),
        ],
        security_notes: vec![
            "Multi-sig governance ensures no single party controls treasury funds.".to_string(),
            "Airdrop caps prevent Sybil attacks from draining the treasury.".to_string(),
        ],
        source_code: r#"{
  // TreasuryBox: network treasury with governance
  val adminPk = SELF.R4[SigmaProp].get
  val totalAllocated = SELF.R5[Long].get
  val maxBudget = 100000000000000L // 100k ERG
  val action = OUTPUTS(0).R6[Int].get
  sigmaProp(
    adminPk && totalAllocated + OUTPUTS(0).value <= maxBudget
  )
}"#.to_string(),
    }
}

fn make_gpu_rental() -> ContractVisual {
    ContractVisual {
        name: "GpuRental".to_string(),
        description: "GPU rental contract enabling trustless leasing of compute resources between renters and providers on Xergon.".to_string(),
        ergo_tree_hex: "4e0308cde4e50008cda5d1db08e4e50008cda54e0308cde4e50d1db084e0308cda54e03d1db08".to_string(),
        category: "GPU".to_string(),
        register_layout: vec![
            RegisterLayout {
                register: "R4".to_string(),
                sigma_type: "SigmaProp".to_string(),
                description: "Renter's public key for the GPU lease".to_string(),
                example_value: "proveDlog(renterPk)".to_string(),
            },
            RegisterLayout {
                register: "R5".to_string(),
                sigma_type: "String".to_string(),
                description: "GPU hardware info and availability metadata".to_string(),
                example_value: "\"H100,80GB,PCIe\"".to_string(),
            },
            RegisterLayout {
                register: "R6".to_string(),
                sigma_type: "Int".to_string(),
                description: "Block height when the rental period ends".to_string(),
                example_value: "825000".to_string(),
            },
            RegisterLayout {
                register: "R7".to_string(),
                sigma_type: "Long".to_string(),
                description: "Price per hour in nanoERG".to_string(),
                example_value: "50000000L".to_string(),
            },
        ],
        spend_paths: vec![
            SpendPath {
                name: "rent".to_string(),
                description: "Initiate a GPU rental by locking payment collateral.".to_string(),
                conditions: vec!["Renter signature".to_string(), "Sufficient ERG locked for rental duration".to_string(), "GPU not currently rented".to_string()],
                required_proofs: vec!["SigmaProp (renter key)".to_string()],
                estimated_gas: 22_000,
            },
            SpendPath {
                name: "release".to_string(),
                description: "Release the GPU back to the provider after rental ends.".to_string(),
                conditions: vec!["Rental period expired (HEIGHT >= R6)".to_string(), "Remaining collateral returned to renter".to_string()],
                required_proofs: vec!["SigmaProp (renter key or auto-release)".to_string()],
                estimated_gas: 15_000,
            },
            SpendPath {
                name: "dispute".to_string(),
                description: "Open a dispute if GPU was unavailable or underperforming.".to_string(),
                conditions: vec!["Evidence box attached with logs".to_string(), "Within dispute window (48 blocks)".to_string()],
                required_proofs: vec!["SigmaProp (renter key)".to_string(), "Dispute evidence hash".to_string()],
                estimated_gas: 35_000,
            },
        ],
        token_requirements: vec![
            "GPU NFT identifying the hardware unit".to_string(),
            "XRG payment token for rental fees".to_string(),
        ],
        security_notes: vec![
            "Escrow mechanism ensures payment is only released if GPU availability is confirmed.".to_string(),
            "Dispute window of 48 blocks (~4h) prevents stale claims from being adjudicated.".to_string(),
        ],
        source_code: r#"{
  // GpuRental: trustless GPU compute leasing
  val renterPk = SELF.R4[SigmaProp].get
  val rentalEnd = SELF.R6[Int].get
  val pricePerHour = SELF.R7[Long].get
  val hoursRented = SELF.R5[Int].get
  sigmaProp(
    renterPk && (
      // rent: lock collateral for rental
      OUTPUTS(0).value >= pricePerHour * hoursRented ||
      // release: rental period over, return collateral
      HEIGHT >= rentalEnd ||
      // dispute: within dispute window
      (HEIGHT < rentalEnd + 48 && CONTEXT.dataInputs(0).R4[Coll[Byte]].isDefined)
    )
  )
}"#.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Router & handlers
// ---------------------------------------------------------------------------

pub fn build_router() -> Router {
    let state = Arc::new(ContractExplorerState::new());
    Router::new()
        .route("/v1/contracts/explore", get(browse_handler))
        .route("/v1/contracts/explore/{name}", get(get_contract_handler))
        .route("/v1/contracts/inspect", post(inspect_handler))
        .route("/v1/contracts/compare", post(compare_handler))
        .route("/v1/contracts/playground/evaluate", post(playground_handler))
        .route("/v1/contracts/metrics", get(metrics_handler))
        .route("/v1/contracts/categories", get(categories_handler))
        .with_state(state)
}

async fn browse_handler(
    State(state): State<Arc<ContractExplorerState>>,
    Query(params): Query<BrowseQuery>,
) -> Json<Vec<ContractVisual>> {
    let results = state.browse_library(
        params.category.as_deref(),
        params.search.as_deref(),
    );
    Json(results)
}

async fn get_contract_handler(
    State(state): State<Arc<ContractExplorerState>>,
    Path(name): Path<String>,
) -> Json<serde_json::Value> {
    match state.get_contract(&name) {
        Some(contract) => {
            state.record_view(&name);
            Json(serde_json::to_value(contract).unwrap_or_default())
        }
        None => Json(serde_json::json!({
            "error": format!("Contract '{}' not found", name)
        })),
    }
}

async fn inspect_handler(
    State(state): State<Arc<ContractExplorerState>>,
    Json(body): Json<InspectRequest>,
) -> Json<InspectResult> {
    let result = state.inspect_ergotree(&body.hex);
    Json(result)
}

async fn compare_handler(
    State(state): State<Arc<ContractExplorerState>>,
    Json(body): Json<CompareRequest>,
) -> Json<CompareResult> {
    let result = state.compare_contracts(&body.hex1, &body.hex2);
    Json(result)
}

async fn playground_handler(
    State(state): State<Arc<ContractExplorerState>>,
    Json(body): Json<PlaygroundRequest>,
) -> Json<PlaygroundEvalResult> {
    let result = state.playground_evaluate(&body.hex, &body.inputs);
    Json(result)
}

async fn metrics_handler(
    State(state): State<Arc<ContractExplorerState>>,
) -> Json<ContractMetrics> {
    let metrics = state.get_metrics();
    Json(metrics)
}

async fn categories_handler(
    State(state): State<Arc<ContractExplorerState>>,
) -> Json<Vec<String>> {
    let categories = state.get_categories();
    Json(categories)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> Arc<ContractExplorerState> {
        Arc::new(ContractExplorerState::new())
    }

    #[test]
    fn test_library_loaded() {
        let state = make_state();
        assert_eq!(state.library.len(), 6);

        let names: Vec<String> = state.library.iter().map(|e| e.key().clone()).collect();
        assert!(names.contains(&"ProviderBox".to_string()));
        assert!(names.contains(&"ProviderRegistry".to_string()));
        assert!(names.contains(&"UsageProof".to_string()));
        assert!(names.contains(&"StakingBox".to_string()));
        assert!(names.contains(&"TreasuryBox".to_string()));
        assert!(names.contains(&"GpuRental".to_string()));
    }

    #[test]
    fn test_browse_all() {
        let state = make_state();
        let results = state.browse_library(None, None);
        assert_eq!(results.len(), 6);
    }

    #[test]
    fn test_browse_by_category() {
        let state = make_state();
        let results = state.browse_library(Some("Inference"), None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "ProviderBox");
    }

    #[test]
    fn test_browse_by_search() {
        let state = make_state();
        let results = state.browse_library(None, Some("provider"));
        // "ProviderBox" and "ProviderRegistry" both contain "provider"
        assert!(results.len() >= 2);
        let names: Vec<&str> = results.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"ProviderBox"));
        assert!(names.contains(&"ProviderRegistry"));
    }

    #[test]
    fn test_get_contract_found() {
        let state = make_state();
        let contract = state.get_contract("StakingBox");
        assert!(contract.is_some());
        let c = contract.unwrap();
        assert_eq!(c.name, "StakingBox");
        assert_eq!(c.category, "Staking");
        assert_eq!(c.spend_paths.len(), 3);
        assert!(!c.source_code.is_empty());
    }

    #[test]
    fn test_get_contract_not_found() {
        let state = make_state();
        let contract = state.get_contract("NonExistent");
        assert!(contract.is_none());
    }

    #[test]
    fn test_inspect_ergotree() {
        let state = make_state();
        let hex = "0008cde4e50008cda54e03d1db08e4e50008cda54e0308cde4e50d1db084e03";
        let result = state.inspect_ergotree(hex);
        assert!(result.opcodes.len() >= 3); // version, flags, constants
        assert!(result.estimated_size > 0);
        assert!(result.opcodes.iter().any(|op| op.starts_with("VERSION_")));
    }

    #[test]
    fn test_compare_contracts() {
        let state = make_state();
        let hex1 = "aabbccddeeff00112233445566778899";
        let hex2 = "aabbccddeeff00112233445566778800";
        let result = state.compare_contracts(hex1, hex2);
        // They share many byte pairs so common should be non-empty
        assert!(!result.common.is_empty());
        assert!(result.size_diff == 0); // same length
    }

    #[test]
    fn test_playground_evaluate_valid() {
        let state = make_state();
        // Version byte 0x00 => valid
        let hex = "0008cde4e500";
        let inputs = r#"{"amount": 100}"#;
        let result = state.playground_evaluate(hex, inputs);
        assert!(result.valid);
        assert!(result.error.is_none());
        assert!(result.gas_used > 0);
    }

    #[test]
    fn test_playground_evaluate_invalid() {
        let state = make_state();
        // Version byte 0xff => invalid
        let hex = "ff08cde4e500";
        let inputs = r#"{"amount": 100}"#;
        let result = state.playground_evaluate(hex, inputs);
        assert!(!result.valid);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_playground_evaluate_bad_json() {
        let state = make_state();
        let hex = "0008cde4e500";
        let inputs = "not json";
        let result = state.playground_evaluate(hex, inputs);
        assert!(!result.valid);
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("Invalid JSON"));
    }

    #[test]
    fn test_metrics() {
        let state = make_state();
        let metrics = state.get_metrics();
        assert_eq!(metrics.total_contracts, 6);
        assert_eq!(metrics.total_inspections, 0);
        assert_eq!(metrics.total_comparisons, 0);
        assert_eq!(metrics.total_playground_evals, 0);
    }

    #[test]
    fn test_categories() {
        let state = make_state();
        let cats = state.get_categories();
        assert!(cats.contains(&"Inference".to_string()));
        assert!(cats.contains(&"Governance".to_string()));
        assert!(cats.contains(&"Payment".to_string()));
        assert!(cats.contains(&"Staking".to_string()));
        assert!(cats.contains(&"Treasury".to_string()));
        assert!(cats.contains(&"GPU".to_string()));
        assert_eq!(cats.len(), 6);
    }

    #[test]
    fn test_record_view() {
        let state = make_state();

        // Check initial view count is 0
        let initial = state
            .contract_views
            .get("ProviderBox")
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0);
        assert_eq!(initial, 0);

        // Record a view
        state.record_view("ProviderBox");

        // Check it incremented
        let after = state
            .contract_views
            .get("ProviderBox")
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0);
        assert_eq!(after, 1);

        // Non-existent contract should not panic
        state.record_view("DoesNotExist");
    }

    #[test]
    fn test_browse_category_and_search_combined() {
        let state = make_state();
        // Search for "staking" with category "Staking" should find StakingBox
        let results = state.browse_library(Some("Staking"), Some("staking"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "StakingBox");

        // Search for "staking" with category "GPU" should find nothing
        let results2 = state.browse_library(Some("GPU"), Some("staking"));
        assert!(results2.is_empty());
    }

    #[test]
    fn test_contract_details_integrity() {
        let state = make_state();

        // Verify each contract has expected fields populated
        let all_contracts: Vec<ContractVisual> = state
            .library
            .iter()
            .map(|e| e.value().clone())
            .collect();

        for contract in &all_contracts {
            assert!(!contract.name.is_empty());
            assert!(!contract.description.is_empty());
            assert!(!contract.ergo_tree_hex.is_empty());
            assert!(!contract.category.is_empty());
            assert!(!contract.register_layout.is_empty());
            assert!(!contract.spend_paths.is_empty());
            assert!(!contract.token_requirements.is_empty());
            assert!(!contract.security_notes.is_empty());
            assert!(!contract.source_code.is_empty());

            // Verify hex is at least 64 chars
            assert!(
                contract.ergo_tree_hex.len() >= 64,
                "Contract {} hex too short: {}",
                contract.name,
                contract.ergo_tree_hex.len()
            );

            // Verify security notes has at least 2 entries
            assert!(
                contract.security_notes.len() >= 2,
                "Contract {} should have at least 2 security notes",
                contract.name
            );

            // Verify token requirements has at least 2 entries
            assert!(
                contract.token_requirements.len() >= 2,
                "Contract {} should have at least 2 token requirements",
                contract.name
            );
        }
    }

    #[test]
    fn test_metrics_after_operations() {
        let state = make_state();

        // Perform some operations
        state.inspect_ergotree("0008cde4e500");
        state.inspect_ergotree("0008cde4e501");
        state.compare_contracts("aabbcc", "ddeeff");
        state.playground_evaluate("0008cde4e500", "{}");

        let metrics = state.get_metrics();
        assert_eq!(metrics.total_inspections, 2);
        assert_eq!(metrics.total_comparisons, 1);
        assert_eq!(metrics.total_playground_evals, 1);
    }
}
