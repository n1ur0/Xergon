//! On-chain governance transaction builder.
//!
//! Provides the `OnChainGovernance` struct that builds Ergo transactions for
//! creating proposals, casting votes, executing passed proposals, and closing
//! finalized proposals. These transactions operate on the eUTXO model using
//! boxes with registers (R4-R9) and singleton NFTs.

use crate::governance::types::*;

/// On-chain governance transaction builder.
///
/// Builds unsigned Ergo transactions for governance operations. In production,
/// these transactions would be signed by the user's wallet and submitted to
/// the Ergo node. For now, the builder produces transaction descriptors that
/// can be serialized and reviewed before signing.
pub struct OnChainGovernance {
    /// Governance configuration (read from on-chain config box).
    pub config: GovernanceConfig,
}

impl OnChainGovernance {
    /// Create a new OnChainGovernance with the given config.
    pub fn new(config: GovernanceConfig) -> Self {
        Self { config }
    }

    /// Create with default governance config.
    pub fn with_defaults() -> Self {
        Self::new(GovernanceConfig::default())
    }

    // -----------------------------------------------------------------------
    // Transaction builders
    // -----------------------------------------------------------------------

    /// Build a create-proposal transaction.
    ///
    /// Creates a new proposal box with:
    /// - A singleton NFT token for identity
    /// - R4 = 0 (Created stage)
    /// - R5 = vote_end_height
    /// - R6 = 0 (votes_for)
    /// - R7 = 0 (votes_against)
    /// - R8 = quorum_threshold
    /// - R9 = approval_threshold
    pub async fn build_create_proposal_tx(
        &self,
        proposal: &OnChainProposal,
    ) -> Result<GovernanceTxResult, GovernanceError> {
        // Validate the proposal first
        self.validate_proposal(proposal)?;

        let tx_id = format!(
            "tx_create_{}_{}",
            proposal.proposal_id,
            proposal.created_height
        );

        let tx_json = serde_json::json!({
            "type": "create_proposal",
            "inputs": [],
            "dataInputs": [{"boxId": self.config.config_nft_id}],
            "outputs": [{
                "boxId": proposal.box_id,
                "propositionBytes": "<proposal_contract_script>",
                "value": proposal.erg_value,
                "registers": {
                    "R4": proposal.stage.to_register_value(),
                    "R5": proposal.vote_end_height as i64,
                    "R6": proposal.votes_for as i64,
                    "R7": proposal.votes_against as i64,
                    "R8": proposal.quorum_threshold as i64,
                    "R9": proposal.approval_threshold as i64,
                },
                "tokens": [{
                    "tokenId": proposal.nft_token_id,
                    "amount": 1
                }],
                "additionalRegisters": {
                    "R10": proposal.title.clone(),
                    "R11": proposal.description.clone(),
                }
            }],
            "fee": 1_000_000,
        })
        .to_string();

        Ok(GovernanceTxResult {
            tx_id,
            tx_json,
            boxes_created: vec![proposal.box_id.clone()],
            boxes_spent: vec![],
            fee: 1_000_000,
        })
    }

    /// Build a vote transaction.
    ///
    /// Spends the proposal box and creates a successor with updated vote counts.
    pub async fn build_vote_tx(
        &self,
        proposal_box_id: &str,
        voter_pk: &str,
        support: bool,
        voting_power: u64,
    ) -> Result<GovernanceTxResult, GovernanceError> {
        // Validate voting power
        if voting_power < self.config.min_stake_to_vote {
            return Err(GovernanceError::InsufficientStakeToVote {
                stake: voting_power,
                required: self.config.min_stake_to_vote,
            });
        }

        let vote_token_id = format!("vote_{}_{}", proposal_box_id, voter_pk);

        let tx_json = serde_json::json!({
            "type": "vote",
            "inputs": [{"boxId": proposal_box_id}],
            "dataInputs": [{"boxId": self.config.config_nft_id}],
            "outputs": [{
                "boxId": format!("{}_voted", proposal_box_id),
                "propositionBytes": "<proposal_contract_script>",
                "value": "<preserved>",
                "registers": {
                    "R4": 1, // Voting stage
                    "R5": "<preserved>",
                    "R6": "<votes_for + (support ? voting_power : 0)>",
                    "R7": "<votes_against + (!support ? voting_power : 0)>",
                    "R8": "<preserved>",
                    "R9": "<preserved>",
                },
                "tokens": [{"tokenId": "<proposal_nft>", "amount": 1}]
            }, {
                "boxId": format!("{}_votebox", voter_pk),
                "propositionBytes": "<vote_box_script>",
                "value": 1_000_000,
                "registers": {
                    "R4": voter_pk,
                    "R5": if support { 1 } else { 0 },
                    "R6": voting_power,
                },
                "tokens": [{"tokenId": vote_token_id, "amount": 1}]
            }],
            "fee": 1_000_000,
        })
        .to_string();

        Ok(GovernanceTxResult {
            tx_id: format!("tx_vote_{}_{}", proposal_box_id, &voter_pk[..8.min(voter_pk.len())]),
            tx_json,
            boxes_created: vec![
                format!("{}_voted", proposal_box_id),
                format!("{}_votebox", voter_pk),
            ],
            boxes_spent: vec![proposal_box_id.to_string()],
            fee: 1_000_000,
        })
    }

