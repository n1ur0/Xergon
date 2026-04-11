# Xergon Network - Wiring Diagrams & Data Flow

**Generated:** 2026-04-10  
**Scope:** Complete wiring analysis for Agent ↔ Relay ↔ Marketplace  
**Based on:** Actual code implementation in Xergon-Network repository

---

## 1. System Architecture Overview

### High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Xergon Network Architecture                       │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐          │
│  │ Ergo Node    │◄────►│ Xergon Agent │◄────►│ Xergon Relay │          │
│  │ (:9053)      │  PoNW│ (:9099)      │ HTTP │ (:9090)      │          │
│  │              │Health│              │      │              │          │
│  └──────────────┘      └──────┬───────┘      └──────┬───────┘          │
│       │                       │                     │                   │
│       │ On-chain              │ Settlement          │ Routing           │
│       │ (Contracts)           │ (ERG)               │ (Load Balance)    │
│       ▼                       ▼                     ▼                   │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐          │
│  │ ErgoChain    │      │ LLM Backend  │      │ Marketplace  │          │
│  │ (Testnet/    │      │ (Ollama/     │      │ (Next.js 15) │          │
│  │  Mainnet)    │      │  llama.cpp)  │      │ (:3000)      │          │
│  └──────────────┘      └──────────────┘      └──────────────┘          │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Data Flow Diagrams

### 2.1 User Request Flow (Chat Completion)

```
┌─────────────────────────────────────────────────────────────────────┐
│                    User Request Flow (Chat)                          │
└─────────────────────────────────────────────────────────────────────┘

1. User → Marketplace (Next.js)
   │
   │ POST /v1/chat/completions
   │ Headers: Authorization: Bearer <token>
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
   ├─► Check balance/tier (balance.rs)
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
          Create usage_proof box → On-chain
```

### 2.2 Provider Registration Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                   Provider Registration Flow                         │
└─────────────────────────────────────────────────────────────────────┘

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

### 2.3 Settlement Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Settlement Flow (ERG)                           │
└─────────────────────────────────────────────────────────────────────┘

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

## 3. API Endpoint Mapping

### 3.1 Agent API (xergon-agent, :9099)

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

### 3.2 Relay API (xergon-relay, :9090)

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

/v1/contracts/oracle/rate:
  GET: Oracle price feed

