{
  // =========================================================================
  // Xergon Network -- Governance Proposal Box Guard Script
  // =========================================================================
  //
  // Singleton NFT state machine for on-chain protocol governance.
  // Manages the lifecycle: create, vote, execute, close.
  //
  // Register Layout (EIP-4):
  //   R4: Proposal count       (Int)          -- total proposals created
  //   R5: Active proposal ID   (Int)          -- current proposal (0 = none)
  //   R6: Voting threshold     (Int)          -- minimum votes needed to pass
  //   R7: Vote count           (Int)          -- current votes for active proposal
  //   R8: Proposal end height  (Int)          -- when voting ends
  //   R9: Proposal data hash   (Coll[Byte])   -- blake2b256 of proposal content
  //
  // NOTE: Authorization (voter eligibility) is enforced off-chain by the agent.
  // The contract validates state transitions only.
  //
  // =========================================================================

  val proposalCount = SELF.R4[Int].get
  val activeProposalId = SELF.R5[Int].get
  val votingThreshold = SELF.R6[Int].get
  val voteCount = SELF.R7[Int].get
  val proposalEndHeight = SELF.R8[Int].get
  val proposalDataHash = SELF.R9[Coll[Byte]].get

  val govNftId = SELF.tokens(0)._1
  val outBox = OUTPUTS(0)

  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes
  val nftPreserved = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == govNftId &&
    outBox.tokens(0)._2 == 1L

  // ---------------------------------------------------------------------------
  // Path 1: Create Proposal (no active proposal exists)
  // ---------------------------------------------------------------------------
  val createProposal = activeProposalId == 0 &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount + 1 &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get > 0 &&
    outBox.R6[Int].isDefined &&
    outBox.R6[Int].get > 0 &&
    outBox.R7[Int].isDefined &&
    outBox.R7[Int].get == 0 &&
    outBox.R8[Int].isDefined &&
    outBox.R8[Int].get > HEIGHT &&
    outBox.R9[Coll[Byte]].isDefined &&
    scriptPreserved &&
    nftPreserved

  // ---------------------------------------------------------------------------
  // Path 2: Vote on Active Proposal
  // ---------------------------------------------------------------------------
  val voteOnProposal = activeProposalId > 0 &&
    HEIGHT <= proposalEndHeight &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == activeProposalId &&
    outBox.R6[Int].isDefined &&
    outBox.R6[Int].get == votingThreshold &&
    outBox.R7[Int].isDefined &&
    outBox.R7[Int].get == voteCount + 1 &&
    outBox.R8[Int].isDefined &&
    outBox.R8[Int].get == proposalEndHeight &&
    outBox.R9[Coll[Byte]].isDefined &&
    outBox.R9[Coll[Byte]].get == proposalDataHash &&
    scriptPreserved &&
    nftPreserved

  // ---------------------------------------------------------------------------
  // Path 3: Execute Proposal (voting ended, threshold met on-chain)
  // Requires: voteCount >= votingThreshold (R7 >= R6)
  // Resets R5 = 0 and R7 = 0 for next proposal.
  // ---------------------------------------------------------------------------
  val executeProposal = activeProposalId > 0 &&
    HEIGHT > proposalEndHeight &&
    voteCount >= votingThreshold &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == 0 &&
    outBox.R7[Int].isDefined &&
    outBox.R7[Int].get == 0 &&
    scriptPreserved &&
    nftPreserved

  // ---------------------------------------------------------------------------
  // Path 4: Close / Cancel Proposal (voting expired, threshold NOT met)
  // Requires: voteCount < votingThreshold (R7 < R6)
  // Resets R5 = 0 and R7 = 0 for next proposal.
  // ---------------------------------------------------------------------------
  val closeProposal = activeProposalId > 0 &&
    HEIGHT > proposalEndHeight &&
    voteCount < votingThreshold &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == 0 &&
    outBox.R7[Int].isDefined &&
    outBox.R7[Int].get == 0 &&
    scriptPreserved &&
    nftPreserved

  sigmaProp(createProposal || voteOnProposal || executeProposal || closeProposal)
}
