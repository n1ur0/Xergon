//! Inference Cost Tracking
//!
//! Tracks per-request, per-model, and per-provider inference costs with budgeting
//! and alerting. Provides cost aggregation, budget management, and threshold-based
//! alerts for spending control.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Cost entry
// ---------------------------------------------------------------------------

/// A single cost entry representing one inference request.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostEntry {
    /// Unique entry identifier.
    pub id: String,
    /// The request this cost entry belongs to.
    pub request_id: String,
    /// The model used for inference.
    pub model_id: String,
    /// The provider that served the inference.
    pub provider_id: String,
    /// Number of input tokens consumed.
    pub input_tokens: u32,
    /// Number of output tokens produced.
    pub output_tokens: u32,
    /// Cost in nanoERG (1 ERG = 1,000,000,000 nanoERG).
    pub cost_nanoerg: u64,
    /// When this cost was incurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional metadata about the request.
    pub metadata: HashMap<String, String>,
}

impl CostEntry {
    /// Creates a new cost entry.
    pub fn new(
        request_id: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        input_tokens: u32,
        output_tokens: u32,
        cost_nanoerg: u64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: request_id.into(),
            model_id: model_id.into(),
            provider_id: provider_id.into(),
            input_tokens,
            output_tokens,
            cost_nanoerg,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Returns the total tokens (input + output) for this entry.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens as u64 + self.output_tokens as u64
    }
}

// ---------------------------------------------------------------------------
// Cost aggregation
// ---------------------------------------------------------------------------

/// Aggregated cost statistics for a model or provider over a time period.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostAggregation {
    /// The model or provider being aggregated.
    pub model_id: String,
    /// The provider ID (when aggregating by model, this may be "all").
    pub provider_id: String,
    /// Total number of requests in this period.
    pub total_requests: u64,
    /// Total tokens consumed in this period.
    pub total_tokens: u64,
    /// Total cost in nanoERG for this period.
    pub total_cost_nanoerg: u64,
    /// Average cost per request in nanoERG.
    pub avg_cost_per_request: u64,
    /// Start of the aggregation period.
    pub period_start: chrono::DateTime<chrono::Utc>,
    /// End of the aggregation period.
    pub period_end: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Cost alert
// ---------------------------------------------------------------------------

/// An alert triggered when a budget threshold is crossed.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostAlert {
    /// Unique alert identifier.
    pub id: String,
    /// The budget that triggered this alert.
    pub budget_id: String,
    /// The threshold percentage that was crossed (e.g., 50.0 for 50%).
    pub threshold_pct: f64,
    /// Current cost in nanoERG when the alert was triggered.
    pub current_cost_nanoerg: u64,
    /// Budget limit in nanoERG.
    pub budget_limit_nanoerg: u64,
    /// When the alert was triggered.
    pub triggered_at: chrono::DateTime<chrono::Utc>,
    /// Whether this alert has been acknowledged.
    #[serde(skip)]
    pub acknowledged: Arc<AtomicBool>,
}

