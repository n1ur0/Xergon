#!/bin/bash
# Xergon Network Load Test
# Requires: hey (https://github.com/rakyll/hey), jq

set -e

AGENT_URL="${AGENT_URL:-http://localhost:9010}"
RELAY_URL="${RELAY_URL:-http://localhost:9011}"
CONCURRENT="${CONCURRENT:-100}"
TOTAL_REQUESTS="${TOTAL_REQUESTS:-10000}"

echo "=== Xergon Load Test ==="
echo "Agent: $AGENT_URL"
echo "Relay: $RELAY_URL"
echo "Concurrent: $CONCURRENT"
echo "Total requests: $TOTAL_REQUESTS"
echo ""

# Check hey is installed
if ! command -v hey &> /dev/null; then
    echo "Installing hey..."
    go install github.com/rakyll/hey@latest
fi

echo "--- Test 1: Agent Health ---"
hey -n 100 -c 10 "${AGENT_URL}/api/health"
echo ""

echo "--- Test 2: Agent Metrics ---"
hey -n 100 -c 10 "${AGENT_URL}/api/metrics"
echo ""

echo "--- Test 3: Relay Health ---"
hey -n 100 -c 10 "${RELAY_URL}/v1/health"
echo ""

echo "--- Test 4: Relay Models ---"
hey -n 100 -c 10 "${RELAY_URL}/v1/models"
echo ""

echo "--- Test 5: Relay Chat (with auth) ---"
# Requires API key in config
PK="${PK:-}"
if [ -n "$PK" ]; then
    TIMESTAMP=$(date +%s)
    SIGNATURE=$(echo -n "${TIMESTAMP}GET/v1/chat/completions" | openssl dgst -sha256 -hmac "${PK}" -hex | awk '{print $NF}')
    BODY='{"model":"llama3.1:8b","messages":[{"role":"user","content":"Hello"}],"max_tokens":10}'
    hey -n "${TOTAL_REQUESTS}" -c "${CONCURRENT}" \
        -H "Content-Type: application/json" \
        -H "X-Timestamp: ${TIMESTAMP}" \
        -H "X-Signature: ${SIGNATURE}" \
        -H "X-Public-Key: ${PK}" \
        -d "${BODY}" \
        "${RELAY_URL}/v1/chat/completions"
else
    echo "Skipping chat test (no PK set). Set PK env var to test."
fi

echo ""
echo "=== Load Test Complete ==="
