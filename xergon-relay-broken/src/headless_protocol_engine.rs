//! Headless Protocol Engine
//!
//! Implements the Ergo headless dApp pattern for Xergon protocol contracts.
//! Provides 5 layers:
//!   1. Box Specifications — declarative contract box schemas
//!   2. Wrapped Boxes — validation of raw boxes against specs
//!   3. Protocol Equations — pure math for tokenomics
//!   4. Action Builders — unsigned transaction construction
//!   5. Box Finder — UTXO scanning and discovery

use axum::{
    extract::{Path, State},
    Json,
    Router,
    routing::{get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use crate::proxy;

// ================================================================
// Layer 1 — Box Specifications
// ================================================================

/// Token constraint inside a box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpec {
    pub token_id: String,
    pub min_amount: u64,
    pub max_amount: Option<u64>,
}

/// Register constraint inside a box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSpec {
    pub register: String,          // "R4".."R9"
    pub expected_type: String,     // "SByteString", "SLong", "SColl"
    pub purpose: String,
    pub required: bool,
}

/// Full box specification for a contract type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxSpec {
    pub name: String,
    pub ergo_tree: String,
    pub required_tokens: Vec<TokenSpec>,
    pub register_specs: HashMap<String, RegisterSpec>,
    pub min_value: u64,
}

/// Token held in a raw box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxToken {
    pub token_id: String,
    pub amount: u64,
}

/// Raw box representation (chain data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub tokens: Vec<BoxToken>,
    pub registers: HashMap<String, String>,
    pub creation_height: u32,
}

// ---- Built-in BoxSpecs ----

fn provider_box_spec() -> BoxSpec {
    let mut register_specs = HashMap::new();
    register_specs.insert("R4".into(), RegisterSpec {
        register: "R4".into(), expected_type: "SByteString".into(),
        purpose: "provider_id".into(), required: true,
    });
    register_specs.insert("R5".into(), RegisterSpec {
        register: "R5".into(), expected_type: "SByteString".into(),
        purpose: "endpoint".into(), required: true,
    });
    register_specs.insert("R6".into(), RegisterSpec {
        register: "R6".into(), expected_type: "SColl(SByteString)".into(),
        purpose: "models".into(), required: true,
    });
    register_specs.insert("R7".into(), RegisterSpec {
        register: "R7".into(), expected_type: "SByteString".into(),
        purpose: "address".into(), required: true,
    });
    register_specs.insert("R8".into(), RegisterSpec {
        register: "R8".into(), expected_type: "SInt".into(),
        purpose: "pown_score".into(), required: true,
    });
    BoxSpec {
        name: "ProviderBox".into(),
        ergo_tree: "100204020004000500060007000800".into(),
        required_tokens: vec![TokenSpec {
            token_id: "PROVIDER_NFT".into(), min_amount: 1, max_amount: Some(1),
        }],
        register_specs,
        min_value: 1_000_000_000, // 1 ERG
    }
}

fn provider_registry_spec() -> BoxSpec {
    let mut register_specs = HashMap::new();
    register_specs.insert("R4".into(), RegisterSpec {
        register: "R4".into(), expected_type: "SByteString".into(),
        purpose: "provider_id".into(), required: true,
    });
    register_specs.insert("R5".into(), RegisterSpec {
        register: "R5".into(), expected_type: "SByteString".into(),
        purpose: "ergo_tree".into(), required: true,
    });
    BoxSpec {
        name: "ProviderRegistry".into(),
        ergo_tree: "100204020004000500".into(),
        required_tokens: vec![TokenSpec {
            token_id: "REGISTRY_NFT".into(), min_amount: 1, max_amount: Some(1),
        }],
        register_specs,
        min_value: 100_000_000,
    }
}

fn usage_proof_spec() -> BoxSpec {
    let mut register_specs = HashMap::new();
    register_specs.insert("R4".into(), RegisterSpec {
        register: "R4".into(), expected_type: "SByteString".into(),
        purpose: "user_pk".into(), required: true,
    });
    register_specs.insert("R5".into(), RegisterSpec {
        register: "R5".into(), expected_type: "SByteString".into(),
        purpose: "provider_id".into(), required: true,
    });
    register_specs.insert("R6".into(), RegisterSpec {
        register: "R6".into(), expected_type: "SByteString".into(),
        purpose: "model".into(), required: true,
    });
    register_specs.insert("R7".into(), RegisterSpec {
        register: "R7".into(), expected_type: "SLong".into(),
        purpose: "token_count".into(), required: true,
    });
    register_specs.insert("R8".into(), RegisterSpec {
        register: "R8".into(), expected_type: "SLong".into(),
        purpose: "timestamp".into(), required: true,
    });
    BoxSpec {
        name: "UsageProof".into(),
        ergo_tree: "100204020004000500060007000800".into(),
        required_tokens: vec![],
        register_specs,
        min_value: 10_000_000,
    }
}

fn staking_box_spec() -> BoxSpec {
    let mut register_specs = HashMap::new();
    register_specs.insert("R4".into(), RegisterSpec {
        register: "R4".into(), expected_type: "SByteString".into(),
        purpose: "user_pk".into(), required: true,
    });
    register_specs.insert("R5".into(), RegisterSpec {
        register: "R5".into(), expected_type: "SLong".into(),
        purpose: "balance_nanoerg".into(), required: true,
    });
    register_specs.insert("R6".into(), RegisterSpec {
        register: "R6".into(), expected_type: "SLong".into(),
        purpose: "created_at".into(), required: true,
    });
    BoxSpec {
        name: "StakingBox".into(),
        ergo_tree: "1002040200040005000600".into(),
        required_tokens: vec![TokenSpec {
            token_id: "STAKING_TOKEN".into(), min_amount: 1, max_amount: None,
        }],
        register_specs,
        min_value: 1_000_000_000,
    }
}

fn treasury_box_spec() -> BoxSpec {
    let mut register_specs = HashMap::new();
    register_specs.insert("R4".into(), RegisterSpec {
        register: "R4".into(), expected_type: "SByteString".into(),
        purpose: "admin_pk".into(), required: true,
    });
    register_specs.insert("R5".into(), RegisterSpec {
        register: "R5".into(), expected_type: "SLong".into(),
        purpose: "total_allocated".into(), required: true,
    });
    register_specs.insert("R6".into(), RegisterSpec {
        register: "R6".into(), expected_type: "SLong".into(),
        purpose: "airdrop_count".into(), required: true,
    });
    BoxSpec {
        name: "TreasuryBox".into(),
        ergo_tree: "1002040200040005000600".into(),
        required_tokens: vec![TokenSpec {
            token_id: "TREASURY_NFT".into(), min_amount: 1, max_amount: Some(1),
        }],
        register_specs,
        min_value: 10_000_000_000,
    }
}

