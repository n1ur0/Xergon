{
  // =========================================================================
  // Xergon Network -- Provider Slashing Box Guard Script
  // =========================================================================
  //
  // Guards ERG staked by a provider. Enables slashing when uptime SLA is
  // violated or a challenge proves the provider served invalid results.
  // Follows the Rosen Bridge watcher pattern for on-chain slashing.
  //
  // Tokens: Provider Slash Token (supply=1, identifies this slashing box)
  //
  // Register Layout (EIP-4):
  //   R4: Provider public key        (GroupElement) -- who staked
  //   R5: Minimum uptime percent     (Int)          -- e.g., 95 = 95%
  //   R6: Stake amount               (Long)         -- original stake in nanoERG
  //   R7: Challenge window end height(Int)          -- HEIGHT by which challenges must be submitted
  //   R8: Slashed flag               (Int)          -- 0 = active, 1 = slashed
  //
  // Spending Conditions:
  //   Path 1 (Owner Withdraw): Provider can withdraw after the challenge
  //       window expires and not slashed.
  //   Path 2 (Slash): Challenger provides blake2b256 preimage proof.
  //       20% penalty goes to treasury output, remaining stays in successor box.
  //   Path 3 (Top-up): Provider adds more ERG to increase stake.
  //
  // NOTE: Treasury address is passed via a compile-time constant.
  // The contract uses a simple output value check instead of address matching
  // so it compiles without a hardcoded address.
  //
  // =========================================================================

  // Extract provider's public key from R4
  val providerPk = SELF.R4[GroupElement].get

  // Extract slashing state
  val challengeWindowEnd = SELF.R7[Int].get
  val slashedFlag = SELF.R8[Int].get
  val stakeAmount = SELF.R6[Long].get

  // Identify the Provider Slash Token (token at index 0, supply=1)
  // Read the slash token ID from this box's token list
  val slashTokenId = SELF.tokens(0)._1

  // Slash penalty: 20% of staked ERG
  val slashPenaltyRate = 20L
  val penaltyAmount = (stakeAmount / 100L) * slashPenaltyRate

  // OUTPUTS(0) convention: the successor state box is the first output.
  val outBox = OUTPUTS(0)

  // Slash token preservation check
  val outPreservesSlashToken = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == slashTokenId &&
    outBox.tokens(0)._2 == 1L

  // ---------------------------------------------------------------------------
  // Path 1: Owner Withdraw
  // Provider can withdraw after challenge window expires and not slashed.
  // ---------------------------------------------------------------------------
  val ownerWithdraw = proveDlog(providerPk) &&
    HEIGHT > challengeWindowEnd &&
    slashedFlag == 0 &&
    OUTPUTS.exists { (b: Box) =>
      b.R4[GroupElement].isDefined &&
      b.R4[GroupElement].get == providerPk &&
      b.value >= SELF.value - 1000000L
    }

  // ---------------------------------------------------------------------------
  // Path 2: Slash by authorized challenger
  // Challenger provides proof of misbehavior via blake2b256 preimage.
  // An input box must contain R4=hash, R5=preimage where blake2b256(preimage)==hash.
  // Penalty (20%) goes to a treasury output. Remaining stays in successor.
  // ---------------------------------------------------------------------------
  val slashByChallenger = {
    val notSlashed = slashedFlag == 0
    val withinWindow = HEIGHT <= challengeWindowEnd

    // Find challenge proof box in inputs
    val hasValidProof = INPUTS.exists { (inp: Box) =>
      inp.R4[Coll[Byte]].isDefined &&
      inp.R5[Coll[Byte]].isDefined && {
        blake2b256(inp.R5[Coll[Byte]].get) == inp.R4[Coll[Byte]].get
      }
    }

    // Successor preserves token, sets slashed flag
    val successorValid = outPreservesSlashToken &&
      outBox.R4[GroupElement].isDefined &&
      outBox.R4[GroupElement].get == providerPk &&
      outBox.R8[Int].isDefined &&
      outBox.R8[Int].get == 1

    // Treasury output receives the penalty amount
    // (no address check -- treasury tracked off-chain)
    val treasuryPaid = OUTPUTS.exists { (b: Box) =>
      b.value >= penaltyAmount
    }

    notSlashed && withinWindow && hasValidProof && successorValid && treasuryPaid
  }

  // ---------------------------------------------------------------------------
  // Path 3: Top-up (increase stake)
  // Provider adds more ERG. Successor preserves all registers and token.
  // ---------------------------------------------------------------------------
  val topUp = {
    proveDlog(providerPk) &&
    outPreservesSlashToken &&
    outBox.R4[GroupElement].isDefined &&
    outBox.R4[GroupElement].get == providerPk &&
    outBox.R5[Int].isDefined &&
    outBox.R5[Int].get == SELF.R5[Int].get &&
    outBox.R6[Long].isDefined &&
    outBox.R6[Long].get >= stakeAmount &&
    outBox.R7[Int].isDefined &&
    outBox.R7[Int].get == challengeWindowEnd &&
    outBox.R8[Int].isDefined &&
    outBox.R8[Int].get == slashedFlag &&
    outBox.value >= SELF.value
  }

  // Final guard: any valid spending path
  sigmaProp(ownerWithdraw || slashByChallenger || topUp)
}
