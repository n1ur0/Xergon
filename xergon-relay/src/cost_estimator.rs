//! Inference Cost Estimator.
//!
//! Estimates per-request and per-session inference costs based on model parameters,
//! token counts, and provider pricing. Supports cost budgets, cost caps, and cost
//! forecasting.
//!
//! Endpoints:
//!   POST /api/cost/estimate       -- estimate cost for a request
//!   POST /api/cost/budget        -- create a cost budget
//!   GET  /api/cost/budget/:id    -- get budget status
//!   GET  /api/cost/budgets       -- list all budgets
//!   GET  /api/cost/metrics       -- get cost metrics snapshot
//!   GET  /api/cost/forecast      -- get cost forecast
//!   GET  /api/cost/tiers         -- list pricing tiers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Cost category for filtering and reporting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CostCategory {
    Input,
    Output,
    Total,
}

/// A pricing tier definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    pub name: String,
    pub input_price_per_1k: f64,
    pub output_price_per_1k: f64,
    #[serde(default)]
    pub min_tokens: u32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_priority_boost")]
    pub priority_boost: f64,
}

fn default_max_tokens() -> u32 {
    1_000_000
}

fn default_priority_boost() -> f64 {
    1.0
}

/// Result of a cost estimation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub model_id: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub input_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
    #[serde(default = "default_currency")]
    pub currency: String,
    pub tier_used: String,
    pub timestamp: DateTime<Utc>,
}

fn default_currency() -> String {
    "ERG".to_string()
}

/// A cost budget with alert threshold.
#[derive(Debug, Serialize, Deserialize)]
pub struct CostBudget {
    pub budget_id: String,
    pub max_cost: f64,
    #[serde(skip)]
    pub current_cost_nanoerg: AtomicU64,
    pub alert_threshold: f64,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl CostBudget {
    /// Get current cost in ERG (nanoerg stored as u64).
    pub fn current_cost(&self) -> f64 {
        self.current_cost_nanoerg.load(Ordering::Relaxed) as f64 / 1_000_000_000.0
    }

    /// Check if budget is expired.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => Utc::now() > exp,
            None => false,
        }
    }
}

// Manual Clone implementation because AtomicU64 doesn't impl Clone
impl Clone for CostBudget {
    fn clone(&self) -> Self {
        Self {
            budget_id: self.budget_id.clone(),
            max_cost: self.max_cost,
            current_cost_nanoerg: AtomicU64::new(self.current_cost_nanoerg.load(Ordering::Relaxed)),
            alert_threshold: self.alert_threshold,
            created_at: self.created_at,
            expires_at: self.expires_at,
        }
    }
}

/// Cost forecast result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostForecast {
    pub period: String,
    pub estimated_cost: f64,
    pub estimated_tokens: u64,
    pub confidence: f64,
}

/// Aggregated cost metrics snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct CostMetricsSnapshot {
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub avg_cost_per_request: f64,
    pub avg_cost_per_1k_tokens: f64,
    pub budgets_active: u64,
    pub budgets_exhausted: u64,
}

// ---------------------------------------------------------------------------
// Request / Response types for HTTP handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EstimateRequest {
    pub model_id: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub tier: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BudgetRequest {
    pub budget_id: String,
    pub max_cost: f64,
    #[serde(default = "default_alert_threshold")]
    pub alert_threshold: f64,
    pub expires_at: Option<DateTime<Utc>>,
}

fn default_alert_threshold() -> f64 {
    0.8
}

#[derive(Debug, Deserialize)]
pub struct ForecastQuery {
    #[serde(default = "default_forecast_hours")]
    pub hours: u64,
}

fn default_forecast_hours() -> u64 {
    24
}

// ---------------------------------------------------------------------------
// InferenceCostEstimator
// ---------------------------------------------------------------------------

/// Core cost estimator engine.
pub struct InferenceCostEstimator {
    tiers: DashMap<String, PricingTier>,
    budgets: DashMap<String, Arc<CostBudget>>,
    model_tier_map: DashMap<String, String>,
    total_requests: AtomicU64,
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    total_cost_nanoerg: AtomicU64,
}

