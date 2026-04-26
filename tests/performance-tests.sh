#!/usr/bin/env bash
# Xergon Marketplace Performance & Load Tests
# Tests marketplace endpoints under load

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MARKETPLACE_URL="${MARKETPLACE_URL:-http://localhost:3000}"
RELAY_URL="${RELAY_URL:-http://localhost:9090}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test configuration
CONCURRENCY_LEVELS=(1 5 10 20 50)
REQUEST_COUNT=100
TIMEOUT=30

log_info() { echo -e "  ${CYAN}ℹ INFO${NC} $1"; }
log_pass() { echo -e "  ${GREEN}✓ PASS${NC} $1"; }
log_fail() { echo -e "  ${RED}✗ FAIL${NC} $1"; }
log_section() { echo -e "\n${BLUE}════════════════════════════════════════${NC}"; echo -e "${BLUE}  $1${NC}"; echo -e "${BLUE}════════════════════════════════════════${NC}\n"; }

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    if ! command -v curl &>/dev/null; then
        echo -e "${RED}✗ Error: curl is required${NC}"
        exit 1
    fi
    
    if ! command -v jq &>/dev/null; then
        echo -e "${RED}✗ Error: jq is required${NC}"
        exit 1
    fi
    
    # Check if services are running
    log_info "Checking if services are accessible..."
    
    local marketplace_status
    marketplace_status=$(curl -s -o /dev/null -w "%{http_code}" "$MARKETPLACE_URL/api/marketplace/models" 2>/dev/null || echo "000")
    
    if [ "$marketplace_status" = "000" ]; then
        echo -e "${YELLOW}⚠ Warning: Marketplace not accessible at $MARKETPLACE_URL${NC}"
        echo -e "${YELLOW}   Run tests with: MARKETPLACE_URL=http://your-url ./tests/performance-tests.sh${NC}"
        return 1
    fi
    
    log_pass "Marketplace accessible (HTTP $marketplace_status)"
    return 0
}

# Single request test
test_single_request() {
    local endpoint="$1"
    local description="$2"
    
    local start_time end_time duration
    start_time=$(date +%s%N)
    
    local response
    response=$(curl -s -o /dev/null -w "%{http_code}:%{time_total}" "$endpoint" 2>/dev/null || echo "000:0")
    
    end_time=$(date +%s%N)
    
    local http_code duration_sec
    http_code=$(echo "$response" | cut -d':' -f1)
    duration_sec=$(echo "$response" | cut -d':' -f2)
    
    # Convert to milliseconds
    local duration_ms
    duration_ms=$(echo "$duration_sec * 1000" | bc 2>/dev/null || echo "0")
    
    if [ "$http_code" = "200" ]; then
        log_pass "$description: ${duration_ms}ms"
        echo "$duration_ms"
    else
        log_fail "$description: HTTP $http_code (${duration_ms}ms)"
        echo "-1"
    fi
}

# Concurrent request test
test_concurrent_requests() {
    local endpoint="$1"
    local concurrency="$2"
    local description="$3"
    
    log_info "Testing $concurrency concurrent requests to $description..."
    
    local start_time end_time total_time
    start_time=$(date +%s%N)
    
    # Send concurrent requests
    local pids=()
    local success_count=0
    local fail_count=0
    local total_duration=0
    
    for ((i=1; i<=concurrency; i++)); do
        (
            local response
            response=$(curl -s -o /dev/null -w "%{http_code}:%{time_total}" "$endpoint" 2>/dev/null || echo "000:0")
            local http_code duration
            http_code=$(echo "$response" | cut -d':' -f1)
            duration=$(echo "$response" | cut -d':' -f2)
            
            if [ "$http_code" = "200" ]; then
                echo "success:$duration"
            else
                echo "fail:$duration"
            fi
        ) &
        pids+=($!)
    done
    
    # Wait for all requests and collect results
    for pid in "${pids[@]}"; do
        local result
        result=$(wait "$pid" 2>/dev/null && echo "done" || echo "error")
        
        if [ "$result" = "done" ]; then
            ((success_count++))
        else
            ((fail_count++))
        fi
    done
    
    end_time=$(date +%s%N)
    total_time=$(( (end_time - start_time) / 1000000 ))
    
    local avg_duration
    if [ $success_count -gt 0 ]; then
        avg_duration=$((total_time / concurrency))
    else
        avg_duration=0
    fi
    
    echo "  Concurrent ($concurrency): $success_count success, $fail_count fail, ${total_time}ms total, ${avg_duration}ms avg"
    
    if [ $fail_count -eq 0 ]; then
        log_pass "$description ($concurrency concurrent): ${total_time}ms total"
    else
        log_fail "$description ($concurrency concurrent): $fail_count failures"
    fi
}

# Endpoint performance test
test_endpoint_performance() {
    local endpoint="$1"
    local description="$2"
    
    log_section "Endpoint Performance: $description"
    
    # Single request baseline
    log_info "Baseline single request..."
    local baseline
    baseline=$(test_single_request "$endpoint" "Single request")
    
    if [ "$baseline" = "-1" ]; then
        log_fail "Endpoint not responding"
        return
    fi
    
    # Concurrent tests
    for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
        if [ $concurrency -gt 1 ]; then
            test_concurrent_requests "$endpoint" "$concurrency" "$description"
        fi
    done
    
    echo ""
}

