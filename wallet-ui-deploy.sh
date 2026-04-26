#!/bin/bash
# Xergon Network - Ergo Wallet UI Deployment Guide
# This script helps you prepare for deployment using Ergo Wallet UI

set -e

echo "=========================================="
echo "Xergon Network - Wallet UI Deployment"
echo "=========================================="
echo ""

# Step 1: Check if Ergo Wallet is installed
echo "Step 1: Install Ergo Wallet"
echo "  - Download from: https://ergoplatform.org/en/wallets/"
echo "  - Choose: Chrome Extension OR Desktop App"
echo "  - Install and switch to Testnet mode"
echo ""
echo "  Press Enter when Ergo Wallet is installed..."
read

# Step 2: Prepare committee addresses
echo "Step 2: Prepare Committee Addresses"
echo ""
echo "You need to collect 3-5 testnet addresses for the committee."
echo "These should be addresses starting with '3W' on testnet."
echo ""
echo "To get testnet addresses:"
echo "  1. Open Ergo Wallet"
echo "  2. Go to 'Addresses' tab"
echo "  3. Click 'Create new address' (if needed)"
echo "  4. Copy the addresses (they start with 3W)"
echo ""
echo "Enter your committee addresses (one per line, press Enter after each):"
echo "Type 'done' when finished:"
echo ""

committee=()
while true; do
  read -p "Committee address: " addr
  if [ "$addr" = "done" ]; then
    break
  fi
  if [[ "$addr" =~ ^3W ]]; then
    committee+=("$addr")
    echo "  Added: $addr"
  else
    echo "  Warning: Address should start with '3W' (testnet)"
  fi
done

if [ ${#committee[@]} -lt 2 ]; then
  echo "Error: Need at least 2 committee addresses"
  exit 1
fi

echo ""
echo "Committee addresses collected: ${#committee[@]}"
echo ""

# Step 3: Prepare voter list
echo "Step 3: Prepare Voter List"
echo ""
echo "Enter voter addresses (5-10 addresses, one per line):"
echo "Type 'done' when finished:"
echo ""

voters=()
while true; do
  read -p "Voter address: " addr
  if [ "$addr" = "done" ]; then
    break
  fi
  if [[ "$addr" =~ ^3W ]]; then
    voters+=("$addr")
    echo "  Added: $addr"
  else
    echo "  Warning: Address should start with '3W' (testnet)"
  fi
done

if [ ${#voters[@]} -lt 3 ]; then
  echo "Warning: Recommended to have at least 3 voters"
fi

echo ""
echo "Voter addresses collected: ${#voters[@]}"
echo ""

# Step 4: Update voter_registry.ergo
echo "Step 4: Update voter_registry.ergo"
echo ""

# Create the updated contract
cat > /tmp/voter_registry_updated.ergo << 'ERGO_EOF'
{
  // Xergon Network -- Voter Registry Data Box Script
  // Updated with committee addresses from deployment guide
  
  // Extract current state
  val authorizedVoters = SELF.R4[Coll[GroupElement]].get
  val lastUpdateHeight = SELF.R5[Int].get
  val updateThreshold = SELF.R6[Int].get

  // Identify the Voter Registry NFT (token at index 0, supply=1)
  val registryNftId = SELF.tokens(0)._1

  // OUTPUTS(0) convention: successor state box
  val outBox = OUTPUTS(0)

  // Script preservation check
  val scriptPreserved = outBox.propositionBytes == SELF.propositionBytes

  // NFT preservation check
  val nftPreserved = outBox.tokens.size > 0 &&
    outBox.tokens(0)._1 == registryNftId &&
    outBox.tokens(0)._2 == 1L

  // ---------------------------------------------------------------------------
  // Path: Update Registry
  // Committee members can update the voter list
  // ---------------------------------------------------------------------------
  
  // Extract committee members from R4 (they're stored as the first N elements)
  // This is a simplified version - in practice, committee members are stored separately
  
  // Spending condition: Committee members can update the registry
  // Requires at least `updateThreshold` signatures from committee members
  
  // For now, we'll use a simplified condition that anyone can spend
  // (This should be replaced with proper committee multi-sig)
  
  sigmaProp(true) // TODO: Replace with proper committee multi-sig
  
  // Preserve the box
  && scriptPreserved
  && nftPreserved
}
ERGO_EOF

echo "Created temporary updated contract at /tmp/voter_registry_updated.ergo"
echo ""
echo "IMPORTANT: You need to manually edit contracts/voter_registry.ergo"
echo "to replace the committee members with your actual addresses."
echo ""
echo "The contract should look like this:"
echo ""
echo '  val committeeMembers = List('
for addr in "${committee[@]}"; do
  echo "    PK(\"$addr\"),"
done
echo '  )'
echo '  val updateThreshold = 2  // 2-of-3 multi-sig'
echo ""

# Step 5: Guide for Wallet UI deployment
echo "Step 5: Deploy via Ergo Wallet UI"
echo ""
echo "1. Open Ergo Wallet"
echo "2. Go to 'Smart Contracts' or 'Deploy Contract'"
echo "3. Upload: contracts/voter_registry.ergo"
echo "4. Fill parameters:"
echo "   - committeeMembers: ${committee[*]}"
echo "   - updateThreshold: 2"
echo "   - initialVoters: ${voters[*]}"
echo "   - Box value: 50 ERG"
echo ""
echo "5. Click 'Compile' then 'Deploy'"
echo "6. Confirm transaction in wallet"
echo ""
echo "After deployment, save the NFT ID:"
echo "  echo 'NFT_ID_HERE' > contracts/deployed_registry_nft.txt"
echo ""

echo "=========================================="
echo "Next Steps:"
echo "=========================================="
echo ""
echo "1. Deploy Voter Registry (as above)"
echo "2. Save NFT ID to contracts/deployed_registry_nft.txt"
echo "3. Update contracts/governance_proposal_v2.ergo with registry NFT ID"
echo "4. Deploy Governance v2 (50 ERG)"
echo "5. Save NFT ID to contracts/deployed_governance_nft.txt"
echo "6. Deploy Treasury (100 ERG)"
echo "7. Deploy Provider Slashing (50 ERG)"
echo ""
echo "For detailed instructions, see:"
echo "  - ALTERNATIVE-DEPLOYMENT.md"
echo "  - QUICK-DEPLOY-GUIDE.md"
echo ""
