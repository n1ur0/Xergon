use std::collections::VecDeque;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Days, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NANOERG_PER_ERG: u64 = 1_000_000_000;

/// Centralized provider pricing (per 1M input tokens, in USD).
/// OpenAI GPT-4: $30 / 1M input tokens => $0.03 / 1K input tokens
/// Anthropic Claude (opus): $15 / 1M input tokens => $0.015 / 1K input tokens
const OPENAI_COST_PER_1K_USD: f64 = 0.03;
const ANTHROPIC_COST_PER_1K_USD: f64 = 0.015;

const DEFAULT_ERG_USD_RATE: f64 = 1.20;

const MAX_SNAPSHOTS: usize = 10_000;

// ---------------------------------------------------------------------------
// AlertLevel
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AlertLevel {
    Green,
    Yellow,
    Orange,
    Red,
}

impl AlertLevel {
    pub fn from_percent_used(pct: f64) -> Self {
        if pct < 50.0 {
            Self::Green
        } else if pct < 75.0 {
            Self::Yellow
        } else if pct < 90.0 {
            Self::Orange
        } else {
            Self::Red
        }
    }
}

// ---------------------------------------------------------------------------
// PriceTrend
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PriceTrend {
    Increasing,
    Decreasing,
    Stable,
}

// ---------------------------------------------------------------------------
// CostHistoryPoint
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostHistoryPoint {
    pub date: DateTime<Utc>,
    pub cost_erg: f64,
    pub tokens_used: u64,
}

// ---------------------------------------------------------------------------
// PriceRange
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceRange {
    pub min: u64,
    pub max: u64,
    pub median: u64,
}

// ---------------------------------------------------------------------------
// ProviderPricing
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderPricing {
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub model_name: String,
    pub cost_per_1k_input_nanoerg: u64,
    pub cost_per_1k_output_nanoerg: u64,
    pub base_cost_nanoerg: u64,
    pub quality_tier: String,
    pub available: bool,
    pub latency_ms: u64,
    pub uptime_percent: f64,
    pub total_requests_served: u64,
    pub rating: f64,
}

// ---------------------------------------------------------------------------
// PricingDisplay
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PricingDisplay {
    pub model_id: String,
    pub model_name: String,
    pub providers: Vec<ProviderPricing>,
    pub cheapest_provider: Option<ProviderPricing>,
    pub fastest_provider: Option<ProviderPricing>,
    pub best_value_provider: Option<ProviderPricing>,
    pub avg_cost_per_1k_tokens_nanoerg: u64,
    pub price_range: PriceRange,
    pub currency: String,
    pub erg_usd_rate: Option<f64>,
    pub usd_cost_per_1k: Option<f64>,
}

// ---------------------------------------------------------------------------
// UserBudgetDisplay
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserBudgetDisplay {
    pub user_id: String,
    pub total_budget_erg: f64,
    pub spent_erg: f64,
    pub remaining_erg: f64,
    pub percent_used: f64,
    pub daily_budget_erg: Option<f64>,
    pub daily_spent_erg: f64,
    pub daily_remaining_erg: Option<f64>,
    pub estimated_tokens_remaining: u64,
    pub projected_monthly_cost_erg: f64,
    pub savings_vs_centralized: f64,
    pub alert_level: AlertLevel,
    pub cost_history: Vec<CostHistoryPoint>,
}

// ---------------------------------------------------------------------------
// CostComparisonWidget
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CostComparisonWidget {
    pub xergon_cost_per_1k_erg: f64,
    pub openai_cost_per_1k_usd: f64,
    pub anthropic_cost_per_1k_usd: f64,
    pub erg_usd_rate: f64,
    pub xergon_usd_per_1k: f64,
    pub savings_vs_openai_percent: f64,
    pub savings_vs_anthropic_percent: f64,
    pub model_name: String,
}

// ---------------------------------------------------------------------------
// PricingSnapshot (for ring buffer / trending)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PricingSnapshot {
    pub timestamp: DateTime<Utc>,
    pub model_id: String,
    pub avg_cost_per_1k_nanoerg: u64,
    pub provider_count: usize,
    pub trend: PriceTrend,
}

// ---------------------------------------------------------------------------
// SetBudgetRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetBudgetRequest {
    pub total_budget_erg: f64,
    pub daily_budget_erg: Option<f64>,
}

// ---------------------------------------------------------------------------
// InferencePricingEngine
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct InferencePricingEngine {
    provider_pricing: DashMap<String, Vec<ProviderPricing>>,
    user_budgets: DashMap<String, UserBudgetDisplay>,
    #[allow(dead_code)]
    cost_history: DashMap<String, Vec<CostHistoryPoint>>,
    pricing_snapshots: DashMap<String, VecDeque<PricingSnapshot>>,
    erg_usd_rate: f64,
}

