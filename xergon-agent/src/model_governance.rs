//! Model Governance & Audit Trail
//!
//! Tracks all model lifecycle events, policy compliance, and provides audit logging
//! for regulatory requirements. Supports policy management, compliance checking,
//! and governance metrics collection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

/// Types of governance events that can occur in the model lifecycle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum GovernanceEventType {
    ModelRegistered,
    ModelUpdated,
    ModelDeprecated,
    ModelRemoved,
    ModelVersionCreated,
    ModelVersionPromoted,
    ModelVersionRolledBack,
    PolicyViolation,
    AccessGranted,
    AccessRevoked,
    ComplianceCheck,
    DeploymentApproved,
    DeploymentRejected,
    ConfigChanged,
}

impl GovernanceEventType {
    /// Returns a string representation of the event type.
    pub fn as_str(&self) -> &str {
        match self {
            GovernanceEventType::ModelRegistered => "ModelRegistered",
            GovernanceEventType::ModelUpdated => "ModelUpdated",
            GovernanceEventType::ModelDeprecated => "ModelDeprecated",
            GovernanceEventType::ModelRemoved => "ModelRemoved",
            GovernanceEventType::ModelVersionCreated => "ModelVersionCreated",
            GovernanceEventType::ModelVersionPromoted => "ModelVersionPromoted",
            GovernanceEventType::ModelVersionRolledBack => "ModelVersionRolledBack",
            GovernanceEventType::PolicyViolation => "PolicyViolation",
            GovernanceEventType::AccessGranted => "AccessGranted",
            GovernanceEventType::AccessRevoked => "AccessRevoked",
            GovernanceEventType::ComplianceCheck => "ComplianceCheck",
            GovernanceEventType::DeploymentApproved => "DeploymentApproved",
            GovernanceEventType::DeploymentRejected => "DeploymentRejected",
            GovernanceEventType::ConfigChanged => "ConfigChanged",
        }
    }
}

// ---------------------------------------------------------------------------
// Governance event
// ---------------------------------------------------------------------------

/// A single governance event in the audit trail.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GovernanceEvent {
    /// Unique event identifier.
    pub id: String,
    /// The type of governance event.
    pub event_type: GovernanceEventType,
    /// The model this event relates to.
    pub model_id: String,
    /// The actor (user or system) that triggered this event.
    pub actor: String,
    /// When the event occurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional structured metadata about the event.
    pub metadata: HashMap<String, serde_json::Value>,
    /// Severity level: "info", "warning", "error", "critical".
    pub severity: String,
}

impl GovernanceEvent {
    /// Creates a new governance event with the given parameters.
    pub fn new(
        event_type: GovernanceEventType,
        model_id: String,
        actor: String,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            event_type,
            model_id,
            actor,
            timestamp: Utc::now(),
            metadata,
            severity: "info".to_string(),
        }
    }

    /// Sets the severity level for this event.
    pub fn with_severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = severity.into();
        self
    }
}

// ---------------------------------------------------------------------------
// Policy rule operators
// ---------------------------------------------------------------------------

/// Supported comparison operators for policy rules.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PolicyOperator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
    #[serde(rename = "gt")]
    Gt,
    #[serde(rename = "lt")]
    Lt,
    #[serde(rename = "gte")]
    Gte,
    #[serde(rename = "lte")]
    Lte,
    #[serde(rename = "contains")]
    Contains,
    #[serde(rename = "regex")]
    Regex,
}

// ---------------------------------------------------------------------------
// Policy rule
// ---------------------------------------------------------------------------

/// A single rule within a model policy.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PolicyRule {
    /// The field in the model metadata to check.
    pub field: String,
    /// The comparison operator.
    pub operator: String,
    /// The expected value to compare against.
    pub value: serde_json::Value,
    /// Severity when this rule is violated: "warning" or "error".
    #[serde(default = "default_severity")]
    pub severity: String,
}

fn default_severity() -> String {
    "warning".to_string()
}

impl PolicyRule {
    /// Creates a new policy rule.
    pub fn new(
        field: impl Into<String>,
        operator: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        Self {
            field: field.into(),
            operator: operator.into(),
            value,
            severity: default_severity(),
        }
    }

