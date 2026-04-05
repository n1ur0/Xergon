{
  // =========================================================================
  // Xergon Network -- GPU Rental Listing Box Guard Script
  // =========================================================================
  //
  // Providers create this box to list their GPU for rent on the GPU Bazar
  // marketplace. The box is readable by anyone (just scan UTXO set) but
  // only the provider can spend/update it.
  //
  // Singleton NFT Pattern:
  //   - A token with supply=1 (Listing NFT) is minted when the listing is
  //     first created. This NFT travels with the listing box across
  //     spend/recreate cycles.
  //   - The NFT ID uniquely identifies the listing on-chain.
  //   - No other box can hold this NFT (enforced by the contract).
  //   - OUTPUTS(0) convention: successor listing box must be OUTPUTS(0).
  //   - scriptPreserved check: successor must have the same ErgoTree.
  //
  // Register Layout (EIP-4):
  //   R4: Provider public key  (GroupElement) -- spending authorization
  //   R5: GPU type             (Coll[Byte])    -- UTF-8 e.g. "RTX 4090"
  //   R6: VRAM GB              (Int)           -- e.g. 24, 80
  //   R7: Price per hour       (Long)          -- nanoERG per hour
  //   R8: Region               (Coll[Byte])    -- UTF-8 e.g. "us-east"
  //   R9: Available            (Int)           -- 1=available, 0=unavailable
  //
  // Spending Conditions:
  //   1. Only the provider (proveDlog of R4 pk) can spend this box.
  //   2. OUTPUTS(0) must carry the same Listing NFT (token index 0).
  //   3. OUTPUTS(0) must be guarded by the same ErgoTree (scriptPreserved).
  //   4. OUTPUTS(0) must have the same provider public key in R4.
  //   5. OUTPUTS(0) value must be >= SELF value (value preservation).
  //
  // Security Notes:
  //   - [INFO] GPU type (R5), VRAM (R6), price (R7), region (R8), and
  //     availability (R9) are stored as raw values with no on-chain
  //     validation (e.g., no check that VRAM is a sensible number or
  //     price is positive). The relay/off-chain UI MUST validate these
  //     before displaying listings to users.
  //   - [INFO] Provider PK (R4) cannot be rotated. Same consideration
  //     as provider_box.ergo — key compromise requires new registration.
  //   - Storage rent: value preservation ensures the listing box won't
  //     decay. Providers must keep it funded to remain listed.
  //
  // =========================================================================

  // Extract the provider's public key from R4
  val providerPk = SELF.R4[GroupElement].get

  // Identify our Listing NFT by its token ID (token at index 0)
  val nftId = SELF.tokens(0)._1

  // OUTPUTS(0) convention: the successor listing box must be the first output.
  val outBox = OUTPUTS(0)

  // scriptPreserved check: successor must be guarded by the same ErgoTree.
  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes

  // Verify the output box preserves the Listing NFT with exact amount.
  val outPreservesNft = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == nftId &&
    outBox.tokens(0)._2 == 1L

  // Verify the output box is guarded by the same provider public key.
  val outHasCorrectGuard = outBox.R4[GroupElement].isDefined &&
    outBox.R4[GroupElement].get == providerPk

  // Value preservation: the successor box must hold at least as much ERG.
  val valuePreserved = outBox.value >= SELF.value

  // Final spending condition: all checks must pass AND the spender
  // must prove ownership via sigma protocol (proveDlog).
  proveDlog(providerPk) && scriptPreserved && outPreservesNft &&
    outHasCorrectGuard && valuePreserved
}
