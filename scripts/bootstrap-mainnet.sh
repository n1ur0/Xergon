#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Xergon Network -- Mainnet Bootstrap Script
#
# Deploys the Xergon Network protocol to Ergo mainnet:
#   1. Mints the Xergon Network NFT (Singleton, supply=1)
#   2. Creates the Treasury Box (holds airdrop ERG, protected by deployer's key)
#   3. Verifies all boxes were created correctly
#   4. Outputs all important IDs and generates deployment metadata
#
# This script calls the xergon-agent binary for all on-chain operations.
# It does NOT re-implement bootstrap logic.
#
# Usage:
#   ./scripts/bootstrap-mainnet.sh [OPTIONS]
#
# Options:
#   --dry-run              Show what would happen without spending ERG
#   --force-testnet        Allow running on testnet (default: mainnet only)
#   --node-url URL         Ergo node REST URL (default: http://127.0.0.1:9053)
#   --agent-bin PATH       Path to xergon-agent binary (default: auto-detect)
#   --treasury-erg AMOUNT  ERG to lock in Treasury Box (default: 1.0)
#   --deployer-addr ADDR   Deployer Ergo address (default: from node wallet)
#   --treasury-tree HEX    Compiled treasury contract ErgoTree hex
#   --nft-name NAME        NFT token name (default: XergonNetworkNFT)
#   --nft-desc DESC        NFT token description
#   --skip-verify          Skip post-deployment verification
#   --yes                  Skip all confirmation prompts (DANGEROUS on mainnet)
#   -h, --help             Show this help message
#
# EXIT CODES:
#   0  Success
#   1  General error
#   2  Prerequisite check failed
#   3  Network validation failed (wrong network)
#   4  Insufficient ERG balance
#   5  Transaction failed
#   6  Verification failed
#   7  Partial deployment (rollback needed)
#   130 Interrupted (Ctrl+C)
# ---------------------------------------------------------------------------
set -euo pipefail

# ---- Colors & formatting ----
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

log_info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()      { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error()   { echo -e "${RED}[ERROR]${NC} $*"; }
log_header()  { echo -e "\n${BOLD}${CYAN}== $* ==${NC}\n"; }
log_dryrun()  { echo -e "${YELLOW}[DRY-RUN]${NC} $*"; }

# ---- Defaults ----
DRY_RUN=false
FORCE_TESTNET=false
NODE_URL="http://127.0.0.1:9053"
AGENT_BIN=""
TREASURY_ERG="1.0"
DEPLOYER_ADDR=""
TREASURY_TREE=""
NFT_NAME="XergonNetworkNFT"
NFT_DESC="Xergon Network Protocol Identity"
SKIP_VERIFY=false
AUTO_YES=false
DEPLOYMENT_DIR="$HOME/.xergon"
DEPLOYMENT_FILE="$DEPLOYMENT_DIR/mainnet-deployment.json"
CONFIG_OUTPUT="$DEPLOYMENT_DIR/mainnet-config.toml"

# Minimum recommended ERG balance for mainnet deployment (treasury + fees + buffer)
MIN_RECOMMENDED_ERG="2.0"
# Absolute minimum to proceed (treasury + fees)
MIN_ABSOLUTE_ERG="1.5"

# ---- Parse arguments ----
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)         DRY_RUN=true; shift ;;
        --force-testnet)   FORCE_TESTNET=true; shift ;;
        --node-url)        NODE_URL="$2"; shift 2 ;;
        --agent-bin)       AGENT_BIN="$2"; shift 2 ;;
        --treasury-erg)    TREASURY_ERG="$2"; shift 2 ;;
        --deployer-addr)   DEPLOYER_ADDR="$2"; shift 2 ;;
        --treasury-tree)   TREASURY_TREE="$2"; shift 2 ;;
        --nft-name)        NFT_NAME="$2"; shift 2 ;;
        --nft-desc)        NFT_DESC="$2"; shift 2 ;;
        --skip-verify)     SKIP_VERIFY=true; shift ;;
        --yes)             AUTO_YES=true; shift ;;
        -h|--help)
            sed -n '2,/^$/s/^# \?//p' "$0"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# ---- Cleanup on interrupt ----
cleanup() {
    local exit_code=$?
    if [[ $exit_code -eq 130 ]]; then
        log_warn "Interrupted by user. No transactions were submitted."
    fi
    exit $exit_code
}
trap cleanup EXIT INT TERM

