{
  // Xergon Payment Bridge — Invoice-based Lock-and-Mint
  //
  // An invoice box representing a cross-chain payment request.
  // The buyer locks ERG, pays on a foreign chain (BTC/ETH/ADA),
  // and the bridge operator confirms and releases payment to the provider.
  //
  // Registers:
  //   R4: buyerPK        (SigmaProp) — buyer who can refund after timeout
  //   R5: providerPK     (SigmaProp) — provider who receives payment
  //   R6: amountNanoerg  (Long)      — payment amount in nanoERG
  //   R7: foreignTxId    (Coll[Byte])— tx ID on source chain (set on confirm)
  //   R8: foreignChain   (Int)       — 0=BTC, 1=ETH, 2=ADA
  //   R9: bridgePK       (SigmaProp) — bridge operator key
  //   tokens(0): invoice NFT (singleton, minted via EIP-4)
  //
  // Spending paths:
  //   1. Bridge operator (after confirming foreign-chain payment)
  //      - Must pay provider >= amountNanoerg
  //      - Invoice NFT is consumed (burned) on this path — acceptable
  //        since the bridge completion is terminal
  //   2. Buyer refund (after timeout, if bridge hasn't confirmed)
  //
  // Security Notes:
  //   - [MEDIUM FIXED] Bridge path now enforces provider payment.
  //     Previously bridgePK alone was sufficient, allowing the operator
  //     to claim the ERG without paying the provider.
  //   - [INFO] foreignTxId and foreignChain are stored but not validated
  //     on-chain. The bridge operator is trusted to verify the foreign-chain
  //     payment before spending. Off-chain monitoring SHOULD verify that
  //     bridge operators act honestly.
  //   - [INFO] Invoice NFT is intentionally NOT preserved on the bridge path
  //     (it's a one-time payment). The refund path also burns the NFT.
  //   - Storage rent: the timeout (720 blocks ~= 24h) ensures the box
  //     won't linger indefinitely if the bridge operator is unresponsive.

  val buyerPK = SELF.R4[SigmaProp].get
  val providerPK = SELF.R5[SigmaProp].get
  val amountNanoerg = SELF.R6[Long].get
  val foreignTxId = SELF.R7[Coll[Byte]].get
  val foreignChain = SELF.R8[Int].get
  val bridgePK = SELF.R9[SigmaProp].get
  val invoiceNFT = SELF.tokens(0)._1

  // Bridge operator path: confirmed foreign-chain payment.
  // SECURITY: Provider must receive >= amountNanoerg to prevent
  // the bridge operator from claiming the escrowed ERG.
  val bridgePath = {
    val providerPaid = OUTPUTS.exists { (out: Box) =>
      out.propositionBytes == providerPK.propBytes &&
      out.value >= amountNanoerg
    }
    bridgePK && providerPaid
  }

  // Refund path: buyer can reclaim after timeout (~24 hours at 2-min blocks)
  val refundPath = {
    val timeout = SELF.creationHeight + 720
    HEIGHT >= timeout && buyerPK
  }

  sigmaProp(bridgePath || refundPath)
}
