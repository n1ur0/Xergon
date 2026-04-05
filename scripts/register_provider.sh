#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Xergon Network Provider Registration Script
#
# Creates a Provider Box with a new per-provider NFT (singleton, supply=1).
# The NFT token ID = blake2b256(first_input_box_id) per Ergo's minting rule.
#
# The Provider Box registers:
#   R4 = providerPK (GroupElement)
#   R5 = endpointURL (Coll[Byte], UTF-8)
#   R6 = modelsJSON (Coll[Byte], UTF-8 JSON array)
#   R7 = pownScore (Int, initial 0)
#   R8 = lastHeartbeat (Int, initial 0)
#   R9 = region (Coll[Byte], UTF-8)
#
# Environment variables:
#   ERGO_NODE_URL     - Ergo node REST API URL (default: http://127.0.0.1:9053)
#   XERGON_API_URL    - Xergon agent API URL (default: http://127.0.0.1:9090)
#   PROVIDER_PK_HEX   - Provider public key in hex (required)
#
# Usage:
#   ./register_provider.sh <provider_id> <endpoint> <region> [models_json]
#
# Examples:
#   ./register_provider.sh provider-1 "http://192.168.1.5:9099" "us-east" '["llama-3.1-8b","mistral-7b"]'
#   ./register_provider.sh provider-2 "http://10.0.0.1:9099" "eu-west"
# ---------------------------------------------------------------------------

set -euo pipefail

# ---- Defaults ----
ERGO_NODE_URL="${ERGO_NODE_URL:-http://127.0.0.1:9053}"
XERGON_API_URL="${XERGON_API_URL:-http://127.0.0.1:9090}"
PROVIDER_PK_HEX="${PROVIDER_PK_HEX:-}"

SAFE_MIN_BOX_VALUE=1000000      # 0.001 ERG
RECOMMENDED_MIN_FEE=1000000     # 0.001 ERG

# ---- Helpers ----
log()  { echo "[register_provider] $*"; }
err()  { echo "[register_provider] ERROR: $*" >&2; }
die()  { err "$@"; exit 1; }

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "Required command not found: $1"
}

