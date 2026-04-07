//! Multi-sig treasury management for governance-driven spending.
//!
//! Manages treasury state: deposits, spends (locked by passed proposals),
//! fund locking/unlocking, and audit trail. Uses threshold signatures
//! (Ergo's atLeast(k, keys) pattern) for spend authorization.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

// ─── Constants ──────────────────────────────────────────────────────

const AUDIT_MAX: usize = 20_000;
const PROPORTION_BASE: u64 = 10_000_000;

// ─── Enums ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpendStatus {
    Pending,
    Completed,
    Failed,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Deposit,
    SpendRequested,
    SpendCompleted,
    SpendFailed,
    FundsLocked,
    FundsUnlocked,
    SignatoryRotated,
    EmergencySpend,
}

// ─── Data Types ─────────────────────────────────────────────────────

/// Threshold signature configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    pub required_signatures: u32,
    pub total_signatories: u32,
    pub signatory_addresses: Vec<String>,
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            required_signatures: 3,
            total_signatories: 5,
            signatory_addresses: vec![
                "9signatory1".to_string(),
                "9signatory2".to_string(),
                "9signatory3".to_string(),
                "9signatory4".to_string(),
                "9signatory5".to_string(),
            ],
        }
    }
}

/// Treasury state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryState {
    pub total_deposits_nanoerg: u64,
    pub total_spent_nanoerg: u64,
    pub available_balance: u64,
    pub locked_balance: u64,
    pub pending_spends: u64,
    pub completed_spends: u64,
    pub failed_spends: u64,
}

/// A treasury spend record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasurySpend {
    pub id: String,
    pub proposal_id: String,
    pub recipient: String,
    pub amount_nanoerg: u64,
    pub status: SpendStatus,
    pub locked_at: String,
    pub executed_at: Option<String>,
    pub tx_id: Option<String>,
    pub signatures_collected: u32,
    pub signatures_required: u32,
}

/// A deposit record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRecord {
    pub id: String,
    pub depositor: String,
    pub amount_nanoerg: u64,
    pub timestamp: String,
    pub tx_id: String,
}

/// Audit event for treasury operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryEvent {
    pub event_type: EventType,
    pub spend_id: Option<String>,
    pub actor: String,
    pub timestamp: String,
    pub amount_nanoerg: u64,
    pub details: String,
}

/// Signatory rotation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatoryRotation {
    pub id: String,
    pub old_signatories: Vec<String>,
    pub new_signatories: Vec<String>,
    pub effective_height: u64,
    pub overlap_blocks: u64,
    pub timestamp: String,
}

// ─── Core Functions ─────────────────────────────────────────────────

/// Generate ErgoScript for atLeast(k, keys) threshold signature.
pub fn create_threshold_script(required: u32, addresses: &[String]) -> String {
    let keys: Vec<String> = addresses.iter().map(|a| format!("PK(\"{}\")", a)).collect();
    format!("{{ atLeast({}, Coll({})) }}", required, keys.join(", "))
}

/// Verify a set of signatures meets the required threshold.
pub fn verify_threshold(
    signatures_count: usize,
    required: u32,
    total: u32,
) -> bool {
    signatures_count >= required as usize && signatures_count <= total as usize
}

/// Calculate proportional treasury distribution.
/// Returns (awarded_amount, remaining_amount).
pub fn calculate_proportional_spend(
    treasury_balance: u64,
    proportion_numerator: u64,
) -> (u64, u64) {
    if proportion_numerator == 0 {
        return (0, treasury_balance);
    }
    let awarded = (treasury_balance as u128 * proportion_numerator as u128 / PROPORTION_BASE as u128) as u64;
    let remaining = treasury_balance.saturating_sub(awarded);
    (awarded, remaining)
}

/// Validate a signatory rotation.
pub fn validate_rotation(
    old_config: &ThresholdConfig,
    new_required: u32,
    new_signatories: &[String],
) -> Result<(), String> {
    if new_signatories.is_empty() {
        return Err("Signatory list cannot be empty".to_string());
    }
    if new_required == 0 {
        return Err("Required signatures must be > 0".to_string());
    }
    if new_required as usize > new_signatories.len() {
        return Err(format!(
            "Required ({}) cannot exceed total signatories ({})",
            new_required,
            new_signatories.len()
        ));
    }
    // Require majority overlap during rotation
    let overlap = old_config
        .signatory_addresses
        .iter()
        .filter(|s| new_signatories.contains(s))
        .count();
    let min_overlap = (old_config.required_signatures as usize + 1) / 2;
    if overlap < min_overlap {
        return Err(format!(
            "Rotation requires at least {} overlapping signatories, got {}",
            min_overlap, overlap
        ));
    }
    Ok(())
}

