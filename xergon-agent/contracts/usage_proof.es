{
  // =========================================================================
  // Xergon Network -- Usage Proof Box Guard Script
  // =========================================================================
  //
  // Created after each inference request as a cryptographic receipt.
  // These boxes are one-way records -- they are created but never spent
  // during normal operation. They accumulate in the UTXO set until
  // cleaned up by the Ergo storage rent mechanism.
  //
  // The usage proof serves as an on-chain audit trail:
  //   - Which user made the request (R4: user pk hash)
  //   - Which provider served it (R5: provider NFT ID)
  //   - Which model was used (R6: model name)
  //   - How many tokens were generated (R7: token count)
  //   - When the request occurred (R8: Unix timestamp)
  //
  // Register Layout (EIP-4):
  //   R4: User public key hash (Coll[Byte]) -- blake2b256 of user pk
  //   R5: Provider NFT ID       (Coll[Byte]) -- which provider served
  //   R6: Model name            (Coll[Byte]) -- UTF-8 encoded
  //   R7: Token count           (Int)        -- response tokens
  //   R8: Request timestamp     (Long)       -- Unix ms
  //
  // Spending Conditions:
  //   Only storage rent cleanup after 4 years (1,051,200 blocks).
  //   This prevents premature garbage collection and ensures the
  //   audit trail remains intact for a reasonable period.
  //
  // Transaction Flow:
  //   1. User's Staking Box is spent (balance deducted).
  //   2. Provider Box is updated (heartbeat / payment received).
  //   3. Usage Proof Box is created as a new output in the same tx.
  //   4. All three actions occur atomically in one transaction.
  //
  // Security Notes:
  //   - [LOW] This contract does NOT require a provider signature. Anyone
  //     can create a proof box claiming any provider served a request.
  //     This is BY DESIGN: the proof is created atomically in the same
  //     transaction as the user's staking box spend, so the user implicitly
  //     authorizes the proof by signing the staking box spend. The trust
  //     assumption is that the transaction builder (relay) is honest.
  //     Off-chain validation SHOULD cross-reference proof boxes against
  //     actual staking box spends to detect fabricated proofs.
  //   - [INFO] Provider NFT ID (R5) is not validated on-chain. The relay
  //     should verify the provider NFT exists in a valid provider_box
  //     before accepting the proof for reputation calculations.
  //   - Storage rent: proof boxes are created with minimum box value.
  //     They persist for ~4 years before cleanup. Batch cleanup via
  //     the usage_commitment contract can reduce UTXO set growth.
  //
  // =========================================================================

  // Calculate when storage rent cleanup is allowed.
  val creationHeight = SELF.creationInfo._1
  val rentExpiry = creationHeight + 1051200

  // The box can only be spent after the rent period expires.
  // Until then, it remains in the UTXO set as an immutable receipt.
  HEIGHT >= rentExpiry
}
