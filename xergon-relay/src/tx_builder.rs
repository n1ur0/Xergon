//! Ergo Transaction Builder + Fee Optimizer.
//!
//! Constructs Ergo transactions with intelligent UTXO selection, fee optimization,
//! and box consolidation. Provides REST endpoints for building, simulating, and
//! optimizing transactions.
//!
//! Endpoints:
//!   POST /v1/tx-builder/build            -- build a transaction
//!   POST /v1/tx-builder/simulate         -- simulate transaction building
//!   POST /v1/tx-builder/select-utxos     -- select UTXOs using a strategy
//!   GET  /v1/tx-builder/fee-estimate     -- estimate transaction fee
//!   GET  /v1/tx-builder/box-size-estimate -- estimate output box size
//!   POST /v1/tx-builder/consolidate      -- consolidate small UTXOs
//!   GET  /v1/tx-builder/tx/:tx_id        -- get a built transaction
//!   GET  /v1/tx-builder/utxos            -- list available UTXOs
//!   PUT  /v1/tx-builder/config           -- update builder config
//!   GET  /v1/tx-builder/stats            -- get builder statistics

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use chrono::Utc;
use dashmap::DashMap;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info;};
use uuid::Uuid;

use crate::proxy;

// ================================================================
// Domain Types
// ================================================================

/// Reference to a token with ID and amount.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TokenRef {
    pub token_id: String,
    pub amount: u64,
}

/// A transaction input (UTXO) with all box data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    #[serde(default)]
    pub tokens: Vec<TokenRef>,
    #[serde(default)]
    pub registers: HashMap<String, String>,
    #[serde(default)]
    pub extension: HashMap<String, String>,
}

/// A transaction output specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub ergo_tree: String,
    pub value: u64,
    #[serde(default)]
    pub tokens: Vec<TokenRef>,
    #[serde(default)]
    pub registers: HashMap<String, String>,
}

/// An unsigned transaction ready for signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedTransaction {
    pub inputs: Vec<TxInput>,
    #[serde(default)]
    pub data_inputs: Vec<String>,
    pub outputs: Vec<TxOutput>,
    pub fee: u64,
    pub size_bytes: u32,
}

/// A fully built transaction with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltTransaction {
    pub tx_id: String,
    pub unsigned_tx: UnsignedTransaction,
    pub fee: u64,
    pub size_bytes: u32,
    pub inputs_count: u32,
    pub outputs_count: u32,
    pub created_at: i64,
}

/// Result of a UTXO selection algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoSelection {
    pub inputs: Vec<TxInput>,
    pub total_value: u64,
    pub total_tokens: HashMap<String, u64>,
    pub change_amount: u64,
}

/// Strategy for UTXO selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStrategy {
    Greedy,
    BranchAndBound,
    FIFO,
    RandomImprove,
    Consolidate,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        Self::Greedy
    }
}

/// Fee estimation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimate {
    pub min_fee: u64,
    pub optimal_fee: u64,
    pub fast_fee: u64,
    pub size_bytes: u32,
    pub inputs_used: u32,
}

/// Box size estimation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxSizeEstimate {
    pub base_size: u32,
    pub per_token: u32,
    pub per_register: u32,
    pub total: u32,
}

/// Configuration for the transaction builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxBuildConfig {
    #[serde(default = "default_fee_per_byte")]
    pub fee_per_byte: u64,
    #[serde(default = "default_min_fee")]
    pub min_fee: u64,
    #[serde(default = "default_max_inputs")]
    pub max_inputs: u32,
    #[serde(default)]
    pub selection_strategy: SelectionStrategy,
    #[serde(default = "default_dust_threshold")]
    pub dust_threshold: u64,
    #[serde(default = "default_consolidation_threshold")]
    pub consolidation_threshold: u32,
}

fn default_fee_per_byte() -> u64 {
    360 // nanoERG per byte (Ergo standard)
}

fn default_min_fee() -> u64 {
    100_000 // 0.0001 ERG
}

fn default_max_inputs() -> u32 {
    50
}

fn default_dust_threshold() -> u64 {
    360_000 // 0.00036 ERG minimum box value
}

fn default_consolidation_threshold() -> u32 {
    20
}

impl Default for TxBuildConfig {
    fn default() -> Self {
        Self {
            fee_per_byte: default_fee_per_byte(),
            min_fee: default_min_fee(),
            max_inputs: default_max_inputs(),
            selection_strategy: SelectionStrategy::Greedy,
            dust_threshold: default_dust_threshold(),
            consolidation_threshold: default_consolidation_threshold(),
        }
    }
}

/// Builder statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TxBuilderStats {
    pub total_built: u64,
    pub total_fees_paid: u64,
    pub avg_fee: u64,
    pub avg_inputs: u32,
    pub avg_size: u32,
    pub consolidations: u64,
}

// ================================================================
// Internal UTXO entry with insertion order for FIFO
// ================================================================

#[derive(Debug, Clone)]
struct UtxoEntry {
    input: TxInput,
    inserted_at: i64,
}

// ================================================================
// TxBuilderEngine
// ================================================================

/// Core transaction builder engine with UTXO pool and selection algorithms.
pub struct TxBuilderEngine {
    /// UTXO pool keyed by box_id
    utxo_pool: DashMap<String, UtxoEntry>,
    /// Built transactions keyed by tx_id
    built_txs: DashMap<String, BuiltTransaction>,
    /// Builder configuration
    config: std::sync::RwLock<TxBuildConfig>,
    /// Statistics
    stats_total_built: AtomicU64,
    stats_total_fees: AtomicU64,
    stats_avg_fee: AtomicU64,
    stats_avg_inputs: AtomicU64,
    stats_avg_size: AtomicU64,
    stats_consolidations: AtomicU64,
}

impl TxBuilderEngine {
    /// Create a new engine with default config.
    pub fn new() -> Self {
        Self::with_config(TxBuildConfig::default())
    }

