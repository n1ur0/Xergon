//! Feature flag system with boolean, variant, percentage, and gradual rollout.
//!
//! Provides:
//! - Feature flag CRUD operations
//! - Boolean flags, variant/multivariate flags, percentage rollout, gradual rollout
//! - Rule-based evaluation with priority
//! - Per-user overrides
//! - REST API endpoints for flag management

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The type of a feature flag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FlagType {
    /// Simple on/off boolean flag.
    Boolean,
    /// Multivariate flag with multiple possible values.
    Variant,
    /// Percentage-based rollout (0.0-100.0).
    Percentage,
    /// Gradual rollout that increases over time.
    Gradual,
}

/// The source of a flag evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationSource {
    /// The default value of the flag was used.
    Default,
    /// A rule matched and provided the value.
    Rule,
    /// A per-user override was applied.
    Override,
}

/// A feature flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    /// Unique flag identifier.
    pub flag_id: String,
    /// Human-readable flag name.
    pub name: String,
    /// Description of the flag.
    pub description: String,
    /// The type of flag.
    pub flag_type: FlagType,
    /// The current value of the flag (JSON).
    pub value: serde_json::Value,
    /// Available variant values for Variant type flags.
    pub variants: Vec<serde_json::Value>,
    /// Whether the flag is enabled.
    pub enabled: bool,
    /// Creation timestamp (Unix epoch seconds).
    pub created_at: u64,
    /// Last update timestamp (Unix epoch seconds).
    pub updated_at: u64,
}

/// A rule that can override a flag's value based on conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// The flag this rule belongs to.
    pub flag_id: String,
    /// Condition expression (e.g., "user.region == 'us'").
    pub condition: String,
    /// The value to return when this rule matches.
    pub value: serde_json::Value,
    /// Priority (lower = higher priority).
    pub priority: i32,
    /// Whether the rule is enabled.
    pub enabled: bool,
}

/// Result of evaluating a feature flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagEvaluation {
    /// The flag ID that was evaluated.
    pub flag_id: String,
    /// The evaluated value.
    pub value: serde_json::Value,
    /// The source of the evaluation.
    pub source: EvaluationSource,
    /// When the evaluation occurred.
    pub evaluated_at: u64,
}

/// Statistics about the feature flag service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagStats {
    /// Total flags.
    pub total_flags: usize,
    /// Enabled flags.
    pub enabled_flags: usize,
    /// Disabled flags.
    pub disabled_flags: usize,
    /// Total rules across all flags.
    pub total_rules: usize,
    /// Total user overrides.
    pub total_overrides: usize,
    /// Flags by type.
    pub by_type: HashMap<String, usize>,
}

// ---------------------------------------------------------------------------
// FeatureFlagService
// ---------------------------------------------------------------------------

/// DashMap-backed feature flag service with CRUD, evaluation, rules, and overrides.
#[derive(Debug, Clone)]
pub struct FeatureFlagService {
    /// Feature flags keyed by flag_id.
    flags: Arc<DashMap<String, FeatureFlag>>,
    /// Rules keyed by rule_id.
    rules: Arc<DashMap<String, FlagRule>>,
    /// Per-user overrides: (flag_id, user_id) -> value.
    user_overrides: Arc<DashMap<(String, String), serde_json::Value>>,
    /// Counter for generating IDs.
    flag_counter: Arc<AtomicU64>,
    rule_counter: Arc<AtomicU64>,
}