impl CostAlert {
    /// Creates a new cost alert.
    pub fn new(
        budget_id: impl Into<String>,
        threshold_pct: f64,
        current_cost_nanoerg: u64,
        budget_limit_nanoerg: u64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            budget_id: budget_id.into(),
            threshold_pct,
            current_cost_nanoerg,
            budget_limit_nanoerg,
            triggered_at: Utc::now(),
            acknowledged: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns whether this alert has been acknowledged.
    pub fn is_acknowledged(&self) -> bool {
        self.acknowledged.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Cost budget
// ---------------------------------------------------------------------------

/// A budget for controlling inference spending.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostBudget {
    /// Unique budget identifier.
    pub id: String,
    /// Human-readable budget name.
    pub name: String,
    /// Maximum spend allowed in nanoERG.
    pub limit_nanoerg: u64,
    /// Current spend in nanoERG.
    #[serde(skip)]
    pub current_nanoerg: Arc<AtomicU64>,
    /// Threshold percentages that trigger alerts (e.g., [50.0, 75.0, 90.0]).
    pub alert_thresholds: Vec<f64>,
    /// Budget period: "daily", "weekly", "monthly".
    #[serde(default = "default_period")]
    pub period: String,
    /// When this budget was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

fn default_period() -> String {
    "monthly".to_string()
}

impl CostBudget {
    /// Creates a new cost budget.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        limit_nanoerg: u64,
        alert_thresholds: Vec<f64>,
        period: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            limit_nanoerg,
            current_nanoerg: Arc::new(AtomicU64::new(0)),
            alert_thresholds,
            period: period.into(),
            created_at: Utc::now(),
        }
    }

    /// Returns the current spend in nanoERG.
    pub fn current_spend(&self) -> u64 {
        self.current_nanoerg.load(Ordering::Relaxed)
    }

    /// Returns the remaining budget in nanoERG.
    pub fn remaining(&self) -> u64 {
        self.limit_nanoerg.saturating_sub(self.current_spend())
    }

    /// Returns the budget utilization as a percentage (0.0 - 100.0).
    pub fn utilization_pct(&self) -> f64 {
        if self.limit_nanoerg == 0 {
            return 100.0;
        }
        let current = self.current_spend() as f64;
        let limit = self.limit_nanoerg as f64;
        ((current / limit) * 100.0).min(100.0)
    }

    /// Returns whether the budget has been exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.current_spend() >= self.limit_nanoerg
    }

