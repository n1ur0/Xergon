#!/usr/bin/env bash
#
# create_staking.sh — Create a User Staking Box on the Ergo testnet.
#
# This script calls the Ergo node API directly to:
#   1. Compile user_staking.es via POST /script/p2sAddress
#   2. Build & submit a transaction via POST /wallet/payment/send
#   3. Print the resulting tx_id, box_id, and staking address
#
# Usage:
#   ./scripts/create_staking.sh \
#     --node-url http://127.0.0.1:9053 \
#     --user-pk-hex 02<66 hex chars> \
#     --amount-erg 1.0 \
#     [--contract contracts/user_staking.es]
#
# Environment variables:
#   ERG_NODE_URL  — Ergo node URL (default: http://127.0.0.1:9053)
#   USER_PK_HEX   — User's compressed secp256k1 public key (hex, 66 chars)
#   AMOUNT_ERG    — ERG amount to lock in the staking box (default: 1.0)
#
set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
NODE_URL="${ERG_NODE_URL:-http://127.0.0.1:9053}"
USER_PK_HEX="${USER_PK_HEX:-}"
AMOUNT_ERG="${AMOUNT_ERG:-1.0}"
CONTRACT_FILE="contracts/user_staking.es"
FEE_NANOERG=1100000  # 0.0011 ERG

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --node-url)
            NODE_URL="$2"; shift 2 ;;
        --user-pk-hex)
            USER_PK_HEX="$2"; shift 2 ;;
        --amount-erg)
            AMOUNT_ERG="$2"; shift 2 ;;
        --contract)
            CONTRACT_FILE="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: $0 [--node-url URL] [--user-pk-hex HEX] [--amount-erg ERG] [--contract FILE]"
            exit 0 ;;
        *)
            echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------
if [[ -z "$USER_PK_HEX" ]]; then
    echo "ERROR: USER_PK_HEX is required (33-byte compressed secp256k1 public key, 66 hex chars)" >&2
    echo "  Set via --user-pk-hex or USER_PK_HEX env var" >&2
    exit 1
fi