// ─── App State ──────────────────────────────────────────────────────

/// Treasury app state (no Clone -- atomics).
#[derive(Debug)]
pub struct TreasuryAppState {
    pub spends: Arc<DashMap<String, TreasurySpend>>,
    pub deposits: Arc<DashMap<String, DepositRecord>>,
    pub events: Arc<DashMap<String, VecDeque<TreasuryEvent>>>,
    pub threshold: Arc<std::sync::RwLock<ThresholdConfig>>,
    pub total_deposited: AtomicU64,
    pub total_spent: AtomicU64,
    pub locked_funds: AtomicU64,
    pub event_total: AtomicU64,
}

impl TreasuryAppState {
    pub fn new() -> Self {
        Self {
            spends: Arc::new(DashMap::new()),
            deposits: Arc::new(DashMap::new()),
            events: Arc::new(DashMap::new()),
            threshold: Arc::new(std::sync::RwLock::new(ThresholdConfig::default())),
            total_deposited: AtomicU64::new(0),
            total_spent: AtomicU64::new(0),
            locked_funds: AtomicU64::new(0),
            event_total: AtomicU64::new(0),
        }
    }
}

impl Default for TreasuryAppState {
    fn default() -> Self {
        Self::new()
    }
}

impl TreasuryAppState {
    /// Record a deposit.
    pub fn deposit(&self, depositor: &str, amount_nanoerg: u64, tx_id: &str) -> DepositRecord {
        let record = DepositRecord {
            id: Uuid::new_v4().to_string(),
            depositor: depositor.to_string(),
            amount_nanoerg,
            timestamp: Utc::now().to_rfc3339(),
            tx_id: tx_id.to_string(),
        };
        self.deposits.insert(record.id.clone(), record.clone());
        self.total_deposited.fetch_add(amount_nanoerg, Ordering::Relaxed);
        self.record_event(EventType::Deposit, None, depositor, amount_nanoerg, "deposit");
        record
    }

    /// Request a spend (locks funds).
    pub fn request_spend(
        &self,
        proposal_id: &str,
        recipient: &str,
        amount_nanoerg: u64,
    ) -> Result<TreasurySpend, String> {
        let threshold = self.threshold.read().unwrap();
        let required = threshold.required_signatures;
        drop(threshold);

        let available = self.total_deposited.load(Ordering::Relaxed)
            - self.total_spent.load(Ordering::Relaxed)
            - self.locked_funds.load(Ordering::Relaxed);
        if amount_nanoerg > available {
            return Err(format!(
                "Insufficient available funds: requested {}, available {}",
                amount_nanoerg, available
            ));
        }

        let spend = TreasurySpend {
            id: Uuid::new_v4().to_string(),
            proposal_id: proposal_id.to_string(),
            recipient: recipient.to_string(),
            amount_nanoerg,
            status: SpendStatus::Pending,
            locked_at: Utc::now().to_rfc3339(),
            executed_at: None,
            tx_id: None,
            signatures_collected: 0,
            signatures_required: required,
        };
        self.locked_funds.fetch_add(amount_nanoerg, Ordering::Relaxed);
        self.spends.insert(spend.id.clone(), spend.clone());
        self.record_event(EventType::SpendRequested, Some(spend.id.clone()), recipient, amount_nanoerg, "spend requested");
        Ok(spend)
    }

    /// Add a signature to a pending spend.
    pub fn add_signature(&self, spend_id: &str, signer: &str) -> Result<TreasurySpend, String> {
        let threshold = self.threshold.read().unwrap();
        if !threshold.signatory_addresses.contains(&signer.to_string()) {
            return Err(format!("{} is not a valid signatory", signer));
        }
        drop(threshold);

        let mut spend = self.spends.get_mut(spend_id).ok_or("Spend not found")?;
        if spend.status != SpendStatus::Pending {
            return Err(format!("Spend is not pending: {:?}", spend.status));
        }
        spend.signatures_collected += 1;
        Ok(spend.clone())
    }