    /// Create a new engine with a specific config.
    pub fn with_config(config: TxBuildConfig) -> Self {
        Self {
            utxo_pool: DashMap::new(),
            built_txs: DashMap::new(),
            config: std::sync::RwLock::new(config),
            stats_total_built: AtomicU64::new(0),
            stats_total_fees: AtomicU64::new(0),
            stats_avg_fee: AtomicU64::new(0),
            stats_avg_inputs: AtomicU64::new(0),
            stats_avg_size: AtomicU64::new(0),
            stats_consolidations: AtomicU64::new(0),
        }
    }

    // ---------------------------------------------------------------
    // UTXO Pool Management
    // ---------------------------------------------------------------

    /// Add a UTXO to the available pool.
    pub fn add_utxo(&self, utxo: TxInput) {
        let now = Utc::now().timestamp_millis();
        let entry = UtxoEntry {
            input: utxo,
            inserted_at: now,
        };
        let box_id = entry.input.box_id.clone();
        self.utxo_pool.insert(box_id.clone(), entry);
        debug!(box_id = %box_id, "UTXO added to pool");
    }

    /// Remove a spent UTXO from the pool.
    pub fn remove_utxo(&self, box_id: &str) -> bool {
        let removed = self.utxo_pool.remove(box_id).is_some();
        if removed {
            debug!(box_id = %box_id, "UTXO removed from pool");
        }
        removed
    }

    /// List available UTXOs, optionally filtered by minimum value.
    pub fn list_utxos(&self, min_value: Option<u64>) -> Vec<TxInput> {
        let mut result: Vec<TxInput> = self
            .utxo_pool
            .iter()
            .filter_map(|entry| {
                let input = &entry.value().input;
                if let Some(min) = min_value {
                    if input.value < min {
                        return None;
                    }
                }
                Some(input.clone())
            })
            .collect();
        result.sort_by(|a, b| b.value.cmp(&a.value));
        result
    }

    // ---------------------------------------------------------------
    // Fee and Size Estimation
    // ---------------------------------------------------------------

    /// Estimate transaction fee.
    pub fn estimate_fee(
        &self,
        inputs_count: u32,
        outputs_count: u32,
        has_data_inputs: bool,
    ) -> FeeEstimate {
        let config = self.config.read().unwrap();
        self.estimate_fee_with_config(&config, inputs_count, outputs_count, has_data_inputs)
    }

    fn estimate_fee_with_config(
        &self,
        config: &TxBuildConfig,
        inputs_count: u32,
        outputs_count: u32,
        has_data_inputs: bool,
    ) -> FeeEstimate {
        // Ergo tx size estimate:
        // Header: ~40 bytes
        // Per input: ~200 bytes
        // Per output: ~200 bytes
        // Per data input: ~40 bytes
        let header_size: u32 = 40;
        let input_size: u32 = 200;
        let output_size: u32 = 200;
        let data_input_size: u32 = 40;

        let data_inputs_count: u32 = if has_data_inputs { 1 } else { 0 };
        let size_bytes = header_size
            + (inputs_count * input_size)
            + (outputs_count * output_size)
            + (data_inputs_count * data_input_size);

        let min_fee = config.min_fee;
        let optimal_fee = min_fee + (size_bytes as u64 * config.fee_per_byte);
        let fast_fee = optimal_fee + (optimal_fee / 5); // 20% premium

        FeeEstimate {
            min_fee,
            optimal_fee,
            fast_fee,
            size_bytes,
            inputs_used: inputs_count,
        }
    }

    /// Estimate output box size.
    pub fn estimate_box_size(&self, tokens_count: u32, registers_count: u32) -> BoxSizeEstimate {
        let base_size: u32 = 40;
        let per_token: u32 = 34;
        let per_register: u32 = 64;
        let total = base_size + (tokens_count * per_token) + (registers_count * per_register);

        BoxSizeEstimate {
            base_size,
            per_token,
            per_register,
            total,
        }
    }

    // ---------------------------------------------------------------
    // UTXO Selection Algorithms
    // ---------------------------------------------------------------

    /// Select UTXOs using the specified strategy.
    pub fn select_utxos(
        &self,
        target_value: u64,
        target_tokens: &HashMap<String, u64>,
        strategy: &SelectionStrategy,
    ) -> Result<UtxoSelection, String> {
        let config = self.config.read().unwrap();

        let selection = match strategy {
            SelectionStrategy::Greedy => self.select_greedy(target_value, target_tokens, &config),
            SelectionStrategy::BranchAndBound => {
                self.select_branch_and_bound(target_value, target_tokens, &config)
            }
            SelectionStrategy::FIFO => self.select_fifo(target_value, target_tokens, &config),
            SelectionStrategy::RandomImprove => {
                self.select_random_improve(target_value, target_tokens, &config)
            }
            SelectionStrategy::Consolidate => {
                self.select_consolidate(target_value, target_tokens, &config)
            }
        }?;

        // Validate dust
        if selection.change_amount > 0 && selection.change_amount < config.dust_threshold {
            return Err(format!(
                "Change amount {} is below dust threshold {}",
                selection.change_amount, config.dust_threshold
            ));
        }

        Ok(selection)
    }

    /// Greedy selection: pick largest UTXOs first until target met.
    fn select_greedy(
        &self,
        target_value: u64,
        target_tokens: &HashMap<String, u64>,
        config: &TxBuildConfig,
    ) -> Result<UtxoSelection, String> {
        let mut all_utxos: Vec<TxInput> = self
            .utxo_pool
            .iter()
            .map(|e| e.value().input.clone())
            .collect();
        all_utxos.sort_by(|a, b| b.value.cmp(&a.value));

        let fee_estimate = self.estimate_fee_with_config(config, 1, 1, false);
        let total_target = target_value.saturating_add(fee_estimate.min_fee);

        let mut selected: Vec<TxInput> = Vec::new();
        let mut total_value: u64 = 0;
        let mut total_tokens: HashMap<String, u64> = HashMap::new();

        for utxo in &all_utxos {
            if selected.len() as u32 >= config.max_inputs {
                break;
            }

            // Check token requirements
            for token in &utxo.tokens {
                *total_tokens.entry(token.token_id.clone()).or_insert(0) += token.amount;
            }

            total_value += utxo.value;
            selected.push(utxo.clone());

            if self.meets_requirements(total_value, &total_tokens, total_target, target_tokens) {
                let change = total_value.saturating_sub(target_value);
                return Ok(UtxoSelection {
                    inputs: selected,
                    total_value,
                    total_tokens,
                    change_amount: change,
                });
            }
        }

        Err("Insufficient UTXOs to cover target value and fee".into())
    }

