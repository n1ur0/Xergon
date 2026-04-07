//! Inference Cost Oracle
//!
//! Provides on-chain ERG pricing per AI model, cost estimation for inference
//! requests, and budget enforcement. Supports dynamic pricing, provider
//! competition detection, bulk discounts, and token-price conversion via
//! oracle feeds.

use std::collections::VecDeque;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NANOERG_PER_ERG: u64 = 1_000_000_000;
const MAX_HISTORY_SIZE: usize = 50_000;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Quality tier for inference requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityTier {
    Standard,
    Express,
    Premium,
}

impl Default for QualityTier {
    fn default() -> Self {
        Self::Standard
    }
}

impl std::fmt::Display for QualityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Express => write!(f, "express"),
            Self::Premium => write!(f, "premium"),
        }
    }
}

impl std::str::FromStr for QualityTier {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" => Ok(Self::Standard),
            "express" => Ok(Self::Express),
            "premium" => Ok(Self::Premium),
            _ => Err(format!("unknown quality tier: {}", s)),
        }
    }
}

/// Payment currency type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentCurrency {
    Erg,
    Token,
}

impl Default for PaymentCurrency {
    fn default() -> Self {
        Self::Erg
    }
}

impl std::fmt::Display for PaymentCurrency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Erg => write!(f, "erg"),
            Self::Token => write!(f, "token"),
        }
    }
}

// ---------------------------------------------------------------------------
// Model cost profile
// ---------------------------------------------------------------------------

/// Per-model cost profile registered by a provider.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelCostProfile {
    /// Unique model identifier (e.g. "llama-3.1-70b").
    pub model_id: String,
    /// Human-readable model name.
    pub model_name: String,
    /// Provider that hosts this model.
    pub provider_id: String,
    /// ERG cost per 1 000 input tokens (nanoERG).
    pub cost_per_1k_tokens_nanoerg: u64,
    /// ERG cost per 1 000 output tokens (nanoERG).
    pub cost_per_1k_output_tokens_nanoerg: u64,
    /// Minimum per-request cost (nanoERG).
    pub base_cost_nanoerg: u64,
    /// Maximum context window (input tokens).
    pub max_context_tokens: u32,
    /// Maximum output tokens per request.
    pub max_output_tokens: u32,
    /// Quality tier.
    pub quality_tier: QualityTier,
    /// Payment currency.
    pub currency: PaymentCurrency,
    /// Token ID for token-based payments.
    pub token_id: Option<String>,
    /// Token decimals (for converting raw amounts).
    pub token_decimals: Option<u8>,
    /// ERG price per raw token unit, sourced from oracle.
    pub token_price_nanoerg: Option<u64>,
    /// Unix timestamp of last price update.
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// Cost estimate
// ---------------------------------------------------------------------------

/// Detailed cost breakdown for a single request.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostBreakdown {
    pub base_cost: u64,
    pub input_cost: u64,
    pub output_cost: u64,
    pub overhead: u64,
}

/// Estimated cost for a single inference request.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InferenceCostEstimate {
    pub model_id: String,
    pub provider_id: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub estimated_cost_nanoerg: u64,
    pub estimated_cost_erg: f64,
    /// Token-denominated cost (present when currency is Token).
    pub estimated_cost_token: Option<f64>,
    pub quality_tier: QualityTier,
    pub currency: PaymentCurrency,
    /// Confidence 0.0 – 1.0 in how accurate the estimate is.
    pub confidence: f64,
    pub breakdown: CostBreakdown,
}

// ---------------------------------------------------------------------------
// Budget
// ---------------------------------------------------------------------------

/// User budget for spending control.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Budget {
    pub user_id: String,
    pub total_budget_nanoerg: u64,
    pub spent_nanoerg: u64,
    pub remaining_nanoerg: u64,
    pub daily_limit_nanoerg: Option<u64>,
    pub daily_spent_nanoerg: u64,
    pub alert_threshold_percent: u8,
    pub is_over_budget: bool,
    pub is_over_daily_limit: bool,
}

impl Budget {
    /// Recompute derived fields.
    fn recompute(&mut self) {
        self.remaining_nanoerg = self.total_budget_nanoerg.saturating_sub(self.spent_nanoerg);
        self.is_over_budget = self.spent_nanoerg >= self.total_budget_nanoerg;
        self.is_over_daily_limit = self
            .daily_limit_nanoerg
            .map(|lim| self.daily_spent_nanoerg >= lim)
            .unwrap_or(false);
    }
}