    /// Resets the current spend to zero.
    pub fn reset(&self) {
        self.current_nanoerg.store(0, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Inference cost tracker
// ---------------------------------------------------------------------------

/// Tracks inference costs across requests, models, and providers with budgeting.
pub struct InferenceCostTracker {
    /// All recorded cost entries, keyed by entry ID.
    pub entries: DashMap<String, CostEntry>,
    /// All budgets, keyed by budget ID.
    pub budgets: DashMap<String, Arc<CostBudget>>,
    /// All alerts, keyed by alert ID.
    pub alerts: DashMap<String, CostAlert>,
    /// Total number of cost entries recorded.
    pub total_entries: AtomicU64,
    /// Total cost across all entries in nanoERG.
    pub total_cost_nanoerg: AtomicU64,
}

impl InferenceCostTracker {
    /// Creates a new, empty inference cost tracker.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            budgets: DashMap::new(),
            alerts: DashMap::new(),
            total_entries: AtomicU64::new(0),
            total_cost_nanoerg: AtomicU64::new(0),
        }
    }

    // ---- Cost recording ----

    /// Records a new cost entry for an inference request.
    /// Returns the created cost entry.
    pub fn record_cost(
        &self,
        request_id: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        input_tokens: u32,
        output_tokens: u32,
        cost_nanoerg: u64,
    ) -> CostEntry {
        let entry = CostEntry::new(
            request_id,
            model_id,
            provider_id,
            input_tokens,
            output_tokens,
            cost_nanoerg,
        );
        self.entries.insert(entry.id.clone(), entry.clone());
        self.total_entries.fetch_add(1, Ordering::Relaxed);
        self.total_cost_nanoerg
            .fetch_add(cost_nanoerg, Ordering::Relaxed);
        entry
    }

    /// Records a new cost entry with additional metadata.
    /// Returns the created cost entry.
    pub fn record_cost_with_metadata(
        &self,
        request_id: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        input_tokens: u32,
        output_tokens: u32,
        cost_nanoerg: u64,
        metadata: HashMap<String, String>,
    ) -> CostEntry {
        let mut entry = CostEntry::new(
            request_id,
            model_id,
            provider_id,
            input_tokens,
            output_tokens,
            cost_nanoerg,
        );
        entry.metadata = metadata;
        self.entries.insert(entry.id.clone(), entry.clone());
        self.total_entries.fetch_add(1, Ordering::Relaxed);
        self.total_cost_nanoerg
            .fetch_add(cost_nanoerg, Ordering::Relaxed);
        entry
    }

    // ---- Query methods ----

    /// Gets a specific cost entry by ID.
    pub fn get_entry(&self, id: &str) -> Option<CostEntry> {
        self.entries.get(id).map(|e| e.value().clone())
    }

    /// Gets all cost entries for a specific model.
    pub fn get_entries_by_model(&self, model_id: &str) -> Vec<CostEntry> {
        self.entries
            .iter()
            .filter(|e| e.value().model_id == model_id)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Gets all cost entries for a specific provider.
    pub fn get_entries_by_provider(&self, provider_id: &str) -> Vec<CostEntry> {
        self.entries
            .iter()
            .filter(|e| e.value().provider_id == provider_id)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Gets all cost entries for a specific request.
    pub fn get_entries_by_request(&self, request_id: &str) -> Vec<CostEntry> {
        self.entries
            .iter()
            .filter(|e| e.value().request_id == request_id)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Gets cost entries within a time range.
    pub fn get_entries_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Vec<CostEntry> {
        self.entries
            .iter()
            .filter(|e| {
                let ts = e.value().timestamp;
                ts >= start && ts <= end
            })
            .map(|e| e.value().clone())
            .collect()
    }

    /// Returns the total number of recorded cost entries.
    pub fn entry_count(&self) -> u64 {
        self.total_entries.load(Ordering::Relaxed)
    }

    /// Returns the total cost across all entries in nanoERG.
    pub fn total_cost(&self) -> u64 {
        self.total_cost_nanoerg.load(Ordering::Relaxed)
    }

    // ---- Aggregation ----

    /// Aggregates costs by model within a time period.
    pub fn aggregate_by_model(
        &self,
        period_start: chrono::DateTime<chrono::Utc>,
        period_end: chrono::DateTime<chrono::Utc>,
    ) -> Vec<CostAggregation> {
        let mut model_data: HashMap<String, (u64, u64, u64)> = HashMap::new();

        for entry in self.entries.iter() {
            let e = entry.value();
            if e.timestamp < period_start || e.timestamp > period_end {
                continue;
            }
            let stats = model_data
                .entry(e.model_id.clone())
                .or_insert((0, 0, 0));
            stats.0 += 1; // total_requests
            stats.1 += e.total_tokens(); // total_tokens
            stats.2 += e.cost_nanoerg; // total_cost
        }

        model_data
            .into_iter()
            .map(|(model_id, (requests, tokens, cost))| {
                let avg = if requests > 0 {
                    ((cost as f64) / (requests as f64)).ceil() as u64
                } else {
                    0
                };
                CostAggregation {
                    model_id,
                    provider_id: "all".to_string(),
                    total_requests: requests,
                    total_tokens: tokens,
                    total_cost_nanoerg: cost,
                    avg_cost_per_request: avg.max(1),
                    period_start,
                    period_end,
                }
            })
            .collect()
    }

    /// Aggregates costs by provider within a time period.
    pub fn aggregate_by_provider(
        &self,
        period_start: chrono::DateTime<chrono::Utc>,
        period_end: chrono::DateTime<chrono::Utc>,
    ) -> Vec<CostAggregation> {
        let mut provider_data: HashMap<String, (u64, u64, u64)> = HashMap::new();

        for entry in self.entries.iter() {
            let e = entry.value();
            if e.timestamp < period_start || e.timestamp > period_end {
                continue;
            }
            let stats = provider_data
                .entry(e.provider_id.clone())
                .or_insert((0, 0, 0));
            stats.0 += 1;
            stats.1 += e.total_tokens();
            stats.2 += e.cost_nanoerg;
        }

        provider_data
            .into_iter()
            .map(|(provider_id, (requests, tokens, cost))| {
                let avg = if requests > 0 {
                    ((cost as f64) / (requests as f64)).ceil() as u64
                } else {
                    0
                };
                CostAggregation {
                    model_id: "all".to_string(),
                    provider_id,
                    total_requests: requests,
                    total_tokens: tokens,
                    total_cost_nanoerg: cost,
                    avg_cost_per_request: avg.max(1),
                    period_start,
                    period_end,
                }
            })
            .collect()
    }

    // ---- Budget management ----

    /// Creates a new budget.
    /// Returns the created budget.
    pub fn create_budget(
        &self,
        name: impl Into<String>,
        limit_nanoerg: u64,
        alert_thresholds: Vec<f64>,
        period: impl Into<String>,
    ) -> CostBudget {
        let id = uuid::Uuid::new_v4().to_string();
        let budget = CostBudget::new(&id, name, limit_nanoerg, alert_thresholds, period);
        let budget = Arc::new(budget);
        self.budgets.insert(id.clone(), budget.clone());
        (*budget).clone()
    }

    /// Checks whether an additional cost would exceed the budget.
    /// Returns true if the cost can be accommodated.
    pub fn check_budget(
        &self,
        budget_id: &str,
        additional_nanoerg: u64,
    ) -> Result<bool, String> {
        let budget = self.budgets.get(budget_id).ok_or_else(|| {
            format!("Budget '{}' not found", budget_id)
        })?;
        let current = budget.value().current_spend();
        let remaining = budget.value().limit_nanoerg.saturating_sub(current);
        Ok(additional_nanoerg <= remaining)
    }

    /// Consumes budget for a given cost, triggering alerts if thresholds are crossed.
    /// Returns the remaining budget in nanoERG.
    pub fn consume_budget(
        &self,
        budget_id: &str,
        nanoerg: u64,
    ) -> Result<u64, String> {
        let budget = self.budgets.get(budget_id).ok_or_else(|| {
            format!("Budget '{}' not found", budget_id)
        })?;

        let current = budget.value().current_nanoerg.fetch_add(nanoerg, Ordering::Relaxed);
        let new_total = current + nanoerg;
        let remaining = budget.value().limit_nanoerg.saturating_sub(new_total);

        // Check alert thresholds
        let budget_val = budget.value();
        if budget_val.limit_nanoerg > 0 {
            let utilization = (new_total as f64 / budget_val.limit_nanoerg as f64) * 100.0;
            for &threshold in &budget_val.alert_thresholds {
                let prev_utilization = (current as f64 / budget_val.limit_nanoerg as f64) * 100.0;
                if prev_utilization < threshold && utilization >= threshold {
                    let alert = CostAlert::new(
                        budget_id,
                        threshold,
                        new_total,
                        budget_val.limit_nanoerg,
                    );
                    self.alerts.insert(alert.id.clone(), alert);
                }
            }
        }

        Ok(remaining)
    }

    /// Gets a specific budget by ID.
    pub fn get_budget(&self, budget_id: &str) -> Option<CostBudget> {
        self.budgets
            .get(budget_id)
            .map(|b| Arc::unwrap_or_clone(b.value().clone()))
    }

    /// Lists all budgets.
    pub fn list_budgets(&self) -> Vec<CostBudget> {
        self.budgets.iter().map(|b| Arc::unwrap_or_clone(b.value().clone())).collect()
    }

    /// Removes a budget by ID.
    pub fn remove_budget(&self, budget_id: &str) -> Result<(), String> {
        if self.budgets.remove(budget_id).is_none() {
            return Err(format!("Budget '{}' not found", budget_id));
        }
        Ok(())
    }

    /// Resets a budget's current spend to zero.
    pub fn reset_budget(&self, budget_id: &str) -> Result<(), String> {
        let budget = self.budgets.get(budget_id).ok_or_else(|| {
            format!("Budget '{}' not found", budget_id)
        })?;
        budget.value().reset();
        Ok(())
    }

    // ---- Alerts ----

    /// Gets all alerts.
    pub fn get_alerts(&self) -> Vec<CostAlert> {
        self.alerts.iter().map(|a| a.value().clone()).collect()
    }

    /// Gets only unacknowledged alerts.
    pub fn get_unacknowledged_alerts(&self) -> Vec<CostAlert> {
        self.alerts
            .iter()
            .filter(|a| !a.value().is_acknowledged())
            .map(|a| a.value().clone())
            .collect()
    }

    /// Gets alerts for a specific budget.
    pub fn get_alerts_for_budget(&self, budget_id: &str) -> Vec<CostAlert> {
        self.alerts
            .iter()
            .filter(|a| a.value().budget_id == budget_id)
            .map(|a| a.value().clone())
            .collect()
    }

    /// Acknowledges an alert by ID.
    pub fn acknowledge_alert(&self, id: &str) -> Result<(), String> {
        let alert = self.alerts.get(id).ok_or_else(|| {
            format!("Alert '{}' not found", id)
        })?;
        alert.value().acknowledged.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Clears all alerts.
    pub fn clear_alerts(&self) {
        self.alerts.clear();
    }

    /// Returns the number of alerts.
    pub fn alert_count(&self) -> usize {
        self.alerts.len()
    }

    // ---- Metrics ----

    /// Returns (total_entries, total_cost_nanoerg) metrics.
    pub fn get_metrics(&self) -> (u64, u64) {
        (
            self.total_entries.load(Ordering::Relaxed),
            self.total_cost_nanoerg.load(Ordering::Relaxed),
        )
    }
}

impl Default for InferenceCostTracker {
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

    fn make_tracker() -> InferenceCostTracker {
        InferenceCostTracker::new()
    }

    fn record_sample_cost(
        tracker: &InferenceCostTracker,
        model: &str,
        provider: &str,
        cost: u64,
    ) -> CostEntry {
        tracker.record_cost("req-1", model, provider, 100, 50, cost)
    }

    // -- Cost entry tests --

    #[test]
    fn test_record_cost_basic() {
        let tracker = make_tracker();
        let entry = tracker.record_cost("req-1", "llama-3", "provider-a", 100, 50, 1000);
        assert_eq!(entry.model_id, "llama-3");
        assert_eq!(entry.provider_id, "provider-a");
        assert_eq!(entry.input_tokens, 100);
        assert_eq!(entry.output_tokens, 50);
        assert_eq!(entry.cost_nanoerg, 1000);
        assert!(!entry.id.is_empty());
    }

    #[test]
    fn test_record_cost_with_metadata() {
        let tracker = make_tracker();
        let mut meta = HashMap::new();
        meta.insert("region".to_string(), "us-east".to_string());
        let entry = tracker.record_cost_with_metadata(
            "req-1", "model-1", "prov-1", 10, 5, 100, meta,
        );
        assert_eq!(entry.metadata.get("region").unwrap(), "us-east");
    }

    #[test]
    fn test_record_cost_increments_counters() {
        let tracker = make_tracker();
        tracker.record_cost("req-1", "m1", "p1", 10, 5, 100);
        tracker.record_cost("req-2", "m1", "p1", 20, 10, 200);
        assert_eq!(tracker.entry_count(), 2);
        assert_eq!(tracker.total_cost(), 300);
    }

    #[test]
    fn test_cost_entry_total_tokens() {
        let entry = CostEntry::new("req-1", "m1", "p1", 150, 75, 1000);
        assert_eq!(entry.total_tokens(), 225);
    }

    #[test]
    fn test_cost_entry_serialization() {
        let entry = CostEntry::new("req-1", "m1", "p1", 10, 5, 100);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("m1"));
        let deserialized: CostEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, entry.id);
    }

    // -- Query tests --

    #[test]
    fn test_get_entry() {
        let tracker = make_tracker();
        let entry = tracker.record_cost("req-1", "m1", "p1", 10, 5, 100);
        let found = tracker.get_entry(&entry.id).unwrap();
        assert_eq!(found.request_id, "req-1");
    }

    #[test]
    fn test_get_entry_not_found() {
        let tracker = make_tracker();
        assert!(tracker.get_entry("nonexistent").is_none());
    }

    #[test]
    fn test_get_entries_by_model() {
        let tracker = make_tracker();
        tracker.record_cost("r1", "model-a", "p1", 10, 5, 100);
        tracker.record_cost("r2", "model-a", "p1", 20, 10, 200);
        tracker.record_cost("r3", "model-b", "p1", 30, 15, 300);
        let entries = tracker.get_entries_by_model("model-a");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_get_entries_by_provider() {
        let tracker = make_tracker();
        tracker.record_cost("r1", "m1", "prov-x", 10, 5, 100);
        tracker.record_cost("r2", "m2", "prov-x", 20, 10, 200);
        tracker.record_cost("r3", "m1", "prov-y", 30, 15, 300);
        let entries = tracker.get_entries_by_provider("prov-x");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_get_entries_by_request() {
        let tracker = make_tracker();
        tracker.record_cost("req-abc", "m1", "p1", 10, 5, 100);
        tracker.record_cost("req-abc", "m2", "p1", 20, 10, 200);
        let entries = tracker.get_entries_by_request("req-abc");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_get_entries_in_range() {
        let tracker = make_tracker();
        let entry = tracker.record_cost("r1", "m1", "p1", 10, 5, 100);
        let start = entry.timestamp - chrono::Duration::seconds(1);
        let end = entry.timestamp + chrono::Duration::seconds(1);
        let entries = tracker.get_entries_in_range(start, end);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_get_entries_in_range_empty() {
        let tracker = make_tracker();
        tracker.record_cost("r1", "m1", "p1", 10, 5, 100);
        let start = Utc::now() - chrono::Duration::days(30);
        let end = Utc::now() - chrono::Duration::days(29);
        let entries = tracker.get_entries_in_range(start, end);
        assert!(entries.is_empty());
    }

    // -- Aggregation tests --

    #[test]
    fn test_aggregate_by_model() {
        let tracker = make_tracker();
        tracker.record_cost("r1", "model-a", "p1", 100, 50, 1000);
        tracker.record_cost("r2", "model-a", "p2", 200, 100, 2000);
        tracker.record_cost("r3", "model-b", "p1", 50, 25, 500);

        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now() + chrono::Duration::hours(1);
        let agg = tracker.aggregate_by_model(start, end);

        assert_eq!(agg.len(), 2);
        let model_a = agg.iter().find(|a| a.model_id == "model-a").unwrap();
        assert_eq!(model_a.total_requests, 2);
        assert_eq!(model_a.total_cost_nanoerg, 3000);
        assert!(model_a.avg_cost_per_request >= 1);
    }

    #[test]
    fn test_aggregate_by_provider() {
        let tracker = make_tracker();
        tracker.record_cost("r1", "m1", "prov-a", 100, 50, 1000);
        tracker.record_cost("r2", "m2", "prov-a", 200, 100, 2000);
        tracker.record_cost("r3", "m1", "prov-b", 50, 25, 500);

        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now() + chrono::Duration::hours(1);
        let agg = tracker.aggregate_by_provider(start, end);

        assert_eq!(agg.len(), 2);
        let prov_a = agg.iter().find(|a| a.provider_id == "prov-a").unwrap();
        assert_eq!(prov_a.total_requests, 2);
        assert_eq!(prov_a.total_cost_nanoerg, 3000);
    }

    #[test]
    fn test_aggregate_empty_period() {
        let tracker = make_tracker();
        tracker.record_cost("r1", "m1", "p1", 10, 5, 100);
        let start = Utc::now() - chrono::Duration::days(30);
        let end = Utc::now() - chrono::Duration::days(29);
        let agg = tracker.aggregate_by_model(start, end);
        assert!(agg.is_empty());
    }

    #[test]
    fn test_aggregation_serialization() {
        let agg = CostAggregation {
            model_id: "m1".to_string(),
            provider_id: "p1".to_string(),
            total_requests: 10,
            total_tokens: 1000,
            total_cost_nanoerg: 5000,
            avg_cost_per_request: 500,
            period_start: Utc::now(),
            period_end: Utc::now(),
        };
        let json = serde_json::to_string(&agg).unwrap();
        assert!(json.contains("m1"));
        let _: CostAggregation = serde_json::from_str(&json).unwrap();
    }

    // -- Budget tests --

    #[test]
    fn test_create_budget() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test-budget", 1_000_000, vec![50.0, 90.0], "monthly");
        assert_eq!(budget.name, "test-budget");
        assert_eq!(budget.limit_nanoerg, 1_000_000);
        assert_eq!(budget.period, "monthly");
        assert!(!budget.id.is_empty());
    }

    #[test]
    fn test_budget_default_period() {
        let budget = CostBudget::new("id", "name", 1000, vec![], "monthly");
        assert_eq!(budget.period, "monthly");
    }

    #[test]
    fn test_check_budget_ok() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![], "monthly");
        let result = tracker.check_budget(&budget.id, 500_000);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_check_budget_exceeds() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![], "monthly");
        let result = tracker.check_budget(&budget.id, 1_500_000);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_check_budget_nonexistent() {
        let tracker = make_tracker();
        let result = tracker.check_budget("nonexistent", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_consume_budget() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![], "monthly");
        let remaining = tracker.consume_budget(&budget.id, 300_000).unwrap();
        assert_eq!(remaining, 700_000);
        let b = tracker.get_budget(&budget.id).unwrap();
        assert_eq!(b.current_spend(), 300_000);
    }

    #[test]
    fn test_consume_budget_triggers_alert() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![50.0], "monthly");
        // Consume exactly 50% to trigger alert
        tracker.consume_budget(&budget.id, 500_000).unwrap();
        let alerts = tracker.get_alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].threshold_pct, 50.0);
    }