# Load test with increasing concurrency
run_load_test() {
    log_section "Load Test: Increasing Concurrency"
    
    local endpoint="$1"
    local description="$2"
    
    for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
        log_info "Testing with $concurrency concurrent users..."
        
        local start_time end_time
        start_time=$(date +%s%N)
        
        # Fire concurrent requests
        local successes=0
        local failures=0
        local total_time=0
        
        for ((i=1; i<=concurrency; i++)); do
            local response
            response=$(curl -s -o /dev/null -w "%{http_code}:%{time_total}" --max-time 10 "$endpoint" 2>/dev/null || echo "000:0")
            
            local http_code
            http_code=$(echo "$response" | cut -d':' -f1)
            
            if [ "$http_code" = "200" ]; then
                ((successes++))
            else
                ((failures++))
            fi
        done
        
        end_time=$(date +%s%N)
        local duration_ms=$(( (end_time - start_time) / 1000000 ))
        
        local success_rate=0
        if [ $concurrency -gt 0 ]; then
            success_rate=$((successes * 100 / concurrency))
        fi
        
        echo "  $concurrency concurrent: $successes/$concurrency successful (${success_rate}%), ${duration_ms}ms"
        
        if [ $success_rate -lt 90 ]; then
            log_fail "Success rate dropped below 90% at $concurrency concurrency"
            break
        fi
    done
}

# API response time analysis
analyze_response_times() {
    log_section "Response Time Analysis"
    
    local endpoints=(
        "/api/marketplace/models:Model Marketplace"
        "/api/xergon-relay/providers:Provider List"
        "/api/xergon-relay/health:Health Check"
        "/api/earnings:Earnings (auth required)"
    )
    
    echo "Testing response times for all endpoints..."
    echo ""
    
    for endpoint_data in "${endpoints[@]}"; do
        local endpoint="${endpoint_data%%:*}"
        local description="${endpoint_data##*:}"
        
        local response
        response=$(curl -s -o /dev/null -w "%{http_code}:%{time_total}" "$MARKETPLACE_URL$endpoint" 2>/dev/null || echo "000:0")
        
        local http_code time_total
        http_code=$(echo "$response" | cut -d':' -f1)
        time_total=$(echo "$response" | cut -d':' -f2)
        
        local time_ms
        time_ms=$(echo "$time_total * 1000" | bc 2>/dev/null || echo "0")
        
        if [ "$http_code" = "200" ] || [ "$http_code" = "401" ]; then
            log_pass "$description: ${time_ms}ms (HTTP $http_code)"
        else
            log_fail "$description: HTTP $http_code (${time_ms}ms)"
        fi
    done
}

# Stress test
run_stress_test() {
    log_section "Stress Test: Sustained Load"
    
    local endpoint="${1:-$MARKETPLACE_URL/api/marketplace/models}"
    local duration="${2:-30}"
    
    log_info "Running stress test for ${duration} seconds..."
    log_info "Endpoint: $endpoint"
    
    local start_time end_time
    start_time=$(date +%s)
    local request_count=0
    local success_count=0
    local fail_count=0
    
    while true; do
        local current_time
        current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $duration ]; then
            break
        fi
        
        # Fire 10 requests per iteration
        for ((i=1; i<=10; i++)); do
            local response
            response=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$endpoint" 2>/dev/null || echo "000")
            
            ((request_count++))
            
            if [ "$response" = "200" ]; then
                ((success_count++))
            else
                ((fail_count++))
            fi
        done
        
        # Progress update every 5 seconds
        if [ $((elapsed % 5)) -eq 0 ]; then
            echo -ne "  Progress: ${request_count} requests, ${success_count} success, ${fail_count} fail\r"
        fi
    done
    
    end_time=$(date +%s)
    local total_time=$((end_time - start_time))
    local rps=$((request_count / total_time))
    local success_rate=0
    
    if [ $request_count -gt 0 ]; then
        success_rate=$((success_count * 100 / request_count))
    fi
    
    echo ""
    log_pass "Stress test completed:"
    echo "  Duration: ${total_time}s"
    echo "  Total requests: $request_count"
    echo "  Requests/sec: $rps"
    echo "  Success rate: ${success_rate}%"
    echo "  Successful: $success_count"
    echo "  Failed: $fail_count"
}

# Main test runner
run_all_tests() {
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║        Xergon Marketplace Performance Tests              ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}\n"
    
    if ! check_prerequisites; then
        echo -e "${YELLOW}⚠ Running in offline mode - skipping service checks${NC}\n"
    fi
    
    # Response time analysis
    analyze_response_times
    
    # Load tests
    if [ "$MARKETPLACE_URL" != "http://localhost:3000" ] || curl -s "$MARKETPLACE_URL/api/marketplace/models" >/dev/null 2>&1; then
        run_load_test "$MARKETPLACE_URL/api/marketplace/models" "Model Marketplace"
        run_load_test "$MARKETPLACE_URL/api/xergon-relay/providers" "Provider List"
    else
        echo -e "${YELLOW}⚠ Skipping load tests - marketplace not accessible${NC}\n"
    fi
    
    # Optional stress test
    if [ "${1:-}" = "--stress" ]; then
        run_stress_test "$MARKETPLACE_URL/api/marketplace/models" 30
    fi
    
    echo -e "\n${GREEN}✓ Performance tests completed${NC}"
}

# Run if executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    run_all_tests "$@"
fi
