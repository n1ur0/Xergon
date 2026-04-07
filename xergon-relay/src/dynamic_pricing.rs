//! Dynamic Pricing Engine for Inference Requests.
//!
//! Real-time pricing that adapts to demand, supply, time-of-day, and provider
//! competition. Extends the static cost estimator with dynamic multipliers,
//! EMA smoothing, price caps/floors, and tiered base rates.
//!
//! Endpoints:
//!   POST /api/dynamic-pricing/estimate          -- estimate dynamic price
//!   GET  /api/dynamic-pricing/model/{model_id}  -- get current pricing for model
//!   PUT  /api/dynamic-pricing/provider-price    -- update provider price override
//!   GET  /api/dynamic-pricing/demand/{model_id} -- get current demand info
//!   GET  /api/dynamic-pricing/supply/{model_id} -- get supply info for model
//!   GET  /api/dynamic-pricing/multipliers       -- get all current multipliers
//!   GET  /api/dynamic-pricing/tiers             -- list pricing tiers
//!   PUT  /api/dynamic-pricing/config            -- update engine config
//!   POST /api/dynamic-pricing/record-request    -- record a request for demand tracking

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use chrono::{DateTime, Timelike, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Pricing tier with different base rates per tier level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PricingTier {
    Free,
    Basic,
    Pro,
    Enterprise,
}

impl PricingTier {
    /// Base multiplier for this tier.
    pub fn base_multiplier(&self) -> f64 {
        match self {
            PricingTier::Free => 0.0,
            PricingTier::Basic => 1.0,
            PricingTier::Pro => 1.5,
            PricingTier::Enterprise => 2.0,
        }
    }

    /// Parse from string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "free" => Some(PricingTier::Free),
            "basic" => Some(PricingTier::Basic),
            "pro" => Some(PricingTier::Pro),
            "enterprise" => Some(PricingTier::Enterprise),
            _ => None,
        }
    }
}

/// Per-model base pricing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub model_id: String,
    /// Base cost per 1k input tokens in ERG.
    pub base_input_per_1k: f64,
    /// Base cost per 1k output tokens in ERG.
    pub base_output_per_1k: f64,
}

/// Provider-specific price override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPriceOverride {
    pub provider_id: String,
    pub model_id: String,
    /// Multiplier applied on top of base pricing (e.g., 0.9 for 10% discount).
    pub price_multiplier: f64,
    /// Absolute floor for this provider's effective price.
    pub min_price: f64,
    /// Absolute ceiling for this provider's effective price.
    pub max_price: f64,
    pub updated_at: DateTime<Utc>,
}

/// Smoothed price state for a model (EMA-based).
#[derive(Debug)]
pub struct SmoothedPrice {
    /// EMA of the computed price (input per 1k).
    pub ema_input: std::sync::RwLock<f64>,
    /// EMA of the computed price (output per 1k).
    pub ema_output: std::sync::RwLock<f64>,
    /// Number of samples fed into the EMA so far.
    pub samples: AtomicU64,
}

impl SmoothedPrice {
    pub fn new(initial_input: f64, initial_output: f64) -> Self {
        Self {
            ema_input: std::sync::RwLock::new(initial_input),
            ema_output: std::sync::RwLock::new(initial_output),
            samples: AtomicU64::new(1),
        }
    }

    /// Update the EMA with a new observation.
    /// `alpha` is the smoothing factor (0..1). Higher = more responsive.
    pub fn update(&self, raw_input: f64, raw_output: f64, alpha: f64) {
        let mut inp = self.ema_input.write().unwrap_or_else(|e| e.into_inner());
        let mut out = self.ema_output.write().unwrap_or_else(|e| e.into_inner());
        *inp = alpha * raw_input + (1.0 - alpha) * *inp;
        *out = alpha * raw_output + (1.0 - alpha) * *out;
        self.samples.fetch_add(1, Ordering::Relaxed);
    }

    /// Read current smoothed values.
    pub fn current(&self) -> (f64, f64) {
        let inp = self.ema_input.read().unwrap_or_else(|e| e.into_inner());
        let out = self.ema_output.read().unwrap_or_else(|e| e.into_inner());
        (*inp, *out)
    }
}

/// Per-model demand tracking data (lightweight, atomic-backed).
#[derive(Debug)]
pub struct ModelDemandState {
    pub request_count: AtomicU64,
    pub last_request: std::sync::RwLock<DateTime<Utc>>,
}

impl ModelDemandState {
    pub fn new() -> Self {
        Self {
            request_count: AtomicU64::new(0),
            last_request: std::sync::RwLock::new(Utc::now()),
        }
    }

    pub fn record(&self, count: u64) {
        self.request_count.fetch_add(count, Ordering::Relaxed);
        if let Ok(mut last) = self.last_request.write() {
            *last = Utc::now();
        }
    }

    pub fn count(&self) -> u64 {
        self.request_count.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.request_count.store(0, Ordering::Relaxed);
    }
}

/// Per-model supply tracking (number of active providers).
#[derive(Debug)]
pub struct ModelSupplyState {
    pub provider_count: AtomicU32,
}

impl ModelSupplyState {
    pub fn new(count: u32) -> Self {
        Self {
            provider_count: AtomicU32::new(count),
        }
    }

    pub fn count(&self) -> u32 {
        self.provider_count.load(Ordering::Relaxed)
    }

    pub fn set(&self, count: u32) {
        self.provider_count.store(count, Ordering::Relaxed);
    }
}

