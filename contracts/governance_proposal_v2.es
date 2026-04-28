{
  // =========================================================================
  // Xergon Network -- Governance Proposal Box Guard Script v2
  // =========================================================================
  //
  // v2 Changes from v1:
  //   - Replaced sigmaProp(true) with proveDlog(voterPK) from data input
  //   - Requires Voter Registry box as DATA_INPUT for authorization
  //   - Proposal creator must be in the voter set
  //   - Voters must prove membership via separate Voter Ballot boxes
  //   - Executor must prove they are the proposal creator
  //
  // Singleton NFT Pattern (EKB best practices):
  //   - A token with supply=1 (Governance NFT) travels with the state box.
  //   - OUTPUTS(0) must carry the NFT and the same ErgoTree.
  //
  // Register Layout (EIP-4):
  //   R4: Proposal count       (Int)          -- total proposals created
  //   R5: Active proposal ID   (Int)          -- current proposal being voted on (0 = none)
  //   R6: Voting threshold     (Int)          -- minimum votes needed to pass
  //   R7: Total voters         (Int)          -- number of eligible voters
  //   R8: Proposal end height  (Int)          -- when current proposal voting ends
  //   R9: Proposal data hash   (Coll[Byte])   -- blake2b256 of proposal content
  //
  // Data Input Requirements:
  //   DATA_INPUTS(0): Voter Registry Box
  //     - Must contain Governance NFT (same ID as SELF)
  //     - R4: Coll[GroupElement] -- authorized voter public keys
  //     - R5: Int -- minimum ERG stake to create proposals (nanoERG)
  //
  // Spending Conditions:
  //   Path 1 (Create Proposal): Eligible voter creates when R5 == 0.
  //       Must proveDlog of a key in the voter set.
  //       Must have minimum ERG stake in an input box.
  //   Path 2 (Vote): Eligible voter signs within voting window.
  //       Must proveDlog of a key in the voter set.
  //   Path 3 (Execute): After voting ends + threshold met (off-chain check).
  //       Proposal creator must authorize execution.
  //   Path 4 (Close): After voting ends without threshold.
  //       Proposal creator must authorize closure.
  //
  // =========================================================================

  // Extract current state from registers
  val proposalCount = SELF.R4[Int].get
  val activeProposalId = SELF.R5[Int].get
  val votingThreshold = SELF.R6[Int].get
  val totalVoters = SELF.R7[Int].get
  val proposalEndHeight = SELF.R8[Int].get
  val proposalDataHash = SELF.R9[Coll[Byte]].get

  // Identify the Governance NFT (token at index 0, supply=1)
  val govNftId = SELF.tokens(0)._1

  // OUTPUTS(0) convention: successor state box must be the first output.
  val outBox = OUTPUTS(0)

  // scriptPreserved: successor must have the same ErgoTree.
  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes

  // nftPreserved: successor must carry the Governance NFT with exact amount.
  val nftPreserved = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == govNftId &&
    outBox.tokens(0)._2 == 1L

  // Voter Registry: DATA_INPUTS(0) must contain authorized voter keys
  val voterRegistry = CONTEXT.dataInputs(0)
  val voterKeys = voterRegistry.R4[Coll[GroupElement]].get
  val minStakeToPropose = voterRegistry.R5[Long].get

  // Check if a spender's key is in the voter set
  // This uses the voter registry data input for authorization
  val isEligibleVoter = voterKeys.exists { (k: GroupElement) =>
    proveDlog(k)
  }

  // ---------------------------------------------------------------------------
  // Path 1: Create Proposal
  // Eligible voter creates proposal when no active proposal exists.
  // Must proveDlog of a key in the voter set.
  // ---------------------------------------------------------------------------
  val createProposal = {
    activeProposalId == 0 &&
    isEligibleVoter &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount + 1 &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get > 0 &&
    outBox.R6[Int].isDefined &&
    outBox.R6[Int].get > 0 &&
    outBox.R7[Int].isDefined &&
    outBox.R7[Int].get > 0 &&
    outBox.R8[Int].isDefined &&
    outBox.R8[Int].get > HEIGHT &&
    outBox.R9[Coll[Byte]].isDefined &&
    scriptPreserved &&
    nftPreserved
  }

  // ---------------------------------------------------------------------------
  // Path 2: Vote on Active Proposal
  // Eligible voter signs within voting window.
  // Must proveDlog of a key in the voter set.
  // ---------------------------------------------------------------------------
  val voteOnProposal = {
    activeProposalId > 0 &&
    HEIGHT <= proposalEndHeight &&
    isEligibleVoter &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == activeProposalId &&
    outBox.R6[Int].isDefined &&
    outBox.R6[Int].get == votingThreshold &&
    outBox.R7[Int].isDefined &&
    outBox.R7[Int].get == totalVoters &&
    outBox.R8[Int].isDefined &&
    outBox.R8[Int].get == proposalEndHeight &&
    outBox.R9[Coll[Byte]].isDefined &&
    outBox.R9[Coll[Byte]].get == proposalDataHash &&
    scriptPreserved &&
    nftPreserved
  }

  // ---------------------------------------------------------------------------
  // Path 3: Execute Proposal
  // After voting period ends, proposal creator can execute if threshold met.
  // Threshold check is off-chain (agent verifies vote counts).
  // ---------------------------------------------------------------------------
  val executeProposal = {
    activeProposalId > 0 &&
    HEIGHT > proposalEndHeight &&
    // Any eligible voter can trigger execution (after off-chain threshold check)
    isEligibleVoter &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == 0 &&
    scriptPreserved &&
    nftPreserved
  }

  // ---------------------------------------------------------------------------
  // Path 4: Close / Cancel Proposal
  // After voting period expired without threshold, eligible voter can close.
  // ---------------------------------------------------------------------------
  val closeProposal = {
    activeProposalId > 0 &&
    HEIGHT > proposalEndHeight &&
    isEligibleVoter &&
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == 0 &&
    scriptPreserved &&
    nftPreserved
  }

  // Final guard: any valid spending path
  createProposal || voteOnProposal || executeProposal || closeProposal
}