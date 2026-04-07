//! Ergo Cost Accounting — ERG-denominated cost tracking
//!
//! Tracks per-request, per-model, per-provider costs in nanoERG with USD
//! conversion via oracle feeds. Provides budget management with configurable
//! thresholds and alerts, multi-period aggregation, and cost categorization.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use axum::response::IntoResponse;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Cost category for tracking different spend types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CostCategory {
    #[serde(rename = "inference")]
    Inference,
    #[serde(rename = "training")]
    Training,
    #[serde(rename = "staking")]
    Staking,
}

impl std::fmt::Display for CostCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inference => write!(f, "inference"),
            Self::Training => write!(f, "training"),
            Self::Staking => write!(f, "staking"),
        }
    }
}

/// Budget reset period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetResetPeriod {
    #[serde(rename = "daily")]
    Daily,
    #[serde(rename = "weekly")]
    Weekly,
    #[serde(rename = "monthly")]
    Monthly,
}

impl Default for BudgetResetPeriod {
    fn default() -> Self {
        Self::Daily
    }
}

impl std::fmt::Display for BudgetResetPeriod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daily => write!(f, "daily"),
            Self::Weekly => write!(f, "weekly"),
            Self::Monthly => write!(f, "monthly"),
        }
    }
}

/// Aggregation period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationPeriod {
    #[serde(rename = "hourly")]
    Hourly,
    #[serde(rename = "daily")]
    Daily,
    #[serde(rename = "weekly")]
    Weekly,
    #[serde(rename = "monthly")]
    Monthly,
    #[serde(rename = "all")]
    All,
}

impl Default for AggregationPeriod {
    fn default() -> Self {
        Self::Daily
    }
}

impl std::fmt::Display for AggregationPeriod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hourly => write!(f, "hourly"),
            Self::Daily => write!(f, "daily"),
            Self::Weekly => write!(f, "weekly"),
            Self::Monthly => write!(f, "monthly"),
            Self::All => write!(f, "all"),
        }
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single cost entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    /// Unique entry identifier.
    pub entry_id: String,
    /// The request this cost belongs to.
    pub request_id: String,
    /// Provider that served the request.
    pub provider_id: String,
    /// Model used.
    pub model_id: String,
    /// Cost in nanoERG (1 ERG = 1,000,000,000 nanoERG).
    pub erg_cost_nanoerg: i64,
    /// Equivalent cost in USD (at time of recording).
    pub usd_equivalent: f64,
    /// Total tokens used (input + output).
    pub tokens_used: u64,
    /// Timestamp of the cost.
    pub timestamp: i64,
    /// Cost category.
    pub category: CostCategory,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
}

impl CostEntry {
    /// Create a new cost entry.
    pub fn new(
        request_id: impl Into<String>,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        erg_cost_nanoerg: i64,
        usd_equivalent: f64,
        tokens_used: u64,
        category: CostCategory,
    ) -> Self {
        Self {
            entry_id: uuid::Uuid::new_v4().to_string(),
            request_id: request_id.into(),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            erg_cost_nanoerg,
            usd_equivalent,
            tokens_used,
            timestamp: Utc::now().timestamp(),
            category,
            metadata: HashMap::new(),
        }
    }
}

/// Aggregated cost statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAggregationResult {
    /// Aggregation period.
    pub period: AggregationPeriod,
    /// Total cost in nanoERG.
    pub total_erg: i64,
    /// Total cost in USD.
    pub total_usd: f64,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Total number of entries.
    pub entry_count: u64,
    /// Cost breakdown by model (model_id -> nanoERG).
    pub by_model: HashMap<String, i64>,
    /// Cost breakdown by provider (provider_id -> nanoERG).
    pub by_provider: HashMap<String, i64>,
    /// Cost breakdown by category (category -> nanoERG).
    pub by_category: HashMap<String, i64>,
    /// Average cost per request in nanoERG.
    pub avg_cost_per_request: f64,
    /// Start timestamp.
    pub start_timestamp: i64,
    /// End timestamp.
    pub end_timestamp: i64,
}

/// Budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Budget limit in nanoERG.
    pub budget_limit_nanoerg: i64,
    /// Alert threshold percentage (0-100).
    pub alert_threshold_pct: u32,
    /// Reset period.
    pub reset_period: BudgetResetPeriod,
    /// Whether budget is enabled.
    pub enabled: bool,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            budget_limit_nanoerg: 1_000_000_000_000i64, // 1000 ERG
            alert_threshold_pct: 80,
            reset_period: BudgetResetPeriod::Monthly,
            enabled: false,
        }
    }
}

/// Budget status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetStatus {
    /// Budget configuration.
    pub config: BudgetConfig,
    /// Current spend in nanoERG.
    pub current_spend_nanoerg: i64,
    /// Current spend in ERG.
    pub current_spend_erg: f64,
    /// Budget utilization percentage.
    pub utilization_pct: f64,
    /// Remaining budget in nanoERG.
    pub remaining_nanoerg: i64,
    /// Whether budget is exceeded.
    pub exceeded: bool,
    /// Active (unacknowledged) alerts.
    pub active_alerts: Vec<BudgetAlert>,
}

/// A budget alert triggered when threshold is crossed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAlert {
    /// Unique alert identifier.
    pub alert_id: String,
    /// Budget utilization percentage at trigger.
    pub budget_pct: f64,
    /// Current spend at trigger time in nanoERG.
    pub current_spend: i64,
    /// Budget limit in nanoERG.
    pub limit: i64,
    /// When the alert was triggered.
    pub triggered_at: i64,
    /// Whether the alert has been acknowledged.
    pub acknowledged: bool,
}

/// Cost accounting summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    /// Total entries recorded.
    pub total_entries: u64,
    /// Total spend in nanoERG.
    pub total_erg: i64,
    /// Total spend in ERG.
    pub total_erg_float: f64,
    /// Total spend in USD.
    pub total_usd: f64,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Average cost per token in nanoERG.
    pub avg_cost_per_token: f64,
    /// Number of active budgets.
    pub active_budgets: usize,
    /// Number of unacknowledged alerts.
    pub active_alerts: usize,
}

// ---------------------------------------------------------------------------
// ErgoCostAccountant
// ---------------------------------------------------------------------------

/// ERG-denominated cost accounting service backed by DashMap.
pub struct ErgoCostAccountant {
    /// Cost entries keyed by entry_id.
    entries: DashMap<String, CostEntry>,
    /// Budget configurations keyed by provider_id (or "global").
    budgets: DashMap<String, BudgetConfig>,
    /// Budget alerts.
    alerts: DashMap<String, BudgetAlert>,
    /// Total entries counter.
    total_entries: Arc<AtomicU64>,
    /// Exchange rate (nanoERG per USD) for conversions.
    exchange_rate_nanoerg_per_usd: Arc<std::sync::atomic::AtomicI64>,
}

