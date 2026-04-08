//! Staking Rewards module.
//!
//! Manages a reward pool that distributes yields to staked providers.
//! Rewards are distributed at configurable intervals, with performance
//! bonuses for high-quality providers based on health scores.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the staking reward system.
#[derive(Debug, Clone, Serialize)]
pub struct StakingRewardConfig {
    /// Annual percentage rate (default: 0.05 = 5%)
    pub reward_rate: f64,
    /// Blocks between distributions (default: 720 ~ 2 days)
    pub reward_interval: u64,
    /// Minimum nanoERG staked to earn rewards
    pub min_stake: u64,
    /// Bonus rate for high-performing providers (default: 0.02 = 2%)
    pub bonus_rate: f64,
    /// Minimum health score to qualify for bonus (default: 0.9)
    pub performance_threshold: f64,
    /// Maximum total rewards distributed per epoch (nanoERG, 0 = unlimited)
    pub pool_cap: u64,
}

impl Default for StakingRewardConfig {
    fn default() -> Self {
        Self {
            reward_rate: 0.05,
            reward_interval: 720,
            min_stake: 1_000_000_000,   // 1 ERG
            bonus_rate: 0.02,
            performance_threshold: 0.9,
            pool_cap: 0, // unlimited
        }
    }
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Per-provider reward tracking.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderRewards {
    pub staked_amount: u64,
    pub accumulated_rewards: u64,
    pub last_claim_height: u64,
    pub performance_bonus: f64,
}

/// Summary of the reward pool state.
#[derive(Debug, Clone, Serialize)]
pub struct PoolStats {
    pub total_staked: u64,
    pub total_rewards_distributed: u64,
    pub last_distribution_height: u64,
    pub providers_count: u64,
    pub reward_rate: f64,
    pub bonus_rate: f64,
    pub reward_interval: u64,
}

/// Leaderboard entry for top earners.
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardEntry {
    pub provider_id: String,
    pub staked_amount: u64,
    pub accumulated_rewards: u64,
    pub current_apy: f64,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Manages the staking reward pool and yield distribution.
pub struct StakingRewardPool {
    config: StakingRewardConfig,
    provider_rewards: DashMap<String, ProviderRewards>,
    total_staked: AtomicU64,
    total_rewards_distributed: AtomicU64,
    last_distribution_height: AtomicU64,
}

impl StakingRewardPool {
    /// Create a new staking reward pool with the given config.
    pub fn new(config: StakingRewardConfig) -> Self {
        Self {
            config,
            provider_rewards: DashMap::new(),
            total_staked: AtomicU64::new(0),
            total_rewards_distributed: AtomicU64::new(0),
            last_distribution_height: AtomicU64::new(0),
        }
    }

    /// Add stake for a provider.
    pub fn add_stake(&self, provider_id: &str, amount: u64) {
        let mut entry = self
            .provider_rewards
            .entry(provider_id.to_string())
            .or_insert_with(|| ProviderRewards {
                staked_amount: 0,
                accumulated_rewards: 0,
                last_claim_height: 0,
                performance_bonus: 0.0,
            });
        entry.staked_amount += amount;
        self.total_staked.fetch_add(amount, Ordering::Relaxed);
        debug!(provider = %provider_id, amount, "Stake added");
    }

    /// Remove stake for a provider. Returns error if insufficient stake.
    pub fn remove_stake(&self, provider_id: &str, amount: u64) -> Result<(), String> {
        let mut entry = self
            .provider_rewards
            .get_mut(provider_id)
            .ok_or_else(|| format!("No stake found for provider {}", provider_id))?;

        if entry.staked_amount < amount {
            return Err(format!(
                "Insufficient stake: have {}, want {}",
                entry.staked_amount, amount
            ));
        }

        entry.staked_amount -= amount;
        self.total_staked.fetch_sub(amount, Ordering::Relaxed);
        debug!(provider = %provider_id, amount, "Stake removed");
        Ok(())
    }

    /// Compute performance bonus multiplier based on health score.
    ///
    /// Returns a multiplier: 1.0 + bonus_rate if health >= threshold, else 1.0.
    pub fn compute_performance_bonus(&self, health_score: f64) -> f64 {
        if health_score >= self.config.performance_threshold {
            1.0 + self.config.bonus_rate
        } else {
            1.0
        }
    }

    /// Distribute rewards to all staked providers for the current epoch.
    ///
    /// Rewards are calculated as:
    ///   `staked * reward_rate * (interval / blocks_per_year) * bonus_multiplier`
    ///
    /// Returns total nanoERG distributed.
    pub fn distribute_rewards(
        &self,
        current_block: u64,
        health_scores: &DashMap<String, f64>,
    ) -> u64 {
        let last = self.last_distribution_height.load(Ordering::Relaxed);
        let interval = self.config.reward_interval;

        if current_block - last < interval {
            return 0;
        }

        let blocks_per_year: f64 = 525_600.0;
        let epoch_fraction = interval as f64 / blocks_per_year;
        let mut total_distributed: u64 = 0;
        let pool_cap = self.config.pool_cap;

        for mut entry in self.provider_rewards.iter_mut() {
            // Look up health score for bonus (must borrow key before mut borrow)
            let key = entry.key().clone();
            let health = health_scores
                .get(&key)
                .map(|r| *r.value())
                .unwrap_or(0.5);
            let bonus = self.compute_performance_bonus(health);

            let rewards = entry.value_mut();

            if rewards.staked_amount < self.config.min_stake {
                continue;
            }

            rewards.performance_bonus = bonus - 1.0; // store just the bonus portion

            let reward = ((rewards.staked_amount as f64
                * self.config.reward_rate
                * epoch_fraction
                * bonus) as u64)
                .min(
                    pool_cap
                        .saturating_sub(total_distributed)
                        .max(if pool_cap == 0 { u64::MAX } else { 0 }),
                );

            if reward > 0 && (pool_cap == 0 || total_distributed + reward <= pool_cap) {
                rewards.accumulated_rewards += reward;
                total_distributed += reward;
            }

            if pool_cap > 0 && total_distributed >= pool_cap {
                break;
            }
        }

        if total_distributed > 0 {
            self.total_rewards_distributed
                .fetch_add(total_distributed, Ordering::Relaxed);
            self.last_distribution_height.store(current_block, Ordering::Relaxed);
            info!(total = total_distributed, block = current_block, "Staking rewards distributed");
        }

        total_distributed
    }

    /// Claim accumulated rewards for a provider.
    /// Returns the claimable amount and resets the accumulator.
    pub fn claim_rewards(&self, provider_id: &str) -> Result<u64, String> {
        let mut entry = self
            .provider_rewards
            .get_mut(provider_id)
            .ok_or_else(|| format!("No stake found for provider {}", provider_id))?;

        let claimable = entry.accumulated_rewards;
        if claimable == 0 {
            return Ok(0);
        }

        entry.accumulated_rewards = 0;
        info!(provider = %provider_id, claimable, "Rewards claimed");
        Ok(claimable)
    }

    /// Get the current yield/APY for a provider.
    pub fn get_yield(&self, provider_id: &str) -> f64 {
        self.provider_rewards
            .get(provider_id)
            .map(|r| {
                let base = self.config.reward_rate;
                let bonus = r.performance_bonus;
                base * (1.0 + bonus)
            })
            .unwrap_or(0.0)
    }

    /// Get aggregate pool statistics.
    pub fn get_pool_stats(&self) -> PoolStats {
        PoolStats {
            total_staked: self.total_staked.load(Ordering::Relaxed),
            total_rewards_distributed: self.total_rewards_distributed.load(Ordering::Relaxed),
            last_distribution_height: self.last_distribution_height.load(Ordering::Relaxed),
            providers_count: self.provider_rewards.len() as u64,
            reward_rate: self.config.reward_rate,
            bonus_rate: self.config.bonus_rate,
            reward_interval: self.config.reward_interval,
        }
    }

    /// Get the top N earners for the leaderboard.
    pub fn get_leaderboard(&self, top_n: usize) -> Vec<LeaderboardEntry> {
        let mut entries: Vec<LeaderboardEntry> = self
            .provider_rewards
            .iter()
            .filter(|r| r.value().staked_amount >= self.config.min_stake)
            .map(|r| {
                let v = r.value();
                LeaderboardEntry {
                    provider_id: r.key().clone(),
                    staked_amount: v.staked_amount,
                    accumulated_rewards: v.accumulated_rewards,
                    current_apy: self.config.reward_rate * (1.0 + v.performance_bonus),
                }
            })
            .collect();

        entries.sort_by(|a, b| b.accumulated_rewards.cmp(&a.accumulated_rewards));
        entries.truncate(top_n);
        entries
    }

    /// Get rewards info for a specific provider.
    pub fn get_provider_rewards(&self, provider_id: &str) -> Option<ProviderRewards> {
        self.provider_rewards
            .get(provider_id)
            .map(|r| r.value().clone())
    }
}

// ---------------------------------------------------------------------------
// Admin API handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{
        Query,
        State
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use crate::proxy::AppState;

#[derive(Debug, Deserialize)]
pub struct ClaimRequest {
    pub provider_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ProviderQuery {
    pub provider_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    #[serde(default = "default_top_n")]
    pub top_n: usize,
}

fn default_top_n() -> usize {
    20
}

fn verify_admin(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = &state.config.admin.api_key;
    let provided = headers
        .get("x-admin-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided.is_empty() || provided != expected {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn err(msg: &str, code: StatusCode) -> Response {
    (code, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn ok(val: serde_json::Value) -> Response {
    (StatusCode::OK, Json(val)).into_response()
}

/// GET /admin/staking/pool — Pool statistics.
async fn pool_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    ok(serde_json::to_value(state.staking_pool.get_pool_stats()).unwrap_or_default())
}

/// GET /admin/staking/rewards?provider_id=... — Provider rewards.
async fn rewards_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ProviderQuery>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    match q.provider_id {
        Some(pid) => match state.staking_pool.get_provider_rewards(&pid) {
            Some(rewards) => ok(serde_json::to_value(rewards).unwrap_or_default()),
            None => err(&format!("No rewards found for {}", pid), StatusCode::NOT_FOUND),
        },
        None => err("provider_id query parameter required", StatusCode::BAD_REQUEST),
    }
}

/// POST /admin/staking/claim — Claim rewards.
async fn claim_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ClaimRequest>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    match state.staking_pool.claim_rewards(&body.provider_id) {
        Ok(amount) => ok(serde_json::json!({
            "provider_id": body.provider_id,
            "claimed": amount,
        })),
        Err(e) => err(&e, StatusCode::BAD_REQUEST),
    }
}

/// GET /admin/staking/yield?provider_id=... — Current APY.
async fn yield_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ProviderQuery>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    match q.provider_id {
        Some(pid) => {
            let apy = state.staking_pool.get_yield(&pid);
            ok(serde_json::json!({
                "provider_id": pid,
                "apy": apy,
                "apy_pct": format!("{:.2}%", apy * 100.0),
            }))
        }
        None => err("provider_id query parameter required", StatusCode::BAD_REQUEST),
    }
}

/// GET /admin/staking/leaderboard — Top earners.
async fn leaderboard_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LeaderboardQuery>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    let entries = state.staking_pool.get_leaderboard(q.top_n);
    ok(serde_json::to_value(entries).unwrap_or_default())
}

/// Build the staking rewards admin router. Mounted under `/admin/staking`.
pub fn build_staking_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/staking/pool", get(pool_handler))
        .route("/admin/staking/rewards", get(rewards_handler))
        .route("/admin/staking/claim", post(claim_handler))
        .route("/admin/staking/yield", get(yield_handler))
        .route("/admin/staking/leaderboard", get(leaderboard_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> StakingRewardConfig {
        StakingRewardConfig {
            reward_rate: 0.05,
            reward_interval: 100,
            min_stake: 100,
            bonus_rate: 0.02,
            performance_threshold: 0.9,
            pool_cap: 0,
        }
    }

    #[test]
    fn test_new_pool() {
        let pool = StakingRewardPool::new(test_config());
        let stats = pool.get_pool_stats();
        assert_eq!(stats.total_staked, 0);
        assert_eq!(stats.total_rewards_distributed, 0);
        assert_eq!(stats.providers_count, 0);
    }

    #[test]
    fn test_add_stake() {
        let pool = StakingRewardPool::new(test_config());
        pool.add_stake("provider1", 1000);
        let stats = pool.get_pool_stats();
        assert_eq!(stats.total_staked, 1000);
        assert_eq!(stats.providers_count, 1);
    }

    #[test]
    fn test_add_stake_accumulates() {
        let pool = StakingRewardPool::new(test_config());
        pool.add_stake("provider1", 1000);
        pool.add_stake("provider1", 500);
        let rewards = pool.get_provider_rewards("provider1").unwrap();
        assert_eq!(rewards.staked_amount, 1500);
    }

    #[test]
    fn test_remove_stake() {
        let pool = StakingRewardPool::new(test_config());
        pool.add_stake("provider1", 1000);
        pool.remove_stake("provider1", 400).unwrap();
        let rewards = pool.get_provider_rewards("provider1").unwrap();
        assert_eq!(rewards.staked_amount, 600);
    }

    #[test]
    fn test_remove_stake_insufficient() {
        let pool = StakingRewardPool::new(test_config());
        pool.add_stake("provider1", 1000);
        let result = pool.remove_stake("provider1", 2000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient"));
    }

    #[test]
    fn test_remove_stake_no_provider() {
        let pool = StakingRewardPool::new(test_config());
        let result = pool.remove_stake("nonexistent", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_performance_bonus_above_threshold() {
        let pool = StakingRewardPool::new(test_config());
        let bonus = pool.compute_performance_bonus(0.95);
        assert_eq!(bonus, 1.02);
    }

    #[test]
    fn test_compute_performance_bonus_below_threshold() {
        let pool = StakingRewardPool::new(test_config());
        let bonus = pool.compute_performance_bonus(0.5);
        assert_eq!(bonus, 1.0);
    }

    #[test]
    fn test_distribute_rewards_interval_not_reached() {
        let pool = StakingRewardPool::new(test_config());
        pool.add_stake("provider1", 1000);
        let health = DashMap::new();
        let distributed = pool.distribute_rewards(50, &health);
        assert_eq!(distributed, 0);
    }

    #[test]
    fn test_distribute_rewards() {
        let pool = StakingRewardPool::new(test_config());
        // Use a large stake so the reward doesn't truncate to 0
        // Formula: stake * rate * (interval / 525600) * bonus
        // = 10_000_000_000 * 0.05 * (100 / 525600) * 1.02 = ~971 nanoERG
        pool.add_stake("provider1", 10_000_000_000);
        let health = DashMap::new();
        health.insert("provider1".to_string(), 0.95);
        let distributed = pool.distribute_rewards(200, &health);
        assert!(distributed > 0);
    }

    #[test]
    fn test_claim_rewards_no_provider() {
        let pool = StakingRewardPool::new(test_config());
        let result = pool.claim_rewards("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_pool_stats() {
        let pool = StakingRewardPool::new(test_config());
        pool.add_stake("provider1", 1000);
        pool.add_stake("provider2", 2000);
        let stats = pool.get_pool_stats();
        assert_eq!(stats.total_staked, 3000);
        assert_eq!(stats.providers_count, 2);
        assert_eq!(stats.reward_rate, 0.05);
        assert_eq!(stats.bonus_rate, 0.02);
    }
}