/v1/bridge/*:
  Cross-chain bridge endpoints

/admin/*:
  Admin-only endpoints
```

### 3.3 Marketplace Routes (xergon-marketplace, :3000)

```typescript
/                    → Homepage (hero + playground)
/playground          → Chat interface
/models              → Model browser
/providers           → Provider dashboard
/api/*              → API routes (proxy to relay)
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
XERGON_RELAY__PROVIDERS__REGISTRATION_TOKEN=secret
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

## 5. Module Wiring (Agent)

### 5.1 Core Module Dependencies

```rust
// xergon-agent/src/main.rs
use xergon_agent::{
    api::AppState,           // REST API handlers
    config::AgentConfig,     // Configuration loading
    node_health::NodeHealthChecker,  // Ergo node monitoring
    peer_discovery::PeerDiscovery,   // P2P discovery
    pown::PownCalculator,    // PoNW scoring
    settlement::SettlementEngine,    // ERG settlement
};

// Wiring in main():
let state = AppState {
    pown_status: Arc::new(RwLock::new(...)),
    peer_state: Arc::new(RwLock::new(...)),
    node_health: Arc::new(RwLock::new(...)),
    settlement: Some(Arc::new(SettlementEngine::new(...))),
    // ... 40+ other fields
};

// Router setup:
let app = Router::new()
    .route("/xergon/status", get(status_handler))
    .route("/xergon/peers", get(peers_handler))
    .route("/v1/chat/completions", post(inference_handler))
    .layer(TraceLayer::new_for_http())
    .with_state(state);
```

### 5.2 Key Handler Wiring

```rust
// xergon-agent/src/api/mod.rs (simplified)

pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Status endpoints
        .route("/xergon/status", get(get_status))
        .route("/xergon/peers", get(get_peers))
        .route("/xergon/health", get(get_health))
        
        // Pricing
        .route("/xergon/pricing", get(list_pricing).post(set_pricing))
        
        // Settlement
        .route("/xergon/settlement", get(get_settlement).post(trigger_settlement))
        
        // Inference proxy
        .route("/v1/chat/completions", post(proxy_inference))
        .route("/v1/models", get(list_models))
        
        // Governance
        .nest("/xergon/governance", governance::router())
        
        .with_state(state)
}

// Handler example:
async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let pown = state.pown_status.read().await;
    let health = state.node_health.read().await;
    Json(StatusResponse {
        pown: pown.clone(),
        health: health.clone(),
        // ...
    })
}
```

---

## 6. Module Wiring (Relay)

### 6.1 Core Module Dependencies

```rust
// xergon-relay/src/main.rs
use xergon_relay::{
    proxy::AppState,
    handlers::{chat, models, providers, auth},
    provider::ProviderRegistry,
    chain::ChainScanner,
    cache::Cache,
    metrics::RelayMetrics,
};

// Wiring in main():
let state = AppState {
    provider_registry: Arc::new(ProviderRegistry::new(config)),
    http_client: reqwest::Client::new(),
    chain_scanner: Some(Arc::new(ChainScanner::new(...))),
    cache: Arc::new(Cache::new()),
    metrics: Arc::new(RelayMetrics::new()),
    // ... 20+ other fields
};

// Router setup:
let app = Router::new()
    .route("/v1/chat/completions", post(chat::handle))
    .route("/v1/models", get(models::handle))
    .route("/register", post(providers::register))
    .route("/heartbeat", post(providers::heartbeat))
    .layer(axum::middleware::from_fn(auth::verify))
    .with_state(state);
```

### 6.2 Provider Selection Logic

```rust
// xergon-relay/src/handlers/routing.rs (simplified)

pub async fn select_provider(
    State(state): State<AppState>,
    request: ChatCompletionRequest,
) -> Result<Provider, Error> {
    let registry = state.provider_registry.read().await;
    
    // Filter by model availability
    let available = registry
        .providers()
        .filter(|p| p.has_model(&request.model));
    
    // Score by PoNW + region match
    let scored = available.map(|p| {
        let pown_score = p.health_score().await;
        let region_match = p.region() == request.preferred_region;
        (p, pown_score + if region_match { 10 } else { 0 })
    });
    
    // Select best
    scored.max_by_key(|(_, score)| *score)
        .map(|(p, _)| p.clone())
        .ok_or(Error::NoProvider)
}
```

---

## 7. Marketplace Wiring

### 7.1 API Client (TypeScript)

```typescript
// xergon-marketplace/lib/api.ts (simplified)

export class XergonClient {
  private relayUrl: string;
  
  constructor(relayUrl: string) {
    this.relayUrl = relayUrl;
  }
  
  async chatCompletion(request: ChatCompletionRequest): Promise<Response> {
    const response = await fetch(`${this.relayUrl}/v1/chat/completions`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${this.getAuthToken()}`,
      },
      body: JSON.stringify(request),
    });
    
    return response;
  }
  
  async listModels(): Promise<Model[]> {
    const response = await fetch(`${this.relayUrl}/v1/models`);
    return response.json();
  }
}
```

### 7.2 React Component Wiring

```typescript
// xergon-marketplace/app/playground/page.tsx

export default function Playground() {
  const [messages, setMessages] = useState<Message[]>([]);
  const client = useXergonClient(); // Zustand hook
  
  const sendMessage = async (content: string) => {
    const response = await client.chatCompletion({
      model: 'llama-3.1-8b',
      messages: [...messages, { role: 'user', content }],
      stream: true,
    });
    
    // Handle SSE stream
    for await (const chunk of response.body) {
      setMessages(prev => [...prev, chunk]);
    }
  };
  
  return <ChatInterface messages={messages} onSend={sendMessage} />;
}
```

---

## 8. Critical Wiring Issues Identified

### 8.1 Missing/Incomplete Wiring

1. **Settlement → Marketplace**
   - Issue: Marketplace doesn't show settlement history
   - Fix: Add `/xergon/settlement` endpoint in marketplace UI

2. **Governance → UI**
   - Issue: Governance CLI exists but no UI
   - Fix: Add governance dashboard in marketplace

3. **Cross-chain Bridge → SDK**
   - Issue: Bridge implementation exists but SDK not integrated
   - Fix: Add bridge methods to @xergon/sdk

### 8.2 Dead Code Impacting Wiring

1. **25+ unused relay modules** (see module-audit-report.md)
   - Cross-chain modules not wired to handlers
   - Quantum crypto not integrated
   - gRPC support unused

2. **Agent modules with dead_code**
   - Many `#[allow(dead_code)]` annotations
   - Need to determine: remove vs feature-flag

---

## 9. Wiring Verification Commands

```bash
# Check Agent API endpoints
cd xergon-agent
cargo run -- run --config config.toml
curl http://localhost:9099/xergon/status

# Check Relay endpoints
cd xergon-relay
cargo run --release
curl http://localhost:9090/v1/models

# Check Marketplace
cd xergon-marketplace
npm run dev
# Visit http://localhost:3000

# Integration test
./tests/integration-test.sh
```

---

## 10. References

- **OpenAPI Spec:** `docs/openapi.yaml` (2939 lines)
- **Agent Config:** `xergon-agent/config.toml.example`
- **Relay Config:** `xergon-relay/config.toml.example`
- **Module Audit:** `module-audit-report.md`
- **Tech Stack:** `../wiki/guides/xergon-tech-stack.md`

---

**Last Updated:** 2026-04-10  
**Verified Against:** Xergon-Network main branch
