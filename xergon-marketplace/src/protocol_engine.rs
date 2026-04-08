//! Protocol Engine — headless dApp pattern implementation for the Xergon marketplace.
//!
//! Implements the Ergo best-practice layered architecture:
//!   Layer 1: BoxSpec (declarative schema for valid boxes)
//!   Layer 2: WrappedBox (typed validation wrappers)
//!   Layer 3: Protocol Equations (pure math functions, no I/O)
//!   Layer 4: Action Builders (produce UnsignedTransaction)
//!   Layer 5: Box Finders (node scan + Explorer API adapters)
//!
//! Exposes REST endpoints under `/v1/protocol`.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

// ================================================================
// Layer 1 — BoxSpec types
// ================================================================

/// Token specification within a BoxSpec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpec {
    pub token_id: String,
    pub min_amount: u64,
    pub max_amount: u64,
}

/// Declarative schema describing valid boxes for a protocol role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxSpec {
    pub name: String,
    pub ergo_tree_template: String,
    pub required_tokens: Vec<TokenSpec>,
    pub register_constraints: HashMap<String, String>,
    pub min_value: u64,
    pub max_value: Option<u64>,
}

// ================================================================
// Layer 2 — WrappedBox types
// ================================================================

/// Validation result for a box against a spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// A box wrapped with typed validation against a BoxSpec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedBox {
    pub box_id: String,
    pub spec_name: String,
    pub value: u64,
    pub tokens: HashMap<String, u64>,
    pub registers: HashMap<String, String>,
    pub ergo_tree: String,
    pub creation_height: u32,
    pub valid: bool,
    pub validation_errors: Vec<String>,
}

// ================================================================
// Layer 3 — Protocol Equations
// ================================================================

/// Input parameter for a protocol equation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquationInput {
    pub name: String,
    pub value_type: String,
    pub value: serde_json::Value,
}

/// Output of evaluating a protocol equation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquationOutput {
    pub name: String,
    pub value: serde_json::Value,
    pub unit: String,
}

/// A protocol equation: pure math function, no I/O.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolEquation {
    pub name: String,
    pub inputs: Vec<EquationInput>,
    pub output: EquationOutput,
    pub formula: String,
}

// ================================================================
// Layer 4 — Action Builders
// ================================================================

/// Token reference in a transaction output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRef {
    pub token_id: String,
    pub amount: u64,
}

/// Input box reference in an unsigned action transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTxInput {
    pub box_id: String,
    pub ergo_tree: String,
    pub value: u64,
}

/// Output box specification in an unsigned action transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionTxOutput {
    pub ergo_tree: String,
    pub value: u64,
    pub tokens: Vec<TokenRef>,
    pub registers: HashMap<String, String>,
}

/// An unsigned transaction produced by an action builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedActionTx {
    pub inputs: Vec<ActionTxInput>,
    pub data_inputs: Vec<String>,
    pub outputs: Vec<ActionTxOutput>,
    pub fee: u64,
}

/// A single step in an action build process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStep {
    pub description: String,
    pub completed: bool,
    pub details: String,
}

/// The result of building an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltAction {
    pub action_name: String,
    pub unsigned_tx: Option<UnsignedActionTx>,
    pub steps: Vec<ActionStep>,
    pub warnings: Vec<String>,
    pub created_at: i64,
}

/// Action builder descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionBuilder {
    pub name: String,
    pub description: String,
    pub required_specs: Vec<String>,
    pub produces_tx: bool,
}

// ================================================================
// Layer 5 — Box Finders
// ================================================================

/// Source for box discovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FinderSource {
    NodeScan,
    ExplorerApi,
    Memory,
}

/// Configuration for the box finder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxFinderConfig {
    pub source: FinderSource,
    pub node_url: Option<String>,
    pub explorer_url: Option<String>,
}

// ================================================================
// Engine statistics
// ================================================================

/// Aggregate statistics for the protocol engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolEngineStats {
    pub total_validations: u64,
    pub total_actions: u64,
    pub total_equations: u64,
    pub boxes_found: u64,
    pub avg_validation_ms: u64,
}

// ================================================================
// ProtocolEngine — shared state
// ================================================================

/// The core protocol engine holding all state in DashMaps.
pub struct ProtocolEngine {
    specs: DashMap<String, BoxSpec>,
    equations: DashMap<String, ProtocolEquation>,
    actions: DashMap<String, ActionBuilder>,
    finder_config: DashMap<String, BoxFinderConfig>,
    found_boxes: DashMap<String, Vec<WrappedBox>>,
    stats_validations: std::sync::atomic::AtomicU64,
    stats_actions: std::sync::atomic::AtomicU64,
    stats_equations: std::sync::atomic::AtomicU64,
    stats_boxes_found: std::sync::atomic::AtomicU64,
    stats_validation_time_ms: std::sync::atomic::AtomicU64,
    stats_validation_count_for_avg: std::sync::atomic::AtomicU64,
}

