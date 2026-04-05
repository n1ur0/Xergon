#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Xergon Network -- Genesis Deployment Script
#
# Full protocol deployment covering ALL 11 contracts:
#   1. Compile all contracts via Ergo node
#   2. Mint Xergon Network NFT + create Treasury Box
#   3. Compile all contract ErgoTree hex values
#   4. Save deployment manifest with all addresses, token IDs, box IDs
#
# This script orchestrates:
#   - compile_contracts binary (contract compilation)
#   - xergon-agent bootstrap (NFT mint + treasury creation)
#   - Direct node API calls (verification)
#
# Usage:
#   ./scripts/deploy-genesis.sh [OPTIONS]
#
# Options:
#   --node-url URL         Ergo node REST URL (default: http://127.0.0.1:9053)
#   --network NETWORK      "mainnet" or "testnet" (default: auto-detect from node)
#   --treasury-erg AMOUNT  ERG to lock in Treasury Box (default: 1.0)
#   --deployer-addr ADDR   Deployer Ergo address (default: from node wallet)
#   --nft-name NAME        NFT token name (default: XergonNetworkNFT)
#   --nft-desc DESC        NFT token description
#   --contracts-dir DIR    Directory with contract source files (default: contracts)
#   --output-dir DIR       Output directory for compiled hex (default: contracts/compiled)
#   --deployment-dir DIR   Where to save deployment manifest (default: ~/.xergon)
#   --dry-run              Show what would happen without spending ERG
#   --skip-compile         Skip contract compilation (use existing .hex files)
#   --skip-bootstrap       Skip treasury/NFT bootstrap (only compile contracts)
#   --skip-verify          Skip post-deployment verification
#   --yes                  Skip confirmation prompts
#   -h, --help             Show this help message
#
# EXIT CODES:
#   0  Success
#   1  General error
#   2  Prerequisite check failed
#   3  Network validation failed
#   4  Insufficient ERG balance
#   5  Transaction failed
#   6  Verification failed
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
NC='\033[0m'

log_info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()      { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error()   { echo -e "${RED}[ERROR]${NC} $*"; }
log_header()  { echo -e "\n${BOLD}${CYAN}== $* ==${NC}\n"; }
log_dryrun()  { echo -e "${YELLOW}[DRY-RUN]${NC} $*"; }

# ---- Defaults ----
DRY_RUN=false
NODE_URL="http://127.0.0.1:9053"
NETWORK=""
TREASURY_ERG="1.0"
DEPLOYER_ADDR=""
NFT_NAME="XergonNetworkNFT"
NFT_DESC="Xergon Network Protocol Identity"
CONTRACTS_DIR="contracts"
OUTPUT_DIR="contracts/compiled"
DEPLOYMENT_DIR="$HOME/.xergon"
SKIP_COMPILE=false
SKIP_BOOTSTRAP=false
SKIP_VERIFY=false
AUTO_YES=false

# All 11 contracts (name = filename without extension)
ALL_CONTRACTS=(
    provider_box
    provider_registration
    treasury_box
    usage_proof
    user_staking
    gpu_rental
    usage_commitment
    relay_registry
    gpu_rating
    gpu_rental_listing
    payment_bridge
)

# ---- Parse arguments ----
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)         DRY_RUN=true; shift ;;
        --node-url)        NODE_URL="$2"; shift 2 ;;
        --network)         NETWORK="$2"; shift 2 ;;
        --treasury-erg)    TREASURY_ERG="$2"; shift 2 ;;
        --deployer-addr)   DEPLOYER_ADDR="$2"; shift 2 ;;
        --nft-name)        NFT_NAME="$2"; shift 2 ;;
        --nft-desc)        NFT_DESC="$2"; shift 2 ;;
        --contracts-dir)   CONTRACTS_DIR="$2"; shift 2 ;;
        --output-dir)      OUTPUT_DIR="$2"; shift 2 ;;
        --deployment-dir)  DEPLOYMENT_DIR="$2"; shift 2 ;;
        --skip-compile)    SKIP_COMPILE=true; shift ;;
        --skip-bootstrap)  SKIP_BOOTSTRAP=true; shift ;;
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
        log_warn "Interrupted by user."
    fi
    exit $exit_code
}
trap cleanup EXIT INT TERM

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

