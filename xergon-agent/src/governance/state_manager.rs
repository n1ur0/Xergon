//! In-memory governance proposal store with full CRUD, vote recording, and tallying.
//!
//! Provides a thread-safe, DashMap-backed store for managing proposals through
//! their lifecycle. Supports concurrent access via `dashmap::DashMap` and atomic
//! counters for proposal sequencing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use dashmap::DashMap;
use uuid::Uuid;

use super::types::*;

// ---------------------------------------------------------------------------
// ProposalStore
// ---------------------------------------------------------------------------

/// In-memory proposal store with DashMap backing for concurrent access.
///
/// Holds all proposals and vote records, providing atomic counters for
/// proposal sequencing and full lifecycle management.
#[derive(Debug)]
pub struct ProposalStore {
    /// Proposals keyed by `proposal_id`.
    proposals: DashMap<String, OnChainProposal>,
    /// Vote records keyed by `"{proposal_id}:{voter_pk}"`.
    votes: DashMap<String, VoteRecord>,
    /// Monotonic counter for total proposals created.
    proposal_count: AtomicU64,
    /// Governance configuration used for validation.
    config: GovernanceConfig,
}

impl ProposalStore {
    /// Create a new empty store with the given governance config.
    pub fn new(config: GovernanceConfig) -> Self {
        Self {
            proposals: DashMap::new(),
            votes: DashMap::new(),
            proposal_count: AtomicU64::new(config.proposal_count),
            config,
        }
    }

    /// Create a new store with default governance config.
    pub fn with_defaults() -> Self {
        Self::new(GovernanceConfig::default())
    }

    // ---- Create ----

    /// Create and store a new proposal.
    ///
    /// Validates the proposal against `GovernanceConfig` (title length,
    /// description length), generates a UUID, and stores it in `Created` stage.
    pub fn create_proposal(
        &self,
        title: &str,
        description: &str,
        category: ProposalCategory,
        creator_pk: &str,
        created_height: u32,
        execution_data: Option<Vec<u8>>,
    ) -> Result<OnChainProposal, GovernanceError> {
        // Validate title length
        let title_len = title.chars().count() as u32;
        if title_len > self.config.max_proposal_title_len {
            return Err(GovernanceError::TitleTooLong {
                len: title_len,
                max: self.config.max_proposal_title_len,
            });
        }

        // Validate description length
        let desc_len = description.chars().count() as u32;
        if desc_len > self.config.max_proposal_desc_len {
            return Err(GovernanceError::DescriptionTooLong {
                len: desc_len,
                max: self.config.max_proposal_desc_len,
            });
        }

        let proposal_id = Uuid::new_v4().to_string();
        let nft_token_id = Uuid::new_v4().to_string();
        let vote_start_height = created_height;
        let vote_end_height = created_height.saturating_add(self.config.default_vote_duration);

        let proposal = OnChainProposal {
            box_id: String::new(), // populated when submitted on-chain
            proposal_id: proposal_id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            category,
            stage: ProposalStage::Created,
            creator_pk: creator_pk.to_string(),
            created_height,
            vote_start_height,
            vote_end_height,
            quorum_threshold: self.config.default_quorum,
            approval_threshold: self.config.default_approval,
            votes_for: 0,
            votes_against: 0,
            voters: Vec::new(),
            execution_data,
            nft_token_id,
            erg_value: self.config.min_proposal_erg,
        };

        self.proposals.insert(proposal_id.clone(), proposal.clone());
        self.proposal_count.fetch_add(1, Ordering::SeqCst);

        Ok(proposal)
    }

    // ---- Read ----

    /// Get a proposal by ID.
    pub fn get_proposal(&self, proposal_id: &str) -> Result<OnChainProposal, GovernanceError> {
        self.proposals
            .get(proposal_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| GovernanceError::ProposalNotFound(proposal_id.to_string()))
    }