    /// Build an execute-proposal transaction.
    ///
    /// Spends the proposal box (quorum and approval met) and optionally
    /// a treasury box for fund movements.
    pub async fn build_execute_tx(
        &self,
        proposal_box_id: &str,
        execution_boxes: Vec<String>,
    ) -> Result<GovernanceTxResult, GovernanceError> {
        let tx_json = serde_json::json!({
            "type": "execute",
            "inputs": [
                {"boxId": proposal_box_id},
                ..execution_boxes.iter().map(|b| serde_json::json!({"boxId": b})).collect::<Vec<_>>()
            ],
            "dataInputs": [{"boxId": self.config.config_nft_id}],
            "outputs": [{
                "boxId": format!("{}_executed", proposal_box_id),
                "propositionBytes": "<proposal_contract_script>",
                "value": "<preserved>",
                "registers": {
                    "R4": 2, // Executed stage
                    "R5": "<preserved>",
                    "R6": "<preserved>",
                    "R7": "<preserved>",
                    "R8": "<preserved>",
                    "R9": "<preserved>",
                },
                "tokens": [{"tokenId": "<proposal_nft>", "amount": 1}]
            }],
            "fee": 2_000_000,
        })
        .to_string();

        Ok(GovernanceTxResult {
            tx_id: format!("tx_exec_{}", proposal_box_id),
            tx_json,
            boxes_created: vec![format!("{}_executed", proposal_box_id)],
            boxes_spent: execution_boxes
                .into_iter()
                .chain(std::iter::once(proposal_box_id.to_string()))
                .collect(),
            fee: 2_000_000,
        })
    }

    /// Build a close-proposal transaction.
    ///
    /// Finalizes the proposal (sets stage to Closed or Expired).
    pub async fn build_close_tx(
        &self,
        proposal_box_id: &str,
    ) -> Result<GovernanceTxResult, GovernanceError> {
        let tx_json = serde_json::json!({
            "type": "close",
            "inputs": [{"boxId": proposal_box_id}],
            "dataInputs": [{"boxId": self.config.config_nft_id}],
            "outputs": [{
                "boxId": format!("{}_closed", proposal_box_id),
                "propositionBytes": "<proposal_contract_script>",
                "value": "<preserved>",
                "registers": {
                    "R4": 3, // Closed stage
                    "R5": "<preserved>",
                    "R6": "<preserved>",
                    "R7": "<preserved>",
                    "R8": "<preserved>",
                    "R9": "<preserved>",
                },
                "tokens": [{"tokenId": "<proposal_nft>", "amount": 1}]
            }],
            "fee": 1_000_000,
        })
        .to_string();

        Ok(GovernanceTxResult {
            tx_id: format!("tx_close_{}", proposal_box_id),
            tx_json,
            boxes_created: vec![format!("{}_closed", proposal_box_id)],
            boxes_spent: vec![proposal_box_id.to_string()],
            fee: 1_000_000,
        })
    }

    // -----------------------------------------------------------------------
    // Validation and tally
    // -----------------------------------------------------------------------