# ---- Helper: compile a single contract via node API ----
compile_contract() {
    local name="$1"
    local source_file="$2"

    if [[ ! -f "$source_file" ]]; then
        log_error "Contract source not found: $source_file"
        return 1
    fi

    local source
    source=$(cat "$source_file")

    # Call node's script compilation endpoint
    local response
    response=$(curl -sf -X POST "${NODE_URL}/script/p2sAddress" \
        -H "Content-Type: application/json" \
        -d "$(jq -n --arg src "$source" '{source: $src}')" 2>/dev/null) || {
        log_error "Failed to compile contract: $name"
        return 1
    }

    local address
    address=$(echo "$response" | jq -r '.address' 2>/dev/null)

    if [[ -z "$address" || "$address" == "null" ]]; then
        log_error "Node returned invalid address for: $name"
        log_error "Response: $response"
        return 1
    fi

    # Decode base58 P2S address to extract ErgoTree hex
    # P2S format: 1 byte network prefix + ErgoTree bytes + 4 byte BLAKE2b checksum
    local decoded
    decoded=$(echo "$address" | python3 -c "
import sys, base64, hashlib
addr = sys.stdin.read().strip()
# Base58 decode
alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
n = 0
for c in addr:
    n = n * 58 + alphabet.index(c)
# Convert to bytes
b = n.to_bytes((n.bit_length() + 7) // 8, 'big')
# Strip network prefix byte and 4-byte checksum
ergotree = b[1:-4]
print(ergotree.hex())
" 2>/dev/null) || {
        log_error "Failed to decode P2S address for: $name"
        return 1
    }

    if [[ -z "$decoded" ]]; then
        log_error "Empty ErgoTree hex for: $name"
        return 1
    fi

    # Write hex file
    mkdir -p "$OUTPUT_DIR"
    echo -n "$decoded" > "${OUTPUT_DIR}/${name}.hex"

    echo "$address"
}

# ---- Banner ----
echo ""
echo -e "${BOLD}${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}${CYAN}║         XERGON NETWORK -- GENESIS DEPLOYMENT              ║${NC}"
echo -e "${BOLD}${CYAN}║         (All 11 Contracts + Treasury Bootstrap)           ║${NC}"
echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""
if [[ "$DRY_RUN" == true ]]; then
    log_warn "*** DRY-RUN MODE -- No transactions will be submitted ***"
    echo ""
fi

# =========================================================================
# Step 1: Validate Ergo node connectivity
# =========================================================================
log_header "Step 1: Validate Ergo node"

node_info=$(node_get "/info")
if [[ -z "$node_info" ]]; then
    log_error "Could not get node info."
    exit 2
fi

DETECTED_NETWORK=$(jq_get "$node_info" ".networkType")
NODE_NAME=$(jq_get "$node_info" ".name")
FULL_HEIGHT=$(jq_get "$node_info" ".fullHeight")
PEERS_COUNT=$(jq_get "$node_info" ".peersCount")

# Auto-detect network if not specified
if [[ -z "$NETWORK" ]]; then
    if [[ "$DETECTED_NETWORK" == *"Mainnet"* ]]; then
        NETWORK="mainnet"
    elif [[ "$DETECTED_NETWORK" == *"Testnet"* ]]; then
        NETWORK="testnet"
    else
        NETWORK="unknown"
    fi
fi

log_info "Node:      $NODE_NAME"
log_info "Network:   $NETWORK (detected: $DETECTED_NETWORK)"
log_info "Height:    $FULL_HEIGHT"
log_info "Peers:     $PEERS_COUNT"

if [[ "$FULL_HEIGHT" == "null" || "$FULL_HEIGHT" == "0" ]]; then
    log_error "Node is not synced (height=0)."
    exit 2
fi
log_ok "Node is reachable and synced."

# =========================================================================
# Step 2: Compile all contracts
# =========================================================================
if [[ "$SKIP_COMPILE" == true ]]; then
    log_header "Step 2: Contract compilation (SKIPPED)"
    log_info "Using existing .hex files in $OUTPUT_DIR"
else
    log_header "Step 2: Compile all contracts"

    declare -A CONTRACT_ADDRESSES
    COMPILE_ERRORS=0

    for name in "${ALL_CONTRACTS[@]}"; do
        # Try both .es and .ergo extensions
        source_file=""
        if [[ -f "${CONTRACTS_DIR}/${name}.es" ]]; then
            source_file="${CONTRACTS_DIR}/${name}.es"
        elif [[ -f "${CONTRACTS_DIR}/${name}.ergo" ]]; then
            source_file="${CONTRACTS_DIR}/${name}.ergo"
        else
            log_warn "Source file not found for: $name (tried .es and .ergo)"
            COMPILE_ERRORS=$((COMPILE_ERRORS + 1))
            continue
        fi

        if [[ "$DRY_RUN" == true ]]; then
            log_dryrun "Would compile: $name ($source_file)"
            continue
        fi

        log_info "Compiling: $name..."
        address=$(compile_contract "$name" "$source_file") || {
            log_error "Failed to compile: $name"
            COMPILE_ERRORS=$((COMPILE_ERRORS + 1))
            continue
        }

        CONTRACT_ADDRESSES[$name]="$address"
        log_ok "Compiled: $name -> $address"
    done

    if [[ "$DRY_RUN" != true && $COMPILE_ERRORS -gt 0 ]]; then
        log_warn "$COMPILE_ERRORS contract(s) failed to compile."
        log_warn "Review errors above. Some contracts may use features not supported by this node version."
    fi

    if [[ "$DRY_RUN" != true && $COMPILE_ERRORS -eq 0 ]]; then
        log_ok "All ${#ALL_CONTRACTS[@]} contracts compiled successfully."
    fi
fi

# =========================================================================
# Step 3: Treasury bootstrap (NFT mint + Treasury Box)
# =========================================================================
if [[ "$SKIP_BOOTSTRAP" == true ]]; then
    log_header "Step 3: Treasury bootstrap (SKIPPED)"
    log_info "Skipping NFT mint and Treasury Box creation."
    log_info "If treasury already exists, load its IDs from existing deployment manifest."

    # Try to load existing deployment
    EXISTING_MANIFEST="$DEPLOYMENT_DIR/${NETWORK}-genesis.json"
    if [[ -f "$EXISTING_MANIFEST" ]]; then
        log_info "Found existing manifest: $EXISTING_MANIFEST"
    fi
else
    log_header "Step 3: Treasury bootstrap (NFT mint + Treasury Box)"

    # Check wallet
    wallet_status=$(node_get "/wallet/status")
    WALLET_UNLOCKED=$(jq_get "$wallet_status" ".isUnlocked")

    if [[ "$WALLET_UNLOCKED" != "true" ]]; then
        log_error "Wallet is LOCKED. Unlock it first:"
        log_error "  curl -X POST $NODE_URL/wallet/unlock -H 'Content-Type: application/json' \\"
        log_error "    -d '{\"pass\": \"YOUR_PASSWORD\"}'"
        exit 2
    fi
    log_ok "Wallet is unlocked."

    # Get deployer address
    if [[ -z "$DEPLOYER_ADDR" ]]; then
        addresses_resp=$(node_get "/wallet/addresses")
        DEPLOYER_ADDR=$(echo "$addresses_resp" | jq -r '.[0]' 2>/dev/null)
        if [[ -z "$DEPLOYER_ADDR" || "$DEPLOYER_ADDR" == "null" ]]; then
            log_error "Could not get deployer address from wallet."
            exit 2
        fi
    fi
    log_info "Deployer: $DEPLOYER_ADDR"

    if [[ "$DRY_RUN" == true ]]; then
        log_dryrun "Would mint NFT + create Treasury Box with $TREASURY_ERG ERG"
        log_dryrun "Deployer: $DEPLOYER_ADDR"
    else
        # Check balance
        wallet_bal=$(node_get "/wallet/balances" 2>/dev/null)
        BALANCE_NANOERG=$(echo "$wallet_bal" | jq -r '.[0].nanoErgs // "0"' 2>/dev/null)
        BALANCE_ERG=$(echo "scale=4; ${BALANCE_NANOERG:-0} / 1000000000" | bc 2>/dev/null || echo "0")
        log_info "Wallet balance: $BALANCE_ERG ERG"

        MIN_ERG="1.5"
        if (( $(echo "$BALANCE_ERG < $MIN_ERG" | bc -l 2>/dev/null || echo 0) )); then
            log_error "Insufficient balance: $BALANCE_ERG ERG (need >= $MIN_ERG ERG)"
            exit 4
        fi

        # Confirm
        if [[ "$AUTO_YES" != true ]]; then
            echo -e "${RED}${BOLD}WARNING: This will mint the Xergon Network NFT and create a Treasury Box on $NETWORK.${NC}"
            echo -e "${RED}${BOLD}This costs real ERG and is IRREVERSIBLE.${NC}"
            read -r -p "Type 'yes' to confirm: " response
            if [[ "$response" != "yes" ]]; then
                log_info "Aborted."
                exit 130
            fi
        fi

        # Use the existing bootstrap script
        BOOTSTRAP_SCRIPT="$(dirname "$0")/bootstrap-mainnet.sh"
        if [[ -x "$BOOTSTRAP_SCRIPT" ]]; then
            log_info "Running bootstrap-mainnet.sh..."
            BOOTSTRAP_ARGS=(
                "$BOOTSTRAP_SCRIPT"
                --node-url "$NODE_URL"
                --deployer-addr "$DEPLOYER_ADDR"
                --treasury-erg "$TREASURY_ERG"
                --nft-name "$NFT_NAME"
                --nft-desc "$NFT_DESC"
                --deployment-dir "$DEPLOYMENT_DIR"
                --yes
            )
            if [[ "$NETWORK" == "testnet" ]]; then
                BOOTSTRAP_ARGS+=(--force-testnet)
            fi
            if [[ "$SKIP_VERIFY" == true ]]; then
                BOOTSTRAP_ARGS+=(--skip-verify)
            fi

            "${BOOTSTRAP_ARGS[@]}" || {
                log_error "Bootstrap failed. Check the output above."
                exit 5
            }
        else
            log_warn "bootstrap-mainnet.sh not found at $BOOTSTRAP_SCRIPT"
            log_info "Falling back to manual treasury creation..."
            log_info "You will need to create the Treasury Box manually via the node API or xergon-agent."
        fi
    fi
fi

# =========================================================================
# Step 4: Build deployment manifest
# =========================================================================
log_header "Step 4: Build deployment manifest"

mkdir -p "$DEPLOYMENT_DIR"

MANIFEST_FILE="$DEPLOYMENT_DIR/${NETWORK}-genesis.json"

# Load existing treasury info if available
EXISTING_MANIFEST="$DEPLOYMENT_DIR/${NETWORK}-deployment.json"
NFT_TOKEN_ID=""
TREASURY_BOX_ID=""
GENESIS_TX_ID=""

if [[ -f "$EXISTING_MANIFEST" ]]; then
    EXISTING=$(cat "$EXISTING_MANIFEST")
    NFT_TOKEN_ID=$(jq_get "$EXISTING" ".treasury.nft_token_id")
    TREASURY_BOX_ID=$(jq_get "$EXISTING" ".treasury.treasury_box_id")
    GENESIS_TX_ID=$(jq_get "$EXISTING" ".treasury.genesis_tx_id")
    # Skip "UNKNOWN" values
    [[ "$NFT_TOKEN_ID" == "UNKNOWN" || "$NFT_TOKEN_ID" == "null" ]] && NFT_TOKEN_ID=""
    [[ "$TREASURY_BOX_ID" == "UNKNOWN" || "$TREASURY_BOX_ID" == "null" ]] && TREASURY_BOX_ID=""
    [[ "$GENESIS_TX_ID" == "UNKNOWN" || "$GENESIS_TX_ID" == "null" ]] && GENESIS_TX_ID=""
fi

# Build contract entries for manifest
CONTRACT_JSON_ENTRIES=""
for name in "${ALL_CONTRACTS[@]}"; do
    hex_file="${OUTPUT_DIR}/${name}.hex"
    hex_value=""
    address_value=""

    if [[ -f "$hex_file" ]]; then
        hex_value=$(cat "$hex_file" | tr -d '[:space:]')
    fi

    # Try to get address from compile results
    if [[ -n "${CONTRACT_ADDRESSES[$name]:-}" ]]; then
        address_value="${CONTRACT_ADDRESSES[$name]}"
    fi

    if [[ -n "$CONTRACT_JSON_ENTRIES" ]]; then
        CONTRACT_JSON_ENTRIES="$CONTRACT_JSON_ENTRIES,"
    fi

    CONTRACT_JSON_ENTRIES="$CONTRACT_JSON_ENTRIES
    \"$name\": {
        \"source_file\": \"$name.es\",
        \"hex_file\": \"$hex_file\",
        \"ergo_tree_hex\": \"${hex_value:-PLACEHOLDER}\",
        \"p2s_address\": \"${address_value:-NOT_COMPILED}\"
    }"