    /// List proposals with optional filtering and pagination.
    ///
    /// - `stage_filter`: only return proposals in this stage (if `Some`).
    /// - `category_filter`: only return proposals of this category (if `Some`).
    /// - `offset`: skip the first N results.
    /// - `limit`: maximum number of results to return (0 = no limit).
    pub fn list_proposals(
        &self,
        stage_filter: Option<ProposalStage>,
        category_filter: Option<ProposalCategory>,
        offset: usize,
        limit: usize,
    ) -> Vec<OnChainProposal> {
        let mut results: Vec<OnChainProposal> = self
            .proposals
            .iter()
            .filter(|entry| {
                let p = entry.value();
                if let Some(stage) = stage_filter {
                    if p.stage != stage {
                        return false;
                    }
                }
                if let Some(cat) = category_filter {
                    if p.category != cat {
                        return false;
                    }
                }
                true
            })
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by created_height descending (newest first)
        results.sort_by(|a, b| b.created_height.cmp(&a.created_height));

        // Apply pagination
        if offset > 0 {
            results = results.into_iter().skip(offset).collect();
        }
        if limit > 0 {
            results.truncate(limit);
        }

        results
    }

    /// Total number of proposals in the store.
    pub fn proposal_count(&self) -> u64 {
        self.proposal_count.load(Ordering::SeqCst)
    }

    /// Number of proposals currently stored (may differ from count if removed).
    pub fn len(&self) -> usize {
        self.proposals.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.proposals.is_empty()
    }

    // ---- Vote ----

    /// Cast a vote on a proposal.
    ///
    /// Validates that the proposal is in a votable stage, the voter hasn't
    /// already voted, and the voting period hasn't ended. Updates the
    /// proposal's tally atomically.
    pub fn cast_vote(
        &self,
        proposal_id: &str,
        voter_pk: &str,
        support: bool,
        voting_power: u64,
        height: u32,
        tx_id: &str,
    ) -> Result<VoteRecord, GovernanceError> {
        let mut proposal = self
            .proposals
            .get_mut(proposal_id)
            .ok_or_else(|| GovernanceError::ProposalNotFound(proposal_id.to_string()))?;

        // Check stage allows voting
        if !proposal.stage.is_voting() {
            return Err(GovernanceError::ProposalFinalized(format!(
                "{:?}",
                proposal.stage
            )));
        }

        // Check voting period
        if height > proposal.vote_end_height {
            return Err(GovernanceError::VotingPeriodEnded {
                current: height,
                end: proposal.vote_end_height,
            });
        }

        // Dedup voter
        if proposal.has_voted(voter_pk) {
            return Err(GovernanceError::AlreadyVoted(voter_pk.to_string()));
        }

        // Update tally
        if support {
            proposal.votes_for += voting_power;
        } else {
            proposal.votes_against += voting_power;
        }
        proposal.voters.push(voter_pk.to_string());

        let vote_record = VoteRecord {
            voter_pk: voter_pk.to_string(),
            proposal_id: proposal_id.to_string(),
            support,
            voting_power,
            height,
            tx_id: tx_id.to_string(),
        };

        let vote_key = format!("{}:{}", proposal_id, voter_pk);
        self.votes.insert(vote_key, vote_record.clone());

        Ok(vote_record)
    }

    /// Get a vote record for a specific voter on a proposal.
    pub fn get_vote(
        &self,
        proposal_id: &str,
        voter_pk: &str,
    ) -> Option<VoteRecord> {
        let key = format!("{}:{}", proposal_id, voter_pk);
        self.votes.get(&key).map(|r| r.value().clone())
    }

    /// Get all vote records for a proposal.
    pub fn get_proposal_votes(&self, proposal_id: &str) -> Vec<VoteRecord> {
        let prefix = format!("{}:", proposal_id);
        self.votes
            .iter()
            .filter(|entry| entry.key().starts_with(&prefix))
            .map(|entry| entry.value().clone())
            .collect()
    }

    // ---- Tally ----

