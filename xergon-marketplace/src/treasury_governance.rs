use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use chrono::Utc;

// ===========================================================================
// ProposalCategory
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProposalCategory {
    ProtocolParam,
    ProviderAction,
    TreasurySpend,
    ContractUpgrade,
    Emergency,
    ConfigChange,
}

impl ProposalCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ProtocolParam => "ProtocolParam",
            Self::ProviderAction => "ProviderAction",
            Self::TreasurySpend => "TreasurySpend",
            Self::ContractUpgrade => "ContractUpgrade",
            Self::Emergency => "Emergency",
            Self::ConfigChange => "ConfigChange",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ProtocolParam" => Some(Self::ProtocolParam),
            "ProviderAction" => Some(Self::ProviderAction),
            "TreasurySpend" => Some(Self::TreasurySpend),
            "ContractUpgrade" => Some(Self::ContractUpgrade),
            "Emergency" => Some(Self::Emergency),
            "ConfigChange" => Some(Self::ConfigChange),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProposalCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ===========================================================================
// ProposalStage
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProposalStage {
    Created,
    Voting,
    Executed,
    Closed,
    Expired,
}

impl ProposalStage {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Created => "Created",
            Self::Voting => "Voting",
            Self::Executed => "Executed",
            Self::Closed => "Closed",
            Self::Expired => "Expired",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Created" => Some(Self::Created),
            "Voting" => Some(Self::Voting),
            "Executed" => Some(Self::Executed),
            "Closed" => Some(Self::Closed),
            "Expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// Returns true if the proposal is still active (Created or Voting).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Created | Self::Voting)
    }
}

impl std::fmt::Display for ProposalStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ===========================================================================
// GovernanceProposal
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GovernanceProposal {
    pub proposal_id: String,
    pub title: String,
    pub category: ProposalCategory,
    pub stage: ProposalStage,
    pub votes_for: u64,
    pub votes_against: u64,
    pub quorum_threshold: u64,
    pub approval_threshold: f64,
    pub created_height: u64,
    pub vote_end_height: u64,
    pub creator_pk: String,
}

impl GovernanceProposal {
    /// Create a new governance proposal with sensible defaults.
    pub fn new(
        proposal_id: &str,
        title: &str,
        category: ProposalCategory,
        creator_pk: &str,
        created_height: u64,
        vote_end_height: u64,
    ) -> Self {
        Self {
            proposal_id: proposal_id.to_string(),
            title: title.to_string(),
            category,
            stage: ProposalStage::Created,
            votes_for: 0,
            votes_against: 0,
            quorum_threshold: 100,
            approval_threshold: 0.67,
            created_height,
            vote_end_height,
            creator_pk: creator_pk.to_string(),
        }
    }

    /// Total number of votes cast.
    pub fn total_votes(&self) -> u64 {
        self.votes_for.saturating_add(self.votes_against)
    }

    /// Approval percentage (0.0 - 1.0). Returns 0.0 if no votes cast.
    pub fn approval_pct(&self) -> f64 {
        let total = self.total_votes();
        if total == 0 {
            return 0.0;
        }
        self.votes_for as f64 / total as f64
    }

    /// Quorum percentage (0.0 - 1.0). Compares total votes to quorum threshold.
    pub fn quorum_pct(&self) -> f64 {
        if self.quorum_threshold == 0 {
            return 1.0;
        }
        (self.total_votes() as f64 / self.quorum_threshold as f64).min(1.0)
    }
}

// ===========================================================================
// OperationStatus
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum OperationStatus {
    Pending,
    Completed,
    Failed,
    Refunded,
}

impl OperationStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Refunded => "Refunded",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Pending" => Some(Self::Pending),
            "Completed" => Some(Self::Completed),
            "Failed" => Some(Self::Failed),
            "Refunded" => Some(Self::Refunded),
            _ => None,
        }
    }
}

