//! Provider Monitor for the Xergon Network marketplace.
//!
//! Tracks uptime, SLA compliance, reputation correlation, and alert management
//! for model providers. Provides real-time monitoring dashboards and automated
//! alerting when providers deviate from agreed service levels.
//!
//! Uses a dark theme consistent with other marketplace pages.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Provider status for uptime checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ProviderStatus {
    Up,
    Down,
    Degraded,
}

impl std::fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderStatus::Up => write!(f, "Up"),
            ProviderStatus::Down => write!(f, "Down"),
            ProviderStatus::Degraded => write!(f, "Degraded"),
        }
    }
}

/// A single uptime check record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeRecord {
    pub provider_id: String,
    pub timestamp: DateTime<Utc>,
    pub status: ProviderStatus,
    pub response_time_ms: u64,
    pub error_rate: f64,
}

/// SLA violation type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ViolationType {
    Uptime,
    ResponseTime,
    ErrorRate,
}

impl std::fmt::Display for ViolationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViolationType::Uptime => write!(f, "Uptime"),
            ViolationType::ResponseTime => write!(f, "ResponseTime"),
            ViolationType::ErrorRate => write!(f, "ErrorRate"),
        }
    }
}

/// SLA definition for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SLADefinition {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    pub target_uptime: f64,
    pub max_response_time_ms: u64,
    pub max_error_rate: f64,
    pub penalty_rate: f64,
    pub created_at: DateTime<Utc>,
}

/// A recorded SLA violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SLAViolation {
    pub id: String,
    pub sla_id: String,
    pub provider_id: String,
    pub violation_type: ViolationType,
    pub actual_value: f64,
    pub threshold: f64,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
}

/// Reputation correlation data for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReputationCorrelation {
    pub provider_id: String,
    pub reputation_score: f64,
    pub uptime_pct: f64,
    pub avg_response_time_ms: u64,
    pub error_rate: f64,
    pub violation_count: u64,
    pub correlation_factor: f64,
}

/// Alert condition type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AlertConditionType {
    UptimeBelow,
    ResponseTimeAbove,
    ErrorRateAbove,
    ReputationDrop,
    ProviderDown,
}

impl std::fmt::Display for AlertConditionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertConditionType::UptimeBelow => write!(f, "UptimeBelow"),
            AlertConditionType::ResponseTimeAbove => write!(f, "ResponseTimeAbove"),
            AlertConditionType::ErrorRateAbove => write!(f, "ErrorRateAbove"),
            AlertConditionType::ReputationDrop => write!(f, "ReputationDrop"),
            AlertConditionType::ProviderDown => write!(f, "ProviderDown"),
        }
    }
}

/// Alert severity level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "Info"),
            AlertSeverity::Warning => write!(f, "Warning"),
            AlertSeverity::Critical => write!(f, "Critical"),
        }
    }
}

/// An alert rule definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub condition_type: AlertConditionType,
    pub threshold: f64,
    pub provider_filter: Option<String>,
    pub enabled: bool,
    pub cooldown_minutes: u64,
    pub created_at: DateTime<Utc>,
}

/// A triggered alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub provider_id: String,
    pub message: String,
    pub severity: AlertSeverity,
    pub triggered_at: DateTime<Utc>,
    pub acknowledged: bool,
}

/// Activity feed item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityItem {
    pub id: String,
    pub activity_type: String,
    pub description: String,
    pub timestamp: u64,
    pub provider_id: Option<String>,
}

/// Monitoring statistics overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorStats {
    pub providers_tracked: u64,
    pub total_uptime_checks: u64,
    pub total_sla_definitions: u64,
    pub total_violations: u64,
    pub active_alerts: u64,
    pub acknowledged_alerts: u64,
    pub alert_rules: u64,
}

/// Dashboard data aggregation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub stats: MonitorStats,
    pub top_violations: Vec<SLAViolation>,
    pub active_alerts: Vec<Alert>,
    pub recent_activity: Vec<ActivityItem>,
}

/// Uptime statistics for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeStats {
    pub provider_id: String,
    pub uptime_pct: f64,
    pub avg_response_time_ms: u64,
    pub avg_error_rate: f64,
    pub total_checks: u64,
    pub up_count: u64,
    pub down_count: u64,
    pub degraded_count: u64,
}

// ---------------------------------------------------------------------------
// Request / Query types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RecordUptimeRequest {
    pub provider_id: String,
    pub status: ProviderStatus,
    pub response_time_ms: u64,
    pub error_rate: f64,
}

