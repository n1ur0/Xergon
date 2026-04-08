//! Compliance Engine for AI Model Inference on the Xergon Network
//!
//! Provides regulatory compliance layering on top of model_governance (lifecycle)
//! and inference_cost_tracker (cost tracking). Handles:
//!
//! - Policy management (create / update / delete compliance policies)
//! - Content policy rules (blocked categories, PII flags, toxicity thresholds)
//! - Usage policy rules (max requests per user, max tokens per day, tier-based model access)
//! - Data retention policies (auto-delete inference logs after TTL, GDPR right-to-erasure)
//! - Model access control (restrict models by user tier, region, KYC level)
//! - Audit event recording (every compliance check logged with timestamp, outcome, policy ref)
//! - Compliance reporting (violations, checks passed, policy coverage)
//! - Risk scoring (per-user and per-request risk assessment based on behavior patterns)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Severity levels for compliance events and policy violations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "critical")]
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Low => write!(f, "low"),
            Severity::Medium => write!(f, "medium"),
            Severity::High => write!(f, "high"),
            Severity::Critical => write!(f, "critical"),
        }
    }
}

impl Severity {
    /// Numeric score for comparison: 0 (info) .. 4 (critical).
    pub fn score(&self) -> u8 {
        match self {
            Severity::Info => 0,
            Severity::Low => 1,
            Severity::Medium => 2,
            Severity::High => 3,
            Severity::Critical => 4,
        }
    }
}

/// User access tier.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum UserTier {
    #[serde(rename = "free")]
    Free,
    #[serde(rename = "basic")]
    Basic,
    #[serde(rename = "pro")]
    Pro,
    #[serde(rename = "enterprise")]
    Enterprise,
}

impl std::fmt::Display for UserTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserTier::Free => write!(f, "free"),
            UserTier::Basic => write!(f, "basic"),
            UserTier::Pro => write!(f, "pro"),
            UserTier::Enterprise => write!(f, "enterprise"),
        }
    }
}

/// KYC verification level.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum KycLevel {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "id_verified")]
    IdVerified,
    #[serde(rename = "full")]
    Full,
}

impl std::fmt::Display for KycLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KycLevel::None => write!(f, "none"),
            KycLevel::Email => write!(f, "email"),
            KycLevel::IdVerified => write!(f, "id_verified"),
            KycLevel::Full => write!(f, "full"),
        }
    }
}

/// Outcome of a compliance check.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    #[serde(rename = "pass")]
    Pass,
    #[serde(rename = "fail")]
    Fail,
    #[serde(rename = "warn")]
    Warn,
}

/// Categories of content that can be blocked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ContentCategory {
    #[serde(rename = "hate_speech")]
    HateSpeech,
    #[serde(rename = "violence")]
    Violence,
    #[serde(rename = "sexual_content")]
    SexualContent,
    #[serde(rename = "self_harm")]
    SelfHarm,
    #[serde(rename = "harassment")]
    Harassment,
    #[serde(rename = "illegal_activity")]
    IllegalActivity,
    #[serde(rename = "pii")]
    PII,
    #[serde(rename = "malware")]
    Malware,
    #[serde(rename = "disinformation")]
    Disinformation,
    #[serde(rename = "copyright_violation")]
    CopyrightViolation,
}

impl std::fmt::Display for ContentCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentCategory::HateSpeech => write!(f, "hate_speech"),
            ContentCategory::Violence => write!(f, "violence"),
            ContentCategory::SexualContent => write!(f, "sexual_content"),
            ContentCategory::SelfHarm => write!(f, "self_harm"),
            ContentCategory::Harassment => write!(f, "harassment"),
            ContentCategory::IllegalActivity => write!(f, "illegal_activity"),
            ContentCategory::PII => write!(f, "pii"),
            ContentCategory::Malware => write!(f, "malware"),
            ContentCategory::Disinformation => write!(f, "disinformation"),
            ContentCategory::CopyrightViolation => write!(f, "copyright_violation"),
        }
    }
}

/// Policy types.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PolicyType {
    #[serde(rename = "content")]
    Content,
    #[serde(rename = "usage")]
    Usage,
    #[serde(rename = "retention")]
    Retention,
    #[serde(rename = "access_control")]
    AccessControl,
}

impl std::fmt::Display for PolicyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyType::Content => write!(f, "content"),
            PolicyType::Usage => write!(f, "usage"),
            PolicyType::Retention => write!(f, "retention"),
            PolicyType::AccessControl => write!(f, "access_control"),
        }
    }
}

// ---------------------------------------------------------------------------
// Content Policy
// ---------------------------------------------------------------------------

/// Content policy rules: blocked categories, PII detection flags, toxicity thresholds.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContentPolicy {
    /// Blocked content categories.
    pub blocked_categories: Vec<ContentCategory>,
    /// Whether PII detection is enabled.
    pub pii_detection_enabled: bool,
    /// Whether to block content containing PII (vs just flagging).
    pub pii_block_on_detect: bool,
    /// Toxicity threshold (0.0 - 1.0). Content scoring above this is blocked.
    pub toxicity_threshold: f64,
}

impl Default for ContentPolicy {
    fn default() -> Self {
        Self {
            blocked_categories: vec![
                ContentCategory::HateSpeech,
                ContentCategory::Violence,
                ContentCategory::SexualContent,
                ContentCategory::SelfHarm,
                ContentCategory::IllegalActivity,
            ],
            pii_detection_enabled: true,
            pii_block_on_detect: false,
            toxicity_threshold: 0.75,
        }
    }
}

impl ContentPolicy {
    pub fn new(
        blocked_categories: Vec<ContentCategory>,
        pii_detection_enabled: bool,
        pii_block_on_detect: bool,
        toxicity_threshold: f64,
    ) -> Self {
        Self {
            blocked_categories,
            pii_detection_enabled,
            pii_block_on_detect,
            toxicity_threshold: toxicity_threshold.clamp(0.0, 1.0),
        }
    }

    /// Check whether a content category is blocked.
    pub fn is_category_blocked(&self, category: &ContentCategory) -> bool {
        self.blocked_categories.contains(category)
    }

    /// Check whether a toxicity score exceeds the threshold.
    pub fn exceeds_toxicity(&self, score: f64) -> bool {
        score > self.toxicity_threshold
    }
}

// ---------------------------------------------------------------------------
// Usage Policy
// ---------------------------------------------------------------------------

/// Usage policy rules: request limits, token limits, tier-based model access.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsagePolicy {
    /// Maximum requests per user per day.
    pub max_requests_per_user_per_day: u64,
    /// Maximum tokens per user per day.
    pub max_tokens_per_user_per_day: u64,
    /// Maximum tokens per single request.
    pub max_tokens_per_request: u32,
    /// Models allowed per user tier (tier -> set of model IDs).
    pub tier_model_access: HashMap<String, Vec<String>>,
}

impl Default for UsagePolicy {
    fn default() -> Self {
        let mut tier_model_access = HashMap::new();
        tier_model_access.insert("free".to_string(), vec!["model-small".to_string()]);
        tier_model_access.insert("basic".to_string(), vec!["model-small".to_string(), "model-medium".to_string()]);
        tier_model_access.insert("pro".to_string(), vec!["model-small".to_string(), "model-medium".to_string(), "model-large".to_string()]);
        tier_model_access.insert("enterprise".to_string(), vec!["model-small".to_string(), "model-medium".to_string(), "model-large".to_string(), "model-xlarge".to_string()]);
        Self {
            max_requests_per_user_per_day: 1000,
            max_tokens_per_user_per_day: 100_000,
            max_tokens_per_request: 4096,
            tier_model_access,
        }
    }
}

impl UsagePolicy {
    pub fn new(
        max_requests_per_user_per_day: u64,
        max_tokens_per_user_per_day: u64,
        max_tokens_per_request: u32,
        tier_model_access: HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            max_requests_per_user_per_day,
            max_tokens_per_user_per_day,
            max_tokens_per_request,
            tier_model_access,
        }
    }

    /// Check whether a user tier has access to a given model.
    pub fn tier_has_model_access(&self, tier: &str, model_id: &str) -> bool {
        self.tier_model_access
            .get(tier)
            .map(|models| models.iter().any(|m| m == model_id))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Data Retention Policy
// ---------------------------------------------------------------------------

/// Data retention policy: TTL for inference logs, GDPR erasure support.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RetentionPolicy {
    /// TTL in seconds for inference logs. 0 = retain indefinitely.
    pub log_ttl_seconds: u64,
    /// Whether GDPR right-to-erasure is enabled.
    pub gdpr_erasure_enabled: bool,
    /// Whether to anonymize logs instead of deleting them.
    pub anonymize_on_expiry: bool,
    /// Categories of data subject to retention rules.
    pub retention_categories: Vec<String>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            log_ttl_seconds: 90 * 24 * 3600, // 90 days
            gdpr_erasure_enabled: true,
            anonymize_on_expiry: false,
            retention_categories: vec![
                "inference_log".to_string(),
                "audit_event".to_string(),
                "user_data".to_string(),
            ],
        }
    }
}