done

# Generate manifest JSON
cat > "$MANIFEST_FILE" <<MANIFEST_EOF
{
    "deployment": {
        "network": "$NETWORK",
        "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
        "node_url": "$NODE_URL",
        "node_name": "$NODE_NAME",
        "height": $FULL_HEIGHT
    },
    "treasury": {
        "deployer_address": "$DEPLOYER_ADDR",
        "treasury_erg": "$TREASURY_ERG",
        "nft_token_id": "${NFT_TOKEN_ID:-PENDING}",
        "treasury_box_id": "${TREASURY_BOX_ID:-PENDING}",
        "genesis_tx_id": "${GENESIS_TX_ID:-PENDING}",
        "nft_name": "$NFT_NAME",
        "nft_description": "$NFT_DESC"
    },
    "contracts": {$CONTRACT_JSON_ENTRIES
    },
    "file_locations": {
        "contract_sources": "$CONTRACTS_DIR",
        "compiled_hex": "$OUTPUT_DIR",
        "manifest": "$MANIFEST_FILE"
    }
}
MANIFEST_EOF

# Pretty-print
if command -v jq &>/dev/null; then
    jq '.' "$MANIFEST_FILE" > "$MANIFEST_FILE.tmp" && mv "$MANIFEST_FILE.tmp" "$MANIFEST_FILE"
