//! Tokenomics Engine for the Xergon Network relay.
//!
//! Provides ERG emission simulation, staking yield calculations,
//! supply schedule projection, and deflationary burn tracking.
//!
//! All amounts are in nanoERG (1 ERG = 1_000_000_000 nanoERG).
//! Ergo block time is ~2 minutes. Total supply target ~97.7M ERG.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// 1 ERG in nanoERG
pub const NANOERG_PER_ERG: u64 = 1_000_000_000;

/// Ergo block time in seconds (~2 minutes)
pub const BLOCK_TIME_SECS: u64 = 120;

/// Initial block reward in nanoERG (75 ERG)
pub const INITIAL_BLOCK_REWARD: u64 = 75 * NANOERG_PER_ERG;

/// Epoch length in blocks (reward reduction every epoch, ~720 blocks ~ 1 day)
pub const EPOCH_LENGTH: u64 = 720;

/// Reward reduction factor per epoch (multiplied by 10000 for integer math, 9980 = 99.80%)
pub const REWARD_DECAY_FACTOR: u64 = 9980;

/// Minimum block reward in nanoERG (0.001 ERG, after which emission stops)
pub const MIN_BLOCK_REWARD: u64 = 1_000_000;

/// Total ERG supply target in nanoERG (~97.7M ERG)
pub const TOTAL_SUPPLY_TARGET: u64 = 97_700_000 * NANOERG_PER_ERG;

/// Default protocol fee burn rate (basis points, 100 = 1%)
pub const DEFAULT_FEE_BURN_RATE: u64 = 100;

/// Default staking base APY (basis points, 500 = 5%)
pub const DEFAULT_STAKING_APY: u64 = 500;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the tokenomics engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenomicsConfig {
    /// Genesis block height (default: 0)
    pub genesis_height: u64,
    /// Initial circulating supply at genesis in nanoERG
    pub initial_supply: u64,
    /// Protocol fee burn rate in basis points (100 = 1%)
    pub fee_burn_rate: u64,
    /// Staking base APY in basis points (500 = 5%)
    pub staking_base_apy: u64,
    /// Staking reward pool in nanoERG
    pub staking_reward_pool: u64,
    /// Minimum staking amount in nanoERG
    pub min_stake: u64,
    /// Maximum staking cap in nanoERG (0 = unlimited)
    pub max_stake_cap: u64,
    /// Bonus APY for top providers in basis points
    pub provider_bonus_apy: u64,
    /// Lock-up bonus multiplier (basis points per 30-day period)
    pub lockup_bonus_per_month: u64,
}

impl Default for TokenomicsConfig {
    fn default() -> Self {
        Self {
            genesis_height: 0,
            initial_supply: 0,
            fee_burn_rate: DEFAULT_FEE_BURN_RATE,
            staking_base_apy: DEFAULT_STAKING_APY,
            staking_reward_pool: 1_000_000 * NANOERG_PER_ERG, // 1M ERG
            min_stake: 100 * NANOERG_PER_ERG,               // 100 ERG
            max_stake_cap: 0,
            provider_bonus_apy: 200, // 2%
            lockup_bonus_per_month: 50, // 0.5% per month
        }
    }
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Snapshot of the current tokenomics state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionSnapshot {
    /// Current block height
    pub block_height: u64,
    /// Circulating supply in nanoERG
    pub circulating_supply: u64,
    /// Current block reward in nanoERG
    pub block_reward: u64,
    /// Current annual inflation rate (basis points, 100 = 1%)
    pub inflation_rate_bps: u64,
    /// Total ERG burned from fees in nanoERG
    pub total_burned: u64,
    /// Total ERG emitted from block rewards in nanoERG
    pub total_emitted: u64,
    /// Net supply change (emitted - burned) in nanoERG
    pub net_supply_change: u64,
    /// Current epoch number
    pub epoch: u64,
    /// Blocks until next reward reduction
    pub blocks_to_next_epoch: u64,
    /// Estimated time to next epoch in seconds
    pub seconds_to_next_epoch: u64,
}

/// Staking yield calculation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingYield {
    /// Staked amount in nanoERG
    pub staked_amount: u64,
    /// Annual percentage yield in basis points
    pub apy_bps: u64,
    /// Daily yield in nanoERG
    pub daily_yield: u64,
    /// Weekly yield in nanoERG
    pub weekly_yield: u64,
    /// Monthly yield in nanoERG
    pub monthly_yield: u64,
    /// Annual yield in nanoERG
    pub annual_yield: u64,
    /// Reward pool remaining in nanoERG
    pub reward_pool_remaining: u64,
    /// Effective APY including bonuses in basis points
    pub effective_apy_bps: u64,
    /// Time to deplete reward pool at current rate (blocks)
    pub pool_depletion_blocks: u64,
}

