#!/bin/bash
# Manual deployment guide for Xergon Network testnet
# Use this if you don't have Ergo compiler tools installed

set -e

# Configuration
DEPLOYER_ADDRESS="3Wvjqkyee4VDXqSVAsx29ohaomS8HgUabvZ8yoasVaQQwsYBThqj"
TREASURY_ADDRESS="3WzAsN3gvwuQNyKG8cSKvTEvyU6pvDqJGx87BYqF7EWmpxntgrc1"
ERGO_NODE_URL="${ERGO_NODE_URL:-http://192.168.1.75:9052}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

echo "=== Xergon Network - Manual Testnet Deployment Guide ==="
echo ""
log_info "This guide helps you deploy contracts manually using available tools."
echo ""

# Check Ergo node
log_info "Checking Ergo node..."
node_info=$(curl -s "$ERGO_NODE_URL/info" 2>/dev/null || echo "")
if [ -z "$node_info" ]; then
    log_error "Cannot connect to Ergo node at $ERGO_NODE_URL"
    log_info "Make sure your local Ergo node is running"
    exit 1
fi

echo "$node_info" | jq -r '.height, .stateVersion, .network' 2>/dev/null || echo "$node_info"
log_info "Ergo node is running"

# Check deployer funding
log_info "Checking deployer funding..."
balance=$(curl -s "$ERGO_NODE_URL/boxes/unspent/toAddress/$DEPLOYER_ADDRESS" 2>/dev/null | \
    jq '[.[].value] | add // 0' 2>/dev/null || echo "0")

balance_erg=$((balance / 1000000000))
echo "Deployer balance: $balance_erg ERG"

if [ "$balance_erg" -lt 10 ]; then
    log_warn "Deployer has less than 10 ERG. Recommend funding with at least 10 ERG for deployment."
    log_info "Get testnet ERG from: https://ergoplatform.org/en/testnet-faucet/"
else
    log_info "Sufficient funds available"
fi

echo ""
log_info "=== Next Steps ==="
echo ""
echo "You have two options for deployment:"
echo ""
echo "OPTION 1: Use Ergo Wallet UI (Recommended for beginners)"
echo "  1. Install Ergo Wallet: https://ergoplatform.org/en/wallets/"
echo "  2. Connect to testnet"
echo "  3. Import your deployer address (if needed)"
echo "  4. Use the wallet's 'Deploy Contract' feature"
echo "  5. For each contract:"
echo "     - Load the .ergo file"
echo "     - Compile to ErgoTree"
echo "     - Deploy with initial parameters"
echo ""
echo "OPTION 2: Use ergo-appkit (for developers)"
echo "  1. Install Node.js (v16+)"
echo "  2. Install ergo-appkit: npm install ergo-appkit"
echo "  3. Use the example scripts to compile and deploy"
echo "  4. See: https://github.com/ergoplatform/ergo-appkit-js"
echo ""
echo "OPTION 3: Use SigmaJS (alternative)"
echo "  1. Install: npm install @ergoplatform/sigma-js"
echo "  2. Use SigmaJS to compile contracts"
echo "  3. Deploy via ergo-node API"
echo ""
echo "=== Contract Compilation (Manual) ==="
echo ""
echo "If you have access to an Ergo compiler, run:"
echo ""
echo "  ergo-compiler compile contracts/voter_registry.ergo -o contracts/voter_registry.ergotree"
echo "  ergo-compiler compile contracts/governance_proposal_v2.ergo -o contracts/governance_proposal_v2.ergotree"
echo "  ergo-compiler compile contracts/treasury.ergo -o contracts/treasury.ergotree"
echo "  ergo-compiler compile contracts/provider_slashing.ergo -o contracts/provider_slashing.ergotree"
echo "  ergo-compiler compile contracts/user_staking.ergo -o contracts/user_staking.ergotree"
echo "  ergo-compiler compile contracts/provider_box.ergo -o contracts/provider_box.ergotree"
echo "  ergo-compiler compile contracts/usage_proof.ergo -o contracts/usage_proof.ergotree"
echo ""
echo "=== Deployment Order ==="
echo ""
echo "1. Voter Registry (with committee members)"
echo "   - Set committee addresses in contract"
echo "   - Deploy with initial voter list"
echo "   - Record NFT ID"
echo ""
echo "2. Governance v2 (with Voter Registry NFT ID)"
echo "   - Use registry NFT ID from step 1"
echo "   - Deploy with initial threshold"
echo "   - Record NFT ID"
echo ""
echo "3. Treasury"
echo "   - Already has deployer address set"
echo "   - Deploy with initial ERG"
echo ""
echo "4. Provider Slashing"
echo "   - Already has treasury address set"
echo "   - Deploy with initial parameters"
echo ""
echo "5. User Staking & Provider Box (per-user/provider)"
echo "   - These are instantiated per user/provider"
echo "   - Deploy template, then create instances"
echo ""
echo "6. Usage Proof"
echo "   - Deploy template for usage recording"
echo ""
echo "=== Verification ==="
echo ""
echo "After deployment, verify with:"
echo ""
echo "  curl -s '$ERGO_NODE_URL/boxes/filter' -X POST -H 'Content-Type: application/json' \\"
echo "    -d '{\"condition\": \"contains(boxes.tokens, {\\\"id\\\":\\\"YOUR_NFT_ID\\\"})\"}'"
echo ""
echo "=== Testing Checklist ==="
echo ""
echo "  [ ] Voter registry can be queried"
echo "  [ ] Authorized voters can create proposals"
echo "  [ ] Unauthorized users cannot create proposals"
echo "  [ ] Governance NFT preserved in transactions"
echo "  [ ] Treasury can receive funds"
echo "  [ ] Provider registration works"
echo "  [ ] User staking works"
echo ""
echo "=== Resources ==="
echo ""
echo "Ergo Documentation: https://docs.ergoplatform.org/"
echo "Ergo Testnet Faucet: https://ergoplatform.org/en/testnet-faucet/"
echo "Ergo Wallet: https://ergoplatform.org/en/wallets/"
echo "Ergo AppKit: https://github.com/ergoplatform/ergo-appkit"
echo ""
echo "For detailed deployment steps, see: TESTNET_DEPLOYMENT_CHECKLIST.md"
echo "For security fixes, see: CRITICAL_FIXES_IMPLEMENTATION.md"
echo ""
log_info "Your contracts are ready at: contracts/*.ergo"
echo ""
