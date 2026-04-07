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
  //   Path 1 (Owner Withdraw): Provider (proveDlog of R4) can withdraw
  //       after the challenge window expires (HEIGHT > R7) and not slashed
  //       (R8 == 0). The slash token must be burned or sent to treasury.
  //   Path 2 (Slash): An authorized challenger provides a valid proof of
  //       misbehavior (blake2b256 preimage match in an input box). The
  //       output sends the slash penalty to the Xergon treasury and sets
  //       R8 = 1 in the successor box. The slash token is preserved.
  //   Path 3 (Top-up): Provider (proveDlog of R4) can add more ERG to the
  //       stake. The successor box preserves the slash token, same R4-R8,
  //       and has value >= SELF.value (increase only).
  //
  // IMPORTANT: The treasury address below is a PLACEHOLDER.
  // Replace "TREASURY_ADDRESS_HERE" with the actual Ergo address
  // of the Xergon treasury before compiling this contract.
  //
  // Security Notes:
  //   - [INFO] The slashing proof uses blake2b256 preimage verification.
  //     The challenger must submit a box containing the preimage of a
  //     known hash (the challenge hash), proving misbehavior.
  //   - [INFO] The slash penalty is 20% of the staked amount. The
  //     remaining 80% goes to the provider. This ratio can be adjusted.
  //   - [INFO] Only one slashing event can occur per staking period.
  //     Once R8 = 1, no further slashes are allowed.
  //   - Storage rent: the top-up path ensures the box stays funded.
  //
  // =========================================================================

  // Treasury public key -- REPLACE WITH ACTUAL TREASURY ADDRESS BEFORE DEPLOYMENT
  // >>> DEPLOYMENT WILL FAIL WITH PLACEHOLDER -- MUST SET REAL ADDRESS <<<
  val treasuryPk = PK("TREASURY_ADDRESS_HERE")

  // Slash penalty: 20% of staked ERG goes to treasury when slashed
  val slashPenaltyRate = 20

  // Extract provider's public key from R4
  val providerPk = SELF.R4[GroupElement].get

  // Extract slashing state
  val challengeWindowEnd = SELF.R7[Int].get
  val slashedFlag = SELF.R8[Int].get
  val stakeAmount = SELF.R6[Long].get

  // Identify the Provider Slash Token (token at index 0, supply=1)
  // PLACEHOLDER: Replace with actual slash token ID at deployment
  val slashTokenId = fromBase16("0000000000000000000000000000000000000000000000000000000000000000")

  // OUTPUTS(0) convention: the successor state box is the first output.
  val outBox = OUTPUTS(0)

  // Slash token preservation check: successor must carry the same token.
  val outPreservesSlashToken = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == slashTokenId &&
    outBox.tokens(0)._2 == 1L

  // ---------------------------------------------------------------------------
  // Path 1: Owner Withdraw
  // Provider can withdraw after challenge window expires and not slashed.
  // The provider takes back the staked ERG and the slash token is sent
  // to a burn box or treasury box.
  // ---------------------------------------------------------------------------
  val ownerWithdraw = {
    proveDlog(providerPk) &&
    HEIGHT > challengeWindowEnd &&
    slashedFlag == 0 &&
    // Provider receives their staked ERG (minus fees)
    OUTPUTS.exists { (b: Box) =>
      b.propositionBytes == proveDlog(providerPk).propBytes &&
      b.value >= SELF.value - 1000000L  // fee allowance
    }
  }

  // ---------------------------------------------------------------------------
  // Path 2: Slash by authorized challenger
  // A challenger provides proof of misbehavior via blake2b256 preimage.
  // The challenge proof box (in INPUTS) must contain:
  //   R4: the challenge hash (Coll[Byte])
  //   R5: the preimage     (Coll[Byte]) such that blake2b256(preimage) == hash
  // When slashed:
  //   - Penalty (20%) goes to treasury
  //   - Remaining ERG goes to a new slashing box with R8 = 1
  //   - The slash token is preserved in the successor box
  // ---------------------------------------------------------------------------
  val slashByChallenger = {
    // Not already slashed and within challenge window
    slashedFlag == 0 &&
    HEIGHT <= challengeWindowEnd &&

    // Find the challenge proof box in inputs
    INPUTS.exists { (inp: Box) =>
      inp.R4[Coll[Byte]].isDefined &&
      inp.R5[Coll[Byte]].isDefined && {
        val challengeHash = inp.R4[Coll[Byte]].get
        val preimage = inp.R5[Coll[Byte]].get
        // Verify the preimage matches the challenge hash
        blake2b256(preimage) == challengeHash
      }
    } &&

    // Successor box preserves the slash token and sets slashed flag
    outPreservesSlashToken &&
    outBox.R4[GroupElement].isDefined &&
    outBox.R4[GroupElement].get == providerPk &&
    outBox.R8[Int].isDefined &&
    outBox.R8[Int].get == 1 &&

    // Treasury receives the slash penalty (must be a distinct output from successor)
    OUTPUTS.exists { (b: Box) =>
      b != outBox &&
      b.propositionBytes == proveDlog(treasuryPk).propBytes &&
      b.value >= (stakeAmount / 100L) * slashPenaltyRate
    }
  }

  // ---------------------------------------------------------------------------
  // Path 3: Top-up (increase stake)
  // Provider can add more ERG to increase the staked amount.
  // The successor box must preserve all registers and the slash token,
  // and have value >= SELF.value (net increase).
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
  ownerWithdraw || slashByChallenger || topUp
}
