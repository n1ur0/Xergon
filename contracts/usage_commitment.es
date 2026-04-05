{
  // =========================================================================
  // Xergon Network -- Usage Commitment Box Guard Script
  // =========================================================================
  //
  // A batched commitment box that replaces N individual usage proof boxes
  // with a single box containing a Merkle root of all proofs.
  //
  // This dramatically reduces on-chain footprint: instead of creating one
  // box per inference request (potentially thousands per day), the provider
  // batches proofs into epochs and submits a single commitment box.
  //
  // Register Layout (EIP-4):
  //   R4: Provider public key       (SigmaProp) -- who can spend this box
  //   R5: Epoch start block height  (Int)        -- start of the epoch
  //   R6: Epoch end block height    (Int)        -- end of the epoch
  //   R7: Proof count               (Int)        -- number of proofs in the batch
  //   R8: Merkle root               (Coll[Byte]) -- blake2b256 root of all proof hashes
  //
  // Tokens:
  //   tokens(0): Commitment NFT -- unique ID linking all commitment boxes
  //              from the same provider. Preserved across spends.
  //
  // Spending Conditions:
  //   1. Only the provider (R4) can spend the box
  //   2. The commitment NFT must be preserved in outputs
  //   3. After storage rent period (1051200 blocks), anyone can clean up
  //
  // Verification (off-chain):
  //   Given a proof (user_pk, provider_pk, model, token_count, timestamp),
  //   compute leaf = blake2b256(user_pk || provider_pk || model || token_count || timestamp).
  //   Use the Merkle proof (path + sibling hashes) to verify the leaf hashes
  //   to the stored root (R8).
  //
  // Security Notes:
  //   - [INFO] The Merkle root (R8) is stored as-is with no on-chain
  //     verification of correct construction. The relay/off-chain verifier
  //     MUST validate Merkle proofs against this root to ensure integrity.
  //   - [INFO] Epoch bounds (R5, R6) are stored but not validated for
  //     overlap or ordering. A provider could submit overlapping epochs.
  //     Off-chain logic SHOULD enforce epoch ordering constraints.
  //   - Storage rent: commitment boxes replace N proof boxes, significantly
  //     reducing UTXO set growth. The NFT preservation check ensures the
  //     provider's commitment chain remains intact across spends.
  //
  // =========================================================================

  val providerPK = SELF.R4[SigmaProp].get
  val epochStart = SELF.R5[Int].get
  val epochEnd = SELF.R6[Int].get
  val proofCount = SELF.R7[Int].get
  val merkleRoot = SELF.R8[Coll[Byte]].get
  val commitmentNFT = SELF.tokens(0)._1

  // Spending condition 1: only the provider can spend (or storage rent cleanup)
  val canSpend = providerPK || {
    // Allow storage rent cleanup after 4 years
    val creationHeight = SELF.creationInfo._1
    HEIGHT >= creationHeight + 1051200
  }

  // Spending condition 2: commitment NFT must be preserved with
  // exact amount (1) AND script preserved to prevent hijacking.
  val preservesNFT = OUTPUTS.exists { (out: Box) =>
    out.tokens.exists { (t: (Coll[Byte], Long)) =>
      t._1 == commitmentNFT &&
      t._2 == 1L &&
      out.propositionBytes == SELF.propositionBytes
    }
  }

  canSpend && preservesNFT
}
