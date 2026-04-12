# Xergon Network - Component Wiring Map

**Date:** 2026-04-11  
**Legend:**  
✅ = Fully wired and working  
⚠️ = Partially wired / stub implementation  
❌ = Not wired / missing connection  
🔴 = Critical gap (blocks functionality)

---

## System Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Xergon Network Stack                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    FRONTEND (Marketplace)                        │    │
│  │                    Next.js 15 + React 19                        │    │
│  ├─────────────────────────────────────────────────────────────────┤    │
│  │                                                                   │    │
│  │  ✅ Playground UI  ──⚠️─→  SDK Client  ──❌─→  Relay API        │    │
│  │  ✅ Provider List  ──❌─→  SDK Client  ──❌─→  Relay API        │    │
│  │  ✅ Model Browser  ──⚠️─→  SDK Client  ──❌─→  Relay API        │    │
│  │  ❌ Bridge UI     ──❌─→  SDK Client  ──❌─→  Relay API        │    │
│  │  ❌ Governance    ──❌─→  SDK CLI     ──❌─→  On-Chain         │    │
│  │  ❌ GPU Bazar     ──❌─→  SDK Client  ──❌─→  Relay API        │    │
│  │  ⚠️ i18n System   ──❌─→  Components  (not integrated)         │    │
│  │                                                                   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                   │                                       │
│                                   ▼                                       │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    MIDDLEWARE (Relay)                            │    │
│  │                    Axum + Tokio (Rust)                          │    │
│  ├─────────────────────────────────────────────────────────────────┤    │
│  │                                                                   │    │
│  │  🔴 Auth Middleware      ──❌─→  Not implemented                │    │
│  │  🔴 Rate Limiting        ──❌─→  Not implemented                │    │
│  │  ⚠️ Request Routing      ──→  Basic (first provider)           │    │
│  │  ❌ Provider Registry    ──❌─→  Empty (no registration)        │    │
│  │  ❌ Health Monitoring    ──❌─→  No heartbeat endpoint          │    │
│  │  ❌ Caching              ──❌─→  Not implemented                │    │
│  │  ❌ Settlement Recording ──❌─→  Not implemented                │    │
│  │  ❌ Metrics/Telemetry    ──❌─→  Not implemented                │    │
│  │                                                                   │    │
│  │  Endpoints Implemented:                                           │    │
│  │    ✅ GET  /health                                               │    │
│  │    ✅ POST /v1/chat/completions  (basic proxy)                  │    │
│  │    ❌ POST /register         (missing)                           │    │
│  │    ❌ POST /heartbeat        (missing)                           │    │
│  │    ❌ GET  /v1/providers     (missing)                           │    │
│  │    ❌ GET  /v1/models        (missing)                           │    │
│  │    ❌ GET  /v1/balance       (missing)                           │    │
│  │                                                                   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                   │                                       │
│                                   ▼                                       │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    AGENT (Sidecar)                               │    │
│  │                    Axum + Tokio (Rust)                          │    │
│  ├─────────────────────────────────────────────────────────────────┤    │
│  │                                                                   │    │
│  │  ✅ PoNW Scoring     ──❌─→  Not reported to relay              │    │
│  │  ✅ Health Monitor   ──❌─→  Not reported to relay              │    │
│  │  ✅ Inference Proxy  ──⚠️─→  Works (local LLM only)            │    │
│  │  ❌ Settlement       ──❌─→  Not integrated with relay          │    │
│  │  ❌ Provider Reg     ──❌─→  No relay connection                │    │
│  │  ✅ Peer Discovery   ──❌─→  P2P only (no relay sync)           │    │
│  │                                                                   │    │
│  │  Modules: 120+ declared, ~20 active, 100+ dead code             │    │
│  │                                                                   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                   │                                       │
│              ┌────────────────────┼────────────────────┐                │
│              │                    │                    │                │
│              ▼                    ▼                    ▼                │
│  ┌────────────────┐    ┌────────────────┐    ┌────────────────┐       │
│  │  Ergo Node     │    │  LLM Backend   │    │  Ergo Chain    │       │
│  │  (:9053)       │    │  (Ollama/etc)  │    │  (Testnet)     │       │
│  │  ✅ Connected  │    │  ⚠️ Local only │    │  ⚠️ Partial    │       │
│  └────────────────┘    └────────────────┘    └────────────────┘       │
│                                                                   │     │
└───────────────────────────────────────────────────────────────────────┘
```

---

## Critical Data Flows

### 1. User Request Flow (Chat Completion)

```
✅ WORKING (Basic Path):
User → Marketplace → SDK → Relay → Agent → LLM → Back to User

