//! Oracle price feed integration for the Xergon Network agent.
//!
//! Implements an EIP-23 oracle pool adapter for real-time ERG-denominated pricing
//! of inference services, model costs, and provider payouts. Reads on-chain
//! oracle pool state (PoolBox, RefreshBox, OracleBox) and computes median
//! prices with deviation filtering per the EIP-23 specification.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Oracle feed status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OracleStatus {
    Active,
    Inactive,
    Slashed,
    Bootstrapping,
}

impl Default for OracleStatus {
    fn default() -> Self {
        Self::Bootstrapping
    }
}

/// Supported price pairs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PricePair {
    ErgUsd,
    ErgBtc,
    ErgAda,
    ErgEth,
    XrgUsd,
    ErgEur,
    ErgSol,
}

impl PricePair {
    pub fn pair_str(&self) -> &'static str {
        match self {
            PricePair::ErgUsd => "ERG/USD",
            PricePair::ErgBtc => "ERG/BTC",
            PricePair::ErgAda => "ERG/ADA",
            PricePair::ErgEth => "ERG/ETH",
            PricePair::XrgUsd => "XRG/USD",
            PricePair::ErgEur => "ERG/EUR",
            PricePair::ErgSol => "ERG/SOL",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "ERG/USD" => Some(PricePair::ErgUsd),
            "ERG/BTC" => Some(PricePair::ErgBtc),
            "ERG/ADA" => Some(PricePair::ErgAda),
            "ERG/ETH" => Some(PricePair::ErgEth),
            "XRG/USD" => Some(PricePair::XrgUsd),
            "ERG/EUR" => Some(PricePair::ErgEur),
            "ERG/SOL" => Some(PricePair::ErgSol),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Oracle pool configuration (EIP-23 parameters).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleConfig {
    pub pool_nft_id: String,
    pub refresh_token_id: String,
    pub update_token_id: String,
    pub min_data_points: usize,
    pub max_deviation_percent: f64,
    pub epoch_length_blocks: u32,
    pub reward_token_id: Option<String>,
    pub node_url: String,
    pub max_price_age_secs: u64,
    pub fallback_price: f64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            pool_nft_id: "xergon-oracle-pool-nft".to_string(),
            refresh_token_id: "xergon-refresh-token".to_string(),
            update_token_id: "xergon-update-token".to_string(),
            min_data_points: 6,
            max_deviation_percent: 15.0,
            epoch_length_blocks: 30,
            reward_token_id: Some("xergon-reward-token".to_string()),
            node_url: "http://127.0.0.1:9053".to_string(),
            max_price_age_secs: 3600,
            fallback_price: 0.50,
        }
    }
}

/// On-chain PoolBox state (EIP-23).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolBoxState {
    pub box_id: String,
    pub pool_nft_id: String,
    pub current_price: i64,       // nanoERG per unit
    pub epoch_counter: i32,
    pub last_refresh_height: i32,
    pub box_value: u64,
    pub oracle_count: u32,
}

/// On-chain RefreshBox state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshBoxState {
    pub box_id: String,
    pub refresh_token_id: String,
    pub min_data_points: usize,
    pub max_deviation_percent: f64,
    pub epoch_length: u32,
}

/// On-chain OracleBox state per oracle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleBoxState {
    pub box_id: String,
    pub oracle_token_id: String,
    pub public_key: String,
    pub epoch_counter: i32,
    pub posted_value: Option<i64>,
    pub reward_token_amount: u64,
    pub last_post_height: Option<i32>,
    pub status: OracleStatus,
}

/// On-chain BallotBox state for governance votes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallotBoxState {
    pub box_id: String,
    pub ballot_token_id: String,
    pub oracle_public_key: String,
    pub proposed_hash: Option<String>,
    pub vote_height: Option<i32>,
}

/// On-chain UpdateBox state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateBoxState {
    pub box_id: String,
    pub update_token_id: String,
    pub min_votes: u32,
    pub proposed_contract_hash: String,
}

/// Cached price with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPrice {
    pub pair: String,
    pub price: f64,
    pub source: String,
    pub timestamp: u64,
    pub epoch: i32,
    pub confidence: f64,
    pub oracle_count: u32,
    pub deviation_percent: f64,
}

/// Inference cost calculation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceCost {
    pub model_id: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_nanoerg: u64,
    pub cost_erg: f64,
    pub cost_usd: f64,
    pub price_pair: String,
    pub price_at_calculation: f64,
    pub timestamp: u64,
}

/// Oracle feed health summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleHealth {
    pub active_oracles: usize,
    pub total_oracles: usize,
    pub current_epoch: i32,
    pub pool_age_blocks: i32,
    pub last_refresh_height: i32,
    pub avg_deviation: f64,
    pub cached_pairs: usize,
    pub staleness_secs: u64,
}

// ---------------------------------------------------------------------------
// State (thread-safe via DashMap)
// ---------------------------------------------------------------------------

/// Shared oracle feed state.
pub struct OracleState {
    pub config: DashMap<String, OracleConfig>,
    pub pool_box: DashMap<String, PoolBoxState>,
    pub refresh_box: DashMap<String, RefreshBoxState>,
    pub oracle_boxes: DashMap<String, OracleBoxState>,
    pub ballot_boxes: DashMap<String, BallotBoxState>,
    pub update_box: DashMap<String, UpdateBoxState>,
    pub price_cache: DashMap<String, CachedPrice>,
    pub metrics: DashMap<String, u64>,
    pub current_height: AtomicU64,
}