    /// Execute a fully-signed spend.
    pub fn execute_spend(&self, spend_id: &str, tx_id: &str) -> Result<TreasurySpend, String> {
        let mut spend = self.spends.get_mut(spend_id).ok_or("Spend not found")?;
        if spend.status != SpendStatus::Pending {
            return Err(format!("Spend is not pending: {:?}", spend.status));
        }
        if spend.signatures_collected < spend.signatures_required {
            return Err(format!(
                "Not enough signatures: {}/{}",
                spend.signatures_collected, spend.signatures_required
            ));
        }
        spend.status = SpendStatus::Completed;
        spend.executed_at = Some(Utc::now().to_rfc3339());
        spend.tx_id = Some(tx_id.to_string());
        let amount = spend.amount_nanoerg;
        let updated = spend.clone();
        drop(spend);

        self.locked_funds.fetch_sub(amount, Ordering::Relaxed);
        self.total_spent.fetch_add(amount, Ordering::Relaxed);
        self.spends.insert(spend_id.to_string(), updated.clone());
        self.record_event(EventType::SpendCompleted, Some(spend_id.to_string()), "system", amount, tx_id);
        Ok(updated)
    }

    /// Fail a spend and unlock funds.
    pub fn fail_spend(&self, spend_id: &str, reason: &str) -> Result<TreasurySpend, String> {
        let mut spend = self.spends.get_mut(spend_id).ok_or("Spend not found")?;
        if spend.status != SpendStatus::Pending {
            return Err(format!("Spend is not pending: {:?}", spend.status));
        }
        spend.status = SpendStatus::Failed;
        let amount = spend.amount_nanoerg;
        let updated = spend.clone();
        drop(spend);

        self.locked_funds.fetch_sub(amount, Ordering::Relaxed);
        self.spends.insert(spend_id.to_string(), updated.clone());
        self.record_event(EventType::SpendFailed, Some(spend_id.to_string()), "system", amount, reason);
        Ok(updated)
    }

    /// Get treasury state snapshot.
    pub fn get_state(&self) -> TreasuryState {
        let deposited = self.total_deposited.load(Ordering::Relaxed);
        let spent = self.total_spent.load(Ordering::Relaxed);
        let locked = self.locked_funds.load(Ordering::Relaxed);
        let available = deposited.saturating_sub(spent).saturating_sub(locked);

        let mut pending = 0u64;
        let mut completed = 0u64;
        let mut failed = 0u64;
        for entry in self.spends.iter() {
            match entry.value().status {
                SpendStatus::Pending => pending += 1,
                SpendStatus::Completed => completed += 1,
                SpendStatus::Failed => failed += 1,
                SpendStatus::Refunded => {}
            }
        }

        TreasuryState {
            total_deposits_nanoerg: deposited,
            total_spent_nanoerg: spent,
            available_balance: available,
            locked_balance: locked,
            pending_spends: pending,
            completed_spends: completed,
            failed_spends: failed,
        }
    }

    /// Get spend history.
    pub fn get_spends(&self, limit: usize) -> Vec<TreasurySpend> {
        let mut all: Vec<TreasurySpend> = self.spends.iter().map(|r| r.value().clone()).collect();
        all.sort_by(|a, b| b.locked_at.cmp(&a.locked_at));
        all.truncate(limit);
        all
    }

    /// Update threshold config.
    pub fn update_threshold(&self, required: u32, signatories: Vec<String>) -> Result<ThresholdConfig, String> {
        let old = self.threshold.read().unwrap().clone();
        validate_rotation(&old, required, &signatories)?;
        let mut config = self.threshold.write().unwrap();
        config.required_signatures = required;
        config.total_signatories = signatories.len() as u32;
        config.signatory_addresses = signatories;
        let updated = config.clone();
        drop(config);
        self.record_event(EventType::SignatoryRotated, None, "system", 0, "threshold updated");
        Ok(updated)
    }

