//! SigmaUSD Pricing — Stablecoin pricing mode for the Xergon marketplace
//!
//! Provides dual pricing (ERG + USD) with exchange rate from oracle feeds,
//! slippage protection, per-model pricing rules, and stable price quotes
//! with TTL. Supports ErgoNative, SigmaUsd, and Hybrid pricing modes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use axum::response::IntoResponse;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Pricing mode for the marketplace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PricingMode {
    /// Native ERG pricing only.
    #[serde(rename = "ergo_native")]
    ErgoNative,
    /// SigmaUSD stablecoin pricing (prices quoted in USD, settled in ERG).
    #[serde(rename = "sigma_usd")]
    SigmaUsd,
    /// Hybrid mode: prices quoted in both ERG and USD.
    #[serde(rename = "hybrid")]
    Hybrid,
}

impl std::fmt::Display for PricingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ErgoNative => write!(f, "ergo_native"),
            Self::SigmaUsd => write!(f, "sigma_usd"),
            Self::Hybrid => write!(f, "hybrid"),
        }
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// SigmaUSD pricing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaUsdConfig {
    /// Exchange rate: nanoERG per 1 USD.
    pub exchange_rate_nanoerg_per_usd: i64,
    /// How often to update the exchange rate (seconds).
    pub update_interval_secs: u64,
    /// Minimum ERG fee per transaction (in ERG).
    pub min_erg_fee: f64,
    /// Maximum allowed slippage percentage.
    pub max_slippage_pct: f64,
    /// Whether stable pricing is enabled.
    pub enable_stable_pricing: bool,
    /// Quote TTL in seconds.
    pub quote_ttl_secs: u64,
}

impl Default for SigmaUsdConfig {
    fn default() -> Self {
        Self {
            exchange_rate_nanoerg_per_usd: 2_222_222_222, // ~0.45 ERG/USD
            update_interval_secs: 300,
            min_erg_fee: 0.001,
            max_slippage_pct: 2.0,
            enable_stable_pricing: true,
            quote_ttl_secs: 300,
        }
    }
}

/// A stable price quote with expiration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StablePriceQuote {
    /// Unique quote identifier.
    pub quote_id: String,
    /// Model being quoted.
    pub model_id: String,
    /// Price in USD.
    pub price_usd: f64,
    /// Price in ERG (derived from exchange rate).
    pub price_erg: f64,
    /// Exchange rate used (nanoERG per USD).
    pub exchange_rate: i64,
    /// Quote creation timestamp.
    pub created_at: i64,
    /// Quote expiration timestamp.
    pub valid_until: i64,
    /// Maximum slippage percentage.
    pub slippage_pct: f64,
    /// Whether this quote is still valid.
    pub is_valid: bool,
    /// Pricing mode used for this quote.
    pub mode: PricingMode,
}

impl StablePriceQuote {
    /// Check if this quote has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() >= self.valid_until
    }

    /// Calculate the effective ERG price with slippage protection.
    pub fn effective_erg_price(&self) -> f64 {
        if self.is_expired() {
            return 0.0;
        }
        // Apply slippage buffer: price * (1 + slippage_pct/100)
        self.price_erg * (1.0 + self.slippage_pct / 100.0)
    }

    /// Calculate the maximum acceptable ERG price (with slippage).
    pub fn max_acceptable_erg(&self) -> f64 {
        self.price_erg * (1.0 + self.slippage_pct / 100.0)
    }

    /// Calculate the minimum acceptable ERG price (with negative slippage).
    pub fn min_acceptable_erg(&self) -> f64 {
        let min_slippage = self.slippage_pct * 0.5; // Half slippage on the low side
        self.price_erg * (1.0 - min_slippage / 100.0)
    }
}

/// Per-model pricing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// Model this rule applies to.
    pub model_id: String,
    /// Base price in USD per 1M tokens.
    pub base_price_usd: f64,
    /// ERG price adjustment percentage (positive = more expensive in ERG).
    pub erg_adjustment_pct: f64,
    /// Whether this rule is enabled.
    pub enabled: bool,
    /// Rule creation timestamp.
    pub created_at: i64,
}

