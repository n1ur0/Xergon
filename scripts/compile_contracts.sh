#!/usr/bin/env bash
#
# compile_contracts.sh -- Compile ErgoScript contracts to ErgoTree hex via Ergo node
#
# This script builds and runs the compile_contracts Rust binary, which sends
# each .es source file to the Ergo node's REST API (POST /script/p2sAddress),
# extracts the ErgoTree hex from the resulting P2S address, and writes it to
# the contracts/compiled/ directory.
#
# Usage:
#   ./scripts/compile_contracts.sh                        # default node at localhost:9053
#   ./scripts/compile_contracts.sh http://192.168.1.100:9053  # custom node URL
#   ./scripts/compile_contracts.sh --dry-run              # preview without compiling
#   ./scripts/compile_contracts.sh --validate-only        # just validate existing hex files
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
AGENT_DIR="$PROJECT_ROOT/xergon-agent"
COMPILED_DIR="$AGENT_DIR/contracts/compiled"

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Validate-only mode (original behavior)
# ---------------------------------------------------------------------------

validate_hex() {
    local hex_file="$1"
    local contract_name
    contract_name="$(basename "$hex_file" .hex)"

    if [[ ! -f "$hex_file" ]]; then
        echo -e "  ${RED}[MISSING]${NC}  $contract_name"
        return 1
    fi

    local hex_content
    hex_content="$(tr -d '[:space:]' < "$hex_file")"

    if [[ -z "$hex_content" ]]; then
        echo -e "  ${RED}[EMPTY]${NC}    $contract_name"
        return 1
    fi

    if ! echo "$hex_content" | grep -qE '^[0-9a-fA-F]+$'; then
        echo -e "  ${RED}[INVALID]${NC}  $contract_name (not valid base16)"
        return 1
    fi

    local len="${#hex_content}"
    if [[ "$len" -lt 64 ]]; then
        echo -e "  ${YELLOW}[SHORT]${NC}    $contract_name (${len} chars, minimum 64)"
        return 1
    fi

    # Check for placeholder pattern
    if [[ "$hex_content" == "100804020e36100204a00b08cd"* ]]; then
        echo -e "  ${YELLOW}[PLACEHOLDER]${NC} $contract_name (${len} chars)"
        return 1
    fi

    echo -e "  ${GREEN}[OK]${NC}        $contract_name (${len} chars)"
    return 0
}

EXPECTED_CONTRACTS=(
    provider_box
    provider_registration
    treasury_box
    usage_proof
    user_staking
)

# Handle --validate-only or --help
case "${1:-}" in
    --validate-only)
        echo "Validating compiled contract hex files..."
        echo "Directory: $COMPILED_DIR"
        echo
        errors=0
        for contract in "${EXPECTED_CONTRACTS[@]}"; do
            validate_hex "$COMPILED_DIR/${contract}.hex" || errors=$((errors + 1))
        done
        echo
        if [[ "$errors" -eq 0 ]]; then
            echo -e "${GREEN}All ${#EXPECTED_CONTRACTS[@]} contracts validated successfully.${NC}"
        else
            echo -e "${RED}${errors} contract(s) failed validation.${NC}"
            exit 1
        fi
        exit 0
        ;;
    --help|-h)
        cat <<'HELP'

Xergon Contract Compilation
============================

Usage:
  ./scripts/compile_contracts.sh [OPTIONS] [NODE_URL]

Options:
  --dry-run           Preview contracts without compiling or writing
  --validate-only     Only validate existing hex files (no compilation)
  --help, -h          Show this help

