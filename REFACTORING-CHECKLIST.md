# Xergon Network - Refactoring Checklist

**Status:** Ready to execute  
**Created:** April 10, 2026  
**Reference:** REFACTORING-PLAN.md

---

## Phase 1: Critical Cleanup

### 1.1 Remove Full-Module Dead Code (Relay)

**Priority:** CRITICAL  
**Estimated Time:** 4-6 hours

**Modules to Remove:**

```bash
# Experimental features (likely unused)
rm xergon-relay/src/quantum_crypto.rs
rm xergon-relay/src/homomorphic_compute.rs
rm xergon-relay/src/zkp_verification.rs
rm xergon-relay/src/encrypted_inference.rs

# Likely unused infrastructure
rm xergon-relay/src/grpc/proto.rs
rm xergon-relay/src/openapi.rs
rm xergon-relay/src/schemas.rs
rm xergon-relay/src/multi_region.rs
rm xergon-relay/src/admin.rs
rm xergon-relay/src/websocket_v2.rs
rm xergon-relay/src/api_version.rs
rm xergon-relay/src/priority_queue.rs
rm xergon-relay/src/capability_negotiation.rs
rm xergon-relay/src/model_registry.rs
rm xergon-relay/src/protocol_adapter.rs
rm xergon-relay/src/provider_attestation.rs
rm xergon-relay/src/storage_rent_monitor.rs
rm xergon-relay/src/babel_box_discovery.rs
rm xergon-relay/src/rent_guard.rs
rm xergon-relay/src/tokenomics_engine.rs
rm xergon-relay/src/health_monitor_v2.rs
```

**Update main.rs:**
```bash
# Remove these module declarations from xergon-relay/src/main.rs
# Lines to remove (example):
# mod quantum_crypto;
# mod homomorphic_compute;
# mod zkp_verification;
# ... etc for all 25+ modules
```

**Verify:**
```bash
cd /home/n1ur0/Xergon-Network/xergon-relay
cargo build --release
cargo test
```

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

### 1.2 Consolidate Duplicates (Relay)

**Priority:** HIGH  
**Estimated Time:** 6-8 hours

#### Rate Limiting
- **Keep:** `rate_limit.rs`
- **Remove:** `rate_limit_tiers.rs`, `rate_limiter_v2.rs`
- **Action:** Check if unique functionality exists, migrate if needed

```bash
# Review for unique functionality
grep -r "rate_limit_tiers" xergon-relay/src/
grep -r "rate_limiter_v2" xergon-relay/src/

# If no unique usage, remove
rm xergon-relay/src/rate_limit_tiers.rs
rm xergon-relay/src/rate_limiter_v2.rs
```

**Status:** [ ] Not started [ ] In progress [ ] Complete

#### Health Monitoring
- **Keep:** `health.rs`
- **Remove:** `health_monitor_v2.rs`, `health_score.rs`

```bash
# Review for unique functionality
grep -r "health_monitor_v2" xergon-relay/src/
grep -r "health_score" xergon-relay/src/

# If no unique usage, remove
rm xergon-relay/src/health_monitor_v2.rs
rm xergon-relay/src/health_score.rs
```

**Status:** [ ] Not started [ ] In progress [ ] Complete

#### WebSocket
- **Keep:** `ws.rs`
- **Remove:** `websocket_v2.rs`

```bash
# Review for unique functionality
grep -r "websocket_v2" xergon-relay/src/

# If no unique usage, remove
rm xergon-relay/src/websocket_v2.rs
```

**Status:** [ ] Not started [ ] In progress [ ] Complete

#### Request Deduplication
- **Keep:** `dedup.rs`
- **Remove:** `request_dedup_v2.rs`

```bash
# Review for unique functionality
grep -r "request_dedup_v2" xergon-relay/src/

# If no unique usage, remove
rm xergon-relay/src/request_dedup_v2.rs
```

**Status:** [ ] Not started [ ] In progress [ ] Complete

#### Caching (Evaluate)
- **Modules:** `cache.rs`, `semantic_cache.rs`
- **Action:** Determine if genuinely different use cases
- **Decision:** Keep both OR consolidate

**Status:** [ ] Not started [ ] In progress [ ] Complete

**Verify:**
```bash
cd /home/n1ur0/Xergon-Network/xergon-relay
cargo build --release
cargo test
```

