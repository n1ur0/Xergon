//! Context Extension Builder for Xergon Agent.
//!
//! Builds and validates ErgoScript CONTEXT variables for contract execution.
//! Provides SELF, INPUTS, OUTPUTS, HEIGHT, dataInputs, and custom extension
//! variable construction. Supports template-based context creation, token
//! coverage verification, and ERG value balance checks.
//!
//! # ErgoScript Context Variables
//!
//! - `SELF` -> the box being spent (first input or contract box)
//! - `INPUTS` -> all input boxes (zero-indexed array)
//! - `OUTPUTS` -> all output boxes (zero-indexed array)
//! - `HEIGHT` -> current block height (Int)
//! - `dataInputs` -> data input boxes (zero-indexed, NOT consumed)
//! - Extension variables: custom key-value pairs attached to spending proofs

use axum::{
    extract::{Path, State},
    Json, Router,
    routing::{get, post, put},
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

// ================================================================
// Enums
// ================================================================

/// ErgoScript value types for context extension variables.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContextValueType {
    Int,
    Long,
    BigInt,
    Byte,
    Boolean,
    String,
    Coll,
    Tuple,
    Box,
    GroupElement,
    Any,
}

impl Default for ContextValueType {
    fn default() -> Self {
        Self::Any
    }
}

impl std::fmt::Display for ContextValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int => write!(f, "Int"),
            Self::Long => write!(f, "Long"),
            Self::BigInt => write!(f, "BigInt"),
            Self::Byte => write!(f, "Byte"),
            Self::Boolean => write!(f, "Boolean"),
            Self::String => write!(f, "String"),
            Self::Coll => write!(f, "Coll"),
            Self::Tuple => write!(f, "Tuple"),
            Self::Box => write!(f, "Box"),
            Self::GroupElement => write!(f, "GroupElement"),
            Self::Any => write!(f, "Any"),
        }
    }
}

// ================================================================
// Data Types
// ================================================================

/// A single context extension variable with type, value, and description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextVariable {
    pub name: String,
    pub value_type: ContextValueType,
    pub value: serde_json::Value,
    pub description: String,
}

/// A token reference (token ID + amount).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TokenRef {
    pub token_id: String,
    pub amount: u64,
}

/// A box representation within a transaction context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxContext {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub tokens: Vec<TokenRef>,
    pub registers: HashMap<String, String>,
    pub creation_height: u32,
    pub extension: HashMap<String, String>,
}

/// Full transaction context for ErgoScript contract execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionContext {
    pub id: String,
    pub self_box: BoxContext,
    pub inputs: Vec<BoxContext>,
    pub outputs: Vec<BoxContext>,
    pub data_inputs: Vec<BoxContext>,
    pub height: u32,
    pub extensions: HashMap<String, ContextVariable>,
    pub created_at: i64,
}

/// Template for a box (used in context templates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxTemplate {
    pub ergo_tree: String,
    pub min_value: u64,
    pub required_tokens: Vec<TokenRef>,
    pub register_defaults: HashMap<String, String>,
}

/// A reusable context template for building transaction contexts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variables: Vec<ContextVariable>,
    pub self_box_template: Option<BoxTemplate>,
    pub created_at: i64,
}

/// Result of validating a transaction context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Configuration for the context builder engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBuilderConfig {
    pub max_inputs: u32,
    pub max_outputs: u32,
    pub max_data_inputs: u32,
    pub max_extensions: u32,
}

impl Default for ContextBuilderConfig {
    fn default() -> Self {
        Self {
            max_inputs: 100,
            max_outputs: 200,
            max_data_inputs: 50,
            max_extensions: 64,
        }
    }
}

/// Statistics for the context builder engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBuilderStats {
    pub total_built: u64,
    pub total_validated: u64,
    pub total_templates: u64,
    pub avg_build_time_ms: u64,
}

// ================================================================
// Request / Response types
// ================================================================

#[derive(Debug, Deserialize)]
pub struct BuildContextRequest {
    pub self_box: BoxContext,
    pub inputs: Vec<BoxContext>,
    pub outputs: Vec<BoxContext>,
    #[serde(default)]
    pub data_inputs: Vec<BoxContext>,
    pub height: u32,
    #[serde(default)]
    pub extensions: HashMap<String, ContextVariable>,
}

