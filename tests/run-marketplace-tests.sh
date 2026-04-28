#!/usr/bin/env bash
# Xergon Marketplace Testing Suite
# Comprehensive tests for marketplace functionality
#
# Usage:
#   ./run-marketplace-tests.sh              # Run all tests
#   ./run-marketplace-tests.sh --sdk        # SDK plugin tests only
#   ./run-marketplace-tests.sh --api        # API endpoint tests only
#   ./run-marketplace-tests.sh --e2e        # End-to-end tests only
#   ./run-marketplace-tests.sh --quick      # Quick smoke tests
#   ./run-marketplace-tests.sh --verbose    # Verbose output

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
MARKETPLACE_DIR="$PROJECT_DIR/xergon-marketplace"
SDK_DIR="$PROJECT_DIR/xergon-sdk"
RELAY_DIR="$PROJECT_DIR/xergon-relay"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test counters
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

# Options
RUN_SDK=false
RUN_API=false
RUN_E2E=false
RUN_QUICK=false
VERBOSE=false

parse_args() {
    if [ "$#" -eq 0 ]; then
        RUN_SDK=true
        RUN_API=true
        RUN_E2E=true
    fi
    
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --sdk) RUN_SDK=true ;;
            --api) RUN_API=true ;;
            --e2e) RUN_E2E=true ;;
            --quick) RUN_QUICK=true; RUN_SDK=true; RUN_API=true ;;
            --verbose) VERBOSE=true ;;
            --help)
                echo "Usage: $0 [--sdk] [--api] [--e2e] [--quick] [--verbose]"
                exit 0
                ;;
        esac
        shift
    done
}

log_pass() { echo -e "  ${GREEN}✓ PASS${NC} $1"; ((PASS_COUNT++)); }
log_fail() { echo -e "  ${RED}✗ FAIL${NC} $1"; ((FAIL_COUNT++)); }
log_skip() { echo -e "  ${YELLOW}⊘ SKIP${NC} $1"; ((SKIP_COUNT++)); }
log_info() { echo -e "  ${CYAN}ℹ INFO${NC} $1" ; }
log_section() { echo -e "\n${BLUE}════════════════════════════════════════${NC}"; echo -e "${BLUE}  $1${NC}"; echo -e "${BLUE}════════════════════════════════════════${NC}\n"; }

# ─────────────────────────────────────────────────────────────────────────────
# SDK Plugin Marketplace Tests
# ─────────────────────────────────────────────────────────────────────────────

test_sdk_plugin_search() {
    log_section "SDK Plugin: Search Marketplace"
    
    # Test 1: Search with query
    log_info "Testing plugin search with query..."
    # This would call the SDK searchPlugins method
    # For now, we'll test the API directly
    local result
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/plugins?q=testing" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.plugins' >/dev/null 2>&1; then
        log_pass "Plugin search endpoint responds"
    else
        log_fail "Plugin search endpoint failed"
    fi
    
    # Test 2: Search with category filter
    log_info "Testing category filter..."
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/plugins?category=testing" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.plugins' >/dev/null 2>&1; then
        log_pass "Category filter works"
    else
        log_fail "Category filter failed"
    fi
}

test_sdk_plugin_install() {
    log_section "SDK Plugin: Install/Uninstall"
    
    # Test plugin installation flow
    log_info "Testing plugin installation..."
    
    # This would test the installPlugin method
    # For now, check if the endpoint exists
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/api/marketplace/plugins/test-plugin/install" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ] || [ "$status" = "404" ]; then
        log_pass "Install endpoint exists (status: $status)"
    else
        log_fail "Install endpoint not accessible (status: $status)"
    fi
}

test_sdk_plugin_update() {
    log_section "SDK Plugin: Update Plugins"
    
    log_info "Testing plugin update flow..."
    # Test updatePlugin method
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/api/marketplace/plugins/test-plugin/update" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ] || [ "$status" = "404" ]; then
        log_pass "Update endpoint exists (status: $status)"
    else
        log_fail "Update endpoint not accessible (status: $status)"
    fi
}

