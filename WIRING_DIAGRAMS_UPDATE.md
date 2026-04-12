# Xergon Network - Updated Wiring Diagrams

**Generated:** 2026-04-10  
**Based on:** Actual implementation in Xergon-Network repository

---

## 1. System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        XERGON NETWORK ARCHITECTURE                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐           │
│  │ Ergo Node    │◄────►│ Xergon Agent │◄────►│ Xergon Relay │           │
│  │ (:9053)      │  PoNW│ (:9099)      │ HTTP │ (:9090)      │           │
│  │              │Health│              │      │              │           │
│  └──────────────┘      └──────┬───────┘      └──────┬───────┘           │
│       │                       │                     │                    │
│       │ On-chain              │ Settlement          │ Routing            │
│       │ (Contracts)           │ (ERG)               │ (Load Balance)     │
│       ▼                       ▼                     ▼                    │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐           │
│  │ ErgoChain    │      │ LLM Backend  │      │ Marketplace  │           │
│  │ (Testnet/    │      │ (Ollama/     │      │ (Next.js 15) │           │
│  │  Mainnet)    │      │  llama.cpp)  │      │ (:3000)      │           │
│  └──────────────┘      └──────────────┘      └──────────────┘           │
│                                                                           │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Component Wiring

### 2.1 Xergon Agent (xergon-agent)

**Port:** 9099  
**Language:** Rust (Axum + Tokio)

**Modules (345 total Rust files, key modules below):**

```
xergon-agent/src/
├── api/                    # REST API handlers
│   ├── mod.rs             # Router setup
│   ├── governance/        # Governance endpoints
│   │   ├── api.rs         # HTTP handlers
│   │   ├── contract.rs    # ErgoScript templates
│   │   ├── onchain.rs     # Transaction builder
│   │   └── types.rs       # Proposal/vote types
│   └── inference.rs       # Chat completion proxy
├── chain/                 # On-chain operations
│   ├── mod.rs
│   └── tx_builder.rs      # Transaction construction
├── config/                # Configuration loading
│   └── mod.rs            # TOML + env var overrides
├── governance/            # Governance system
│   ├── mod.rs
│   ├── api.rs            # REST endpoints
│   ├── contract.rs       # ErgoScript contracts
│   ├── onchain.rs        # On-chain tx builder
│   ├── proposal_templates.rs
│   ├── state_manager.rs
│   ├── treasury.rs
│   └── types.rs
├── inference/             # LLM proxy
│   ├── mod.rs
│   └── handler.rs        # OpenAI-compatible proxy
├── node_health/          # Ergo node monitoring
│   ├── mod.rs
│   └── checker.rs        # Health checks
├── pown/                 # Proof-of-Node-Work
│   ├── mod.rs
│   └── calculator.rs     # Scoring engine
├── peer_discovery/       # P2P peer discovery
│   ├── mod.rs
│   └── discovery.rs      # Peer scanning
├── settlement/           # ERG settlement
│   ├── mod.rs
│   └── engine.rs         # Settlement logic
├── gpu_rental/           # GPU rental system
│   ├── mod.rs
│   └── session.rs        # SSH tunnel management
├── payment_bridge/       # Cross-chain bridge
│   └── mod.rs
└── wallet/               # BIP-39 key management
    └── mod.rs
```

**API Endpoints:**

```yaml
/xergon/status:
  GET: Provider status (PoNW, health, peers)
  Response: { node, pown_status, provider, models }

/xergon/peers:
  GET: Peer discovery state
  Response: { xergon_peers, confirmations }

/xergon/health:
  GET: Basic health check
  Response: { status, uptime }

/xergon/pricing:
  GET: List model pricing
  POST: Set model pricing (requires API key)

/xergon/settlement:
  GET: Settlement history
  POST: Trigger settlement

/xergon/governance/*:
  Various governance endpoints (propose, vote, execute)

/v1/chat/completions:
  POST: Inference proxy to LLM backend
  Stream: SSE

/v1/models:
  GET: List available models
```

---

### 2.2 Xergon Relay (xergon-relay)

**Port:** 9090  
**Language:** Rust (Axum + Tokio + Rusqlite)

**Key Modules:**