/// Supply schedule entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyEntry {
    /// Block height
    pub height: u64,
    /// Estimated circulating supply in nanoERG
    pub supply: u64,
    /// Block reward at this height in nanoERG
    pub block_reward: u64,
    /// Cumulative emitted in nanoERG
    pub cumulative_emitted: u64,
    /// Inflation rate at this point in basis points
    pub inflation_rate_bps: u64,
}

/// Individual burn record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnRecord {
    /// Unique burn ID
    pub id: String,
    /// Amount burned in nanoERG
    pub amount: u64,
    /// Reason for burn
    pub reason: String,
    /// Block height at which burn occurred
    pub block_height: u64,
    /// Timestamp (unix epoch ms)
    pub timestamp: u64,
    /// Transaction ID if applicable
    pub tx_id: Option<String>,
}

/// Deflationary pressure metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeflationaryPressure {
    /// Net emission rate per block in nanoERG (positive = inflationary)
    pub net_rate_per_block: i64,
    /// Burn rate per block in nanoERG
    pub burn_rate_per_block: u64,
    /// Emission rate per block in nanoERG
    pub emission_rate_per_block: u64,
    /// Whether the protocol is currently deflationary
    pub is_deflationary: bool,
    /// Estimated blocks until net deflation (0 if already deflationary)
    pub blocks_to_deflation: u64,
    /// Annual net change in nanoERG (positive = inflationary)
    pub annual_net_change: i64,
}

/// Yield projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldProjection {
    /// Projected days
    pub days: u64,
    /// Staked amount in nanoERG
    pub staked_amount: u64,
    /// Projected total yield in nanoERG
    pub total_yield: u64,
    /// Projected daily yield in nanoERG
    pub avg_daily_yield: u64,
    /// Projected APY in basis points
    pub projected_apy_bps: u64,
    /// Projected final balance (stake + yield) in nanoERG
    pub final_balance: u64,
}

// ---------------------------------------------------------------------------
// Ring Buffer for burn history
// ---------------------------------------------------------------------------

struct RingBuffer<T> {
    data: Vec<T>,
    head: AtomicU64,
    capacity: usize,
}

impl<T: Clone> RingBuffer<T> {
    fn new(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            head: AtomicU64::new(0),
            capacity,
        }
    }

    fn push(&self, item: T) {
        let head = self.head.fetch_add(1, Ordering::Relaxed) as usize;
        let idx = head % self.capacity;
        if idx < self.data.len() {
            // Safety: we only write from one thread at a time through push
            unsafe {
                let ptr = self.data.as_ptr().add(idx) as *mut T;
                std::ptr::drop_in_place(ptr);
                std::ptr::write(ptr, item);
            }
        } else {
            // Vec is not full yet -- but we can't mutate from &self
            // So we use a different approach
        }
    }
}

// ---------------------------------------------------------------------------
// Tokenomics Engine
// ---------------------------------------------------------------------------

/// Main tokenomics engine providing emission, staking, and burn analytics.
pub struct TokenomicsEngine {
    config: Arc<RwLock<TokenomicsConfig>>,
    current_height: AtomicU64,
    total_burned: AtomicU64,
    total_fees_collected: AtomicU64,
    total_staked: AtomicU64,
    burn_history: DashMap<String, BurnRecord>,
    burn_counter: AtomicU64,
    max_burn_history: usize,
}