impl InferenceCostEstimator {
    /// Create a new estimator with default pricing tiers.
    pub fn new() -> Self {
        let estimator = Self {
            tiers: DashMap::new(),
            budgets: DashMap::new(),
            model_tier_map: DashMap::new(),
            total_requests: AtomicU64::new(0),
            total_input_tokens: AtomicU64::new(0),
            total_output_tokens: AtomicU64::new(0),
            total_cost_nanoerg: AtomicU64::new(0),
        };

        // Pre-populate default tiers
        estimator.tiers.insert(
            "standard".to_string(),
            PricingTier {
                name: "standard".to_string(),
                input_price_per_1k: 0.0005,
                output_price_per_1k: 0.0015,
                min_tokens: 0,
                max_tokens: 1_000_000,
                priority_boost: 1.0,
            },
        );
        estimator.tiers.insert(
            "premium".to_string(),
            PricingTier {
                name: "premium".to_string(),
                input_price_per_1k: 0.001,
                output_price_per_1k: 0.003,
                min_tokens: 0,
                max_tokens: 1_000_000,
                priority_boost: 1.5,
            },
        );
        estimator.tiers.insert(
            "budget".to_string(),
            PricingTier {
                name: "budget".to_string(),
                input_price_per_1k: 0.0002,
                output_price_per_1k: 0.0006,
                min_tokens: 0,
                max_tokens: 500_000,
                priority_boost: 0.7,
            },
        );

        estimator
    }

    /// Estimate cost using a specific tier.
    pub fn estimate(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        tier_name: &str,
    ) -> Result<CostEstimate, String> {
        let tier = self
            .tiers
            .get(tier_name)
            .ok_or_else(|| format!("Tier '{}' not found", tier_name))?
            .clone();

        self.estimate_with_tier(model_id, input_tokens, output_tokens, &tier)
    }

    /// Internal estimation using a resolved tier.
    fn estimate_with_tier(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        tier: &PricingTier,
    ) -> Result<CostEstimate, String> {
        let total_tokens = input_tokens.saturating_add(output_tokens);
        if total_tokens < tier.min_tokens {
            return Err(format!(
                "Token count {} below minimum {} for tier '{}'",
                total_tokens, tier.min_tokens, tier.name
            ));
        }
        if total_tokens > tier.max_tokens {
            return Err(format!(
                "Token count {} exceeds maximum {} for tier '{}'",
                total_tokens, tier.max_tokens, tier.name
            ));
        }

        let input_cost = (input_tokens as f64 / 1000.0) * tier.input_price_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * tier.output_price_per_1k;
        let total_cost = input_cost + output_cost;

        // Record metrics
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_input_tokens.fetch_add(input_tokens as u64, Ordering::Relaxed);
        self.total_output_tokens.fetch_add(output_tokens as u64, Ordering::Relaxed);
        let cost_nanoerg = ((total_cost * 1_000_000_000.0).ceil() as u64).max(0);
        self.total_cost_nanoerg.fetch_add(cost_nanoerg, Ordering::Relaxed);

        Ok(CostEstimate {
            model_id: model_id.to_string(),
            input_tokens,
            output_tokens,
            input_cost,
            output_cost,
            total_cost,
            currency: "ERG".to_string(),
            tier_used: tier.name.clone(),
            timestamp: Utc::now(),
        })
    }