```
xergon-relay/src/
├── handlers/             # HTTP handlers
│   ├── chat.rs          # Chat completion routing
│   ├── models.rs        # Model listing
│   ├── providers.rs     # Provider management
│   └── auth.rs          # Authentication
├── proxy.rs             # Request proxying
├── provider.rs          # Provider registry
├── chain.rs             # Chain integration
├── cross_chain_bridge.rs # 6-chain bridge (689 lines)
│   ├── mod.rs
│   ├── chain_adapters.rs
│   └── fraud_proof.rs
├── cache.rs             # Caching layer
├── metrics.rs           # Prometheus metrics
├── rate_limit.rs        # Rate limiting
├── health_score.rs      # Provider health scoring
├── geo_router.rs        # Geographic routing
└── ergopay_builder.rs   # ERG payment builder
```

**API Endpoints:**

```yaml
/v1/chat/completions:
  POST: Main inference endpoint
  Middleware: auth → rate_limit → routing
  Stream: SSE

/v1/models:
  GET: List all models across providers

/v1/providers:
  GET: List registered providers
  POST: Admin-only provider management

/register:
  POST: Provider registration
  Headers: X-Registration-Token

/heartbeat:
  POST: Provider heartbeat
  Body: Health metrics

/v1/bridge/invoice:
  POST: Create bridge invoice (6 chains)
  GET: /v1/bridge/invoice/:id
  POST: /v1/bridge/confirm

/admin/*:
  Admin-only endpoints
```

---

### 2.3 Xergon Marketplace (xergon-marketplace)

**Port:** 3000  
**Language:** TypeScript (Next.js 15 + React 19 + Tailwind 4)

**Key Directories:**

```
xergon-marketplace/
├── app/                  # Next.js 15 app router
│   ├── layout.tsx       # Root layout
│   ├── page.tsx         # Homepage
│   ├── playground/      # Chat interface
│   ├── models/          # Model browser
│   ├── providers/       # Provider dashboard
│   └── api/             # API routes (proxy to relay)
├── lib/
│   ├── api.ts           # Relay client
│   ├── i18n/            # Internationalization
│   │   ├── config.ts    # Locale config (en, ja, zh, es)
│   │   ├── dictionary.ts # 1,359 translation keys
│   │   ├── hooks/
│   │   │   ├── use-t.ts
│   │   │   └── useLocale.ts
│   │   └── stores/
│   │       └── localeStore.ts
│   └── store/           # Zustand state
└── components/          # React components
```

**Routes:**

```typescript
/                    → Homepage (hero + playground)
/playground          → Chat interface
/models              → Model browser
/providers           → Provider dashboard
/api/*              → API routes (proxy to relay)
```

---

## 3. Data Flow Diagrams

### 3.1 User Request Flow (Chat Completion)

```
1. User → Marketplace (Next.js)
   │
   │ POST /api/v1/chat/completions
   │ Headers: Authorization: Bearer *** (EIP-12 wallet signature)
   │ Body: { model, messages, stream }
   ▼
2. Marketplace → Relay (Axum)
   │
   │ Route: POST /v1/chat/completions
   │ Middleware: auth → rate_limit → tracing
   ▼
3. Relay: Provider Selection
   │
   ├─► Check API key validity (auth.rs)
   ├─► Check balance/tier (balance.rs) - ERG staking box
   ├─► Select provider by:
   │   ├─ PoNW score (health_score.rs)
   │   ├─ Region match (geo_router.rs)
   │   └─ Model availability (provider.rs)
   ▼
4. Relay → Agent (HTTP Proxy)
   │
   │ Forward: POST /v1/chat/completions
   │ Headers: X-Provider-ID, X-Request-ID
   │ Stream: SSE enabled
   ▼
5. Agent → LLM Backend
   │
   │ Proxy: POST /api/generate
   │ Target: http://localhost:11434 (Ollama)
   │        or http://localhost:8080 (llama.cpp)
   ▼
6. Response Streaming Back
   │
   Agent ← LLM (SSE chunks)
   Relay ← Agent (SSE chunks)
   Marketplace ← Relay (SSE chunks)
   User ← Marketplace (SSE chunks)
   │
   ▼
7. Settlement Recording
   │
   Agent: Record usage → settlement/
          Create usage_proof box → On-chain (Ergo)
```