impl ErgoCostAccountant {
    /// Create a new cost accountant with default settings.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            budgets: DashMap::new(),
            alerts: DashMap::new(),
            total_entries: Arc::new(AtomicU64::new(0)),
            exchange_rate_nanoerg_per_usd: Arc::new(std::sync::atomic::AtomicI64::new(
                2_222_222_222, // ~0.45 ERG/USD
            )),
        }
    }

    /// Create with a specific exchange rate.
    pub fn with_exchange_rate(nanoerg_per_usd: i64) -> Self {
        let svc = Self::new();
        svc.exchange_rate_nanoerg_per_usd
            .store(nanoerg_per_usd, Ordering::Relaxed);
        svc
    }

    // ----- Cost recording -----

    /// Record a cost entry.
    pub fn record_cost(&self, mut entry: CostEntry) -> CostEntry {
        // If no USD equivalent provided, compute from ERG
        if entry.usd_equivalent == 0.0 && entry.erg_cost_nanoerg > 0 {
            entry.usd_equivalent = self.convert_erg_to_usd(entry.erg_cost_nanoerg);
        }

        let entry_id = entry.entry_id.clone();
        self.entries.insert(entry_id.clone(), entry.clone());
        self.total_entries.fetch_add(1, Ordering::Relaxed);

        // Check all budgets
        self.check_all_budgets(&entry);

        debug!(
            entry_id = %entry_id,
            erg = entry.erg_cost_nanoerg,
            usd = entry.usd_equivalent,
            category = %entry.category,
            "Cost entry recorded"
        );

        entry
    }

    /// Record a cost with minimal parameters.
    pub fn record_cost_simple(
        &self,
        request_id: impl Into<String>,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        erg_cost_nanoerg: i64,
        tokens_used: u64,
        category: CostCategory,
    ) -> CostEntry {
        let entry = CostEntry::new(
            request_id,
            provider_id,
            model_id,
            erg_cost_nanoerg,
            0.0, // Will be auto-computed
            tokens_used,
            category,
        );
        self.record_cost(entry)
    }

    // ----- Querying -----

    /// Get a specific cost entry by ID.
    pub fn get_entry(&self, entry_id: &str) -> Option<CostEntry> {
        self.entries.get(entry_id).map(|r| r.value().clone())
    }

    /// Get cost entries with optional filtering.
    pub fn get_costs(
        &self,
        provider_id: Option<&str>,
        model_id: Option<&str>,
        category: Option<CostCategory>,
        limit: usize,
    ) -> Vec<CostEntry> {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| {
                let e = entry.value();
                if let Some(pid) = provider_id {
                    if e.provider_id != pid {
                        return false;
                    }
                }
                if let Some(mid) = model_id {
                    if e.model_id != mid {
                        return false;
                    }
                }
                if let Some(cat) = category {
                    if e.category != cat {
                        return false;
                    }
                }
                true
            })
            .map(|r| r.value().clone())
            .collect();

        // Sort by timestamp descending
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(limit);
        entries
    }

    /// Get all cost entries.
    pub fn get_all_entries(&self) -> Vec<CostEntry> {
        self.entries.iter().map(|r| r.value().clone()).collect()
    }

    /// Aggregate costs for a given period.
    pub fn aggregate(&self, period: AggregationPeriod) -> CostAggregationResult {
        let now = Utc::now().timestamp();
        let cutoff = match period {
            AggregationPeriod::Hourly => now - 3600,
            AggregationPeriod::Daily => now - 86400,
            AggregationPeriod::Weekly => now - 86400 * 7,
            AggregationPeriod::Monthly => now - 86400 * 30,
            AggregationPeriod::All => 0,
        };

        let mut total_erg: i64 = 0;
        let mut total_usd: f64 = 0.0;
        let mut total_tokens: u64 = 0;
        let mut entry_count: u64 = 0;
        let mut by_model: HashMap<String, i64> = HashMap::new();
        let mut by_provider: HashMap<String, i64> = HashMap::new();
        let mut by_category: HashMap<String, i64> = HashMap::new();
        let mut start_ts = i64::MAX;

        for entry in self.entries.iter() {
            let e = entry.value();
            if e.timestamp < cutoff {
                continue;
            }

            total_erg += e.erg_cost_nanoerg;
            total_usd += e.usd_equivalent;
            total_tokens += e.tokens_used;
            entry_count += 1;

            *by_model.entry(e.model_id.clone()).or_insert(0) += e.erg_cost_nanoerg;
            *by_provider.entry(e.provider_id.clone()).or_insert(0) += e.erg_cost_nanoerg;
            *by_category.entry(e.category.to_string()).or_insert(0) += e.erg_cost_nanoerg;

            if e.timestamp < start_ts {
                start_ts = e.timestamp;
            }
        }

        if entry_count == 0 {
            start_ts = now;
        }

        let avg_cost = if entry_count > 0 {
            total_erg as f64 / entry_count as f64
        } else {
            0.0
        };

        CostAggregationResult {
            period,
            total_erg,
            total_usd,
            total_tokens,
            entry_count,
            by_model,
            by_provider,
            by_category,
            avg_cost_per_request: avg_cost,
            start_timestamp: start_ts,
            end_timestamp: now,
        }
    }

    // ----- Budget management -----

    /// Set a budget for a provider (or "global").
    pub fn set_budget(&self, scope: impl Into<String>, config: BudgetConfig) {
        let scope_key = scope.into();
        let limit_nanoerg = config.budget_limit_nanoerg;
        self.budgets.insert(scope_key.clone(), config);
        info!(
            scope = %scope_key,
            limit_nanoerg = limit_nanoerg,
            "Budget configured"
        );
    }

    /// Get budget status for a scope.
    pub fn get_budget_status(&self, scope: &str) -> Option<BudgetStatus> {
        let config = self.budgets.get(scope)?;
        let cfg = config.value().clone();
        if !cfg.enabled {
            return Some(BudgetStatus {
                config: cfg.clone(),
                current_spend_nanoerg: 0,
                current_spend_erg: 0.0,
                utilization_pct: 0.0,
                remaining_nanoerg: cfg.budget_limit_nanoerg,
                exceeded: false,
                active_alerts: vec![],
            });
        }

        let current = self.get_current_spend(scope);
        let utilization = if cfg.budget_limit_nanoerg > 0 {
            (current as f64 / cfg.budget_limit_nanoerg as f64) * 100.0
        } else {
            0.0
        };
        let remaining = (cfg.budget_limit_nanoerg - current).max(0);

        let active_alerts: Vec<BudgetAlert> = self
            .alerts
            .iter()
            .filter(|a| !a.value().acknowledged)
            .map(|a| a.value().clone())
            .collect();

        Some(BudgetStatus {
            config: cfg.clone(),
            current_spend_nanoerg: current,
            current_spend_erg: current as f64 / 1_000_000_000.0,
            utilization_pct: utilization,
            remaining_nanoerg: remaining,
            exceeded: current >= cfg.budget_limit_nanoerg,
            active_alerts,
        })
    }

    /// Get all budget statuses.
    pub fn get_all_budget_statuses(&self) -> Vec<BudgetStatus> {
        self.budgets
            .iter()
            .filter_map(|r| self.get_budget_status(r.key()))
            .collect()
    }

    /// Get alerts (optionally filtered by acknowledgment status).
    pub fn get_alerts(&self, include_acknowledged: bool) -> Vec<BudgetAlert> {
        self.alerts
            .iter()
            .filter(|a| include_acknowledged || !a.value().acknowledged)
            .map(|a| a.value().clone())
            .collect()
    }

    /// Acknowledge a budget alert.
    pub fn acknowledge_alert(&self, alert_id: &str) -> bool {
        if let Some(mut alert) = self.alerts.get_mut(alert_id) {
            alert.acknowledged = true;
            info!(alert_id = %alert_id, "Budget alert acknowledged");
            true
        } else {
            false
        }
    }

    /// Get overall cost summary.
    pub fn get_summary(&self) -> CostSummary {
        let total_entries = self.total_entries.load(Ordering::Relaxed);
        let mut total_erg: i64 = 0;
        let mut total_usd: f64 = 0.0;
        let mut total_tokens: u64 = 0;

        for entry in self.entries.iter() {
            total_erg += entry.value().erg_cost_nanoerg;
            total_usd += entry.value().usd_equivalent;
            total_tokens += entry.value().tokens_used;
        }

        let avg_per_token = if total_tokens > 0 {
            total_erg as f64 / total_tokens as f64
        } else {
            0.0
        };

        let active_alerts = self
            .alerts
            .iter()
            .filter(|a| !a.value().acknowledged)
            .count();

        CostSummary {
            total_entries,
            total_erg,
            total_erg_float: total_erg as f64 / 1_000_000_000.0,
            total_usd,
            total_tokens,
            avg_cost_per_token: avg_per_token,
            active_budgets: self.budgets.len(),
            active_alerts,
        }
    }

    // ----- Currency conversion -----

    /// Convert nanoERG to USD using the current exchange rate.
    pub fn convert_erg_to_usd(&self, nanoerg: i64) -> f64 {
        let rate = self.exchange_rate_nanoerg_per_usd.load(Ordering::Relaxed);
        if rate == 0 {
            return 0.0;
        }
        nanoerg as f64 / rate as f64
    }

    /// Convert USD to nanoERG using the current exchange rate.
    pub fn convert_usd_to_erg(&self, usd: f64) -> i64 {
        let rate = self.exchange_rate_nanoerg_per_usd.load(Ordering::Relaxed);
        (usd * rate as f64) as i64
    }

    /// Update the exchange rate (nanoERG per USD).
    pub fn set_exchange_rate(&self, nanoerg_per_usd: i64) {
        self.exchange_rate_nanoerg_per_usd
            .store(nanoerg_per_usd, Ordering::Relaxed);
        debug!(rate = nanoerg_per_usd, "Exchange rate updated");
    }

    /// Get the current exchange rate.
    pub fn get_exchange_rate(&self) -> i64 {
        self.exchange_rate_nanoerg_per_usd.load(Ordering::Relaxed)
    }

    /// Clear all entries (for testing / reset).
    pub fn clear(&self) {
        self.entries.clear();
        self.alerts.clear();
        self.total_entries.store(0, Ordering::Relaxed);
    }

    /// Remove a budget.
    pub fn remove_budget(&self, scope: &str) -> bool {
        self.budgets.remove(scope).is_some()
    }

    // ----- Internal -----

    fn get_current_spend(&self, scope: &str) -> i64 {
        if scope == "global" {
            self.entries
                .iter()
                .map(|e| e.value().erg_cost_nanoerg)
                .sum()
        } else {
            self.entries
                .iter()
                .filter(|e| e.value().provider_id == scope)
                .map(|e| e.value().erg_cost_nanoerg)
                .sum()
        }
    }

    fn check_all_budgets(&self, entry: &CostEntry) {
        for budget in self.budgets.iter() {
            let cfg = budget.value();
            if !cfg.enabled {
                continue;
            }

            let scope = budget.key();
            let is_global = scope == "global";
            let matches_scope = is_global || entry.provider_id == *scope;
            if !matches_scope {
                continue;
            }

            let current = self.get_current_spend(scope);
            let utilization = if cfg.budget_limit_nanoerg > 0 {
                (current as f64 / cfg.budget_limit_nanoerg as f64) * 100.0
            } else {
                0.0
            };

            if utilization >= cfg.alert_threshold_pct as f64 {
                // Check if we already have a recent alert for this scope/threshold
                let has_recent = self
                    .alerts
                    .iter()
                    .any(|a| {
                        let alert = a.value();
                        !alert.acknowledged
                            && (alert.limit == cfg.budget_limit_nanoerg)
                            && (Utc::now().timestamp() - alert.triggered_at) < 300
                    });

                if !has_recent {
                    let alert = BudgetAlert {
                        alert_id: uuid::Uuid::new_v4().to_string(),
                        budget_pct: utilization,
                        current_spend: current,
                        limit: cfg.budget_limit_nanoerg,
                        triggered_at: Utc::now().timestamp(),
                        acknowledged: false,
                    };
                    self.alerts.insert(alert.alert_id.clone(), alert);
                    warn!(
                        scope = %scope,
                        utilization = utilization,
                        threshold = cfg.alert_threshold_pct,
                        "Budget alert triggered"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// REST API router builder
// ---------------------------------------------------------------------------

/// Build the cost accounting router.
pub fn build_cost_accounting_router(state: crate::api::AppState) -> axum::Router<()> {
    use axum::routing::{get, post, put};

    axum::Router::new()
        .route("/v1/costing/record", post(cost_record_handler))
        .route("/v1/costing/entries", get(cost_entries_handler))
        .route("/v1/costing/aggregation", get(cost_aggregation_handler))
        .route("/v1/costing/budget", post(cost_budget_handler))
        .route("/v1/costing/budget/status", get(cost_budget_status_handler))
        .route("/v1/costing/alerts", get(cost_alerts_handler))
        .route("/v1/costing/alerts/{id}/ack", put(cost_alert_ack_handler))
        .with_state(state)
}

// ----- Request/Response types -----

#[derive(Debug, Deserialize)]
struct RecordCostRequest {
    pub request_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub erg_cost_nanoerg: i64,
    #[serde(default)]
    pub usd_equivalent: f64,
    #[serde(default)]
    pub tokens_used: u64,
    pub category: CostCategory,
}

#[derive(Debug, Deserialize)]
struct SetBudgetRequest {
    pub scope: String,
    pub budget_limit_nanoerg: i64,
    #[serde(default = "default_threshold")]
    pub alert_threshold_pct: u32,
    #[serde(default)]
    pub reset_period: BudgetResetPeriod,
    #[serde(default)]
    pub enabled: bool,
}

fn default_threshold() -> u32 {
    80
}

#[derive(Debug, Deserialize)]
struct EntriesQuery {
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub category: Option<CostCategory>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Debug, Deserialize)]
struct AggregationQuery {
    #[serde(default)]
    pub period: AggregationPeriod,
}

// ----- Handlers -----

async fn cost_record_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::Json(req): axum::Json<RecordCostRequest>,
) -> axum::Json<serde_json::Value> {
    let entry = CostEntry::new(
        req.request_id,
        req.provider_id,
        req.model_id,
        req.erg_cost_nanoerg,
        req.usd_equivalent,
        req.tokens_used,
        req.category,
    );

    let recorded = state.cost_accountant.record_cost(entry);
    axum::Json(serde_json::json!({
        "ok": true,
        "entry_id": recorded.entry_id,
        "erg_cost_nanoerg": recorded.erg_cost_nanoerg,
        "usd_equivalent": recorded.usd_equivalent
    }))
}

async fn cost_entries_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Query(query): axum::extract::Query<EntriesQuery>,
) -> axum::Json<serde_json::Value> {
    let entries = state.cost_accountant.get_costs(
        query.provider_id.as_deref(),
        query.model_id.as_deref(),
        query.category,
        query.limit,
    );

    axum::Json(serde_json::json!({
        "entries": entries,
        "count": entries.len()
    }))
}

async fn cost_aggregation_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Query(query): axum::extract::Query<AggregationQuery>,
) -> axum::Json<serde_json::Value> {
    let agg = state.cost_accountant.aggregate(query.period);
    axum::Json(serde_json::json!(agg))
}

async fn cost_budget_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::Json(req): axum::Json<SetBudgetRequest>,
) -> axum::Json<serde_json::Value> {
    let config = BudgetConfig {
        budget_limit_nanoerg: req.budget_limit_nanoerg,
        alert_threshold_pct: req.alert_threshold_pct,
        reset_period: req.reset_period,
        enabled: req.enabled,
    };

    state.cost_accountant.set_budget(&req.scope, config);
    axum::Json(serde_json::json!({
        "ok": true,
        "scope": req.scope
    }))
}