#[derive(Debug, Deserialize)]
pub struct CreateSLARequest {
    pub provider_id: String,
    pub name: String,
    pub target_uptime: f64,
    pub max_response_time_ms: u64,
    pub max_error_rate: f64,
    pub penalty_rate: f64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSLARequest {
    pub name: Option<String>,
    pub target_uptime: Option<f64>,
    pub max_response_time_ms: Option<u64>,
    pub max_error_rate: Option<f64>,
    pub penalty_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAlertRuleRequest {
    pub name: String,
    pub condition_type: AlertConditionType,
    pub threshold: f64,
    pub provider_filter: Option<String>,
    pub cooldown_minutes: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAlertRuleRequest {
    pub name: Option<String>,
    pub threshold: Option<f64>,
    pub provider_filter: Option<Option<String>>,
    pub cooldown_minutes: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UptimeQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ViolationQuery {
    pub provider_id: Option<String>,
    pub acknowledged: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AlertQuery {
    pub provider_id: Option<String>,
    pub acknowledged: Option<bool>,
    pub severity: Option<String>,
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// Provider Monitor
// ---------------------------------------------------------------------------

const MAX_UPTIME_RECORDS: usize = 1000;
const MAX_ACTIVITY: usize = 200;

/// Main provider monitoring struct.
pub struct ProviderMonitor {
    uptime_records: DashMap<String, VecDeque<UptimeRecord>>,
    sla_definitions: DashMap<String, SLADefinition>,
    sla_violations: DashMap<String, SLAViolation>,
    reputation_correlations: DashMap<String, ProviderReputationCorrelation>,
    alert_rules: DashMap<String, AlertRule>,
    alerts: DashMap<String, Alert>,
    activity: Mutex<VecDeque<ActivityItem>>,
    activity_counter: AtomicU64,
    total_checks: AtomicU64,
    alert_counter: AtomicU64,
}

pub type MonitorState = Arc<ProviderMonitor>;

impl ProviderMonitor {
    /// Create a new provider monitor.
    pub fn new() -> Self {
        Self {
            uptime_records: DashMap::new(),
            sla_definitions: DashMap::new(),
            sla_violations: DashMap::new(),
            reputation_correlations: DashMap::new(),
            alert_rules: DashMap::new(),
            alerts: DashMap::new(),
            activity: Mutex::new(VecDeque::with_capacity(MAX_ACTIVITY)),
            activity_counter: AtomicU64::new(0),
            total_checks: AtomicU64::new(0),
            alert_counter: AtomicU64::new(0),
        }
    }

    // -----------------------------------------------------------------------
    // Activity feed
    // -----------------------------------------------------------------------

    fn record_activity(&self, activity_type: &str, description: &str, provider_id: Option<&str>) {
        let item = ActivityItem {
            id: format!(
                "mon_act_{}",
                self.activity_counter.fetch_add(1, Ordering::Relaxed)
            ),
            activity_type: activity_type.to_string(),
            description: description.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            provider_id: provider_id.map(|s| s.to_string()),
        };
        let mut feed = self.activity.lock().unwrap_or_else(|e| e.into_inner());
        if feed.len() >= MAX_ACTIVITY {
            feed.pop_front();
        }
        feed.push_back(item);
    }

    fn get_activity_feed(&self, limit: usize) -> Vec<ActivityItem> {
        let feed = self.activity.lock().unwrap_or_else(|e| e.into_inner());
        feed.iter().rev().take(limit).cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Uptime recording
    // -----------------------------------------------------------------------

    /// Record an uptime check for a provider.
    pub fn record_uptime(&self, record: UptimeRecord) {
        self.total_checks.fetch_add(1, Ordering::Relaxed);
        let provider_id = record.provider_id.clone();
        let status_str = record.status.to_string();

        let mut records = self
            .uptime_records
            .entry(provider_id.clone())
            .or_insert_with(|| VecDeque::with_capacity(MAX_UPTIME_RECORDS));
        if records.len() >= MAX_UPTIME_RECORDS {
            records.pop_front();
        }
        records.push_back(record);

        self.record_activity(
            "uptime_check",
            &format!("Provider {} status: {}", &provider_id, &status_str),
            Some(&provider_id),
        );

        debug!(
            provider_id = %provider_id,
            status = %status_str,
            "Uptime check recorded"
        );
    }

    /// Get uptime history for a provider.
    pub fn get_uptime_history(&self, provider_id: &str, limit: usize) -> Vec<UptimeRecord> {
        match self.uptime_records.get(provider_id) {
            Some(records) => records.iter().rev().take(limit).cloned().collect(),
            None => Vec::new(),
        }
    }

    /// Get aggregated uptime statistics for a provider.
    pub fn get_uptime_stats(&self, provider_id: &str) -> Option<UptimeStats> {
        let records = self.uptime_records.get(provider_id)?;
        let recs: Vec<&UptimeRecord> = records.iter().collect();
        if recs.is_empty() {
            return None;
        }

        let total = recs.len() as u64;
        let up_count = recs.iter().filter(|r| r.status == ProviderStatus::Up).count() as u64;
        let down_count = recs.iter().filter(|r| r.status == ProviderStatus::Down).count() as u64;
        let degraded_count = recs.iter().filter(|r| r.status == ProviderStatus::Degraded).count() as u64;

        let uptime_pct = if total > 0 {
            (up_count as f64 + degraded_count as f64 * 0.5) / total as f64
        } else {
            0.0
        };

        let total_response: u64 = recs.iter().map(|r| r.response_time_ms).sum();
        let avg_response_time = if total > 0 { total_response / total } else { 0 };

        let total_error: f64 = recs.iter().map(|r| r.error_rate).sum();
        let avg_error_rate = if total > 0 { total_error / total as f64 } else { 0.0 };

        Some(UptimeStats {
            provider_id: provider_id.to_string(),
            uptime_pct,
            avg_response_time_ms: avg_response_time,
            avg_error_rate: avg_error_rate,
            total_checks: total,
            up_count,
            down_count,
            degraded_count,
        })
    }

    /// Get all tracked provider IDs.
    pub fn get_tracked_providers(&self) -> Vec<String> {
        self.uptime_records.iter().map(|r| r.key().clone()).collect()
    }

    // -----------------------------------------------------------------------
    // SLA CRUD
    // -----------------------------------------------------------------------

    /// Create a new SLA definition.
    pub fn create_sla(&self, sla: SLADefinition) {
        let provider_id = sla.provider_id.clone();
        self.sla_definitions.insert(sla.id.clone(), sla);
        self.record_activity(
            "sla_created",
            &format!("SLA created for provider {}", &provider_id),
            Some(&provider_id),
        );
        info!(provider_id = %provider_id, "SLA definition created");
    }

    /// List all SLA definitions.
    pub fn list_slas(&self) -> Vec<SLADefinition> {
        self.sla_definitions.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a single SLA definition by ID.
    pub fn get_sla(&self, id: &str) -> Option<SLADefinition> {
        self.sla_definitions.get(id).map(|r| r.value().clone())
    }

    /// Update an existing SLA definition.
    pub fn update_sla(&self, id: &str, update: &UpdateSLARequest) -> bool {
        let mut sla = match self.sla_definitions.get_mut(id) {
            Some(s) => s,
            None => return false,
        };
        if let Some(ref name) = update.name {
            sla.name = name.clone();
        }
        if let Some(target) = update.target_uptime {
            sla.target_uptime = target;
        }
        if let Some(max_rt) = update.max_response_time_ms {
            sla.max_response_time_ms = max_rt;
        }
        if let Some(max_er) = update.max_error_rate {
            sla.max_error_rate = max_er;
        }
        if let Some(penalty) = update.penalty_rate {
            sla.penalty_rate = penalty;
        }
        self.record_activity(
            "sla_updated",
            &format!("SLA {} updated", id),
            Some(&sla.provider_id.clone()),
        );
        true
    }

    /// Delete an SLA definition.
    pub fn delete_sla(&self, id: &str) -> bool {
        let removed = self.sla_definitions.remove(id);
        if let Some((_, sla)) = removed {
            self.record_activity(
                "sla_deleted",
                &format!("SLA {} deleted", id),
                Some(&sla.provider_id),
            );
            true
        } else {
            false
        }
    }

    // -----------------------------------------------------------------------
    // SLA compliance
    // -----------------------------------------------------------------------

    /// Check SLA compliance for a provider against a specific SLA.
    pub fn check_sla_compliance(&self, sla_id: &str) -> Vec<SLAViolation> {
        let sla = match self.sla_definitions.get(sla_id) {
            Some(s) => s.value().clone(),
            None => return Vec::new(),
        };

        let stats = match self.get_uptime_stats(&sla.provider_id) {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut violations = Vec::new();

        // Check uptime
        if stats.uptime_pct < sla.target_uptime {
            violations.push(SLAViolation {
                id: uuid::Uuid::new_v4().to_string(),
                sla_id: sla.id.clone(),
                provider_id: sla.provider_id.clone(),
                violation_type: ViolationType::Uptime,
                actual_value: stats.uptime_pct,
                threshold: sla.target_uptime,
                timestamp: Utc::now(),
                acknowledged: false,
            });
        }

        // Check response time
        if stats.avg_response_time_ms > sla.max_response_time_ms {
            violations.push(SLAViolation {
                id: uuid::Uuid::new_v4().to_string(),
                sla_id: sla.id.clone(),
                provider_id: sla.provider_id.clone(),
                violation_type: ViolationType::ResponseTime,
                actual_value: stats.avg_response_time_ms as f64,
                threshold: sla.max_response_time_ms as f64,
                timestamp: Utc::now(),
                acknowledged: false,
            });
        }

        // Check error rate
        if stats.avg_error_rate > sla.max_error_rate {
            violations.push(SLAViolation {
                id: uuid::Uuid::new_v4().to_string(),
                sla_id: sla.id.clone(),
                provider_id: sla.provider_id.clone(),
                violation_type: ViolationType::ErrorRate,
                actual_value: stats.avg_error_rate,
                threshold: sla.max_error_rate,
                timestamp: Utc::now(),
                acknowledged: false,
            });
        }

        // Store violations
        for v in &violations {
            self.sla_violations.insert(v.id.clone(), v.clone());
            warn!(
                sla_id = %v.sla_id,
                provider_id = %v.provider_id,
                violation_type = %v.violation_type,
                "SLA violation recorded"
            );
        }

        violations
    }

    /// Record a manual SLA violation.
    pub fn record_violation(&self, violation: SLAViolation) {
        self.sla_violations.insert(violation.id.clone(), violation);
    }

    /// List SLA violations with optional filters.
    pub fn list_violations(
        &self,
        provider_id: Option<&str>,
        acknowledged: Option<bool>,
        limit: usize,
    ) -> Vec<SLAViolation> {
        let mut results: Vec<SLAViolation> = self
            .sla_violations
            .iter()
            .filter(|r| {
                provider_id
                    .map(|pid| r.value().provider_id == pid)
                    .unwrap_or(true)
                    && acknowledged
                        .map(|ack| r.value().acknowledged == ack)
                        .unwrap_or(true)
            })
            .map(|r| r.value().clone())
            .collect();
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(limit);
        results
    }

    // -----------------------------------------------------------------------
    // Reputation correlation
    // -----------------------------------------------------------------------

    /// Compute reputation correlation for a provider.
    pub fn compute_reputation_correlation(
        &self,
        provider_id: &str,
        reputation_score: f64,
    ) -> ProviderReputationCorrelation {
        let stats = self.get_uptime_stats(provider_id);

        let uptime_pct = stats.as_ref().map(|s| s.uptime_pct).unwrap_or(0.0);
        let avg_response_time_ms = stats.as_ref().map(|s| s.avg_response_time_ms).unwrap_or(0);
        let error_rate = stats.as_ref().map(|s| s.avg_error_rate).unwrap_or(0.0);

        let violation_count = self
            .sla_violations
            .iter()
            .filter(|r| r.value().provider_id == provider_id)
            .count() as u64;

        // Correlation factor: positive correlation between uptime and reputation,
        // negative correlation between error rate and reputation.
        // Range -1.0 to 1.0
        let uptime_factor = uptime_pct * reputation_score;
        let error_factor = (1.0 - error_rate.min(1.0)) * reputation_score;
        let violation_penalty = (violation_count as f64 * 0.05).min(1.0);
        let correlation_factor = ((uptime_factor + error_factor) / 2.0 - violation_penalty)
            .max(-1.0)
            .min(1.0);

        let correlation = ProviderReputationCorrelation {
            provider_id: provider_id.to_string(),
            reputation_score,
            uptime_pct,
            avg_response_time_ms,
            error_rate,
            violation_count,
            correlation_factor,
        };

        self.reputation_correlations
            .insert(provider_id.to_string(), correlation.clone());

        info!(
            provider_id = %provider_id,
            correlation = correlation.correlation_factor,
            "Reputation correlation computed"
        );

        correlation
    }

    /// Get reputation correlation for a single provider.
    pub fn get_correlation(&self, provider_id: &str) -> Option<ProviderReputationCorrelation> {
        self.reputation_correlations
            .get(provider_id)
            .map(|r| r.value().clone())
    }

    /// List all reputation correlations.
    pub fn list_correlations(&self) -> Vec<ProviderReputationCorrelation> {
        self.reputation_correlations
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Alert rules
    // -----------------------------------------------------------------------

    /// Create a new alert rule.
    pub fn create_alert_rule(&self, rule: AlertRule) {
        let name = rule.name.clone();
        self.alert_rules.insert(rule.id.clone(), rule);
        self.record_activity(
            "alert_rule_created",
            &format!("Alert rule created: {}", &name),
            None,
        );
    }

    /// List all alert rules.
    pub fn list_alert_rules(&self) -> Vec<AlertRule> {
        self.alert_rules.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a single alert rule by ID.
    pub fn get_alert_rule(&self, id: &str) -> Option<AlertRule> {
        self.alert_rules.get(id).map(|r| r.value().clone())
    }

    /// Update an alert rule.
    pub fn update_alert_rule(&self, id: &str, update: &UpdateAlertRuleRequest) -> bool {
        let mut rule = match self.alert_rules.get_mut(id) {
            Some(r) => r,
            None => return false,
        };
        if let Some(ref name) = update.name {
            rule.name = name.clone();
        }
        if let Some(threshold) = update.threshold {
            rule.threshold = threshold;
        }
        if let Some(ref provider_filter) = update.provider_filter {
            rule.provider_filter = provider_filter.clone();
        }
        if let Some(cooldown) = update.cooldown_minutes {
            rule.cooldown_minutes = cooldown;
        }
        self.record_activity("alert_rule_updated", &format!("Alert rule {} updated", id), None);
        true
    }

    /// Delete an alert rule.
    pub fn delete_alert_rule(&self, id: &str) -> bool {
        self.alert_rules.remove(id).is_some()
    }

    /// Enable an alert rule.
    pub fn enable_alert_rule(&self, id: &str) -> bool {
        match self.alert_rules.get_mut(id) {
            Some(mut r) => {
                r.enabled = true;
                true
            }
            None => false,
        }
    }

    /// Disable an alert rule.
    pub fn disable_alert_rule(&self, id: &str) -> bool {
        match self.alert_rules.get_mut(id) {
            Some(mut r) => {
                r.enabled = false;
                true
            }
            None => false,
        }
    }

    // -----------------------------------------------------------------------
    // Alert evaluation
    // -----------------------------------------------------------------------

    /// Evaluate all enabled alert rules against all tracked providers.
    pub fn evaluate_alerts(&self) -> Vec<Alert> {
        let mut new_alerts = Vec::new();
        let providers = self.get_tracked_providers();

        for rule_entry in self.alert_rules.iter() {
            let rule = rule_entry.value();
            if !rule.enabled {
                continue;
            }

            let target_providers: Vec<String> = match &rule.provider_filter {
                Some(filter) => providers.iter().filter(|p| *p == filter).cloned().collect(),
                None => providers.clone(),
            };

            for provider_id in target_providers {
                let stats = match self.get_uptime_stats(&provider_id) {
                    Some(s) => s,
                    None => continue,
                };

                let should_alert = match rule.condition_type {
                    AlertConditionType::UptimeBelow => stats.uptime_pct < rule.threshold,
                    AlertConditionType::ResponseTimeAbove => {
                        stats.avg_response_time_ms as f64 > rule.threshold
                    }
                    AlertConditionType::ErrorRateAbove => stats.avg_error_rate > rule.threshold,
                    AlertConditionType::ReputationDrop => {
                        if let Some(correlation) = self.get_correlation(&provider_id) {
                            correlation.reputation_score < rule.threshold
                        } else {
                            false
                        }
                    }
                    AlertConditionType::ProviderDown => {
                        rule.threshold <= 0.0
                            && stats.down_count > 0
                            && stats.down_count as f64 / stats.total_checks as f64 > 0.5
                    }
                };

                if should_alert {
                    // Check cooldown: don't re-alert if recent alert exists for this rule+provider
                    let recent_exists = self.alerts.iter().any(|a| {
                        a.value().rule_id == rule.id
                            && a.value().provider_id == provider_id
                            && !a.value().acknowledged
                    });
                    if recent_exists {
                        continue;
                    }

                    let severity = match rule.condition_type {
                        AlertConditionType::ProviderDown => AlertSeverity::Critical,
                        AlertConditionType::UptimeBelow => {
                            if rule.threshold < 0.5 {
                                AlertSeverity::Critical
                            } else {
                                AlertSeverity::Warning
                            }
                        }
                        AlertConditionType::ResponseTimeAbove => AlertSeverity::Warning,
                        AlertConditionType::ErrorRateAbove => {
                            if rule.threshold > 0.1 {
                                AlertSeverity::Critical
                            } else {
                                AlertSeverity::Warning
                            }
                        }
                        AlertConditionType::ReputationDrop => AlertSeverity::Info,
                    };

                    let message = format!(
                        "Alert [{}]: Provider {} triggered {} condition (threshold: {})",
                        &rule.name,
                        &provider_id,
                        &rule.condition_type,
                        rule.threshold
                    );

                    let alert = Alert {
                        id: format!(
                            "alert_{}",
                            self.alert_counter.fetch_add(1, Ordering::Relaxed)
                        ),
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        provider_id: provider_id.clone(),
                        message,
                        severity,
                        triggered_at: Utc::now(),
                        acknowledged: false,
                    };

                    self.alerts.insert(alert.id.clone(), alert.clone());
                    self.record_activity(
                        "alert_triggered",
                        &format!(
                            "Alert triggered for {}: {}",
                            &provider_id, &alert.rule_name
                        ),
                        Some(&provider_id),
                    );
                    warn!(
                        provider_id = %provider_id,
                        rule_id = %alert.rule_id,
                        severity = %alert.severity,
                        "Alert triggered"
                    );

                    new_alerts.push(alert);
                }
            }
        }

        new_alerts
    }

    /// Acknowledge an alert.
    pub fn acknowledge_alert(&self, alert_id: &str) -> bool {
        match self.alerts.get_mut(alert_id) {
            Some(mut a) => {
                a.acknowledged = true;
                self.record_activity(
                    "alert_acknowledged",
                    &format!("Alert {} acknowledged", alert_id),
                    Some(&a.provider_id),
                );
                true
            }
            None => false,
        }
    }

    /// List alerts with optional filters.
    pub fn list_alerts(
        &self,
        provider_id: Option<&str>,
        acknowledged: Option<bool>,
        severity: Option<&str>,
        limit: usize,
    ) -> Vec<Alert> {
        let mut results: Vec<Alert> = self
            .alerts
            .iter()
            .filter(|r| {
                provider_id
                    .map(|pid| r.value().provider_id == pid)
                    .unwrap_or(true)
                    && acknowledged
                        .map(|ack| r.value().acknowledged == ack)
                        .unwrap_or(true)
                    && severity
                        .map(|s| r.value().severity.to_string() == s)
                        .unwrap_or(true)
            })
            .map(|r| r.value().clone())
            .collect();
        results.sort_by(|a, b| b.triggered_at.cmp(&a.triggered_at));
        results.truncate(limit);
        results
    }

    /// Get only active (unacknowledged) alerts.
    pub fn get_active_alerts(&self) -> Vec<Alert> {
        self.list_alerts(None, Some(false), None, 100)
    }

    // -----------------------------------------------------------------------
    // Stats / Dashboard
    // -----------------------------------------------------------------------

    /// Get monitoring statistics.
    pub fn get_stats(&self) -> MonitorStats {
        let active = self
            .alerts
            .iter()
            .filter(|r| !r.value().acknowledged)
            .count() as u64;
        let acknowledged = self
            .alerts
            .iter()
            .filter(|r| r.value().acknowledged)
            .count() as u64;

        MonitorStats {
            providers_tracked: self.uptime_records.len() as u64,
            total_uptime_checks: self.total_checks.load(Ordering::Relaxed),
            total_sla_definitions: self.sla_definitions.len() as u64,
            total_violations: self.sla_violations.len() as u64,
            active_alerts: active,
            acknowledged_alerts: acknowledged,
            alert_rules: self.alert_rules.len() as u64,
        }
    }

    /// Get aggregated dashboard data.
    pub fn get_dashboard_data(&self) -> DashboardData {
        let stats = self.get_stats();
        let top_violations = self.list_violations(None, Some(false), 10);
        let active_alerts = self.get_active_alerts();
        let recent_activity = self.get_activity_feed(20);

        DashboardData {
            stats,
            top_violations,
            active_alerts,
            recent_activity,
        }
    }
}

impl Default for ProviderMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HTML Page
// ---------------------------------------------------------------------------

/// Generate the provider monitor dashboard HTML page.
pub fn monitor_dashboard_page(data: &DashboardData) -> String {
    let stats = &data.stats;
    let mut violation_rows = String::new();
    for v in data.top_violations.iter().take(10) {
        let vtype_color = match v.violation_type {
            ViolationType::Uptime => "#ef4444",
            ViolationType::ResponseTime => "#f59e0b",
            ViolationType::ErrorRate => "#f97316",
        };
        let actual_str = format!("{:.4}", v.actual_value);
        let threshold_str = format!("{:.4}", v.threshold);
        violation_rows.push_str(&format!(r#"
        <tr>
          <td style="padding:8px 12px;border-bottom:1px solid #262626;font-size:12px;color:#e5e5e5;">{provider}</td>
          <td style="padding:8px 12px;border-bottom:1px solid #262626;font-size:12px;"><span style="color:{vcolor};font-weight:600;">{vtype}</span></td>
          <td style="padding:8px 12px;border-bottom:1px solid #262626;font-size:12px;color:#a3a3a3;">{actual} / {thresh}</td>
          <td style="padding:8px 12px;border-bottom:1px solid #262626;font-size:12px;color:#737373;">{time}</td>
        </tr>"#,
            provider = html_escape(&v.provider_id),
            vcolor = vtype_color,
            vtype = v.violation_type,
            actual = actual_str,
            thresh = threshold_str,
            time = v.timestamp.format("%Y-%m-%d %H:%M"),
        ));
    }

    if violation_rows.is_empty() {
        violation_rows = r#"<tr><td colspan="4" style="padding:20px;text-align:center;color:#737373;">No active violations</td></tr>"#.to_string();
    }

    let mut alert_rows = String::new();
    for a in data.active_alerts.iter().take(10) {
        let sev_color = match a.severity {
            AlertSeverity::Critical => "#ef4444",
            AlertSeverity::Warning => "#f59e0b",
            AlertSeverity::Info => "#3b82f6",
        };
        alert_rows.push_str(&format!(r#"
        <div style="background:#141414;border:1px solid #262626;border-left:3px solid {sev_color};border-radius:4px;padding:12px;margin-bottom:8px;">
          <div style="display:flex;justify-content:space-between;align-items:center;">
            <span style="font-size:13px;font-weight:600;color:#e5e5e5;">{provider}</span>
            <span style="background:{sev_color};color:#000;padding:1px 6px;border-radius:3px;font-size:10px;font-weight:700;text-transform:uppercase;">{severity}</span>
          </div>
          <div style="font-size:12px;color:#a3a3a3;margin-top:4px;">{msg}</div>
        </div>"#,
            sev_color = sev_color,
            severity = a.severity,
            provider = html_escape(&a.provider_id),
            msg = html_escape(&a.message),
        ));
    }

    if alert_rows.is_empty() {
        alert_rows = r#"<div style="text-align:center;color:#737373;padding:20px;">No active alerts</div>"#.to_string();
    }

    let providers_str = stats.providers_tracked.to_string();
    let checks_str = stats.total_uptime_checks.to_string();
    let slas_str = stats.total_sla_definitions.to_string();
    let violations_str = stats.total_violations.to_string();
    let active_alerts_str = stats.active_alerts.to_string();

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Xergon Provider Monitor</title>
</head>
<body style="background:#0a0a0a;color:#e5e5e5;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;margin:0;padding:20px;">
<div style="max-width:1200px;margin:0 auto;">
  <h1 style="font-size:24px;font-weight:700;margin-bottom:4px;">Xergon Provider Monitor</h1>
  <p style="color:#737373;font-size:13px;margin-bottom:24px;">Real-time provider uptime, SLA compliance, and alerting</p>

  <!-- Stats Bar -->
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-bottom:24px;">
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Providers</div>
      <div style="font-size:24px;font-weight:700;color:#10b981;">{providers}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Uptime Checks</div>
      <div style="font-size:24px;font-weight:700;color:#3b82f6;">{checks}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">SLAs</div>
      <div style="font-size:24px;font-weight:700;color:#8b5cf6;">{slas}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Violations</div>
      <div style="font-size:24px;font-weight:700;color:#f59e0b;">{violations}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Active Alerts</div>
      <div style="font-size:24px;font-weight:700;color:#ef4444;">{active_alerts}</div>
    </div>
  </div>

  <!-- Main Grid -->
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:20px;margin-bottom:24px;">
    <div>
      <h2 style="font-size:16px;font-weight:600;margin-bottom:12px;">SLA Violations</h2>
      <div style="background:#141414;border:1px solid #262626;border-radius:8px;overflow:hidden;">
        <table style="width:100%;border-collapse:collapse;">
          <thead>
            <tr style="background:#1a1a1a;">
              <th style="padding:8px 12px;text-align:left;font-size:11px;color:#737373;text-transform:uppercase;">Provider</th>
              <th style="padding:8px 12px;text-align:left;font-size:11px;color:#737373;text-transform:uppercase;">Type</th>
              <th style="padding:8px 12px;text-align:left;font-size:11px;color:#737373;text-transform:uppercase;">Actual / Threshold</th>
              <th style="padding:8px 12px;text-align:left;font-size:11px;color:#737373;text-transform:uppercase;">Time</th>
            </tr>
          </thead>
          <tbody>{violation_rows}</tbody>
        </table>
      </div>
    </div>
    <div>
      <h2 style="font-size:16px;font-weight:600;margin-bottom:12px;">Active Alerts</h2>
      <div style="min-height:200px;">{alert_rows}</div>
    </div>
  </div>
</div>
</body>
</html>"#,
        providers = providers_str,
        checks = checks_str,
        slas = slas_str,
        violations = violations_str,
        active_alerts = active_alerts_str,
        violation_rows = violation_rows,
        alert_rows = alert_rows,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// API Handlers
// ---------------------------------------------------------------------------

/// Handler: POST /api/monitor/uptime
pub async fn record_uptime_handler(
    State(monitor): State<MonitorState>,
    Json(req): Json<RecordUptimeRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let record = UptimeRecord {
        provider_id: req.provider_id,
        timestamp: Utc::now(),
        status: req.status,
        response_time_ms: req.response_time_ms,
        error_rate: req.error_rate,
    };
    monitor.record_uptime(record);
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "ok", "message": "Uptime recorded"})),
    )
}

/// Handler: GET /api/monitor/uptime/:provider_id
pub async fn get_uptime_history_handler(
    State(monitor): State<MonitorState>,
    Path(provider_id): Path<String>,
    Query(query): Query<UptimeQuery>,
) -> Json<Vec<UptimeRecord>> {
    let limit = query.limit.unwrap_or(100);
    Json(monitor.get_uptime_history(&provider_id, limit))
}

/// Handler: POST /api/monitor/slas
pub async fn create_sla_handler(
    State(monitor): State<MonitorState>,
    Json(req): Json<CreateSLARequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let sla = SLADefinition {
        id: uuid::Uuid::new_v4().to_string(),
        provider_id: req.provider_id,
        name: req.name,
        target_uptime: req.target_uptime,
        max_response_time_ms: req.max_response_time_ms,
        max_error_rate: req.max_error_rate,
        penalty_rate: req.penalty_rate,
        created_at: Utc::now(),
    };
    let sla_id = sla.id.clone();
    monitor.create_sla(sla);
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"status": "ok", "sla_id": sla_id})),
    )
}

/// Handler: GET /api/monitor/slas
pub async fn list_slas_handler(
    State(monitor): State<MonitorState>,
) -> Json<Vec<SLADefinition>> {
    Json(monitor.list_slas())
}

/// Handler: GET /api/monitor/slas/:id/compliance
pub async fn check_compliance_handler(
    State(monitor): State<MonitorState>,
    Path(id): Path<String>,
) -> Json<Vec<SLAViolation>> {
    let violations = monitor.check_sla_compliance(&id);
    Json(violations)
}

/// Handler: GET /api/monitor/violations
pub async fn list_violations_handler(
    State(monitor): State<MonitorState>,
    Query(query): Query<ViolationQuery>,
) -> Json<Vec<SLAViolation>> {
    let limit = query.limit.unwrap_or(50);
    Json(monitor.list_violations(
        query.provider_id.as_deref(),
        query.acknowledged,
        limit,
    ))
}

/// Handler: GET /api/monitor/reputation
pub async fn list_correlations_handler(
    State(monitor): State<MonitorState>,
) -> Json<Vec<ProviderReputationCorrelation>> {
    Json(monitor.list_correlations())
}

/// Handler: POST /api/monitor/alerts/rules
pub async fn create_alert_rule_handler(
    State(monitor): State<MonitorState>,
    Json(req): Json<CreateAlertRuleRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let rule = AlertRule {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        condition_type: req.condition_type,
        threshold: req.threshold,
        provider_filter: req.provider_filter,
        enabled: true,
        cooldown_minutes: req.cooldown_minutes,
        created_at: Utc::now(),
    };
    let rule_id = rule.id.clone();
    monitor.create_alert_rule(rule);
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"status": "ok", "rule_id": rule_id})),
    )
}