---

### 3.2 Provider Registration Flow

```
1. Provider → Agent (Local Config)
   │
   │ Edit: config.toml
   │   [xergon]
   │   provider_id = "my-provider"
   │   ergo_address = "9fDrt..."
   │   region = "us-east"
   ▼
2. Agent Start
   │
   │ Load config → Validate → Connect to Ergo node
   ▼
3. Agent → Ergo Node
   │
   │ GET /info → Check sync status
   │ GET /peers → Peer discovery
   ▼
4. Agent → Relay (Registration)
   │
   │ POST /register
   │ Headers: X-Registration-Token
   │ Body: { provider_id, ergo_address, region, models }
   ▼
5. Relay: Validate & Store
   │
   ├─► Verify registration token (auth)
   ├─► Check Ergo address ownership (balance check)
   └─► Store in ProviderRegistry
   ▼
6. Agent: Heartbeat Loop (every 30s)
   │
   │ POST /heartbeat
   │ Body: { pown_score, node_health, peer_count, models }
   │
   └─► Relay updates provider health status
```

---

### 3.3 Settlement Flow (ERG)

```
1. Inference Complete
   │
   │ Agent tracks: tokens_generated, model_used
   ▼
2. Create Usage Proof
   │
   │ Structure: {
   │   provider_id, user_pk, model, tokens, timestamp, signature
   │ }
   ▼
3. Batch to Rollup (Optional)
   │
   │ Accumulate N proofs → Merkle tree → Commitment box
   ▼
4. Agent → Ergo Node
   │
   │ Build transaction:
   │   Input: Provider box (with NFT)
   │   Output: Usage proof box + change
   │   Sign with wallet key
   ▼
5. Broadcast Transaction
   │
   │ POST /transactions → Ergo node
   ▼
6. On-chain Confirmation
   │
   │ Block confirms → Usage proof immutable
   │ Provider earns: tokens * price_per_token ERG
```

---

## 4. Configuration Wiring

### 4.1 Environment Variables → Config Mapping

```bash
# Agent (xergon-agent)
XERGON_CONFIG=/path/to/config.toml          # Config file path
XERGON__ERGO_NODE__REST_URL=http://...       # Override ergo_node.rest_url
XERGON__API__LISTEN_ADDR=0.0.0.0:9099        # Override api.listen_addr
XERGON__XERGON__PROVIDER_ID=my-provider      # Override xergon.provider_id

# Relay (xergon-relay)
XERGON_RELAY__RELAY__LISTEN_ADDR=0.0.0.0:9090
XERGON_RELAY__PROVIDERS__REGISTRATION_TOKEN=***
XERGON_RELAY__CHAIN__ENABLED=true

# Marketplace (Next.js)
NEXT_PUBLIC_XERGON_AGENT_BASE=http://localhost:9099
NEXT_PUBLIC_API_BASE=/api
RELAY_URL=http://localhost:9090
```

### 4.2 Config.toml Structure (Agent)

```toml
[ergo_node]
rest_url = "http://127.0.0.1:9053"
wallet_address = "9fDrtPahmtQDAPbq9AccibtZVmyPD8xmNJkrNXBbFDkejkez1kM"

[xergon]
provider_id = "my-provider"
ergo_address = "9fDrt..."
region = "us-east"

[api]
listen_addr = "0.0.0.0:9099"
management_api_key = "secret-key"

[inference]
enabled = true
url = "http://127.0.0.1:11434"  # Ollama

[settlement]
enabled = true
cost_per_1k_tokens_nanoerg = 200000000

[contracts]
provider_box_hex = "embedded-hex-or-override"
```

---

## 5. Smart Contract Wiring

### 5.1 Contract Registry

| Contract | File | Purpose |
|----------|------|---------|
| Provider Box | `provider_box.ergo` | Provider identity and state |
| Provider Registration | `provider_registration.es` | Initial onboarding |
| Treasury Box | `treasury.ergo` | Protocol treasury and NFT |
| Usage Proof | `usage_proof.ergo` | Inference receipt (immutable) |
| User Staking | `user_staking.ergo` | Prepaid ERG balance |
| Governance Proposal | `governance_proposal.es` | On-chain voting |
| GPU Rental | `gpu_rental.es` | GPU rental escrow |
| Payment Bridge | `payment_bridge.es` | Cross-chain lock-and-mint |

