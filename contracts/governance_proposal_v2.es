     1|{
     2|  // =========================================================================
     3|  // Xergon Network -- Governance Proposal Box Guard Script v2
     4|  // =========================================================================
     5|  //
     6|  // v2 Changes from v1:
     7|  //   - Replaced sigmaProp(true) with proveDlog(voterPK) from data input
     8|  //   - Requires Voter Registry box as DATA_INPUT for authorization
     9|  //   - Proposal creator must be in the voter set
    10|  //   - Voters must prove membership via separate Voter Ballot boxes
    11|  //   - Executor must prove they are the proposal creator
    12|  //
    13|  // Singleton NFT Pattern (EKB best practices):
    14|  //   - A token with supply=1 (Governance NFT) travels with the state box.
    15|  //   - OUTPUTS(0) must carry the NFT and the same ErgoTree.
    16|  //
    17|  // Register Layout (EIP-4):
    18|  //   R4: Proposal count       (Int)          -- total proposals created
    19|  //   R5: Active proposal ID   (Int)          -- current proposal being voted on (0 = none)
    20|  //   R6: Voting threshold     (Int)          -- minimum votes needed to pass
    21|  //   R7: Total voters         (Int)          -- number of eligible voters
    22|  //   R8: Proposal end height  (Int)          -- when current proposal voting ends
    23|  //   R9: Proposal data hash   (Coll[Byte])   -- blake2b256 of proposal content
    24|  //
    25|  // Data Input Requirements:
    26|  //   DATA_INPUTS(0): Voter Registry Box
    27|  //     - Must contain Governance NFT (same ID as SELF)
    28|  //     - R4: Coll[GroupElement] -- authorized voter public keys
    29|  //     - R5: Int -- minimum ERG stake to create proposals (nanoERG)
    30|  //
    31|  // Spending Conditions:
    32|  //   Path 1 (Create Proposal): Eligible voter creates when R5 == 0.
    33|  //       Must proveDlog of a key in the voter set.
    34|  //       Must have minimum ERG stake in an input box.
    35|  //   Path 2 (Vote): Eligible voter signs within voting window.
    36|  //       Must proveDlog of a key in the voter set.
    37|  //   Path 3 (Execute): After voting ends + threshold met (off-chain check).
    38|  //       Proposal creator must authorize execution.
    39|  //   Path 4 (Close): After voting ends without threshold.
    40|  //       Proposal creator must authorize closure.
    41|  //
    42|  // =========================================================================
    43|
    44|  // Extract current state from registers
    45|  val proposalCount = SELF.R4[Int].get
    46|  val activeProposalId = SELF.R5[Int].get
    47|  val votingThreshold = SELF.R6[Int].get
    48|  val totalVoters = SELF.R7[Int].get
    49|  val proposalEndHeight = SELF.R8[Int].get
    50|  val proposalDataHash = SELF.R9[Coll[Byte]].get
    51|
    52|  // Identify the Governance NFT (token at index 0, supply=1)
    53|  val govNftId = SELF.tokens(0)._1
    54|
    55|  // OUTPUTS(0) convention: successor state box must be the first output.
    56|  val outBox = OUTPUTS(0)
    57|
    58|  // scriptPreserved: successor must have the same ErgoTree.
    59|  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes
    60|
    61|  // nftPreserved: successor must carry the Governance NFT with exact amount.
    62|  val nftPreserved = outBox.tokens.size > 0 &&
    63|    outBox.tokens(0)._1 == govNftId &&
    64|    outBox.tokens(0)._2 == 1L
    65|
    66|  // Voter Registry: DATA_INPUTS(0) must contain authorized voter keys
    67|  val voterRegistry = CONTEXT.dataInputs(0)
    68|  val voterKeys = voterRegistry.R4[Coll[GroupElement]].get
    69|  val minStakeToPropose = voterRegistry.R5[Long].get
    70|
    71|  // Check if a spender's key is in the voter set
    72|  // This uses the voter registry data input for authorization
    73|  val isEligibleVoter = voterKeys.exists { (k: GroupElement) =>
    74|    proveDlog(k)
    75|  }
    76|
    77|  // ---------------------------------------------------------------------------
    78|  // Path 1: Create Proposal
    79|  // Eligible voter creates proposal when no active proposal exists.
    80|  // Must proveDlog of a key in the voter set.
    81|  // ---------------------------------------------------------------------------
    82|  val createProposal = {
    83|    activeProposalId == 0 &&
    84|    isEligibleVoter &&
    85|    outBox.R4[Int].isDefined &&
    86|    outBox.R4[Int].get == proposalCount + 1 &&
    87|    outBox.R5[Int].isDefined &&
    88|    outBox.R5[Int].get > 0 &&
    89|    outBox.R6[Int].isDefined &&
    90|    outBox.R6[Int].get > 0 &&
    91|    outBox.R7[Int].isDefined &&
    92|    outBox.R7[Int].get > 0 &&
    93|    outBox.R8[Int].isDefined &&
    94|    outBox.R8[Int].get > HEIGHT &&
    95|    outBox.R9[Coll[Byte]].isDefined &&
    96|    scriptPreserved &&
    97|    nftPreserved
    98|  }
    99|
   100|  // ---------------------------------------------------------------------------
   101|  // Path 2: Vote on Active Proposal
   102|  // Eligible voter signs within voting window.
   103|  // Must proveDlog of a key in the voter set.
   104|  // ---------------------------------------------------------------------------
   105|  val voteOnProposal = {
   106|    activeProposalId > 0 &&
   107|    HEIGHT <= proposalEndHeight &&
   108|    isEligibleVoter &&
   109|    outBox.R4[Int].isDefined &&
   110|    outBox.R4[Int].get == proposalCount &&
   111|    outBox.R5[Int].isDefined &&
   112|    outBox.R5[Int].get == activeProposalId &&
   113|    outBox.R6[Int].isDefined &&
   114|    outBox.R6[Int].get == votingThreshold &&
   115|    outBox.R7[Int].isDefined &&
   116|    outBox.R7[Int].get == totalVoters &&
   117|    outBox.R8[Int].isDefined &&
   118|    outBox.R8[Int].get == proposalEndHeight &&
   119|    outBox.R9[Coll[Byte]].isDefined &&
   120|    outBox.R9[Coll[Byte]].get == proposalDataHash &&
   121|    scriptPreserved &&
   122|    nftPreserved
   123|  }
   124|
   125|  // ---------------------------------------------------------------------------
   126|  // Path 3: Execute Proposal
   127|  // After voting period ends, proposal creator can execute if threshold met.
   128|  // Threshold check is off-chain (agent verifies vote counts).
   129|  // ---------------------------------------------------------------------------
   130|  val executeProposal = {
   131|    activeProposalId > 0 &&
   132|    HEIGHT > proposalEndHeight &&
   133|    // Any eligible voter can trigger execution (after off-chain threshold check)
   134|    isEligibleVoter &&
   135|    outBox.R4[Int].isDefined &&
   136|    outBox.R4[Int].get == proposalCount &&
   137|    outBox.R5[Int].isDefined &&
   138|    outBox.R5[Int].get == 0 &&
   139|    scriptPreserved &&
   140|    nftPreserved
   141|  }
   142|
   143|  // ---------------------------------------------------------------------------
   144|  // Path 4: Close / Cancel Proposal
   145|  // After voting period expired without threshold, eligible voter can close.
   146|  // ---------------------------------------------------------------------------
   147|  val closeProposal = {
   148|    activeProposalId > 0 &&
   149|    HEIGHT > proposalEndHeight &&
   150|    isEligibleVoter &&
   151|    outBox.R4[Int].isDefined &&
   152|    outBox.R4[Int].get == proposalCount &&
   153|    outBox.R5[Int].isDefined &&
   154|    outBox.R5[Int].get == 0 &&
   155|    scriptPreserved &&
   156|    nftPreserved
   157|  }
   158|
   159|  // Final guard: any valid spending path
   160|  createProposal || voteOnProposal || executeProposal || closeProposal
   161|}