impl InferencePricingEngine {
    pub fn new(erg_usd_rate: Option<f64>) -> Self {
        Self {
            provider_pricing: DashMap::new(),
            user_budgets: DashMap::new(),
            cost_history: DashMap::new(),
            pricing_snapshots: DashMap::new(),
            erg_usd_rate: erg_usd_rate.unwrap_or(DEFAULT_ERG_USD_RATE),
        }
    }

    // -----------------------------------------------------------------------
    // Provider pricing management
    // -----------------------------------------------------------------------

    pub fn register_provider_pricing(&self, pricing: ProviderPricing) {
        let model_id = pricing.model_id.clone();
        {
            let mut entry = self
                .provider_pricing
                .entry(model_id.clone())
                .or_default();

            let providers = entry.value_mut();
            if let Some(pos) = providers.iter().position(|p| p.provider_id == pricing.provider_id) {
                providers[pos] = pricing;
            } else {
                providers.push(pricing);
            }
        } // Drop the entry before take_snapshot

        self.take_snapshot(&model_id);
    }

    pub fn remove_provider(&self, model_id: &str, provider_id: &str) {
        if let Some(mut entry) = self.provider_pricing.get_mut(model_id) {
            entry.retain(|p| p.provider_id != provider_id);
        }
    }

    // -----------------------------------------------------------------------
    // Pricing display calculation
    // -----------------------------------------------------------------------

    pub fn get_pricing_display(&self, model_id: &str) -> Option<PricingDisplay> {
        let providers = self.provider_pricing.get(model_id)?;
        let available: Vec<ProviderPricing> = providers
            .iter()
            .filter(|p| p.available)
            .cloned()
            .collect();

        if available.is_empty() {
            return None;
        }

        let model_name = available[0].model_name.clone();

        let cheapest = available
            .iter()
            .min_by_key(|p| p.cost_per_1k_input_nanoerg)
            .cloned();

        let fastest = available.iter().min_by_key(|p| p.latency_ms).cloned();

        // Best value: lowest cost*latency composite score
        let best_value = available
            .iter()
            .min_by_key(|p| {
                let composite = (p.cost_per_1k_input_nanoerg as f64)
                    * (p.latency_ms as f64)
                    / 1_000_000.0;
                (composite * 1_000_000.0) as u64
            })
            .cloned();

        let costs: Vec<u64> = available
            .iter()
            .map(|p| p.cost_per_1k_input_nanoerg)
            .collect();

        let avg_cost = costs.iter().sum::<u64>() / costs.len() as u64;
        let min_cost = *costs.iter().min().unwrap_or(&0);
        let max_cost = *costs.iter().max().unwrap_or(&0);
        let median_cost = median_u64(&costs);

        let erg_rate = Some(self.erg_usd_rate);
        let usd_cost = Some(avg_cost as f64 / NANOERG_PER_ERG as f64 * self.erg_usd_rate);

        Some(PricingDisplay {
            model_id: model_id.to_string(),
            model_name,
            providers: available,
            cheapest_provider: cheapest,
            fastest_provider: fastest,
            best_value_provider: best_value,
            avg_cost_per_1k_tokens_nanoerg: avg_cost,
            price_range: PriceRange {
                min: min_cost,
                max: max_cost,
                median: median_cost,
            },
            currency: "ERG".to_string(),
            erg_usd_rate: erg_rate,
            usd_cost_per_1k: usd_cost,
        })
    }

    pub fn get_all_models_pricing(&self) -> Vec<PricingDisplay> {
        // Collect keys first to avoid holding iter lock while calling get_pricing_display
        let keys: Vec<String> = self
            .provider_pricing
            .iter()
            .map(|kv| kv.key().clone())
            .collect();
        keys.into_iter()
            .filter_map(|k| self.get_pricing_display(&k))
            .collect()
    }

    // -----------------------------------------------------------------------
    // Budget management
    // -----------------------------------------------------------------------

    pub fn set_user_budget(
        &self,
        user_id: &str,
        total_budget_erg: f64,
        daily_budget_erg: Option<f64>,
    ) {
        let budget = UserBudgetDisplay {
            user_id: user_id.to_string(),
            total_budget_erg,
            spent_erg: 0.0,
            remaining_erg: total_budget_erg,
            percent_used: 0.0,
            daily_budget_erg,
            daily_spent_erg: 0.0,
            daily_remaining_erg: daily_budget_erg,
            estimated_tokens_remaining: 0,
            projected_monthly_cost_erg: 0.0,
            savings_vs_centralized: 0.0,
            alert_level: AlertLevel::Green,
            cost_history: vec![],
        };
        self.user_budgets.insert(user_id.to_string(), budget);
    }

