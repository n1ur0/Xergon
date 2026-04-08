//! Staking pool manager for the Xergon Network agent.
//!
//! Implements liquid staking, delegation, yield optimization, and auto-compound
//! functionality. Inspired by Ergo oracle pool patterns (EIP-23) with
//! epoch-based reward cycles, Singleton NFT pool state, and reserve ratio
//! bounds enforced at the contract level.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Staking pool status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PoolStatus {
    Active,
    Paused,
    Closed,
    Slashing,
}

impl Default for PoolStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Staking action result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakeResult {
    Success { tx_id: String, new_position: u64 },
    InsufficientFunds { required: u64, available: u64 },
    PoolClosed { pool_id: String },
    BelowMinimum { min_stake: u64, attempted: u64 },
    AlreadyStaked { current_amount: u64 },
    PoolNotFound { pool_id: String },
    RewardClaimed { amount: u64, new_balance: u64 },
    Compounded { amount: u64, new_stake: u64 },
    Delegated { pool_id: String, amount: u64 },
    Undelegated { amount: u64, returned_to: String },
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A staking pool.
#[derive(Debug, Serialize, Deserialize)]
pub struct StakingPool {
    pub pool_id: String,
    pub name: String,
    pub erg_staked: AtomicU64,
    pub xrg_staked: AtomicU64,
    pub reward_token_id: String,
    pub min_stake: u64,
    pub max_stake: u64,
    pub epoch_length: u32,
    pub reward_rate_bps: u32, // APY in basis points (e.g. 1200 = 12%)
    pub total_rewards_distributed: AtomicU64,
    pub active_stakers: AtomicU64,
    pub creation_height: u64,
    pub status: PoolStatus,
}

impl StakingPool {
    pub fn new(
        pool_id: String,
        name: String,
        reward_token_id: String,
        min_stake: u64,
        max_stake: u64,
        epoch_length: u32,
        reward_rate_bps: u32,
        creation_height: u64,
    ) -> Self {
        Self {
            pool_id,
            name,
            erg_staked: AtomicU64::new(0),
            xrg_staked: AtomicU64::new(0),
            reward_token_id,
            min_stake,
            max_stake,
            epoch_length,
            reward_rate_bps,
            total_rewards_distributed: AtomicU64::new(0),
            active_stakers: AtomicU64::new(0),
            creation_height,
            status: PoolStatus::Active,
        }
    }

    pub fn apy_percent(&self) -> f64 {
        self.reward_rate_bps as f64 / 100.0
    }

    pub fn tvl_erg(&self) -> u64 {
        self.erg_staked.load(Ordering::Relaxed)
    }

    pub fn tvl_xrg(&self) -> u64 {
        self.xrg_staked.load(Ordering::Relaxed)
    }
}

/// Individual staker position in a pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakerPosition {
    pub staker_address: String,
    pub pool_id: String,
    pub erg_staked: u64,
    pub xrg_staked: u64,
    pub reward_balance: u64,
    pub pending_rewards: u64,
    pub entry_height: u64,
    pub last_reward_height: u64,
    pub auto_compound: bool,
}

impl StakerPosition {
    pub fn new(staker_address: String, pool_id: String, entry_height: u64) -> Self {
        Self {
            staker_address,
            pool_id,
            erg_staked: 0,
            xrg_staked: 0,
            reward_balance: 0,
            pending_rewards: 0,
            entry_height,
            last_reward_height: entry_height,
            auto_compound: false,
        }
    }

    pub fn total_staked(&self) -> u64 {
        self.erg_staked.saturating_add(self.xrg_staked)
    }
}

/// Epoch reward computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochReward {
    pub pool_id: String,
    pub epoch_number: u64,
    pub total_rewards: u64,
    pub per_staker_rewards: u64,
    pub active_stakers: u32,
    pub block_height: u64,
}

/// Read-only snapshot of a pool for borrowing (avoids AtomicU64 Clone).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct PoolSnapshot {
    pub pool_id: String,
    pub name: String,
    pub reward_token_id: String,
    pub min_stake: u64,
    pub max_stake: u64,
    pub epoch_length: u32,
    pub reward_rate_bps: u32,
    pub creation_height: u64,
    pub status: PoolStatus,
    pub erg_staked: u64,
    pub xrg_staked: u64,
    pub total_rewards_distributed: u64,
    pub active_stakers: u64,
}

impl PoolSnapshot {
    fn tvl_xrg(&self) -> u64 {
        self.xrg_staked
    }
}