---

### 1.3 Verify Compilation

**Priority:** CRITICAL  
**Estimated Time:** 1-2 hours

```bash
# Agent
cd /home/n1ur0/Xergon-Network/xergon-agent
cargo clean
cargo build --release
cargo test

# Relay
cd ../xergon-relay
cargo clean
cargo build --release
cargo test
```

**Expected:** No compilation errors, all tests pass

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

## Phase 2: Module Audit & Feature Flagging

### 2.1 Audit `#[allow(dead_code)]` Annotations

**Priority:** HIGH  
**Estimated Time:** 8-10 hours

**Process:**
1. List all files with dead_code annotations
2. For each file, determine:
   - Is it actually used?
   - Is it marked as "TODO" or "will be used"?
   - Should it be removed, feature-flagged, or kept with docs?

**Commands:**
```bash
# Find all files with dead_code
grep -r "#\[allow(dead_code)\]" xergon-relay/src/ --include="*.rs"
grep -r "#\[allow(dead_code)\]" xergon-agent/src/ --include="*.rs"

# Count occurrences
grep -r "#\[allow(dead_code)\]" xergon-relay/src/ --include="*.rs" | wc -l
grep -r "#\[allow(dead_code)\]" xergon-agent/src/ --include="*.rs" | wc -l
```

**Categorization Template:**

| File | Lines | Category | Action |
|------|-------|----------|--------|
| quantum_crypto.rs | 1-500 | Remove | Delete |
| protocol/actions.rs | 99, 397 | Feature-flag | Add cfg |
| settlement/market.rs | 45 | Keep with docs | Add comment |

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

### 2.2 Add Feature Flags

**Priority:** MEDIUM  
**Estimated Time:** 4-6 hours

**Update Cargo.toml (Relay):**
```toml
[features]
default = []
experimental = ["quantum", "zkp", "homomorphic", "encrypted-inference"]
quantum = []
zkp = []
homomorphic = []
encrypted-inference = []
grpc = []
admin-api = []
multi-region = []
cross-chain = []
oracle-consumer = []
```

**Update module declarations:**
```rust
// In xergon-relay/src/main.rs
#[cfg(feature = "quantum")]
mod quantum_crypto;

#[cfg(feature = "zkp")]
mod zkp_verification;

#[cfg(feature = "grpc")]
mod grpc;
```

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

### 2.3 Clean Up TODO/FIXME Comments

**Priority:** MEDIUM  
**Estimated Time:** 2-4 hours

**Commands:**
```bash
# Find all TODO/FIXME/HACK/WIP
grep -r "TODO\|FIXME\|HACK\|WIP" xergon-relay/src/ --include="*.rs" | head -50
grep -r "TODO\|FIXME\|HACK\|WIP" xergon-agent/src/ --include="*.rs" | head -50
```

**Focus areas:**
- `protocol/actions.rs` (many "production wiring" TODOs)
- `settlement/market.rs` (dynamic pricing TODO)

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

## Phase 3: Documentation & Wiring

### 3.1 Document Implemented Features

**Priority:** HIGH  
**Estimated Time:** 8-10 hours

**Features to document:**

1. **i18n/L10n System** (2 hours)
   - Location: Dictionary file (1,359 lines)
   - Integration: Relay and marketplace
   - Usage examples needed

2. **Cross-Chain Bridge** (2 hours)
   - 6-chain support
   - Invoice flow
   - Refund mechanism

3. **Governance System** (2 hours)
   - CLI commands
   - On-chain mechanism
   - Voting/delegation

4. **Oracle Integration** (2 hours)
   - EIP-23 aggregation
   - Staleness detection
   - Pricing usage

5. **GPU Bazar** (2 hours)
   - Rental listings
   - Time-boxed contracts
   - Reputation system

**Deliverables:**
- Update `docs/implementation-docs.md`
- Create new guides in `docs/`

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

### 3.2 Create Module Dependency Graph

**Priority:** MEDIUM  
**Estimated Time:** 3-4 hours

**Tools:**
```bash
# Generate dependency tree
cargo tree > dependency-tree.txt

# Or use cargo-deps if installed
cargo deps --output-format dot > dependencies.dot
```