    /// Auto-select the best tier and estimate cost.
    pub fn estimate_with_best_tier(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> CostEstimate {
        // Check if model has a specific tier assigned
        if let Some(tier_name) = self.model_tier_map.get(model_id) {
            if let Ok(est) = self.estimate(model_id, input_tokens, output_tokens, &tier_name) {
                return est;
            }
        }

        // Try to find the cheapest valid tier
        let mut best_estimate: Option<CostEstimate> = None;
        let mut best_cost = f64::MAX;

        for tier_entry in self.tiers.iter() {
            let tier = tier_entry.value();
            let total_tokens = input_tokens.saturating_add(output_tokens);

            // Skip tiers where token count is out of range
            if total_tokens < tier.min_tokens || total_tokens > tier.max_tokens {
                continue;
            }

            let input_cost = (input_tokens as f64 / 1000.0) * tier.input_price_per_1k;
            let output_cost = (output_tokens as f64 / 1000.0) * tier.output_price_per_1k;
            let total_cost = input_cost + output_cost;

            if total_cost < best_cost {
                best_cost = total_cost;
                best_estimate = Some(CostEstimate {
                    model_id: model_id.to_string(),
                    input_tokens,
                    output_tokens,
                    input_cost,
                    output_cost,
                    total_cost,
                    currency: "ERG".to_string(),
                    tier_used: tier.name.clone(),
                    timestamp: Utc::now(),
                });
            }
        }

        // Record metrics (only if we found a valid tier)
        if best_estimate.is_some() {
            self.total_requests.fetch_add(1, Ordering::Relaxed);
            self.total_input_tokens
                .fetch_add(input_tokens as u64, Ordering::Relaxed);
            self.total_output_tokens
                .fetch_add(output_tokens as u64, Ordering::Relaxed);
            let cost_nanoerg = ((best_cost * 1_000_000_000.0).ceil() as u64).max(0);
            self.total_cost_nanoerg.fetch_add(cost_nanoerg, Ordering::Relaxed);
        }

        best_estimate.unwrap_or_else(|| CostEstimate {
            model_id: model_id.to_string(),
            input_tokens,
            output_tokens,
            input_cost: 0.0,
            output_cost: 0.0,
            total_cost: 0.0,
            currency: "ERG".to_string(),
            tier_used: "none".to_string(),
            timestamp: Utc::now(),
        })
    }

    /// Create a new cost budget.
    pub fn create_budget(
        &self,
        id: &str,
        max_cost: f64,
        alert_threshold: f64,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<CostBudget, String> {
        if max_cost <= 0.0 {
            return Err("max_cost must be positive".to_string());
        }
        if alert_threshold <= 0.0 || alert_threshold > 1.0 {
            return Err("alert_threshold must be between 0.0 and 1.0".to_string());
        }
        if self.budgets.contains_key(id) {
            return Err(format!("Budget '{}' already exists", id));
        }

        let budget = CostBudget {
            budget_id: id.to_string(),
            max_cost,
            current_cost_nanoerg: AtomicU64::new(0),
            alert_threshold,
            created_at: Utc::now(),
            expires_at,
        };

        self.budgets.insert(id.to_string(), Arc::new(budget));
        Ok(self.get_budget(id).unwrap())
    }

    /// Get a budget by ID.
    pub fn get_budget(&self, id: &str) -> Option<CostBudget> {
        self.budgets.get(id).map(|b| (**b.value()).clone())
    }

    /// Check if additional cost would fit within budget.
    pub fn check_budget(&self, id: &str, additional_cost: f64) -> Result<bool, String> {
        let budget = self.budgets.get(id).ok_or_else(|| {
            format!("Budget '{}' not found", id)
        })?;

        let budget_ref = budget.value();
        if budget_ref.is_expired() {
            return Err(format!("Budget '{}' is expired", id));
        }

        let current = budget_ref.current_cost();
        Ok(current + additional_cost <= budget_ref.max_cost)
    }

    /// Consume cost from a budget, returns remaining balance.
    pub fn consume_budget(&self, id: &str, cost: f64) -> Result<f64, String> {
        let budget = self.budgets.get(id).ok_or_else(|| {
            format!("Budget '{}' not found", id)
        })?;

        let budget_ref = budget.value();
        if budget_ref.is_expired() {
            return Err(format!("Budget '{}' is expired", id));
        }

        let cost_nanoerg = ((cost * 1_000_000_000.0).ceil() as u64).max(1);
        let current = budget_ref.current_cost_nanoerg.fetch_add(cost_nanoerg, Ordering::Relaxed);
        let new_current = current + cost_nanoerg;
        let max_nanoerg = ((budget_ref.max_cost * 1_000_000_000.0).ceil() as u64).max(1);

        if new_current > max_nanoerg {
            // Rollback
            budget_ref.current_cost_nanoerg.fetch_sub(cost_nanoerg, Ordering::Relaxed);
            return Err(format!(
                "Budget '{}' exceeded: current {} ERG, max {} ERG",
                id,
                new_current as f64 / 1_000_000_000.0,
                budget_ref.max_cost
            ));
        }

        let remaining = ((max_nanoerg.saturating_sub(new_current)) as f64 / 1_000_000_000.0).max(0.0);
        Ok(remaining)
    }

    /// List all budgets.
    pub fn list_budgets(&self) -> Vec<CostBudget> {
        self.budgets.iter().map(|b| (**b.value()).clone()).collect()
    }

    /// Simple linear cost forecast based on recent usage.
    pub fn forecast(&self, hours: u64) -> CostForecast {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let total_input_tokens = self.total_input_tokens.load(Ordering::Relaxed);
        let total_output_tokens = self.total_output_tokens.load(Ordering::Relaxed);
        let total_cost_nanoerg = self.total_cost_nanoerg.load(Ordering::Relaxed);

        // Assume metrics represent 1 hour of usage for extrapolation
        let cost_per_hour = if total_requests > 0 {
            total_cost_nanoerg as f64 / 1_000_000_000.0
        } else {
            0.0
        };

        let tokens_per_hour = total_input_tokens.saturating_add(total_output_tokens);
        let estimated_cost = cost_per_hour * hours as f64;
        let estimated_tokens = tokens_per_hour.saturating_mul(hours);

        // Confidence decreases with forecast distance
        let confidence = (1.0 / (1.0 + (hours as f64 / 24.0))).max(0.1);

        CostForecast {
            period: format!("{}h", hours),
            estimated_cost,
            estimated_tokens,
            confidence,
        }
    }

    /// Get aggregated metrics snapshot.
    pub fn get_metrics(&self) -> CostMetricsSnapshot {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let total_input_tokens = self.total_input_tokens.load(Ordering::Relaxed);
        let total_output_tokens = self.total_output_tokens.load(Ordering::Relaxed);
        let total_tokens = total_input_tokens.saturating_add(total_output_tokens);
        let total_cost_nanoerg = self.total_cost_nanoerg.load(Ordering::Relaxed);
        let total_cost = total_cost_nanoerg as f64 / 1_000_000_000.0;

        let avg_cost_per_request = if total_requests > 0 {
            total_cost / total_requests as f64
        } else {
            0.0
        };

        let avg_cost_per_1k_tokens = if total_tokens > 0 {
            (total_cost / total_tokens as f64) * 1000.0
        } else {
            0.0
        };

        let mut budgets_active: u64 = 0;
        let mut budgets_exhausted: u64 = 0;
        for budget in self.budgets.iter() {
            let b = budget.value();
            if b.is_expired() {
                continue;
            }
            if b.current_cost() >= b.max_cost {
                budgets_exhausted += 1;
            } else {
                budgets_active += 1;
            }
        }

        CostMetricsSnapshot {
            total_requests,
            total_tokens,
            total_cost,
            avg_cost_per_request,
            avg_cost_per_1k_tokens,
            budgets_active,
            budgets_exhausted,
        }
    }

    /// Add a new pricing tier.
    pub fn add_tier(&self, name: &str, tier: PricingTier) -> Result<(), String> {
        if self.tiers.contains_key(name) {
            return Err(format!("Tier '{}' already exists", name));
        }
        self.tiers.insert(name.to_string(), tier);
        Ok(())
    }

    /// Get a pricing tier by name.
    pub fn get_tier(&self, name: &str) -> Option<PricingTier> {
        self.tiers.get(name).map(|t| t.value().clone())
    }

    /// Assign a tier to a specific model.
    pub fn set_model_tier(&self, model_id: &str, tier_name: &str) {
        self.model_tier_map.insert(model_id.to_string(), tier_name.to_string());
    }

    /// Get the assigned tier for a model.
    pub fn get_model_tier(&self, model_id: &str) -> Option<String> {
        self.model_tier_map.get(model_id).map(|t| t.value().clone())
    }

    /// Get all tier names.
    pub fn list_tiers(&self) -> Vec<PricingTier> {
        self.tiers.iter().map(|t| t.value().clone()).collect()
    }

    /// Reset all metrics.
    pub fn reset_metrics(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.total_input_tokens.store(0, Ordering::Relaxed);
        self.total_output_tokens.store(0, Ordering::Relaxed);
        self.total_cost_nanoerg.store(0, Ordering::Relaxed);
    }
}

impl Default for InferenceCostEstimator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// POST /api/cost/estimate
async fn estimate_handler(
    State(state): State<AppState>,
    Json(body): Json<EstimateRequest>,
) -> impl IntoResponse {
    let estimator = &state.cost_estimator;
    let result = if let Some(ref tier) = body.tier {
        estimator.estimate(&body.model_id, body.input_tokens, body.output_tokens, tier)
    } else {
        Ok(estimator.estimate_with_best_tier(&body.model_id, body.input_tokens, body.output_tokens))
    };

    match result {
        Ok(estimate) => (StatusCode::OK, Json(estimate)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// POST /api/cost/budget
async fn create_budget_handler(
    State(state): State<AppState>,
    Json(body): Json<BudgetRequest>,
) -> impl IntoResponse {
    match state.cost_estimator.create_budget(
        &body.budget_id,
        body.max_cost,
        body.alert_threshold,
        body.expires_at,
    ) {
        Ok(budget) => (StatusCode::CREATED, Json(budget)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// GET /api/cost/budget/:id
async fn get_budget_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.cost_estimator.get_budget(&id) {
        Some(budget) => (StatusCode::OK, Json(budget)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Budget not found" })),
        )
            .into_response(),
    }
}

/// GET /api/cost/budgets
async fn list_budgets_handler(State(state): State<AppState>) -> Json<Vec<CostBudget>> {
    Json(state.cost_estimator.list_budgets())
}

/// GET /api/cost/metrics
async fn metrics_handler(State(state): State<AppState>) -> Json<CostMetricsSnapshot> {
    Json(state.cost_estimator.get_metrics())
}

/// GET /api/cost/forecast
async fn forecast_handler(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ForecastQuery>,
) -> Json<CostForecast> {
    Json(state.cost_estimator.forecast(query.hours))
}

/// GET /api/cost/tiers
async fn tiers_handler(State(state): State<AppState>) -> Json<HashMap<String, PricingTier>> {
    let tiers = state.cost_estimator.list_tiers();
    let map: HashMap<String, PricingTier> = tiers.into_iter().map(|t| (t.name.clone(), t)).collect();
    Json(map)
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the cost estimator router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/cost/estimate", post(estimate_handler))
        .route("/api/cost/budget", post(create_budget_handler))
        .route("/api/cost/budget/{id}", get(get_budget_handler))
        .route("/api/cost/budgets", get(list_budgets_handler))
        .route("/api/cost/metrics", get(metrics_handler))
        .route("/api/cost/forecast", get(forecast_handler))
        .route("/api/cost/tiers", get(tiers_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn make_estimator() -> InferenceCostEstimator {
        InferenceCostEstimator::new()
    }

    #[test]
    fn test_new_has_default_tiers() {
        let est = make_estimator();
        assert!(est.get_tier("standard").is_some());
        assert!(est.get_tier("premium").is_some());
        assert!(est.get_tier("budget").is_some());
        assert_eq!(est.list_tiers().len(), 3);
    }

    #[test]
    fn test_estimate_standard_tier() {
        let est = make_estimator();
        let result = est.estimate("gpt-4", 100, 50, "standard");
        assert!(result.is_ok());
        let est_result = result.unwrap();
        assert_eq!(est_result.model_id, "gpt-4");
        assert_eq!(est_result.input_tokens, 100);
        assert_eq!(est_result.output_tokens, 50);
        assert!(est_result.total_cost > 0.0);
        assert_eq!(est_result.tier_used, "standard");
        assert_eq!(est_result.currency, "ERG");
    }

    #[test]
    fn test_estimate_premium_tier_more_expensive() {
        let est = make_estimator();
        let std_result = est.estimate("model", 1000, 500, "standard").unwrap();
        let prem_result = est.estimate("model", 1000, 500, "premium").unwrap();
        assert!(prem_result.total_cost > std_result.total_cost);
    }

    #[test]
    fn test_estimate_budget_tier_cheapest() {
        let est = make_estimator();
        let std_result = est.estimate("model", 1000, 500, "standard").unwrap();
        let bgt_result = est.estimate("model", 1000, 500, "budget").unwrap();
        assert!(bgt_result.total_cost < std_result.total_cost);
    }

    #[test]
    fn test_estimate_invalid_tier() {
        let est = make_estimator();
        let result = est.estimate("model", 100, 50, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_estimate_zero_tokens() {
        let est = make_estimator();
        let result = est.estimate("model", 0, 0, "standard");
        assert!(result.is_ok());
        let est_result = result.unwrap();
        assert_eq!(est_result.total_cost, 0.0);
    }

    #[test]
    fn test_estimate_large_tokens() {
        let est = make_estimator();
        let result = est.estimate("model", 500_000, 500_000, "standard");
        assert!(result.is_ok());
    }

    #[test]
    fn test_estimate_exceeds_max_tokens_budget_tier() {
        let est = make_estimator();
        // budget tier max is 500_000
        let result = est.estimate("model", 300_000, 300_000, "budget");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds maximum"));
    }

    #[test]
    fn test_estimate_with_best_tier_selects_cheapest() {
        let est = make_estimator();
        let result = est.estimate_with_best_tier("model", 1000, 500);
        assert_eq!(result.tier_used, "budget"); // budget is cheapest
    }

    #[test]
    fn test_estimate_with_best_tier_respects_model_mapping() {
        let est = make_estimator();
        est.set_model_tier("special-model", "premium");
        let result = est.estimate_with_best_tier("special-model", 1000, 500);
        assert_eq!(result.tier_used, "premium");
    }

    #[test]
    fn test_estimate_with_best_tier_falls_back_if_mapped_tier_invalid() {
        let est = make_estimator();
        est.set_model_tier("special-model", "premium");
        // Use tokens that would exceed premium max (1M) — actually premium max is 1M
        // Let's set model to a nonexistent tier
        est.set_model_tier("broken-model", "nonexistent");
        let result = est.estimate_with_best_tier("broken-model", 1000, 500);
        // Should fall back to cheapest valid tier
        assert_eq!(result.tier_used, "budget");
    }

    #[test]
    fn test_create_budget() {
        let est = make_estimator();
        let result = est.create_budget("b1", 10.0, 0.8, None);
        assert!(result.is_ok());
        let budget = result.unwrap();
        assert_eq!(budget.budget_id, "b1");
        assert_eq!(budget.max_cost, 10.0);
        assert_eq!(budget.current_cost(), 0.0);
    }

    #[test]
    fn test_create_budget_duplicate_fails() {
        let est = make_estimator();
        assert!(est.create_budget("dup", 10.0, 0.8, None).is_ok());
        assert!(est.create_budget("dup", 10.0, 0.8, None).is_err());
    }

    #[test]
    fn test_create_budget_invalid_max_cost() {
        let est = make_estimator();
        assert!(est.create_budget("bad", 0.0, 0.8, None).is_err());
        assert!(est.create_budget("bad", -5.0, 0.8, None).is_err());
    }

    #[test]
    fn test_create_budget_invalid_threshold() {
        let est = make_estimator();
        assert!(est.create_budget("bad", 10.0, 0.0, None).is_err());
        assert!(est.create_budget("bad", 10.0, 1.5, None).is_err());
    }

    #[test]
    fn test_get_budget_not_found() {
        let est = make_estimator();
        assert!(est.get_budget("nonexistent").is_none());
    }

    #[test]
    fn test_check_budget_within_limit() {
        let est = make_estimator();
        est.create_budget("b1", 10.0, 0.8, None).unwrap();
        assert!(est.check_budget("b1", 5.0).unwrap());
    }

    #[test]
    fn test_check_budget_exceeds_limit() {
        let est = make_estimator();
        est.create_budget("b1", 10.0, 0.8, None).unwrap();
        assert!(!est.check_budget("b1", 15.0).unwrap());
    }

    #[test]
    fn test_consume_budget() {
        let est = make_estimator();
        est.create_budget("b1", 10.0, 0.8, None).unwrap();
        let remaining = est.consume_budget("b1", 3.5).unwrap();
        assert!((remaining - 6.5).abs() < 0.01);
    }

    #[test]
    fn test_consume_budget_exceeds() {
        let est = make_estimator();
        est.create_budget("b1", 1.0, 0.8, None).unwrap();
        assert!(est.consume_budget("b1", 5.0).is_err());
    }

    #[test]
    fn test_consume_budget_expired() {
        let est = make_estimator();
        let past = Utc::now() - Duration::hours(1);
        est.create_budget("b1", 10.0, 0.8, Some(past)).unwrap();
        assert!(est.consume_budget("b1", 1.0).is_err());
    }

    #[test]
    fn test_list_budgets() {
        let est = make_estimator();
        est.create_budget("b1", 10.0, 0.8, None).unwrap();
        est.create_budget("b2", 20.0, 0.9, None).unwrap();
        let budgets = est.list_budgets();
        assert_eq!(budgets.len(), 2);
    }

    #[test]
    fn test_add_tier() {
        let est = make_estimator();
        let tier = PricingTier {
            name: "custom".to_string(),
            input_price_per_1k: 0.001,
            output_price_per_1k: 0.002,
            min_tokens: 100,
            max_tokens: 50_000,
            priority_boost: 1.2,
        };
        assert!(est.add_tier("custom", tier).is_ok());
        assert!(est.get_tier("custom").is_some());
    }

    #[test]
    fn test_add_tier_duplicate() {
        let est = make_estimator();
        let tier = PricingTier {
            name: "standard".to_string(),
            input_price_per_1k: 0.001,
            output_price_per_1k: 0.002,
            min_tokens: 0,
            max_tokens: 1_000_000,
            priority_boost: 1.0,
        };
        assert!(est.add_tier("standard", tier).is_err());
    }

    #[test]
    fn test_set_and_get_model_tier() {
        let est = make_estimator();
        est.set_model_tier("my-model", "premium");
        assert_eq!(est.get_model_tier("my-model"), Some("premium".to_string()));
    }

    #[test]
    fn test_get_metrics_initial() {
        let est = make_estimator();
        let metrics = est.get_metrics();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.total_tokens, 0);
        assert_eq!(metrics.total_cost, 0.0);
        assert_eq!(metrics.avg_cost_per_request, 0.0);
        assert_eq!(metrics.avg_cost_per_1k_tokens, 0.0);
        assert_eq!(metrics.budgets_active, 0);
        assert_eq!(metrics.budgets_exhausted, 0);
    }

    #[test]
    fn test_get_metrics_after_estimates() {
        let est = make_estimator();
        est.estimate("model", 100, 50, "standard").unwrap();
        est.estimate("model", 200, 100, "standard").unwrap();
        let metrics = est.get_metrics();
        assert_eq!(metrics.total_requests, 2);
        assert_eq!(metrics.total_tokens, 450);
        assert!(metrics.total_cost > 0.0);
        assert!(metrics.avg_cost_per_request > 0.0);
    }

    #[test]
    fn test_forecast() {
        let est = make_estimator();
        let forecast = est.forecast(24);
        assert_eq!(forecast.period, "24h");
        assert!(forecast.confidence > 0.0);
        assert!(forecast.confidence <= 1.0);
    }

    #[test]
    fn test_forecast_confidence_decreases_with_hours() {
        let est = make_estimator();
        let f1 = est.forecast(1);
        let f2 = est.forecast(168); // 1 week
        assert!(f1.confidence > f2.confidence);
    }

    #[test]
    fn test_reset_metrics() {
        let est = make_estimator();
        est.estimate("model", 100, 50, "standard").unwrap();
        est.reset_metrics();
        let metrics = est.get_metrics();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.total_cost, 0.0);
    }

    #[test]
    fn test_budget_is_expired() {
        let past = Utc::now() - Duration::hours(1);
        let budget = CostBudget {
            budget_id: "expired".to_string(),
            max_cost: 10.0,
            current_cost_nanoerg: AtomicU64::new(0),
            alert_threshold: 0.8,
            created_at: Utc::now() - Duration::hours(2),
            expires_at: Some(past),
        };
        assert!(budget.is_expired());

        let future = Utc::now() + Duration::hours(1);
        let budget2 = CostBudget {
            budget_id: "active".to_string(),
            max_cost: 10.0,
            current_cost_nanoerg: AtomicU64::new(0),
            alert_threshold: 0.8,
            created_at: Utc::now(),
            expires_at: Some(future),
        };
        assert!(!budget2.is_expired());
    }

    #[test]
    fn test_cost_category_serialization() {
        let cat = CostCategory::Input;
        let json = serde_json::to_string(&cat).unwrap();
        assert_eq!(json, "\"Input\"");

        let deserialized: CostCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(cat, deserialized);
    }

    #[test]
    fn test_cost_estimate_serialization() {
        let est = CostEstimate {
            model_id: "test-model".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            input_cost: 0.05,
            output_cost: 0.075,
            total_cost: 0.125,
            currency: "ERG".to_string(),
            tier_used: "standard".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&est).unwrap();
        assert!(json.contains("test-model"));
        let deserialized: CostEstimate = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model_id, "test-model");
    }

    #[test]
    fn test_metrics_budgets_active_exhausted() {
        let est = make_estimator();
        est.create_budget("b1", 1.0, 0.8, None).unwrap();
        est.create_budget("b2", 10.0, 0.8, None).unwrap();

        // Exhaust b1
        est.consume_budget("b1", 1.0).unwrap();

        let metrics = est.get_metrics();
        assert_eq!(metrics.budgets_active, 1);
        assert_eq!(metrics.budgets_exhausted, 1);
    }
}