impl StakingPool {
    pub(crate) fn snapshot(&self) -> PoolSnapshot {
        PoolSnapshot {
            pool_id: self.pool_id.clone(),
            name: self.name.clone(),
            reward_token_id: self.reward_token_id.clone(),
            min_stake: self.min_stake,
            max_stake: self.max_stake,
            epoch_length: self.epoch_length,
            reward_rate_bps: self.reward_rate_bps,
            creation_height: self.creation_height,
            status: self.status.clone(),
            erg_staked: self.erg_staked.load(Ordering::Relaxed),
            xrg_staked: self.xrg_staked.load(Ordering::Relaxed),
            total_rewards_distributed: self.total_rewards_distributed.load(Ordering::Relaxed),
            active_stakers: self.active_stakers.load(Ordering::Relaxed),
        }
    }
}

/// Pool statistics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub pool_id: String,
    pub name: String,
    pub tvl_erg: u64,
    pub tvl_xrg: u64,
    pub apy_bps: u32,
    pub active_stakers: u64,
    pub total_rewards_distributed: u64,
    pub status: PoolStatus,
    pub epoch_length: u32,
    pub creation_height: u64,
}

/// Yield optimization suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldSuggestion {
    pub from_pool: Option<String>,
    pub to_pool: String,
    pub current_apy: f64,
    pub projected_apy: f64,
    pub estimated_gain_bps: u32,
    pub risk_level: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Epoch Tracker
// ---------------------------------------------------------------------------

/// Tracks reward epochs per pool.
pub struct EpochTracker {
    current_epochs: DashMap<String, u64>,
    epoch_history: DashMap<String, VecDeque<EpochReward>>,
    max_history: usize,
}

impl EpochTracker {
    pub fn new(max_history: usize) -> Self {
        Self {
            current_epochs: DashMap::new(),
            epoch_history: DashMap::new(),
            max_history,
        }
    }

    pub fn init_pool(&self, pool_id: &str) {
        let key = pool_id.to_string();
        self.current_epochs.entry(key.clone()).or_insert(0);
        self.epoch_history
            .entry(key)
            .or_insert_with(VecDeque::new);
    }

    pub fn current_epoch(&self, pool_id: &str) -> u64 {
        self.current_epochs
            .get(pool_id)
            .map(|e| *e.value())
            .unwrap_or(0)
    }

    pub fn advance_epoch(&self, pool_id: &str, reward: EpochReward) {
        let key = pool_id.to_string();
        if let Some(mut epoch) = self.current_epochs.get_mut(&key) {
            *epoch += 1;
        }
        if let Some(mut history) = self.epoch_history.get_mut(&key) {
            history.push_back(reward);
            while history.len() > self.max_history {
                history.pop_front();
            }
        }
    }