fn gpu_rental_spec() -> BoxSpec {
    let mut register_specs = HashMap::new();
    register_specs.insert("R4".into(), RegisterSpec {
        register: "R4".into(), expected_type: "SByteString".into(),
        purpose: "renter_pk".into(), required: true,
    });
    register_specs.insert("R5".into(), RegisterSpec {
        register: "R5".into(), expected_type: "SByteString".into(),
        purpose: "gpu_info".into(), required: true,
    });
    register_specs.insert("R6".into(), RegisterSpec {
        register: "R6".into(), expected_type: "SLong".into(),
        purpose: "rental_end".into(), required: true,
    });
    register_specs.insert("R7".into(), RegisterSpec {
        register: "R7".into(), expected_type: "SLong".into(),
        purpose: "price_per_hour".into(), required: true,
    });
    BoxSpec {
        name: "GpuRental".into(),
        ergo_tree: "10020402000400050006000700".into(),
        required_tokens: vec![TokenSpec {
            token_id: "GPU_NFT".into(), min_amount: 1, max_amount: Some(1),
        }],
        register_specs,
        min_value: 500_000_000,
    }
}

/// Returns all 6 built-in BoxSpecs
pub fn all_box_specs() -> Vec<BoxSpec> {
    vec![
        provider_box_spec(),
        provider_registry_spec(),
        usage_proof_spec(),
        staking_box_spec(),
        treasury_box_spec(),
        gpu_rental_spec(),
    ]
}

/// Get a named BoxSpec by name
pub fn get_box_spec(name: &str) -> Option<BoxSpec> {
    all_box_specs().into_iter().find(|s| s.name == name)
}

// ================================================================
// Layer 2 — Wrapped Boxes / Validation
// ================================================================

/// Result of validating a box against a spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub register_values: HashMap<String, String>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self { valid: true, errors: vec![], warnings: vec![], register_values: HashMap::new() }
    }

    pub fn invalid(errors: Vec<String>) -> Self {
        Self { valid: false, errors, warnings: vec![], register_values: HashMap::new() }
    }
}