impl PricingRule {
    /// Create a new pricing rule.
    pub fn new(
        model_id: impl Into<String>,
        base_price_usd: f64,
        erg_adjustment_pct: f64,
    ) -> Self {
        Self {
            rule_id: uuid::Uuid::new_v4().to_string(),
            model_id: model_id.into(),
            base_price_usd,
            erg_adjustment_pct,
            enabled: true,
            created_at: Utc::now().timestamp(),
        }
    }

    /// Get the effective USD price with adjustment.
    pub fn effective_price_usd(&self, tokens_millions: f64) -> f64 {
        self.base_price_usd * tokens_millions
    }

    /// Get the effective ERG price.
    pub fn effective_price_erg(&self, tokens_millions: f64, nanoerg_per_usd: i64) -> f64 {
        let usd_price = self.effective_price_usd(tokens_millions);
        let base_erg = usd_price * nanoerg_per_usd as f64 / 1_000_000_000.0;
        base_erg * (1.0 + self.erg_adjustment_pct / 100.0)
    }
}

/// Pricing statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingStats {
    /// Current pricing mode.
    pub mode: PricingMode,
    /// Current exchange rate (nanoERG per USD).
    pub exchange_rate: i64,
    /// ERG/USD implied rate.
    pub erg_usd_rate: f64,
    /// Number of active pricing rules.
    pub active_rules: usize,
    /// Number of valid (non-expired) quotes.
    pub valid_quotes: usize,
    /// Total quotes generated.
    pub total_quotes: u64,
    /// Stable pricing enabled.
    pub stable_pricing_enabled: bool,
}

/// Quote request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteRequest {
    /// Model to get a quote for.
    pub model_id: String,
    /// Number of tokens (in millions).
    pub tokens_millions: f64,
    /// Preferred pricing mode (optional, uses current mode if not specified).
    pub preferred_mode: Option<PricingMode>,
    /// Custom slippage override (optional).
    pub slippage_override: Option<f64>,
}

// ---------------------------------------------------------------------------
// SigmaUsdPricer
// ---------------------------------------------------------------------------

/// SigmaUSD stablecoin pricing service backed by DashMap.
///
/// Provides dual pricing (ERG + USD), exchange rate management,
/// per-model pricing rules, and stable price quotes with TTL.
pub struct SigmaUsdPricer {
    /// Current pricing quotes keyed by quote_id.
    quotes: DashMap<String, StablePriceQuote>,
    /// Per-model pricing rules keyed by model_id.
    rules: DashMap<String, PricingRule>,
    /// Service configuration.
    config: Arc<std::sync::RwLock<SigmaUsdConfig>>,
    /// Current pricing mode.
    mode: Arc<std::sync::RwLock<PricingMode>>,
    /// Total quotes generated counter.
    total_quotes: Arc<AtomicBool>,
    /// Quotes counter (as u64).
    total_quotes_count: Arc<std::sync::atomic::AtomicU64>,
    /// Last exchange rate update timestamp.
    last_rate_update: Arc<std::sync::atomic::AtomicI64>,
}

