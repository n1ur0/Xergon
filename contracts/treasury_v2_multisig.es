{
  // =========================================================================
  // Xergon Network -- Treasury Box Guard Script v2 (Multi-Sig Committee)
  // =========================================================================
  //
  // Upgrade from v1 single-key to multi-sig committee control.
  // Uses atLeast(k, keys) threshold for all spending operations.
  //
  // Register Layout (EIP-4):
  //   R4: Total airdropped amount (Long) -- cumulative nanoERGs distributed
  //
  // Token:
  //   Xergon Network NFT (supply=1) at token index 0 -- protocol identity.
  //   This NFT MUST be preserved in any spending transaction.
  //
  // Spending Conditions:
  //   1. At least `threshold` of `committeeSize` members must authorize.
  //   2. The output carrying the Xergon Network NFT must also carry the
  //      same ErgoTree (scriptPreserved), preventing NFT hijacking.
  //
  // Deployment:
  //   Replace committee members below with real Ergo addresses.
  //   Adjust threshold (k) for desired security level.
  //   Recommended: 3-of-5 or 4-of-7 for production.
  //
  // Migration from v1:
  //   The v1 deployer creates a transaction spending the v1 treasury box,
  //   sending the NFT to an output with this v2 script. This is a one-time
  //   migration that requires the v1 deployer's signature.
  //
  // =========================================================================

  // Committee members (replace with real Ergo addresses)
  // These are the authorized signers for treasury operations.
  val member1 = PK("COMMITTEE_MEMBER_1_ADDRESS")
  val member2 = PK("COMMITTEE_MEMBER_2_ADDRESS")
  val member3 = PK("COMMITTEE_MEMBER_3_ADDRESS")
  val member4 = PK("COMMITTEE_MEMBER_4_ADDRESS")
  val member5 = PK("COMMITTEE_MEMBER_5_ADDRESS")

  // Committee key collection and threshold
  val committeeKeys = Coll(member1, member2, member3, member4, member5)
  val threshold = 3  // 3-of-5 threshold

  // Identify the Xergon Network NFT (token at index 0)
  val nftId = SELF.tokens(0)._1

  // Verify that the output carrying the Xergon Network NFT also preserves
  // the same ErgoTree AND the NFT amount is exactly 1.
  // This prevents NFT hijacking AND NFT splitting.
  val outPreservesNftAndScript = OUTPUTS.exists { (b: Box) =>
    b.tokens.size > 0 &&
    b.tokens(0)._1 == nftId &&
    b.tokens(0)._2 == 1L &&
    b.propositionBytes == SELF.propositionBytes
  }

  // Final guard: multi-sig threshold + NFT + script preservation
  atLeast(threshold, committeeKeys) && outPreservesNftAndScript
}