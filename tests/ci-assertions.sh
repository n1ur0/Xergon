#!/usr/bin/env bash
# CI Assertions -- lightweight health check for GitHub Actions
# Runs before integration/performance tests to confirm services are up.
set -euo pipefail

AGENT_URL="${AGENT_URL:-http://localhost:9099}"
RELAY_URL="${RELAY_URL:-http://localhost:9090}"
MARKETPLACE_URL="${MARKETPLACE_URL:-http://localhost:3000}"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

FAILED=0

check() {
    local name="$1"
    local url="$2"
    local expected="${3:-200}"
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$url" 2>/dev/null || echo "000")
    if [ "$code" = "$expected" ]; then
        echo -e "  ${GREEN}PASS${NC} $name (HTTP $code)"
    else
        echo -e "  ${RED}FAIL${NC} $name — expected $expected, got $code"
        FAILED=$((FAILED + 1))
    fi
}

echo "=== CI Assertions ==="
check "xergon-agent health"    "$AGENT_URL/xergon/health"
check "xergon-relay health"   "$RELAY_URL/health"
check "Marketplace /health"   "$MARKETPLACE_URL/api/v1/health"
check "Marketplace /models"   "$MARKETPLACE_URL/api/marketplace/models"
check "Relay /providers"      "$MARKETPLACE_URL/api/xergon-relay/providers"

if [ $FAILED -gt 0 ]; then
    echo -e "\n${RED}CI assertions failed: $FAILED check(s) failed${NC}"
    exit 1
fi
echo -e "\n${GREEN}All CI assertions passed${NC}"
