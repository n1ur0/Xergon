//! Proposal templates for standardized governance proposals.
//!
//! Templates define parameterized proposal schemas with type-safe parameter
//! validation, default values, and category metadata. Built-in templates cover
//! common governance actions like protocol parameter changes, provider management,
//! treasury operations, and emergency actions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::governance::types::{GovernanceError, ProposalCategory, ValidationResult};

// ---------------------------------------------------------------------------
// Template parameter types
// ---------------------------------------------------------------------------

/// The type of a template parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateParamType {
    String,
    Integer,
    Float,
    Boolean,
    /// Ergo address.
    Address,
    /// Ergo token ID (hex).
    TokenId,
    /// Percentage value 0-100.
    Percentage,
    /// Duration in blocks.
    Duration,
    /// One of the predefined options.
    Select,
    /// Multiple of the predefined options.
    MultiSelect,
    /// Arbitrary JSON object.
    Json,
}

// ---------------------------------------------------------------------------
// Template parameter definition
// ---------------------------------------------------------------------------

/// A single parameter in a proposal template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    /// Parameter name (used as key in parameter map).
    pub name: String,
    /// Human-readable label.
    pub label: String,
    /// Parameter type.
    pub param_type: TemplateParamType,
    /// Whether this parameter is required.
    pub required: bool,
    /// Human-readable description.
    pub description: String,
    /// Default value (used if not provided).
    pub default: Option<serde_json::Value>,
    /// Minimum value (for numeric types).
    pub min: Option<f64>,
    /// Maximum value (for numeric types).
    pub max: Option<f64>,
    /// Allowed options (for Select/MultiSelect types).
    pub options: Vec<String>,
}

impl Default for TemplateParameter {
    fn default() -> Self {
        Self {
            name: String::new(),
            label: String::new(),
            param_type: TemplateParamType::String,
            required: false,
            description: String::new(),
            default: None,
            min: None,
            max: None,
            options: vec![],
        }
    }
}

