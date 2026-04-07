use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SlaLevel
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlaLevel {
    Platinum, // 99.99%
    Gold,     // 99.9%
    Silver,   // 99.5%
    Bronze,   // 99.0%
    None,     // below 99%
}

impl SlaLevel {
    /// Determine SLA level from an uptime percentage.
    pub fn from_uptime(uptime_pct: f64) -> Self {
        if uptime_pct >= 99.99 {
            Self::Platinum
        } else if uptime_pct >= 99.9 {
            Self::Gold
        } else if uptime_pct >= 99.5 {
            Self::Silver
        } else if uptime_pct >= 99.0 {
            Self::Bronze
        } else {
            Self::None
        }
    }

    /// Get the minimum uptime percentage required for this SLA level.
    pub fn min_uptime(&self) -> f64 {
        match self {
            Self::Platinum => 99.99,
            Self::Gold => 99.9,
            Self::Silver => 99.5,
            Self::Bronze => 99.0,
            Self::None => 0.0,
        }
    }

    /// Get the credit multiplier for this SLA level (higher tiers = more credits).
    pub fn credit_multiplier(&self) -> f64 {
        match self {
            Self::Platinum => 3.0,
            Self::Gold => 2.0,
            Self::Silver => 1.5,
            Self::Bronze => 1.0,
            Self::None => 0.5,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Platinum => "Platinum",
            Self::Gold => "Gold",
            Self::Silver => "Silver",
            Self::Bronze => "Bronze",
            Self::None => "None",
        }
    }
}

impl std::fmt::Display for SlaLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// SlaViolationType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SlaViolationType {
    Downtime,
    Latency,
    ErrorRate,
}

impl std::fmt::Display for SlaViolationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Downtime => write!(f, "Downtime"),
            Self::Latency => write!(f, "Latency"),
            Self::ErrorRate => write!(f, "ErrorRate"),
        }
    }
}

// ---------------------------------------------------------------------------
// ViolationSeverity
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViolationSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl ViolationSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }
}

// ---------------------------------------------------------------------------
// SlaMetric
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SlaMetric {
    pub uptime_pct: f64,
    pub avg_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub error_rate: f64,
    pub requests_per_hour: u64,
    pub availability_windows: Vec<AvailabilityWindow>,
}

