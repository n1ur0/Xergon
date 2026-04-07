//! Babel Fee Integration — Ergo transaction fee calculation and management
//!
//! Implements Ergo's fee model for inference requests:
//! - Minimum fee: 0.001 ERG (1,000,000 nanoERG)
//! - Size-dependent fees: 360 nanoERG/byte
//! - Tiered pricing: Standard, Express, Batch, Free
//! - Bulk discounts for high-volume users
//! - Batch fee settlement for aggregated payments

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Ergo nanoERG per ERG
pub const NANOERG_PER_ERG: i64 = 1_000_000_000;

/// Recommended minimum fee in nanoERG (0.001 ERG)
pub const MIN_FEE_NANOERG: i64 = 1_000_000;

/// Ergo byte cost: 360 nanoERG per byte
pub const BYTE_COST_NANOERG: i64 = 360;

/// Default maximum fee records to retain
const MAX_FEE_RECORDS: usize = 100_000;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Fee tier for inference requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeeTier {
    /// Standard processing — base fee rate
    Standard,
    /// Express/priority — 2x base fee rate
    Express,
    /// Batch processing — 0.5x base fee rate
    Batch,
    /// Free tier — no fee (rate-limited)
    Free,
}

impl Default for FeeTier {
    fn default() -> Self {
        FeeTier::Standard
    }
}

impl FeeTier {
    /// Returns the multiplier applied to the base fee for this tier.
    pub fn multiplier(&self) -> f64 {
        match self {
            FeeTier::Standard => 1.0,
            FeeTier::Express => 2.0,
            FeeTier::Batch => 0.5,
            FeeTier::Free => 0.0,
        }
    }

    /// Parse a fee tier from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "express" | "priority" => FeeTier::Express,
            "batch" => FeeTier::Batch,
            "free" => FeeTier::Free,
            _ => FeeTier::Standard,
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Babel fee engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BabelFeeConfig {
    /// Minimum fee in nanoERG (default: 1,000,000 = 0.001 ERG)
    pub min_fee_nanoerg: i64,
    /// Cost per byte of request payload in nanoERG (default: 360)
    pub byte_cost: i64,
    /// Tier-specific fee multipliers (tier_name -> multiplier)
    pub tiers: HashMap<String, f64>,
    /// Accumulated nanoERG threshold for bulk discount eligibility
    pub discount_threshold: i64,
    /// Discount percentage (0.0 - 1.0) applied when threshold is met
    pub discount_pct: f64,
    /// Maximum fee in nanoERG (cap to prevent runaway fees)
    pub max_fee_nanoerg: i64,
    /// Whether fee integration is enabled
    pub enabled: bool,
}

impl Default for BabelFeeConfig {
    fn default() -> Self {
        let mut tiers = HashMap::new();
        tiers.insert("standard".to_string(), 1.0);
        tiers.insert("express".to_string(), 2.0);
        tiers.insert("batch".to_string(), 0.5);
        tiers.insert("free".to_string(), 0.0);

        BabelFeeConfig {
            min_fee_nanoerg: MIN_FEE_NANOERG,
            byte_cost: BYTE_COST_NANOERG,
            tiers,
            discount_threshold: 100_000_000, // 0.1 ERG accumulated
            discount_pct: 0.10,              // 10% discount
            max_fee_nanoerg: 10_000_000,     // 0.01 ERG max
            enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Fee Estimate
// ---------------------------------------------------------------------------

/// Result of a fee estimation calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimate {
    /// Base fee before tier multiplier (nanoERG)
    pub base_fee: i64,
    /// Size-dependent fee component (nanoERG)
    pub size_fee: i64,
    /// Total fee in nanoERG
    pub total_fee_nanoerg: i64,
    /// Total fee in ERG (floating point)
    pub total_fee_erg: f64,
    /// Fee tier applied
    pub tier: FeeTier,
    /// Detailed breakdown of fee components
    pub breakdown: HashMap<String, i64>,
    /// Whether a bulk discount was applied
    pub discount_applied: bool,
    /// Discount amount in nanoERG
    pub discount_nanoerg: i64,
}

// ---------------------------------------------------------------------------
// Fee Record
// ---------------------------------------------------------------------------

/// A recorded fee transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeRecord {
    /// Unique fee record identifier
    pub fee_id: String,
    /// Associated request ID
    pub request_id: String,
    /// Provider that received the fee
    pub provider_id: String,
    /// User who paid the fee
    pub user_id: String,
    /// Fee amount in nanoERG
    pub amount_nanoerg: i64,
    /// Fee tier used
    pub tier: FeeTier,
    /// When the fee was recorded
    pub timestamp: DateTime<Utc>,
    /// Optional Ergo transaction ID for on-chain settlement
    pub tx_id: Option<String>,
    /// Whether this fee has been settled on-chain
    pub settled: bool,
}

// ---------------------------------------------------------------------------
// Fee Accumulator
// ---------------------------------------------------------------------------

/// Per-user fee accumulator for bulk discount tracking.
#[derive(Debug, Default)]
struct FeeAccumulator {
    /// Total accumulated fees in nanoERG
    total: AtomicI64,
    /// Number of fee records
    count: AtomicI64,
}

impl FeeAccumulator {
    fn new() -> Self {
        Self::default()
    }