/// Validate a raw box against a box specification
pub fn validate_box(box_: &RawBox, spec: &BoxSpec) -> ValidationResult {
    let mut errors: Vec<String> = vec![];
    let mut warnings: Vec<String> = vec![];
    let mut register_values: HashMap<String, String> = HashMap::new();

    // Check minimum value
    if box_.value < spec.min_value {
        errors.push(format!(
            "Box value {} nanoERG is below minimum {}",
            box_.value, spec.min_value
        ));
    }

    // Check ErgoTree pattern match (prefix or exact)
    if !box_.ergo_tree.starts_with(&spec.ergo_tree) {
        errors.push(format!(
            "ErgoTree mismatch: box starts with {} but spec expects prefix {}",
            &box_.ergo_tree[..box_.ergo_tree.len().min(16)],
            &spec.ergo_tree[..spec.ergo_tree.len().min(16)],
        ));
    }

    // Check required tokens
    for ts in &spec.required_tokens {
        let found = box_.tokens.iter().find(|t| t.token_id == ts.token_id);
        match found {
            None => errors.push(format!("Missing required token: {}", ts.token_id)),
            Some(t) => {
                if t.amount < ts.min_amount {
                    errors.push(format!(
                        "Token {} amount {} below minimum {}",
                        ts.token_id, t.amount, ts.min_amount
                    ));
                }
                if let Some(max) = ts.max_amount {
                    if t.amount > max {
                        errors.push(format!(
                            "Token {} amount {} exceeds maximum {}",
                            ts.token_id, t.amount, max
                        ));
                    }
                }
            }
        }
    }

    // Check registers
    for (reg_name, reg_spec) in &spec.register_specs {
        match box_.registers.get(reg_name) {
            None => {
                if reg_spec.required {
                    errors.push(format!("Missing required register {}", reg_name));
                }
            }
            Some(val) => {
                register_values.insert(reg_name.clone(), val.clone());
                // Type check — we do a simple heuristic length check
                let type_ok = match reg_spec.expected_type.as_str() {
                    "SLong" => val.parse::<i64>().is_ok(),
                    "SInt" => val.parse::<i32>().is_ok(),
                    "SByteString" => !val.is_empty(),
                    "SColl(SByteString)" => val.starts_with('[') || !val.is_empty(),
                    _ => true, // unknown types are not checked
                };
                if !type_ok {
                    warnings.push(format!(
                        "Register {} value may not match expected type {}",
                        reg_name, reg_spec.expected_type
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        ValidationResult { valid: true, errors, warnings, register_values }
    } else {
        ValidationResult { valid: false, errors, warnings, register_values }
    }
}

// ================================================================
// Layer 3 — Protocol Equations (pure math, no I/O)
// ================================================================

/// Calculate PoNW (Proof of Neural Work) score for a provider node.
/// Combines computational work, network contribution, and AI work.
pub fn calculate_pown_score(node_work: f64, network_work: f64, ai_work: f64) -> f64 {
    if node_work + network_work + ai_work <= 0.0 {
        return 0.0;
    }
    // Weighted geometric mean: 40% node, 30% network, 30% AI
    let node_component = node_work * 0.4;
    let network_component = network_work * 0.3;
    let ai_component = ai_work * 0.3;
    (node_component + network_component + ai_component).sqrt() * 1000.0
}

/// Calculate staking yield in nanoERG per epoch.
/// yield = staked_amount * (emission_rate / total_staked)
pub fn calculate_staking_yield(staked_amount: u64, total_staked: u64, emission_rate: u64) -> f64 {
    if total_staked == 0 {
        return 0.0;
    }
    let share = staked_amount as f64 / total_staked as f64;
    share * emission_rate as f64
}

/// Calculate GPU rental cost in nanoERG.
pub fn calculate_rent_cost(gpu_type: &str, hours: u32, price_per_hour: u64) -> u64 {
    let multiplier = match gpu_type {
        "H100" => 2.0_f64,
        "A100" => 1.5_f64,
        "A6000" => 1.0_f64,
        "RTX4090" => 0.8_f64,
        _ => 1.0_f64,
    };
    (hours as f64 * price_per_hour as f64 * multiplier) as u64
}

/// Calculate inference cost in nanoERG.
pub fn calculate_inference_cost(model: &str, tokens: u32, provider_tier: &str) -> u64 {
    let base_cost_per_1k: u64 = match model {
        "llama-3.1-70b" => 50_000,
        "llama-3.1-8b" => 15_000,
        "mixtral-8x7b" => 40_000,
        "codellama-34b" => 45_000,
        _ => 30_000,
    };
    let tier_multiplier = match provider_tier {
        "premium" => 1.2_f64,
        "standard" => 1.0_f64,
        "budget" => 0.7_f64,
        _ => 1.0_f64,
    };
    let token_cost = (tokens as f64 / 1000.0) * base_cost_per_1k as f64;
    (token_cost * tier_multiplier) as u64
}

/// Calculate the interval (in blocks) since the last heartbeat.
/// Returns 0 if heartbeat is current or invalid.
pub fn calculate_heartbeat_interval(last_heartbeat: u32, current_height: u32) -> u32 {
    if current_height > last_heartbeat {
        current_height - last_heartbeat
    } else {
        0
    }
}

/// Validate that output token balance is conserved (no minting/burning).
pub fn validate_token_balance(input_tokens: &[(String, u64)], output_tokens: &[(String, u64)]) -> bool {
    let mut in_map: HashMap<&str, u64> = HashMap::new();
    let mut out_map: HashMap<&str, u64> = HashMap::new();

    for (id, amt) in input_tokens {
        *in_map.entry(id.as_str()).or_insert(0) += amt;
    }
    for (id, amt) in output_tokens {
        *out_map.entry(id.as_str()).or_insert(0) += amt;
    }

    // Every output token must exist in inputs with >= amount
    for (id, amt) in &out_map {
        let in_amt = in_map.get(*id).copied().unwrap_or(0);
        if *amt > in_amt {
            return false;
        }
    }
    true
}

/// Calculate minimum box value in nanoERG based on box contents.
/// Uses the Ergo min-box-value formula: (ergo_tree_size + num_tokens * 300 + register_size) * 360
pub fn calculate_min_box_value(ergo_tree_size: usize, num_tokens: usize, register_size: usize) -> u64 {
    let total_bytes = ergo_tree_size + num_tokens * 300 + register_size;
    (total_bytes as u64) * 360
}

/// List all available equation names
pub fn list_equations() -> Vec<&'static str> {
    vec![
        "calculate_pown_score",
        "calculate_staking_yield",
        "calculate_rent_cost",
        "calculate_inference_cost",
        "calculate_heartbeat_interval",
        "validate_token_balance",
        "calculate_min_box_value",
    ]
}

// ================================================================
// Layer 4 — Action Builders (unsigned transaction construction)
// ================================================================

/// Input reference for an unsigned transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub box_id: String,
    pub extension: HashMap<String, String>,
}

/// Output for an unsigned transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub value: u64,
    pub ergo_tree: String,
    pub tokens: Vec<BoxToken>,
    pub registers: HashMap<String, String>,
}

/// Unsigned transaction (to be signed by client wallet)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedTx {
    pub inputs: Vec<TxInput>,
    pub data_inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub fee: u64,
    pub creation_height: u32,
}

/// Build a provider registration transaction
pub fn build_provider_registration(
    provider_id: &str,
    endpoint: &str,
    models: &str,
    address: &str,
) -> UnsignedTx {
    let mut registers = HashMap::new();
    registers.insert("R4".into(), provider_id.into());
    registers.insert("R5".into(), endpoint.into());
    registers.insert("R6".into(), models.into());
    registers.insert("R7".into(), address.into());
    registers.insert("R8".into(), "0".into());

    UnsignedTx {
        inputs: vec![TxInput {
            box_id: "CHANGE_BOX_ID".into(),
            extension: HashMap::new(),
        }],
        data_inputs: vec![],
        outputs: vec![TxOutput {
            value: 1_000_000_000,
            ergo_tree: provider_box_spec().ergo_tree,
            tokens: vec![BoxToken {
                token_id: "PROVIDER_NFT".into(), amount: 1,
            }],
            registers,
        }],
        fee: 1_100_000,
        creation_height: 800_000,
    }
}

/// Build a heartbeat update transaction
pub fn build_heartbeat_update(provider_box: &RawBox, new_pown: i32) -> UnsignedTx {
    let mut registers = HashMap::new();
    registers.insert("R4".into(), provider_box.registers.get("R4").cloned().unwrap_or_default());
    registers.insert("R5".into(), provider_box.registers.get("R5").cloned().unwrap_or_default());
    registers.insert("R6".into(), provider_box.registers.get("R6").cloned().unwrap_or_default());
    registers.insert("R7".into(), provider_box.registers.get("R7").cloned().unwrap_or_default());
    registers.insert("R8".into(), new_pown.to_string());

    UnsignedTx {
        inputs: vec![TxInput {
            box_id: provider_box.box_id.clone(),
            extension: HashMap::new(),
        }],
        data_inputs: vec![],
        outputs: vec![TxOutput {
            value: provider_box.value,
            ergo_tree: provider_box.ergo_tree.clone(),
            tokens: provider_box.tokens.clone(),
            registers,
        }],
        fee: 1_100_000,
        creation_height: 800_000,
    }
}

/// Build a usage proof transaction
pub fn build_usage_proof(
    user_pk: &str,
    provider_id: &str,
    model: &str,
    tokens: u64,
) -> UnsignedTx {
    let mut registers = HashMap::new();
    registers.insert("R4".into(), user_pk.into());
    registers.insert("R5".into(), provider_id.into());
    registers.insert("R6".into(), model.into());
    registers.insert("R7".into(), tokens.to_string());
    registers.insert("R8".into(), chrono::Utc::now().timestamp().to_string());

    UnsignedTx {
        inputs: vec![TxInput {
            box_id: "USER_INPUT_BOX".into(),
            extension: HashMap::new(),
        }],
        data_inputs: vec![],
        outputs: vec![TxOutput {
            value: 10_000_000,
            ergo_tree: usage_proof_spec().ergo_tree,
            tokens: vec![],
            registers,
        }],
        fee: 1_100_000,
        creation_height: 800_000,
    }
}

/// Build a stake deposit transaction
pub fn build_stake_deposit(user_pk: &str, amount_nanoerg: u64) -> UnsignedTx {
    let mut registers = HashMap::new();
    registers.insert("R4".into(), user_pk.into());
    registers.insert("R5".into(), amount_nanoerg.to_string());
    registers.insert("R6".into(), chrono::Utc::now().timestamp().to_string());

    UnsignedTx {
        inputs: vec![TxInput {
            box_id: "DEPOSIT_INPUT_BOX".into(),
            extension: HashMap::new(),
        }],
        data_inputs: vec![],
        outputs: vec![TxOutput {
            value: amount_nanoerg,
            ergo_tree: staking_box_spec().ergo_tree,
            tokens: vec![BoxToken {
                token_id: "STAKING_TOKEN".into(), amount: 1,
            }],
            registers,
        }],
        fee: 1_100_000,
        creation_height: 800_000,
    }
}

/// Build a stake withdrawal transaction
pub fn build_stake_withdraw(staking_box: &RawBox, amount: u64) -> UnsignedTx {
    let remaining = staking_box.value.saturating_sub(amount);

    let mut outputs = vec![];
    if remaining > calculate_min_box_value(100, 1, 64) {
        let mut registers = HashMap::new();
        registers.insert("R4".into(), staking_box.registers.get("R4").cloned().unwrap_or_default());
        registers.insert("R5".into(), remaining.to_string());
        registers.insert("R6".into(), staking_box.registers.get("R6").cloned().unwrap_or_default());

        outputs.push(TxOutput {
            value: remaining,
            ergo_tree: staking_box.ergo_tree.clone(),
            tokens: staking_box.tokens.clone(),
            registers,
        });
    }

    // User withdrawal output
    outputs.push(TxOutput {
        value: amount,
        ergo_tree: "P2PK_USER_SCRIPT".into(),
        tokens: vec![],
        registers: HashMap::new(),
    });

    UnsignedTx {
        inputs: vec![TxInput {
            box_id: staking_box.box_id.clone(),
            extension: HashMap::new(),
        }],
        data_inputs: vec![],
        outputs,
        fee: 1_100_000,
        creation_height: 800_000,
    }
}

/// Build an airdrop transaction from treasury
pub fn build_airdrop(treasury_box: &RawBox, recipient: &str, amount: u64) -> UnsignedTx {
    let current_allocated: u64 = treasury_box
        .registers
        .get("R5")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let current_count: u64 = treasury_box
        .registers
        .get("R6")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut registers = HashMap::new();
    registers.insert("R4".into(), treasury_box.registers.get("R4").cloned().unwrap_or_default());
    registers.insert("R5".into(), (current_allocated + amount).to_string());
    registers.insert("R6".into(), (current_count + 1).to_string());

    UnsignedTx {
        inputs: vec![TxInput {
            box_id: treasury_box.box_id.clone(),
            extension: HashMap::new(),
        }],
        data_inputs: vec![],
        outputs: vec![
            TxOutput {
                value: treasury_box.value.saturating_sub(amount),
                ergo_tree: treasury_box.ergo_tree.clone(),
                tokens: treasury_box.tokens.clone(),
                registers,
            },
            TxOutput {
                value: amount,
                ergo_tree: recipient.into(),
                tokens: vec![],
                registers: HashMap::new(),
            },
        ],
        fee: 1_100_000,
        creation_height: 800_000,
    }
}

/// Build a settlement transaction (user pays provider)
pub fn build_settlement(
    user_box: &RawBox,
    provider_box: &RawBox,
    amount: u64,
    fee: u64,
) -> UnsignedTx {
    let user_remaining = user_box.value.saturating_sub(amount + fee);

    let mut outputs = vec![];

    // Provider payment output
    let mut prov_registers = HashMap::new();
    prov_registers.insert("R4".into(), provider_box.registers.get("R4").cloned().unwrap_or_default());
    prov_registers.insert("R5".into(), provider_box.registers.get("R5").cloned().unwrap_or_default());
    prov_registers.insert("R6".into(), provider_box.registers.get("R6").cloned().unwrap_or_default());
    prov_registers.insert("R7".into(), provider_box.registers.get("R7").cloned().unwrap_or_default());
    prov_registers.insert("R8".into(), provider_box.registers.get("R8").cloned().unwrap_or_default());

    outputs.push(TxOutput {
        value: provider_box.value + amount,
        ergo_tree: provider_box.ergo_tree.clone(),
        tokens: provider_box.tokens.clone(),
        registers: prov_registers,
    });

    // User change output
    if user_remaining > calculate_min_box_value(100, 0, 0) {
        outputs.push(TxOutput {
            value: user_remaining,
            ergo_tree: user_box.ergo_tree.clone(),
            tokens: user_box.tokens.clone(),
            registers: user_box.registers.clone(),
        });
    }

    UnsignedTx {
        inputs: vec![
            TxInput { box_id: user_box.box_id.clone(), extension: HashMap::new() },
            TxInput { box_id: provider_box.box_id.clone(), extension: HashMap::new() },
        ],
        data_inputs: vec![],
        outputs,
        fee,
        creation_height: 800_000,
    }
}

// ================================================================
// Layer 5 — Box Finder
// ================================================================

/// A box found matching a spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxMatch {
    pub box_id: String,
    pub spec_name: String,
    pub value: u64,
    pub tokens: Vec<BoxToken>,
    pub registers: HashMap<String, String>,
    pub validation: ValidationResult,
}