    /// Creates a new policy rule with a custom severity.
    pub fn with_severity(
        field: impl Into<String>,
        operator: impl Into<String>,
        value: serde_json::Value,
        severity: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            operator: operator.into(),
            value,
            severity: severity.into(),
        }
    }

    /// Evaluates this rule against the given actual value.
    /// Returns true if the rule passes (no violation).
    pub fn evaluate(&self, actual: &serde_json::Value) -> bool {
        match self.operator.as_str() {
            "eq" => actual == &self.value,
            "ne" => actual != &self.value,
            "gt" => compare_json_values(actual, &self.value) > 0,
            "lt" => compare_json_values(actual, &self.value) < 0,
            "gte" => compare_json_values(actual, &self.value) >= 0,
            "lte" => compare_json_values(actual, &self.value) <= 0,
            "contains" => {
                let actual_str = match actual {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let expected_str = self.value.as_str().unwrap_or("").to_string();
                actual_str.contains(&expected_str)
            }
            "regex" => {
                let actual_str = match actual {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let pattern = self.value.as_str().unwrap_or("");
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(&actual_str))
                    .unwrap_or(false)
            }
            _ => false,
        }
    }
}

/// Compare two JSON values numerically. Returns ordering or 0 if not comparable.
fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> i32 {
    let a_num = match a {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        _ => return 0,
    };
    let b_num = match b {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        _ => return 0,
    };
    a_num.partial_cmp(&b_num).map(|o| o as i32).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Model policy
// ---------------------------------------------------------------------------

/// A governance policy containing rules that models must comply with.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelPolicy {
    /// Unique policy identifier.
    pub id: String,
    /// Human-readable policy name.
    pub name: String,
    /// Description of what this policy enforces.
    pub description: String,
    /// The rules that make up this policy.
    pub rules: Vec<PolicyRule>,
    /// When this policy was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this policy was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Whether this policy is currently active.
    #[serde(skip)]
    pub is_active: Arc<std::sync::atomic::AtomicBool>,
}

impl ModelPolicy {
    /// Creates a new model policy.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        rules: Vec<PolicyRule>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            rules,
            created_at: now,
            updated_at: now,
            is_active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    /// Returns whether this policy is active.
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Policy violation
// ---------------------------------------------------------------------------

/// A single policy violation found during compliance checking.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PolicyViolation {
    /// The policy that was violated.
    pub policy_id: String,
    /// The field that caused the violation.
    pub rule_field: String,
    /// The actual value found in the model metadata.
    pub actual_value: serde_json::Value,
    /// The expected value per the policy rule.
    pub expected_value: serde_json::Value,
    /// Severity of the violation.
    pub severity: String,
    /// Human-readable message describing the violation.
    pub message: String,
}

impl PolicyViolation {
    /// Creates a new policy violation from a rule evaluation.
    pub fn new(
        policy_id: impl Into<String>,
        rule: &PolicyRule,
        actual_value: serde_json::Value,
    ) -> Self {
        let message = format!(
            "Field '{}' failed {} check: expected {:?}, got {:?}",
            rule.field, rule.operator, rule.value, actual_value
        );
        Self {
            policy_id: policy_id.into(),
            rule_field: rule.field.clone(),
            actual_value,
            expected_value: rule.value.clone(),
            severity: rule.severity.clone(),
            message,
        }
    }
}

// ---------------------------------------------------------------------------
// Compliance report
// ---------------------------------------------------------------------------

/// The result of a compliance check against a model.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComplianceReport {
    /// The model that was checked.
    pub model_id: String,
    /// When the compliance check was performed.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Whether the model passed all policy checks.
    pub passed: bool,
    /// Hard violations (severity "error") found.
    pub violations: Vec<PolicyViolation>,
    /// Soft violations (severity "warning") found.
    pub warnings: Vec<PolicyViolation>,
    /// List of policy IDs that were checked.
    pub checked_policies: Vec<String>,
}

// ---------------------------------------------------------------------------
// Governance metrics snapshot
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of governance metrics.
#[derive(Serialize)]
pub struct GovernanceMetricsSnapshot {
    /// Total governance events recorded.
    pub total_events: u64,
    /// Total policy violations detected.
    pub policy_violations: u64,
    /// Total compliance checks performed.
    pub compliance_checks: u64,
    /// Percentage of compliance checks that passed (0.0 - 100.0).
    pub compliance_rate: f64,
    /// Number of currently active policies.
    pub active_policies: u32,
    /// Number of models currently governed (have policies assigned).
    pub models_governed: u32,
}

// ---------------------------------------------------------------------------
// Model governance engine
// ---------------------------------------------------------------------------

/// Core governance engine that tracks model lifecycle events and enforces policies.
pub struct ModelGovernance {
    /// All recorded governance events, keyed by event ID.
    pub events: DashMap<String, GovernanceEvent>,
    /// All registered policies, keyed by policy ID.
    pub policies: DashMap<String, Arc<ModelPolicy>>,
    /// Mapping from model ID to assigned policy IDs.
    pub model_policies: DashMap<String, Vec<String>>,
    /// Counter for total events recorded.
    pub total_events: AtomicU64,
    /// Counter for total violations detected.
    pub violations: AtomicU64,
    /// Counter for total compliance checks performed.
    pub compliance_checks: AtomicU64,
}

