{
  // Provider Registration Contract
  // Spends ERG + creates a Provider Box with a new NFT.
  // The NFT ID = INPUTS(0).id (Ergo's token creation rule).
  
  val newNFTId = INPUTS(0).id
  val providerOutput = OUTPUTS(0)
  
  // Output must contain our newly minted NFT
  val hasNFT = providerOutput.tokens.exists { (t: (Coll[Byte], Long)) =>
    t._1 == newNFTId && t._2 == 1L
  }
  
  // Output must have minimum value
  // NOTE: 1000000L (1 milliERG) is a conservative hardcoded floor.
  // SAFE_MIN_BOX_VALUE is dynamic based on box size (register data + tokens),
  // but 1 milliERG covers any reasonable provider box. If the box grows
  // significantly (e.g., more registers), this value should be revisited.
  // No signature requirement: registration is intentionally open so anyone
  // can become a provider by creating a valid registration box.
  val hasMinValue = providerOutput.value >= 1000000L
  
  // Output must have R4 (provider PK)
  val hasPK = providerOutput.R4[GroupElement].isDefined
  
  sigmaProp(hasNFT && hasMinValue && hasPK)
}