    #[test]
    fn test_consume_budget_no_alert_below_threshold() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![50.0], "monthly");
        tracker.consume_budget(&budget.id, 100_000).unwrap();
        let alerts = tracker.get_alerts();
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_consume_budget_nonexistent() {
        let tracker = make_tracker();
        let result = tracker.consume_budget("nonexistent", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_budget() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1000, vec![], "monthly");
        let found = tracker.get_budget(&budget.id).unwrap();
        assert_eq!(found.name, "test");
    }

    #[test]
    fn test_get_budget_not_found() {
        let tracker = make_tracker();
        assert!(tracker.get_budget("nonexistent").is_none());
    }

    #[test]
    fn test_list_budgets() {
        let tracker = make_tracker();
        tracker.create_budget("b1", 1000, vec![], "daily");
        tracker.create_budget("b2", 2000, vec![], "monthly");
        assert_eq!(tracker.list_budgets().len(), 2);
    }

    #[test]
    fn test_remove_budget() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1000, vec![], "monthly");
        tracker.remove_budget(&budget.id).unwrap();
        assert!(tracker.get_budget(&budget.id).is_none());
    }

    #[test]
    fn test_remove_budget_not_found() {
        let tracker = make_tracker();
        let result = tracker.remove_budget("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_budget() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![], "monthly");
        tracker.consume_budget(&budget.id, 500_000).unwrap();
        tracker.reset_budget(&budget.id).unwrap();
        let b = tracker.get_budget(&budget.id).unwrap();
        assert_eq!(b.current_spend(), 0);
    }

    #[test]
    fn test_reset_budget_not_found() {
        let tracker = make_tracker();
        let result = tracker.reset_budget("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_budget_remaining() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![], "monthly");
        tracker.consume_budget(&budget.id, 300_000).unwrap();
        let b = tracker.get_budget(&budget.id).unwrap();
        assert_eq!(b.remaining(), 700_000);
    }

    #[test]
    fn test_budget_utilization() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![], "monthly");
        tracker.consume_budget(&budget.id, 250_000).unwrap();
        let b = tracker.get_budget(&budget.id).unwrap();
        let util = b.utilization_pct();
        assert!(util > 24.0 && util < 26.0);
    }

    #[test]
    fn test_budget_is_exhausted() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000, vec![], "monthly");
        assert!(!tracker.get_budget(&budget.id).unwrap().is_exhausted());
        tracker.consume_budget(&budget.id, 1_000).unwrap();
        assert!(tracker.get_budget(&budget.id).unwrap().is_exhausted());
    }

    // -- Alert tests --

    #[test]
    fn test_acknowledge_alert() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![50.0], "monthly");
        tracker.consume_budget(&budget.id, 500_000).unwrap();
        let alerts = tracker.get_alerts();
        assert_eq!(alerts.len(), 1);
        assert!(!alerts[0].is_acknowledged());

        tracker.acknowledge_alert(&alerts[0].id).unwrap();
        let alerts = tracker.get_alerts();
        assert!(alerts[0].is_acknowledged());
    }

    #[test]
    fn test_acknowledge_alert_not_found() {
        let tracker = make_tracker();
        let result = tracker.acknowledge_alert("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_unacknowledged_alerts() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![50.0, 90.0], "monthly");
        tracker.consume_budget(&budget.id, 500_000).unwrap();
        tracker.consume_budget(&budget.id, 400_000).unwrap();
        // Both alerts should be unacknowledged
        assert_eq!(tracker.get_unacknowledged_alerts().len(), 2);

        // Acknowledge one
        let alerts = tracker.get_alerts();
        tracker.acknowledge_alert(&alerts[0].id).unwrap();
        assert_eq!(tracker.get_unacknowledged_alerts().len(), 1);
    }

    #[test]
    fn test_get_alerts_for_budget() {
        let tracker = make_tracker();
        let b1 = tracker.create_budget("b1", 1_000_000, vec![50.0], "monthly");
        let _b2 = tracker.create_budget("b2", 1_000_000, vec![50.0], "monthly");
        tracker.consume_budget(&b1.id, 500_000).unwrap();
        tracker.consume_budget(&_b2.id, 500_000).unwrap();
        let alerts_b1 = tracker.get_alerts_for_budget(&b1.id);
        assert_eq!(alerts_b1.len(), 1);
    }

    #[test]
    fn test_clear_alerts() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![50.0], "monthly");
        tracker.consume_budget(&budget.id, 500_000).unwrap();
        assert_eq!(tracker.alert_count(), 1);
        tracker.clear_alerts();
        assert_eq!(tracker.alert_count(), 0);
    }

    #[test]
    fn test_alert_serialization() {
        let alert = CostAlert::new("budget-1", 50.0, 500_000, 1_000_000);
        let json = serde_json::to_string(&alert).unwrap();
        assert!(json.contains("budget-1"));
        let _: CostAlert = serde_json::from_str(&json).unwrap();
    }

    // -- Metrics tests --

    #[test]
    fn test_metrics_initial() {
        let tracker = make_tracker();
        let (entries, cost) = tracker.get_metrics();
        assert_eq!(entries, 0);
        assert_eq!(cost, 0);
    }

    #[test]
    fn test_metrics_after_recording() {
        let tracker = make_tracker();
        tracker.record_cost("r1", "m1", "p1", 100, 50, 1000);
        tracker.record_cost("r2", "m1", "p1", 200, 100, 2000);
        let (entries, cost) = tracker.get_metrics();
        assert_eq!(entries, 2);
        assert_eq!(cost, 3000);
    }

    #[test]
    fn test_tracker_default() {
        let tracker = InferenceCostTracker::default();
        assert_eq!(tracker.entry_count(), 0);
    }

    #[test]
    fn test_budget_serialization() {
        let budget = CostBudget::new("id", "test", 1000, vec![50.0], "monthly");
        let json = serde_json::to_string(&budget).unwrap();
        assert!(json.contains("test"));
        let _: CostBudget = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_multiple_thresholds_trigger_once() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![25.0, 50.0, 75.0], "monthly");
        // Jump straight past all thresholds in one consume
        tracker.consume_budget(&budget.id, 800_000).unwrap();
        // All 3 thresholds should have triggered
        assert_eq!(tracker.alert_count(), 3);
    }

    #[test]
    fn test_consume_does_not_retrigger_same_threshold() {
        let tracker = make_tracker();
        let budget = tracker.create_budget("test", 1_000_000, vec![50.0], "monthly");
        // First consume crosses 50%
        tracker.consume_budget(&budget.id, 600_000).unwrap();
        assert_eq!(tracker.alert_count(), 1);
        // Second consume stays above 50%, should not trigger again
        tracker.consume_budget(&budget.id, 100_000).unwrap();
        assert_eq!(tracker.alert_count(), 1);
    }
}