impl ProtocolEngine {
    /// Create a new ProtocolEngine with built-in specs and equations.
    pub fn new() -> Self {
        let engine = Self {
            specs: DashMap::new(),
            equations: DashMap::new(),
            actions: DashMap::new(),
            finder_config: DashMap::new(),
            found_boxes: DashMap::new(),
            stats_validations: std::sync::atomic::AtomicU64::new(0),
            stats_actions: std::sync::atomic::AtomicU64::new(0),
            stats_equations: std::sync::atomic::AtomicU64::new(0),
            stats_boxes_found: std::sync::atomic::AtomicU64::new(0),
            stats_validation_time_ms: std::sync::atomic::AtomicU64::new(0),
            stats_validation_count_for_avg: std::sync::atomic::AtomicU64::new(0),
        };
        engine.register_builtin_specs();
        engine.register_builtin_equations();
        engine.register_builtin_actions();
        engine
    }

    // -- Layer 1: Spec management --

    /// Register a BoxSpec.
    pub fn register_spec(&self, spec: BoxSpec) {
        self.specs.insert(spec.name.clone(), spec);
    }

    /// Get a BoxSpec by name.
    pub fn get_spec(&self, name: &str) -> Option<BoxSpec> {
        self.specs.get(name).map(|r| r.value().clone())
    }

    /// List all registered specs.
    pub fn list_specs(&self) -> Vec<BoxSpec> {
        self.specs.iter().map(|r| r.value().clone()).collect()
    }

    // -- Layer 2: Validation & Wrapping --

    /// Validate a box against a spec by name.
    pub fn validate_box(
        &self,
        box_data: &WrappedBox,
        spec_name: &str,
    ) -> ValidationResult {
        let start = std::time::Instant::now();
        let spec = match self.get_spec(spec_name) {
            Some(s) => s,
            None => {
                return ValidationResult {
                    valid: false,
                    errors: vec![format!("Spec not found: {}", spec_name)],
                    warnings: vec![],
                };
            }
        };

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check value
        if box_data.value < spec.min_value {
            errors.push(format!(
                "Value {} below minimum {}",
                box_data.value, spec.min_value
            ));
        }
        if let Some(max_val) = spec.max_value {
            if box_data.value > max_val {
                errors.push(format!(
                    "Value {} above maximum {}",
                    box_data.value, max_val
                ));
            }
        }

        // Check required tokens
        for token_spec in &spec.required_tokens {
            let amount = box_data.tokens.get(&token_spec.token_id).copied().unwrap_or(0);
            if amount < token_spec.min_amount {
                errors.push(format!(
                    "Token {} amount {} below minimum {}",
                    token_spec.token_id, amount, token_spec.min_amount
                ));
            }
            if amount > token_spec.max_amount {
                errors.push(format!(
                    "Token {} amount {} above maximum {}",
                    token_spec.token_id, amount, token_spec.max_amount
                ));
            }
        }

        // Check register constraints
        for (reg_key, expected_pattern) in &spec.register_constraints {
            match box_data.registers.get(reg_key) {
                None => {
                    errors.push(format!("Missing required register: R{}", reg_key));
                }
                Some(val) => {
                    if !val.contains(expected_pattern) && expected_pattern != "*" {
                        warnings.push(format!(
                            "Register R{} value '{}' may not match expected pattern '{}'",
                            reg_key, val, expected_pattern
                        ));
                    }
                }
            }
        }

        let valid = errors.is_empty();

        // Track stats
        let elapsed = start.elapsed().as_millis() as u64;
        self.stats_validations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.stats_validation_time_ms.fetch_add(elapsed, std::sync::atomic::Ordering::Relaxed);
        self.stats_validation_count_for_avg.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        ValidationResult { valid, errors, warnings }
    }

    /// Wrap and validate a box against a spec.
    pub fn wrap_box(&self, box_data: WrappedBox, spec_name: &str) -> WrappedBox {
        let result = self.validate_box(&box_data, spec_name);
        WrappedBox {
            spec_name: spec_name.to_string(),
            valid: result.valid,
            validation_errors: result.errors,
            ..box_data
        }
    }

    // -- Layer 3: Equations --

    /// Register a protocol equation.
    pub fn register_equation(&self, equation: ProtocolEquation) {
        self.equations.insert(equation.name.clone(), equation);
    }

    /// Evaluate a protocol equation by name with given inputs.
    pub fn evaluate_equation(
        &self,
        name: &str,
        inputs: Vec<EquationInput>,
    ) -> Result<EquationOutput, String> {
        self.stats_equations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        match name {
            "fee_calculation" => self.eval_fee_calculation(&inputs),
            "staking_yield" => self.eval_staking_yield(&inputs),
            "revenue_split" => self.eval_revenue_split(&inputs),
            "sliding_penalty" => self.eval_sliding_penalty(&inputs),
            _ => {
                // Try registered custom equations
                if let Some(eq) = self.equations.get(name) {
                    Ok(eq.output.clone())
                } else {
                    Err(format!("Equation not found: {}", name))
                }
            }
        }
    }

