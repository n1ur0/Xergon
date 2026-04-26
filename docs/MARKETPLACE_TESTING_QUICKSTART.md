# Xergon Marketplace Testing - Quick Start Guide

## 🎯 Overview

This guide helps you get started with testing the Xergon DEX marketplace ecosystem. We've created a comprehensive test suite covering SDK plugins, API endpoints, and performance testing.

## 📦 What Was Created

### 1. Unit Tests (SDK Plugin Marketplace)
**File:** `xergon-sdk/tests/plugin-marketplace.test.ts`
- ✅ Plugin search & filtering
- ✅ Plugin installation/uninstallation
- ✅ Plugin updates & publishing
- ✅ Reviews & ratings
- ✅ Cache management

### 2. Integration Tests (API Endpoints)
**File:** `xergon-marketplace/__tests__/integration/marketplace-api.test.ts`
- ✅ Model marketplace endpoints
- ✅ Provider listing & health
- ✅ Billing & earnings
- ✅ Authentication flows
- ✅ Error handling

### 3. Shell-Based Test Suite
**File:** `tests/run-marketplace-tests.sh`
- ✅ SDK plugin flow tests
- ✅ API health checks
- ✅ Provider registration
- ✅ End-to-end workflows

### 4. Performance Tests
**File:** `tests/performance-tests.sh`
- ✅ Response time analysis
- ✅ Concurrent load testing
- ✅ Stress testing
- ✅ Performance benchmarks

### 5. Documentation
**Files:**
- `docs/MARKETPLACE_TESTING_STRATEGY.md` - Comprehensive testing strategy
- `docs/MARKETPLACE_TESTING_QUICKSTART.md` - This guide

## 🚀 Quick Start

### Prerequisites

```bash
# Install dependencies
cd xergon-sdk
npm install

cd xergon-marketplace
npm install

# Make scripts executable
chmod +x tests/*.sh
```

### Run Tests

#### Quick Smoke Test (Recommended First)
```bash
cd /home/n1ur0/Xergon-Network
./tests/run-marketplace-tests.sh --quick
```

#### Full Test Suite
```bash
# All tests
./tests/run-marketplace-tests.sh

# Specific components
./tests/run-marketplace-tests.sh --sdk    # SDK plugin tests only
./tests/run-marketplace-tests.sh --api    # API endpoint tests only
./tests/run-marketplace-tests.sh --e2e    # End-to-end tests only
```

#### Performance Tests
```bash
# Basic performance tests
./tests/performance-tests.sh

# With stress test
./tests/performance-tests.sh --stress
```

#### Unit Tests (Jest)
```bash
# SDK unit tests
cd xergon-sdk
npm test -- plugin-marketplace.test.ts

# API integration tests
cd xergon-marketplace
npm run test:integration
```

## 📊 Test Coverage

| Component | Tests Created | Status |
|-----------|---------------|--------|
| SDK Plugin Marketplace | 15+ unit tests | ✅ Ready |
| API Integration | 20+ integration tests | ✅ Ready |
| Shell-based Tests | 12+ E2E tests | ✅ Ready |
| Performance Tests | 8+ load tests | ✅ Ready |
| **Total** | **55+ tests** | **✅ Ready** |

## 🎯 Test Categories

### Unit Tests
- Test individual functions in isolation
- Mock external dependencies
- Fast execution (< 1 second per test)

### Integration Tests
- Test API endpoints with real HTTP requests
- Test database interactions
- Moderate execution (1-5 seconds per test)

### End-to-End Tests
- Test complete user workflows
- Test multi-step processes
- Slower execution (5-30 seconds per test)

### Performance Tests
- Response time benchmarks
- Concurrent load testing
- Stress testing

## 📝 Running Specific Tests

### SDK Plugin Tests

```bash
# Search functionality
./tests/run-marketplace-tests.sh --sdk

# Install/uninstall
# (Part of --sdk tests)

# Reviews & ratings
# (Part of --sdk tests)
```

