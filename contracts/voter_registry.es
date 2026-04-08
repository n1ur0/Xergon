{
  // =========================================================================
  // Xergon Network -- Voter Registry Box (governance_proposal_v2.es companion)
  // =========================================================================
  //
  // This box serves as a DATA_INPUT for the Governance Proposal Box v2.
  // It contains the set of authorized voter public keys and configuration.
  //
  // Token: Governance NFT (supply=1, same ID as governance proposal box)
  //
  // Register Layout:
  //   R4: Coll[GroupElement] -- authorized voter public keys
  //   R5: Long               -- minimum ERG stake (nanoERG) to create proposals
  //   R6: Long               -- minimum ERG stake (nanoERG) to vote
  //   R7: Int                -- proposal voting duration (blocks)
  //
  // Modification:
  //   Only a threshold of existing voters (atLeast(ceil(n/2), voterKeys))
  //   can modify this registry. This prevents unauthorized voter addition/removal.
  //
  // =========================================================================

  val voterKeys = SELF.R4[Coll[GroupElement]].get
  val minProposalStake = SELF.R5[Long].get
  val minVoteStake = SELF.R6[Long].get
  val votingDuration = SELF.R7[Int].get

  // Governance NFT
  val govNftId = SELF.tokens(0)._1

  // OUTPUTS(0) must be the successor registry box
  val outBox = OUTPUTS(0)

  // Script must be preserved (same registry contract)
  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes

  // NFT must be preserved
  val nftPreserved = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == govNftId &&
    outBox.tokens(0)._2 == 1L

  // Authorization: at least ceil(n/2) of current voters must approve changes.
  // For n voters, threshold = (n + 1) / 2 (integer division rounds down,
  // so for odd n this gives majority; for even n this gives simple majority).
  val voterCount = voterKeys.size
  val threshold = (voterCount + 1) / 2
  val authorizedModification = atLeast(threshold, voterKeys)

  // Path 1: Update voter set (add/remove voters)
  val updateVoters = {
    authorizedModification &&
    outBox.R4[Coll[GroupElement]].isDefined &&
    outBox.R4[Coll[GroupElement]].get.size > 0 &&
    outBox.R5[Long].isDefined &&
    outBox.R6[Long].isDefined &&
    outBox.R7[Int].isDefined &&
    scriptPreserved &&
    nftPreserved
  }

  // Path 2: Update configuration (stake requirements, voting duration)
  val updateConfig = {
    authorizedModification &&
    outBox.R4[Coll[GroupElement]].isDefined &&
    outBox.R4[Coll[GroupElement]].get == voterKeys &&
    outBox.R5[Long].isDefined &&
    outBox.R6[Long].isDefined &&
    outBox.R7[Int].isDefined &&
    scriptPreserved &&
    nftPreserved
  }

  updateVoters || updateConfig
}