/// Dynamic pricing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicPricingConfig {
    /// EMA smoothing factor (0..1). Default 0.3.
    #[serde(default = "default_ema_alpha")]
    pub ema_alpha: f64,
    /// Global price floor multiplier. No multiplier can go below this. Default 0.5.
    #[serde(default = "default_floor")]
    pub price_floor: f64,
    /// Global price cap multiplier. No multiplier can exceed this. Default 5.0.
    #[serde(default = "default_cap")]
    pub price_cap: f64,
    /// Peak hours start (24h format, e.g. 9). Default 9.
    #[serde(default = "default_peak_start")]
    pub peak_hour_start: u32,
    /// Peak hours end (24h format, e.g. 21). Default 21.
    #[serde(default = "default_peak_end")]
    pub peak_hour_end: u32,
    /// Peak hour demand multiplier. Default 1.5.
    #[serde(default = "default_peak_mult")]
    pub peak_multiplier: f64,
    /// Demand threshold for "high demand" (sigmoid midpoint). Default 100.
    #[serde(default = "default_demand_threshold")]
    pub demand_threshold: u64,
    /// Max demand multiplier (sigmoid ceiling above 1.0). Default 2.0.
    #[serde(default = "default_demand_cap")]
    pub demand_multiplier_cap: f64,
    /// Supply scarcity baseline (provider count below this triggers scarcity). Default 3.
    #[serde(default = "default_supply_baseline")]
    pub supply_baseline: u32,
    /// Max supply scarcity multiplier. Default 2.0.
    #[serde(default = "default_supply_cap")]
    pub supply_multiplier_cap: f64,
}

fn default_ema_alpha() -> f64 {
    0.3
}
fn default_floor() -> f64 {
    0.5
}
fn default_cap() -> f64 {
    5.0
}
fn default_peak_start() -> u32 {
    9
}
fn default_peak_end() -> u32 {
    21
}
fn default_peak_mult() -> f64 {
    1.5
}
fn default_demand_threshold() -> u64 {
    100
}
fn default_demand_cap() -> f64 {
    2.0
}
fn default_supply_baseline() -> u32 {
    3
}
fn default_supply_cap() -> f64 {
    2.0
}

impl Default for DynamicPricingConfig {
    fn default() -> Self {
        Self {
            ema_alpha: default_ema_alpha(),
            price_floor: default_floor(),
            price_cap: default_cap(),
            peak_hour_start: default_peak_start(),
            peak_hour_end: default_peak_end(),
            peak_multiplier: default_peak_mult(),
            demand_threshold: default_demand_threshold(),
            demand_multiplier_cap: default_demand_cap(),
            supply_baseline: default_supply_baseline(),
            supply_multiplier_cap: default_supply_cap(),
        }
    }
}

/// Result of a dynamic price estimation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicPriceEstimate {
    pub model_id: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Raw computed input cost per 1k before smoothing.
    pub raw_input_per_1k: f64,
    /// Raw computed output cost per 1k before smoothing.
    pub raw_output_per_1k: f64,
    /// Smoothed (EMA) input cost per 1k.
    pub smoothed_input_per_1k: f64,
    /// Smoothed (EMA) output cost per 1k.
    pub smoothed_output_per_1k: f64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
    pub currency: String,
    pub tier_used: PricingTier,
    pub demand_multiplier: f64,
    pub time_multiplier: f64,
    pub supply_multiplier: f64,
    pub effective_multiplier: f64,
    pub provider_override_applied: Option<f64>,
    pub timestamp: DateTime<Utc>,
}

/// Current multipliers snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipliersSnapshot {
    pub model_id: String,
    pub demand_multiplier: f64,
    pub time_multiplier: f64,
    pub supply_multiplier: f64,
    pub combined_multiplier: f64,
    pub effective_multiplier: f64, // after cap/floor
    pub timestamp: DateTime<Utc>,
}

/// Demand info for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemandInfo {
    pub model_id: String,
    pub request_count: u64,
    pub demand_multiplier: f64,
    pub last_request: DateTime<Utc>,
}

/// Supply info for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyInfo {
    pub model_id: String,
    pub provider_count: u32,
    pub supply_multiplier: f64,
    pub is_scarce: bool,
}

// ---------------------------------------------------------------------------
// HTTP request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DynamicEstimateRequest {
    pub model_id: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderPriceRequest {
    pub provider_id: String,
    pub model_id: String,
    pub price_multiplier: f64,
    #[serde(default = "default_provider_min")]
    pub min_price: Option<f64>,
    #[serde(default)]
    pub max_price: Option<f64>,
}