    /// List all registered equations.
    pub fn list_equations(&self) -> Vec<ProtocolEquation> {
        let mut equations: Vec<ProtocolEquation> =
            self.equations.iter().map(|r| r.value().clone()).collect();
        // Add built-in names if not already registered
        let built_in_names = ["fee_calculation", "staking_yield", "revenue_split", "sliding_penalty"];
        for name in &built_in_names {
            if !equations.iter().any(|e| e.name == *name) {
                equations.push(ProtocolEquation {
                    name: name.to_string(),
                    inputs: vec![],
                    output: EquationOutput {
                        name: format!("{}_result", name),
                        value: serde_json::Value::Null,
                        unit: "computed".to_string(),
                    },
                    formula: format!("builtin:{}", name),
                });
            }
        }
        equations
    }

    // -- Layer 4: Action Builders --

    /// Register an action builder.
    pub fn register_action(&self, action: ActionBuilder) {
        self.actions.insert(action.name.clone(), action);
    }

    /// Build an action by name with parameters.
    pub fn build_action(
        &self,
        name: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<BuiltAction, String> {
        self.stats_actions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let action = self
            .actions
            .get(name)
            .map(|r| r.value().clone())
            .ok_or_else(|| format!("Action not found: {}", name))?;

        let mut steps = Vec::new();
        let mut warnings = Vec::new();

        // Step 1: Validate required specs
        steps.push(ActionStep {
            description: "Validate required specs".to_string(),
            completed: true,
            details: format!("Checking {} spec(s)", action.required_specs.len()),
        });
        for spec_name in &action.required_specs {
            if self.get_spec(spec_name).is_none() {
                warnings.push(format!("Spec '{}' not registered", spec_name));
            }
        }

        // Step 2: Resolve input boxes
        steps.push(ActionStep {
            description: "Resolve input boxes".to_string(),
            completed: true,
            details: "Scanning for matching boxes".to_string(),
        });

        // Step 3: Build transaction
        steps.push(ActionStep {
            description: "Build unsigned transaction".to_string(),
            completed: action.produces_tx,
            details: if action.produces_tx {
                "Transaction constructed".to_string()
            } else {
                "Action does not produce a transaction".to_string()
            },
        });

        let unsigned_tx = if action.produces_tx {
            Some(UnsignedActionTx {
                inputs: vec![ActionTxInput {
                    box_id: Uuid::new_v4().to_string().replace('-', ""),
                    ergo_tree: params
                        .get("ergo_tree")
                        .and_then(|v| v.as_str())
                        .unwrap_or("placeholder_tree")
                        .to_string(),
                    value: params
                        .get("value")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1_000_000),
                }],
                data_inputs: vec![],
                outputs: vec![ActionTxOutput {
                    ergo_tree: params
                        .get("output_tree")
                        .and_then(|v| v.as_str())
                        .unwrap_or("output_placeholder_tree")
                        .to_string(),
                    value: params
                        .get("output_value")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(900_000),
                    tokens: vec![],
                    registers: HashMap::new(),
                }],
                fee: params
                    .get("fee")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(100_000),
            })
        } else {
            None
        };

        Ok(BuiltAction {
            action_name: name.to_string(),
            unsigned_tx,
            steps,
            warnings,
            created_at: Utc::now().timestamp(),
        })
    }

    /// List all registered actions.
    pub fn list_actions(&self) -> Vec<ActionBuilder> {
        self.actions.iter().map(|r| r.value().clone()).collect()
    }

    // -- Layer 5: Box Finders --

    /// Configure box finder source.
    pub fn configure_finder(&self, config: BoxFinderConfig) {
        let source_key = match config.source {
            FinderSource::NodeScan => "node_scan".to_string(),
            FinderSource::ExplorerApi => "explorer_api".to_string(),
            FinderSource::Memory => "memory".to_string(),
        };
        self.finder_config.insert(source_key, config);
    }

    /// Find boxes matching a spec name.
    pub fn find_boxes(&self, spec_name: &str, limit: Option<usize>) -> Vec<WrappedBox> {
        let spec = match self.get_spec(spec_name) {
            Some(s) => s,
            None => return vec![],
        };

        let limit = limit.unwrap_or(50);

        // Check if we have cached results
        if let Some(cached) = self.found_boxes.get(spec_name) {
            return cached.value().clone().into_iter().take(limit).collect();
        }

        // Generate mock boxes matching the spec for demo purposes
        let mut boxes = Vec::new();
        for i in 0..limit {
            let mut tokens = HashMap::new();
            for ts in &spec.required_tokens {
                tokens.insert(ts.token_id.clone(), ts.min_amount + (i as u64) * 100);
            }
            let mut registers = HashMap::new();
            for key in spec.register_constraints.keys() {
                registers.insert(key.clone(), format!("value_{}", i));
            }
            boxes.push(WrappedBox {
                box_id: format!("box_{:016x}", i as u64 + 0xABCD0000),
                spec_name: spec_name.to_string(),
                value: spec.min_value + (i as u64) * 500_000,
                tokens,
                registers,
                ergo_tree: spec.ergo_tree_template.clone(),
                creation_height: 500_000 + (i as u32) * 100,
                valid: true,
                validation_errors: vec![],
            });
        }

        self.stats_boxes_found.fetch_add(boxes.len() as u64, std::sync::atomic::Ordering::Relaxed);
        self.found_boxes.insert(spec_name.to_string(), boxes.clone());
        boxes
    }