/// Protocol engine state (thread-safe)
pub struct HeadlessProtocolState {
    /// In-memory box cache (box_id -> RawBox)
    box_cache: DashMap<String, RawBox>,
    /// Stats counters
    validations_total: AtomicU64,
    validations_passed: AtomicU64,
    equations_calculated: AtomicU64,
    actions_built: AtomicU64,
    boxes_scanned: AtomicU64,
}

impl HeadlessProtocolState {
    pub fn new() -> Self {
        Self {
            box_cache: DashMap::new(),
            validations_total: AtomicU64::new(0),
            validations_passed: AtomicU64::new(0),
            equations_calculated: AtomicU64::new(0),
            actions_built: AtomicU64::new(0),
            boxes_scanned: AtomicU64::new(0),
        }
    }

    /// Find boxes matching a spec name from the cache
    pub fn find_boxes(&self, spec_name: &str) -> Vec<BoxMatch> {
        let spec = match get_box_spec(spec_name) {
            Some(s) => s,
            None => return vec![],
        };

        let mut matches = vec![];
        for entry in self.box_cache.iter() {
            let box_ = entry.value();
            let validation = validate_box(box_, &spec);
            if validation.valid {
                matches.push(BoxMatch {
                    box_id: box_.box_id.clone(),
                    spec_name: spec_name.into(),
                    value: box_.value,
                    tokens: box_.tokens.clone(),
                    registers: box_.registers.clone(),
                    validation,
                });
            }
        }
        matches
    }

    /// Scan cached boxes by contract type (any box whose ergo_tree starts with pattern)
    pub fn scan_utxo_set(&self, contract_type: &str) -> Vec<RawBox> {
        let mut result = vec![];
        for entry in self.box_cache.iter() {
            let box_ = entry.value();
            if box_.ergo_tree.contains(contract_type) || box_.ergo_tree.starts_with(contract_type) {
                result.push(box_.clone());
            }
        }
        result
    }

    /// Get a box by its token ID from the cache
    pub fn get_box_by_token_id(&self, token_id: &str) -> Option<RawBox> {
        for entry in self.box_cache.iter() {
            let box_ = entry.value();
            if box_.tokens.iter().any(|t| t.token_id == token_id) {
                return Some(box_.clone());
            }
        }
        None
    }

    /// Insert a box into the cache
    pub fn cache_box(&self, box_: RawBox) {
        self.box_cache.insert(box_.box_id.clone(), box_);
    }

