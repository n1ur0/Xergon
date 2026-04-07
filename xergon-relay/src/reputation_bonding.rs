//! Reputation Bonding module.
//!
//! Providers stake ERG to boost their reputation score and face slashing
//! for violations. Bonds have configurable min/max amounts, durations,
//! slash rates, and reward rates. The reputation boost is proportional
//! to the bonded amount.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the reputation bonding system.
#[derive(Debug, Clone, Serialize)]
pub struct BondingConfig {
    /// Minimum nanoERG to bond (default: 10 ERG = 10_000_000_000)
    pub min_bond_amount: u64,
    /// Maximum nanoERG to bond (default: 10_000 ERG)
    pub max_bond_amount: u64,
    /// Percentage slashed on violation (default: 0.1 = 10%)
    pub slash_rate: f64,
    /// Annual yield rate (default: 0.05 = 5%)
    pub reward_rate: f64,
    /// Minimum bond duration in blocks (default: 720 ~ 2 days)
    pub min_bond_duration: u64,
    /// Reputation boost per ERG bonded (default: 0.001)
    pub reputation_boost_rate: f64,
    /// Auto-compound rewards into bond (default: false)
    pub auto_compound: bool,
}

impl Default for BondingConfig {
    fn default() -> Self {
        Self {
            min_bond_amount: 10_000_000_000,      // 10 ERG
            max_bond_amount: 10_000_000_000_000,   // 10_000 ERG
            slash_rate: 0.10,
            reward_rate: 0.05,
            min_bond_duration: 720,
            reputation_boost_rate: 0.001,
            auto_compound: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Status of a reputation bond.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondStatus {
    Active,
    Unbonding,
    Unbonded,
    Slashed,
    Expired,
}

/// A single slash event applied to a bond.
#[derive(Debug, Clone, Serialize)]
pub struct SlashEvent {
    pub reason: String,
    pub amount: u64,
    pub block_height: u64,
    pub timestamp: DateTime<Utc>,
}

/// A reputation bond held by a provider.
#[derive(Debug, Clone, Serialize)]
pub struct ReputationBond {
    pub id: String,
    pub provider_id: String,
    pub amount: u64,                // nanoERG
    pub bonded_at: u64,             // block height
    pub expires_at: u64,            // block height
    pub reputation_boost: f64,
    pub rewards_earned: u64,        // accumulated rewards in nanoERG
    pub status: BondStatus,
    pub slash_history: Vec<SlashEvent>,
}

// ---------------------------------------------------------------------------
// Aggregate stats
// ---------------------------------------------------------------------------

/// Summary statistics for the bonding system.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BondingStats {
    pub total_bonds: u64,
    pub active_bonds: u64,
    pub unbonding_bonds: u64,
    pub total_bonded: u64,
    pub total_slashed: u64,
    pub total_rewards_distributed: u64,
    pub slash_count: u64,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Manages reputation bonds for all providers.
pub struct BondingManager {
    config: BondingConfig,
    /// bond_id -> ReputationBond
    bonds: DashMap<String, ReputationBond>,
    /// provider_id -> Vec<bond_id>
    provider_bonds: DashMap<String, Vec<String>>,
    /// Monotonic counter for bond IDs
    next_id: AtomicU64,
    /// Running totals
    total_slashed: AtomicU64,
    total_rewards_distributed: AtomicU64,
}

impl BondingManager {
    /// Create a new bonding manager with the given config.
    pub fn new(config: BondingConfig) -> Self {
        Self {
            config,
            bonds: DashMap::new(),
            provider_bonds: DashMap::new(),
            next_id: AtomicU64::new(1),
            total_slashed: AtomicU64::new(0),
            total_rewards_distributed: AtomicU64::new(0),
        }
    }

    /// Create a new bond for a provider.
    ///
    /// Returns the created `ReputationBond` or an error string.
    pub fn bond(
        &self,
        provider_id: &str,
        amount: u64,
        current_block: u64,
    ) -> Result<ReputationBond, String> {
        if amount < self.config.min_bond_amount {
            return Err(format!(
                "Bond amount {} is below minimum {}",
                amount, self.config.min_bond_amount
            ));
        }
        if amount > self.config.max_bond_amount {
            return Err(format!(
                "Bond amount {} exceeds maximum {}",
                amount, self.config.max_bond_amount
            ));
        }

        let id = format!("bond_{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let reputation_boost = (amount as f64 / 1_000_000_000.0) * self.config.reputation_boost_rate;
        let expires_at = current_block + self.config.min_bond_duration;

        let bond = ReputationBond {
            id: id.clone(),
            provider_id: provider_id.to_string(),
            amount,
            bonded_at: current_block,
            expires_at,
            reputation_boost,
            rewards_earned: 0,
            status: BondStatus::Active,
            slash_history: Vec::new(),
        };

        self.bonds.insert(id.clone(), bond.clone());
        self.provider_bonds
            .entry(provider_id.to_string())
            .or_default()
            .push(id.clone());

        info!(bond_id = %id, provider = %provider_id, amount, "New reputation bond created");
        Ok(bond)
    }

    /// Start unbonding a bond. Funds become withdrawable after `min_bond_duration`
    /// blocks from the unbonding initiation.
    pub fn unbond(&self, bond_id: &str, current_block: u64) -> Result<(), String> {
        let mut bond = self
            .bonds
            .get_mut(bond_id)
            .ok_or_else(|| format!("Bond {} not found", bond_id))?;

        match bond.status {
            BondStatus::Active | BondStatus::Unbonding => {
                bond.status = BondStatus::Unbonding;
                // Extend expiry by min_bond_duration from now (unbonding period)
                bond.expires_at = current_block + self.config.min_bond_duration;
                debug!(bond_id, "Bond moved to unbonding");
                Ok(())
            }
            s => Err(format!("Cannot unbond bond in status {:?}", s)),
        }
    }

    /// Slash a provider's active bonds for a violation.
    ///
    /// `severity` is a multiplier (1.0 = normal, 2.0 = double slash).
    /// Returns the total amount slashed.
    pub fn slash(
        &self,
        provider_id: &str,
        reason: &str,
        severity: f64,
        current_block: u64,
    ) -> Result<u64, String> {
        let bond_ids = self
            .provider_bonds
            .get(provider_id)
            .map(|v| v.clone())
            .unwrap_or_default();

        if bond_ids.is_empty() {
            return Err(format!("No bonds found for provider {}", provider_id));
        }

        let slash_fraction = (self.config.slash_rate * severity.clamp(0.1, 5.0)).min(1.0);
        let mut total_slashed: u64 = 0;

        for bond_id in &bond_ids {
            if let Some(mut bond) = self.bonds.get_mut(bond_id) {
                if bond.status != BondStatus::Active {
                    continue;
                }
                let slash_amount = (bond.amount as f64 * slash_fraction) as u64;
                if slash_amount == 0 {
                    continue;
                }
                bond.amount = bond.amount.saturating_sub(slash_amount);
                bond.reputation_boost =
                    (bond.amount as f64 / 1_000_000_000.0) * self.config.reputation_boost_rate;

                if bond.amount == 0 {
                    bond.status = BondStatus::Slashed;
                }

                bond.slash_history.push(SlashEvent {
                    reason: reason.to_string(),
                    amount: slash_amount,
                    block_height: current_block,
                    timestamp: Utc::now(),
                });

                total_slashed += slash_amount;
            }
        }

        if total_slashed > 0 {
            self.total_slashed.fetch_add(total_slashed, Ordering::Relaxed);
            warn!(
                provider = %provider_id,
                slashed = total_slashed,
                reason,
                "Provider bonds slashed"
            );
        }

        Ok(total_slashed)
    }

    /// Get a single bond by ID.
    pub fn get_bond(&self, bond_id: &str) -> Option<ReputationBond> {
        self.bonds.get(bond_id).map(|r| r.clone())
    }

    /// Get all bonds for a provider.
    pub fn get_provider_bonds(&self, provider_id: &str) -> Vec<ReputationBond> {
        let bond_ids = self
            .provider_bonds
            .get(provider_id)
            .map(|v| v.clone())
            .unwrap_or_default();

        bond_ids
            .iter()
            .filter_map(|id| self.bonds.get(id).map(|r| r.clone()))
            .collect()
    }

    /// Compute the total reputation boost for a provider across all active bonds.
    pub fn compute_reputation_boost(&self, provider_id: &str) -> f64 {
        self.get_provider_bonds(provider_id)
            .iter()
            .filter(|b| b.status == BondStatus::Active)
            .map(|b| b.reputation_boost)
            .sum()
    }

    /// Distribute rewards to all active bonds based on elapsed blocks.
    ///
    /// The reward formula is: `amount * reward_rate * blocks_elapsed / blocks_per_year`
    /// where blocks_per_year = 525600 (Ergo ~2min blocks).
    ///
    /// Returns the total nanoERG distributed.
    pub fn distribute_rewards(&self, current_block: u64) -> u64 {
        let blocks_per_year: f64 = 525_600.0;
        let mut total_distributed: u64 = 0;

        for mut entry in self.bonds.iter_mut() {
            let bond = entry.value_mut();
            if bond.status != BondStatus::Active {
                continue;
            }

            // Use bonded_at as the baseline for first distribution, then track via expires_at.
            // Simple approach: reward proportional to blocks since last distribution.
            // We store bonded_at and use a heuristic: reward since bond creation, minus already earned.
            let blocks_elapsed = (current_block - bond.bonded_at).max(0) as f64;
            let expected_total_reward =
                (bond.amount as f64 * self.config.reward_rate * blocks_elapsed / blocks_per_year)
                    as u64;
            let new_reward = expected_total_reward.saturating_sub(bond.rewards_earned);

            if new_reward > 0 {
                bond.rewards_earned += new_reward;
                if self.config.auto_compound {
                    bond.amount += new_reward;
                    bond.reputation_boost =
                        (bond.amount as f64 / 1_000_000_000.0) * self.config.reputation_boost_rate;
                }
                total_distributed += new_reward;
            }

            // Check expiry
            if current_block >= bond.expires_at {
                bond.status = BondStatus::Expired;
                debug!(bond_id = %bond.id, "Bond expired");
            }
        }

        if total_distributed > 0 {
            self.total_rewards_distributed
                .fetch_add(total_distributed, Ordering::Relaxed);
            debug!(total = total_distributed, "Bonding rewards distributed");
        }

        total_distributed
    }

    /// Process unbonding bonds that have completed their unbonding period.
    pub fn process_unbonding(&self, current_block: u64) -> Vec<String> {
        let mut completed = Vec::new();

        for mut entry in self.bonds.iter_mut() {
            let bond = entry.value_mut();
            if bond.status == BondStatus::Unbonding && current_block >= bond.expires_at {
                bond.status = BondStatus::Unbonded;
                completed.push(bond.id.clone());
                debug!(bond_id = %bond.id, "Unbonding completed");
            }
        }

        completed
    }

    /// Get aggregate bonding statistics.
    pub fn get_stats(&self) -> BondingStats {
        let mut stats = BondingStats::default();
        stats.total_slashed = self.total_slashed.load(Ordering::Relaxed);
        stats.total_rewards_distributed = self.total_rewards_distributed.load(Ordering::Relaxed);

        for entry in self.bonds.iter() {
            let bond = entry.value();
            stats.total_bonds += 1;
            match bond.status {
                BondStatus::Active => {
                    stats.active_bonds += 1;
                    stats.total_bonded += bond.amount;
                }
                BondStatus::Unbonding => {
                    stats.unbonding_bonds += 1;
                    stats.total_bonded += bond.amount;
                }
                _ => {}
            }
            stats.slash_count += bond.slash_history.len() as u64;
        }

        stats
    }

    /// Claim rewards for a specific bond, resetting earned counter.
    /// Returns the claimable nanoERG amount.
    pub fn claim_bond_rewards(&self, bond_id: &str) -> Result<u64, String> {
        let mut bond = self
            .bonds
            .get_mut(bond_id)
            .ok_or_else(|| format!("Bond {} not found", bond_id))?;

        let claimable = bond.rewards_earned;
        if claimable == 0 {
            return Ok(0);
        }

        bond.rewards_earned = 0;
        info!(bond_id, claimable, "Bond rewards claimed");
        Ok(claimable)
    }
}

// ---------------------------------------------------------------------------
// Admin API handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use crate::proxy::AppState;

#[derive(Debug, Deserialize)]
pub struct BondRequest {
    pub provider_id: String,
    pub amount: u64,
}

#[derive(Debug, Deserialize)]
pub struct SlashRequest {
    pub provider_id: String,
    pub reason: String,
    #[serde(default = "default_severity")]
    pub severity: f64,
}

fn default_severity() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct BondQuery {
    pub provider_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RewardsQuery {
    pub provider_id: Option<String>,
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

/// POST /admin/bonding/bond — Create a new reputation bond.
async fn bond_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BondRequest>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    let mgr = &state.bonding_manager;
    // Use Unix epoch seconds as a proxy for block height when no chain scanner
    let block_height = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 120) // ~2 min blocks
        .unwrap_or(0);
    match mgr.bond(&body.provider_id, body.amount, block_height) {
        Ok(bond) => ok(serde_json::to_value(bond).unwrap_or_default()),
        Err(e) => err(&e, StatusCode::BAD_REQUEST),
    }
}

/// POST /admin/bonding/unbond/:id — Start unbonding.
async fn unbond_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    let mgr = &state.bonding_manager;
    let block_height = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 120)
        .unwrap_or(0);
    match mgr.unbond(&id, block_height) {
        Ok(()) => ok(serde_json::json!({ "status": "unbonding", "bond_id": id })),
        Err(e) => err(&e, StatusCode::BAD_REQUEST),
    }
}

/// GET /admin/bonding/bonds?provider_id=... — List bonds.
async fn bonds_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<BondQuery>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    let mgr = &state.bonding_manager;
    let bonds = match q.provider_id {
        Some(pid) => mgr.get_provider_bonds(&pid),
        None => mgr.bonds.iter().map(|r| r.value().clone()).collect::<Vec<_>>(),
    };
    ok(serde_json::to_value(bonds).unwrap_or_default())
}

/// GET /admin/bonding/stats — Aggregate statistics.
async fn stats_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    ok(serde_json::to_value(state.bonding_manager.get_stats()).unwrap_or_default())
}

