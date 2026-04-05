{
  // =========================================================================
  // Xergon Network -- Treasury Box Guard Script
  // =========================================================================
  //
  // Holds the protocol treasury and the Xergon Network NFT (supply=1).
  // Only the protocol deployer can spend this box, which is used for:
  //   - Airdropping XGN tokens to early adopters
  //   - Funding protocol development
  //   - Managing protocol-level ERG reserves
  //
  // Register Layout (EIP-4):
  //   R4: Total airdropped amount (Long) -- cumulative nanoERGs distributed
  //
  // Token:
  //   Xergon Network NFT (supply=1) at token index 0 -- protocol identity.
  //   This NFT MUST be preserved in any spending transaction.
  //
  // Spending Conditions:
  //   1. Only the deployer (proveDlog of deployerPk) can spend.
  //   2. The output carrying the Xergon Network NFT must also carry the
  //      same ErgoTree (scriptPreserved), preventing NFT hijacking.
  //
  // IMPORTANT: The deployer address below is a PLACEHOLDER.
  // Replace "DEPLOYER_ADDRESS_HERE" with the actual Ergo address
  // of the protocol deployer before compiling this contract.
  //
  // *** DEPLOYMENT SAFETY CHECK ***
  // The placeholder string below will cause a compilation error if
  // deployed as-is. If you see a "cannot resolve PK" error, you
  // have not replaced the placeholder with a real Ergo address.
  //
  // Example valid addresses:
  //   Mainnet: "3WvsT2Gm4EpsM9Pg18PdY6XyhNNMqDsgv2e"
  //   Testnet: "3WwxnK... (testnet address)"
  //
  // Security Notes:
  //   - [INFO] Single-key deployer control is a centralization risk.
  //     Consider migrating to a multi-sig committee for production.
  //   - [INFO] The NFT + script preservation check prevents the deployer
  //     from moving the protocol NFT to a weaker guard script.
  //   - Storage rent: the deployer must ensure the treasury box stays
  //     funded above minimum box value to prevent NFT loss.
  //
  // =========================================================================

  // Deployer public key -- REPLACE WITH ACTUAL DEPLOYER ADDRESS BEFORE DEPLOYMENT
  // >>> DEPLOYMENT WILL FAIL WITH PLACEHOLDER — MUST SET REAL ADDRESS <<<
  val deployerPk = PK("DEPLOYER_ADDRESS_HERE")

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

  // Final guard: deployer authorization + NFT + script preservation
  proveDlog(deployerPk) && outPreservesNftAndScript
}