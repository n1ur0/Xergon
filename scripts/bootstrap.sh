#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Xergon Network Bootstrap Deployment Script
#
# Mints the Xergon Network NFT (singleton, supply=1) and creates the
# Treasury Box that holds the NFT + ERG reserve, guarded by the deployer PK.
#
# Uses the Ergo node wallet API (POST /wallet/payment/send) to build,
# sign, and broadcast the genesis transaction.
#
# Environment variables:
#   ERGO_NODE_URL     - Ergo node REST API URL (default: http://127.0.0.1:9053)
#   DEPLOYER_ADDRESS  - Ergo P2S/P2PK address for the Treasury Box (required)
#   TREASURY_ERG      - ERG amount to lock in Treasury (default: 1.0 ERG)
#   NFT_NAME          - NFT token name (default: "XergonNetworkNFT")
#   NFT_DESCRIPTION   - NFT token description (default: "Xergon Network Protocol Identity")
#   NFT_DECIMALS      - NFT token decimals (default: 0)
# ---------------------------------------------------------------------------

set -euo pipefail

# ---- Defaults ----
ERGO_NODE_URL="${ERGO_NODE_URL:-http://127.0.0.1:9053}"
DEPLOYER_ADDRESS="${DEPLOYER_ADDRESS:-}"
TREASURY_ERG="${TREASURY_ERG:-1.0}"
NFT_NAME="${NFT_NAME:-XergonNetworkNFT}"
NFT_DESC="${NFT_DESCRIPTION:-Xergon Network Protocol Identity}"
NFT_DECIMALS="${NFT_DECIMALS:-0}"

# Ergo constants
SAFE_MIN_BOX_VALUE=1000000      # 0.001 ERG
RECOMMENDED_MIN_FEE=1000000     # 0.001 ERG

# ---- Helpers ----
log()  { echo "[bootstrap] $*"; }
err()  { echo "[bootstrap] ERROR: $*" >&2; }
die()  { err "$@"; exit 1; }

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "Required command not found: $1"
}

# Wait for Ergo node to be ready
wait_for_node() {
    local url="$1"
    local max_wait="${2:-60}"
    local elapsed=0
    log "Waiting for Ergo node at ${url} (timeout: ${max_wait}s)..."
    while [ $elapsed -lt $max_wait ]; do
        if curl -sf "${url}/info" >/dev/null 2>&1; then
            log "Ergo node is ready."
            return 0
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done
    die "Ergo node not reachable after ${max_wait}s at ${url}"
}

# Check wallet status
check_wallet() {
    local url="$1"
    local resp
    resp=$(curl -sf "${url}/wallet/status" 2>/dev/null) || die "Cannot reach wallet API"
    local unlocked
    unlocked=$(echo "$resp" | jq -r '.isUnlocked // false')
    if [ "$unlocked" != "true" ]; then
        die "Wallet is locked. Unlock it first: curl -X POST ${url}/wallet/unlock -d '{\"pass\":\"your-pass\"}'"
    fi
    log "Wallet is unlocked."
}

# Check if treasury box already exists by looking for the Xergon Network NFT
# Since we don't know the token ID yet, we check by looking at boxes with the
# deployer address that contain a token named "XergonNetworkNFT".
# For idempotency, we also check for a state file.
check_existing_deployment() {
    local state_file="$1"
    if [ -f "$state_file" ]; then
        log "Found existing deployment state file: ${state_file}"
        local nft_id treasury_box_id tx_id
        nft_id=$(jq -r '.nft_token_id' "$state_file")
        treasury_box_id=$(jq -r '.treasury_box_id' "$state_file")
        tx_id=$(jq -r '.genesis_tx_id' "$state_file")

        # Verify the box still exists on-chain
        if [ "$nft_id" != "null" ] && [ -n "$nft_id" ]; then
            log "Verifying Treasury box still exists on-chain (NFT: ${nft_id})..."
            local boxes
            boxes=$(curl -sf "${ERGO_NODE_URL}/api/v1/boxes/unspent/byTokenId/${nft_id}" 2>/dev/null) || true
            local box_count
            box_count=$(echo "$boxes" | jq 'length' 2>/dev/null || echo "0")
            if [ "$box_count" -gt 0 ]; then
                log "Treasury box is still live on-chain."
                log "  NFT Token ID:    ${nft_id}"
                log "  Treasury Box ID: ${treasury_box_id}"
                log "  Genesis Tx ID:   ${tx_id}"
                log ""
                log "Skipping deployment -- Xergon Network is already bootstrapped."
                log "To redeploy, delete ${state_file} first."
                return 0
            else
                log "Treasury box no longer exists on-chain. Re-deploying..."
            fi
        fi
    fi
    return 1
}