### 5.2 Contract Compilation Pipeline

```
contracts/*.es  -->  Ergo Playground/AppKit  -->  contracts/compiled/*.hex
                                                              |
                                                              v
                                                     include_str!() at compile time
                                                              |
                                                              v
                                                     xergon-agent binary
```

**Override mechanism:**
Config `[contracts]` section can override embedded hex per-deployment:

```toml
[contracts]
provider_box_hex = "custom-compiled-hex-here"
```

---

## 6. Ergo Integration

### 6.1 Ergo MCP Tools

The Ergo MCP server provides blockchain interaction tools:

```typescript
// From /home/n1ur0/Ergo-MCP/ergo-mcp-server/src/tools.ts
- getAddressBalance(address)       // Check ERG/token balances
- getTransactionDetails(txId)      // Query transactions
- getBlockHeader(height/hash)      // Block queries
- searchTokens(query)              // Token discovery
- getErgoPrice()                   // Real-time price
- getAddressTransactions(address)  // Transaction history
- getBoxesByAddress(address)       // UTXO queries
- getBoxesByTokenId(tokenId)       // NFT/contract boxes
- getNetworkState()                // Network info
```

### 6.2 Cross-Reference with Xergon

| Xergon Feature | Ergo MCP Tool | Integration |
|----------------|---------------|-------------|
| Provider balance | `getAddressBalance` | ✅ Used in relay |
| Transaction tracking | `getTransactionDetails` | ✅ Settlement engine |
| Block scanning | `getBlockHeader` | ✅ Chain scanner |
| NFT queries | `getBoxesByTokenId` | ✅ Provider/usage boxes |
| Price feeds | `getErgoPrice` | ⚠️ Custom oracle implementation |

---

## 7. Key Implementation Details

### 7.1 PoNW Scoring Algorithm

```rust
// xergon-agent/src/pown/calculator.rs

// 40/30/30 split
Node Work (40%):
  - Uptime (50% of node): (uptime_hours / 100) × 100, capped at 100
  - Synced (30%): 1 if synced, 0 otherwise
  - Peers (20%): min(peer_count / 10, 1) × 100

Network Work (30%):
  - Unique peers
  - Total confirms
  - Total tokens served

AI Work (30%):
  - Model inference metrics
  - Tokens generated
  - Request success rate

Final Score = 0.40 × Node + 0.30 × Network + 0.30 × AI
```

### 7.2 Provider Routing Algorithm

```rust
// xergon-relay/src/handlers/routing.rs

score = 0.40 × normalize(pown.work_points)
      + 0.35 × (1 / latency_ms)
      + 0.25 × (1 / active_requests)

// Picks provider with highest score
// Fallback: try next highest on failure (up to 3 retries)
```

### 7.3 i18n System

```typescript
// xergon-marketplace/lib/i18n/config.ts

export const supportedLocales = ['en', 'ja', 'zh', 'es'] as const;
export const defaultLocale: Locale = 'en';

// 1,359 translation keys in dictionary.ts
export const dictionaries = {
  en: { /* English translations */ },
  ja: { /* Japanese translations */ },
  zh: { /* Chinese translations */ },
  es: { /* Spanish translations */ },
};
```

### 7.4 Cross-Chain Bridge

```rust
// xergon-relay/src/cross_chain_bridge.rs

pub enum ChainId {
    Ergo, Ethereum, Cardano, Bitcoin, Bsc, Polygon
}

// Bridge architecture (Rosen-bridge-style):
// 1. Lock tokens on source chain
// 2. Watchers detect lock event
// 3. Commit-reveal transaction
// 4. Fraud proof window
// 5. Release on destination chain
```

---

## 8. Deployment Topology

### Development

```
Local machine: xergon-agent + Ollama + Ergo node (regtest)
```

### Single Provider

```
Server: xergon-agent + LLM backend + Ergo node
Cloud: xergon-relay + xergon-marketplace
```

### Production Network

```
Multiple providers (various regions)
Multiple relays (load balanced)
CDN-backed marketplace
Shared Ergo full node or public nodes
```

---

**Document Status:** Complete  
**Last Updated:** 2026-04-10