/// POST /admin/bonding/slash — Slash a provider's bonds.
async fn slash_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SlashRequest>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    let mgr = &state.bonding_manager;
    let block_height = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 120)
        .unwrap_or(0);
    match mgr.slash(&body.provider_id, &body.reason, body.severity, block_height) {
        Ok(slashed) => ok(serde_json::json!({
            "status": "slashed",
            "provider_id": body.provider_id,
            "amount_slashed": slashed,
        })),
        Err(e) => err(&e, StatusCode::BAD_REQUEST),
    }
}

/// GET /admin/bonding/rewards?provider_id=... — View rewards.
async fn rewards_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RewardsQuery>,
) -> Response {
    if verify_admin(&state, &headers).is_err() {
        return err("Forbidden", StatusCode::FORBIDDEN);
    }
    let mgr = &state.bonding_manager;
    match q.provider_id {
        Some(pid) => {
            let bonds = mgr.get_provider_bonds(&pid);
            let rewards: Vec<serde_json::Value> = bonds
                .iter()
                .map(|b| {
                    serde_json::json!({
                        "bond_id": b.id,
                        "status": b.status,
                        "rewards_earned": b.rewards_earned,
                        "amount": b.amount,
                    })
                })
                .collect();
            ok(serde_json::json!({
                "provider_id": pid,
                "bonds": rewards,
                "total_rewards": rewards.iter().map(|r| r["rewards_earned"].as_u64().unwrap_or(0)).sum::<u64>(),
            }))
        }
        None => {
            let stats = mgr.get_stats();
            ok(serde_json::json!({
                "total_rewards_distributed": stats.total_rewards_distributed,
                "total_bonds": stats.total_bonds,
                "active_bonds": stats.active_bonds,
            }))
        }
    }
}