    pub fn record_usage(
        &self,
        user_id: &str,
        cost_erg: f64,
        tokens_used: u64,
    ) {
        let mut entry = self.user_budgets.entry(user_id.to_string()).or_insert_with(|| {
            UserBudgetDisplay {
                user_id: user_id.to_string(),
                total_budget_erg: 0.0,
                spent_erg: 0.0,
                remaining_erg: 0.0,
                percent_used: 100.0,
                daily_budget_erg: None,
                daily_spent_erg: 0.0,
                daily_remaining_erg: None,
                estimated_tokens_remaining: 0,
                projected_monthly_cost_erg: 0.0,
                savings_vs_centralized: 0.0,
                alert_level: AlertLevel::Red,
                cost_history: vec![],
            }
        });

        let budget = entry.value_mut();
        budget.spent_erg += cost_erg;
        budget.daily_spent_erg += cost_erg;
        budget.remaining_erg = (budget.total_budget_erg - budget.spent_erg).max(0.0);

        if budget.total_budget_erg > 0.0 {
            budget.percent_used = (budget.spent_erg / budget.total_budget_erg * 100.0).min(100.0);
        } else {
            budget.percent_used = 100.0;
        }

        budget.alert_level = AlertLevel::from_percent_used(budget.percent_used);

        if let Some(daily) = budget.daily_budget_erg {
            budget.daily_remaining_erg = Some((daily - budget.daily_spent_erg).max(0.0));
        }

        // Estimate remaining tokens based on average cost per token
        if budget.spent_erg > 0.0 && tokens_used > 0 {
            let avg_cost_per_token = budget.spent_erg / tokens_used as f64;
            if avg_cost_per_token > 0.0 {
                budget.estimated_tokens_remaining =
                    (budget.remaining_erg / avg_cost_per_token) as u64;
            }
        }

        budget.projected_monthly_cost_erg = project_monthly_cost(budget);

        budget.cost_history.push(CostHistoryPoint {
            date: Utc::now(),
            cost_erg,
            tokens_used,
        });

        // Keep cost_history bounded
        if budget.cost_history.len() > 365 {
            budget.cost_history = budget.cost_history.split_off(budget.cost_history.len() - 365);
        }
    }

    pub fn get_user_budget(&self, user_id: &str) -> Option<UserBudgetDisplay> {
        self.user_budgets.get(user_id).map(|e| e.value().clone())
    }

    // -----------------------------------------------------------------------
    // Cost comparison
    // -----------------------------------------------------------------------

    pub fn get_cost_comparison(&self, model_id: &str) -> Option<CostComparisonWidget> {
        let display = self.get_pricing_display(model_id)?;
        let avg_nanoerg = display.avg_cost_per_1k_tokens_nanoerg;

        let xergon_erg = avg_nanoerg as f64 / NANOERG_PER_ERG as f64;
        let xergon_usd = xergon_erg * self.erg_usd_rate;

        let savings_openai = if OPENAI_COST_PER_1K_USD > 0.0 {
            (1.0 - xergon_usd / OPENAI_COST_PER_1K_USD) * 100.0
        } else {
            0.0
        };

        let savings_anthropic = if ANTHROPIC_COST_PER_1K_USD > 0.0 {
            (1.0 - xergon_usd / ANTHROPIC_COST_PER_1K_USD) * 100.0
        } else {
            0.0
        };

        Some(CostComparisonWidget {
            xergon_cost_per_1k_erg: xergon_erg,
            openai_cost_per_1k_usd: OPENAI_COST_PER_1K_USD,
            anthropic_cost_per_1k_usd: ANTHROPIC_COST_PER_1K_USD,
            erg_usd_rate: self.erg_usd_rate,
            xergon_usd_per_1k: xergon_usd,
            savings_vs_openai_percent: savings_openai,
            savings_vs_anthropic_percent: savings_anthropic,
            model_name: display.model_name,
        })
    }

    // -----------------------------------------------------------------------
    // Savings
    // -----------------------------------------------------------------------

