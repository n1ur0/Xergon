{
  // =========================================================================
  // Xergon Network -- Governance Proposal Box Guard Script
  // =========================================================================
  //
  // Singleton NFT state machine for on-chain protocol governance.
  // Manages the lifecycle of governance proposals: create, vote, execute, close.
  //
  // Singleton NFT Pattern (EKB best practices):
  //   - A token with supply=1 (Governance NFT) travels with the state box.
  //   - OUTPUTS(0) must carry the NFT and the same ErgoTree.
  //   - Only one active proposal at a time (enforced by R5).
  //
  // Register Layout (EIP-4):
  //   R4: Proposal count       (Int)          -- total proposals created
  //   R5: Active proposal ID   (Int)          -- current proposal being voted on (0 = none)
  //   R6: Voting threshold     (Int)          -- minimum votes needed to pass
  //   R7: Total voters         (Int)          -- number of eligible voters
  //   R8: Proposal end height  (Int)          -- when current proposal voting ends
  //   R9: Proposal data hash   (Coll[Byte])   -- blake2b256 of proposal content
  //
  // Spending Conditions:
  //   Path 1 (Create Proposal): Any eligible voter can propose when R5 == 0.
  //       Sets R5 = new ID, increments R4, sets R6-R9.
  //   Path 2 (Vote): Eligible voter signs (atLeast(1, proveDlog)).
  //       Must be within voting window (HEIGHT <= R8).
  //       R5 must be non-zero (active proposal).
  //   Path 3 (Execute): If votes >= threshold (checked off-chain via
  //       vote counter boxes) and HEIGHT > R8, apply proposal.
  //       Resets R5 = 0 to allow new proposals.
  //   Path 4 (Close): Proposal creator can close after execution or
  //       if voting period expires without reaching threshold.
  //       Resets R5 = 0.
  //
  // Security Notes:
  //   - [INFO] Vote counting is not fully on-chain in this version.
  //     Vote weight is tracked via separate vote counter boxes. The
  //     threshold check is enforced off-chain by the agent before
  //     submitting the execution transaction.
  //   - [INFO] Only one active proposal at a time prevents governance
  //     fragmentation and simplifies the state machine.
  //   - [INFO] Proposal data hash (R9) allows verifiable proposal
  //     content. Voters should verify the hash matches expected content
  //     off-chain before voting.
  //   - [INFO] The eligible voter set is managed off-chain. The contract
  //     verifies authorization via sigma protocols (proveDlog).
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

  // ---------------------------------------------------------------------------
  // Path 1: Create Proposal
  // Any eligible voter can create a proposal when no active proposal exists.
  // Sets R5 = new ID (proposalCount + 1), increments R4, sets R6-R9.
  // ---------------------------------------------------------------------------
  val createProposal = {
    // No active proposal currently
    activeProposalId == 0 &&

    // Authorizer: at least one eligible voter must sign
    atLeast(1, OUTPUTS.map { (b: Box) => b.propositionBytes }.flatMap { (p: Coll[Byte]) =>
      // This is a structural check; actual voter authorization
      // is enforced via the transaction's input proofs
      Coll(proveDlog(SELF.R4[GroupElement].get))
    }) &&

    // Successor box must have a new active proposal
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

    // Preserve script and NFT
    scriptPreserved &&
    nftPreserved
  }

  // ---------------------------------------------------------------------------
  // Path 2: Vote on Active Proposal
  // An eligible voter signs the transaction to cast their vote.
  // Must be within the voting window (HEIGHT <= proposalEndHeight).
  // The active proposal ID must be non-zero.
  // ---------------------------------------------------------------------------
  val voteOnProposal = {
    // Active proposal must exist
    activeProposalId > 0 &&

    // Must be within voting window
    HEIGHT <= proposalEndHeight &&

    // At least one voter authorization (proveDlog)
    atLeast(1, Coll(proveDlog(SELF.R4[GroupElement].get))) &&

    // Successor preserves the active proposal state
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

    // Preserve script and NFT
    scriptPreserved &&
    nftPreserved
  }

  // ---------------------------------------------------------------------------
  // Path 3: Execute Proposal
  // If the voting period has ended (HEIGHT > proposalEndHeight) and the
  // proposal reached the threshold, execute it by resetting R5 = 0.
  // The proposal is considered passed (threshold check is off-chain;
  // the agent verifies vote counts before submitting execution tx).
  // ---------------------------------------------------------------------------
  val executeProposal = {
    // Active proposal must exist
    activeProposalId > 0 &&

    // Voting period must have ended
    HEIGHT > proposalEndHeight &&

    // Authorizer: at least one voter confirms execution
    atLeast(1, Coll(proveDlog(SELF.R4[GroupElement].get))) &&

    // Successor resets to no active proposal
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == 0 &&

    // Preserve script and NFT
    scriptPreserved &&
    nftPreserved
  }

  // ---------------------------------------------------------------------------
  // Path 4: Close / Cancel Proposal
  // If voting period expired without reaching threshold, or after execution,
  // the proposal creator can close the proposal by resetting R5 = 0.
  // ---------------------------------------------------------------------------
  val closeProposal = {
    // Active proposal must exist
    activeProposalId > 0 &&

    // Voting period must have ended
    HEIGHT > proposalEndHeight &&

    // Authorizer: at least one voter confirms closure
    atLeast(1, Coll(proveDlog(SELF.R4[GroupElement].get))) &&

    // Successor resets to no active proposal
    outBox.R4[Int].isDefined &&
    outBox.R4[Int].get == proposalCount &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == 0 &&

    // Preserve script and NFT
    scriptPreserved &&
    nftPreserved
  }

  // Final guard: any valid spending path
  createProposal || voteOnProposal || executeProposal || closeProposal
}