/// Handler: GET /api/monitor/alerts/rules
pub async fn list_alert_rules_handler(
    State(monitor): State<MonitorState>,
) -> Json<Vec<AlertRule>> {
    Json(monitor.list_alert_rules())
}

/// Handler: GET /api/monitor/alerts
pub async fn list_alerts_handler(
    State(monitor): State<MonitorState>,
    Query(query): Query<AlertQuery>,
) -> Json<Vec<Alert>> {
    let limit = query.limit.unwrap_or(50);
    Json(monitor.list_alerts(
        query.provider_id.as_deref(),
        query.acknowledged,
        query.severity.as_deref(),
        limit,
    ))
}

/// Handler: POST /api/monitor/alerts/:id/acknowledge
pub async fn acknowledge_alert_handler(
    State(monitor): State<MonitorState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    if monitor.acknowledge_alert(&id) {
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ok", "message": "Alert acknowledged"})),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"status": "error", "message": "Alert not found"})),
        )
    }
}

/// Handler: GET /api/monitor/stats
pub async fn get_stats_handler(
    State(monitor): State<MonitorState>,
) -> Json<MonitorStats> {
    Json(monitor.get_stats())
}

/// Handler: GET /api/monitor/dashboard
pub async fn get_dashboard_handler(
    State(monitor): State<MonitorState>,
) -> Html<String> {
    let data = monitor.get_dashboard_data();
    let html = monitor_dashboard_page(&data);
    Html(html)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn provider_monitor_routes() -> Router<MonitorState> {
    Router::new()
        .route(
            "/api/monitor/uptime",
            axum::routing::post(record_uptime_handler),
        )
        .route(
            "/api/monitor/uptime/:provider_id",
            axum::routing::get(get_uptime_history_handler),
        )
        .route("/api/monitor/slas", axum::routing::post(create_sla_handler))
        .route("/api/monitor/slas", axum::routing::get(list_slas_handler))
        .route(
            "/api/monitor/slas/:id/compliance",
            axum::routing::get(check_compliance_handler),
        )
        .route(
            "/api/monitor/violations",
            axum::routing::get(list_violations_handler),
        )
        .route(
            "/api/monitor/reputation",
            axum::routing::get(list_correlations_handler),
        )
        .route(
            "/api/monitor/alerts/rules",
            axum::routing::post(create_alert_rule_handler),
        )
        .route(
            "/api/monitor/alerts/rules",
            axum::routing::get(list_alert_rules_handler),
        )
        .route(
            "/api/monitor/alerts",
            axum::routing::get(list_alerts_handler),
        )
        .route(
            "/api/monitor/alerts/:id/acknowledge",
            axum::routing::post(acknowledge_alert_handler),
        )
        .route(
            "/api/monitor/stats",
            axum::routing::get(get_stats_handler),
        )
        .route(
            "/api/monitor/dashboard",
            axum::routing::get(get_dashboard_handler),
        )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_monitor() -> ProviderMonitor {
        ProviderMonitor::new()
    }

    fn sample_uptime_record(provider_id: &str, status: ProviderStatus, response_ms: u64, error_rate: f64) -> UptimeRecord {
        UptimeRecord {
            provider_id: provider_id.to_string(),
            timestamp: Utc::now(),
            status,
            response_time_ms: response_ms,
            error_rate,
        }
    }

    #[test]
    fn test_monitor_creation() {
        let monitor = make_monitor();
        assert_eq!(monitor.total_checks.load(Ordering::Relaxed), 0);
        assert_eq!(monitor.uptime_records.len(), 0);
    }

    #[test]
    fn test_record_and_get_uptime() {
        let monitor = make_monitor();
        monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Up, 50, 0.01));
        monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Up, 60, 0.02));
        monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Degraded, 200, 0.1));

        let history = monitor.get_uptime_history("prov_1", 100);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_uptime_stats_calculation() {
        let monitor = make_monitor();
        // 8 up, 1 down, 1 degraded -> uptime_pct = (8 + 0.5) / 10 = 0.85
        for _ in 0..8 {
            monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Up, 50, 0.01));
        }
        monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Down, 0, 1.0));
        monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Degraded, 200, 0.1));

        let stats = monitor.get_uptime_stats("prov_1").unwrap();
        assert_eq!(stats.total_checks, 10);
        assert_eq!(stats.up_count, 8);
        assert_eq!(stats.down_count, 1);
        assert_eq!(stats.degraded_count, 1);
        assert!((stats.uptime_pct - 0.85).abs() < 0.001);
        assert_eq!(stats.avg_response_time_ms, 60); // (8*50 + 0 + 200) / 10 = 60
    }

    #[test]
    fn test_uptime_history_for_unknown_provider() {
        let monitor = make_monitor();
        let history = monitor.get_uptime_history("unknown", 100);
        assert!(history.is_empty());
    }

    #[test]
    fn test_sla_crud() {
        let monitor = make_monitor();

        // Create
        let sla = SLADefinition {
            id: "sla_1".to_string(),
            provider_id: "prov_1".to_string(),
            name: "Gold SLA".to_string(),
            target_uptime: 0.99,
            max_response_time_ms: 100,
            max_error_rate: 0.01,
            penalty_rate: 0.05,
            created_at: Utc::now(),
        };
        monitor.create_sla(sla);

        // Get
        let fetched = monitor.get_sla("sla_1").unwrap();
        assert_eq!(fetched.name, "Gold SLA");

        // List
        let all = monitor.list_slas();
        assert_eq!(all.len(), 1);

        // Update
        let update = UpdateSLARequest {
            name: Some("Platinum SLA".to_string()),
            target_uptime: None,
            max_response_time_ms: None,
            max_error_rate: None,
            penalty_rate: None,
        };
        assert!(monitor.update_sla("sla_1", &update));
        let updated = monitor.get_sla("sla_1").unwrap();
        assert_eq!(updated.name, "Platinum SLA");

        // Delete
        assert!(monitor.delete_sla("sla_1"));
        assert!(monitor.get_sla("sla_1").is_none());
        assert!(!monitor.delete_sla("nonexistent"));
    }

    #[test]
    fn test_sla_compliance_violations() {
        let monitor = make_monitor();

        // Record poor uptime: all down
        for _ in 0..5 {
            monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Down, 500, 0.5));
        }

        // Create strict SLA
        let sla = SLADefinition {
            id: "sla_strict".to_string(),
            provider_id: "prov_1".to_string(),
            name: "Strict SLA".to_string(),
            target_uptime: 0.99,
            max_response_time_ms: 100,
            max_error_rate: 0.05,
            penalty_rate: 0.1,
            created_at: Utc::now(),
        };
        monitor.create_sla(sla);

        let violations = monitor.check_sla_compliance("sla_strict");
        assert_eq!(violations.len(), 3); // uptime, response time, and error rate

        // Verify stored violations
        let all_violations = monitor.list_violations(None, None, 100);
        assert_eq!(all_violations.len(), 3);
    }

    #[test]
    fn test_sla_compliance_passing() {
        let monitor = make_monitor();

        // Record excellent uptime
        for _ in 0..10 {
            monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Up, 30, 0.001));
        }

        let sla = SLADefinition {
            id: "sla_easy".to_string(),
            provider_id: "prov_1".to_string(),
            name: "Easy SLA".to_string(),
            target_uptime: 0.9,
            max_response_time_ms: 100,
            max_error_rate: 0.05,
            penalty_rate: 0.1,
            created_at: Utc::now(),
        };
        monitor.create_sla(sla);

        let violations = monitor.check_sla_compliance("sla_easy");
        assert!(violations.is_empty());
    }

    #[test]
    fn test_reputation_correlation() {
        let monitor = make_monitor();

        // Good provider
        for _ in 0..10 {
            monitor.record_uptime(sample_uptime_record("good_prov", ProviderStatus::Up, 40, 0.005));
        }

        let corr = monitor.compute_reputation_correlation("good_prov", 0.95);
        assert_eq!(corr.provider_id, "good_prov");
        assert_eq!(corr.reputation_score, 0.95);
        assert!(corr.uptime_pct > 0.9);
        assert!(corr.correlation_factor > 0.5);
        assert_eq!(corr.violation_count, 0);

        // Verify retrieval
        let fetched = monitor.get_correlation("good_prov").unwrap();
        assert_eq!(fetched.correlation_factor, corr.correlation_factor);

        let all_corrs = monitor.list_correlations();
        assert_eq!(all_corrs.len(), 1);
    }

    #[test]
    fn test_alert_rule_crud_and_toggle() {
        let monitor = make_monitor();

        let rule = AlertRule {
            id: "rule_1".to_string(),
            name: "High Error Rate".to_string(),
            condition_type: AlertConditionType::ErrorRateAbove,
            threshold: 0.1,
            provider_filter: None,
            enabled: true,
            cooldown_minutes: 30,
            created_at: Utc::now(),
        };
        monitor.create_alert_rule(rule);

        let fetched = monitor.get_alert_rule("rule_1").unwrap();
        assert!(fetched.enabled);

        // Disable
        assert!(monitor.disable_alert_rule("rule_1"));
        assert!(!monitor.get_alert_rule("rule_1").unwrap().enabled);

        // Enable
        assert!(monitor.enable_alert_rule("rule_1"));
        assert!(monitor.get_alert_rule("rule_1").unwrap().enabled);

        // Update
        let update = UpdateAlertRuleRequest {
            name: Some("Very High Error Rate".to_string()),
            threshold: Some(0.05),
            provider_filter: Some(Some("prov_1".to_string())),
            cooldown_minutes: Some(60),
        };
        assert!(monitor.update_alert_rule("rule_1", &update));
        let updated = monitor.get_alert_rule("rule_1").unwrap();
        assert_eq!(updated.name, "Very High Error Rate");
        assert_eq!(updated.threshold, 0.05);
        assert_eq!(updated.provider_filter, Some("prov_1".to_string()));

        // Delete
        assert!(monitor.delete_alert_rule("rule_1"));
        assert!(monitor.get_alert_rule("rule_1").is_none());

        // List
        let rules = monitor.list_alert_rules();
        assert!(rules.is_empty());
    }

    #[test]
    fn test_alert_evaluation_triggers() {
        let monitor = make_monitor();

        // Record bad provider data
        for _ in 0..5 {
            monitor.record_uptime(sample_uptime_record("bad_prov", ProviderStatus::Down, 1000, 0.8));
        }

        // Create alert rule for error rate
        let rule = AlertRule {
            id: "err_rule".to_string(),
            name: "Error Rate Alert".to_string(),
            condition_type: AlertConditionType::ErrorRateAbove,
            threshold: 0.1,
            provider_filter: None,
            enabled: true,
            cooldown_minutes: 30,
            created_at: Utc::now(),
        };
        monitor.create_alert_rule(rule);

        let new_alerts = monitor.evaluate_alerts();
        assert_eq!(new_alerts.len(), 1);
        assert_eq!(new_alerts[0].provider_id, "bad_prov");
        assert!(!new_alerts[0].acknowledged);

        // Re-evaluate should not duplicate (cooldown via existing unacknowledged)
        let again = monitor.evaluate_alerts();
        assert!(again.is_empty());
    }

    #[test]
    fn test_alert_acknowledgment() {
        let monitor = make_monitor();

        let alert = Alert {
            id: "alert_1".to_string(),
            rule_id: "rule_1".to_string(),
            rule_name: "Test Rule".to_string(),
            provider_id: "prov_1".to_string(),
            message: "Test alert".to_string(),
            severity: AlertSeverity::Warning,
            triggered_at: Utc::now(),
            acknowledged: false,
        };
        monitor.alerts.insert(alert.id.clone(), alert);

        let active = monitor.get_active_alerts();
        assert_eq!(active.len(), 1);

        assert!(monitor.acknowledge_alert("alert_1"));
        let active_after = monitor.get_active_alerts();
        assert!(active_after.is_empty());

        // Cannot acknowledge again
        assert!(!monitor.acknowledge_alert("nonexistent"));
    }

    #[test]
    fn test_violation_filtering() {
        let monitor = make_monitor();

        let v1 = SLAViolation {
            id: "v1".to_string(),
            sla_id: "sla_1".to_string(),
            provider_id: "prov_a".to_string(),
            violation_type: ViolationType::Uptime,
            actual_value: 0.5,
            threshold: 0.99,
            timestamp: Utc::now(),
            acknowledged: false,
        };
        let v2 = SLAViolation {
            id: "v2".to_string(),
            sla_id: "sla_1".to_string(),
            provider_id: "prov_b".to_string(),
            violation_type: ViolationType::ErrorRate,
            actual_value: 0.2,
            threshold: 0.05,
            timestamp: Utc::now(),
            acknowledged: true,
        };
        monitor.record_violation(v1);
        monitor.record_violation(v2);

        // Filter by provider
        let prov_a = monitor.list_violations(Some("prov_a"), None, 100);
        assert_eq!(prov_a.len(), 1);

        // Filter by acknowledged
        let unacked = monitor.list_violations(None, Some(false), 100);
        assert_eq!(unacked.len(), 1);

        // No filter
        let all = monitor.list_violations(None, None, 100);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_concurrent_access() {
        let monitor = Arc::new(make_monitor());
        let mut handles = vec![];

        for i in 0..5 {
            let m = monitor.clone();
            handles.push(std::thread::spawn(move || {
                let pid = format!("prov_{}", i);
                for j in 0..10 {
                    m.record_uptime(sample_uptime_record(
                        &pid,
                        if j % 3 == 0 { ProviderStatus::Degraded } else { ProviderStatus::Up },
                        50 + j,
                        0.01 * j as f64,
                    ));
                }
                m.compute_reputation_correlation(&pid, 0.8 + i as f64 * 0.04);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let stats = monitor.get_stats();
        assert_eq!(stats.providers_tracked, 5);
        assert_eq!(stats.total_uptime_checks, 50);

        let corrs = monitor.list_correlations();
        assert_eq!(corrs.len(), 5);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let record = sample_uptime_record("prov_1", ProviderStatus::Up, 42, 0.03);
        let json = serde_json::to_string(&record).unwrap();
        let decoded: UptimeRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "prov_1");
        assert_eq!(decoded.response_time_ms, 42);
        assert_eq!(decoded.status, ProviderStatus::Up);
    }

    #[test]
    fn test_sla_serialization_roundtrip() {
        let sla = SLADefinition {
            id: "sla_1".to_string(),
            provider_id: "prov_1".to_string(),
            name: "Gold".to_string(),
            target_uptime: 0.99,
            max_response_time_ms: 100,
            max_error_rate: 0.01,
            penalty_rate: 0.05,
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&sla).unwrap();
        let decoded: SLADefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "Gold");
        assert_eq!(decoded.target_uptime, 0.99);
    }

    #[test]
    fn test_alert_serialization_roundtrip() {
        let alert = Alert {
            id: "a1".to_string(),
            rule_id: "r1".to_string(),
            rule_name: "Test".to_string(),
            provider_id: "p1".to_string(),
            message: "Alert!".to_string(),
            severity: AlertSeverity::Critical,
            triggered_at: Utc::now(),
            acknowledged: false,
        };
        let json = serde_json::to_string(&alert).unwrap();
        let decoded: Alert = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.severity, AlertSeverity::Critical);
        assert_eq!(decoded.provider_id, "p1");
    }

    #[test]
    fn test_stats_and_dashboard_data() {
        let monitor = make_monitor();
        monitor.record_uptime(sample_uptime_record("p1", ProviderStatus::Up, 50, 0.01));
        monitor.record_uptime(sample_uptime_record("p2", ProviderStatus::Down, 0, 1.0));

        let stats = monitor.get_stats();
        assert_eq!(stats.providers_tracked, 2);
        assert_eq!(stats.total_uptime_checks, 2);

        let dashboard = monitor.get_dashboard_data();
        assert_eq!(dashboard.stats.providers_tracked, 2);
    }

    #[test]
    fn test_uptime_records_bounded() {
        let monitor = make_monitor();
        for i in 0..(MAX_UPTIME_RECORDS + 100) {
            monitor.record_uptime(sample_uptime_record("prov_1", ProviderStatus::Up, 50, 0.01));
        }
        let history = monitor.get_uptime_history("prov_1", MAX_UPTIME_RECORDS + 200);
        assert!(history.len() <= MAX_UPTIME_RECORDS);
    }

    #[test]
    fn test_html_dashboard_page() {
        let dashboard = DashboardData {
            stats: MonitorStats {
                providers_tracked: 3,
                total_uptime_checks: 150,
                total_sla_definitions: 2,
                total_violations: 5,
                active_alerts: 1,
                acknowledged_alerts: 3,
                alert_rules: 4,
            },
            top_violations: vec![SLAViolation {
                id: "v1".to_string(),
                sla_id: "sla_1".to_string(),
                provider_id: "prov_1".to_string(),
                violation_type: ViolationType::Uptime,
                actual_value: 0.85,
                threshold: 0.99,
                timestamp: Utc::now(),
                acknowledged: false,
            }],
            active_alerts: vec![Alert {
                id: "a1".to_string(),
                rule_id: "r1".to_string(),
                rule_name: "High Error".to_string(),
                provider_id: "prov_1".to_string(),
                message: "Error rate exceeded".to_string(),
                severity: AlertSeverity::Critical,
                triggered_at: Utc::now(),
                acknowledged: false,
            }],
            recent_activity: vec![],
        };
        let html = monitor_dashboard_page(&dashboard);
        assert!(html.contains("Xergon Provider Monitor"));
        assert!(html.contains("prov_1"));
        assert!(html.contains("150"));
        assert!(html.contains("<!DOCTYPE html>"));
    }
}
