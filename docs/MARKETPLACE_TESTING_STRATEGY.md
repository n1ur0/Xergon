# Xergon Marketplace Testing Strategy

## Overview

This document outlines the comprehensive testing strategy for the Xergon DEX marketplace ecosystem, covering the SDK plugin marketplace, API endpoints, and end-to-end workflows.

## Testing Components

### 1. SDK Plugin Marketplace Tests

**Location:** `xergon-sdk/tests/plugin-marketplace.test.ts`

**Coverage:**
- ✅ Plugin search and filtering
- ✅ Plugin installation/uninstallation
- ✅ Plugin updates
- ✅ Plugin reviews and ratings
- ✅ Plugin publishing
- ✅ Cache management
- ✅ Error handling and fallbacks

**Test Framework:** Jest with TypeScript
**Run Command:**
```bash
cd xergon-sdk
npm test -- plugin-marketplace.test.ts
```

### 2. API Integration Tests

**Location:** `xergon-marketplace/__tests__/integration/marketplace-api.test.ts`

**Coverage:**
- ✅ Model marketplace endpoints
- ✅ Provider listing and health
- ✅ Billing and earnings endpoints
- ✅ Authentication requirements
- ✅ Support request handling
- ✅ Error handling
- ✅ Performance benchmarks

**Test Framework:** Jest with Supertest
**Run Command:**
```bash
cd xergon-marketplace
npm run test:integration
```

### 3. Shell-Based Integration Tests

**Location:** `tests/run-marketplace-tests.sh`

**Coverage:**
- ✅ SDK plugin flows
- ✅ API endpoint health checks
- ✅ Provider registration
- ✅ Model marketplace queries
- ✅ Authentication verification
- ✅ End-to-end lifecycle tests

**Run Commands:**
```bash
# All tests
./tests/run-marketplace-tests.sh

# Specific suites
./tests/run-marketplace-tests.sh --sdk
./tests/run-marketplace-tests.sh --api
./tests/run-marketplace-tests.sh --e2e
./tests/run-marketplace-tests.sh --quick
```

## Test Categories

### Unit Tests
- Test individual functions and methods in isolation
- Mock external dependencies (fetch, file system)
- Fast execution, high coverage

### Integration Tests
- Test API endpoints with real HTTP requests
- Test database interactions
- Test authentication flows
- Moderate execution time

### End-to-End Tests
- Test complete user workflows
- Test multi-step processes
- Test system components working together
- Slower execution, critical path coverage

### Performance Tests
- Response time benchmarks
- Concurrent request handling
- Load testing

## Test Scenarios

### Plugin Marketplace

#### Search & Discovery
1. Search plugins by query string
2. Filter by category
3. Sort by downloads, rating, newest
4. Pagination and pagination limits
5. Featured and trending plugins

#### Plugin Installation
1. Install new plugin
2. Handle already-installed plugins
3. Clean up on failure
4. Cache management
5. Version selection

#### Plugin Management
1. Update to latest version
2. Uninstall plugins
3. List installed plugins
4. Check for updates

#### Plugin Publishing
1. Validate manifest
2. Publish new plugin
3. Handle validation errors

#### Reviews & Ratings
1. Fetch plugin reviews
2. Submit reviews
3. Validate rating range (1-5)
4. Aggregate rating calculation

### Model Marketplace

#### Model Discovery
1. List all models
2. Filter by category (nlp, code, vision, audio, etc.)
3. Filter by tier (free, pro)
4. Featured models
5. Trending models

#### Model Details
1. View model specifications
2. Check pricing
3. View provider information
4. Check availability

#### Provider Information
1. List registered providers
2. Check provider health status
3. View provider uptime
4. Check model pricing per provider
5. Regional distribution

### Authentication & Authorization

#### Protected Endpoints
1. Earnings API
2. Billing API
3. Admin endpoints

#### Authentication Flow
1. Token validation
2. Invalid token rejection
3. Missing auth handling

## Test Data

### Mock Data Sets

#### Plugins
```typescript
const mockPlugins = [
  {
    id: 'plugin-1',
    name: 'test-plugin',
    version: '1.0.0',
    category: 'testing',
    downloads: 100,
    rating: 4.5,
    // ... full plugin structure
  },
  // ... more plugins
];
```

#### Models
```typescript
const mockModels = [
  {
    id: 'llama-3.3-70b',
    name: 'Llama 3.3 70B',
    category: 'nlp',
    tier: 'pro',
    pricePerInputTokenNanoerg: 200,
    // ... full model structure
  },
  // ... more models
];
```