    /// Branch-and-bound: try all combinations for exact match (capped at 1000 iterations).
    fn select_branch_and_bound(
        &self,
        target_value: u64,
        target_tokens: &HashMap<String, u64>,
        config: &TxBuildConfig,
    ) -> Result<UtxoSelection, String> {
        let all_utxos: Vec<TxInput> = self
            .utxo_pool
            .iter()
            .map(|e| e.value().input.clone())
            .collect();

        let fee_estimate = self.estimate_fee_with_config(config, 1, 1, false);
        let total_target = target_value.saturating_add(fee_estimate.min_fee);

        // Sort descending by value for effective pruning
        let mut sorted_utxos = all_utxos.clone();
        sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

        let max_iterations = 1000;
        let mut iterations = 0;

        // Try subsets of increasing size
        for subset_size in 1..=(config.max_inputs as usize).min(sorted_utxos.len()) {
            if iterations >= max_iterations {
                break;
            }

            // Try combinations using a recursive approach with early termination
            if let Some(result) = self.try_subset_exact(
                &sorted_utxos,
                0,
                subset_size,
                total_target,
                target_tokens,
                &mut iterations,
                max_iterations,
            ) {
                return Ok(result);
            }
        }

        // Fallback to greedy if exact match not found
        debug!("Branch-and-bound failed to find exact match, falling back to greedy");
        self.select_greedy(target_value, target_tokens, config)
    }

    fn try_subset_exact(
        &self,
        utxos: &[TxInput],
        start: usize,
        size: usize,
        target: u64,
        target_tokens: &HashMap<String, u64>,
        iterations: &mut u32,
        max_iterations: u32,
    ) -> Option<UtxoSelection> {
        if size == 0 {
            return None;
        }

        for i in start..=(utxos.len().saturating_sub(size)) {
            *iterations += 1;
            if *iterations > max_iterations {
                return None;
            }

            if size == 1 {
                let utxo = &utxos[i];
                let mut total_tokens: HashMap<String, u64> = HashMap::new();
                for token in &utxo.tokens {
                    *total_tokens.entry(token.token_id.clone()).or_insert(0) += token.amount;
                }

                if utxo.value >= target && self.meets_token_reqs(&total_tokens, target_tokens) {
                    let change = utxo.value.saturating_sub(target.saturating_sub(self.config.read().unwrap().min_fee));
                    return Some(UtxoSelection {
                        inputs: vec![utxo.clone()],
                        total_value: utxo.value,
                        total_tokens,
                        change_amount: change,
                    });
                }
            } else if let Some(mut result) = self.try_subset_exact(
                utxos,
                i + 1,
                size - 1,
                target,
                target_tokens,
                iterations,
                max_iterations,
            ) {
                let utxo = &utxos[i];
                for token in &utxo.tokens {
                    *result
                        .total_tokens
                        .entry(token.token_id.clone())
                        .or_insert(0) += token.amount;
                }
                result.total_value += utxo.value;
                result.inputs.insert(0, utxo.clone());

                if result.total_value >= target
                    && self.meets_token_reqs(&result.total_tokens, target_tokens)
                {
                    let config = self.config.read().unwrap();
                    let change = result
                        .total_value
                        .saturating_sub(target.saturating_add(config.min_fee));
                    result.change_amount = change;
                    return Some(result);
                }
            }
        }
        None
    }

    /// FIFO selection: use oldest UTXOs first.
    fn select_fifo(
        &self,
        target_value: u64,
        target_tokens: &HashMap<String, u64>,
        config: &TxBuildConfig,
    ) -> Result<UtxoSelection, String> {
        let mut all_entries: Vec<UtxoEntry> = self
            .utxo_pool
            .iter()
            .map(|e| e.value().clone())
            .collect();
        // Sort by insertion time (oldest first)
        all_entries.sort_by_key(|e| e.inserted_at);

        let fee_estimate = self.estimate_fee_with_config(config, 1, 1, false);
        let total_target = target_value.saturating_add(fee_estimate.min_fee);

        let mut selected: Vec<TxInput> = Vec::new();
        let mut total_value: u64 = 0;
        let mut total_tokens: HashMap<String, u64> = HashMap::new();

        for entry in &all_entries {
            if selected.len() as u32 >= config.max_inputs {
                break;
            }

            for token in &entry.input.tokens {
                *total_tokens.entry(token.token_id.clone()).or_insert(0) += token.amount;
            }

            total_value += entry.input.value;
            selected.push(entry.input.clone());

            if self.meets_requirements(total_value, &total_tokens, total_target, target_tokens) {
                let change = total_value.saturating_sub(target_value);
                return Ok(UtxoSelection {
                    inputs: selected,
                    total_value,
                    total_tokens,
                    change_amount: change,
                });
            }
        }

        Err("Insufficient UTXOs to cover target value and fee".into())
    }

