#!/usr/bin/env bash
# =========================================================================
# Xergon Network -- Testnet Contract Deployment Script
# =========================================================================
#
# Compiles and deploys all Xergon Network ErgoScript contracts to the
# Ergo testnet via the node REST API.
#
# Compilation: POST /script/p2sAddress  (ErgoScript source -> P2S address)
# Box funding: POST /wallet/payment/send  (ERG to P2S address)
#
# The deployer address (DEPLOYER_ADDRESS) is required for:
#   - treasury.ergo: embeds deployer PK as the only authorized spender
#   - provider_slashing.ergo: embeds treasury address for slash penalties
#
# Usage:
#   export DEPLOYER_ADDRESS="3WwxnK..."
#   export ERGO_NODE_URL="http://127.0.0.1:9053"
#   export ERGO_API_KEY="my-api-key"
#   ./scripts/deploy-testnet.sh
#
# Flags:
#   --dry-run      Compile all contracts but do NOT send transactions
#   --treasury-only Only deploy treasury.ergo (requires DEPLOYER_ADDRESS)
#   --force        Skip confirmation prompt
#   --verbose      Show full API request/response bodies
#
# =========================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG_FILE="${CONTRACTS_DIR}/testnet-config.toml"
MANIFEST_FILE="${CONTRACTS_DIR}/deployment-manifest.json"

# Node defaults (overridable via env)
ERGO_NODE_URL="${ERGO_NODE_URL:-http://127.0.0.1:9053}"
ERGO_API_KEY="${ERGO_API_KEY:-}"
DEPLOYER_ADDRESS="${DEPLOYER_ADDRESS:-}"

# Treasury address for slashing contract (defaults to deployer if not set)
TREASURY_ADDRESS="${TREASURY_ADDRESS:-$DEPLOYER_ADDRESS}"

# Defaults
DRY_RUN=false
TREASURY_ONLY=false
FORCE=false
VERBOSE=false
FEE_NANOERG=1000000  # 0.001 ERG default fee

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ---------------------------------------------------------------------------
# All contract files (relative to contracts/)
# ---------------------------------------------------------------------------
CONTRACTS=(
    "provider_box.ergo"
    "user_staking.ergo"
    "usage_proof.ergo"
    "treasury.ergo"
    "governance_proposal.ergo"
    "payment_bridge.es"
    "gpu_rental.es"
    "gpu_rental_listing.es"
    "gpu_rating.es"
    "provider_slashing.es"
    "provider_slashing.ergo"
    "relay_registry.es"
    "usage_commitment.es"
    "governance_proposal.es"
)

# Contracts that require DEPLOYER_ADDRESS substitution
DEPLOYER_CONTRACTS=("treasury.ergo")

# Contracts that require TREASURY_ADDRESS substitution
TREASURY_CONTRACTS=("provider_slashing.ergo")

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --treasury-only)
            TREASURY_ONLY=true
            shift
            ;;
        --force)
            FORCE=true
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --dry-run        Compile only, no transactions"
            echo "  --treasury-only  Only deploy treasury.ergo"
            echo "  --force          Skip confirmation prompt"
            echo "  --verbose        Show full API responses"
            echo "  -h, --help       Show this help"
            echo ""
            echo "Environment:"
            echo "  DEPLOYER_ADDRESS  Required for treasury.ergo"
            echo "  ERGO_NODE_URL     Node REST URL (default: http://127.0.0.1:9053)"
            echo "  ERGO_API_KEY      Node API key if auth is enabled"
            echo "  TREASURY_ADDRESS  For provider_slashing.ergo (default: DEPLOYER_ADDRESS)"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Utility functions
