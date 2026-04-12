#!/bin/bash
# Xergon Network - Testnet Deployment Script
# This script deploys all contracts to Ergo testnet

set -e

# Configuration
DEPLOYER_ADDRESS="3Wvjqkyee4VDXqSVAsx29ohaomS8HgUabvZ8yoasVaQQwsYBThqj"
TREASURY_ADDRESS="3WzAsN3gvwuQNyKG8cSKvTEvyU6pvDqJGx87BYqF7EWmpxntgrc1"
ERGO_NODE_URL="${ERGO_NODE_URL:-http://192.168.1.75:9052}"
MIN_FUND_AMOUNT=10000000000  # 10 ERG in nanoERG

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check dependencies
check_dependencies() {
    log_info "Checking dependencies..."
    
    if ! command -v ergo-compiler &> /dev/null; then
        log_error "ergo-compiler not found. Install from: https://github.com/ergoplatform/ergo-appkit"
        exit 1
    fi
    
    if ! command -v ergo-wallet &> /dev/null; then
        log_error "ergo-wallet not found"
        exit 1
    fi
    
    log_info "All dependencies found"
}

# Check deployer funding
check_funding() {
    log_info "Checking deployer funding..."
    
    balance=$(curl -s "$ERGO_NODE_URL/boxes/unspent/toAddress/$DEPLOYER_ADDRESS?minValue=1" | jq '.[0].value // 0')
    
    if [ "$balance" -lt "$MIN_FUND_AMOUNT" ]; then
        log_error "Deployer address has insufficient funds: $balance nanoERG (need $MIN_FUND_AMOUNT)"
        log_info "Fund the address with at least 10 ERG on testnet"
        exit 1
    fi
    
    log_info "Deployer has $((balance / 1000000000)) ERG - sufficient for deployment"
}

# Compile contract
compile_contract() {
    local contract_file=$1
    local output_file=$2
    
    log_info "Compiling $contract_file..."
    
    if [ ! -f "$contract_file" ]; then
        log_error "Contract file not found: $contract_file"
        exit 1
    fi
    
    # Compile to ErgoTree
    ergo-compiler compile "$contract_file" -o "$output_file"
    
    if [ $? -eq 0 ]; then
        log_info "Compiled successfully: $output_file"
    else
        log_error "Compilation failed for $contract_file"
        exit 1
    fi
}

# Deploy contract
deploy_contract() {
    local contract_file=$1
    local contract_name=$2
    local initial_value=${3:-10000000}  # Default 0.01 ERG
    
    log_info "Deploying $contract_name..."
    
    # Get contract ErgoTree
    ergo_tree=$(cat "$contract_file" | jq -r '.ergoTree')
    
    # Create deployment transaction
    # Note: This is a simplified example - adjust based on your wallet setup
    tx_id=$(ergo-wallet send --to "$ergo_tree" --amount "$initial_value" \
        --data "$contract_file" --note "Deploying $contract_name")
    
    if [ $? -eq 0 ]; then
        log_info "Deployed $contract_name - TX: $tx_id"
        echo "$tx_id"
    else
        log_error "Failed to deploy $contract_name"
        exit 1
    fi
}

# Deploy Voter Registry
deploy_voter_registry() {
    log_info "=== Deploying Voter Registry ==="
    
    # Compile voter registry
    compile_contract "contracts/voter_registry.ergo" "contracts/voter_registry.ergotree"
    
    # TODO: Add committee member addresses
    # These should be replaced with actual committee member addresses
    COMMITTEE_MEMBERS=(
        "3Wvq...committee1"  # Replace with actual address
        "3Wvq...committee2"  # Replace with actual address
        "3Wvq...committee3"  # Replace with actual address
    )
    
    # Initialize voter list (replace with actual voter addresses)
    INITIAL_VOTERS=(
        "3Wvq...voter1"  # Replace with actual address
        "3Wvq...voter2"  # Replace with actual address
    )
    
    # Deploy with initial state
    REGISTRY_NFT_ID=$(deploy_contract "contracts/voter_registry.ergotree" "Voter Registry" 50000000)
    
    log_info "Voter Registry deployed - NFT ID: $REGISTRY_NFT_ID"
    echo "$REGISTRY_NFT_ID" > contracts/deployed_registry_nft.txt
    
    return 0
}