impl TemplateParameter {
    /// Validate a value against this parameter's constraints.
    pub fn validate_value(&self, value: &serde_json::Value) -> Result<(), String> {
        match self.param_type {
            TemplateParamType::String => {
                if !value.is_string() {
                    return Err(format!("'{}' must be a string", self.name));
                }
            }
            TemplateParamType::Integer => {
                if !value.is_i64() {
                    return Err(format!("'{}' must be an integer", self.name));
                }
                let v = value.as_i64().unwrap();
                if let Some(min) = self.min {
                    if (v as f64) < min {
                        return Err(format!(
                            "'{}' value {} is below minimum {}",
                            self.name, v, min
                        ));
                    }
                }
                if let Some(max) = self.max {
                    if (v as f64) > max {
                        return Err(format!(
                            "'{}' value {} exceeds maximum {}",
                            self.name, v, max
                        ));
                    }
                }
            }
            TemplateParamType::Float => {
                if !value.is_f64() && !value.is_i64() {
                    return Err(format!("'{}' must be a number", self.name));
                }
                let v = value.as_f64().unwrap();
                if let Some(min) = self.min {
                    if v < min {
                        return Err(format!(
                            "'{}' value {} is below minimum {}",
                            self.name, v, min
                        ));
                    }
                }
                if let Some(max) = self.max {
                    if v > max {
                        return Err(format!(
                            "'{}' value {} exceeds maximum {}",
                            self.name, v, max
                        ));
                    }
                }
            }
            TemplateParamType::Boolean => {
                if !value.is_boolean() {
                    return Err(format!("'{}' must be a boolean", self.name));
                }
            }
            TemplateParamType::Address => {
                if !value.is_string() {
                    return Err(format!("'{}' must be a string (address)", self.name));
                }
                let s = value.as_str().unwrap();
                // Basic Ergo address validation: should start with common prefixes
                if !s.starts_with('3') && !s.starts_with('9') {
                    return Err(format!(
                        "'{}' does not appear to be a valid Ergo address",
                        self.name
                    ));
                }
            }
            TemplateParamType::TokenId => {
                if !value.is_string() {
                    return Err(format!("'{}' must be a string (token ID)", self.name));
                }
            }
            TemplateParamType::Percentage => {
                let v = if value.is_i64() {
                    value.as_i64().unwrap() as f64
                } else if value.is_f64() {
                    value.as_f64().unwrap()
                } else {
                    return Err(format!("'{}' must be a number (percentage)", self.name));
                };
                if v < 0.0 || v > 100.0 {
                    return Err(format!(
                        "'{}' value {} is not in range 0-100",
                        self.name, v
                    ));
                }
                if let Some(min) = self.min {
                    if v < min {
                        return Err(format!(
                            "'{}' value {} is below minimum {}",
                            self.name, v, min
                        ));
                    }
                }
                if let Some(max) = self.max {
                    if v > max {
                        return Err(format!(
                            "'{}' value {} exceeds maximum {}",
                            self.name, v, max
                        ));
                    }
                }
            }
            TemplateParamType::Duration => {
                if !value.is_i64() {
                    return Err(format!("'{}' must be an integer (blocks)", self.name));
                }
                let v = value.as_i64().unwrap();
                if let Some(min) = self.min {
                    if (v as f64) < min {
                        return Err(format!(
                            "'{}' value {} is below minimum {}",
                            self.name, v, min
                        ));
                    }
                }
                if let Some(max) = self.max {
                    if (v as f64) > max {
                        return Err(format!(
                            "'{}' value {} exceeds maximum {}",
                            self.name, v, max
                        ));
                    }
                }
            }
            TemplateParamType::Select => {
                if !value.is_string() {
                    return Err(format!("'{}' must be a string", self.name));
                }
                let s = value.as_str().unwrap();
                if !self.options.is_empty() && !self.options.contains(&s.to_string()) {
                    return Err(format!(
                        "'{}' value '{}' is not one of: {:?}",
                        self.name,
                        s,
                        self.options
                    ));
                }
            }
            TemplateParamType::MultiSelect => {
                if let Some(arr) = value.as_array() {
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            if !self.options.is_empty()
                                && !self.options.contains(&s.to_string())
                            {
                                return Err(format!(
                                    "'{}' option '{}' is not allowed. Allowed: {:?}",
                                    self.name, s, self.options
                                ));
                            }
                        } else {
                            return Err(format!(
                                "'{}' multi-select values must be strings",
                                self.name
                            ));
                        }
                    }
                } else {
                    return Err(format!("'{}' must be an array", self.name));
                }
            }
            TemplateParamType::Json => {
                if !value.is_object() && !value.is_null() {
                    return Err(format!("'{}' must be a JSON object", self.name));
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Proposal template
// ---------------------------------------------------------------------------

/// A proposal template defining a standardized governance action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalTemplate {
    /// Unique template identifier (e.g., "change_fee_rate").
    pub id: String,
    /// Human-readable template name.
    pub name: String,
    /// Proposal category.
    pub category: ProposalCategory,
    /// Template description.
    pub description: String,
    /// Parameter definitions.
    pub parameters: Vec<TemplateParameter>,
    /// Default parameter values.
    pub default_values: HashMap<String, serde_json::Value>,
    /// Whether this template requires stake to create.
    pub requires_stake: bool,
    /// Override the default vote duration (in blocks).
    pub vote_duration_override: Option<u32>,
}

impl Default for ProposalTemplate {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            category: ProposalCategory::ConfigChange,
            description: String::new(),
            parameters: vec![],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        }
    }
}

impl ProposalTemplate {
    /// Validate a set of parameter values against this template.
    pub fn validate_params(
        &self,
        params: &HashMap<String, serde_json::Value>,
    ) -> ValidationResult {
        let mut errors = vec![];

        // Check for required parameters
        for param in &self.parameters {
            if param.required {
                match params.get(&param.name) {
                    None => {
                        if param.default.is_none() {
                            errors.push(format!(
                                "Missing required parameter '{}' ({})",
                                param.name, param.label
                            ));
                        }
                    }
                    Some(value) => {
                        if let Err(e) = param.validate_value(value) {
                            errors.push(e);
                        }
                    }
                }
            } else if let Some(value) = params.get(&param.name) {
                if let Err(e) = param.validate_value(value) {
                    errors.push(e);
                }
            }
        }

        if errors.is_empty() {
            ValidationResult::valid()
        } else {
            ValidationResult::invalid(errors)
        }
    }

    /// Fill in missing parameters with defaults.
    pub fn apply_defaults(
        &self,
        params: &HashMap<String, serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        let mut merged = params.clone();
        for param in &self.parameters {
            if !merged.contains_key(&param.name) {
                if let Some(ref default) = param.default {
                    merged.insert(param.name.clone(), default.clone());
                } else if let Some(default) = self.default_values.get(&param.name) {
                    merged.insert(param.name.clone(), default.clone());
                }
            }
        }
        merged
    }
}

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