impl std::fmt::Display for OperationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ===========================================================================
// TreasuryOperation
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TreasuryOperation {
    pub id: String,
    pub proposal_id: String,
    pub recipient: String,
    pub amount_nanoerg: u64,
    pub status: OperationStatus,
    pub signatures_collected: u32,
    pub signatures_required: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TreasuryOperation {
    /// Create a new pending treasury operation.
    pub fn new(
        id: &str,
        proposal_id: &str,
        recipient: &str,
        amount_nanoerg: u64,
        signatures_required: u32,
    ) -> Self {
        Self {
            id: id.to_string(),
            proposal_id: proposal_id.to_string(),
            recipient: recipient.to_string(),
            amount_nanoerg,
            status: OperationStatus::Pending,
            signatures_collected: 0,
            signatures_required,
            created_at: Utc::now(),
            executed_at: None,
        }
    }

    /// Whether enough signatures have been collected.
    pub fn has_quorum_signatures(&self) -> bool {
        self.signatures_collected >= self.signatures_required
    }
}

// ===========================================================================
// QuorumStatus
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuorumStatus {
    pub quorum_met: bool,
    pub approval_met: bool,
    pub passes: bool,
    pub approval_pct: f64,
    pub quorum_pct: f64,
}

// ===========================================================================
// TreasurySnapshot
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TreasurySnapshot {
    pub total_deposits_nanoerg: u64,
    pub total_spent_nanoerg: u64,
    pub available_balance: u64,
    pub locked_balance: u64,
    pub pending_spends: u64,
    pub completed_spends: u64,
    pub failed_spends: u64,
    pub total_proposals: u64,
    pub active_proposals: u64,
}

// ===========================================================================
// TreasuryGovernanceManager
// ===========================================================================

#[derive(Debug)]
pub struct TreasuryGovernanceManager {
    proposals: DashMap<String, GovernanceProposal>,
    treasury_ops: DashMap<String, TreasuryOperation>,
    total_proposals_seen: AtomicU64,
    total_spends_seen: AtomicU64,
    agent_api_url: String,
}

impl TreasuryGovernanceManager {
    /// Create a new TreasuryGovernanceManager pointing at the given agent API.
    pub fn new(agent_api_url: &str) -> Self {
        Self {
            proposals: DashMap::new(),
            treasury_ops: DashMap::new(),
            total_proposals_seen: AtomicU64::new(0),
            total_spends_seen: AtomicU64::new(0),
            agent_api_url: agent_api_url.to_string(),
        }
    }

    /// Cache (insert or update) a governance proposal.
    pub fn cache_proposal(&self, proposal: GovernanceProposal) {
        let is_new = !self.proposals.contains_key(&proposal.proposal_id);
        self.proposals
            .insert(proposal.proposal_id.clone(), proposal);
        if is_new {
            self.total_proposals_seen.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Retrieve a cached proposal by ID.
    pub fn get_proposal(&self, id: &str) -> Option<GovernanceProposal> {
        self.proposals.get(id).map(|p| p.clone())
    }

    /// List proposals, optionally filtered by stage, up to the given limit.
    pub fn list_proposals(
        &self,
        stage_filter: Option<&str>,
        limit: usize,
    ) -> Vec<GovernanceProposal> {
        let stage = stage_filter.and_then(ProposalStage::from_str);
        self.proposals
            .iter()
            .filter(|entry| {
                if let Some(ref s) = stage {
                    entry.value().stage == *s
                } else {
                    true
                }
            })
            .map(|entry| entry.value().clone())
            .take(limit)
            .collect()
    }

    /// Cache (insert or update) a treasury operation.
    pub fn cache_treasury_op(&self, op: TreasuryOperation) {
        let is_new = !self.treasury_ops.contains_key(&op.id);
        self.treasury_ops.insert(op.id.clone(), op);
        if is_new {
            self.total_spends_seen.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Retrieve treasury ops, optionally filtered by status, up to the given limit.
    pub fn get_treasury_ops(
        &self,
        status_filter: Option<&str>,
        limit: usize,
    ) -> Vec<TreasuryOperation> {
        let status = status_filter.and_then(OperationStatus::from_str);
        self.treasury_ops
            .iter()
            .filter(|entry| {
                if let Some(ref s) = status {
                    entry.value().status == *s
                } else {
                    true
                }
            })
            .map(|entry| entry.value().clone())
            .take(limit)
            .collect()
    }

    /// Compute a snapshot of the treasury state from cached data.
    pub fn get_snapshot(&self) -> TreasurySnapshot {
        let total_deposits_nanoerg: u64 = 0;
        let mut total_spent_nanoerg: u64 = 0;
        let mut locked_balance: u64 = 0;
        let mut pending_spends: u64 = 0;
        let mut completed_spends: u64 = 0;
        let mut failed_spends: u64 = 0;
        let mut active_proposals: u64 = 0;

        for entry in self.proposals.iter() {
            if entry.value().stage.is_active() {
                active_proposals += 1;
            }
        }

        for entry in self.treasury_ops.iter() {
            let op = entry.value();
            match op.status {
                OperationStatus::Pending => {
                    pending_spends += 1;
                    locked_balance = locked_balance.saturating_add(op.amount_nanoerg);
                }
                OperationStatus::Completed => {
                    completed_spends += 1;
                    total_spent_nanoerg = total_spent_nanoerg.saturating_add(op.amount_nanoerg);
                }
                OperationStatus::Failed => {
                    failed_spends += 1;
                }
                OperationStatus::Refunded => {
                    // Refunded ops don't count as spent
                }
            }
        }

        // total_deposits is the sum of spent + locked + available
        // We derive available from deposits - spent - locked
        let available_balance = total_deposits_nanoerg
            .saturating_sub(total_spent_nanoerg)
            .saturating_sub(locked_balance);

        TreasurySnapshot {
            total_deposits_nanoerg,
            total_spent_nanoerg,
            available_balance,
            locked_balance,
            pending_spends,
            completed_spends,
            failed_spends,
            total_proposals: self.total_proposals_seen.load(Ordering::Relaxed),
            active_proposals,
        }
    }

    /// Count proposals that are in Created or Voting stage.
    pub fn active_proposal_count(&self) -> usize {
        self.proposals
            .iter()
            .filter(|entry| entry.value().stage.is_active())
            .count()
    }

    /// Compute the quorum/approval status of a proposal.
    pub fn compute_quorum_status(&self, proposal: &GovernanceProposal) -> QuorumStatus {
        let quorum_pct = proposal.quorum_pct();
        let approval_pct = proposal.approval_pct();

        let quorum_met = quorum_pct >= 1.0;
        let approval_met = approval_pct >= proposal.approval_threshold;
        let passes = quorum_met && approval_met;

        QuorumStatus {
            quorum_met,
            approval_met,
            passes,
            approval_pct,
            quorum_pct,
        }
    }

    /// Convert nanoERG to ERG (floating point).
    pub fn nanoerg_to_erg(nanoerg: u64) -> f64 {
        nanoerg as f64 / 1_000_000_000.0
    }

    /// Format a nanoERG amount as a human-readable ERG string (9 decimal places).
    pub fn format_erg(nanoerg: u64) -> String {
        format!("{:.9} ERG", Self::nanoerg_to_erg(nanoerg))
    }

    /// Return a summary map of proposal stages and their counts.
    pub fn proposal_summary(&self) -> HashMap<String, u64> {
        let mut summary = HashMap::new();
        for entry in self.proposals.iter() {
            let stage = entry.value().stage.as_str().to_string();
            *summary.entry(stage).or_insert(0) += 1;
        }
        summary
    }

    /// The agent API URL this manager was configured with.
    pub fn agent_api_url(&self) -> &str {
        &self.agent_api_url
    }

    /// Total number of unique proposals ever cached.
    pub fn total_proposals_seen(&self) -> u64 {
        self.total_proposals_seen.load(Ordering::Relaxed)
    }

    /// Total number of unique treasury operations ever cached.
    pub fn total_spends_seen(&self) -> u64 {
        self.total_spends_seen.load(Ordering::Relaxed)
    }

    /// Number of proposals currently cached.
    pub fn cached_proposal_count(&self) -> usize {
        self.proposals.len()
    }

    /// Number of treasury operations currently cached.
    pub fn cached_op_count(&self) -> usize {
        self.treasury_ops.len()
    }

    /// Remove a proposal from the cache by ID. Returns true if it existed.
    pub fn remove_proposal(&self, id: &str) -> bool {
        self.proposals.remove(id).is_some()
    }

    /// Remove a treasury operation from the cache by ID. Returns true if it existed.
    pub fn remove_treasury_op(&self, id: &str) -> bool {
        self.treasury_ops.remove(id).is_some()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // -- helpers ------------------------------------------------------------

    fn make_manager() -> Arc<TreasuryGovernanceManager> {
        Arc::new(TreasuryGovernanceManager::new("http://localhost:9010"))
    }

    fn make_proposal(id: &str, stage: ProposalStage, votes_for: u64, votes_against: u64) -> GovernanceProposal {
        GovernanceProposal {
            proposal_id: id.to_string(),
            title: format!("Proposal {}", id),
            category: ProposalCategory::TreasurySpend,
            stage,
            votes_for,
            votes_against,
            quorum_threshold: 100,
            approval_threshold: 0.67,
            created_height: 1000,
            vote_end_height: 2000,
            creator_pk: "0xDeadBeef".to_string(),
        }
    }

    fn make_treasury_op(id: &str, status: OperationStatus, amount: u64) -> TreasuryOperation {
        let sigs = match &status {
            OperationStatus::Pending => 1,
            OperationStatus::Completed => 3,
            OperationStatus::Failed => 2,
            OperationStatus::Refunded => 3,
        };
        let executed_at = match &status {
            OperationStatus::Completed | OperationStatus::Failed | OperationStatus::Refunded => Some(Utc::now()),
            OperationStatus::Pending => None,
        };
        TreasuryOperation {
            id: id.to_string(),
            proposal_id: "prop-1".to_string(),
            recipient: "9fB6zCFUzMZ7kPcBHcY6w7hjVjZzGjUcVb7cDKYFB7R9mCp3NsR".to_string(),
            amount_nanoerg: amount,
            status,
            signatures_collected: sigs,
            signatures_required: 3,
            created_at: Utc::now(),
            executed_at,
        }
    }

    // -- construction -------------------------------------------------------

    #[test]
    fn test_new_manager() {
        let mgr = TreasuryGovernanceManager::new("http://agent:9010");
        assert_eq!(mgr.agent_api_url(), "http://agent:9010");
        assert_eq!(mgr.cached_proposal_count(), 0);
        assert_eq!(mgr.cached_op_count(), 0);
        assert_eq!(mgr.total_proposals_seen(), 0);
        assert_eq!(mgr.total_spends_seen(), 0);
        assert_eq!(mgr.active_proposal_count(), 0);
    }

    // -- proposal CRUD ------------------------------------------------------

    #[test]
    fn test_cache_and_get_proposal() {
        let mgr = make_manager();
        let p = make_proposal("prop-1", ProposalStage::Created, 0, 0);
        mgr.cache_proposal(p.clone());

        let retrieved = mgr.get_proposal("prop-1").unwrap();
        assert_eq!(retrieved.proposal_id, "prop-1");
        assert_eq!(retrieved.title, "Proposal prop-1");
        assert_eq!(retrieved.stage, ProposalStage::Created);
    }

    #[test]
    fn test_cache_proposal_updates_existing() {
        let mgr = make_manager();

        let p1 = make_proposal("prop-1", ProposalStage::Created, 0, 0);
        mgr.cache_proposal(p1);
        assert_eq!(mgr.total_proposals_seen(), 1);
        assert_eq!(mgr.cached_proposal_count(), 1);

        // Updating should not increment the seen counter
        let p2 = make_proposal("prop-1", ProposalStage::Voting, 50, 10);
        mgr.cache_proposal(p2);
        assert_eq!(mgr.total_proposals_seen(), 1);
        assert_eq!(mgr.cached_proposal_count(), 1);

        let retrieved = mgr.get_proposal("prop-1").unwrap();
        assert_eq!(retrieved.stage, ProposalStage::Voting);
        assert_eq!(retrieved.votes_for, 50);
    }

    #[test]
    fn test_get_proposal_not_found() {
        let mgr = make_manager();
        assert!(mgr.get_proposal("nonexistent").is_none());
    }

    #[test]
    fn test_list_proposals_no_filter() {
        let mgr = make_manager();
        mgr.cache_proposal(make_proposal("a", ProposalStage::Created, 0, 0));
        mgr.cache_proposal(make_proposal("b", ProposalStage::Voting, 10, 5));
        mgr.cache_proposal(make_proposal("c", ProposalStage::Executed, 80, 20));

        let all = mgr.list_proposals(None, 100);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_list_proposals_with_stage_filter() {
        let mgr = make_manager();
        mgr.cache_proposal(make_proposal("a", ProposalStage::Created, 0, 0));
        mgr.cache_proposal(make_proposal("b", ProposalStage::Voting, 10, 5));
        mgr.cache_proposal(make_proposal("c", ProposalStage::Voting, 30, 10));
        mgr.cache_proposal(make_proposal("d", ProposalStage::Executed, 80, 20));

        let voting = mgr.list_proposals(Some("Voting"), 100);
        assert_eq!(voting.len(), 2);

        let created = mgr.list_proposals(Some("Created"), 100);
        assert_eq!(created.len(), 1);
    }

    #[test]
    fn test_list_proposals_limit() {
        let mgr = make_manager();
        mgr.cache_proposal(make_proposal("a", ProposalStage::Created, 0, 0));
        mgr.cache_proposal(make_proposal("b", ProposalStage::Created, 0, 0));
        mgr.cache_proposal(make_proposal("c", ProposalStage::Created, 0, 0));

        let limited = mgr.list_proposals(None, 2);
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn test_remove_proposal() {
        let mgr = make_manager();
        mgr.cache_proposal(make_proposal("x", ProposalStage::Created, 0, 0));
        assert_eq!(mgr.cached_proposal_count(), 1);

        assert!(mgr.remove_proposal("x"));
        assert_eq!(mgr.cached_proposal_count(), 0);
        assert!(!mgr.remove_proposal("x")); // already removed
    }

    // -- treasury ops -------------------------------------------------------

    #[test]
    fn test_cache_and_list_treasury_ops() {
        let mgr = make_manager();
        mgr.cache_treasury_op(make_treasury_op("op-1", OperationStatus::Pending, 500_000_000));
        mgr.cache_treasury_op(make_treasury_op("op-2", OperationStatus::Completed, 1_000_000_000));
        mgr.cache_treasury_op(make_treasury_op("op-3", OperationStatus::Failed, 200_000_000));

        assert_eq!(mgr.total_spends_seen(), 3);

        let all = mgr.get_treasury_ops(None, 100);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_list_treasury_ops_with_filter() {
        let mgr = make_manager();
        mgr.cache_treasury_op(make_treasury_op("op-1", OperationStatus::Pending, 100));
        mgr.cache_treasury_op(make_treasury_op("op-2", OperationStatus::Completed, 200));
        mgr.cache_treasury_op(make_treasury_op("op-3", OperationStatus::Completed, 300));

        let completed = mgr.get_treasury_ops(Some("Completed"), 100);
        assert_eq!(completed.len(), 2);

        let pending = mgr.get_treasury_ops(Some("Pending"), 100);
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_cache_treasury_op_update_does_not_increment_counter() {
        let mgr = make_manager();
        let op = make_treasury_op("op-1", OperationStatus::Pending, 100);
        mgr.cache_treasury_op(op);
        assert_eq!(mgr.total_spends_seen(), 1);

        let op2 = make_treasury_op("op-1", OperationStatus::Completed, 100);
        mgr.cache_treasury_op(op2);
        assert_eq!(mgr.total_spends_seen(), 1);
    }

    #[test]
    fn test_remove_treasury_op() {
        let mgr = make_manager();
        mgr.cache_treasury_op(make_treasury_op("op-1", OperationStatus::Pending, 100));
        assert!(mgr.remove_treasury_op("op-1"));
        assert!(!mgr.remove_treasury_op("op-1"));
    }

    // -- snapshot -----------------------------------------------------------

    #[test]
    fn test_snapshot_empty() {
        let mgr = make_manager();
        let snap = mgr.get_snapshot();
        assert_eq!(snap.total_deposits_nanoerg, 0);
        assert_eq!(snap.total_spent_nanoerg, 0);
        assert_eq!(snap.available_balance, 0);
        assert_eq!(snap.locked_balance, 0);
        assert_eq!(snap.pending_spends, 0);
        assert_eq!(snap.completed_spends, 0);
        assert_eq!(snap.failed_spends, 0);
        assert_eq!(snap.total_proposals, 0);
        assert_eq!(snap.active_proposals, 0);
    }

    #[test]
    fn test_snapshot_with_data() {
        let mgr = make_manager();
        mgr.cache_proposal(make_proposal("p1", ProposalStage::Voting, 50, 10));
        mgr.cache_proposal(make_proposal("p2", ProposalStage::Executed, 100, 0));

        mgr.cache_treasury_op(make_treasury_op("op-1", OperationStatus::Pending, 500_000_000));
        mgr.cache_treasury_op(make_treasury_op("op-2", OperationStatus::Completed, 1_000_000_000));
        mgr.cache_treasury_op(make_treasury_op("op-3", OperationStatus::Failed, 200_000_000));
        mgr.cache_treasury_op(make_treasury_op("op-4", OperationStatus::Refunded, 300_000_000));

        let snap = mgr.get_snapshot();
        assert_eq!(snap.active_proposals, 1); // p1 is Voting
        assert_eq!(snap.pending_spends, 1);
        assert_eq!(snap.completed_spends, 1);
        assert_eq!(snap.failed_spends, 1);
        assert_eq!(snap.total_spent_nanoerg, 1_000_000_000);
        assert_eq!(snap.locked_balance, 500_000_000);
        assert_eq!(snap.total_proposals, 2);
    }

    // -- active proposal count ----------------------------------------------

    #[test]
    fn test_active_proposal_count() {
        let mgr = make_manager();
        assert_eq!(mgr.active_proposal_count(), 0);

        mgr.cache_proposal(make_proposal("p1", ProposalStage::Created, 0, 0));
        assert_eq!(mgr.active_proposal_count(), 1);

        mgr.cache_proposal(make_proposal("p2", ProposalStage::Voting, 10, 5));
        assert_eq!(mgr.active_proposal_count(), 2);

        mgr.cache_proposal(make_proposal("p3", ProposalStage::Executed, 100, 0));
        assert_eq!(mgr.active_proposal_count(), 2);

        mgr.cache_proposal(make_proposal("p4", ProposalStage::Closed, 50, 50));
        assert_eq!(mgr.active_proposal_count(), 2);

        mgr.cache_proposal(make_proposal("p5", ProposalStage::Expired, 5, 5));
        assert_eq!(mgr.active_proposal_count(), 2);
    }

    // -- quorum status ------------------------------------------------------

    #[test]
    fn test_quorum_status_no_votes() {
        let mgr = make_manager();
        let p = make_proposal("p1", ProposalStage::Voting, 0, 0);
        let qs = mgr.compute_quorum_status(&p);
        assert!(!qs.quorum_met);
        assert!(!qs.approval_met);
        assert!(!qs.passes);
        assert!((qs.approval_pct - 0.0).abs() < f64::EPSILON);
        assert!((qs.quorum_pct - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quorum_status_meets_quorum_but_not_approval() {
        let mgr = make_manager();
        // quorum_threshold=100, votes=100 but only 60% for
        let p = make_proposal("p1", ProposalStage::Voting, 60, 40);
        let qs = mgr.compute_quorum_status(&p);
        assert!(qs.quorum_met);
        assert!(!qs.approval_met);
        assert!(!qs.passes);
        assert!((qs.approval_pct - 0.6).abs() < 1e-10);
    }

    #[test]
    fn test_quorum_status_passes() {
        let mgr = make_manager();
        // quorum_threshold=100, votes=100, 80% for, threshold=0.67
        let p = make_proposal("p1", ProposalStage::Voting, 80, 20);
        let qs = mgr.compute_quorum_status(&p);
        assert!(qs.quorum_met);
        assert!(qs.approval_met);
        assert!(qs.passes);
        assert!((qs.approval_pct - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_quorum_status_exceeds_quorum() {
        let mgr = make_manager();
        // More votes than threshold, high approval
        let p = make_proposal("p1", ProposalStage::Voting, 200, 10);
        let qs = mgr.compute_quorum_status(&p);
        // quorum_pct capped at 1.0
        assert!((qs.quorum_pct - 1.0).abs() < f64::EPSILON);
        assert!(qs.quorum_met);
        assert!(qs.passes);
    }

    #[test]
    fn test_quorum_status_approval_but_no_quorum() {
        let mgr = make_manager();
        // Only 30 votes (out of 100 needed) but 100% approval
        let p = make_proposal("p1", ProposalStage::Voting, 30, 0);
        let qs = mgr.compute_quorum_status(&p);
        assert!(!qs.quorum_met);
        assert!(qs.approval_met);
        assert!(!qs.passes); // must meet BOTH
    }

    // -- formatting ---------------------------------------------------------

    #[test]
    fn test_nanoerg_to_erg() {
        assert_eq!(TreasuryGovernanceManager::nanoerg_to_erg(0), 0.0);
        assert_eq!(TreasuryGovernanceManager::nanoerg_to_erg(1_000_000_000), 1.0);
        assert_eq!(TreasuryGovernanceManager::nanoerg_to_erg(123_456_789_000), 123.456789);
        assert_eq!(TreasuryGovernanceManager::nanoerg_to_erg(500_000_000), 0.5);
    }

    #[test]
    fn test_format_erg() {
        assert_eq!(
            TreasuryGovernanceManager::format_erg(1_000_000_000),
            "1.000000000 ERG"
        );
        assert_eq!(
            TreasuryGovernanceManager::format_erg(123_456_789_000),
            "123.456789000 ERG"
        );
        assert_eq!(
            TreasuryGovernanceManager::format_erg(0),
            "0.000000000 ERG"
        );
    }

    // -- proposal summary ---------------------------------------------------

    #[test]
    fn test_proposal_summary_empty() {
        let mgr = make_manager();
        let summary = mgr.proposal_summary();
        assert!(summary.is_empty());
    }

    #[test]
    fn test_proposal_summary_with_data() {
        let mgr = make_manager();
        mgr.cache_proposal(make_proposal("p1", ProposalStage::Created, 0, 0));
        mgr.cache_proposal(make_proposal("p2", ProposalStage::Created, 0, 0));
        mgr.cache_proposal(make_proposal("p3", ProposalStage::Voting, 10, 5));
        mgr.cache_proposal(make_proposal("p4", ProposalStage::Executed, 80, 20));

        let summary = mgr.proposal_summary();
        assert_eq!(summary.get("Created").copied(), Some(2));
        assert_eq!(summary.get("Voting").copied(), Some(1));
        assert_eq!(summary.get("Executed").copied(), Some(1));
        assert_eq!(summary.len(), 3);
    }

    // -- enum conversions ---------------------------------------------------

    #[test]
    fn test_proposal_stage_roundtrip() {
        for stage in &[
            ProposalStage::Created,
            ProposalStage::Voting,
            ProposalStage::Executed,
            ProposalStage::Closed,
            ProposalStage::Expired,
        ] {
            let s = stage.as_str();
            assert_eq!(ProposalStage::from_str(s), Some(stage.clone()));
        }
        assert_eq!(ProposalStage::from_str("Invalid"), None);
    }

    #[test]
    fn test_proposal_stage_is_active() {
        assert!(ProposalStage::Created.is_active());
        assert!(ProposalStage::Voting.is_active());
        assert!(!ProposalStage::Executed.is_active());
        assert!(!ProposalStage::Closed.is_active());
        assert!(!ProposalStage::Expired.is_active());
    }

    #[test]
    fn test_proposal_category_roundtrip() {
        for cat in &[
            ProposalCategory::ProtocolParam,
            ProposalCategory::ProviderAction,
            ProposalCategory::TreasurySpend,
            ProposalCategory::ContractUpgrade,
            ProposalCategory::Emergency,
            ProposalCategory::ConfigChange,
        ] {
            let s = cat.as_str();
            assert_eq!(ProposalCategory::from_str(s), Some(cat.clone()));
        }
        assert_eq!(ProposalCategory::from_str("Invalid"), None);
    }

    #[test]
    fn test_operation_status_roundtrip() {
        for status in &[
            OperationStatus::Pending,
            OperationStatus::Completed,
            OperationStatus::Failed,
            OperationStatus::Refunded,
        ] {
            let s = status.as_str();
            assert_eq!(OperationStatus::from_str(s), Some(status.clone()));
        }
        assert_eq!(OperationStatus::from_str("Unknown"), None);
    }

    // -- GovernanceProposal helpers -----------------------------------------

    #[test]
    fn test_governance_proposal_new() {
        let p = GovernanceProposal::new(
            "gp-1",
            "Increase fee",
            ProposalCategory::ProtocolParam,
            "0xABC",
            500,
            1500,
        );
        assert_eq!(p.proposal_id, "gp-1");
        assert_eq!(p.title, "Increase fee");
        assert_eq!(p.stage, ProposalStage::Created);
        assert_eq!(p.votes_for, 0);
        assert_eq!(p.votes_against, 0);
        assert_eq!(p.quorum_threshold, 100);
        assert!((p.approval_threshold - 0.67).abs() < f64::EPSILON);
    }

    #[test]
    fn test_governance_proposal_total_votes() {
        let p = make_proposal("p1", ProposalStage::Voting, 75, 25);
        assert_eq!(p.total_votes(), 100);
    }

    #[test]
    fn test_governance_proposal_approval_pct() {
        let p = make_proposal("p1", ProposalStage::Voting, 75, 25);
        assert!((p.approval_pct() - 0.75).abs() < 1e-10);

        let p_zero = make_proposal("p2", ProposalStage::Created, 0, 0);
        assert_eq!(p_zero.approval_pct(), 0.0);
    }

    // -- TreasuryOperation helpers ------------------------------------------

    #[test]
    fn test_treasury_operation_new() {
        let op = TreasuryOperation::new("op-1", "prop-1", "recipient", 500_000_000, 3);
        assert_eq!(op.id, "op-1");
        assert_eq!(op.status, OperationStatus::Pending);
        assert_eq!(op.signatures_collected, 0);
        assert_eq!(op.signatures_required, 3);
        assert!(op.executed_at.is_none());
    }

    #[test]
    fn test_treasury_operation_has_quorum_signatures() {
        let op = TreasuryOperation::new("op-1", "prop-1", "recip", 100, 3);
        assert!(!op.has_quorum_signatures());

        let mut op2 = op.clone();
        op2.signatures_collected = 3;
        assert!(op2.has_quorum_signatures());

        let mut op3 = op.clone();
        op3.signatures_collected = 5;
        assert!(op3.has_quorum_signatures());
    }

    // -- concurrent access --------------------------------------------------

    #[test]
    fn test_concurrent_proposal_access() {
        use std::thread;

        let mgr = Arc::new(TreasuryGovernanceManager::new("http://localhost:9010"));
        let mut handles = Vec::new();

        for i in 0..10 {
            let mgr_clone = Arc::clone(&mgr);
            handles.push(thread::spawn(move || {
                let id = format!("prop-{}", i);
                let p = make_proposal(&id, ProposalStage::Created, 0, 0);
                mgr_clone.cache_proposal(p);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(mgr.cached_proposal_count(), 10);
        assert_eq!(mgr.total_proposals_seen(), 10);
    }

    #[test]
    fn test_concurrent_treasury_ops() {
        use std::thread;

        let mgr = Arc::new(TreasuryGovernanceManager::new("http://localhost:9010"));
        let mut handles = Vec::new();

        for i in 0..10 {
            let mgr_clone = Arc::clone(&mgr);
            handles.push(thread::spawn(move || {
                let id = format!("op-{}", i);
                let op = make_treasury_op(&id, OperationStatus::Pending, 100 * (i as u64 + 1));
                mgr_clone.cache_treasury_op(op);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(mgr.cached_op_count(), 10);
        assert_eq!(mgr.total_spends_seen(), 10);
    }

    #[test]
    fn test_concurrent_read_write() {
        use std::thread;

        let mgr = Arc::new(TreasuryGovernanceManager::new("http://localhost:9010"));

        // Pre-populate
        for i in 0..5 {
            let p = make_proposal(&format!("prop-{}", i), ProposalStage::Voting, 10, 5);
            mgr.cache_proposal(p);
        }

        let mut handles = Vec::new();

        // Writer threads
        for i in 5..15 {
            let mgr_clone = Arc::clone(&mgr);
            handles.push(thread::spawn(move || {
                let p = make_proposal(&format!("prop-{}", i), ProposalStage::Created, 0, 0);
                mgr_clone.cache_proposal(p);
            }));
        }

        // Reader threads
        for _ in 0..5 {
            let mgr_clone = Arc::clone(&mgr);
            handles.push(thread::spawn(move || {
                let _ = mgr_clone.list_proposals(None, 100);
                let _ = mgr_clone.get_snapshot();
                let _ = mgr_clone.active_proposal_count();
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(mgr.cached_proposal_count(), 15);
        assert_eq!(mgr.total_proposals_seen(), 15);
    }

    // -- serialization roundtrip --------------------------------------------

    #[test]
    fn test_proposal_serialization_roundtrip() {
        let p = make_proposal("p1", ProposalStage::Voting, 75, 25);
        let json = serde_json::to_value(&p).unwrap();
        let deserialized: GovernanceProposal = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.proposal_id, p.proposal_id);
        assert_eq!(deserialized.stage, p.stage);
        assert_eq!(deserialized.votes_for, p.votes_for);
        assert_eq!(deserialized.votes_against, p.votes_against);
    }

    #[test]
    fn test_treasury_operation_serialization_roundtrip() {
        let op = make_treasury_op("op-1", OperationStatus::Completed, 500_000_000);
        let json = serde_json::to_value(&op).unwrap();
        let deserialized: TreasuryOperation = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.id, op.id);
        assert_eq!(deserialized.status, op.status);
        assert_eq!(deserialized.amount_nanoerg, op.amount_nanoerg);
    }

    #[test]
    fn test_snapshot_serialization() {
        let mgr = make_manager();
        mgr.cache_proposal(make_proposal("p1", ProposalStage::Voting, 10, 5));
        let snap = mgr.get_snapshot();
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["total_proposals"], 1);
        assert_eq!(json["active_proposals"], 1);
    }

    #[test]
    fn test_quorum_status_serialization() {
        let qs = QuorumStatus {
            quorum_met: true,
            approval_met: true,
            passes: true,
            approval_pct: 0.8,
            quorum_pct: 1.0,
        };
        let json = serde_json::to_value(&qs).unwrap();
        assert_eq!(json["passes"], true);
        assert!((json["approval_pct"].as_f64().unwrap() - 0.8).abs() < f64::EPSILON);
    }

    // -- display traits -----------------------------------------------------

    #[test]
    fn test_display_traits() {
        assert_eq!(format!("{}", ProposalStage::Voting), "Voting");
        assert_eq!(format!("{}", ProposalCategory::TreasurySpend), "TreasurySpend");
        assert_eq!(format!("{}", OperationStatus::Pending), "Pending");
    }
}