// ---------------------------------------------------------------------------
// Budget check
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BudgetCheckResult {
    pub allowed: bool,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Token price
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenPrice {
    pub token_id: String,
    pub price_nanoerg_per_raw: u64,
    pub decimals: u8,
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// Cost record (history ring buffer entry)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostRecord {
    pub user_id: String,
    pub model_id: String,
    pub provider_id: String,
    pub cost_nanoerg: u64,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub timestamp: i64,
}

// ---------------------------------------------------------------------------
// Batch request / estimate
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchRequest {
    pub model_id: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub quality_tier: Option<QualityTier>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BatchCostEstimate {
    pub requests: Vec<InferenceCostEstimate>,
    pub total_cost_nanoerg: u64,
    pub total_cost_erg: f64,
    pub bulk_discount_percent: f64,
    pub discounted_cost_nanoerg: u64,
    pub discounted_cost_erg: f64,
    pub request_count: usize,
}

// ---------------------------------------------------------------------------
// Model cost ranking
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelCostRanking {
    pub model_id: String,
    pub model_name: String,
    pub provider_id: String,
    pub cost_per_1k_input_nanoerg: u64,
    pub cost_per_1k_output_nanoerg: u64,
    pub quality_tier: QualityTier,
}

// ---------------------------------------------------------------------------
// Cost comparison (across providers for a single model)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostComparison {
    pub model_id: String,
    pub providers: Vec<ProviderCostEntry>,
    pub cheapest_provider_id: Option<String>,
    pub savings_vs_most_expensive_percent: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderCostEntry {
    pub provider_id: String,
    pub cost_per_1k_input_nanoerg: u64,
    pub cost_per_1k_output_nanoerg: u64,
    pub base_cost_nanoerg: u64,
    pub quality_tier: QualityTier,
    pub currency: PaymentCurrency,
}

// ---------------------------------------------------------------------------
// Demand tracker (per-model request frequency for dynamic pricing)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct DemandTracker {
    request_count: u64,
    last_updated: i64,
}

// ---------------------------------------------------------------------------
// Main oracle struct
// ---------------------------------------------------------------------------

/// Core inference cost oracle – holds model cost profiles, budgets, token
/// prices, and a ring buffer of cost records.
#[derive(Debug)]
pub struct InferenceCostOracle {
    model_costs: DashMap<String, ModelCostProfile>,
    budgets: DashMap<String, Budget>,
    price_cache: DashMap<String, TokenPrice>,
    history: tokio::sync::Mutex<VecDeque<CostRecord>>,
    demand: DashMap<String, DemandTracker>,
}

impl InferenceCostOracle {
    /// Create a new, empty oracle.
    pub fn new() -> Self {
        Self {
            model_costs: DashMap::new(),
            budgets: DashMap::new(),
            price_cache: DashMap::new(),
            history: tokio::sync::Mutex::new(VecDeque::with_capacity(MAX_HISTORY_SIZE)),
            demand: DashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Cost estimation
    // -----------------------------------------------------------------------

    /// Find the cheapest provider for a given model_id.
    fn find_cheapest_profile(&self, model_id: &str) -> Option<ModelCostProfile> {
        let mut best: Option<ModelCostProfile> = None;
        for entry in self.model_costs.iter() {
            if entry.model_id == model_id {
                if best.is_none()
                    || entry.cost_per_1k_tokens_nanoerg
                        < best.as_ref().unwrap().cost_per_1k_tokens_nanoerg
                {
                    best = Some(entry.clone());
                }
            }
        }
        best
    }

    /// Estimate the cost of a single inference request.
    pub fn estimate_cost(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        quality_tier: QualityTier,
    ) -> Result<InferenceCostEstimate, String> {
        // Find cheapest provider for this model.
        let profile = self
            .find_cheapest_profile(model_id)
            .ok_or_else(|| format!("model cost profile not found: {}", model_id))?;

        let profile = profile.clone();

        // Apply quality-tier multiplier.
        let tier_mult = match quality_tier {
            QualityTier::Standard => 1.0,
            QualityTier::Express => 1.5,
            QualityTier::Premium => 2.5,
        };

        // Compute raw costs.
        let input_cost =
            ((profile.cost_per_1k_tokens_nanoerg as u128 * input_tokens as u128) / 1000) as u64;
        let output_cost =
            ((profile.cost_per_1k_output_tokens_nanoerg as u128 * output_tokens as u128) / 1000)
                as u64;
        let overhead = ((input_cost + output_cost) as f64 * 0.05) as u64; // 5 % overhead

        let total = (profile.base_cost_nanoerg as f64
            + input_cost as f64
            + output_cost as f64
            + overhead as f64)
            * tier_mult;

        let estimated_cost_nanoerg = total.ceil() as u64;
        let estimated_cost_erg = estimated_cost_nanoerg as f64 / NANOERG_PER_ERG as f64;

        // Token-denominated cost when currency is Token.
        let estimated_cost_token = if profile.currency == PaymentCurrency::Token {
            if let (Some(price), Some(decimals)) =
                (profile.token_price_nanoerg, profile.token_decimals)
            {
                let divisor = 10u64.pow(decimals as u32);
                let token_cost_raw = estimated_cost_nanoerg / price.max(1);
                Some(token_cost_raw as f64 / divisor as f64)
            } else {
                None
            }
        } else {
            None
        };

        // Dynamic pricing confidence based on demand volatility.
        let confidence = self.compute_confidence(model_id);

        Ok(InferenceCostEstimate {
            model_id: profile.model_id.clone(),
            provider_id: profile.provider_id.clone(),
            input_tokens,
            output_tokens,
            estimated_cost_nanoerg,
            estimated_cost_erg,
            estimated_cost_token,
            quality_tier,
            currency: profile.currency,
            confidence,
            breakdown: CostBreakdown {
                base_cost: profile.base_cost_nanoerg,
                input_cost,
                output_cost,
                overhead,
            },
        })
    }

    /// Estimate cost for a batch of requests, applying bulk discounts.
    pub fn estimate_batch_cost(
        &self,
        requests: Vec<BatchRequest>,
    ) -> Result<BatchCostEstimate, String> {
        let count = requests.len();
        let discount = self.bulk_discount(count);

        let mut estimates = Vec::with_capacity(count);
        let mut total = 0u64;

        for req in &requests {
            let tier = req.quality_tier.unwrap_or_default();
            let est = self.estimate_cost(&req.model_id, req.input_tokens, req.output_tokens, tier)?;
            total += est.estimated_cost_nanoerg;
            estimates.push(est);
        }

        let discounted = (total as f64 * (1.0 - discount)) as u64;

        Ok(BatchCostEstimate {
            requests: estimates,
            total_cost_nanoerg: total,
            total_cost_erg: total as f64 / NANOERG_PER_ERG as f64,
            bulk_discount_percent: discount * 100.0,
            discounted_cost_nanoerg: discounted,
            discounted_cost_erg: discounted as f64 / NANOERG_PER_ERG as f64,
            request_count: count,
        })
    }

    // -----------------------------------------------------------------------
    // Model registration
    // -----------------------------------------------------------------------

    /// Build a composite key for model_costs: "{model_id}:{provider_id}".
    fn model_key(model_id: &str, provider_id: &str) -> String {
        format!("{}:{}", model_id, provider_id)
    }

    /// Register or update a model cost profile.
    pub fn register_model_cost(&self, profile: ModelCostProfile) {
        let key = Self::model_key(&profile.model_id, &profile.provider_id);
        info!(model_id = %profile.model_id, provider_id = %profile.provider_id, "registering model cost profile");
        self.model_costs.insert(key, profile);
    }

    // -----------------------------------------------------------------------
    // Token price updates
    // -----------------------------------------------------------------------

    /// Update a cached token price (from oracle feeds).
    pub fn update_token_price(&self, token_id: &str, price_nanoerg_per_raw: u64, decimals: u8) {
        let now = Utc::now().timestamp();
        let price = TokenPrice {
            token_id: token_id.to_string(),
            price_nanoerg_per_raw,
            decimals,
            updated_at: now,
        };
        debug!(token_id, price = price_nanoerg_per_raw, "updating token price");
        self.price_cache.insert(token_id.to_string(), price);

        // Propagate to any model profiles that use this token.
        for mut entry in self.model_costs.iter_mut() {
            if entry.value().token_id.as_deref() == Some(token_id) {
                entry.value_mut().token_price_nanoerg = Some(price_nanoerg_per_raw);
                entry.value_mut().token_decimals = Some(decimals);
                entry.value_mut().updated_at = now;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Budget management
    // -----------------------------------------------------------------------

    /// Check whether a user is allowed to spend the given amount.
    pub fn check_budget(&self, user_id: &str, estimated_cost_nanoerg: u64) -> BudgetCheckResult {
        if let Some(budget) = self.budgets.get(user_id) {
            // Check daily limit first.
            if budget.is_over_daily_limit {
                return BudgetCheckResult {
                    allowed: false,
                    reason: "daily spending limit exceeded".to_string(),
                };
            }
            // Check if this single request would exceed daily limit.
            if let Some(daily_lim) = budget.daily_limit_nanoerg {
                if budget.daily_spent_nanoerg + estimated_cost_nanoerg > daily_lim {
                    return BudgetCheckResult {
                        allowed: false,
                        reason: "request would exceed daily spending limit".to_string(),
                    };
                }
            }
            if budget.is_over_budget {
                return BudgetCheckResult {
                    allowed: false,
                    reason: "total budget exceeded".to_string(),
                };
            }
            if estimated_cost_nanoerg > budget.remaining_nanoerg {
                return BudgetCheckResult {
                    allowed: false,
                    reason: format!(
                        "insufficient remaining budget: need {} nanoERG, have {}",
                        estimated_cost_nanoerg, budget.remaining_nanoerg
                    ),
                };
            }
            // Check alert threshold.
            let new_spent = budget.spent_nanoerg + estimated_cost_nanoerg;
            let pct = (new_spent as f64 / budget.total_budget_nanoerg as f64) * 100.0;
            if pct >= budget.alert_threshold_percent as f64 {
                warn!(
                    user_id,
                    percent = pct,
                    threshold = budget.alert_threshold_percent,
                    "budget alert threshold reached"
                );
            }
            BudgetCheckResult {
                allowed: true,
                reason: String::new(),
            }
        } else {
            // No budget set – allow by default.
            BudgetCheckResult {
                allowed: true,
                reason: "no budget configured".to_string(),
            }
        }
    }

    /// Get a snapshot of a user's budget (or None if not configured).
    pub fn get_user_budget(&self, user_id: &str) -> Option<Budget> {
        self.budgets.get(user_id).map(|b| b.clone())
    }

    /// Create or replace a user's budget.
    pub fn set_budget(
        &self,
        user_id: &str,
        total_nanoerg: u64,
        daily_limit: Option<u64>,
    ) {
        let mut budget = Budget {
            user_id: user_id.to_string(),
            total_budget_nanoerg: total_nanoerg,
            spent_nanoerg: 0,
            remaining_nanoerg: total_nanoerg,
            daily_limit_nanoerg: daily_limit,
            daily_spent_nanoerg: 0,
            alert_threshold_percent: 80,
            is_over_budget: false,
            is_over_daily_limit: false,
        };
        budget.recompute();
        info!(user_id, total = total_nanoerg, "setting user budget");
        self.budgets.insert(user_id.to_string(), budget);
    }

    // -----------------------------------------------------------------------
    // Usage recording
    // -----------------------------------------------------------------------

    /// Record actual inference usage against a user's budget.
    pub async fn record_usage(
        &self,
        user_id: &str,
        model_id: &str,
        cost_nanoerg: u64,
        input_tokens: u32,
        output_tokens: u32,
    ) {
        // Update budget.
        if let Some(mut budget) = self.budgets.get_mut(user_id) {
            budget.spent_nanoerg += cost_nanoerg;
            budget.daily_spent_nanoerg += cost_nanoerg;
            budget.recompute();
        }

        // Determine provider_id from model profile (find cheapest).
        let provider_id = self
            .find_cheapest_profile(model_id)
            .map(|p| p.provider_id)
            .unwrap_or_default();

        // Append to ring buffer.
        let record = CostRecord {
            user_id: user_id.to_string(),
            model_id: model_id.to_string(),
            provider_id,
            cost_nanoerg,
            input_tokens,
            output_tokens,
            timestamp: Utc::now().timestamp(),
        };
        let mut history = self.history.lock().await;
        if history.len() >= MAX_HISTORY_SIZE {
            history.pop_front();
        }
        history.push_back(record);

        // Update demand tracker.
        if let Some(mut d) = self.demand.get_mut(model_id) {
            d.request_count += 1;
            d.last_updated = Utc::now().timestamp();
        } else {
            self.demand.insert(
                model_id.to_string(),
                DemandTracker {
                    request_count: 1,
                    last_updated: Utc::now().timestamp(),
                },
            );
        }

        debug!(user_id, model_id, cost = cost_nanoerg, "recorded usage");
    }

    // -----------------------------------------------------------------------
    // Analytics
    // -----------------------------------------------------------------------

    /// Return top models sorted by input cost (cheapest first).
    pub fn get_top_models_by_cost(&self, limit: usize) -> Vec<ModelCostRanking> {
        let mut models: Vec<ModelCostRanking> = self
            .model_costs
            .iter()
            .map(|entry| ModelCostRanking {
                model_id: entry.model_id.clone(),
                model_name: entry.model_name.clone(),
                provider_id: entry.provider_id.clone(),
                cost_per_1k_input_nanoerg: entry.cost_per_1k_tokens_nanoerg,
                cost_per_1k_output_nanoerg: entry.cost_per_1k_output_tokens_nanoerg,
                quality_tier: entry.quality_tier,
            })
            .collect();

        models.sort_by_key(|m| m.cost_per_1k_input_nanoerg);
        models.truncate(limit);
        models
    }

    /// Compare costs across all providers that serve a given model.
    pub fn get_cost_savings_comparison(&self, model_id: &str) -> CostComparison {
        let providers: Vec<ProviderCostEntry> = self
            .model_costs
            .iter()
            .filter(|e| e.model_id == model_id)
            .map(|e| ProviderCostEntry {
                provider_id: e.provider_id.clone(),
                cost_per_1k_input_nanoerg: e.cost_per_1k_tokens_nanoerg,
                cost_per_1k_output_nanoerg: e.cost_per_1k_output_tokens_nanoerg,
                base_cost_nanoerg: e.base_cost_nanoerg,
                quality_tier: e.quality_tier,
                currency: e.currency,
            })
            .collect();

        let cheapest_id = providers
            .iter()
            .min_by_key(|p| p.cost_per_1k_input_nanoerg)
            .map(|p| p.provider_id.clone());

        let savings = if providers.len() >= 2 {
            let min = providers
                .iter()
                .map(|p| p.cost_per_1k_input_nanoerg)
                .min()
                .unwrap_or(0);
            let max = providers
                .iter()
                .map(|p| p.cost_per_1k_input_nanoerg)
                .max()
                .unwrap_or(0);
            if max > 0 {
                Some(((max - min) as f64 / max as f64) * 100.0)
            } else {
                None
            }
        } else {
            None
        };

        CostComparison {
            model_id: model_id.to_string(),
            providers,
            cheapest_provider_id: cheapest_id,
            savings_vs_most_expensive_percent: savings,
        }
    }

    /// Retrieve recent cost history for a user.
    pub async fn get_user_history(&self, user_id: &str, limit: usize) -> Vec<CostRecord> {
        let history = self.history.lock().await;
        history
            .iter()
            .rev()
            .filter(|r| r.user_id == user_id)
            .take(limit)
            .cloned()
            .collect()
    }

    /// List all registered model cost profiles.
    pub fn list_models(&self) -> Vec<ModelCostProfile> {
        self.model_costs.iter().map(|e| e.clone()).collect()
    }

    // -----------------------------------------------------------------------
    // Bulk discount
    // -----------------------------------------------------------------------

    fn bulk_discount(&self, count: usize) -> f64 {
        match count {
            0..=9 => 0.0,
            10..=49 => 0.15,
            50..=99 => 0.25,
            _ => 0.35,
        }
    }

    // -----------------------------------------------------------------------
    // Confidence (dynamic pricing signal)
    // -----------------------------------------------------------------------

    fn compute_confidence(&self, model_id: &str) -> f64 {
        match self.demand.get(model_id) {
            Some(d) => {
                // High request counts reduce confidence in static pricing accuracy.
                let decay = 1.0 / (1.0 + (d.request_count as f64 * 0.001));
                (decay * 0.3 + 0.7).min(1.0) // floor 0.7
            }
            None => 1.0, // No demand data – full confidence in listed price.
        }
    }
}

impl Default for InferenceCostOracle {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// REST API
// ===========================================================================

/// Shared state type for axum handlers.
type OracleState = Arc<InferenceCostOracle>;

// -- Query / Body types --

#[derive(Deserialize)]
struct EstimateQuery {
    model_id: String,
    input_tokens: u32,
    output_tokens: u32,
    tier: Option<String>,
}

#[derive(Deserialize)]
struct SetBudgetBody {
    user_id: String,
    total_budget_nanoerg: u64,
    daily_limit_nanoerg: Option<u64>,
}

#[derive(Deserialize)]
struct RecordUsageBody {
    user_id: String,
    model_id: String,
    cost_nanoerg: u64,
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
struct HistoryQuery {
    limit: Option<usize>,
}

// -- Handlers --

async fn handle_estimate(
    State(state): State<OracleState>,
    Query(q): Query<EstimateQuery>,
) -> impl IntoResponse {
    let tier = q
        .tier
        .as_deref()
        .unwrap_or("standard")
        .parse::<QualityTier>()
        .unwrap_or_default();

    match state.estimate_cost(&q.model_id, q.input_tokens, q.output_tokens, tier) {
        Ok(est) => (StatusCode::OK, Json(est)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn handle_estimate_batch(
    State(state): State<OracleState>,
    Json(body): Json<Vec<BatchRequest>>,
) -> impl IntoResponse {
    match state.estimate_batch_cost(body) {
        Ok(est) => (StatusCode::OK, Json(est)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn handle_list_models(State(state): State<OracleState>) -> Json<Vec<ModelCostProfile>> {
    Json(state.list_models())
}

async fn handle_compare(
    State(state): State<OracleState>,
    Path(model_id): Path<String>,
) -> impl IntoResponse {
    let comp = state.get_cost_savings_comparison(&model_id);
    (StatusCode::OK, Json(comp)).into_response()
}

async fn handle_set_budget(
    State(state): State<OracleState>,
    Json(body): Json<SetBudgetBody>,
) -> impl IntoResponse {
    state.set_budget(&body.user_id, body.total_budget_nanoerg, body.daily_limit_nanoerg);
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response()
}

async fn handle_get_budget(
    State(state): State<OracleState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.get_user_budget(&user_id) {
        Some(b) => (StatusCode::OK, Json(b)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "budget not found"})),
        )
            .into_response(),
    }
}

async fn handle_budget_history(
    State(state): State<OracleState>,
    Path(user_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Json<Vec<CostRecord>> {
    let limit = q.limit.unwrap_or(50);
    Json(state.get_user_history(&user_id, limit).await)
}

async fn handle_record_usage(
    State(state): State<OracleState>,
    Json(body): Json<RecordUsageBody>,
) -> impl IntoResponse {
    state
        .record_usage(
            &body.user_id,
            &body.model_id,
            body.cost_nanoerg,
            body.input_tokens,
            body.output_tokens,
        )
        .await;
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response()
}

/// Build the axum Router for this module.
pub fn router(state: Arc<InferenceCostOracle>) -> Router {
    Router::new()
        .route("/api/v1/cost/estimate", get(handle_estimate))
        .route("/api/v1/cost/estimate-batch", post(handle_estimate_batch))
        .route("/api/v1/cost/models", get(handle_list_models))
        .route("/api/v1/cost/compare/:model_id", get(handle_compare))
        .route("/api/v1/budget/set", post(handle_set_budget))
        .route("/api/v1/budget/:user_id", get(handle_get_budget))
        .route("/api/v1/budget/:user_id/history", get(handle_budget_history))
        .route("/api/v1/cost/record-usage", post(handle_record_usage))
        .with_state(state)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(
        model_id: &str,
        provider_id: &str,
        input_rate: u64,
        output_rate: u64,
    ) -> ModelCostProfile {
        ModelCostProfile {
            model_id: model_id.to_string(),
            model_name: format!("{} Model", model_id),
            provider_id: provider_id.to_string(),
            cost_per_1k_tokens_nanoerg: input_rate,
            cost_per_1k_output_tokens_nanoerg: output_rate,
            base_cost_nanoerg: 10_000_000, // 0.01 ERG
            max_context_tokens: 128_000,
            max_output_tokens: 4_096,
            quality_tier: QualityTier::Standard,
            currency: PaymentCurrency::Erg,
            token_id: None,
            token_decimals: None,
            token_price_nanoerg: None,
            updated_at: Utc::now().timestamp(),
        }
    }

    fn make_token_profile(
        model_id: &str,
        provider_id: &str,
        token_id: &str,
        token_price_nanoerg: u64,
        token_decimals: u8,
    ) -> ModelCostProfile {
        ModelCostProfile {
            model_id: model_id.to_string(),
            model_name: format!("{} Model", model_id),
            provider_id: provider_id.to_string(),
            cost_per_1k_tokens_nanoerg: 500_000,
            cost_per_1k_output_tokens_nanoerg: 1_500_000,
            base_cost_nanoerg: 10_000_000,
            max_context_tokens: 128_000,
            max_output_tokens: 4_096,
            quality_tier: QualityTier::Standard,
            currency: PaymentCurrency::Token,
            token_id: Some(token_id.to_string()),
            token_decimals: Some(token_decimals),
            token_price_nanoerg: Some(token_price_nanoerg),
            updated_at: Utc::now().timestamp(),
        }
    }

    #[test]
    fn test_cost_estimation_various_token_counts() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("m1", "p1", 500_000, 1_500_000));

        // 1K input, 500 output.
        let est = oracle
            .estimate_cost("m1", 1000, 500, QualityTier::Standard)
            .unwrap();
        // base=10M + input=500K + output=750K = 15.75M (with 5% overhead)
        assert!(est.estimated_cost_nanoerg > 0);
        assert!(est.estimated_cost_erg > 0.0);
        assert_eq!(est.input_tokens, 1000);
        assert_eq!(est.output_tokens, 500);
        assert!(est.breakdown.base_cost == 10_000_000);

        // Zero tokens – should still incur base cost.
        let est0 = oracle
            .estimate_cost("m1", 0, 0, QualityTier::Standard)
            .unwrap();
        assert!(est0.estimated_cost_nanoerg >= 10_000_000);
    }

    #[test]
    fn test_budget_enforcement_over_budget() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("m1", "p1", 500_000, 1_500_000));
        oracle.set_budget("user1", 50_000_000, None); // 0.05 ERG total

        // Spend 40M.
        let result = oracle.check_budget("user1", 40_000_000);
        assert!(result.allowed);

        // Record 40M.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            oracle.record_usage("user1", "m1", 40_000_000, 1000, 500).await;
        });

        // Try to spend 20M more – should be denied.
        let result = oracle.check_budget("user1", 20_000_000);
        assert!(!result.allowed);
        assert!(result.reason.contains("budget"));
    }

    #[test]
    fn test_budget_daily_limit() {
        let oracle = InferenceCostOracle::new();
        oracle.set_budget("user2", 1_000_000_000, Some(30_000_000)); // daily 0.03 ERG

        // Spend 25M.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            oracle.record_usage("user2", "m1", 25_000_000, 1000, 500).await;
        });

        // Try 10M more – exceeds daily.
        let result = oracle.check_budget("user2", 10_000_000);
        assert!(!result.allowed);
        assert!(result.reason.contains("daily"));
    }

    #[test]
    fn test_token_price_conversion() {
        let oracle = InferenceCostOracle::new();
        // Token priced at 0.5 nanoERG per raw unit, 6 decimals.
        oracle.register_model_cost(make_token_profile(
            "m-token",
            "p1",
            "tok1",
            500_000, // 0.5 nanoERG per raw
            6,
        ));

        let est = oracle
            .estimate_cost("m-token", 1000, 500, QualityTier::Standard)
            .unwrap();
        assert_eq!(est.currency, PaymentCurrency::Token);
        assert!(est.estimated_cost_token.is_some());
        let token_cost = est.estimated_cost_token.unwrap();
        assert!(token_cost > 0.0);
    }

    #[test]
    fn test_bulk_discount_calculation() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("m1", "p1", 500_000, 1_500_000));

        // No discount for 5 requests.
        let batch5 = vec![
            BatchRequest { model_id: "m1".into(), input_tokens: 100, output_tokens: 50, quality_tier: None };
            5
        ];
        let est = oracle.estimate_batch_cost(batch5).unwrap();
        assert_eq!(est.request_count, 5);
        assert_eq!(est.bulk_discount_percent, 0.0);
        assert_eq!(est.total_cost_nanoerg, est.discounted_cost_nanoerg);

        // 15% discount for 10 requests.
        let batch10 = vec![
            BatchRequest { model_id: "m1".into(), input_tokens: 100, output_tokens: 50, quality_tier: None };
            10
        ];
        let est = oracle.estimate_batch_cost(batch10).unwrap();
        assert_eq!(est.bulk_discount_percent, 15.0);
        assert!(est.discounted_cost_nanoerg < est.total_cost_nanoerg);

        // 25% for 50.
        let batch50 = vec![
            BatchRequest { model_id: "m1".into(), input_tokens: 100, output_tokens: 50, quality_tier: None };
            50
        ];
        let est = oracle.estimate_batch_cost(batch50).unwrap();
        assert_eq!(est.bulk_discount_percent, 25.0);

        // 35% for 100.
        let batch100 = vec![
            BatchRequest { model_id: "m1".into(), input_tokens: 100, output_tokens: 50, quality_tier: None };
            100
        ];
        let est = oracle.estimate_batch_cost(batch100).unwrap();
        assert_eq!(est.bulk_discount_percent, 35.0);
    }

    #[test]
    fn test_provider_comparison() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("shared-model", "cheap-provider", 300_000, 900_000));
        oracle.register_model_cost(make_profile("shared-model", "expensive-provider", 800_000, 2_000_000));
        oracle.register_model_cost(make_profile("other-model", "some-provider", 500_000, 1_500_000));

        let comp = oracle.get_cost_savings_comparison("shared-model");
        assert_eq!(comp.model_id, "shared-model");
        assert_eq!(comp.providers.len(), 2);
        assert_eq!(comp.cheapest_provider_id.as_deref(), Some("cheap-provider"));
        assert!(comp.savings_vs_most_expensive_percent.is_some());
        let savings = comp.savings_vs_most_expensive_percent.unwrap();
        assert!(savings > 0.0);

        // Single-provider model.
        let comp2 = oracle.get_cost_savings_comparison("other-model");
        assert_eq!(comp2.providers.len(), 1);
        assert!(comp2.savings_vs_most_expensive_percent.is_none());
    }

    #[test]
    fn test_batch_cost_estimation() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("m1", "p1", 500_000, 1_500_000));
        oracle.register_model_cost(make_profile("m2", "p1", 1_000_000, 3_000_000));

        let batch = vec![
            BatchRequest { model_id: "m1".into(), input_tokens: 2000, output_tokens: 1000, quality_tier: None },
            BatchRequest { model_id: "m2".into(), input_tokens: 500, output_tokens: 250, quality_tier: None },
        ];
        let est = oracle.estimate_batch_cost(batch).unwrap();
        assert_eq!(est.request_count, 2);
        assert!(est.total_cost_nanoerg > 0);
        assert!(est.total_cost_erg > 0.0);
    }

    #[test]
    fn test_budget_alert_threshold() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("m1", "p1", 500_000, 1_500_000));
        oracle.set_budget("user3", 100_000_000, None); // 0.1 ERG

        // Spend 75M (75% of budget – just under default 80% alert threshold).
        let result = oracle.check_budget("user3", 75_000_000);
        assert!(result.allowed);

        // Spend 5M more – 80M total, exactly at threshold.
        let result = oracle.check_budget("user3", 5_000_000);
        assert!(result.allowed); // Still allowed, but warning logged.
    }

    #[test]
    fn test_unknown_model_returns_error() {
        let oracle = InferenceCostOracle::new();
        let result = oracle.estimate_cost("nonexistent", 100, 50, QualityTier::Standard);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_budget_allows_all() {
        let oracle = InferenceCostOracle::new();
        let result = oracle.check_budget("ghost", 999_999_999_999);
        assert!(result.allowed);
    }

    #[test]
    fn test_quality_tier_pricing() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("m1", "p1", 500_000, 1_500_000));

        let std = oracle
            .estimate_cost("m1", 1000, 500, QualityTier::Standard)
            .unwrap();
        let exp = oracle
            .estimate_cost("m1", 1000, 500, QualityTier::Express)
            .unwrap();
        let prem = oracle
            .estimate_cost("m1", 1000, 500, QualityTier::Premium)
            .unwrap();

        // Express ≈ 1.5x Standard, Premium ≈ 2.5x Standard.
        let ratio_exp = exp.estimated_cost_nanoerg as f64 / std.estimated_cost_nanoerg as f64;
        let ratio_prem = prem.estimated_cost_nanoerg as f64 / std.estimated_cost_nanoerg as f64;
        assert!((ratio_exp - 1.5).abs() < 0.02);
        assert!((ratio_prem - 2.5).abs() < 0.02);
    }

    #[test]
    fn test_token_price_update_propagates() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_token_profile(
            "m-tok", "p1", "tok123", 100, 9,
        ));

        // Update oracle price.
        oracle.update_token_price("tok123", 200, 9);

        // Check that the model profile was updated.
        let key = InferenceCostOracle::model_key("m-tok", "p1");
        let profile = oracle.model_costs.get(&key).unwrap();
        assert_eq!(profile.value().token_price_nanoerg, Some(200));
        assert_eq!(profile.value().token_decimals, Some(9));
    }

    #[test]
    fn test_top_models_sorted() {
        let oracle = InferenceCostOracle::new();
        oracle.register_model_cost(make_profile("expensive", "p1", 2_000_000, 5_000_000));
        oracle.register_model_cost(make_profile("cheap", "p1", 200_000, 600_000));
        oracle.register_model_cost(make_profile("mid", "p1", 800_000, 2_400_000));

        let top = oracle.get_top_models_by_cost(10);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].model_id, "cheap");
        assert_eq!(top[1].model_id, "mid");
        assert_eq!(top[2].model_id, "expensive");
    }
}