    fn record_event(&self, event_type: EventType, spend_id: Option<String>, actor: &str, amount: u64, details: &str) {
        self.event_total.fetch_add(1, Ordering::Relaxed);
        let event = TreasuryEvent {
            event_type,
            spend_id,
            actor: actor.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            amount_nanoerg: amount,
            details: details.to_string(),
        };
        let mut log = self.events.entry("treasury".to_string()).or_insert_with(VecDeque::new);
        if log.len() >= AUDIT_MAX {
            log.pop_front();
        }
        log.push_back(event);
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_threshold_script() {
        let script = create_threshold_script(3, &["9a".into(), "9b".into(), "9c".into(), "9d".into(), "9e".into()]);
        assert!(script.contains("atLeast(3"));
        assert!(script.contains("PK(\"9a\")"));
        assert!(script.contains("PK(\"9e\")"));
    }

    #[test]
    fn test_verify_threshold() {
        assert!(verify_threshold(3, 3, 5));
        assert!(!verify_threshold(2, 3, 5));
        assert!(verify_threshold(5, 3, 5)); // can have more
        assert!(!verify_threshold(6, 3, 5)); // can't exceed total
        assert!(verify_threshold(0, 0, 5)); // 0 >= 0
    }

    #[test]
    fn test_calculate_proportional_spend() {
        let (awarded, remaining) = calculate_proportional_spend(1_000_000_000, 5_000_000);
        assert_eq!(awarded, 500_000_000);
        assert_eq!(remaining, 500_000_000);
    }

    #[test]
    fn test_calculate_proportional_full() {
        let (awarded, remaining) = calculate_proportional_spend(1_000_000_000, PROPORTION_BASE);
        assert_eq!(awarded, 1_000_000_000);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_calculate_proportional_zero() {
        let (awarded, remaining) = calculate_proportional_spend(1_000_000_000, 0);
        assert_eq!(awarded, 0);
        assert_eq!(remaining, 1_000_000_000);
    }

    #[test]
    fn test_validate_rotation_success() {
        let old = ThresholdConfig {
            required_signatures: 3,
            total_signatories: 5,
            signatory_addresses: vec!["9a".into(), "9b".into(), "9c".into(), "9d".into(), "9e".into()],
        };
        let new_signatories = vec!["9a".into(), "9b".into(), "9c".into(), "9f".into(), "9g".into()];
        assert!(validate_rotation(&old, 3, &new_signatories).is_ok());
    }

    #[test]
    fn test_validate_rotation_empty() {
        let old = ThresholdConfig::default();
        assert!(validate_rotation(&old, 3, &[]).is_err());
    }

    #[test]
    fn test_validate_rotation_zero_required() {
        let old = ThresholdConfig::default();
        assert!(validate_rotation(&old, 0, &["9a".into()]).is_err());
    }

    #[test]
    fn test_validate_rotation_exceeds_total() {
        let old = ThresholdConfig::default();
        assert!(validate_rotation(&old, 5, &["9a".into(), "9b".into(), "9c".into()]).is_err());
    }

    #[test]
    fn test_validate_rotation_insufficient_overlap() {
        let old = ThresholdConfig {
            required_signatures: 3,
            total_signatories: 5,
            signatory_addresses: vec!["9a".into(), "9b".into(), "9c".into(), "9d".into(), "9e".into()],
        };
        // Only 1 overlap, need at least 2
        let new_signatories = vec!["9a".into(), "9x".into(), "9y".into(), "9z".into(), "9w".into()];
        assert!(validate_rotation(&old, 3, &new_signatories).is_err());
    }

    #[test]
    fn test_deposit() {
        let state = TreasuryAppState::new();
        let record = state.deposit("9alice", 1_000_000_000, "tx1");
        assert_eq!(record.amount_nanoerg, 1_000_000_000);
        assert_eq!(state.total_deposited.load(Ordering::Relaxed), 1_000_000_000);
    }

    #[test]
    fn test_request_spend() {
        let state = TreasuryAppState::new();
        state.deposit("9alice", 2_000_000_000, "tx1");
        let spend = state.request_spend("p1", "9bob", 500_000_000).unwrap();
        assert_eq!(spend.amount_nanoerg, 500_000_000);
        assert_eq!(spend.status, SpendStatus::Pending);
        assert_eq!(state.locked_funds.load(Ordering::Relaxed), 500_000_000);
    }

    #[test]
    fn test_request_spend_insufficient() {
        let state = TreasuryAppState::new();
        state.deposit("9alice", 100_000_000, "tx1");
        assert!(state.request_spend("p1", "9bob", 200_000_000).is_err());
    }

    #[test]
    fn test_execute_spend() {
        let state = TreasuryAppState::new();
        state.deposit("9alice", 2_000_000_000, "tx1");
        let spend = state.request_spend("p1", "9bob", 500_000_000).unwrap();
        // Add enough signatures
        for i in 0..3 {
            state.add_signature(&spend.id, &format!("9signatory{}", i + 1)).unwrap();
        }
        let executed = state.execute_spend(&spend.id, "tx2").unwrap();
        assert_eq!(executed.status, SpendStatus::Completed);
        assert_eq!(state.total_spent.load(Ordering::Relaxed), 500_000_000);
        assert_eq!(state.locked_funds.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_execute_spend_not_enough_sigs() {
        let state = TreasuryAppState::new();
        state.deposit("9alice", 2_000_000_000, "tx1");
        let spend = state.request_spend("p1", "9bob", 500_000_000).unwrap();
        state.add_signature(&spend.id, "9signatory1").unwrap();
        assert!(state.execute_spend(&spend.id, "tx2").is_err());
    }

    #[test]
    fn test_fail_spend_unlocks_funds() {
        let state = TreasuryAppState::new();
        state.deposit("9alice", 2_000_000_000, "tx1");
        let spend = state.request_spend("p1", "9bob", 500_000_000).unwrap();
        let failed = state.fail_spend(&spend.id, "proposal failed").unwrap();
        assert_eq!(failed.status, SpendStatus::Failed);
        assert_eq!(state.locked_funds.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_get_state() {
        let state = TreasuryAppState::new();
        state.deposit("9alice", 3_000_000_000, "tx1");
        state.deposit("9bob", 2_000_000_000, "tx2");
        let spend = state.request_spend("p1", "9charlie", 1_000_000_000).unwrap();
        let ts = state.get_state();
        assert_eq!(ts.total_deposits_nanoerg, 5_000_000_000);
        assert_eq!(ts.available_balance, 4_000_000_000);
        assert_eq!(ts.locked_balance, 1_000_000_000);
        assert_eq!(ts.pending_spends, 1);
    }

    #[test]
    fn test_get_spends() {
        let state = TreasuryAppState::new();
        state.deposit("9alice", 5_000_000_000, "tx1");
        state.request_spend("p1", "9bob", 100_000_000).unwrap();
        state.request_spend("p2", "9carol", 200_000_000).unwrap();
        let spends = state.get_spends(10);
        assert_eq!(spends.len(), 2);
    }

    #[test]
    fn test_update_threshold() {
        let state = TreasuryAppState::new();
        let new_signatories = vec!["9signatory1".into(), "9signatory2".into(), "9signatory3".into(), "9signatory4".into(), "9signatory5".into()];
        let updated = state.update_threshold(3, new_signatories).unwrap();
        assert_eq!(updated.required_signatures, 3);
        assert_eq!(updated.total_signatories, 5);
    }

    #[test]
    fn test_event_ring_buffer() {
        let state = TreasuryAppState::new();
        for i in 0..(AUDIT_MAX + 100) {
            state.deposit(&format!("9user{}", i), 100, &format!("tx{}", i));
        }
        let log = state.events.get("treasury").unwrap();
        assert_eq!(log.len(), AUDIT_MAX);
    }

    #[test]
    fn test_treasury_state_serde() {
        let state = TreasuryState {
            total_deposits_nanoerg: 1000,
            total_spent_nanoerg: 200,
            available_balance: 600,
            locked_balance: 200,
            pending_spends: 1,
            completed_spends: 2,
            failed_spends: 0,
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: TreasuryState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.available_balance, parsed.available_balance);
    }

    #[test]
    fn test_spend_status_serde() {
        let statuses = vec![SpendStatus::Pending, SpendStatus::Completed, SpendStatus::Failed, SpendStatus::Refunded];
        for s in statuses {
            let json = serde_json::to_string(&s).unwrap();
            let parsed: SpendStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, parsed);
        }
    }
}
