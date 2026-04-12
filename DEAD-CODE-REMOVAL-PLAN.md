# Dead Code Removal Plan

**Date:** 2026-04-10  
**Scope:** Remove or feature-flag 25+ unused relay modules

---

## Immediate Actions (Priority 1)

### Remove These Modules (Not Used)

These modules have `#![allow(dead_code)]` and are clearly experimental/unused:

1. **quantum_crypto** - Quantum cryptography (not integrated)
2. **homomorphic_compute** - Homomorphic encryption (not integrated)
3. **zkp_verification** - Zero-knowledge proofs (not integrated)
4. **grpc** - gRPC support (unused, HTTP only)
5. **cross_provider_orchestration** - Not wired
6. **speculative_decoding** - Not wired
7. **request_fusion** - Not wired
8. **continuous_batching** - Not wired
9. **token_streaming** - Duplicate of existing streaming
10. **scheduling_optimizer** - Not wired
11. **utxo_consolidation** - Not wired
12. **storage_rent_monitor** - Not wired
13. **tokenomics_engine** - Not wired
14. **provider_attestation** - Not wired
15. **ergopay_signing** - Not wired
16. **protocol_adapter** - Not wired
17. **rent_guard** - Duplicate of existing rent logic
18. **openapi** - Not generating OpenAPI
19. **response_cache_headers** - Duplicate cache logic
20. **babel_box_discovery** - Not wired

### Consolidate These (Duplicates)

1. **rate_limit** + **rate_limit_tiers** + **rate_limiter_v2** → Keep `rate_limit`, remove others
2. **health** + **health_score** + **health_monitor_v2** → Keep `health`, remove others
3. **cache** + **cache_middleware** + **semantic_cache** → Keep `cache`, remove others
4. **websocket_v2** + **ws** + **ws_pool** → Keep `ws`, remove others
5. **coalesce** + **coalesce_buffer** + **request_coalescing** → Keep `coalesce`, remove others

### Feature-Flag These (May Be Used Later)

1. **cross_chain_bridge** - Keep with feature flag
2. **cross_chain_event_router** - Keep with feature flag
3. **oracle_consumer** - Keep with feature flag
4. **governance** - Keep with feature flag
5. **graphql** - Keep with feature flag
6. **admin** - Keep with feature flag

---

## Execution Plan

### Step 1: Add Feature Flags to Cargo.toml

```toml
[features]
default = ["core", "auth", "routing", "cache", "metrics"]

experimental = []
cross-chain = ["experimental"]
quantum-crypto = ["experimental"]
zkp = ["experimental"]
homomorphic = ["experimental"]
grpc = ["experimental"]
```

### Step 2: Wrap Modules in Feature Flags

In `src/main.rs`:

```rust
#[cfg(feature = "quantum-crypto")]
pub mod quantum_crypto;

#[cfg(feature = "grpc")]
pub mod grpc;

// ... etc
```

### Step 3: Remove Unused Modules

Delete these files:
- `src/quantum_crypto.rs`
- `src/homomorphic_compute.rs`
- `src/zkp_verification.rs`
- `src/grpc/` directory
- `src/cross_provider_orchestration.rs`
- `src/speculative_decoding.rs`
- `src/request_fusion.rs`
- `src/continuous_batching.rs`
- `src/token_streaming.rs`
- `src/scheduling_optimizer.rs`
- `src/utxo_consolidation.rs`
- `src/storage_rent_monitor.rs`
- `src/tokenomics_engine.rs`
- `src/provider_attestation.rs`
- `src/ergopay_signing.rs`
- `src/protocol_adapter.rs`
- `src/rent_guard.rs`
- `src/openapi.rs`
- `src/response_cache_headers.rs`
- `src/babel_box_discovery.rs`

### Step 4: Consolidate Duplicates

Keep these:
- `src/rate_limit.rs` (remove `rate_limit_tiers.rs`, `rate_limiter_v2.rs`)
- `src/health.rs` (remove `health_score.rs`, `health_monitor_v2.rs`)
- `src/cache.rs` (remove `cache_middleware.rs`, `semantic_cache.rs`)
- `src/ws.rs` (remove `websocket_v2.rs`, `ws_pool.rs`)
- `src/coalesce.rs` (remove `coalesce_buffer.rs`, `request_coalescing.rs`)

### Step 5: Update main.rs

Remove module declarations for deleted modules.

### Step 6: Verify Compilation

```bash
cd xergon-relay
cargo build --release
cargo test
```

---

## Expected Results

**Before:**
- 124 Rust files
- 92 module declarations
- 25+ full-module dead_code suppressions
- ~66,400 bytes in main.rs alone

**After:**
- ~80 Rust files (35% reduction)
- ~50 module declarations
- 0 dead_code suppressions
- Cleaner, more maintainable codebase

---

## Risk Assessment

**Low Risk:**
- Experimental features (quantum_crypto, homomorphic, etc.) - Not used
- Duplicate modules (rate_limit_v2, health_v2) - Originals exist
- Unused protocols (gRPC, OpenAPI gen) - Not integrated

**Medium Risk:**
- Cross-chain bridge - May be needed later, feature-flag instead of remove
- Governance - May be needed later, feature-flag instead of remove
- Oracle consumer - May be needed later, feature-flag instead of remove

**Mitigation:**
- Keep feature-flagged modules in git history
- Document removal rationale
- Can restore from git if needed

---

## Rollback Plan

If issues arise:
```bash
git checkout HEAD -- src/
git checkout HEAD -- Cargo.toml
cargo build --release
```

All changes are tracked in git, full rollback possible.

---

## Verification Commands

```bash
# Check compilation
cargo check --release

# Run tests
cargo test --release

# Check for dead code warnings
cargo clippy --release -- -W dead_code

# Check binary size
ls -lh target/release/xergon-relay
```

---

## Timeline

- **Day 1:** Add feature flags, remove obvious dead code
- **Day 2:** Consolidate duplicates, verify compilation
- **Day 3:** Run tests, fix any issues
- **Day 4:** Performance benchmarking
- **Day 5:** Documentation updates

---

**Status:** Ready to execute  
**Estimated Time:** 2-3 hours  
**Risk:** Low (all changes reversible via git)