# ---------------------------------------------------------------------------
log_info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# Build curl args with optional API key header
curl_args() {
    local args=(-s -X POST)
    if [[ -n "$ERGO_API_KEY" ]]; then
        args+=(-H "api_key: ${ERGO_API_KEY}")
    fi
    echo "${args[@]}"
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
log_info "Running pre-flight checks..."

# Check for required tools
for cmd in curl jq; do
    if ! command -v "$cmd" &>/dev/null; then
        log_error "Required tool not found: $cmd"
        log_error "Install with: brew install $cmd (macOS) or apt install $cmd (Linux)"
        exit 1
    fi
done
log_ok "curl and jq are available"

# Check node connectivity
if ! curl -s "${ERGO_NODE_URL}/info" | jq -e '.fullHeight' &>/dev/null; then
    log_error "Cannot connect to Ergo node at ${ERGO_NODE_URL}"
    log_error "Ensure the node is running and synced."
    exit 1
fi

NODE_HEIGHT=$(curl -s "${ERGO_NODE_URL}/info" | jq -r '.fullHeight')
log_ok "Connected to Ergo node (height: ${NODE_HEIGHT})"

# Check network is testnet
NODE_NETWORK=$(curl -s "${ERGO_NODE_URL}/info" | jq -r '.network')
if [[ "$NODE_NETWORK" != "testnet" ]]; then
    log_warn "Node network is '${NODE_NETWORK}', expected 'testnet'"
    log_warn "Proceed with caution -- this may be a mainnet node!"
    if [[ "$FORCE" != true ]]; then
        read -p "Continue anyway? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_info "Aborted."
            exit 0
        fi
    fi
else
    log_ok "Node is on testnet"
fi

# Check wallet status (needed for funding boxes)
WALLET_STATUS=$(curl -s "${ERGO_NODE_URL}/wallet/status" | jq -r '.isInitialized // empty')
if [[ -z "$WALLET_STATUS" || "$WALLET_STATUS" != "true" ]]; then
    log_warn "Node wallet may not be initialized or unlocked"
    log_warn "Box funding will not work without an unlocked wallet"
    if [[ "$DRY_RUN" != true && "$FORCE" != true ]]; then
        read -p "Continue without wallet? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_info "Aborted."
            exit 0
        fi
    fi
else
    log_ok "Node wallet is initialized"
fi

# Check DEPLOYER_ADDRESS for contracts that need it
if [[ -z "$DEPLOYER_ADDRESS" ]]; then
    if [[ "$TREASURY_ONLY" == true ]]; then
        log_error "DEPLOYER_ADDRESS is required for treasury.ergo deployment"
        log_error "Set it via: export DEPLOYER_ADDRESS=\"3WwxnK...\""
        exit 1
    else
        log_warn "DEPLOYER_ADDRESS not set -- treasury.ergo and provider_slashing.ergo will be skipped"
        log_warn "Set via: export DEPLOYER_ADDRESS=\"3WwxnK...\""
    fi
else
    log_ok "Deployer address: ${DEPLOYER_ADDRESS}"
fi

# ---------------------------------------------------------------------------
# Compile contract: ErgoScript source -> P2S address via node API
# ---------------------------------------------------------------------------
compile_contract() {
    local contract_file="$1"
    local contract_path="${CONTRACTS_DIR}/${contract_file}"
    local contract_name="${contract_file%.*}"

    if [[ ! -f "$contract_path" ]]; then
        log_error "Contract file not found: ${contract_path}"
        return 1
    fi

    log_info "Compiling ${contract_file}..."

    # Read contract source
    local source
    source=$(cat "$contract_path")

    # Call node /script/p2sAddress endpoint
    # This endpoint accepts ErgoScript source and returns a P2S address
    local response
    response=$(curl -s -X POST "${ERGO_NODE_URL}/script/p2sAddress" \
        -H "Content-Type: application/json" \
        ${ERGO_API_KEY:+-H "api_key: ${ERGO_API_KEY}"} \
        -d "{\"source\": $(echo "$source" | jq -Rs .), \"treeVersion\": \"0\"}")

    if [[ "$VERBOSE" == true ]]; then
        echo "  Request source length: ${#source} chars"
        echo "  Response: ${response}"
    fi

    # Check for compilation errors
    local error
    error=$(echo "$response" | jq -r '.error // empty')
    if [[ -n "$error" ]]; then
        log_error "Compilation failed for ${contract_file}: ${error}"
        return 1
    fi

    local p2s_address
    p2s_address=$(echo "$response" | jq -r '.address // empty')
    if [[ -z "$p2s_address" || "$p2s_address" == "null" ]]; then
        log_error "No P2S address returned for ${contract_file}"
        log_error "Response: ${response}"
        return 1
    fi

    log_ok "${contract_file} -> ${p2s_address}"
    echo "$p2s_address"
}

# ---------------------------------------------------------------------------
# Fund a box at a P2S address (sends ERG from node wallet)
# ---------------------------------------------------------------------------
fund_box() {
    local p2s_address="$1"
    local amount_nanoerg="$2"
    local label="$3"

    log_info "Funding ${label} with $(echo "scale=6; $amount_nanoerg / 1000000000" | bc) ERG..."

    local response
    response=$(curl -s -X POST "${ERGO_NODE_URL}/wallet/payment/send" \
        -H "Content-Type: application/json" \
        ${ERGO_API_KEY:+-H "api_key: ${ERGO_API_KEY}"} \
        -d "{
            \"requests\": [{
                \"address\": \"${p2s_address}\",
                \"value\": ${amount_nanoerg},
                \"assets\": []
            }],
            \"fee\": ${FEE_NANOERG},
            \"inputsRaw\": []
        }")

    if [[ "$VERBOSE" == true ]]; then
        echo "  Response: ${response}"
    fi

    local tx_id
    tx_id=$(echo "$response" | jq -r '.id // empty')
    if [[ -z "$tx_id" || "$tx_id" == "null" ]]; then
        local error
        error=$(echo "$response" | jq -r '.detail // .error // "unknown error"')
        log_error "Funding failed for ${label}: ${error}"
        return 1
    fi

    log_ok "${label} funded in TX: ${tx_id}"
    echo "$tx_id"
}