# ---- Helper: confirm prompt ----
confirm() {
    local msg="$1"
    if [[ "$AUTO_YES" == true ]]; then
        return 0
    fi
    echo -e "${YELLOW}$msg${NC}"
    read -r -p "Type 'yes' to confirm: " response
    if [[ "$response" != "yes" ]]; then
        log_info "Aborted by user."
        exit 130
    fi
}

# ---- Helper: JSON query with jq ----
jq_get() {
    local json="$1"
    local key="$2"
    echo "$json" | jq -r "$key" 2>/dev/null || echo ""
}

# ---- Helper: curl wrapper with error handling ----
node_get() {
    local endpoint="$1"
    local url="${NODE_URL}${endpoint}"
    local response
    local http_code

    response=$(curl -sf -w "\n%{http_code}" "$url" 2>/dev/null) || {
        log_error "Failed to reach Ergo node at $NODE_URL"
        log_error "Is the node running? Check: curl $NODE_URL/info"
        exit 2
    }

    http_code=$(echo "$response" | tail -1)
    local body=$(echo "$response" | sed '$d')

    if [[ "$http_code" != "200" ]]; then
        log_error "Node returned HTTP $http_code for $endpoint"
        log_error "Response: $body"
        exit 2
    fi

    echo "$body"
}

# ---- Step 0: Banner ----
echo ""
echo -e "${BOLD}${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}${CYAN}║           XERGON NETWORK -- MAINNET BOOTSTRAP           ║${NC}"
echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""
if [[ "$DRY_RUN" == true ]]; then
    log_warn "*** DRY-RUN MODE -- No transactions will be submitted ***"
    echo ""
fi

# ---- Step 1: Locate xergon-agent binary ----
log_header "Step 1: Locate xergon-agent binary"

if [[ -z "$AGENT_BIN" ]]; then
    # Try common locations
    for candidate in \
        "./target/release/xergon-agent" \
        "./target/debug/xergon-agent" \
        "../target/release/xergon-agent" \
        "/opt/xergon/bin/xergon-agent" \
        "$(which xergon-agent 2>/dev/null)" \
    ; do
        if [[ -n "$candidate" && -x "$candidate" ]]; then
            AGENT_BIN="$candidate"
            break
        fi
    done
fi

if [[ -z "$AGENT_BIN" || ! -x "$AGENT_BIN" ]]; then
    log_error "xergon-agent binary not found."
    log_error "Build it first: cargo build --release -p xergon-agent"
    log_error "Or specify: --agent-bin /path/to/xergon-agent"
    exit 2
fi

AGENT_VERSION=$("$AGENT_BIN" --version 2>/dev/null || echo "unknown")
log_ok "Found xergon-agent: $AGENT_BIN ($AGENT_VERSION)"

# ---- Step 2: Validate Ergo node connectivity ----
log_header "Step 2: Validate Ergo node"

node_info=$(node_get "/info")
if [[ -z "$node_info" ]]; then
    log_error "Could not get node info. Is the node running at $NODE_URL?"
    exit 2
fi

# Extract network type
NETWORK_TYPE=$(jq_get "$node_info" ".networkType")
NODE_NAME=$(jq_get "$node_info" ".name")
FULL_HEIGHT=$(jq_get "$node_info" ".fullHeight")
HEADERS_HEIGHT=$(jq_get "$node_info" ".headersHeight")
PEERS_COUNT=$(jq_get "$node_info" ".peersCount")
IS_MINING=$(jq_get "$node_info" ".mining")

log_info "Node:      $NODE_NAME"
log_info "Network:   $NETWORK_TYPE"
log_info "Height:    $FULL_HEIGHT (headers: $HEADERS_HEIGHT)"
log_info "Peers:     $PEERS_COUNT"
log_info "Mining:    $IS_MINING"

# Network validation: refuse testnet unless --force-testnet
if [[ "$NETWORK_TYPE" == *"Testnet"* || "$NODE_URL" == *"testnet"* ]]; then
    if [[ "$FORCE_TESTNET" != true ]]; then
        log_error "DETECTED TESTNET NODE."
        log_error "This is the MAINNET bootstrap script. Running on testnet could"
        log_error "create conflicting NFTs and waste testnet ERG."
        log_error ""
        log_error "If you want to deploy to testnet, use the testnet deployment guide:"
        log_error "  docs/TESTNET_DEPLOYMENT.md"
        log_error ""
        log_error "To override and continue anyway: --force-testnet"
        exit 3
    else
        log_warn "Proceeding on TESTNET despite mainnet script (--force-testnet)"
    fi