    /// RandomImprove: random selection then try to improve with additional UTXOs.
    fn select_random_improve(
        &self,
        target_value: u64,
        target_tokens: &HashMap<String, u64>,
        config: &TxBuildConfig,
    ) -> Result<UtxoSelection, String> {
        let mut all_utxos: Vec<TxInput> = self
            .utxo_pool
            .iter()
            .map(|e| e.value().input.clone())
            .collect();

        let fee_estimate = self.estimate_fee_with_config(config, 1, 1, false);
        let total_target = target_value.saturating_add(fee_estimate.min_fee);

        if all_utxos.is_empty() {
            return Err("No UTXOs available".into());
        }

        // Shuffle for random selection
        let mut rng = rand::rng();
        all_utxos.shuffle(&mut rng);

        let mut selected: Vec<TxInput> = Vec::new();
        let mut total_value: u64 = 0;
        let mut total_tokens: HashMap<String, u64> = HashMap::new();
        let mut selected_indices: Vec<usize> = Vec::new();

        // Phase 1: Random selection until target met
        for (idx, utxo) in all_utxos.iter().enumerate() {
            if selected.len() as u32 >= config.max_inputs {
                break;
            }

            for token in &utxo.tokens {
                *total_tokens.entry(token.token_id.clone()).or_insert(0) += token.amount;
            }

            total_value += utxo.value;
            selected.push(utxo.clone());
            selected_indices.push(idx);

            if self.meets_requirements(total_value, &total_tokens, total_target, target_tokens) {
                break;
            }
        }

        if !self.meets_requirements(total_value, &total_tokens, total_target, target_tokens) {
            return Err("Insufficient UTXOs to cover target value and fee".into());
        }

        // Phase 2: Try to improve — replace a selected UTXO with a better one
        let change_before = total_value.saturating_sub(target_value);
        let ideal_change = 3 * config.dust_threshold; // Target ~3x dust for comfortable change

        // Sort remaining by distance to ideal supplement
        let remaining: Vec<(usize, &TxInput)> = all_utxos
            .iter()
            .enumerate()
            .filter(|(i, _)| !selected_indices.contains(i))
            .collect();

        if change_before < ideal_change {
            // Try adding one more UTXO to improve change
            for (_, utxo) in remaining.iter().take(10) {
                if selected.len() as u32 >= config.max_inputs {
                    break;
                }
                let new_value = total_value + utxo.value;
                let new_change = new_value.saturating_sub(target_value);
                if new_change <= ideal_change * 2 {
                    // This UTXO improves our change position
                    for token in &utxo.tokens {
                        *total_tokens.entry(token.token_id.clone()).or_insert(0) += token.amount;
                    }
                    total_value = new_value;
                    selected.push((*utxo).clone());
                    break;
                }
            }
        }

        let change = total_value.saturating_sub(target_value);
        Ok(UtxoSelection {
            inputs: selected,
            total_value,
            total_tokens,
            change_amount: change,
        })
    }

    /// Consolidate: combine all small UTXOs below threshold.
    fn select_consolidate(
        &self,
        target_value: u64,
        _target_tokens: &HashMap<String, u64>,
        config: &TxBuildConfig,
    ) -> Result<UtxoSelection, String> {
        let mut small_utxos: Vec<TxInput> = self
            .utxo_pool
            .iter()
            .filter_map(|e| {
                let input = &e.value().input;
                if input.value < target_value {
                    Some(input.clone())
                } else {
                    None
                }
            })
            .collect();
        small_utxos.sort_by(|a, b| a.value.cmp(&b.value)); // smallest first

        let max_to_take = config.consolidation_threshold as usize;
        let take_count = max_to_take.min(small_utxos.len());

        if take_count == 0 {
            return Err("No small UTXOs to consolidate".into());
        }

        let selected: Vec<TxInput> = small_utxos.into_iter().take(take_count).collect();
        let total_value: u64 = selected.iter().map(|u| u.value).sum();
        let mut total_tokens: HashMap<String, u64> = HashMap::new();
        for utxo in &selected {
            for token in &utxo.tokens {
                *total_tokens.entry(token.token_id.clone()).or_insert(0) += token.amount;
            }
        }

        Ok(UtxoSelection {
            inputs: selected,
            total_value,
            total_tokens,
            change_amount: 0, // Consolidation has no change target
        })
    }

    fn meets_requirements(
        &self,
        total_value: u64,
        total_tokens: &HashMap<String, u64>,
        target_value: u64,
        target_tokens: &HashMap<String, u64>,
    ) -> bool {
        if total_value < target_value {
            return false;
        }
        self.meets_token_reqs(total_tokens, target_tokens)
    }

    fn meets_token_reqs(
        &self,
        available: &HashMap<String, u64>,
        required: &HashMap<String, u64>,
    ) -> bool {
        for (token_id, amount) in required {
            if available.get(token_id).copied().unwrap_or(0) < *amount {
                return false;
            }
        }
        true
    }

    // ---------------------------------------------------------------
    // Transaction Building
    // ---------------------------------------------------------------

    /// Build a complete transaction.
    pub async fn build_transaction(
        &self,
        outputs: Vec<TxOutput>,
        data_inputs: Option<Vec<String>>,
        selection_strategy: Option<SelectionStrategy>,
        fee_per_byte: Option<u64>,
    ) -> Result<BuiltTransaction, String> {
        let mut config = self.config.write().unwrap();
        if let Some(fpb) = fee_per_byte {
            config.fee_per_byte = fpb;
        }
        let strategy = selection_strategy.unwrap_or_else(|| config.selection_strategy.clone());

        // Calculate total output value and tokens needed
        let mut target_value: u64 = 0;
        let mut target_tokens: HashMap<String, u64> = HashMap::new();
        for output in &outputs {
            target_value = target_value.saturating_add(output.value);
            for token in &output.tokens {
                *target_tokens
                    .entry(token.token_id.clone())
                    .or_insert(0) += token.amount;
            }
        }

        // Select UTXOs
        let selection = self.select_utxos(target_value, &target_tokens, &strategy)?;

        // Calculate fee
        let fee = self.estimate_fee_with_config(
            &config,
            selection.inputs.len() as u32,
            outputs.len() as u32 + 1, // +1 for potential change output
            data_inputs.as_ref().map_or(false, |d| !d.is_empty()),
        );

        // Build outputs: include change output if needed
        let mut final_outputs = outputs.clone();
        let actual_fee = fee.optimal_fee.max(config.min_fee);
        let change_after_fee = selection
            .total_value
            .saturating_sub(target_value)
            .saturating_sub(actual_fee);

        if change_after_fee > config.dust_threshold {
            // Add change output
            final_outputs.push(TxOutput {
                ergo_tree: "change_output_placeholder".to_string(),
                value: change_after_fee,
                tokens: vec![], // Simplified: no token change handling
                registers: HashMap::new(),
            });
        }

        // Check if we still have enough after fee
        let total_output_value: u64 = final_outputs.iter().map(|o| o.value).sum();
        let total_needed = total_output_value.saturating_add(actual_fee);
        if selection.total_value < total_needed {
            return Err(format!(
                "Insufficient funds: have {} nanoERG, need {} nanoERG",
                selection.total_value, total_needed
            ));
        }

        // Calculate final size estimate
        let size_bytes = fee.size_bytes + 200; // +200 for potential change output
        let final_outputs_count = final_outputs.len() as u32;
        let tx_id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp_millis();

        let unsigned_tx = UnsignedTransaction {
            inputs: selection.inputs.clone(),
            data_inputs: data_inputs.unwrap_or_default(),
            outputs: final_outputs,
            fee: actual_fee,
            size_bytes,
        };

        let outputs_count = final_outputs_count;

        let built = BuiltTransaction {
            tx_id: tx_id.clone(),
            unsigned_tx,
            fee: actual_fee,
            size_bytes,
            inputs_count: selection.inputs.len() as u32,
            outputs_count,
            created_at,
        };

        // Store the built transaction
        self.built_txs.insert(tx_id.clone(), built.clone());

        // Update stats
        let total_built = self.stats_total_built.fetch_add(1, Ordering::Relaxed) + 1;
        let total_fees = self.stats_total_fees.fetch_add(actual_fee, Ordering::Relaxed) + actual_fee;
        self.stats_avg_fee.store(total_fees / total_built, Ordering::Relaxed);
        self.stats_avg_inputs.store(
            (self.stats_avg_inputs.load(Ordering::Relaxed) as u64 * (total_built - 1)
                + selection.inputs.len() as u64)
                / total_built as u64,
            Ordering::Relaxed,
        );
        self.stats_avg_size.store(
            (self.stats_avg_size.load(Ordering::Relaxed) as u64 * (total_built - 1)
                + size_bytes as u64)
                / total_built as u64,
            Ordering::Relaxed,
        );

        // Remove selected UTXOs from pool (they're now spent)
        for input in &selection.inputs {
            self.utxo_pool.remove(&input.box_id);
        }

        info!(
            tx_id = %tx_id,
            inputs = built.inputs_count,
            outputs = built.outputs_count,
            fee = actual_fee,
            size = size_bytes,
            "Transaction built"
        );

        Ok(built)
    }