impl SigmaUsdPricer {
    /// Create a new pricer with default configuration.
    pub fn new() -> Self {
        Self {
            quotes: DashMap::new(),
            rules: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(SigmaUsdConfig::default())),
            mode: Arc::new(std::sync::RwLock::new(PricingMode::ErgoNative)),
            total_quotes: Arc::new(AtomicBool::new(false)),
            total_quotes_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_rate_update: Arc::new(std::sync::atomic::AtomicI64::new(Utc::now().timestamp())),
        }
    }

    /// Create with a specific pricing mode.
    pub fn with_mode(mode: PricingMode) -> Self {
        let pricer = Self::new();
        *pricer.mode.write().unwrap() = mode;
        pricer
    }

    /// Create with a specific exchange rate.
    pub fn with_exchange_rate(nanoerg_per_usd: i64) -> Self {
        let pricer = Self::new();
        pricer.config.write().unwrap().exchange_rate_nanoerg_per_usd = nanoerg_per_usd;
        pricer
    }

    // ----- Quote management -----

    /// Get a price quote for a model.
    pub fn get_quote(&self, request: QuoteRequest) -> Result<StablePriceQuote, String> {
        let config = self.config.read().unwrap();
        let mode = *self.mode.read().unwrap();
        let effective_mode = request.preferred_mode.unwrap_or(mode);

        if !config.enable_stable_pricing && effective_mode == PricingMode::SigmaUsd {
            return Err("Stable pricing is disabled".to_string());
        }

        // Find pricing rule for the model
        let rule = self.rules.get(&request.model_id);
        let (base_usd, erg_adj) = match rule {
            Some(r) if r.value().enabled => {
                (r.value().base_price_usd, r.value().erg_adjustment_pct)
            }
            _ => (0.10, 0.0), // Default: $0.10 per 1M tokens
        };

        let tokens = request.tokens_millions.max(0.001); // Minimum 1K tokens
        let price_usd = base_usd * tokens;
        let rate = config.exchange_rate_nanoerg_per_usd;
        let price_erg = (price_usd * rate as f64 / 1_000_000_000.0)
            * (1.0 + erg_adj / 100.0);

        // Apply minimum fee
        let price_erg = price_erg.max(config.min_erg_fee);

        let slippage = request
            .slippage_override
            .unwrap_or(config.max_slippage_pct);

        let now = Utc::now().timestamp();
        let quote = StablePriceQuote {
            quote_id: uuid::Uuid::new_v4().to_string(),
            model_id: request.model_id.clone(),
            price_usd,
            price_erg,
            exchange_rate: rate,
            created_at: now,
            valid_until: now + config.quote_ttl_secs as i64,
            slippage_pct: slippage,
            is_valid: true,
            mode: effective_mode,
        };

        self.quotes.insert(quote.quote_id.clone(), quote.clone());
        self.total_quotes_count.fetch_add(1, Ordering::Relaxed);

        info!(
            quote_id = %quote.quote_id,
            model = %quote.model_id,
            price_usd = quote.price_usd,
            price_erg = quote.price_erg,
            mode = %quote.mode,
            "Price quote generated"
        );

        Ok(quote)
    }

    /// Get a quote by ID.
    pub fn get_quote_by_id(&self, quote_id: &str) -> Option<StablePriceQuote> {
        self.quotes.get(quote_id).map(|q| {
            let mut quote = q.value().clone();
            quote.is_valid = !quote.is_expired();
            quote
        })
    }

    /// Validate a quote (check if it's still valid and not expired).
    pub fn validate_quote(&self, quote_id: &str) -> Result<StablePriceQuote, String> {
        let quote = self.get_quote_by_id(quote_id).ok_or("Quote not found")?;

        if quote.is_expired() {
            return Err(format!(
                "Quote expired at {}",
                quote.valid_until
            ));
        }

        Ok(quote)
    }

    // ----- Pricing mode -----

    /// Set the pricing mode.
    pub fn set_pricing_mode(&self, new_mode: PricingMode) {
        let mut mode = self.mode.write().unwrap();
        let old_mode = *mode;
        *mode = new_mode;
        info!(old_mode = %old_mode, new_mode = %new_mode, "Pricing mode updated");
    }

    /// Get the current pricing mode.
    pub fn get_pricing_mode(&self) -> PricingMode {
        *self.mode.read().unwrap()
    }

    // ----- Exchange rate -----

    /// Update the exchange rate from an oracle source.
    pub fn update_exchange_rate(&self, nanoerg_per_usd: i64) {
        if nanoerg_per_usd <= 0 {
            warn!(rate = nanoerg_per_usd, "Invalid exchange rate rejected");
            return;
        }

        let mut config = self.config.write().unwrap();
        let old_rate = config.exchange_rate_nanoerg_per_usd;
        config.exchange_rate_nanoerg_per_usd = nanoerg_per_usd;
        self.last_rate_update.store(Utc::now().timestamp(), Ordering::Relaxed);

        // Invalidate existing quotes when rate changes significantly
        let rate_change_pct = if old_rate > 0 {
            ((nanoerg_per_usd as f64 - old_rate as f64) / old_rate as f64 * 100.0).abs()
        } else {
            100.0
        };

        if rate_change_pct > 1.0 {
            let invalidated = self.invalidate_all_quotes();
            if invalidated > 0 {
                info!(
                    invalidated = invalidated,
                    rate_change_pct = rate_change_pct,
                    "Quotes invalidated due to exchange rate change"
                );
            }
        }

        info!(
            old_rate = old_rate,
            new_rate = nanoerg_per_usd,
            erg_usd = 1_000_000_000.0 / nanoerg_per_usd as f64,
            "Exchange rate updated"
        );
    }

    /// Get the current exchange rate.
    pub fn get_exchange_rate(&self) -> i64 {
        self.config.read().unwrap().exchange_rate_nanoerg_per_usd
    }

    // ----- Pricing rules -----

    /// Add or update a pricing rule for a model.
    pub fn add_pricing_rule(&self, rule: PricingRule) {
        let model_id = rule.model_id.clone();
        let rule_id = rule.rule_id.clone();
        self.rules.insert(model_id.clone(), rule);
        info!(
            rule_id = %rule_id,
            model = %model_id,
            "Pricing rule added/updated"
        );
    }

    /// Remove a pricing rule for a model.
    pub fn remove_rule(&self, model_id: &str) -> bool {
        if self.rules.remove(model_id).is_some() {
            info!(model = %model_id, "Pricing rule removed");
            true
        } else {
            false
        }
    }

    /// Get the pricing rule for a specific model.
    pub fn get_model_rule(&self, model_id: &str) -> Option<PricingRule> {
        self.rules.get(model_id).map(|r| r.value().clone())
    }

    /// Get the model price in both ERG and USD.
    pub fn get_model_price(&self, model_id: &str, tokens_millions: f64) -> Option<(f64, f64)> {
        let rule = self.rules.get(model_id)?;
        let r = rule.value();
        if !r.enabled {
            return None;
        }
        let config = self.config.read().unwrap();
        let usd = r.effective_price_usd(tokens_millions);
        let erg = r.effective_price_erg(tokens_millions, config.exchange_rate_nanoerg_per_usd);
        Some((usd, erg))
    }

    /// Get all pricing rules.
    pub fn get_all_rules(&self) -> Vec<PricingRule> {
        self.rules.iter().map(|r| r.value().clone()).collect()
    }

    // ----- Currency conversion -----

    /// Convert an ERG amount to USD using the current exchange rate.
    pub fn convert_to_stable(&self, erg_amount: f64) -> f64 {
        let config = self.config.read().unwrap();
        let rate = config.exchange_rate_nanoerg_per_usd;
        if rate == 0 {
            return 0.0;
        }
        erg_amount * 1_000_000_000.0 / rate as f64
    }

    /// Convert a USD amount to ERG using the current exchange rate.
    pub fn convert_from_stable(&self, usd_amount: f64) -> f64 {
        let config = self.config.read().unwrap();
        let rate = config.exchange_rate_nanoerg_per_usd;
        usd_amount * rate as f64 / 1_000_000_000.0
    }

    // ----- Configuration -----

    /// Get pricing statistics.
    pub fn get_stats(&self) -> PricingStats {
        let config = self.config.read().unwrap();
        let mode = *self.mode.read().unwrap();
        let rate = config.exchange_rate_nanoerg_per_usd;
        let now = Utc::now().timestamp();

        let valid_quotes = self
            .quotes
            .iter()
            .filter(|q| !q.value().is_expired())
            .count();

        let active_rules = self
            .rules
            .iter()
            .filter(|r| r.value().enabled)
            .count();

        PricingStats {
            mode,
            exchange_rate: rate,
            erg_usd_rate: if rate > 0 { 1_000_000_000.0 / rate as f64 } else { 0.0 },
            active_rules,
            valid_quotes,
            total_quotes: self.total_quotes_count.load(Ordering::Relaxed),
            stable_pricing_enabled: config.enable_stable_pricing,
        }
    }

    /// Get the current configuration.
    pub fn get_config(&self) -> SigmaUsdConfig {
        self.config.read().unwrap().clone()
    }

    /// Update the configuration.
    pub fn update_config(&self, new_config: SigmaUsdConfig) {
        let mut config = self.config.write().unwrap();
        *config = new_config;
        info!("SigmaUSD configuration updated");
    }

    /// Enable or disable stable pricing.
    pub fn set_stable_pricing(&self, enabled: bool) {
        let mut config = self.config.write().unwrap();
        config.enable_stable_pricing = enabled;
        info!(enabled = enabled, "Stable pricing {}", if enabled { "enabled" } else { "disabled" });
    }

    // ----- Cleanup -----

    /// Invalidate all existing quotes.
    fn invalidate_all_quotes(&self) -> usize {
        let count = self.quotes.len();
        self.quotes.retain(|_, q| {
            q.is_valid = false;
            false // Remove all
        });
        count
    }

    /// Clean up expired quotes.
    pub fn cleanup_expired_quotes(&self) -> usize {
        let mut removed = 0;
        self.quotes.retain(|_, q| {
            if q.is_expired() {
                removed += 1;
                false
            } else {
                true
            }
        });
        if removed > 0 {
            debug!(removed = removed, "Expired quotes cleaned up");
        }
        removed
    }

    /// Get all valid (non-expired) quotes.
    pub fn get_valid_quotes(&self) -> Vec<StablePriceQuote> {
        self.quotes
            .iter()
            .filter_map(|q| {
                let quote = q.value().clone();
                if !quote.is_expired() {
                    Some(quote)
                } else {
                    None
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// REST API router builder
// ---------------------------------------------------------------------------

/// Build the SigmaUSD pricing router.
pub fn build_sigma_usd_pricing_router(state: crate::api::AppState) -> axum::Router<()> {
    use axum::routing::{get, post, put};

    axum::Router::new()
        .route("/v1/pricing/quote", post(pricing_quote_handler))
        .route("/v1/pricing/mode", get(pricing_mode_get_handler).put(pricing_mode_put_handler))
        .route("/v1/pricing/exchange-rate", get(pricing_exchange_rate_handler))
        .route("/v1/pricing/rules", post(pricing_rules_post_handler))
        .route("/v1/pricing/rules/{model_id}", get(pricing_rules_get_handler))
        .route("/v1/pricing/config", get(pricing_config_handler))
        .with_state(state)
}

// ----- Request/Response types -----

#[derive(Debug, Deserialize)]
struct QuoteRequestBody {
    pub model_id: String,
    pub tokens_millions: f64,
    pub preferred_mode: Option<PricingMode>,
    pub slippage_override: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SetModeBody {
    pub mode: PricingMode,
}

#[derive(Debug, Deserialize)]
struct SetExchangeRateBody {
    pub nanoerg_per_usd: i64,
}

#[derive(Debug, Deserialize)]
struct AddRuleBody {
    pub model_id: String,
    pub base_price_usd: f64,
    #[serde(default)]
    pub erg_adjustment_pct: f64,
}

// ----- Handlers -----

async fn pricing_quote_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::Json(req): axum::Json<QuoteRequestBody>,
) -> axum::response::Response {
    let request = QuoteRequest {
        model_id: req.model_id,
        tokens_millions: req.tokens_millions,
        preferred_mode: req.preferred_mode,
        slippage_override: req.slippage_override,
    };

    match state.sigma_usd_pricer.get_quote(request) {
        Ok(quote) => axum::Json(serde_json::json!(quote)).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn pricing_mode_get_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let mode = state.sigma_usd_pricer.get_pricing_mode();
    axum::Json(serde_json::json!({
        "mode": mode,
        "description": format!("Current pricing mode: {}", mode)
    }))
}

async fn pricing_mode_put_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::Json(body): axum::Json<SetModeBody>,
) -> axum::Json<serde_json::Value> {
    state.sigma_usd_pricer.set_pricing_mode(body.mode);
    axum::Json(serde_json::json!({
        "ok": true,
        "mode": body.mode
    }))
}

async fn pricing_exchange_rate_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let rate = state.sigma_usd_pricer.get_exchange_rate();
    let erg_usd = if rate > 0 {
        1_000_000_000.0 / rate as f64
    } else {
        0.0
    };

    axum::Json(serde_json::json!({
        "exchange_rate_nanoerg_per_usd": rate,
        "erg_usd_rate": erg_usd,
        "last_update": state.sigma_usd_pricer.get_stats().valid_quotes as i64
    }))
}

async fn pricing_rules_post_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::Json(body): axum::Json<AddRuleBody>,
) -> axum::Json<serde_json::Value> {
    let rule = PricingRule::new(body.model_id, body.base_price_usd, body.erg_adjustment_pct);
    let rule_id = rule.rule_id.clone();
    state.sigma_usd_pricer.add_pricing_rule(rule);
    axum::Json(serde_json::json!({
        "ok": true,
        "rule_id": rule_id
    }))
}

async fn pricing_rules_get_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> axum::response::Response {
    match state.sigma_usd_pricer.get_model_rule(&model_id) {
        Some(rule) => axum::Json(serde_json::json!(rule)).into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": "No pricing rule found for model",
                "model_id": model_id
            })),
        )
            .into_response(),
    }
}

