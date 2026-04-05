{
  // =========================================================================
  // Xergon Network -- Provider State Box Guard Script
  // =========================================================================
  //
  // This is the main provider state box. It uses a Singleton NFT
  // (Provider NFT, supply=1) as identity. The box holds provider metadata
  // in registers and is updated on every heartbeat.
  //
  // Singleton NFT Pattern (EKB best practices):
  //   - A token with supply=1 is minted when the provider first registers.
  //   - This NFT travels with the state box across spend/recreate cycles.
  //   - The NFT ID uniquely identifies the provider on-chain.
  //   - No other box can hold this NFT (enforced by the contract).
  //   - OUTPUTS(0) convention: the successor state box must be OUTPUTS(0).
  //   - scriptPreserved check: successor must have the same ErgoTree,
  //     preventing NFT hijacking by moving it to a weaker guard.
  //
  // Register Layout (EIP-4):
  //   R4: Provider public key   (GroupElement) -- spending authorization
  //   R5: Endpoint URL          (Coll[Byte])   -- UTF-8 encoded string
  //   R6: Models served         (Coll[Byte])   -- JSON encoded array string
  //   R7: PoNW score            (Int)          -- 0-1000
  //   R8: Last heartbeat height (Int)          -- block height of last heartbeat
  //   R9: Region                (Coll[Byte])   -- UTF-8 encoded string
  //
  // Spending Conditions:
  //   1. Only the provider (proveDlog of R4 pk) can spend this box.
  //   2. OUTPUTS(0) must carry the same Provider NFT (token index 0).
  //   3. OUTPUTS(0) must be guarded by the same ErgoTree (scriptPreserved).
  //   4. OUTPUTS(0) must have the same provider public key in R4.
  //   5. OUTPUTS(0) value must be >= SELF value (value preservation).
  //   6. Heartbeat monotonicity: OUTPUTS(0) R8 must be >= current R8.
  //
  // Security Notes:
  //   - [INFO] Provider PK (R4) cannot be rotated on-chain. If a provider's
  //     key is compromised, they must create a new provider registration
  //     (new NFT) and migrate off-chain. Consider adding a key rotation
  //     mechanism (e.g., R10 = authorized successor PK) for production.
  //   - [INFO] Endpoint URL (R5) and models served (R6) are stored as
  //     raw bytes with no on-chain validation. The relay MUST validate
  //     these fields off-chain (e.g., verify URL format, model names).
  //   - Storage rent: value preservation (outBox.value >= SELF.value)
  //     ensures the box won't decay to dust. Provider must keep the box
  //     funded for continued operation.
  //
  // =========================================================================

  // Extract the provider's public key from R4
  val providerPk = SELF.R4[GroupElement].get

  // Identify our Provider NFT by its token ID (token at index 0)
  val nftId = SELF.tokens(0)._1

  // OUTPUTS(0) convention: the successor state box must be the first output.
  // This is the standard singleton NFT state machine pattern from EKB.
  val outBox = OUTPUTS(0)

  // scriptPreserved check: the successor must be guarded by the same ErgoTree.
  // This prevents an attacker from moving the NFT to a box with a weaker guard
  // (e.g., a simple P2PK that they control), which would hijack the singleton.
  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes

  // Verify the output box preserves the Provider NFT with exact amount.
  val outPreservesNft = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == nftId &&
    outBox.tokens(0)._2 == 1L

  // Verify the output box is guarded by the same provider public key.
  // This prevents key rotation attacks during heartbeat transactions.
  val outHasCorrectGuard = outBox.R4[GroupElement].isDefined &&
    outBox.R4[GroupElement].get == providerPk

  // Value preservation: the successor box must hold at least as much ERG.
  // This prevents a provider from draining value from their own state box
  // (e.g., by setting output value below minimum box value to force cleanup).
  val valuePreserved = outBox.value >= SELF.value

  // Enforce heartbeat monotonicity: the last heartbeat height (R8)
  // must not decrease. This prevents replay attacks where a malicious
  // provider reverts to an older state.
  val heartbeatMonotonic = outBox.R8[Int].isDefined &&
    outBox.R8[Int].get >= SELF.R8[Int].get

  // Final spending condition: all checks must pass AND the spender
  // must prove ownership via sigma protocol (proveDlog).
  proveDlog(providerPk) && scriptPreserved && outPreservesNft &&
    outHasCorrectGuard && valuePreserved && heartbeatMonotonic
}