elif [[ "$NETWORK_TYPE" != *"Mainnet"* ]]; then
    log_error "Unexpected network type: $NETWORK_TYPE"
    log_error "Expected Mainnet. Aborting for safety."
    exit 3
else
    log_ok "Mainnet node confirmed."
fi

# Sync check
if [[ "$FULL_HEIGHT" == "null" || "$FULL_HEIGHT" == "0" ]]; then
    log_error "Node is not synced (height=0). Wait for full sync before deploying."
    exit 2
fi

HEIGHT_DIFF=$((HEADERS_HEIGHT - FULL_HEIGHT))
if [[ "$HEIGHT_DIFF" -gt 10 ]]; then
    log_warn "Node is still syncing (behind by $HEIGHT_DIFF blocks)."
    log_warn "Deployment will work but the node should be fully synced for best results."
else
    log_ok "Node is synced."
fi

# Peers check
if [[ "$PEERS_COUNT" -lt 3 ]]; then
    log_warn "Low peer count ($PEERS_COUNT). Recommended: >= 3 peers for reliable mainnet."
fi

# ---- Step 3: Check wallet status ----
log_header "Step 3: Check wallet status"

wallet_status=$(node_get "/wallet/status")
WALLET_UNLOCKED=$(jq_get "$wallet_status" ".isUnlocked")
WALLET_BALANCE=$(jq_get "$wallet_status" ".balance")

if [[ "$WALLET_UNLOCKED" != "true" ]]; then
    log_error "Ergo node wallet is LOCKED."
    log_error "Unlock it before bootstrap:"
    log_error "  curl -X POST $NODE_URL/wallet/unlock -H 'Content-Type: application/json' \\"
    log_error "    -d '{\"pass\": \"YOUR_WALLET_PASSWORD\"}'"
    exit 2
fi
log_ok "Wallet is unlocked."

# Parse balance (nanoERG)
if [[ -z "$WALLET_BALANCE" || "$WALLET_BALANCE" == "null" ]]; then
    # Try alternative endpoint
    wallet_bal_resp=$(node_get "/wallet/balances")
    WALLET_BALANCE=$(echo "$wallet_bal_resp" | jq -r '.[0].nanoErgs // empty' 2>/dev/null)
fi

if [[ -n "$WALLET_BALANCE" && "$WALLET_BALANCE" != "null" ]]; then
    # Convert nanoERG to ERG (1 ERG = 1_000_000_000 nanoERG)
    WALLET_BALANCE_ERG=$(echo "scale=4; $WALLET_BALANCE / 1000000000" | bc 2>/dev/null || echo "unknown")
    log_info "Wallet balance: $WALLET_BALANCE_ERG ERG ($WALLET_BALANCE nanoERG)"

    # Balance check
    if (( $(echo "$WALLET_BALANCE_ERG < $MIN_ABSOLUTE_ERG" | bc -l 2>/dev/null || echo 0) )); then
        log_error "Insufficient ERG balance: $WALLET_BALANCE_ERG ERG"
        log_error "Minimum required: $MIN_ABSOLUTE_ERG ERG (treasury + fees)"
        log_error "Recommended:      $MIN_RECOMMENDED_ERG ERG (treasury + fees + buffer)"
        exit 4
    elif (( $(echo "$WALLET_BALANCE_ERG < $MIN_RECOMMENDED_ERG" | bc -l 2>/dev/null || echo 0) )); then
        log_warn "ERG balance is low: $WALLET_BALANCE_ERG ERG (recommended: $MIN_RECOMMENDED_ERG ERG)"
    else
        log_ok "Sufficient ERG balance: $WALLET_BALANCE_ERG ERG"
    fi
else
    log_warn "Could not determine wallet balance. Proceed with caution."
fi

# ---- Step 4: Check for existing deployment ----
log_header "Step 4: Check for existing deployment"