# ---------------------------------------------------------------------------
# Substitute placeholder addresses in contract source
# ---------------------------------------------------------------------------
substitute_address() {
    local source="$1"
    local placeholder="$2"
    local address="$3"

    echo "$source" | sed "s|\"${placeholder}\"|\"${address}\"|g"
}

# ---------------------------------------------------------------------------
# Main deployment loop
# ---------------------------------------------------------------------------
log_info "Starting deployment${DRY_RUN:+ (DRY RUN -- no transactions)}..."
echo ""

# Initialize manifest
MANIFEST=$(jq -n '{
    deployed_at: (now | todate),
    network: "testnet",
    node_url: $node_url,
    deployer_address: $deployer,
    contracts: []
}' --arg node_url "$ERGO_NODE_URL" --arg deployer "${DEPLOYER_ADDRESS:-UNSET}")

DEPLOY_COUNT=0
SKIP_COUNT=0
ERROR_COUNT=0

for contract_file in "${CONTRACTS[@]}"; do
    contract_name="${contract_file%.*}"
    contract_path="${CONTRACTS_DIR}/${contract_file}"

    # Filter for --treasury-only
    if [[ "$TREASURY_ONLY" == true && "$contract_file" != "treasury.ergo" ]]; then
        continue
    fi

    echo "---"
    log_info "Processing: ${contract_file}"

    # Read source
    source=$(cat "$contract_path")

    # Check if this contract needs DEPLOYER_ADDRESS
    if printf '%s\n' "${DEPLOYER_CONTRACTS[@]}" | grep -qF "$contract_file"; then
        if [[ -z "$DEPLOYER_ADDRESS" ]]; then
            log_warn "Skipping ${contract_file} (DEPLOYER_ADDRESS not set)"
            SKIP_COUNT=$((SKIP_COUNT + 1))
            continue
        fi
        source=$(substitute_address "$source" "DEPLOYER_ADDRESS_HERE" "$DEPLOYER_ADDRESS")
        log_info "Substituted DEPLOYER_ADDRESS in ${contract_file}"
    fi

    # Check if this contract needs TREASURY_ADDRESS
    if printf '%s\n' "${TREASURY_CONTRACTS[@]}" | grep -qF "$contract_file"; then
        if [[ -z "$TREASURY_ADDRESS" ]]; then
            log_warn "Skipping ${contract_file} (TREASURY_ADDRESS not set)"
            SKIP_COUNT=$((SKIP_COUNT + 1))
            continue
        fi
        source=$(substitute_address "$source" "TREASURY_ADDRESS_HERE" "$TREASURY_ADDRESS")
        log_info "Substituted TREASURY_ADDRESS in ${contract_file}"
    fi

    # Write substituted source to temp file for compilation
    tmp_source=$(mktemp)
    echo "$source" > "$tmp_source"

    # Compile (using temp file with substitutions)
    response=$(curl -s -X POST "${ERGO_NODE_URL}/script/p2sAddress" \
        -H "Content-Type: application/json" \
        ${ERGO_API_KEY:+-H "api_key: ${ERGO_API_KEY}"} \
        -d "{\"source\": $(cat \"$tmp_source\" | jq -Rs .), \"treeVersion\": \"0\"}")

    rm -f "$tmp_source"

    # Check for errors
    error=$(echo "$response" | jq -r '.error // empty')
    if [[ -n "$error" ]]; then
        log_error "Compilation failed for ${contract_file}: ${error}"
        ERROR_COUNT=$((ERROR_COUNT + 1))
        # Add error to manifest
        MANIFEST=$(echo "$MANIFEST" | jq \
            --arg name "$contract_name" \
            --arg file "$contract_file" \
            --arg err "$error" \
            '.contracts += [{
                name: $name,
                file: $file,
                status: "error",
                error: $err
            }]')
        continue
    fi

    p2s_address=$(echo "$response" | jq -r '.address // empty')
    if [[ -z "$p2s_address" || "$p2s_address" == "null" ]]; then
        log_error "No P2S address returned for ${contract_file}"
        ERROR_COUNT=$((ERROR_COUNT + 1))
        continue
    fi

    log_ok "Compiled: ${contract_file} -> ${p2s_address}"

    # Add to manifest
    MANIFEST=$(echo "$MANIFEST" | jq \
        --arg name "$contract_name" \
        --arg file "$contract_file" \
        --arg addr "$p2s_address" \
        '.contracts += [{
            name: $name,
            file: $file,
            p2s_address: $addr,
            status: "compiled"
        }]')

    # Skip funding in dry-run mode
    if [[ "$DRY_RUN" == true ]]; then
        log_warn "DRY RUN: skipping box funding for ${contract_name}"
        DEPLOY_COUNT=$((DEPLOY_COUNT + 1))
        continue
    fi

    # Fund the box (sends minimum ERG from node wallet)
    # Different contracts have different minimum value requirements
    case "$contract_name" in
        treasury)
            fund_amount=50000000  # 0.05 ERG for treasury
            ;;
        provider_slashing)
            fund_amount=50000000  # 0.05 ERG for slashing stake
            ;;
        governance_proposal)
            fund_amount=10000000  # 0.01 ERG
            ;;
        *)
            fund_amount=1000000   # 0.001 ERG minimum box value
            ;;
    esac

    tx_id=$(fund_box "$p2s_address" "$fund_amount" "$contract_name") || {
        log_error "Failed to fund ${contract_name}"
        ERROR_COUNT=$((ERROR_COUNT + 1))
        # Update manifest with error
        MANIFEST=$(echo "$MANIFEST" | jq \
            --arg name "$contract_name" \
            --arg err "funding_failed" \
            '(.contracts | last) += {status: "error", error: $err}')
        continue
    }

    # Update manifest with TX ID
    MANIFEST=$(echo "$MANIFEST" | jq \
        --arg tx "$tx_id" \
        --arg amount "$fund_amount" \
        '(.contracts | last) += {
            status: "deployed",
            tx_id: $tx,
            funded_amount_nanoerg: ($amount | tonumber)
        }')

    DEPLOY_COUNT=$((DEPLOY_COUNT + 1))
done

# ---------------------------------------------------------------------------
# Write deployment manifest
# ---------------------------------------------------------------------------
echo "$MANIFEST" | jq '.' > "$MANIFEST_FILE"
echo ""
log_ok "Deployment manifest written to: ${MANIFEST_FILE}"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "========================================"
echo "  Deployment Summary"
echo "========================================"
echo "  Mode:          ${DRY_RUN:-LIVE}"
echo "  Node:          ${ERGO_NODE_URL}"
echo "  Network:       ${NODE_NETWORK}"
echo "  Deployer:      ${DEPLOYER_ADDRESS:-UNSET}"
echo "  Deployed:      ${DEPLOY_COUNT}"
echo "  Skipped:       ${SKIP_COUNT}"
echo "  Errors:        ${ERROR_COUNT}"
echo "  Manifest:      ${MANIFEST_FILE}"
echo "========================================"

if [[ "$ERROR_COUNT" -gt 0 ]]; then
    echo ""
    log_warn "Some contracts failed to deploy. Check the manifest for details."
    exit 1
fi

log_ok "Done!"