impl OracleState {
    pub fn new() -> Self {
        let state = Self {
            config: DashMap::new(),
            pool_box: DashMap::new(),
            refresh_box: DashMap::new(),
            oracle_boxes: DashMap::new(),
            ballot_boxes: DashMap::new(),
            update_box: DashMap::new(),
            price_cache: DashMap::new(),
            metrics: DashMap::new(),
            current_height: AtomicU64::new(0),
        };
        state.config.insert("default".to_string(), OracleConfig::default());
        state
    }
}

impl Default for OracleState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Business Logic
// ---------------------------------------------------------------------------

impl OracleState {
    /// Compute median price from valid oracle datapoints with deviation filtering.
    pub fn compute_median_price(&self, _oracle_id: &str) -> Result<(f64, f64, usize), String> {
        let config = self.config.get("default").unwrap();
        let pool = self.pool_box.get("main").ok_or("No pool box")?;
        let current_epoch = pool.epoch_counter;

        let mut values: Vec<i64> = self.oracle_boxes.iter()
            .filter(|o| {
                o.epoch_counter == current_epoch
                    && o.posted_value.is_some()
                    && o.status == OracleStatus::Active
            })
            .map(|o| o.posted_value.unwrap())
            .collect();

        if values.len() < config.min_data_points {
            return Err(format!(
                "Not enough datapoints: {} < {}",
                values.len(),
                config.min_data_points
            ));
        }

        values.sort();
        let median = values[values.len() / 2];
        let median_f = median as f64;

        // Filter by max deviation
        let valid: Vec<i64> = values.iter()
            .filter(|v| {
                let deviation = ((**v as f64 - median_f).abs() / median_f) * 100.0;
                deviation <= config.max_deviation_percent
            })
            .copied()
            .collect();

        let final_median = if valid.is_empty() { median } else { valid[valid.len() / 2] };
        let avg_dev = if median_f > 0.0 {
            values.iter().map(|v| ((v - median).abs() as f64 / median_f) * 100.0).sum::<f64>() / values.len() as f64
        } else { 0.0 };

        Ok((final_median as f64, avg_dev, valid.len()))
    }

    /// Calculate inference cost in ERG from token counts using oracle prices.
    pub fn calculate_inference_cost(
        &self,
        model_id: &str,
        tokens_in: u32,
        tokens_out: u32,
    ) -> InferenceCost {
        let config = self.config.get("default").unwrap();
        let cached = self.price_cache.get("ERG/USD").map(|c| c.price).unwrap_or(config.fallback_price);
        let erg_per_1k_input = 0.0001; // base rate
        let erg_per_1k_output = 0.0002;

        let cost_erg = (tokens_in as f64 / 1000.0) * erg_per_1k_input
            + (tokens_out as f64 / 1000.0) * erg_per_1k_output;
        let cost_nanoerg = (cost_erg * 1_000_000_000.0) as u64;
        let cost_usd = cost_erg * cached;

        InferenceCost {
            model_id: model_id.to_string(),
            tokens_in,
            tokens_out,
            cost_nanoerg,
            cost_erg,
            cost_usd,
            price_pair: "ERG/USD".to_string(),
            price_at_calculation: cached,
            timestamp: now_secs(),
        }
    }

    /// Validate a datapoint submission.
    pub fn validate_data_point(&self, oracle_id: &str, value: i64) -> Result<(), String> {
        if !self.oracle_boxes.contains_key(oracle_id) {
            return Err(format!("Unknown oracle: {}", oracle_id));
        }
        if value <= 0 {
            return Err("Value must be positive".into());
        }
        if value > i64::MAX / 2 {
            return Err("Value exceeds reasonable range".into());
        }
        Ok(())
    }

    /// Check if quorum (min_data_points) is met for current epoch.
    pub fn check_quorum(&self) -> (bool, usize, usize) {
        let config = self.config.get("default").unwrap();
        let pool = self.pool_box.get("main");
        let current_epoch = pool.as_ref().map(|p| p.epoch_counter).unwrap_or(0);

        let count = self.oracle_boxes.iter()
            .filter(|o| o.epoch_counter == current_epoch && o.posted_value.is_some())
            .count();

        (count >= config.min_data_points, count, config.min_data_points)
    }

    /// Compute epoch number from block height.
    pub fn epoch_from_height(&self, height: u64) -> i32 {
        let config = self.config.get("default").unwrap();
        height as i32 / config.epoch_length_blocks as i32
    }

    /// Get oracle feed health.
    pub fn get_health(&self) -> OracleHealth {
        let active = self.oracle_boxes.iter().filter(|o| o.status == OracleStatus::Active).count();
        let total = self.oracle_boxes.len();
        let epoch = self.pool_box.get("main").map(|p| p.epoch_counter).unwrap_or(0);
        let last_refresh = self.pool_box.get("main").map(|p| p.last_refresh_height).unwrap_or(0);
        let current_height = self.current_height.load(Ordering::Relaxed) as i32;
        let pool_age = current_height.saturating_sub(last_refresh);
        let pairs = self.price_cache.len();

        let avg_dev = self.price_cache.iter()
            .map(|c| c.deviation_percent)
            .sum::<f64>() / pairs.max(1) as f64;

        let newest = self.price_cache.iter()
            .map(|c| c.timestamp)
            .max()
            .unwrap_or(0);
        let staleness = now_secs().saturating_sub(newest);

        OracleHealth {
            active_oracles: active,
            total_oracles: total,
            current_epoch: epoch,
            pool_age_blocks: pool_age,
            last_refresh_height: last_refresh,
            avg_deviation: avg_dev,
            cached_pairs: pairs,
            staleness_secs: staleness,
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