### API Tests

```bash
# All API tests
./tests/run-marketplace-tests.sh --api

# Specific endpoints
# (Run individual Jest tests)
cd xergon-marketplace
npm test -- marketplace-api.test.ts
```

### Performance Tests

```bash
# Basic performance
./tests/performance-tests.sh

# With stress test
./tests/performance-tests.sh --stress

# Custom URL
MARKETPLACE_URL=http://your-url ./tests/performance-tests.sh
```

## 🔧 Configuration

### Environment Variables

```bash
# Test configuration
export TEST_BASE_URL=http://localhost:3000
export TEST_AUTH_TOKEN=test-token
export XERGON_RELAY_BASE=http://127.0.0.1:9090
export MARKETPLACE_URL=http://localhost:3000
export VERBOSE=true  # Verbose output
```

### Test Data

Mock data is included in test files:
- **Plugins:** 10+ test plugins with various categories
- **Models:** 12+ test models across different categories
- **Providers:** 12+ test providers with different regions

## 🐛 Troubleshooting

### Common Issues

**Issue:** Tests fail with "Connection refused"
```bash
# Solution: Start services
cd xergon-marketplace
npm run dev  # Terminal 1

cd xergon-relay
cargo run --release  # Terminal 2
```

**Issue:** Mock data not loading
```bash
# Solution: Check test directories
ls -la xergon-sdk/tests/
ls -la xergon-marketplace/__tests__/
```

**Issue:** Authentication tests fail
```bash
# Solution: Set test token
export TEST_AUTH_TOKEN=test-token
```

### Debug Mode

```bash
# Verbose output
./tests/run-marketplace-tests.sh --verbose

# Check specific endpoint
curl -v http://localhost:3000/api/marketplace/models
```

## 📈 Test Results

### Expected Output

```
════════════════════════════════════════
  SDK Plugin: Search Marketplace
════════════════════════════════════════

  ✓ PASS Plugin search endpoint responds
  ✓ PASS Category filter works

════════════════════════════════════════
  Test Summary
════════════════════════════════════════
  Passed: 15
  Failed: 2
  Skipped: 1
  Total: 18
```

### Success Criteria

- ✅ 80%+ test pass rate
- ✅ All critical paths tested
- ✅ Performance within acceptable limits
- ✅ No authentication bypasses

## 🔄 CI/CD Integration

### GitHub Actions

Add to `.github/workflows/marketplace-tests.yml`:

```yaml
name: Marketplace Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '20'
      
      - name: Install dependencies
        run: |
          cd xergon-sdk && npm install
          cd ../xergon-marketplace && npm install
      
      - name: Run tests
        run: |
          chmod +x tests/run-marketplace-tests.sh
          ./tests/run-marketplace-tests.sh --quick
```

## 📚 Next Steps

1. ✅ **Run quick tests** - Verify setup
2. ⏳ **Run full test suite** - Complete coverage
3. ⏳ **Add your tests** - Extend coverage
4. ⏳ **Set up CI/CD** - Automated testing
5. ⏳ **Monitor test health** - Track trends

## 📖 Additional Resources

- **Testing Strategy:** `docs/MARKETPLACE_TESTING_STRATEGY.md`
- **SDK Documentation:** `xergon-sdk/README.md`
- **API Documentation:** `docs/api/README.md`
- **Performance Guide:** `docs/PERFORMANCE_BEST_PRACTICES.md`

## 🎉 Summary

You now have a comprehensive testing suite for the Xergon marketplace:

- **55+ tests** covering all major functionality
- **3 test frameworks** (Jest, shell scripts, performance tests)
- **Full documentation** for maintenance and extension
- **Ready for CI/CD** integration

**Run your first test now:**
```bash
./tests/run-marketplace-tests.sh --quick
```

---

**Created:** 2026-04-13  
**Test Suite Version:** 1.0.0  
**Maintained by:** Xergon Development Team