impl RetentionPolicy {
    pub fn new(
        log_ttl_seconds: u64,
        gdpr_erasure_enabled: bool,
        anonymize_on_expiry: bool,
        retention_categories: Vec<String>,
    ) -> Self {
        Self {
            log_ttl_seconds,
            gdpr_erasure_enabled,
            anonymize_on_expiry,
            retention_categories,
        }
    }

    /// Check whether a log entry has expired based on its timestamp.
    pub fn is_expired(&self, created_at: chrono::DateTime<chrono::Utc>, now: chrono::DateTime<chrono::Utc>) -> bool {
        if self.log_ttl_seconds == 0 {
            return false;
        }
        let ttl = Duration::seconds(self.log_ttl_seconds as i64);
        created_at + ttl < now
    }
}

// ---------------------------------------------------------------------------
// Access Control Policy
// ---------------------------------------------------------------------------

/// Model access control: restrict by tier, region, KYC level.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AccessControlPolicy {
    /// Per-model access rules.
    pub model_rules: HashMap<String, ModelAccessRule>,
    /// Default rule when no model-specific rule exists.
    pub default_rule: ModelAccessRule,
    /// Blocked regions (ISO country codes).
    pub blocked_regions: Vec<String>,
}

/// Access rule for a single model.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelAccessRule {
    /// Minimum user tier required.
    pub min_tier: String,
    /// Minimum KYC level required.
    pub min_kyc: String,
    /// Regions where this model is available (empty = all).
    pub allowed_regions: Vec<String>,
    /// Whether the model requires explicit opt-in.
    pub requires_opt_in: bool,
}

impl Default for ModelAccessRule {
    fn default() -> Self {
        Self {
            min_tier: "free".to_string(),
            min_kyc: "none".to_string(),
            allowed_regions: vec![],
            requires_opt_in: false,
        }
    }
}

impl Default for AccessControlPolicy {
    fn default() -> Self {
        Self {
            model_rules: HashMap::new(),
            default_rule: ModelAccessRule::default(),
            blocked_regions: vec![],
        }
    }
}

impl AccessControlPolicy {
    pub fn new(
        model_rules: HashMap<String, ModelAccessRule>,
        default_rule: ModelAccessRule,
        blocked_regions: Vec<String>,
    ) -> Self {
        Self {
            model_rules,
            default_rule,
            blocked_regions,
        }
    }

    /// Check if a user with given tier, KYC level, and region can access a model.
    pub fn check_access(
        &self,
        model_id: &str,
        user_tier: &str,
        user_kyc: &str,
        user_region: &str,
    ) -> AccessCheckResult {
        // Check blocked regions first
        if self.blocked_regions.contains(&user_region.to_uppercase()) {
            return AccessCheckResult::Denied {
                reason: format!("region '{}' is blocked", user_region),
            };
        }

        let rule = self
            .model_rules
            .get(model_id)
            .unwrap_or(&self.default_rule);

        // Check tier
        if !tier_gte(user_tier, &rule.min_tier) {
            return AccessCheckResult::Denied {
                reason: format!(
                    "tier '{}' insufficient, requires '{}'",
                    user_tier, rule.min_tier
                ),
            };
        }

        // Check KYC
        if !kyc_gte(user_kyc, &rule.min_kyc) {
            return AccessCheckResult::Denied {
                reason: format!(
                    "KYC level '{}' insufficient, requires '{}'",
                    user_kyc, rule.min_kyc
                ),
            };
        }

        // Check region allowlist (empty = all allowed)
        if !rule.allowed_regions.is_empty()
            && !rule
                .allowed_regions
                .iter()
                .any(|r| r.eq_ignore_ascii_case(user_region))
        {
            return AccessCheckResult::Denied {
                reason: format!("region '{}' not in allowed list for model '{}'", user_region, model_id),
            };
        }

        AccessCheckResult::Allowed
    }
}

/// Result of an access control check.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum AccessCheckResult {
    Allowed,
    Denied { reason: String },
}

/// Tier ordering: free < basic < pro < enterprise.
fn tier_gte(actual: &str, required: &str) -> bool {
    let tier_score = |t: &str| -> u8 {
        match t.to_lowercase().as_str() {
            "free" => 0,
            "basic" => 1,
            "pro" => 2,
            "enterprise" => 3,
            _ => 0,
        }
    };
    tier_score(actual) >= tier_score(required)
}

/// KYC ordering: none < email < id_verified < full.
fn kyc_gte(actual: &str, required: &str) -> bool {
    let kyc_score = |k: &str| -> u8 {
        match k.to_lowercase().as_str() {
            "none" => 0,
            "email" => 1,
            "id_verified" => 2,
            "full" => 3,
            _ => 0,
        }
    };
    kyc_score(actual) >= kyc_score(required)
}

// ---------------------------------------------------------------------------
// Compliance Policy (top-level)
// ---------------------------------------------------------------------------

/// A compliance policy that wraps content, usage, retention, or access-control rules.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompliancePolicy {
    /// Unique policy identifier.
    pub id: String,
    /// Human-readable policy name.
    pub name: String,
    /// Description.
    pub description: String,
    /// The type of this policy.
    pub policy_type: PolicyType,
    /// Whether this policy is currently active.
    pub is_active: bool,
    /// Content policy rules (set when policy_type == Content).
    pub content: Option<ContentPolicy>,
    /// Usage policy rules (set when policy_type == Usage).
    pub usage: Option<UsagePolicy>,
    /// Retention policy rules (set when policy_type == Retention).
    pub retention: Option<RetentionPolicy>,
    /// Access control rules (set when policy_type == AccessControl).
    pub access_control: Option<AccessControlPolicy>,
    /// When this policy was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this policy was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl CompliancePolicy {
    /// Create a new content compliance policy.
    pub fn new_content(id: impl Into<String>, name: impl Into<String>, description: impl Into<String>, content: ContentPolicy) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            policy_type: PolicyType::Content,
            is_active: true,
            content: Some(content),
            usage: None,
            retention: None,
            access_control: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new usage compliance policy.
    pub fn new_usage(id: impl Into<String>, name: impl Into<String>, description: impl Into<String>, usage: UsagePolicy) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            policy_type: PolicyType::Usage,
            is_active: true,
            content: None,
            usage: Some(usage),
            retention: None,
            access_control: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new retention compliance policy.
    pub fn new_retention(id: impl Into<String>, name: impl Into<String>, description: impl Into<String>, retention: RetentionPolicy) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            policy_type: PolicyType::Retention,
            is_active: true,
            content: None,
            usage: None,
            retention: Some(retention),
            access_control: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new access-control compliance policy.
    pub fn new_access_control(id: impl Into<String>, name: impl Into<String>, description: impl Into<String>, access_control: AccessControlPolicy) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            policy_type: PolicyType::AccessControl,
            is_active: true,
            content: None,
            usage: None,
            retention: None,
            access_control: Some(access_control),
            created_at: now,
            updated_at: now,
        }
    }
}

// ---------------------------------------------------------------------------
// Audit Event
// ---------------------------------------------------------------------------