impl FeatureFlagService {
    /// Create a new feature flag service.
    pub fn new() -> Self {
        Self {
            flags: Arc::new(DashMap::new()),
            rules: Arc::new(DashMap::new()),
            user_overrides: Arc::new(DashMap::new()),
            flag_counter: Arc::new(AtomicU64::new(1)),
            rule_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    fn now_epoch() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn next_flag_id(&self) -> String {
        format!("flag-{}", self.flag_counter.fetch_add(1, Ordering::SeqCst))
    }

    fn next_rule_id(&self) -> String {
        format!("rule-{}", self.rule_counter.fetch_add(1, Ordering::SeqCst))
    }

    /// Create a new feature flag.
    pub fn create_flag(
        &self,
        name: String,
        description: String,
        flag_type: FlagType,
        value: serde_json::Value,
        variants: Vec<serde_json::Value>,
        enabled: bool,
    ) -> Result<FeatureFlag, String> {
        if name.is_empty() {
            return Err("Flag name cannot be empty".to_string());
        }

        if matches!(flag_type, FlagType::Variant) && variants.is_empty() {
            return Err("Variant flags require at least one variant".to_string());
        }

        let id = self.next_flag_id();
        let now = Self::now_epoch();

        let flag = FeatureFlag {
            flag_id: id,
            name,
            description,
            flag_type,
            value,
            variants,
            enabled,
            created_at: now,
            updated_at: now,
        };

        let flag_id = flag.flag_id.clone();
        self.flags.insert(flag_id, flag.clone());
        Ok(flag)
    }

    /// Update an existing feature flag.
    pub fn update_flag(
        &self,
        flag_id: &str,
        name: Option<String>,
        description: Option<String>,
        value: Option<serde_json::Value>,
        enabled: Option<bool>,
        variants: Option<Vec<serde_json::Value>>,
    ) -> Result<FeatureFlag, String> {
        let mut flag = self
            .flags
            .get_mut(flag_id)
            .ok_or_else(|| format!("Flag '{}' not found", flag_id))?;

        if let Some(n) = name {
            if n.is_empty() {
                return Err("Flag name cannot be empty".to_string());
            }
            flag.name = n;
        }
        if let Some(d) = description {
            flag.description = d;
        }
        if let Some(v) = value {
            flag.value = v;
        }
        if let Some(e) = enabled {
            flag.enabled = e;
        }
        if let Some(v) = variants {
            if matches!(flag.flag_type, FlagType::Variant) && v.is_empty() {
                return Err("Variant flags require at least one variant".to_string());
            }
            flag.variants = v;
        }
        flag.updated_at = Self::now_epoch();

        Ok(flag.clone())
    }

    /// Delete a feature flag and all its rules and overrides.
    pub fn delete_flag(&self, flag_id: &str) -> Result<(), String> {
        if self.flags.remove(flag_id).is_none() {
            return Err(format!("Flag '{}' not found", flag_id));
        }

        // Remove all rules for this flag
        let rules_to_remove: Vec<String> = self
            .rules
            .iter()
            .filter(|r| r.value().flag_id == flag_id)
            .map(|r| r.key().clone())
            .collect();
        for rule_id in rules_to_remove {
            self.rules.remove(&rule_id);
        }

        // Remove all user overrides for this flag
        let overrides_to_remove: Vec<(String, String)> = self
            .user_overrides
            .iter()
            .filter(|r| r.key().0 == flag_id)
            .map(|r| r.key().clone())
            .collect();
        for key in overrides_to_remove {
            self.user_overrides.remove(&key);
        }

        Ok(())
    }

    /// Evaluate a feature flag for a given user.
    ///
    /// Priority: user override > rules (by priority) > default value.
    pub fn evaluate(
        &self,
        flag_id: &str,
        user_id: Option<&str>,
        context: Option<&HashMap<String, String>>,
    ) -> Result<FlagEvaluation, String> {
        let flag = self
            .flags
            .get(flag_id)
            .ok_or_else(|| format!("Flag '{}' not found", flag_id))?;

        if !flag.enabled {
            return Ok(FlagEvaluation {
                flag_id: flag_id.to_string(),
                value: flag.value.clone(),
                source: EvaluationSource::Default,
                evaluated_at: Self::now_epoch(),
            });
        }

        // 1. Check user override
        if let Some(uid) = user_id {
            let override_key = (flag_id.to_string(), uid.to_string());
            if let Some(override_value) = self.user_overrides.get(&override_key) {
                return Ok(FlagEvaluation {
                    flag_id: flag_id.to_string(),
                    value: override_value.clone(),
                    source: EvaluationSource::Override,
                    evaluated_at: Self::now_epoch(),
                });
            }
        }

        // 2. Check rules (sorted by priority)
        let mut matching_rules: Vec<FlagRule> = self
            .rules
            .iter()
            .filter(|r| {
                r.value().flag_id == flag_id && r.value().enabled
            })
            .map(|r| r.value().clone())
            .collect();
        matching_rules.sort_by_key(|r| r.priority);

        for rule in &matching_rules {
            if Self::evaluate_condition(&rule.condition, context) {
                return Ok(FlagEvaluation {
                    flag_id: flag_id.to_string(),
                    value: rule.value.clone(),
                    source: EvaluationSource::Rule,
                    evaluated_at: Self::now_epoch(),
                });
            }
        }

        // 3. Check percentage rollout for Percentage type
        if flag.flag_type == FlagType::Percentage {
            if let Some(uid) = user_id {
                let hash = Self::hash_user(flag_id, uid);
                let percentage = flag.value.as_f64().unwrap_or(0.0);
                let threshold = percentage / 100.0;
                let hash_frac = (hash as f64) / (u64::MAX as f64);

                let value = if hash_frac <= threshold {
                    serde_json::Value::Bool(true)
                } else {
                    serde_json::Value::Bool(false)
                };

                return Ok(FlagEvaluation {
                    flag_id: flag_id.to_string(),
                    value,
                    source: EvaluationSource::Default,
                    evaluated_at: Self::now_epoch(),
                });
            }
        }

        // 4. Check gradual rollout for Gradual type
        if flag.flag_type == FlagType::Gradual {
            if let Some(uid) = user_id {
                let hash = Self::hash_user(flag_id, uid);
                // Gradual: value represents the current rollout percentage
                let rollout_pct = flag.value.as_f64().unwrap_or(0.0);
                let threshold = rollout_pct / 100.0;
                let hash_frac = (hash as f64) / (u64::MAX as f64);

                let value = if hash_frac <= threshold {
                    serde_json::Value::Bool(true)
                } else {
                    serde_json::Value::Bool(false)
                };

                return Ok(FlagEvaluation {
                    flag_id: flag_id.to_string(),
                    value,
                    source: EvaluationSource::Default,
                    evaluated_at: Self::now_epoch(),
                });
            }
        }

        // 5. Default value
        Ok(FlagEvaluation {
            flag_id: flag_id.to_string(),
            value: flag.value.clone(),
            source: EvaluationSource::Default,
            evaluated_at: Self::now_epoch(),
        })
    }

    /// Simple deterministic hash for user-flag pairs.
    fn hash_user(flag_id: &str, user_id: &str) -> u64 {
        let mut hash: u64 = 5381;
        for byte in format!("{}:{}", flag_id, user_id).bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        hash
    }

    /// Simple condition evaluation (supports key == value checks).
    fn evaluate_condition(
        condition: &str,
        context: Option<&HashMap<String, String>>,
    ) -> bool {
        // Parse simple conditions like "key == value" or "key != value"
        let parts: Vec<&str> = condition.splitn(3, char::is_whitespace).collect();
        if parts.len() != 3 {
            return false;
        }

        let key = parts[0];
        let op = parts[1];
        let expected = parts[2];

        let actual = context
            .and_then(|ctx| ctx.get(key))
            .map(|s| s.as_str());

        match op {
            "==" => actual.map(|a| a == expected).unwrap_or(false),
            "!=" => actual.map(|a| a != expected).unwrap_or(true),
            _ => false,
        }
    }

    /// Add a rule to a flag.
    pub fn add_rule(
        &self,
        flag_id: &str,
        condition: String,
        value: serde_json::Value,
        priority: i32,
    ) -> Result<FlagRule, String> {
        // Verify the flag exists
        self.flags
            .get(flag_id)
            .ok_or_else(|| format!("Flag '{}' not found", flag_id))?;

        let rule_id = self.next_rule_id();
        let rule = FlagRule {
            rule_id: rule_id.clone(),
            flag_id: flag_id.to_string(),
            condition,
            value,
            priority,
            enabled: true,
        };

        self.rules.insert(rule_id, rule.clone());
        Ok(rule)
    }

    /// Remove a rule.
    pub fn remove_rule(&self, rule_id: &str) -> Result<(), String> {
        if self.rules.remove(rule_id).is_none() {
            return Err(format!("Rule '{}' not found", rule_id));
        }
        Ok(())
    }

    /// Set a per-user override for a flag.
    pub fn set_override(
        &self,
        flag_id: &str,
        user_id: &str,
        value: serde_json::Value,
    ) -> Result<(), String> {
        // Verify the flag exists
        self.flags
            .get(flag_id)
            .ok_or_else(|| format!("Flag '{}' not found", flag_id))?;

        let key = (flag_id.to_string(), user_id.to_string());
        self.user_overrides.insert(key, value);
        Ok(())
    }

    /// Remove a per-user override.
    pub fn remove_override(&self, flag_id: &str, user_id: &str) -> Result<(), String> {
        let key = (flag_id.to_string(), user_id.to_string());
        if self.user_overrides.remove(&key).is_none() {
            return Err(format!(
                "No override found for flag '{}' user '{}'",
                flag_id, user_id
            ));
        }
        Ok(())
    }

    /// Get a flag by ID.
    pub fn get_flag(&self, flag_id: &str) -> Option<FeatureFlag> {
        self.flags.get(flag_id).map(|f| f.clone())
    }

    /// List all flags, optionally filtered by enabled status.
    pub fn list_flags(&self, enabled_only: Option<bool>) -> Vec<FeatureFlag> {
        self.flags
            .iter()
            .map(|f| f.value().clone())
            .filter(|f| enabled_only.is_none() || Some(f.enabled) == enabled_only)
            .collect()
    }

    /// Get rules for a flag.
    pub fn get_rules(&self, flag_id: &str) -> Vec<FlagRule> {
        self.rules
            .iter()
            .filter(|r| r.value().flag_id == flag_id)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get statistics about the flag service.
    pub fn get_stats(&self) -> FlagStats {
        let total_flags = self.flags.len();
        let mut enabled_flags = 0;
        let mut disabled_flags = 0;
        let mut by_type: HashMap<String, usize> = HashMap::new();

        for flag in self.flags.iter() {
            if flag.value().enabled {
                enabled_flags += 1;
            } else {
                disabled_flags += 1;
            }
            let type_str = format!("{:?}", flag.value().flag_type).to_lowercase();
            *by_type.entry(type_str).or_insert(0) += 1;
        }

        FlagStats {
            total_flags,
            enabled_flags,
            disabled_flags,
            total_rules: self.rules.len(),
            total_overrides: self.user_overrides.len(),
            by_type,
        }
    }
}

impl Default for FeatureFlagService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};

/// Build the feature flags router.
pub fn build_feature_flags_router(state: crate::api::AppState) -> Router {
    Router::new()
        .route("/v1/flags", post(create_flag_handler))
        .route("/v1/flags", get(list_flags_handler))
        .route("/v1/flags/{id}", get(get_flag_handler))
        .route("/v1/flags/{id}", put(update_flag_handler))
        .route("/v1/flags/{id}", delete(delete_flag_handler))
        .route("/v1/flags/evaluate", post(evaluate_flag_handler))
        .route("/v1/flags/{id}/rules", get(get_rules_handler))
        .route("/v1/flags/stats", get(get_stats_handler))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct CreateFlagRequest {
    name: String,
    #[serde(default)]
    description: String,
    flag_type: FlagType,
    value: serde_json::Value,
    #[serde(default)]
    variants: Vec<serde_json::Value>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct UpdateFlagRequest {
    name: Option<String>,
    description: Option<String>,
    value: Option<serde_json::Value>,
    enabled: Option<bool>,
    variants: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct EvaluateFlagRequest {
    flag_id: String,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    context: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AddRuleRequest {
    condition: String,
    value: serde_json::Value,
    #[serde(default)]
    priority: i32,
}

async fn create_flag_handler(
    State(state): State<crate::api::AppState>,
    axum::Json(req): axum::Json<CreateFlagRequest>,
) -> impl IntoResponse {
    match state.feature_flags.create_flag(
        req.name,
        req.description,
        req.flag_type,
        req.value,
        req.variants,
        req.enabled,
    ) {
        Ok(flag) => (axum::http::StatusCode::CREATED, axum::Json(flag)).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn list_flags_handler(
    State(state): State<crate::api::AppState>,
) -> impl IntoResponse {
    axum::Json(state.feature_flags.list_flags(None))
}

async fn get_flag_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.feature_flags.get_flag(&id) {
        Some(flag) => axum::Json(flag).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": "Flag not found"})),
        )
            .into_response(),
    }
}

async fn update_flag_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
    axum::Json(req): axum::Json<UpdateFlagRequest>,
) -> impl IntoResponse {
    match state.feature_flags.update_flag(&id, req.name, req.description, req.value, req.enabled, req.variants) {
        Ok(flag) => axum::Json(flag).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn delete_flag_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.feature_flags.delete_flag(&id) {
        Ok(()) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({"deleted": true})),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn evaluate_flag_handler(
    State(state): State<crate::api::AppState>,
    axum::Json(req): axum::Json<EvaluateFlagRequest>,
) -> impl IntoResponse {
    match state.feature_flags.evaluate(&req.flag_id, req.user_id.as_deref(), Some(&req.context)) {
        Ok(eval) => axum::Json(eval).into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn get_rules_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    axum::Json(state.feature_flags.get_rules(&id))
}

async fn get_stats_handler(
    State(state): State<crate::api::AppState>,
) -> impl IntoResponse {
    axum::Json(state.feature_flags.get_stats())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_service() -> FeatureFlagService {
        FeatureFlagService::new()
    }

    #[test]
    fn test_create_boolean_flag() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "dark_mode".into(),
                "Enable dark mode".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(true),
                vec![],
                true,
            )
            .unwrap();

        assert_eq!(flag.name, "dark_mode");
        assert_eq!(flag.flag_type, FlagType::Boolean);
        assert_eq!(flag.enabled, true);
    }

    #[test]
    fn test_create_flag_empty_name_fails() {
        let svc = create_service();
        assert!(svc
            .create_flag(
                "".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(true),
                vec![],
                true,
            )
            .is_err());
    }

    #[test]
    fn test_create_variant_flag_no_variants_fails() {
        let svc = create_service();
        assert!(svc
            .create_flag(
                "theme".into(),
                "desc".into(),
                FlagType::Variant,
                serde_json::Value::String("default".into()),
                vec![],
                true,
            )
            .is_err());
    }

    #[test]
    fn test_update_flag() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "feature".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(true),
                vec![],
                true,
            )
            .unwrap();

        let updated = svc
            .update_flag(&flag.flag_id, Some("renamed".into()), None, None, None, None)
            .unwrap();

        assert_eq!(updated.name, "renamed");
        assert_eq!(updated.enabled, true);
    }

    #[test]
    fn test_delete_flag() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "temp".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(true),
                vec![],
                true,
            )
            .unwrap();

        svc.delete_flag(&flag.flag_id).unwrap();
        assert!(svc.get_flag(&flag.flag_id).is_none());
    }

    #[test]
    fn test_evaluate_boolean_flag() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "new_ui".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(true),
                vec![],
                true,
            )
            .unwrap();

        let eval = svc.evaluate(&flag.flag_id, None, None).unwrap();
        assert_eq!(eval.value, serde_json::Value::Bool(true));
        assert_eq!(eval.source, EvaluationSource::Default);
    }