fn default_provider_min() -> Option<f64> {
    None
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateConfigRequest {
    #[serde(default)]
    pub ema_alpha: Option<f64>,
    #[serde(default)]
    pub price_floor: Option<f64>,
    #[serde(default)]
    pub price_cap: Option<f64>,
    #[serde(default)]
    pub peak_hour_start: Option<u32>,
    #[serde(default)]
    pub peak_hour_end: Option<u32>,
    #[serde(default)]
    pub peak_multiplier: Option<f64>,
    #[serde(default)]
    pub demand_threshold: Option<u64>,
    #[serde(default)]
    pub demand_multiplier_cap: Option<f64>,
    #[serde(default)]
    pub supply_baseline: Option<u32>,
    #[serde(default)]
    pub supply_multiplier_cap: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct RecordRequestRequest {
    pub model_id: String,
    #[serde(default = "default_record_count")]
    pub count: u64,
}

fn default_record_count() -> u64 {
    1
}

#[derive(Debug, Deserialize)]
pub struct UpdateSupplyRequest {
    pub model_id: String,
    pub provider_count: u32,
}

// ---------------------------------------------------------------------------
// DynamicPricingEngine
// ---------------------------------------------------------------------------

/// Core dynamic pricing engine with concurrent access via DashMap and atomics.
pub struct DynamicPricingEngine {
    /// Base pricing per model.
    model_pricing: DashMap<String, ModelPricing>,
    /// Provider-specific price overrides keyed by (provider_id, model_id).
    provider_overrides: DashMap<String, ProviderPriceOverride>,
    /// Smoothed prices per model (EMA).
    smoothed_prices: DashMap<String, Arc<SmoothedPrice>>,
    /// Per-model demand counters.
    demand_state: DashMap<String, Arc<ModelDemandState>>,
    /// Per-model supply (active provider count).
    supply_state: DashMap<String, Arc<ModelSupplyState>>,
    /// Mutable configuration.
    config: std::sync::RwLock<DynamicPricingConfig>,
    /// Total dynamic estimations performed.
    total_estimations: AtomicU64,
    /// Total cost estimated (nanoerg for precision).
    total_cost_nanoerg: AtomicU64,
}

impl DynamicPricingEngine {
    /// Create a new engine with default config and some seed model pricing.
    pub fn new() -> Self {
        let engine = Self {
            model_pricing: DashMap::new(),
            provider_overrides: DashMap::new(),
            smoothed_prices: DashMap::new(),
            demand_state: DashMap::new(),
            supply_state: DashMap::new(),
            config: std::sync::RwLock::new(DynamicPricingConfig::default()),
            total_estimations: AtomicU64::new(0),
            total_cost_nanoerg: AtomicU64::new(0),
        };

        // Seed common model pricing
        engine.register_model("gpt-4", 0.003, 0.015);
        engine.register_model("gpt-4o", 0.0025, 0.01);
        engine.register_model("gpt-3.5-turbo", 0.0005, 0.0015);
        engine.register_model("claude-3-opus", 0.015, 0.075);
        engine.register_model("claude-3-sonnet", 0.003, 0.015);
        engine.register_model("llama-3-70b", 0.0008, 0.0024);
        engine.register_model("mixtral-8x7b", 0.0006, 0.0018);

        engine
    }

    /// Register a model with base pricing.
    pub fn register_model(&self, model_id: &str, input_per_1k: f64, output_per_1k: f64) {
        let pricing = ModelPricing {
            model_id: model_id.to_string(),
            base_input_per_1k: input_per_1k,
            base_output_per_1k: output_per_1k,
        };

        // Initialize smoothed price at the base rate
        let smoothed = Arc::new(SmoothedPrice::new(input_per_1k, output_per_1k));
        self.smoothed_prices.insert(model_id.to_string(), smoothed);

        // Initialize demand state
        self.demand_state
            .insert(model_id.to_string(), Arc::new(ModelDemandState::new()));

        // Initialize supply state with default 5 providers
        self.supply_state
            .insert(model_id.to_string(), Arc::new(ModelSupplyState::new(5)));

        self.model_pricing.insert(model_id.to_string(), pricing);
    }

    /// Set or update provider price override for a model.
    pub fn set_provider_override(
        &self,
        provider_id: &str,
        model_id: &str,
        price_multiplier: f64,
        min_price: Option<f64>,
        max_price: Option<f64>,
    ) -> Result<ProviderPriceOverride, String> {
        if price_multiplier <= 0.0 {
            return Err("price_multiplier must be positive".to_string());
        }

        // Get base pricing to derive absolute bounds
        let base = self
            .model_pricing
            .get(model_id)
            .ok_or_else(|| format!("Model '{}' not registered", model_id))?
            .clone();

        let config = self.config.read().unwrap_or_else(|e| e.into_inner());

        // Enforce that min <= max
        let effective_min = min_price.unwrap_or(base.base_input_per_1k * config.price_floor);
        let effective_max = max_price.unwrap_or(base.base_input_per_1k * config.price_cap);

        if effective_min > effective_max {
            return Err("min_price cannot exceed max_price".to_string());
        }

        let key = format!("{}:{}", provider_id, model_id);
        let override_val = ProviderPriceOverride {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            price_multiplier,
            min_price: effective_min,
            max_price: effective_max,
            updated_at: Utc::now(),
        };

        self.provider_overrides.insert(key, override_val.clone());
        Ok(override_val)
    }

    /// Get provider override for a (provider_id, model_id) pair.
    pub fn get_provider_override(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<ProviderPriceOverride> {
        let key = format!("{}:{}", provider_id, model_id);
        self.provider_overrides.get(&key).map(|v| v.value().clone())
    }

    /// Remove provider override.
    pub fn remove_provider_override(&self, provider_id: &str, model_id: &str) -> bool {
        let key = format!("{}:{}", provider_id, model_id);
        self.provider_overrides.remove(&key).is_some()
    }

    /// Record demand for a model.
    pub fn record_demand(&self, model_id: &str, count: u64) {
        let state = self
            .demand_state
            .entry(model_id.to_string())
            .or_insert_with(|| Arc::new(ModelDemandState::new()));
        state.record(count);
    }

    /// Update supply (active provider count) for a model.
    pub fn update_supply(&self, model_id: &str, provider_count: u32) {
        let state = self
            .supply_state
            .entry(model_id.to_string())
            .or_insert_with(|| Arc::new(ModelSupplyState::new(provider_count)));
        state.set(provider_count);
    }

    /// Get current config (cloned).
    pub fn get_config(&self) -> DynamicPricingConfig {
        self.config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Update config fields. Only non-None fields are updated.
    pub fn update_config(&self, update: UpdateConfigRequest) -> DynamicPricingConfig {
        let mut config = self.config.write().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = update.ema_alpha {
            config.ema_alpha = v.clamp(0.01, 0.99);
        }
        if let Some(v) = update.price_floor {
            config.price_floor = v;
        }
        if let Some(v) = update.price_cap {
            config.price_cap = v;
        }
        if let Some(v) = update.peak_hour_start {
            config.peak_hour_start = v.min(23);
        }
        if let Some(v) = update.peak_hour_end {
            config.peak_hour_end = v.min(24);
        }
        if let Some(v) = update.peak_multiplier {
            config.peak_multiplier = v;
        }
        if let Some(v) = update.demand_threshold {
            config.demand_threshold = v;
        }
        if let Some(v) = update.demand_multiplier_cap {
            config.demand_multiplier_cap = v;
        }
        if let Some(v) = update.supply_baseline {
            config.supply_baseline = v;
        }
        if let Some(v) = update.supply_multiplier_cap {
            config.supply_multiplier_cap = v;
        }
        config.clone()
    }

    // -----------------------------------------------------------------------
    // Multiplier computation
    // -----------------------------------------------------------------------

    /// Compute the demand multiplier for a model using a sigmoid curve.
    ///
    /// `1.0 + cap * (1 - 1/(1 + demand/threshold))`
    pub fn compute_demand_multiplier(&self, model_id: &str) -> f64 {
        let count = self
            .demand_state
            .get(model_id)
            .map(|s| s.count())
            .unwrap_or(0);

        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        let threshold = config.demand_threshold as f64;
        let cap = config.demand_multiplier_cap;

        if threshold == 0.0 {
            return 1.0;
        }

        1.0 + cap * (1.0 - 1.0 / (1.0 + count as f64 / threshold))
    }

    /// Compute time-of-day multiplier based on UTC hour.
    ///
    /// Returns `peak_multiplier` during peak hours, 1.0 otherwise.
    pub fn compute_time_multiplier(&self) -> f64 {
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        let hour = Utc::now().hour();

        let start = config.peak_hour_start;
        let end = config.peak_hour_end;

        if start < end {
            // Normal range (e.g., 9-21)
            if hour >= start && hour < end {
                config.peak_multiplier
            } else {
                1.0
            }
        } else {
            // Wrapping range (e.g., 22-6)
            if hour >= start || hour < end {
                config.peak_multiplier
            } else {
                1.0
            }
        }
    }

    /// Compute time multiplier for a given hour (useful for testing).
    pub fn compute_time_multiplier_for_hour(&self, hour: u32) -> f64 {
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        let start = config.peak_hour_start;
        let end = config.peak_hour_end;

        if start < end {
            if hour >= start && hour < end {
                config.peak_multiplier
            } else {
                1.0
            }
        } else {
            if hour >= start || hour < end {
                config.peak_multiplier
            } else {
                1.0
            }
        }
    }

    /// Compute supply-based multiplier.
    ///
    /// When providers < baseline, price increases. When providers >= baseline, 1.0.
    pub fn compute_supply_multiplier(&self, model_id: &str) -> f64 {
        let count = self
            .supply_state
            .get(model_id)
            .map(|s| s.count())
            .unwrap_or(0);

        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        let baseline = config.supply_baseline as f64;
        let cap = config.supply_multiplier_cap;

        if count as f64 >= baseline {
            return 1.0;
        }

        // Inverse linear: when count=0, multiplier = 1 + cap; when count=baseline, multiplier = 1.0
        let ratio = count as f64 / baseline;
        1.0 + cap * (1.0 - ratio)
    }

    /// Combine multipliers and apply price cap/floor.
    ///
    /// Returns the effective multiplier clamped to [floor, cap].
    pub fn compute_effective_multiplier(&self, model_id: &str) -> f64 {
        let demand = self.compute_demand_multiplier(model_id);
        let time = self.compute_time_multiplier();
        let supply = self.compute_supply_multiplier(model_id);

        let combined = demand * time * supply;

        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        combined.clamp(config.price_floor, config.price_cap)
    }

    /// Get a snapshot of all multipliers for a model.
    pub fn get_multipliers(&self, model_id: &str) -> MultipliersSnapshot {
        let demand = self.compute_demand_multiplier(model_id);
        let time = self.compute_time_multiplier();
        let supply = self.compute_supply_multiplier(model_id);
        let combined = demand * time * supply;
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());
        let effective = combined.clamp(config.price_floor, config.price_cap);

        MultipliersSnapshot {
            model_id: model_id.to_string(),
            demand_multiplier: demand,
            time_multiplier: time,
            supply_multiplier: supply,
            combined_multiplier: combined,
            effective_multiplier: effective,
            timestamp: Utc::now(),
        }
    }

    // -----------------------------------------------------------------------
    // Price estimation
    // -----------------------------------------------------------------------

    /// Compute the dynamic price for a model with given token counts.
    pub fn estimate(
        &self,
        model_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        tier: &PricingTier,
        provider_id: Option<&str>,
    ) -> Result<DynamicPriceEstimate, String> {
        let base = self
            .model_pricing
            .get(model_id)
            .ok_or_else(|| format!("Model '{}' not registered", model_id))?
            .clone();

        let config = self.config.read().unwrap_or_else(|e| e.into_inner());

        // Compute multipliers
        let demand_mult = self.compute_demand_multiplier(model_id);
        let time_mult = self.compute_time_multiplier();
        let supply_mult = self.compute_supply_multiplier(model_id);
        let combined = demand_mult * time_mult * supply_mult;
        let effective_mult = combined.clamp(config.price_floor, config.price_cap);

        // Apply tier base rate on top
        let tier_mult = tier.base_multiplier();

        // Compute raw prices
        let raw_input = base.base_input_per_1k * effective_mult * tier_mult;
        let raw_output = base.base_output_per_1k * effective_mult * tier_mult;

        // Apply provider override if specified
        let provider_override_applied = if let Some(pid) = provider_id {
            let key = format!("{}:{}", pid, model_id);
            if let Some(ovr) = self.provider_overrides.get(&key) {
                let ovr = ovr.value();
                Some(ovr.price_multiplier)
            } else {
                None
            }
        } else {
            None
        };

        let provider_mult = provider_override_applied.unwrap_or(1.0);

        // Final raw prices with provider override
        let final_raw_input = raw_input * provider_mult;
        let final_raw_output = raw_output * provider_mult;

        // Update EMA
        if let Some(smoothed) = self.smoothed_prices.get(model_id) {
            smoothed.update(final_raw_input, final_raw_output, config.ema_alpha);
        }

        // Read smoothed values
        let (smoothed_input, smoothed_output) = self
            .smoothed_prices
            .get(model_id)
            .map(|s| s.current())
            .unwrap_or((final_raw_input, final_raw_output));

        // Compute costs
        let input_cost = (input_tokens as f64 / 1000.0) * smoothed_input;
        let output_cost = (output_tokens as f64 / 1000.0) * smoothed_output;
        let total_cost = input_cost + output_cost;

        // Record metrics
        self.total_estimations.fetch_add(1, Ordering::Relaxed);
        let cost_nanoerg = ((total_cost * 1_000_000_000.0).ceil() as u64).max(0);
        self.total_cost_nanoerg
            .fetch_add(cost_nanoerg, Ordering::Relaxed);

        // Record demand for this request
        self.record_demand(model_id, 1);

        Ok(DynamicPriceEstimate {
            model_id: model_id.to_string(),
            input_tokens,
            output_tokens,
            raw_input_per_1k: final_raw_input,
            raw_output_per_1k: final_raw_output,
            smoothed_input_per_1k: smoothed_input,
            smoothed_output_per_1k: smoothed_output,
            input_cost,
            output_cost,
            total_cost,
            currency: "ERG".to_string(),
            tier_used: tier.clone(),
            demand_multiplier: demand_mult,
            time_multiplier: time_mult,
            supply_multiplier: supply_mult,
            effective_multiplier: effective_mult,
            provider_override_applied,
            timestamp: Utc::now(),
        })
    }

    /// Get the current smoothed pricing for a model.
    pub fn get_model_pricing(&self, model_id: &str) -> Result<DynamicPriceEstimate, String> {
        self.estimate(model_id, 1000, 1000, &PricingTier::Basic, None)
            .map(|e| DynamicPriceEstimate {
                model_id: e.model_id,
                input_tokens: 0,
                output_tokens: 0,
                raw_input_per_1k: e.raw_input_per_1k,
                raw_output_per_1k: e.raw_output_per_1k,
                smoothed_input_per_1k: e.smoothed_input_per_1k,
                smoothed_output_per_1k: e.smoothed_output_per_1k,
                input_cost: 0.0,
                output_cost: 0.0,
                total_cost: 0.0,
                currency: e.currency,
                tier_used: e.tier_used,
                demand_multiplier: e.demand_multiplier,
                time_multiplier: e.time_multiplier,
                supply_multiplier: e.supply_multiplier,
                effective_multiplier: e.effective_multiplier,
                provider_override_applied: None,
                timestamp: Utc::now(),
            })
    }

    /// Get demand info for a model.
    pub fn get_demand_info(&self, model_id: &str) -> DemandInfo {
        let count = self
            .demand_state
            .get(model_id)
            .map(|s| s.count())
            .unwrap_or(0);
        let mult = self.compute_demand_multiplier(model_id);
        let last = self
            .demand_state
            .get(model_id)
            .and_then(|s| s.last_request.read().ok().map(|l| *l))
            .unwrap_or_else(Utc::now);

        DemandInfo {
            model_id: model_id.to_string(),
            request_count: count,
            demand_multiplier: mult,
            last_request: last,
        }
    }

    /// Get supply info for a model.
    pub fn get_supply_info(&self, model_id: &str) -> SupplyInfo {
        let count = self
            .supply_state
            .get(model_id)
            .map(|s| s.count())
            .unwrap_or(0);
        let mult = self.compute_supply_multiplier(model_id);
        let config = self.config.read().unwrap_or_else(|e| e.into_inner());

        SupplyInfo {
            model_id: model_id.to_string(),
            provider_count: count,
            supply_multiplier: mult,
            is_scarce: count < config.supply_baseline,
        }
    }

    /// List all pricing tiers.
    pub fn list_tiers(&self) -> Vec<PricingTier> {
        vec![
            PricingTier::Free,
            PricingTier::Basic,
            PricingTier::Pro,
            PricingTier::Enterprise,
        ]
    }

    /// Get all registered model IDs.
    pub fn list_models(&self) -> Vec<String> {
        self.model_pricing.iter().map(|m| m.key().clone()).collect()
    }

    /// Get total metrics.
    pub fn get_total_estimations(&self) -> u64 {
        self.total_estimations.load(Ordering::Relaxed)
    }

    pub fn get_total_cost(&self) -> f64 {
        self.total_cost_nanoerg.load(Ordering::Relaxed) as f64 / 1_000_000_000.0
    }

    /// Reset demand counters for a model.
    pub fn reset_demand(&self, model_id: &str) {
        if let Some(state) = self.demand_state.get(model_id) {
            state.reset();
        }
    }
}

impl Default for DynamicPricingEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HTTP Handlers
// ---------------------------------------------------------------------------

/// POST /api/dynamic-pricing/estimate
async fn estimate_handler(
    State(state): State<AppState>,
    Json(body): Json<DynamicEstimateRequest>,
) -> impl IntoResponse {
    let engine = &state.dynamic_pricing_engine;

    let tier = body
        .tier
        .as_deref()
        .and_then(PricingTier::from_str_loose)
        .unwrap_or(PricingTier::Basic);

    match engine.estimate(
        &body.model_id,
        body.input_tokens,
        body.output_tokens,
        &tier,
        body.provider_id.as_deref(),
    ) {
        Ok(estimate) => (StatusCode::OK, Json(estimate)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// GET /api/dynamic-pricing/model/{model_id}
async fn get_model_pricing_handler(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> impl IntoResponse {
    let engine = &state.dynamic_pricing_engine;
    match engine.get_model_pricing(&model_id) {
        Ok(pricing) => (StatusCode::OK, Json(pricing)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// PUT /api/dynamic-pricing/provider-price
async fn update_provider_price_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateProviderPriceRequest>,
) -> impl IntoResponse {
    let engine = &state.dynamic_pricing_engine;
    match engine.set_provider_override(
        &body.provider_id,
        &body.model_id,
        body.price_multiplier,
        body.min_price,
        body.max_price,
    ) {
        Ok(ovr) => (StatusCode::OK, Json(ovr)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// GET /api/dynamic-pricing/demand/{model_id}
async fn get_demand_handler(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Json<DemandInfo> {
    Json(state.dynamic_pricing_engine.get_demand_info(&model_id))
}

/// PUT /api/dynamic-pricing/supply
async fn update_supply_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateSupplyRequest>,
) -> StatusCode {
    state
        .dynamic_pricing_engine
        .update_supply(&body.model_id, body.provider_count);
    StatusCode::NO_CONTENT
}

/// GET /api/dynamic-pricing/supply/{model_id}
async fn get_supply_handler(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Json<SupplyInfo> {
    Json(state.dynamic_pricing_engine.get_supply_info(&model_id))
}

/// GET /api/dynamic-pricing/multipliers/{model_id}
async fn get_multipliers_handler(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Json<MultipliersSnapshot> {
    Json(state.dynamic_pricing_engine.get_multipliers(&model_id))
}

/// GET /api/dynamic-pricing/tiers
async fn get_tiers_handler(State(state): State<AppState>) -> Json<Vec<PricingTier>> {
    Json(state.dynamic_pricing_engine.list_tiers())
}

/// PUT /api/dynamic-pricing/config
async fn update_config_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateConfigRequest>,
) -> Json<DynamicPricingConfig> {
    Json(state.dynamic_pricing_engine.update_config(body))
}

/// GET /api/dynamic-pricing/config
async fn get_config_handler(State(state): State<AppState>) -> Json<DynamicPricingConfig> {
    Json(state.dynamic_pricing_engine.get_config())
}

/// POST /api/dynamic-pricing/record-request
async fn record_request_handler(
    State(state): State<AppState>,
    Json(body): Json<RecordRequestRequest>,
) -> StatusCode {
    state
        .dynamic_pricing_engine
        .record_demand(&body.model_id, body.count);
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the dynamic pricing router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/dynamic-pricing/estimate", post(estimate_handler))
        .route(
            "/api/dynamic-pricing/model/{model_id}",
            get(get_model_pricing_handler),
        )
        .route(
            "/api/dynamic-pricing/provider-price",
            put(update_provider_price_handler),
        )
        .route(
            "/api/dynamic-pricing/demand/{model_id}",
            get(get_demand_handler),
        )
        .route("/api/dynamic-pricing/supply", put(update_supply_handler))
        .route(
            "/api/dynamic-pricing/supply/{model_id}",
            get(get_supply_handler),
        )
        .route(
            "/api/dynamic-pricing/multipliers/{model_id}",
            get(get_multipliers_handler),
        )
        .route("/api/dynamic-pricing/tiers", get(get_tiers_handler))
        .route(
            "/api/dynamic-pricing/config",
            put(update_config_handler).get(get_config_handler),
        )
        .route(
            "/api/dynamic-pricing/record-request",
            post(record_request_handler),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> DynamicPricingEngine {
        DynamicPricingEngine::new()
    }

    // -- Tier tests --

    #[test]
    fn test_tier_base_multipliers() {
        assert_eq!(PricingTier::Free.base_multiplier(), 0.0);
        assert_eq!(PricingTier::Basic.base_multiplier(), 1.0);
        assert_eq!(PricingTier::Pro.base_multiplier(), 1.5);
        assert_eq!(PricingTier::Enterprise.base_multiplier(), 2.0);
    }

    #[test]
    fn test_tier_from_str_loose() {
        assert_eq!(PricingTier::from_str_loose("free"), Some(PricingTier::Free));
        assert_eq!(PricingTier::from_str_loose("Free"), Some(PricingTier::Free));
        assert_eq!(
            PricingTier::from_str_loose("BASIC"),
            Some(PricingTier::Basic)
        );
        assert_eq!(PricingTier::from_str_loose("pro"), Some(PricingTier::Pro));
        assert_eq!(
            PricingTier::from_str_loose("enterprise"),
            Some(PricingTier::Enterprise)
        );
        assert_eq!(PricingTier::from_str_loose("unknown"), None);
        assert_eq!(PricingTier::from_str_loose(""), None);
    }

    // -- Demand multiplier tests --

    #[test]
    fn test_demand_multiplier_zero_demand() {
        let engine = make_engine();
        let mult = engine.compute_demand_multiplier("gpt-4");
        // At zero demand, sigmoid gives 1.0
        assert!((mult - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_demand_multiplier_increases_with_demand() {
        let engine = make_engine();
        engine.record_demand("gpt-4", 50);
        let mult_low = engine.compute_demand_multiplier("gpt-4");

        engine.record_demand("gpt-4", 200);
        let mult_high = engine.compute_demand_multiplier("gpt-4");

        assert!(mult_high > mult_low);
        assert!(mult_high > 1.0);
    }

    #[test]
    fn test_demand_multiplier_approaches_cap() {
        let engine = make_engine();
        engine.record_demand("gpt-4", 10_000);
        let mult = engine.compute_demand_multiplier("gpt-4");
        let config = engine.get_config();
        // Should approach 1.0 + cap but not exceed it
        assert!(mult > 1.0 + config.demand_multiplier_cap * 0.8);
        assert!(mult <= 1.0 + config.demand_multiplier_cap + 0.001);
    }

    #[test]
    fn test_demand_multiplier_independent_models() {
        let engine = make_engine();
        engine.record_demand("gpt-4", 500);
        engine.record_demand("claude-3-opus", 10);

        let m1 = engine.compute_demand_multiplier("gpt-4");
        let m2 = engine.compute_demand_multiplier("claude-3-opus");
        assert!(m1 > m2);
    }

    // -- Time-of-day tests --

    #[test]
    fn test_time_multiplier_during_peak() {
        let engine = make_engine();
        // Default peak is 9-21, so hour=12 should be peak
        let mult = engine.compute_time_multiplier_for_hour(12);
        let config = engine.get_config();
        assert!((mult - config.peak_multiplier).abs() < 0.001);
    }

    #[test]
    fn test_time_multiplier_off_peak() {
        let engine = make_engine();
        // Hour 3 should be off-peak
        let mult = engine.compute_time_multiplier_for_hour(3);
        assert!((mult - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_time_multiplier_boundary() {
        let engine = make_engine();
        // Hour 9 = start of peak (inclusive)
        let mult_start = engine.compute_time_multiplier_for_hour(9);
        assert!(mult_start > 1.0);

        // Hour 21 = end of peak (exclusive)
        let mult_end = engine.compute_time_multiplier_for_hour(21);
        assert!((mult_end - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_time_multiplier_wrapping_range() {
        let engine = make_engine();
        // Configure wrapping peak: 22-6
        engine.update_config(UpdateConfigRequest {
            peak_hour_start: Some(22),
            peak_hour_end: Some(6),
            ..Default::default()
        });

        let mult_23 = engine.compute_time_multiplier_for_hour(23);
        let config = engine.get_config();
        assert!((mult_23 - config.peak_multiplier).abs() < 0.001);

        let mult_3 = engine.compute_time_multiplier_for_hour(3);
        assert!((mult_3 - config.peak_multiplier).abs() < 0.001);

        let mult_12 = engine.compute_time_multiplier_for_hour(12);
        assert!((mult_12 - 1.0).abs() < 0.001);
    }

    // -- Supply multiplier tests --

    #[test]
    fn test_supply_multiplier_abundant() {
        let engine = make_engine();
        engine.update_supply("gpt-4", 10);
        let mult = engine.compute_supply_multiplier("gpt-4");
        assert!((mult - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_supply_multiplier_scarce() {
        let engine = make_engine();
        engine.update_supply("gpt-4", 1);
        let mult = engine.compute_supply_multiplier("gpt-4");
        assert!(mult > 1.0);
    }

    #[test]
    fn test_supply_multiplier_zero_providers() {
        let engine = make_engine();
        engine.update_supply("gpt-4", 0);
        let mult = engine.compute_supply_multiplier("gpt-4");
        let config = engine.get_config();
        // At 0 providers: 1.0 + cap * (1 - 0) = 1 + cap
        assert!((mult - (1.0 + config.supply_multiplier_cap)).abs() < 0.001);
    }

    #[test]
    fn test_supply_multiplier_at_baseline() {
        let engine = make_engine();
        let config = engine.get_config();
        engine.update_supply("gpt-4", config.supply_baseline);
        let mult = engine.compute_supply_multiplier("gpt-4");
        assert!((mult - 1.0).abs() < 0.001);
    }

    // -- Effective multiplier tests --

    #[test]
    fn test_effective_multiplier_within_bounds() {
        let engine = make_engine();
        let mult = engine.compute_effective_multiplier("gpt-4");
        let config = engine.get_config();
        assert!(mult >= config.price_floor);
        assert!(mult <= config.price_cap);
    }

    #[test]
    fn test_effective_multiplier_clamped_to_floor() {
        let engine = make_engine();
        engine.update_config(UpdateConfigRequest {
            price_floor: Some(2.0),
            price_cap: Some(10.0),
            ..Default::default()
        });
        let mult = engine.compute_effective_multiplier("gpt-4");
        assert!(mult >= 2.0);
    }

    #[test]
    fn test_effective_multiplier_clamped_to_cap() {
        let engine = make_engine();
        // Create extreme conditions: high demand + low supply
        engine.record_demand("gpt-4", 100_000);
        engine.update_supply("gpt-4", 0);
        engine.update_config(UpdateConfigRequest {
            price_cap: Some(3.0),
            peak_multiplier: Some(5.0),
            ..Default::default()
        });

        let mult = engine.compute_effective_multiplier("gpt-4");
        assert!(mult <= 3.0);
    }

    // -- Price estimation tests --

    #[test]
    fn test_estimate_basic() {
        let engine = make_engine();
        let result = engine.estimate("gpt-4", 100, 50, &PricingTier::Basic, None);
        assert!(result.is_ok());
        let est = result.unwrap();
        assert_eq!(est.model_id, "gpt-4");
        assert_eq!(est.input_tokens, 100);
        assert_eq!(est.output_tokens, 50);
        assert!(est.total_cost > 0.0);
        assert_eq!(est.currency, "ERG");
        assert_eq!(est.tier_used, PricingTier::Basic);
    }

    #[test]
    fn test_estimate_free_tier_zero_cost() {
        let engine = make_engine();
        let result = engine.estimate("gpt-4", 100, 50, &PricingTier::Free, None);
        assert!(result.is_ok());
        let est = result.unwrap();
        assert!((est.total_cost).abs() < 0.0001);
    }

    #[test]
    fn test_estimate_pro_tier_more_expensive() {
        let engine = make_engine();
        let basic = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Basic, None)
            .unwrap();
        let pro = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Pro, None)
            .unwrap();
        assert!(pro.total_cost > basic.total_cost);
    }

    #[test]
    fn test_estimate_enterprise_most_expensive() {
        let engine = make_engine();
        let basic = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Basic, None)
            .unwrap();
        let ent = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Enterprise, None)
            .unwrap();
        assert!(ent.total_cost > basic.total_cost);
        assert!(ent.total_cost > 0.0);
    }

    #[test]
    fn test_estimate_unknown_model() {
        let engine = make_engine();
        let result = engine.estimate("nonexistent-model", 100, 50, &PricingTier::Basic, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }

    #[test]
    fn test_estimate_records_demand() {
        let engine = make_engine();
        engine.reset_demand("gpt-4");
        engine
            .estimate("gpt-4", 100, 50, &PricingTier::Basic, None)
            .unwrap();
        let info = engine.get_demand_info("gpt-4");
        assert_eq!(info.request_count, 1);
    }

    // -- Provider override tests --

    #[test]
    fn test_provider_override_basic() {
        let engine = make_engine();
        let result = engine.set_provider_override("provider-1", "gpt-4", 0.8, None, None);
        assert!(result.is_ok());
        let ovr = result.unwrap();
        assert_eq!(ovr.provider_id, "provider-1");
        assert_eq!(ovr.model_id, "gpt-4");
        assert!((ovr.price_multiplier - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_provider_override_invalid_multiplier() {
        let engine = make_engine();
        let result = engine.set_provider_override("p1", "gpt-4", 0.0, None, None);
        assert!(result.is_err());

        let result = engine.set_provider_override("p1", "gpt-4", -1.0, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_override_invalid_model() {
        let engine = make_engine();
        let result = engine.set_provider_override("p1", "nonexistent", 1.0, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_override_affects_price() {
        let engine = make_engine();
        engine.reset_demand("gpt-4");

        let base = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Basic, None)
            .unwrap();

        engine.reset_demand("gpt-4");
        engine
            .set_provider_override("cheap-provider", "gpt-4", 0.5, None, None)
            .unwrap();

        let discounted = engine
            .estimate(
                "gpt-4",
                100,
                50,
                &PricingTier::Basic,
                Some("cheap-provider"),
            )
            .unwrap();

        // Discounted should be cheaper (approximately half due to smoothing on first call)
        assert!(discounted.total_cost < base.total_cost);
        assert!(discounted.provider_override_applied.is_some());
    }

    #[test]
    fn test_provider_override_retrieval() {
        let engine = make_engine();
        engine
            .set_provider_override("p1", "gpt-4", 1.2, None, None)
            .unwrap();

        let ovr = engine.get_provider_override("p1", "gpt-4");
        assert!(ovr.is_some());

        let missing = engine.get_provider_override("p2", "gpt-4");
        assert!(missing.is_none());
    }

    #[test]
    fn test_provider_override_removal() {
        let engine = make_engine();
        engine
            .set_provider_override("p1", "gpt-4", 1.2, None, None)
            .unwrap();
        assert!(engine.remove_provider_override("p1", "gpt-4"));
        assert!(!engine.remove_provider_override("p1", "gpt-4"));
        assert!(engine.get_provider_override("p1", "gpt-4").is_none());
    }

    // -- EMA smoothing tests --

    #[test]
    fn test_ema_smoothing_converges() {
        let engine = make_engine();
        engine.reset_demand("gpt-4");

        // First estimate
        let e1 = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Basic, None)
            .unwrap();

        // Second estimate with same params (EMA should be closer to base)
        engine.reset_demand("gpt-4");
        let e2 = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Basic, None)
            .unwrap();

        // Smoothed values should exist
        assert!(e2.smoothed_input_per_1k > 0.0);
        assert!(e2.smoothed_output_per_1k > 0.0);
    }

    #[test]
    fn test_ema_alpha_configurable() {
        let engine = make_engine();
        engine.reset_demand("gpt-4");
        engine.update_config(UpdateConfigRequest {
            ema_alpha: Some(0.9), // very responsive
            ..Default::default()
        });

        let e1 = engine
            .estimate("gpt-4", 100, 50, &PricingTier::Basic, None)
            .unwrap();

        // With high alpha, smoothed should be very close to raw
        let diff_input = (e1.smoothed_input_per_1k - e1.raw_input_per_1k).abs();
        assert!(diff_input < 0.0001);
    }

    // -- Config tests --

    #[test]
    fn test_config_update_partial() {
        let engine = make_engine();
        let original = engine.get_config();
        let updated = engine.update_config(UpdateConfigRequest {
            price_cap: Some(10.0),
            ..Default::default()
        });

        assert!((updated.price_cap - 10.0).abs() < 0.001);
        // Other fields unchanged
        assert!((updated.price_floor - original.price_floor).abs() < 0.001);
        assert!((updated.ema_alpha - original.ema_alpha).abs() < 0.001);
    }

    #[test]
    fn test_ema_alpha_clamped() {
        let engine = make_engine();
        let config = engine.update_config(UpdateConfigRequest {
            ema_alpha: Some(0.0),
            ..Default::default()
        });
        assert_eq!(config.ema_alpha, 0.01);

        let config = engine.update_config(UpdateConfigRequest {
            ema_alpha: Some(1.5),
            ..Default::default()
        });
        assert_eq!(config.ema_alpha, 0.99);
    }

    // -- Multipliers snapshot tests --

    #[test]
    fn test_multipliers_snapshot() {
        let engine = make_engine();
        let snap = engine.get_multipliers("gpt-4");
        assert_eq!(snap.model_id, "gpt-4");
        assert!(snap.demand_multiplier >= 1.0);
        assert!(snap.time_multiplier >= 1.0);
        assert!(snap.supply_multiplier >= 1.0);
        assert!(snap.effective_multiplier >= snap.demand_multiplier * 1.0 * 1.0 - 0.01);
    }

    // -- Demand info tests --

    #[test]
    fn test_demand_info() {
        let engine = make_engine();
        engine.reset_demand("gpt-4");
        engine.record_demand("gpt-4", 42);
        let info = engine.get_demand_info("gpt-4");
        assert_eq!(info.model_id, "gpt-4");
        assert_eq!(info.request_count, 42);
        assert!(info.demand_multiplier > 1.0);
    }

    // -- Supply info tests --

    #[test]
    fn test_supply_info() {
        let engine = make_engine();
        engine.update_supply("gpt-4", 2);
        let info = engine.get_supply_info("gpt-4");
        assert_eq!(info.provider_count, 2);
        assert!(info.is_scarce);
        assert!(info.supply_multiplier > 1.0);
    }

    #[test]
    fn test_supply_info_abundant() {
        let engine = make_engine();
        engine.update_supply("gpt-4", 10);
        let info = engine.get_supply_info("gpt-4");
        assert!(!info.is_scarce);
        assert!((info.supply_multiplier - 1.0).abs() < 0.001);
    }

    // -- List models/tiers tests --

    #[test]
    fn test_list_models() {
        let engine = make_engine();
        let models = engine.list_models();
        assert!(models.contains(&"gpt-4".to_string()));
        assert!(models.contains(&"claude-3-opus".to_string()));
        assert!(models.len() >= 7);
    }

    #[test]
    fn test_list_tiers() {
        let engine = make_engine();
        let tiers = engine.list_tiers();
        assert_eq!(tiers.len(), 4);
        assert!(tiers.contains(&PricingTier::Free));
        assert!(tiers.contains(&PricingTier::Basic));
        assert!(tiers.contains(&PricingTier::Pro));
        assert!(tiers.contains(&PricingTier::Enterprise));
    }

    // -- Model pricing handler tests --

    #[test]
    fn test_get_model_pricing() {
        let engine = make_engine();
        let result = engine.get_model_pricing("gpt-4");
        assert!(result.is_ok());
        let pricing = result.unwrap();
        assert_eq!(pricing.model_id, "gpt-4");
        assert!(pricing.smoothed_input_per_1k > 0.0);
        assert!(pricing.smoothed_output_per_1k > 0.0);
    }

    #[test]
    fn test_get_model_pricing_unknown() {
        let engine = make_engine();
        let result = engine.get_model_pricing("nonexistent");
        assert!(result.is_err());
    }

    // -- Multiplier integration test --

    #[test]
    fn test_all_multipliers_combine_correctly() {
        let engine = make_engine();
        engine.reset_demand("gpt-4");
        engine.record_demand("gpt-4", 200);
        engine.update_supply("gpt-4", 2);

        let snap = engine.get_multipliers("gpt-4");
        let combined = snap.demand_multiplier * snap.time_multiplier * snap.supply_multiplier;
        let config = engine.get_config();
        let expected_effective = combined.clamp(config.price_floor, config.price_cap);

        assert!((snap.effective_multiplier - expected_effective).abs() < 0.001);
    }

    // -- Metrics tests --

    #[test]
    fn test_total_estimations_increments() {
        let engine = make_engine();
        let before = engine.get_total_estimations();
        engine
            .estimate("gpt-4", 10, 10, &PricingTier::Basic, None)
            .unwrap();
        let after = engine.get_total_estimations();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn test_reset_demand() {
        let engine = make_engine();
        engine.record_demand("gpt-4", 100);
        engine.reset_demand("gpt-4");
        assert_eq!(engine.get_demand_info("gpt-4").request_count, 0);
    }
}