/// A single audit event recorded for every compliance check.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditEvent {
    /// Unique event identifier.
    pub id: String,
    /// The type of compliance check performed.
    pub check_type: String,
    /// The policy ID that was evaluated.
    pub policy_id: String,
    /// The user being checked (if applicable).
    pub user_id: Option<String>,
    /// The model involved (if applicable).
    pub model_id: Option<String>,
    /// The outcome of the check.
    pub outcome: CheckOutcome,
    /// Severity of the event.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// When the event occurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional structured metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl AuditEvent {
    pub fn new(
        check_type: impl Into<String>,
        policy_id: impl Into<String>,
        outcome: CheckOutcome,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            check_type: check_type.into(),
            policy_id: policy_id.into(),
            user_id: None,
            model_id: None,
            outcome,
            severity,
            message: message.into(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = Some(model_id.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

// ---------------------------------------------------------------------------
// Inference Log Entry (for retention tracking)
// ---------------------------------------------------------------------------

/// An inference log entry subject to retention policies.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InferenceLog {
    /// Unique log identifier.
    pub id: String,
    /// The user who made the request.
    pub user_id: String,
    /// The model used.
    pub model_id: String,
    /// Number of tokens consumed.
    pub tokens: u32,
    /// When this log entry was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Whether this entry has been marked for deletion.
    pub marked_for_deletion: bool,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl InferenceLog {
    pub fn new(
        user_id: impl Into<String>,
        model_id: impl Into<String>,
        tokens: u32,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.into(),
            model_id: model_id.into(),
            tokens,
            created_at: Utc::now(),
            marked_for_deletion: false,
            metadata: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Risk Score
// ---------------------------------------------------------------------------

/// Per-user or per-request risk assessment.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RiskScore {
    /// The entity being scored (user ID or request ID).
    pub entity_id: String,
    /// Whether this is a user-level or request-level score.
    pub score_type: RiskScoreType,
    /// Overall risk score 0.0 (safe) to 1.0 (dangerous).
    pub score: f64,
    /// Contributing factors.
    pub factors: Vec<RiskFactor>,
    /// When the score was computed.
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Type of risk score.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RiskScoreType {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "request")]
    Request,
}

/// A single factor contributing to a risk score.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RiskFactor {
    /// Name of the factor.
    pub name: String,
    /// This factor's contribution to the overall score (0.0 - 1.0).
    pub weight: f64,
    /// Description of why this factor was flagged.
    pub description: String,
}

impl RiskScore {
    pub fn new(entity_id: impl Into<String>, score_type: RiskScoreType) -> Self {
        Self {
            entity_id: entity_id.into(),
            score_type,
            score: 0.0,
            factors: vec![],
            computed_at: Utc::now(),
        }
    }

    /// Add a risk factor and recompute the weighted average score.
    pub fn with_factor(mut self, name: impl Into<String>, weight: f64, description: impl Into<String>) -> Self {
        self.factors.push(RiskFactor {
            name: name.into(),
            weight: weight.clamp(0.0, 1.0),
            description: description.into(),
        });
        self.recompute_score();
        self
    }

    /// Recompute the overall score as the weighted average of factors.
    fn recompute_score(&mut self) {
        if self.factors.is_empty() {
            self.score = 0.0;
            return;
        }
        let total: f64 = self.factors.iter().map(|f| f.weight).sum();
        self.score = (total / self.factors.len() as f64).clamp(0.0, 1.0);
    }

    /// Classify the risk level based on score thresholds.
    pub fn risk_level(&self) -> &str {
        if self.score < 0.25 {
            "low"
        } else if self.score < 0.5 {
            "medium"
        } else if self.score < 0.75 {
            "high"
        } else {
            "critical"
        }
    }
}

// ---------------------------------------------------------------------------
// Compliance Report
// ---------------------------------------------------------------------------

/// A compliance report snapshot.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComplianceReport {
    /// Report generation timestamp.
    pub generated_at: chrono::DateTime<chrono::Utc>,
    /// Total compliance checks performed.
    pub total_checks: u64,
    /// Total checks passed.
    pub checks_passed: u64,
    /// Total checks failed.
    pub checks_failed: u64,
    /// Total checks warned.
    pub checks_warned: u64,
    /// Compliance pass rate as percentage (0.0 - 100.0).
    pub pass_rate: f64,
    /// Number of active policies.
    pub active_policies: usize,
    /// Total violations recorded.
    pub total_violations: u64,
    /// Violations by policy type.
    pub violations_by_type: HashMap<String, u64>,
    /// Number of inference logs currently retained.
    pub retained_logs: usize,
    /// Users flagged as high-risk.
    pub high_risk_users: usize,
}

// ---------------------------------------------------------------------------
// User Usage Tracking
// ---------------------------------------------------------------------------

/// Tracks per-user request and token counts for usage policy enforcement.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserUsageRecord {
    pub user_id: String,
    pub request_count: u64,
    pub token_count: u64,
    pub period_start: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Compliance Engine
// ---------------------------------------------------------------------------

/// Core compliance engine for AI model inference on the Xergon network.
///
/// Thread-safe via DashMap and AtomicU64. All public methods are &self.
pub struct ComplianceEngine {
    /// All registered compliance policies, keyed by policy ID.
    policies: DashMap<String, CompliancePolicy>,

    /// Audit event log, keyed by event ID.
    audit_log: DashMap<String, AuditEvent>,

    /// Inference logs subject to retention, keyed by log ID.
    inference_logs: DashMap<String, InferenceLog>,

    /// Per-user usage tracking, keyed by user ID.
    user_usage: DashMap<String, UserUsageRecord>,

    /// Per-user risk scores, keyed by user ID.
    user_risk_scores: DashMap<String, RiskScore>,

    /// Per-request risk scores, keyed by request ID.
    request_risk_scores: DashMap<String, RiskScore>,

    /// Users who have opted in to specific models, keyed by (user_id, model_id).
    opt_ins: DashMap<String, bool>,

    // ---- Atomic counters ----
    /// Total compliance checks performed.
    total_checks: AtomicU64,
    /// Total checks that passed.
    checks_passed: AtomicU64,
    /// Total checks that failed.
    checks_failed: AtomicU64,
    /// Total checks that warned.
    checks_warned: AtomicU64,
    /// Total violations recorded.
    total_violations: AtomicU64,
    /// Violations by policy type.
    violations_by_type: DashMap<String, AtomicU64>,
}

impl ComplianceEngine {
    /// Creates a new, empty compliance engine.
    pub fn new() -> Self {
        Self {
            policies: DashMap::new(),
            audit_log: DashMap::new(),
            inference_logs: DashMap::new(),
            user_usage: DashMap::new(),
            user_risk_scores: DashMap::new(),
            request_risk_scores: DashMap::new(),
            opt_ins: DashMap::new(),
            total_checks: AtomicU64::new(0),
            checks_passed: AtomicU64::new(0),
            checks_failed: AtomicU64::new(0),
            checks_warned: AtomicU64::new(0),
            total_violations: AtomicU64::new(0),
            violations_by_type: DashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Policy Management
    // -----------------------------------------------------------------------

    /// Create a new compliance policy. Returns error if a policy with the same
    /// ID already exists.
    pub fn create_policy(&self, policy: CompliancePolicy) -> Result<CompliancePolicy, String> {
        let id = policy.id.clone();
        if self.policies.contains_key(&id) {
            return Err(format!("Policy '{}' already exists", id));
        }
        self.policies.insert(id.clone(), policy.clone());
        Ok(policy)
    }

    /// Update an existing compliance policy. Returns error if the policy does
    /// not exist.
    pub fn update_policy(&self, policy: CompliancePolicy) -> Result<CompliancePolicy, String> {
        let id = policy.id.clone();
        if !self.policies.contains_key(&id) {
            return Err(format!("Policy '{}' not found", id));
        }
        let mut updated = policy.clone();
        updated.updated_at = Utc::now();
        self.policies.insert(id, updated.clone());
        Ok(updated)
    }

    /// Delete a compliance policy by ID. Returns the deleted policy.
    pub fn delete_policy(&self, policy_id: &str) -> Result<CompliancePolicy, String> {
        self.policies
            .remove(policy_id)
            .map(|(_, p)| p)
            .ok_or_else(|| format!("Policy '{}' not found", policy_id))
    }

    /// Get a compliance policy by ID.
    pub fn get_policy(&self, policy_id: &str) -> Option<CompliancePolicy> {
        self.policies.get(policy_id).map(|r| r.value().clone())
    }

    /// List all policies, optionally filtered by type and active status.
    pub fn list_policies(&self, policy_type: Option<&PolicyType>, active_only: bool) -> Vec<CompliancePolicy> {
        self.policies
            .iter()
            .filter(|r| {
                if let Some(pt) = policy_type {
                    if &r.value().policy_type != pt {
                        return false;
                    }
                }
                if active_only && !r.value().is_active {
                    return false;
                }
                true
            })
            .map(|r| r.value().clone())
            .collect()
    }

    /// Activate or deactivate a policy.
    pub fn set_policy_active(&self, policy_id: &str, active: bool) -> Result<(), String> {
        let mut policy = self
            .policies
            .get_mut(policy_id)
            .ok_or_else(|| format!("Policy '{}' not found", policy_id))?;
        policy.is_active = active;
        policy.updated_at = Utc::now();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Content Policy Checks
    // -----------------------------------------------------------------------

    /// Check content against all active content policies.
    /// Returns (pass/fail, list of violations as messages).
    pub fn check_content(
        &self,
        categories_detected: &[ContentCategory],
        toxicity_score: f64,
        pii_detected: bool,
    ) -> ContentCheckResult {
        self.total_checks.fetch_add(1, Ordering::Relaxed);

        let content_policies: Vec<CompliancePolicy> = self
            .policies
            .iter()
            .filter(|r| {
                r.value().is_active && r.value().policy_type == PolicyType::Content
            })
            .map(|r| r.value().clone())
            .collect();

        if content_policies.is_empty() {
            self.checks_passed.fetch_add(1, Ordering::Relaxed);
            return ContentCheckResult {
                passed: true,
                blocked_categories: vec![],
                toxicity_violation: false,
                pii_violation: false,
                messages: vec![],
            };
        }

        let mut blocked_categories: Vec<ContentCategory> = vec![];
        let mut toxicity_violation = false;
        let mut pii_violation = false;
        let mut messages: Vec<String> = vec![];

        for policy in &content_policies {
            let content = match &policy.content {
                Some(c) => c,
                None => continue,
            };

            for cat in categories_detected {
                if content.is_category_blocked(cat) {
                    if !blocked_categories.contains(cat) {
                        blocked_categories.push(cat.clone());
                        messages.push(format!("Content category '{}' is blocked by policy '{}'", cat, policy.id));
                    }
                }
            }

            if content.exceeds_toxicity(toxicity_score) {
                toxicity_violation = true;
                messages.push(format!(
                    "Toxicity score {:.2} exceeds threshold {:.2} (policy '{}')",
                    toxicity_score, content.toxicity_threshold, policy.id
                ));
            }

            if pii_detected && content.pii_detection_enabled && content.pii_block_on_detect {
                pii_violation = true;
                messages.push(format!("PII detected and blocking is enabled (policy '{}')", policy.id));
            }

            // Record audit event per policy checked
            let outcome = if blocked_categories.is_empty() && !toxicity_violation && !pii_violation {
                CheckOutcome::Pass
            } else {
                CheckOutcome::Fail
            };
            self.record_audit(AuditEvent::new(
                "content_check",
                &policy.id,
                outcome.clone(),
                if outcome == CheckOutcome::Fail { Severity::High } else { Severity::Info },
                &messages.join("; "),
            ));
        }

        let passed = blocked_categories.is_empty() && !toxicity_violation && !pii_violation;

        if passed {
            self.checks_passed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.checks_failed.fetch_add(1, Ordering::Relaxed);
            self.total_violations.fetch_add(messages.len() as u64, Ordering::Relaxed);
            self.increment_violations_by_type("content");
        }

        ContentCheckResult {
            passed,
            blocked_categories,
            toxicity_violation,
            pii_violation,
            messages,
        }
    }

    // -----------------------------------------------------------------------
    // Usage Policy Checks
    // -----------------------------------------------------------------------

    /// Check whether a user's current usage is within policy limits.
    /// Also increments usage counters.
    pub fn check_usage(
        &self,
        user_id: &str,
        requested_tokens: u32,
    ) -> UsageCheckResult {
        self.total_checks.fetch_add(1, Ordering::Relaxed);

        let usage_policies: Vec<CompliancePolicy> = self
            .policies
            .iter()
            .filter(|r| {
                r.value().is_active && r.value().policy_type == PolicyType::Usage
            })
            .map(|r| r.value().clone())
            .collect();

        if usage_policies.is_empty() {
            // No usage policies -> increment counters but allow
            self.increment_usage(user_id, requested_tokens);
            self.checks_passed.fetch_add(1, Ordering::Relaxed);
            return UsageCheckResult {
                passed: true,
                requests_allowed: true,
                tokens_allowed: true,
                model_access_allowed: true,
                messages: vec![],
            };
        }

        let mut messages: Vec<String> = vec![];
        let mut requests_allowed = true;
        let mut tokens_allowed = true;
        let model_access_allowed = true;

        // Get or create usage record
        let current_requests = self.get_user_request_count(user_id);
        let current_tokens = self.get_user_token_count(user_id);

        for policy in &usage_policies {
            let usage = match &policy.usage {
                Some(u) => u,
                None => continue,
            };

            // Check request limit
            if current_requests >= usage.max_requests_per_user_per_day {
                requests_allowed = false;
                messages.push(format!(
                    "User '{}' exceeded max requests ({}/{}) (policy '{}')",
                    user_id,
                    current_requests,
                    usage.max_requests_per_user_per_day,
                    policy.id
                ));
            }

            // Check token limit
            if current_tokens + requested_tokens as u64 > usage.max_tokens_per_user_per_day {
                tokens_allowed = false;
                messages.push(format!(
                    "User '{}' would exceed max tokens ({}+{} > {}) (policy '{}')",
                    user_id,
                    current_tokens,
                    requested_tokens,
                    usage.max_tokens_per_user_per_day,
                    policy.id
                ));
            }

            // Check per-request token limit
            if requested_tokens > usage.max_tokens_per_request {
                tokens_allowed = false;
                messages.push(format!(
                    "Request tokens {} exceeds per-request max {} (policy '{}')",
                    requested_tokens,
                    usage.max_tokens_per_request,
                    policy.id
                ));
            }

            let outcome = if requests_allowed && tokens_allowed {
                CheckOutcome::Pass
            } else {
                CheckOutcome::Fail
            };
            self.record_audit(AuditEvent::new(
                "usage_check",
                &policy.id,
                outcome.clone(),
                if outcome == CheckOutcome::Fail { Severity::Medium } else { Severity::Info },
                &messages.join("; "),
            ).with_user(user_id));
        }

        let passed = requests_allowed && tokens_allowed && model_access_allowed;

        if passed {
            self.increment_usage(user_id, requested_tokens);
            self.checks_passed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.checks_failed.fetch_add(1, Ordering::Relaxed);
            self.total_violations.fetch_add(messages.len() as u64, Ordering::Relaxed);
            self.increment_violations_by_type("usage");
        }

        UsageCheckResult {
            passed,
            requests_allowed,
            tokens_allowed,
            model_access_allowed,
            messages,
        }
    }

    /// Check whether a user tier can access a specific model.
    pub fn check_model_access_by_tier(
        &self,
        user_id: &str,
        user_tier: &str,
        model_id: &str,
    ) -> ModelAccessCheckResult {
        self.total_checks.fetch_add(1, Ordering::Relaxed);

        let usage_policies: Vec<CompliancePolicy> = self
            .policies
            .iter()
            .filter(|r| {
                r.value().is_active && r.value().policy_type == PolicyType::Usage
            })
            .map(|r| r.value().clone())
            .collect();

        for policy in &usage_policies {
            let usage = match &policy.usage {
                Some(u) => u,
                None => continue,
            };

            if !usage.tier_has_model_access(user_tier, model_id) {
                self.checks_failed.fetch_add(1, Ordering::Relaxed);
                self.total_violations.fetch_add(1, Ordering::Relaxed);
                self.increment_violations_by_type("usage");
                self.record_audit(AuditEvent::new(
                    "model_access_check",
                    &policy.id,
                    CheckOutcome::Fail,
                    Severity::Medium,
                    format!(
                        "User '{}' (tier '{}') cannot access model '{}' per usage policy",
                        user_id, user_tier, model_id
                    ),
                ).with_user(user_id).with_model(model_id));
                return ModelAccessCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Tier '{}' does not have access to model '{}'",
                        user_tier, model_id
                    )),
                };
            }
        }

        self.checks_passed.fetch_add(1, Ordering::Relaxed);
        self.record_audit(AuditEvent::new(
            "model_access_check",
            "usage_policy",
            CheckOutcome::Pass,
            Severity::Info,
            format!("User '{}' (tier '{}') granted access to model '{}'", user_id, user_tier, model_id),
        ).with_user(user_id).with_model(model_id));

        ModelAccessCheckResult {
            allowed: true,
            reason: None,
        }
    }

    // -----------------------------------------------------------------------
    // Data Retention
    // -----------------------------------------------------------------------

    /// Record an inference log entry.
    pub fn record_inference_log(&self, user_id: &str, model_id: &str, tokens: u32) -> InferenceLog {
        let log = InferenceLog::new(user_id, model_id, tokens);
        self.inference_logs.insert(log.id.clone(), log.clone());
        log
    }

    /// Expire inference logs based on active retention policies.
    /// Returns the number of logs expired/deleted.
    pub fn expire_logs(&self) -> usize {
        let retention_policies: Vec<CompliancePolicy> = self
            .policies
            .iter()
            .filter(|r| {
                r.value().is_active && r.value().policy_type == PolicyType::Retention
            })
            .map(|r| r.value().clone())
            .collect();

        if retention_policies.is_empty() {
            return 0;
        }

        let now = Utc::now();
        let mut expired_ids: Vec<String> = vec![];

        for entry in self.inference_logs.iter() {
            let log = entry.value();
            if log.marked_for_deletion {
                continue;
            }
            for policy in &retention_policies {
                let retention = match &policy.retention {
                    Some(r) => r,
                    None => continue,
                };
                if retention.is_expired(log.created_at, now) {
                    expired_ids.push(log.id.clone());
                    break;
                }
            }
        }

        let count = expired_ids.len();
        for id in expired_ids {
            if let Some(mut entry) = self.inference_logs.get_mut(&id) {
                entry.marked_for_deletion = true;
            }
        }
        count
    }

    /// Purge all logs marked for deletion. Returns count purged.
    pub fn purge_expired_logs(&self) -> usize {
        let to_purge: Vec<String> = self
            .inference_logs
            .iter()
            .filter(|r| r.value().marked_for_deletion)
            .map(|r| r.key().clone())
            .collect();
        let count = to_purge.len();
        for id in to_purge {
            self.inference_logs.remove(&id);
        }
        count
    }

    /// GDPR right-to-erasure: delete all inference logs for a specific user.
    /// Returns the number of logs deleted.
    pub fn gdpr_erase_user(&self, user_id: &str) -> Result<usize, String> {
        // Check if any active retention policy enables GDPR erasure
        let gdpr_enabled = self
            .policies
            .iter()
            .any(|r| {
                r.value().is_active
                    && r.value().policy_type == PolicyType::Retention
                    && r.value()
                        .retention
                        .as_ref()
                        .map(|ret| ret.gdpr_erasure_enabled)
                        .unwrap_or(false)
            });

        if !gdpr_enabled {
            return Err("GDPR erasure is not enabled by any active retention policy".to_string());
        }

        let to_delete: Vec<String> = self
            .inference_logs
            .iter()
            .filter(|r| r.value().user_id == user_id)
            .map(|r| r.key().clone())
            .collect();

        let count = to_delete.len();
        for id in to_delete {
            self.inference_logs.remove(&id);
        }

        // Also erase user usage records
        self.user_usage.remove(user_id);

        // Also erase user risk scores
        self.user_risk_scores.remove(user_id);

        // Record audit event
        self.record_audit(AuditEvent::new(
            "gdpr_erasure",
            "retention_policy",
            CheckOutcome::Pass,
            Severity::High,
            format!("GDPR erasure completed for user '{}', {} logs deleted", user_id, count),
        ).with_user(user_id));

        Ok(count)
    }

    /// Get all inference logs for a user.
    pub fn get_user_logs(&self, user_id: &str) -> Vec<InferenceLog> {
        self.inference_logs
            .iter()
            .filter(|r| r.value().user_id == user_id && !r.value().marked_for_deletion)
            .map(|r| r.value().clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Model Access Control
    // -----------------------------------------------------------------------

    /// Full access control check: tier, KYC, region, opt-in.
    pub fn check_model_access(
        &self,
        user_id: &str,
        user_tier: &str,
        user_kyc: &str,
        user_region: &str,
        model_id: &str,
    ) -> AccessCheckResult {
        self.total_checks.fetch_add(1, Ordering::Relaxed);

        let ac_policies: Vec<CompliancePolicy> = self
            .policies
            .iter()
            .filter(|r| {
                r.value().is_active && r.value().policy_type == PolicyType::AccessControl
            })
            .map(|r| r.value().clone())
            .collect();

        if ac_policies.is_empty() {
            self.checks_passed.fetch_add(1, Ordering::Relaxed);
            return AccessCheckResult::Allowed;
        }

        for policy in &ac_policies {
            let ac = match &policy.access_control {
                Some(a) => a,
                None => continue,
            };

            let result = ac.check_access(model_id, user_tier, user_kyc, user_region);

            // Check opt-in requirement
            if let AccessCheckResult::Allowed = &result {
                let rule = ac.model_rules.get(model_id).unwrap_or(&ac.default_rule);
                if rule.requires_opt_in {
                    let opt_key = format!("{}:{}", user_id, model_id);
                    if !self.opt_ins.get(&opt_key).map(|r| *r.value()).unwrap_or(false) {
                        let result = AccessCheckResult::Denied {
                            reason: format!("Model '{}' requires opt-in from user '{}'", model_id, user_id),
                        };
                        self.checks_failed.fetch_add(1, Ordering::Relaxed);
                        self.total_violations.fetch_add(1, Ordering::Relaxed);
                        self.increment_violations_by_type("access_control");
                        self.record_audit(AuditEvent::new(
                            "access_control_check",
                            &policy.id,
                            CheckOutcome::Fail,
                            Severity::Medium,
                            result.clone().denied_reason().unwrap_or_default(),
                        ).with_user(user_id).with_model(model_id));
                        return result;
                    }
                }
            }

            match &result {
                AccessCheckResult::Allowed => {
                    self.checks_passed.fetch_add(1, Ordering::Relaxed);
                    self.record_audit(AuditEvent::new(
                        "access_control_check",
                        &policy.id,
                        CheckOutcome::Pass,
                        Severity::Info,
                        format!("Access granted: user '{}' -> model '{}'", user_id, model_id),
                    ).with_user(user_id).with_model(model_id));
                }
                AccessCheckResult::Denied { reason } => {
                    self.checks_failed.fetch_add(1, Ordering::Relaxed);
                    self.total_violations.fetch_add(1, Ordering::Relaxed);
                    self.increment_violations_by_type("access_control");
                    self.record_audit(AuditEvent::new(
                        "access_control_check",
                        &policy.id,
                        CheckOutcome::Fail,
                        Severity::High,
                        reason,
                    ).with_user(user_id).with_model(model_id));
                    return result;
                }
            }
        }

        AccessCheckResult::Allowed
    }

    /// User opts in to a model.
    pub fn opt_in_model(&self, user_id: &str, model_id: &str) {
        let key = format!("{}:{}", user_id, model_id);
        self.opt_ins.insert(key, true);
    }

    /// User opts out of a model.
    pub fn opt_out_model(&self, user_id: &str, model_id: &str) {
        let key = format!("{}:{}", user_id, model_id);
        self.opt_ins.remove(&key);
    }

    // -----------------------------------------------------------------------
    // Audit Event Recording
    // -----------------------------------------------------------------------

    /// Record an audit event. Every compliance check should call this.
    pub fn record_audit(&self, event: AuditEvent) -> AuditEvent {
        self.audit_log.insert(event.id.clone(), event.clone());
        event
    }

    /// Get audit events, optionally filtered by policy_id, user_id, or outcome.
    pub fn get_audit_events(
        &self,
        policy_id: Option<&str>,
        user_id: Option<&str>,
        outcome: Option<&CheckOutcome>,
    ) -> Vec<AuditEvent> {
        self.audit_log
            .iter()
            .filter(|r| {
                let e = r.value();
                if let Some(pid) = policy_id {
                    if e.policy_id != pid {
                        return false;
                    }
                }
                if let Some(uid) = user_id {
                    if e.user_id.as_deref() != Some(uid) {
                        return false;
                    }
                }
                if let Some(o) = outcome {
                    if &e.outcome != o {
                        return false;
                    }
                }
                true
            })
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get audit events for a specific time range.
    pub fn get_audit_events_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Vec<AuditEvent> {
        self.audit_log
            .iter()
            .filter(|r| {
                let ts = r.value().timestamp;
                ts >= start && ts <= end
            })
            .map(|r| r.value().clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Compliance Reporting
    // -----------------------------------------------------------------------

    /// Generate a compliance report snapshot.
    pub fn generate_report(&self) -> ComplianceReport {
        let total = self.total_checks.load(Ordering::Relaxed);
        let passed = self.checks_passed.load(Ordering::Relaxed);
        let failed = self.checks_failed.load(Ordering::Relaxed);
        let warned = self.checks_warned.load(Ordering::Relaxed);

        let pass_rate = if total > 0 {
            (passed as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let active_policies = self
            .policies
            .iter()
            .filter(|r| r.value().is_active)
            .count();

        let violations_by_type: HashMap<String, u64> = self
            .violations_by_type
            .iter()
            .map(|r| (r.key().clone(), r.value().load(Ordering::Relaxed)))
            .collect();

        let retained_logs = self
            .inference_logs
            .iter()
            .filter(|r| !r.value().marked_for_deletion)
            .count();

        let high_risk_users = self
            .user_risk_scores
            .iter()
            .filter(|r| r.value().score >= 0.75)
            .count();

        ComplianceReport {
            generated_at: Utc::now(),
            total_checks: total,
            checks_passed: passed,
            checks_failed: failed,
            checks_warned: warned,
            pass_rate,
            active_policies,
            total_violations: self.total_violations.load(Ordering::Relaxed),
            violations_by_type,
            retained_logs,
            high_risk_users,
        }
    }

    // -----------------------------------------------------------------------
    // Risk Scoring
    // -----------------------------------------------------------------------

    /// Compute or update a user's risk score based on behavior patterns.
    pub fn compute_user_risk(&self, user_id: &str) -> RiskScore {
        let mut score = RiskScore::new(user_id, RiskScoreType::User);

        // Factor 1: Recent violation count (from audit log)
        let violation_count = self
            .audit_log
            .iter()
            .filter(|r| {
                let e = r.value();
                e.user_id.as_deref() == Some(user_id)
                    && e.outcome == CheckOutcome::Fail
            })
            .count() as f64;

        if violation_count > 0.0 {
            let weight = (violation_count * 0.15).min(1.0);
            score = score.with_factor(
                "violation_history",
                weight,
                format!("{} violations recorded", violation_count as u64),
            );
        }

        // Factor 2: Usage proximity to limits
        if let Some(usage) = self.user_usage.get(user_id) {
            let u = usage.value();
            // Find the strictest usage policy
            let max_req = self
                .policies
                .iter()
                .filter(|r| {
                    r.value().is_active && r.value().policy_type == PolicyType::Usage
                })
                .filter_map(|r| r.value().usage.clone())
                .map(|u| u.max_requests_per_user_per_day)
                .min()
                .unwrap_or(u64::MAX);

            if max_req > 0 {
                let usage_ratio = u.request_count as f64 / max_req as f64;
                if usage_ratio > 0.8 {
                    score = score.with_factor(
                        "high_usage",
                        (usage_ratio - 0.8) * 5.0, // 0.0 at 80%, 1.0 at 100%
                        format!("Usage at {:.0}% of daily limit", usage_ratio * 100.0),
                    );
                }
            }
        }

        // Factor 3: Opt-in to restricted models (slightly suspicious for free tier)
        let restricted_opt_ins = self
            .opt_ins
            .iter()
            .filter(|r| {
                let key = r.key();
                key.starts_with(&format!("{}:", user_id))
            })
            .count() as f64;

        if restricted_opt_ins > 0.0 {
            score = score.with_factor(
                "restricted_model_opt_in",
                (restricted_opt_ins * 0.2).min(0.6),
                format!("Opted in to {} restricted models", restricted_opt_ins as u64),
            );
        }

        score.computed_at = Utc::now();
        self.user_risk_scores
            .insert(user_id.to_string(), score.clone());
        score
    }

    /// Compute a per-request risk score.
    pub fn compute_request_risk(
        &self,
        request_id: &str,
        user_id: &str,
        _model_id: &str,
        toxicity_score: f64,
        pii_detected: bool,
        token_count: u32,
    ) -> RiskScore {
        let mut score = RiskScore::new(request_id, RiskScoreType::Request);

        // Factor 1: toxicity
        if toxicity_score > 0.5 {
            score = score.with_factor(
                "toxicity",
                toxicity_score,
                format!("Toxicity score: {:.2}", toxicity_score),
            );
        }

        // Factor 2: PII detected
        if pii_detected {
            score = score.with_factor("pii_detected", 0.6, "PII detected in request");
        }

        // Factor 3: user's historical risk
        if let Some(user_score) = self.user_risk_scores.get(user_id) {
            if user_score.value().score > 0.5 {
                score = score.with_factor(
                    "user_risk_history",
                    user_score.value().score,
                    format!("User risk score: {:.2}", user_score.value().score),
                );
            }
        }

        // Factor 4: large token count
        if token_count > 2000 {
            score = score.with_factor(
                "large_request",
                ((token_count - 2000) as f64 / 2000.0).min(0.5),
                format!("Large request: {} tokens", token_count),
            );
        }

        score.computed_at = Utc::now();
        self.request_risk_scores
            .insert(request_id.to_string(), score.clone());
        score
    }

    /// Get a user's current risk score.
    pub fn get_user_risk(&self, user_id: &str) -> Option<RiskScore> {
        self.user_risk_scores.get(user_id).map(|r| r.value().clone())
    }

    /// Get a request's risk score.
    pub fn get_request_risk(&self, request_id: &str) -> Option<RiskScore> {
        self.request_risk_scores
            .get(request_id)
            .map(|r| r.value().clone())
    }

    /// Get all high-risk users (score >= 0.75).
    pub fn get_high_risk_users(&self) -> Vec<RiskScore> {
        self.user_risk_scores
            .iter()
            .filter(|r| r.value().score >= 0.75)
            .map(|r| r.value().clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn increment_usage(&self, user_id: &str, tokens: u32) {
        let mut entry = self.user_usage.entry(user_id.to_string()).or_insert_with(|| {
            UserUsageRecord {
                user_id: user_id.to_string(),
                request_count: 0,
                token_count: 0,
                period_start: Utc::now(),
            }
        });
        entry.request_count += 1;
        entry.token_count += tokens as u64;
    }

    fn get_user_request_count(&self, user_id: &str) -> u64 {
        self.user_usage
            .get(user_id)
            .map(|r| r.value().request_count)
            .unwrap_or(0)
    }

    fn get_user_token_count(&self, user_id: &str) -> u64 {
        self.user_usage
            .get(user_id)
            .map(|r| r.value().token_count)
            .unwrap_or(0)
    }

    fn increment_violations_by_type(&self, policy_type: &str) {
        let entry = self
            .violations_by_type
            .entry(policy_type.to_string())
            .or_insert_with(|| AtomicU64::new(0));
        entry.fetch_add(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Check result types
// ---------------------------------------------------------------------------

/// Result of a content compliance check.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContentCheckResult {
    pub passed: bool,
    pub blocked_categories: Vec<ContentCategory>,
    pub toxicity_violation: bool,
    pub pii_violation: bool,
    pub messages: Vec<String>,
}

/// Result of a usage compliance check.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageCheckResult {
    pub passed: bool,
    pub requests_allowed: bool,
    pub tokens_allowed: bool,
    pub model_access_allowed: bool,
    pub messages: Vec<String>,
}

/// Result of a model access check (tier-based).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelAccessCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl AccessCheckResult {
    /// Extract the denial reason, if any.
    pub fn denied_reason(&self) -> Option<&str> {
        match self {
            AccessCheckResult::Denied { reason } => Some(reason),
            AccessCheckResult::Allowed => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> ComplianceEngine {
        ComplianceEngine::new()
    }

    fn make_content_policy() -> CompliancePolicy {
        CompliancePolicy::new_content(
            "content-1",
            "Default Content Policy",
            "Blocks hate speech, violence, PII",
            ContentPolicy::new(
                vec![
                    ContentCategory::HateSpeech,
                    ContentCategory::Violence,
                    ContentCategory::SexualContent,
                ],
                true,   // pii_detection_enabled
                false,  // pii_block_on_detect
                0.75,   // toxicity_threshold
            ),
        )
    }

    fn make_usage_policy() -> CompliancePolicy {
        let mut tier_access = HashMap::new();
        tier_access.insert("free".to_string(), vec!["model-small".to_string()]);
        tier_access.insert("pro".to_string(), vec!["model-small".to_string(), "model-large".to_string()]);

        CompliancePolicy::new_usage(
            "usage-1",
            "Default Usage Policy",
            "Rate limits and tier access",
            UsagePolicy::new(
                100,           // max_requests_per_user_per_day
                50_000,        // max_tokens_per_user_per_day
                4096,          // max_tokens_per_request
                tier_access,
            ),
        )
    }

    fn make_retention_policy(ttl_seconds: u64) -> CompliancePolicy {
        CompliancePolicy::new_retention(
            "retention-1",
            "Default Retention Policy",
            "90-day log retention with GDPR erasure",
            RetentionPolicy::new(
                ttl_seconds,
                true,   // gdpr_erasure_enabled
                false,  // anonymize_on_expiry
                vec!["inference_log".to_string(), "audit_event".to_string()],
            ),
        )
    }

    fn make_access_control_policy() -> CompliancePolicy {
        let mut model_rules = HashMap::new();
        model_rules.insert(
            "model-large".to_string(),
            ModelAccessRule {
                min_tier: "pro".to_string(),
                min_kyc: "id_verified".to_string(),
                allowed_regions: vec![],
                requires_opt_in: false,
            },
        );
        model_rules.insert(
            "model-restricted".to_string(),
            ModelAccessRule {
                min_tier: "enterprise".to_string(),
                min_kyc: "full".to_string(),
                allowed_regions: vec!["US".to_string(), "GB".to_string()],
                requires_opt_in: true,
            },
        );

        CompliancePolicy::new_access_control(
            "ac-1",
            "Default Access Control",
            "Tier and region restrictions",
            AccessControlPolicy::new(
                model_rules,
                ModelAccessRule::default(),
                vec!["KP".to_string()], // blocked region
            ),
        )
    }

    // -- Policy management tests --

    #[test]
    fn test_create_policy() {
        let engine = make_engine();
        let policy = make_content_policy();
        let result = engine.create_policy(policy.clone());
        assert!(result.is_ok());
        let retrieved = engine.get_policy("content-1").unwrap();
        assert_eq!(retrieved.name, "Default Content Policy");
        assert_eq!(retrieved.policy_type, PolicyType::Content);
    }

    #[test]
    fn test_create_duplicate_policy_fails() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let result = engine.create_policy(make_content_policy());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_update_policy() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let mut policy = engine.get_policy("content-1").unwrap();
        policy.name = "Updated Content Policy".to_string();
        let result = engine.update_policy(policy).unwrap();
        assert_eq!(result.name, "Updated Content Policy");
        assert_eq!(engine.get_policy("content-1").unwrap().name, "Updated Content Policy");
    }

    #[test]
    fn test_update_nonexistent_policy_fails() {
        let engine = make_engine();
        let policy = make_content_policy();
        let result = engine.update_policy(policy);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_delete_policy() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let deleted = engine.delete_policy("content-1").unwrap();
        assert_eq!(deleted.id, "content-1");
        assert!(engine.get_policy("content-1").is_none());
    }

    #[test]
    fn test_delete_nonexistent_policy_fails() {
        let engine = make_engine();
        let result = engine.delete_policy("no-such-policy");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_policies_filtered() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let _ = engine.create_policy(make_usage_policy());
        let _ = engine.create_policy(make_retention_policy(90 * 24 * 3600));

        let content = engine.list_policies(Some(&PolicyType::Content), false);
        assert_eq!(content.len(), 1);

        let all = engine.list_policies(None, false);
        assert_eq!(all.len(), 3);

        let active = engine.list_policies(None, true);
        assert_eq!(active.len(), 3);

        // Deactivate one and re-check
        engine.set_policy_active("content-1", false).unwrap();
        let active2 = engine.list_policies(None, true);
        assert_eq!(active2.len(), 2);
    }

    #[test]
    fn test_set_policy_active() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        engine.set_policy_active("content-1", false).unwrap();
        assert!(!engine.get_policy("content-1").unwrap().is_active);
        engine.set_policy_active("content-1", true).unwrap();
        assert!(engine.get_policy("content-1").unwrap().is_active);
    }

    // -- Content policy tests --

    #[test]
    fn test_content_check_pass() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let result = engine.check_content(&[], 0.3, false);
        assert!(result.passed);
        assert!(result.blocked_categories.is_empty());
        assert!(!result.toxicity_violation);
        assert!(!result.pii_violation);
    }

    #[test]
    fn test_content_check_blocked_category() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let result = engine.check_content(
            &[ContentCategory::HateSpeech],
            0.1,
            false,
        );
        assert!(!result.passed);
        assert!(result.blocked_categories.contains(&ContentCategory::HateSpeech));
    }

    #[test]
    fn test_content_check_toxicity_violation() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let result = engine.check_content(&[], 0.9, false);
        assert!(!result.passed);
        assert!(result.toxicity_violation);
    }

    #[test]
    fn test_content_check_pii_violation() {
        let engine = make_engine();
        // Create a content policy that blocks on PII
        let policy = CompliancePolicy::new_content(
            "content-pii",
            "PII Block Policy",
            "Blocks PII",
            ContentPolicy::new(vec![], true, true, 0.75),
        );
        let _ = engine.create_policy(policy);
        let result = engine.check_content(&[], 0.1, true);
        assert!(!result.passed);
        assert!(result.pii_violation);
    }

    #[test]
    fn test_content_check_no_policies() {
        let engine = make_engine();
        let result = engine.check_content(
            &[ContentCategory::HateSpeech],
            1.0,
            true,
        );
        assert!(result.passed);
    }

    // -- Usage policy tests --

    #[test]
    fn test_usage_check_pass() {
        let engine = make_engine();
        let _ = engine.create_policy(make_usage_policy());
        let result = engine.check_usage("user-1", 100);
        assert!(result.passed);
        assert!(result.requests_allowed);
        assert!(result.tokens_allowed);
    }

    #[test]
    fn test_usage_check_request_limit() {
        let engine = make_engine();
        let _ = engine.create_policy(make_usage_policy());
        // Exhaust the request limit
        for _ in 0..100 {
            let r = engine.check_usage("user-1", 10);
            assert!(r.passed);
        }
        // 101st should fail
        let result = engine.check_usage("user-1", 10);
        assert!(!result.passed);
        assert!(!result.requests_allowed);
    }

    #[test]
    fn test_usage_check_token_limit() {
        let engine = make_engine();
        let _ = engine.create_policy(make_usage_policy());
        // Request just under the limit
        let result = engine.check_usage("user-1", 49_999);
        assert!(result.passed);
        // Next request should exceed
        let result2 = engine.check_usage("user-1", 100);
        assert!(!result2.passed);
        assert!(!result2.tokens_allowed);
    }

    #[test]
    fn test_usage_check_per_request_token_limit() {
        let engine = make_engine();
        let _ = engine.create_policy(make_usage_policy());
        let result = engine.check_usage("user-1", 5000);
        assert!(!result.passed);
        assert!(!result.tokens_allowed);
        assert!(result.messages.iter().any(|m| m.contains("per-request")));
    }

    #[test]
    fn test_model_access_by_tier_allowed() {
        let engine = make_engine();
        let _ = engine.create_policy(make_usage_policy());
        let result = engine.check_model_access_by_tier("user-1", "free", "model-small");
        assert!(result.allowed);
    }

    #[test]
    fn test_model_access_by_tier_denied() {
        let engine = make_engine();
        let _ = engine.create_policy(make_usage_policy());
        let result = engine.check_model_access_by_tier("user-1", "free", "model-large");
        assert!(!result.allowed);
        assert!(result.reason.is_some());
    }

    // -- Data retention tests --

    #[test]
    fn test_record_and_get_inference_log() {
        let engine = make_engine();
        engine.record_inference_log("user-1", "model-small", 100);
        let logs = engine.get_user_logs("user-1");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].tokens, 100);
    }

    #[test]
    fn test_expire_logs() {
        let engine = make_engine();
        // Create a retention policy with 0 TTL so everything expires immediately
        let _ = engine.create_policy(make_retention_policy(0));
        engine.record_inference_log("user-1", "model-small", 100);
        let expired = engine.expire_logs();
        assert_eq!(expired, 1);
        // The log should still be there but marked for deletion
        let logs = engine.get_user_logs("user-1");
        assert_eq!(logs.len(), 0); // marked logs are filtered out
    }

    #[test]
    fn test_purge_expired_logs() {
        let engine = make_engine();
        let _ = engine.create_policy(make_retention_policy(0));
        engine.record_inference_log("user-1", "model-small", 100);
        engine.expire_logs();
        let purged = engine.purge_expired_logs();
        assert_eq!(purged, 1);
        // Log should be gone
        let logs = engine.get_user_logs("user-1");
        assert_eq!(logs.len(), 0);
    }

    #[test]
    fn test_gdpr_erase_user() {
        let engine = make_engine();
        let _ = engine.create_policy(make_retention_policy(90 * 24 * 3600));
        engine.record_inference_log("user-1", "model-small", 100);
        engine.record_inference_log("user-1", "model-small", 200);
        engine.record_inference_log("user-2", "model-small", 300);

        let erased = engine.gdpr_erase_user("user-1").unwrap();
        assert_eq!(erased, 2);
        assert!(engine.get_user_logs("user-1").is_empty());
        // user-2 should be unaffected
        assert_eq!(engine.get_user_logs("user-2").len(), 1);
    }

    #[test]
    fn test_gdpr_erase_disabled() {
        let engine = make_engine();
        // No retention policy with GDPR enabled
        engine.record_inference_log("user-1", "model-small", 100);
        let result = engine.gdpr_erase_user("user-1");
        assert!(result.is_err());
    }

    // -- Access control tests --

    #[test]
    fn test_access_control_allowed() {
        let engine = make_engine();
        let _ = engine.create_policy(make_access_control_policy());
        let result = engine.check_model_access("user-1", "free", "none", "US", "model-small");
        assert_eq!(result, AccessCheckResult::Allowed);
    }

    #[test]
    fn test_access_control_denied_by_tier() {
        let engine = make_engine();
        let _ = engine.create_policy(make_access_control_policy());
        let result = engine.check_model_access("user-1", "free", "none", "US", "model-large");
        assert!(matches!(result, AccessCheckResult::Denied { .. }));
    }

    #[test]
    fn test_access_control_denied_by_kyc() {
        let engine = make_engine();
        let _ = engine.create_policy(make_access_control_policy());
        // model-large requires id_verified KYC
        let result = engine.check_model_access("user-1", "pro", "none", "US", "model-large");
        assert!(matches!(result, AccessCheckResult::Denied { .. }));
    }

    #[test]
    fn test_access_control_denied_by_region() {
        let engine = make_engine();
        let _ = engine.create_policy(make_access_control_policy());
        let result = engine.check_model_access("user-1", "enterprise", "full", "KP", "model-small");
        assert!(matches!(result, AccessCheckResult::Denied { .. }));
    }

    #[test]
    fn test_access_control_opt_in_required() {
        let engine = make_engine();
        let _ = engine.create_policy(make_access_control_policy());
        // model-restricted requires opt-in
        let result = engine.check_model_access("user-1", "enterprise", "full", "US", "model-restricted");
        assert!(matches!(result, AccessCheckResult::Denied { .. }));

        // Opt in and try again
        engine.opt_in_model("user-1", "model-restricted");
        let result2 = engine.check_model_access("user-1", "enterprise", "full", "US", "model-restricted");
        assert_eq!(result2, AccessCheckResult::Allowed);

        // Opt out
        engine.opt_out_model("user-1", "model-restricted");
        let result3 = engine.check_model_access("user-1", "enterprise", "full", "US", "model-restricted");
        assert!(matches!(result3, AccessCheckResult::Denied { .. }));
    }

    #[test]
    fn test_access_control_no_policies() {
        let engine = make_engine();
        let result = engine.check_model_access("user-1", "free", "none", "XX", "any-model");
        assert_eq!(result, AccessCheckResult::Allowed);
    }

    // -- Audit event tests --

    #[test]
    fn test_record_and_get_audit_events() {
        let engine = make_engine();
        engine.record_audit(AuditEvent::new(
            "test_check",
            "policy-1",
            CheckOutcome::Pass,
            Severity::Info,
            "Test audit message",
        ));
        let events = engine.get_audit_events(Some("policy-1"), None, None);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].check_type, "test_check");
    }

    #[test]
    fn test_audit_events_filtered_by_outcome() {
        let engine = make_engine();
        engine.record_audit(AuditEvent::new("check", "p1", CheckOutcome::Pass, Severity::Info, "ok"));
        engine.record_audit(AuditEvent::new("check", "p1", CheckOutcome::Fail, Severity::High, "bad"));
        engine.record_audit(AuditEvent::new("check", "p1", CheckOutcome::Pass, Severity::Info, "ok2"));

        let passed = engine.get_audit_events(None, None, Some(&CheckOutcome::Pass));
        assert_eq!(passed.len(), 2);

        let failed = engine.get_audit_events(None, None, Some(&CheckOutcome::Fail));
        assert_eq!(failed.len(), 1);
    }

    #[test]
    fn test_audit_events_filtered_by_user() {
        let engine = make_engine();
        engine.record_audit(
            AuditEvent::new("check", "p1", CheckOutcome::Pass, Severity::Info, "ok").with_user("alice"),
        );
        engine.record_audit(
            AuditEvent::new("check", "p1", CheckOutcome::Pass, Severity::Info, "ok").with_user("bob"),
        );

        let alice = engine.get_audit_events(None, Some("alice"), None);
        assert_eq!(alice.len(), 1);
        assert_eq!(alice[0].user_id.as_deref(), Some("alice"));
    }

    #[test]
    fn test_audit_events_in_range() {
        let engine = make_engine();
        let now = Utc::now();
        engine.record_audit(AuditEvent::new("check", "p1", CheckOutcome::Pass, Severity::Info, "ok"));

        let all = engine.get_audit_events_in_range(now - Duration::seconds(60), now + Duration::seconds(60));
        assert_eq!(all.len(), 1);

        let none = engine.get_audit_events_in_range(now - Duration::days(30), now - Duration::days(1));
        assert_eq!(none.len(), 0);
    }

    // -- Risk scoring tests --

    #[test]
    fn test_user_risk_score_clean_user() {
        let engine = make_engine();
        let score = engine.compute_user_risk("user-1");
        assert_eq!(score.score, 0.0);
        assert_eq!(score.risk_level(), "low");
    }

    #[test]
    fn test_user_risk_score_with_violations() {
        let engine = make_engine();
        // Simulate violations by recording failing audit events
        for _ in 0..5 {
            engine.record_audit(
                AuditEvent::new("check", "p1", CheckOutcome::Fail, Severity::High, "violation")
                    .with_user("user-1"),
            );
        }
        let score = engine.compute_user_risk("user-1");
        assert!(score.score > 0.0);
        assert_eq!(score.score_type, RiskScoreType::User);
    }

    #[test]
    fn test_request_risk_score_clean() {
        let engine = make_engine();
        let score = engine.compute_request_risk("req-1", "user-1", "model-small", 0.1, false, 100);
        assert_eq!(score.score, 0.0);
    }

    #[test]
    fn test_request_risk_score_with_toxicity() {
        let engine = make_engine();
        let score = engine.compute_request_risk("req-1", "user-1", "model-small", 0.9, false, 100);
        assert!(score.score > 0.5);
        assert!(score.factors.iter().any(|f| f.name == "toxicity"));
    }

    #[test]
    fn test_request_risk_score_with_pii() {
        let engine = make_engine();
        let score = engine.compute_request_risk("req-1", "user-1", "model-small", 0.1, true, 100);
        assert!(score.score > 0.0);
        assert!(score.factors.iter().any(|f| f.name == "pii_detected"));
    }

    #[test]
    fn test_request_risk_score_with_user_history() {
        let engine = make_engine();
        // Give the user a high risk score
        let mut user_score = RiskScore::new("user-1", RiskScoreType::User);
        user_score = user_score.with_factor("violations", 0.9, "many violations");
        engine.user_risk_scores.insert("user-1".to_string(), user_score);

        let req_score = engine.compute_request_risk("req-1", "user-1", "model-small", 0.1, false, 100);
        assert!(req_score.factors.iter().any(|f| f.name == "user_risk_history"));
    }

    #[test]
    fn test_get_high_risk_users() {
        let engine = make_engine();
        let mut high = RiskScore::new("bad-user", RiskScoreType::User);
        high = high.with_factor("violations", 0.8, "many violations");
        engine.user_risk_scores.insert("bad-user".to_string(), high);

        let mut low = RiskScore::new("good-user", RiskScoreType::User);
        low = low.with_factor("minor", 0.1, "minor issue");
        engine.user_risk_scores.insert("good-user".to_string(), low);

        let high_risk = engine.get_high_risk_users();
        assert_eq!(high_risk.len(), 1);
        assert_eq!(high_risk[0].entity_id, "bad-user");
    }

    // -- Compliance reporting tests --

    #[test]
    fn test_generate_report() {
        let engine = make_engine();
        let _ = engine.create_policy(make_content_policy());
        let _ = engine.create_policy(make_usage_policy());

        // Run some checks
        engine.check_content(&[], 0.1, false);
        engine.check_content(&[ContentCategory::Violence], 0.1, false);
        engine.check_usage("user-1", 100);

        let report = engine.generate_report();
        assert_eq!(report.active_policies, 2);
        assert_eq!(report.total_checks, 3);
        assert!(report.checks_passed >= 1);
        assert!(report.checks_failed >= 1);
        assert!(report.pass_rate > 0.0 && report.pass_rate <= 100.0);
        assert!(report.violations_by_type.contains_key("content"));
    }

    #[test]
    fn test_generate_report_empty() {
        let engine = make_engine();
        let report = engine.generate_report();
        assert_eq!(report.total_checks, 0);
        assert_eq!(report.active_policies, 0);
        assert_eq!(report.pass_rate, 100.0);
    }

    // -- Severity and helper tests --

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical.score() > Severity::High.score());
        assert!(Severity::High.score() > Severity::Medium.score());
        assert!(Severity::Medium.score() > Severity::Low.score());
        assert!(Severity::Low.score() > Severity::Info.score());
    }

    #[test]
    fn test_tier_gte() {
        assert!(tier_gte("enterprise", "free"));
        assert!(tier_gte("pro", "basic"));
        assert!(tier_gte("basic", "free"));
        assert!(!tier_gte("free", "pro"));
        assert!(!tier_gte("basic", "enterprise"));
    }

    #[test]
    fn test_kyc_gte() {
        assert!(kyc_gte("full", "none"));
        assert!(kyc_gte("id_verified", "email"));
        assert!(kyc_gte("email", "none"));
        assert!(!kyc_gte("none", "email"));
        assert!(!kyc_gte("email", "full"));
    }

    #[test]
    fn test_risk_level_classification() {
        let mut s = RiskScore::new("e", RiskScoreType::User);
        s.score = 0.1;
        assert_eq!(s.risk_level(), "low");

        s.score = 0.3;
        assert_eq!(s.risk_level(), "medium");

        s.score = 0.6;
        assert_eq!(s.risk_level(), "high");

        s.score = 0.9;
        assert_eq!(s.risk_level(), "critical");
    }

    #[test]
    fn test_access_check_result_denied_reason() {
        let allowed = AccessCheckResult::Allowed;
        assert!(allowed.denied_reason().is_none());

        let denied = AccessCheckResult::Denied {
            reason: "blocked".to_string(),
        };
        assert_eq!(denied.denied_reason(), Some("blocked"));
    }

    #[test]
    fn test_content_policy_category_check() {
        let cp = ContentPolicy::default();
        assert!(cp.is_category_blocked(&ContentCategory::HateSpeech));
        assert!(!cp.is_category_blocked(&ContentCategory::Malware));
    }

    #[test]
    fn test_retention_policy_expiry() {
        let rp = RetentionPolicy::new(3600, true, false, vec![]);
        let now = Utc::now();
        assert!(rp.is_expired(now - Duration::seconds(7200), now));
        assert!(!rp.is_expired(now - Duration::seconds(1800), now));
    }

    #[test]
    fn test_retention_policy_zero_ttl() {
        let rp = RetentionPolicy::new(0, true, false, vec![]);
        let now = Utc::now();
        assert!(!rp.is_expired(now - Duration::days(365), now));
    }

    #[test]
    fn test_audit_event_builder() {
        let event = AuditEvent::new("test", "p1", CheckOutcome::Pass, Severity::Info, "msg")
            .with_user("alice")
            .with_model("m1")
            .with_metadata("key1", serde_json::json!(42));
        assert_eq!(event.user_id.as_deref(), Some("alice"));
        assert_eq!(event.model_id.as_deref(), Some("m1"));
        assert_eq!(event.metadata.get("key1").unwrap(), &serde_json::json!(42));
    }

    #[test]
    fn test_compliance_policy_constructors() {
        let content = CompliancePolicy::new_content("c1", "C", "desc", ContentPolicy::default());
        assert_eq!(content.policy_type, PolicyType::Content);
        assert!(content.content.is_some());
        assert!(content.usage.is_none());

        let usage = CompliancePolicy::new_usage("u1", "U", "desc", UsagePolicy::default());
        assert_eq!(usage.policy_type, PolicyType::Usage);

        let retention = CompliancePolicy::new_retention("r1", "R", "desc", RetentionPolicy::default());
        assert_eq!(retention.policy_type, PolicyType::Retention);

        let ac = CompliancePolicy::new_access_control("a1", "A", "desc", AccessControlPolicy::default());
        assert_eq!(ac.policy_type, PolicyType::AccessControl);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let policy = make_content_policy();
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: CompliancePolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, policy.id);
        assert_eq!(deserialized.policy_type, policy.policy_type);
    }

    #[test]
    fn test_report_serialization() {
        let engine = make_engine();
        let report = engine.generate_report();
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("total_checks"));
        let _deserialized: ComplianceReport = serde_json::from_str(&json).unwrap();
    }
}