#[derive(Debug, Serialize)]
pub struct BuildContextResponse {
    pub context_id: String,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct ValidateContextRequest {
    pub context_id: String,
}

#[derive(Debug, Serialize)]
pub struct ValidateContextResponse {
    pub context_id: String,
    pub validation: ContextValidation,
}

#[derive(Debug, Deserialize)]
pub struct AddBoxRequest {
    pub box_context: BoxContext,
}

#[derive(Debug, Serialize)]
pub struct AddBoxResponse {
    pub context_id: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SetHeightRequest {
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct SetHeightResponse {
    pub context_id: String,
    pub previous_height: u32,
    pub new_height: u32,
}

#[derive(Debug, Deserialize)]
pub struct AddExtensionRequest {
    pub name: String,
    pub value_type: ContextValueType,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct AddExtensionResponse {
    pub context_id: String,
    pub extension_name: String,
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct VerifyTokensResponse {
    pub context_id: String,
    pub valid: bool,
    pub missing_tokens: Vec<TokenRef>,
    pub excess_tokens: Vec<TokenRef>,
}

#[derive(Debug, Serialize)]
pub struct VerifyBalanceResponse {
    pub context_id: String,
    pub valid: bool,
    pub input_total: u64,
    pub output_total: u64,
    pub deficit: i64,
}

#[derive(Debug, Deserialize)]
pub struct SaveTemplateRequest {
    pub name: String,
    pub description: String,
    pub variables: Vec<ContextVariable>,
    #[serde(default)]
    pub self_box_template: Option<BoxTemplate>,
}

#[derive(Debug, Serialize)]
pub struct SaveTemplateResponse {
    pub template_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct BuildFromTemplateRequest {
    #[serde(default)]
    pub overrides: Option<TemplateOverrides>,
}

#[derive(Debug, Deserialize)]
pub struct TemplateOverrides {
    pub self_box: Option<BoxContext>,
    pub inputs: Option<Vec<BoxContext>>,
    pub outputs: Option<Vec<BoxContext>>,
    pub data_inputs: Option<Vec<BoxContext>>,
    pub height: Option<u32>,
    pub extensions: Option<HashMap<String, ContextVariable>>,
}

#[derive(Debug, Serialize)]
pub struct BuildFromTemplateResponse {
    pub context_id: String,
    pub template_id: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct GetContextResponse {
    pub context: Option<TransactionContext>,
}

#[derive(Debug, Serialize)]
pub struct GetTemplateResponse {
    pub template: Option<ContextTemplate>,
}

#[derive(Debug, Serialize)]
pub struct ListTemplatesResponse {
    pub templates: Vec<ContextTemplate>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct GetStatsResponse {
    pub stats: ContextBuilderStats,
}

#[derive(Debug, Serialize)]
pub struct GetConfigResponse {
    pub config: ContextBuilderConfig,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub max_inputs: Option<u32>,
    pub max_outputs: Option<u32>,
    pub max_data_inputs: Option<u32>,
    pub max_extensions: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct UpdateConfigResponse {
    pub success: bool,
    pub config: ContextBuilderConfig,
}

// ================================================================
// ContextBuilderEngine
// ================================================================

/// Core engine for building, validating, and managing ErgoScript transaction contexts.
///
/// Stores built contexts and templates in DashMaps for concurrent access.
/// All state is accessed via `self.state.inner.field` pattern.
pub struct ContextBuilderEngine {
    contexts: DashMap<String, TransactionContext>,
    templates: DashMap<String, ContextTemplate>,
    config: DashMap<String, ContextBuilderConfig>, // single key "config"
    stats_total_built: AtomicU64,
    stats_total_validated: AtomicU64,
    stats_total_templates: AtomicU64,
    stats_build_time_sum: AtomicU64,
    stats_build_count: AtomicU64,
}

impl ContextBuilderEngine {
    /// Create a new context builder engine with default configuration.
    pub fn new() -> Self {
        Self::with_config(ContextBuilderConfig::default())
    }

    /// Create a new context builder engine with custom configuration.
    pub fn with_config(config: ContextBuilderConfig) -> Self {
        let engine = Self {
            contexts: DashMap::new(),
            templates: DashMap::new(),
            config: DashMap::new(),
            stats_total_built: AtomicU64::new(0),
            stats_total_validated: AtomicU64::new(0),
            stats_total_templates: AtomicU64::new(0),
            stats_build_time_sum: AtomicU64::new(0),
            stats_build_count: AtomicU64::new(0),
        };
        engine
            .config
            .insert("config".to_string(), config);
        info!("ContextBuilderEngine initialized");
        engine
    }

    // -- Config management ------------------------------------------------

    fn get_config_value(&self) -> ContextBuilderConfig {
        self.config
            .get("config")
            .map(|c| c.value().clone())
            .unwrap_or_default()
    }

    /// Get the current builder configuration.
    pub fn get_config(&self) -> ContextBuilderConfig {
        self.get_config_value()
    }

    /// Update builder configuration. Only non-None fields are changed.
    pub fn update_config(&self, updates: UpdateConfigRequest) -> ContextBuilderConfig {
        let current = self.get_config_value();
        let updated = ContextBuilderConfig {
            max_inputs: updates.max_inputs.unwrap_or(current.max_inputs),
            max_outputs: updates.max_outputs.unwrap_or(current.max_outputs),
            max_data_inputs: updates.max_data_inputs.unwrap_or(current.max_data_inputs),
            max_extensions: updates.max_extensions.unwrap_or(current.max_extensions),
        };
        self.config
            .insert("config".to_string(), updated.clone());
        info!(?updated, "Context builder config updated");
        updated
    }

    // -- Context building --------------------------------------------------

    /// Build a full transaction context from individual components.
    pub fn build_context(
        &self,
        self_box: BoxContext,
        inputs: Vec<BoxContext>,
        outputs: Vec<BoxContext>,
        data_inputs: Vec<BoxContext>,
        height: u32,
        extensions: HashMap<String, ContextVariable>,
    ) -> Result<TransactionContext, String> {
        let cfg = self.get_config_value();

        // Validate limits
        if inputs.len() as u32 > cfg.max_inputs {
            return Err(format!(
                "Too many inputs: {} exceeds max {}",
                inputs.len(),
                cfg.max_inputs
            ));
        }
        if outputs.len() as u32 > cfg.max_outputs {
            return Err(format!(
                "Too many outputs: {} exceeds max {}",
                outputs.len(),
                cfg.max_outputs
            ));
        }
        if data_inputs.len() as u32 > cfg.max_data_inputs {
            return Err(format!(
                "Too many data inputs: {} exceeds max {}",
                data_inputs.len(),
                cfg.max_data_inputs
            ));
        }
        if extensions.len() as u32 > cfg.max_extensions {
            return Err(format!(
                "Too many extensions: {} exceeds max {}",
                extensions.len(),
                cfg.max_extensions
            ));
        }

        // Validate box IDs
        if self_box.box_id.is_empty() {
            return Err("SELF box ID cannot be empty".to_string());
        }
        if self_box.ergo_tree.is_empty() {
            return Err("SELF box ErgoTree cannot be empty".to_string());
        }

        let start = std::time::Instant::now();
        let context_id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp_millis();

        let tx_context = TransactionContext {
            id: context_id.clone(),
            self_box,
            inputs,
            outputs,
            data_inputs,
            height,
            extensions,
            created_at,
        };

        self.contexts
            .insert(context_id.clone(), tx_context.clone());

        // Update stats
        let elapsed = start.elapsed().as_millis() as u64;
        self.stats_total_built.fetch_add(1, Ordering::Relaxed);
        self.stats_build_time_sum.fetch_add(elapsed, Ordering::Relaxed);
        self.stats_build_count.fetch_add(1, Ordering::Relaxed);

        debug!(context_id = %context_id, elapsed_ms = elapsed, "Transaction context built");
        Ok(tx_context)
    }

    /// Build a SELF box from individual fields.
    pub fn build_self_box(
        &self,
        ergo_tree: &str,
        value: u64,
        tokens: Vec<TokenRef>,
        registers: HashMap<String, String>,
        creation_height: u32,
    ) -> BoxContext {
        let box_id = Uuid::new_v4().to_string();
        BoxContext {
            box_id,
            value,
            ergo_tree: ergo_tree.to_string(),
            tokens,
            registers,
            creation_height,
            extension: HashMap::new(),
        }
    }

    // -- Context mutation --------------------------------------------------

    /// Add an input box to an existing transaction context.
    pub fn add_input(
        &self,
        tx_context_id: &str,
        box_context: BoxContext,
    ) -> Result<(), String> {
        let cfg = self.get_config_value();
        let mut ctx = self
            .contexts
            .get_mut(tx_context_id)
            .ok_or_else(|| format!("Context not found: {}", tx_context_id))?;

        if ctx.inputs.len() as u32 >= cfg.max_inputs {
            return Err(format!(
                "Cannot add input: max inputs ({}) reached",
                cfg.max_inputs
            ));
        }

        ctx.inputs.push(box_context);
        debug!(context_id = %tx_context_id, "Input box added");
        Ok(())
    }

    /// Add an output box to an existing transaction context.
    pub fn add_output(
        &self,
        tx_context_id: &str,
        box_context: BoxContext,
    ) -> Result<(), String> {
        let cfg = self.get_config_value();
        let mut ctx = self
            .contexts
            .get_mut(tx_context_id)
            .ok_or_else(|| format!("Context not found: {}", tx_context_id))?;

        if ctx.outputs.len() as u32 >= cfg.max_outputs {
            return Err(format!(
                "Cannot add output: max outputs ({}) reached",
                cfg.max_outputs
            ));
        }

        ctx.outputs.push(box_context);
        debug!(context_id = %tx_context_id, "Output box added");
        Ok(())
    }

    /// Add a data input box to an existing transaction context.
    pub fn add_data_input(
        &self,
        tx_context_id: &str,
        box_context: BoxContext,
    ) -> Result<(), String> {
        let cfg = self.get_config_value();
        let mut ctx = self
            .contexts
            .get_mut(tx_context_id)
            .ok_or_else(|| format!("Context not found: {}", tx_context_id))?;

        if ctx.data_inputs.len() as u32 >= cfg.max_data_inputs {
            return Err(format!(
                "Cannot add data input: max data inputs ({}) reached",
                cfg.max_data_inputs
            ));
        }

        ctx.data_inputs.push(box_context);
        debug!(context_id = %tx_context_id, "Data input box added");
        Ok(())
    }

    /// Set the block height on an existing transaction context.
    pub fn set_height(&self, tx_context_id: &str, height: u32) -> Result<u32, String> {
        let mut ctx = self
            .contexts
            .get_mut(tx_context_id)
            .ok_or_else(|| format!("Context not found: {}", tx_context_id))?;

        let previous = ctx.height;
        ctx.height = height;
        debug!(
            context_id = %tx_context_id,
            previous,
            new = height,
            "Height updated"
        );
        Ok(previous)
    }

    /// Add a context extension variable to an existing transaction context.
    pub fn add_extension(
        &self,
        tx_context_id: &str,
        name: String,
        value_type: ContextValueType,
        value: serde_json::Value,
    ) -> Result<(), String> {
        let cfg = self.get_config_value();
        let mut ctx = self
            .contexts
            .get_mut(tx_context_id)
            .ok_or_else(|| format!("Context not found: {}", tx_context_id))?;

        if ctx.extensions.len() as u32 >= cfg.max_extensions {
            return Err(format!(
                "Cannot add extension: max extensions ({}) reached",
                cfg.max_extensions
            ));
        }

        if name.is_empty() {
            return Err("Extension name cannot be empty".to_string());
        }

        let var = ContextVariable {
            name: name.clone(),
            value_type,
            value,
            description: String::new(),
        };
        ctx.extensions.insert(name.clone(), var);
        debug!(context_id = %tx_context_id, extension_name = %name, "Extension added");
        Ok(())
    }

    // -- Context retrieval -------------------------------------------------

    /// Get a built context by ID.
    pub fn get_context(&self, id: &str) -> Option<TransactionContext> {
        self.contexts.get(id).map(|c| c.value().clone())
    }

    // -- Validation --------------------------------------------------------

    /// Validate a transaction context.
    pub fn validate_context(&self, tx_context: &TransactionContext) -> ContextValidation {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check SELF box
        if tx_context.self_box.box_id.is_empty() {
            errors.push("SELF box ID is empty".to_string());
        }
        if tx_context.self_box.ergo_tree.is_empty() {
            errors.push("SELF box ErgoTree is empty".to_string());
        }
        if tx_context.self_box.value == 0 {
            errors.push("SELF box value is zero".to_string());
        }
        if tx_context.self_box.value < 1_000_000 {
            warnings.push("SELF box value is below minimum (1,000,000 nanoERG)".to_string());
        }

        // Check inputs
        for (i, input) in tx_context.inputs.iter().enumerate() {
            if input.box_id.is_empty() {
                errors.push(format!("Input[{}] has empty box ID", i));
            }
            if input.ergo_tree.is_empty() {
                errors.push(format!("Input[{}] has empty ErgoTree", i));
            }
            if input.value == 0 {
                warnings.push(format!("Input[{}] has zero value", i));
            }
        }

        // Check outputs
        for (i, output) in tx_context.outputs.iter().enumerate() {
            if output.box_id.is_empty() {
                errors.push(format!("Output[{}] has empty box ID", i));
            }
            if output.ergo_tree.is_empty() {
                errors.push(format!("Output[{}] has empty ErgoTree", i));
            }
        }

        // Check data inputs
        for (i, di) in tx_context.data_inputs.iter().enumerate() {
            if di.box_id.is_empty() {
                errors.push(format!("dataInputs[{}] has empty box ID", i));
            }
        }

        // Check height
        if tx_context.height == 0 {
            errors.push("HEIGHT is zero".to_string());
        }

        // Check for duplicate box IDs across inputs
        let mut input_ids: Vec<&str> = tx_context.inputs.iter().map(|b| b.box_id.as_str()).collect();
        input_ids.push(&tx_context.self_box.box_id);
        let mut seen = std::collections::HashSet::new();
        for id in &input_ids {
            if !seen.insert(*id) {
                errors.push(format!("Duplicate input box ID: {}", id));
            }
        }

        // Check for duplicate box IDs across outputs
        let mut output_seen = std::collections::HashSet::new();
        for output in &tx_context.outputs {
            if !output_seen.insert(&output.box_id) {
                warnings.push(format!("Duplicate output box ID: {}", output.box_id));
            }
        }

        self.stats_total_validated.fetch_add(1, Ordering::Relaxed);

        let valid = errors.is_empty();
        ContextValidation {
            valid,
            errors,
            warnings,
        }
    }

    /// Verify that tokens in outputs are covered by inputs (including SELF).
    pub fn verify_token_coverage(&self, tx_context: &TransactionContext) -> Vec<TokenRef> {
        // Aggregate all input tokens (SELF + inputs)
        let mut input_tokens: HashMap<String, u64> = HashMap::new();
        for token in &tx_context.self_box.tokens {
            *input_tokens
                .entry(token.token_id.clone())
                .or_insert(0) += token.amount;
        }
        for input in &tx_context.inputs {
            for token in &input.tokens {
                *input_tokens
                    .entry(token.token_id.clone())
                    .or_insert(0) += token.amount;
            }
        }

        // Aggregate all output tokens
        let mut output_tokens: HashMap<String, u64> = HashMap::new();
        for output in &tx_context.outputs {
            for token in &output.tokens {
                *output_tokens
                    .entry(token.token_id.clone())
                    .or_insert(0) += token.amount;
            }
        }

        // Find tokens in outputs not covered by inputs
        let mut missing = Vec::new();
        for (token_id, output_amount) in &output_tokens {
            let input_amount = input_tokens.get(token_id).copied().unwrap_or(0);
            if output_amount > &input_amount {
                missing.push(TokenRef {
                    token_id: token_id.clone(),
                    amount: output_amount - input_amount,
                });
            }
        }

        missing
    }

    /// Verify ERG value balance (inputs >= outputs + minimum fee).
    pub fn verify_value_balance(&self, tx_context: &TransactionContext) -> (bool, u64, u64, i64) {
        const MIN_FEE: u64 = 1_000_000; // 0.001 ERG minimum fee

        let input_total: u64 = tx_context.self_box.value
            + tx_context
                .inputs
                .iter()
                .map(|i| i.value)
                .sum::<u64>();

        let output_total: u64 = tx_context
            .outputs
            .iter()
            .map(|o| o.value)
            .sum::<u64>();

        let deficit = (input_total as i64) - (output_total as i64) - (MIN_FEE as i64);
        let valid = deficit >= 0;

        (valid, input_total, output_total, deficit)
    }

    // -- Template management -----------------------------------------------

    /// Save a context template.
    pub fn save_template(&self, mut template: ContextTemplate) -> ContextTemplate {
        if template.id.is_empty() {
            template.id = Uuid::new_v4().to_string();
        }
        if template.created_at == 0 {
            template.created_at = Utc::now().timestamp_millis();
        }
        let id = template.id.clone();
        self.templates.insert(id.clone(), template.clone());
        self.stats_total_templates.fetch_add(1, Ordering::Relaxed);
        info!(template_id = %id, "Context template saved");
        template
    }

    /// Get a template by ID.
    pub fn get_template(&self, id: &str) -> Option<ContextTemplate> {
        self.templates.get(id).map(|t| t.value().clone())
    }

    /// List all saved templates.
    pub fn list_templates(&self) -> Vec<ContextTemplate> {
        self.templates
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Build a context from a template with optional overrides.
    pub fn build_from_template(
        &self,
        template_id: &str,
        overrides: Option<TemplateOverrides>,
    ) -> Result<TransactionContext, String> {
        let template = self
            .templates
            .get(template_id)
            .ok_or_else(|| format!("Template not found: {}", template_id))?;

        let template = template.value();

        // Build SELF box from template or override
        let self_box = if let Some(ref overrides) = overrides {
            overrides
                .self_box
                .clone()
                .or_else(|| {
                    template.self_box_template.as_ref().map(|tmpl| {
                        let mut ext = HashMap::new();
                        if let Some(ref vars) = overrides.extensions {
                            for (k, v) in vars {
                                ext.insert(
                                    k.clone(),
                                    serde_json::to_string(&v.value).unwrap_or_default(),
                                );
                            }
                        }
                        BoxContext {
                            box_id: Uuid::new_v4().to_string(),
                            value: tmpl.min_value,
                            ergo_tree: tmpl.ergo_tree.clone(),
                            tokens: tmpl.required_tokens.clone(),
                            registers: tmpl.register_defaults.clone(),
                            creation_height: 0,
                            extension: ext,
                        }
                    })
                })
                .ok_or_else(|| "No SELF box provided and no template SELF box template".to_string())?
        } else {
            template
                .self_box_template
                .as_ref()
                .map(|tmpl| {
                    let mut ext = HashMap::new();
                    for var in &template.variables {
                        ext.insert(
                            var.name.clone(),
                            serde_json::to_string(&var.value).unwrap_or_default(),
                        );
                    }
                    BoxContext {
                        box_id: Uuid::new_v4().to_string(),
                        value: tmpl.min_value,
                        ergo_tree: tmpl.ergo_tree.clone(),
                        tokens: tmpl.required_tokens.clone(),
                        registers: tmpl.register_defaults.clone(),
                        creation_height: 0,
                        extension: ext,
                    }
                })
                .ok_or_else(|| "No SELF box template defined and no overrides provided".to_string())?
        };

        let inputs = overrides
            .as_ref()
            .and_then(|o| o.inputs.clone())
            .unwrap_or_default();
        let outputs = overrides
            .as_ref()
            .and_then(|o| o.outputs.clone())
            .unwrap_or_default();
        let data_inputs = overrides
            .as_ref()
            .and_then(|o| o.data_inputs.clone())
            .unwrap_or_default();
        let height = overrides
            .as_ref()
            .and_then(|o| o.height)
            .unwrap_or(0);

        // Merge template variables with overrides
        let mut extensions: HashMap<String, ContextVariable> = HashMap::new();
        for var in &template.variables {
            extensions.insert(var.name.clone(), var.clone());
        }
        if let Some(ref overrides) = overrides {
            if let Some(ref ext_overrides) = overrides.extensions {
                for (name, var) in ext_overrides {
                    extensions.insert(name.clone(), var.clone());
                }
            }
        }

        let tx_context = self.build_context(
            self_box,
            inputs,
            outputs,
            data_inputs,
            height,
            extensions,
        )?;

        info!(
            context_id = %tx_context.id,
            template_id = %template_id,
            "Context built from template"
        );
        Ok(tx_context)
    }

    // -- Statistics --------------------------------------------------------

    /// Get builder statistics.
    pub fn get_stats(&self) -> ContextBuilderStats {
        let total_built = self.stats_total_built.load(Ordering::Relaxed);
        let total_validated = self.stats_total_validated.load(Ordering::Relaxed);
        let total_templates = self.stats_total_templates.load(Ordering::Relaxed);
        let build_time_sum = self.stats_build_time_sum.load(Ordering::Relaxed);
        let build_count = self.stats_build_count.load(Ordering::Relaxed);
        let avg_build_time_ms = if build_count > 0 {
            build_time_sum / build_count
        } else {
            0
        };

        ContextBuilderStats {
            total_built,
            total_validated,
            total_templates,
            avg_build_time_ms,
        }
    }
}

// ================================================================
// REST Handlers
// ================================================================

/// POST /v1/context/build - Build a transaction context
async fn handle_build_context(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Json(req): Json<BuildContextRequest>,
) -> Json<serde_json::Value> {
    match engine.build_context(
        req.self_box,
        req.inputs,
        req.outputs,
        req.data_inputs,
        req.height,
        req.extensions,
    ) {
        Ok(ctx) => Json(serde_json::json!({
            "context_id": ctx.id,
            "created_at": ctx.created_at,
        })),
        Err(e) => Json(serde_json::json!({
            "error": e,
        })),
    }
}

/// POST /v1/context/validate - Validate a context
async fn handle_validate_context(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Json(req): Json<ValidateContextRequest>,
) -> Json<serde_json::Value> {
    match engine.get_context(&req.context_id) {
        Some(ctx) => {
            let validation = engine.validate_context(&ctx);
            Json(serde_json::json!({
                "context_id": ctx.id,
                "validation": validation,
            }))
        }
        None => Json(serde_json::json!({
            "error": format!("Context not found: {}", req.context_id),
        })),
    }
}

/// GET /v1/context/:id - Get context
async fn handle_get_context(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match engine.get_context(&id) {
        Some(ctx) => Json(serde_json::json!({
            "context": ctx,
        })),
        None => Json(serde_json::json!({
            "context": null,
        })),
    }
}

/// POST /v1/context/:id/input - Add input box
async fn handle_add_input(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
    Json(req): Json<AddBoxRequest>,
) -> Json<serde_json::Value> {
    match engine.add_input(&id, req.box_context) {
        Ok(()) => Json(serde_json::json!({
            "context_id": id,
            "success": true,
            "message": "Input box added",
        })),
        Err(e) => Json(serde_json::json!({
            "context_id": id,
            "success": false,
            "error": e,
        })),
    }
}

/// POST /v1/context/:id/output - Add output box
async fn handle_add_output(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
    Json(req): Json<AddBoxRequest>,
) -> Json<serde_json::Value> {
    match engine.add_output(&id, req.box_context) {
        Ok(()) => Json(serde_json::json!({
            "context_id": id,
            "success": true,
            "message": "Output box added",
        })),
        Err(e) => Json(serde_json::json!({
            "context_id": id,
            "success": false,
            "error": e,
        })),
    }
}

/// POST /v1/context/:id/data-input - Add data input box
async fn handle_add_data_input(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
    Json(req): Json<AddBoxRequest>,
) -> Json<serde_json::Value> {
    match engine.add_data_input(&id, req.box_context) {
        Ok(()) => Json(serde_json::json!({
            "context_id": id,
            "success": true,
            "message": "Data input box added",
        })),
        Err(e) => Json(serde_json::json!({
            "context_id": id,
            "success": false,
            "error": e,
        })),
    }
}

/// PUT /v1/context/:id/height - Set height
async fn handle_set_height(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
    Json(req): Json<SetHeightRequest>,
) -> Json<serde_json::Value> {
    match engine.set_height(&id, req.height) {
        Ok(previous) => Json(serde_json::json!({
            "context_id": id,
            "previous_height": previous,
            "new_height": req.height,
        })),
        Err(e) => Json(serde_json::json!({
            "context_id": id,
            "error": e,
        })),
    }
}

/// POST /v1/context/:id/extension - Add extension variable
async fn handle_add_extension(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
    Json(req): Json<AddExtensionRequest>,
) -> Json<serde_json::Value> {
    match engine.add_extension(&id, req.name, req.value_type, req.value) {
        Ok(()) => Json(serde_json::json!({
            "context_id": id,
            "extension_name": "added",
            "success": true,
        })),
        Err(e) => Json(serde_json::json!({
            "context_id": id,
            "success": false,
            "error": e,
        })),
    }
}

/// POST /v1/context/verify-tokens/:id - Verify token coverage
async fn handle_verify_tokens(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match engine.get_context(&id) {
        Some(ctx) => {
            let missing = engine.verify_token_coverage(&ctx);
            let valid = missing.is_empty();
            Json(serde_json::json!({
                "context_id": id,
                "valid": valid,
                "missing_tokens": missing,
            }))
        }
        None => Json(serde_json::json!({
            "context_id": id,
            "error": format!("Context not found: {}", id),
        })),
    }
}

/// POST /v1/context/verify-balance/:id - Verify value balance
async fn handle_verify_balance(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match engine.get_context(&id) {
        Some(ctx) => {
            let (valid, input_total, output_total, deficit) =
                engine.verify_value_balance(&ctx);
            Json(serde_json::json!({
                "context_id": id,
                "valid": valid,
                "input_total": input_total,
                "output_total": output_total,
                "deficit": deficit,
            }))
        }
        None => Json(serde_json::json!({
            "context_id": id,
            "error": format!("Context not found: {}", id),
        })),
    }
}

/// POST /v1/context/templates - Save template
async fn handle_save_template(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Json(req): Json<SaveTemplateRequest>,
) -> Json<serde_json::Value> {
    let template = ContextTemplate {
        id: String::new(),
        name: req.name,
        description: req.description,
        variables: req.variables,
        self_box_template: req.self_box_template,
        created_at: 0,
    };
    let saved = engine.save_template(template);
    Json(serde_json::json!({
        "template_id": saved.id,
        "name": saved.name,
    }))
}

/// GET /v1/context/templates - List templates
async fn handle_list_templates(
    State(engine): State<Arc<ContextBuilderEngine>>,
) -> Json<serde_json::Value> {
    let templates = engine.list_templates();
    let count = templates.len();
    Json(serde_json::json!({
        "templates": templates,
        "count": count,
    }))
}

/// POST /v1/context/templates/:id/build - Build from template
async fn handle_build_from_template(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Path(id): Path<String>,
    Json(req): Json<BuildFromTemplateRequest>,
) -> Json<serde_json::Value> {
    match engine.build_from_template(&id, req.overrides) {
        Ok(ctx) => Json(serde_json::json!({
            "context_id": ctx.id,
            "template_id": id,
            "created_at": ctx.created_at,
        })),
        Err(e) => Json(serde_json::json!({
            "template_id": id,
            "error": e,
        })),
    }
}

/// GET /v1/context/stats - Get statistics
async fn handle_get_stats(
    State(engine): State<Arc<ContextBuilderEngine>>,
) -> Json<serde_json::Value> {
    let stats = engine.get_stats();
    Json(serde_json::json!({
        "stats": stats,
    }))
}

/// GET /v1/context/config - Get configuration
async fn handle_get_config(
    State(engine): State<Arc<ContextBuilderEngine>>,
) -> Json<serde_json::Value> {
    let config = engine.get_config();
    Json(serde_json::json!({
        "config": config,
    }))
}

/// PUT /v1/context/config - Update configuration
async fn handle_update_config(
    State(engine): State<Arc<ContextBuilderEngine>>,
    Json(req): Json<UpdateConfigRequest>,
) -> Json<serde_json::Value> {
    let config = engine.update_config(req);
    Json(serde_json::json!({
        "success": true,
        "config": config,
    }))
}

// ================================================================
// Router
// ================================================================

/// Build the axum Router for the context builder module.
pub fn router(state: Arc<ContextBuilderEngine>) -> Router {
    Router::new()
        .route(
            "/v1/context/build",
            post(handle_build_context),
        )
        .route(
            "/v1/context/validate",
            post(handle_validate_context),
        )
        .route(
            "/v1/context/:id",
            get(handle_get_context),
        )
        .route(
            "/v1/context/:id/input",
            post(handle_add_input),
        )
        .route(
            "/v1/context/:id/output",
            post(handle_add_output),
        )
        .route(
            "/v1/context/:id/data-input",
            post(handle_add_data_input),
        )
        .route(
            "/v1/context/:id/height",
            put(handle_set_height),
        )
        .route(
            "/v1/context/:id/extension",
            post(handle_add_extension),
        )
        .route(
            "/v1/context/verify-tokens/:id",
            post(handle_verify_tokens),
        )
        .route(
            "/v1/context/verify-balance/:id",
            post(handle_verify_balance),
        )
        .route(
            "/v1/context/templates",
            post(handle_save_template),
        )
        .route(
            "/v1/context/templates",
            get(handle_list_templates),
        )
        .route(
            "/v1/context/templates/:id/build",
            post(handle_build_from_template),
        )
        .route(
            "/v1/context/stats",
            get(handle_get_stats),
        )
        .route(
            "/v1/context/config",
            get(handle_get_config),
        )
        .route(
            "/v1/context/config",
            put(handle_update_config),
        )
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> ContextBuilderEngine {
        ContextBuilderEngine::new()
    }

    fn make_box(value: u64, tokens: Vec<TokenRef>) -> BoxContext {
        BoxContext {
            box_id: Uuid::new_v4().to_string(),
            value,
            ergo_tree: "100204".to_string(),
            tokens,
            registers: HashMap::new(),
            creation_height: 500_000,
            extension: HashMap::new(),
        }
    }

    fn make_token(id: &str, amount: u64) -> TokenRef {
        TokenRef {
            token_id: id.to_string(),
            amount,
        }
    }

    #[test]
    fn test_build_context() {
        let engine = make_engine();
        let self_box = make_box(10_000_000, vec![]);
        let inputs = vec![make_box(5_000_000, vec![])];
        let outputs = vec![make_box(14_000_000, vec![])];

        let ctx = engine
            .build_context(
                self_box,
                inputs,
                outputs,
                vec![],
                800_000,
                HashMap::new(),
            )
            .unwrap();

        assert!(!ctx.id.is_empty());
        assert_eq!(ctx.height, 800_000);
        assert_eq!(ctx.inputs.len(), 1);
        assert_eq!(ctx.outputs.len(), 1);
        assert!(ctx.created_at > 0);
    }

    #[test]
    fn test_build_self_box() {
        let engine = make_engine();
        let tokens = vec![make_token("token1", 1000)];
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), "0e01".to_string());

        let box_ctx = engine.build_self_box(
            "100204",
            5_000_000,
            tokens.clone(),
            registers.clone(),
            500_000,
        );

        assert!(!box_ctx.box_id.is_empty());
        assert_eq!(box_ctx.value, 5_000_000);
        assert_eq!(box_ctx.ergo_tree, "100204");
        assert_eq!(box_ctx.tokens.len(), 1);
        assert_eq!(box_ctx.tokens[0].amount, 1000);
        assert_eq!(box_ctx.registers.get("R4").unwrap(), "0e01");
        assert_eq!(box_ctx.creation_height, 500_000);
    }

    #[test]
    fn test_add_input_output() {
        let engine = make_engine();
        let self_box = make_box(10_000_000, vec![]);
        let ctx = engine
            .build_context(self_box, vec![], vec![], vec![], 800_000, HashMap::new())
            .unwrap();

        let input = make_box(5_000_000, vec![]);
        engine.add_input(&ctx.id, input).unwrap();

        let output = make_box(14_000_000, vec![]);
        engine.add_output(&ctx.id, output).unwrap();

        let retrieved = engine.get_context(&ctx.id).unwrap();
        assert_eq!(retrieved.inputs.len(), 1);
        assert_eq!(retrieved.outputs.len(), 1);
    }

    #[test]
    fn test_add_data_input() {
        let engine = make_engine();
        let self_box = make_box(10_000_000, vec![]);
        let ctx = engine
            .build_context(self_box, vec![], vec![], vec![], 800_000, HashMap::new())
            .unwrap();

        let data_input = make_box(1_000_000, vec![]);
        engine.add_data_input(&ctx.id, data_input).unwrap();

        let retrieved = engine.get_context(&ctx.id).unwrap();
        assert_eq!(retrieved.data_inputs.len(), 1);
        assert_eq!(retrieved.data_inputs[0].value, 1_000_000);
    }

    #[test]
    fn test_set_height() {
        let engine = make_engine();
        let self_box = make_box(10_000_000, vec![]);
        let ctx = engine
            .build_context(self_box, vec![], vec![], vec![], 800_000, HashMap::new())
            .unwrap();

        let previous = engine.set_height(&ctx.id, 900_000).unwrap();
        assert_eq!(previous, 800_000);

        let retrieved = engine.get_context(&ctx.id).unwrap();
        assert_eq!(retrieved.height, 900_000);
    }

    #[test]
    fn test_add_extension() {
        let engine = make_engine();
        let self_box = make_box(10_000_000, vec![]);
        let ctx = engine
            .build_context(self_box, vec![], vec![], vec![], 800_000, HashMap::new())
            .unwrap();

        engine
            .add_extension(
                &ctx.id,
                "myVar".to_string(),
                ContextValueType::Int,
                serde_json::json!(42),
            )
            .unwrap();

        let retrieved = engine.get_context(&ctx.id).unwrap();
        assert!(retrieved.extensions.contains_key("myVar"));
        let var = &retrieved.extensions["myVar"];
        assert_eq!(var.value_type, ContextValueType::Int);
        assert_eq!(var.value, serde_json::json!(42));
    }

    #[test]
    fn test_validate_context() {
        let engine = make_engine();
        let self_box = make_box(10_000_000, vec![]);
        let inputs = vec![make_box(5_000_000, vec![])];
        let outputs = vec![make_box(14_000_000, vec![])];

        let ctx = engine
            .build_context(
                self_box,
                inputs,
                outputs,
                vec![],
                800_000,
                HashMap::new(),
            )
            .unwrap();

        let validation = engine.validate_context(&ctx);
        assert!(validation.valid);
        assert!(validation.errors.is_empty());
    }

    #[test]
    fn test_verify_token_coverage() {
        let engine = make_engine();
        let token = make_token("tok1", 1000);
        let self_box = make_box(10_000_000, vec![token.clone()]);
        let output = make_box(14_000_000, vec![token.clone()]);

        let ctx = engine
            .build_context(self_box, vec![], vec![output], vec![], 800_000, HashMap::new())
            .unwrap();

        let missing = engine.verify_token_coverage(&ctx);
        assert!(missing.is_empty(), "Expected no missing tokens");
    }

    #[test]
    fn test_verify_token_coverage_missing() {
        let engine = make_engine();
        let token = make_token("tok1", 500);
        let self_box = make_box(10_000_000, vec![token]);
        let output = make_box(14_000_000, vec![make_token("tok1", 1000)]);

        let ctx = engine
            .build_context(self_box, vec![], vec![output], vec![], 800_000, HashMap::new())
            .unwrap();

        let missing = engine.verify_token_coverage(&ctx);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].token_id, "tok1");
        assert_eq!(missing[0].amount, 500);
    }

    #[test]
    fn test_verify_value_balance() {
        let engine = make_engine();
        let self_box = make_box(10_000_000, vec![]);
        let inputs = vec![make_box(5_000_000, vec![])];
        let outputs = vec![make_box(12_000_000, vec![])];

        let ctx = engine
            .build_context(self_box, inputs, outputs, vec![], 800_000, HashMap::new())
            .unwrap();

        let (valid, input_total, output_total, deficit) = engine.verify_value_balance(&ctx);
        // Input: 10M + 5M = 15M. Output: 12M. Min fee: 1M. Deficit: 15M - 12M - 1M = 2M
        assert!(valid);
        assert_eq!(input_total, 15_000_000);
        assert_eq!(output_total, 12_000_000);
        assert_eq!(deficit, 2_000_000);
    }

    #[test]
    fn test_save_and_get_template() {
        let engine = make_engine();
        let template = ContextTemplate {
            id: String::new(),
            name: "test-template".to_string(),
            description: "A test template".to_string(),
            variables: vec![ContextVariable {
                name: "myVar".to_string(),
                value_type: ContextValueType::Int,
                value: serde_json::json!(42),
                description: "Test variable".to_string(),
            }],
            self_box_template: Some(BoxTemplate {
                ergo_tree: "100204".to_string(),
                min_value: 1_000_000,
                required_tokens: vec![],
                register_defaults: HashMap::new(),
            }),
            created_at: 0,
        };

        let saved = engine.save_template(template);
        assert!(!saved.id.is_empty());
        assert_eq!(saved.name, "test-template");

        let retrieved = engine.get_template(&saved.id).unwrap();
        assert_eq!(retrieved.id, saved.id);
        assert_eq!(retrieved.variables.len(), 1);
    }

    #[test]
    fn test_build_from_template() {
        let engine = make_engine();

        let template = ContextTemplate {
            id: String::new(),
            name: "staking-template".to_string(),
            description: "Staking contract template".to_string(),
            variables: vec![],
            self_box_template: Some(BoxTemplate {
                ergo_tree: "100204".to_string(),
                min_value: 10_000_000,
                required_tokens: vec![make_token("stake_token", 1)],
                register_defaults: HashMap::new(),
            }),
            created_at: 0,
        };

        let saved = engine.save_template(template);

        let overrides = TemplateOverrides {
            self_box: None,
            inputs: Some(vec![]),
            outputs: Some(vec![make_box(10_000_000, vec![make_token("stake_token", 1)])]),
            data_inputs: Some(vec![]),
            height: Some(800_000),
            extensions: None,
        };

        let ctx = engine.build_from_template(&saved.id, Some(overrides)).unwrap();
        assert!(!ctx.id.is_empty());
        assert_eq!(ctx.height, 800_000);
        assert_eq!(ctx.self_box.value, 10_000_000);
        assert_eq!(ctx.self_box.tokens.len(), 1);
    }

    #[test]
    fn test_list_templates() {
        let engine = make_engine();

        for i in 0..3 {
            let template = ContextTemplate {
                id: String::new(),
                name: format!("template-{}", i),
                description: format!("Template {}", i),
                variables: vec![],
                self_box_template: None,
                created_at: 0,
            };
            engine.save_template(template);
        }

        let templates = engine.list_templates();
        assert_eq!(templates.len(), 3);
    }

    #[test]
    fn test_concurrent_builds() {
        use std::thread;

        let engine = Arc::new(make_engine());
        let mut handles = Vec::new();

        for i in 0..10 {
            let eng = engine.clone();
            handles.push(thread::spawn(move || {
                let self_box = BoxContext {
                    box_id: Uuid::new_v4().to_string(),
                    value: 10_000_000 + i as u64,
                    ergo_tree: "100204".to_string(),
                    tokens: vec![],
                    registers: HashMap::new(),
                    creation_height: 500_000,
                    extension: HashMap::new(),
                };
                eng.build_context(
                    self_box,
                    vec![],
                    vec![],
                    vec![],
                    800_000,
                    HashMap::new(),
                )
            }));
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, 10);

        let stats = engine.get_stats();
        assert_eq!(stats.total_built, 10);
    }

    #[test]
    fn test_stats_tracking() {
        let engine = make_engine();

        for _ in 0..5 {
            let self_box = make_box(10_000_000, vec![]);
            let ctx = engine
                .build_context(self_box, vec![], vec![], vec![], 800_000, HashMap::new())
                .unwrap();
            engine.validate_context(&ctx);
        }

        let stats = engine.get_stats();
        assert_eq!(stats.total_built, 5);
        assert_eq!(stats.total_validated, 5);

        // Save a template
        let template = ContextTemplate {
            id: String::new(),
            name: "stats-template".to_string(),
            description: "For stats test".to_string(),
            variables: vec![],
            self_box_template: None,
            created_at: 0,
        };
        engine.save_template(template);

        let stats = engine.get_stats();
        assert_eq!(stats.total_templates, 1);
    }
}