    /// Get engine stats
    pub fn stats(&self) -> ProtocolEngineStats {
        ProtocolEngineStats {
            cached_boxes: self.box_cache.len() as u64,
            validations_total: self.validations_total.load(Ordering::Relaxed),
            validations_passed: self.validations_passed.load(Ordering::Relaxed),
            equations_calculated: self.equations_calculated.load(Ordering::Relaxed),
            actions_built: self.actions_built.load(Ordering::Relaxed),
            boxes_scanned: self.boxes_scanned.load(Ordering::Relaxed),
        }
    }

    /// Record a validation
    fn record_validation(&self, passed: bool) {
        self.validations_total.fetch_add(1, Ordering::Relaxed);
        if passed {
            self.validations_passed.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn record_equation(&self) {
        self.equations_calculated.fetch_add(1, Ordering::Relaxed);
    }

    fn record_action(&self) {
        self.actions_built.fetch_add(1, Ordering::Relaxed);
    }

    fn record_scan(&self, count: u64) {
        self.boxes_scanned.fetch_add(count, Ordering::Relaxed);
    }
}

impl Clone for HeadlessProtocolState {
    fn clone(&self) -> Self {
        // DashMap clone: create fresh empty cache (reference semantics would be unsafe)
        Self {
            box_cache: DashMap::new(),
            validations_total: AtomicU64::new(self.validations_total.load(Ordering::Relaxed)),
            validations_passed: AtomicU64::new(self.validations_passed.load(Ordering::Relaxed)),
            equations_calculated: AtomicU64::new(self.equations_calculated.load(Ordering::Relaxed)),
            actions_built: AtomicU64::new(self.actions_built.load(Ordering::Relaxed)),
            boxes_scanned: AtomicU64::new(self.boxes_scanned.load(Ordering::Relaxed)),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProtocolEngineStats {
    pub cached_boxes: u64,
    pub validations_total: u64,
    pub validations_passed: u64,
    pub equations_calculated: u64,
    pub actions_built: u64,
    pub boxes_scanned: u64,
}

// ================================================================
// REST API Types
// ================================================================

#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub box_data: RawBox,
    pub spec_name: String,
}

#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    pub ok: bool,
    pub result: ValidationResult,
}

#[derive(Debug, Deserialize)]
pub struct BatchValidateRequest {
    pub boxes: Vec<RawBox>,
    pub spec_name: String,
}

#[derive(Debug, Serialize)]
pub struct BatchValidateResponse {
    pub results: Vec<ValidationResult>,
    pub total: usize,
    pub passed: usize,
}

#[derive(Debug, Deserialize)]
pub struct EquationRequest {
    pub equation: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct EquationResponse {
    pub ok: bool,
    pub equation: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActionBuildRequest {
    pub action: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ActionBuildResponse {
    pub ok: bool,
    pub tx: Option<UnsignedTx>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CompileRequest {
    pub ergoscript: String,
}

#[derive(Debug, Serialize)]
pub struct CompileResponse {
    pub ok: bool,
    pub ergo_tree_hex: Option<String>,
    pub error: Option<String>,
}

// ================================================================
// REST Handlers
// ================================================================

async fn list_specs_handler() -> Json<Vec<BoxSpec>> {
    Json(all_box_specs())
}

async fn validate_box_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<ValidateRequest>,
) -> Json<ValidateResponse> {
    let spec = get_box_spec(&req.spec_name);
    match spec {
        None => Json(ValidateResponse {
            ok: false,
            result: ValidationResult::invalid(vec![format!("Unknown spec: {}", req.spec_name)]),
        }),
        Some(s) => {
            let result = validate_box(&req.box_data, &s);
            state.headless_protocol.record_validation(result.valid);
            Json(ValidateResponse { ok: result.valid, result })
        }
    }
}

async fn batch_validate_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<BatchValidateRequest>,
) -> Json<BatchValidateResponse> {
    let spec = get_box_spec(&req.spec_name);
    match spec {
        None => Json(BatchValidateResponse {
            results: vec![],
            total: 0,
            passed: 0,
        }),
        Some(s) => {
            let results: Vec<ValidationResult> = req
                .boxes
                .iter()
                .map(|b| {
                    let r = validate_box(b, &s);
                    state.headless_protocol.record_validation(r.valid);
                    r
                })
                .collect();
            let passed = results.iter().filter(|r| r.valid).count();
            Json(BatchValidateResponse {
                total: results.len(),
                passed,
                results,
            })
        }
    }
}

async fn calculate_equation_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<EquationRequest>,
) -> Json<EquationResponse> {
    state.headless_protocol.record_equation();
    let result = match req.equation.as_str() {
        "calculate_pown_score" => {
            let nw = req.params.get("node_work").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let netw = req.params.get("network_work").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let aiw = req.params.get("ai_work").and_then(|v| v.as_f64()).unwrap_or(0.0);
            serde_json::json!(calculate_pown_score(nw, netw, aiw))
        }
        "calculate_staking_yield" => {
            let sa = req.params.get("staked_amount").and_then(|v| v.as_u64()).unwrap_or(0);
            let ts = req.params.get("total_staked").and_then(|v| v.as_u64()).unwrap_or(0);
            let er = req.params.get("emission_rate").and_then(|v| v.as_u64()).unwrap_or(0);
            serde_json::json!(calculate_staking_yield(sa, ts, er))
        }
        "calculate_rent_cost" => {
            let gt = req.params.get("gpu_type").and_then(|v| v.as_str()).unwrap_or("A100");
            let hrs = req.params.get("hours").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let pph = req.params.get("price_per_hour").and_then(|v| v.as_u64()).unwrap_or(100_000);
            serde_json::json!(calculate_rent_cost(gt, hrs, pph))
        }
        "calculate_inference_cost" => {
            let model = req.params.get("model").and_then(|v| v.as_str()).unwrap_or("llama-3.1-8b");
            let tokens = req.params.get("tokens").and_then(|v| v.as_u64()).unwrap_or(1000) as u32;
            let tier = req.params.get("provider_tier").and_then(|v| v.as_str()).unwrap_or("standard");
            serde_json::json!(calculate_inference_cost(model, tokens, tier))
        }
        "calculate_heartbeat_interval" => {
            let lh = req.params.get("last_heartbeat").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let ch = req.params.get("current_height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            serde_json::json!(calculate_heartbeat_interval(lh, ch))
        }
        "validate_token_balance" => {
            let inputs = req.params.get("input_tokens").and_then(|v| v.as_array());
            let outputs = req.params.get("output_tokens").and_then(|v| v.as_array());
            let in_tokens: Vec<(String, u64)> = inputs
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let id = item.get("id")?.as_str()?.to_string();
                            let amt = item.get("amount")?.as_u64()?;
                            Some((id, amt))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let out_tokens: Vec<(String, u64)> = outputs
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            let id = item.get("id")?.as_str()?.to_string();
                            let amt = item.get("amount")?.as_u64()?;
                            Some((id, amt))
                        })
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!(validate_token_balance(&in_tokens, &out_tokens))
        }
        "calculate_min_box_value" => {
            let ets = req.params.get("ergo_tree_size").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
            let nt = req.params.get("num_tokens").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let rs = req.params.get("register_size").and_then(|v| v.as_u64()).unwrap_or(64) as usize;
            serde_json::json!(calculate_min_box_value(ets, nt, rs))
        }
        _ => {
            let eq_name = req.equation.clone();
            return Json(EquationResponse {
                ok: false,
                equation: eq_name.clone(),
                result: None,
                error: Some(format!("Unknown equation: {}", eq_name)),
            });
        }
    };
    Json(EquationResponse {
        ok: true,
        equation: req.equation,
        result: Some(result),
        error: None,
    })
}

async fn list_equations_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "equations": list_equations(),
    }))
}