impl Default for SlaMetric {
    fn default() -> Self {
        Self {
            uptime_pct: 100.0,
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            error_rate: 0.0,
            requests_per_hour: 0,
            availability_windows: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// AvailabilityWindow
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AvailabilityWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub available: bool,
}

// ---------------------------------------------------------------------------
// SlaViolation
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SlaViolation {
    pub violation_id: String,
    pub violation_type: SlaViolationType,
    pub severity: ViolationSeverity,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub impact_description: String,
    pub duration_minutes: Option<u64>,
    pub credit_impact: f64,
}

impl SlaViolation {
    pub fn new(
        violation_type: SlaViolationType,
        severity: ViolationSeverity,
        impact_description: &str,
    ) -> Self {
        Self {
            violation_id: uuid::Uuid::new_v4().to_string(),
            violation_type,
            severity,
            started_at: Utc::now(),
            ended_at: None,
            impact_description: impact_description.to_string(),
            duration_minutes: None,
            credit_impact: 0.0,
        }
    }

    pub fn resolve(&mut self) {
        self.ended_at = Some(Utc::now());
        if let Some(end) = self.ended_at {
            let duration = end - self.started_at;
            self.duration_minutes = Some(duration.num_minutes().max(1) as u64);
        }
    }
}

// ---------------------------------------------------------------------------
// SlaCreditRecord
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SlaCreditRecord {
    pub id: String,
    pub provider_id: String,
    pub amount: f64,
    pub reason: String,
    pub violation_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// SlaReport
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SlaReport {
    pub provider_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub level: SlaLevel,
    pub metrics: SlaMetric,
    pub violations: Vec<SlaViolation>,
    pub credits: Vec<SlaCreditRecord>,
    pub total_credits: f64,
}

// ---------------------------------------------------------------------------
// SlaConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SlaConfig {
    pub measurement_window_hours: u64,
    pub max_latency_ms: f64,
    pub max_p99_latency_ms: f64,
    pub max_error_rate: f64,
    pub credit_per_minute_downtime: f64,
    pub latency_violation_threshold_ms: f64,
    pub error_rate_violation_threshold: f64,
}

impl Default for SlaConfig {
    fn default() -> Self {
        Self {
            measurement_window_hours: 720, // 30 days
            max_latency_ms: 500.0,
            max_p99_latency_ms: 2000.0,
            max_error_rate: 0.01, // 1%
            credit_per_minute_downtime: 0.1,
            latency_violation_threshold_ms: 1000.0,
            error_rate_violation_threshold: 0.05, // 5%
        }
    }
}

// ---------------------------------------------------------------------------
// SlaTrendPoint
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SlaTrendPoint {
    pub timestamp: DateTime<Utc>,
    pub uptime_pct: f64,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
    pub level: SlaLevel,
}

// ---------------------------------------------------------------------------
// ProviderSlaState
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ProviderSlaState {
    #[allow(dead_code)]
    provider_id: String,
    metrics: SlaMetric,
    violations: Vec<SlaViolation>,
    credits: Vec<SlaCreditRecord>,
    trends: Vec<SlaTrendPoint>,
    config: SlaConfig,
    active_violations: HashMap<String, SlaViolation>,
}

impl ProviderSlaState {
    fn new(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            metrics: SlaMetric::default(),
            violations: Vec::new(),
            credits: Vec::new(),
            trends: Vec::new(),
            config: SlaConfig::default(),
            active_violations: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// DashboardSummary
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DashboardSummary {
    pub total_providers: usize,
    pub providers_by_level: HashMap<String, usize>,
    pub total_active_violations: usize,
    pub total_credits_issued: f64,
    pub average_uptime: f64,
    pub average_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// SlaDashboard
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SlaDashboard {
    providers: DashMap<String, ProviderSlaState>,
    violation_counter: AtomicU64,
}

impl Default for SlaDashboard {
    fn default() -> Self {
        Self::new()
    }
}

impl SlaDashboard {
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
            violation_counter: AtomicU64::new(0),
        }
    }

    /// Register a provider for SLA tracking.
    pub fn register_provider(&self, provider_id: &str, config: Option<SlaConfig>) {
        let mut state = ProviderSlaState::new(provider_id);
        if let Some(cfg) = config {
            state.config = cfg;
        }
        self.providers.insert(provider_id.to_string(), state);
    }

    /// Calculate the current SLA level for a provider.
    pub fn calculate_sla(&self, provider_id: &str) -> Result<SlaLevel, String> {
        let state = self
            .providers
            .get(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        Ok(SlaLevel::from_uptime(state.metrics.uptime_pct))
    }

    /// Record a violation for a provider.
    pub fn record_violation(
        &self,
        provider_id: &str,
        violation_type: SlaViolationType,
        severity: ViolationSeverity,
        impact_description: &str,
    ) -> Result<SlaViolation, String> {
        let mut state = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let mut violation = SlaViolation::new(violation_type, severity, impact_description);

        // Calculate credit impact
        violation.credit_impact = self.calculate_violation_credit(&state, &violation);

        let violation_id = violation.violation_id.clone();
        state.active_violations.insert(violation_id.clone(), violation.clone());
        state.violations.push(violation.clone());

        self.violation_counter.fetch_add(1, Ordering::Relaxed);

        Ok(violation)
    }

    /// Resolve an active violation.
    pub fn resolve_violation(
        &self,
        provider_id: &str,
        violation_id: &str,
    ) -> Result<SlaViolation, String> {
        let mut state = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let mut violation = state
            .active_violations
            .remove(violation_id)
            .ok_or_else(|| format!("Violation '{}' not found or not active", violation_id))?;

        violation.resolve();

        // Recalculate credit impact based on actual duration
        let credit = self.calculate_violation_credit(&state, &violation);
        violation.credit_impact = credit;

        // Issue credit
        let sla_level = SlaLevel::from_uptime(state.metrics.uptime_pct);
        let credit_amount = credit * sla_level.credit_multiplier();
        let credit_record = SlaCreditRecord {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: provider_id.to_string(),
            amount: credit_amount,
            reason: format!("{} violation: {}", violation.violation_type, violation.impact_description),
            violation_id: Some(violation_id.to_string()),
            created_at: Utc::now(),
            period_start: violation.started_at,
            period_end: violation.ended_at.unwrap_or_else(Utc::now),
        };
        state.credits.push(credit_record);

        // Update the violation in the full list
        if let Some(v) = state.violations.iter_mut().find(|v| v.violation_id == violation_id) {
            *v = violation.clone();
        }

        Ok(violation)
    }

    /// Get the SLA report for a provider.
    pub fn get_report(&self, provider_id: &str, hours: u64) -> Result<SlaReport, String> {
        let state = self
            .providers
            .get(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let now = Utc::now();
        let period_start = now - Duration::hours(hours as i64);
        let level = SlaLevel::from_uptime(state.metrics.uptime_pct);

        let violations: Vec<SlaViolation> = state
            .violations
            .iter()
            .filter(|v| v.started_at >= period_start)
            .cloned()
            .collect();

        let credits: Vec<SlaCreditRecord> = state
            .credits
            .iter()
            .filter(|c| c.created_at >= period_start)
            .cloned()
            .collect();

        let total_credits: f64 = credits.iter().map(|c| c.amount).sum();

        Ok(SlaReport {
            provider_id: provider_id.to_string(),
            period_start,
            period_end: now,
            level,
            metrics: state.metrics.clone(),
            violations,
            credits,
            total_credits,
        })
    }

    /// Get the full dashboard summary.
    pub fn get_dashboard(&self) -> DashboardSummary {
        let mut total_providers = 0usize;
        let mut providers_by_level: HashMap<String, usize> = HashMap::new();
        let mut total_active_violations = 0usize;
        let mut total_credits_issued = 0.0;
        let mut uptime_sum = 0.0;
        let mut latency_sum = 0.0;
        let mut count = 0usize;

        for entry in self.providers.iter() {
            let state = entry.value();
            total_providers += 1;
            total_active_violations += state.active_violations.len();
            total_credits_issued += state.credits.iter().map(|c| c.amount).sum::<f64>();
            uptime_sum += state.metrics.uptime_pct;
            latency_sum += state.metrics.avg_latency_ms;
            count += 1;

            let level = SlaLevel::from_uptime(state.metrics.uptime_pct);
            *providers_by_level.entry(level.as_str().to_string()).or_insert(0) += 1;
        }

        DashboardSummary {
            total_providers,
            providers_by_level,
            total_active_violations,
            total_credits_issued,
            average_uptime: if count > 0 { uptime_sum / count as f64 } else { 0.0 },
            average_latency_ms: if count > 0 { latency_sum / count as f64 } else { 0.0 },
        }
    }

    /// Calculate credits for a provider over a period.
    pub fn calculate_credits(&self, provider_id: &str, hours: u64) -> Result<f64, String> {
        let state = self
            .providers
            .get(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let cutoff = Utc::now() - Duration::hours(hours as i64);
        let total: f64 = state
            .credits
            .iter()
            .filter(|c| c.created_at >= cutoff)
            .map(|c| c.amount)
            .sum();

        Ok(total)
    }

    /// Get SLA trends for a provider.
    pub fn get_trends(&self, provider_id: &str, limit: usize) -> Result<Vec<SlaTrendPoint>, String> {
        let state = self
            .providers
            .get(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let mut trends = state.trends.clone();
        trends.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        trends.truncate(limit);
        Ok(trends)
    }

    /// Update provider metrics.
    pub fn update_metrics(&self, provider_id: &str, metrics: SlaMetric) -> Result<(), String> {
        let mut state = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        // Record trend point before updating
        let trend = SlaTrendPoint {
            timestamp: Utc::now(),
            uptime_pct: metrics.uptime_pct,
            avg_latency_ms: metrics.avg_latency_ms,
            error_rate: metrics.error_rate,
            level: SlaLevel::from_uptime(metrics.uptime_pct),
        };
        state.trends.push(trend);

        state.metrics = metrics;

        // Check for automatic violation triggers
        self.check_auto_violations(&mut state);

        Ok(())
    }

    /// Get violations for a provider.
    pub fn get_violations(&self, provider_id: &str) -> Result<Vec<SlaViolation>, String> {
        let state = self
            .providers
            .get(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let mut violations = state.violations.clone();
        violations.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(violations)
    }

    /// Get provider SLA config.
    pub fn get_config(&self, provider_id: &str) -> Result<SlaConfig, String> {
        let state = self
            .providers
            .get(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        Ok(state.config.clone())
    }

    /// Update provider SLA config.
    pub fn update_config(&self, provider_id: &str, config: SlaConfig) -> Result<(), String> {
        let mut state = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        state.config = config;
        Ok(())
    }

    /// Record a trend data point.
    pub fn record_trend(
        &self,
        provider_id: &str,
        uptime_pct: f64,
        avg_latency_ms: f64,
        error_rate: f64,
    ) -> Result<(), String> {
        let mut state = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let trend = SlaTrendPoint {
            timestamp: Utc::now(),
            uptime_pct,
            avg_latency_ms,
            error_rate,
            level: SlaLevel::from_uptime(uptime_pct),
        };
        state.trends.push(trend);

        Ok(())
    }

    /// Get dashboard stats.
    pub fn get_stats(&self) -> SlaDashboardStats {
        let total_providers = self.providers.len();
        let total_violations = self.violation_counter.load(Ordering::Relaxed);
        let mut total_credits = 0.0;
        let mut active_violations = 0usize;

        for entry in self.providers.iter() {
            total_credits += entry.value().credits.iter().map(|c| c.amount).sum::<f64>();
            active_violations += entry.value().active_violations.len();
        }

        SlaDashboardStats {
            total_providers,
            total_violations,
            total_credits_issued: total_credits,
            active_violations,
        }
    }

    // -- Internal methods --

    fn calculate_violation_credit(&self, state: &ProviderSlaState, violation: &SlaViolation) -> f64 {
        let base_credit = match violation.violation_type {
            SlaViolationType::Downtime => {
                let minutes = violation.duration_minutes.unwrap_or(30);
                minutes as f64 * state.config.credit_per_minute_downtime
            }
            SlaViolationType::Latency => {
                match violation.severity {
                    ViolationSeverity::Critical => 50.0,
                    ViolationSeverity::High => 25.0,
                    ViolationSeverity::Medium => 10.0,
                    ViolationSeverity::Low => 2.0,
                }
            }
            SlaViolationType::ErrorRate => {
                match violation.severity {
                    ViolationSeverity::Critical => 75.0,
                    ViolationSeverity::High => 40.0,
                    ViolationSeverity::Medium => 15.0,
                    ViolationSeverity::Low => 5.0,
                }
            }
        };

        base_credit * SlaLevel::from_uptime(state.metrics.uptime_pct).credit_multiplier()
    }

    fn check_auto_violations(&self, state: &mut ProviderSlaState) {
        // Check latency violation
        if state.metrics.p99_latency_ms > state.config.latency_violation_threshold_ms {
            let severity = if state.metrics.p99_latency_ms > state.config.latency_violation_threshold_ms * 2.0 {
                ViolationSeverity::Critical
            } else if state.metrics.p99_latency_ms > state.config.latency_violation_threshold_ms * 1.5 {
                ViolationSeverity::High
            } else {
                ViolationSeverity::Medium
            };

            let mut violation = SlaViolation::new(
                SlaViolationType::Latency,
                severity,
                &format!(
                    "P99 latency {}ms exceeds threshold {}ms",
                    state.metrics.p99_latency_ms, state.config.latency_violation_threshold_ms
                ),
            );
            violation.credit_impact = self.calculate_violation_credit(state, &violation);
            let vid = violation.violation_id.clone();
            state.active_violations.insert(vid, violation.clone());
            state.violations.push(violation);
            self.violation_counter.fetch_add(1, Ordering::Relaxed);
        }

        // Check error rate violation
        if state.metrics.error_rate > state.config.error_rate_violation_threshold {
            let severity = if state.metrics.error_rate > state.config.error_rate_violation_threshold * 2.0 {
                ViolationSeverity::Critical
            } else if state.metrics.error_rate > state.config.error_rate_violation_threshold * 1.5 {
                ViolationSeverity::High
            } else {
                ViolationSeverity::Medium
            };

            let mut violation = SlaViolation::new(
                SlaViolationType::ErrorRate,
                severity,
                &format!(
                    "Error rate {:.2}% exceeds threshold {:.2}%",
                    state.metrics.error_rate * 100.0,
                    state.config.error_rate_violation_threshold * 100.0
                ),
            );
            violation.credit_impact = self.calculate_violation_credit(state, &violation);
            let vid = violation.violation_id.clone();
            state.active_violations.insert(vid, violation.clone());
            state.violations.push(violation);
            self.violation_counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// SlaDashboardStats
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SlaDashboardStats {
    pub total_providers: usize,
    pub total_violations: u64,
    pub total_credits_issued: f64,
    pub active_violations: usize,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dashboard() -> SlaDashboard {
        SlaDashboard::new()
    }

    fn register_test_provider(dashboard: &SlaDashboard, id: &str) {
        dashboard.register_provider(id, None);
        let metrics = SlaMetric {
            uptime_pct: 99.95,
            avg_latency_ms: 120.0,
            p99_latency_ms: 500.0,
            error_rate: 0.005,
            requests_per_hour: 10000,
            availability_windows: Vec::new(),
        };
        dashboard.update_metrics(id, metrics).unwrap();
    }

    // -- SLA level calculation --

    #[test]
    fn test_sla_level_platinum() {
        assert_eq!(SlaLevel::from_uptime(99.99), SlaLevel::Platinum);
        assert_eq!(SlaLevel::from_uptime(100.0), SlaLevel::Platinum);
    }

    #[test]
    fn test_sla_level_gold() {
        assert_eq!(SlaLevel::from_uptime(99.9), SlaLevel::Gold);
        assert_eq!(SlaLevel::from_uptime(99.95), SlaLevel::Gold);
    }

    #[test]
    fn test_sla_level_silver() {
        assert_eq!(SlaLevel::from_uptime(99.5), SlaLevel::Silver);
        assert_eq!(SlaLevel::from_uptime(99.7), SlaLevel::Silver);
    }

    #[test]
    fn test_sla_level_bronze() {
        assert_eq!(SlaLevel::from_uptime(99.0), SlaLevel::Bronze);
        assert_eq!(SlaLevel::from_uptime(99.3), SlaLevel::Bronze);
    }

    #[test]
    fn test_sla_level_none() {
        assert_eq!(SlaLevel::from_uptime(98.5), SlaLevel::None);
        assert_eq!(SlaLevel::from_uptime(95.0), SlaLevel::None);
    }

    #[test]
    fn test_sla_level_min_uptime() {
        assert_eq!(SlaLevel::Platinum.min_uptime(), 99.99);
        assert_eq!(SlaLevel::Gold.min_uptime(), 99.9);
        assert_eq!(SlaLevel::Silver.min_uptime(), 99.5);
        assert_eq!(SlaLevel::Bronze.min_uptime(), 99.0);
        assert_eq!(SlaLevel::None.min_uptime(), 0.0);
    }

    // -- violation recording --

    #[test]
    fn test_record_violation() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");

        let violation = dash
            .record_violation(
                "p1",
                SlaViolationType::Downtime,
                ViolationSeverity::High,
                "Unplanned maintenance",
            )
            .unwrap();

        assert_eq!(violation.violation_type, SlaViolationType::Downtime);
        assert_eq!(violation.severity, ViolationSeverity::High);
        assert!(violation.credit_impact > 0.0);
        assert!(violation.ended_at.is_none());
    }

    #[test]
    fn test_record_violation_unknown_provider() {
        let dash = make_dashboard();
        let result = dash.record_violation(
            "unknown",
            SlaViolationType::Downtime,
            ViolationSeverity::High,
            "test",
        );
        assert!(result.is_err());
    }

    // -- violation resolution and credit calculation --

    #[test]
    fn test_resolve_violation_and_credits() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");

        let violation = dash
            .record_violation(
                "p1",
                SlaViolationType::Downtime,
                ViolationSeverity::High,
                "Outage",
            )
            .unwrap();

        let vid = violation.violation_id.clone();
        let resolved = dash.resolve_violation("p1", &vid).unwrap();

        assert!(resolved.ended_at.is_some());
        assert!(resolved.duration_minutes.is_some());

        let credits = dash.calculate_credits("p1", 720).unwrap();
        assert!(credits > 0.0);
    }

    #[test]
    fn test_credit_calculation_multiplier() {
        assert_eq!(SlaLevel::Platinum.credit_multiplier(), 3.0);
        assert_eq!(SlaLevel::Gold.credit_multiplier(), 2.0);
        assert_eq!(SlaLevel::Silver.credit_multiplier(), 1.5);
        assert_eq!(SlaLevel::Bronze.credit_multiplier(), 1.0);
        assert_eq!(SlaLevel::None.credit_multiplier(), 0.5);
    }

    // -- trend analysis --

    #[test]
    fn test_trend_recording() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");

        dash.record_trend("p1", 99.9, 100.0, 0.01).unwrap();
        dash.record_trend("p1", 99.8, 150.0, 0.02).unwrap();
        dash.record_trend("p1", 99.95, 80.0, 0.005).unwrap();

        let trends = dash.get_trends("p1", 10).unwrap();
        // register_test_provider calls update_metrics which adds 1 trend
        assert_eq!(trends.len(), 4);
        // Most recent first
        assert!(trends[0].timestamp >= trends[1].timestamp);
    }

    #[test]
    fn test_trend_limit() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");

        for i in 0..10 {
            dash.record_trend("p1", 99.0 + (i as f64) * 0.1, 100.0, 0.01)
                .unwrap();
        }

        let trends = dash.get_trends("p1", 5).unwrap();
        assert_eq!(trends.len(), 5);
    }

    // -- dashboard aggregation --

    #[test]
    fn test_dashboard_summary() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");
        register_test_provider(&dash, "p2");

        let summary = dash.get_dashboard();
        assert_eq!(summary.total_providers, 2);
        assert!(summary.average_uptime > 0.0);
    }

    #[test]
    fn test_dashboard_violations_count() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");

        dash.record_violation("p1", SlaViolationType::Downtime, ViolationSeverity::High, "test")
            .unwrap();
        dash.record_violation("p1", SlaViolationType::Latency, ViolationSeverity::Medium, "slow")
            .unwrap();

        let summary = dash.get_dashboard();
        assert_eq!(summary.total_active_violations, 2);
    }

    // -- report generation --

    #[test]
    fn test_get_report() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");

        let report = dash.get_report("p1", 720).unwrap();
        assert_eq!(report.provider_id, "p1");
        assert_eq!(report.level, SlaLevel::Gold);
        assert!(report.metrics.uptime_pct > 0.0);
    }

    // -- config management --

    #[test]
    fn test_update_config() {
        let dash = make_dashboard();
        dash.register_provider("p1", None);

        let new_config = SlaConfig {
            max_latency_ms: 300.0,
            credit_per_minute_downtime: 0.5,
            ..Default::default()
        };
        dash.update_config("p1", new_config.clone()).unwrap();

        let retrieved = dash.get_config("p1").unwrap();
        assert_eq!(retrieved.max_latency_ms, 300.0);
        assert_eq!(retrieved.credit_per_minute_downtime, 0.5);
    }

    // -- auto violation detection --

    #[test]
    fn test_auto_latency_violation() {
        let dash = make_dashboard();
        dash.register_provider("p1", None);

        // Set metrics that will trigger auto-violation
        let metrics = SlaMetric {
            uptime_pct: 99.5,
            avg_latency_ms: 200.0,
            p99_latency_ms: 3000.0, // Exceeds default threshold of 1000ms
            error_rate: 0.005,
            requests_per_hour: 5000,
            availability_windows: Vec::new(),
        };
        dash.update_metrics("p1", metrics).unwrap();

        let violations = dash.get_violations("p1").unwrap();
        assert!(!violations.is_empty());
        let latency_violations: Vec<_> = violations
            .iter()
            .filter(|v| v.violation_type == SlaViolationType::Latency)
            .collect();
        assert!(!latency_violations.is_empty());
    }

    // -- stats --

    #[test]
    fn test_dashboard_stats() {
        let dash = make_dashboard();
        register_test_provider(&dash, "p1");
        register_test_provider(&dash, "p2");

        let stats = dash.get_stats();
        assert_eq!(stats.total_providers, 2);
    }
}