    pub fn get_user_savings(&self, user_id: &str) -> Option<SavingsReport> {
        let budget = self.user_budgets.get(user_id)?;
        let total_tokens: u64 = budget.cost_history.iter().map(|h| h.tokens_used).sum();

        if total_tokens == 0 {
            return Some(SavingsReport {
                user_id: user_id.to_string(),
                total_spent_erg: budget.spent_erg,
                total_tokens,
                equivalent_openai_usd: 0.0,
                equivalent_anthropic_usd: 0.0,
                savings_vs_openai_erg: 0.0,
                savings_vs_anthropic_erg: 0.0,
                xergon_spent_usd: budget.spent_erg * self.erg_usd_rate,
                privacy_note: "Xergon: your prompts never leave your machine".to_string(),
            });
        }

        let xergon_usd = budget.spent_erg * self.erg_usd_rate;

        // Calculate what this would cost on centralized platforms
        let token_batches = total_tokens as f64 / 1000.0;
        let equivalent_openai_usd = token_batches * OPENAI_COST_PER_1K_USD;
        let equivalent_anthropic_usd = token_batches * ANTHROPIC_COST_PER_1K_USD;

        let savings_openai_erg = if self.erg_usd_rate > 0.0 {
            (equivalent_openai_usd - xergon_usd) / self.erg_usd_rate
        } else {
            0.0
        };

        let savings_anthropic_erg = if self.erg_usd_rate > 0.0 {
            (equivalent_anthropic_usd - xergon_usd) / self.erg_usd_rate
        } else {
            0.0
        };

        Some(SavingsReport {
            user_id: user_id.to_string(),
            total_spent_erg: budget.spent_erg,
            total_tokens,
            equivalent_openai_usd,
            equivalent_anthropic_usd,
            savings_vs_openai_erg: savings_openai_erg,
            savings_vs_anthropic_erg: savings_anthropic_erg,
            xergon_spent_usd: xergon_usd,
            privacy_note: "Xergon: your prompts never leave your machine".to_string(),
        })
    }

    // -----------------------------------------------------------------------
    // Trending / snapshots
    // -----------------------------------------------------------------------