if [[ -f "$DEPLOYMENT_FILE" ]]; then
    EXISTING_STATE=$(cat "$DEPLOYMENT_FILE")
    EXISTING_NFT=$(jq_get "$EXISTING_STATE" ".nft_token_id")
    EXISTING_TX=$(jq_get "$EXISTING_STATE" ".genesis_tx_id")

    if [[ -n "$EXISTING_NFT" && "$EXISTING_NFT" != "" && "$EXISTING_NFT" != "null" ]]; then
        log_warn "Found existing mainnet deployment:"
        log_warn "  NFT Token ID:     $EXISTING_NFT"
        log_warn "  Genesis TX ID:    $EXISTING_TX"
        log_warn "  Deployment file:  $DEPLOYMENT_FILE"
        echo ""

        if [[ "$AUTO_YES" != true ]]; then
            read -r -p "A deployment already exists. Continue anyway? (yes/no): " response
            if [[ "$response" != "yes" ]]; then
                log_info "Aborted. Use the existing deployment or delete $DEPLOYMENT_FILE to re-deploy."
                exit 0
            fi
        fi
    fi
else
    log_info "No existing deployment found. Fresh bootstrap."
fi

# ---- Step 5: Get deployer address ----
log_header "Step 5: Resolve deployer address"

if [[ -z "$DEPLOYER_ADDR" ]]; then
    # Try to get from node wallet
    addresses_resp=$(node_get "/wallet/addresses")
    DEPLOYER_ADDR=$(echo "$addresses_resp" | jq -r '.[0]' 2>/dev/null)

    if [[ -z "$DEPLOYER_ADDR" || "$DEPLOYER_ADDR" == "null" ]]; then
        log_error "Could not determine deployer address from node wallet."
        log_error "Specify manually: --deployer-addr YOUR_ERGO_ADDRESS"
        exit 2
    fi
fi

log_info "Deployer address: $DEPLOYER_ADDR"

# Validate address format
if [[ ! "$DEPLOYER_ADDR" =~ ^[0-9a-zA-Z]{20,}$ ]]; then
    log_error "Invalid Ergo address format: $DEPLOYER_ADDR"
    log_error "Expected a P2PK or P2S address (starts with 3 or 9)."
    exit 2
fi

# ---- Step 6: Confirm deployment parameters ----
log_header "Step 6: Deployment parameters"

echo "  Network:          $([ "$FORCE_TESTNET" == true ] && echo "TESTNET (forced)" || echo "MAINNET")"
echo "  Node URL:         $NODE_URL"
echo "  Deployer:         $DEPLOYER_ADDR"
echo "  Treasury ERG:     $TREASURY_ERG ERG"
echo "  NFT Name:         $NFT_NAME"
echo "  NFT Description:  $NFT_DESC"
echo "  Treasury Tree:    $([ -n "$TREASURY_TREE" ] && echo "${TREASURY_TREE:0:20}..." || echo "(using deployer address)")"
echo "  Deployment dir:   $DEPLOYMENT_DIR"
echo ""

if [[ "$DRY_RUN" == true ]]; then
    log_dryrun "Would create a bootstrap transaction with the above parameters."
    log_dryrun "The xergon-agent binary would be called as:"
    echo ""
    echo "  $AGENT_BIN bootstrap \\"
    echo "    --node-url $NODE_URL \\"
    echo "    --deployer-address $DEPLOYER_ADDR \\"
    echo "    --treasury-erg $TREASURY_ERG \\"
    echo "    --nft-name \"$NFT_NAME\" \\"
    echo "    --nft-description \"$NFT_DESC\" \\"
    $([ -n "$TREASURY_TREE" ] && echo "    --treasury-tree $TREASURY_TREE \\")
    echo "    --network mainnet"
    echo ""
    log_dryrun "Expected outputs:"
    log_dryrun "  - Xergon Network NFT (singleton, supply=1)"
    log_dryrun "  - Treasury Box with $TREASURY_ERG ERG + NFT"
    log_dryrun "  - Transaction ID for explorer verification"
    log_dryrun ""
    log_dryrun "Deployment metadata would be saved to: $DEPLOYMENT_FILE"
    log_dryrun "Mainnet config would be saved to:       $CONFIG_OUTPUT"
    echo ""
    log_ok "Dry-run complete. No transactions were submitted."
    exit 0
fi