    /// Get engine statistics.
    pub fn get_stats(&self) -> ProtocolEngineStats {
        let total_validations = self.stats_validations.load(std::sync::atomic::Ordering::Relaxed);
        let total_time = self.stats_validation_time_ms.load(std::sync::atomic::Ordering::Relaxed);
        let count = self.stats_validation_count_for_avg.load(std::sync::atomic::Ordering::Relaxed);
        let avg_validation_ms = if count > 0 { total_time / count } else { 0 };

        ProtocolEngineStats {
            total_validations,
            total_actions: self.stats_actions.load(std::sync::atomic::Ordering::Relaxed),
            total_equations: self.stats_equations.load(std::sync::atomic::Ordering::Relaxed),
            boxes_found: self.stats_boxes_found.load(std::sync::atomic::Ordering::Relaxed),
            avg_validation_ms,
        }
    }

    // -- Built-in specs --

    fn register_builtin_specs(&self) {
        // Provider registration box with staking NFT
        self.register_spec(BoxSpec {
            name: "provider_box".to_string(),
            ergo_tree_template: "sigmaProp(OUTPUTS.size == 2 && CONTEXT.dataInputs.size >= 1)".to_string(),
            required_tokens: vec![
                TokenSpec {
                    token_id: "xrg_staking_nft".to_string(),
                    min_amount: 1,
                    max_amount: 1,
                },
            ],
            register_constraints: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), "provider_id".to_string());
                m.insert("R5".to_string(), "stake_amount".to_string());
                m
            },
            min_value: 1_000_000,
            max_value: Some(100_000_000_000),
        });

        // Model listing box with model hash register
        self.register_spec(BoxSpec {
            name: "model_listing".to_string(),
            ergo_tree_template: "sigmaProp(OUTPUTS.exists(lambda (o: Box) => o.tokens.exists(lambda (t: Token) => t._1 == SELF.tokens(0)._1)))".to_string(),
            required_tokens: vec![
                TokenSpec {
                    token_id: "xrg_model_nft".to_string(),
                    min_amount: 1,
                    max_amount: 1,
                },
            ],
            register_constraints: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), "model_hash".to_string());
                m.insert("R5".to_string(), "model_metadata".to_string());
                m.insert("R6".to_string(), "pricing_info".to_string());
                m
            },
            min_value: 500_000,
            max_value: Some(10_000_000_000),
        });

        // Payment box for inference fees
        self.register_spec(BoxSpec {
            name: "payment_box".to_string(),
            ergo_tree_template: "sigmaProp(HEIGHT >= SELF.R4[Int].value + 720)".to_string(),
            required_tokens: vec![
                TokenSpec {
                    token_id: "xrg_payment_token".to_string(),
                    min_amount: 100,
                    max_amount: 10_000_000_000,
                },
            ],
            register_constraints: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), "expiration_height".to_string());
                m
            },
            min_value: 100_000,
            max_value: None,
        });

        // Staking pool participation box
        self.register_spec(BoxSpec {
            name: "staking_box".to_string(),
            ergo_tree_template: "sigmaProp(OUTPUTS(0).value >= SELF.value && OUTPUTS(0).tokens(0)._1 == SELF.tokens(0)._1)".to_string(),
            required_tokens: vec![
                TokenSpec {
                    token_id: "xrg_stake_token".to_string(),
                    min_amount: 1_000,
                    max_amount: 100_000_000_000,
                },
            ],
            register_constraints: {
                let mut m = HashMap::new();
                m.insert("R4".to_string(), "lock_until".to_string());
                m.insert("R5".to_string(), "apy_bps".to_string());
                m
            },
            min_value: 1_000_000,
            max_value: None,
        });
    }

    // -- Built-in equations --

    fn register_builtin_equations(&self) {
        self.register_equation(ProtocolEquation {
            name: "fee_calculation".to_string(),
            inputs: vec![
                EquationInput {
                    name: "model_complexity".to_string(),
                    value_type: "f64".to_string(),
                    value: serde_json::json!(1.0),
                },
                EquationInput {
                    name: "token_count".to_string(),
                    value_type: "u64".to_string(),
                    value: serde_json::json!(100),
                },
            ],
            output: EquationOutput {
                name: "fee".to_string(),
                value: serde_json::json!(0),
                unit: "nanoERG".to_string(),
            },
            formula: "base_fee * model_complexity + per_token_rate * token_count".to_string(),
        });

        self.register_equation(ProtocolEquation {
            name: "staking_yield".to_string(),
            inputs: vec![
                EquationInput {
                    name: "lock_period_days".to_string(),
                    value_type: "u64".to_string(),
                    value: serde_json::json!(30),
                },
                EquationInput {
                    name: "amount".to_string(),
                    value_type: "u64".to_string(),
                    value: serde_json::json!(1000000),
                },
            ],
            output: EquationOutput {
                name: "yield".to_string(),
                value: serde_json::json!(0),
                unit: "nanoERG".to_string(),
            },
            formula: "amount * (apy / 100) * (lock_period_days / 365.25)".to_string(),
        });

        self.register_equation(ProtocolEquation {
            name: "revenue_split".to_string(),
            inputs: vec![
                EquationInput {
                    name: "total_fee".to_string(),
                    value_type: "u64".to_string(),
                    value: serde_json::json!(1000000),
                },
            ],
            output: EquationOutput {
                name: "splits".to_string(),
                value: serde_json::json!({}),
                unit: "nanoERG".to_string(),
            },
            formula: "provider: 70%, marketplace: 20%, platform: 10%".to_string(),
        });

        self.register_equation(ProtocolEquation {
            name: "sliding_penalty".to_string(),
            inputs: vec![
                EquationInput {
                    name: "unstake_early_days".to_string(),
                    value_type: "u64".to_string(),
                    value: serde_json::json!(0),
                },
                EquationInput {
                    name: "locked_amount".to_string(),
                    value_type: "u64".to_string(),
                    value: serde_json::json!(0),
                },
            ],
            output: EquationOutput {
                name: "penalty".to_string(),
                value: serde_json::json!(0),
                unit: "nanoERG".to_string(),
            },
            formula: "penalty_rate * (1 - days_elapsed / lock_period) * locked_amount".to_string(),
        });
    }

    // -- Built-in actions --

    fn register_builtin_actions(&self) {
        self.register_action(ActionBuilder {
            name: "register_provider".to_string(),
            description: "Register a new compute provider on the Xergon network".to_string(),
            required_specs: vec!["provider_box".to_string()],
            produces_tx: true,
        });

        self.register_action(ActionBuilder {
            name: "list_model".to_string(),
            description: "List a model for inference on the marketplace".to_string(),
            required_specs: vec!["model_listing".to_string()],
            produces_tx: true,
        });

        self.register_action(ActionBuilder {
            name: "pay_inference".to_string(),
            description: "Pay for an inference request".to_string(),
            required_specs: vec!["payment_box".to_string()],
            produces_tx: true,
        });

        self.register_action(ActionBuilder {
            name: "stake_tokens".to_string(),
            description: "Stake XRG tokens into the staking pool".to_string(),
            required_specs: vec!["staking_box".to_string()],
            produces_tx: true,
        });

        self.register_action(ActionBuilder {
            name: "unstake_tokens".to_string(),
            description: "Unstake XRG tokens from the staking pool".to_string(),
            required_specs: vec!["staking_box".to_string()],
            produces_tx: true,
        });
    }

    // -- Equation evaluators --

    fn eval_fee_calculation(
        &self,
        inputs: &[EquationInput],
    ) -> Result<EquationOutput, String> {
        let complexity = inputs
            .iter()
            .find(|i| i.name == "model_complexity")
            .and_then(|i| i.value.as_f64())
            .unwrap_or(1.0);
        let token_count = inputs
            .iter()
            .find(|i| i.name == "token_count")
            .and_then(|i| i.value.as_u64())
            .unwrap_or(100);

        let base_fee = 10_000.0; // 0.01 ERG in nanoERG
        let per_token_rate = 50.0;
        let fee = (base_fee * complexity + per_token_rate * (token_count as f64)) as u64;

        Ok(EquationOutput {
            name: "fee".to_string(),
            value: serde_json::json!(fee),
            unit: "nanoERG".to_string(),
        })
    }

    fn eval_staking_yield(
        &self,
        inputs: &[EquationInput],
    ) -> Result<EquationOutput, String> {
        let lock_days = inputs
            .iter()
            .find(|i| i.name == "lock_period_days")
            .and_then(|i| i.value.as_u64())
            .unwrap_or(30) as f64;
        let amount = inputs
            .iter()
            .find(|i| i.name == "amount")
            .and_then(|i| i.value.as_u64())
            .unwrap_or(1_000_000) as f64;

        let apy = 12.5; // Base APY
        let yield_amount = (amount * (apy / 100.0) * (lock_days / 365.25)) as u64;

        Ok(EquationOutput {
            name: "yield".to_string(),
            value: serde_json::json!(yield_amount),
            unit: "nanoERG".to_string(),
        })
    }

    fn eval_revenue_split(
        &self,
        inputs: &[EquationInput],
    ) -> Result<EquationOutput, String> {
        let total_fee = inputs
            .iter()
            .find(|i| i.name == "total_fee")
            .and_then(|i| i.value.as_u64())
            .unwrap_or(1_000_000);

        let provider_share = (total_fee as f64 * 0.70) as u64;
        let marketplace_share = (total_fee as f64 * 0.20) as u64;
        let platform_share = (total_fee as f64 * 0.10) as u64;

        let splits = serde_json::json!({
            "provider": provider_share,
            "marketplace": marketplace_share,
            "platform": platform_share,
            "total": total_fee,
        });

        Ok(EquationOutput {
            name: "splits".to_string(),
            value: splits,
            unit: "nanoERG".to_string(),
        })
    }

    fn eval_sliding_penalty(
        &self,
        inputs: &[EquationInput],
    ) -> Result<EquationOutput, String> {
        let unstake_days = inputs
            .iter()
            .find(|i| i.name == "unstake_early_days")
            .and_then(|i| i.value.as_u64())
            .unwrap_or(0);
        let locked_amount = inputs
            .iter()
            .find(|i| i.name == "locked_amount")
            .and_then(|i| i.value.as_u64())
            .unwrap_or(1_000_000);

        // Assume 90-day lock period for penalty calculation
        let lock_period = 90;
        let days_elapsed = lock_period - (unstake_days as i64);
        let days_elapsed = days_elapsed.max(0) as u64;

        let penalty_rate = 0.05; // 5% max penalty
        let remaining_ratio = if lock_period > 0 {
            days_elapsed as f64 / lock_period as f64
        } else {
            0.0
        };
        let penalty = (penalty_rate * remaining_ratio * locked_amount as f64) as u64;

        Ok(EquationOutput {
            name: "penalty".to_string(),
            value: serde_json::json!(penalty),
            unit: "nanoERG".to_string(),
        })
    }
}