    #[test]
    fn test_evaluate_disabled_flag() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "disabled".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(true),
                vec![],
                false,
            )
            .unwrap();

        let eval = svc.evaluate(&flag.flag_id, Some("user1"), None).unwrap();
        assert_eq!(eval.value, serde_json::Value::Bool(true));
        assert_eq!(eval.source, EvaluationSource::Default);
    }

    #[test]
    fn test_user_override() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "feature".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(false),
                vec![],
                true,
            )
            .unwrap();

        svc.set_override(&flag.flag_id, "user1", serde_json::Value::Bool(true))
            .unwrap();

        let eval = svc
            .evaluate(&flag.flag_id, Some("user1"), None)
            .unwrap();
        assert_eq!(eval.value, serde_json::Value::Bool(true));
        assert_eq!(eval.source, EvaluationSource::Override);

        // Other users should get the default
        let eval2 = svc
            .evaluate(&flag.flag_id, Some("user2"), None)
            .unwrap();
        assert_eq!(eval2.value, serde_json::Value::Bool(false));
        assert_eq!(eval2.source, EvaluationSource::Default);
    }

    #[test]
    fn test_rule_evaluation() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "region_feature".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(false),
                vec![],
                true,
            )
            .unwrap();

        svc.add_rule(
            &flag.flag_id,
            "region == eu".into(),
            serde_json::Value::Bool(true),
            1,
        )
        .unwrap();

        let mut ctx = HashMap::new();
        ctx.insert("region".to_string(), "eu".to_string());

        let eval = svc
            .evaluate(&flag.flag_id, Some("user1"), Some(&ctx))
            .unwrap();
        assert_eq!(eval.value, serde_json::Value::Bool(true));
        assert_eq!(eval.source, EvaluationSource::Rule);
    }

    #[test]
    fn test_percentage_rollout() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "pct_rollout".into(),
                "desc".into(),
                FlagType::Percentage,
                serde_json::json!(50.0),
                vec![],
                true,
            )
            .unwrap();

        // Evaluate for many users - should get roughly 50% true
        let mut true_count = 0;
        for i in 0..1000 {
            let eval = svc
                .evaluate(&flag.flag_id, Some(&format!("user-{}", i)), None)
                .unwrap();
            if eval.value == serde_json::Value::Bool(true) {
                true_count += 1;
            }
        }

        assert!(
            true_count > 400 && true_count < 600,
            "Expected ~50%% true, got {}%",
            true_count / 10
        );
    }

    #[test]
    fn test_gradual_rollout() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "gradual".into(),
                "desc".into(),
                FlagType::Gradual,
                serde_json::json!(25.0),
                vec![],
                true,
            )
            .unwrap();

        let mut true_count = 0;
        for i in 0..1000 {
            let eval = svc
                .evaluate(&flag.flag_id, Some(&format!("user-{}", i)), None)
                .unwrap();
            if eval.value == serde_json::Value::Bool(true) {
                true_count += 1;
            }
        }

        assert!(
            true_count > 150 && true_count < 350,
            "Expected ~25%% true, got {}%",
            true_count / 10
        );
    }

    #[test]
    fn test_get_stats() {
        let svc = create_service();
        svc.create_flag(
            "a".into(),
            "".into(),
            FlagType::Boolean,
            serde_json::Value::Bool(true),
            vec![],
            true,
        )
        .unwrap();
        svc.create_flag(
            "b".into(),
            "".into(),
            FlagType::Variant,
            serde_json::Value::String("v1".into()),
            vec![serde_json::Value::String("v1".into())],
            false,
        )
        .unwrap();

        let stats = svc.get_stats();
        assert_eq!(stats.total_flags, 2);
        assert_eq!(stats.enabled_flags, 1);
        assert_eq!(stats.disabled_flags, 1);
    }

    #[test]
    fn test_override_priority_over_rules() {
        let svc = create_service();
        let flag = svc
            .create_flag(
                "priority_test".into(),
                "desc".into(),
                FlagType::Boolean,
                serde_json::Value::Bool(false),
                vec![],
                true,
            )
            .unwrap();

        svc.add_rule(
            &flag.flag_id,
            "region == us".into(),
            serde_json::Value::Bool(true),
            1,
        )
        .unwrap();

        svc.set_override(&flag.flag_id, "user1", serde_json::Value::String("custom".into()))
            .unwrap();

        let mut ctx = HashMap::new();
        ctx.insert("region".to_string(), "us".to_string());

        // Override should take priority over rule
        let eval = svc
            .evaluate(&flag.flag_id, Some("user1"), Some(&ctx))
            .unwrap();
        assert_eq!(eval.value, serde_json::Value::String("custom".into()));
        assert_eq!(eval.source, EvaluationSource::Override);
    }
}