⚠️ ISSUES:
- No authentication (anyone can use)
- No rate limiting (no DoS protection)
- No provider selection (always first provider)
- No settlement (no payment)
- No metrics (no tracking)
```

### 2. Provider Registration Flow

```
❌ COMPLETELY BROKEN:
Agent → [NO /register endpoint] → Relay

EXPECTED:
Agent → POST /register → Relay validates → Store in ProviderRegistry
Agent → POST /heartbeat (30s) → Relay updates health status
Relay → GET /v1/providers → Marketplace displays providers

CURRENT STATE:
- No /register endpoint
- No /heartbeat endpoint
- No ProviderRegistry
- Providers can't register
- Marketplace can't show providers
```

### 3. Settlement Flow

```
❌ COMPLETELY MISSING:
Inference → [NO RECORDING] → Settlement

EXPECTED:
1. Agent completes inference
2. Count tokens (input + output)
3. Create usage_proof struct
4. Sign with provider key
5. Submit to Relay
6. Relay batches proofs
7. Create on-chain TX (ERG)
8. Record settlement history

CURRENT STATE:
- No usage tracking
- No proof generation
- No settlement recording
- No payment processing
```

### 4. Authentication Flow

```
❌ NOT IMPLEMENTED:
User → [NO AUTH] → Relay

EXPECTED:
1. User has API key (public + private)
2. Client signs request: HMAC-SHA256(private_key, payload)
3. Request: X-User-PK: public_key, X-Signature: signature
4. Relay verifies: HMAC-SHA256(public_key, payload) == signature
5. Check rate limits, balance, permissions
6. Allow/deny request

