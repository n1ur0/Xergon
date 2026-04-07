{
  // =========================================================================
  // Xergon Network -- GPU Rating Contract (Reputation System)
  // =========================================================================
  //
  // A lightweight on-chain rating box for the GPU Bazar reputation system.
  // Created when a rental completes (after deadline height). Either party
  // (provider or renter) can submit a rating for the other.
  //
  // The rating box is immutable once created -- only the rater can spend it
  // (to update their rating). The relay reads rating boxes from the UTXO set
  // to compute aggregated reputation scores off-chain.
  //
  // Register Layout (EIP-4):
  //   R4: Rater public key     (GroupElement) -- who submitted this rating
  //   R5: Rated public key     (GroupElement) -- who is being rated
  //   R6: Role                 (Coll[Byte])    -- "provider" or "renter"
  //   R7: Rental box ID        (Coll[Byte])    -- links to the specific rental
  //   R8: Rating               (Int)           -- 1-5 stars
  //   R9: Comment hash         (Coll[Byte])    -- blake2b256 of optional comment
  //
  // Spending path:
  //   - Only the rater (R4) can spend (to update their rating)
  //
  // Security Notes:
  //   - [LOW] rentalBoxId (R7) is stored but NOT verified on-chain to
  //     point to an actual rental box. An attacker could submit ratings
  //     for non-existent rentals. This is acceptable because:
  //     (a) ratings are advisory — the relay reads them off-chain and
  //         filters garbage/invalid ratings during aggregation.
  //     (b) on-chain verification would require a data-input check
  //         against the rental contract, adding complexity and cost.
  //     To fully fix: the spending tx should include the rental box as
  //     a data input and verify its ErgoTree matches the rental contract
  //     and its R7 (listingBoxId) is valid.
  //   - Storage rent: rating boxes are small (registers only) so the
  //     minimum box value covers rent for years. No special handling needed.
  //
  // =========================================================================

  // Who submitted this rating
  val raterPK = SELF.R4[GroupElement].get

  // Who is being rated (provider or renter)
  val ratedPK = SELF.R5[GroupElement].get

  // The rental box ID this rating is for
  val rentalBoxId = SELF.R7[Coll[Byte]].get

  // Rating: 1-5 stars stored as Int in R8
  val rating = SELF.R8[Int].get

  // Validate rating is in range 1-5
  val validRating = rating >= 1 && rating <= 5

  // If updating, output must also be a valid rating box
  val outputValid = OUTPUTS.forall { (out: Box) =>
    val hasRater = out.R4[GroupElement].isDefined
    // If the output has R4 (rater), it must be a valid rating box
    !hasRater ||
      (out.R5[GroupElement].isDefined &&
       out.R5[GroupElement].get == ratedPK &&
       out.R7[Coll[Byte]].isDefined &&
       out.R7[Coll[Byte]].get == rentalBoxId &&
       out.R8[Int].isDefined &&
       out.R8[Int].get >= 1 &&
       out.R8[Int].get <= 5)
  }

  // Only the rater can spend (update) their rating
  proveDlog(raterPK) && validRating && outputValid
}