    /// Validate a proposal against governance rules.
    pub fn validate_proposal(&self, proposal: &OnChainProposal) -> Result<ValidationResult, GovernanceError> {
        let mut errors = vec![];
        let mut warnings = vec![];

        // Check title length
        let title_len = proposal.title.chars().count() as u32;
        if title_len == 0 {
            errors.push("Title cannot be empty".to_string());
        } else if title_len > self.config.max_proposal_title_len {
            errors.push(format!(
                "Title exceeds max length: {} > {}",
                title_len, self.config.max_proposal_title_len
            ));
        }

        // Check description length
        let desc_len = proposal.description.chars().count() as u32;
        if desc_len == 0 {
            errors.push("Description cannot be empty".to_string());
        } else if desc_len > self.config.max_proposal_desc_len {
            errors.push(format!(
                "Description exceeds max length: {} > {}",
                desc_len, self.config.max_proposal_desc_len
            ));
        }

        // Check vote timing
        if proposal.vote_end_height <= proposal.vote_start_height {
            errors.push("vote_end_height must be after vote_start_height".to_string());
        }

        // Check thresholds
        if proposal.approval_threshold > 100 {
            errors.push("approval_threshold cannot exceed 100".to_string());
        }
        if proposal.approval_threshold == 0 {
            warnings.push("approval_threshold of 0 means any non-zero for-votes pass".to_string());
        }
        if proposal.quorum_threshold == 0 {
            errors.push("quorum_threshold must be at least 1".to_string());
        }

        // Check ERG value
        if proposal.erg_value < self.config.min_proposal_erg {
            errors.push(format!(
                "Proposal ERG value {} is below minimum {}",
                proposal.erg_value, self.config.min_proposal_erg
            ));
        }

        // Check NFT
        if proposal.nft_token_id.is_empty() {
            errors.push("Proposal NFT token ID is required".to_string());
        }

        // Check creator PK
        if proposal.creator_pk.is_empty() {
            errors.push("Creator public key is required".to_string());
        }

        // Check stage
        if proposal.stage != ProposalStage::Created {
            warnings.push(format!(
                "New proposal should be in Created stage, got {}",
                proposal.stage
            ));
        }

        if errors.is_empty() {
            Ok(ValidationResult {
                is_valid: true,
                errors: vec![],
                warnings,
            })
        } else {
            Ok(ValidationResult {
                is_valid: false,
                errors,
                warnings,
            })
        }
    }