**Output:** Visual diagram showing:
- Module relationships
- Circular dependencies
- Tightly-coupled components

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

### 3.3 Update Remaining Documentation

**Priority:** MEDIUM  
**Estimated Time:** 4-6 hours

**Files to update:**
- [ ] `xergon-api-reference.md` - Update endpoints
- [ ] All integration guides - Remove "conceptual" language
- [ ] Unimplemented integrations - Mark as "Planned"

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

## Phase 4: Testing & Validation

### 4.1 Add Integration Tests

**Priority:** HIGH  
**Estimated Time:** 10-15 hours

**Test coverage needed:**

1. **End-to-end flow** (3 hours)
   - Marketplace → Relay → Agent → LLM
   - Full request/response cycle

2. **Settlement flow** (3 hours)
   - Usage → Payment → On-chain verification
   - Box creation and confirmation

3. **Provider registration** (2 hours)
   - Heartbeat → Box update → Chain sync

4. **Cross-chain bridge** (3 hours)
   - Invoice → Payment → Refund flow

5. **Governance** (2 hours)
   - Propose → Vote → Execute

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

### 4.2 Performance Benchmarking

**Priority:** MEDIUM  
**Estimated Time:** 4-6 hours

**Metrics to track:**
- Request latency (before/after)
- Throughput (req/sec)
- Memory usage
- Build times

**Commands:**
```bash
# Benchmark relay
cd xergon-relay
cargo bench

# Or use custom benchmark scripts
./scripts/load_test.sh
```

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

### 4.3 Load Testing

**Priority:** MEDIUM  
**Estimated Time:** 4-6 hours

**Test scenarios:**
1. 100+ concurrent users (2 hours)
2. High-frequency heartbeats (1 hour)
3. Batch settlement (1 hour)
4. Cross-chain bridge stress (2 hours)

**Status:** [ ] Not started [ ] In progress [ ] Complete

---

## Progress Tracking

### Overall Progress

| Phase | Status | Progress |
|-------|--------|----------|
| Phase 1: Critical Cleanup | [ ] | 0% |
| Phase 2: Module Audit | [ ] | 0% |
| Phase 3: Documentation | [ ] | 0% |
| Phase 4: Testing | [ ] | 0% |
| **Total** | | **0%** |

### Key Milestones

- [ ] Phase 1 Complete (Week 1-2)
- [ ] Phase 2 Complete (Week 2-3)
- [ ] Phase 3 Complete (Week 3-4)
- [ ] Phase 4 Complete (Week 4-5)
- [ ] All tests passing
- [ ] Documentation complete

---

## Risk Mitigation

### Before Starting Each Phase

1. **Create git branch:**
   ```bash
   git checkout -b refactor/phase-1-cleanup
   ```

2. **Commit current state:**
   ```bash
   git commit -m "Pre-refactoring baseline"
   ```

3. **Run full test suite:**
   ```bash
   cargo test --all
   ```

### If Something Breaks

1. **Don't panic** - You're on a feature branch
2. **Identify the issue:**
   - Which module was removed?
   - What dependency was broken?
3. **Rollback if needed:**
   ```bash
   git checkout main
   git checkout -b refactor/phase-1-debug
   ```
4. **Investigate and fix** before continuing

---

## Tools & Commands Reference

### Code Analysis
```bash
# Find dead_code annotations
grep -r "#\[allow(dead_code)\]" . --include="*.rs"

# Find TODO/FIXME comments
grep -r "TODO\|FIXME" . --include="*.rs"

# Count lines in modules
find xergon-relay/src -name "*.rs" -exec wc -l {} \; | sort -n

# Check module usage
grep -r "use.*::module_name" . --include="*.rs"
```

### Build & Test
```bash
# Clean build
cargo clean
cargo build --release

# Run tests
cargo test --all

# Check for warnings
cargo clippy -- -D warnings

# Format code
cargo fmt
```

### Feature Flags
```bash
# Build with specific feature
cargo build --features "experimental"

# Build without default features
cargo build --no-default-features --features "quantum,zkp"
```

---

## Notes

- **Always commit changes** before moving to next step
- **Run tests after each modification**
- **Document decisions** in commit messages
- **Communicate progress** to team regularly

---

**Last Updated:** April 10, 2026  
**Prepared by:** Hermes Agent