# ---- MAINNET CONFIRMATION ----
echo -e "${RED}${BOLD}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${RED}${BOLD}║  WARNING: THIS WILL SPEND REAL ERG ON MAINNET           ║${NC}"
echo -e "${RED}${BOLD}║                                                          ║${NC}"
echo -e "${RED}${BOLD}║  The following transaction will be submitted:            ║${NC}"
echo -e "${RED}${BOLD}║  - Mint Xergon Network NFT (singleton, supply=1)         ║${NC}"
echo -e "${RED}${BOLD}║  - Create Treasury Box with ${TREASURY_ERG} ERG             ║${NC}"
echo -e "${RED}${BOLD}║  - Network fee: ~0.001 ERG                              ║${NC}"
echo -e "${RED}${BOLD}║                                                          ║${NC}"
echo -e "${RED}${BOLD}║  This action is IRREVERSIBLE. The NFT ID is derived      ║${NC}"
echo -e "${RED}${BOLD}║  from the input box ID and CANNOT be changed later.      ║${NC}"
echo -e "${RED}${BOLD}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""

confirm "Do you want to proceed with mainnet deployment?"

# ---- Step 7: Create deployment directory ----
log_header "Step 7: Prepare deployment directory"

mkdir -p "$DEPLOYMENT_DIR"
log_ok "Deployment directory: $DEPLOYMENT_DIR"

# ---- Step 8: Submit bootstrap transaction ----
log_header "Step 8: Submit bootstrap transaction"

log_info "Calling xergon-agent to mint NFT and create Treasury Box..."
echo ""

BOOTSTRAP_ARGS=(
    "$AGENT_BIN" bootstrap
    --node-url "$NODE_URL"
    --deployer-address "$DEPLOYER_ADDR"
    --treasury-erg "$TREASURY_ERG"
    --nft-name "$NFT_NAME"
    --nft-description "$NFT_DESC"
    --network mainnet
)

if [[ -n "$TREASURY_TREE" ]]; then
    BOOTSTRAP_ARGS+=(--treasury-tree "$TREASURY_TREE")
fi

log_info "Running: ${BOOTSTRAP_ARGS[*]}"
echo ""

# Run the bootstrap command and capture output
BOOTSTRAP_OUTPUT=$("${BOOTSTRAP_ARGS[@]}" 2>&1) || {
    local_exit=$?
    log_error "Bootstrap transaction FAILED (exit code: $local_exit)"
    log_error "Output:"
    echo "$BOOTSTRAP_OUTPUT"
    log_error ""
    log_error "Possible causes:"
    log_error "  - Insufficient ERG in wallet (need $TREASURY_ERG + ~0.001 fee)"
    log_error "  - Wallet locked after initial check"
    log_error "  - Network connectivity issue"
    log_error "  - Invalid deployer address or treasury tree"
    exit 5
}

echo "$BOOTSTRAP_OUTPUT"
echo ""

# Parse bootstrap output for IDs
# The agent outputs structured data; try to extract key values
NFT_TOKEN_ID=""
TREASURY_BOX_ID=""
GENESIS_TX_ID=""

# Try to parse structured output (JSON lines or key=value)
if echo "$BOOTSTRAP_OUTPUT" | jq -e . >/dev/null 2>&1; then
    # JSON output
    NFT_TOKEN_ID=$(echo "$BOOTSTRAP_OUTPUT" | jq -r '.nft_token_id // .nft_tokenId // empty' 2>/dev/null)
    TREASURY_BOX_ID=$(echo "$BOOTSTRAP_OUTPUT" | jq -r '.treasury_box_id // .treasuryBoxId // empty' 2>/dev/null)
    GENESIS_TX_ID=$(echo "$BOOTSTRAP_OUTPUT" | jq -r '.genesis_tx_id // .txId // .tx_id // empty' 2>/dev/null)
fi

# Fallback: regex parsing of text output
if [[ -z "$NFT_TOKEN_ID" ]]; then
    NFT_TOKEN_ID=$(echo "$BOOTSTRAP_OUTPUT" | grep -oP '(?:NFT[ _]Token[ _]ID|nft_token_id)[:\s=]+\K[a-f0-9]{64}' 2>/dev/null | head -1)
fi
if [[ -z "$TREASURY_BOX_ID" ]]; then
    TREASURY_BOX_ID=$(echo "$BOOTSTRAP_OUTPUT" | grep -oP '(?:Treasury[ _]Box[ _]ID|treasury_box_id)[:\s=]+\K[a-f0-9]{64}' 2>/dev/null | head -1)
fi
if [[ -z "$GENESIS_TX_ID" ]]; then
    GENESIS_TX_ID=$(echo "$BOOTSTRAP_OUTPUT" | grep -oP '(?:TX[ _]ID|tx_id|genesis_tx_id|Transaction[ _]ID)[:\s=]+\K[a-f0-9]{64}' 2>/dev/null | head -1)