if [[ ${#USER_PK_HEX} -ne 66 ]]; then
    echo "ERROR: USER_PK_HEX must be 66 hex characters (33 bytes), got ${#USER_PK_HEX}" >&2
    exit 1
fi

# Validate hex
if ! echo "$USER_PK_HEX" | grep -qE '^[0-9a-fA-F]{66}$'; then
    echo "ERROR: USER_PK_HEX is not valid hex" >&2
    exit 1
fi

# Convert ERG to nanoERG
AMOUNT_NANOERG=$(echo "$AMOUNT_ERG * 1000000000" | bc | cut -d. -f1)
if [[ "$AMOUNT_NANOERG" -lt 1000000 ]]; then
    echo "ERROR: Amount must be at least 0.001 ERG (1,000,000 nanoERG)" >&2
    exit 1
fi

# Strip trailing slash from node URL
NODE_URL="${NODE_URL%/}"

echo "=== Xergon Network: Create User Staking Box ==="
echo "Node URL:        $NODE_URL"
echo "User PK prefix:  ${USER_PK_HEX:0:8}..."
echo "Amount:          $AMOUNT_ERG ERG ($AMOUNT_NANOERG nanoERG)"
echo "Contract file:   $CONTRACT_FILE"
echo ""

# ---------------------------------------------------------------------------
# Step 0: Check node is synced
# ---------------------------------------------------------------------------
echo "[0/3] Checking node status..."
NODE_INFO=$(curl -sf "$NODE_URL/info" 2>/dev/null) || {
    echo "ERROR: Cannot connect to Ergo node at $NODE_URL" >&2
    exit 1
}

HEADERS_HEIGHT=$(echo "$NODE_INFO" | jq -r '.headersHeight // 0')
FULL_HEIGHT=$(echo "$NODE_INFO" | jq -r '.fullHeight // 0')

echo "  Headers height: $HEADERS_HEIGHT"
echo "  Full height:    $FULL_HEIGHT"

HEIGHT_DIFF=$((HEADERS_HEIGHT - FULL_HEIGHT))
if [[ "$HEADERS_HEIGHT" -eq 0 ]] || [[ "$HEIGHT_DIFF" -gt 10 ]]; then
    echo "WARNING: Node may not be fully synced (diff=$HEIGHT_DIFF). Proceeding anyway."
fi

# Check wallet is unlocked
WALLET_STATUS=$(curl -sf "$NODE_URL/wallet/status" 2>/dev/null) || {
    echo "ERROR: Cannot check wallet status. Is the wallet enabled?" >&2
    exit 1
}
WALLET_UNLOCKED=$(echo "$WALLET_STATUS" | jq -r '.isUnlocked // false')
if [[ "$WALLET_UNLOCKED" != "true" ]]; then
    echo "ERROR: Wallet is locked. Unlock it before creating a staking box." >&2
    exit 1
fi
echo "  Wallet: unlocked"

# ---------------------------------------------------------------------------
# Step 1: Compile user_staking.es via node /script/p2sAddress
# ---------------------------------------------------------------------------
echo ""
echo "[1/3] Compiling user_staking.es contract..."

if [[ ! -f "$CONTRACT_FILE" ]]; then
    echo "ERROR: Contract file not found: $CONTRACT_FILE" >&2
    exit 1
fi

CONTRACT_SOURCE=$(cat "$CONTRACT_FILE")

COMPILE_RESPONSE=$(curl -sf -X POST "$NODE_URL/script/p2sAddress" \
    -H "Content-Type: application/json" \
    -d "$(jq -n --arg source "$CONTRACT_SOURCE" '{source: $source}')" 2>/dev/null) || {
    echo "ERROR: Failed to compile contract via node. Is the source valid?" >&2
    echo "  Node response: $COMPILE_RESPONSE" >&2
    exit 1
}

STAKING_ADDRESS=$(echo "$COMPILE_RESPONSE" | jq -r '.address // empty')

if [[ -z "$STAKING_ADDRESS" ]]; then
    echo "ERROR: No address returned from contract compilation" >&2
    echo "  Response: $COMPILE_RESPONSE" >&2
    exit 1
fi

echo "  Staking address: $STAKING_ADDRESS"

# Extract ErgoTree hex from the P2S address (base58 decode, skip network prefix + checksum)
ERGO_TREE_HEX=$(python3 -c "
import base58, hashlib, sys
addr = '$STAKING_ADDRESS'
raw = base58.b58decode(addr)
# Network prefix (1 byte) + ErgoTree + checksum (4 bytes)
ergotree = raw[1:-4]
print(ergotree.hex())
" 2>/dev/null) || {
    # Fallback: just use the address directly in the request
    echo "  WARNING: Could not extract ErgoTree hex, will use address directly"
    ERGO_TREE_HEX=""
}

if [[ -n "$ERGO_TREE_HEX" ]]; then
    echo "  ErgoTree hex:   ${ERGO_TREE_HEX:0:16}... (${#ERGO_TREE_HEX} chars)"
fi

# ---------------------------------------------------------------------------
# Step 2: Build and submit transaction via /wallet/payment/send
# ---------------------------------------------------------------------------
echo ""
echo "[2/3] Building staking box transaction..."

# Encode the user PK as Sigma GroupElement: 0e 21 <33 bytes hex>
R4_REGISTER="0e21${USER_PK_HEX}"

# Build the request JSON
if [[ -n "$ERGO_TREE_HEX" ]]; then
    REQUEST_JSON=$(jq -n \
        --arg ergotree "$ERGO_TREE_HEX" \
        --argjson value "$AMOUNT_NANOERG" \
        --arg r4 "$R4_REGISTER" \
        --argjson fee "$FEE_NANOERG" \
        '{
            requests: [{
                ergoTree: $ergotree,
                value: ($value | tostring),
                assets: [],
                registers: { R4: $r4 }
            }],
            fee: ($fee | tostring)
        }')
else
    # Fallback: use the P2S address
    REQUEST_JSON=$(jq -n \
        --arg address "$STAKING_ADDRESS" \
        --argjson value "$AMOUNT_NANOERG" \
        --arg r4 "$R4_REGISTER" \
        --argjson fee "$FEE_NANOERG" \
        '{
            requests: [{
                address: $address,
                value: ($value | tostring),
                assets: [],
                registers: { R4: $r4 }
            }],
            fee: ($fee | tostring)
        }')
fi

echo "  Request: $REQUEST_JSON" | head -c 200
echo "..."

TX_RESPONSE=$(curl -sf -X POST "$NODE_URL/wallet/payment/send" \
    -H "Content-Type: application/json" \
    -d "$REQUEST_JSON" 2>/dev/null) || {
    echo "ERROR: Failed to submit staking box transaction" >&2
    echo "  Response: $TX_RESPONSE" >&2
    exit 1
}

TX_ID=$(echo "$TX_RESPONSE" | jq -r '.id // empty')

if [[ -z "$TX_ID" ]]; then
    echo "ERROR: No transaction ID returned" >&2
    echo "  Response: $TX_RESPONSE" >&2
    exit 1
fi

echo "  Transaction ID: $TX_ID"

# ---------------------------------------------------------------------------
# Step 3: Fetch tx details to extract box ID
# ---------------------------------------------------------------------------
echo ""
echo "[3/3] Fetching transaction details..."

sleep 2  # Brief pause to let the node index the transaction

TX_DETAIL=$(curl -sf "$NODE_URL/transactions/$TX_ID" 2>/dev/null) || {
    echo "WARNING: Could not fetch transaction details (tx may not be indexed yet)" >&2
    echo "  TX ID: $TX_ID"
    echo "  Staking address: $STAKING_ADDRESS"
    echo ""
    echo "=== Staking box creation submitted (details pending) ==="
    exit 0
}

BOX_ID=$(echo "$TX_DETAIL" | jq -r '.outputs[0].boxId // empty')

if [[ -z "$BOX_ID" ]]; then
    echo "WARNING: Could not extract box ID from transaction" >&2
    BOX_ID="(pending)"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=== User Staking Box Created ==="
echo "TX ID:           $TX_ID"
echo "Box ID:          $BOX_ID"
echo "Staking address: $STAKING_ADDRESS"
echo "User PK prefix:  ${USER_PK_HEX:0:16}..."
echo "Amount locked:   $AMOUNT_ERG ERG"
echo "R4 register:     $R4_REGISTER"
echo ""
echo "Verify on-chain:"
echo "  curl -s $NODE_URL/transactions/$TX_ID | jq ."
echo "  curl -s $NODE_URL/boxes/$BOX_ID | jq ."