async fn build_action_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<ActionBuildRequest>,
) -> Json<ActionBuildResponse> {
    state.headless_protocol.record_action();
    let tx = match req.action.as_str() {
        "provider_registration" => {
            let pid = req.params.get("provider_id").and_then(|v| v.as_str()).unwrap_or("");
            let ep = req.params.get("endpoint").and_then(|v| v.as_str()).unwrap_or("");
            let models = req.params.get("models").and_then(|v| v.as_str()).unwrap_or("");
            let addr = req.params.get("address").and_then(|v| v.as_str()).unwrap_or("");
            build_provider_registration(pid, ep, models, addr)
        }
        "heartbeat_update" => {
            let pown = req.params.get("new_pown").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            // Build a synthetic provider box from params
            let box_id = req.params.get("box_id").and_then(|v| v.as_str()).unwrap_or("synth_box");
            let value = req.params.get("value").and_then(|v| v.as_u64()).unwrap_or(1_000_000_000);
            let mut regs = HashMap::new();
            regs.insert("R4".into(), req.params.get("provider_id").and_then(|v| v.as_str()).unwrap_or("").into());
            regs.insert("R5".into(), req.params.get("endpoint").and_then(|v| v.as_str()).unwrap_or("").into());
            regs.insert("R6".into(), req.params.get("models").and_then(|v| v.as_str()).unwrap_or("").into());
            regs.insert("R7".into(), req.params.get("address").and_then(|v| v.as_str()).unwrap_or("").into());
            regs.insert("R8".into(), "0".into());
            let synth_box = RawBox {
                box_id: box_id.into(),
                value,
                ergo_tree: provider_box_spec().ergo_tree,
                tokens: vec![BoxToken { token_id: "PROVIDER_NFT".into(), amount: 1 }],
                registers: regs,
                creation_height: 800_000,
            };
            build_heartbeat_update(&synth_box, pown)
        }
        "usage_proof" => {
            let user = req.params.get("user_pk").and_then(|v| v.as_str()).unwrap_or("");
            let prov = req.params.get("provider_id").and_then(|v| v.as_str()).unwrap_or("");
            let model = req.params.get("model").and_then(|v| v.as_str()).unwrap_or("");
            let tokens = req.params.get("tokens").and_then(|v| v.as_u64()).unwrap_or(1000);
            build_usage_proof(user, prov, model, tokens)
        }
        "stake_deposit" => {
            let pk = req.params.get("user_pk").and_then(|v| v.as_str()).unwrap_or("");
            let amt = req.params.get("amount_nanoerg").and_then(|v| v.as_u64()).unwrap_or(1_000_000_000);
            build_stake_deposit(pk, amt)
        }
        "stake_withdraw" => {
            let box_id = req.params.get("box_id").and_then(|v| v.as_str()).unwrap_or("synth_box");
            let value = req.params.get("value").and_then(|v| v.as_u64()).unwrap_or(10_000_000_000);
            let amount = req.params.get("amount").and_then(|v| v.as_u64()).unwrap_or(1_000_000_000);
            let mut regs = HashMap::new();
            regs.insert("R4".into(), req.params.get("user_pk").and_then(|v| v.as_str()).unwrap_or("").into());
            regs.insert("R5".into(), value.to_string());
            regs.insert("R6".into(), "1700000000".into());
            let synth_box = RawBox {
                box_id: box_id.into(),
                value,
                ergo_tree: staking_box_spec().ergo_tree,
                tokens: vec![BoxToken { token_id: "STAKING_TOKEN".into(), amount: 1 }],
                registers: regs,
                creation_height: 800_000,
            };
            build_stake_withdraw(&synth_box, amount)
        }
        "airdrop" => {
            let recipient = req.params.get("recipient").and_then(|v| v.as_str()).unwrap_or("P2PK_SCRIPT");
            let amount = req.params.get("amount").and_then(|v| v.as_u64()).unwrap_or(100_000_000);
            let mut regs = HashMap::new();
            regs.insert("R4".into(), "admin_pk_placeholder".into());
            regs.insert("R5".into(), "0".into());
            regs.insert("R6".into(), "0".into());
            let synth_treasury = RawBox {
                box_id: "treasury_synth".into(),
                value: 10_000_000_000,
                ergo_tree: treasury_box_spec().ergo_tree,
                tokens: vec![BoxToken { token_id: "TREASURY_NFT".into(), amount: 1 }],
                registers: regs,
                creation_height: 800_000,
            };
            build_airdrop(&synth_treasury, recipient, amount)
        }
        "settlement" => {
            let amount = req.params.get("amount").and_then(|v| v.as_u64()).unwrap_or(500_000_000);
            let fee = req.params.get("fee").and_then(|v| v.as_u64()).unwrap_or(1_100_000);
            let mut user_regs = HashMap::new();
            user_regs.insert("R4".into(), "user_pk".into());
            let user_box = RawBox {
                box_id: "user_synth".into(),
                value: 2_000_000_000,
                ergo_tree: "USER_SCRIPT".into(),
                tokens: vec![],
                registers: user_regs,
                creation_height: 800_000,
            };
            let mut prov_regs = HashMap::new();
            prov_regs.insert("R4".into(), "provider_id".into());
            prov_regs.insert("R5".into(), "https://endpoint".into());
            prov_regs.insert("R6".into(), "llama-3.1-70b".into());
            prov_regs.insert("R7".into(), "9h...addr".into());
            prov_regs.insert("R8".into(), "850".into());
            let provider_box = RawBox {
                box_id: "provider_synth".into(),
                value: 1_000_000_000,
                ergo_tree: provider_box_spec().ergo_tree,
                tokens: vec![BoxToken { token_id: "PROVIDER_NFT".into(), amount: 1 }],
                registers: prov_regs,
                creation_height: 800_000,
            };
            build_settlement(&user_box, &provider_box, amount, fee)
        }
        _ => {
            return Json(ActionBuildResponse {
                ok: false,
                tx: None,
                error: Some(format!("Unknown action: {}", req.action)),
            });
        }
    };
    Json(ActionBuildResponse { ok: true, tx: Some(tx), error: None })
}