fi

if [[ -z "$NFT_TOKEN_ID" || -z "$GENESIS_TX_ID" ]]; then
    log_warn "Could not parse all IDs from bootstrap output."
    log_warn "You may need to extract them manually from the output above."
    log_warn "Save the output and check the Ergo explorer:"
    log_warn "  https://explorer.ergoplatform.com"
fi

# ---- Step 9: Post-deployment verification ----
if [[ "$SKIP_VERIFY" != true ]]; then
    log_header "Step 9: Verify deployment on-chain"

    if [[ -n "$GENESIS_TX_ID" ]]; then
        log_info "Checking genesis transaction on node..."
        TX_DETAIL=$(node_get "/transactions/$GENESIS_TX_ID" 2>/dev/null) || {
            log_warn "Transaction not yet found on node (may need a few seconds)."
            log_warn "Verify manually: https://explorer.ergoplatform.com/en/transactions/$GENESIS_TX_ID"
        }

        if [[ -n "$TX_DETAIL" ]]; then
            # Extract NFT token ID from transaction outputs if we don't have it
            if [[ -z "$NFT_TOKEN_ID" ]]; then
                NFT_TOKEN_ID=$(echo "$TX_DETAIL" | jq -r '
                    [.outputs[].assets[]? | select(.amount == 1)][0].tokenId // empty
                ' 2>/dev/null)
            fi
            if [[ -z "$TREASURY_BOX_ID" ]]; then
                TREASURY_BOX_ID=$(echo "$TX_DETAIL" | jq -r '
                    [.outputs[] | select(.assets[]?.tokenId == "'"$NFT_TOKEN_ID"'")][0].boxId // empty
                ' 2>/dev/null)
            fi

            log_ok "Transaction confirmed on node."
        fi
    fi

    # Verify NFT exists in UTXO set
    if [[ -n "$NFT_TOKEN_ID" ]]; then
        log_info "Checking NFT in UTXO set..."
        NFT_BOXES=$(node_get "/utxo/withTokenId/$NFT_TOKEN_ID" 2>/dev/null) || {
            log_warn "Could not query UTXO set for NFT."
        }

        if [[ -n "$NFT_BOXES" ]]; then
            BOX_COUNT=$(echo "$NFT_BOXES" | jq 'length' 2>/dev/null || echo "0")
            if [[ "$BOX_COUNT" -ge 1 ]]; then
                log_ok "NFT found in UTXO set ($BOX_COUNT box(es) contain it)."
            else
                log_error "NFT NOT found in UTXO set. Something went wrong."
                exit 6
            fi
        fi
    fi

    echo ""
    log_info "Explorer verification links:"
    if [[ -n "$GENESIS_TX_ID" ]]; then
        log_info "  Transaction: https://explorer.ergoplatform.com/en/transactions/$GENESIS_TX_ID"
    fi
    if [[ -n "$NFT_TOKEN_ID" ]]; then
        log_info "  NFT Token:   https://explorer.ergoplatform.com/en/tokens/$NFT_TOKEN_ID"
    fi
    if [[ -n "$TREASURY_BOX_ID" ]]; then
        log_info "  Treasury Box: https://explorer.ergoplatform.com/en/boxes/$TREASURY_BOX_ID"
    fi
else
    log_info "Verification skipped (--skip-verify)."
fi

# ---- Step 10: Save deployment metadata ----
log_header "Step 10: Save deployment metadata"

DEPLOYMENT_JSON=$(cat <<EOF
{
    "deployment": {
        "network": "$([ "$FORCE_TESTNET" == true ] && echo "testnet" || echo "mainnet")",
        "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
        "xergon_agent_version": "$AGENT_VERSION",
        "node_url": "$NODE_URL"
    },
    "treasury": {
        "deployer_address": "$DEPLOYER_ADDR",
        "treasury_erg": "$TREASURY_ERG",
        "nft_token_id": "${NFT_TOKEN_ID:-UNKNOWN}",
        "treasury_box_id": "${TREASURY_BOX_ID:-UNKNOWN}",
        "genesis_tx_id": "${GENESIS_TX_ID:-UNKNOWN}",
        "treasury_tree_hex": "${TREASURY_TREE:-}"
    },
    "nft": {
        "name": "$NFT_NAME",
        "description": "$NFT_DESC",
        "decimals": 0,
        "supply": 1
    },
    "verification": {
        "verified": "$([ "$SKIP_VERIFY" == true ] && echo "false" || echo "true")",
        "explorer_base": "https://explorer.ergoplatform.com"
    }
}
EOF
)

echo "$DEPLOYMENT_JSON" | jq '.' > "$DEPLOYMENT_FILE"
log_ok "Deployment metadata saved: $DEPLOYMENT_FILE"

# ---- Step 11: Generate mainnet config ----
log_header "Step 11: Generate mainnet config"

CONFIG_TOML=$(cat <<EOF
# Xergon Network -- Mainnet Configuration
# Generated by scripts/bootstrap-mainnet.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
# DO NOT commit this file to version control. It contains deployment-specific values.

[ergo_node]
# Use your local mainnet node (or a trusted public node)
rest_url = "http://127.0.0.1:9053"

[xergon]
# Provider identity -- fill in your details
provider_id = "CHANGE_ME"
provider_name = "CHANGE_ME"
region = "CHANGE_ME"
ergo_address = "$DEPLOYER_ADDR"

[peer_discovery]
discovery_interval_secs = 300
probe_timeout_secs = 5
xergon_agent_port = 9099
max_concurrent_probes = 10
max_peers_per_cycle = 50

[api]
listen_addr = "0.0.0.0:9099"
# Set a strong API key for production
api_key = ""

[settlement]
enabled = true
interval_secs = 86400
dry_run = false
min_settlement_usd = 0.10

[chain]
heartbeat_tx_enabled = true
usage_proof_tx_enabled = true
usage_proof_batch_interval_secs = 30
usage_proof_min_value_nanoerg = 1000000
# Provider NFT token ID -- set after registering as a provider
provider_nft_token_id = ""

[contracts]
# Override with mainnet-compiled ErgoTree hex values
# These should come from 'make compile-contracts' against a mainnet node
provider_box_hex = ""
provider_registration_hex = ""
treasury_box_hex = "${TREASURY_TREE:-}"
usage_proof_hex = ""
user_staking_hex = ""

[update]
release_url = "https://api.github.com/repos/n1ur0/Xergon-Network/releases/latest"
auto_check = false
check_interval_hours = 24
EOF
)

echo "$CONFIG_TOML" > "$CONFIG_OUTPUT"
log_ok "Mainnet config generated: $CONFIG_OUTPUT"

# ---- Step 12: Summary ----
echo ""
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}            MAINNET DEPLOYMENT COMPLETE                      ${NC}"
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════${NC}"
echo ""
echo "  Deployer Address:  $DEPLOYER_ADDR"
echo "  NFT Token ID:      ${NFT_TOKEN_ID:-SEE_OUTPUT_ABOVE}"
echo "  Treasury Box ID:   ${TREASURY_BOX_ID:-SEE_OUTPUT_ABOVE}"
echo "  Genesis TX ID:     ${GENESIS_TX_ID:-SEE_OUTPUT_ABOVE}"
echo ""
echo "  Files created:"
echo "    $DEPLOYMENT_FILE"
echo "    $CONFIG_OUTPUT"
echo ""
echo "  Next steps:"
echo "    1. Verify on explorer: https://explorer.ergoplatform.com/en/transactions/${GENESIS_TX_ID:-TX_ID}"
echo "    2. Edit $CONFIG_OUTPUT with your provider details"
echo "    3. Register as a provider: xergon-agent register --network mainnet"
echo "    4. Start the agent: xergon-agent serve"
echo ""
echo -e "${YELLOW}IMPORTANT: Back up $DEPLOYMENT_DIR to a secure location.${NC}"
echo -e "${YELLOW}The NFT token ID is your protocol's permanent on-chain identity.${NC}"
echo ""

# ---- Partial rollback note ----
if [[ -z "$NFT_TOKEN_ID" || -z "$TREASURY_BOX_ID" || -z "$GENESIS_TX_ID" ]]; then
    log_warn "Some IDs could not be parsed from the output."
    log_warn "This may indicate a partial or problematic deployment."
    log_warn "Review the bootstrap output above and verify on the explorer."
    log_warn "If the NFT was minted but the box is missing, the deployer key"
    log_warn "can spend the box (it's P2PK to the deployer address)."
    exit 7
fi

exit 0