impl ModelGovernance {
    /// Creates a new, empty model governance instance.
    pub fn new() -> Self {
        Self {
            events: DashMap::new(),
            policies: DashMap::new(),
            model_policies: DashMap::new(),
            total_events: AtomicU64::new(0),
            violations: AtomicU64::new(0),
            compliance_checks: AtomicU64::new(0),
        }
    }

    // ---- Event management ----

    /// Records a new governance event and stores it in the audit trail.
    /// Returns the created event.
    pub fn record_event(
        &self,
        event_type: GovernanceEventType,
        model_id: impl Into<String>,
        actor: impl Into<String>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> GovernanceEvent {
        let event = GovernanceEvent::new(
            event_type,
            model_id.into(),
            actor.into(),
            metadata,
        );
        self.events.insert(event.id.clone(), event.clone());
        self.total_events.fetch_add(1, Ordering::Relaxed);
        event
    }

    /// Records a new governance event with a custom severity level.
    /// Returns the created event.
    pub fn record_event_with_severity(
        &self,
        event_type: GovernanceEventType,
        model_id: impl Into<String>,
        actor: impl Into<String>,
        metadata: HashMap<String, serde_json::Value>,
        severity: impl Into<String>,
    ) -> GovernanceEvent {
        let event = GovernanceEvent::new(
            event_type,
            model_id.into(),
            actor.into(),
            metadata,
        )
        .with_severity(severity);
        self.events.insert(event.id.clone(), event.clone());
        self.total_events.fetch_add(1, Ordering::Relaxed);
        event
    }

    /// Gets all governance events for a specific model.
    pub fn get_events(&self, model_id: &str) -> Vec<GovernanceEvent> {
        self.events
            .iter()
            .filter(|e| e.value().model_id == model_id)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Gets all governance events of a specific type.
    pub fn get_events_by_type(&self, event_type: &GovernanceEventType) -> Vec<GovernanceEvent> {
        self.events
            .iter()
            .filter(|e| &e.value().event_type == event_type)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Gets a specific governance event by its ID.
    pub fn get_event(&self, id: &str) -> Option<GovernanceEvent> {
        self.events.get(id).map(|e| e.value().clone())
    }

    /// Returns the total number of recorded events.
    pub fn event_count(&self) -> u64 {
        self.total_events.load(Ordering::Relaxed)
    }

    /// Removes all events for a given model. Returns the number removed.
    pub fn clear_events_for_model(&self, model_id: &str) -> usize {
        let keys: Vec<String> = self
            .events
            .iter()
            .filter(|e| e.value().model_id == model_id)
            .map(|e| e.key().clone())
            .collect();
        let count = keys.len();
        for key in keys {
            self.events.remove(&key);
        }
        count
    }

    // ---- Policy management ----

    /// Adds a new policy to the governance engine.
    /// Returns an error if a policy with the same ID already exists.
    pub fn add_policy(&self, policy: ModelPolicy) -> Result<ModelPolicy, String> {
        let id = policy.id.clone();
        if self.policies.contains_key(&id) {
            return Err(format!("Policy '{}' already exists", id));
        }
        let policy = Arc::new(policy);
        self.policies.insert(id.clone(), policy.clone());
        Ok(Arc::unwrap_or_clone(policy))
    }

    /// Gets a specific policy by its ID.
    pub fn get_policy(&self, id: &str) -> Option<ModelPolicy> {
        self.policies.get(id).map(|p| Arc::unwrap_or_clone(p.value().clone()))
    }

    /// Lists all registered policies.
    pub fn list_policies(&self) -> Vec<ModelPolicy> {
        self.policies
            .iter()
            .map(|p| Arc::unwrap_or_clone(p.value().clone()))
            .collect()
    }

    /// Lists only active policies.
    pub fn list_active_policies(&self) -> Vec<ModelPolicy> {
        self.policies
            .iter()
            .filter(|p| p.value().is_active())
            .map(|p| Arc::unwrap_or_clone(p.value().clone()))
            .collect()
    }

    /// Activates a policy by ID.
    pub fn activate_policy(&self, id: &str) -> Result<(), String> {
        let policy = self.policies.get(id).ok_or_else(|| {
            format!("Policy '{}' not found", id)
        })?;
        policy.value().is_active.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Deactivates a policy by ID.
    pub fn deactivate_policy(&self, id: &str) -> Result<(), String> {
        let policy = self.policies.get(id).ok_or_else(|| {
            format!("Policy '{}' not found", id)
        })?;
        policy.value().is_active.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Removes a policy by ID. Returns an error if the policy is not found.
    pub fn remove_policy(&self, id: &str) -> Result<(), String> {
        if self.policies.remove(id).is_none() {
            return Err(format!("Policy '{}' not found", id));
        }
        // Also remove from model assignments
        let mut models_to_clean: Vec<String> = Vec::new();
        for mut entry in self.model_policies.iter_mut() {
            entry.value_mut().retain(|pid| pid != id);
            if entry.value().is_empty() {
                models_to_clean.push(entry.key().clone());
            }
        }
        for model_id in models_to_clean {
            self.model_policies.remove(&model_id);
        }
        Ok(())
    }

    /// Updates an existing policy. Replaces all fields.
    /// Returns an error if the policy does not exist.
    pub fn update_policy(&self, policy: ModelPolicy) -> Result<(), String> {
        let id = policy.id.clone();
        if !self.policies.contains_key(&id) {
            return Err(format!("Policy '{}' not found", id));
        }
        let mut updated = policy;
        updated.updated_at = Utc::now();
        self.policies.insert(id, Arc::new(updated));
        Ok(())
    }

    // ---- Model-policy assignment ----

    /// Assigns a policy to a model. Both must exist.
    pub fn assign_policy_to_model(
        &self,
        policy_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        if !self.policies.contains_key(policy_id) {
            return Err(format!("Policy '{}' not found", policy_id));
        }
        let mut entry = self
            .model_policies
            .entry(model_id.to_string())
            .or_insert_with(Vec::new);
        let policies = entry.value_mut();
        if !policies.contains(&policy_id.to_string()) {
            policies.push(policy_id.to_string());
        }
        Ok(())
    }

    /// Removes a policy assignment from a model.
    pub fn remove_policy_from_model(
        &self,
        policy_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        let mut entry = self
            .model_policies
            .get_mut(model_id)
            .ok_or_else(|| format!("No policies assigned to model '{}'", model_id))?;
        let len_before = entry.len();
        entry.retain(|pid| pid != policy_id);
        let removed = entry.len() != len_before;
        let is_empty = entry.is_empty();
        drop(entry);
        if is_empty {
            self.model_policies.remove(model_id);
        }
        if !removed {
            return Err(format!(
                "Policy '{}' is not assigned to model '{}'",
                policy_id, model_id
            ));
        }
        Ok(())
    }

    /// Gets all policy IDs assigned to a model.
    pub fn get_policies_for_model(&self, model_id: &str) -> Vec<String> {
        self.model_policies
            .get(model_id)
            .map(|e| e.value().clone())
            .unwrap_or_default()
    }

    /// Gets all model IDs that have a specific policy assigned.
    pub fn get_models_for_policy(&self, policy_id: &str) -> Vec<String> {
        self.model_policies
            .iter()
            .filter(|e| e.value().contains(&policy_id.to_string()))
            .map(|e| e.key().clone())
            .collect()
    }

    // ---- Compliance checking ----

    /// Checks a model's metadata against all assigned active policies.
    /// Returns a detailed compliance report.
    pub fn check_compliance(
        &self,
        model_id: &str,
        model_metadata: HashMap<String, serde_json::Value>,
    ) -> ComplianceReport {
        self.compliance_checks.fetch_add(1, Ordering::Relaxed);

        let assigned_policy_ids = self
            .model_policies
            .get(model_id)
            .map(|e| e.value().clone())
            .unwrap_or_default();

        let mut violations: Vec<PolicyViolation> = Vec::new();
        let mut warnings: Vec<PolicyViolation> = Vec::new();
        let mut checked_policies: Vec<String> = Vec::new();

        for policy_id in &assigned_policy_ids {
            if let Some(policy) = self.policies.get(policy_id) {
                if !policy.value().is_active() {
                    continue;
                }
                checked_policies.push(policy_id.clone());
                let policy = policy.value();

                for rule in &policy.rules {
                    let actual_value = model_metadata
                        .get(&rule.field)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);

                    if !rule.evaluate(&actual_value) {
                        let violation =
                            PolicyViolation::new(&policy.id, rule, actual_value);
                        if rule.severity == "error" {
                            violations.push(violation);
                        } else {
                            warnings.push(violation);
                        }
                    }
                }
            }
        }

        let violation_count = violations.len() as u64;
        self.violations.fetch_add(violation_count, Ordering::Relaxed);

        let passed = violations.is_empty();

        // Record a compliance check event
        let event_severity = if passed {
            "info"
        } else {
            "error"
        };
        let mut meta = HashMap::new();
        meta.insert("passed".to_string(), serde_json::Value::Bool(passed));
        meta.insert(
            "violation_count".to_string(),
            serde_json::json!(violations.len()),
        );
        meta.insert(
            "warning_count".to_string(),
            serde_json::json!(warnings.len()),
        );
        self.record_event_with_severity(
            GovernanceEventType::ComplianceCheck,
            model_id,
            "system",
            meta,
            event_severity,
        );

        ComplianceReport {
            model_id: model_id.to_string(),
            timestamp: Utc::now(),
            passed,
            violations,
            warnings,
            checked_policies,
        }
    }

    /// Checks a model's metadata against a specific policy only.
    /// Returns a detailed compliance report scoped to that policy.
    pub fn check_compliance_against_policy(
        &self,
        model_id: &str,
        policy_id: &str,
        model_metadata: HashMap<String, serde_json::Value>,
    ) -> Result<ComplianceReport, String> {
        let policy = self.policies.get(policy_id).ok_or_else(|| {
            format!("Policy '{}' not found", policy_id)
        })?;
        let policy = policy.value();

        if !policy.is_active() {
            return Err(format!("Policy '{}' is not active", policy_id));
        }

        let mut violations: Vec<PolicyViolation> = Vec::new();
        let mut warnings: Vec<PolicyViolation> = Vec::new();

        for rule in &policy.rules {
            let actual_value = model_metadata
                .get(&rule.field)
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            if !rule.evaluate(&actual_value) {
                let violation = PolicyViolation::new(&policy.id, rule, actual_value);
                if rule.severity == "error" {
                    violations.push(violation);
                } else {
                    warnings.push(violation);
                }
            }
        }

        let passed = violations.is_empty();

        Ok(ComplianceReport {
            model_id: model_id.to_string(),
            timestamp: Utc::now(),
            passed,
            violations,
            warnings,
            checked_policies: vec![policy_id.to_string()],
        })
    }

    // ---- Metrics ----

    /// Returns a point-in-time snapshot of governance metrics.
    pub fn get_metrics(&self) -> GovernanceMetricsSnapshot {
        let total_checks = self.compliance_checks.load(Ordering::Relaxed);
        let total_violations = self.violations.load(Ordering::Relaxed);
        let compliance_rate = if total_checks > 0 {
            let checks_with_no_violations = total_checks.saturating_sub(total_violations);
            ((checks_with_no_violations as f64) / (total_checks as f64)) * 100.0
        } else {
            100.0
        };

        let active_policies = self
            .policies
            .iter()
            .filter(|p| p.value().is_active())
            .count() as u32;

        let models_governed = self.model_policies.len() as u32;

        GovernanceMetricsSnapshot {
            total_events: self.total_events.load(Ordering::Relaxed),
            policy_violations: total_violations,
            compliance_checks: total_checks,
            compliance_rate,
            active_policies,
            models_governed,
        }
    }
}

impl Default for ModelGovernance {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_governance() -> ModelGovernance {
        ModelGovernance::new()
    }

    fn make_test_policy(id: &str) -> ModelPolicy {
        ModelPolicy::new(
            id,
            format!("Policy {}", id),
            format!("Test policy {}", id),
            vec![
                PolicyRule::new("param_count", "gte", serde_json::json!(1)),
                PolicyRule::new("status", "eq", serde_json::json!("approved")),
            ],
        )
    }

    fn make_metadata(param_count: i64, status: &str) -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("param_count".to_string(), serde_json::json!(param_count));
        m.insert("status".to_string(), serde_json::json!(status));
        m
    }

    // -- Event tests --

    #[test]
    fn test_record_event_basic() {
        let gov = make_governance();
        let event = gov.record_event(
            GovernanceEventType::ModelRegistered,
            "model-1",
            "alice",
            HashMap::new(),
        );
        assert_eq!(event.model_id, "model-1");
        assert_eq!(event.actor, "alice");
        assert_eq!(event.event_type, GovernanceEventType::ModelRegistered);
        assert_eq!(event.severity, "info");
        assert!(!event.id.is_empty());
        assert_eq!(gov.event_count(), 1);
    }

    #[test]
    fn test_record_event_with_severity() {
        let gov = make_governance();
        let event = gov.record_event_with_severity(
            GovernanceEventType::PolicyViolation,
            "model-1",
            "system",
            HashMap::new(),
            "critical",
        );
        assert_eq!(event.severity, "critical");
    }

    #[test]
    fn test_get_event() {
        let gov = make_governance();
        let event = gov.record_event(
            GovernanceEventType::ModelUpdated,
            "model-1",
            "bob",
            HashMap::new(),
        );
        let retrieved = gov.get_event(&event.id).unwrap();
        assert_eq!(retrieved.id, event.id);
        assert_eq!(retrieved.actor, "bob");
    }

    #[test]
    fn test_get_event_not_found() {
        let gov = make_governance();
        assert!(gov.get_event("nonexistent").is_none());
    }

    #[test]
    fn test_get_events_by_model() {
        let gov = make_governance();
        gov.record_event(
            GovernanceEventType::ModelRegistered,
            "model-a",
            "alice",
            HashMap::new(),
        );
        gov.record_event(
            GovernanceEventType::ModelUpdated,
            "model-a",
            "bob",
            HashMap::new(),
        );
        gov.record_event(
            GovernanceEventType::ModelRegistered,
            "model-b",
            "alice",
            HashMap::new(),
        );
        let events = gov.get_events("model-a");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_get_events_by_type() {
        let gov = make_governance();
        gov.record_event(
            GovernanceEventType::ModelRegistered,
            "m1",
            "alice",
            HashMap::new(),
        );
        gov.record_event(
            GovernanceEventType::ModelRegistered,
            "m2",
            "bob",
            HashMap::new(),
        );
        gov.record_event(
            GovernanceEventType::ModelUpdated,
            "m1",
            "alice",
            HashMap::new(),
        );
        let events = gov.get_events_by_type(&GovernanceEventType::ModelRegistered);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_clear_events_for_model() {
        let gov = make_governance();
        gov.record_event(
            GovernanceEventType::ModelRegistered,
            "model-x",
            "alice",
            HashMap::new(),
        );
        gov.record_event(
            GovernanceEventType::ModelUpdated,
            "model-y",
            "bob",
            HashMap::new(),
        );
        let removed = gov.clear_events_for_model("model-x");
        assert_eq!(removed, 1);
        assert_eq!(gov.event_count(), 2); // DashMap removal doesn't decrement counter
    }

    #[test]
    fn test_event_type_as_str() {
        assert_eq!(
            GovernanceEventType::ModelRegistered.as_str(),
            "ModelRegistered"
        );
        assert_eq!(GovernanceEventType::ConfigChanged.as_str(), "ConfigChanged");
    }

    #[test]
    fn test_event_serialization() {
        let event = GovernanceEvent::new(
            GovernanceEventType::DeploymentApproved,
            "model-1".to_string(),
            "admin".to_string(),
            HashMap::new(),
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("DeploymentApproved"));
        let deserialized: GovernanceEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, event.id);
    }

    // -- Policy tests --

    #[test]
    fn test_add_policy() {
        let gov = make_governance();
        let policy = make_test_policy("pol-1");
        let result = gov.add_policy(policy);
        assert!(result.is_ok());
        let retrieved = gov.get_policy("pol-1").unwrap();
        assert_eq!(retrieved.name, "Policy pol-1");
    }

    #[test]
    fn test_add_duplicate_policy_fails() {
        let gov = make_governance();
        let _ = gov.add_policy(make_test_policy("pol-1"));
        let result = gov.add_policy(make_test_policy("pol-1"));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_policies() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.add_policy(make_test_policy("pol-2")).unwrap();
        let policies = gov.list_policies();
        assert_eq!(policies.len(), 2);
    }

    #[test]
    fn test_list_active_policies() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.add_policy(make_test_policy("pol-2")).unwrap();
        gov.deactivate_policy("pol-2").unwrap();
        let active = gov.list_active_policies();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "pol-1");
    }

    #[test]
    fn test_activate_deactivate_policy() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.deactivate_policy("pol-1").unwrap();
        assert!(!gov.get_policy("pol-1").unwrap().is_active());
        gov.activate_policy("pol-1").unwrap();
        assert!(gov.get_policy("pol-1").unwrap().is_active());
    }

    #[test]
    fn test_activate_nonexistent_policy() {
        let gov = make_governance();
        let result = gov.activate_policy("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_policy() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.remove_policy("pol-1").unwrap();
        assert!(gov.get_policy("pol-1").is_none());
    }

    #[test]
    fn test_remove_nonexistent_policy() {
        let gov = make_governance();
        let result = gov.remove_policy("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_update_policy() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        let mut updated = gov.get_policy("pol-1").unwrap();
        updated.name = "Updated Policy".to_string();
        gov.update_policy(updated).unwrap();
        assert_eq!(gov.get_policy("pol-1").unwrap().name, "Updated Policy");
    }

    #[test]
    fn test_update_nonexistent_policy() {
        let gov = make_governance();
        let policy = make_test_policy("nonexistent");
        let result = gov.update_policy(policy);
        assert!(result.is_err());
    }

    // -- Policy assignment tests --

    #[test]
    fn test_assign_policy_to_model() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        let policies = gov.get_policies_for_model("model-1");
        assert_eq!(policies, vec!["pol-1".to_string()]);
    }

    #[test]
    fn test_assign_nonexistent_policy() {
        let gov = make_governance();
        let result = gov.assign_policy_to_model("nonexistent", "model-1");
        assert!(result.is_err());
    }

    #[test]
    fn test_assign_duplicate_policy_to_model() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        let policies = gov.get_policies_for_model("model-1");
        assert_eq!(policies.len(), 1);
    }

    #[test]
    fn test_remove_policy_from_model() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        gov.remove_policy_from_model("pol-1", "model-1").unwrap();
        assert!(gov.get_policies_for_model("model-1").is_empty());
    }

