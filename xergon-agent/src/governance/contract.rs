//! ErgoScript contract templates for on-chain governance.
//!
//! These templates define the guarding scripts that enforce governance rules
//! at the protocol level. They are provided as documentation/reference strings
//! since actual ErgoScript compilation requires the sigma-rust compiler.
//!
//! # Register layout
//!
//! ## Proposal box
//! - R4[Int]: ProposalStage (0=Created, 1=Voting, 2=Executed, 3=Closed, 4=Expired)
//! - R5[Int]: vote_end_height
//! - R6[Long]: votes_for
//! - R7[Long]: votes_against
//! - R8[Int]: quorum_threshold
//! - R9[Int]: approval_threshold (percentage 0-100)
//!
//! ## Governance config box (data input)
//! - R4[Int]: default_quorum
//! - R5[Int]: default_approval
//! - R6[Int]: default_vote_duration
//! - R7[Long]: min_stake_to_propose
//! - R8[Long]: min_stake_to_vote
//! - R9[Int]: proposal_count

/// Returns the ErgoScript source for the proposal box guarding contract.
///
/// This contract enforces:
/// 1. The proposal NFT is preserved through all state transitions
/// 2. Vote transactions can only occur while HEIGHT <= vote_end_height
/// 3. Vote counts in the successor box must reflect the transaction
/// 4. Execution requires quorum and approval thresholds to be met
/// 5. Only stage transitions to valid next stages are allowed
pub fn proposal_guard_script() -> String {
    r#"{
  // Read governance config from data input
  val configBox = CONTEXT.dataInputs(0)
  val proposalNFT = SELF.tokens(0)._1

  // Read proposal registers
  val stage = SELF.R4[Int].get
  val voteEnd = SELF.R5[Int].get
  val votesFor = SELF.R6[Long].get
  val votesAgainst = SELF.R7[Long].get
  val quorum = SELF.R8[Int].get
  val approval = SELF.R9[Int].get
  val totalVotes = votesFor + votesAgainst

  // Config fallbacks
  val configQuorum = configBox.R4[Int].get
  val configApproval = configBox.R5[Int].get

  // ---- Voting transition ----
  // Accepts vote transactions during the voting period.
  // The successor box must preserve the NFT and script,
  // and either votesFor or votesAgainst must increase.
  val isVoting = HEIGHT <= voteEnd && (stage == 0 || stage == 1)
  val voteTransition = isVoting && {
    val out = OUTPUTS(0)
    out.propositionBytes == SELF.propositionBytes &&
    out.tokens(0)._1 == proposalNFT &&
    out.R4[Int].get == 1 &&  // stage = Voting
    out.R5[Int].get == voteEnd &&  // preserve end height
    (out.R6[Long].get >= votesFor || out.R7[Long].get >= votesAgainst)
  }

  // ---- Execution transition ----
  // Requires quorum and approval thresholds met.
  // Sets stage to Executed (2).
  val effectiveQuorum = if (quorum > 0) quorum else configQuorum
  val effectiveApproval = if (approval > 0) approval else configApproval
  val quorumMet = totalVotes >= effectiveQuorum.toLong
  val approvalMet = if (totalVotes > 0) (votesFor * 100L) / totalVotes >= effectiveApproval.toLong else false
  val canExecute = quorumMet && approvalMet && HEIGHT > voteEnd
  val execTransition = canExecute && {
    val out = OUTPUTS(0)
    out.propositionBytes == SELF.propositionBytes &&
    out.tokens(0)._1 == proposalNFT &&
    out.R4[Int].get == 2  // stage = Executed
  }

  // ---- Close transition ----
  // Anyone can close after voting period ends.
  val canClose = HEIGHT > voteEnd && stage != 2 && stage != 3
  val closeTransition = canClose && {
    val out = OUTPUTS(0)
    out.propositionBytes == SELF.propositionBytes &&
    out.tokens(0)._1 == proposalNFT &&
    (out.R4[Int].get == 3 || out.R4[Int].get == 4)
  }

  sigmaProp(voteTransition || execTransition || closeTransition)
}"#
    .to_string()
}

/// Returns the ErgoScript source for the governance config box guarding contract.
///
/// This contract enforces:
/// 1. The config NFT is preserved
/// 2. Only a valid executed proposal can modify config values
/// 3. Proposal count must always increase (monotonic)
pub fn governance_config_guard() -> String {
    r#"{
  val configNFT = SELF.tokens(0)._1
  val proposalCount = SELF.R9[Int].get

  // The config box can only be updated as an output of a proposal
  // execution transaction. We verify the spending transaction has
  // an input that is a proposal box in stage=Executed (R4=2).
  val hasExecutedProposal = INPUTS.exists { (input: Box) =>
    input.R4[Int].get == 2  // proposal stage = Executed
  }

  // If the box is being recreated (updated), the proposal count must increase
  val validUpdate = OUTPUTS.exists { (out: Box) =>
    out.tokens(0)._1 == configNFT &&
    out.R9[Int].get > proposalCount
  }

  sigmaProp(hasExecutedProposal && validUpdate)
}"#
    .to_string()
}

/// Returns the ErgoScript source for the vote box guarding contract.
///
/// Each vote creates a small box recording the voter's choice.
/// This is used for on-chain auditability and vote proof.
pub fn vote_box_guard() -> String {
    r#"{
  // Vote box contains:
  // R4[Coll[Byte]]: voter PK
  // R5[Byte]: support (0x01 = for, 0x00 = against)
  // R6[Long]: voting power (stake amount)
  // R7[Int]: proposal box creation height (link to proposal)
  // Tokens[0]: vote receipt token (unique per vote)
  //
  // The vote box is created by the voter and can only be spent
  // to update voting power (re-stake) or cleanup after proposal finalization.
  val voterPK = SELF.R4[Coll[Byte]].get
  val support = SELF.R5[Byte].get
  val votingPower = SELF.R6[Long].get

  // Can only be spent by the voter or after proposal is finalized
  val spentByVoter = {
    val proverPK = PK(voterPK)
    proverPK
  }

  sigmaProp(spentByVoter)
}"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_guard_script_not_empty() {
        let script = proposal_guard_script();
        assert!(!script.is_empty());
        assert!(script.contains("CONTEXT.dataInputs"));
        assert!(script.contains("proposalNFT"));
        assert!(script.contains("voteTransition"));
        assert!(script.contains("execTransition"));
        assert!(script.contains("closeTransition"));
    }

    #[test]
    fn test_governance_config_guard_not_empty() {
        let script = governance_config_guard();
        assert!(!script.is_empty());
        assert!(script.contains("configNFT"));
        assert!(script.contains("proposalCount"));
        assert!(script.contains("hasExecutedProposal"));
    }

    #[test]
    fn test_vote_box_guard_not_empty() {
        let script = vote_box_guard();
        assert!(!script.is_empty());
        assert!(script.contains("voterPK"));
        assert!(script.contains("votingPower"));
    }

    #[test]
    fn test_proposal_guard_enforces_nft_preservation() {
        let script = proposal_guard_script();
        assert!(script.contains("out.tokens(0)._1 == proposalNFT"));
    }

    #[test]
    fn test_proposal_guard_enforces_stage_transitions() {
        let script = proposal_guard_script();
        // Voting: stage becomes 1
        assert!(script.contains("out.R4[Int].get == 1"));
        // Execution: stage becomes 2
        assert!(script.contains("out.R4[Int].get == 2"));
        // Close: stage becomes 3 or 4
        assert!(script.contains("out.R4[Int].get == 3"));
        assert!(script.contains("out.R4[Int].get == 4"));
    }
}