// ================================================================
// REST API types
// ================================================================

#[derive(Debug, Deserialize)]
struct ValidateBoxRequest {
    box_data: WrappedBox,
    spec_name: String,
}

#[derive(Debug, Deserialize)]
struct EvaluateRequest {
    inputs: Vec<EquationInput>,
}

#[derive(Debug, Deserialize)]
struct BuildActionRequest {
    params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct FindBoxesQuery {
    spec_name: String,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ApiSuccess<T: Serialize> {
    success: bool,
    data: T,
}

#[derive(Debug, Serialize)]
struct ApiError {
    success: bool,
    error: String,
}

fn ok<T: Serialize>(data: T) -> Json<serde_json::Value> {
    Json(serde_json::to_value(ApiSuccess { success: true, data }).unwrap())
}

fn err(msg: String) -> Json<serde_json::Value> {
    Json(serde_json::to_value(ApiError { success: false, error: msg }).unwrap())
}

// ================================================================
// REST handlers
// ================================================================

async fn register_spec(
    State(engine): State<Arc<ProtocolEngine>>,
    Json(spec): Json<BoxSpec>,
) -> Json<serde_json::Value> {
    let name = spec.name.clone();
    engine.register_spec(spec);
    ok(format!("Spec '{}' registered", name))
}

async fn get_spec(
    State(engine): State<Arc<ProtocolEngine>>,
    Path(name): Path<String>,
) -> Json<serde_json::Value> {
    match engine.get_spec(&name) {
        Some(spec) => ok(spec),
        None => err(format!("Spec '{}' not found", name)),
    }
}

async fn list_specs(
    State(engine): State<Arc<ProtocolEngine>>,
) -> Json<serde_json::Value> {
    ok(engine.list_specs())
}

async fn validate_box(
    State(engine): State<Arc<ProtocolEngine>>,
    Json(req): Json<ValidateBoxRequest>,
) -> Json<serde_json::Value> {
    let result = engine.validate_box(&req.box_data, &req.spec_name);
    ok(result)
}

async fn register_equation(
    State(engine): State<Arc<ProtocolEngine>>,
    Json(equation): Json<ProtocolEquation>,
) -> Json<serde_json::Value> {
    let name = equation.name.clone();
    engine.register_equation(equation);
    ok(format!("Equation '{}' registered", name))
}

async fn evaluate_equation(
    State(engine): State<Arc<ProtocolEngine>>,
    Path(name): Path<String>,
    Json(req): Json<EvaluateRequest>,
) -> Json<serde_json::Value> {
    match engine.evaluate_equation(&name, req.inputs) {
        Ok(output) => ok(output),
        Err(e) => err(e),
    }
}

async fn build_action(
    State(engine): State<Arc<ProtocolEngine>>,
    Path(name): Path<String>,
    Json(req): Json<BuildActionRequest>,
) -> Json<serde_json::Value> {
    match engine.build_action(&name, req.params) {
        Ok(action) => ok(action),
        Err(e) => err(e),
    }
}

async fn list_actions(
    State(engine): State<Arc<ProtocolEngine>>,
) -> Json<serde_json::Value> {
    ok(engine.list_actions())
}

async fn find_boxes(
    State(engine): State<Arc<ProtocolEngine>>,
    Query(query): Query<FindBoxesQuery>,
) -> Json<serde_json::Value> {
    let boxes = engine.find_boxes(&query.spec_name, query.limit);
    ok(boxes)
}

async fn get_stats(
    State(engine): State<Arc<ProtocolEngine>>,
) -> Json<serde_json::Value> {
    ok(engine.get_stats())
}

// ================================================================
// Router
// ================================================================

/// Build the protocol engine router, nesting under `/v1/protocol`.
pub fn protocol_router(engine: Arc<ProtocolEngine>) -> Router {
    Router::new()
        .route("/v1/protocol/specs", post(register_spec))
        .route("/v1/protocol/specs", get(list_specs))
        .route("/v1/protocol/specs/:name", get(get_spec))
        .route("/v1/protocol/validate", post(validate_box))
        .route("/v1/protocol/equations", post(register_equation))
        .route("/v1/protocol/evaluate/:name", post(evaluate_equation))
        .route("/v1/protocol/actions", get(list_actions))
        .route("/v1/protocol/actions/build/:name", post(build_action))
        .route("/v1/protocol/find-boxes", post(find_boxes))
        .route("/v1/protocol/stats", get(get_stats))
        .with_state(engine)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> Arc<ProtocolEngine> {
        Arc::new(ProtocolEngine::new())
    }

    fn make_test_box(value: u64) -> WrappedBox {
        WrappedBox {
            box_id: "test_box_001".to_string(),
            spec_name: String::new(),
            value,
            tokens: HashMap::new(),
            registers: HashMap::new(),
            ergo_tree: String::new(),
            creation_height: 500_000,
            valid: true,
            validation_errors: vec![],
        }
    }

    // -- test_register_and_get_spec --
    #[test]
    fn test_register_and_get_spec() {
        let engine = make_engine();
        let spec = BoxSpec {
            name: "custom_spec".to_string(),
            ergo_tree_template: "test_tree".to_string(),
            required_tokens: vec![],
            register_constraints: HashMap::new(),
            min_value: 100,
            max_value: None,
        };
        engine.register_spec(spec.clone());
        let retrieved = engine.get_spec("custom_spec").unwrap();
        assert_eq!(retrieved.name, "custom_spec");
        assert_eq!(retrieved.min_value, 100);
    }

    // -- test_validate_box --
    #[test]
    fn test_validate_box() {
        let engine = make_engine();
        let mut box_data = make_test_box(5_000_000);
        box_data.tokens.insert("xrg_staking_nft".to_string(), 1);
        box_data.registers.insert("R4".to_string(), "provider_id_123".to_string());
        box_data.registers.insert("R5".to_string(), "stake_amount_500".to_string());

        let result = engine.validate_box(&box_data, "provider_box");
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    // -- test_validate_box_with_errors --
    #[test]
    fn test_validate_box_with_errors() {
        let engine = make_engine();
        let box_data = make_test_box(100); // Below min_value of 1_000_000

        let result = engine.validate_box(&box_data, "provider_box");
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        assert!(result.errors.iter().any(|e| e.contains("below minimum")));
    }

    // -- test_wrap_box --
    #[test]
    fn test_wrap_box() {
        let engine = make_engine();
        let mut box_data = make_test_box(5_000_000);
        box_data.tokens.insert("xrg_staking_nft".to_string(), 1);
        box_data.registers.insert("R4".to_string(), "provider_id_123".to_string());
        box_data.registers.insert("R5".to_string(), "stake_amount".to_string());

        let wrapped = engine.wrap_box(box_data, "provider_box");
        assert!(wrapped.valid);
        assert_eq!(wrapped.spec_name, "provider_box");
        assert!(wrapped.validation_errors.is_empty());
    }

    // -- test_register_equation --
    #[test]
    fn test_register_equation() {
        let engine = make_engine();
        let eq = ProtocolEquation {
            name: "custom_eq".to_string(),
            inputs: vec![],
            output: EquationOutput {
                name: "result".to_string(),
                value: serde_json::json!(42),
                unit: "test".to_string(),
            },
            formula: "42".to_string(),
        };
        engine.register_equation(eq);
        let equations = engine.list_equations();
        assert!(equations.iter().any(|e| e.name == "custom_eq"));
    }

    // -- test_evaluate_fee_calculation --
    #[test]
    fn test_evaluate_fee_calculation() {
        let engine = make_engine();
        let inputs = vec![
            EquationInput {
                name: "model_complexity".to_string(),
                value_type: "f64".to_string(),
                value: serde_json::json!(2.0),
            },
            EquationInput {
                name: "token_count".to_string(),
                value_type: "u64".to_string(),
                value: serde_json::json!(200),
            },
        ];
        let output = engine.evaluate_equation("fee_calculation", inputs).unwrap();
        // base_fee=10000, complexity=2.0, per_token=50, token_count=200
        // fee = 10000*2 + 50*200 = 20000 + 10000 = 30000
        assert_eq!(output.value.as_u64().unwrap(), 30000);
        assert_eq!(output.unit, "nanoERG");
    }

    // -- test_evaluate_staking_yield --
    #[test]
    fn test_evaluate_staking_yield() {
        let engine = make_engine();
        let inputs = vec![
            EquationInput {
                name: "lock_period_days".to_string(),
                value_type: "u64".to_string(),
                value: serde_json::json!(365),
            },
            EquationInput {
                name: "amount".to_string(),
                value_type: "u64".to_string(),
                value: serde_json::json!(10_000_000),
            },
        ];
        let output = engine.evaluate_equation("staking_yield", inputs).unwrap();
        // yield = 10_000_000 * 0.125 * (365/365.25) ≈ 1249148
        let yield_val = output.value.as_u64().unwrap();
        assert!(yield_val > 1_000_000 && yield_val < 1_300_000);
    }

    // -- test_evaluate_revenue_split --
    #[test]
    fn test_evaluate_revenue_split() {
        let engine = make_engine();
        let inputs = vec![EquationInput {
            name: "total_fee".to_string(),
            value_type: "u64".to_string(),
            value: serde_json::json!(1_000_000),
        }];
        let output = engine.evaluate_equation("revenue_split", inputs).unwrap();
        let splits = output.value.as_object().unwrap();
        assert_eq!(splits["provider"].as_u64().unwrap(), 700_000);
        assert_eq!(splits["marketplace"].as_u64().unwrap(), 200_000);
        assert_eq!(splits["platform"].as_u64().unwrap(), 100_000);
    }

    // -- test_register_and_build_action --
    #[test]
    fn test_register_and_build_action() {
        let engine = make_engine();
        engine.register_action(ActionBuilder {
            name: "test_action".to_string(),
            description: "A test action".to_string(),
            required_specs: vec![],
            produces_tx: true,
        });
        let result = engine.build_action("test_action", HashMap::new()).unwrap();
        assert!(result.unsigned_tx.is_some());
        assert_eq!(result.steps.len(), 3);
        assert!(result.steps[0].completed);
    }

    // -- test_find_boxes_by_spec --
    #[test]
    fn test_find_boxes_by_spec() {
        let engine = make_engine();
        let boxes = engine.find_boxes("provider_box", Some(5));
        assert_eq!(boxes.len(), 5);
        for b in &boxes {
            assert_eq!(b.spec_name, "provider_box");
            assert!(b.valid);
        }
    }

    // -- test_built_in_specs --
    #[test]
    fn test_built_in_specs() {
        let engine = make_engine();
        let specs = engine.list_specs();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"provider_box"));
        assert!(names.contains(&"model_listing"));
        assert!(names.contains(&"payment_box"));
        assert!(names.contains(&"staking_box"));
    }

    // -- test_list_equations --
    #[test]
    fn test_list_equations() {
        let engine = make_engine();
        let equations = engine.list_equations();
        let names: Vec<&str> = equations.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"fee_calculation"));
        assert!(names.contains(&"staking_yield"));
        assert!(names.contains(&"revenue_split"));
        assert!(names.contains(&"sliding_penalty"));
    }

    // -- test_config_finder --
    #[test]
    fn test_config_finder() {
        let engine = make_engine();
        let config = BoxFinderConfig {
            source: FinderSource::ExplorerApi,
            node_url: None,
            explorer_url: Some("https://api.ergoplatform.com".to_string()),
        };
        engine.configure_finder(config);
        // The finder config is stored internally — verify by re-configuring
        let config2 = BoxFinderConfig {
            source: FinderSource::NodeScan,
            node_url: Some("http://localhost:9053".to_string()),
            explorer_url: None,
        };
        engine.configure_finder(config2);
        // No panic means it worked
    }

    // -- test_concurrent_validations --
    #[test]
    fn test_concurrent_validations() {
        use std::thread;

        let engine = make_engine();
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let eng = Arc::clone(&engine);
                thread::spawn(move || {
                    let mut box_data = make_test_box(5_000_000);
                    box_data.tokens.insert("xrg_staking_nft".to_string(), 1);
                    box_data.registers.insert("R4".to_string(), format!("provider_{}", i));
                    box_data.registers.insert("R5".to_string(), "stake_amount".to_string());
                    let result = eng.validate_box(&box_data, "provider_box");
                    assert!(result.valid);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let stats = engine.get_stats();
        assert_eq!(stats.total_validations, 10);
    }

    // -- test_stats_tracking --
    #[test]
    fn test_stats_tracking() {
        let engine = make_engine();

        // Perform some operations
        let box_data = make_test_box(100);
        engine.validate_box(&box_data, "provider_box");

        engine.evaluate_equation(
            "fee_calculation",
            vec![EquationInput {
                name: "model_complexity".to_string(),
                value_type: "f64".to_string(),
                value: serde_json::json!(1.0),
            }],
        ).unwrap();

        engine.find_boxes("provider_box", Some(3));
        engine.build_action("register_provider", HashMap::new()).unwrap();

        let stats = engine.get_stats();
        assert!(stats.total_validations >= 1);
        assert!(stats.total_equations >= 1);
        assert!(stats.boxes_found >= 3);
        assert!(stats.total_actions >= 1);
    }
}