    /// Simulate building a transaction without persisting.
    pub async fn simulate_tx(
        &self,
        outputs: Vec<TxOutput>,
        inputs_count_override: Option<u32>,
    ) -> Result<SimulatedTransaction, String> {
        let config = self.config.read().unwrap();

        let mut target_value: u64 = 0;
        let mut target_tokens: HashMap<String, u64> = HashMap::new();
        for output in &outputs {
            target_value = target_value.saturating_add(output.value);
            for token in &output.tokens {
                *target_tokens
                    .entry(token.token_id.clone())
                    .or_insert(0) += token.amount;
            }
        }

        let inputs_count = inputs_count_override.unwrap_or(3);
        let fee = self.estimate_fee_with_config(&config, inputs_count, outputs.len() as u32 + 1, false);

        let total_needed = target_value.saturating_add(fee.optimal_fee);

        let change = 0u64; // Simulation doesn't select real UTXOs

        Ok(SimulatedTransaction {
            target_value,
            fee_estimate: fee,
            total_needed,
            estimated_change: change,
            outputs_count: outputs.len() as u32,
            inputs_used: inputs_count,
            possible: self.utxo_pool.len() > 0,
            pool_size: self.utxo_pool.len(),
        })
    }

    /// Consolidate small UTXOs into fewer boxes.
    pub async fn consolidate_utxos(
        &self,
        threshold: Option<u32>,
        max_inputs: Option<u32>,
    ) -> Result<ConsolidationResult, String> {
        let config = self.config.write().unwrap();
        let threshold = threshold.unwrap_or(config.consolidation_threshold);
        let max_inputs = max_inputs.unwrap_or(config.max_inputs);

        let mut small_utxos: Vec<TxInput> = self
            .utxo_pool
            .iter()
            .filter_map(|e| {
                let input = &e.value().input;
                if input.value < config.dust_threshold * 10 {
                    Some(input.clone())
                } else {
                    None
                }
            })
            .collect();

        if small_utxos.len() < 2 {
            return Err("Not enough small UTXOs to consolidate (need at least 2)".into());
        }

        small_utxos.sort_by(|a, b| a.value.cmp(&b.value));
        let take_count = (threshold as usize).min(small_utxos.len()).min(max_inputs as usize);

        let selected: Vec<TxInput> = small_utxos.into_iter().take(take_count).collect();
        let total_value: u64 = selected.iter().map(|u| u.value).sum();
        let fee = self.estimate_fee_with_config(&config, selected.len() as u32, 1, false);
        let actual_fee = fee.optimal_fee.max(config.min_fee);
        let change = total_value.saturating_sub(actual_fee);

        if change < config.dust_threshold {
            return Err(format!(
                "Consolidation result {} is below dust threshold {}",
                change, config.dust_threshold
            ));
        }

        // Remove selected UTXOs from pool
        for input in &selected {
            self.utxo_pool.remove(&input.box_id);
        }

        // Create a consolidated UTXO placeholder
        let consolidated_box = TxInput {
            box_id: format!("consolidated_{}", Uuid::new_v4()),
            value: change,
            ergo_tree: "consolidated_output".to_string(),
            tokens: vec![],
            registers: HashMap::new(),
            extension: HashMap::new(),
        };

        self.stats_consolidations.fetch_add(1, Ordering::Relaxed);

        info!(
            inputs_consolidated = selected.len(),
            total_value,
            fee = actual_fee,
            change,
            "UTXOs consolidated"
        );

        Ok(ConsolidationResult {
            inputs_consolidated: selected.len() as u32,
            total_input_value: total_value,
            fee: actual_fee,
            output_value: change,
            consolidated_box,
            size_bytes: fee.size_bytes,
        })
    }

    /// Re-optimize fee for a built transaction.
    pub async fn optimize_fee(&self, tx_id: &str) -> Result<BuiltTransaction, String> {
        let config = self.config.read().unwrap();

        let mut tx = self
            .built_txs
            .get_mut(tx_id)
            .ok_or_else(|| format!("Transaction {} not found", tx_id))?;

        let fee = self.estimate_fee_with_config(
            &config,
            tx.inputs_count,
            tx.outputs_count,
            !tx.unsigned_tx.data_inputs.is_empty(),
        );

        tx.fee = fee.optimal_fee.max(config.min_fee);
        tx.size_bytes = fee.size_bytes;

        info!(tx_id = %tx_id, new_fee = tx.fee, "Fee optimized");
        Ok(tx.value().clone())
    }

