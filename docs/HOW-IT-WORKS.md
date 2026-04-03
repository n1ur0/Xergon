# How Xergon Works

> A visual walkthrough of the Xergon Network — from user click to ERG settlement.

---

## The Big Picture

Xergon turns Ergo node operators into AI compute providers. Users buy credits, send prompts, and get AI responses — routed through a decentralized provider network. Providers earn ERG for their work. Nobody touches a cloud API.

```
                    THE ERGONOMICS OF XERGON

    USER                    NETWORK                   PROVIDER
    ────                    ───────                   ────────

    "Write me a poem"  ──►  Marketplace UI  ──►  Xergon Relay  ──►  Xergon Agent
    pays $0.002/1K tok     (Next.js :3000)     (Rust :9090)       (Rust :9099)
                                                              │
                                                    ┌─────────┴──────────┐
                                                    │                    │
                                              Ergo Node            Local LLM
                                              (:9053)              (Ollama)
                                                    │                    │
                                                    ▼                    ▼
                                              PoNW Score ◄──── Tokens served
                                                    │
                                                    ▼
                                              ERG Settlement
                                              (to provider wallet)
```

Three pieces of software, one invisible pipeline.

---

## The Three Components

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          XERGON NETWORK                                      │
│                                                                             │
│  ┌──────────────────────┐   ┌──────────────────────┐   ┌──────────────────┐ │
│  │   xergon-marketplace │   │    xergon-relay      │   │  xergon-agent    │ │
│  │                      │   │                      │   │                  │ │
│  │  Next.js 15          │   │  Rust + Axum         │   │  Rust + Axum     │ │
│  │  React 19            │   │  SQLite              │   │                  │ │
│  │  Tailwind 4          │   │                      │   │  What it does:   │ │
│  │  Zustand             │   │  What it does:       │   │                  │ │
│  │                      │   │                      │   │  • Monitors      │ │
│  │  What it does:       │   │  • User auth (JWT)   │   │    Ergo node     │ │
│  │                      │   │  • Credits system    │   │  • Discovers     │ │
│  │  • User signup/login │   │  • Stripe payments   │   │    peers (P2P)   │ │
│  │  • Buy credits       │   │  • Rate limiting     │   │  • Scores PoNW   │ │
│  │  • Browse models     │   │  • Provider routing  │   │  • Proxies LLM   │ │
│  │  • Send prompts      │   │  • Fallback chain    │   │  • Settles ERG   │ │
│  │  • Provider dashboard│   │  • Admin panel       │   │                  │ │
│  │                      │   │                      │   │  Port: 9099      │ │
│  │  Port: 3000          │   │  Port: 9090          │   │                  │ │
│  └──────────┬───────────┘   └──────────┬───────────┘   └────────┬─────────┘ │
│             │                          │                        │           │
│             │    /api/v1/* rewrite     │    health polls        │           │
│             └─────────────────────────►└───────────────────────►┘           │
│             │                                                   │           │
│             └────────────────── /api/xergon-agent/* ────────────┘           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Journey 1: Becoming a Provider

### What you need

```
┌─────────────────────────────────────────────────┐
│              PROVIDER SETUP CHECKLIST            │
│                                                 │
│  [✓] Ergo node running and synced              │
│  [✓] Wallet unlocked with some ERG             │
│  [✓] Local LLM (Ollama or llama.cpp) running   │
│  [✓] xergon-agent binary                       │
│  [✓] config.toml with provider identity         │
└─────────────────────────────────────────────────┘
```

### Step-by-step

```
STEP 1: Configure your identity
─────────────────────────────────
Edit xergon-agent/config.toml:

    [xergon]
    provider_id = "Xergon_LT"           ← your unique name
    provider_name = "Xergon Test Node"  ← display name
    region = "us-east"                  ← where you are
    ergo_address = "9fDr...1kM"         ← your ERG address

    [inference]
    enabled = true
    url = "http://127.0.0.1:11434"      ← your LLM backend


STEP 2: Start the agent
─────────────────────────
    $ cargo run --release -- --config config.toml

    The agent immediately starts:
    ┌──────────────────────────────────────────────────┐
    │  xergon-agent                                    │
    │                                                  │
    │  ┌─────────┐  ┌──────────┐  ┌──────────────┐    │
    │  │ Node    │  │ Peer     │  │ LLM Health   │    │
    │  │ Health  │  │ Discovery│  │ Probe        │    │
    │  │ Poller  │  │ Scanner  │  │ (llama.cpp)  │    │
    │  └────┬────┘  └────┬─────┘  └──────┬───────┘    │
    │       │            │               │             │
    │       ▼            ▼               ▼             │
    │  ┌─────────────────────────────────────┐         │
    │  │         PoNW Calculator             │         │
    │  │  Node: 40%  Network: 30%  AI: 30%   │         │
    │  └─────────────────────────────────────┘         │
    │                      │                           │
    │                      ▼                           │
    │  ┌─────────────────────────────────────┐         │
    │  │     REST API (:9099)                │         │
    │  │  GET /xergon/status   ← relay      │         │
    │  │  GET /xergon/health   ← monitoring │         │
    │  │  POST /v1/chat/completions ← relay  │         │
    │  └─────────────────────────────────────┘         │
    └──────────────────────────────────────────────────┘


STEP 3: Register with the relay (optional)
───────────────────────────────────────────
Set in config.toml:
    [relay]
    register_on_start = true
    relay_url = "http://your-relay:9090"
    token = "shared-secret-token"

    Agent sends:
    POST /v1/providers/register
    Headers: X-Provider-Token: shared-secret-token
    Body: {
        "provider_id": "Xergon_LT",
        "provider_name": "Xergon Test Node",
        "region": "us-east",
        "ergo_address": "9fDr...1kM",
        "endpoint": "http://your-server:9099",
        "models": ["qwen3.5-4b-f16.gguf"]
    }

    Then every 60 seconds:
    POST /v1/providers/heartbeat  ← "I'm still alive!"

    On shutdown:
    DELETE /v1/providers/register  ← "I'm leaving"


STEP 4: You're live
───────────────────
Your provider now appears in the marketplace.
Users can send you prompts. You earn credits for every token.
```

---

## Journey 2: Using the Marketplace

### The user sees

```
┌──────────────────────────────────────────────────────────────┐
│  degens.world/xergon                                          │
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                                                         │ │
│  │              XERGON NETWORK                             │ │
│  │         Decentralized AI Compute                        │ │
│  │                                                         │ │
│  │    [ Playground ]  [ Models ]  [ Pricing ]  [ Settings ]│ │
│  │                                                         │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Playground                              Credits: $5.00 │ │
│  │  ─────────────────────────────────────────────────────  │ │
│  │  Model: [qwen3.5-4b-f16.gguf          ▼]              │ │
│  │                                                         │ │
│  │  ┌─────────────────────────────────────────────────┐   │ │
│  │  │ Write a poem about Ergo blockchain...            │   │ │
│  │  └─────────────────────────────────────────────────┘   │ │
│  │                                                         │ │
│  │  ┌─────────────────────────────────────────────────┐   │ │
│  │  │ 🤖 In the land of cryptographic dreams,         │   │ │
│  │  │     where UTXOs flow like mountain streams...   │   │ │
│  │  │                                                 │   │ │
│  │  │     ERG whispers through consensus trees,       │   │ │
│  │  │     each block a proof that nobody cheats...    │   │ │
│  │  └─────────────────────────────────────────────────┘   │ │
│  │                                                         │ │
│  │  Tokens: 127 in / 89 out  |  Cost: $0.0004             │ │
│  └─────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### What happens behind the scenes

```
  USER CLICKS "SEND"
        │
        ▼
  ┌──────────────────────────────────────────────────────────────────┐
  │                    REQUEST LIFECYCLE                              │
  │                                                                  │
  │  ① MARKETPLACE (Next.js)                                        │
  │  ─────────────────────────                                       │
  │  POST /api/v1/chat/completions                                   │
  │  Body: { model, messages, stream: true }                         │
  │  Headers: Authorization: Bearer <jwt>                            │
  │        │                                                        │
  │        │  Next.js rewrites /api/v1/* → relay                     │
  │        ▼                                                        │
  │                                                                  │
  │  ② RELAY (Rust :9090)                                           │
  │  ─────────────────────                                           │
  │                                                                  │
  │  ┌─────────────┐    ┌──────────────┐    ┌───────────────────┐   │
  │  │  AUTH CHECK │───►│ RATE LIMIT   │───►│ CREDIT CHECK      │   │
  │  │             │    │              │    │                   │   │
  │  │ JWT or API  │    │ Free: 10/day │    │ Balance >= cost?  │   │
  │  │ key         │    │ Pro: 10K/30d │    │ Pre-auth stream   │   │
  │  └─────────────┘    └──────────────┘    └────────┬──────────┘   │
  │                                                   │              │
  │                                                   ▼              │
  │  ┌──────────────────────────────────────────────────────────┐   │
  │  │              PROVIDER SELECTION                          │   │
  │  │                                                          │   │
  │  │  Score = 0.4 × PoNW_points                               │   │
  │  │        + 0.35 × (1 / latency_ms)                         │   │
  │  │        + 0.25 × (1 / active_requests)                    │   │
  │  │                                                          │   │
  │  │  Picks provider with highest score.                      │   │
  │  │  If provider fails → try next (up to 3 fallbacks).       │   │
  │  └──────────────────────┬───────────────────────────────────┘   │
  │                         │                                        │
  │                         ▼                                        │
  │  ┌──────────────────────────────────────────────────────────┐   │
  │  │              PROXY TO AGENT                              │   │
  │  │                                                          │   │
  │  │  POST http://provider:9099/v1/chat/completions           │   │
  │  │  Headers: Authorization: Bearer <agent-key>              │   │
  │  │                                                          │   │
  │  │  If streaming (SSE):                                     │   │
  │  │    • Pre-authorize estimated cost                        │   │
  │  │    • Stream tokens to user in real-time                  │   │
  │  │    • Count completion tokens from stream                 │   │
  │  │    • Reconcile: refund overcharge or deduct undercharge  │   │
  │  │                                                          │   │
  │  │  If non-streaming:                                       │   │
  │  │    • Forward request, get response                       │   │
  │  │    • Extract usage from JSON response                    │   │
  │  │    • Deduct exact cost                                   │   │
  │  └──────────────────────┬───────────────────────────────────┘   │
  │                         │                                        │
  │                         ▼                                        │
  │  ③ AGENT (Rust :9099)                                           │
  │  ────────────────────                                           │
  │                                                                  │
  │  ┌──────────────────────────────────────────────────────────┐   │
  │  │  RECEIVES: POST /v1/chat/completions                     │   │
  │  │                                                          │   │
  │  │  ┌─────────────────┐    ┌──────────────────────────────┐ │   │
  │  │  │  AUTH CHECK     │───►│  FORWARD TO LOCAL LLM        │ │   │
  │  │  │  (if api_key    │    │                              │ │   │
  │  │  │   configured)   │    │  POST http://localhost:11434 │ │   │
  │  │  └─────────────────┘    │  /v1/chat/completions        │ │   │
  │  │                         │                              │ │   │
  │  │                         │  (Ollama or llama.cpp)       │ │   │
  │  │                         └──────────────┬───────────────┘ │   │
  │  │                                        │                  │   │
  │  │                                        ▼                  │   │
  │  │                         ┌──────────────────────────────┐ │   │
  │  │                         │  STREAM BACK TO RELAY        │ │   │
  │  │                         │  + Count tokens              │ │   │
  │  │                         │  + Update PoNW score          │ │   │
  │  │                         │  + Record in settlement       │ │   │
  │  │                         └──────────────────────────────┘ │   │
  │  └──────────────────────────────────────────────────────────┘   │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘
```

---

## Journey 3: The Credit System

### Buying credits

```
  USER                    STRIPE                  RELAY
   │                       │                       │
   │  "Buy $10 pack"       │                       │
   │──────────────────────►│                       │
   │                       │                       │
   │  POST /v1/credits/purchase                   │
   │  { pack_id: "pack_10" }                      │
   │──────────────────────────────────────────────►│
   │                       │                       │
   │                       │  Relay creates        │
   │                       │  Stripe Checkout      │
   │                       │  Session              │
   │                       │◄──────────────────────│
   │                       │                       │
   │  Redirect to          │                       │
   │  Stripe Checkout      │                       │
   │◄──────────────────────│                       │
   │                       │                       │
   │  ┌───────────────────┐│                       │
   │  │  STRIPE CHECKOUT   ││                       │
   │  │                   ││                       │
   │  │  Card: **** 4242  ││                       │
   │  │  Amount: $10.00   ││                       │
   │  │                   ││                       │
   │  │  [ Pay $10.00 ]   ││                       │
   │  └───────────────────┘│                       │
   │                       │                       │
   │                       │  POST /v1/webhooks/stripe
   │                       │  (checkout.session.completed)
   │                       │──────────────────────►│
   │                       │                       │
   │                       │  HMAC-SHA256 verify   │
   │                       │  Idempotent process   │
   │                       │  Add $10 + $1 bonus   │
   │                       │                       │
   │  "Payment successful!"                        │
   │◄──────────────────────────────────────────────│
   │  Balance: $11.00 (with $1 bonus)              │
```

### Credit packs

```
  ┌──────────────────────────────────────────┐
  │          CREDIT PACKS                     │
  │                                           │
  │   $5.00    →  $5.00 credits               │
  │   $10.00   →  $11.00 credits (+$1 bonus)  │
  │   $25.00   →  $30.00 credits (+$5 bonus)  │
  │                                           │
  │   Cost: $0.002 per 1,000 tokens           │
  │                                           │
  │   That's ~500K tokens for $1              │
  │   ~15K tokens per cent                    │
  └──────────────────────────────────────────┘
```

### Spending credits

```
  Every request:
  ─────────────

  input_tokens  (chars / 4)     ×  $0.002 / 1000
  + completion_tokens            ×  $0.002 / 1000
  ──────────────────────────────────────────────
  = total_cost_usd

  Deducted from balance in atomic SQLite transaction
  (BEGIN IMMEDIATE to prevent race conditions)
```

---

## Journey 4: Provider Discovery & Routing

### How the relay finds providers

```
  ┌────────────────────────────────────────────────────────────────┐
  │                  PROVIDER DISCOVERY                             │
  │                                                                │
  │  TWO METHODS:                                                  │
  │                                                                │
  │  METHOD A: Static config (legacy)                              │
  │  ────────────────────────────────                              │
  │  relay config.toml:                                            │
  │    known_endpoints = ["http://agent1:9099", "http://agent2"]   │
  │                                                                │
  │  METHOD B: Dynamic registration (recommended)                  │
  │  ───────────────────────────────────────────                   │
  │                                                                │
  │       AGENT                          RELAY                     │
  │         │                              │                        │
  │         │  POST /v1/providers/register │                        │
  │         │─────────────────────────────►│  + Add to registry     │
  │         │                              │  + Start TTL timer     │
  │         │                              │    (default: 180s)     │
  │         │◄─────────────────────────────│                        │
  │         │  { ttl: 180, status: "ok" }  │                        │
  │         │                              │                        │
  │         │  ... 60 seconds later ...    │                        │
  │         │                              │                        │
  │         │  POST /v1/providers/heartbeat│                        │
  │         │─────────────────────────────►│  + Reset TTL           │
  │         │  { work_points: 3521,       │  + Update score        │
  │         │    ai_model: "qwen3.5...",   │  + Update models       │
  │         │    ai_total_tokens: 648 }    │                        │
  │         │                              │                        │
  │         │  ... agent misses heartbeat  │                        │
  │         │      for 180s ...            │                        │
  │         │                              │  TTL expired!          │
  │         │                              │  Remove from registry  │
  │         │                              │                        │
  └────────────────────────────────────────────────────────────────┘
```

### Smart routing algorithm

```
  When a request arrives at the relay:

  ┌─────────────────────────────────────────────────┐
  │              ROUTING DECISION                    │
  │                                                  │
  │  For each healthy provider (ai_enabled=true):    │
  │                                                  │
  │    score = 0.40 × normalize(pown.work_points)   │
  │          + 0.35 × (1 / latency_ms)             │
  │          + 0.25 × (1 / active_requests)         │
  │                                                  │
  │  ┌─────────┐  ┌─────────┐  ┌─────────┐         │
  │  │Agent A  │  │Agent B  │  │Agent C  │         │
  │  │PoNW:5000│  │PoNW:3200│  │PoNW:8000│         │
  │  │Lat: 45ms│  │Lat: 12ms│  │Lat:200ms│         │
  │  │Load: 3  │  │Load: 1  │  │Load: 0  │         │
  │  │Score:.84│  │Score:.91│  │Score:.72│         │
  │  └─────────┘  └────┬────┘  └─────────┘         │
  │                     │                           │
  │              PICK THIS ONE                      │
  │              (highest score)                    │
  │                                                  │
  │  If Agent B fails:                              │
  │    → Try Agent A (next highest)                 │
  │    → Try Agent C (last resort)                  │
  │    → If all fail: return 503                    │
  └─────────────────────────────────────────────────┘
```

---

## Journey 5: Proof-of-Node-Work (PoNW)

### The scoring engine

```
  ┌────────────────────────────────────────────────────────────────┐
  │                  PoNW SCORING ENGINE                            │
  │                                                                │
  │                    ┌─────────────────┐                          │
  │                    │  WORK_POINTS   │                          │
  │                    │  (total score)  │                          │
  │                    └────────┬────────┘                          │
  │                             │                                   │
  │              ┌──────────────┼──────────────┐                   │
  │              │              │              │                   │
  │         ┌────┴────┐   ┌────┴────┐   ┌────┴────┐               │
  │         │  NODE   │   │ NETWORK │   │   AI    │               │
  │         │  WORK   │   │  WORK   │   │  WORK   │               │
  │         │  40%    │   │  30%    │   │  30%    │               │
  │         └────┬────┘   └────┬────┘   └────┬────┘               │
  │              │              │              │                    │
  │    ┌─────────┼─────────┐   │    ┌─────────┼─────────┐         │
  │    │         │         │   │    │         │         │         │
  │  Uptime   Synced   Peers  │  Unique  Total    Total          │
  │  50%      30%      20%   │  Peers   Confirms Tokens          │
  │                            │                                  │
  └────────────────────────────────────────────────────────────────┘
```

### How each category works

```
  ┌─────────────────────────────────────────────────────────────────┐
  │  NODE WORK (40% of total)                                       │
  │  ─────────────────────────                                       │
  │                                                                  │
  │  Uptime Score (50% of node)                                     │
  │  ████████████████████░░░░░░░  72/100                            │
  │  = (uptime_hours / 100) × 100, capped at 100                   │
  │  Full points after 100 hours continuous running                 │
  │                                                                  │
  │  Sync Score (30% of node)                                       │
  │  ████████████████████████░░  100/100                            │
  │  = 100 if synced, 0 if not                                     │
  │  "Synced" = within 2 blocks of network tip                      │
  │                                                                  │
  │  Peer Score (20% of node)                                       │
  │  ████████████████████░░░░░░  80/100                             │
  │  = (peer_count / 10) × 100, capped at 100                      │
  │  More Ergo peers = healthier node                               │
  └─────────────────────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────────────────┐
  │  NETWORK WORK (30% of total)                                     │
  │  ──────────────────────────                                      │
  │                                                                  │
  │  Unique Xergon Peers Seen Score                                 │
  │  ████████████████████████░░  100/100                            │
  │  = (unique_peers / 10) × 100, capped at 100                    │
  │                                                                  │
  │  How it works:                                                   │
  │  1. Agent fetches Ergo node's peer list (/peers/all)            │
  │  2. Extracts IP addresses from peers                            │
  │  3. Concurrently probes each IP on port 9099                    │
  │  4. GET /xergon/status → if 200 OK, it's a Xergon peer         │
  │  5. Persists known peers to JSON file between restarts          │
  │                                                                  │
  │  ┌──────────┐     probe      ┌──────────┐                      │
  │  │ Agent A  │───────────────►│ Agent B  │  ✓ Xergon peer!      │
  │  │ :9099    │  GET /xergon/  │ :9099    │                      │
  │  │          │  status        │          │                      │
  │  └──────────┘               └──────────┘                      │
  │       │                                                          │
  │       │     probe      ┌──────────┐                            │
  │       └───────────────►│ Ergo Peer│  ✗ Not Xergon             │
  │                      │ (no agent)│                             │
  │                      └──────────┘                              │
  └─────────────────────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────────────────┐
  │  AI WORK (30% of total)                                         │
  │  ──────────────────────                                         │
  │                                                                  │
  │  AI Score                                                        │
  │  ██████████████░░░░░░░░░░░░  40/100                             │
  │  = (total_tokens / 10000) × 100, capped at 100                 │
  │                                                                  │
  │  How it works:                                                   │
  │  1. LLM health probe detects running model (GET /v1/models)     │
  │  2. Every inference request counts tokens served                │
  │  3. Tokens accumulate across all requests                       │
  │  4. 1 PoNW point per 100 tokens generated                       │
  │                                                                  │
  │  Token counting:                                                 │
  │  • Streaming: extract from SSE usage.completion_tokens          │
  │  • Fallback: chars / 4 heuristic                                │
  │  • Drop guard: counts partial tokens if client disconnects      │
  └─────────────────────────────────────────────────────────────────┘
```

### The tick cycle

```
  Every 120 seconds (configurable):

  ┌──────────────────────────────────────────────────────────┐
  │  PoNW TICK                                               │
  │                                                          │
  │  1. Check Ergo node health (/info)                      │
  │     → synced? height? peer_count?                        │
  │                                                          │
  │  2. Run peer discovery cycle                             │
  │     → fetch Ergo peers → probe for Xergon agents         │
  │                                                          │
  │  3. Calculate new scores                                 │
  │     → node_work  = f(uptime, sync, peers)               │
  │     → network_work = f(unique_xergon_peers)              │
  │     → ai_work    = f(total_tokens_served)                │
  │     → work_points = 0.4×node + 0.3×network + 0.3×ai     │
  │                                                          │
  │  4. Update internal state (exposed via /xergon/status)   │
  │                                                          │
  └──────────────────────────────────────────────────────────┘
```

---

## Journey 6: ERG Settlement

### The invisible payment layer

```
  Users pay in USD credits.
  Providers get paid in ERG.
  The settlement engine bridges the two.

  ┌────────────────────────────────────────────────────────────────┐
  │                 SETTLEMENT PIPELINE                             │
  │                                                                │
  │  ┌───────────┐     ┌──────────────┐     ┌──────────────────┐  │
  │  │  Usage     │────►│  Aggregate   │────►│  Convert USD→ERG │  │
  │  │  Records   │     │  per-provider│     │  (CoinGecko)     │  │
  │  │            │     │              │     │                  │  │
  │  │  tokens_in │     │  Agent A:    │     │  $2.50 @ $0.30/  │  │
  │  │  tokens_out│     │    1.2M tok  │     │  ERG = 8.33 ERG  │  │
  │  │  cost_usd  │     │    $2.40 usd │     │                  │  │
  │  └───────────┘     └──────────────┘     └────────┬─────────┘  │
  │                                                    │            │
  │                                                    ▼            │
  │  ┌──────────────────────────────────────────────────────────┐  │
  │  │               BUILD BATCH TRANSACTION                    │  │
  │  │                                                          │  │
  │  │  SettlementBatch {                                       │  │
  │  │    batch_id: "batch_2024_04_03_001",                    │  │
  │  │    payments: [                                           │  │
  │  │      { provider: "Agent A",                              │  │
  │  │        ergo_address: "9fDr...1kM",                       │  │
  │  │        nanoerg: 8_330_000_000,    // 8.33 ERG           │  │
  │  │        usd_amount: 2.40 },                               │  │
  │  │      { provider: "Agent B",                              │  │
  │  │        ergo_address: "3aBc...9xZ",                       │  │
  │  │        nanoerg: 4_170_000_000,    // 4.17 ERG           │  │
  │  │        usd_amount: 1.20 }                                │  │
  │  │    ],                                                    │  │
  │  │    total_erg: 12.50,                                     │  │
  │  │    status: Pending                                        │  │
  │  │  }                                                       │  │
  │  └──────────────────────┬───────────────────────────────────┘  │
  │                         │                                       │
  │                         ▼                                       │
  │  ┌──────────────────────────────────────────────────────────┐  │
  │  │              BROADCAST TO ERG NETWORK                    │  │
  │  │                                                          │  │
  │  │  For each payment in batch:                              │  │
  │  │    POST /wallet/payment/send                             │  │
  │  │    { amount: nanoerg, recipient: ergo_address }          │  │
  │  │                                                          │  │
  │  │  Via Ergo node REST API (:9053)                         │  │
  │  │  500ms delay between payments (rate limit safety)        │  │
  │  └──────────────────────┬───────────────────────────────────┘  │
  │                         │                                       │
  │                         ▼                                       │
  │  ┌──────────────────────────────────────────────────────────┐  │
  │  │              CONFIRMATION TRACKING                        │  │
  │  │                                                          │  │
  │  │  Poll /transactions/byId/{txId}                          │  │
  │  │  → Check inclusionHeight                                 │  │
  │  │  → If present: mark Confirmed                            │  │
  │  │  → If stale (>4h): mark Failed                           │  │
  │  │                                                          │  │
  │  │  Ledger persisted to JSON (max 100 batches)              │  │
  │  └──────────────────────────────────────────────────────────┘  │
  │                                                                │
  └────────────────────────────────────────────────────────────────┘
```

### Settlement timing

```
  ┌──────────────────────────────────────────────┐
  │  SETTLEMENT SCHEDULE                          │
  │                                               │
  │  Interval:  24 hours (configurable)           │
  │  Dry run:   true by default (no real ERG)    │
  │  Min USD:   $0.01 per provider (skip dust)   │
  │  Min ERG:   0.001 ERG per payment            │
  │  Fee:       0.001 ERG per transaction        │
  │                                               │
  │  ERG/USD rate from CoinGecko:                 │
  │  • In-memory cache (1h TTL)                  │
  │  • Stale cache fallback                      │
  │  • Persisted to disk as last resort          │
  └──────────────────────────────────────────────┘
```

---

## Data Flow: The Complete Picture

```
  ┌─────────────────────────────────────────────────────────────────────────┐
  │                        COMPLETE DATA FLOW                                │
  │                                                                         │
  │                                                                         │
  │  ┌──────────┐                                                           │
  │  │  USER    │  Signs up, buys credits, sends prompts                    │
  │  │  BROWSER│                                                           │
  │  └────┬─────┘                                                           │
  │       │ HTTP                                                            │
  │       ▼                                                                 │
  │  ┌──────────────────┐       JWT          ┌──────────────────┐          │
  │  │ xergon-marketplace│──────────────────►│  xergon-relay    │          │
  │  │    :3000          │  /api/v1/* proxy  │    :9090         │          │
  │  │                  │                   │                  │          │
  │  │  • Auth UI        │                   │  ┌────────────┐  │          │
  │  │  • Playground     │                   │  │  SQLite DB │  │          │
  │  │  • Model browser  │                   │  │            │  │          │
  │  │  • Provider dash  │                   │  │  users      │  │          │
  │  │  • Settings       │                   │  │  credits   │  │          │
  │  │  • Pricing        │                   │  │  usage     │  │          │
  │  └──────────────────┘                   │  │  api_keys  │  │          │
  │       │                                  │  │  pricing   │  │          │
  │       │ /api/xergon-agent/* proxy        │  └────────────┘  │          │
  │       │                                  │                  │          │
  │       │                                  │  ┌────────────┐  │          │
  │       │                                  │  │  Provider  │  │          │
  │       │                                  │  │  Registry  │  │          │
  │       │                                  │  │  (DashMap) │  │          │
  │       │                                  │  └─────┬──────┘  │          │
  │       │                                  │        │ health  │          │
  │       │                                  │        │ polls   │          │
  │       │                                  │        │         │          │
  │       │                                  │  ┌─────┴──────┐  │          │
  │       │                                  │  │  Smart     │  │          │
  │       │                                  │  │  Router    │  │          │
  │       │                                  │  │  (weighted)│  │          │
  │       │                                  │  └─────┬──────┘  │          │
  │       │                                  │        │         │          │
  │       │                                  │        │ /v1/*   │          │
  │       │                                  │        │ proxy  │          │
  │       │                                  └────────┼─────────┘          │
  │       │                                           │                     │
  │       │                                           ▼                     │
  │       │                                  ┌──────────────────┐          │
  │       └──────────────────────────────────│  xergon-agent    │          │
  │                                          │    :9099         │          │
  │                                          │                  │          │
  │                                          │  ┌────────────┐  │          │
  │                                          │  │ PoNW Engine │  │          │
  │                                          │  │ (scoring)  │  │          │
  │                                          │  └────────────┘  │          │
  │                                          │                  │          │
  │                                          │  ┌────────────┐  │          │
  │                                          │  │ Settlement │  │          │
  │                                          │  │ Engine     │  │          │
  │                                          │  │ (ERG pay)  │  │          │
  │                                          │  └─────┬──────┘  │          │
  │                                          │        │         │          │
  │                                          └────────┼─────────┘          │
  │                                                   │                     │
  │                                    ┌──────────────┼──────────────┐     │
  │                                    │              │              │     │
  │                                    ▼              ▼              ▼     │
  │                              ┌──────────┐  ┌──────────┐  ┌──────────┐│
  │                              │ Ergo     │  │ Ollama / │  │ Other    ││
  │                              │ Node     │  │ llama.cpp│  │ Xergon   ││
  │                              │ :9053    │  │ :11434   │  │ Agents   ││
  │                              │          │  │          │  │ (P2P)    ││
  │                              │ • Blocks │  │ • LLM    │  │ :9099    ││
  │                              │ • Peers  │  │ • Tokens │  │          ││
  │                              │ • Wallet │  │ • Models │  │          ││
  │                              └──────────┘  └──────────┘  └──────────┘│
  │                                                                         │
  └─────────────────────────────────────────────────────────────────────────┘
```

---

## Security Model

```
  ┌──────────────────────────────────────────────────────────────┐
  │                    SECURITY LAYERS                            │
  │                                                              │
  │  LAYER 1: User Auth                                          │
  │  ─────────────────                                           │
  │  • Passwords hashed with Argon2                              │
  │  • JWT tokens (7-day expiry)                                 │
  │  • API keys (xrg_ prefix, SHA-256 hashed, max 10/user)       │
  │  • Unified auth: JWT OR API key per request                  │
  │                                                              │
  │  LAYER 2: Provider Auth                                      │
  │  ─────────────────                                           │
  │  • Shared secret token (X-Provider-Token header)             │
  │  • Constant-time comparison (timing attack safe)             │
  │  • Registration required before serving requests            │
  │  • TTL-based expiry (stale providers auto-removed)           │
  │                                                              │
  │  LAYER 3: Admin Auth                                         │
  │  ─────────────────                                           │
  │  • Separate admin token (X-Admin-Token header)               │
  │  • Environment variable only (never in config file)          │
  │  • Returns 503 if not configured                             │
  │                                                              │
  │  LAYER 4: Rate Limiting                                      │
  │  ─────────────────                                           │
  │  • Anonymous: 10 requests/day per IP                         │
  │  • Free tier: 10 requests/day                                │
  │  • Pro tier: 10,000 requests/30 days                         │
  │  • IP addresses SHA-256 hashed for privacy                   │
  │                                                              │
  │  LAYER 5: Production Safety                                  │
  │  ─────────────────                                           │
  │  • Relay refuses to start with default JWT secret            │
  │    unless XERGON_ENV=development                             │
  │  • Stripe webhooks: HMAC-SHA256 signature verification       │
  │  • 5-minute timestamp anti-replay on webhooks                │
  │  • Idempotent event processing (no double-fulfillment)       │
  │  • Atomic credit deduction (BEGIN IMMEDIATE)                 │
  │  • CORS restricted to localhost in agent                     │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

---

## API Map

```
  xergon-relay (:9090)
  ────────────────────

  AUTH
    POST /v1/auth/signup           Create account
    POST /v1/auth/login            Get JWT
    POST /v1/auth/forgot-password  Request reset email
    POST /v1/auth/reset-password   Reset with token

  CREDITS
    GET  /v1/credits/balance       Current balance
    GET  /v1/credits/transactions  History
    POST /v1/credits/purchase      Stripe checkout
    POST /v1/webhooks/stripe       Stripe webhook

  INFERENCE
    POST /v1/chat/completions      Chat (streaming + non-streaming)
    POST /v1/inference             Simple prompt→completion
    GET  /v1/models                Available models with pricing

  USER
    GET  /v1/me                    Profile
    PUT  /v1/me                    Update profile
    PUT  /v1/me/password           Change password
    PUT  /v1/me/wallet             Link Ergo address
    GET  /v1/usage                 Usage stats + history

  API KEYS
    POST /v1/api-keys              Create key
    GET  /v1/api-keys              List keys
    DELETE /v1/api-keys/:id        Revoke key

  PROVIDERS (registration protocol)
    POST   /v1/providers/register       Register
    DELETE /v1/providers/register       Deregister
    POST   /v1/providers/heartbeat      Heartbeat
    GET    /v1/providers                Directory
    PUT    /v1/providers/pricing        Update pricing

  ADMIN
    GET  /v1/admin/users              List users
    PUT  /v1/admin/users/:id/tier     Update tier
    PUT  /v1/admin/users/:id/credits  Adjust credits
    GET  /v1/admin/providers          List providers
    GET  /v1/admin/stats              Platform stats


  xergon-agent (:9099)
  ────────────────────

  STATUS
    GET /xergon/status          Full status (for relay + peer discovery)
    GET /xergon/health          Liveness + uptime
    GET /xergon/peers           Current peer state
    GET /xergon/settlement      Settlement engine status
    GET /xergon/dashboard       Aggregated dashboard (all data)

  INFERENCE (proxied to local LLM)
    POST /v1/chat/completions   OpenAI-compatible chat
    GET  /v1/models             Detected models
```

---

## What Makes Xergon Different

```
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │   CENTRALIZED AI          VS          XERGON NETWORK        │
  │   (OpenAI, Anthropic)                  (Decentralized)       │
  │                                                              │
  │   ┌──────────────┐             ┌──────────────────────┐     │
  │   │ Single company│             │ Anyone with an       │     │
  │   │ controls      │             │ Ergo node + GPU      │     │
  │   │ access        │             │ can be a provider    │     │
  │   └──────────────┘             └──────────────────────┘     │
  │                                                              │
  │   ┌──────────────┐             ┌──────────────────────┐     │
  │   │ Cloud APIs    │             │ Local inference      │     │
  │   │ send your     │             │ data never leaves    │     │
  │   │ data to       │             │ the provider machine │     │
  │   │ their servers │             │                      │     │
  │   └──────────────┘             └──────────────────────┘     │
  │                                                              │
  │   ┌──────────────┐             ┌──────────────────────┐     │
  │   │ Pricing set   │             │ Open pricing per     │     │
  │   │ by the        │             │ provider, market     │     │
  │   │ provider      │             │ driven competition   │     │
  │   └──────────────┘             └──────────────────────┘     │
  │                                                              │
  │   ┌──────────────┐             ┌──────────────────────┐     │
  │   │ Can be        │             │ Verifiable via        │     │
  │   │ censored      │             │ on-chain proofs       │     │
  │   │ or shut down  │             │ and peer confirmations│     │
  │   └──────────────┘             └──────────────────────┘     │
  │                                                              │
  │   ┌──────────────┐             ┌──────────────────────┐     │
  │   │ Pay with      │             │ Pay with ERG         │     │
  │   │ credit card   │             │ (crypto-native)      │     │
  │   └──────────────┘             └──────────────────────┘     │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

---

## Roadmap

```
  PHASE 1 ─ Core PoNW           ████████████████████ 100%  DONE
  PHASE 2 ─ Marketplace          ██████████████████░░  85%  IN PROGRESS
  PHASE 3 ─ Economic Layer       ████████░░░░░░░░░░░░  35%  PLANNED
  PHASE 4 ─ Network Layer        ████░░░░░░░░░░░░░░░  15%  PLANNED
  PHASE 5 ─ Full Compute Network ░░░░░░░░░░░░░░░░░░░   0%  FUTURE
```

---

*Powered by [Degens World](https://degens.world) | Built on [Ergo](https://ergoplatform.org)*
