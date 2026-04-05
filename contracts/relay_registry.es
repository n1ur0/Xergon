{
  // Relay Registry Contract
  //
  // On-chain box that relays register themselves into.
  // Each relay creates one box per registration with a unique NFT (singleton pattern).
  //
  // Register layout:
  //   R4: SigmaProp — relay owner's public key (spend guard)
  //   R5: Coll[Byte] — relay endpoint URL (UTF-8 encoded string)
  //   R6: Int — last heartbeat timestamp (epoch seconds)
  //   Tokens(0): relay NFT (singleton, 1 supply)
  //
  // Spend conditions:
  //   1. Only the relay owner (R4 PK) can spend/update this box
  //   2. Output must preserve the relay NFT token
  //
  // Security Notes:
  //   - [LOW] lastHeartbeat (R6) is stored but NOT validated for freshness
  //     on-chain. A relay could update its registration with a stale timestamp.
  //     This is acceptable because the relay off-chain logic filters stale
  //     relays (e.g., heartbeat older than N minutes) when building the
  //     active relay set for request routing.
  //   - [INFO] The NFT preservation check prevents unauthorized NFT theft
  //     but does not prevent the relay owner from moving the NFT to a box
  //     with different metadata (e.g., fake endpoint URL). Off-chain
  //     validation SHOULD verify endpoint reachability.
  //   - Storage rent: relay owners must keep their box funded to avoid
  //     storage-rent cleanup. The NFT preservation ensures cleanup doesn't
  //     leave orphaned NFTs in other boxes.
  
  val relayPK = SELF.R4[SigmaProp].get
  val relayEndpoint = SELF.R5[Coll[Byte]].get
  val lastHeartbeat = SELF.R6[Int].get
  val relayNFT = SELF.tokens(0)._1
  
  // Only relay owner can update (spend guard)
  val canSpend = relayPK
  
  // Output must preserve the NFT with exact amount AND same script.
  val preservesNFT = OUTPUTS.exists { (out: Box) =>
    out.tokens.exists { (t: (Coll[Byte], Long)) =>
      t._1 == relayNFT &&
      t._2 == 1L &&
      out.propositionBytes == SELF.propositionBytes
    }
  }
  
  canSpend && preservesNFT
}
