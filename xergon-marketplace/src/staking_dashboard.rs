use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PoolStatus {
    Active,
    Paused,
    Closed,
    Slashing,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SortField {
    Tvl,
    Apy,
    Stakers,
    Name,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RewardClaim {
    pub claim_id: String,
    pub amount: f64,
    pub pool_id: String,
    pub claimed_at_height: u64,
    pub tx_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct PoolOverview {
    pub pool_id: String,
    pub name: String,
    pub tvl_erg: f64,
    pub tvl_xrg: f64,
    pub apy_current: f64,
    pub apy_30d: f64,
    pub apy_90d: f64,
    pub staker_count: u64,
    pub status: PoolStatus,
    pub reward_token_id: String,
    pub epoch_progress: f64,
    pub next_epoch_height: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct YieldComparison {
    pub pool_id: String,
    pub pool_name: String,
    pub apy: f64,
    pub risk_score: f64,
    pub lock_period_days: u32,
    pub min_stake: f64,
    pub auto_compound_available: bool,
    pub historical_rewards: Vec<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DelegationInfo {
    pub delegator_address: String,
    pub pool_id: String,
    pub delegated_amount: f64,
    pub rewards_earned: f64,
    pub delegation_height: u64,
    pub auto_compound: bool,
    pub yield_percentage: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct RewardsTracker {
    pub claim_history: Vec<RewardClaim>,
    pub pending_rewards: f64,
    pub total_claimed: f64,
    pub next_payout_estimate: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct YieldEstimate {
    pub pool_id: String,
    pub amount: f64,
    pub duration_days: u32,
    pub projected_earnings: f64,
    pub effective_apy: f64,
    pub compound_frequency: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct DelegateRequest {
    pub user_address: String,
    pub pool_id: String,
    pub amount: f64,
    pub auto_compound: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct UndelegateRequest {
    pub user_address: String,
    pub pool_id: String,
    pub amount: f64,
}

// ---------------------------------------------------------------------------
// StakingDashboard
// ---------------------------------------------------------------------------

/// Staking dashboard with pool overview, yield comparison, delegation UI,
/// and rewards tracker. Uses DashMap for concurrent-safe state.
pub struct StakingDashboard {
    pool_cache: DashMap<String, PoolOverview>,
    delegations: DashMap<String, Vec<DelegationInfo>>,
    recent_claims: std::sync::Mutex<VecDeque<RewardClaim>>,
    total_tvl: AtomicU64,
}

impl StakingDashboard {
    pub fn new() -> Self {
        Self {
            pool_cache: DashMap::new(),
            delegations: DashMap::new(),
            recent_claims: std::sync::Mutex::new(VecDeque::with_capacity(1000)),
            total_tvl: AtomicU64::new(0),
        }
    }
}

impl Default for StakingDashboard {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Business Logic
// ---------------------------------------------------------------------------

impl StakingDashboard {
    /// Register or update a pool. Recalculates total TVL.
    pub fn upsert_pool(&self, pool: PoolOverview) {
        let tvl_delta = (pool.tvl_erg + pool.tvl_xrg) as u64;
        if let Some(existing) = self.pool_cache.get(&pool.pool_id) {
            let old = (existing.tvl_erg + existing.tvl_xrg) as u64;
            self.total_tvl
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
                    Some(c.saturating_sub(old).saturating_add(tvl_delta))
                })
                .ok();
        } else {
            self.total_tvl.fetch_add(tvl_delta, Ordering::Relaxed);
        }
        self.pool_cache.insert(pool.pool_id.clone(), pool);
    }

    /// Remove a pool and deduct its TVL.
    pub fn remove_pool(&self, pool_id: &str) -> bool {
        if let Some((_, pool)) = self.pool_cache.remove(pool_id) {
            self.total_tvl.fetch_sub((pool.tvl_erg + pool.tvl_xrg) as u64, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Refresh pool data — increments epoch progress for active pools.
    pub fn refresh_pool_data(&self) -> usize {
        let mut n = 0;
        for mut e in self.pool_cache.iter_mut() {
            if e.value().status == PoolStatus::Active {
                e.value_mut().epoch_progress = (e.value().epoch_progress + 1.0) % 100.0;
                n += 1;
            }
        }
        n
    }

    /// Compare yields across selected pools, sorted by APY descending.
    pub fn compare_pools(&self, ids: &[&str]) -> Result<Vec<YieldComparison>, String> {
        if ids.is_empty() {
            return Err("pool_ids must not be empty".into());
        }
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.pool_cache.get(*id) {
                Some(p) => {
                    let v = p.value();
                    out.push(YieldComparison {
                        pool_id: v.pool_id.clone(),
                        pool_name: v.name.clone(),
                        apy: v.apy_current,
                        risk_score: 1.0 / (1.0 + v.staker_count as f64),
                        lock_period_days: 30,
                        min_stake: 100.0,
                        auto_compound_available: true,
                        historical_rewards: vec![v.apy_90d, v.apy_30d, v.apy_current],
                    });
                }
                None => return Err(format!("pool not found: {}", id)),
            }
        }
        out.sort_by(|a, b| b.apy.partial_cmp(&a.apy).unwrap_or(std::cmp::Ordering::Equal));
        Ok(out)
    }

    /// Get all delegations for a user.
    pub fn get_user_delegations(&self, addr: &str) -> Vec<DelegationInfo> {
        self.delegations.get(addr).map(|v| v.clone()).unwrap_or_default()
    }

    /// Delegate tokens to a pool.
    pub fn delegate(&self, req: &DelegateRequest) -> Result<DelegationInfo, String> {
        if !self.pool_cache.contains_key(&req.pool_id) {
            return Err(format!("pool not found: {}", req.pool_id));
        }
        if req.amount <= 0.0 {
            return Err("amount must be positive".into());
        }
        let del = DelegationInfo {
            delegator_address: req.user_address.clone(),
            pool_id: req.pool_id.clone(),
            delegated_amount: req.amount,
            rewards_earned: 0.0,
            delegation_height: 0,
            auto_compound: req.auto_compound,
            yield_percentage: 0.0,
        };
        let result = del.clone();
        self.delegations
            .entry(req.user_address.clone())
            .and_modify(|d| d.push(del.clone()))
            .or_insert_with(|| vec![del]);
        if let Some(mut pool) = self.pool_cache.get_mut(&req.pool_id) {
            pool.staker_count += 1;
            pool.tvl_erg += req.amount;
        }
        Ok(result)
    }

    /// Undelegate tokens from a pool.
    pub fn undelegate(&self, req: &UndelegateRequest) -> Result<f64, String> {
        if req.amount <= 0.0 {
            return Err("amount must be positive".into());
        }
        let mut total = 0.0;
        let mut rm = Vec::new();
        if let Some(mut ud) = self.delegations.get_mut(&req.user_address) {
            for (i, d) in ud.iter_mut().enumerate() {
                if d.pool_id == req.pool_id && d.delegated_amount > 0.0 {
                    let take = d.delegated_amount.min(req.amount - total);
                    d.delegated_amount -= take;
                    total += take;
                    if d.delegated_amount <= 0.0 {
                        rm.push(i);
                    }
                    if total >= req.amount {
                        break;
                    }
                }
            }
            for &idx in rm.iter().rev() {
                ud.remove(idx);
            }
        }
        if total <= 0.0 {
            return Err("no delegation found for the given pool".into());
        }
        if let Some(mut pool) = self.pool_cache.get_mut(&req.pool_id) {
            pool.tvl_erg = (pool.tvl_erg - total).max(0.0);
            pool.staker_count = pool.staker_count.saturating_sub(rm.len() as u64);
        }
        Ok(total)
    }

    /// Get rewards history for a user.
    pub fn get_rewards_history(&self, addr: &str) -> RewardsTracker {
        let prefix = format!("{}:", addr);
        let claims: Vec<RewardClaim> = self
            .recent_claims
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.tx_id.starts_with(&prefix))
            .cloned()
            .collect();
        let total_claimed: f64 = claims.iter().map(|c| c.amount).sum();
        let pending = self.get_user_delegations(addr).iter().map(|d| d.rewards_earned).sum();
        RewardsTracker {
            claim_history: claims,
            pending_rewards: pending,
            total_claimed,
            next_payout_estimate: Utc::now() + chrono::Duration::days(1),
        }
    }

    /// Record a reward claim into the ring buffer (max 1000).
    pub fn record_claim(&self, claim: RewardClaim) {
        let mut buf = self.recent_claims.lock().unwrap();
        if buf.len() >= 1000 {
            buf.pop_front();
        }
        buf.push_back(claim);
    }

    /// Get top pools leaderboard sorted by the given field.
    pub fn get_top_pools(&self, limit: usize, sort: SortField) -> Vec<PoolOverview> {
        let mut pools: Vec<PoolOverview> = self.pool_cache.iter().map(|r| r.value().clone()).collect();
        match sort {
            SortField::Tvl => pools.sort_by(|a, b| {
                (b.tvl_erg + b.tvl_xrg)
                    .partial_cmp(&(a.tvl_erg + a.tvl_xrg))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortField::Apy => pools.sort_by(|a, b| {
                b.apy_current
                    .partial_cmp(&a.apy_current)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortField::Stakers => pools.sort_by(|a, b| b.staker_count.cmp(&a.staker_count)),
            SortField::Name => pools.sort_by(|a, b| a.name.cmp(&b.name)),
        }
        pools.truncate(limit);
        pools
    }

    /// Estimate projected yield (daily compounding).
    pub fn estimate_yield(&self, pool_id: &str, amt: f64, days: u32) -> Result<YieldEstimate, String> {
        if amt <= 0.0 { return Err("amount must be positive".into()); }
        if days == 0 { return Err("duration_days must be positive".into()); }
        let pool = self.pool_cache.get(pool_id)
            .ok_or_else(|| format!("pool not found: {}", pool_id))?;
        let v = pool.value();
        let daily = (v.apy_current / 100.0) / 365.0;
        let projected = amt * ((1.0 + daily).powi(days as i32) - 1.0);
        Ok(YieldEstimate {
            pool_id: v.pool_id.clone(), amount: amt, duration_days: days,
            projected_earnings: projected, effective_apy: v.apy_current,
            compound_frequency: "daily".to_string(),
        })
    }

    pub fn get_total_tvl(&self) -> u64 { self.total_tvl.load(Ordering::Relaxed) }
    pub fn get_all_pools(&self) -> Vec<PoolOverview> {
        self.pool_cache.iter().map(|r| r.value().clone()).collect()
    }
    pub fn get_pool(&self, id: &str) -> Option<PoolOverview> {
        self.pool_cache.get(id).map(|p| p.value().clone())
    }
    pub fn pool_count(&self) -> usize { self.pool_cache.len() }
    pub fn claim_count(&self) -> usize { self.recent_claims.lock().unwrap().len() }
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

impl StakingDashboard {
    pub fn handle_pools_overview(&self) -> Vec<PoolOverview> { self.get_all_pools() }
    pub fn handle_pool_comparison(&self, ids: &[&str]) -> Result<Vec<YieldComparison>, String> {
        self.compare_pools(ids)
    }
    pub fn handle_user_delegations(&self, addr: &str) -> Vec<DelegationInfo> {
        self.get_user_delegations(addr)
    }
    pub fn handle_rewards_tracker(&self, addr: &str) -> RewardsTracker {
        self.get_rewards_history(addr)
    }
    pub fn handle_yield_estimate(&self, pid: &str, amt: f64, days: u32) -> Result<YieldEstimate, String> {
        self.estimate_yield(pid, amt, days)
    }
    pub fn handle_leaderboard(&self, limit: usize, sort: SortField) -> Vec<PoolOverview> {
        self.get_top_pools(limit, sort)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn mp(id: &str, name: &str, apy: f64, tvl: f64, stk: u64) -> PoolOverview {
        PoolOverview {
            pool_id: id.to_string(), name: name.to_string(),
            tvl_erg: tvl, tvl_xrg: tvl * 0.5,
            apy_current: apy, apy_30d: apy * 0.95, apy_90d: apy * 0.90,
            staker_count: stk, status: PoolStatus::Active,
            reward_token_id: "xrg".to_string(),
            epoch_progress: 42.0, next_epoch_height: 100_000,
        }
    }

    fn d() -> StakingDashboard {
        let d = StakingDashboard::new();
        d.upsert_pool(mp("pa", "Alpha", 12.5, 50_000.0, 120));
        d.upsert_pool(mp("pb", "Beta", 8.3, 30_000.0, 80));
        d.upsert_pool(mp("pc", "Gamma", 15.0, 80_000.0, 200));
        d
    }

    #[test]
    fn test_upsert_remove_pool() {
        let s = StakingDashboard::new();
        s.upsert_pool(mp("x", "X", 10.0, 1_000.0, 5));
        assert_eq!(s.pool_count(), 1);
        assert!(s.remove_pool("x"));
        assert_eq!(s.pool_count(), 0);
        assert!(!s.remove_pool("nope"));
    }

    #[test]
    fn test_total_tvl() {
        let s = d();
        let t = s.get_total_tvl();
        assert!(t > 0);
        s.remove_pool("pa");
        assert!(s.get_total_tvl() < t);
    }

    #[test]
    fn test_refresh() {
        let s = d();
        assert_eq!(s.refresh_pool_data(), 3);
        assert!((s.get_pool("pa").unwrap().epoch_progress - 43.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_paused_skipped() {
        let s = StakingDashboard::new();
        let mut p = mp("px", "P", 5.0, 1_000.0, 10);
        p.status = PoolStatus::Paused;
        s.upsert_pool(p);
        assert_eq!(s.refresh_pool_data(), 0);
    }

    #[test]
    fn test_compare_sorts_by_apy() {
        let c = d().compare_pools(&["pa", "pb", "pc"]).unwrap();
        assert_eq!(c[0].pool_id, "pc");
        assert_eq!(c[1].pool_id, "pa");
        assert_eq!(c[2].pool_id, "pb");
    }

    #[test]
    fn test_compare_errors() {
        let s = d();
        assert!(s.compare_pools(&[]).is_err());
        assert!(s.compare_pools(&["zzz"]).is_err());
    }

    #[test]
    fn test_delegate_undelegate() {
        let s = d();
        let r = s.delegate(&DelegateRequest {
            user_address: "a1".into(), pool_id: "pa".into(),
            amount: 5_000.0, auto_compound: true,
        }).unwrap();
        assert_eq!(r.delegated_amount, 5_000.0);
        assert_eq!(s.get_pool("pa").unwrap().staker_count, 121);
        let u = s.undelegate(&UndelegateRequest {
            user_address: "a1".into(), pool_id: "pa".into(), amount: 2_000.0,
        }).unwrap();
        assert!((u - 2_000.0).abs() < f64::EPSILON);
        assert!((s.get_user_delegations("a1")[0].delegated_amount - 3_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_undelegate_full() {
        let s = d();
        s.delegate(&DelegateRequest {
            user_address: "a2".into(), pool_id: "pa".into(),
            amount: 2_000.0, auto_compound: false,
        }).unwrap();
        s.undelegate(&UndelegateRequest {
            user_address: "a2".into(), pool_id: "pa".into(), amount: 2_000.0,
        }).unwrap();
        assert!(s.get_user_delegations("a2").is_empty());
    }

    #[test]
    fn test_delegate_invalid() {
        let s = d();
        assert!(s.delegate(&DelegateRequest {
            user_address: "a".into(), pool_id: "nope".into(),
            amount: 100.0, auto_compound: false,
        }).is_err());
        assert!(s.delegate(&DelegateRequest {
            user_address: "a".into(), pool_id: "pa".into(),
            amount: 0.0, auto_compound: false,
        }).is_err());
    }

    #[test]
    fn test_claims_and_history() {
        let s = StakingDashboard::new();
        s.record_claim(RewardClaim {
            claim_id: "c1".into(), amount: 50.0, pool_id: "p".into(),
            claimed_at_height: 100, tx_id: "u1:t1".into(),
        });
        s.record_claim(RewardClaim {
            claim_id: "c2".into(), amount: 30.0, pool_id: "p".into(),
            claimed_at_height: 200, tx_id: "u2:t2".into(),
        });
        let h = s.get_rewards_history("u1");
        assert_eq!(h.claim_history.len(), 1);
        assert!((h.total_claimed - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ring_buffer_cap() {
        let s = StakingDashboard::new();
        for i in 0..1100u64 {
            s.record_claim(RewardClaim {
                claim_id: format!("c{}", i), amount: 1.0,
                pool_id: "p".into(), claimed_at_height: i,
                tx_id: format!("u:{}", i),
            });
        }
        assert_eq!(s.claim_count(), 1000);
    }

    #[test]
    fn test_top_pools() {
        let s = d();
        assert_eq!(s.get_top_pools(2, SortField::Tvl)[0].pool_id, "pc");
        assert_eq!(s.get_top_pools(10, SortField::Name)[0].name, "Alpha");
    }

    #[test]
    fn test_estimate_yield() {
        let s = d();
        let e = s.estimate_yield("pa", 10_000.0, 365).unwrap();
        assert!(e.projected_earnings > 0.0);
        assert_eq!(e.compound_frequency, "daily");
        assert!(s.estimate_yield("pa", 0.0, 30).is_err());
        assert!(s.estimate_yield("nope", 100.0, 30).is_err());
    }

    #[test]
    fn test_rest_handlers() {
        let s = d();
        assert_eq!(s.handle_pools_overview().len(), 3);
        assert_eq!(s.handle_user_delegations("x").len(), 0);
        assert!(s.handle_rewards_tracker("x").claim_history.is_empty());
        assert!(s.handle_yield_estimate("pc", 5_000.0, 90).is_ok());
        assert_eq!(s.handle_leaderboard(1, SortField::Apy)[0].pool_id, "pc");
    }
}