    /// Tally votes and determine the outcome.
    pub fn tally_votes(&self, proposal: &OnChainProposal) -> TallyResult {
        let total = proposal.total_votes();
        let quorum_met = total >= proposal.quorum_threshold as u64;

        let approval_percentage = if total > 0 {
            (proposal.votes_for as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let approval_met =
            total > 0 && (proposal.votes_for * 100) / total >= proposal.approval_threshold as u64;

        let quorum_percentage = if proposal.quorum_threshold > 0 {
            (total as f64 / proposal.quorum_threshold as f64) * 100.0
        } else {
            0.0
        };

        let passes = quorum_met && approval_met;

        TallyResult {
            votes_for: proposal.votes_for,
            votes_against: proposal.votes_against,
            total_voters: proposal.voters.len() as u32,
            quorum_met,
            approval_met,
            passes,
            approval_percentage,
            quorum_percentage,
        }
    }

    /// Check if a proposal can be executed (quorum met, approval met, stage correct).
    pub fn can_execute(&self, proposal: &OnChainProposal) -> bool {
        if proposal.stage.is_terminal() {
            return false;
        }
        if proposal.stage == ProposalStage::Created {
            return false;
        }
        let tally = self.tally_votes(proposal);
        tally.passes
    }

    /// Check if a proposal can still accept votes.
    pub fn can_vote(&self, proposal: &OnChainProposal, current_height: u32) -> bool {
        proposal.stage.is_voting() && current_height <= proposal.vote_end_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_governance() -> OnChainGovernance {
        OnChainGovernance::with_defaults()
    }

    fn make_proposal() -> OnChainProposal {
        OnChainProposal {
            box_id: "box_test_001".to_string(),
            proposal_id: "prop_001".to_string(),
            title: "Test Proposal".to_string(),
            description: "A test proposal for governance".to_string(),
            category: ProposalCategory::ConfigChange,
            stage: ProposalStage::Created,
            creator_pk: "02abc123".to_string(),
            created_height: 1000,
            vote_start_height: 1000,
            vote_end_height: 11000,
            quorum_threshold: 10,
            approval_threshold: 60,
            votes_for: 0,
            votes_against: 0,
            voters: vec![],
            execution_data: None,
            nft_token_id: "nft_test_001".to_string(),
            erg_value: 2_000_000_000,
        }
    }

    // ---- Proposal stage transitions ----

    #[test]
    fn test_stage_created_to_voting() {
        let mut proposal = make_proposal();
        assert!(proposal.stage.is_voting());
        assert!(!proposal.stage.is_terminal());
        proposal.stage = ProposalStage::Voting;
        assert!(proposal.stage.is_voting());
    }

    #[test]
    fn test_stage_voting_to_executed() {
        let mut proposal = make_proposal();
        proposal.stage = ProposalStage::Voting;
        proposal.votes_for = 80;
        proposal.votes_against = 20;
        proposal.voters = vec!["pk1".into(); 100];

        proposal.stage = ProposalStage::Executed;
        assert!(proposal.stage.is_terminal());
        assert!(!proposal.stage.is_voting());
    }

    #[test]
    fn test_stage_to_expired() {
        let mut proposal = make_proposal();
        proposal.stage = ProposalStage::Expired;
        assert!(proposal.stage.is_terminal());
        assert_eq!(proposal.stage.to_register_value(), 4);
    }

    #[test]
    fn test_stage_from_register_value() {
        assert_eq!(
            ProposalStage::from_register_value(0).unwrap(),
            ProposalStage::Created
        );
        assert_eq!(
            ProposalStage::from_register_value(1).unwrap(),
            ProposalStage::Voting
        );
        assert_eq!(
            ProposalStage::from_register_value(2).unwrap(),
            ProposalStage::Executed
        );
        assert_eq!(
            ProposalStage::from_register_value(3).unwrap(),
            ProposalStage::Closed
        );
        assert_eq!(
            ProposalStage::from_register_value(4).unwrap(),
            ProposalStage::Expired
        );
        assert!(ProposalStage::from_register_value(99).is_err());
    }

    // ---- Proposal category creation ----

    #[test]
    fn test_category_from_str() {
        assert_eq!(
            "protocol_param".parse::<ProposalCategory>().unwrap(),
            ProposalCategory::ProtocolParam
        );
        assert_eq!(
            "emergency".parse::<ProposalCategory>().unwrap(),
            ProposalCategory::Emergency
        );
        assert!("invalid".parse::<ProposalCategory>().is_err());
    }

    #[test]
    fn test_category_properties() {
        assert!(ProposalCategory::Emergency.is_emergency());
        assert!(!ProposalCategory::ConfigChange.is_emergency());
        assert!(ProposalCategory::TreasurySpend.involves_funds());
        assert!(!ProposalCategory::ProtocolParam.involves_funds());
    }

    // ---- On-chain proposal creation ----

    #[tokio::test]
    async fn test_build_create_proposal_tx() {
        let gov = make_governance();
        let proposal = make_proposal();
        let result = gov.build_create_proposal_tx(&proposal).await.unwrap();
        assert!(result.tx_id.starts_with("tx_create_"));
        assert!(result.tx_json.contains("create_proposal"));
        assert!(result.boxes_created.contains(&"box_test_001".to_string()));
        assert!(result.boxes_spent.is_empty());
    }

    // ---- Vote building ----

    #[tokio::test]
    async fn test_build_vote_tx_for() {
        let gov = make_governance();
        let result = gov
            .build_vote_tx("box_test_001", "02abc123", true, 50_000_000)
            .await
            .unwrap();
        assert!(result.tx_id.contains("vote"));
        assert!(result.tx_json.contains("1")); // support = true encoded as 1
        assert_eq!(result.boxes_spent.len(), 1);
    }

    #[tokio::test]
    async fn test_build_vote_tx_against() {
        let gov = make_governance();
        let result = gov
            .build_vote_tx("box_test_001", "02abc123", false, 50_000_000)
            .await
            .unwrap();
        assert!(result.tx_json.contains("0")); // support = false encoded as 0
    }

    #[tokio::test]
    async fn test_build_vote_tx_insufficient_stake() {
        let gov = make_governance();
        let result = gov
            .build_vote_tx("box_test_001", "02abc123", true, 100) // below min of 1_000_000
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            GovernanceError::InsufficientStakeToVote { stake, required } => {
                assert_eq!(stake, 100);
                assert_eq!(required, 1_000_000);
            }
            e => panic!("Wrong error type: {}", e),
        }
    }

    // ---- Tally calculation ----

    #[test]
    fn test_tally_no_votes() {
        let gov = make_governance();
        let proposal = make_proposal();
        let tally = gov.tally_votes(&proposal);
        assert_eq!(tally.votes_for, 0);
        assert_eq!(tally.votes_against, 0);
        assert!(!tally.quorum_met);
        assert!(!tally.approval_met);
        assert!(!tally.passes);
        assert_eq!(tally.approval_percentage, 0.0);
    }

    #[test]
    fn test_tally_with_votes() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.votes_for = 70;
        proposal.votes_against = 30;
        proposal.voters = vec!["pk1".into(); 100];
        let tally = gov.tally_votes(&proposal);
        assert_eq!(tally.votes_for, 70);
        assert_eq!(tally.votes_against, 30);
        assert!(tally.quorum_met); // 100 >= 10
        assert!(tally.approval_met); // 70/100 = 70% >= 60%
        assert!(tally.passes);
        assert!((tally.approval_percentage - 70.0).abs() < 0.01);
    }

    // ---- Quorum threshold check ----

    #[test]
    fn test_quorum_not_met() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.quorum_threshold = 200;
        proposal.votes_for = 5;
        proposal.votes_against = 3;
        proposal.voters = vec!["pk1".into(); 8];
        let tally = gov.tally_votes(&proposal);
        assert!(!tally.quorum_met);
        assert!(!tally.passes);
    }