/// Returns the list of built-in proposal templates.
pub fn built_in_templates() -> Vec<ProposalTemplate> {
    use serde_json::json;

    vec![
        // ---- Protocol parameter changes ----
        ProposalTemplate {
            id: "change_fee_rate".into(),
            name: "Change Relay Fee Rate".into(),
            category: ProposalCategory::ProtocolParam,
            description: "Adjust the relay fee rate percentage".into(),
            parameters: vec![TemplateParameter {
                name: "new_fee_rate".into(),
                label: "New Fee Rate (%)".into(),
                param_type: TemplateParamType::Float,
                required: true,
                description: "New fee rate between 0.1 and 10.0".into(),
                min: Some(0.1),
                max: Some(10.0),
                ..Default::default()
            }],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        ProposalTemplate {
            id: "change_timeout".into(),
            name: "Change Request Timeout".into(),
            category: ProposalCategory::ProtocolParam,
            description: "Adjust the request timeout duration in seconds".into(),
            parameters: vec![TemplateParameter {
                name: "new_timeout_secs".into(),
                label: "New Timeout (seconds)".into(),
                param_type: TemplateParamType::Integer,
                required: true,
                description: "New timeout in seconds (10-3600)".into(),
                min: Some(10.0),
                max: Some(3600.0),
                ..Default::default()
            }],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        // ---- Provider actions ----
        ProposalTemplate {
            id: "add_provider".into(),
            name: "Add Provider".into(),
            category: ProposalCategory::ProviderAction,
            description: "Whitelist a new compute provider".into(),
            parameters: vec![
                TemplateParameter {
                    name: "provider_pk".into(),
                    label: "Provider Public Key".into(),
                    param_type: TemplateParamType::Address,
                    required: true,
                    description: "Provider's Ergo address or public key".into(),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "region".into(),
                    label: "Region".into(),
                    param_type: TemplateParamType::Select,
                    required: true,
                    description: "Provider's geographic region".into(),
                    options: vec![
                        "us-east".into(),
                        "us-west".into(),
                        "eu-west".into(),
                        "eu-central".into(),
                        "asia-east".into(),
                        "asia-south".into(),
                    ],
                    ..Default::default()
                },
                TemplateParameter {
                    name: "max_models".into(),
                    label: "Max Concurrent Models".into(),
                    param_type: TemplateParamType::Integer,
                    required: false,
                    description: "Maximum concurrent models this provider can serve".into(),
                    min: Some(1.0),
                    max: Some(100.0),
                    default: Some(json!(10)),
                    ..Default::default()
                },
            ],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        ProposalTemplate {
            id: "remove_provider".into(),
            name: "Remove Provider".into(),
            category: ProposalCategory::ProviderAction,
            description: "Remove a compute provider from the whitelist".into(),
            parameters: vec![
                TemplateParameter {
                    name: "provider_pk".into(),
                    label: "Provider Public Key".into(),
                    param_type: TemplateParamType::Address,
                    required: true,
                    description: "Provider's Ergo address to remove".into(),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "reason".into(),
                    label: "Reason".into(),
                    param_type: TemplateParamType::String,
                    required: true,
                    description: "Reason for removal".into(),
                    ..Default::default()
                },
            ],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        ProposalTemplate {
            id: "suspend_provider".into(),
            name: "Suspend Provider".into(),
            category: ProposalCategory::ProviderAction,
            description: "Temporarily suspend a compute provider".into(),
            parameters: vec![
                TemplateParameter {
                    name: "provider_pk".into(),
                    label: "Provider Public Key".into(),
                    param_type: TemplateParamType::Address,
                    required: true,
                    ..Default::default()
                },
                TemplateParameter {
                    name: "duration_blocks".into(),
                    label: "Suspension Duration (blocks)".into(),
                    param_type: TemplateParamType::Duration,
                    required: true,
                    min: Some(1.0),
                    max: Some(43200.0),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "reason".into(),
                    label: "Reason".into(),
                    param_type: TemplateParamType::String,
                    required: true,
                    ..Default::default()
                },
            ],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        // ---- Treasury ----
        ProposalTemplate {
            id: "treasury_spend".into(),
            name: "Treasury Spend".into(),
            category: ProposalCategory::TreasurySpend,
            description: "Allocate treasury funds for a specific purpose".into(),
            parameters: vec![
                TemplateParameter {
                    name: "recipient".into(),
                    label: "Recipient Address".into(),
                    param_type: TemplateParamType::Address,
                    required: true,
                    description: "Ergo address to receive funds".into(),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "amount_nanoerg".into(),
                    label: "Amount (nanoERG)".into(),
                    param_type: TemplateParamType::Integer,
                    required: true,
                    description: "Amount to transfer in nanoERG".into(),
                    min: Some(0.0),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "purpose".into(),
                    label: "Purpose".into(),
                    param_type: TemplateParamType::String,
                    required: true,
                    description: "Description of the spend purpose".into(),
                    ..Default::default()
                },
            ],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        // ---- Emergency ----
        ProposalTemplate {
            id: "emergency_pause".into(),
            name: "Emergency Pause".into(),
            category: ProposalCategory::Emergency,
            description: "Emergency pause of network operations".into(),
            parameters: vec![
                TemplateParameter {
                    name: "duration_blocks".into(),
                    label: "Pause Duration (blocks)".into(),
                    param_type: TemplateParamType::Duration,
                    required: true,
                    description: "How long the pause lasts (1-7200 blocks)".into(),
                    min: Some(1.0),
                    max: Some(7200.0),
                    default: Some(json!(100)),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "reason".into(),
                    label: "Reason".into(),
                    param_type: TemplateParamType::String,
                    required: true,
                    description: "Reason for the emergency pause".into(),
                    ..Default::default()
                },
            ],
            default_values: HashMap::new(),
            requires_stake: false, // emergency council can bypass
            vote_duration_override: Some(72), // shorter for emergency (~2.4 hours)
        },

        ProposalTemplate {
            id: "emergency_unpause".into(),
            name: "Emergency Unpause".into(),
            category: ProposalCategory::Emergency,
            description: "Lift an emergency pause".into(),
            parameters: vec![TemplateParameter {
                name: "reason".into(),
                label: "Reason".into(),
                param_type: TemplateParamType::String,
                required: true,
                description: "Reason for lifting the pause".into(),
                ..Default::default()
            }],
            default_values: HashMap::new(),
            requires_stake: false,
            vote_duration_override: Some(72),
        },

        // ---- Config changes ----
        ProposalTemplate {
            id: "change_vote_duration".into(),
            name: "Change Default Vote Duration".into(),
            category: ProposalCategory::ConfigChange,
            description: "Change the default voting period duration".into(),
            parameters: vec![TemplateParameter {
                name: "new_duration".into(),
                label: "New Duration (blocks)".into(),
                param_type: TemplateParamType::Duration,
                required: true,
                description: "New default voting duration (100-43200 blocks)".into(),
                min: Some(100.0),
                max: Some(43200.0),
                ..Default::default()
            }],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        ProposalTemplate {
            id: "change_quorum".into(),
            name: "Change Quorum Threshold".into(),
            category: ProposalCategory::ConfigChange,
            description: "Change the minimum vote quorum required for proposals".into(),
            parameters: vec![TemplateParameter {
                name: "new_quorum".into(),
                label: "New Quorum".into(),
                param_type: TemplateParamType::Integer,
                required: true,
                description: "New quorum threshold (1-1000)".into(),
                min: Some(1.0),
                max: Some(1000.0),
                ..Default::default()
            }],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        ProposalTemplate {
            id: "change_approval_threshold".into(),
            name: "Change Approval Threshold".into(),
            category: ProposalCategory::ConfigChange,
            description: "Change the approval percentage needed for proposals to pass".into(),
            parameters: vec![TemplateParameter {
                name: "new_threshold".into(),
                label: "New Threshold (%)".into(),
                param_type: TemplateParamType::Percentage,
                required: true,
                description: "New approval threshold (50-100%)".into(),
                min: Some(50.0),
                max: Some(100.0),
                ..Default::default()
            }],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        ProposalTemplate {
            id: "change_min_stake".into(),
            name: "Change Minimum Stake".into(),
            category: ProposalCategory::ConfigChange,
            description: "Change the minimum stake required to propose or vote".into(),
            parameters: vec![
                TemplateParameter {
                    name: "min_stake_to_propose".into(),
                    label: "Min Stake to Propose (nanoERG)".into(),
                    param_type: TemplateParamType::Integer,
                    required: true,
                    min: Some(0.0),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "min_stake_to_vote".into(),
                    label: "Min Stake to Vote (nanoERG)".into(),
                    param_type: TemplateParamType::Integer,
                    required: true,
                    min: Some(0.0),
                    ..Default::default()
                },
            ],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },

        // ---- Contract upgrade ----
        ProposalTemplate {
            id: "contract_upgrade".into(),
            name: "Contract Upgrade".into(),
            category: ProposalCategory::ContractUpgrade,
            description: "Upgrade a protocol smart contract to a new version".into(),
            parameters: vec![
                TemplateParameter {
                    name: "contract_name".into(),
                    label: "Contract Name".into(),
                    param_type: TemplateParamType::String,
                    required: true,
                    description: "Name of the contract to upgrade".into(),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "new_script_hash".into(),
                    label: "New Script Hash".into(),
                    param_type: TemplateParamType::TokenId,
                    required: true,
                    description: "Blake2b256 hash of the new contract script".into(),
                    ..Default::default()
                },
                TemplateParameter {
                    name: "justification".into(),
                    label: "Justification".into(),
                    param_type: TemplateParamType::String,
                    required: true,
                    description: "Reason for the upgrade".into(),
                    ..Default::default()
                },
            ],
            default_values: HashMap::new(),
            requires_stake: true,
            vote_duration_override: None,
        },
    ]
}

/// Get a template by ID. Returns None if not found.
pub fn get_template(id: &str) -> Option<ProposalTemplate> {
    built_in_templates()
        .into_iter()
        .find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn templates() -> Vec<ProposalTemplate> {
        built_in_templates()
    }

    #[test]
    fn test_builtin_templates_exist() {
        let templates = templates();
        assert!(
            templates.len() >= 13,
            "Expected at least 13 built-in templates, got {}",
            templates.len()
        );
    }

    #[test]
    fn test_template_ids_unique() {
        let templates = templates();
        let ids: Vec<&str> = templates.iter().map(|t| t.id.as_str()).collect();
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            assert!(
                seen.insert(id),
                "Duplicate template ID: {}",
                id
            );
        }
    }

    #[test]
    fn test_fee_rate_template_has_float_param() {
        let template = get_template("change_fee_rate").unwrap();
        assert_eq!(template.id, "change_fee_rate");
        assert_eq!(template.category, ProposalCategory::ProtocolParam);
        assert_eq!(template.parameters.len(), 1);
        let param = &template.parameters[0];
        assert_eq!(param.param_type, TemplateParamType::Float);
        assert_eq!(param.min, Some(0.1));
        assert_eq!(param.max, Some(10.0));
        assert!(param.required);
    }

    #[test]
    fn test_add_provider_template_params() {
        let template = get_template("add_provider").unwrap();
        assert_eq!(template.category, ProposalCategory::ProviderAction);
        assert_eq!(template.parameters.len(), 3);
        // Check required params
        let required: Vec<_> = template
            .parameters
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.as_str())
            .collect();
        assert!(required.contains(&"provider_pk"));
        assert!(required.contains(&"region"));
        // Check optional param
        let max_models = template
            .parameters
            .iter()
            .find(|p| p.name == "max_models")
            .unwrap();
        assert!(!max_models.required);
        assert_eq!(max_models.default, Some(serde_json::json!(10)));
    }

    #[test]
    fn test_treasury_spend_template() {
        let template = get_template("treasury_spend").unwrap();
        assert_eq!(template.category, ProposalCategory::TreasurySpend);
        let param_names: Vec<_> =
            template.parameters.iter().map(|p| p.name.as_str()).collect();
        assert!(param_names.contains(&"recipient"));
        assert!(param_names.contains(&"amount_nanoerg"));
        assert!(param_names.contains(&"purpose"));
    }

    #[test]
    fn test_emergency_pause_shorter_duration() {
        let template = get_template("emergency_pause").unwrap();
        assert_eq!(template.category, ProposalCategory::Emergency);
        assert!(!template.requires_stake);
        assert_eq!(template.vote_duration_override, Some(72));
    }

    #[test]
    fn test_emergency_unpause_no_stake_required() {
        let template = get_template("emergency_unpause").unwrap();
        assert!(!template.requires_stake);
        assert_eq!(template.vote_duration_override, Some(72));
    }

    #[test]
    fn test_config_change_templates() {
        let ids = vec![
            "change_vote_duration",
            "change_quorum",
            "change_approval_threshold",
            "change_min_stake",
        ];
        for id in ids {
            let template = get_template(id).unwrap();
            assert_eq!(
                template.category,
                ProposalCategory::ConfigChange,
                "Template {} should be ConfigChange",
                id
            );
        }
    }

    #[test]
    fn test_template_validation_required_param_missing() {
        let template = get_template("change_fee_rate").unwrap();
        let params = HashMap::new();
        let result = template.validate_params(&params);
        assert!(!result.is_valid);
        assert!(result.errors[0].contains("new_fee_rate"));
    }

    #[test]
    fn test_template_validation_valid_params() {
        let template = get_template("change_fee_rate").unwrap();
        let mut params = HashMap::new();
        params.insert(
            "new_fee_rate".into(),
            serde_json::json!(2.5),
        );
        let result = template.validate_params(&params);
        assert!(result.is_valid);
    }

    #[test]
    fn test_template_validation_out_of_range() {
        let template = get_template("change_fee_rate").unwrap();
        let mut params = HashMap::new();
        params.insert(
            "new_fee_rate".into(),
            serde_json::json!(50.0),
        );
        let result = template.validate_params(&params);
        assert!(!result.is_valid);
        assert!(result.errors[0].contains("exceeds maximum"));
    }

    #[test]
    fn test_template_validation_wrong_type() {
        let template = get_template("change_fee_rate").unwrap();
        let mut params = HashMap::new();
        params.insert(
            "new_fee_rate".into(),
            serde_json::json!("not a number"),
        );
        let result = template.validate_params(&params);
        assert!(!result.is_valid);
        assert!(result.errors[0].contains("must be a number"));
    }

    #[test]
    fn test_template_select_param_validation() {
        let template = get_template("add_provider").unwrap();
        // Valid option
        let mut params = HashMap::new();
        params.insert("provider_pk".into(), serde_json::json!("3Wxy..."));
        params.insert("region".into(), serde_json::json!("us-east"));
        let result = template.validate_params(&params);
        // Should not error on region (provider_pk is required too but we provided it)
        assert!(
            !result.errors.iter().any(|e| e.contains("region")),
            "Unexpected region error: {:?}",
            result.errors
        );

        // Invalid option
        let mut params2 = HashMap::new();
        params2.insert("provider_pk".into(), serde_json::json!("3Wxy..."));
        params2.insert("region".into(), serde_json::json!("invalid-region"));
        let result2 = template.validate_params(&params2);
        assert!(result2
            .errors
            .iter()
            .any(|e| e.contains("not one of")));
    }

    #[test]
    fn test_template_percentage_validation() {
        let template = get_template("change_approval_threshold").unwrap();
        // Valid
        let mut params = HashMap::new();
        params.insert("new_threshold".into(), serde_json::json!(75));
        let result = template.validate_params(&params);
        assert!(result.is_valid);

        // Out of range (too low)
        let mut params2 = HashMap::new();
        params2.insert("new_threshold".into(), serde_json::json!(30));
        let result2 = template.validate_params(&params2);
        assert!(!result2.is_valid);
        assert!(result2.errors[0].contains("below minimum"));
    }

    #[test]
    fn test_apply_defaults_fills_missing() {
        let template = get_template("add_provider").unwrap();
        let mut params = HashMap::new();
        params.insert("provider_pk".into(), serde_json::json!("3Wxy..."));
        params.insert("region".into(), serde_json::json!("us-east"));
        // max_models is not provided, should get default of 10
        let filled = template.apply_defaults(&params);
        assert_eq!(filled.get("max_models").unwrap(), &serde_json::json!(10));
    }

    #[test]
    fn test_get_template_not_found() {
        assert!(get_template("nonexistent_template").is_none());
    }

    #[test]
    fn test_contract_upgrade_template() {
        let template = get_template("contract_upgrade").unwrap();
        assert_eq!(template.category, ProposalCategory::ContractUpgrade);
        let param_names: Vec<_> =
            template.parameters.iter().map(|p| p.name.as_str()).collect();
        assert!(param_names.contains(&"contract_name"));
        assert!(param_names.contains(&"new_script_hash"));
        assert!(param_names.contains(&"justification"));
    }
}
