//! Governance Executor for the Xergon Network agent.
//!
//! Bridges the on-chain governance transaction builder (OnChainGovernance)
//! with actual execution logic, vote delegation, and auto-execution.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::governance::{
    GovernanceError, OnChainGovernance, OnChainProposal,
    ProposalCategory, ProposalStage, ProposalStore, TallyResult,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    pub quorum_percent: u32,
    pub approval_percent: u32,
    pub vote_duration_blocks: u32,
    pub execution_delay_blocks: u32,
    pub auto_execute: bool,
    pub max_delegations_per_address: u32,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            quorum_percent: 10,
            approval_percent: 60,
            vote_duration_blocks: 10_000,
            execution_delay_blocks: 100,
            auto_execute: true,
            max_delegations_per_address: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub tx_id: String,
    pub proposal_id: String,
    pub executed_at: u64,
    pub result: String,
    pub gas_used: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegation {
    pub delegator: String,
    pub delegate: String,
    pub weight: u64,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalSummary {
    pub proposal_id: String,
    pub title: String,
    pub category: String,
    pub stage: String,
    pub votes_for: u64,
    pub votes_against: u64,
    pub total_voters: u32,
    pub quorum_met: bool,
    pub approval_met: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceStats {
    pub total_proposals: u64,
    pub active_proposals: u64,
    pub passed_proposals: u64,
    pub failed_proposals: u64,
    pub executed_proposals: u64,
    pub total_votes_cast: u64,
    pub participation_rate: f64,
    pub total_delegations: u64,
    pub avg_execution_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Governance Executor
// ---------------------------------------------------------------------------

pub struct GovernanceExecutor {
    config: Arc<RwLock<ExecutorConfig>>,
    store: Arc<ProposalStore>,
    onchain: Arc<OnChainGovernance>,
    proposals: DashMap<String, ProposalSummary>,
    delegations: DashMap<String, Delegation>,
    receipts: DashMap<String, ExecutionReceipt>,
    #[allow(dead_code)]
    receipt_counter: AtomicU64,
    current_height: AtomicU64,
}

impl GovernanceExecutor {
    pub fn new(
        store: Arc<ProposalStore>,
        onchain: Arc<OnChainGovernance>,
        config: ExecutorConfig,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            store,
            onchain,
            proposals: DashMap::new(),
            delegations: DashMap::new(),
            receipts: DashMap::new(),
            receipt_counter: AtomicU64::new(0),
            current_height: AtomicU64::new(0),
        }
    }

    pub fn with_defaults(store: Arc<ProposalStore>, onchain: Arc<OnChainGovernance>) -> Self {
        Self::new(store, onchain, ExecutorConfig::default())
    }

    // -----------------------------------------------------------------------
    // Proposal lifecycle
    // -----------------------------------------------------------------------

    pub fn submit_proposal(
        &self,
        title: String,
        description: String,
        category: ProposalCategory,
        proposer_pk: String,
    ) -> Result<ProposalSummary, GovernanceError> {
        let height = self.current_height.load(Ordering::Relaxed) as u32;

        let proposal = self.store.create_proposal(
            &title,
            &description,
            category,
            &proposer_pk,
            height,
            None,
        )?;

        let summary = self.proposal_to_summary(&proposal);
        self.proposals.insert(proposal.proposal_id.clone(), summary.clone());

        info!(proposal_id = %proposal.proposal_id, title = %title, "Proposal submitted");
        Ok(summary)
    }

    pub fn cast_vote(
        &self,
        proposal_id: &str,
        voter_pk: &str,
        support: bool,
        voting_power: u64,
    ) -> Result<TallyResult, GovernanceError> {
        let proposal = self.store.get_proposal(proposal_id)?;

        if !proposal.stage.is_voting() {
            return Err(GovernanceError::ProposalFinalized(format!(
                "Proposal {} is in stage {:?}",
                proposal_id, proposal.stage
            )));
        }

        if proposal.has_voted(voter_pk) {
            return Err(GovernanceError::AlreadyVoted(voter_pk.to_string()));
        }

        let height = self.current_height.load(Ordering::Relaxed) as u32;
        let _vote = self.store.cast_vote(
            proposal_id,
            voter_pk,
            support,
            voting_power,
            height,
            &format!("tx_vote_{}_{}", proposal_id, voter_pk),
        )?;

        let tally = self.store.tally_proposal(proposal_id)?;

        if let Ok(updated) = self.store.get_proposal(proposal_id) {
            let summary = self.proposal_to_summary(&updated);
            self.proposals.insert(proposal_id.to_string(), summary);
        }

        info!(proposal_id = %proposal_id, voter = %voter_pk, support = support, "Vote cast");
        Ok(tally)
    }

    pub fn tally_proposal(&self, proposal_id: &str) -> Result<TallyResult, GovernanceError> {
        let tally = self.store.tally_proposal(proposal_id)?;
        info!(proposal_id = %proposal_id, passes = tally.passes, "Proposal tallied");
        Ok(tally)
    }

    pub fn execute_proposal(&self, proposal_id: &str) -> Result<ExecutionReceipt, GovernanceError> {
        let proposal = self.store.get_proposal(proposal_id)?;

        if proposal.stage.is_terminal() {
            return Err(GovernanceError::ProposalFinalized(format!(
                "Proposal {} is already in stage {:?}",
                proposal_id, proposal.stage
            )));
        }

        let tally = self.store.tally_proposal(proposal_id)?;
        if !tally.passes {
            return Err(GovernanceError::ApprovalNotMet {
                percentage: tally.approval_percentage as u64,
                required: proposal.approval_threshold,
            });
        }

        let start = std::time::Instant::now();
        let box_id = if proposal.box_id.is_empty() {
            proposal.proposal_id.clone()
        } else {
            proposal.box_id.clone()
        };

        let tx_result = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async { self.onchain.build_execute_tx(&box_id, vec![]).await })
        })?;

        self.store.advance_stage(proposal_id)?;

        let elapsed = start.elapsed().as_millis() as u64;
        let receipt = ExecutionReceipt {
            tx_id: tx_result.tx_id.clone(),
            proposal_id: proposal_id.to_string(),
            executed_at: Utc::now().timestamp_millis() as u64,
            result: "success".to_string(),
            gas_used: tx_result.fee,
            error: None,
        };
        self.receipts.insert(receipt.tx_id.clone(), receipt.clone());

        if let Ok(updated) = self.store.get_proposal(proposal_id) {
            let summary = self.proposal_to_summary(&updated);
            self.proposals.insert(proposal_id.to_string(), summary);
        }

        info!(proposal_id = %proposal_id, tx_id = %tx_result.tx_id, elapsed_ms = elapsed, "Proposal executed");
        Ok(receipt)
    }

    pub fn close_proposal(&self, proposal_id: &str) -> Result<(), GovernanceError> {
        let proposal = self.store.get_proposal(proposal_id)?;

        if proposal.stage == ProposalStage::Closed {
            return Err(GovernanceError::ProposalFinalized("Already closed".to_string()));
        }

        let box_id = if proposal.box_id.is_empty() {
            proposal.proposal_id.clone()
        } else {
            proposal.box_id.clone()
        };

        let _tx = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async { self.onchain.build_close_tx(&box_id).await })
        })?;

        self.store.advance_stage(proposal_id)?;
        if let Ok(updated) = self.store.get_proposal(proposal_id) {
            let summary = self.proposal_to_summary(&updated);
            self.proposals.insert(proposal_id.to_string(), summary);
        }

        info!(proposal_id = %proposal_id, "Proposal closed");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Delegation
    // -----------------------------------------------------------------------

    pub fn delegate_votes(
        &self,
        delegator: &str,
        delegate: &str,
        weight: u64,
    ) -> Result<Delegation, GovernanceError> {
        let config = self.config.blocking_read();

        let count = self
            .delegations
            .iter()
            .filter(|e| {
                e.key().starts_with(&format!("{}:", delegator)) && e.value().revoked_at.is_none()
            })
            .count() as u32;

        if count >= config.max_delegations_per_address {
            return Err(GovernanceError::ValidationFailed(format!(
                "Max delegations ({}) exceeded for {}",
                config.max_delegations_per_address, delegator
            )));
        }

        let delegation = Delegation {
            delegator: delegator.to_string(),
            delegate: delegate.to_string(),
            weight,
            created_at: Utc::now().timestamp_millis() as u64,
            revoked_at: None,
        };

        let key = format!("{}:{}", delegator, delegate);
        self.delegations.insert(key, delegation.clone());

        info!(delegator = %delegator, delegate = %delegate, weight = weight, "Delegation created");
        Ok(delegation)
    }

    pub fn revoke_delegation(&self, delegator: &str) -> Result<(), GovernanceError> {
        let mut found = false;
        for mut entry in self.delegations.iter_mut() {
            if entry.key().starts_with(&format!("{}:", delegator)) && entry.value().revoked_at.is_none()
            {
                entry.value_mut().revoked_at = Some(Utc::now().timestamp_millis() as u64);
                found = true;
            }
        }

        if !found {
            return Err(GovernanceError::ValidationFailed(format!(
                "No active delegation found for {}",
                delegator
            )));
        }

        info!(delegator = %delegator, "Delegation revoked");
        Ok(())
    }

    pub fn get_delegations(&self, address: &str) -> Vec<Delegation> {
        self.delegations
            .iter()
            .filter(|e| {
                e.key().starts_with(&format!("{}:", address))
                    || e.key().ends_with(&format!(":{}", address))
            })
            .map(|e| e.value().clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    pub fn get_proposal_summary(&self, proposal_id: &str) -> Option<ProposalSummary> {
        self.proposals.get(proposal_id).map(|r| r.value().clone())
    }

    pub fn list_proposals(&self, stage_filter: Option<&str>, limit: usize) -> Vec<ProposalSummary> {
        let mut results: Vec<ProposalSummary> = self
            .proposals
            .iter()
            .filter(|e| stage_filter.map(|s| e.value().stage == s).unwrap_or(true))
            .map(|e| e.value().clone())
            .collect();
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.truncate(limit);
        results
    }

    pub fn get_stats(&self) -> GovernanceStats {
        let total = self.proposals.len() as u64;
        let active = self
            .proposals
            .iter()
            .filter(|e| e.value().stage == "voting" || e.value().stage == "created")
            .count() as u64;
        let passed = self
            .proposals
            .iter()
            .filter(|e| e.value().stage == "executed")
            .count() as u64;
        let failed = self
            .proposals
            .iter()
            .filter(|e| e.value().stage == "closed" || e.value().stage == "expired")
            .count() as u64;

        let total_votes: u64 = self
            .proposals
            .iter()
            .map(|e| e.value().votes_for + e.value().votes_against)
            .sum();
        let participation = if total > 0 {
            (active as f64 / total as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        let total_delegations = self
            .delegations
            .iter()
            .filter(|e| e.value().revoked_at.is_none())
            .count() as u64;

        let avg_exec_ms = if passed > 0 {
            let total_ms: u64 = self
                .receipts
                .iter()
                .filter(|e| e.value().result == "success")
                .map(|e| e.value().gas_used)
                .sum();
            total_ms / passed.max(1)
        } else {
            0
        };

        GovernanceStats {
            total_proposals: total,
            active_proposals: active,
            passed_proposals: passed,
            failed_proposals: failed,
            executed_proposals: passed,
            total_votes_cast: total_votes,
            participation_rate: participation,
            total_delegations,
            avg_execution_time_ms: avg_exec_ms,
        }
    }

    pub fn get_receipts(&self, proposal_id: &str) -> Vec<ExecutionReceipt> {
        self.receipts
            .iter()
            .filter(|e| e.value().proposal_id == proposal_id)
            .map(|e| e.value().clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Auto-execution
    // -----------------------------------------------------------------------

    pub fn auto_execute_passing(&self) -> u64 {
        if !self.config.blocking_read().auto_execute {
            return 0;
        }

        let height = self.current_height.load(Ordering::Relaxed) as u32;
        let mut executed = 0u64;

        let proposals: Vec<String> = self
            .proposals
            .iter()
            .filter(|e| e.value().stage == "created" || e.value().stage == "voting")
            .map(|e| e.key().clone())
            .collect();

        for pid in proposals {
            if let Ok(proposal) = self.store.get_proposal(&pid) {
                if proposal.vote_end_height <= height {
                    match self.tally_proposal(&pid) {
                        Ok(tally) if tally.passes => {
                            match self.execute_proposal(&pid) {
                                Ok(_) => executed += 1,
                                Err(e) => warn!(proposal_id = %pid, error = %e, "Auto-execute failed"),
                            }
                        }
                        _ => {
                            let _ = self.close_proposal(&pid);
                        }
                    }
                }
            }
        }

        if executed > 0 {
            info!(count = executed, "Auto-executed proposals");
        }
        executed
    }

    // -----------------------------------------------------------------------
    // State management
    // -----------------------------------------------------------------------

    pub fn set_height(&self, height: u64) {
        self.current_height.store(height, Ordering::Relaxed);
    }

    pub async fn get_config(&self) -> ExecutorConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config(&self, config: ExecutorConfig) {
        *self.config.write().await = config;
    }

    pub fn update_config_sync(&self, config: ExecutorConfig) {
        *self.config.blocking_write() = config;
    }

    fn proposal_to_summary(&self, proposal: &OnChainProposal) -> ProposalSummary {
        ProposalSummary {
            proposal_id: proposal.proposal_id.clone(),
            title: proposal.title.clone(),
            category: proposal.category.to_string(),
            stage: proposal.stage.to_string(),
            votes_for: proposal.votes_for,
            votes_against: proposal.votes_against,
            total_voters: proposal.voters.len() as u32,
            quorum_met: proposal.meets_quorum(),
            approval_met: proposal.meets_approval(),
            created_at: proposal.created_height as u64,
            expires_at: proposal.vote_end_height as u64,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::GovernanceConfig;

    fn make_executor() -> GovernanceExecutor {
        let store = Arc::new(ProposalStore::new(GovernanceConfig::default()));
        let onchain = Arc::new(OnChainGovernance::with_defaults());
        let executor = GovernanceExecutor::with_defaults(store, onchain);
        executor.set_height(1000);
        executor
    }

    #[test]
    fn test_executor_creation() {
        let executor = make_executor();
        let stats = executor.get_stats();
        assert_eq!(stats.total_proposals, 0);
        assert_eq!(stats.total_delegations, 0);
    }

    #[test]
    fn test_submit_proposal() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Test Proposal".to_string(),
                "Description".to_string(),
                ProposalCategory::ProtocolParam,
                "test_pk".to_string(),
            )
            .unwrap();

        assert_eq!(summary.title, "Test Proposal");
        assert_eq!(summary.stage, "created");
        let stats = executor.get_stats();
        assert_eq!(stats.total_proposals, 1);
    }

    #[test]
    fn test_cast_vote_for() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Vote Test".to_string(),
                "Desc".to_string(),
                ProposalCategory::ConfigChange,
                "proposer".to_string(),
            )
            .unwrap();

        let tally = executor.cast_vote(&summary.proposal_id, "voter1", true, 100).unwrap();
        assert_eq!(tally.votes_for, 100);
        assert_eq!(tally.total_voters, 1);
    }

    #[test]
    fn test_cast_vote_against() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Vote Against".to_string(),
                "Desc".to_string(),
                ProposalCategory::Emergency,
                "proposer".to_string(),
            )
            .unwrap();

        let tally = executor.cast_vote(&summary.proposal_id, "voter1", false, 50).unwrap();
        assert_eq!(tally.votes_against, 50);
        assert_eq!(tally.votes_for, 0);
    }

    #[test]
    fn test_tally_passes() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Tally Test".to_string(),
                "Desc".to_string(),
                ProposalCategory::ProtocolParam,
                "proposer".to_string(),
            )
            .unwrap();

        executor.cast_vote(&summary.proposal_id, "v1", true, 100).unwrap();
        executor.cast_vote(&summary.proposal_id, "v2", true, 50).unwrap();

        let tally = executor.tally_proposal(&summary.proposal_id).unwrap();
        assert!(tally.passes);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_execute_proposal() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Execute Test".to_string(),
                "Desc".to_string(),
                ProposalCategory::ProtocolParam,
                "proposer".to_string(),
            )
            .unwrap();

        for i in 0..15 {
            executor
                .cast_vote(&summary.proposal_id, &format!("v{}", i), true, 100)
                .unwrap();
        }

        let receipt = executor.execute_proposal(&summary.proposal_id).unwrap();
        assert_eq!(receipt.result, "success");
        assert!(receipt.tx_id.starts_with("tx_exec"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_execute_already_executed() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Double Exec".to_string(),
                "Desc".to_string(),
                ProposalCategory::ProtocolParam,
                "proposer".to_string(),
            )
            .unwrap();

        for i in 0..15 {
            executor
                .cast_vote(&summary.proposal_id, &format!("v{}", i), true, 100)
                .unwrap();
        }

        executor.execute_proposal(&summary.proposal_id).unwrap();
        let result = executor.execute_proposal(&summary.proposal_id);
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_close_proposal() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Close Test".to_string(),
                "Desc".to_string(),
                ProposalCategory::ProtocolParam,
                "proposer".to_string(),
            )
            .unwrap();

        executor.close_proposal(&summary.proposal_id).unwrap();
        let updated = executor.get_proposal_summary(&summary.proposal_id).unwrap();
        assert_eq!(updated.stage, "closed");
    }

    #[test]
    fn test_delegation_creation() {
        let executor = make_executor();
        let del = executor.delegate_votes("alice", "bob", 500).unwrap();
        assert_eq!(del.delegator, "alice");
        assert_eq!(del.delegate, "bob");
        assert!(del.revoked_at.is_none());
        let stats = executor.get_stats();
        assert_eq!(stats.total_delegations, 1);
    }

    #[test]
    fn test_delegation_revocation() {
        let executor = make_executor();
        executor.delegate_votes("alice", "bob", 500).unwrap();
        executor.revoke_delegation("alice").unwrap();
        let dels = executor.get_delegations("alice");
        assert_eq!(dels.len(), 1);
        assert!(dels[0].revoked_at.is_some());
    }

    #[test]
    fn test_delegation_limit() {
        let executor = make_executor();
        executor.update_config_sync(ExecutorConfig {
            max_delegations_per_address: 2,
            ..Default::default()
        });

        executor.delegate_votes("alice", "bob", 100).unwrap();
        executor.delegate_votes("alice", "carol", 200).unwrap();
        let result = executor.delegate_votes("alice", "dave", 300);
        assert!(result.is_err());
    }

    #[test]
    fn test_double_vote() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Double".into(), "D".into(), ProposalCategory::ProtocolParam, "pk".into(),
            )
            .unwrap();

        executor.cast_vote(&summary.proposal_id, "v1", true, 100).unwrap();
        let result = executor.cast_vote(&summary.proposal_id, "v1", false, 100);
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_vote_on_closed_proposal() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Closed Vote".into(), "D".into(), ProposalCategory::ProtocolParam, "pk".into(),
            )
            .unwrap();

        executor.close_proposal(&summary.proposal_id).unwrap();
        let result = executor.cast_vote(&summary.proposal_id, "v1", true, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_proposals_with_filter() {
        let executor = make_executor();
        executor
            .submit_proposal("P1".into(), "D".into(), ProposalCategory::ProtocolParam, "pk".into())
            .unwrap();
        executor
            .submit_proposal("P2".into(), "D".into(), ProposalCategory::Emergency, "pk".into())
            .unwrap();

        let all = executor.list_proposals(None, 100);
        assert_eq!(all.len(), 2);

        let executed = executor.list_proposals(Some("executed"), 100);
        assert_eq!(executed.len(), 0);
    }

    #[test]
    fn test_auto_execute_passing_empty() {
        let executor = make_executor();
        let count = executor.auto_execute_passing();
        assert_eq!(count, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_auto_execute_passing_ready() {
        let executor = make_executor();
        let summary = executor
            .submit_proposal(
                "Auto Exec".into(),
                "D".into(),
                ProposalCategory::ProtocolParam,
                "pk".into(),
            )
            .unwrap();

        for i in 0..15 {
            executor
                .cast_vote(&summary.proposal_id, &format!("v{}", i), true, 100)
                .unwrap();
        }

        let vote_end = summary.expires_at + 100;
        executor.set_height(vote_end);

        let count = executor.auto_execute_passing();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_concurrent_access() {
        let executor = Arc::new(make_executor());
        let mut handles = vec![];

        for i in 0..5 {
            let e = executor.clone();
            handles.push(std::thread::spawn(move || {
                let _ = e.submit_proposal(
                    format!("Concurrent {}", i),
                    "Desc".into(),
                    ProposalCategory::ConfigChange,
                    format!("pk_{}", i),
                );
                e.get_stats();
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let stats = executor.get_stats();
        assert!(stats.total_proposals >= 5);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let summary = ProposalSummary {
            proposal_id: "p1".into(),
            title: "Test".into(),
            category: "protocol_param".into(),
            stage: "created".into(),
            votes_for: 100,
            votes_against: 50,
            total_voters: 2,
            quorum_met: true,
            approval_met: true,
            created_at: 1000,
            expires_at: 11000,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: ProposalSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.proposal_id, summary.proposal_id);
    }
}