    fn add(&self, amount_nanoerg: i64) {
        self.total.fetch_add(amount_nanoerg, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    fn get_total(&self) -> i64 {
        self.total.load(Ordering::Relaxed)
    }

    fn get_count(&self) -> i64 {
        self.count.load(Ordering::Relaxed)
    }

    fn reset(&self) {
        self.total.store(0, Ordering::Relaxed);
        self.count.store(0, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Babel Fee Manager
// ---------------------------------------------------------------------------

/// Core fee management engine for Ergo transaction fees.
///
/// Thread-safe via DashMap for fee records and RwLock for configuration.
/// Tracks per-user accumulated fees for bulk discount eligibility.
pub struct BabelFeeManager {
    /// Fee records indexed by fee_id
    fee_records: DashMap<String, FeeRecord>,
    /// Per-user fee accumulators
    user_accumulators: DashMap<String, FeeAccumulator>,
    /// Fee configuration
    config: Arc<std::sync::RwLock<BabelFeeConfig>>,
}

impl BabelFeeManager {
    /// Create a new fee manager with default configuration.
    pub fn new() -> Self {
        Self {
            fee_records: DashMap::new(),
            user_accumulators: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(BabelFeeConfig::default())),
        }
    }

    /// Create a new fee manager with custom configuration.
    pub fn with_config(config: BabelFeeConfig) -> Self {
        Self {
            fee_records: DashMap::new(),
            user_accumulators: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(config)),
        }
    }

    // -----------------------------------------------------------------------
    // Core fee calculation
    // -----------------------------------------------------------------------

    /// Estimate the fee for a request without recording it.
    pub fn estimate_fee(
        &self,
        request_id: &str,
        payload_size_bytes: usize,
        tier: FeeTier,
        user_id: &str,
    ) -> FeeEstimate {
        let config = self.config.read().unwrap();

        if !config.enabled {
            return FeeEstimate {
                base_fee: 0,
                size_fee: 0,
                total_fee_nanoerg: 0,
                total_fee_erg: 0.0,
                tier,
                breakdown: HashMap::new(),
                discount_applied: false,
                discount_nanoerg: 0,
            };
        }

        // Calculate base fee (minimum fee)
        let base_fee = config.min_fee_nanoerg;

        // Calculate size-dependent fee
        let size_fee = (payload_size_bytes as i64) * config.byte_cost;

        // Get tier multiplier
        let tier_mult = tier.multiplier();

        // Calculate raw total before discount
        let raw_total = ((base_fee + size_fee) as f64 * tier_mult) as i64;

        // Check for bulk discount eligibility
        let user_total = self
            .user_accumulators
            .get(user_id)
            .map(|a| a.get_total())
            .unwrap_or(0);

        let (discount_applied, discount_nanoerg) = if user_total >= config.discount_threshold {
            let discount = (raw_total as f64 * config.discount_pct) as i64;
            (true, discount)
        } else {
            (false, 0)
        };

        let total = raw_total - discount_nanoerg;

        // Apply cap
        let total_fee_nanoerg = total.min(config.max_fee_nanoerg);

        let mut breakdown = HashMap::new();
        breakdown.insert("base_fee".to_string(), base_fee);
        breakdown.insert("size_fee".to_string(), size_fee);
        breakdown.insert("tier_multiplier".to_string(), (tier_mult * 100.0) as i64);
        if discount_applied {
            breakdown.insert("discount".to_string(), -discount_nanoerg);
        }

        FeeEstimate {
            base_fee,
            size_fee,
            total_fee_nanoerg,
            total_fee_erg: total_fee_nanoerg as f64 / NANOERG_PER_ERG as f64,
            tier,
            breakdown,
            discount_applied,
            discount_nanoerg,
        }
    }

    /// Calculate fee for a given payload size and tier.
    pub fn calculate_fee(&self, payload_size_bytes: usize, tier: FeeTier) -> i64 {
        self.estimate_fee("", payload_size_bytes, tier, "")
            .total_fee_nanoerg
    }

    /// Record a fee after it has been paid.
    pub fn record_fee(
        &self,
        request_id: &str,
        provider_id: &str,
        user_id: &str,
        amount_nanoerg: i64,
        tier: FeeTier,
        tx_id: Option<String>,
    ) -> FeeRecord {
        let fee_id = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now();

        let record = FeeRecord {
            fee_id: fee_id.clone(),
            request_id: request_id.to_string(),
            provider_id: provider_id.to_string(),
            user_id: user_id.to_string(),
            amount_nanoerg,
            tier,
            timestamp,
            tx_id: tx_id.clone(),
            settled: tx_id.is_some(),
        };

        // Evict oldest records if at capacity
        if self.fee_records.len() >= MAX_FEE_RECORDS {
            let oldest_key = self
                .fee_records
                .iter()
                .min_by_key(|e| e.value().timestamp)
                .map(|e| e.key().clone());
            if let Some(key) = oldest_key {
                self.fee_records.remove(&key);
            }
        }

        self.fee_records.insert(fee_id.clone(), record.clone());

        // Update user accumulator
        let accumulator = self
            .user_accumulators
            .entry(user_id.to_string())
            .or_insert_with(FeeAccumulator::new);
        accumulator.add(amount_nanoerg);

        debug!(
            fee_id = %fee_id,
            request_id = %request_id,
            amount_nanoerg,
            tier = ?tier,
            "Fee recorded"
        );

        record
    }

    /// Get fee history for a specific user.
    pub fn get_fee_history(&self, user_id: &str, limit: usize) -> Vec<FeeRecord> {
        let mut records: Vec<FeeRecord> = self
            .fee_records
            .iter()
            .filter(|e| e.value().user_id == user_id)
            .map(|e| e.value().clone())
            .collect();

        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        records.truncate(limit);
        records
    }

    /// Get accumulated fee totals for a user.
    pub fn get_accumulated_fees(&self, user_id: &str) -> (i64, i64) {
        let accumulator = self
            .user_accumulators
            .get(user_id)
            .map(|a| (a.get_total(), a.get_count()))
            .unwrap_or((0, 0));
        accumulator
    }

    /// Check if a user is eligible for a bulk discount.
    pub fn is_discount_eligible(&self, user_id: &str) -> bool {
        let config = self.config.read().unwrap();
        let total = self
            .user_accumulators
            .get(user_id)
            .map(|a| a.get_total())
            .unwrap_or(0);
        total >= config.discount_threshold
    }

    /// Apply a discount to a fee amount based on user's accumulated fees.
    pub fn apply_discount(&self, user_id: &str, amount_nanoerg: i64) -> i64 {
        if !self.is_discount_eligible(user_id) {
            return amount_nanoerg;
        }
        let config = self.config.read().unwrap();
        let discount = (amount_nanoerg as f64 * config.discount_pct) as i64;
        amount_nanoerg - discount
    }

    /// Get the current fee configuration.
    pub fn get_config(&self) -> BabelFeeConfig {
        self.config.read().unwrap().clone()
    }

    /// Update the fee configuration.
    pub fn update_config(&self, new_config: BabelFeeConfig) {
        info!(
            min_fee = new_config.min_fee_nanoerg,
            byte_cost = new_config.byte_cost,
            discount_threshold = new_config.discount_threshold,
            "Fee config updated"
        );
        *self.config.write().unwrap() = new_config;
    }

    /// Batch settle multiple fee records with a single transaction ID.
    /// Returns the number of records settled and total amount.
    pub fn batch_settle(
        &self,
        fee_ids: &[String],
        tx_id: &str,
    ) -> (usize, i64) {
        let mut settled_count = 0;
        let mut total_amount = 0i64;

        for fee_id in fee_ids {
            if let Some(mut record) = self.fee_records.get_mut(fee_id) {
                if !record.settled {
                    record.settled = true;
                    record.tx_id = Some(tx_id.to_string());
                    total_amount += record.amount_nanoerg;
                    settled_count += 1;
                }
            }
        }

        info!(
            settled_count,
            total_amount_nanoerg = total_amount,
            tx_id = %tx_id,
            "Batch fee settlement complete"
        );

        (settled_count, total_amount)
    }

    /// Get total number of fee records.
    pub fn record_count(&self) -> usize {
        self.fee_records.len()
    }

    /// Get total unsettled fee amount.
    pub fn unsettled_total(&self) -> i64 {
        self.fee_records
            .iter()
            .filter(|e| !e.value().settled)
            .map(|e| e.value().amount_nanoerg)
            .sum()
    }

    /// Prune fee records older than the given timestamp.
    pub fn prune_before(&self, before: DateTime<Utc>) -> usize {
        let keys: Vec<String> = self
            .fee_records
            .iter()
            .filter(|e| e.value().timestamp < before)
            .map(|e| e.key().clone())
            .collect();

        let count = keys.len();
        for key in keys {
            self.fee_records.remove(&key);
        }

        if count > 0 {
            info!(pruned = count, "Pruned old fee records");
        }
        count
    }
}

impl Default for BabelFeeManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API Request/Response Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EstimateFeeRequest {
    pub request_id: Option<String>,
    pub payload_size_bytes: usize,
    #[serde(default)]
    pub tier: String,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordFeeRequest {
    pub request_id: String,
    pub provider_id: String,
    pub user_id: String,
    pub amount_nanoerg: i64,
    #[serde(default)]
    pub tier: String,
    pub tx_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchSettleRequest {
    pub fee_ids: Vec<String>,
    pub tx_id: String,
}

#[derive(Debug, Serialize)]
pub struct FeeHistoryResponse {
    pub user_id: String,
    pub records: Vec<FeeRecord>,
    pub total_records: usize,
}

#[derive(Debug, Serialize)]
pub struct AccumulatedFeesResponse {
    pub user_id: String,
    pub total_nanoerg: i64,
    pub total_erg: f64,
    pub record_count: i64,
    pub discount_eligible: bool,
}

#[derive(Debug, Serialize)]
pub struct BatchSettleResponse {
    pub settled_count: usize,
    pub total_amount_nanoerg: i64,
    pub total_amount_erg: f64,
    pub tx_id: String,
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

/// POST /v1/fees/estimate — Estimate fee for a request
async fn estimate_fee_handler(
    State(state): State<AppState>,
    Json(req): Json<EstimateFeeRequest>,
) -> impl IntoResponse {
    let tier = FeeTier::from_str_loose(&req.tier);
    let user_id = req.user_id.as_deref().unwrap_or("");
    let request_id = req.request_id.as_deref().unwrap_or("");

    let estimate = state.babel_fee_manager.estimate_fee(
        request_id,
        req.payload_size_bytes,
        tier,
        user_id,
    );

    (StatusCode::OK, Json(estimate)).into_response()
}

/// POST /v1/fees/record — Record a paid fee
async fn record_fee_handler(
    State(state): State<AppState>,
    Json(req): Json<RecordFeeRequest>,
) -> impl IntoResponse {
    let tier = FeeTier::from_str_loose(&req.tier);

    let record = state.babel_fee_manager.record_fee(
        &req.request_id,
        &req.provider_id,
        &req.user_id,
        req.amount_nanoerg,
        tier,
        req.tx_id,
    );

    (StatusCode::CREATED, Json(record)).into_response()
}

/// GET /v1/fees/history/:user_id — Get fee history for a user
async fn get_fee_history_handler(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let records = state.babel_fee_manager.get_fee_history(&user_id, 100);
    let response = FeeHistoryResponse {
        user_id: user_id.clone(),
        total_records: records.len(),
        records,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/fees/accumulated/:user_id — Get accumulated fee totals
async fn get_accumulated_fees_handler(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let (total_nanoerg, record_count) = state.babel_fee_manager.get_accumulated_fees(&user_id);
    let discount_eligible = state.babel_fee_manager.is_discount_eligible(&user_id);

    let response = AccumulatedFeesResponse {
        user_id: user_id.clone(),
        total_nanoerg,
        total_erg: total_nanoerg as f64 / NANOERG_PER_ERG as f64,
        record_count,
        discount_eligible,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /v1/fees/batch-settle — Batch settle fee records
async fn batch_settle_handler(
    State(state): State<AppState>,
    Json(req): Json<BatchSettleRequest>,
) -> impl IntoResponse {
    let (settled_count, total_amount) =
        state.babel_fee_manager.batch_settle(&req.fee_ids, &req.tx_id);

    let response = BatchSettleResponse {
        settled_count,
        total_amount_nanoerg: total_amount,
        total_amount_erg: total_amount as f64 / NANOERG_PER_ERG as f64,
        tx_id: req.tx_id,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/fees/config — Get current fee configuration
async fn get_fee_config_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let config = state.babel_fee_manager.get_config();
    (StatusCode::OK, Json(config)).into_response()
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the fee integration router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/fees/estimate", post(estimate_fee_handler))
        .route("/v1/fees/record", post(record_fee_handler))
        .route(
            "/v1/fees/history/{user_id}",
            get(get_fee_history_handler),
        )
        .route(
            "/v1/fees/accumulated/{user_id}",
            get(get_accumulated_fees_handler),
        )
        .route("/v1/fees/batch-settle", post(batch_settle_handler))
        .route("/v1/fees/config", get(get_fee_config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> BabelFeeManager {
        BabelFeeManager::new()
    }

    #[test]
    fn test_fee_tier_default_is_standard() {
        assert_eq!(FeeTier::default(), FeeTier::Standard);
    }

    #[test]
    fn test_fee_tier_multipliers() {
        assert_eq!(FeeTier::Standard.multiplier(), 1.0);
        assert_eq!(FeeTier::Express.multiplier(), 2.0);
        assert_eq!(FeeTier::Batch.multiplier(), 0.5);
        assert_eq!(FeeTier::Free.multiplier(), 0.0);
    }

    #[test]
    fn test_fee_tier_from_str() {
        assert_eq!(FeeTier::from_str_loose("express"), FeeTier::Express);
        assert_eq!(FeeTier::from_str_loose("priority"), FeeTier::Express);
        assert_eq!(FeeTier::from_str_loose("batch"), FeeTier::Batch);
        assert_eq!(FeeTier::from_str_loose("free"), FeeTier::Free);
        assert_eq!(FeeTier::from_str_loose("standard"), FeeTier::Standard);
        assert_eq!(FeeTier::from_str_loose("unknown"), FeeTier::Standard);
    }

    #[test]
    fn test_estimate_fee_standard_tier() {
        let mgr = make_manager();
        let estimate = mgr.estimate_fee("req-1", 100, FeeTier::Standard, "user-1");

        // base_fee = 1_000_000, size_fee = 100 * 360 = 36_000
        // total = (1_000_000 + 36_000) * 1.0 = 1_036_000
        assert_eq!(estimate.base_fee, MIN_FEE_NANOERG);
        assert_eq!(estimate.size_fee, 36_000);
        assert_eq!(estimate.total_fee_nanoerg, 1_036_000);
        assert_eq!(estimate.tier, FeeTier::Standard);
        assert!(!estimate.discount_applied);
    }

    #[test]
    fn test_estimate_fee_express_tier() {
        let mgr = make_manager();
        let estimate = mgr.estimate_fee("req-2", 100, FeeTier::Express, "user-1");

        // total = (1_000_000 + 36_000) * 2.0 = 2_072_000
        assert_eq!(estimate.total_fee_nanoerg, 2_072_000);
        assert_eq!(estimate.tier, FeeTier::Express);
    }

    #[test]
    fn test_estimate_fee_free_tier() {
        let mgr = make_manager();
        let estimate = mgr.estimate_fee("req-3", 100, FeeTier::Free, "user-1");

        assert_eq!(estimate.total_fee_nanoerg, 0);
        assert_eq!(estimate.tier, FeeTier::Free);
    }

    #[test]
    fn test_record_fee() {
        let mgr = make_manager();
        let record = mgr.record_fee("req-1", "provider-1", "user-1", 1_000_000, FeeTier::Standard, None);

        assert_eq!(record.request_id, "req-1");
        assert_eq!(record.provider_id, "provider-1");
        assert_eq!(record.user_id, "user-1");
        assert_eq!(record.amount_nanoerg, 1_000_000);
        assert!(!record.settled);
        assert!(record.tx_id.is_none());
        assert_eq!(mgr.record_count(), 1);
    }

    #[test]
    fn test_accumulated_fees() {
        let mgr = make_manager();
        mgr.record_fee("req-1", "p1", "user-1", 500_000, FeeTier::Standard, None);
        mgr.record_fee("req-2", "p1", "user-1", 300_000, FeeTier::Standard, None);
        mgr.record_fee("req-3", "p1", "user-2", 200_000, FeeTier::Standard, None);

        let (total, count) = mgr.get_accumulated_fees("user-1");
        assert_eq!(total, 800_000);
        assert_eq!(count, 2);

        let (total2, count2) = mgr.get_accumulated_fees("user-2");
        assert_eq!(total2, 200_000);
        assert_eq!(count2, 1);
    }

    #[test]
    fn test_bulk_discount_eligibility() {
        let mut config = BabelFeeConfig::default();
        config.discount_threshold = 500_000;
        let mgr = BabelFeeManager::with_config(config);

        // Not eligible yet
        assert!(!mgr.is_discount_eligible("user-1"));

        // Accumulate fees to cross threshold
        mgr.record_fee("r1", "p1", "user-1", 300_000, FeeTier::Standard, None);
        assert!(!mgr.is_discount_eligible("user-1"));

        mgr.record_fee("r2", "p1", "user-1", 300_000, FeeTier::Standard, None);
        assert!(mgr.is_discount_eligible("user-1"));
    }

    #[test]
    fn test_batch_settle() {
        let mgr = make_manager();
        let r1 = mgr.record_fee("r1", "p1", "u1", 100_000, FeeTier::Standard, None);
        let r2 = mgr.record_fee("r2", "p1", "u1", 200_000, FeeTier::Standard, None);

        let (count, total) = mgr.batch_settle(
            &[r1.fee_id.clone(), r2.fee_id.clone()],
            "tx-abc-123",
        );

        assert_eq!(count, 2);
        assert_eq!(total, 300_000);
    }

    #[test]
    fn test_disabled_fees_return_zero() {
        let mut config = BabelFeeConfig::default();
        config.enabled = false;
        let mgr = BabelFeeManager::with_config(config);

        let estimate = mgr.estimate_fee("r1", 1000, FeeTier::Standard, "u1");
        assert_eq!(estimate.total_fee_nanoerg, 0);
    }

    #[test]
    fn test_fee_history_limit() {
        let mgr = make_manager();
        for i in 0..20 {
            mgr.record_fee(&format!("r{}", i), "p1", "user-1", 100_000, FeeTier::Standard, None);
        }

        let history = mgr.get_fee_history("user-1", 5);
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_nanoerg_constants() {
        assert_eq!(NANOERG_PER_ERG, 1_000_000_000);
        assert_eq!(MIN_FEE_NANOERG, 1_000_000);
        assert_eq!(BYTE_COST_NANOERG, 360);
    }
}