    pub fn get_history(&self, pool_id: &str, limit: usize) -> Vec<EpochReward> {
        self.epoch_history
            .get(pool_id)
            .map(|h| h.value().iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Pool Manager
// ---------------------------------------------------------------------------

/// Main staking pool manager.
pub struct PoolManager {
    pools: DashMap<String, StakingPool>,
    positions: DashMap<String, StakerPosition>, // key: "{staker_address}:{pool_id}"
    epoch_tracker: EpochTracker,
    reward_events: AtomicU64,
    total_pools: AtomicU64,
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            pools: DashMap::new(),
            positions: DashMap::new(),
            epoch_tracker: EpochTracker::new(100),
            reward_events: AtomicU64::new(0),
            total_pools: AtomicU64::new(0),
        }
    }

    // --- Pool CRUD ---

    pub fn create_pool(&self, pool: StakingPool) -> StakeResult {
        let id = pool.pool_id.clone();
        if self.pools.contains_key(&id) {
            return StakeResult::PoolNotFound { pool_id: id };
        }
        self.epoch_tracker.init_pool(&id);
        self.pools.insert(id.clone(), pool);
        self.total_pools.fetch_add(1, Ordering::Relaxed);
        StakeResult::Success {
            tx_id: format!("create_{}", id),
            new_position: 1,
        }
    }

    pub fn close_pool(&self, pool_id: &str) -> StakeResult {
        if let Some(mut pool) = self.pools.get_mut(pool_id) {
            pool.value_mut().status = PoolStatus::Closed;
            return StakeResult::Success {
                tx_id: format!("close_{}", pool_id),
                new_position: 0,
            };
        }
        StakeResult::PoolNotFound { pool_id: pool_id.to_string() }
    }

    pub fn pause_pool(&self, pool_id: &str) -> StakeResult {
        if let Some(mut pool) = self.pools.get_mut(pool_id) {
            if pool.value().status == PoolStatus::Closed {
                return StakeResult::PoolClosed { pool_id: pool_id.to_string() };
            }
            pool.value_mut().status = PoolStatus::Paused;
            return StakeResult::Success {
                tx_id: format!("pause_{}", pool_id),
                new_position: 0,
            };
        }
        StakeResult::PoolNotFound { pool_id: pool_id.to_string() }
    }

    pub fn get_pool(&self, pool_id: &str) -> Option<PoolStats> {
        self.pools.get(pool_id).map(|p| {
            let pool = p.value();
            PoolStats {
                pool_id: pool.pool_id.clone(),
                name: pool.name.clone(),
                tvl_erg: pool.tvl_erg(),
                tvl_xrg: pool.tvl_xrg(),
                apy_bps: pool.reward_rate_bps,
                active_stakers: pool.active_stakers.load(Ordering::Relaxed),
                total_rewards_distributed: pool.total_rewards_distributed.load(Ordering::Relaxed),
                status: pool.status.clone(),
                epoch_length: pool.epoch_length,
                creation_height: pool.creation_height,
            }
        })
    }

    pub fn list_pools(&self) -> Vec<PoolStats> {
        self.pools
            .iter()
            .map(|p| {
                let pool = p.value();
                PoolStats {
                    pool_id: pool.pool_id.clone(),
                    name: pool.name.clone(),
                    tvl_erg: pool.tvl_erg(),
                    tvl_xrg: pool.tvl_xrg(),
                    apy_bps: pool.reward_rate_bps,
                    active_stakers: pool.active_stakers.load(Ordering::Relaxed),
                    total_rewards_distributed: pool.total_rewards_distributed.load(Ordering::Relaxed),
                    status: pool.status.clone(),
                    epoch_length: pool.epoch_length,
                    creation_height: pool.creation_height,
                }
            })
            .collect()
    }

    // --- Staking ---

    pub fn stake(
        &self,
        staker_address: &str,
        pool_id: &str,
        xrg_amount: u64,
        erg_amount: u64,
        current_height: u64,
    ) -> StakeResult {
        let pool = match self.pools.get(pool_id) {
            Some(p) => p.value().snapshot(),
            None => return StakeResult::PoolNotFound { pool_id: pool_id.to_string() },
        };

        if pool.status == PoolStatus::Closed {
            return StakeResult::PoolClosed { pool_id: pool_id.to_string() };
        }

        let total = xrg_amount.saturating_add(erg_amount);
        if total < pool.min_stake {
            return StakeResult::BelowMinimum { min_stake: pool.min_stake, attempted: total };
        }

        let key = format!("{}:{}", staker_address, pool_id);
        let is_new = !self.positions.contains_key(&key);

        if let Some(mut pos) = self.positions.get_mut(&key) {
            let pos = pos.value_mut();
            pos.xrg_staked = pos.xrg_staked.saturating_add(xrg_amount);
            pos.erg_staked = pos.erg_staked.saturating_add(erg_amount);
        } else {
            let mut pos = StakerPosition::new(staker_address.to_string(), pool_id.to_string(), current_height);
            pos.xrg_staked = xrg_amount;
            pos.erg_staked = erg_amount;
            self.positions.insert(key.clone(), pos);
        }

        if is_new {
            if let Some(mut p) = self.pools.get_mut(pool_id) {
                p.value_mut().active_stakers.fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Some(mut p) = self.pools.get_mut(pool_id) {
            p.value_mut().xrg_staked.fetch_add(xrg_amount, Ordering::Relaxed);
            p.value_mut().erg_staked.fetch_add(erg_amount, Ordering::Relaxed);
        }

        StakeResult::Success {
            tx_id: format!("stake_{}_{}", pool_id, current_height),
            new_position: total,
        }
    }

    pub fn unstake(
        &self,
        staker_address: &str,
        pool_id: &str,
        xrg_amount: u64,
        erg_amount: u64,
    ) -> StakeResult {
        let key = format!("{}:{}", staker_address, pool_id);
        if let Some(mut pos) = self.positions.get_mut(&key) {
            let pos = pos.value_mut();
            let actual_xrg = if xrg_amount == 0 { pos.xrg_staked } else { xrg_amount.min(pos.xrg_staked) };
            let actual_erg = if erg_amount == 0 { pos.erg_staked } else { erg_amount.min(pos.erg_staked) };
            pos.xrg_staked = pos.xrg_staked.saturating_sub(actual_xrg);
            pos.erg_staked = pos.erg_staked.saturating_sub(actual_erg);

            if pos.total_staked() == 0 {
                let _ = pos;
                self.positions.remove(&key);
                if let Some(mut p) = self.pools.get_mut(pool_id) {
                    p.value_mut().active_stakers.fetch_sub(1, Ordering::Relaxed);
                }
            }

            if let Some(mut p) = self.pools.get_mut(pool_id) {
                p.value_mut().xrg_staked.fetch_sub(actual_xrg, Ordering::Relaxed);
                p.value_mut().erg_staked.fetch_sub(actual_erg, Ordering::Relaxed);
            }

            return StakeResult::Undelegated {
                amount: actual_xrg.saturating_add(actual_erg),
                returned_to: staker_address.to_string(),
            };
        }
        StakeResult::PoolNotFound { pool_id: pool_id.to_string() }
    }

    pub fn delegate(
        &self,
        staker_address: &str,
        pool_id: &str,
        amount: u64,
        current_height: u64,
    ) -> StakeResult {
        self.stake(staker_address, pool_id, amount, 0, current_height)
    }

    // --- Rewards ---

    pub fn claim_rewards(&self, staker_address: &str, pool_id: &str) -> StakeResult {
        let key = format!("{}:{}", staker_address, pool_id);
        if let Some(mut pos) = self.positions.get_mut(&key) {
            let pos = pos.value_mut();
            let claimed = pos.pending_rewards;
            if claimed == 0 {
                return StakeResult::RewardClaimed { amount: 0, new_balance: pos.reward_balance };
            }
            pos.reward_balance = pos.reward_balance.saturating_add(claimed);
            let new_balance = pos.reward_balance;
            pos.pending_rewards = 0;
            self.reward_events.fetch_add(1, Ordering::Relaxed);
            return StakeResult::RewardClaimed { amount: claimed, new_balance };
        }
        StakeResult::PoolNotFound { pool_id: pool_id.to_string() }
    }

    pub fn compound_rewards(&self, staker_address: &str, pool_id: &str) -> StakeResult {
        let key = format!("{}:{}", staker_address, pool_id);
        if let Some(mut pos) = self.positions.get_mut(&key) {
            let pos = pos.value_mut();
            let amount = pos.pending_rewards;
            if amount == 0 {
                return StakeResult::Compounded { amount: 0, new_stake: pos.xrg_staked };
            }
            pos.xrg_staked = pos.xrg_staked.saturating_add(amount);
            if let Some(mut p) = self.pools.get_mut(pool_id) {
                p.value_mut().xrg_staked.fetch_add(amount, Ordering::Relaxed);
            }
            let new_stake = pos.xrg_staked;
            pos.pending_rewards = 0;
            self.reward_events.fetch_add(1, Ordering::Relaxed);
            return StakeResult::Compounded { amount, new_stake };
        }
        StakeResult::PoolNotFound { pool_id: pool_id.to_string() }
    }

    pub fn compute_epoch_rewards(&self, pool_id: &str, current_height: u64) {
        let pool = match self.pools.get(pool_id) {
            Some(p) => p.value().snapshot(),
            None => return,
        };
        if pool.status != PoolStatus::Active {
            return;
        }

        let epoch = self.epoch_tracker.current_epoch(pool_id);
        let active = pool.active_stakers as u32;
        if active == 0 {
            return;
        }

        let tvl = pool.tvl_xrg();
        let rewards = (tvl as u128 * pool.reward_rate_bps as u128
            / (10000 * (365 * 24 * 3600 / pool.epoch_length as u64).max(1) as u128))
            as u64;
        let per_staker = rewards / active.max(1) as u64;

        // Distribute to positions
        let mut distributed = 0u64;
        for mut entry in self.positions.iter_mut() {
            let pos = entry.value_mut();
            if pos.pool_id == pool_id && pos.xrg_staked > 0 {
                pos.pending_rewards = pos.pending_rewards.saturating_add(per_staker);
                distributed = distributed.saturating_add(per_staker);
            }
        }

        if let Some(mut p) = self.pools.get_mut(pool_id) {
            p.value_mut().total_rewards_distributed.fetch_add(distributed, Ordering::Relaxed);
        }

        let reward = EpochReward {
            pool_id: pool_id.to_string(),
            epoch_number: epoch,
            total_rewards: distributed,
            per_staker_rewards: per_staker,
            active_stakers: active,
            block_height: current_height,
        };
        self.epoch_tracker.advance_epoch(pool_id, reward);
    }

    // --- Analytics ---

    pub fn get_staker_yield(&self, staker_address: &str, pool_id: &str) -> Option<f64> {
        let key = format!("{}:{}", staker_address, pool_id);
        self.positions.get(&key).map(|pos| {
            let pos = pos.value();
            let total_staked = pos.total_staked().max(1);
            let total_rewards = pos.reward_balance.saturating_add(pos.pending_rewards);
            (total_rewards as f64 / total_staked as f64) * 100.0
        })
    }

    pub fn get_apy_estimate(&self, pool_id: &str) -> Option<f64> {
        self.pools.get(pool_id).map(|p| p.value().apy_percent())
    }

    pub fn suggest_optimal_pool(&self, _staker_address: &str, amount: u64) -> Vec<YieldSuggestion> {
        let mut pool_apis: Vec<(String, f64)> = self.pools
            .iter()
            .filter(|p| p.value().status == PoolStatus::Active)
            .filter(|p| amount >= p.value().min_stake)
            .map(|p| (p.value().pool_id.clone(), p.value().apy_percent()))
            .collect();
        pool_apis.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut suggestions = Vec::new();
        for (pool_id, apy) in pool_apis.iter().take(3) {
            let risk = if *apy > 20.0 { "high" } else if *apy > 10.0 { "medium" } else { "low" };
            suggestions.push(YieldSuggestion {
                from_pool: None,
                to_pool: pool_id.clone(),
                current_apy: 0.0,
                projected_apy: *apy,
                estimated_gain_bps: (*apy * 100.0) as u32,
                risk_level: risk.to_string(),
                reason: format!("Pool {} offers {:.2}% APY", pool_id, apy),
            });
        }
        suggestions
    }

    pub fn get_position(&self, staker_address: &str, pool_id: &str) -> Option<StakerPosition> {
        let key = format!("{}:{}", staker_address, pool_id);
        self.positions.get(&key).map(|p| p.value().clone())
    }

    pub fn pool_count(&self) -> u64 {
        self.total_pools.load(Ordering::Relaxed)
    }

    pub fn reward_event_count(&self) -> u64 {
        self.reward_events.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

pub async fn handle_create_pool(manager: &PoolManager, pool: StakingPool) -> String {
    serde_json::to_string(&manager.create_pool(pool)).unwrap_or_default()
}

pub async fn handle_list_pools(manager: &PoolManager) -> String {
    serde_json::to_string(&manager.list_pools()).unwrap_or_default()
}

pub async fn handle_get_pool(manager: &PoolManager, pool_id: &str) -> String {
    serde_json::to_string(&manager.get_pool(pool_id)).unwrap_or_default()
}

pub async fn handle_stake(
    manager: &PoolManager,
    staker: &str,
    pool_id: &str,
    xrg: u64,
    erg: u64,
    height: u64,
) -> String {
    serde_json::to_string(&manager.stake(staker, pool_id, xrg, erg, height)).unwrap_or_default()
}

pub async fn handle_unstake(
    manager: &PoolManager,
    staker: &str,
    pool_id: &str,
    xrg: u64,
    erg: u64,
) -> String {
    serde_json::to_string(&manager.unstake(staker, pool_id, xrg, erg)).unwrap_or_default()
}

pub async fn handle_claim(manager: &PoolManager, staker: &str, pool_id: &str) -> String {
    serde_json::to_string(&manager.claim_rewards(staker, pool_id)).unwrap_or_default()
}

pub async fn handle_compound(manager: &PoolManager, staker: &str, pool_id: &str) -> String {
    serde_json::to_string(&manager.compound_rewards(staker, pool_id)).unwrap_or_default()
}

pub async fn handle_delegate(
    manager: &PoolManager,
    staker: &str,
    pool_id: &str,
    amount: u64,
    height: u64,
) -> String {
    serde_json::to_string(&manager.delegate(staker, pool_id, amount, height)).unwrap_or_default()
}

pub async fn handle_suggest(manager: &PoolManager, staker: &str, amount: u64) -> String {
    serde_json::to_string(&manager.suggest_optimal_pool(staker, amount)).unwrap_or_default()
}

pub async fn handle_staker_yield(manager: &PoolManager, staker: &str, pool_id: &str) -> String {
    serde_json::to_string(&manager.get_staker_yield(staker, pool_id)).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pool(id: &str, apy_bps: u32) -> StakingPool {
        StakingPool::new(
            id.to_string(),
            format!("Pool {}", id),
            format!("reward_{}", id),
            100,
            1_000_000,
            720,
            apy_bps,
            1000,
        )
    }

    fn make_manager() -> PoolManager {
        PoolManager::new()
    }

    #[test]
    fn test_create_and_list_pools() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        mgr.create_pool(make_pool("beta", 800));
        assert_eq!(mgr.pool_count(), 2);
        let pools = mgr.list_pools();
        assert_eq!(pools.len(), 2);
    }

    #[test]
    fn test_stake_and_unstake() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        let result = mgr.stake("addr1", "alpha", 5000, 1000, 1100);
        assert!(matches!(result, StakeResult::Success { .. }));

        let pos = mgr.get_position("addr1", "alpha").unwrap();
        assert_eq!(pos.xrg_staked, 5000);
        assert_eq!(pos.erg_staked, 1000);

        let result = mgr.unstake("addr1", "alpha", 2000, 0);
        assert!(matches!(result, StakeResult::Undelegated { .. }));
        let pos = mgr.get_position("addr1", "alpha").unwrap();
        assert_eq!(pos.xrg_staked, 3000);
    }

    #[test]
    fn test_stake_below_minimum() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        let result = mgr.stake("addr1", "alpha", 50, 0, 1100);
        assert!(matches!(result, StakeResult::BelowMinimum { .. }));
    }

    #[test]
    fn test_stake_closed_pool() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        mgr.close_pool("alpha");
        let result = mgr.stake("addr1", "alpha", 5000, 0, 1100);
        assert!(matches!(result, StakeResult::PoolClosed { .. }));
    }

    #[test]
    fn test_claim_rewards() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        mgr.stake("addr1", "alpha", 5000, 0, 1100);

        // Simulate pending rewards
        let key = format!("{}:{}", "addr1", "alpha");
        if let Some(mut pos) = mgr.positions.get_mut(&key) {
            pos.value_mut().pending_rewards = 500;
        }

        let result = mgr.claim_rewards("addr1", "alpha");
        assert!(matches!(result, StakeResult::RewardClaimed { amount: 500, .. }));
    }