# Deploy Governance v2
deploy_governance_v2() {
    log_info "=== Deploying Governance v2 ==="
    
    # Check if registry NFT ID exists
    if [ ! -f "contracts/deployed_registry_nft.txt" ]; then
        log_error "Voter Registry not deployed yet. Run deploy_voter_registry first."
        exit 1
    fi
    
    REGISTRY_NFT_ID=$(cat contracts/deployed_registry_nft.txt)
    log_info "Using Voter Registry NFT ID: $REGISTRY_NFT_ID"
    
    # Compile governance v2
    compile_contract "contracts/governance_proposal_v2.ergo" "contracts/governance_proposal_v2.ergotree"
    
    # Update governance contract with registry NFT ID
    # This requires modifying the contract before compilation or using a deployment script
    sed "s/val voterRegistryNftId = SELF.R10\[Coll[Byte]\].get/val voterRegistryNftId = hex\"${REGISTRY_NFT_ID}\"/" \
        contracts/governance_proposal_v2.ergo > contracts/governance_proposal_v2_deployed.ergo
    
    # Deploy
    GOV_NFT_ID=$(deploy_contract "contracts/governance_proposal_v2_deployed.ergotree" "Governance v2" 50000000)
    
    log_info "Governance v2 deployed - NFT ID: $GOV_NFT_ID"
    echo "$GOV_NFT_ID" > contracts/deployed_governance_nft.txt
    
    return 0
}

# Deploy User Staking
deploy_user_staking() {
    log_info "=== Deploying User Staking Contract ==="
    compile_contract "contracts/user_staking.ergo" "contracts/user_staking.ergotree"
    # Note: User staking is instantiated per user, not deployed as a single contract
    log_info "User staking contract template compiled"
}

# Deploy Provider Box
deploy_provider_box() {
    log_info "=== Deploying Provider Box Contract ==="
    compile_contract "contracts/provider_box.ergo" "contracts/provider_box.ergotree"
    # Note: Provider boxes are instantiated per provider
    log_info "Provider box contract template compiled"
}

# Deploy Treasury
deploy_treasury() {
    log_info "=== Deploying Treasury Contract ==="
    compile_contract "contracts/treasury.ergo" "contracts/treasury.ergotree"
    
    # Treasury should already be deployed with the deployer address
    # Verify deployment
    log_info "Treasury contract compiled - ready for deployment"
}

# Deploy Provider Slashing
deploy_provider_slashing() {
    log_info "=== Deploying Provider Slashing Contract ==="
    compile_contract "contracts/provider_slashing.ergo" "contracts/provider_slashing.ergotree"
    log_info "Provider slashing contract compiled - ready for deployment"
}

# Deploy Usage Proof
deploy_usage_proof() {
    log_info "=== Deploying Usage Proof Contract ==="
    compile_contract "contracts/usage_proof.ergo" "contracts/usage_proof.ergotree"
    log_info "Usage proof contract compiled - ready for deployment"
}

# Verify deployments
verify_deployments() {
    log_info "=== Verifying Deployments ==="
    
    # Check if all NFTs are present
    local required_nfts=(
        "contracts/deployed_registry_nft.txt"
        "contracts/deployed_governance_nft.txt"
    )
    
    for nft_file in "${required_nfts[@]}"; do
        if [ -f "$nft_file" ]; then
            nft_id=$(cat "$nft_file")
            log_info "NFT verified: $nft_id"
        else
            log_warn "NFT file not found: $nft_file"
        fi
    done
    
    log_info "Deployment verification complete"
}

# Main deployment function
main() {
    log_info "=== Xergon Network Testnet Deployment ==="
    log_info "Node URL: $ERGO_NODE_URL"
    log_info "Deployer: $DEPLOYER_ADDRESS"
    
    check_dependencies
    check_funding
    
    # Deploy in order
    deploy_voter_registry
    deploy_governance_v2
    deploy_user_staking
    deploy_provider_box
    deploy_treasury
    deploy_provider_slashing
    deploy_usage_proof
    
    verify_deployments
    
    log_info "=== Deployment Complete ==="
    log_info "Next steps:"
    log_info "1. Fund contracts with initial ERG if needed"
    log_info "2. Test governance flows with voter registry"
    log_info "3. Verify unauthorized users cannot act"
    log_info "4. Run integration tests"
}

# Parse arguments
case "${1:-deploy}" in
    deploy)
        main
        ;;
    compile-only)
        check_dependencies
        for contract in voter_registry governance_proposal_v2 user_staking provider_box treasury provider_slashing usage_proof; do
            compile_contract "contracts/${contract}.ergo" "contracts/${contract}.ergotree"
        done
        log_info "All contracts compiled"
        ;;
    verify)
        verify_deployments
        ;;
    *)
        echo "Usage: $0 {deploy|compile-only|verify}"
        exit 1
        ;;
esac