    /// Tally the votes on a proposal and return a detailed result.
    pub fn tally_proposal(&self, proposal_id: &str) -> Result<TallyResult, GovernanceError> {
        let proposal = self.get_proposal(proposal_id)?;
        let total = proposal.total_votes();
        let total_voters = proposal.voters.len() as u32;
        let quorum_met = proposal.meets_quorum();
        let approval_met = proposal.meets_approval();

        let approval_percentage = if total > 0 {
            (proposal.votes_for as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let quorum_percentage = if proposal.quorum_threshold > 0 {
            (total as f64 / proposal.quorum_threshold as f64) * 100.0
        } else {
            0.0
        };

        let passes = quorum_met && approval_met;

        Ok(TallyResult {
            votes_for: proposal.votes_for,
            votes_against: proposal.votes_against,
            total_voters,
            quorum_met,
            approval_met,
            passes,
            approval_percentage,
            quorum_percentage,
        })
    }

    // ---- Stage transitions ----

    /// Advance a proposal to the next lifecycle stage.
    ///
    /// Transitions:
    /// - `Created` -> `Voting`
    /// - `Voting` -> `Executed` (quorum met + approval met)
    /// - `Voting` -> `Closed` (quorum met + approval NOT met)
    /// - `Voting` -> `Expired` (quorum NOT met)
    pub fn advance_stage(&self, proposal_id: &str) -> Result<ProposalStage, GovernanceError> {
        let mut proposal = self
            .proposals
            .get_mut(proposal_id)
            .ok_or_else(|| GovernanceError::ProposalNotFound(proposal_id.to_string()))?;

        match proposal.stage {
            ProposalStage::Created => {
                proposal.stage = ProposalStage::Voting;
                Ok(ProposalStage::Voting)
            }
            ProposalStage::Voting => {
                let tally = TallyResult {
                    votes_for: proposal.votes_for,
                    votes_against: proposal.votes_against,
                    total_voters: proposal.voters.len() as u32,
                    quorum_met: proposal.meets_quorum(),
                    approval_met: proposal.meets_approval(),
                    passes: proposal.meets_quorum() && proposal.meets_approval(),
                    approval_percentage: if proposal.total_votes() > 0 {
                        (proposal.votes_for as f64 / proposal.total_votes() as f64) * 100.0
                    } else {
                        0.0
                    },
                    quorum_percentage: if proposal.quorum_threshold > 0 {
                        (proposal.total_votes() as f64 / proposal.quorum_threshold as f64) * 100.0
                    } else {
                        0.0
                    },
                };

                if tally.passes {
                    proposal.stage = ProposalStage::Executed;
                    Ok(ProposalStage::Executed)
                } else if tally.quorum_met {
                    proposal.stage = ProposalStage::Closed;
                    Ok(ProposalStage::Closed)
                } else {
                    proposal.stage = ProposalStage::Expired;
                    Ok(ProposalStage::Expired)
                }
            }
            ProposalStage::Executed | ProposalStage::Closed | ProposalStage::Expired => {
                Err(GovernanceError::ProposalFinalized(format!(
                    "{:?}",
                    proposal.stage
                )))
            }
        }
    }

    /// Cancel a proposal (move to `Closed` stage).
    ///
    /// Only proposals in `Created` or `Voting` stage can be cancelled.
    pub fn cancel_proposal(&self, proposal_id: &str) -> Result<ProposalStage, GovernanceError> {
        let mut proposal = self
            .proposals
            .get_mut(proposal_id)
            .ok_or_else(|| GovernanceError::ProposalNotFound(proposal_id.to_string()))?;

        if proposal.stage.is_terminal() {
            return Err(GovernanceError::ProposalFinalized(format!(
                "{:?}",
                proposal.stage
            )));
        }

        proposal.stage = ProposalStage::Closed;
        Ok(ProposalStage::Closed)
    }

    // ---- Validation ----

    /// Validate a proposal against the governance config.
    pub fn validate_proposal(
        &self,
        title: &str,
        description: &str,
        category: ProposalCategory,
        creator_pk: &str,
        creator_stake: u64,
    ) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Title length
        let title_len = title.chars().count() as u32;
        if title_len == 0 {
            errors.push("Title cannot be empty".to_string());
        } else if title_len > self.config.max_proposal_title_len {
            errors.push(format!(
                "Title exceeds max length: {} > {}",
                title_len, self.config.max_proposal_title_len
            ));
        }

        // Description length
        let desc_len = description.chars().count() as u32;
        if desc_len == 0 {
            errors.push("Description cannot be empty".to_string());
        } else if desc_len > self.config.max_proposal_desc_len {
            errors.push(format!(
                "Description exceeds max length: {} > {}",
                desc_len, self.config.max_proposal_desc_len
            ));
        }

        // Stake check (emergency proposals bypass stake requirement)
        if !category.is_emergency()
            && !self.config.emergency_council_pks.contains(&creator_pk.to_string())
            && creator_stake < self.config.min_stake_to_propose
        {
            errors.push(format!(
                "Insufficient stake to propose: {} < required {}",
                creator_stake, self.config.min_stake_to_propose
            ));
        }

        // Warnings
        if category.is_emergency() && !self.config.emergency_council_pks.contains(&creator_pk.to_string()) {
            warnings.push(
                "Emergency proposal from non-council member".to_string(),
            );
        }

        if category.involves_funds() {
            warnings.push(
                "Treasury spend proposals require additional treasury validation".to_string(),
            );
        }

        if errors.is_empty() {
            let mut result = ValidationResult::valid();
            for w in &warnings {
                result = result.with_warning(w);
            }
            result
        } else {
            let mut result = ValidationResult::invalid(errors);
            for w in &warnings {
                result = result.with_warning(w);
            }
            result
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> ProposalStore {
        ProposalStore::with_defaults()
    }

    fn create_test_proposal(store: &ProposalStore) -> String {
        let p = store
            .create_proposal(
                "Test Proposal",
                "A test proposal for unit tests",
                ProposalCategory::ProtocolParam,
                "9creator_pk",
                1000,
                None,
            )
            .unwrap();
        p.proposal_id
    }

    // ---- create_proposal tests ----

    #[test]
    fn test_create_proposal_success() {
        let store = test_store();
        let p = store
            .create_proposal(
                "Test Title",
                "Test description",
                ProposalCategory::ConfigChange,
                "9alice",
                500,
                None,
            )
            .unwrap();
        assert_eq!(p.title, "Test Title");
        assert_eq!(p.stage, ProposalStage::Created);
        assert_eq!(p.category, ProposalCategory::ConfigChange);
        assert_eq!(p.creator_pk, "9alice");
        assert!(!p.proposal_id.is_empty());
        assert!(p.created_height == 500);
        assert_eq!(p.votes_for, 0);
        assert_eq!(p.votes_against, 0);
        assert!(p.voters.is_empty());
    }

    #[test]
    fn test_create_proposal_generates_unique_ids() {
        let store = test_store();
        let p1 = store
            .create_proposal("T1", "D1", ProposalCategory::ProtocolParam, "9a", 1, None)
            .unwrap();
        let p2 = store
            .create_proposal("T2", "D2", ProposalCategory::ProtocolParam, "9a", 2, None)
            .unwrap();
        assert_ne!(p1.proposal_id, p2.proposal_id);
        assert_ne!(p1.nft_token_id, p2.nft_token_id);
    }

    #[test]
    fn test_create_proposal_title_too_long() {
        let store = test_store();
        let long_title = "X".repeat((store.config.max_proposal_title_len + 1) as usize);
        let result = store.create_proposal(
            &long_title,
            "desc",
            ProposalCategory::ProtocolParam,
            "9a",
            1,
            None,
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::TitleTooLong { len, max } => {
                assert_eq!(len, store.config.max_proposal_title_len + 1);
                assert_eq!(max, store.config.max_proposal_title_len);
            }
            e => panic!("Wrong error type: {:?}", e),
        }
    }

    #[test]
    fn test_create_proposal_description_too_long() {
        let store = test_store();
        let long_desc = "Y".repeat((store.config.max_proposal_desc_len + 1) as usize);
        let result = store.create_proposal(
            "Title",
            &long_desc,
            ProposalCategory::ProtocolParam,
            "9a",
            1,
            None,
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::DescriptionTooLong { len, max } => {
                assert_eq!(len, store.config.max_proposal_desc_len + 1);
                assert_eq!(max, store.config.max_proposal_desc_len);
            }
            e => panic!("Wrong error type: {:?}", e),
        }
    }

    #[test]
    fn test_create_proposal_increments_counter() {
        let store = test_store();
        assert_eq!(store.proposal_count(), 0);
        store.create_proposal("T", "D", ProposalCategory::ProtocolParam, "9a", 1, None).unwrap();
        assert_eq!(store.proposal_count(), 1);
        store.create_proposal("T2", "D2", ProposalCategory::ProtocolParam, "9a", 2, None).unwrap();
        assert_eq!(store.proposal_count(), 2);
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn test_create_proposal_with_execution_data() {
        let store = test_store();
        let data = vec![1, 2, 3, 4];
        let p = store
            .create_proposal(
                "Treasury Spend",
                "Spend funds",
                ProposalCategory::TreasurySpend,
                "9a",
                1,
                Some(data.clone()),
            )
            .unwrap();
        assert_eq!(p.execution_data, Some(data));
        assert_eq!(p.category, ProposalCategory::TreasurySpend);
    }

    // ---- get_proposal tests ----

    #[test]
    fn test_get_proposal_found() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        let p = store.get_proposal(&pid).unwrap();
        assert_eq!(p.proposal_id, pid);
        assert_eq!(p.title, "Test Proposal");
    }

    #[test]
    fn test_get_proposal_not_found() {
        let store = test_store();
        let result = store.get_proposal("nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::ProposalNotFound(id) => assert_eq!(id, "nonexistent"),
            e => panic!("Wrong error: {:?}", e),
        }
    }

    // ---- list_proposals tests ----

    #[test]
    fn test_list_proposals_all() {
        let store = test_store();
        store.create_proposal("T1", "D1", ProposalCategory::ProtocolParam, "9a", 10, None).unwrap();
        store.create_proposal("T2", "D2", ProposalCategory::Emergency, "9b", 20, None).unwrap();
        store.create_proposal("T3", "D3", ProposalCategory::TreasurySpend, "9c", 30, None).unwrap();
        let all = store.list_proposals(None, None, 0, 0);
        assert_eq!(all.len(), 3);
        // Sorted by height desc
        assert_eq!(all[0].created_height, 30);
        assert_eq!(all[1].created_height, 20);
        assert_eq!(all[2].created_height, 10);
    }

    #[test]
    fn test_list_proposals_filter_by_stage() {
        let store = test_store();
        let p1 = store
            .create_proposal("T1", "D1", ProposalCategory::ProtocolParam, "9a", 10, None)
            .unwrap();
        let _p2 = store
            .create_proposal("T2", "D2", ProposalCategory::ProtocolParam, "9b", 20, None)
            .unwrap();
        store.advance_stage(&p1.proposal_id).unwrap(); // Created -> Voting
        let voting = store.list_proposals(Some(ProposalStage::Voting), None, 0, 0);
        assert_eq!(voting.len(), 1);
        assert_eq!(voting[0].proposal_id, p1.proposal_id);
    }

    #[test]
    fn test_list_proposals_filter_by_category() {
        let store = test_store();
        store.create_proposal("T1", "D1", ProposalCategory::Emergency, "9a", 10, None).unwrap();
        store.create_proposal("T2", "D2", ProposalCategory::ProtocolParam, "9b", 20, None).unwrap();
        store.create_proposal("T3", "D3", ProposalCategory::Emergency, "9c", 30, None).unwrap();
        let emergency = store.list_proposals(None, Some(ProposalCategory::Emergency), 0, 0);
        assert_eq!(emergency.len(), 2);
    }

    #[test]
    fn test_list_proposals_pagination() {
        let store = test_store();
        for i in 0..10 {
            store
                .create_proposal(
                    &format!("T{}", i),
                    &format!("D{}", i),
                    ProposalCategory::ProtocolParam,
                    "9a",
                    i * 10,
                    None,
                )
                .unwrap();
        }
        let page1 = store.list_proposals(None, None, 0, 3);
        assert_eq!(page1.len(), 3);
        assert_eq!(page1[0].created_height, 90);

        let page2 = store.list_proposals(None, None, 3, 3);
        assert_eq!(page2.len(), 3);
        assert_eq!(page2[0].created_height, 60);

        let rest = store.list_proposals(None, None, 9, 10);
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn test_list_proposals_empty() {
        let store = test_store();
        let all = store.list_proposals(None, None, 0, 0);
        assert!(all.is_empty());
    }

    // ---- cast_vote tests ----

    #[test]
    fn test_cast_vote_for() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // Created -> Voting

        let record = store
            .cast_vote(&pid, "9voter1", true, 100, 1050, "tx1")
            .unwrap();
        assert!(record.support);
        assert_eq!(record.voting_power, 100);

        let p = store.get_proposal(&pid).unwrap();
        assert_eq!(p.votes_for, 100);
        assert_eq!(p.votes_against, 0);
        assert_eq!(p.voters.len(), 1);
    }

    #[test]
    fn test_cast_vote_against() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();

        store.cast_vote(&pid, "9voter1", false, 50, 1050, "tx1").unwrap();
        let p = store.get_proposal(&pid).unwrap();
        assert_eq!(p.votes_for, 0);
        assert_eq!(p.votes_against, 50);
    }

    #[test]
    fn test_cast_vote_dedup() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();

        store.cast_vote(&pid, "9voter1", true, 100, 1050, "tx1").unwrap();
        let result = store.cast_vote(&pid, "9voter1", false, 200, 1060, "tx2");
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::AlreadyVoted(pk) => assert_eq!(pk, "9voter1"),
            e => panic!("Wrong error: {:?}", e),
        }
    }