/// Build the bonding admin router. Mounted under `/admin/bonding`.
pub fn build_bonding_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin/bonding/bond", post(bond_handler))
        .route("/admin/bonding/unbond/{id}", post(unbond_handler))
        .route("/admin/bonding/bonds", get(bonds_handler))
        .route("/admin/bonding/stats", get(stats_handler))
        .route("/admin/bonding/slash", post(slash_handler))
        .route("/admin/bonding/rewards", get(rewards_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> BondingConfig {
        BondingConfig {
            min_bond_amount: 100,
            max_bond_amount: 1_000_000_000,
            slash_rate: 0.10,
            reward_rate: 0.05,
            min_bond_duration: 100,
            reputation_boost_rate: 0.001,
            auto_compound: false,
        }
    }

    #[test]
    fn test_new_manager() {
        let mgr = BondingManager::new(test_config());
        let stats = mgr.get_stats();
        assert_eq!(stats.total_bonds, 0);
        assert_eq!(stats.active_bonds, 0);
        assert_eq!(stats.total_slashed, 0);
    }

    #[test]
    fn test_bond_creates_active_bond() {
        let mgr = BondingManager::new(test_config());
        let bond = mgr.bond("provider1", 1000, 50).unwrap();
        assert_eq!(bond.status, BondStatus::Active);
        assert_eq!(bond.amount, 1000);
        assert_eq!(bond.provider_id, "provider1");
        assert!(bond.reputation_boost > 0.0);
    }

    #[test]
    fn test_bond_below_minimum_rejects() {
        let mgr = BondingManager::new(test_config());
        let result = mgr.bond("provider1", 50, 50);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("below minimum"));
    }

    #[test]
    fn test_bond_above_maximum_rejects() {
        let mgr = BondingManager::new(test_config());
        let result = mgr.bond("provider1", 2_000_000_000, 50);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds maximum"));
    }

    #[test]
    fn test_unbond_active_bond() {
        let mgr = BondingManager::new(test_config());
        let bond = mgr.bond("provider1", 1000, 50).unwrap();
        let result = mgr.unbond(&bond.id, 60);
        assert!(result.is_ok());
        let updated = mgr.get_bond(&bond.id).unwrap();
        assert_eq!(updated.status, BondStatus::Unbonding);
    }

    #[test]
    fn test_unbond_nonexistent_bond() {
        let mgr = BondingManager::new(test_config());
        let result = mgr.unbond("nonexistent", 60);
        assert!(result.is_err());
    }

    #[test]
    fn test_slash_provider_bonds() {
        let mgr = BondingManager::new(test_config());
        mgr.bond("provider1", 10000, 50).unwrap();
        let slashed = mgr.slash("provider1", "violation", 1.0, 60).unwrap();
        assert!(slashed > 0);
        let bond = mgr.get_provider_bonds("provider1")[0].clone();
        assert!(bond.amount < 10000);
        assert_eq!(bond.slash_history.len(), 1);
    }

    #[test]
    fn test_slash_no_bonds_for_provider() {
        let mgr = BondingManager::new(test_config());
        let result = mgr.slash("nonexistent", "violation", 1.0, 60);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_bond_by_id() {
        let mgr = BondingManager::new(test_config());
        assert!(mgr.get_bond("nonexistent").is_none());
        let bond = mgr.bond("provider1", 1000, 50).unwrap();
        let fetched = mgr.get_bond(&bond.id).unwrap();
        assert_eq!(fetched.id, bond.id);
    }

    #[test]
    fn test_compute_reputation_boost() {
        let mgr = BondingManager::new(test_config());
        assert_eq!(mgr.compute_reputation_boost("provider1"), 0.0);
        mgr.bond("provider1", 1000, 50).unwrap();
        let boost = mgr.compute_reputation_boost("provider1");
        assert!(boost > 0.0);
    }

    #[test]
    fn test_distribute_rewards() {
        let mgr = BondingManager::new(test_config());
        mgr.bond("provider1", 10000, 50).unwrap();
        let distributed = mgr.distribute_rewards(50);
        // At block 50 with bond at block 50, blocks_elapsed = 0, so no rewards yet
        assert_eq!(distributed, 0);
        // Advance enough blocks to earn rewards
        let distributed = mgr.distribute_rewards(100_000);
        assert!(distributed > 0);
    }

    #[test]
    fn test_get_stats_after_bonding() {
        let mgr = BondingManager::new(test_config());
        mgr.bond("provider1", 1000, 50).unwrap();
        mgr.bond("provider1", 2000, 50).unwrap();
        let stats = mgr.get_stats();
        assert_eq!(stats.total_bonds, 2);
        assert_eq!(stats.active_bonds, 2);
        assert_eq!(stats.total_bonded, 3000);
    }

    #[test]
    fn test_claim_bond_rewards() {
        let mgr = BondingManager::new(test_config());
        let bond = mgr.bond("provider1", 10000, 50).unwrap();
        // No rewards yet
        let claimed = mgr.claim_bond_rewards(&bond.id).unwrap();
        assert_eq!(claimed, 0);
        // Distribute rewards at a future block
        mgr.distribute_rewards(100_000);
        let claimed = mgr.claim_bond_rewards(&bond.id).unwrap();
        assert!(claimed > 0);
        // Second claim should be 0
        let claimed_again = mgr.claim_bond_rewards(&bond.id).unwrap();
        assert_eq!(claimed_again, 0);
    }
}