# ---- Main ----
main() {
    require_cmd curl
    require_cmd jq

    log "=== Xergon Network Bootstrap ==="
    log "Node URL:       ${ERGO_NODE_URL}"
    log "Deployer:       ${DEPLOYER_ADDRESS}"
    log "Treasury ERG:   ${TREASURY_ERG}"
    log "NFT Name:       ${NFT_NAME}"
    log ""

    if [ -z "$DEPLOYER_ADDRESS" ]; then
        die "DEPLOYER_ADDRESS is required. Set it via environment variable."
    fi

    # Wait for node
    wait_for_node "$ERGO_NODE_URL"

    # Check wallet
    check_wallet "$ERGO_NODE_URL"

    # State file for idempotency
    local state_file
    state_file="$(cd "$(dirname "$0")/.." && pwd)/.xergon_bootstrap_state.json"

    # Check for existing deployment
    if check_existing_deployment "$state_file"; then
        exit 0
    fi

    # Convert ERG to nanoERG
    local treasury_nanoerg
    treasury_nanoerg=$(echo "$TREASURY_ERG * 1000000000" | bc | cut -d. -f1)
    if [ -z "$treasury_nanoerg" ] || [ "$treasury_nanoerg" -lt "$SAFE_MIN_BOX_VALUE" ]; then
        die "Treasury ERG value too low: ${TREASURY_ERG} ERG = ${treasury_nanoerg} nanoERG (minimum: ${SAFE_MIN_BOX_VALUE})"
    fi

    log "Treasury value: ${treasury_nanoerg} nanoERG"

    # Get current block height for creation height reference
    local current_height
    current_height=$(curl -sf "${ERGO_NODE_URL}/blocks/lastHeader" | jq -r '.height // 0')
    log "Current block height: ${current_height}"

    # Build the payment request to mint the NFT and create the Treasury Box.
    #
    # The Ergo node wallet API /wallet/payment/send handles:
    # - Selecting inputs from the wallet
    # - Minting tokens when a token with amount > 0 is specified on an output
    #   whose token ID matches a box being spent (first input rule)
    #
    # For NFT minting (supply=1):
    # - The token ID will be blake2b256(first_input_box_id) automatically
    # - We set amount=1 on the token
    # - The NFT name/description/decimals go in the "assets" array
    log "Building treasury creation transaction..."

    # We use the PkAddress directly. The ErgoTree for the Treasury Box
    # is derived from the deployer's P2PK/P2S address. In the real deployment,
    # this would use the compiled treasury_box.es ErgoTree instead.
    #
    # For now, we use the deployer address directly as the guard.
    # When the compiled contract is available, replace "address" with
    # "ergoTree" and use the compiled treasury_box.es hex.
    local payment_json
    payment_json=$(jq -n \
        --arg address "$DEPLOYER_ADDRESS" \
        --argjson value "$treasury_nanoerg" \
        --arg nft_name "$NFT_NAME" \
        --arg nft_desc "$NFT_DESC" \
        --argjson nft_decimals "$NFT_DECIMALS" \
        '{
            requests: [{
                address: $address,
                value: ($value | tostring),
                assets: [{
                    amount: 1,
                    name: $nft_name,
                    description: $nft_desc,
                    decimals: $nft_decimals,
                    type: "EIP-004"
                }],
                registers: {}
            }],
            fee: 1000000
        }')

    log "Payment request:"
    echo "$payment_json" | jq '.' 2>/dev/null || echo "$payment_json"
    log ""

    # Submit the transaction
    log "Submitting transaction to node wallet..."
    local tx_resp
    tx_resp=$(curl -sf -X POST \
        "${ERGO_NODE_URL}/wallet/payment/send" \
        -H "Content-Type: application/json" \
        -d "$payment_json" 2>&1) || die "Transaction submission failed: $tx_resp"

    local tx_id
    tx_id=$(echo "$tx_resp" | jq -r '.id // empty')
    if [ -z "$tx_id" ]; then
        die "No transaction ID in response: $(echo "$tx_resp" | jq '.')"
    fi

    log "Transaction submitted successfully!"
    log "  Tx ID: ${tx_id}"

    # Wait for the transaction to be included in a block
    log "Waiting for transaction confirmation..."
    local confirm_wait=0
    local max_confirm_wait=120
    while [ $confirm_wait -lt $max_confirm_wait ]; do
        local tx_info
        tx_info=$(curl -sf "${ERGO_NODE_URL}/api/v1/transactions/${tx_id}" 2>/dev/null) || true
        local num_confirmations
        num_confirmations=$(echo "$tx_info" | jq -r '.numConfirmations // 0' 2>/dev/null || echo "0")
        if [ "$num_confirmations" -gt 0 ]; then
            log "Transaction confirmed! (${num_confirmations} confirmations)"
            break
        fi
        sleep 5
        confirm_wait=$((confirm_wait + 5))
    done

    if [ $confirm_wait -ge $max_confirm_wait ]; then
        log "WARNING: Transaction not confirmed after ${max_confirm_wait}s, but it was accepted by the node."
    fi

    # Extract the NFT token ID from the transaction outputs
    # The NFT token ID = blake2b256(first_input_box_id)
    # We need to find it from the transaction's outputs
    log "Extracting NFT token ID from transaction..."

    local tx_detail
    tx_detail=$(curl -sf "${ERGO_NODE_URL}/api/v1/transactions/${tx_id}" 2>/dev/null) || \
        die "Cannot fetch transaction details"

    # Find the output that contains a token with amount=1 (the NFT)
    local nft_token_id
    nft_token_id=$(echo "$tx_detail" | jq -r '
        [.outputs[]? | select(.assets != null) | .assets[]? | select(.amount == 1)][0].tokenId // empty
    ')

    if [ -z "$nft_token_id" ]; then
        # Fallback: compute from first input box ID
        local first_input_id
        first_input_id=$(echo "$tx_detail" | jq -r '.inputs[0].boxId // empty')
        if [ -n "$first_input_id" ]; then
            log "Computing NFT token ID from first input box ID: ${first_input_id}"
            # blake2b256 of the box ID bytes (hex) gives us the token ID
            nft_token_id=$(echo -n "$first_input_id" | xxd -r -p | b2sum -l 256 | awk '{print $1}')
        fi
    fi

    if [ -z "$nft_token_id" ]; then
        die "Could not determine NFT token ID from transaction"
    fi

    log "  NFT Token ID: ${nft_token_id}"

    # Verify the treasury box exists by scanning for the NFT
    log "Verifying Treasury box on-chain..."
    sleep 3  # Brief pause for indexer

    local treasury_boxes
    treasury_boxes=$(curl -sf "${ERGO_NODE_URL}/api/v1/boxes/unspent/byTokenId/${nft_token_id}" 2>/dev/null) || \
        die "Cannot query boxes by token ID"

    local treasury_box_id
    treasury_box_id=$(echo "$treasury_boxes" | jq -r '.[0].boxId // empty')

    if [ -z "$treasury_box_id" ]; then
        err "WARNING: Treasury box not found in UTXO scan. This may take a moment."
        err "The NFT was minted (token ID: ${nft_token_id}) but the box scan returned empty."
        err "You can verify manually:"
        err "  curl ${ERGO_NODE_URL}/api/v1/boxes/unspent/byTokenId/${nft_token_id}"
        treasury_box_id="(pending confirmation)"
    else
        log "  Treasury Box ID: ${treasury_box_id}"

        # Verify box value
        local box_value
        box_value=$(echo "$treasury_boxes" | jq -r '.[0].value // 0')
        log "  Treasury Value:  ${box_value} nanoERG ($(echo "scale=9; ${box_value} / 1000000000" | bc) ERG)"
    fi

    # Save deployment state for idempotency
    log "Saving deployment state..."
    cat > "$state_file" <<EOF
{
    "nft_token_id": "${nft_token_id}",
    "treasury_box_id": "${treasury_box_id}",
    "genesis_tx_id": "${tx_id}",
    "deployer_address": "${DEPLOYER_ADDRESS}",
    "treasury_erg": "${TREASURY_ERG}",
    "treasury_nanoerg": ${treasury_nanoerg},
    "block_height": ${current_height},
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

    log ""
    log "=== Bootstrap Complete ==="
    log "  NFT Token ID:    ${nft_token_id}"
    log "  Treasury Box ID: ${treasury_box_id}"
    log "  Genesis Tx ID:   ${tx_id}"
    log ""
    log "State saved to: ${state_file}"
    log ""
    log "IMPORTANT: Add these to your xergon-agent config.toml:"
    log "  [protocol]"
    log "  network_nft_token_id = \"${nft_token_id}\""
    log "  treasury_box_id = \"${treasury_box_id}\""
}

main "$@"