async fn cost_budget_status_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let statuses = state.cost_accountant.get_all_budget_statuses();
    axum::Json(serde_json::json!({
        "budgets": statuses,
        "count": statuses.len()
    }))
}

async fn cost_alerts_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let alerts = state.cost_accountant.get_alerts(false);
    axum::Json(serde_json::json!({
        "alerts": alerts,
        "count": alerts.len()
    }))
}

async fn cost_alert_ack_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Path(alert_id): axum::extract::Path<String>,
) -> axum::response::Response {
    if state.cost_accountant.acknowledge_alert(&alert_id) {
        axum::Json(serde_json::json!({
            "ok": true,
            "alert_id": alert_id
        }))
        .into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": "Alert not found",
                "alert_id": alert_id
            })),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_accountant() -> ErgoCostAccountant {
        ErgoCostAccountant::with_exchange_rate(2_222_222_222) // ~0.45 ERG/USD
    }

    #[test]
    fn test_new_accountant() {
        let acct = make_accountant();
        let summary = acct.get_summary();
        assert_eq!(summary.total_entries, 0);
        assert_eq!(summary.total_erg, 0);
    }

    #[test]
    fn test_record_cost() {
        let acct = make_accountant();
        let entry = acct.record_cost_simple(
            "req-1",
            "provider-1",
            "model-1",
            1_000_000_000, // 1 ERG
            1000,
            CostCategory::Inference,
        );

        assert!(!entry.entry_id.is_empty());
        assert_eq!(entry.erg_cost_nanoerg, 1_000_000_000);
        assert!(entry.usd_equivalent > 0.0);
    }

    #[test]
    fn test_convert_erg_to_usd() {
        let acct = make_accountant();
        let usd = acct.convert_erg_to_usd(2_222_222_222); // 2.222... ERG
        assert!((usd - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_convert_usd_to_erg() {
        let acct = make_accountant();
        let nanoerg = acct.convert_usd_to_erg(1.0);
        assert_eq!(nanoerg, 2_222_222_222);
    }

    #[test]
    fn test_get_costs_filtered() {
        let acct = make_accountant();
        acct.record_cost_simple("r1", "p1", "m1", 100, 100, CostCategory::Inference);
        acct.record_cost_simple("r2", "p2", "m2", 200, 200, CostCategory::Training);
        acct.record_cost_simple("r3", "p1", "m1", 300, 300, CostCategory::Inference);

        let p1_entries = acct.get_costs(Some("p1"), None, None, 100);
        assert_eq!(p1_entries.len(), 2);

        let inf_entries = acct.get_costs(None, None, Some(CostCategory::Inference), 100);
        assert_eq!(inf_entries.len(), 2);
    }

    #[test]
    fn test_aggregate_daily() {
        let acct = make_accountant();
        acct.record_cost_simple("r1", "p1", "m1", 1_000_000_000, 100, CostCategory::Inference);
        acct.record_cost_simple("r2", "p2", "m2", 500_000_000, 50, CostCategory::Training);

        let agg = acct.aggregate(AggregationPeriod::Daily);
        assert_eq!(agg.entry_count, 2);
        assert_eq!(agg.total_erg, 1_500_000_000);
        assert_eq!(agg.total_tokens, 150);
        assert!(agg.by_model.contains_key("m1"));
        assert!(agg.by_category.contains_key("inference"));
    }

    #[test]
    fn test_set_and_check_budget() {
        let acct = make_accountant();
        acct.set_budget(
            "global",
            BudgetConfig {
                budget_limit_nanoerg: 1_000_000_000,
                alert_threshold_pct: 50,
                reset_period: BudgetResetPeriod::Daily,
                enabled: true,
            },
        );

        // Record costs that exceed 50% threshold
        acct.record_cost_simple("r1", "p1", "m1", 600_000_000, 100, CostCategory::Inference);

        let status = acct.get_budget_status("global").unwrap();
        assert!(status.current_spend_nanoerg > 0);
        assert!(status.utilization_pct > 50.0);
    }

    #[test]
    fn test_alerts_triggered() {
        let acct = make_accountant();
        acct.set_budget(
            "global",
            BudgetConfig {
                budget_limit_nanoerg: 1_000_000_000,
                alert_threshold_pct: 50,
                reset_period: BudgetResetPeriod::Daily,
                enabled: true,
            },
        );

        acct.record_cost_simple("r1", "p1", "m1", 600_000_000, 100, CostCategory::Inference);

        let alerts = acct.get_alerts(false);
        assert!(alerts.len() >= 1);
    }

    #[test]
    fn test_acknowledge_alert() {
        let acct = make_accountant();
        acct.set_budget(
            "global",
            BudgetConfig {
                budget_limit_nanoerg: 1_000_000_000,
                alert_threshold_pct: 50,
                reset_period: BudgetResetPeriod::Daily,
                enabled: true,
            },
        );

        acct.record_cost_simple("r1", "p1", "m1", 600_000_000, 100, CostCategory::Inference);

        let alerts = acct.get_alerts(false);
        assert!(!alerts.is_empty());
        let alert_id = alerts[0].alert_id.clone();

        assert!(acct.acknowledge_alert(&alert_id));
        assert_eq!(acct.get_alerts(false).len(), 0);
    }

    #[test]
    fn test_get_summary() {
        let acct = make_accountant();
        acct.record_cost_simple("r1", "p1", "m1", 1_000_000_000, 1000, CostCategory::Inference);

        let summary = acct.get_summary();
        assert_eq!(summary.total_entries, 1);
        assert_eq!(summary.total_erg, 1_000_000_000);
        assert!(summary.total_usd > 0.0);
        assert_eq!(summary.total_tokens, 1000);
    }

    #[test]
    fn test_update_exchange_rate() {
        let acct = make_accountant();
        acct.set_exchange_rate(1_000_000_000); // 1 ERG = 1 USD

        let usd = acct.convert_erg_to_usd(1_000_000_000);
        assert!((usd - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_remove_budget() {
        let acct = make_accountant();
        acct.set_budget(
            "test",
            BudgetConfig {
                budget_limit_nanoerg: 100,
                alert_threshold_pct: 80,
                reset_period: BudgetResetPeriod::Daily,
                enabled: true,
            },
        );

        assert!(acct.remove_budget("test"));
        assert!(!acct.remove_budget("nonexistent"));
    }

    #[test]
    fn test_clear() {
        let acct = make_accountant();
        acct.record_cost_simple("r1", "p1", "m1", 100, 10, CostCategory::Inference);

        acct.clear();
        assert_eq!(acct.get_summary().total_entries, 0);
    }
}