CURRENT STATE:
- No auth middleware
- No signature verification
- No rate limiting
- No permission checking
```

---

## Feature Wiring Matrix

| Feature | Frontend | SDK | Relay | Agent | On-Chain | Status |
|---------|----------|-----|-------|-------|----------|--------|
| **Core Inference** | ⚠️ | ⚠️ | ⚠️ | ✅ | ❌ | 40% |
| Provider Registration | ❌ | ❌ | ❌ | ❌ | ❌ | 0% 🔴 |
| Settlement/Payment | ❌ | ❌ | ❌ | ⚠️ | ⚠️ | 20% 🔴 |
| Authentication | ❌ | ❌ | ❌ | ❌ | ❌ | 0% 🔴 |
| Rate Limiting | ❌ | ❌ | ❌ | ❌ | ❌ | 0% 🔴 |
| Health Monitoring | ❌ | ❌ | ❌ | ⚠️ | ❌ | 10% |
| Provider Discovery | ❌ | ❌ | ❌ | ⚠️ | ❌ | 10% |
| i18n/L10n | ⚠️ | ❌ | N/A | N/A | N/A | 30% |
| Cross-Chain Bridge | ❌ | ✅ | ❌ | ❌ | ⚠️ | 30% |
| Governance | ❌ | ⚠️ | ❌ | ✅ | ✅ | 40% |
| Oracle Integration | ❌ | ❌ | ❌ | ✅ | ✅ | 30% |
| GPU Bazar | ❌ | ✅ | ❌ | ✅ | ✅ | 30% |
| Caching | ❌ | ❌ | ❌ | ❌ | ❌ | 0% |
| Metrics/Telemetry | ❌ | ❌ | ❌ | ❌ | ❌ | 0% |

**Overall: 22% Complete** 🔴

---

## Module Wiring Map

### xergon-relay (100+ modules, only 4 active)

**Active (4):**
- ✅ `config.rs` - Configuration loading
- ✅ `handlers.rs` - API endpoints (2 routes)
- ✅ `provider.rs` - Basic HTTP proxy
- ✅ `types.rs` - Type definitions

**Dead Code (96+):**
- ❌ `quantum_crypto.rs` - Never used
- ❌ `homomorphic_compute.rs` - Never used
- ❌ `zkp_verification.rs` - Never used
- ❌ `grpc/` - Never used
- ❌ `cross_provider_orchestration.rs` - Never used
- ❌ `speculative_decoding.rs` - Never used
- ❌ `request_fusion.rs` - Never used
- ❌ `continuous_batching.rs` - Never used
- ❌ `token_streaming.rs` - Never used
- ❌ `scheduling_optimizer.rs` - Never used
- ❌ `utxo_consolidation.rs` - Never used
- ❌ `storage_rent_monitor.rs` - Never used
- ❌ `tokenomics_engine.rs` - Never used
- ❌ `provider_attestation.rs` - Never used
- ❌ `ergopay_signing.rs` - Never used
- ❌ `protocol_adapter.rs` - Never used
- ❌ `rent_guard.rs` - Duplicate
- ❌ `openapi.rs` - Never used
- ❌ `response_cache_headers.rs` - Duplicate
- ❌ `babel_box_discovery.rs` - Never used
- ❌ ... 76+ more

**Duplicates (should consolidate):**
- ⚠️ `rate_limit.rs` + `rate_limit_tiers.rs` + `rate_limiter_v2.rs`
- ⚠️ `health.rs` + `health_score.rs` + `health_monitor_v2.rs`
- ⚠️ `cache.rs` + `cache_middleware.rs` + `semantic_cache.rs`
- ⚠️ `ws.rs` + `websocket_v2.rs` + `ws_pool.rs`
- ⚠️ `coalesce.rs` + `coalesce_buffer.rs` + `request_coalescing.rs`

### xergon-agent (120 modules, ~20 active)

**Active (~20):**
- ✅ `api/` - REST API handlers
- ✅ `pown/` - Proof-of-Node-Work
- ✅ `node_health/` - Ergo node monitoring
- ✅ `peer_discovery/` - P2P discovery
- ✅ `settlement/` - Settlement engine (not wired)
- ✅ `chain/` - Blockchain operations
- ✅ `config/` - Configuration
- ✅ `inference/` - LLM proxy
- ✅ `wallet/` - Key management
- ✅ `signing/` - Transaction signing
- ✅ `provider_registry/` - Local registry
- ✅ `metrics/` - Observability
- ✅ `auth/` - Authentication
- ✅ `gpu_rental/` - GPU contracts (not wired)
- ✅ `governance/` - Governance (not wired)
- ✅ `oracle_service/` - Oracle pool (not wired)
- ✅ `health_deep/` - Deep health checks
- ✅ `relay_client/` - Relay connection (broken)
- ✅ `marketplace_listing/` - Listings (not wired)
- ✅ `inference_gateway/` - Gateway (not wired)

**Dead/Unused (~100):**
- ❌ `alignment_training/` - Never used
- ❌ `federated_learning/` - Never used
- ❌ `model_optimizer/` - Never used
- ❌ `chaos_testing/` - Development only
- ❌ `quantization_v2/` - Duplicate
- ❌ ... 95+ more

---

## API Endpoint Wiring

### Implemented Endpoints

**Marketplace (Next.js):**
```
✅ GET  /api/health                    → Internal health check
⚠️ GET  /api/xergon-relay/health      → Proxies to relay /v1/health
❌ GET  /api/xergon-relay/providers   → Proxies to non-existent /v1/providers
❌ GET  /api/xergon-relay/stats       → Proxies to non-existent /v1/stats
✅ GET  /api/xergon-relay/health      → Returns mock degraded data
```

**Relay (Axum):**
```
✅ GET  /health                        → Basic health response
✅ POST /v1/chat/completions           → Basic proxy to provider
❌ POST /register                      → Missing
❌ POST /heartbeat                     → Missing
❌ GET  /v1/providers                  → Missing
❌ GET  /v1/models                     → Missing
❌ GET  /v1/balance                    → Missing
❌ POST /v1/usage                      → Missing
```

**Agent (Axum):**
```
✅ GET  /health                        → Agent health
✅ POST /v1/chat/completions           → Proxy to LLM
✅ GET  /provider/health               → Provider health
❌ POST /register                      → Missing (should register to relay)
❌ GET  /providers                     → Missing (peer discovery)
```

---

## Missing Integrations

### 1. Frontend ↔ SDK

```
Marketplace uses: lib/api/client.ts (endpoints)
SDK provides: XergonClientCore