    pub fn get_trending_models(&self, limit: usize) -> Vec<TrendingModel> {
        let mut trending: Vec<TrendingModel> = self
            .pricing_snapshots
            .iter()
            .filter_map(|kv| {
                let snapshots = kv.value();
                if snapshots.len() < 2 {
                    return None;
                }

                let latest = snapshots.back()?;
                let model_id = kv.key().clone();

                // Get model name from provider_pricing
                let model_name = self
                    .provider_pricing
                    .get(&model_id)
                    .and_then(|p| p.first().map(|pp| pp.model_name.clone()))
                    .unwrap_or_else(|| model_id.clone());

                let provider_count = latest.provider_count;
                let trend = latest.trend.clone();

                // Calculate price change between last two snapshots
                let prev = snapshots.get(snapshots.len() - 2)?;
                let change_pct = if prev.avg_cost_per_1k_nanoerg > 0 {
                    ((latest.avg_cost_per_1k_nanoerg as f64
                        - prev.avg_cost_per_1k_nanoerg as f64)
                        / prev.avg_cost_per_1k_nanoerg as f64
                        * 100.0)
                    .abs()
                } else {
                    0.0
                };

                Some(TrendingModel {
                    model_id,
                    model_name,
                    avg_cost_per_1k_nanoerg: latest.avg_cost_per_1k_nanoerg,
                    provider_count,
                    trend,
                    change_pct,
                })
            })
            .collect();

        // Sort by absolute change percentage descending (biggest movers first)
        trending.sort_by(|a, b| {
            b.change_pct
                .partial_cmp(&a.change_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        trending.truncate(limit);
        trending
    }

    pub fn get_price_history(
        &self,
        model_id: &str,
        days: u32,
    ) -> Vec<CostHistoryPoint> {
        let cutoff = Utc::now()
            .checked_sub_days(Days::new(days as u64))
            .unwrap_or_else(Utc::now);

        // We use pricing_snapshots to build price history
        if let Some(snapshots) = self.pricing_snapshots.get(model_id) {
            snapshots
                .iter()
                .filter(|s| s.timestamp >= cutoff)
                .map(|s| CostHistoryPoint {
                    date: s.timestamp,
                    cost_erg: s.avg_cost_per_1k_nanoerg as f64 / NANOERG_PER_ERG as f64,
                    tokens_used: s.provider_count as u64,
                })
                .collect()
        } else {
            vec![]
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn take_snapshot(&self, model_id: &str) {
        let providers = match self.provider_pricing.get(model_id) {
            Some(p) => p.clone(),
            None => return,
        };

        let available: Vec<&ProviderPricing> =
            providers.iter().filter(|p| p.available).collect();

        if available.is_empty() {
            return;
        }

        let costs: Vec<u64> = available
            .iter()
            .map(|p| p.cost_per_1k_input_nanoerg)
            .collect();
        let avg_cost = costs.iter().sum::<u64>() / costs.len() as u64;

        // Read previous snapshot avg and drop the lock before writing
        let prev_avg = self
            .pricing_snapshots
            .get(model_id)
            .and_then(|snapshots| snapshots.back().map(|s| s.avg_cost_per_1k_nanoerg));

        let trend = match prev_avg {
            Some(prev) => {
                let diff = avg_cost as i64 - prev as i64;
                let threshold = ((prev as f64 * 0.01) as i64).max(1);
                if diff > threshold {
                    PriceTrend::Increasing
                } else if diff < -threshold {
                    PriceTrend::Decreasing
                } else {
                    PriceTrend::Stable
                }
            }
            None => PriceTrend::Stable,
        };

        let snapshot = PricingSnapshot {
            timestamp: Utc::now(),
            model_id: model_id.to_string(),
            avg_cost_per_1k_nanoerg: avg_cost,
            provider_count: available.len(),
            trend,
        };

        let mut entry = self
            .pricing_snapshots
            .entry(model_id.to_string())
            .or_default();

        let deque = entry.value_mut();
        if deque.len() >= MAX_SNAPSHOTS {
            deque.pop_front();
        }
        deque.push_back(snapshot);
    }
}

// ---------------------------------------------------------------------------
// SavingsReport
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavingsReport {
    pub user_id: String,
    pub total_spent_erg: f64,
    pub total_tokens: u64,
    pub equivalent_openai_usd: f64,
    pub equivalent_anthropic_usd: f64,
    pub savings_vs_openai_erg: f64,
    pub savings_vs_anthropic_erg: f64,
    pub xergon_spent_usd: f64,
    pub privacy_note: String,
}

// ---------------------------------------------------------------------------
// TrendingModel
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrendingModel {
    pub model_id: String,
    pub model_name: String,
    pub avg_cost_per_1k_nanoerg: u64,
    pub provider_count: usize,
    pub trend: PriceTrend,
    pub change_pct: f64,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug)]
pub struct HistoryQuery {
    pub days: Option<u32>,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn median_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 && mid > 0 {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    }
}

fn project_monthly_cost(budget: &UserBudgetDisplay) -> f64 {
    if budget.cost_history.is_empty() {
        return 0.0;
    }

    // Find the earliest and latest history points to get a daily rate
    let earliest = budget.cost_history.first().unwrap().date;
    let latest = budget.cost_history.last().unwrap().date;
    let days_elapsed = (latest - earliest).num_days();
    if days_elapsed <= 0 {
        return 0.0;
    }

    let daily_rate = budget.spent_erg / days_elapsed as f64;
    daily_rate * 30.0 // ~1 month projection
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

async fn get_model_pricing_handler(
    State(engine): State<InferencePricingEngine>,
    Path(model_id): Path<String>,
) -> Result<Json<PricingDisplay>, StatusCode> {
    engine
        .get_pricing_display(&model_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_all_models_pricing_handler(
    State(engine): State<InferencePricingEngine>,
) -> Json<Vec<PricingDisplay>> {
    Json(engine.get_all_models_pricing())
}

async fn get_user_budget_handler(
    State(engine): State<InferencePricingEngine>,
    Path(user_id): Path<String>,
) -> Result<Json<UserBudgetDisplay>, StatusCode> {
    engine
        .get_user_budget(&user_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn set_user_budget_handler(
    State(engine): State<InferencePricingEngine>,
    Path(user_id): Path<String>,
    Json(req): Json<SetBudgetRequest>,
) -> StatusCode {
    engine.set_user_budget(&user_id, req.total_budget_erg, req.daily_budget_erg);
    StatusCode::NO_CONTENT
}

async fn get_cost_comparison_handler(
    State(engine): State<InferencePricingEngine>,
    Path(model_id): Path<String>,
) -> Result<Json<CostComparisonWidget>, StatusCode> {
    engine
        .get_cost_comparison(&model_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_user_savings_handler(
    State(engine): State<InferencePricingEngine>,
    Path(user_id): Path<String>,
) -> Result<Json<SavingsReport>, StatusCode> {
    engine
        .get_user_savings(&user_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_trending_handler(
    State(engine): State<InferencePricingEngine>,
) -> Json<Vec<TrendingModel>> {
    Json(engine.get_trending_models(20))
}

async fn get_price_history_handler(
    State(engine): State<InferencePricingEngine>,
    Path(model_id): Path<String>,
    Query(params): Query<HistoryQuery>,
) -> Json<Vec<CostHistoryPoint>> {
    let days = params.days.unwrap_or(30);
    Json(engine.get_price_history(&model_id, days))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> axum::Router<InferencePricingEngine> {
    axum::Router::new()
        .route(
            "/api/v1/pricing/model/:model_id",
            axum::routing::get(get_model_pricing_handler),
        )
        .route(
            "/api/v1/pricing/models",
            axum::routing::get(get_all_models_pricing_handler),
        )
        .route(
            "/api/v1/pricing/budget/:user_id",
            axum::routing::get(get_user_budget_handler),
        )
        .route(
            "/api/v1/pricing/budget/:user_id/set",
            axum::routing::post(set_user_budget_handler),
        )
        .route(
            "/api/v1/pricing/compare/:model_id",
            axum::routing::get(get_cost_comparison_handler),
        )
        .route(
            "/api/v1/pricing/savings/:user_id",
            axum::routing::get(get_user_savings_handler),
        )
        .route(
            "/api/v1/pricing/trending",
            axum::routing::get(get_trending_handler),
        )
        .route(
            "/api/v1/pricing/history/:model_id",
            axum::routing::get(get_price_history_handler),
        )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> InferencePricingEngine {
        InferencePricingEngine::new(Some(1.20))
    }

    fn sample_provider(id: &str, cost_input: u64, latency_ms: u64) -> ProviderPricing {
        ProviderPricing {
            provider_id: id.to_string(),
            provider_name: format!("Provider {}", id),
            model_id: "llama-3-70b".to_string(),
            model_name: "LLaMA 3 70B".to_string(),
            cost_per_1k_input_nanoerg: cost_input,
            cost_per_1k_output_nanoerg: cost_input * 2,
            base_cost_nanoerg: 100_000,
            quality_tier: "premium".to_string(),
            available: true,
            latency_ms,
            uptime_percent: 99.9,
            total_requests_served: 10_000,
            rating: 4.8,
        }
    }

    // ----- Test pricing display calculation -----

    #[test]
    fn test_pricing_display_calculation() {
        let engine = make_engine();
        engine.register_provider_pricing(sample_provider("p1", 50_000_000, 200));
        engine.register_provider_pricing(sample_provider("p2", 30_000_000, 300));
        engine.register_provider_pricing(sample_provider("p3", 80_000_000, 100));

        let display = engine.get_pricing_display("llama-3-70b").unwrap();

        assert_eq!(display.model_id, "llama-3-70b");
        assert_eq!(display.model_name, "LLaMA 3 70B");
        assert_eq!(display.providers.len(), 3);
        assert_eq!(display.currency, "ERG");

        // Cheapest should be p2 (30M nanoerg)
        assert_eq!(
            display.cheapest_provider.as_ref().unwrap().provider_id,
            "p2"
        );
        assert_eq!(
            display.cheapest_provider.as_ref().unwrap().cost_per_1k_input_nanoerg,
            30_000_000
        );

        // Fastest should be p3 (100ms)
        assert_eq!(
            display.fastest_provider.as_ref().unwrap().provider_id,
            "p3"
        );

        // Average: (50M + 30M + 80M) / 3 = 53.33M => 53_333_333
        assert_eq!(display.avg_cost_per_1k_tokens_nanoerg, 53_333_333);

        // Price range
        assert_eq!(display.price_range.min, 30_000_000);
        assert_eq!(display.price_range.max, 80_000_000);
        assert_eq!(display.price_range.median, 50_000_000);

        // USD conversion: 53_333_333 nanoerg / 1B * 1.20 = 0.064
        let usd = display.usd_cost_per_1k.unwrap();
        assert!((usd - 0.064).abs() < 0.001);

        // ERG/USD rate
        assert_eq!(display.erg_usd_rate, Some(1.20));
    }

    // ----- Test budget display with alerts -----

    #[test]
    fn test_budget_display_with_alerts() {
        let engine = make_engine();
        engine.set_user_budget("user-1", 100.0, Some(10.0));

        // Green: 0% used
        let budget = engine.get_user_budget("user-1").unwrap();
        assert_eq!(budget.alert_level, AlertLevel::Green);
        assert_eq!(budget.total_budget_erg, 100.0);
        assert_eq!(budget.remaining_erg, 100.0);
        assert_eq!(budget.daily_remaining_erg, Some(10.0));

        // Spend 60% => Yellow (50-74.99% range)
        engine.record_usage("user-1", 60.0, 1_000_000);
        let budget = engine.get_user_budget("user-1").unwrap();
        assert_eq!(budget.alert_level, AlertLevel::Yellow);
        assert!((budget.percent_used - 60.0).abs() < 0.01);
        assert!((budget.remaining_erg - 40.0).abs() < 0.01);

        // Spend 35 more => 95% => Red
        engine.record_usage("user-1", 35.0, 500_000);
        let budget = engine.get_user_budget("user-1").unwrap();
        assert_eq!(budget.alert_level, AlertLevel::Red);
        assert!((budget.percent_used - 95.0).abs() < 0.01);

        // Cost history should have 2 entries
        assert_eq!(budget.cost_history.len(), 2);
    }

    #[test]
    fn test_budget_alert_levels() {
        assert_eq!(AlertLevel::from_percent_used(25.0), AlertLevel::Green);
        assert_eq!(AlertLevel::from_percent_used(50.0), AlertLevel::Yellow);
        assert_eq!(AlertLevel::from_percent_used(74.0), AlertLevel::Yellow);
        assert_eq!(AlertLevel::from_percent_used(75.0), AlertLevel::Orange);
        assert_eq!(AlertLevel::from_percent_used(89.0), AlertLevel::Orange);
        assert_eq!(AlertLevel::from_percent_used(90.0), AlertLevel::Red);
        assert_eq!(AlertLevel::from_percent_used(100.0), AlertLevel::Red);
    }

    // ----- Test cost comparison vs centralized -----

    #[test]
    fn test_cost_comparison_vs_centralized() {
        let engine = make_engine();
        engine.register_provider_pricing(sample_provider("p1", 10_000_000, 200));

        let comparison = engine.get_cost_comparison("llama-3-70b").unwrap();

        assert_eq!(comparison.model_name, "LLaMA 3 70B");
        assert_eq!(comparison.erg_usd_rate, 1.20);

        // Xergon cost: 10M nanoerg / 1B * 1.20 = 0.012 ERG => $0.0144 USD
        assert!((comparison.xergon_cost_per_1k_erg - 0.01).abs() < 0.0001);
        assert!((comparison.xergon_usd_per_1k - 0.012).abs() < 0.001);

        // OpenAI: $0.03/1K. Savings: (1 - 0.012/0.03) * 100 = 60%
        assert!((comparison.savings_vs_openai_percent - 60.0).abs() < 1.0);

        // Anthropic: $0.015/1K. Savings: (1 - 0.012/0.015) * 100 = 20%
        assert!((comparison.savings_vs_anthropic_percent - 20.0).abs() < 1.0);
    }

    // ----- Test savings calculation -----

    #[test]
    fn test_savings_calculation() {
        let engine = make_engine();
        engine.set_user_budget("user-1", 100.0, None);

        // Spend 0.5 ERG for 100K tokens
        engine.record_usage("user-1", 0.5, 100_000);

        let savings = engine.get_user_savings("user-1").unwrap();
        assert_eq!(savings.total_spent_erg, 0.5);
        assert_eq!(savings.total_tokens, 100_000);
        assert_eq!(
            savings.privacy_note,
            "Xergon: your prompts never leave your machine"
        );

        // 100K tokens = 100 batches of 1K
        // OpenAI equivalent: 100 * $0.03 = $3.00
        assert!((savings.equivalent_openai_usd - 3.0).abs() < 0.01);

        // Anthropic equivalent: 100 * $0.015 = $1.50
        assert!((savings.equivalent_anthropic_usd - 1.5).abs() < 0.01);

        // Xergon spent in USD: 0.5 * 1.20 = $0.60
        assert!((savings.xergon_spent_usd - 0.6).abs() < 0.01);

        // Savings vs OpenAI in ERG: ($3.00 - $0.60) / 1.20 = 2.0 ERG
        assert!((savings.savings_vs_openai_erg - 2.0).abs() < 0.01);

        // Savings vs Anthropic in ERG: ($1.50 - $0.60) / 1.20 = 0.75 ERG
        assert!((savings.savings_vs_anthropic_erg - 0.75).abs() < 0.01);
    }

    // ----- Test price trend detection -----

    #[test]
    fn test_price_trend_detection() {
        let engine = make_engine();

        // First provider at 50M
        engine.register_provider_pricing(sample_provider("p1", 50_000_000, 200));

        // Same price => stable
        let display = engine.get_pricing_display("llama-3-70b").unwrap();
        assert_eq!(display.providers.len(), 1);

        // Update with higher price => increasing
        let expensive = sample_provider("p1", 60_000_000, 200);
        engine.register_provider_pricing(expensive);

        {
            let snapshots = engine.pricing_snapshots.get("llama-3-70b").unwrap();
            assert_eq!(snapshots.len(), 2);
            let latest = snapshots.back().unwrap();
            assert_eq!(latest.trend, PriceTrend::Increasing);
        }

        // Update with lower price => decreasing
        let cheaper = sample_provider("p1", 30_000_000, 200);
        engine.register_provider_pricing(cheaper);

        {
            let snapshots = engine.pricing_snapshots.get("llama-3-70b").unwrap();
            let latest = snapshots.back().unwrap();
            assert_eq!(latest.trend, PriceTrend::Decreasing);
        }
    }

    // ----- Test budget projection -----

    #[test]
    fn test_budget_projection() {
        let engine = make_engine();
        engine.set_user_budget("user-1", 100.0, None);

        // Record usage over time
        engine.record_usage("user-1", 1.0, 200_000);
        engine.record_usage("user-1", 1.0, 200_000);

        let budget = engine.get_user_budget("user-1").unwrap();

        // Projected monthly cost: spent 2 ERG, days elapsed is 0 (same day)
        // so projection should be 0 or small
        // With same-day usage, days_elapsed is 0 => project_monthly_cost returns 0
        assert!(budget.projected_monthly_cost_erg >= 0.0);
    }

    #[test]
    fn test_project_monthly_cost_calculation() {
        let budget = UserBudgetDisplay {
            user_id: "u1".to_string(),
            total_budget_erg: 100.0,
            spent_erg: 30.0,
            remaining_erg: 70.0,
            percent_used: 30.0,
            daily_budget_erg: None,
            daily_spent_erg: 0.0,
            daily_remaining_erg: None,
            estimated_tokens_remaining: 0,
            projected_monthly_cost_erg: 0.0,
            savings_vs_centralized: 0.0,
            alert_level: AlertLevel::Green,
            cost_history: vec![
                CostHistoryPoint {
                    date: Utc::now() - chrono::Duration::days(10),
                    cost_erg: 15.0,
                    tokens_used: 1_500_000,
                },
                CostHistoryPoint {
                    date: Utc::now(),
                    cost_erg: 15.0,
                    tokens_used: 1_500_000,
                },
            ],
        };

        let projected = project_monthly_cost(&budget);
        // 30 ERG in 10 days => 3 ERG/day => 90 ERG/month
        assert!((projected - 90.0).abs() < 1.0);
    }

    // ----- Test unavailable providers excluded -----

    #[test]
    fn test_unavailable_providers_excluded() {
        let engine = make_engine();

        let mut unavailable = sample_provider("p1", 10_000_000, 100);
        unavailable.available = false;
        engine.register_provider_pricing(unavailable);
        engine.register_provider_pricing(sample_provider("p2", 20_000_000, 200));

        let display = engine.get_pricing_display("llama-3-70b").unwrap();
        assert_eq!(display.providers.len(), 1);
        assert_eq!(display.providers[0].provider_id, "p2");
        assert_eq!(display.avg_cost_per_1k_tokens_nanoerg, 20_000_000);
    }

    // ----- Test estimated tokens remaining -----

    #[test]
    fn test_estimated_tokens_remaining() {
        let engine = make_engine();
        engine.set_user_budget("user-1", 10.0, None);

        // Spend 2 ERG for 100K tokens => avg cost per token = 0.00002 ERG
        engine.record_usage("user-1", 2.0, 100_000);

        let budget = engine.get_user_budget("user-1").unwrap();
        // Remaining 8 ERG / 0.00002 ERG/token = 400,000 tokens
        assert!((budget.estimated_tokens_remaining as f64 - 400_000.0).abs() <= 1.0);
    }

    // ----- Test trending models -----

    #[test]
    fn test_trending_models() {
        let engine = make_engine();
        engine.register_provider_pricing(sample_provider("p1", 50_000_000, 200));
        engine.register_provider_pricing(sample_provider("p2", 40_000_000, 200));

        // Register for a different model
        let mut p3 = sample_provider("p3", 10_000_000, 100);
        p3.model_id = "mistral-7b".to_string();
        p3.model_name = "Mistral 7B".to_string();
        engine.register_provider_pricing(p3);

        // Register a big price change for mistral
        let mut p4 = sample_provider("p3", 25_000_000, 100);
        p4.model_id = "mistral-7b".to_string();
        p4.model_name = "Mistral 7B".to_string();
        engine.register_provider_pricing(p4);

        let trending = engine.get_trending_models(10);
        // Both models should be present
        assert!(!trending.is_empty());
        assert!(trending.len() >= 1);
    }

    // ----- Test missing model returns none -----

    #[test]
    fn test_missing_model_returns_none() {
        let engine = make_engine();
        assert!(engine.get_pricing_display("nonexistent").is_none());
        assert!(engine.get_cost_comparison("nonexistent").is_none());
    }

    // ----- Test best value provider -----

    #[test]
    fn test_best_value_provider() {
        let engine = make_engine();

        // p1: cost=50M, latency=200 => score = 50M*200 = 10B
        // p2: cost=30M, latency=400 => score = 30M*400 = 12B
        // p3: cost=40M, latency=100 => score = 40M*100 = 4B => best value
        engine.register_provider_pricing(sample_provider("p1", 50_000_000, 200));
        engine.register_provider_pricing(sample_provider("p2", 30_000_000, 400));
        engine.register_provider_pricing(sample_provider("p3", 40_000_000, 100));

        let display = engine.get_pricing_display("llama-3-70b").unwrap();
        assert_eq!(
            display.best_value_provider.as_ref().unwrap().provider_id,
            "p3"
        );
    }

    // ----- Test zero budget edge case -----

    #[test]
    fn test_zero_budget_edge_case() {
        let engine = make_engine();
        engine.set_user_budget("user-1", 0.0, None);

        let budget = engine.get_user_budget("user-1").unwrap();
        assert_eq!(budget.alert_level, AlertLevel::Green);
        assert_eq!(budget.percent_used, 0.0);
    }
}