fi

log_ok "Deployment manifest saved: $MANIFEST_FILE"

# =========================================================================
# Step 5: Summary
# =========================================================================
echo ""
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}            GENESIS DEPLOYMENT COMPLETE                        ${NC}"
echo -e "${GREEN}${BOLD}════════════════════════════════════════════════════════════${NC}"
echo ""
echo "  Network:          $NETWORK"
echo "  Deployer:         $DEPLOYER_ADDR"
echo "  Contracts:        ${#ALL_CONTRACTS[@]} compiled"
echo ""
if [[ -n "$NFT_TOKEN_ID" ]]; then
    echo "  NFT Token ID:     $NFT_TOKEN_ID"
fi
if [[ -n "$TREASURY_BOX_ID" ]]; then
    echo "  Treasury Box ID:  $TREASURY_BOX_ID"
fi
if [[ -n "$GENESIS_TX_ID" ]]; then
    echo "  Genesis TX ID:    $GENESIS_TX_ID"
fi
echo ""
echo "  Files:"
echo "    Manifest:   $MANIFEST_FILE"
echo "    Hex files:  $OUTPUT_DIR/*.hex"
echo ""
echo "  Contract addresses:"
for name in "${ALL_CONTRACTS[@]}"; do
    hex_file="${OUTPUT_DIR}/${name}.hex"
    hex_len=0
    if [[ -f "$hex_file" ]]; then
        hex_len=$(wc -c < "$hex_file" | tr -d ' ')
    fi
    addr="${CONTRACT_ADDRESSES[$name]:-NOT_COMPILED}"
    printf "    %-25s %s (hex: %d bytes)\n" "$name" "$addr" "$hex_len"
done
echo ""
echo "  Next steps:"
echo "    1. Verify contracts on explorer"
echo "    2. Update xergon-agent config.toml with compiled hex values"
echo "    3. Register as provider: xergon-agent bootstrap"
echo "    4. Start serving: xergon-agent run"
echo ""

if [[ "$DRY_RUN" == true ]]; then
    log_dryrun "Dry-run complete. No transactions submitted."
fi

exit 0