Wiring: ⚠️ PARTIAL
- ✅ listModels → sdk.models.list()
- ✅ infer → sdk.chat.completions.create()
- ❌ inferStream → Direct fetch (bypasses SDK)
- ❌ Leaderboard → sdk.leaderboard() (not implemented)
```

### 2. SDK ↔ Relay

```
SDK uses: DEFAULT_BASE_URL = 'https://relay.xergon.gg'
Marketplace uses: API_BASE = 'http://127.0.0.1:9090'

Wiring: ❌ BROKEN
- Different base URLs
- SDK hardcoded to production
- No local development support
```

### 3. Relay ↔ Agent

```
Relay should: Forward requests to registered agents
Agent should: Register with relay, send heartbeats

Wiring: ❌ NOT CONNECTED
- No registration protocol
- No heartbeat mechanism
- No provider discovery
- Agents run standalone
```

### 4. Agent ↔ On-Chain

```
Agent should: Submit settlement proofs to Ergo chain
Chain should: Verify proofs, distribute ERG

Wiring: ⚠️ PARTIAL
- ✅ Settlement engine exists
- ✅ Contract compilation works
- ❌ Not called from inference flow
- ❌ No relay coordination
```

---

## Priority Fix Order

### Week 1: Critical Infrastructure

1. **Add Provider Registration** 🔴
   - Implement `/register` endpoint in relay
   - Add registration logic in agent
   - Store providers in memory/DB

2. **Add Heartbeat System** 🔴
   - Implement `/heartbeat` endpoint in relay
   - Add heartbeat loop in agent (30s)
   - Track provider health status

3. **Wire Settlement Flow** 🔴
   - Record usage in agent after inference
   - Create settlement batch job
   - Generate on-chain transactions

4. **Add Basic Auth** 🔴
   - Implement HMAC signature verification
   - Add auth middleware to relay
   - Generate API keys for users

### Week 2: Core Features

5. **Implement Rate Limiting** 🟡
   - Add rate limit middleware
   - Configure tiers (free, premium)
   - Track usage per user

6. **Wire Marketplace APIs** 🟡
   - Connect SDK to local relay
   - Implement provider list endpoint
   - Display providers in UI

7. **Add Caching** 🟡
   - Implement response cache
   - Cache provider list
   - Cache model registry

### Week 3: Polish & Cleanup

8. **Remove Dead Code** 🟢
   - Delete 96+ unused relay modules
   - Remove 100+ unused agent modules
   - Consolidate duplicates

9. **Wire i18n** 🟢
   - Integrate translations into components
   - Add locale switcher
   - Test all 4 locales

10. **Add Missing UI** 🟢
    - Bridge interface
    - Governance dashboard
    - GPU rental UI

---

## Files to Create/Modify

### Create (New Files)

```
xergon-relay/src/auth.rs              → HMAC auth middleware
xergon-relay/src/rate_limit.rs        → Rate limiting middleware
xergon-relay/src/registry.rs          → Provider registry
xergon-relay/src/heartbeat.rs         → Heartbeat handler
xergon-relay/src/settlement.rs        → Settlement recording

xergon-agent/src/relay_client.rs      → Relay connection client
xergon-agent/src/registration.rs      → Provider registration
xergon-agent/src/heartbeat.rs         → Heartbeat sender

xergon-marketplace/components/bridge/ → Bridge UI
xergon-marketplace/components/governance/ → Governance UI
xergon-marketplace/components/gpu/    → GPU rental UI
```

### Modify (Existing Files)

```
xergon-relay/src/main.rs              → Add middleware stack
xergon-relay/src/handlers.rs          → Add new endpoints
xergon-relay/Cargo.toml               → Remove dead deps

xergon-agent/src/main.rs              → Add relay client
xergon-agent/src/lib.rs               → Remove dead modules
xergon-agent/Cargo.toml               → Remove dead deps

xergon-marketplace/lib/api/config.ts  → Fix SDK base URL
xergon-marketplace/lib/api/client.ts  → Use SDK properly
xergon-marketplace/components/        → Add i18n usage
```

---

**Last Updated:** 2026-04-11  
**Prepared by:** Hermes Agent  
**Model:** Qwen3.5-122B-A10B-NVFP4
