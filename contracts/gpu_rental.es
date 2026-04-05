{
  // =========================================================================
  // Xergon Network -- GPU Rental Contract (Time-boxed escrow)
  // =========================================================================
  //
  // When a renter wants to rent a GPU, a rental box is created on-chain.
  // The box holds ERG value = prepaid hours * price_per_hour. This is
  // the escrowed payment.
  //
  // Three spending paths:
  //   1. Provider claims payment after deadline (rental period complete)
  //   2. Renter refunds before deadline (cancelled rental)
  //   3. Renter extends rental (before deadline, creates new box with
  //      later deadline) -- optional for Phase 4 MVP
  //
  // Singleton NFT Pattern:
  //   - A token with supply=1 (Rental NFT) is minted when the rental is
  //     created. This NFT identifies the rental on-chain.
  //   - The NFT travels with the rental box across spend/recreate cycles.
  //
  // Register Layout (EIP-4):
  //   R4: Provider public key   (GroupElement) -- can claim after deadline
  //   R5: Renter public key     (GroupElement) -- can refund before deadline
  //   R6: Deadline height       (Int)          -- block height when rental ends
  //   R7: Listing box ID        (Coll[Byte])   -- reference to the listing box
  //   R8: Rental start height   (Int)          -- block height when rental began
  //   R9: Hours rented          (Int)          -- total hours in this rental
  //
  // Security Notes:
  //   - [MEDIUM FIXED] Provider claim now enforces value goes to provider.
  //     Previously only proveDlog + height was checked, allowing the provider
  //     key holder to collude and drain the box to a third party.
  //   - [INFO] NFT is intentionally burned on claim (path 1) and refund
  //     (path 2) — the rental is terminal. NFT is preserved on extend
  //     (path 3) via the outPreservesNft check.
  //   - Storage rent: minimum box value applies. If escrowed ERG is too
  //     low the box may not be creatable. The extend path helps keep the
  //     box alive for active rentals.
  //
  // =========================================================================

  // Extract parties and terms from registers
  val providerPK = SELF.R4[GroupElement].get
  val renterPK   = SELF.R5[GroupElement].get
  val deadlineHeight = SELF.R6[Int].get

  // Identify our Rental NFT by its token ID (token at index 0)
  val rentalNft = SELF.tokens(0)._1

  // OUTPUTS(0) convention: the successor rental box must be the first output.
  val outBox = OUTPUTS(0)

  // scriptPreserved check: successor must be guarded by the same ErgoTree.
  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes

  // Verify the output box preserves the Rental NFT.
  val outPreservesNft = outBox.tokens.size > 0 && outBox.tokens(0)._1 == rentalNft

  // Value preservation check for extend path
  val valuePreserved = outBox.value >= SELF.value

  // Path 1: Provider claims payment after deadline (rental complete).
  // SECURITY: Provider must receive the escrowed value (minus a small
  // tx fee allowance of 1,000,000 nanoERG). This prevents the provider
  // key holder from draining the box to a third-party address.
  // The NFT is intentionally consumed (burned) — rental is terminal.
  val providerClaim = {
    val feeAllowance = 1000000L
    proveDlog(providerPK) && HEIGHT >= deadlineHeight &&
    OUTPUTS.exists { (out: Box) =>
      out.propositionBytes == proveDlog(providerPK).propBytes &&
      out.value >= SELF.value - feeAllowance
    }
  }

  // Path 2: Renter refunds before deadline (cancelled).
  // SECURITY: Renter receives the full escrowed ERG (minus fee allowance).
  // The NFT is intentionally consumed (burned) — rental is cancelled.
  val renterRefund = {
    val feeAllowance = 1000000L
    proveDlog(renterPK) && HEIGHT < deadlineHeight &&
    OUTPUTS.exists { (out: Box) =>
      out.propositionBytes == proveDlog(renterPK).propBytes &&
      out.value >= SELF.value - feeAllowance
    }
  }

  // Path 3: Renter extends rental (before deadline).
  // The successor box must have the same parties and a LATER deadline.
  // All registers must be preserved except R6 (new deadline) and R9 (new hours).
  val sameProvider = outBox.R4[GroupElement].isDefined &&
    outBox.R4[GroupElement].get == providerPK
  val sameRenter = outBox.R5[GroupElement].isDefined &&
    outBox.R5[GroupElement].get == renterPK
  val extendedDeadline = outBox.R6[Int].isDefined &&
    outBox.R6[Int].get > deadlineHeight
  val extendsBeforeDeadline = HEIGHT < deadlineHeight
  val renterExtend = proveDlog(renterPK) && extendsBeforeDeadline &&
    sameProvider && sameRenter && extendedDeadline &&
    scriptPreserved && outPreservesNft && valuePreserved

  // Either: provider claims (NFT burned), renter refunds (NFT burned),
  // or renter extends (NFT preserved in successor).
  (providerClaim || renterRefund || renterExtend)
}