Arguments:
  NODE_URL            Ergo node REST API URL (default: http://127.0.0.1:9053)
                      Can also be set via ERGO_NODE_URL environment variable.

Examples:
  ./scripts/compile_contracts.sh
  ./scripts/compile_contracts.sh http://192.168.1.100:9053
  ERGO_NODE_URL=http://node:9053 ./scripts/compile_contracts.sh
  ./scripts/compile_contracts.sh --dry-run
  ./scripts/compile_contracts.sh --validate-only

Requirements:
  - Rust toolchain (cargo)
  - Ergo node running and accessible via REST API
  - The .es source files in xergon-agent/contracts/

How it works:
  1. Builds the compile_contracts binary: cargo build --bin compile_contracts --release
  2. Runs the binary, which:
     a. Reads each .es file from contracts/
     b. Sends it to the node's POST /script/p2sAddress endpoint
     c. Decodes the returned P2S address (base58) to extract ErgoTree hex
     d. Writes the hex to contracts/compiled/{name}.hex
  3. Validates output hex differs from known placeholders

HELP
        exit 0
        ;;
esac

# ---------------------------------------------------------------------------
# Determine node URL and flags
# ---------------------------------------------------------------------------

ERGO_NODE_URL="${ERGO_NODE_URL:-http://127.0.0.1:9053}"
EXTRA_ARGS=()

# Parse arguments
for arg in "$@"; do
    case "$arg" in
        --dry-run)
            EXTRA_ARGS+=("--dry-run")
            ;;
        http://*|https://*)
            ERGO_NODE_URL="$arg"
            ;;
        *)
            echo -e "${RED}Unknown argument: $arg${NC}"
            echo "Run with --help for usage."
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Build the binary
# ---------------------------------------------------------------------------

echo -e "${CYAN}Building compile_contracts binary...${NC}"
echo "  Node URL: $ERGO_NODE_URL"
echo "  Agent dir: $AGENT_DIR"
echo

if ! cargo build --bin compile_contracts --release --manifest-path "$AGENT_DIR/Cargo.toml" 2>&1; then
    echo -e "${RED}Build failed. See errors above.${NC}"
    exit 1
fi

echo -e "${GREEN}Build succeeded.${NC}"
echo

# ---------------------------------------------------------------------------
# Run the binary
# ---------------------------------------------------------------------------

echo -e "${CYAN}Compiling contracts via Ergo node...${NC}"
echo

COMPILE_BIN="$AGENT_DIR/target/release/compile_contracts"

if [[ ! -f "$COMPILE_BIN" ]]; then
    echo -e "${RED}Binary not found at $COMPILE_BIN${NC}"
    exit 1
fi

set +e
ERGO_NODE_URL="$ERGO_NODE_URL" "$COMPILE_BIN" \
    --node-url "$ERGO_NODE_URL" \
    --contracts-dir "$AGENT_DIR/contracts" \
    --output-dir "$COMPILED_DIR" \
    --verify \
    "${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}"
COMPILE_EXIT=$?
set -e

echo

# ---------------------------------------------------------------------------
# Post-compilation validation
# ---------------------------------------------------------------------------

if [[ $COMPILE_EXIT -eq 0 && ${#EXTRA_ARGS[@]} -eq 0 ]]; then
    echo -e "${CYAN}Post-compilation validation...${NC}"
    echo

    errors=0
    for contract in "${EXPECTED_CONTRACTS[@]}"; do
        validate_hex "$COMPILED_DIR/${contract}.hex" || errors=$((errors + 1))
    done

    echo
    if [[ "$errors" -eq 0 ]]; then
        echo -e "${GREEN}All ${#EXPECTED_CONTRACTS[@]} contracts compiled and validated successfully.${NC}"
    else
        echo -e "${YELLOW}${errors} contract(s) have issues (may be placeholders or short).${NC}"
        echo -e "${YELLOW}Run with --validate-only to re-check later.${NC}"
    fi
elif [[ $COMPILE_EXIT -ne 0 ]]; then
    echo -e "${RED}Compilation failed (exit code $COMPILE_EXIT).${NC}"
    echo "Make sure the Ergo node is running and accessible at $ERGO_NODE_URL"
    exit 1
fi