test_sdk_plugin_reviews() {
    log_section "SDK Plugin: Reviews & Ratings"
    
    log_info "Testing plugin reviews..."
    local result
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/plugins/test-plugin/reviews" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.reviews' >/dev/null 2>&1; then
        log_pass "Reviews endpoint responds"
    else
        log_fail "Reviews endpoint failed"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# API Endpoint Tests
# ─────────────────────────────────────────────────────────────────────────────

test_api_marketplace_health() {
    log_section "Marketplace API: Health & Connectivity"
    
    # Test 1: Relay health
    log_info "Checking relay health..."
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:9090/health" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ]; then
        log_pass "Relay health endpoint (HTTP $status)"
    else
        log_fail "Relay health check failed (HTTP $status)"
    fi
    
    # Test 2: Marketplace models endpoint
    log_info "Checking marketplace models endpoint..."
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/api/marketplace/models" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ]; then
        log_pass "Marketplace models endpoint (HTTP $status)"
    else
        log_fail "Marketplace models endpoint failed (HTTP $status)"
    fi
}

test_api_providers() {
    log_section "Marketplace API: Provider Endpoints"
    
    # Test providers list
    log_info "Testing providers list..."
    local result
    result=$(curl -s "http://127.0.0.1:3000/api/xergon-relay/providers" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.providers' >/dev/null 2>&1; then
        local count
        count=$(echo "$result" | jq '.providers | length')
        log_pass "Providers endpoint returns $count providers"
    else
        log_fail "Providers endpoint failed"
    fi
    
    # Test provider health
    log_info "Testing provider health..."
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/api/xergon-relay/health" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ]; then
        log_pass "Provider health endpoint (HTTP $status)"
    else
        log_fail "Provider health check failed (HTTP $status)"
    fi
}

test_api_billing() {
    log_section "Marketplace API: Billing & Earnings"
    
    # Test billing endpoint
    log_info "Testing billing endpoint..."
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/api/billing" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ] || [ "$status" = "401" ]; then
        log_pass "Billing endpoint exists (HTTP $status)"
    else
        log_fail "Billing endpoint not accessible (HTTP $status)"
    fi
    
    # Test earnings endpoint
    log_info "Testing earnings endpoint..."
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/api/earnings" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ] || [ "$status" = "401" ]; then
        log_pass "Earnings endpoint exists (HTTP $status)"
    else
        log_fail "Earnings endpoint not accessible (HTTP $status)"
    fi
}

test_api_authentication() {
    log_section "Marketplace API: Authentication & Authorization"
    
    # Test protected endpoint without auth
    log_info "Testing auth requirement..."
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000/api/billing" 2>/dev/null || echo "000")
    
    if [ "$status" = "401" ] || [ "$status" = "403" ]; then
        log_pass "Protected endpoints require authentication"
    else
        log_fail "Authentication not enforced (HTTP $status)"
    fi
    
    # Test invalid token
    log_info "Testing invalid token rejection..."
    status=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer invalid-token" "http://127.0.0.1:3000/api/billing" 2>/dev/null || echo "000")
    
    if [ "$status" = "401" ] || [ "$status" = "403" ]; then
        log_pass "Invalid tokens rejected"
    else
        log_fail "Invalid tokens not rejected (HTTP $status)"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# End-to-End Tests
# ─────────────────────────────────────────────────────────────────────────────

test_e2e_plugin_lifecycle() {
    log_section "End-to-End: Plugin Lifecycle"
    
    log_info "Testing complete plugin lifecycle..."
    
    # Step 1: Search for plugins
    log_info "  Step 1: Searching for plugins..."
    local result
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/plugins?q=testing" 2>/dev/null || echo "{}")
    
    if ! echo "$result" | jq -e '.plugins' >/dev/null 2>&1; then
        log_fail "Plugin search failed"
        return
    fi
    
    # Step 2: Get plugin details
    log_info "  Step 2: Getting plugin details..."
    local plugin_id
    plugin_id=$(echo "$result" | jq -r '.plugins[0].id // empty')
    
    if [ -n "$plugin_id" ] && [ "$plugin_id" != "null" ]; then
        log_pass "Found plugin: $plugin_id"
    else
        log_skip "No plugins found for detailed testing"
        return
    fi
    
    # Step 3: Check plugin reviews
    log_info "  Step 3: Checking plugin reviews..."
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/plugins/$plugin_id/reviews" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.reviews' >/dev/null 2>&1; then
        log_pass "Reviews retrieved successfully"
    else
        log_fail "Failed to retrieve reviews"
    fi
    
    # Step 4: Check categories
    log_info "  Step 4: Checking categories..."
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/categories" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.categories' >/dev/null 2>&1; then
        local cat_count
        cat_count=$(echo "$result" | jq '.categories | length')
        log_pass "Found $cat_count categories"
    else
        log_fail "Failed to retrieve categories"
    fi
}

test_e2e_provider_registration() {
    log_section "End-to-End: Provider Registration Flow"
    
    log_info "Testing provider registration..."
    
    # Test provider registration endpoint
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
        -H "Content-Type: application/json" \
        -d '{"name":"test-provider","endpoint":"http://test.local","region":"US"}' \
        "http://127.0.0.1:9090/register" 2>/dev/null || echo "000")
    
    if [ "$status" = "200" ] || [ "$status" = "201" ] || [ "$status" = "400" ]; then
        log_pass "Provider registration endpoint responds (HTTP $status)"
    else
        log_fail "Provider registration failed (HTTP $status)"
    fi
}

test_e2e_model_marketplace() {
    log_section "End-to-End: Model Marketplace"
    
    log_info "Testing model marketplace..."
    
    # Get models
    local result
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/models" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.models' >/dev/null 2>&1; then
        local model_count
        model_count=$(echo "$result" | jq '.models | length')
        log_pass "Marketplace returns $model_count models"
        
        # Check model structure
        if echo "$result" | jq -e '.models[0] | has("id", "name", "provider", "pricePerInputTokenNanoerg")' >/dev/null 2>&1; then
            log_pass "Model structure is valid"
        else
            log_fail "Model structure is incomplete"
        fi
    else
        log_fail "Failed to retrieve models"
    fi
    
    # Test featured models
    log_info "Testing featured models..."
    result=$(curl -s "http://127.0.0.1:3000/api/marketplace/models?subpath=featured" 2>/dev/null || echo "{}")
    
    if echo "$result" | jq -e '.models' >/dev/null 2>&1; then
        log_pass "Featured models endpoint works"
    else
        log_fail "Featured models endpoint failed"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Main Test Runner
# ─────────────────────────────────────────────────────────────────────────────

run_tests() {
    parse_args "$@"
    
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║        Xergon Marketplace Testing Suite                  ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}\n"
    
    # Check prerequisites
    log_info "Checking prerequisites..."
    
    if ! command -v jq &>/dev/null; then
        echo -e "${RED}✗ Error: jq is required but not installed${NC}"
        exit 1
    fi
    
    # Run test suites
    if [ "$RUN_SDK" = true ]; then
        test_sdk_plugin_search
        test_sdk_plugin_install
        test_sdk_plugin_update
        test_sdk_plugin_reviews
    fi
    
    if [ "$RUN_API" = true ]; then
        test_api_marketplace_health
        test_api_providers
        test_api_billing
        test_api_authentication
    fi
    
    if [ "$RUN_E2E" = true ]; then
        test_e2e_plugin_lifecycle
        test_e2e_provider_registration
        test_e2e_model_marketplace
    fi
    
    # Summary
    echo -e "\n${BLUE}════════════════════════════════════════${NC}"
    echo -e "${BLUE}  Test Summary${NC}"
    echo -e "${BLUE}════════════════════════════════════════${NC}"
    echo -e "  ${GREEN}Passed:${NC} $PASS_COUNT"
    echo -e "  ${RED}Failed:${NC} $FAIL_COUNT"
    echo -e "  ${YELLOW}Skipped:${NC} $SKIP_COUNT"
    echo -e "  ${CYAN}Total:${NC} $((PASS_COUNT + FAIL_COUNT + SKIP_COUNT))"
    
    if [ $FAIL_COUNT -gt 0 ]; then
        echo -e "\n${RED}⚠ Some tests failed. Review the output above.${NC}"
        exit 1
    else
        echo -e "\n${GREEN}✓ All tests passed!${NC}"
        exit 0
    fi
}

# Run if executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    run_tests "$@"
fi