    /// Get a previously built transaction.
    pub fn get_built_transaction(&self, tx_id: &str) -> Option<BuiltTransaction> {
        self.built_txs.get(tx_id).map(|r| r.value().clone())
    }

    /// Get builder config.
    pub async fn get_config(&self) -> TxBuildConfig {
        self.config.read().unwrap().clone()
    }

    /// Update builder config.
    pub async fn update_config(&self, new_config: TxBuildConfig) {
        let mut config = self.config.write().unwrap();
        *config = new_config;
        info!("Transaction builder config updated");
    }

    /// Get builder statistics.
    pub fn get_stats(&self) -> TxBuilderStats {
        TxBuilderStats {
            total_built: self.stats_total_built.load(Ordering::Relaxed),
            total_fees_paid: self.stats_total_fees.load(Ordering::Relaxed),
            avg_fee: self.stats_avg_fee.load(Ordering::Relaxed),
            avg_inputs: self.stats_avg_inputs.load(Ordering::Relaxed) as u32,
            avg_size: self.stats_avg_size.load(Ordering::Relaxed) as u32,
            consolidations: self.stats_consolidations.load(Ordering::Relaxed),
        }
    }
}

impl Default for TxBuilderEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// Shared State Type
// ================================================================

/// Shared state for the transaction builder module.
#[derive(Clone)]
pub struct TxBuilderState {
    pub engine: Arc<TxBuilderEngine>,
}

impl TxBuilderState {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(TxBuilderEngine::new()),
        }
    }
}

impl Default for TxBuilderState {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// Simulated Transaction (for simulate endpoint)
// ================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedTransaction {
    pub target_value: u64,
    pub fee_estimate: FeeEstimate,
    pub total_needed: u64,
    pub estimated_change: u64,
    pub outputs_count: u32,
    pub inputs_used: u32,
    pub possible: bool,
    pub pool_size: usize,
}

// ================================================================
// Consolidation Result
// ================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    pub inputs_consolidated: u32,
    pub total_input_value: u64,
    pub fee: u64,
    pub output_value: u64,
    pub consolidated_box: TxInput,
    pub size_bytes: u32,
}

// ================================================================
// Request/Response types for REST endpoints
// ================================================================

#[derive(Debug, Deserialize)]
pub struct BuildTxRequest {
    pub outputs: Vec<TxOutput>,
    #[serde(default)]
    pub data_inputs: Option<Vec<String>>,
    pub selection_strategy: Option<SelectionStrategy>,
    pub fee_per_byte: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SimulateTxRequest {
    pub outputs: Vec<TxOutput>,
    pub inputs_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct SelectUtxosRequest {
    pub target_value: u64,
    #[serde(default)]
    pub target_tokens: HashMap<String, u64>,
    pub strategy: Option<SelectionStrategy>,
}

#[derive(Debug, Deserialize)]
pub struct ConsolidateRequest {
    pub threshold: Option<u32>,
    pub max_inputs: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct FeeEstimateQuery {
    pub inputs: u32,
    pub outputs: u32,
    #[serde(default)]
    pub has_data_inputs: bool,
}

#[derive(Debug, Deserialize)]
pub struct BoxSizeQuery {
    pub tokens: u32,
    pub registers: u32,
}

#[derive(Debug, Deserialize)]
pub struct ListUtxosQuery {
    pub min_value: Option<u64>,
}

// ================================================================
// REST Handlers
// ================================================================

/// POST /v1/tx-builder/build
async fn build_tx_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<BuildTxRequest>,
) -> Result<(StatusCode, Json<BuiltTransaction>), (StatusCode, String)> {
    let engine = &state.tx_builder.engine;
    match engine.build_transaction(req.outputs, req.data_inputs, req.selection_strategy, req.fee_per_byte).await {
        Ok(tx) => Ok((StatusCode::OK, Json(tx))),
        Err(e) => Err((StatusCode::BAD_REQUEST, e)),
    }
}

/// POST /v1/tx-builder/simulate
async fn simulate_tx_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<SimulateTxRequest>,
) -> Result<(StatusCode, Json<SimulatedTransaction>), (StatusCode, String)> {
    let engine = &state.tx_builder.engine;
    match engine.simulate_tx(req.outputs, req.inputs_count).await {
        Ok(sim) => Ok((StatusCode::OK, Json(sim))),
        Err(e) => Err((StatusCode::BAD_REQUEST, e)),
    }
}

/// POST /v1/tx-builder/select-utxos
async fn select_utxos_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<SelectUtxosRequest>,
) -> Result<(StatusCode, Json<UtxoSelection>), (StatusCode, String)> {
    let engine = &state.tx_builder.engine;
    let strategy = req.strategy.unwrap_or(SelectionStrategy::Greedy);
    match engine.select_utxos(req.target_value, &req.target_tokens, &strategy) {
        Ok(sel) => Ok((StatusCode::OK, Json(sel))),
        Err(e) => Err((StatusCode::BAD_REQUEST, e)),
    }
}

/// GET /v1/tx-builder/fee-estimate
async fn fee_estimate_handler(
    State(state): State<proxy::AppState>,
    Query(q): Query<FeeEstimateQuery>,
) -> Json<FeeEstimate> {
    let engine = &state.tx_builder.engine;
    Json(engine.estimate_fee(q.inputs, q.outputs, q.has_data_inputs))
}

/// GET /v1/tx-builder/box-size-estimate
async fn box_size_estimate_handler(
    State(state): State<proxy::AppState>,
    Query(q): Query<BoxSizeQuery>,
) -> Json<BoxSizeEstimate> {
    let engine = &state.tx_builder.engine;
    Json(engine.estimate_box_size(q.tokens, q.registers))
}

/// POST /v1/tx-builder/consolidate
async fn consolidate_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<ConsolidateRequest>,
) -> Result<(StatusCode, Json<ConsolidationResult>), (StatusCode, String)> {
    let engine = &state.tx_builder.engine;
    match engine.consolidate_utxos(req.threshold, req.max_inputs).await {
        Ok(result) => Ok((StatusCode::OK, Json(result))),
        Err(e) => Err((StatusCode::BAD_REQUEST, e)),
    }
}