async fn find_boxes_handler(
    State(state): State<proxy::AppState>,
    Path(spec_name): Path<String>,
) -> Json<Vec<BoxMatch>> {
    let matches = state.headless_protocol.find_boxes(&spec_name);
    Json(matches)
}

async fn get_box_handler(
    State(state): State<proxy::AppState>,
    Path(box_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.headless_protocol.box_cache.get(&box_id) {
        Some(box_) => Json(serde_json::json!({"ok": true, "box": box_.value()})),
        None => Json(serde_json::json!({"ok": false, "error": format!("Box {} not found in cache", box_id)})),
    }
}

async fn compile_handler(
    Json(req): Json<CompileRequest>,
) -> Json<CompileResponse> {
    // Minimal ErgoScript compilation stub — returns a placeholder ErgoTree
    // In production this would call the node's /script endpoint
    if req.ergoscript.is_empty() {
        return Json(CompileResponse {
            ok: false,
            ergo_tree_hex: None,
            error: Some("Empty ErgoScript".into()),
        });
    }
    // Stub: return a hex-encoded placeholder that matches sigmaProp(true)
    Json(CompileResponse {
        ok: true,
        ergo_tree_hex: Some("100104c0c311c3".into()),
        error: None,
    })
}

async fn stats_handler(
    State(state): State<proxy::AppState>,
) -> Json<ProtocolEngineStats> {
    Json(state.headless_protocol.stats())
}

// ================================================================
// Router
// ================================================================

pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    Router::new()
        .route("/v1/protocol/specs", post(list_specs_handler))
        .route("/v1/protocol/validate", post(validate_box_handler))
        .route("/v1/protocol/batch-validate", post(batch_validate_handler))
        .route("/v1/protocol/equations/calculate", post(calculate_equation_handler))
        .route("/v1/protocol/equations/list", get(list_equations_handler))
        .route("/v1/protocol/actions/build", post(build_action_handler))
        .route("/v1/protocol/boxes/{spec_name}", get(find_boxes_handler))
        .route("/v1/protocol/box/{box_id}", get(get_box_handler))
        .route("/v1/protocol/compile", post(compile_handler))
        .route("/v1/protocol/stats", get(stats_handler))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider_box() -> RawBox {
        let mut regs = HashMap::new();
        regs.insert("R4".into(), "provider_abc123".into());
        regs.insert("R5".into(), "https://ai.example.com".into());
        regs.insert("R6".into(), "llama-3.1-70b,mixtral-8x7b".into());
        regs.insert("R7".into(), "9hZ8kF2pR4sT7vW1x".into());
        regs.insert("R8".into(), "750".into());
        RawBox {
            box_id: "a".repeat(64),
            value: 1_500_000_000,
            ergo_tree: "100204020004000500060007000800deadbeef".into(),
            tokens: vec![BoxToken {
                token_id: "PROVIDER_NFT".into(),
                amount: 1,
            }],
            registers: regs,
            creation_height: 800_000,
        }
    }

    fn make_staking_box() -> RawBox {
        let mut regs = HashMap::new();
        regs.insert("R4".into(), "user_pk_hex".into());
        regs.insert("R5".into(), "5000000000".into());
        regs.insert("R6".into(), "1700000000".into());
        RawBox {
            box_id: "b".repeat(64),
            value: 5_000_000_000,
            ergo_tree: "1002040200040005000600abcdef".into(),
            tokens: vec![BoxToken {
                token_id: "STAKING_TOKEN".into(),
                amount: 100,
            }],
            registers: regs,
            creation_height: 800_000,
        }
    }

    fn make_treasury_box() -> RawBox {
        let mut regs = HashMap::new();
        regs.insert("R4".into(), "admin_pk_hex".into());
        regs.insert("R5".into(), "10000000000".into());
        regs.insert("R6".into(), "42".into());
        RawBox {
            box_id: "c".repeat(64),
            value: 100_000_000_000,
            ergo_tree: "1002040200040005000600".into(),
            tokens: vec![BoxToken {
                token_id: "TREASURY_NFT".into(),
                amount: 1,
            }],
            registers: regs,
            creation_height: 800_000,
        }
    }

    // ---- Layer 1 tests ----

    #[test]
    fn test_all_box_specs_count() {
        let specs = all_box_specs();
        assert_eq!(specs.len(), 6);
    }

    #[test]
    fn test_get_box_spec_found() {
        let spec = get_box_spec("ProviderBox");
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().name, "ProviderBox");
    }

    #[test]
    fn test_get_box_spec_not_found() {
        let spec = get_box_spec("NonExistent");
        assert!(spec.is_none());
    }

    #[test]
    fn test_provider_spec_has_nft_token() {
        let spec = get_box_spec("ProviderBox").unwrap();
        assert_eq!(spec.required_tokens.len(), 1);
        assert_eq!(spec.required_tokens[0].token_id, "PROVIDER_NFT");
    }

    #[test]
    fn test_usage_proof_spec_registers() {
        let spec = get_box_spec("UsageProof").unwrap();
        assert_eq!(spec.register_specs.len(), 5);
        assert!(spec.register_specs.contains_key("R4"));
        assert!(spec.register_specs.contains_key("R8"));
    }

    // ---- Layer 2 tests ----

    #[test]
    fn test_validate_provider_box_valid() {
        let box_ = make_provider_box();
        let spec = provider_box_spec();
        let result = validate_box(&box_, &spec);
        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.register_values.get("R4").unwrap(), "provider_abc123");
    }

    #[test]
    fn test_validate_box_low_value() {
        let mut box_ = make_provider_box();
        box_.value = 100; // way below minimum
        let spec = provider_box_spec();
        let result = validate_box(&box_, &spec);
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_box_missing_token() {
        let mut box_ = make_provider_box();
        box_.tokens.clear();
        let spec = provider_box_spec();
        let result = validate_box(&box_, &spec);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Missing required token")));
    }

    #[test]
    fn test_validate_box_missing_required_register() {
        let mut box_ = make_provider_box();
        box_.registers.remove("R4");
        let spec = provider_box_spec();
        let result = validate_box(&box_, &spec);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("Missing required register")));
    }

    #[test]
    fn test_validate_staking_box_valid() {
        let box_ = make_staking_box();
        let spec = staking_box_spec();
        let result = validate_box(&box_, &spec);
        assert!(result.valid);
    }

    // ---- Layer 3 tests ----

    #[test]
    fn test_calculate_pown_score() {
        let score = calculate_pown_score(100.0, 50.0, 75.0);
        assert!(score > 0.0);
    }

    #[test]
    fn test_calculate_pown_score_zero() {
        let score = calculate_pown_score(0.0, 0.0, 0.0);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_calculate_staking_yield() {
        let yield_val = calculate_staking_yield(1_000_000, 10_000_000, 500_000);
        assert_eq!(yield_val, 50_000.0);
    }

    #[test]
    fn test_calculate_staking_yield_zero_total() {
        let yield_val = calculate_staking_yield(1_000_000, 0, 500_000);
        assert_eq!(yield_val, 0.0);
    }

    #[test]
    fn test_calculate_rent_cost() {
        let cost = calculate_rent_cost("H100", 10, 100_000);
        assert_eq!(cost, 2_000_000); // 10 * 100000 * 2.0
    }

    #[test]
    fn test_calculate_rent_cost_unknown_gpu() {
        let cost = calculate_rent_cost("UNKNOWN", 5, 100_000);
        assert_eq!(cost, 500_000); // 5 * 100000 * 1.0
    }

    #[test]
    fn test_calculate_inference_cost() {
        let cost = calculate_inference_cost("llama-3.1-70b", 2000, "standard");
        assert_eq!(cost, 100_000); // 2.0 * 50000 * 1.0
    }

    #[test]
    fn test_calculate_heartbeat_interval() {
        let interval = calculate_heartbeat_interval(800_000, 800_100);
        assert_eq!(interval, 100);
    }

    #[test]
    fn test_calculate_heartbeat_interval_no_heartbeat() {
        let interval = calculate_heartbeat_interval(900_000, 800_000);
        assert_eq!(interval, 0);
    }

    #[test]
    fn test_validate_token_balance_conserved() {
        let inputs = vec![("TOKEN_A".into(), 100)];
        let outputs = vec![("TOKEN_A".into(), 80)];
        assert!(validate_token_balance(&inputs, &outputs));
    }

    #[test]
    fn test_validate_token_balance_violated() {
        let inputs = vec![("TOKEN_A".into(), 50)];
        let outputs = vec![("TOKEN_A".into(), 100)];
        assert!(!validate_token_balance(&inputs, &outputs));
    }

    #[test]
    fn test_calculate_min_box_value() {
        let val = calculate_min_box_value(100, 1, 64);
        // (100 + 300 + 64) * 360 = 464 * 360 = 167040
        assert_eq!(val, 167_040);
    }

    // ---- Layer 4 tests ----

    #[test]
    fn test_build_provider_registration() {
        let tx = build_provider_registration("prov1", "https://x.io", "llama-3.1-70b", "9h...addr");
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.fee, 1_100_000);
        assert_eq!(tx.outputs[0].registers.get("R4").unwrap(), "prov1");
    }

    #[test]
    fn test_build_usage_proof() {
        let tx = build_usage_proof("user1", "prov1", "llama-3.1-8b", 500);
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].registers.get("R7").unwrap(), "500");
    }

    #[test]
    fn test_build_stake_deposit() {
        let tx = build_stake_deposit("user1", 2_000_000_000);
        assert_eq!(tx.outputs[0].value, 2_000_000_000);
    }

    #[test]
    fn test_build_stake_withdraw_full() {
        let box_ = make_staking_box();
        let tx = build_stake_withdraw(&box_, 5_000_000_000);
        // Full withdrawal — remaining is 0, so no staking output, just user output
        assert!(tx.outputs.len() >= 1);
    }

    #[test]
    fn test_build_airdrop() {
        let box_ = make_treasury_box();
        let tx = build_airdrop(&box_, "recipient_script", 100_000_000);
        assert_eq!(tx.outputs.len(), 2);
        // First output is the treasury remainder
        assert_eq!(tx.outputs[0].value, 100_000_000_000 - 100_000_000);
        // Second output is the airdrop recipient
        assert_eq!(tx.outputs[1].value, 100_000_000);
        // R5 should reflect updated total_allocated
        assert_eq!(tx.outputs[0].registers.get("R5").unwrap(), "10100000000");
    }

    #[test]
    fn test_build_settlement() {
        let user = RawBox {
            box_id: "u".repeat(64),
            value: 2_000_000_000,
            ergo_tree: "USER_SCRIPT".into(),
            tokens: vec![],
            registers: HashMap::new(),
            creation_height: 800_000,
        };
        let prov = make_provider_box();
        let tx = build_settlement(&user, &prov, 500_000_000, 1_100_000);
        assert_eq!(tx.inputs.len(), 2);
        assert!(tx.outputs.len() >= 1);
        // Provider output should have increased value
        assert_eq!(tx.outputs[0].value, prov.value + 500_000_000);
    }

    // ---- Layer 5 tests ----

    #[test]
    fn test_find_boxes_cached() {
        let state = HeadlessProtocolState::new();
        let box_ = make_provider_box();
        state.cache_box(box_.clone());

        let matches = state.find_boxes("ProviderBox");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].box_id, box_.box_id);
    }

    #[test]
    fn test_find_boxes_wrong_spec() {
        let state = HeadlessProtocolState::new();
        let box_ = make_provider_box();
        state.cache_box(box_);

        let matches = state.find_boxes("TreasuryBox");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_scan_utxo_set() {
        let state = HeadlessProtocolState::new();
        state.cache_box(make_provider_box());
        state.cache_box(make_staking_box());

        let results = state.scan_utxo_set("1002040200040005000600");
        // Staking and treasury use this prefix
        assert!(results.len() >= 1);
    }

    #[test]
    fn test_get_box_by_token_id() {
        let state = HeadlessProtocolState::new();
        state.cache_box(make_provider_box());

        let found = state.get_box_by_token_id("PROVIDER_NFT");
        assert!(found.is_some());

        let not_found = state.get_box_by_token_id("NONEXISTENT");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_protocol_stats() {
        let state = HeadlessProtocolState::new();
        state.cache_box(make_provider_box());
        let result = validate_box(&make_provider_box(), &provider_box_spec());
        state.record_validation(result.valid);
        state.record_equation();
        state.record_action();

        let stats = state.stats();
        assert_eq!(stats.cached_boxes, 1);
        assert_eq!(stats.validations_total, 1);
        assert_eq!(stats.validations_passed, 1);
        assert_eq!(stats.equations_calculated, 1);
        assert_eq!(stats.actions_built, 1);
    }

    #[test]
    fn test_list_equations() {
        let eqs = list_equations();
        assert_eq!(eqs.len(), 7);
        assert!(eqs.contains(&"calculate_pown_score"));
        assert!(eqs.contains(&"calculate_staking_yield"));
    }

    #[test]
    fn test_validate_box_ergo_tree_mismatch() {
        let mut box_ = make_provider_box();
        box_.ergo_tree = "DEADBEEF".into();
        let spec = provider_box_spec();
        let result = validate_box(&box_, &spec);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("ErgoTree mismatch")));
    }

    #[test]
    fn test_gpu_rental_spec() {
        let spec = get_box_spec("GpuRental").unwrap();
        assert_eq!(spec.register_specs.len(), 4);
        assert!(spec.register_specs.contains_key("R6")); // rental_end
        assert!(spec.register_specs.contains_key("R7")); // price_per_hour
    }
}