# Encode a string as Sigma Coll[Byte] hex: 0e <vlb_length> <utf8_bytes>
encode_coll_byte() {
    local str="$1"
    local hex_bytes
    hex_bytes=$(echo -n "$str" | xxd -p | tr -d '\n')
    local len=${#str}
    local vlb
    if [ "$len" -lt 128 ]; then
        vlb=$(printf '%02x' "$len")
    else
        local hi=$(( (len >> 7) | 0x80 ))
        local lo=$(( len & 0x7F ))
        vlb=$(printf '%02x%02x' "$hi" "$lo")
    fi
    echo "0e${vlb}${hex_bytes}"
}

# Encode an Int (4 bytes big-endian) as Sigma constant hex: 04 <4 bytes>
encode_int() {
    local val="$1"
    # Handle negative values by using 2's complement
    if [ "$val" -lt 0 ]; then
        # Convert to unsigned 32-bit 2's complement
        val=$(( (1 << 32) + val ))
    fi
    printf '04%08x' "$val"
}

# Encode a GroupElement (33-byte compressed secp256k1 point) as Coll[Byte]
encode_group_element() {
    local pk_hex="$1"
    # GroupElement is stored as Coll[Byte] with a 0e prefix and length byte
    local len=$(( ${#pk_hex} / 2 ))
    local vlb
    if [ "$len" -lt 128 ]; then
        vlb=$(printf '%02x' "$len")
    else
        local hi=$(( (len >> 7) | 0x80 ))
        local lo=$(( len & 0x7F ))
        vlb=$(printf '%02x%02x' "$hi" "$lo")
    fi
    echo "0e${vlb}${pk_hex}"
}

# ---- Main ----
main() {
    if [ $# -lt 3 ]; then
        echo "Usage: $0 <provider_id> <endpoint> <region> [models_json]"
        echo ""
        echo "Arguments:"
        echo "  provider_id  - Unique provider identifier (e.g., provider-1)"
        echo "  endpoint     - Provider endpoint URL (e.g., http://192.168.1.5:9099)"
        echo "  region       - Provider region code (e.g., us-east, eu-west)"
        echo "  models_json  - JSON array of model names (default: '[]')"
        echo ""
        echo "Environment variables:"
        echo "  ERGO_NODE_URL     - Ergo node REST API URL"
        echo "  XERGON_API_URL    - Xergon agent API URL"
        echo "  PROVIDER_PK_HEX   - Provider public key in hex (33 bytes compressed)"
        echo ""
        echo "Examples:"
        echo "  $0 provider-1 'http://192.168.1.5:9099' us-east '[\"llama-3.1-8b\"]'"
        exit 1
    fi

    local provider_id="$1"
    local endpoint="$2"
    local region="$3"
    local models_json="${4:-[]}"

    require_cmd curl
    require_cmd jq

    log "=== Xergon Provider Registration ==="
    log "Provider ID:  ${provider_id}"
    log "Endpoint:     ${endpoint}"
    log "Region:       ${region}"
    log "Models:       ${models_json}"
    log "Node URL:     ${ERGO_NODE_URL}"
    log "Agent API:    ${XERGON_API_URL}"
    log ""

    if [ -z "$PROVIDER_PK_HEX" ]; then
        die "PROVIDER_PK_HEX is required. Set it via environment variable."
    fi

    # Validate PK length (33 bytes = 66 hex chars for compressed secp256k1)
    if [ ${#PROVIDER_PK_HEX} -ne 66 ]; then
        die "PROVIDER_PK_HEX must be 66 hex chars (33 bytes compressed secp256k1), got ${#PROVIDER_PK_HEX}"
    fi

    # ---- Method 1: Try via Xergon Agent API first ----
    log "Attempting registration via Xergon Agent API..."

    local api_resp
    api_resp=$(curl -sf -X POST \
        "${XERGON_API_URL}/api/v1/providers/register" \
        -H "Content-Type: application/json" \
        -d "$(jq -n \
            --arg provider_id "$provider_id" \
            --arg endpoint "$endpoint" \
            --arg region "$region" \
            --arg models "$models_json" \
            --arg pk_hex "$PROVIDER_PK_HEX" \
            '{
                provider_id: $provider_id,
                endpoint: $endpoint,
                region: $region,
                models: ($models | fromjson),
                pk_hex: $pk_hex
            }')" 2>/dev/null) && {
        local api_tx_id
        api_tx_id=$(echo "$api_resp" | jq -r '.tx_id // .transactionId // empty')
        local api_nft_id
        api_nft_id=$(echo "$api_resp" | jq -r '.nft_token_id // .nftTokenId // empty')
        if [ -n "$api_tx_id" ]; then
            log "Registration submitted via Agent API!"
            log "  Tx ID:       ${api_tx_id}"
            log "  NFT Token ID: ${api_nft_id:-(pending)}"
            log ""
            log "Add to your agent config:"
            log "  [[providers]]"
            log "  id = \"${provider_id}\""
            log "  nft_token_id = \"${api_nft_id:-<from_tx>}\""
            exit 0
        fi
    }

    log "Agent API not available or returned error. Falling back to direct node API."
    log ""

    # ---- Method 2: Direct node wallet API ----
    # Check node is reachable
    curl -sf "${ERGO_NODE_URL}/info" >/dev/null 2>&1 || \
        die "Ergo node not reachable at ${ERGO_NODE_URL}"

    # Check wallet
    local wallet_resp
    wallet_resp=$(curl -sf "${ERGO_NODE_URL}/wallet/status" 2>/dev/null) || \
        die "Cannot reach wallet API"
    local unlocked
    unlocked=$(echo "$wallet_resp" | jq -r '.isUnlocked // false')
    if [ "$unlocked" != "true" ]; then
        die "Wallet is locked. Unlock it first."
    fi

    # Load the provider_box.es compiled ErgoTree (if available)
    local provider_tree_hex=""
    local agent_contracts_dir
    agent_contracts_dir="$(cd "$(dirname "$0")/../xergon-agent/contracts/compiled" 2>/dev/null && pwd)" 2>/dev/null || true
    if [ -n "$agent_contracts_dir" ] && [ -f "${agent_contracts_dir}/provider_box.hex" ]; then
        provider_tree_hex=$(cat "${agent_contracts_dir}/provider_box.hex" | tr -d '[:space:]')
        log "Using compiled provider_box.es ErgoTree (${#provider_tree_hex} chars)"
    else
        log "WARNING: No compiled provider_box.es found. Using deployer address as guard."
        log "  For production, compile contracts first: ./compile_contracts.sh"
    fi

    # Encode registers
    local r4_pk r5_endpoint r6_models r7_pown r8_heartbeat r9_region
    r4_pk=$(encode_group_element "$PROVIDER_PK_HEX")
    r5_endpoint=$(encode_coll_byte "$endpoint")
    r6_models=$(encode_coll_byte "$models_json")
    r7_pown=$(encode_int 0)
    r8_heartbeat=$(encode_int 0)
    r9_region=$(encode_coll_byte "$region")

    log "Encoded registers:"
    log "  R4 (providerPK): ${r4_pk:0:20}..."
    log "  R5 (endpoint):   ${r5_endpoint:0:20}..."
    log "  R6 (models):     ${r6_models:0:20}..."
    log "  R7 (pownScore):  ${r7_pown}"
    log "  R8 (heartbeat):  ${r8_heartbeat}"
    log "  R9 (region):     ${r9_region:0:20}..."
    log ""

    # Build the payment request
    # The NFT is minted by specifying a token with amount=1 on the first output.
    # The token ID is automatically set to blake2b256(first_input_box_id).
    local guard_target
    if [ -n "$provider_tree_hex" ]; then
        guard_target="ergoTree"
        guard_value="$provider_tree_hex"
    else
        # Use DEPLOYER_ADDRESS or PROVIDER_PK_HEX derived address
        # For now, we need a P2S address - use environment variable
        guard_target="address"
        guard_value="${DEPLOYER_ADDRESS:-$(echo "$PROVIDER_PK_HEX" | xxd -r -p | base58 - 2>/dev/null || echo "")}"
        if [ -z "$guard_value" ]; then
            die "Cannot derive address from PK. Set DEPLOYER_ADDRESS env var."
        fi
    fi

    local payment_json
    payment_json=$(jq -n \
        --arg target "$guard_target" \
        --arg value "$guard_value" \
        --argjson box_value "$SAFE_MIN_BOX_VALUE" \
        --arg r4 "$r4_pk" \
        --arg r5 "$r5_endpoint" \
        --arg r6 "$r6_models" \
        --arg r7 "$r7_pown" \
        --arg r8 "$r8_heartbeat" \
        --arg r9 "$r9_region" \
        --arg nft_name "XergonProvider-${provider_id}" \
        '{
            requests: [{
                (\$target): \$value,
                value: (\$box_value | tostring),
                assets: [{
                    amount: 1,
                    name: \$nft_name,
                    description: ("Xergon Provider NFT: " + \$nft_name),
                    decimals: 0,
                    type: "EIP-004"
                }],
                registers: {
                    R4: \$r4,
                    R5: \$r5,
                    R6: \$r6,
                    R7: \$r7,
                    R8: \$r8,
                    R9: \$r9
                }
            }],
            fee: 1100000
        }')

    log "Payment request:"
    echo "$payment_json" | jq '.' 2>/dev/null || echo "$payment_json"
    log ""

    # Submit transaction
    log "Submitting provider registration transaction..."
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

    log "Transaction submitted!"
    log "  Tx ID: ${tx_id}"

    # Wait for confirmation
    log "Waiting for confirmation..."
    local confirm_wait=0
    while [ $confirm_wait -lt 120 ]; do
        local tx_info
        tx_info=$(curl -sf "${ERGO_NODE_URL}/api/v1/transactions/${tx_id}" 2>/dev/null) || true
        local num_confirmations
        num_confirmations=$(echo "$tx_info" | jq -r '.numConfirmations // 0' 2>/dev/null || echo "0")
        if [ "$num_confirmations" -gt 0 ]; then
            log "Confirmed! (${num_confirmations} confirmations)"
            break
        fi
        sleep 5
        confirm_wait=$((confirm_wait + 5))
    done

    # Extract the NFT token ID
    local tx_detail
    tx_detail=$(curl -sf "${ERGO_NODE_URL}/api/v1/transactions/${tx_id}" 2>/dev/null) || \
        die "Cannot fetch transaction details"

    local nft_token_id
    nft_token_id=$(echo "$tx_detail" | jq -r '
        [.outputs[]? | select(.assets != null) | .assets[]? | select(.amount == 1)][0].tokenId // empty
    ')

    if [ -z "$nft_token_id" ]; then
        # Fallback: compute from first input
        local first_input_id
        first_input_id=$(echo "$tx_detail" | jq -r '.inputs[0].boxId // empty')
        if [ -n "$first_input_id" ]; then
            nft_token_id=$(echo -n "$first_input_id" | xxd -r -p | b2sum -l 256 | awk '{print $1}')
        fi
    fi

    # Find the provider box
    local provider_box_id=""
    if [ -n "$nft_token_id" ]; then
        sleep 2
        local boxes
        boxes=$(curl -sf "${ERGO_NODE_URL}/api/v1/boxes/unspent/byTokenId/${nft_token_id}" 2>/dev/null) || true
        provider_box_id=$(echo "$boxes" | jq -r '.[0].boxId // empty')
    fi

    log ""
    log "=== Provider Registration Complete ==="
    log "  Provider ID:     ${provider_id}"
    log "  NFT Token ID:    ${nft_token_id:-(pending)}"
    log "  Provider Box ID: ${provider_box_id:-(pending)}"
    log "  Tx ID:           ${tx_id}"
    log "  Endpoint:        ${endpoint}"
    log "  Region:          ${region}"
    log ""
    log "Add to your agent config.toml:"
    log "  [[providers]]"
    log "  id = \"${provider_id}\""
    log "  nft_token_id = \"${nft_token_id:-<from_tx>}\""
    log "  endpoint = \"${endpoint}\""
    log "  region = \"${region}\""
    log "  models = ${models_json}"
}

main "$@"