async fn pricing_config_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let config = state.sigma_usd_pricer.get_config();
    axum::Json(serde_json::json!(config))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pricer() -> SigmaUsdPricer {
        SigmaUsdPricer::with_exchange_rate(2_222_222_222) // ~0.45 ERG/USD
    }

    #[test]
    fn test_new_pricer() {
        let pricer = make_pricer();
        let stats = pricer.get_stats();
        assert_eq!(stats.mode, PricingMode::ErgoNative);
        assert_eq!(stats.exchange_rate, 2_222_222_222);
    }

    #[test]
    fn test_set_pricing_mode() {
        let pricer = make_pricer();
        pricer.set_pricing_mode(PricingMode::SigmaUsd);
        assert_eq!(pricer.get_pricing_mode(), PricingMode::SigmaUsd);

        pricer.set_pricing_mode(PricingMode::Hybrid);
        assert_eq!(pricer.get_pricing_mode(), PricingMode::Hybrid);
    }

    #[test]
    fn test_get_quote() {
        let pricer = make_pricer();
        let request = QuoteRequest {
            model_id: "llama-7b".to_string(),
            tokens_millions: 1.0,
            preferred_mode: None,
            slippage_override: None,
        };

        let quote = pricer.get_quote(request).unwrap();
        assert!(!quote.quote_id.is_empty());
        assert_eq!(quote.model_id, "llama-7b");
        assert!(quote.price_usd > 0.0);
        assert!(quote.price_erg > 0.0);
        assert!(quote.valid_until > quote.created_at);
    }

    #[test]
    fn test_quote_with_custom_rule() {
        let pricer = make_pricer();
        pricer.add_pricing_rule(PricingRule::new("custom-model", 0.50, 5.0));

        let request = QuoteRequest {
            model_id: "custom-model".to_string(),
            tokens_millions: 2.0,
            preferred_mode: None,
            slippage_override: None,
        };

        let quote = pricer.get_quote(request).unwrap();
        // Base price: $0.50 * 2M = $1.00 USD
        assert!((quote.price_usd - 1.0).abs() < 0.01);
        // ERG price should include 5% adjustment
        assert!(quote.price_erg > 0.0);
    }

    #[test]
    fn test_quote_expiration() {
        let pricer = make_pricer();
        // Set very short TTL
        pricer.update_config(SigmaUsdConfig {
            quote_ttl_secs: 0,
            ..SigmaUsdConfig::default()
        });

        let request = QuoteRequest {
            model_id: "test".to_string(),
            tokens_millions: 1.0,
            preferred_mode: None,
            slippage_override: None,
        };

        let quote = pricer.get_quote(request).unwrap();
        // Small delay to ensure expiration
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(quote.is_expired());
    }

    #[test]
    fn test_update_exchange_rate() {
        let pricer = make_pricer();
        pricer.update_exchange_rate(1_000_000_000); // 1 ERG = 1 USD
        assert_eq!(pricer.get_exchange_rate(), 1_000_000_000);
    }

    #[test]
    fn test_update_exchange_rate_invalid() {
        let pricer = make_pricer();
        let original_rate = pricer.get_exchange_rate();
        pricer.update_exchange_rate(0); // Invalid
        assert_eq!(pricer.get_exchange_rate(), original_rate); // Unchanged
        pricer.update_exchange_rate(-1); // Invalid
        assert_eq!(pricer.get_exchange_rate(), original_rate); // Unchanged
    }

    #[test]
    fn test_convert_to_stable() {
        let pricer = make_pricer();
        let usd = pricer.convert_to_stable(2.222_222_222); // ~2.22 ERG
        assert!((usd - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_convert_from_stable() {
        let pricer = make_pricer();
        let erg = pricer.convert_from_stable(1.0); // $1 USD
        assert!((erg - 2.222_222_222).abs() < 0.01);
    }

    #[test]
    fn test_add_and_remove_rule() {
        let pricer = make_pricer();
        pricer.add_pricing_rule(PricingRule::new("model-a", 0.25, 0.0));
        assert!(pricer.get_model_rule("model-a").is_some());

        pricer.remove_rule("model-a");
        assert!(pricer.get_model_rule("model-a").is_none());
    }

    #[test]
    fn test_get_model_price() {
        let pricer = make_pricer();
        pricer.add_pricing_rule(PricingRule::new("model-b", 0.30, 0.0));

        let (usd, erg) = pricer.get_model_price("model-b", 1.0).unwrap();
        assert!((usd - 0.30).abs() < 0.01);
        assert!(erg > 0.0);
    }

    #[test]
    fn test_get_stats() {
        let pricer = make_pricer();
        pricer.set_pricing_mode(PricingMode::Hybrid);
        pricer.add_pricing_rule(PricingRule::new("m1", 0.10, 0.0));
        pricer.add_pricing_rule(PricingRule::new("m2", 0.20, 0.0));

        let request = QuoteRequest {
            model_id: "m1".to_string(),
            tokens_millions: 1.0,
            preferred_mode: None,
            slippage_override: None,
        };
        let _ = pricer.get_quote(request);

        let stats = pricer.get_stats();
        assert_eq!(stats.mode, PricingMode::Hybrid);
        assert_eq!(stats.active_rules, 2);
        assert_eq!(stats.total_quotes, 1);
        assert_eq!(stats.valid_quotes, 1);
    }

    #[test]
    fn test_cleanup_expired_quotes() {
        let pricer = make_pricer();
        pricer.update_config(SigmaUsdConfig {
            quote_ttl_secs: 0,
            ..SigmaUsdConfig::default()
        });

        let request = QuoteRequest {
            model_id: "test".to_string(),
            tokens_millions: 1.0,
            preferred_mode: None,
            slippage_override: None,
        };
        let _ = pricer.get_quote(request);
        std::thread::sleep(std::time::Duration::from_millis(10));

        let removed = pricer.cleanup_expired_quotes();
        assert!(removed >= 1);
    }

    #[test]
    fn test_quote_with_slippage() {
        let pricer = make_pricer();
        let request = QuoteRequest {
            model_id: "test".to_string(),
            tokens_millions: 1.0,
            preferred_mode: None,
            slippage_override: Some(5.0),
        };

        let quote = pricer.get_quote(request).unwrap();
        assert!((quote.slippage_pct - 5.0).abs() < 0.01);
        assert!(quote.max_acceptable_erg() > quote.price_erg);
        assert!(quote.min_acceptable_erg() < quote.price_erg);
    }

    #[test]
    fn test_effective_erg_price() {
        let pricer = make_pricer();
        let request = QuoteRequest {
            model_id: "test".to_string(),
            tokens_millions: 1.0,
            preferred_mode: None,
            slippage_override: Some(10.0),
        };

        let quote = pricer.get_quote(request).unwrap();
        let effective = quote.effective_erg_price();
        assert!(effective > quote.price_erg);
        assert!((effective - quote.price_erg * 1.1).abs() < 0.01);
    }

    #[test]
    fn test_disabled_stable_pricing() {
        let pricer = make_pricer();
        pricer.set_stable_pricing(false);

        let request = QuoteRequest {
            model_id: "test".to_string(),
            tokens_millions: 1.0,
            preferred_mode: Some(PricingMode::SigmaUsd),
            slippage_override: None,
        };

        let result = pricer.get_quote(request);
        assert!(result.is_err());
    }
}