#### Providers
```typescript
const mockProviders = [
  {
    endpoint: 'https://node-001.xergon.us.net',
    name: 'XergonNode-001',
    region: 'US',
    status: 'online',
    uptime: 99.5,
    // ... full provider structure
  },
  // ... more providers
];
```

## Test Environment Setup

### Prerequisites

```bash
# Install dependencies
npm install

# Set up test environment variables
export TEST_BASE_URL=http://localhost:3000
export TEST_AUTH_TOKEN=test-token
export XERGON_RELAY_BASE=http://127.0.0.1:9090

# Start services for integration tests
# Terminal 1: Next.js dev server
cd xergon-marketplace
npm run dev

# Terminal 2: Relay server
cd xergon-relay
cargo run --release
```

### Test Configuration

**Jest Config (`jest.config.js`):**
```javascript
module.exports = {
  testEnvironment: 'node',
  preset: 'ts-jest',
  testMatch: ['**/__tests__/**/*.test.ts'],
  collectCoverageFrom: ['src/**/*.ts'],
  coverageThreshold: {
    global: {
      branches: 80,
      functions: 80,
      lines: 80,
      statements: 80,
    },
  },
};
```

## Running Tests

### Quick Start

```bash
# Run all marketplace tests
./tests/run-marketplace-tests.sh

# Quick smoke tests
./tests/run-marketplace-tests.sh --quick
```

### By Component

```bash
# SDK tests only
./tests/run-marketplace-tests.sh --sdk

# API tests only
./tests/run-marketplace-tests.sh --api

# E2E tests only
./tests/run-marketplace-tests.sh --e2e
```

### Verbose Mode

```bash
# Show detailed output
./tests/run-marketplace-tests.sh --verbose
```

## Test Coverage Goals

| Component | Line Coverage | Branch Coverage |
|-----------|---------------|-----------------|
| SDK Plugin Marketplace | 90% | 85% |
| API Routes | 85% | 80% |
| Integration Tests | 80% | 75% |
| **Overall** | **85%** | **80%** |

## Known Test Gaps

### Current Limitations

1. **End-to-End Payment Flows**
   - Ergo transaction signing integration
   - Real blockchain interactions
   - **Status:** Requires testnet setup

2. **Provider Registration E2E**
   - Full provider onboarding flow
   - UTXO-based registration
   - **Status:** Integration test pending

3. **Load Testing**
   - High-concurrency scenarios
   - Stress testing provider endpoints
   - **Status:** Not yet implemented

4. **Security Testing**
   - Penetration testing
   - Vulnerability scanning
   - **Status:** Requires security audit

### Planned Tests

- [ ] Contract deployment tests
- [ ] Settlement flow tests
- [ ] Provider heartbeat tests
- [ ] Multi-provider routing tests
- [ ] Rate limiting tests
- [ ] DDoS simulation tests

## Test Results Reporting

### Test Output Format

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

### CI/CD Integration

Tests should run on:
- Every pull request
- Every commit to main branch
- Scheduled daily runs

**GitHub Actions Workflow:**
```yaml
name: Marketplace Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Run Marketplace Tests
        run: |
          cd tests
          chmod +x run-marketplace-tests.sh
          ./run-marketplace-tests.sh --quick
```

## Maintenance

### Adding New Tests

1. **Unit Tests:** Add to appropriate `__tests__` directory
2. **Integration Tests:** Add to `marketplace-api.test.ts`
3. **Shell Tests:** Add test functions to `run-marketplace-tests.sh`

### Test Updates

When modifying marketplace functionality:
1. Update existing tests if behavior changes
2. Add new tests for new features
3. Run full test suite before committing

### Test Health Monitoring

- Track test failure trends
- Monitor test execution time
- Review flaky tests
- Update test data periodically

## Troubleshooting

### Common Issues

**Issue:** Tests fail with "Connection refused"
**Solution:** Ensure services are running on expected ports

**Issue:** Mock data not loading
**Solution:** Check test directory structure and file paths

**Issue:** Authentication tests fail
**Solution:** Set `TEST_AUTH_TOKEN` environment variable

### Debug Mode

```bash
# Enable verbose logging
export VERBOSE=true
./tests/run-marketplace-tests.sh --verbose
```

## Next Steps

1. ✅ Create SDK plugin marketplace tests
2. ✅ Create API integration tests
3. ✅ Create shell-based test suite
4. ⏳ Set up CI/CD integration
5. ⏳ Implement load testing
6. ⏳ Add security testing
7. ⏳ Create test data fixtures
8. ⏳ Document test coverage gaps

---

**Last Updated:** 2026-04-13
**Test Suite Version:** 1.0.0
**Maintainer:** Xergon Development Team