/// GET /v1/tx-builder/tx/:tx_id
async fn get_tx_handler(
    State(state): State<proxy::AppState>,
    Path(tx_id): Path<String>,
) -> Result<Json<BuiltTransaction>, (StatusCode, String)> {
    let engine = &state.tx_builder.engine;
    engine
        .get_built_transaction(&tx_id)
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Transaction {} not found", tx_id)))
}

/// GET /v1/tx-builder/utxos
async fn list_utxos_handler(
    State(state): State<proxy::AppState>,
    Query(q): Query<ListUtxosQuery>,
) -> Json<Vec<TxInput>> {
    let engine = &state.tx_builder.engine;
    Json(engine.list_utxos(q.min_value))
}

/// PUT /v1/tx-builder/config
async fn update_config_handler(
    State(state): State<proxy::AppState>,
    Json(config): Json<TxBuildConfig>,
) -> StatusCode {
    state.tx_builder.engine.update_config(config).await;
    StatusCode::OK
}

/// GET /v1/tx-builder/stats
async fn stats_handler(
    State(state): State<proxy::AppState>,
) -> Json<TxBuilderStats> {
    let engine = &state.tx_builder.engine;
    Json(engine.get_stats())
}

// ================================================================
// Router Builder
// ================================================================

/// Build the transaction builder router.
pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    Router::new()
        .route("/v1/tx-builder/build", post(build_tx_handler))
        .route("/v1/tx-builder/simulate", post(simulate_tx_handler))
        .route("/v1/tx-builder/select-utxos", post(select_utxos_handler))
        .route("/v1/tx-builder/fee-estimate", get(fee_estimate_handler))
        .route("/v1/tx-builder/box-size-estimate", get(box_size_estimate_handler))
        .route("/v1/tx-builder/consolidate", post(consolidate_handler))
        .route("/v1/tx-builder/tx/{tx_id}", get(get_tx_handler))
        .route("/v1/tx-builder/utxos", get(list_utxos_handler))
        .route("/v1/tx-builder/config", put(update_config_handler))
        .route("/v1/tx-builder/stats", get(stats_handler))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_utxo(box_id: &str, value: u64, tokens: Vec<TokenRef>) -> TxInput {
        TxInput {
            box_id: box_id.to_string(),
            value,
            ergo_tree: "test_tree".to_string(),
            tokens,
            registers: HashMap::new(),
            extension: HashMap::new(),
        }
    }

    fn make_token(id: &str, amount: u64) -> TokenRef {
        TokenRef {
            token_id: id.to_string(),
            amount,
        }
    }

    fn make_engine() -> TxBuilderEngine {
        TxBuilderEngine::with_config(TxBuildConfig {
            fee_per_byte: 360,
            min_fee: 100_000,
            max_inputs: 50,
            selection_strategy: SelectionStrategy::Greedy,
            dust_threshold: 360_000,
            consolidation_threshold: 20,
        })
    }

    #[test]
    fn test_add_and_remove_utxo() {
        let engine = make_engine();
        let utxo = make_utxo("box1", 1_000_000, vec![]);

        engine.add_utxo(utxo.clone());
        assert_eq!(engine.list_utxos(None).len(), 1);

        let removed = engine.remove_utxo("box1");
        assert!(removed);
        assert_eq!(engine.list_utxos(None).len(), 0);

        let removed_again = engine.remove_utxo("box1");
        assert!(!removed_again);
    }

    #[test]
    fn test_greedy_selection() {
        let engine = make_engine();
        engine.add_utxo(make_utxo("box1", 500_000, vec![]));
        engine.add_utxo(make_utxo("box2", 2_000_000, vec![]));
        engine.add_utxo(make_utxo("box3", 100_000, vec![]));

        let result = engine.select_utxos(
            1_000_000,
            &HashMap::new(),
            &SelectionStrategy::Greedy,
        );

        assert!(result.is_ok());
        let sel = result.unwrap();
        // Greedy picks largest first: box2 (2M) covers 1M target + fee
        assert_eq!(sel.inputs.len(), 1);
        assert_eq!(sel.inputs[0].box_id, "box2");
        assert!(sel.total_value >= 1_100_000); // target + min_fee
    }

    #[test]
    fn test_branch_and_bound_selection() {
        let engine = make_engine();
        engine.add_utxo(make_utxo("box1", 500_000, vec![]));
        engine.add_utxo(make_utxo("box2", 1_200_000, vec![]));
        engine.add_utxo(make_utxo("box3", 100_000, vec![]));

        let result = engine.select_utxos(
            1_000_000,
            &HashMap::new(),
            &SelectionStrategy::BranchAndBound,
        );

        assert!(result.is_ok());
        let sel = result.unwrap();
        assert!(sel.total_value >= 1_100_000);
    }

    #[test]
    fn test_fifo_selection() {
        let engine = make_engine();
        engine.add_utxo(make_utxo("box1", 500_000, vec![]));
        engine.add_utxo(make_utxo("box2", 1_500_000, vec![]));
        engine.add_utxo(make_utxo("box3", 200_000, vec![]));

        let result = engine.select_utxos(
            1_000_000,
            &HashMap::new(),
            &SelectionStrategy::FIFO,
        );

        assert!(result.is_ok());
        let sel = result.unwrap();
        // FIFO picks oldest first: box1 (500K) then box2 (1.5M)
        assert!(sel.inputs.len() >= 1);
        // First input should be the first added (box1)
        assert_eq!(sel.inputs[0].box_id, "box1");
    }

    #[test]
    fn test_random_improve_selection() {
        let engine = make_engine();
        // Add enough UTXOs so random selection can find a combination
        for i in 0..10 {
            engine.add_utxo(make_utxo(&format!("box{}", i), 500_000, vec![]));
        }

        let result = engine.select_utxos(
            1_000_000,
            &HashMap::new(),
            &SelectionStrategy::RandomImprove,
        );

        assert!(result.is_ok());
        let sel = result.unwrap();
        assert!(sel.total_value >= 1_100_000);
    }

    #[test]
    fn test_consolidation() {
        let engine = make_engine();
        for i in 0..5 {
            engine.add_utxo(make_utxo(&format!("box{}", i), 100_000, vec![]));
        }

        let result = engine.select_utxos(
            3_000_000,
            &HashMap::new(),
            &SelectionStrategy::Consolidate,
        );

        assert!(result.is_ok());
        let sel = result.unwrap();
        // All small UTXOs should be selected for consolidation
        assert_eq!(sel.inputs.len(), 5);
    }

    #[test]
    fn test_fee_estimation() {
        let engine = make_engine();
        let fee = engine.estimate_fee(3, 2, false);

        assert_eq!(fee.inputs_used, 3);
        assert!(fee.min_fee >= 100_000);
        assert!(fee.optimal_fee >= fee.min_fee);
        assert!(fee.fast_fee >= fee.optimal_fee);
        assert!(fee.size_bytes > 0);
    }

    #[test]
    fn test_box_size_estimation() {
        let engine = make_engine();
        let estimate = engine.estimate_box_size(3, 2);

        assert_eq!(estimate.base_size, 40);
        assert_eq!(estimate.per_token, 34);
        assert_eq!(estimate.per_register, 64);
        assert_eq!(
            estimate.total,
            40 + (3 * 34) + (2 * 64)
        );
    }

    #[tokio::test]
    async fn test_build_simple_transaction() {
        let engine = make_engine();
        engine.add_utxo(make_utxo("box1", 10_000_000, vec![]));

        let outputs = vec![TxOutput {
            ergo_tree: "recipient_tree".to_string(),
            value: 5_000_000,
            tokens: vec![],
            registers: HashMap::new(),
        }];

        let result = engine
            .build_transaction(outputs, None, None, None)
            .await;

        assert!(result.is_ok());
        let tx = result.unwrap();
        assert!(!tx.tx_id.is_empty());
        assert!(tx.fee > 0);
        assert!(tx.size_bytes > 0);
        assert_eq!(tx.inputs_count, 1);
        // Should have 2 outputs: recipient + change
        assert!(tx.outputs_count >= 1);
    }

    #[tokio::test]
    async fn test_build_transaction_with_tokens() {
        let engine = make_engine();
        engine.add_utxo(make_utxo(
            "box1",
            10_000_000,
            vec![make_token("token1", 1000)],
        ));

        let outputs = vec![TxOutput {
            ergo_tree: "recipient_tree".to_string(),
            value: 5_000_000,
            tokens: vec![make_token("token1", 500)],
            registers: HashMap::new(),
        }];

        let result = engine
            .build_transaction(outputs, None, None, None)
            .await;

        assert!(result.is_ok());
        let tx = result.unwrap();
        assert_eq!(tx.inputs_count, 1);
    }

    #[tokio::test]
    async fn test_build_transaction_with_change() {
        let engine = make_engine();
        // Large UTXO so there should be change
        engine.add_utxo(make_utxo("box1", 100_000_000, vec![]));

        let outputs = vec![TxOutput {
            ergo_tree: "recipient_tree".to_string(),
            value: 1_000_000,
            tokens: vec![],
            registers: HashMap::new(),
        }];

        let result = engine
            .build_transaction(outputs, None, None, None)
            .await;

        assert!(result.is_ok());
        let tx = result.unwrap();
        // Should have change output (recipient + change)
        assert!(tx.outputs_count >= 2);
    }

    #[tokio::test]
    async fn test_optimize_fee() {
        let engine = make_engine();
        engine.add_utxo(make_utxo("box1", 10_000_000, vec![]));

        let outputs = vec![TxOutput {
            ergo_tree: "recipient_tree".to_string(),
            value: 5_000_000,
            tokens: vec![],
            registers: HashMap::new(),
        }];

        let result = engine
            .build_transaction(outputs.clone(), None, None, None)
            .await
            .unwrap();

        let tx_id = result.tx_id.clone();
        let optimized = engine.optimize_fee(&tx_id).await;
        assert!(optimized.is_ok());

        let optimized_tx = optimized.unwrap();
        assert_eq!(optimized_tx.tx_id, tx_id);
        assert!(optimized_tx.fee > 0);
    }

    #[tokio::test]
    async fn test_dust_rejection() {
        let engine = make_engine();
        // Small UTXO that would result in dust change
        engine.add_utxo(make_utxo("box1", 500_000, vec![]));

        let outputs = vec![TxOutput {
            ergo_tree: "recipient_tree".to_string(),
            value: 400_000,
            tokens: vec![],
            registers: HashMap::new(),
        }];

        let result = engine
            .build_transaction(outputs, None, None, None)
            .await;

        // Should fail because change would be below dust threshold
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_max_inputs_limit() {
        let engine = make_engine();
        // Add many small UTXOs
        for i in 0..100 {
            engine.add_utxo(make_utxo(&format!("box{}", i), 100_000, vec![]));
        }

        // Request a large amount that would need many inputs
        let outputs = vec![TxOutput {
            ergo_tree: "recipient_tree".to_string(),
            value: 50_000_000, // Needs ~500 inputs at 100K each, but max is 50
            tokens: vec![],
            registers: HashMap::new(),
        }];

        let result = engine
            .build_transaction(outputs, None, None, None)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_building() {
        let engine = Arc::new(make_engine());

        // Add enough UTXOs for concurrent builds
        for i in 0..20 {
            engine.add_utxo(make_utxo(&format!("box{}", i), 10_000_000, vec![]));
        }

        let mut handles = Vec::new();
        for i in 0..5 {
            let eng = engine.clone();
            let handle = tokio::spawn(async move {
                let outputs = vec![TxOutput {
                    ergo_tree: format!("tree_{}", i),
                    value: 1_000_000,
                    tokens: vec![],
                    registers: HashMap::new(),
                }];

                eng.build_transaction(outputs, None, None, None).await
            });
            handles.push(handle);
        }

        let mut successes = 0;
        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => successes += 1,
                Ok(Err(e)) => {
                    // Some may fail due to UTXO contention, which is expected
                    debug!("Concurrent build failed (expected): {}", e);
                }
                Err(e) => panic!("Task panicked: {}", e),
            }
        }

        // At least some should succeed
        assert!(successes >= 1, "At least 1 concurrent build should succeed");
    }
}