    #[test]
    fn test_remove_nonassigned_policy_from_model() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        let result = gov.remove_policy_from_model("pol-other", "model-1");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_models_for_policy() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        gov.assign_policy_to_model("pol-1", "model-2").unwrap();
        let models = gov.get_models_for_policy("pol-1");
        assert_eq!(models.len(), 2);
    }

    // -- Compliance tests --

    #[test]
    fn test_compliance_pass() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        let metadata = make_metadata(5, "approved");
        let report = gov.check_compliance("model-1", metadata);
        assert!(report.passed);
        assert!(report.violations.is_empty());
        assert!(report.warnings.is_empty());
        assert_eq!(report.checked_policies, vec!["pol-1"]);
    }

    #[test]
    fn test_compliance_fail_warning() {
        let gov = make_governance();
        let mut policy = make_test_policy("pol-1");
        policy.rules[0] = PolicyRule::with_severity(
            "param_count",
            "gte",
            serde_json::json!(100),
            "warning",
        );
        gov.add_policy(policy).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        let metadata = make_metadata(5, "approved");
        let report = gov.check_compliance("model-1", metadata);
        assert!(report.passed); // only warnings, not errors
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn test_compliance_fail_error() {
        let gov = make_governance();
        let mut policy = make_test_policy("pol-1");
        policy.rules[0] = PolicyRule::with_severity(
            "param_count",
            "gte",
            serde_json::json!(100),
            "error",
        );
        gov.add_policy(policy).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        let metadata = make_metadata(5, "approved");
        let report = gov.check_compliance("model-1", metadata);
        assert!(!report.passed);
        assert_eq!(report.violations.len(), 1);
    }

    #[test]
    fn test_compliance_no_policies_assigned() {
        let gov = make_governance();
        let report = gov.check_compliance("model-1", HashMap::new());
        assert!(report.passed);
        assert!(report.checked_policies.is_empty());
    }

    #[test]
    fn test_compliance_inactive_policy_skipped() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.deactivate_policy("pol-1").unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        let metadata = make_metadata(0, "rejected");
        let report = gov.check_compliance("model-1", metadata);
        assert!(report.passed);
        assert!(report.checked_policies.is_empty());
    }

    #[test]
    fn test_check_compliance_against_specific_policy() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        let metadata = make_metadata(5, "approved");
        let report = gov
            .check_compliance_against_policy("model-1", "pol-1", metadata)
            .unwrap();
        assert!(report.passed);
        assert_eq!(report.checked_policies, vec!["pol-1"]);
    }

    #[test]
    fn test_check_compliance_against_nonexistent_policy() {
        let gov = make_governance();
        let result = gov.check_compliance_against_policy(
            "model-1",
            "nonexistent",
            HashMap::new(),
        );
        assert!(result.is_err());
    }

    // -- Policy rule tests --

    #[test]
    fn test_rule_evaluate_eq() {
        let rule = PolicyRule::new("field", "eq", serde_json::json!("hello"));
        assert!(rule.evaluate(&serde_json::json!("hello")));
        assert!(!rule.evaluate(&serde_json::json!("world")));
    }

    #[test]
    fn test_rule_evaluate_ne() {
        let rule = PolicyRule::new("field", "ne", serde_json::json!("hello"));
        assert!(rule.evaluate(&serde_json::json!("world")));
        assert!(!rule.evaluate(&serde_json::json!("hello")));
    }

    #[test]
    fn test_rule_evaluate_gt_lt() {
        let rule_gt = PolicyRule::new("field", "gt", serde_json::json!(10));
        assert!(rule_gt.evaluate(&serde_json::json!(20)));
        assert!(!rule_gt.evaluate(&serde_json::json!(5)));

        let rule_lt = PolicyRule::new("field", "lt", serde_json::json!(10));
        assert!(rule_lt.evaluate(&serde_json::json!(5)));
        assert!(!rule_lt.evaluate(&serde_json::json!(20)));
    }

    #[test]
    fn test_rule_evaluate_gte_lte() {
        let rule_gte = PolicyRule::new("field", "gte", serde_json::json!(10));
        assert!(rule_gte.evaluate(&serde_json::json!(10)));
        assert!(rule_gte.evaluate(&serde_json::json!(20)));
        assert!(!rule_gte.evaluate(&serde_json::json!(9)));

        let rule_lte = PolicyRule::new("field", "lte", serde_json::json!(10));
        assert!(rule_lte.evaluate(&serde_json::json!(10)));
        assert!(rule_lte.evaluate(&serde_json::json!(5)));
        assert!(!rule_lte.evaluate(&serde_json::json!(11)));
    }

    #[test]
    fn test_rule_evaluate_contains() {
        let rule = PolicyRule::new("field", "contains", serde_json::json!("hello"));
        assert!(rule.evaluate(&serde_json::json!("say hello world")));
        assert!(!rule.evaluate(&serde_json::json!("goodbye")));
    }

    #[test]
    fn test_rule_evaluate_regex() {
        let rule = PolicyRule::new("field", "regex", serde_json::json!(r"^\d{3}-\d{4}$"));
        assert!(rule.evaluate(&serde_json::json!("555-1234")));
        assert!(!rule.evaluate(&serde_json::json!("invalid")));
    }

    #[test]
    fn test_rule_evaluate_null_actual() {
        let rule = PolicyRule::new("field", "eq", serde_json::json!("hello"));
        assert!(!rule.evaluate(&serde_json::Value::Null));
    }

    #[test]
    fn test_rule_evaluate_unknown_operator() {
        let rule = PolicyRule::new("field", "unknown_op", serde_json::json!(42));
        assert!(!rule.evaluate(&serde_json::json!(42)));
    }

    // -- Metrics tests --

    #[test]
    fn test_metrics_initial() {
        let gov = make_governance();
        let metrics = gov.get_metrics();
        assert_eq!(metrics.total_events, 0);
        assert_eq!(metrics.policy_violations, 0);
        assert_eq!(metrics.compliance_checks, 0);
        assert_eq!(metrics.compliance_rate, 100.0);
        assert_eq!(metrics.active_policies, 0);
        assert_eq!(metrics.models_governed, 0);
    }

    #[test]
    fn test_metrics_after_operations() {
        let gov = make_governance();
        gov.record_event(
            GovernanceEventType::ModelRegistered,
            "m1",
            "alice",
            HashMap::new(),
        );
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "m1").unwrap();
        gov.check_compliance("m1", make_metadata(5, "approved"));

        let metrics = gov.get_metrics();
        assert_eq!(metrics.total_events, 3); // 1 registered + 1 compliance + the check_compliance internal event
        assert!(metrics.compliance_checks >= 1);
        assert_eq!(metrics.active_policies, 1);
        assert_eq!(metrics.models_governed, 1);
    }

    #[test]
    fn test_governance_default() {
        let gov = ModelGovernance::default();
        assert_eq!(gov.event_count(), 0);
    }

    #[test]
    fn test_violation_message_format() {
        let rule = PolicyRule::new("status", "eq", serde_json::json!("approved"));
        let v = PolicyViolation::new("pol-1", &rule, serde_json::json!("rejected"));
        assert!(v.message.contains("status"));
        assert!(v.message.contains("eq"));
        assert_eq!(v.policy_id, "pol-1");
        assert_eq!(v.rule_field, "status");
    }

    #[test]
    fn test_event_with_metadata() {
        let gov = make_governance();
        let mut meta = HashMap::new();
        meta.insert("version".to_string(), serde_json::json!("1.0.0"));
        meta.insert("size_mb".to_string(), serde_json::json!(4096));
        let event = gov.record_event(
            GovernanceEventType::ConfigChanged,
            "model-1",
            "admin",
            meta,
        );
        assert_eq!(event.metadata.get("version").unwrap(), "1.0.0");
    }

    #[test]
    fn test_remove_policy_cleans_assignments() {
        let gov = make_governance();
        gov.add_policy(make_test_policy("pol-1")).unwrap();
        gov.assign_policy_to_model("pol-1", "model-1").unwrap();
        gov.remove_policy("pol-1").unwrap();
        assert!(gov.get_policies_for_model("model-1").is_empty());
    }

    #[test]
    fn test_multiple_violations_in_one_check() {
        let gov = make_governance();
        let mut policy = make_test_policy("pol-strict");
        policy.rules = vec![
            PolicyRule::with_severity("a", "eq", serde_json::json!(1), "error"),
            PolicyRule::with_severity("b", "eq", serde_json::json!(2), "error"),
            PolicyRule::with_severity("c", "eq", serde_json::json!(3), "warning"),
        ];
        gov.add_policy(policy).unwrap();
        gov.assign_policy_to_model("pol-strict", "model-1").unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("a".to_string(), serde_json::json!(99));
        metadata.insert("b".to_string(), serde_json::json!(99));
        metadata.insert("c".to_string(), serde_json::json!(99));

        let report = gov.check_compliance("model-1", metadata);
        assert!(!report.passed);
        assert_eq!(report.violations.len(), 2);
        assert_eq!(report.warnings.len(), 1);
    }
}