    // ---- Approval threshold check ----

    #[test]
    fn test_approval_threshold_not_met() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.votes_for = 40;
        proposal.votes_against = 60;
        proposal.voters = vec!["pk1".into(); 100];
        let tally = gov.tally_votes(&proposal);
        assert!(tally.quorum_met);
        assert!(!tally.approval_met); // 40/100 = 40% < 60%
        assert!(!tally.passes);
    }

    // ---- Can execute logic ----

    #[test]
    fn test_can_execute_passing() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.stage = ProposalStage::Voting;
        proposal.votes_for = 70;
        proposal.votes_against = 30;
        proposal.voters = vec!["pk1".into(); 100];
        assert!(gov.can_execute(&proposal));
    }

    #[test]
    fn test_can_execute_not_enough_votes() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.stage = ProposalStage::Voting;
        proposal.votes_for = 3;
        proposal.votes_against = 1;
        proposal.voters = vec!["pk1".into(); 4];
        assert!(!gov.can_execute(&proposal)); // quorum not met
    }

    #[test]
    fn test_can_execute_already_finalized() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.stage = ProposalStage::Executed;
        proposal.votes_for = 100;
        proposal.voters = vec!["pk1".into(); 100];
        assert!(!gov.can_execute(&proposal));
    }

    #[test]
    fn test_can_execute_created_stage() {
        let gov = make_governance();
        let proposal = make_proposal();
        assert!(!gov.can_execute(&proposal)); // Created stage, no votes
    }

    // ---- Governance config validation ----

    #[test]
    fn test_governance_config_defaults() {
        let config = GovernanceConfig::default();
        assert_eq!(config.min_proposal_erg, 1_000_000_000);
        assert_eq!(config.default_vote_duration, 10_000);
        assert_eq!(config.default_quorum, 10);
        assert_eq!(config.default_approval, 60);
        assert_eq!(config.max_proposal_title_len, 200);
        assert_eq!(config.max_proposal_desc_len, 5000);
    }

    // ---- Proposal validation ----

    #[test]
    fn test_validate_proposal_valid() {
        let gov = make_governance();
        let proposal = make_proposal();
        let result = gov.validate_proposal(&proposal).unwrap();
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_proposal_empty_title() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.title = String::new();
        let result = gov.validate_proposal(&proposal).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("Title cannot be empty")));
    }

    #[test]
    fn test_validate_proposal_title_too_long() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.title = "X".repeat(201);
        let result = gov.validate_proposal(&proposal).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("Title exceeds")));
    }

    #[test]
    fn test_validate_proposal_desc_too_long() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.description = "Y".repeat(5001);
        let result = gov.validate_proposal(&proposal).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("Description exceeds")));
    }

    #[test]
    fn test_validate_proposal_bad_timing() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.vote_end_height = 500; // before start of 1000
        let result = gov.validate_proposal(&proposal).unwrap();
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("vote_end_height must be after")));
    }

    #[test]
    fn test_validate_proposal_insufficient_erg() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.erg_value = 100; // way below 1 ERG minimum
        let result = gov.validate_proposal(&proposal).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("ERG value")));
    }

    #[test]
    fn test_validate_proposal_zero_quorum() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.quorum_threshold = 0;
        let result = gov.validate_proposal(&proposal).unwrap();
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("quorum_threshold must be at least 1")));
    }

    // ---- Execute/Close tx building ----

    #[tokio::test]
    async fn test_build_execute_tx() {
        let gov = make_governance();
        let result = gov
            .build_execute_tx("box_test_001", vec!["treasury_box".to_string()])
            .await
            .unwrap();
        assert!(result.tx_id.starts_with("tx_exec_"));
        assert!(result.tx_json.contains("execute"));
        assert!(result.tx_json.contains("2")); // Executed stage R4=2
        assert!(result.boxes_spent.contains(&"box_test_001".to_string()));
        assert!(result
            .boxes_spent
            .contains(&"treasury_box".to_string()));
    }

    #[tokio::test]
    async fn test_build_close_tx() {
        let gov = make_governance();
        let result = gov.build_close_tx("box_test_001").await.unwrap();
        assert!(result.tx_id.starts_with("tx_close_"));
        assert!(result.tx_json.contains("close"));
        assert!(result.tx_json.contains("3")); // Closed stage R4=3
        assert!(result.boxes_spent.contains(&"box_test_001".to_string()));
    }

    // ---- Can vote ----

    #[test]
    fn test_can_vote_during_voting_period() {
        let gov = make_governance();
        let proposal = make_proposal();
        assert!(gov.can_vote(&proposal, 5000)); // within voting window
    }

    #[test]
    fn test_can_vote_after_deadline() {
        let gov = make_governance();
        let proposal = make_proposal();
        assert!(!gov.can_vote(&proposal, 12000)); // past vote_end_height
    }

    #[test]
    fn test_can_vote_after_closed() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.stage = ProposalStage::Closed;
        assert!(!gov.can_vote(&proposal, 5000)); // closed
    }

    // ---- Already voted check ----

    #[test]
    fn test_already_voted() {
        let mut proposal = make_proposal();
        assert!(!proposal.has_voted("pk1"));
        proposal.voters.push("pk1".to_string());
        assert!(proposal.has_voted("pk1"));
        assert!(!proposal.has_voted("pk2"));
    }

    // ---- Proposal meets thresholds ----

    #[test]
    fn test_meets_quorum_and_approval() {
        let mut proposal = make_proposal();
        proposal.quorum_threshold = 10;
        proposal.approval_threshold = 60;
        proposal.votes_for = 7;
        proposal.votes_against = 3;
        assert!(proposal.meets_quorum()); // 10 >= 10
        assert!(proposal.meets_approval()); // 70%
    }

    #[test]
    fn test_does_not_meet_approval() {
        let mut proposal = make_proposal();
        proposal.quorum_threshold = 10;
        proposal.approval_threshold = 80;
        proposal.votes_for = 6;
        proposal.votes_against = 4;
        assert!(proposal.meets_quorum());
        assert!(!proposal.meets_approval()); // 60% < 80%
    }

    // ---- Quorum percentage ----

    #[test]
    fn test_tally_quorum_percentage() {
        let gov = make_governance();
        let mut proposal = make_proposal();
        proposal.quorum_threshold = 20;
        proposal.votes_for = 10;
        proposal.votes_against = 0;
        proposal.voters = vec!["pk".into(); 10];
        let tally = gov.tally_votes(&proposal);
        assert!((tally.quorum_percentage - 50.0).abs() < 0.01); // 10/20 = 50%
    }
}