    #[test]
    fn test_cast_vote_wrong_stage_created() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        // Created stage — is_voting() returns true for Created
        // Actually, is_voting() returns true for Created and Voting
        // So this should succeed
        let result = store.cast_vote(&pid, "9voter1", true, 100, 1050, "tx1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_cast_vote_terminal_stage() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // -> Voting
        store.cancel_proposal(&pid).unwrap(); // -> Closed

        let result = store.cast_vote(&pid, "9voter1", true, 100, 1050, "tx1");
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::ProposalFinalized(_) => {}
            e => panic!("Wrong error: {:?}", e),
        }
    }

    #[test]
    fn test_cast_vote_period_ended() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();

        let p = store.get_proposal(&pid).unwrap();
        let after_end = p.vote_end_height + 100;
        let result = store.cast_vote(&pid, "9voter1", true, 100, after_end, "tx1");
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::VotingPeriodEnded { current, end } => {
                assert_eq!(current, after_end);
                assert_eq!(end, p.vote_end_height);
            }
            e => panic!("Wrong error: {:?}", e),
        }
    }

    #[test]
    fn test_cast_vote_nonexistent_proposal() {
        let store = test_store();
        let result = store.cast_vote("nonexistent", "9voter1", true, 100, 1050, "tx1");
        assert!(result.is_err());
    }

    // ---- get_vote / get_proposal_votes ----

    #[test]
    fn test_get_vote() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();
        store.cast_vote(&pid, "9voter1", true, 100, 1050, "tx1").unwrap();

        let record = store.get_vote(&pid, "9voter1").unwrap();
        assert_eq!(record.voter_pk, "9voter1");
        assert!(record.support);
    }

    #[test]
    fn test_get_vote_missing() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        assert!(store.get_vote(&pid, "9nobody").is_none());
    }

    #[test]
    fn test_get_proposal_votes() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();
        store.cast_vote(&pid, "9voter1", true, 100, 1050, "tx1").unwrap();
        store.cast_vote(&pid, "9voter2", false, 50, 1050, "tx2").unwrap();

        let votes = store.get_proposal_votes(&pid);
        assert_eq!(votes.len(), 2);
    }

    // ---- tally_proposal tests ----

    #[test]
    fn test_tally_no_votes() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        let tally = store.tally_proposal(&pid).unwrap();
        assert_eq!(tally.votes_for, 0);
        assert_eq!(tally.votes_against, 0);
        assert_eq!(tally.total_voters, 0);
        assert!(!tally.quorum_met);
        assert!(!tally.approval_met);
        assert!(!tally.passes);
        assert_eq!(tally.approval_percentage, 0.0);
    }

    #[test]
    fn test_tally_passing() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();

        // default_quorum = 10, default_approval = 60
        // Cast enough votes to meet quorum and approval
        for i in 0..8 {
            store.cast_vote(&pid, &format!("9voter{}", i), true, 10, 1050, &format!("tx{}", i)).unwrap();
        }

        let tally = store.tally_proposal(&pid).unwrap();
        assert_eq!(tally.votes_for, 80);
        assert_eq!(tally.votes_against, 0);
        assert_eq!(tally.total_voters, 8);
        assert!(tally.quorum_met); // 80 >= 10
        assert!(tally.approval_met); // 100% >= 60%
        assert!(tally.passes);
    }

    #[test]
    fn test_tally_quorum_not_met() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();

        // Only 5 votes with power 1 each -> total 5, quorum = 10
        for i in 0..5 {
            store.cast_vote(&pid, &format!("9v{}", i), true, 1, 1050, &format!("tx{}", i)).unwrap();
        }

        let tally = store.tally_proposal(&pid).unwrap();
        assert!(!tally.quorum_met);
        assert!(!tally.passes);
    }

    #[test]
    fn test_tally_approval_not_met() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap();

        // quorum=10, approval=60%
        // Cast 15 votes: 5 for, 10 against -> quorum met (15>=10) but approval 33% < 60%
        for i in 0..5 {
            store.cast_vote(&pid, &format!("9vfor{}", i), true, 1, 1050, &format!("tx{}", i)).unwrap();
        }
        for i in 0..10 {
            store.cast_vote(&pid, &format!("9vag{}", i), false, 1, 1050, &format!("tx{}", 10 + i)).unwrap();
        }

        let tally = store.tally_proposal(&pid).unwrap();
        assert!(tally.quorum_met); // 15 >= 10
        assert!(!tally.approval_met); // 5/15 = 33% < 60%
        assert!(!tally.passes);
    }

    #[test]
    fn test_tally_nonexistent() {
        let store = test_store();
        let result = store.tally_proposal("nonexistent");
        assert!(result.is_err());
    }

    // ---- advance_stage tests ----

    #[test]
    fn test_advance_created_to_voting() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        let new_stage = store.advance_stage(&pid).unwrap();
        assert_eq!(new_stage, ProposalStage::Voting);
        let p = store.get_proposal(&pid).unwrap();
        assert_eq!(p.stage, ProposalStage::Voting);
    }

    #[test]
    fn test_advance_voting_to_executed() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // -> Voting

        // Cast enough votes to pass
        for i in 0..15 {
            store.cast_vote(&pid, &format!("9v{}", i), true, 10, 1050, &format!("tx{}", i)).unwrap();
        }

        let new_stage = store.advance_stage(&pid).unwrap();
        assert_eq!(new_stage, ProposalStage::Executed);
    }

    #[test]
    fn test_advance_voting_to_expired() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // -> Voting

        // Don't cast enough votes to meet quorum (quorum=10)
        // Cast 0 votes -> quorum not met -> Expired
        let new_stage = store.advance_stage(&pid).unwrap();
        assert_eq!(new_stage, ProposalStage::Expired);
    }

    #[test]
    fn test_advance_voting_to_closed() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // -> Voting

        // Meet quorum but NOT approval: all against
        // quorum=10, approval=60
        // Cast 15 votes against -> quorum met (15>=10) but approval 0% < 60%
        for i in 0..15 {
            store.cast_vote(&pid, &format!("9v{}", i), false, 1, 1050, &format!("tx{}", i)).unwrap();
        }

        let new_stage = store.advance_stage(&pid).unwrap();
        assert_eq!(new_stage, ProposalStage::Closed);
    }

    #[test]
    fn test_advance_terminal_fails() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // -> Voting
        store.cancel_proposal(&pid).unwrap(); // -> Closed

        let result = store.advance_stage(&pid);
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::ProposalFinalized(_) => {}
            e => panic!("Wrong error: {:?}", e),
        }
    }

    #[test]
    fn test_advance_nonexistent() {
        let store = test_store();
        let result = store.advance_stage("nonexistent");
        assert!(result.is_err());
    }

    // ---- cancel_proposal tests ----

    #[test]
    fn test_cancel_created_proposal() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        let stage = store.cancel_proposal(&pid).unwrap();
        assert_eq!(stage, ProposalStage::Closed);
        let p = store.get_proposal(&pid).unwrap();
        assert_eq!(p.stage, ProposalStage::Closed);
    }

    #[test]
    fn test_cancel_voting_proposal() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // -> Voting
        let stage = store.cancel_proposal(&pid).unwrap();
        assert_eq!(stage, ProposalStage::Closed);
    }

    #[test]
    fn test_cancel_terminal_proposal_fails() {
        let store = test_store();
        let pid = create_test_proposal(&store);
        store.advance_stage(&pid).unwrap(); // -> Voting
        store.cancel_proposal(&pid).unwrap(); // -> Closed

        let result = store.cancel_proposal(&pid);
        assert!(result.is_err());
    }

    #[test]
    fn test_cancel_nonexistent() {
        let store = test_store();
        let result = store.cancel_proposal("nonexistent");
        assert!(result.is_err());
    }

    // ---- validate_proposal tests ----

    #[test]
    fn test_validate_proposal_valid() {
        let store = test_store();
        let result = store.validate_proposal(
            "Good Title",
            "Good description",
            ProposalCategory::ProtocolParam,
            "9alice",
            500_000_000, // above min_stake_to_propose
        );
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_proposal_empty_title() {
        let store = test_store();
        let result = store.validate_proposal(
            "",
            "Some description",
            ProposalCategory::ProtocolParam,
            "9alice",
            500_000_000,
        );
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("Title cannot be empty")));
    }

    #[test]
    fn test_validate_proposal_empty_description() {
        let store = test_store();
        let result = store.validate_proposal(
            "Title",
            "",
            ProposalCategory::ProtocolParam,
            "9alice",
            500_000_000,
        );
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("Description cannot be empty")));
    }

    #[test]
    fn test_validate_proposal_insufficient_stake() {
        let store = test_store();
        let result = store.validate_proposal(
            "Title",
            "Description",
            ProposalCategory::ProtocolParam,
            "9alice",
            10, // way below min_stake_to_propose
        );
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("Insufficient stake")));
    }

    #[test]
    fn test_validate_emergency_bypasses_stake() {
        let store = test_store();
        let result = store.validate_proposal(
            "Emergency!",
            "Critical action",
            ProposalCategory::Emergency,
            "9noncouncil",
            0,
        );
        // Emergency category bypasses stake, but warning about non-council
        assert!(result.is_valid);
        assert!(result.warnings.iter().any(|w| w.contains("non-council member")));
    }

    #[test]
    fn test_validate_treasury_spend_warning() {
        let store = test_store();
        let result = store.validate_proposal(
            "Spend",
            "Treasury spend proposal",
            ProposalCategory::TreasurySpend,
            "9alice",
            500_000_000,
        );
        assert!(result.is_valid);
        assert!(result.warnings.iter().any(|w| w.contains("Treasury spend")));
    }

    // ---- Concurrent access tests ----

    #[test]
    fn test_concurrent_proposal_creation() {
        use std::thread;

        let store = std::sync::Arc::new(test_store());
        let mut handles = Vec::new();

        for i in 0..10 {
            let s = store.clone();
            handles.push(thread::spawn(move || {
                s.create_proposal(
                    &format!("Concurrent Proposal {}", i),
                    "desc",
                    ProposalCategory::ProtocolParam,
                    &format!("9creator{}", i),
                    100 + i as u32,
                    None,
                )
                .unwrap()
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(store.len(), 10);
        assert_eq!(store.proposal_count(), 10);
    }

    #[test]
    fn test_concurrent_voting() {
        use std::thread;

        let store = std::sync::Arc::new(test_store());
        let p = store
            .create_proposal(
                "Vote Test",
                "desc",
                ProposalCategory::ProtocolParam,
                "9creator",
                1000,
                None,
            )
            .unwrap();
        let pid = p.proposal_id.clone();
        store.advance_stage(&pid).unwrap();

        let mut handles = Vec::new();
        for i in 0..20 {
            let s = store.clone();
            let pid_clone = pid.clone();
            handles.push(thread::spawn(move || {
                s.cast_vote(
                    &pid_clone,
                    &format!("9voter{}", i),
                    true,
                    10,
                    1050,
                    &format!("tx{}", i),
                )
                .unwrap()
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let proposal = store.get_proposal(&pid).unwrap();
        assert_eq!(proposal.voters.len(), 20);
        assert_eq!(proposal.votes_for, 200);
        assert_eq!(proposal.votes_against, 0);
    }

    #[test]
    fn test_is_empty_and_len() {
        let store = test_store();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        create_test_proposal(&store);
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_custom_config_thresholds() {
        let config = GovernanceConfig {
            default_quorum: 100,
            default_approval: 75,
            default_vote_duration: 5000,
            ..GovernanceConfig::default()
        };
        let store = ProposalStore::new(config);
        let p = store
            .create_proposal("T", "D", ProposalCategory::ProtocolParam, "9a", 100, None)
            .unwrap();
        assert_eq!(p.quorum_threshold, 100);
        assert_eq!(p.approval_threshold, 75);
        assert_eq!(p.vote_end_height, 100 + 5000);
    }
}