    #[test]
    fn test_compound_rewards() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        mgr.stake("addr1", "alpha", 5000, 0, 1100);

        let key = format!("{}:{}", "addr1", "alpha");
        if let Some(mut pos) = mgr.positions.get_mut(&key) {
            pos.value_mut().pending_rewards = 300;
        }

        let result = mgr.compound_rewards("addr1", "alpha");
        assert!(matches!(result, StakeResult::Compounded { amount: 300, .. }));
        let pos = mgr.get_position("addr1", "alpha").unwrap();
        assert_eq!(pos.xrg_staked, 5300);
    }

    #[test]
    fn test_epoch_rewards() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        mgr.stake("addr1", "alpha", 100_000, 0, 1100);
        mgr.stake("addr2", "alpha", 50_000, 0, 1100);

        mgr.compute_epoch_rewards("alpha", 1820);

        let pos1 = mgr.get_position("addr1", "alpha").unwrap();
        assert!(pos1.pending_rewards > 0);
        let pos2 = mgr.get_position("addr2", "alpha").unwrap();
        assert!(pos2.pending_rewards > 0);
    }

    #[test]
    fn test_suggest_optimal_pool() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("low", 500));
        mgr.create_pool(make_pool("mid", 1500));
        mgr.create_pool(make_pool("high", 2500));

        let suggestions = mgr.suggest_optimal_pool("addr1", 500);
        assert_eq!(suggestions.len(), 3);
        assert_eq!(suggestions[0].to_pool, "high");
    }

    #[test]
    fn test_pause_pool() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        let result = mgr.pause_pool("alpha");
        assert!(matches!(result, StakeResult::Success { .. }));
        let stats = mgr.get_pool("alpha").unwrap();
        assert_eq!(stats.status, PoolStatus::Paused);
    }

    #[test]
    fn test_full_unstake_removes_position() {
        let mgr = make_manager();
        mgr.create_pool(make_pool("alpha", 1200));
        mgr.stake("addr1", "alpha", 5000, 1000, 1100);
        mgr.unstake("addr1", "alpha", 0, 0); // unstake all
        assert!(mgr.get_position("addr1", "alpha").is_none());
    }
}