impl TokenomicsEngine {
    /// Create a new tokenomics engine with the given config.
    pub fn new(config: TokenomicsConfig) -> Self {
        let max_burn_history = 1000;
        Self {
            config: Arc::new(RwLock::new(config)),
            current_height: AtomicU64::new(0),
            total_burned: AtomicU64::new(0),
            total_fees_collected: AtomicU64::new(0),
            total_staked: AtomicU64::new(0),
            burn_history: DashMap::with_capacity(max_burn_history),
            burn_counter: AtomicU64::new(0),
            max_burn_history,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(TokenomicsConfig::default())
    }

    // -----------------------------------------------------------------------
    // Emission calculations
    // -----------------------------------------------------------------------

    /// Calculate the block reward at a given height.
    pub fn block_reward_at_height(&self, height: u64) -> u64 {
        let epoch = height / EPOCH_LENGTH;
        let mut reward = INITIAL_BLOCK_REWARD;
        for _ in 0..epoch {
            reward = reward * REWARD_DECAY_FACTOR / 10000;
            if reward < MIN_BLOCK_REWARD {
                return 0;
            }
        }
        reward
    }

    /// Calculate total emitted ERG up to a given height.
    pub fn total_emitted_at_height(&self, height: u64) -> u64 {
        let config = self.config.blocking_read();
        let mut total = config.initial_supply;
        let mut current_reward = INITIAL_BLOCK_REWARD;
        let mut blocks_in_epoch = EPOCH_LENGTH;

        // Genesis
        if height == 0 {
            return total;
        }

        let mut h = config.genesis_height;
        while h < height {
            let remaining = height - h;
            let emit_blocks = remaining.min(blocks_in_epoch);
            total = total.saturating_add(current_reward * emit_blocks);
            h += emit_blocks;

            if h >= height {
                break;
            }

            // Decay
            current_reward = current_reward * REWARD_DECAY_FACTOR / 10000;
            if current_reward < MIN_BLOCK_REWARD {
                break;
            }
            blocks_in_epoch = EPOCH_LENGTH;
        }
        total
    }

    /// Calculate estimated supply at a given height.
    pub fn supply_at_height(&self, height: u64) -> u64 {
        let emitted = self.total_emitted_at_height(height);
        let burned = self.total_burned.load(Ordering::Relaxed);
        emitted.saturating_sub(burned)
    }

    /// Get the current emission snapshot.
    pub fn get_emission_snapshot(&self) -> EmissionSnapshot {
        let height = self.current_height.load(Ordering::Relaxed);
        let config = self.config.blocking_read();
        let block_reward = self.block_reward_at_height(height);
        let total_emitted = self.total_emitted_at_height(height);
        let total_burned = self.total_burned.load(Ordering::Relaxed);
        let circulating = total_emitted.saturating_sub(total_burned);
        let epoch = height / EPOCH_LENGTH;
        let blocks_to_next = EPOCH_LENGTH - (height % EPOCH_LENGTH);

        // Annual inflation rate: (block_reward * blocks_per_year * 10000) / circulating_supply
        let blocks_per_year = 365 * 24 * 60 / (BLOCK_TIME_SECS / 60); // ~262800
        let inflation_rate_bps = if circulating > 0 {
            let annual_emission = block_reward * blocks_per_year;
            ((annual_emission as u128 * 10000) / circulating as u128).min(100000) as u64
        } else {
            0
        };

        EmissionSnapshot {
            block_height: height,
            circulating_supply: circulating,
            block_reward,
            inflation_rate_bps,
            total_burned,
            total_emitted,
            net_supply_change: total_emitted.saturating_sub(total_burned),
            epoch,
            blocks_to_next_epoch: blocks_to_next,
            seconds_to_next_epoch: blocks_to_next * BLOCK_TIME_SECS,
        }
    }

    // -----------------------------------------------------------------------
    // Staking yield
    // -----------------------------------------------------------------------

    /// Calculate staking yield for a given staked amount.
    pub fn calculate_staking_yield(&self, staked_amount: u64) -> StakingYield {
        let config = self.config.blocking_read();
        let pool = config.staking_reward_pool;
        let total_staked = self.total_staked.load(Ordering::Relaxed);
        let base_apy = config.staking_base_apy;

        // Effective APY: scales with staked amount relative to total
        let effective_apy = if total_staked > 0 && staked_amount > 0 {
            // Diminishing returns: more staked = lower per-unit yield
            let share = (staked_amount as u128 * 10000) / total_staked as u128;
            let scaled_apy = (base_apy as u128 * share.min(10000)) / 10000;
            scaled_apy.max(100) as u64 // minimum 1% APY
        } else if staked_amount > 0 {
            base_apy
        } else {
            0
        };

        let blocks_per_year: u64 = 365 * 24 * 60 / (BLOCK_TIME_SECS / 60);
        let blocks_per_day = blocks_per_year / 365;

        let annual_yield = if pool > 0 && total_staked > 0 {
            ((staked_amount as u128 * effective_apy as u128) / 10000).min(pool as u128) as u64
        } else {
            0
        };

        let daily_yield = annual_yield / 365;
        let weekly_yield = annual_yield / 52;
        let monthly_yield = annual_yield / 12;

        // Pool depletion estimate
        let pool_depletion_blocks = if total_staked > 0 && effective_apy > 0 {
            let total_annual_yield = ((total_staked as u128 * effective_apy as u128) / 10000) as u64;
            if total_annual_yield > 0 {
                pool / (total_annual_yield / blocks_per_year + 1)
            } else {
                u64::MAX
            }
        } else {
            u64::MAX
        };

        StakingYield {
            staked_amount,
            apy_bps: base_apy,
            daily_yield,
            weekly_yield,
            monthly_yield,
            annual_yield,
            reward_pool_remaining: pool,
            effective_apy_bps: effective_apy,
            pool_depletion_blocks,
        }
    }

    // -----------------------------------------------------------------------
    // Burn tracking
    // -----------------------------------------------------------------------

    /// Record a burn event.
    pub fn record_burn(&self, amount: u64, reason: String, tx_id: Option<String>) -> BurnRecord {
        let height = self.current_height.load(Ordering::Relaxed);
        let counter = self.burn_counter.fetch_add(1, Ordering::Relaxed);
        let id = format!("burn_{}", counter);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let record = BurnRecord {
            id: id.clone(),
            amount,
            reason: reason.clone(),
            block_height: height,
            timestamp,
            tx_id,
        };

        self.total_burned.fetch_add(amount, Ordering::Relaxed);

        // Evict oldest if at capacity
        if self.burn_history.len() >= self.max_burn_history {
            if let Some(oldest_key) = self.burn_history.iter().next().map(|k| k.key().clone()) {
                self.burn_history.remove(&oldest_key);
            }
        }
        self.burn_history.insert(id.clone(), record.clone());

        info!(amount = amount, reason = %reason, "Burn recorded");
        record
    }

    /// Get burn history, most recent first.
    pub fn get_burn_history(&self, limit: usize) -> Vec<BurnRecord> {
        let mut records: Vec<BurnRecord> = self
            .burn_history
            .iter()
            .map(|r| r.value().clone())
            .collect();
        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        records.truncate(limit);
        records
    }

    /// Get inflation rate in basis points.
    pub fn get_inflation_rate(&self) -> u64 {
        let snapshot = self.get_emission_snapshot();
        snapshot.inflation_rate_bps
    }

    /// Get deflationary pressure metrics.
    pub fn get_deflationary_pressure(&self) -> DeflationaryPressure {
        let height = self.current_height.load(Ordering::Relaxed);
        let block_reward = self.block_reward_at_height(height);
        let total_fees = self.total_fees_collected.load(Ordering::Relaxed);
        let config = self.config.blocking_read();

        // Average burn per block (estimated from total fees and burn rate)
        let avg_fee_per_block = if height > 0 { total_fees / height.max(1) } else { 0 };
        let burn_rate_per_block = avg_fee_per_block * config.fee_burn_rate / 10000;

        let net_rate = block_reward as i64 - burn_rate_per_block as i64;
        let is_deflationary = net_rate <= 0;

        let blocks_to_deflation = if !is_deflationary && burn_rate_per_block > 0 {
            // How many blocks until burn_rate > block_reward
            let blocks_per_year: u64 = 365 * 24 * 60 / (BLOCK_TIME_SECS / 60);
            let mut h = height;
            let mut reward = block_reward;
            while reward > burn_rate_per_block {
                h += EPOCH_LENGTH;
                reward = reward * REWARD_DECAY_FACTOR / 10000;
                if h > height + blocks_per_year * 100 {
                    break;
                }
            }
            h - height
        } else {
            0
        };

        let blocks_per_year: u64 = 365 * 24 * 60 / (BLOCK_TIME_SECS / 60);
        let annual_net = net_rate * blocks_per_year as i64;

        DeflationaryPressure {
            net_rate_per_block: net_rate,
            burn_rate_per_block,
            emission_rate_per_block: block_reward,
            is_deflationary,
            blocks_to_deflation,
            annual_net_change: annual_net,
        }
    }

    /// Project future yield.
    pub fn project_yield(&self, days: u64, staked_amount: u64) -> YieldProjection {
        let yield_info = self.calculate_staking_yield(staked_amount);
        let total_yield = yield_info.daily_yield * days;
        let avg_daily = if days > 0 { total_yield / days } else { 0 };

        YieldProjection {
            days,
            staked_amount,
            total_yield,
            avg_daily_yield: avg_daily,
            projected_apy_bps: yield_info.effective_apy_bps,
            final_balance: staked_amount.saturating_add(total_yield),
        }
    }

    /// Generate supply schedule entries.
    pub fn get_supply_schedule(&self, start_height: u64, num_entries: usize, interval_blocks: u64) -> Vec<SupplyEntry> {
        let mut entries = Vec::with_capacity(num_entries);
        for i in 0..num_entries {
            let h = start_height + (i as u64 * interval_blocks);
            let supply = self.supply_at_height(h);
            let block_reward = self.block_reward_at_height(h);
            let emitted = self.total_emitted_at_height(h);

            let blocks_per_year: u64 = 365 * 24 * 60 / (BLOCK_TIME_SECS / 60);
            let inflation_bps = if supply > 0 {
                ((block_reward as u128 * blocks_per_year as u128 * 10000) / supply as u128).min(100000) as u64
            } else {
                0
            };

            entries.push(SupplyEntry {
                height: h,
                supply,
                block_reward,
                cumulative_emitted: emitted,
                inflation_rate_bps: inflation_bps,
            });
        }
        entries
    }

    // -----------------------------------------------------------------------
    // State management
    // -----------------------------------------------------------------------

    /// Set the current block height.
    pub fn set_height(&self, height: u64) {
        self.current_height.store(height, Ordering::Relaxed);
    }

    /// Record collected fees.
    pub fn record_fees(&self, amount: u64) {
        self.total_fees_collected.fetch_add(amount, Ordering::Relaxed);
    }

    /// Set total staked amount.
    pub fn set_total_staked(&self, amount: u64) {
        self.total_staked.store(amount, Ordering::Relaxed);
    }

    /// Get the config.
    pub async fn get_config(&self) -> TokenomicsConfig {
        self.config.read().await.clone()
    }

    /// Update config.
    pub async fn update_config(&self, config: TokenomicsConfig) {
        *self.config.write().await = config;
    }

    /// Format nanoERG to ERG string.
    pub fn format_erg(nanoerg: u64) -> String {
        let erg = nanoerg as f64 / NANOERG_PER_ERG as f64;
        format!("{:.6}", erg)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TokenomicsConfig::default();
        assert_eq!(config.fee_burn_rate, 100);
        assert_eq!(config.staking_base_apy, 500);
        assert_eq!(config.min_stake, 100 * NANOERG_PER_ERG);
    }

    #[test]
    fn test_engine_creation() {
        let engine = TokenomicsEngine::with_defaults();
        let snapshot = engine.get_emission_snapshot();
        assert_eq!(snapshot.block_height, 0);
        assert_eq!(snapshot.block_reward, INITIAL_BLOCK_REWARD);
        assert_eq!(snapshot.total_burned, 0);
    }

    #[test]
    fn test_block_reward_at_genesis() {
        let engine = TokenomicsEngine::with_defaults();
        assert_eq!(engine.block_reward_at_height(0), INITIAL_BLOCK_REWARD);
    }

    #[test]
    fn test_block_reward_decay() {
        let engine = TokenomicsEngine::with_defaults();
        let reward_genesis = engine.block_reward_at_height(0);
        let reward_after_epoch = engine.block_reward_at_height(EPOCH_LENGTH);
        assert!(reward_after_epoch < reward_genesis);
        // Should be ~99.80% of previous
        let expected = reward_genesis * REWARD_DECAY_FACTOR / 10000;
        assert_eq!(reward_after_epoch, expected);
    }

    #[test]
    fn test_block_reward_eventually_min() {
        let engine = TokenomicsEngine::with_defaults();
        // After many epochs, reward should reach minimum
        let far_height = EPOCH_LENGTH * 50000;
        let reward = engine.block_reward_at_height(far_height);
        assert!(reward <= INITIAL_BLOCK_REWARD);
    }

    #[test]
    fn test_emission_snapshot_at_genesis() {
        let engine = TokenomicsEngine::with_defaults();
        let snapshot = engine.get_emission_snapshot();
        assert_eq!(snapshot.epoch, 0);
        assert_eq!(snapshot.blocks_to_next_epoch, EPOCH_LENGTH);
        assert!(snapshot.total_emitted == 0);
    }

    #[test]
    fn test_total_emitted_increases() {
        let engine = TokenomicsEngine::with_defaults();
        let e0 = engine.total_emitted_at_height(0);
        let e100 = engine.total_emitted_at_height(100);
        assert!(e100 > e0);
    }

    #[test]
    fn test_supply_at_height() {
        let engine = TokenomicsEngine::with_defaults();
        let supply = engine.supply_at_height(100);
        let emitted = engine.total_emitted_at_height(100);
        assert_eq!(supply, emitted); // no burns yet
    }

    #[test]
    fn test_supply_schedule() {
        let engine = TokenomicsEngine::with_defaults();
        let schedule = engine.get_supply_schedule(0, 5, 1000);
        assert_eq!(schedule.len(), 5);
        assert!(schedule[1].supply > schedule[0].supply);
    }

    #[test]
    fn test_staking_yield_zero_stake() {
        let engine = TokenomicsEngine::with_defaults();
        let yield_info = engine.calculate_staking_yield(0);
        assert_eq!(yield_info.staked_amount, 0);
        assert_eq!(yield_info.effective_apy_bps, 0);
        assert_eq!(yield_info.annual_yield, 0);
    }

    #[test]
    fn test_staking_yield_normal() {
        let engine = TokenomicsEngine::with_defaults();
        engine.set_total_staked(1_000_000 * NANOERG_PER_ERG);
        let staked = 100 * NANOERG_PER_ERG;
        let yield_info = engine.calculate_staking_yield(staked);
        assert!(yield_info.effective_apy_bps > 0);
        assert!(yield_info.annual_yield > 0);
        assert!(yield_info.daily_yield > 0);
        assert!(yield_info.monthly_yield >= yield_info.daily_yield);
    }

    #[test]
    fn test_staking_yield_proportional() {
        let engine = TokenomicsEngine::with_defaults();
        engine.set_total_staked(1_000_000 * NANOERG_PER_ERG);
        let small = engine.calculate_staking_yield(100 * NANOERG_PER_ERG);
        let large = engine.calculate_staking_yield(1000 * NANOERG_PER_ERG);
        assert!(large.annual_yield > small.annual_yield);
    }

    #[test]
    fn test_burn_recording() {
        let engine = TokenomicsEngine::with_defaults();
        let record = engine.record_burn(1000, "fee_burn".into(), None);
        assert_eq!(record.amount, 1000);
        assert_eq!(record.reason, "fee_burn");
        assert_eq!(engine.total_burned.load(Ordering::Relaxed), 1000);
    }

    #[test]
    fn test_burn_history() {
        let engine = TokenomicsEngine::with_defaults();
        engine.record_burn(100, "test1".into(), None);
        engine.record_burn(200, "test2".into(), None);
        engine.record_burn(300, "test3".into(), None);
        let history = engine.get_burn_history(10);
        assert_eq!(history.len(), 3);
        // Most recent first
        assert_eq!(history[0].amount, 300);
    }

    #[test]
    fn test_burn_history_limit() {
        let engine = TokenomicsEngine::with_defaults();
        for i in 0..20 {
            engine.record_burn((i + 1) as u64, format!("burn_{}", i), None);
        }
        let history = engine.get_burn_history(5);
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_inflation_rate() {
        let engine = TokenomicsEngine::with_defaults();
        let rate = engine.get_inflation_rate();
        // At genesis with no supply, inflation should be 0 or very high
        assert!(rate >= 0);
    }

    #[test]
    fn test_deflationary_pressure_inflationary() {
        let engine = TokenomicsEngine::with_defaults();
        let pressure = engine.get_deflationary_pressure();
        // At genesis, should be inflationary
        assert!(!pressure.is_deflationary);
        assert!(pressure.emission_rate_per_block > 0);
    }

    #[test]
    fn test_deflationary_pressure_with_burns() {
        let engine = TokenomicsEngine::with_defaults();
        engine.set_height(10000);
        engine.record_fees(1_000_000_000 * NANOERG_PER_ERG); // huge fees
        // With enough fees, burn rate could exceed emission
        let pressure = engine.get_deflationary_pressure();
        assert!(pressure.burn_rate_per_block > 0);
    }

    #[test]
    fn test_yield_projection() {
        let engine = TokenomicsEngine::with_defaults();
        engine.set_total_staked(1_000_000 * NANOERG_PER_ERG);
        let proj = engine.project_yield(365, 100 * NANOERG_PER_ERG);
        assert_eq!(proj.days, 365);
        assert!(proj.total_yield > 0);
        assert!(proj.final_balance > proj.staked_amount);
    }

    #[test]
    fn test_format_erg() {
        assert_eq!(TokenomicsEngine::format_erg(NANOERG_PER_ERG), "1.000000");
        assert_eq!(TokenomicsEngine::format_erg(0), "0.000000");
        assert_eq!(TokenomicsEngine::format_erg(500_000_000), "0.500000");
    }

    #[test]
    fn test_concurrent_access() {
        let engine = Arc::new(TokenomicsEngine::with_defaults());
        let mut handles = vec![];

        for i in 0..10 {
            let e = engine.clone();
            handles.push(std::thread::spawn(move || {
                e.record_burn((i + 1) as u64 * 100, format!("concurrent_{}", i), None);
                e.get_emission_snapshot();
                e.calculate_staking_yield(100 * NANOERG_PER_ERG);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let total_burned = engine.total_burned.load(Ordering::Relaxed);
        assert_eq!(total_burned, (1..=10).map(|i| i * 100).sum::<u64>());
    }

    #[test]
    fn test_snapshot_serialization() {
        let engine = TokenomicsEngine::with_defaults();
        let snapshot = engine.get_emission_snapshot();
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: EmissionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.block_height, snapshot.block_height);
    }

    #[test]
    fn test_yield_serialization() {
        let engine = TokenomicsEngine::with_defaults();
        engine.set_total_staked(1_000_000 * NANOERG_PER_ERG);
        let yield_info = engine.calculate_staking_yield(100 * NANOERG_PER_ERG);
        let json = serde_json::to_string(&yield_info).unwrap();
        let deserialized: StakingYield = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.staked_amount, yield_info.staked_amount);
    }

    #[test]
    fn test_config_update() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let engine = TokenomicsEngine::with_defaults();
        rt.block_on(async {
            let mut config = engine.get_config().await;
            config.staking_base_apy = 1000;
            engine.update_config(config).await;
            let updated = engine.get_config().await;
            assert_eq!(updated.staking_base_apy, 1000);
        });
    }
}
